//! Тесты шва [`super::super::transport`].

use crate::ai::transport::{ChatRequest, TestRequest};
use crate::ai::types::AiMessage;
use std::collections::BTreeMap;

fn request_with_secret() -> ChatRequest {
    ChatRequest {
        base_url: "https://example.test/v1".to_string(),
        model: "m".to_string(),
        api_key: "sk-secret-1234".to_string(),
        messages: vec![AiMessage {
            role: "user".to_string(),
            content: "hi".to_string(),
            ..AiMessage::default()
        }],
        max_tokens: 16,
        temperature: 0.0,
        headers: BTreeMap::from([("X-Tenant".to_string(), "acme".to_string())]),
        extra_body: serde_json::Map::new(),
        timeout: None,
        tools: None,
    }
}

#[test]
fn chat_request_debug_masks_secret() {
    let rendered = format!("{:?}", request_with_secret());
    assert!(!rendered.contains("sk-secret"), "секрет скрыт: {rendered}");
    assert!(
        rendered.contains("<redacted>"),
        "секрет маскируется: {rendered}"
    );
    assert!(
        !rendered.contains("acme"),
        "значение заголовка не разворачивается: {rendered}"
    );
    assert!(rendered.contains("X-Tenant"), "имя заголовка видно");
}

#[test]
fn test_request_debug_masks_secret() {
    let request = TestRequest {
        base_url: "https://example.test/v1".to_string(),
        api_key: "sk-secret-1234".to_string(),
        headers: BTreeMap::new(),
        timeout: None,
    };
    let rendered = format!("{request:?}");
    assert!(!rendered.contains("sk-secret"), "секрет скрыт: {rendered}");
    assert!(
        rendered.contains("<redacted>"),
        "секрет маскируется: {rendered}"
    );
}
