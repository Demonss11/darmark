//! Состояние AI-подсистемы: настройки, бэкенд (шов) и клиентский rate limit.
//!
//! Хранится в Tauri через `app.manage` (как `PluginState`). Настройки — источник истины в
//! памяти; мутации пишутся в конфиг через [`crate::ai::settings::persist_to`]. Бэкенд — за
//! трейтом [`AiTransport`] (ADR-0034 §2.2).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::ai::settings;
use crate::ai::transport::AiTransport;
use crate::ai::types::{
    resolve_api_key, AiSettings, AiSettingsInput, ModelEntry, ProviderConfig, ProviderSettings,
};
use crate::error::{CommandError, ErrorCode};

/// Минимальный интервал между запросами (клиентский rate limit ≥1 зап/с, ADR-0032).
const MIN_REQUEST_INTERVAL: Duration = Duration::from_secs(1);

/// Разрешённая активная пара (провайдер + модель) с резолвнутым ключом и надстройкой запроса.
///
/// Собирается [`resolve_active`] после гейта: дальше команда работает только с ней, а не
/// с полным map провайдеров (ADR-0035 §2.2).
#[derive(Clone)]
pub struct ResolvedProvider {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub headers: BTreeMap<String, String>,
    pub body: serde_json::Map<String, serde_json::Value>,
    /// Таймаут запроса в мс (`None` — дефолт транспорта). 0 нормализуется в `None`:
    /// иначе запрос получает `Duration::ZERO` и обрывается мгновенно (ADR-0035 §2.5).
    pub timeout: Option<u64>,
}

/// Ручной `Debug`: секрет (`api_key`) маскируется, а надстройка запроса (`headers`/`body`)
/// не разворачивается — печатаются только имена, чтобы не утечь в логи (H4, ADR-0035 §2.6 п.16).
impl std::fmt::Debug for ResolvedProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolvedProvider")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("headers", &self.headers.keys().collect::<Vec<_>>())
            .field("body", &self.body.keys().collect::<Vec<_>>())
            .field("timeout", &self.timeout)
            .finish()
    }
}

/// Состояние AI-подсистемы для IPC-команд.
pub struct AiState {
    settings: Mutex<AiSettings>,
    backend: Arc<dyn AiTransport>,
    /// Момент, до которого следующий запрос должен подождать (rate limit).
    rate_limit: Mutex<Option<Instant>>,
    /// Путь конфига для записи секции `ai`; `None` — сохранять некуда.
    config_path: Option<PathBuf>,
}

impl AiState {
    /// Состояние с настройками/бэкендом и путём конфига по умолчанию.
    pub fn new(settings: AiSettings, backend: Arc<dyn AiTransport>) -> Self {
        Self::with_config_path(settings, backend, settings::config_path())
    }

    /// Полный конструктор с инъекцией пути конфига (тесты/E2E — без реального профиля).
    pub fn with_config_path(
        settings: AiSettings,
        backend: Arc<dyn AiTransport>,
        config_path: Option<PathBuf>,
    ) -> Self {
        Self {
            settings: Mutex::new(settings),
            backend,
            rate_limit: Mutex::new(None),
            config_path,
        }
    }

    /// Инициализация для `app.manage`: читает настройки из конфига (с миграцией legacy),
    /// поднимает бэкенд. Ленивая (ADR-0034 §2.3): сеть не используется до `ai_chat`.
    pub fn init() -> Self {
        let config_path = settings::config_path();
        let settings = settings::load_and_migrate(config_path.as_deref());
        Self::with_config_path(settings, default_backend(), config_path)
    }

    /// Текущие настройки (клон — секрет нужен для запроса, дальше не утекает).
    pub fn settings(&self) -> AiSettings {
        lock(&self.settings).clone()
    }

    /// Бэкенд шва (клон `Arc` — вызывающий владеет им на время запроса).
    pub fn backend(&self) -> Arc<dyn AiTransport> {
        Arc::clone(&self.backend)
    }

    /// Заменяет провайдеров/активную пару/лимиты и сохраняет в конфиг.
    ///
    /// Секреты провайдеров сохраняются по id (фронт их не присылает); при смене хоста
    /// активного провайдера согласие сбрасывается (ADR-0035 §2.7).
    ///
    /// Мутации применяются к копии настроек: сначала [`settings::persist_to`], и только
    /// при успехе — к памяти. При сбое записи память и диск не расходятся (ADR-0035 §2.9).
    pub fn update_settings(&self, input: AiSettingsInput) -> Result<(), CommandError> {
        let mut settings = lock(&self.settings);
        if input.providers.is_empty() {
            return Err(bad_settings("нужен хотя бы один провайдер"));
        }
        let mut providers: BTreeMap<String, ProviderConfig> = BTreeMap::new();
        for provider in &input.providers {
            validate_base_url(&provider.settings.base_url)?;
            let previous = settings.providers.get(&provider.id);
            let mut models = BTreeMap::new();
            for model in &provider.models {
                models.insert(
                    model.id.clone(),
                    ModelEntry {
                        name: model.name.clone(),
                        model_id: model.model_id.clone(),
                        disabled: model.disabled,
                    },
                );
            }
            providers.insert(
                provider.id.clone(),
                ProviderConfig {
                    name: provider.name.clone(),
                    env: provider.env.clone(),
                    package: previous
                        .map(|p| p.package.clone())
                        .filter(|p| !p.is_empty())
                        .unwrap_or_else(|| "openai-compatible".to_string()),
                    settings: ProviderSettings {
                        base_url: provider.settings.base_url.clone(),
                        api_key: previous.and_then(|p| p.settings.api_key.clone()),
                        timeout: provider.settings.timeout,
                    },
                    headers: provider.headers.clone(),
                    body: provider.body.clone(),
                    models,
                },
            );
        }
        if !providers.contains_key(&input.active_provider) {
            return Err(bad_settings(format!(
                "активный провайдер «{}» не найден",
                input.active_provider
            )));
        }
        let old_host = host_for(&settings.providers, &settings.active_provider);
        let new_host = host_for(&providers, &input.active_provider);
        let mut next = settings.clone();
        next.enabled = input.enabled;
        next.active_provider = input.active_provider;
        next.active_model = input.active_model;
        next.max_tokens = input.max_tokens;
        next.temperature = input.temperature;
        next.consent_document_send = if old_host != new_host {
            false
        } else {
            input.consent_document_send
        };
        next.providers = providers;
        settings::persist_to(self.config_path.as_deref(), &next)?;
        *settings = next;
        Ok(())
    }

    /// Записывает API-ключ указанного провайдера (секрет) в конфиг. Пустая строка очищает.
    ///
    /// Ключ не логируется и не возвращается командами — наружу идёт только `has_key`.
    pub fn set_api_key(&self, provider_id: String, key: String) -> Result<(), CommandError> {
        let mut settings = lock(&self.settings);
        // Ключ меняется в копии: persist-first, чтобы при сбое записи память не разошлась
        // с диском (ADR-0035 §2.9).
        let mut next = settings.clone();
        let provider = next
            .providers
            .get_mut(&provider_id)
            .ok_or_else(|| bad_settings(format!("провайдер «{provider_id}» не найден")))?;
        let trimmed = key.trim();
        provider.settings.api_key = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        };
        settings::persist_to(self.config_path.as_deref(), &next)?;
        *settings = next;
        Ok(())
    }

    /// Клиентский rate limit: гарантирует паузу ≥1 с между запросами (ADR-0032).
    ///
    /// Асинхронный сон (не блокирующий `thread::sleep`): не занимает worker tokio.
    pub async fn enforce_rate_limit(&self) {
        let wait = self.reserve_rate_limit_slot();
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
    }

    /// Резервирует слот и возвращает, сколько нужно подождать до следующего запроса.
    ///
    /// Вынесено из [`Self::enforce_rate_limit`], чтобы проверяться синхронно в тестах.
    pub(super) fn reserve_rate_limit_slot(&self) -> Duration {
        let mut last = lock(&self.rate_limit);
        let wait = last
            .map(|previous| MIN_REQUEST_INTERVAL.saturating_sub(previous.elapsed()))
            .unwrap_or_default();
        *last = Some(Instant::now() + wait);
        wait
    }
}

/// Резолвит активную пару `active_provider`/`active_model` в [`ResolvedProvider`].
///
/// Ошибка `ai_bad_settings` — если провайдер не найден, модель не входит в его `models`,
/// модель отключена или `baseURL` не проходит валидацию (ADR-0035 §2.2, §2.6).
pub fn resolve_active(settings: &AiSettings) -> Result<ResolvedProvider, CommandError> {
    let provider = settings
        .providers
        .get(&settings.active_provider)
        .ok_or_else(|| {
            bad_settings(format!(
                "активный провайдер «{}» не найден",
                settings.active_provider
            ))
        })?;
    let model = provider.models.get(&settings.active_model).ok_or_else(|| {
        bad_settings(format!(
            "модель «{}» не найдена у провайдера «{}»",
            settings.active_model, settings.active_provider
        ))
    })?;
    if model.disabled.unwrap_or(false) {
        return Err(bad_settings(format!(
            "модель «{}» отключена",
            settings.active_model
        )));
    }
    validate_base_url(&provider.settings.base_url)?;

    let name = if provider.name.trim().is_empty() {
        settings.active_provider.clone()
    } else {
        provider.name.clone()
    };
    Ok(ResolvedProvider {
        id: settings.active_provider.clone(),
        name,
        base_url: provider.settings.base_url.clone(),
        model: settings.active_model.clone(),
        api_key: resolve_api_key(provider, |name| std::env::var(name).ok()),
        headers: provider.headers.clone(),
        body: provider.body.clone(),
        timeout: provider.settings.timeout.filter(|ms| *ms > 0),
    })
}

/// Хост провайдера по id в наборе (нормализованный).
///
/// Служит привязкой согласия к адресату (ADR-0035 §2.7): смена хоста → перезапрос.
/// `pub(super)` — проверяется в `ai::tests::state`.
pub(super) fn host_for(providers: &BTreeMap<String, ProviderConfig>, id: &str) -> Option<String> {
    providers
        .get(id)
        .and_then(|provider| host_of_url(&provider.settings.base_url))
}

/// Проверяет `baseURL` (H3): разрешён только `https://`; `http://` — лишь для loopback
/// (`127.0.0.1`/`localhost`/`[::1]`) ради E2E и локальных тестов.
///
/// Не-`https`/loopback адрес отклоняется, чтобы тело документа не ушло по незащищённому
/// каналу на произвольный хост.
pub fn validate_base_url(base_url: &str) -> Result<(), CommandError> {
    let trimmed = base_url.trim();
    let Some((scheme, rest)) = trimmed.split_once("://") else {
        return Err(bad_settings(
            "адрес провайдера должен начинаться с https:// (или http:// для локального хоста)",
        ));
    };
    let host = host_of(rest);
    match scheme.to_ascii_lowercase().as_str() {
        "https" if !host.is_empty() => Ok(()),
        "https" => Err(bad_settings("в адресе провайдера не указан хост")),
        "http" if is_loopback_host(host) => Ok(()),
        "http" => Err(bad_settings(
            "http:// разрешён только для локального хоста (127.0.0.1/localhost/[::1]); \
             для внешних адресов используйте https://",
        )),
        other => Err(bad_settings(format!(
            "неподдерживаемая схема адреса провайдера: {other}"
        ))),
    }
}

/// Нормализованный хост URL (нижний регистр, без порта/пути); `None`, если пуст/нет схемы.
fn host_of_url(url: &str) -> Option<String> {
    let (_, rest) = url.trim().split_once("://")?;
    let host = host_of(rest);
    if host.is_empty() {
        None
    } else {
        Some(host.to_ascii_lowercase())
    }
}

/// Ошибка настроек с единым кодом `ai_bad_settings`.
fn bad_settings(message: impl Into<String>) -> CommandError {
    CommandError::new(ErrorCode::AiBadSettings, message)
}

/// Хост из остатка URL после `://` (без userinfo, порта, пути и query).
fn host_of(rest: &str) -> &str {
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    let host_port = authority
        .rsplit_once('@')
        .map(|(_, host)| host)
        .unwrap_or(authority);
    if let Some(stripped) = host_port.strip_prefix('[') {
        stripped.split(']').next().unwrap_or(stripped)
    } else {
        host_port.split(':').next().unwrap_or(host_port)
    }
}

/// Loopback-хост (`127.0.0.1`, `localhost`, `[::1]`); регистр имени не учитывается
/// (`LOCALHOST`, `LocalHost` — валидны).
fn is_loopback_host(host: &str) -> bool {
    let host = host
        .trim_matches(|c| c == '[' || c == ']')
        .to_ascii_lowercase();
    matches!(host.as_str(), "127.0.0.1" | "localhost" | "::1")
}

fn default_backend() -> Arc<dyn AiTransport> {
    Arc::new(crate::ai::backend::reqwest::ReqwestBackend::new())
}

/// Блокировка мьютекса без паники: под `panic = "abort"` отравление невозможно, но в тестах
/// (`panic = "unwind"`) берём внутреннее значение вместо `expect`.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
