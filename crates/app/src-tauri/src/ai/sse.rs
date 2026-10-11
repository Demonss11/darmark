//! Инкрементальный парсер SSE OpenAI-совместимого API (ADR-0033 §2.2).
//!
//! Поток приходит чанками `Response::bytes_stream()`. Парсер буферизует неполные строки и
//! байты многобайтового UTF-8, разрезанные по границе чанка, и устойчив к CRLF, пустым
//! строкам, комментариям (`:`) и битому JSON — ошибка разбора пропускается, **без паник**.

use crate::ai::types::AiUsage;

/// Разобранное событие SSE-потока.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SseEvent {
    /// Инкрементальный кусок текста (`choices[0].delta.content`).
    Delta(String),
    /// Токены использования (`usage`), если провайдер их прислал (часто в последнем чанке).
    Usage(AiUsage),
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
            line.pop(); // убрать '\n'
            if line.last() == Some(&b'\r') {
                line.pop(); // CRLF: убрать '\r'
            }
            events.extend(parse_line(&line));
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
    // Битый UTF-8 не должен ронять парсер: `from_utf8_lossy` заменяет проблемные байты.
    let text = String::from_utf8_lossy(line);
    let trimmed = text.trim_start();
    // Пустая строка — граница события; строка с `:` — комментарий (keep-alive).
    if trimmed.is_empty() || trimmed.starts_with(':') {
        return Vec::new();
    }
    let Some(data) = trimmed.strip_prefix("data:") else {
        return Vec::new(); // прочие поля SSE (`event:`, `id:`, `retry:`) не нужны
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
        // Точка разреза — сразу после ведущего байта кириллицы (внутри многобайтового символа).
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
        // Non-streaming формат (`message.content`) в SSE-потоке не ожидается — парсер не падает.
        let mut parser = SseParser::new();
        let events = parser.push(b"data: {\"choices\":[{\"message\":{\"content\":\"x\"}}]}\n");
        assert!(events.is_empty());
    }
}
