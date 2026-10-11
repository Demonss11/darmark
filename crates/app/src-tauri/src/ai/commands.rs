//! IPC-команды AI-ассистента (TZ §9.1). Фасад над [`AiState`] и швом [`AiTransport`].
//!
//! Секрет (`api_key`) сюда не возвращается: `ai_get_settings` отдаёт только `has_key`.
//! Сетевые ошибки транслируются в [`CommandError`] и событие `ai:error` — без паник.

use tauri::{AppHandle, Emitter, State};

use crate::ai::gate;
use crate::ai::skills;
use crate::ai::state::AiState;
use crate::ai::transport::{ChatRequest, TestRequest};
use crate::ai::types::{
    AiMessage, AiResponse, AiSettingsInput, AiSettingsView, AiSkill, AiTestResult,
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

/// Возвращает встроенную библиотеку скиллов MVP (ровно 2, TZ §7.1).
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

    state.enforce_rate_limit();

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

    if let Err(e) = app.emit(
        "ai:done",
        serde_json::json!({ "message_id": message_id, "usage": outcome.usage }),
    ) {
        eprintln!("не эмитить ai:done: {e}");
    }

    Ok(AiResponse {
        message: AiMessage {
            role: "assistant".to_string(),
            content: outcome.content,
        },
        usage: outcome.usage,
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
    }

    #[test]
    fn message_ids_are_unique() {
        let first = next_message_id();
        let second = next_message_id();
        assert_ne!(first, second);
        assert!(first.starts_with("ai-"));
    }

    #[test]
    fn list_skills_returns_two() {
        assert_eq!(ai_list_skills().len(), 2);
    }
}
