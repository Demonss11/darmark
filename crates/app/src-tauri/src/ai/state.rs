//! Состояние AI-подсистемы: настройки, бэкенд (шов) и клиентский rate limit.
//!
//! Хранится в Tauri через `app.manage` (как `PluginState`). Настройки — источник истины в
//! памяти; мутации пишутся в конфиг через [`crate::ai::settings::persist_to`]. Бэкенд — за
//! трейтом [`AiTransport`] (ADR-0034 §2.2): на Windows — `reqwest`, иначе заглушка.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::ai::settings;
use crate::ai::transport::AiTransport;
use crate::ai::types::{AiSettings, AiSettingsInput};
use crate::error::{CommandError, ErrorCode};

/// Минимальный интервал между запросами (клиентский rate limit ≥1 зап/с, ADR-0032).
const MIN_REQUEST_INTERVAL: Duration = Duration::from_secs(1);

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

    /// Инициализация для `app.manage`: читает настройки из конфига, поднимает бэкенд.
    ///
    /// Ленивая (ADR-0034 §2.3): сеть/HTTP-клиент не используются, пока не вызван `ai_chat`.
    pub fn init() -> Self {
        let config_path = settings::config_path();
        Self::with_config_path(
            settings::load_from(config_path.as_deref()),
            default_backend(),
            config_path,
        )
    }

    /// Текущие настройки (клон — секрет нужен для запроса, дальше не утекает).
    pub fn settings(&self) -> AiSettings {
        lock(&self.settings).clone()
    }

    /// Бэкенд шва (клон `Arc` — вызывающий владеет им на время запроса).
    pub fn backend(&self) -> Arc<dyn AiTransport> {
        Arc::clone(&self.backend)
    }

    /// Обновляет несекретные настройки (ключ здесь не трогается) и сохраняет в конфиг.
    pub fn update_settings(&self, input: AiSettingsInput) -> Result<(), CommandError> {
        validate_base_url(&input.base_url)?;
        let mut settings = lock(&self.settings);
        settings.enabled = input.enabled;
        settings.provider = input.provider;
        settings.base_url = input.base_url;
        settings.model = input.model;
        settings.max_tokens = input.max_tokens;
        settings.temperature = input.temperature;
        settings.consent_document_send = input.consent_document_send;
        settings::persist_to(self.config_path.as_deref(), &settings)
    }

    /// Записывает API-ключ (секрет) в конфиг. Пустая строка очищает ключ.
    ///
    /// Ключ не логируется и не возвращается командами — наружу идёт только `has_key`.
    pub fn set_api_key(&self, key: String) -> Result<(), CommandError> {
        let trimmed = key.trim().to_string();
        let mut settings = lock(&self.settings);
        settings.api_key = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        };
        settings::persist_to(self.config_path.as_deref(), &settings)
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
    fn reserve_rate_limit_slot(&self) -> Duration {
        let mut last = lock(&self.rate_limit);
        let wait = last
            .map(|previous| MIN_REQUEST_INTERVAL.saturating_sub(previous.elapsed()))
            .unwrap_or_default();
        *last = Some(Instant::now() + wait);
        wait
    }
}

/// Проверяет `base_url` (H3): разрешён только `https://`; `http://` — лишь для loopback
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

/// Ошибка настроек с единым кодом `ai_bad_settings`.
fn bad_settings(message: impl Into<String>) -> CommandError {
    CommandError::new(ErrorCode::AiBadSettings, message)
}

/// Хост из остатка URL после `://` (без userinfo, порта, пути и query).
fn host_of(rest: &str) -> &str {
    // Authority — до первого `/`, `?` или `#`; `@` внутри пути не является userinfo.
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    let host_port = authority
        .rsplit_once('@')
        .map(|(_, host)| host)
        .unwrap_or(authority);
    if let Some(stripped) = host_port.strip_prefix('[') {
        // IPv6 в скобках: `[::1]:8080` → `::1`.
        stripped.split(']').next().unwrap_or(stripped)
    } else {
        // Отбрасываем порт: `host:8080` → `host`.
        host_port.split(':').next().unwrap_or(host_port)
    }
}

/// Loopback-хост (`127.0.0.1`, `localhost`, `[::1]`).
fn is_loopback_host(host: &str) -> bool {
    let host = host.trim_matches(|c| c == '[' || c == ']');
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}

/// Выбор бэкенда по платформе: `reqwest` (Windows) либо заглушка (не-Windows).
fn default_backend() -> Arc<dyn AiTransport> {
    #[cfg(windows)]
    {
        Arc::new(crate::ai::backend::reqwest::ReqwestBackend::new())
    }
    #[cfg(not(windows))]
    {
        Arc::new(UnavailableBackend)
    }
}

/// Блокировка мьютекса без паники: под `panic = "abort"` отравление невозможно, но в тестах
/// (`panic = "unwind"`) берём внутреннее значение вместо `expect`.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Заглушка бэкенда для не-Windows: сетевой слой AI не собирается (ADR-0033 §2.1).
#[cfg(not(windows))]
struct UnavailableBackend;

#[cfg(not(windows))]
impl AiTransport for UnavailableBackend {
    fn chat<'a>(
        &'a self,
        _request: crate::ai::transport::ChatRequest,
        _on_delta: &'a mut (dyn FnMut(String) + Send),
    ) -> crate::ai::transport::BoxFuture<'a, Result<crate::ai::transport::ChatOutcome, CommandError>>
    {
        Box::pin(async {
            Err(CommandError::new(
                crate::error::ErrorCode::AiOffline,
                "AI-ассистент доступен только на Windows",
            ))
        })
    }

    fn test_connection(
        &self,
        _request: crate::ai::transport::TestRequest,
    ) -> crate::ai::transport::BoxFuture<'_, Result<Option<String>, CommandError>> {
        Box::pin(async {
            Err(CommandError::new(
                crate::error::ErrorCode::AiOffline,
                "AI-ассистент доступен только на Windows",
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::transport::{BoxFuture, ChatOutcome, ChatRequest, TestRequest};

    /// Фейковый бэкенд: эхо-ответ, чтобы проверить состояние без сети.
    struct EchoBackend;

    impl AiTransport for EchoBackend {
        fn chat<'a>(
            &'a self,
            request: ChatRequest,
            on_delta: &'a mut (dyn FnMut(String) + Send),
        ) -> BoxFuture<'a, Result<ChatOutcome, CommandError>> {
            Box::pin(async move {
                let text = request
                    .messages
                    .last()
                    .map(|message| message.content.clone())
                    .unwrap_or_default();
                on_delta(text.clone());
                Ok(ChatOutcome {
                    content: text,
                    usage: None,
                    finish_reason: None,
                })
            })
        }

        fn test_connection(
            &self,
            _request: TestRequest,
        ) -> BoxFuture<'_, Result<Option<String>, CommandError>> {
            Box::pin(async { Ok(Some("echo".to_string())) })
        }
    }

    /// Изолированный путь конфига в temp — тесты не трогают реальный профиль.
    fn temp_config(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("darmark-ai-state-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("config.json")
    }

    fn state_with(settings: AiSettings, config_path: Option<PathBuf>) -> AiState {
        AiState::with_config_path(settings, Arc::new(EchoBackend), config_path)
    }

    #[test]
    fn settings_update_keeps_key_and_persists_fields() {
        let path = temp_config("update");
        let state = state_with(
            AiSettings {
                api_key: Some("sk-keep".to_string()),
                ..AiSettings::default()
            },
            Some(path),
        );
        state
            .update_settings(AiSettingsInput {
                enabled: true,
                provider: "openai-compatible".to_string(),
                base_url: "https://example.test/v1".to_string(),
                model: "m".to_string(),
                max_tokens: 128,
                temperature: 0.1,
                consent_document_send: true,
            })
            .expect("настройки сохранены");

        let settings = state.settings();
        assert!(settings.enabled);
        assert_eq!(settings.model, "m");
        assert_eq!(settings.max_tokens, 128);
        assert_eq!(
            settings.api_key.as_deref(),
            Some("sk-keep"),
            "обновление настроек не трогает секрет"
        );
    }

    #[test]
    fn set_api_key_trims_and_clears_on_empty() {
        let path = temp_config("key");
        let state = state_with(AiSettings::default(), Some(path));
        state
            .set_api_key("  sk-new  ".to_string())
            .expect("ключ записан");
        assert_eq!(state.settings().api_key.as_deref(), Some("sk-new"));

        state
            .set_api_key("   ".to_string())
            .expect("пустой ключ очищает");
        assert!(state.settings().api_key.is_none());
    }

    #[test]
    fn backend_is_shared_by_arc() {
        let state = state_with(AiSettings::default(), None);
        let a = state.backend();
        let b = state.backend();
        assert!(
            Arc::ptr_eq(&a, &b),
            "бэкенд переиспользуется, не пересоздаётся"
        );
    }

    #[test]
    fn rate_limit_does_not_panic_without_config() {
        let state = state_with(AiSettings::default(), None);
        // Первый слот свободен (wait = 0), второй — уже занят (wait ≥ 0).
        assert!(state.reserve_rate_limit_slot().is_zero());
        let _ = state.reserve_rate_limit_slot();
    }

    #[test]
    fn update_settings_accepts_https() {
        let state = state_with(AiSettings::default(), None);
        let input = AiSettingsInput {
            enabled: true,
            provider: "openai-compatible".to_string(),
            base_url: "https://routerai.ru/api/v1".to_string(),
            model: "m".to_string(),
            max_tokens: 128,
            temperature: 0.1,
            consent_document_send: false,
        };
        assert!(state.update_settings(input).is_ok());
    }

    #[test]
    fn update_settings_accepts_http_loopback() {
        for base_url in [
            "http://127.0.0.1:8080/v1",
            "http://localhost:5173/v1",
            "http://[::1]:9000/v1",
        ] {
            let state = state_with(AiSettings::default(), None);
            let input = AiSettingsInput {
                enabled: true,
                provider: "openai-compatible".to_string(),
                base_url: base_url.to_string(),
                model: "m".to_string(),
                max_tokens: 128,
                temperature: 0.1,
                consent_document_send: false,
            };
            assert!(state.update_settings(input).is_ok(), "принят: {base_url}");
        }
    }

    #[test]
    fn update_settings_rejects_http_remote_and_bad_scheme() {
        for base_url in [
            "http://example.com/v1",
            "http://192.168.0.1/v1",
            "ftp://example.com",
            "example.com/v1",
        ] {
            let state = state_with(AiSettings::default(), None);
            let input = AiSettingsInput {
                enabled: true,
                provider: "openai-compatible".to_string(),
                base_url: base_url.to_string(),
                model: "m".to_string(),
                max_tokens: 128,
                temperature: 0.1,
                consent_document_send: false,
            };
            let err = state
                .update_settings(input)
                .expect_err(&format!("отклонён: {base_url}"));
            assert_eq!(err.code, ErrorCode::AiBadSettings, "код для {base_url}");
        }
    }

    #[test]
    fn validate_base_url_edge_cases() {
        assert!(validate_base_url("https://example.test").is_ok());
        assert!(validate_base_url("  https://example.test/v1  ").is_ok());
        assert!(validate_base_url("http://127.0.0.1").is_ok());
        assert!(validate_base_url("https://").is_err());
        assert!(validate_base_url("http://user:pass@localhost/v1").is_ok());
        assert!(validate_base_url("http://user:pass@evil.test/v1").is_err());
        assert!(validate_base_url("http://[::1]/v1").is_ok());
        assert!(validate_base_url("http://[2001:db8::1]/v1").is_err());
    }
}
