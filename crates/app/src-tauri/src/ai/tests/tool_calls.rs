//! Тесты разбора tool_calls из ответов провайдера.

use crate::ai::tool_calls::{tool_calls_from_payload, ToolCallAccumulator};

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
