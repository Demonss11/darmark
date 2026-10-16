//! Тесты состояния [`super::super::state`].

use crate::ai::state::{host_for, host_of_url, resolve_active, validate_base_url, AiState};
use crate::ai::tool_loop::ToolsCapability;
use crate::ai::transport::{BoxFuture, ChatOutcome, ChatRequest, TestRequest};
use crate::ai::types::{
    AiSettings, AiSettingsInput, ModelEntry, ModelInput, ProviderConfig, ProviderInput,
    ProviderSettings, ProviderSettingsInput,
};
use crate::error::{CommandError, ErrorCode};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Фейковый бэкенд: эхо-ответ, чтобы проверить состояние без сети.
struct EchoBackend;

impl crate::ai::transport::AiTransport for EchoBackend {
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
                tool_calls: Vec::new(),
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

/// Вход провайдера для `update_settings` с одним ключом модели.
fn provider_input(id: &str, base_url: &str, model: &str) -> ProviderInput {
    ProviderInput {
        id: id.to_string(),
        name: id.to_string(),
        env: Vec::new(),
        settings: ProviderSettingsInput {
            base_url: base_url.to_string(),
            timeout: None,
        },
        headers: std::collections::BTreeMap::new(),
        body: serde_json::Map::new(),
        models: vec![ModelInput {
            id: model.to_string(),
            name: model.to_string(),
            model_id: None,
            disabled: None,
        }],
    }
}

fn input(
    active_provider: &str,
    active_model: &str,
    providers: Vec<ProviderInput>,
) -> AiSettingsInput {
    AiSettingsInput {
        enabled: true,
        active_provider: active_provider.to_string(),
        active_model: active_model.to_string(),
        max_tokens: 128,
        temperature: 0.1,
        consent_document_send: false,
        providers,
    }
}

/// Путь, где persist гарантированно падает: `config.json` — существующая директория,
/// `rename` временного файла на неё отклоняется ОС.
fn unwritable_config(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "darmark-ai-rollback-{}-{}",
        std::process::id(),
        name
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("config.json")).unwrap();
    dir.join("config.json")
}

#[test]
fn settings_update_keeps_key_and_persists_fields() {
    let path = temp_config("update");
    let mut settings = AiSettings::default();
    settings
        .providers
        .get_mut("routerai")
        .unwrap()
        .settings
        .api_key = Some("sk-keep".to_string());
    let state = state_with(settings, Some(path));

    state
        .update_settings(input(
            "routerai",
            "m",
            vec![provider_input("routerai", "https://example.test/v1", "m")],
        ))
        .expect("настройки сохранены");

    let settings = state.settings();
    assert!(settings.enabled);
    assert_eq!(settings.active_model, "m");
    assert_eq!(settings.max_tokens, 128);
    assert_eq!(
        settings.providers["routerai"].settings.api_key.as_deref(),
        Some("sk-keep"),
        "обновление настроек не трогает секрет"
    );
}

#[test]
fn update_settings_rolls_back_memory_on_persist_failure() {
    let path = unwritable_config("update");
    let mut settings = AiSettings::default();
    settings
        .providers
        .get_mut("routerai")
        .unwrap()
        .settings
        .api_key = Some("sk-keep".to_string());
    let state = state_with(settings, Some(path));

    let err = state
        .update_settings(input(
            "routerai",
            "m",
            vec![provider_input("routerai", "https://example.test/v1", "m")],
        ))
        .expect_err("запись в директорию падает");
    assert_eq!(err.code, ErrorCode::Io);

    let settings = state.settings();
    assert!(!settings.enabled, "сбой записи не включил AI в памяти");
    assert_eq!(settings.active_model, "deepseek/deepseek-v4.1-flash");
    assert!(
        settings.providers["routerai"]
            .models
            .contains_key("deepseek/deepseek-v4.1-flash"),
        "модели не изменились"
    );
    assert!(
        !settings.providers["routerai"].models.contains_key("m"),
        "сбой записи не добавил новую модель в память"
    );
    assert_eq!(
        settings.providers["routerai"].settings.api_key.as_deref(),
        Some("sk-keep")
    );
}

#[test]
fn set_api_key_is_scoped_to_provider() {
    let path = temp_config("key");
    let mut settings = AiSettings::default();
    settings.providers.insert(
        "corp".to_string(),
        ProviderConfig {
            name: "Corp".to_string(),
            settings: ProviderSettings {
                base_url: "https://corp.test/v1".to_string(),
                api_key: None,
                timeout: None,
            },
            ..ProviderConfig::default()
        },
    );
    let state = state_with(settings, Some(path));

    state
        .set_api_key("corp".to_string(), "  sk-corp  ".to_string())
        .expect("ключ записан");
    let settings = state.settings();
    assert_eq!(
        settings.providers["corp"].settings.api_key.as_deref(),
        Some("sk-corp")
    );
    assert!(
        settings.providers["routerai"].settings.api_key.is_none(),
        "ключ не задел другого провайдера"
    );

    state
        .set_api_key("corp".to_string(), "   ".to_string())
        .expect("пустой ключ очищает");
    assert!(state.settings().providers["corp"]
        .settings
        .api_key
        .is_none());
}

#[test]
fn set_api_key_rolls_back_memory_on_persist_failure() {
    let path = unwritable_config("key");
    let state = state_with(AiSettings::default(), Some(path));

    let err = state
        .set_api_key("routerai".to_string(), "sk-new".to_string())
        .expect_err("запись в директорию падает");
    assert_eq!(err.code, ErrorCode::Io);

    assert!(
        state.settings().providers["routerai"]
            .settings
            .api_key
            .is_none(),
        "сбой записи не сохранил ключ в памяти"
    );
}

#[test]
fn set_api_key_rejects_unknown_provider() {
    let state = state_with(AiSettings::default(), None);
    let err = state
        .set_api_key("nope".to_string(), "sk".to_string())
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::AiBadSettings);
}

#[test]
fn backend_is_shared_by_arc() {
    let state = state_with(AiSettings::default(), None);
    let a = state.backend();
    let b = state.backend();
    assert!(Arc::ptr_eq(&a, &b), "бэкенд переиспользуется");
}

#[test]
fn rate_limit_does_not_panic_without_config() {
    let state = state_with(AiSettings::default(), None);
    assert!(state.reserve_rate_limit_slot().is_zero());
    let _ = state.reserve_rate_limit_slot();
}

#[test]
fn update_settings_accepts_https_and_http_loopback() {
    for base_url in [
        "https://routerai.ru/api/v1",
        "http://127.0.0.1:8080/v1",
        "http://localhost:5173/v1",
        "http://[::1]:9000/v1",
    ] {
        let state = state_with(AiSettings::default(), None);
        let result = state.update_settings(input(
            "routerai",
            "m",
            vec![provider_input("routerai", base_url, "m")],
        ));
        assert!(result.is_ok(), "принят: {base_url}");
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
        let err = state
            .update_settings(input(
                "routerai",
                "m",
                vec![provider_input("routerai", base_url, "m")],
            ))
            .expect_err(&format!("отклонён: {base_url}"));
        assert_eq!(err.code, ErrorCode::AiBadSettings, "код для {base_url}");
    }
}

#[test]
fn update_settings_rejects_unknown_active_provider() {
    let state = state_with(AiSettings::default(), None);
    let err = state
        .update_settings(input(
            "ghost",
            "m",
            vec![provider_input("routerai", "https://example.test/v1", "m")],
        ))
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::AiBadSettings);
}

#[test]
fn consent_reset_when_active_host_changes() {
    let state = state_with(AiSettings::default(), None);
    let mut settings_input = input(
        "routerai",
        "m",
        vec![provider_input("routerai", "https://other.test/v1", "m")],
    );
    settings_input.consent_document_send = true;
    state.update_settings(settings_input).expect("сохранено");
    assert!(
        !state.settings().consent_document_send,
        "смена хоста сбрасывает согласие"
    );
}

#[test]
fn consent_kept_when_active_host_unchanged() {
    let state = state_with(AiSettings::default(), None);
    let mut settings_input = input(
        "routerai",
        "m",
        vec![provider_input(
            "routerai",
            "https://routerai.ru/api/v2",
            "m",
        )],
    );
    settings_input.consent_document_send = true;
    state.update_settings(settings_input).expect("сохранено");
    assert!(
        state.settings().consent_document_send,
        "тот же хост — согласие сохраняется"
    );
}

#[test]
fn consent_kept_when_provider_changes_but_host_same() {
    let mut settings = AiSettings::default();
    settings.providers.insert(
        "mirror".to_string(),
        ProviderConfig {
            name: "Mirror".to_string(),
            settings: ProviderSettings {
                base_url: "https://routerai.ru/api/v1".to_string(),
                api_key: None,
                timeout: None,
            },
            models: BTreeMap::from([(
                "m".to_string(),
                ModelEntry {
                    name: "m".to_string(),
                    model_id: None,
                    disabled: None,
                },
            )]),
            ..ProviderConfig::default()
        },
    );
    let state = state_with(settings, None);

    let mut settings_input = input(
        "mirror",
        "m",
        vec![
            provider_input("routerai", "https://routerai.ru/api/v1", "m"),
            provider_input("mirror", "https://routerai.ru/api/v1", "m"),
        ],
    );
    settings_input.consent_document_send = true;
    state.update_settings(settings_input).expect("сохранено");
    assert!(
        state.settings().consent_document_send,
        "тот же хост — согласие сохраняется даже при смене провайдера"
    );
}

#[test]
fn active_host_is_normalized() {
    let mut settings = AiSettings::default();
    settings
        .providers
        .get_mut("routerai")
        .unwrap()
        .settings
        .base_url = "https://RouterAI.ru:8443/api/v1".to_string();
    assert_eq!(
        host_for(&settings.providers, "routerai").as_deref(),
        Some("routerai.ru")
    );
}

#[test]
fn resolve_active_returns_active_pair() {
    let settings = AiSettings::default();
    let resolved = resolve_active(&settings).expect("активная пара разрешена");
    assert_eq!(resolved.id, "routerai");
    assert_eq!(resolved.model, "deepseek/deepseek-v4.1-flash");
    assert_eq!(resolved.base_url, "https://routerai.ru/api/v1");
    assert_eq!(resolved.name, "RouterAI");
}

#[test]
fn resolve_active_rejects_unknown_provider() {
    let settings = AiSettings {
        active_provider: "ghost".to_string(),
        ..AiSettings::default()
    };
    assert_eq!(
        resolve_active(&settings).unwrap_err().code,
        ErrorCode::AiBadSettings
    );
}

#[test]
fn resolve_active_rejects_unknown_model() {
    let settings = AiSettings {
        active_model: "ghost/model".to_string(),
        ..AiSettings::default()
    };
    assert_eq!(
        resolve_active(&settings).unwrap_err().code,
        ErrorCode::AiBadSettings
    );
}

#[test]
fn resolve_active_rejects_disabled_model() {
    let mut settings = AiSettings::default();
    settings
        .providers
        .get_mut("routerai")
        .unwrap()
        .models
        .get_mut("deepseek/deepseek-v4.1-flash")
        .unwrap()
        .disabled = Some(true);
    assert_eq!(
        resolve_active(&settings).unwrap_err().code,
        ErrorCode::AiBadSettings
    );
}

#[test]
fn resolve_active_rejects_http_remote() {
    let mut settings = AiSettings::default();
    settings
        .providers
        .get_mut("routerai")
        .unwrap()
        .settings
        .base_url = "http://example.com/v1".to_string();
    assert_eq!(
        resolve_active(&settings).unwrap_err().code,
        ErrorCode::AiBadSettings
    );
}

#[test]
fn resolve_active_reads_configured_key() {
    let mut settings = AiSettings::default();
    settings
        .providers
        .get_mut("routerai")
        .unwrap()
        .settings
        .api_key = Some("sk-active".to_string());
    let resolved = resolve_active(&settings).expect("разрешено");
    assert_eq!(resolved.api_key.as_deref(), Some("sk-active"));
}

#[test]
fn timeout_zero_falls_back_to_default() {
    let mut settings = AiSettings::default();
    settings
        .providers
        .get_mut("routerai")
        .unwrap()
        .settings
        .timeout = Some(0);
    let resolved = resolve_active(&settings).expect("разрешено");
    assert_eq!(
        resolved.timeout, None,
        "0 мс нормализуется в None: иначе ChatRequest получит Duration::ZERO и запрос обрывается мгновенно"
    );
}

#[test]
fn resolved_provider_debug_hides_headers_and_body_values() {
    let mut settings = AiSettings::default();
    let provider = settings.providers.get_mut("routerai").unwrap();
    provider.headers.insert(
        "X-Corporate-Token".to_string(),
        "corp-token-9999".to_string(),
    );
    provider
        .body
        .insert("tenant".to_string(), serde_json::json!("acme-secret"));

    let resolved = resolve_active(&settings).expect("разрешено");
    let rendered = format!("{resolved:?}");
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
    assert!(validate_base_url("http://LOCALHOST").is_ok());
    assert!(validate_base_url("http://LocalHost:8080/v1").is_ok());
}

/// Надстройка провайдера (`headers`, `body`, `timeout`, `env`) обязана пережить
/// `update_settings`: именно эти поля формируют HTTP-запрос к модели.
#[test]
fn update_settings_propagates_headers_body_timeout_env() {
    let path = temp_config("propagate");
    let state = state_with(AiSettings::default(), Some(path));

    let mut headers = std::collections::BTreeMap::new();
    headers.insert("X-Custom-Header".to_string(), "custom-value".to_string());
    let mut body = serde_json::Map::new();
    body.insert("temperature".to_string(), serde_json::json!(0.5));

    let provider = ProviderInput {
        id: "routerai".to_string(),
        name: "routerai".to_string(),
        env: vec!["CUSTOM_API_KEY".to_string()],
        settings: ProviderSettingsInput {
            base_url: "https://example.test/v1".to_string(),
            timeout: Some(45_000),
        },
        headers,
        body,
        models: vec![ModelInput {
            id: "m".to_string(),
            name: "m".to_string(),
            model_id: None,
            disabled: None,
        }],
    };

    state
        .update_settings(input("routerai", "m", vec![provider]))
        .expect("настройки сохранены");

    let settings = state.settings();
    let provider = &settings.providers["routerai"];
    assert_eq!(
        provider.headers.get("X-Custom-Header").map(String::as_str),
        Some("custom-value"),
        "заголовки должны сохраняться"
    );
    assert_eq!(
        provider.body.get("temperature").cloned(),
        Some(serde_json::json!(0.5)),
        "тело запроса должно сохраняться"
    );
    assert_eq!(
        provider.settings.timeout,
        Some(45_000),
        "таймаут должен сохраняться"
    );
    assert_eq!(
        provider.env,
        vec!["CUSTOM_API_KEY".to_string()],
        "env должны сохраняться"
    );
}

/// Переключение активного провайдера на другой хост обязано сбрасывать согласие:
/// пользователь дал согласие на отправку документов конкретному хосту.
#[test]
fn consent_reset_when_active_provider_changes_to_different_host() {
    let mut settings = AiSettings::default();
    settings.providers.insert(
        "mirror".to_string(),
        ProviderConfig {
            name: "Mirror".to_string(),
            settings: ProviderSettings {
                base_url: "https://mirror.example.test/v1".to_string(),
                api_key: None,
                timeout: None,
            },
            models: BTreeMap::from([(
                "m".to_string(),
                ModelEntry {
                    name: "m".to_string(),
                    model_id: None,
                    disabled: None,
                },
            )]),
            ..ProviderConfig::default()
        },
    );
    let state = state_with(settings, None);

    let mut routerai_input = input(
        "routerai",
        "m",
        vec![provider_input(
            "routerai",
            "https://routerai.ru/api/v1",
            "m",
        )],
    );
    routerai_input.consent_document_send = true;
    state.update_settings(routerai_input).expect("сохранено");
    assert!(
        state.settings().consent_document_send,
        "согласие установлено для routerai"
    );

    let mirror_input = input(
        "mirror",
        "m",
        vec![provider_input(
            "mirror",
            "https://mirror.example.test/v1",
            "m",
        )],
    );
    state.update_settings(mirror_input).expect("сохранено");
    assert!(
        !state.settings().consent_document_send,
        "смена активного провайдера на другой хост сбрасывает согласие"
    );
}

/// §2.5 п.20: вердикт «провайдер не умеет `tools`» принадлежит паре `(base_url, model)` как есть,
/// без нормализации: другой адрес или другая модель той же пары не деградируются.
#[test]
fn tools_capability_verdict_scoped_to_base_url_and_model() {
    let state = state_with(AiSettings::default(), None);
    let base_url = "https://routerai.ru/api/v1";
    let model = "deepseek/deepseek-v4.1-flash";

    state.mark_tools_unsupported(base_url, model);

    assert!(
        !state.tools_supported(base_url, model),
        "отмеченная пара считается не поддерживающей tools"
    );
    assert!(
        state.tools_supported(base_url, "deepseek/deepseek-v4.2"),
        "другая модель того же шлюза не затронута"
    );
    assert!(
        state.tools_supported("https://other.test/v1", model),
        "другой адрес не затронут"
    );
    assert!(
        state.tools_supported("https://RouterAI.ru/api/v1", model),
        "ключ сравнивается как есть, без нормализации регистра"
    );
}

/// §2.5 п.23: конфигурация провайдеров могла измениться — устаревший вердикт хуже отсутствия
/// вердикта, поэтому сохранение настроек очищает кэш целиком.
#[test]
fn update_settings_clears_tools_capability_cache() {
    let path = temp_config("capability-clear");
    let state = state_with(AiSettings::default(), Some(path));
    state.mark_tools_unsupported("https://routerai.ru/api/v1", "m");

    state
        .update_settings(input(
            "routerai",
            "m",
            vec![provider_input("routerai", "https://example.test/v1", "m")],
        ))
        .expect("настройки сохранены");

    assert!(
        state.tools_supported("https://routerai.ru/api/v1", "m"),
        "после сохранения настроек кэш возможностей пуст"
    );
}

/// §2.5 п.23: `ai_set_api_key` кэш не трогает — адрес и модель те же, вердикт остаётся в силе.
#[test]
fn set_api_key_keeps_tools_capability_cache() {
    let path = temp_config("capability-key");
    let state = state_with(AiSettings::default(), Some(path));
    state.mark_tools_unsupported("https://routerai.ru/api/v1", "m");

    state
        .set_api_key("routerai".to_string(), "sk-new".to_string())
        .expect("ключ записан");

    assert!(
        !state.tools_supported("https://routerai.ru/api/v1", "m"),
        "смена ключа не сбрасывает кэш возможностей"
    );
}

/// §2.6 п.26: адрес провайдера допускает userinfo и query, а там может лежать секрет шлюза
/// (H4, ADR-0035 §2.6 п.16). `warn_degraded` печатает результат этого нормализатора, поэтому
/// он обязан отбрасывать всё, кроме хоста: иначе секрет утечёт в stderr.
#[test]
fn host_of_url_drops_userinfo_query_and_port() {
    let host = host_of_url("https://user:sk-secret@gateway.example:8443/v1?api-key=sk-x")
        .expect("хост извлечён");

    assert_eq!(host, "gateway.example");
    for secret in ["sk-secret", "user", "api-key", "sk-x", "8443"] {
        assert!(
            !host.contains(secret),
            "нормализованный хост не содержит {secret}: {host}"
        );
    }
}

/// Тот же инвариант для петлевого `http://` (тесты/E2E) и для не-URL, который `warn_degraded`
/// печатает как «неизвестный хост».
#[test]
fn host_of_url_handles_loopback_port_and_garbage() {
    assert_eq!(
        host_of_url("http://user:pass@127.0.0.1:8080/v1").as_deref(),
        Some("127.0.0.1"),
        "userinfo и порт отброшены у loopback-адреса"
    );
    assert_eq!(
        host_of_url("https://RouterAI.ru/api/v1").as_deref(),
        Some("routerai.ru"),
        "регистр нормализован, путь отброшен"
    );
    assert_eq!(host_of_url("не адрес"), None);
    assert_eq!(host_of_url(""), None);
}
