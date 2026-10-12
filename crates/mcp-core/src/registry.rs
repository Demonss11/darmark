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
///
/// `name` обязателен по спецификации MCP (поле `Resource.name`) и отдаётся
/// агенту в `resources/list` — по нему агент понимает назначение ресурса.
pub struct ResourceDefinition {
    pub uri: String,
    pub name: String,
    pub description: String,
    pub reader: Arc<dyn ResourceReader>,
}

/// Результат `call_tool`: различает protocol-ошибку и ошибку исполнения.
///
/// «Инструмент не зарегистрирован» — это ошибка протокола (JSON-RPC error),
/// а не результат работы инструмента. Смешивать их нельзя: агент по `isError`
/// судит о сбое самого вызова, а не о неверном имени (спека MCP).
#[derive(Clone, Debug)]
pub enum CallToolError {
    /// Инструмент не зарегистрирован (protocol error, JSON-RPC `-32602`).
    UnknownTool(String),
    /// Ошибка исполнения инструмента (отдаётся как результат с `isError: true`).
    Execution(ToolError),
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
                    "name": r.name,
                    "description": r.description,
                })
            })
            .collect();
        json!({ "resources": resources })
    }

    /// Вызов инструмента по имени.
    ///
    /// Неизвестное имя — `CallToolError::UnknownTool` (protocol error);
    /// ошибка обработчика — `CallToolError::Execution`.
    pub fn call_tool(&self, name: &str, params: Value) -> Result<Value, CallToolError> {
        let tool = self
            .tools
            .iter()
            .find(|t| t.name == name)
            .ok_or_else(|| CallToolError::UnknownTool(name.to_string()))?;
        tool.handler.call(params).map_err(CallToolError::Execution)
    }

    /// Чтение ресурса по URI.
    pub fn read_resource(&self, uri: &str) -> Result<Value, ResourceError> {
        let resource = self
            .resources
            .iter()
            .find(|r| r.uri == uri)
            .ok_or_else(|| {
                ResourceError::new(
                    protocol::RESOURCE_NOT_FOUND,
                    format!("ресурс не найден: {uri}"),
                )
            })?;
        resource.reader.read(uri)
    }
}
