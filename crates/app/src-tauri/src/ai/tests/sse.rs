//! Тесты SSE-парсера [`super::super::sse`].

use crate::ai::sse::{SseEvent, SseParser, MAX_BUFFER_BYTES};
use crate::ai::types::AiUsage;

fn delta(content: &str) -> SseEvent {
    SseEvent::Delta(content.to_string())
}

#[test]
fn parses_canonical_openai_stream() {
    let mut parser = SseParser::new();
    let stream = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\", \"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"world\"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{}}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
        "data: [DONE]\n\n",
    );
    let events = parser.push(stream.as_bytes());
    assert_eq!(
        events,
        vec![
            delta("Hello"),
            delta(", "),
            delta("world"),
            SseEvent::Usage(AiUsage {
                prompt_tokens: 7,
                completion_tokens: 3
            }),
            SseEvent::Done,
        ]
    );
}

#[test]
fn handles_crlf_blank_lines_and_comments() {
    let mut parser = SseParser::new();
    let stream = ": keep-alive\r\n\r\ndata: {\"choices\":[{\"delta\":{\"content\":\"ok\"}}]}\r\n\r\ndata: [DONE]\r\n";
    let events = parser.push(stream.as_bytes());
    assert_eq!(events, vec![delta("ok"), SseEvent::Done]);
}

#[test]
fn skips_broken_json_without_panic() {
    let mut parser = SseParser::new();
    let stream = "data: {not json}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"fine\"}}]}\n\n";
    let events = parser.push(stream.as_bytes());
    assert_eq!(events, vec![delta("fine")]);
}

#[test]
fn skips_invalid_utf8_without_panic() {
    let mut parser = SseParser::new();
    let mut chunk = b"data: ".to_vec();
    chunk.extend_from_slice(&[0xff, 0xfe]);
    chunk.push(b'\n');
    assert!(parser.push(&chunk).is_empty());
}

#[test]
fn buffers_partial_line_across_chunks() {
    let first = b"data: {\"choices\":[{\"del";
    let mut parser = SseParser::new();
    assert!(parser.push(first).is_empty());
    assert_eq!(
        parser.pending_len(),
        first.len(),
        "неполная строка буферизована"
    );
    let events = parser.push(b"ta\":{\"content\":\"hi\"}}]}\n");
    assert_eq!(events, vec![delta("hi")]);
    assert_eq!(parser.pending_len(), 0);
}

#[test]
fn reassembles_multibyte_utf8_split_mid_character() {
    let full = "data: {\"choices\":[{\"delta\":{\"content\":\"Привет\"}}]}\n";
    let bytes = full.as_bytes();
    let split = bytes
        .iter()
        .position(|&b| b >= 0x80)
        .map(|i| i + 1)
        .expect("в строке есть многобайтовый символ");

    let mut parser = SseParser::new();
    assert!(parser.push(&bytes[..split]).is_empty(), "хвост буферизован");
    let events = parser.push(&bytes[split..]);
    assert_eq!(events, vec![delta("Привет")]);
}

#[test]
fn empty_delta_produces_no_event() {
    let mut parser = SseParser::new();
    let events = parser.push(b"data: {\"choices\":[{\"delta\":{\"content\":\"\"}}]}\n");
    assert!(events.is_empty());
}

#[test]
fn non_streaming_message_content_is_ignored() {
    let mut parser = SseParser::new();
    let events = parser.push(b"data: {\"choices\":[{\"message\":{\"content\":\"x\"}}]}\n");
    assert!(events.is_empty());
}

#[test]
fn oversized_partial_line_is_dropped() {
    let mut parser = SseParser::new();
    let chunk = vec![b'x'; MAX_BUFFER_BYTES + 1024];
    assert!(parser.push(&chunk).is_empty());
    assert_eq!(
        parser.pending_len(),
        0,
        "переполненный буфер неполной строки сброшен"
    );
}

#[test]
fn buffer_stays_bounded_across_many_chunks_without_newline() {
    let mut parser = SseParser::new();
    for _ in 0..8 {
        parser.push(&vec![b'x'; 256 * 1024]);
        assert!(
            parser.pending_len() <= MAX_BUFFER_BYTES,
            "буфер не превышает лимит: {}",
            parser.pending_len()
        );
    }
}

#[test]
fn complete_lines_after_overflow_are_still_parsed() {
    let mut parser = SseParser::new();
    parser.push(&vec![b'x'; MAX_BUFFER_BYTES + 1024]);
    let events = parser.push(b"data: {\"choices\":[{\"delta\":{\"content\":\"ok\"}}]}\n");
    assert_eq!(events, vec![delta("ok")]);
}

#[test]
fn parses_finish_reason_before_done() {
    let mut parser = SseParser::new();
    let stream = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
        "data: [DONE]\n\n",
    );
    let events = parser.push(stream.as_bytes());
    assert_eq!(
        events,
        vec![
            delta("Hello"),
            SseEvent::Finish("stop".to_string()),
            SseEvent::Done,
        ]
    );
}

#[test]
fn parses_finish_reason_length_on_truncated_response() {
    let mut parser = SseParser::new();
    let stream = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"\"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"length\"}]}\n\n",
        "data: [DONE]\n\n",
    );
    let events = parser.push(stream.as_bytes());
    assert_eq!(
        events,
        vec![SseEvent::Finish("length".to_string()), SseEvent::Done,]
    );
}

#[test]
fn missing_or_empty_finish_reason_produces_no_event() {
    let mut parser = SseParser::new();
    let stream = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"a\"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"\"}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"b\"}}]}\n\n",
    );
    let events = parser.push(stream.as_bytes());
    assert_eq!(events, vec![delta("a"), delta("b")]);
}

#[test]
fn finish_reason_alongside_usage_and_done() {
    let mut parser = SseParser::new();
    let stream = concat!(
        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"length\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":20}}\n\n",
        "data: [DONE]\n\n",
    );
    let events = parser.push(stream.as_bytes());
    assert_eq!(
        events,
        vec![
            SseEvent::Usage(AiUsage {
                prompt_tokens: 10,
                completion_tokens: 20
            }),
            SseEvent::Finish("length".to_string()),
            SseEvent::Done,
        ]
    );
}

#[test]
fn sse_parses_tool_call_delta_fragments() {
    let mut parser = SseParser::new();
    let line = br#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"plugin_validate","arguments":"{\"path\":"}}]}}]}"#;
    let mut events = parser.push(line);
    events.extend(parser.push(b"\n\n"));
    assert!(events.iter().any(|e| matches!(
        e,
        SseEvent::ToolCallDelta { index: 0, id: Some(id), name: Some(name), .. }
        if id == "call_1" && name == "plugin_validate"
    )));
}

#[test]
fn sse_parses_multiple_tool_call_deltas_in_one_event() {
    let mut parser = SseParser::new();
    let line = br#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"tool_a","arguments":"{\"a\":"}},{"index":1,"id":"call_2","function":{"name":"tool_b","arguments":"{\"b\":"}}]}}]}"#;
    let mut events = parser.push(line);
    events.extend(parser.push(b"\n\n"));
    let deltas: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, SseEvent::ToolCallDelta { .. }))
        .collect();
    assert_eq!(deltas.len(), 2);
}

#[test]
fn sse_skips_tool_call_delta_with_all_empty_fields() {
    let mut parser = SseParser::new();
    let line = br#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"","function":{"name":"","arguments":""}}]}}]}"#;
    let events = parser.push(line);
    assert!(events
        .iter()
        .all(|e| !matches!(e, SseEvent::ToolCallDelta { .. })));
}

#[test]
fn sse_tool_call_delta_empty_string_fields_become_none() {
    let mut parser = SseParser::new();
    let line = br#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"","function":{"name":"","arguments":"{\"k\":1}"}}]}}]}"#;
    let mut events = parser.push(line);
    events.extend(parser.push(b"\n\n"));
    assert_eq!(
        events
            .iter()
            .find(|e| matches!(e, SseEvent::ToolCallDelta { .. })),
        Some(&SseEvent::ToolCallDelta {
            index: 0,
            id: None,
            name: None,
            arguments: Some("{\"k\":1}".to_string()),
        })
    );
}
