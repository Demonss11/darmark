//! Маппинг ответов провайдера в коды ошибок (ADR-0032 §2.5 п.13).
//!
//! Чистые функции `(status, body, retry_after) → CommandError` без сети — тестируются
//! юнит-тестами. Тело ответа обрезается и не логируется как содержимое (только текст
//! ошибки провайдера для чата).

use crate::error::{CommandError, ErrorCode};

/// Максимум символов тела ответа, попадающих в сообщение (защита статусбара от гигантского тела).
const MAX_BODY_CHARS: usize = 400;

/// Признаки превышения контекста модели в теле ошибки (HTTP 400).
const CONTEXT_MARKERS: [&str; 6] = [
    "context length",
    "context_length",
    "maximum context",
    "too many tokens",
    "reduce the length",
    "token limit",
];

/// Переводит HTTP-статус и тело ответа в ошибку с понятным кодом и сообщением.
///
/// `retry_after` — значение заголовка `Retry-After`, если он есть (для 429).
pub fn map_status(status: u16, body: &str, retry_after: Option<&str>) -> CommandError {
    let detail = summarize(body);
    match status {
        401 => CommandError::new(ErrorCode::AiBadKey, "Неверный или отсутствующий API-ключ"),
        429 => {
            let mut message = "Превышен лимит запросов к провайдеру".to_string();
            if let Some(retry) = retry_after.filter(|value| !value.trim().is_empty()) {
                message.push_str(&format!(" (повторить через {} с)", retry.trim()));
            }
            append_detail(&mut message, &detail);
            CommandError::new(ErrorCode::AiRateLimited, message)
        }
        400 if is_context_length(body) => {
            let mut message = "Запрос превышает контекст модели".to_string();
            append_detail(&mut message, &detail);
            CommandError::new(ErrorCode::AiContextLength, message)
        }
        400 => {
            let mut message = "Некорректный запрос к провайдеру".to_string();
            append_detail(&mut message, &detail);
            CommandError::new(ErrorCode::AiBadRequest, message)
        }
        500..=599 => {
            let mut message = format!("Провайдер вернул ошибку {status}");
            append_detail(&mut message, &detail);
            CommandError::new(ErrorCode::AiServerError, message)
        }
        _ => {
            let mut message = format!("Провайдер отклонил запрос ({status})");
            append_detail(&mut message, &detail);
            CommandError::new(ErrorCode::AiBadRequest, message)
        }
    }
}

/// Есть ли в теле ответа признак превышения контекста модели.
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

/// Достаёт читаемый текст ошибки из OpenAI-конверта `{"error":{"message": ...}}` либо из тела.
fn summarize(body: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unauthorized_maps_to_bad_key() {
        let err = map_status(401, "", None);
        assert_eq!(err.code, ErrorCode::AiBadKey);
    }

    #[test]
    fn rate_limited_includes_retry_after() {
        let err = map_status(429, "quota exceeded", Some("30"));
        assert_eq!(err.code, ErrorCode::AiRateLimited);
        assert!(
            err.message.contains("30"),
            "сообщение несёт Retry-After: {}",
            err.message
        );
    }

    #[test]
    fn context_length_detected_from_body() {
        let err = map_status(
            400,
            "This model's maximum context length is 8192 tokens",
            None,
        );
        assert_eq!(err.code, ErrorCode::AiContextLength);
    }

    #[test]
    fn plain_bad_request_is_not_context_length() {
        let err = map_status(400, "invalid parameter: temperature", None);
        assert_eq!(err.code, ErrorCode::AiBadRequest);
    }

    #[test]
    fn server_errors_are_transient_code() {
        assert_eq!(map_status(500, "", None).code, ErrorCode::AiServerError);
        assert_eq!(
            map_status(503, "unavailable", None).code,
            ErrorCode::AiServerError
        );
    }

    #[test]
    fn other_client_errors_map_to_bad_request() {
        assert_eq!(map_status(403, "", None).code, ErrorCode::AiBadRequest);
        assert_eq!(map_status(404, "", None).code, ErrorCode::AiBadRequest);
    }

    #[test]
    fn error_message_extracted_from_openai_envelope() {
        let body =
            "{\"error\":{\"message\":\"model not found\",\"type\":\"invalid_request_error\"}}";
        let err = map_status(400, body, None);
        assert!(err.message.contains("model not found"), "{}", err.message);
    }
}
