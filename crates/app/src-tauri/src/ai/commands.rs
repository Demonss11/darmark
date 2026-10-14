//! IPC-команды AI-ассистента (ADR-0034). Фасад над [`AiState`] и швом [`AiTransport`].
//!
//! Секрет (`api_key`) сюда не возвращается: `ai_get_settings` отдаёт только `has_key`.
//! Сетевые ошибки транслируются в [`CommandError`] и событие `ai:error` — без паник.

use tauri::{AppHandle, Emitter, State};

use crate::ai::gate;
use crate::ai::skills;
use crate::ai::state::AiState;
use crate::ai::transport::{ChatRequest, TestRequest};
use crate::ai::types::{
    AiMessage, AiResponse, AiSettingsInput, AiSettingsView, AiSkill, AiTestResult, AiUsage,
};
use crate::error::{CommandError, ErrorCode};

/// Возвращает настройки AI для фронтенда (без секрета — только `has_key`).
#[tauri::command]
pub fn ai_get_settings(state: State<'_, AiState>) -> AiSettingsView {
    AiSettingsView::from(&state.settings())
}

/// Сохраняет несекретные настройки (провайдер/модель/лимиты/согласие/включаемость).
#[tauri::command]
pub fn ai_set_settings(
    settings: AiSettingsInput,
    state: State<'_, AiState>,
) -> Result<(), CommandError> {
    state.update_settings(settings)
}

/// Записывает API-ключ в конфиг (секция `ai`). Ключ не логируется и наружу не отдаётся.
#[tauri::command]
pub fn ai_set_api_key(key: String, state: State<'_, AiState>) -> Result<(), CommandError> {
    state.set_api_key(key)
}

/// Возвращает встроенную библиотеку скиллов MVP (ровно 2, ADR-0032).
#[tauri::command]
pub fn ai_list_skills() -> Vec<AiSkill> {
    skills::skills()
}

/// Отправляет чат-запрос провайдеру со стримингом дельт событиями `ai:chunk`.
///
/// Гейт до сети: включён → ключ → согласие (иначе [`CommandError`] с кодом и `ai:error`).
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
    let Some(api_key) = settings.api_key.clone() else {
        // Гейт уже проверил ключ; страховка на случай рассинхронизации (не паника).
        let err = CommandError::new(ErrorCode::AiNoKey, "API-ключ не задан");
        emit_error(&app, &err);
        return Err(err);
    };

    state.enforce_rate_limit().await;

    let message_id = next_message_id();
    let backend = state.backend();
    let request = ChatRequest {
        base_url: settings.base_url.clone(),
        model: settings.model.clone(),
        api_key,
        messages,
        max_tokens: settings.max_tokens,
        temperature: settings.temperature,
    };

    // Дельта стрима → событие `ai:chunk` (best-effort: сбой emit не роняет запрос).
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
    if let Err(err) = gate::check_enabled(&settings).and_then(|()| gate::check_key(&settings)) {
        return Ok(AiTestResult {
            ok: false,
            model: None,
            error: Some(err.message),
        });
    }
    let Some(api_key) = settings.api_key.clone() else {
        return Ok(AiTestResult {
            ok: false,
            model: None,
            error: Some("API-ключ не задан".to_string()),
        });
    };

    let request = TestRequest {
        base_url: settings.base_url.clone(),
        api_key,
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
fn ai_done_payload(
    message_id: &str,
    usage: Option<AiUsage>,
    finish_reason: Option<String>,
) -> serde_json::Value {
    serde_json::json!({ "message_id": message_id, "usage": usage, "finish_reason": finish_reason })
}

/// Идентификатор сообщения для корреляции `ai:chunk`/`ai:done` (без внешних зависимостей).
fn next_message_id() -> String {
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
fn error_code_str(code: ErrorCode) -> String {
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

#[cfg(test)]
mod tests {
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

    /// Эталонный XSS-payload из ADR-0038 §2.5: команда обязана вырезать `onerror`,
    /// `<script` и `javascript:`, сохранив безопасную разметку.
    #[test]
    fn render_markdown_sanitizes_ai_answer() {
        // Пустые строки в payload обязательны (ADR-0038 §2.5): строка с сырым HTML открывает
        // общий HTML-блок CommonMark, который поглощает всё до первой пустой строки — разметка
        // после него осталась бы текстом. Проверено на pulldown-cmark 0.13: без разделителей
        // `[клик](javascript:…)` не становится ссылкой, а `**жирный**`/`код`` не парсятся.
        // С разделителями `href` ссылки режется allowlist-санитайзером, а не остаётся текстом.
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
}
