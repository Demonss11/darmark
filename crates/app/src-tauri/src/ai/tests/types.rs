//! Тесты типов [`super::super::types`].

use std::collections::BTreeMap;

use crate::ai::tool_calls::ToolCall;
use crate::ai::types::{
    resolve_api_key, AiMessage, AiResponse, AiSettings, AiSettingsView, AiUsage, ModelEntry,
    ProviderConfig, ProviderSettings,
};

fn provider_with_api_key(api_key: &str) -> ProviderConfig {
    ProviderConfig {
        settings: ProviderSettings {
            base_url: "https://example.test/v1".to_string(),
            api_key: Some(api_key.to_string()),
            timeout: None,
        },
        ..ProviderConfig::default()
    }
}

#[test]
fn defaults_are_opt_in() {
    let settings = AiSettings::default();
    assert!(!settings.enabled, "AI по умолчанию выключен");
    assert!(
        !settings.consent_document_send,
        "согласие по умолчанию не выдано"
    );
    assert_eq!(settings.active_provider, "routerai");
    assert_eq!(settings.active_model, "deepseek/deepseek-v4.1-flash");
    assert_eq!(settings.max_tokens, 32768);
    assert!((settings.temperature - 0.7).abs() < f32::EPSILON);
    assert!(!settings.migrated_from_legacy);
    let provider = settings.active_config().expect("дефолтный провайдер есть");
    assert_eq!(provider.settings.base_url, "https://routerai.ru/api/v1");
    assert!(provider.settings.api_key.is_none());
    assert!(provider.models.contains_key("deepseek/deepseek-v4.1-flash"));
}

#[test]
fn provider_settings_use_custom_field_names() {
    let settings = ProviderSettings {
        base_url: "https://example.test/v1".to_string(),
        api_key: Some("sk-x".to_string()),
        timeout: Some(5000),
    };
    let json = serde_json::to_value(&settings).expect("сериализация");
    assert_eq!(json["baseURL"], "https://example.test/v1");
    assert_eq!(json["apiKey"], "sk-x");
    assert_eq!(json["timeout"], 5000);
    assert!(
        json.get("base_url").is_none(),
        "сериализуется именно baseURL: {json}"
    );
}

#[test]
fn provider_settings_accept_legacy_aliases() {
    let settings: ProviderSettings =
        serde_json::from_str(r#"{"base_url":"https://legacy.test/v1","api_key":"sk-legacy"}"#)
            .expect("legacy-алиасы приняты");
    assert_eq!(settings.base_url, "https://legacy.test/v1");
    assert_eq!(settings.api_key.as_deref(), Some("sk-legacy"));
    assert!(settings.timeout.is_none());
}

#[test]
fn provider_config_requires_name_package_and_base_url() {
    let missing_name = r#"{
        "package": "openai-compatible",
        "settings": { "baseURL": "https://example.test/v1" }
    }"#;
    assert!(
        serde_json::from_str::<ProviderConfig>(missing_name).is_err(),
        "name обязателен"
    );

    let missing_package = r#"{
        "name": "Example",
        "settings": { "baseURL": "https://example.test/v1" }
    }"#;
    assert!(
        serde_json::from_str::<ProviderConfig>(missing_package).is_err(),
        "package обязателен"
    );

    let missing_base_url = r#"{
        "name": "Example",
        "package": "openai-compatible",
        "settings": { "apiKey": "sk-x" }
    }"#;
    assert!(
        serde_json::from_str::<ProviderConfig>(missing_base_url).is_err(),
        "settings.baseURL обязателен"
    );

    let complete = r#"{
        "name": "Example",
        "package": "openai-compatible",
        "settings": { "baseURL": "https://example.test/v1" }
    }"#;
    let config: ProviderConfig = serde_json::from_str(complete).expect("полная схема разобрана");
    assert_eq!(config.name, "Example");
    assert_eq!(config.settings.base_url, "https://example.test/v1");
}

#[test]
fn model_entry_serializes_model_id_and_disabled() {
    let entry = ModelEntry {
        name: "DeepSeek".to_string(),
        model_id: Some("deepseek-v4".to_string()),
        disabled: Some(true),
    };
    let json = serde_json::to_value(&entry).expect("сериализация");
    assert_eq!(json["name"], "DeepSeek");
    assert_eq!(json["modelID"], "deepseek-v4");
    assert_eq!(json["disabled"], true);

    let minimal: ModelEntry = serde_json::from_str(r#"{"name":"M"}"#).expect("разбор");
    assert!(minimal.model_id.is_none());
    assert!(minimal.disabled.is_none());
}

#[test]
fn legacy_section_deserializes_into_providers() {
    let json = r#"{
        "enabled": true,
        "provider": "openai-compatible",
        "base_url": "https://legacy.test/v1",
        "model": "legacy/model",
        "api_key": "sk-legacy",
        "max_tokens": 100,
        "temperature": 0.2
    }"#;
    let settings: AiSettings = serde_json::from_str(json).expect("legacy разобран");
    assert!(settings.migrated_from_legacy, "отмечена миграция");
    assert_eq!(settings.active_provider, "openai-compatible");
    assert_eq!(settings.active_model, "legacy/model");
    assert_eq!(settings.max_tokens, 100);
    let provider = settings.active_config().expect("провайдер создан");
    assert_eq!(provider.settings.base_url, "https://legacy.test/v1");
    assert_eq!(provider.settings.api_key.as_deref(), Some("sk-legacy"));
    assert!(provider.models.contains_key("legacy/model"));
}

#[test]
fn legacy_section_without_provider_uses_routerai_id() {
    let json = r#"{"base_url":"https://legacy.test/v1","model":"legacy/model"}"#;
    let settings: AiSettings = serde_json::from_str(json).expect("legacy разобран");
    assert!(settings.migrated_from_legacy);
    assert_eq!(settings.active_provider, "routerai");
    assert!(settings.providers.contains_key("routerai"));
}

#[test]
fn new_format_deserializes_without_migration() {
    let json = r#"{
        "enabled": true,
        "active_provider": "corp",
        "active_model": "corp/model",
        "providers": {
            "corp": {
                "name": "Corporate",
                "package": "openai-compatible",
                "settings": { "baseURL": "https://corp.test/v1", "apiKey": "sk-corp" },
                "models": { "corp/model": { "name": "Corp Model" } }
            }
        }
    }"#;
    let settings: AiSettings = serde_json::from_str(json).expect("новая схема разобрана");
    assert!(!settings.migrated_from_legacy);
    assert_eq!(settings.active_provider, "corp");
    assert_eq!(settings.active_model, "corp/model");
    assert!(settings.providers.contains_key("corp"));
    assert_eq!(
        settings.providers["corp"].settings.api_key.as_deref(),
        Some("sk-corp")
    );
}

#[test]
fn missing_fields_fall_back_to_defaults() {
    let settings: AiSettings = serde_json::from_str("{\"enabled\": true}").expect("разбор");
    assert!(settings.enabled);
    assert!(!settings.migrated_from_legacy);
    assert_eq!(settings.active_provider, "routerai");
    assert_eq!(settings.active_model, "deepseek/deepseek-v4.1-flash");
    assert_eq!(settings.max_tokens, 32768);
}

#[test]
fn view_hides_secret_for_every_provider() {
    use crate::ai::history::env_lock;

    let _guard = env_lock();
    let previous = std::env::var_os("ROUTERAI_API_KEY");
    std::env::remove_var("ROUTERAI_API_KEY");

    let mut settings = AiSettings::default();
    settings
        .providers
        .insert("corp".to_string(), provider_with_api_key("sk-super-secret"));
    let view = AiSettingsView::from(&settings);

    
    match previous {
        Some(value) => std::env::set_var("ROUTERAI_API_KEY", value),
        None => std::env::remove_var("ROUTERAI_API_KEY"),
    }

    assert_eq!(view.providers.len(), 2);
    let json = serde_json::to_string(&view).expect("view сериализуется");
    assert!(!json.contains("api_key"), "в виде нет api_key: {json}");
    assert!(!json.contains("apiKey"), "в виде нет apiKey: {json}");
    assert!(
        !json.contains("sk-super-secret"),
        "в виде нет значения: {json}"
    );
    let corp = view
        .providers
        .iter()
        .find(|provider| provider.id == "corp")
        .expect("провайдер в виде");
    assert!(corp.settings.has_key, "вид сообщает о наличии ключа");
    let routerai = view
        .providers
        .iter()
        .find(|provider| provider.id == "routerai")
        .expect("дефолтный провайдер");
    assert!(
        !routerai.settings.has_key,
        "дефолтный ключ не задан и env на время теста очищен"
    );
}

#[test]
fn resolve_api_key_prefers_settings_then_env() {
    let provider = provider_with_api_key("sk-config");
    assert_eq!(
        resolve_api_key(&provider, |_| Some("sk-env".to_string())).as_deref(),
        Some("sk-config"),
        "настройка приоритетнее env"
    );

    let mut blank = provider_with_api_key("   ");
    blank.env = vec!["FIRST".to_string(), "SECOND".to_string()];
    assert_eq!(
        resolve_api_key(&blank, |name| match name {
            "FIRST" => None,
            "SECOND" => Some("sk-second".to_string()),
            _ => None,
        })
        .as_deref(),
        Some("sk-second"),
        "берётся первая заданная env-переменная"
    );

    blank.env.clear();
    assert_eq!(resolve_api_key(&blank, |_| None), None, "ключа нет");
}

#[test]
fn debug_masks_secret() {
    let mut settings = AiSettings::default();
    settings
        .providers
        .insert("corp".to_string(), provider_with_api_key("sk-secret-1234"));
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
fn debug_hides_headers_and_body_values() {
    let mut provider = provider_with_api_key("sk-secret-1234");
    provider.headers.insert(
        "X-Corporate-Token".to_string(),
        "corp-token-9999".to_string(),
    );
    provider
        .body
        .insert("tenant".to_string(), serde_json::json!("acme-secret"));

    let rendered = format!("{provider:?}");
    assert!(
        !rendered.contains("corp-token-9999"),
        "значение заголовка не должно попадать в Debug: {rendered}"
    );
    assert!(
        !rendered.contains("acme-secret"),
        "значение тела не должно попадать в Debug: {rendered}"
    );
    assert!(
        rendered.contains("X-Corporate-Token"),
        "имя заголовка остаётся видимым: {rendered}"
    );
    assert!(
        rendered.contains("tenant"),
        "имя поля тела остаётся видимым: {rendered}"
    );

    let settings = AiSettings {
        providers: BTreeMap::from([("corp".to_string(), provider)]),
        ..AiSettings::default()
    };
    let rendered = format!("{settings:?}");
    assert!(
        !rendered.contains("corp-token-9999"),
        "Debug настроек не должен раскрывать заголовок: {rendered}"
    );
    assert!(
        !rendered.contains("acme-secret"),
        "Debug настроек не должен раскрывать тело: {rendered}"
    );
}

#[test]
fn ai_response_serializes_finish_reason() {
    let response = AiResponse {
        message: AiMessage {
            role: "assistant".to_string(),
            content: "обрезано".to_string(),
            ..AiMessage::default()
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
            ..AiMessage::default()
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


#[test]
fn ai_message_omits_absent_tool_fields() {
    let message = AiMessage {
        role: "user".to_string(),
        content: "hi".to_string(),
        ..AiMessage::default()
    };
    let json = serde_json::to_string(&message).expect("AiMessage сериализуется");
    assert_eq!(
        json, r#"{"role":"user","content":"hi"}"#,
        "нет null-полей без вызовов инструментов: {json}"
    );
}


#[test]
fn ai_message_roundtrips_tool_call_fields() {
    let message = AiMessage {
        role: "assistant".to_string(),
        content: String::new(),
        tool_calls: Some(vec![ToolCall {
            id: "call-1".to_string(),
            name: "fake_tool".to_string(),
            arguments: serde_json::json!({"a": 1}),
        }]),
        ..AiMessage::default()
    };
    let json = serde_json::to_string(&message).expect("AiMessage сериализуется");
    assert!(
        json.contains("\"tool_calls\""),
        "вызовы сохраняются: {json}"
    );

    let restored: AiMessage = serde_json::from_str(&json).expect("AiMessage разбирается");
    assert_eq!(restored, message, "поля tool-loop переживают round-trip");
}
