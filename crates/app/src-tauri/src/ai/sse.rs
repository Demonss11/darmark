//! Инкрементальный парсер SSE OpenAI-совместимого API (ADR-0033 §2.2).
//!
//! Поток приходит чанками `Response::bytes_stream()`. Парсер буферизует неполные строки и
//! байты многобайтового UTF-8, разрезанные по границе чанка, и устойчив к CRLF, пустым
//! строкам, комментариям (`:`) и битому JSON — ошибка разбора пропускается, **без паник**.

use crate::ai::types::AiUsage;

/// Максимум байт неполной строки в буфере. Защита от неограниченного роста, если в
/// недоверенном потоке нет `\n` (валидные SSE-строки на порядки короче).
pub(super) const MAX_BUFFER_BYTES: usize = 1024 * 1024;

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
