//! Инкрементальный парсер SSE OpenAI-совместимого API (ADR-0033 §2.2).
//!
//! Поток приходит чанками `Response::bytes_stream()`. Парсер буферизует неполные строки и
//! байты многобайтового UTF-8, разрезанные по границе чанка, и устойчив к CRLF, пустым
//! строкам, комментариям (`:`) и битому JSON — ошибка разбора пропускается, **без паник**.

use crate::ai::types::AiUsage;

/// Максимум байт неполной строки в буфере. Защита от неограниченного роста, если в
/// недоверенном потоке нет `\n` (валидные SSE-строки на порядки короче).
const MAX_BUFFER_BYTES: usize = 1024 * 1024;

/// Разобранное событие SSE-потока.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SseEvent {
    /// Инкрементальный кусок текста (`choices[0].delta.content`).
    Delta(String),
    /// Токены использования (`usage`), если провайдер их прислал (часто в последнем чанке).
    Usage(AiUsage),
    /// Причина завершения (`choices[0].finish_reason`): `"stop"`, `"length"` и т.д.
    Finish(String),
    /// Терминатор `data: [DONE]`.
    Done,
}

/// Инкрементальный построчный парсер SSE.
#[derive(Debug, Default)]
pub struct SseParser {
    /// Накопленные байты неполной строки (включая разрезанный UTF-8).
    buffer: Vec<u8>,
}

impl SseParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Добавляет очередной чанк байт и возвращает все **полностью** разобранные события.
    ///
    /// Разделитель — байт `\n` (не входит в многобайтовые последовательности UTF-8), поэтому
    /// строка декодируется целиком; неполный хвост остаётся в буфере до следующего чанка.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<SseEvent> {
        self.buffer.extend_from_slice(chunk);
        let mut events = Vec::new();
        while let Some(pos) = self.buffer.iter().position(|&b| b == b'\n') {
            let mut line: Vec<u8> = self.buffer.drain(..=pos).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            events.extend(parse_line(&line));
        }
        if self.buffer.len() > MAX_BUFFER_BYTES {
            self.buffer.clear();
        }
        events
    }

    /// Длина неполного хвоста буфера (диагностика/тесты).
    pub fn pending_len(&self) -> usize {
        self.buffer.len()
    }
}

/// Разбирает одну строку SSE в ноль или более событий (одна строка может нести и `delta`, и `usage`).
fn parse_line(line: &[u8]) -> Vec<SseEvent> {
    let text = String::from_utf8_lossy(line);
    let trimmed = text.trim_start();
    if trimmed.is_empty() || trimmed.starts_with(':') {
        return Vec::new();
    }
    let Some(data) = trimmed.strip_prefix("data:") else {
        return Vec::new();
    };
    let payload = data.strip_prefix(' ').unwrap_or(data).trim();
    if payload == "[DONE]" {
        return vec![SseEvent::Done];
    }
    parse_payload(payload)
}

/// Разбирает JSON-payload одного `data:`-события; битый JSON → пусто (пропуск, не паника).
fn parse_payload(payload: &str) -> Vec<SseEvent> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(payload) else {
        return Vec::new();
    };
    let mut events = Vec::new();
    if let Some(delta) = value
        .get("choices")
        .and_then(|choices| choices.get(0usize))
        .and_then(|choice| choice.get("delta"))
        .and_then(|delta| delta.get("content"))
        .and_then(|content| content.as_str())
    {
        if !delta.is_empty() {
            events.push(SseEvent::Delta(delta.to_string()));
        }
    }
    if let Some(usage) = parse_usage(&value) {
        events.push(SseEvent::Usage(usage));
    }
    if let Some(finish_reason) = value
        .get("choices")
        .and_then(|choices| choices.get(0usize))
        .and_then(|choice| choice.get("finish_reason"))
        .and_then(|reason| reason.as_str())
    {
        if !finish_reason.is_empty() {
            events.push(SseEvent::Finish(finish_reason.to_string()));
        }
    }
    events
}

/// Извлекает `usage.{prompt_tokens,completion_tokens}`, если он есть и корректен.
fn parse_usage(value: &serde_json::Value) -> Option<AiUsage> {
    let usage = value.get("usage")?;
    let prompt = usage.get("prompt_tokens")?.as_u64()?;
    let completion = usage.get("completion_tokens")?.as_u64()?;
    Some(AiUsage {
        prompt_tokens: u32::try_from(prompt).unwrap_or(u32::MAX),
        completion_tokens: u32::try_from(completion).unwrap_or(u32::MAX),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let stream =
            "data: {not json}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"fine\"}}]}\n\n";
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
}
