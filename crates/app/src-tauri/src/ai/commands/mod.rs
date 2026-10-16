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

/// Возвращает настройки AI для фронтенда (без секретов — только `has_key` на провайдера).
#[tauri::command]
pub fn ai_get_settings(state: State<'_, AiState>) -> AiSettingsView {
    AiSettingsView::from(&state.settings())
}

/// Сохраняет настройки провайдеров/активной пары/лимитов/согласия/включаемости.
#[tauri::command]
pub fn ai_set_settings(
    settings: AiSettingsInput,
    state: State<'_, AiState>,
) -> Result<(), CommandError> {
    state.update_settings(settings)
}

/// Записывает API-ключ провайдера в конфиг. Ключ не логируется и наружу не отдаётся.
#[tauri::command]
pub fn ai_set_api_key(
    provider_id: String,
    key: String,
    state: State<'_, AiState>,
) -> Result<(), CommandError> {
    state.set_api_key(provider_id, key)
}

/// Возвращает встроенную библиотеку скиллов MVP (ровно 2, ADR-0032).
#[tauri::command]
pub fn ai_list_skills() -> Vec<AiSkill> {
    skills::skills()
}

/// Эмиттер событий ассистента — граница между командой и транспортом (Tauri-событие в проде).
///
/// Инъекция вместо жёсткой зависимости от [`AppHandle`] делает последовательность событий
/// (`ai:chunk`/`ai:tool-call`/`ai:tool-result`/`ai:done`/`ai:error`) проверяемой Rust-тестом:
/// прод отдаёт [`AppHandle`], тест — собирающий mock. Образец шва — `mcp::LogEmitter`.
pub(super) trait AiEmitter: Send + Sync + 'static {
    /// Отправляет событие `event` с JSON-payload; `Err` — транспорт недоступен.
    fn emit(&self, event: &str, payload: serde_json::Value) -> Result<(), String>;
}

impl AiEmitter for AppHandle {
    fn emit(&self, event: &str, payload: serde_json::Value) -> Result<(), String> {
        Emitter::emit(self, event, payload).map_err(|e| e.to_string())
    }
}

/// Отправляет чат-запрос провайдеру со стримингом дельт событиями `ai:chunk`.
///
/// Аддитивные события жизненного цикла вызова инструментов — `ai:tool-call` и
/// `ai:tool-result` (контракт с фронтендом). Тонкая точка входа: работа — в [`run_chat`],
/// чтобы эмиссию событий можно было подменить в тестах (см. [`AiEmitter`]). Реестр MCP
/// приходит из состояния приложения.
#[tauri::command]
pub async fn ai_chat(
    app: AppHandle,
    messages: Vec<AiMessage>,
    state: State<'_, AiState>,
    mcp: State<'_, McpServer>,
) -> Result<AiResponse, CommandError> {
    run_chat(&app, &state, &mcp, messages).await
}

/// Прогоняет чат через tool-loop MCP, эмитя события жизненного цикла вызова инструментов.
///
/// Гейт до сети: включён → активная пара → ключ → согласие (иначе [`CommandError`] + `ai:error`).
/// **Одна** пауза rate limit и один `message_id` на весь цикл: фронт склеивает ответ и карточки
/// вызовов по нему. Инструменты объявляются провайдеру из реестра MCP ([`provider_tools`]); при
/// пустом реестре запрос уходит без `tools`, и цикл вырождается в один запрос. Если провайдер
/// отверг `tools` (HTTP 400), цикл делает один повтор без них и кэширует вердикт в состоянии
/// (ADR-0040) — деградация пользователю не показывается.
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

/// Инструменты провайдеру из реестра MCP; `None` — объявлять нечего.
///
/// Реестр читается тем же JSON-RPC путём, что и у любого клиента MCP (`tools/list`, ADR-0036):
/// команда не заглядывает внутрь сервера. Пустой список даёт `None` — запрос уходит без поля
/// `tools`. Отказ провайдера принимать `tools` разбирает уже цикл: один повтор без них и кэш
/// вердикта по `(base_url, model)` (ADR-0040 §2.2, §2.5).
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

/// Короткая разметка результата вызова для `ai:tool-result`.
///
/// Только статус: содержимое результата, аргументы и пути в событие не попадают
/// (AC-MCP-8/13). Карточку вызова фронт строит из имени инструмента, `ok` и `duration_ms`.
fn tool_summary(exec: &ToolExecution<'_>) -> &'static str {
    if exec.result.ok {
        "готово"
    } else {
        "ошибка"
    }
}

/// Эмитит событие ассистента (best-effort: сбой emit логируется, не паника).
fn emit_event(emitter: &dyn AiEmitter, event: &str, payload: serde_json::Value) {
    if let Err(e) = emitter.emit(event, payload) {
        eprintln!("не эмитить {event}: {e}");
    }
}

/// Лёгкая проверка соединения с провайдером (`GET /models`). Без паник: ошибка — в `error`.
///
/// Согласие на отправку содержимого здесь не требуется: текст документа не передаётся.
/// Возвращает `Result` (требование Tauri к async-командам со ссылками), но `Err` не
/// порождается: исход проверки всегда кодируется в [`AiTestResult`].
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

/// Формирует payload события `ai:done` для фронтенда.
///
/// `finish_reason` прокидывается явно: фронтенд читает его из этого события, чтобы отличить
/// нормальное завершение (`"stop"`) от обрыва по лимиту (`"length"`).
pub(super) fn ai_done_payload(
    message_id: &str,
    usage: Option<AiUsage>,
    finish_reason: Option<String>,
) -> serde_json::Value {
    serde_json::json!({ "message_id": message_id, "usage": usage, "finish_reason": finish_reason })
}

/// Идентификатор сообщения для корреляции `ai:chunk`/`ai:done` (без внешних зависимостей).
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

/// Эмитит `ai:error { code, message }` (best-effort: сбой emit логируется, не паника).
fn emit_error(emitter: &dyn AiEmitter, err: &CommandError) {
    let payload = serde_json::json!({
        "code": error_code_str(err.code),
        "message": err.message,
    });
    emit_event(emitter, "ai:error", payload);
}

/// Строковое значение кода ошибки (совпадает с serde `snake_case` — контракт с фронтендом).
pub(super) fn error_code_str(code: ErrorCode) -> String {
    serde_json::to_value(code)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "ai_error".to_string())
}

/// Рендерит Markdown-ответ модели в санитизированный HTML (ADR-0038).
///
/// `md_core::to_html` **сам** прогоняет результат через allowlist-санитайзер
/// (`crates/md-core/src/lib.rs:52`), поэтому второй вызов `sanitize_fragment` не нужен.
/// `to_html_mapped` не используется: ответ AI не участвует в инспекторе и не несёт `data-md`.
#[tauri::command]
pub fn ai_render_markdown(text: String) -> String {
    md_core::to_html(&text)
}

/// Возвращает историю чата для фронтенда (без секретов и абсолютных путей).
///
/// Ошибок не возвращает: битый файл / превышение лимита / отсутствие файла — пустая
/// сессия + `warning` (приложение стартует, см. `history::load`).
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

/// Сохраняет историю чата (полная замена файла). Принимает только `role`/`content`.
///
/// Неизвестные поля в JSON игнорируются serde (контракт с фронтендом). Лимит сообщений
/// и атомарная запись — в `history::save`.
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

/// Очищает историю чата: перезаписывает файл пустой сессией со свежей меткой времени.
#[tauri::command]
pub fn ai_history_clear() -> Result<(), CommandError> {
    let base = history_base();
    let mut file = HistoryFile::empty();
    file.updated_at = now_secs();
    history::save(base.as_deref(), &file)
}

/// Каталог для файла истории: родитель конфига (каталог данных приложения).
///
/// На Windows и в тестах путь конфига читает `DARMARK_CONFIG_PATH` (см.
/// `plugins::settings::SettingsStore::config_path`); история лежит рядом с конфигом.
fn history_base() -> Option<std::path::PathBuf> {
    settings::config_path().and_then(|path| path.parent().map(|dir| dir.to_path_buf()))
}

/// Текущие unix-секунды (метка `updated_at` для истории).
fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
