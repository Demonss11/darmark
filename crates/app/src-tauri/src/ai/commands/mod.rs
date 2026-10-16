//! IPC-команды AI-ассистента (ADR-0034). Фасад над [`AiState`] и швом
//! [`crate::ai::transport::AiTransport`].
//!
//! Секреты провайдеров сюда не возвращаются: `ai_get_settings` отдаёт только `has_key`.
//! Сетевые ошибки транслируются в [`CommandError`] и событие `ai:error` — без паник.

use std::time::Duration;

use mcp_core::protocol::{RpcRequest, RPC_VERSION};
use mcp_core::server::McpServer;
use tauri::{AppHandle, Emitter, State};

use crate::ai::gate;
use crate::ai::history::{self, HistoryFile, SCHEMA_VERSION};
use crate::ai::settings;
use crate::ai::skills;
use crate::ai::state::{self, AiState};
use crate::ai::tool_calls::ToolCall;
use crate::ai::tool_loop::{self, ToolExecution};
use crate::ai::transport::{ChatRequest, TestRequest};
use crate::ai::types::{
    AiMessage, AiResponse, AiSettingsInput, AiSettingsView, AiSkill, AiTestResult, AiUsage,
    HistoryView,
};
use crate::error::{CommandError, ErrorCode};


#[tauri::command]
pub fn ai_get_settings(state: State<'_, AiState>) -> AiSettingsView {
    AiSettingsView::from(&state.settings())
}


#[tauri::command]
pub fn ai_set_settings(
    settings: AiSettingsInput,
    state: State<'_, AiState>,
) -> Result<(), CommandError> {
    state.update_settings(settings)
}


#[tauri::command]
pub fn ai_set_api_key(
    provider_id: String,
    key: String,
    state: State<'_, AiState>,
) -> Result<(), CommandError> {
    state.set_api_key(provider_id, key)
}


#[tauri::command]
pub fn ai_list_skills() -> Vec<AiSkill> {
    skills::skills()
}






pub(super) trait AiEmitter: Send + Sync + 'static {
    
    fn emit(&self, event: &str, payload: serde_json::Value) -> Result<(), String>;
}

impl AiEmitter for AppHandle {
    fn emit(&self, event: &str, payload: serde_json::Value) -> Result<(), String> {
        Emitter::emit(self, event, payload).map_err(|e| e.to_string())
    }
}







#[tauri::command]
pub async fn ai_chat(
    app: AppHandle,
    messages: Vec<AiMessage>,
    state: State<'_, AiState>,
    mcp: State<'_, McpServer>,
) -> Result<AiResponse, CommandError> {
    run_chat(&app, &state, &mcp, messages).await
}









pub(super) async fn run_chat(
    emitter: &dyn AiEmitter,
    state: &AiState,
    mcp: &McpServer,
    messages: Vec<AiMessage>,
) -> Result<AiResponse, CommandError> {
    let settings = state.settings();
    if let Err(err) = gate::check_chat(&settings) {
        emit_error(emitter, &err);
        return Err(err);
    }
    let resolved = match state::resolve_active(&settings) {
        Ok(resolved) => resolved,
        Err(err) => {
            emit_error(emitter, &err);
            return Err(err);
        }
    };
    let Some(api_key) = resolved.api_key.clone() else {
        let err = CommandError::new(ErrorCode::AiNoKey, "API-ключ не задан");
        emit_error(emitter, &err);
        return Err(err);
    };

    state.enforce_rate_limit().await;

    let message_id = next_message_id();
    let backend = state.backend();
    let request = ChatRequest {
        base_url: resolved.base_url,
        model: resolved.model,
        api_key,
        messages,
        max_tokens: settings.max_tokens,
        temperature: settings.temperature,
        headers: resolved.headers,
        extra_body: resolved.body,
        timeout: resolved.timeout.map(Duration::from_millis),
        tools: provider_tools(mcp),
    };
    let mut on_delta = |delta: String| {
        emit_event(
            emitter,
            "ai:chunk",
            serde_json::json!({ "message_id": message_id, "delta": delta }),
        );
    };
    let mut on_tool_call = |call: &ToolCall| {
        emit_event(
            emitter,
            "ai:tool-call",
            serde_json::json!({
                "message_id": message_id,
                "call_id": call.id,
                "tool": call.name,
                "arguments": call.arguments,
            }),
        );
    };
    let mut on_tool_result = |exec: &ToolExecution<'_>| {
        emit_event(
            emitter,
            "ai:tool-result",
            serde_json::json!({
                "message_id": message_id,
                "call_id": exec.call.id,
                "tool": exec.call.name,
                "ok": exec.result.ok,
                "duration_ms": exec.duration.as_millis() as u64,
                "summary": tool_summary(exec),
            }),
        );
    };

    let response = match tool_loop::run(
        &*backend,
        mcp,
        request,
        state,
        &mut on_delta,
        &mut on_tool_call,
        &mut on_tool_result,
    )
    .await
    {
        Ok(response) => response,
        Err(err) => {
            emit_error(emitter, &err);
            return Err(err);
        }
    };

    let done_payload = ai_done_payload(&message_id, response.usage, response.finish_reason.clone());
    emit_event(emitter, "ai:done", done_payload);

    Ok(response)
}







fn provider_tools(mcp: &McpServer) -> Option<serde_json::Value> {
    let list = mcp
        .handle(RpcRequest {
            jsonrpc: RPC_VERSION.to_string(),
            id: Some(serde_json::json!(1)),
            method: "tools/list".to_string(),
            params: None,
        })
        .and_then(|response| response.result)
        .unwrap_or(serde_json::Value::Null);
    let specs = tool_loop::tool_specs_from_tools_list(&list);
    if specs.is_empty() {
        None
    } else {
        Some(serde_json::Value::Array(specs))
    }
}





fn tool_summary(exec: &ToolExecution<'_>) -> &'static str {
    if exec.result.ok {
        "готово"
    } else {
        "ошибка"
    }
}


fn emit_event(emitter: &dyn AiEmitter, event: &str, payload: serde_json::Value) {
    if let Err(e) = emitter.emit(event, payload) {
        eprintln!("не эмитить {event}: {e}");
    }
}






#[tauri::command]
pub async fn ai_test_connection(state: State<'_, AiState>) -> Result<AiTestResult, CommandError> {
    let settings = state.settings();
    if let Err(err) = gate::check_enabled(&settings)
        .and_then(|()| gate::check_active(&settings))
        .and_then(|()| gate::check_key(&settings))
    {
        return Ok(AiTestResult {
            ok: false,
            model: None,
            error: Some(err.message),
        });
    }
    let resolved = state::resolve_active(&settings)?;
    let Some(api_key) = resolved.api_key.clone() else {
        return Ok(AiTestResult {
            ok: false,
            model: None,
            error: Some("API-ключ не задан".to_string()),
        });
    };

    let request = TestRequest {
        base_url: resolved.base_url,
        api_key,
        headers: resolved.headers,
        timeout: resolved.timeout.map(Duration::from_millis),
    };
    Ok(match state.backend().test_connection(request).await {
        Ok(model) => AiTestResult {
            ok: true,
            model,
            error: None,
        },
        Err(err) => AiTestResult {
            ok: false,
            model: None,
            error: Some(err.message),
        },
    })
}





pub(super) fn ai_done_payload(
    message_id: &str,
    usage: Option<AiUsage>,
    finish_reason: Option<String>,
) -> serde_json::Value {
    serde_json::json!({ "message_id": message_id, "usage": usage, "finish_reason": finish_reason })
}


pub(super) fn next_message_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or(0);
    format!("ai-{now}-{sequence}")
}


fn emit_error(emitter: &dyn AiEmitter, err: &CommandError) {
    let payload = serde_json::json!({
        "code": error_code_str(err.code),
        "message": err.message,
    });
    emit_event(emitter, "ai:error", payload);
}


pub(super) fn error_code_str(code: ErrorCode) -> String {
    serde_json::to_value(code)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "ai_error".to_string())
}






#[tauri::command]
pub fn ai_render_markdown(text: String) -> String {
    md_core::to_html(&text)
}





#[tauri::command]
pub fn ai_history_load() -> HistoryView {
    let base = history_base();
    let (file, warning) = history::load(base.as_deref());
    HistoryView {
        session_id: file.session_id,
        updated_at: file.updated_at,
        messages: file.messages,
        warning,
    }
}





#[tauri::command]
pub fn ai_history_save(messages: Vec<AiMessage>) -> Result<(), CommandError> {
    let base = history_base();
    let file = HistoryFile {
        schema_version: SCHEMA_VERSION,
        session_id: String::new(),
        updated_at: now_secs(),
        messages,
    };
    history::save(base.as_deref(), &file)
}


#[tauri::command]
pub fn ai_history_clear() -> Result<(), CommandError> {
    let base = history_base();
    let mut file = HistoryFile::empty();
    file.updated_at = now_secs();
    history::save(base.as_deref(), &file)
}





fn history_base() -> Option<std::path::PathBuf> {
    settings::config_path().and_then(|path| path.parent().map(|dir| dir.to_path_buf()))
}


fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
