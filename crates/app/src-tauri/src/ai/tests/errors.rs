//! Тесты маппинга ошибок [`super::super::errors::map_status`].

use crate::ai::errors::map_status;
use crate::error::ErrorCode;

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
    let body = "{\"error\":{\"message\":\"model not found\",\"type\":\"invalid_request_error\"}}";
    let err = map_status(400, body, None);
    assert!(err.message.contains("model not found"), "{}", err.message);
}
