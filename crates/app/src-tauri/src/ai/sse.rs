//! Инкрементальный парсер SSE OpenAI-совместимого API (ADR-0033 §2.2).
//!
//! Поток приходит чанками `Response::bytes_stream()`. Парсер буферизует неполные строки и
//! байты многобайтового UTF-8, разрезанные по границе чанка, и устойчив к CRLF, пустым
//! строкам, комментариям (`:`) и битому JSON — ошибка разбора пропускается, **без паник**.

use crate::ai::types::AiUsage;



pub(super) const MAX_BUFFER_BYTES: usize = 1024 * 1024;


#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SseEvent {
    
    Delta(String),
    
    Usage(AiUsage),
    
    Finish(String),
    
    Done,
    
    ToolCallDelta {
        
        index: u32,
        
        id: Option<String>,
        
        name: Option<String>,
        
        arguments: Option<String>,
    },
}


#[derive(Debug, Default)]
pub struct SseParser {
    
    buffer: Vec<u8>,
}

impl SseParser {
    pub fn new() -> Self {
        Self::default()
    }

    
    
    
    
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

    
    pub fn pending_len(&self) -> usize {
        self.buffer.len()
    }
}


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
    if let Some(tool_calls) = value
        .get("choices")
        .and_then(|choices| choices.get(0usize))
        .and_then(|choice| choice.get("delta"))
        .and_then(|delta| delta.get("tool_calls"))
        .and_then(|tc| tc.as_array())
    {
        for tc in tool_calls {
            if let Some(event) = parse_tool_call_delta(tc) {
                events.push(event);
            }
        }
    }
    events
}




fn parse_tool_call_delta(value: &serde_json::Value) -> Option<SseEvent> {
    let index = u32::try_from(value.get("index")?.as_u64()?).ok()?;
    let id = value
        .get("id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from);
    let name = value
        .get("function")
        .and_then(|f| f.get("name"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from);
    let arguments = value
        .get("function")
        .and_then(|f| f.get("arguments"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from);
    if id.is_none() && name.is_none() && arguments.is_none() {
        return None;
    }
    Some(SseEvent::ToolCallDelta {
        index,
        id,
        name,
        arguments,
    })
}


fn parse_usage(value: &serde_json::Value) -> Option<AiUsage> {
    let usage = value.get("usage")?;
    let prompt = usage.get("prompt_tokens")?.as_u64()?;
    let completion = usage.get("completion_tokens")?.as_u64()?;
    Some(AiUsage {
        prompt_tokens: u32::try_from(prompt).unwrap_or(u32::MAX),
        completion_tokens: u32::try_from(completion).unwrap_or(u32::MAX),
    })
}
