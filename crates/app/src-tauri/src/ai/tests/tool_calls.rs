//! Тесты разбора tool_calls из ответов провайдера и их формы на проводе.

use std::collections::BTreeMap;

use crate::ai::backend::reqwest::build_chat_body;
use crate::ai::tool_calls::{tool_calls_from_payload, ToolCall, ToolCallAccumulator};
use crate::ai::transport::ChatRequest;
use crate::ai::types::AiMessage;


fn chat_request_with_call(arguments: serde_json::Value) -> ChatRequest {
    ChatRequest {
        base_url: "https://example.test/v1".to_string(),
        model: "provider/model".to_string(),
        api_key: "sk-test".to_string(),
        messages: vec![AiMessage {
            role: "assistant".to_string(),
            content: String::new(),
            tool_calls: Some(vec![ToolCall {
                id: "call-1".to_string(),
                name: "fake_tool".to_string(),
                arguments,
            }]),
            ..AiMessage::default()
        }],
        max_tokens: 16,
        temperature: 0.0,
        headers: BTreeMap::new(),
        extra_body: serde_json::Map::new(),
        timeout: None,
        tools: None,
    }
}



#[test]
fn tool_call_serializes_as_openai_function_form() {
    let body = build_chat_body(&chat_request_with_call(serde_json::json!({"a": 1})));
    let call = &body["messages"][0]["tool_calls"][0];
    assert_eq!(call["type"], "function");
    assert_eq!(call["function"]["name"], "fake_tool");
    let arguments = call["function"]["arguments"]
        .as_str()
        .expect("arguments — строка JSON, не объект");
    let parsed: serde_json::Value =
        serde_json::from_str(arguments).expect("строка аргументов парсится обратно");
    assert_eq!(parsed, serde_json::json!({"a": 1}));
}


#[test]
fn tool_call_null_arguments_serialize_as_empty_object_string() {
    let body = build_chat_body(&chat_request_with_call(serde_json::Value::Null));
    let call = &body["messages"][0]["tool_calls"][0];
    assert_eq!(call["function"]["arguments"], "{}");
}


#[test]
fn tool_call_deserializes_openai_and_legacy_forms() {
    let openai: ToolCall = serde_json::from_value(serde_json::json!({
        "id": "call-1",
        "type": "function",
        "function": {"name": "fake_tool", "arguments": "{\"a\":1}"}
    }))
    .expect("форма OpenAI разбирается");
    assert_eq!(openai.name, "fake_tool");
    assert_eq!(openai.arguments, serde_json::json!({"a": 1}));

    let legacy: ToolCall = serde_json::from_value(serde_json::json!({
        "id": "call-2",
        "name": "old_tool",
        "arguments": {"b": 2}
    }))
    .expect("плоская запись истории разбирается");
    assert_eq!(legacy.name, "old_tool");
    assert_eq!(legacy.arguments, serde_json::json!({"b": 2}));
}


#[test]
fn tool_call_broken_arguments_string_becomes_null() {
    let call: ToolCall = serde_json::from_value(serde_json::json!({
        "id": "call-1",
        "type": "function",
        "function": {"name": "fake_tool", "arguments": "not json"}
    }))
    .expect("битый JSON разбирается без паники");
    assert_eq!(call.arguments, serde_json::Value::Null);
}

#[test]
fn accumulator_merges_fragments_by_index() {
    let mut acc = ToolCallAccumulator::default();
    acc.push(
        0,
        Some("call_1".into()),
        Some("plugin_validate".into()),
        Some("{\"pa".into()),
    );
    acc.push(0, None, None, Some("th\":\"/x\"}".into()));
    let calls = acc.finish();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "plugin_validate");
    assert_eq!(calls[0].arguments["path"], "/x");
}

#[test]
fn tool_calls_from_message_envelope() {
    let value = serde_json::json!({"choices":[{"message":{"tool_calls":[
        {"id":"c1","function":{"name":"markdown_render","arguments":"{\"text\":\"# a\"}"}}
    ]}}]});
    let calls = tool_calls_from_payload(&value);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].arguments["text"], "# a");
}

#[test]
fn accumulator_sorts_by_index() {
    let mut acc = ToolCallAccumulator::default();
    acc.push(
        2,
        Some("c3".into()),
        Some("tool_c".into()),
        Some("{}".into()),
    );
    acc.push(
        0,
        Some("c1".into()),
        Some("tool_a".into()),
        Some("{\"a\":1}".into()),
    );
    acc.push(
        1,
        Some("c2".into()),
        Some("tool_b".into()),
        Some("{\"b\":2}".into()),
    );
    let calls = acc.finish();
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[0].name, "tool_a");
    assert_eq!(calls[1].name, "tool_b");
    assert_eq!(calls[2].name, "tool_c");
}

#[test]
fn accumulator_broken_json_arguments_becomes_null() {
    let mut acc = ToolCallAccumulator::default();
    acc.push(
        0,
        Some("c1".into()),
        Some("tool".into()),
        Some("not json".into()),
    );
    let calls = acc.finish();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].arguments, serde_json::Value::Null);
}

#[test]
fn accumulator_id_name_first_non_empty_wins() {
    let mut acc = ToolCallAccumulator::default();
    acc.push(0, None, None, Some("{}".into()));
    acc.push(0, Some("".into()), Some("".into()), None);
    acc.push(0, Some("real_id".into()), Some("real_name".into()), None);
    acc.push(0, Some("later_id".into()), Some("later_name".into()), None);
    let calls = acc.finish();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].id, "real_id");
    assert_eq!(calls[0].name, "real_name");
}

#[test]
fn tool_calls_from_payload_arguments_as_object() {
    let value = serde_json::json!({"choices":[{"message":{"tool_calls":[
        {"id":"c1","function":{"name":"tool","arguments":{"key":"value"}}}
    ]}}]});
    let calls = tool_calls_from_payload(&value);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].arguments["key"], "value");
}

#[test]
fn tool_calls_from_payload_empty_tool_calls() {
    let value = serde_json::json!({"choices":[{"message":{"tool_calls":[]}}]});
    let calls = tool_calls_from_payload(&value);
    assert!(calls.is_empty());
}

#[test]
fn tool_calls_from_payload_missing_tool_calls() {
    let value = serde_json::json!({"choices":[{"message":{"content":"no tools"}}]});
    let calls = tool_calls_from_payload(&value);
    assert!(calls.is_empty());
}
