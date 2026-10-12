use mcp_core::handlers::{ToolError, ToolHandler};
use mcp_core::registry::{Registry, ToolDefinition};
use serde_json::{json, Value};
use std::sync::Arc;

struct DummyTool;

impl ToolHandler for DummyTool {
    fn name(&self) -> &str {
        "dummy"
    }
    fn description(&self) -> &str {
        "Тестовый инструмент"
    }
    fn input_schema(&self) -> Value {
        json!({"type": "object"})
    }
    fn call(&self, _params: Value) -> Result<Value, ToolError> {
        Ok(json!({"ok": true}))
    }
}

#[test]
fn register_and_call_tool() {
    let mut registry = Registry::new();
    registry.register_tool(ToolDefinition {
        name: "dummy".to_string(),
        description: "Тестовый инструмент".to_string(),
        input_schema: json!({"type": "object"}),
        handler: Arc::new(DummyTool),
    });
    let result = registry.call_tool("dummy", json!({})).expect("ok");
    assert_eq!(result["ok"], true);
}

#[test]
fn call_unknown_tool_returns_method_not_found() {
    let registry = Registry::new();
    let err = registry.call_tool("ghost", json!({})).expect_err("ошибка");
    assert_eq!(err.code, mcp_core::protocol::METHOD_NOT_FOUND);
}
