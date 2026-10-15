//! IPC-команды AI-ассистента (ADR-0034). Фасад над [`AiState`] и швом
//! [`crate::ai::transport::AiTransport`].
//!
//! Секреты провайдеров сюда не возвращаются: `ai_get_settings` отдаёт только `has_key`.
//! Сетевые ошибки транслируются в [`CommandError`] и событие `ai:error` — без паник.

use std::time::Duration;

use tauri::{AppHandle, Emitter, State};

use crate::ai::gate;
use crate::ai::history::{self, HistoryFile, SCHEMA_VERSION};
use crate::ai::settings;
use crate::ai::skills;
use crate::ai::state::{self, AiState};
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

/// Отправляет чат-запрос провайдеру со стримингом дельт событиями `ai:chunk`.
///
/// Гейт до сети: включён → активная пара → ключ → согласие (иначе [`CommandError`]).
/// Возвращает склеенный ответ и `usage`; по завершении эмитит `ai:done`.
#[tauri::command]
pub async fn ai_chat(
    app: AppHandle,
    messages: Vec<AiMessage>,
    state: State<'_, AiState>,
) -> Result<AiResponse, CommandError> {
    let settings = state.settings();
    if let Err(err) = gate::check_chat(&settings) {
        emit_error(&app, &err);
        return Err(err);
    }
    let resolved = match state::resolve_active(&settings) {
        Ok(resolved) => resolved,
        Err(err) => {
            emit_error(&app, &err);
            return Err(err);
        }
    };
    let Some(api_key) = resolved.api_key.clone() else {
        let err = CommandError::new(ErrorCode::AiNoKey, "API-ключ не задан");
        emit_error(&app, &err);
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
        tools: None,
    };
    let chunk_app = app.clone();
    let chunk_id = message_id.clone();
    let mut on_delta = move |delta: String| {
        if let Err(e) = chunk_app.emit(
            "ai:chunk",
            serde_json::json!({ "message_id": chunk_id, "delta": delta }),
        ) {
            eprintln!("не эмитить ai:chunk: {e}");
        }
    };

    let outcome = match backend.chat(request, &mut on_delta).await {
        Ok(outcome) => outcome,
        Err(err) => {
            emit_error(&app, &err);
            return Err(err);
        }
    };

    let done_payload = ai_done_payload(&message_id, outcome.usage, outcome.finish_reason.clone());
    if let Err(e) = app.emit("ai:done", done_payload) {
        eprintln!("не эмитить ai:done: {e}");
    }

    Ok(AiResponse {
        message: AiMessage {
            role: "assistant".to_string(),
            content: outcome.content,
        },
        usage: outcome.usage,
        finish_reason: outcome.finish_reason,
    })
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
fn emit_error(app: &AppHandle, err: &CommandError) {
    let payload = serde_json::json!({
        "code": error_code_str(err.code),
        "message": err.message,
    });
    if let Err(e) = app.emit("ai:error", payload) {
        eprintln!("не эмитить ai:error: {e}");
    }
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
