use super::*;

#[test]
fn error_codes_serialize_to_contract_strings() {
    assert_eq!(error_code_str(ErrorCode::AiDisabled), "ai_disabled");
    assert_eq!(error_code_str(ErrorCode::AiNoKey), "ai_no_key");
    assert_eq!(error_code_str(ErrorCode::AiNoConsent), "ai_no_consent");
    assert_eq!(error_code_str(ErrorCode::AiBadKey), "ai_bad_key");
    assert_eq!(error_code_str(ErrorCode::AiRateLimited), "ai_rate_limited");
    assert_eq!(error_code_str(ErrorCode::AiBadRequest), "ai_bad_request");
    assert_eq!(
        error_code_str(ErrorCode::AiContextLength),
        "ai_context_length"
    );
    assert_eq!(error_code_str(ErrorCode::AiServerError), "ai_server_error");
    assert_eq!(error_code_str(ErrorCode::AiOffline), "ai_offline");
    assert_eq!(error_code_str(ErrorCode::AiBadSettings), "ai_bad_settings");
    assert_eq!(error_code_str(ErrorCode::AiStaleEdit), "ai_stale_edit");
    assert_eq!(error_code_str(ErrorCode::AiApplyFailed), "ai_apply_failed");
    assert_eq!(
        error_code_str(ErrorCode::AiPluginInvalid),
        "ai_plugin_invalid"
    );
    assert_eq!(
        error_code_str(ErrorCode::AiPluginExists),
        "ai_plugin_exists"
    );
    assert_eq!(
        error_code_str(ErrorCode::AiPluginNotFound),
        "ai_plugin_not_found"
    );
    assert_eq!(error_code_str(ErrorCode::AiIo), "ai_io");
}

#[test]
fn message_ids_are_unique() {
    let first = next_message_id();
    let second = next_message_id();
    assert_ne!(first, second);
    assert!(first.starts_with("ai-"));
}

#[test]
fn list_skills_returns_seven() {
    assert_eq!(ai_list_skills().len(), 7);
}

#[test]
fn ai_done_payload_contains_finish_reason() {
    let payload = ai_done_payload(
        "ai-123-0",
        Some(AiUsage {
            prompt_tokens: 10,
            completion_tokens: 20,
        }),
        Some("length".to_string()),
    );
    assert_eq!(payload["message_id"], "ai-123-0");
    assert_eq!(payload["finish_reason"], "length");
    assert_eq!(payload["usage"]["prompt_tokens"], 10);
    assert_eq!(payload["usage"]["completion_tokens"], 20);
}

#[test]
fn ai_done_payload_finish_reason_none_serializes_as_null() {
    let payload = ai_done_payload("ai-123-1", None, None);
    assert_eq!(payload["message_id"], "ai-123-1");
    assert_eq!(payload["finish_reason"], serde_json::Value::Null);
    assert_eq!(payload["usage"], serde_json::Value::Null);
}

/// Roundtrip команд истории: save → load → clear → load.
///
/// Изоляция от реального профиля: общий `env_lock` с тестами `history`
/// (команда `DARMARK_CONFIG_PATH` общая на процесс) + уникальный temp-каталог
/// на процесс. Команды внутри используют `settings::config_path()`.
#[test]
fn history_commands_roundtrip() {
    use crate::ai::history::env_lock;

    let _guard = env_lock();

    let dir =
        std::env::temp_dir().join(format!("darmark-history-roundtrip-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let config_path = dir.join("config.json");
    std::env::set_var("DARMARK_CONFIG_PATH", &config_path);

    ai_history_save(vec![
        AiMessage {
            role: "user".to_string(),
            content: "привет".to_string(),
        },
        AiMessage {
            role: "assistant".to_string(),
            content: "здравствуйте".to_string(),
        },
    ])
    .expect("сохранение истории");

    let view = ai_history_load();
    assert_eq!(view.messages.len(), 2, "оба сообщения сохранены");
    assert_eq!(view.messages[0].role, "user");
    assert_eq!(view.messages[0].content, "привет");
    assert_eq!(view.messages[1].role, "assistant");
    assert_eq!(view.messages[1].content, "здравствуйте");
    assert!(
        view.warning.is_none(),
        "предупреждений быть не должно: {:?}",
        view.warning
    );

    ai_history_clear().expect("очистка истории");
    let view = ai_history_load();
    assert!(view.messages.is_empty(), "история очищена");

    std::env::remove_var("DARMARK_CONFIG_PATH");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Эталонный XSS-payload из ADR-0038 §2.5: команда обязана вырезать `onerror`,
/// `<script` и `javascript:`, сохранив безопасную разметку.
#[test]
fn render_markdown_sanitizes_ai_answer() {
    let payload = r##"# Заголовок
<img src=x onerror="window.__pwned=1">
<script>window.__pwned=1</script>

[клик](javascript:window.__pwned=1)
<a href="data:text/html,<script>1</script>">ссылка</a>

**жирный** и `код`
- пункт списка"##;

    let html = ai_render_markdown(payload.to_string());

    assert!(html.contains("<strong>"), "ожидался <strong>: {html}");
    assert!(html.contains("<code>"), "ожидался <code>: {html}");
    assert!(html.contains("<li>"), "ожидался <li>: {html}");
    assert!(!html.contains("onerror"), "остался onerror: {html}");
    assert!(!html.contains("<script"), "остался <script: {html}");
    assert!(
        !html.contains("javascript:"),
        "осталась схема javascript:: {html}"
    );
}
