//! Разбор и накопление tool_calls из ответов провайдера AI.
//!
//! Два формата: инкрементальные SSE-дельты (стриминг) и не-стриминг-конверт
//! `choices[0].message.tool_calls[]`. Аккумулятор склеивает фрагменты одного
//! вызова по `index`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Один вызов инструмента от провайдера.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    /// Идентификатор вызова (для сопоставления результата).
    pub id: String,
    /// Имя инструмента.
    pub name: String,
    /// Аргументы вызова (JSON).
    pub arguments: serde_json::Value,
}

/// Накопитель фрагментов tool_calls из SSE-потока.
#[derive(Debug, Default)]
pub struct ToolCallAccumulator {
    /// Фрагменты вызовов, сгруппированные по индексу.
    calls: BTreeMap<u32, CallBuffer>,
}

/// Промежуточное состояние одного вызова.
#[derive(Debug, Default)]
struct CallBuffer {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

impl ToolCallAccumulator {
    /// Добавляет фрагмент вызова с заданным индексом.
    ///
    /// `id`/`name` — берутся из первого непустого значения; `arguments` — конкатенация
    /// всех фрагментов.
    pub fn push(
        &mut self,
        index: u32,
        id: Option<String>,
        name: Option<String>,
        arguments: Option<String>,
    ) {
        let buffer = self.calls.entry(index).or_default();
        merge_field(&mut buffer.id, id);
        merge_field(&mut buffer.name, name);
        if let Some(args) = arguments {
            buffer.arguments.push_str(&args);
        }
    }

    /// Завершает накопление: возвращает вызовы, отсортированные по `index`.
    ///
    /// `arguments` каждого вызова парсится из склеенной строки JSON; при битом JSON —
    /// `Value::Null` (не паника).
    pub fn finish(&self) -> Vec<ToolCall> {
        self.calls
            .values()
            .map(|buffer| ToolCall {
                id: buffer.id.clone().unwrap_or_default(),
                name: buffer.name.clone().unwrap_or_default(),
                arguments: serde_json::from_str(&buffer.arguments)
                    .unwrap_or(serde_json::Value::Null),
            })
            .collect()
    }
}

/// Объединяет поле: первое непустое значение фиксируется, последующие игнорируются.
fn merge_field(current: &mut Option<String>, new: Option<String>) {
    if current.as_deref().is_some_and(|s| !s.is_empty()) {
        return;
    }
    if let Some(value) = new.filter(|s| !s.is_empty()) {
        *current = Some(value);
    }
}

/// Разбирает tool_calls из не-стриминг-конверта `choices[0].message.tool_calls[]`.
///
/// `arguments` может быть строкой JSON или готовым объектом; при битом JSON —
/// `Value::Null` (не паника).
pub fn tool_calls_from_payload(value: &serde_json::Value) -> Vec<ToolCall> {
    let mut calls = Vec::new();
    if let Some(tool_calls) = value
        .get("choices")
        .and_then(|choices| choices.get(0usize))
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("tool_calls"))
        .and_then(|tc| tc.as_array())
    {
        for tc in tool_calls {
            if let Some(call) = parse_tool_call(tc) {
                calls.push(call);
            }
        }
    }
    calls
}

/// Разбирает один элемент `tool_calls[]`.
fn parse_tool_call(value: &serde_json::Value) -> Option<ToolCall> {
    let function = value.get("function")?;
    Some(ToolCall {
        id: value
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        name: function
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        arguments: parse_arguments(function.get("arguments")),
    })
}

/// Парсит `arguments` из элемента `tool_calls[]`: строка JSON → Value, прочее — как есть.
fn parse_arguments(value: Option<&serde_json::Value>) -> serde_json::Value {
    match value {
        Some(serde_json::Value::String(s)) => {
            serde_json::from_str(s).unwrap_or(serde_json::Value::Null)
        }
        Some(v) => v.clone(),
        None => serde_json::Value::Null,
    }
}
