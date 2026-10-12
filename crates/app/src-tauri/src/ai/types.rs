//! Типы контракта AI-ассистента (IPC + конфиг).
//!
//! Здесь только данные: сообщения чата, ответ, скиллы, настройки и их проекции для
//! фронтенда. Секрет (`api_key`) живёт в [`AiSettings`] и пишется в `config.json`
//! (секция `ai`, D11), но **никогда** не попадает в [`AiSettingsView`] — наружу идёт
//! лишь `has_key` (ADR-0032/0034).

use serde::{Deserialize, Serialize};

/// Сообщение чата. `role` — `"system" | "user" | "assistant"`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiMessage {
    pub role: String,
    pub content: String,
}

/// Токены использования ответа (показ расходов, ADR-0032).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

/// Ответ модели: итоговое сообщение, (если провайдер прислал) `usage` и причина завершения.
#[derive(Clone, Debug, Serialize)]
pub struct AiResponse {
    pub message: AiMessage,
    pub usage: Option<AiUsage>,
    /// Причина завершения стрима (`"stop"`, `"length"` и т.д.); `None`, если не пришла.
    pub finish_reason: Option<String>,
}

/// Готовый скилл (промт) из встроенной библиотеки (ADR-0032).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AiSkill {
    pub id: String,
    pub title: String,
    pub description: String,
    /// Шаблон промта; подстановки — `{{doc}}` (текст документа) и `{{input}}` (описание из чата).
    pub prompt_template: String,
    /// Категория: `document` или `plugins`.
    pub category: String,
}

/// Настройки AI (секция `ai` в `config.json`). Включает **секрет** `api_key`.
///
/// `Default` — opt-in: `enabled = false`, согласие не выдано. Пропущенные поля в старом
/// конфиге заполняются значениями по умолчанию (`#[serde(default)]`).
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AiSettings {
    /// Включаемость без бинарника (ADR-0034 §2.3); по умолчанию выключено.
    pub enabled: bool,
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f32,
    /// Согласие на отправку содержимого документа в облако (ADR-0032 §2.3).
    pub consent_document_send: bool,
    /// СЕКРЕТ: не сериализуется в ответные DTO и не логируется. `None`/пусто — ключа нет.
    pub api_key: Option<String>,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: "openai-compatible".to_string(),
            base_url: "https://routerai.ru/api/v1".to_string(),
            model: "deepseek/deepseek-v4.1-flash".to_string(),
            // Запас на reasoning: модель `deepseek-v4.1-flash` тратит львиную долю бюджета на
            // внутренние рассуждения (живой замер: view-плагин ≈15.5k reasoning + ≈4.5k ответ).
            // При 4096/8192 ответ обрекается на `finish_reason: "length"` и приходит пустым
            // (см. docs/FINDINGS.md §2.3–2.4). Платим только за реально сгенерированные токены.
            max_tokens: 32768,
            temperature: 0.7,
            consent_document_send: false,
            api_key: None,
        }
    }
}

impl AiSettings {
    /// Задан ли непустой API-ключ (для `has_key` и гейта).
    pub fn has_key(&self) -> bool {
        self.api_key
            .as_deref()
            .map(|key| !key.trim().is_empty())
            .unwrap_or(false)
    }
}

/// Ручной `Debug`: секрет (`api_key`) маскируется и не попадает в `{:?}`/логи (H4).
impl std::fmt::Debug for AiSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AiSettings")
            .field("enabled", &self.enabled)
            .field("provider", &self.provider)
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("max_tokens", &self.max_tokens)
            .field("temperature", &self.temperature)
            .field("consent_document_send", &self.consent_document_send)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

/// Проекция настроек для фронтенда: без `api_key`, но с `has_key` (ADR-0034).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AiSettingsView {
    pub enabled: bool,
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f32,
    pub consent_document_send: bool,
    pub has_key: bool,
}

impl From<&AiSettings> for AiSettingsView {
    fn from(settings: &AiSettings) -> Self {
        Self {
            enabled: settings.enabled,
            provider: settings.provider.clone(),
            base_url: settings.base_url.clone(),
            model: settings.model.clone(),
            max_tokens: settings.max_tokens,
            temperature: settings.temperature,
            consent_document_send: settings.consent_document_send,
            has_key: settings.has_key(),
        }
    }
}

/// Входные настройки от фронтенда: без ключа (ключ пишется отдельной командой).
#[derive(Clone, Debug, Deserialize)]
pub struct AiSettingsInput {
    pub enabled: bool,
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f32,
    pub consent_document_send: bool,
}

/// Результат лёгкой проверки соединения (`ai_test_connection`).
#[derive(Clone, Debug, Serialize)]
pub struct AiTestResult {
    pub ok: bool,
    /// Модель, если провайдер вернул список (`GET /models`).
    pub model: Option<String>,
    /// Понятное сообщение об ошибке (ключ/сеть/статус); `None` при успехе.
    pub error: Option<String>,
}

/// Одна операция правки документа (harness, ADR-0034): замена `[start, end)` на `text`.
///
/// Смещения — **байтовые** UTF-8 (как `data-md` и `DocumentStore::apply_edit`, D5).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiEditOp {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

/// Предложенный дифф документа: исходный/новый текст, операции и база для rev-guard.
#[derive(Clone, Debug, Serialize)]
pub struct AiDiff {
    pub doc_id: String,
    /// Ревизия документа, на которой посчитан дифф (проверяется в `ai_apply_edit`).
    pub base_rev: u64,
    pub original: String,
    pub modified: String,
    pub ops: Vec<AiEditOp>,
}

/// Результат валидации плагина (harness, ADR-0034/ADR-0026): статус, понятные ошибки и сырой отчёт CLI.
#[derive(Clone, Debug, Serialize)]
pub struct AiValidationResult {
    pub ok: bool,
    pub errors: Vec<String>,
    /// Сырой JSON-отчёт `darmark-plugin test --json`, если CLI доступен и отчёт версионирован;
    /// `None` — результат получен fallback-путём.
    pub report: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_opt_in() {
        let settings = AiSettings::default();
        assert!(!settings.enabled, "AI по умолчанию выключен");
        assert!(
            !settings.consent_document_send,
            "согласие по умолчанию не выдано"
        );
        assert_eq!(settings.provider, "openai-compatible");
        assert_eq!(settings.base_url, "https://routerai.ru/api/v1");
        assert_eq!(settings.model, "deepseek/deepseek-v4.1-flash");
        assert_eq!(settings.max_tokens, 32768);
        assert!((settings.temperature - 0.7).abs() < f32::EPSILON);
        assert!(settings.api_key.is_none());
        assert!(!settings.has_key());
    }

    #[test]
    fn empty_key_is_not_a_key() {
        let settings = AiSettings {
            api_key: Some("   ".to_string()),
            ..AiSettings::default()
        };
        assert!(!settings.has_key(), "пробельный ключ не считается заданным");
    }

    #[test]
    fn view_hides_secret_but_reports_presence() {
        let without_key = AiSettings::default();
        assert!(!AiSettingsView::from(&without_key).has_key);

        let with_key = AiSettings {
            api_key: Some("sk-secret-1234".to_string()),
            ..AiSettings::default()
        };
        let view = AiSettingsView::from(&with_key);
        assert!(view.has_key);

        // Секрет не утекает в сериализованную проекцию (контракт с фронтендом).
        let json = serde_json::to_string(&view).expect("view сериализуется");
        assert!(!json.contains("api_key"), "в виде нет поля api_key: {json}");
        assert!(
            !json.contains("sk-secret"),
            "в виде нет значения ключа: {json}"
        );
        assert!(
            json.contains("\"has_key\":true"),
            "вид сообщает о наличии ключа"
        );
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        // Старый конфиг без секции `ai`-полей не должен ронять разбор.
        let settings: AiSettings = serde_json::from_str("{\"enabled\": true}").expect("разбор");
        assert!(settings.enabled);
        assert_eq!(settings.model, "deepseek/deepseek-v4.1-flash");
        assert_eq!(settings.max_tokens, 32768);
        assert!(settings.api_key.is_none());
    }

    #[test]
    fn debug_masks_secret() {
        let settings = AiSettings {
            api_key: Some("sk-secret-1234".to_string()),
            ..AiSettings::default()
        };
        let rendered = format!("{settings:?}");
        assert!(
            !rendered.contains("sk-secret"),
            "секрет не должен попадать в Debug: {rendered}"
        );
        assert!(
            rendered.contains("<redacted>"),
            "секрет маскируется в Debug: {rendered}"
        );
    }

    #[test]
    fn ai_response_serializes_finish_reason() {
        let response = AiResponse {
            message: AiMessage {
                role: "assistant".to_string(),
                content: "обрезано".to_string(),
            },
            usage: Some(AiUsage {
                prompt_tokens: 10,
                completion_tokens: 20,
            }),
            finish_reason: Some("length".to_string()),
        };
        let json = serde_json::to_string(&response).expect("AiResponse сериализуется");
        assert!(
            json.contains("\"finish_reason\":\"length\""),
            "finish_reason сериализуется: {json}"
        );
    }

    #[test]
    fn ai_response_serializes_none_finish_reason() {
        let response = AiResponse {
            message: AiMessage {
                role: "assistant".to_string(),
                content: "ok".to_string(),
            },
            usage: None,
            finish_reason: None,
        };
        let json = serde_json::to_string(&response).expect("AiResponse сериализуется");
        assert!(
            json.contains("\"finish_reason\":null"),
            "отсутствующий finish_reason сериализуется как null: {json}"
        );
    }
}
