//! Маппинг ответов провайдера в коды ошибок.
//!
//! Чистые функции `(status, body, retry_after) → CommandError` без сети — тестируются
//! юнит-тестами. Тело ответа обрезается и не логируется как содержимое (только текст
//! ошибки провайдера для чата). Единственный писатель `http_status` ошибки.

use crate::error::{CommandError, ErrorCode};


const MAX_BODY_CHARS: usize = 400;


const CONTEXT_MARKERS: [&str; 6] = [
    "context length",
    "context_length",
    "maximum context",
    "too many tokens",
    "reduce the length",
    "token limit",
];








pub fn map_status(status: u16, body: &str, retry_after: Option<&str>) -> CommandError {
    let detail = error_detail(body);
    match status {
        401 => CommandError::provider(
            status,
            ErrorCode::AiBadKey,
            "Неверный или отсутствующий API-ключ",
        ),
        429 => {
            let mut message = "Превышен лимит запросов к провайдеру".to_string();
            if let Some(retry) = retry_after.filter(|value| !value.trim().is_empty()) {
                message.push_str(&format!(" (повторить через {} с)", retry.trim()));
            }
            append_detail(&mut message, &detail);
            CommandError::provider(status, ErrorCode::AiRateLimited, message)
        }
        400 if is_context_length(body) => {
            let mut message = "Запрос превышает контекст модели".to_string();
            append_detail(&mut message, &detail);
            CommandError::provider(status, ErrorCode::AiContextLength, message)
        }
        400 => {
            let mut message = "Некорректный запрос к провайдеру".to_string();
            append_detail(&mut message, &detail);
            CommandError::provider(status, ErrorCode::AiBadRequest, message)
        }
        500..=599 => {
            let mut message = format!("Провайдер вернул ошибку {status}");
            append_detail(&mut message, &detail);
            CommandError::provider(status, ErrorCode::AiServerError, message)
        }
        _ => {
            let mut message = format!("Провайдер отклонил запрос ({status})");
            append_detail(&mut message, &detail);
            CommandError::provider(status, ErrorCode::AiBadRequest, message)
        }
    }
}


fn is_context_length(body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    CONTEXT_MARKERS.iter().any(|marker| lower.contains(marker))
}

fn append_detail(message: &mut String, detail: &str) {
    if !detail.is_empty() {
        message.push_str(": ");
        message.push_str(detail);
    }
}


fn error_detail(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if let Some(message) = value
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(|message| message.as_str())
        {
            return truncate(message);
        }
    }
    truncate(trimmed)
}

fn truncate(text: &str) -> String {
    let mut result: String = text.chars().take(MAX_BODY_CHARS).collect();
    if text.chars().count() > MAX_BODY_CHARS {
        result.push('…');
    }
    result
}
