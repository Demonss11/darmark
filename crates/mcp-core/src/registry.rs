//! Реестр инструментов и ресурсов.

use std::sync::Arc;

use serde_json::{json, Value};

use crate::handlers::{ResourceError, ResourceReader, ToolError, ToolHandler};
use crate::protocol;

/// Определение инструмента в реестре.
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub handler: Arc<dyn ToolHandler>,
}

/// Определение ресурса в реестре.
pub struct ResourceDefinition {
    pub uri: String,
    pub description: String,
    pub reader: Arc<dyn ResourceReader>,
}

/// Реестр инструментов и ресурсов.
#[derive(Default)]
pub struct Registry {
    tools: Vec<ToolDefinition>,
    resources: Vec<ResourceDefinition>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_tool(&mut self, def: ToolDefinition) {
        self.tools.push(def);
    }

    pub fn register_resource(&mut self, def: ResourceDefinition) {
        self.resources.push(def);
    }

    /// Список инструментов для `tools/list`.
    pub fn tools_list(&self) -> Value {
        let tools: Vec<Value> = self
            .tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "inputSchema": t.input_schema,
                })
            })
            .collect();
        json!({ "tools": tools })
    }

    /// Список ресурсов для `resources/list`.
    pub fn resources_list(&self) -> Value {
        let resources: Vec<Value> = self
            .resources
            .iter()
            .map(|r| {
                json!({
                    "uri": r.uri,
                    "description": r.description,
                })
            })
            .collect();
        json!({ "resources": resources })
    }

    /// Вызов инструмента по имени.
    pub fn call_tool(&self, name: &str, params: Value) -> Result<Value, ToolError> {
        let tool = self.tools.iter().find(|t| t.name == name).ok_or_else(|| {
            ToolError::new(
                protocol::METHOD_NOT_FOUND,
                format!("инструмент не найден: {name}"),
            )
        })?;
        tool.handler.call(params)
    }

    /// Чтение ресурса по URI.
    pub fn read_resource(&self, uri: &str) -> Result<Value, ResourceError> {
        let resource = self
            .resources
            .iter()
            .find(|r| r.uri == uri)
            .ok_or_else(|| {
                ResourceError::new(
                    protocol::METHOD_NOT_FOUND,
                    format!("ресурс не найден: {uri}"),
                )
            })?;
        resource.reader.read(uri)
    }
}
