//! Конфигурация провайдеров LLM и настройки AI (секция `ai`, ADR-0035 §2.1).
//!
//! Провайдеры описываются **данными**, кода на провайдера нет (ADR-0035 §2.1). Секрет
//! `api_key` — локальный (ADR-0035 §2.3): приоритет над `env`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Настройки точки доступа провайдера (`settings` в разделе Custom `opencode.json`).
///
/// Поля названы как в `opencode.json` (`baseURL`/`apiKey`) с `alias` для legacy-написания
/// (ADR-0035 §2.1). Секрет `api_key` — локальный (ADR-0035 §2.3): приоритет над `env`.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderSettings {
    /// Точка доступа провайдера. **Обязательна** (ADR-0035 §2.1 п.2): отсутствие поля —
    /// ошибка разбора, а не молчаливый пустой дефолт. Пустое/невалидное значение отклоняет
    /// `validate_base_url` при резолве активной пары (`ai_bad_settings`).
    #[serde(rename = "baseURL", alias = "base_url")]
    pub base_url: String,
    #[serde(
        rename = "apiKey",
        alias = "api_key",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
}

/// Ручной `Debug`: секрет (`api_key`) маскируется и не попадает в `{:?}`/логи.
impl std::fmt::Debug for ProviderSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderSettings")
            .field("base_url", &self.base_url)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("timeout", &self.timeout)
            .finish()
    }
}

/// Модель провайдера (`models[id]` в Custom `opencode.json`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelEntry {
    pub name: String,
    /// Ключ модели у провайдера, если отличается от id в map (ADR-0035 §2.1).
    #[serde(
        default,
        rename = "modelID",
        alias = "model_id",
        skip_serializing_if = "Option::is_none"
    )]
    pub model_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
}

/// Провайдер LLM: описывается **данными**, кода на провайдера нет (ADR-0035 §2.1).
///
/// Обязательные поля (`name`, `package`, `settings.baseURL`) без serde-дефолта: их отсутствие —
/// ошибка разбора, а не молчаливый дефолт. Ловушка: `SettingsStore::load` при ошибке разбора
/// берёт дефолты (с записью в лог), поэтому реальной границей остаётся валидация `baseURL`
/// (`validate_base_url`) при резолве активной пары.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Отображаемое имя; обязательно (ADR-0035 §2.1 п.2).
    pub name: String,
    /// Упорядоченные env-переменные для ключа (первая заданная побеждает; §2.3).
    #[serde(default)]
    pub env: Vec<String>,
    /// Маркер протокола (`openai-compatible`), не npm-пакет (§2.1); обязателен.
    pub package: String,
    #[serde(default)]
    pub settings: ProviderSettings,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub body: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    pub models: BTreeMap<String, ModelEntry>,
}

/// Ручной `Debug`: надстройка запроса (`headers`/`body`) не разворачивается в логи —
/// печатаются только имена заголовков и полей тела (H4, ADR-0035 §2.6 п.16). Секрет
/// `settings.api_key` маскируется собственным `Debug` у [`ProviderSettings`].
impl std::fmt::Debug for ProviderConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderConfig")
            .field("name", &self.name)
            .field("env", &self.env)
            .field("package", &self.package)
            .field("settings", &self.settings)
            .field("headers", &self.headers.keys().collect::<Vec<_>>())
            .field("body", &self.body.keys().collect::<Vec<_>>())
            .field("models", &self.models)
            .finish()
    }
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            env: Vec::new(),
            package: "openai-compatible".to_string(),
            settings: ProviderSettings::default(),
            headers: BTreeMap::new(),
            body: serde_json::Map::new(),
            models: BTreeMap::new(),
        }
    }
}

/// Идентификатор дефолтного провайдера (ADR-0032/0035 §2.8).
pub const DEFAULT_PROVIDER_ID: &str = "routerai";
/// Дефолтная точка доступа (ADR-0032).
pub const DEFAULT_BASE_URL: &str = "https://routerai.ru/api/v1";
/// Дефолтная модель (ADR-0032).
pub const DEFAULT_MODEL: &str = "deepseek/deepseek-v4.1-flash";

/// Настройки AI (секция `ai` в `config.json`): map провайдеров + активная пара.
///
/// Секреты — в `providers[id].settings.api_key`. `Default` — opt-in: `enabled = false`,
/// согласие не выдано; один провайдер `routerai` (ADR-0035 §2.8).
#[derive(Clone, PartialEq, Serialize)]
pub struct AiSettings {
    pub enabled: bool,
    pub active_provider: String,
    pub active_model: String,
    pub consent_document_send: bool,
    pub max_tokens: u32,
    pub temperature: f32,
    pub providers: BTreeMap<String, ProviderConfig>,
    /// Легаси-секция разобрана и требует перезаписи конфига (ADR-0035 §2.8).
    #[serde(skip)]
    pub migrated_from_legacy: bool,
}

impl Default for AiSettings {
    fn default() -> Self {
        let mut models = BTreeMap::new();
        models.insert(
            DEFAULT_MODEL.to_string(),
            ModelEntry {
                name: "DeepSeek V4.1 Flash".to_string(),
                model_id: None,
                disabled: None,
            },
        );
        let mut providers = BTreeMap::new();
        providers.insert(
            DEFAULT_PROVIDER_ID.to_string(),
            ProviderConfig {
                name: "RouterAI".to_string(),
                env: vec!["ROUTERAI_API_KEY".to_string()],
                package: "openai-compatible".to_string(),
                settings: ProviderSettings {
                    base_url: DEFAULT_BASE_URL.to_string(),
                    api_key: None,
                    timeout: Some(30000),
                },
                headers: BTreeMap::new(),
                body: serde_json::Map::new(),
                models,
            },
        );
        Self {
            enabled: false,
            active_provider: DEFAULT_PROVIDER_ID.to_string(),
            active_model: DEFAULT_MODEL.to_string(),
            consent_document_send: false,
            max_tokens: 32768,
            temperature: 0.7,
            providers,
            migrated_from_legacy: false,
        }
    }
}

/// Поля, которые могут прийти как в новой схеме (`providers`), так и в legacy (ADR-0032).
#[derive(Deserialize)]
struct AiSettingsHelper {
    enabled: Option<bool>,
    active_provider: Option<String>,
    active_model: Option<String>,
    consent_document_send: Option<bool>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    providers: Option<BTreeMap<String, ProviderConfig>>,
    provider: Option<String>,
    base_url: Option<String>,
    model: Option<String>,
    api_key: Option<String>,
}

impl AiSettings {
    /// Собирает настройки из `Helper`: новая схема — как есть; legacy — в первый провайдер.
    fn from_helper(helper: AiSettingsHelper) -> Self {
        let defaults = AiSettings::default();
        let enabled = helper.enabled.unwrap_or(defaults.enabled);
        let consent_document_send = helper
            .consent_document_send
            .unwrap_or(defaults.consent_document_send);
        let max_tokens = helper.max_tokens.unwrap_or(defaults.max_tokens);
        let temperature = helper.temperature.unwrap_or(defaults.temperature);

        if let Some(providers) = helper.providers.filter(|map| !map.is_empty()) {
            let active_provider = helper
                .active_provider
                .unwrap_or_else(|| default_active_provider(&providers));
            let active_model = helper
                .active_model
                .unwrap_or_else(|| default_active_model(&providers, &active_provider));
            return AiSettings {
                enabled,
                active_provider,
                active_model,
                consent_document_send,
                max_tokens,
                temperature,
                providers,
                migrated_from_legacy: false,
            };
        }

        let has_legacy = helper.provider.is_some()
            || helper.base_url.is_some()
            || helper.model.is_some()
            || helper.api_key.is_some();
        if !has_legacy {
            return AiSettings {
                enabled,
                consent_document_send,
                max_tokens,
                temperature,
                ..defaults
            };
        }

        let id = helper
            .provider
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| defaults.active_provider.clone());
        let base_url = helper
            .base_url
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
        let model = helper
            .model
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| defaults.active_model.clone());
        let api_key = helper.api_key.filter(|value| !value.trim().is_empty());

        let mut models = BTreeMap::new();
        models.insert(
            model.clone(),
            ModelEntry {
                name: model.clone(),
                model_id: None,
                disabled: None,
            },
        );
        let mut providers = BTreeMap::new();
        providers.insert(
            id.clone(),
            ProviderConfig {
                name: id.clone(),
                env: Vec::new(),
                package: "openai-compatible".to_string(),
                settings: ProviderSettings {
                    base_url,
                    api_key,
                    timeout: Some(30000),
                },
                headers: BTreeMap::new(),
                body: serde_json::Map::new(),
                models,
            },
        );
        AiSettings {
            enabled,
            active_provider: id,
            active_model: model,
            consent_document_send,
            max_tokens,
            temperature,
            providers,
            migrated_from_legacy: true,
        }
    }

    /// Конфигурация активного провайдера, если id есть в map.
    pub fn active_config(&self) -> Option<&ProviderConfig> {
        self.providers.get(&self.active_provider)
    }
}

/// Активный провайдер по умолчанию, если поле не задано: `routerai`, иначе первый ключ.
fn default_active_provider(providers: &BTreeMap<String, ProviderConfig>) -> String {
    if providers.contains_key(DEFAULT_PROVIDER_ID) {
        return DEFAULT_PROVIDER_ID.to_string();
    }
    providers
        .keys()
        .next()
        .cloned()
        .unwrap_or_else(|| DEFAULT_PROVIDER_ID.to_string())
}

/// Активная модель по умолчанию, если поле не задано: дефолтная, иначе первая у провайдера.
fn default_active_model(
    providers: &BTreeMap<String, ProviderConfig>,
    active_provider: &str,
) -> String {
    if let Some(provider) = providers.get(active_provider) {
        if provider.models.contains_key(DEFAULT_MODEL) {
            return DEFAULT_MODEL.to_string();
        }
        if let Some(model) = provider.models.keys().next() {
            return model.clone();
        }
    }
    DEFAULT_MODEL.to_string()
}

/// Ручной `Deserialize`: терпимо к legacy-схеме (ADR-0035 §2.8) и недостающим полям.
impl<'de> Deserialize<'de> for AiSettings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let helper = AiSettingsHelper::deserialize(deserializer)?;
        Ok(AiSettings::from_helper(helper))
    }
}

/// Ручной `Debug`: секреты всех провайдеров маскируются (H4).
impl std::fmt::Debug for AiSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AiSettings")
            .field("enabled", &self.enabled)
            .field("active_provider", &self.active_provider)
            .field("active_model", &self.active_model)
            .field("consent_document_send", &self.consent_document_send)
            .field("max_tokens", &self.max_tokens)
            .field("temperature", &self.temperature)
            .field("providers", &self.providers)
            .field("migrated_from_legacy", &self.migrated_from_legacy)
            .finish()
    }
}

/// Резолв API-ключа провайдера: непустой `settings.apiKey` приоритетнее `env`; из `env`
/// берётся первая заданная непустая переменная (ADR-0035 §2.3). Чистая — `env` инъектируется.
pub fn resolve_api_key<F>(provider: &ProviderConfig, env: F) -> Option<String>
where
    F: Fn(&str) -> Option<String>,
{
    if let Some(key) = provider.settings.api_key.as_deref() {
        let trimmed = key.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    provider.env.iter().find_map(|name| {
        env(name)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    })
}
