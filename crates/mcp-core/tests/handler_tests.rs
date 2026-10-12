use mcp_core::handlers::{ResourceReader, ToolError, ToolHandler};
use serde_json::{json, Value};

struct EchoTool;

impl ToolHandler for EchoTool {
    fn name(&self) -> &str {
        "echo"
    }
    fn description(&self) -> &str {
        "Возвращает вход как есть"
    }
    fn input_schema(&self) -> Value {
        json!({"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]})
    }
    fn call(&self, params: Value) -> Result<Value, ToolError> {
        Ok(params)
    }
}

#[test]
fn tool_handler_returns_params() {
    let tool = EchoTool;
    let result = tool.call(json!({"text": "hi"})).expect("ok");
    assert_eq!(result["text"], "hi");
}

struct StaticResource;

impl ResourceReader for StaticResource {
    fn read(&self, _uri: &str) -> Result<Value, mcp_core::handlers::ResourceError> {
        Ok(json!({"ok": true}))
    }
}

#[test]
fn resource_reader_returns_value() {
    let reader = StaticResource;
    let result = reader.read("darmark://test").expect("ok");
    assert_eq!(result["ok"], true);
}
