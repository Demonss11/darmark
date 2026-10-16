//! Тесты IPC-команд [`super::super::commands`].
//!
//! `ai_chat` принимает `AppHandle`, которого без Tauri-runtime не создать, поэтому тест
//! жизненного цикла вызова идёт через внутренний `run_chat` с подставным
//! [`AiEmitter`](crate::ai::commands::AiEmitter) (образец шва — `mcp::LogEmitter`). Двойники
//! (эмиттер, MCP-сервер, транспорт, исполнитель фьючеров) лежат в [`super::support`]: проверяется
//! поведение команды — последовательность событий, объявление `tools`, деградация без реестра.

use std::sync::Arc;

use serde_json::json;

use crate::ai::commands::{
    ai_done_payload, ai_history_clear, ai_history_load, ai_history_save, ai_list_skills,
    ai_render_markdown, error_code_str, next_message_id, run_chat,
};
use crate::ai::state::{resolve_active, AiState};
use crate::ai::tool_loop::ToolsCapability;
use crate::ai::transport::AiTransport;
use crate::ai::types::{AiMessage, AiUsage};
use crate::error::ErrorCode;

use super::support::{
    block_on, context_length_exceeded, empty_server, fake_server, final_outcome,
    outcome_with_tool_call, ready_settings, tools_rejected, MockEmitter, ScriptedTransport,
};

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
fn list_skills_returns_five() {
    assert_eq!(ai_list_skills().len(), 5);
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
            ..AiMessage::default()
        },
        AiMessage {
            role: "assistant".to_string(),
            content: "здравствуйте".to_string(),
            ..AiMessage::default()
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



#[test]
fn chat_emits_tool_lifecycle_events() {
    let transport = Arc::new(ScriptedTransport::ok(
        vec![outcome_with_tool_call(), final_outcome("готовый ответ")],
        final_outcome("готовый ответ"),
    ));
    let backend: Arc<dyn AiTransport> = transport.clone();
    let state = AiState::with_config_path(ready_settings(), backend, None);
    let emitter = MockEmitter::default();

    let response = block_on(run_chat(
        &emitter,
        &state,
        &fake_server(),
        vec![AiMessage {
            role: "user".to_string(),
            content: "привет".to_string(),
            ..AiMessage::default()
        }],
    ))
    .expect("чат завершается финальным ответом");

    assert_eq!(response.message.content, "готовый ответ");

    let entries = emitter.entries();
    let lifecycle: Vec<&str> = entries
        .iter()
        .map(|(event, _)| event.as_str())
        .filter(|event| *event != "ai:chunk")
        .collect();
    assert_eq!(
        lifecycle,
        vec!["ai:tool-call", "ai:tool-result", "ai:done"],
        "порядок событий жизненного цикла: {entries:?}"
    );

    let deltas: String = entries
        .iter()
        .filter(|(event, _)| event == "ai:chunk")
        .map(|(_, payload)| payload["delta"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(deltas, "готовый ответ", "дельты не задваиваются через цикл");

    let call = emitter.payload("ai:tool-call");
    let result = emitter.payload("ai:tool-result");
    let done = emitter.payload("ai:done");

    assert_eq!(call["call_id"], "call-1", "call_id — id вызова провайдера");
    assert_eq!(call["tool"], "fake_tool");
    assert_eq!(
        call["arguments"]["path"], "plugins",
        "аргументы вызова доходят до события: {call}"
    );

    assert_eq!(
        result["call_id"], call["call_id"],
        "результат связан с вызовом по id"
    );
    assert_eq!(result["tool"], "fake_tool");
    assert_eq!(result["ok"], true, "успешный вызов: {result}");
    let duration_ms = result["duration_ms"]
        .as_u64()
        .unwrap_or_else(|| panic!("duration_ms — неотрицательное число: {result}"));
    assert!(
        duration_ms < 60_000,
        "длительность вызова в разумных пределах: {duration_ms}"
    );
    assert_eq!(
        result["summary"], "готово",
        "summary — только статус, без содержимого и путей: {result}"
    );

    assert_eq!(call["message_id"], result["message_id"]);
    assert_eq!(
        call["message_id"], done["message_id"],
        "один message_id на весь цикл"
    );
    assert_eq!(done["finish_reason"], "stop");
    assert_eq!(done["usage"]["completion_tokens"], 5);

    let requests = transport.requests();
    assert_eq!(requests.len(), 2, "запрос до вызова инструмента и после");
    assert_eq!(
        requests[0]
            .tools
            .as_ref()
            .map(|tools| tools[0]["function"]["name"].clone()),
        Some(json!("fake_tool")),
        "провайдеру объявлены инструменты MCP: {:?}",
        requests[0].tools
    );
}


#[test]
fn chat_degrades_without_tools_when_registry_empty() {
    let transport = Arc::new(ScriptedTransport::ok(
        Vec::new(),
        final_outcome("обычный ответ"),
    ));
    let backend: Arc<dyn AiTransport> = transport.clone();
    let state = AiState::with_config_path(ready_settings(), backend, None);
    let emitter = MockEmitter::default();

    let response = block_on(run_chat(
        &emitter,
        &state,
        &empty_server(),
        vec![AiMessage {
            role: "user".to_string(),
            content: "привет".to_string(),
            ..AiMessage::default()
        }],
    ))
    .expect("деградация не ломает чат");

    assert_eq!(response.message.content, "обычный ответ");
    let requests = transport.requests();
    assert_eq!(requests.len(), 1, "один запрос без цикла вызовов");
    assert!(
        requests[0].tools.is_none(),
        "без инструментов поле tools не передаётся: {:?}",
        requests[0].tools
    );
    let events: Vec<String> = emitter
        .entries()
        .into_iter()
        .map(|(event, _)| event)
        .collect();
    assert!(
        !events.iter().any(|event| event.starts_with("ai:tool")),
        "событий вызова инструментов нет: {events:?}"
    );
}



#[test]
fn chat_degrades_silently_when_provider_rejects_tools() {
    let transport = Arc::new(ScriptedTransport::new(
        vec![
            Err(tools_rejected()),
            Ok(final_outcome("ответ без инструментов")),
        ],
        Ok(final_outcome("ответ без инструментов")),
    ));
    let backend: Arc<dyn AiTransport> = transport.clone();
    let state = AiState::with_config_path(ready_settings(), backend, None);
    let emitter = MockEmitter::default();

    let response = block_on(run_chat(
        &emitter,
        &state,
        &fake_server(),
        vec![AiMessage {
            role: "user".to_string(),
            content: "привет".to_string(),
            ..AiMessage::default()
        }],
    ))
    .expect("деградация не ломает чат");

    assert_eq!(response.message.content, "ответ без инструментов");

    let entries = emitter.entries();
    let events: Vec<&str> = entries.iter().map(|(event, _)| event.as_str()).collect();
    assert!(
        !events.contains(&"ai:error"),
        "деградация не показывается пользователю как ошибка: {entries:?}"
    );
    assert_eq!(
        events.iter().filter(|event| **event == "ai:done").count(),
        1,
        "один ai:done на весь цикл: {events:?}"
    );
    assert!(
        !events.iter().any(|event| event.starts_with("ai:tool")),
        "вызовов инструментов не было: {events:?}"
    );

    let message_ids: Vec<&serde_json::Value> = entries
        .iter()
        .filter_map(|(_, payload)| payload.get("message_id"))
        .collect();
    assert!(
        message_ids.windows(2).all(|pair| pair[0] == pair[1]),
        "один message_id на деградацию и повтор: {message_ids:?}"
    );

    let deltas: String = entries
        .iter()
        .filter(|(event, _)| event == "ai:chunk")
        .map(|(_, payload)| payload["delta"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(
        deltas, "ответ без инструментов",
        "дельты не задваиваются: отвергнутый запрос текста не приносит"
    );

    let requests = transport.requests();
    assert_eq!(requests.len(), 2, "отвергнутый запрос и один повтор");
    assert!(
        requests[0].tools.is_some(),
        "первый запрос объявляет инструменты MCP: {:?}",
        requests[0].tools
    );
    assert!(
        requests[1].tools.is_none(),
        "повтор уходит без tools: {:?}",
        requests[1].tools
    );

    let resolved = resolve_active(&state.settings()).expect("активная пара разрешена");
    assert!(
        !state.tools_supported(&resolved.base_url, &resolved.model),
        "вердикт о неподдержке tools закэширован в состоянии"
    );
}



#[test]
fn chat_omits_tools_when_capability_cached() {
    let transport = Arc::new(ScriptedTransport::ok(
        Vec::new(),
        final_outcome("ответ без инструментов"),
    ));
    let backend: Arc<dyn AiTransport> = transport.clone();
    let state = AiState::with_config_path(ready_settings(), backend, None);
    let resolved = resolve_active(&state.settings()).expect("активная пара разрешена");
    state.mark_tools_unsupported(&resolved.base_url, &resolved.model);
    let emitter = MockEmitter::default();

    let response = block_on(run_chat(
        &emitter,
        &state,
        &fake_server(),
        vec![AiMessage {
            role: "user".to_string(),
            content: "привет".to_string(),
            ..AiMessage::default()
        }],
    ))
    .expect("кэш-хит не ломает чат");

    assert_eq!(response.message.content, "ответ без инструментов");
    let requests = transport.requests();
    assert_eq!(requests.len(), 1, "один запрос, повтора нет");
    assert!(
        requests[0].tools.is_none(),
        "кэш-хит снимает tools до первого запроса: {:?}",
        requests[0].tools
    );
    let events: Vec<String> = emitter
        .entries()
        .into_iter()
        .map(|(event, _)| event)
        .collect();
    assert!(
        !events.iter().any(|event| event.starts_with("ai:tool")),
        "событий вызова инструментов нет: {events:?}"
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.as_str() == "ai:done")
            .count(),
        1,
        "цикл завершён одним ai:done: {events:?}"
    );
}



#[test]
fn chat_reports_context_length_without_degrading() {
    let transport = Arc::new(ScriptedTransport::new(
        vec![Err(context_length_exceeded())],
        Ok(final_outcome("повтор не должен выполняться")),
    ));
    let backend: Arc<dyn AiTransport> = transport.clone();
    let state = AiState::with_config_path(ready_settings(), backend, None);
    let emitter = MockEmitter::default();

    let error = block_on(run_chat(
        &emitter,
        &state,
        &fake_server(),
        vec![AiMessage {
            role: "user".to_string(),
            content: "привет".to_string(),
            ..AiMessage::default()
        }],
    ))
    .expect_err("превышение контекста уходит пользователю");

    assert_eq!(error.code, ErrorCode::AiContextLength);
    assert_eq!(transport.request_count(), 1, "повтор не выполняется");
    assert_eq!(emitter.payload("ai:error")["code"], "ai_context_length");
}
