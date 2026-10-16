//! Разбор и накопление tool_calls из ответов провайдера AI.
//!
//! Два формата: инкрементальные SSE-дельты (стриминг) и не-стриминг-конверт
//! `choices[0].message.tool_calls[]`. Аккумулятор склеивает фрагменты одного
//! вызова по `index`.

use std::collections::BTreeMap;

use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};






#[derive(Clone, Debug, PartialEq)]
pub struct ToolCall {
    
    pub id: String,
    
    pub name: String,
    
    pub arguments: serde_json::Value,
}


#[derive(Serialize)]
struct FunctionPayload<'a> {
    name: &'a str,
    arguments: String,
}


fn arguments_wire(arguments: &serde_json::Value) -> String {
    match arguments {
        serde_json::Value::Null => "{}".to_string(),
        value => value.to_string(),
    }
}

impl Serialize for ToolCall {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let function = FunctionPayload {
            name: &self.name,
            arguments: arguments_wire(&self.arguments),
        };
        let mut state = serializer.serialize_struct("ToolCall", 3)?;
        state.serialize_field("id", &self.id)?;
        state.serialize_field("type", "function")?;
        state.serialize_field("function", &function)?;
        state.end()
    }
}


#[derive(Deserialize)]
struct ToolCallRepr {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<serde_json::Value>,
    #[serde(default)]
    function: Option<FunctionRepr>,
}


#[derive(Deserialize)]
struct FunctionRepr {
    #[serde(default)]
    name: String,
    #[serde(default)]
    arguments: Option<serde_json::Value>,
}




impl<'de> Deserialize<'de> for ToolCall {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let repr = ToolCallRepr::deserialize(deserializer)?;
        let (name, arguments) = match repr.function {
            Some(function) => (function.name, function.arguments),
            None => (repr.name.unwrap_or_default(), repr.arguments),
        };
        Ok(ToolCall {
            id: repr.id,
            name,
            arguments: parse_arguments(arguments.as_ref()),
        })
    }
}


#[derive(Debug, Default)]
pub struct ToolCallAccumulator {
    
    calls: BTreeMap<u32, CallBuffer>,
}


#[derive(Debug, Default)]
struct CallBuffer {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

impl ToolCallAccumulator {
    
    
    
    
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


fn merge_field(current: &mut Option<String>, new: Option<String>) {
    if current.as_deref().is_some_and(|s| !s.is_empty()) {
        return;
    }
    if let Some(value) = new.filter(|s| !s.is_empty()) {
        *current = Some(value);
    }
}





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


fn parse_arguments(value: Option<&serde_json::Value>) -> serde_json::Value {
    match value {
        Some(serde_json::Value::String(s)) => {
            serde_json::from_str(s).unwrap_or(serde_json::Value::Null)
        }
        Some(v) => v.clone(),
        None => serde_json::Value::Null,
    }
}
