//! Тесты reqwest-бэкенда [`super::super::backend::reqwest`].

use crate::ai::backend::reqwest::{
    build_chat_body, custom_headers, endpoint, ensure_within_response_limit, MAX_RESPONSE_BYTES,
};
use crate::ai::transport::ChatRequest;
use crate::ai::types::AiMessage;
use crate::error::ErrorCode;
use std::collections::BTreeMap;

fn chat_request(extra_body: serde_json::Map<String, serde_json::Value>) -> ChatRequest {
    ChatRequest {
        base_url: "https://example.test/v1".to_string(),
        model: "provider/model".to_string(),
        api_key: "sk-test".to_string(),
        messages: vec![AiMessage {
            role: "user".to_string(),
            content: "hi".to_string(),
        }],
        max_tokens: 16,
        temperature: 0.0,
        headers: BTreeMap::new(),
        extra_body,
        timeout: None,
        tools: None,
    }
}

#[test]
fn endpoint_avoids_double_slash() {
    assert_eq!(
        endpoint("https://example.test/v1/", "chat/completions"),
        "https://example.test/v1/chat/completions"
    );
}

#[test]
fn response_limit_allows_just_under_and_rejects_over() {
    assert!(ensure_within_response_limit(0, MAX_RESPONSE_BYTES).is_ok());
    let err = ensure_within_response_limit(MAX_RESPONSE_BYTES, 1).unwrap_err();
    assert_eq!(err.code, ErrorCode::AiBadRequest);
}

#[test]
fn response_limit_is_saturating() {
    let err = ensure_within_response_limit(usize::MAX, 1).unwrap_err();
    assert_eq!(err.code, ErrorCode::AiBadRequest);
}

#[test]
fn chat_body_merges_extra_body_without_overriding_protocol_fields() {
    let mut extra = serde_json::Map::new();
    extra.insert("model".to_string(), serde_json::json!("evil"));
    extra.insert("stream".to_string(), serde_json::json!(false));
    extra.insert(
        "metadata".to_string(),
        serde_json::json!({ "tenant": "acme" }),
    );
    let body = build_chat_body(&chat_request(extra));

    assert_eq!(body["model"], "provider/model", "протокол не переопределён");
    assert_eq!(body["stream"], true, "stream не переопределён");
    assert_eq!(body["metadata"]["tenant"], "acme", "надстройка влита");
    assert_eq!(body["max_tokens"], 16);
}

#[test]
fn chat_body_includes_tools_when_present() {
    let mut request = chat_request(serde_json::Map::new());
    request.tools = Some(serde_json::json!([
        {"type":"function","function":{"name":"plugin_validate","description":"Валидация пути","parameters":{}}}
    ]));
    let body = build_chat_body(&request);
    assert!(
        body.get("tools").is_some(),
        "tools должны быть в теле запроса"
    );
}

#[test]
fn chat_body_never_overrides_tools_with_extra_body() {
    let mut request = chat_request(serde_json::Map::new());
    request.tools = Some(serde_json::json!([{"type":"function","function":{"name":"real"}}]));
    request.extra_body.insert(
        "tools".into(),
        serde_json::json!([{"type":"function","function":{"name":"evil"}}]),
    );
    let body = build_chat_body(&request);
    assert_eq!(
        body["tools"][0]["function"]["name"], "real",
        "extra_body не должен переопределять tools"
    );
}

#[test]
fn custom_headers_drop_reserved_names() {
    let headers = BTreeMap::from([
        ("Authorization".to_string(), "Bearer evil".to_string()),
        ("Content-Type".to_string(), "text/plain".to_string()),
        ("X-Tenant".to_string(), "acme".to_string()),
    ]);
    let kept = custom_headers(&headers);
    let names: Vec<String> = kept
        .iter()
        .map(|(name, _)| name.as_str().to_string())
        .collect();
    assert_eq!(
        names,
        vec!["x-tenant".to_string()],
        "зарезервированные убраны"
    );
}
