use mcp_core::handlers::{ToolError, ToolHandler};
use mcp_core::protocol::RpcRequest;
use mcp_core::registry::{Registry, ToolDefinition};
use mcp_core::server::McpServer;
use serde_json::{json, Value};
use std::sync::Arc;

struct EchoTool;

impl ToolHandler for EchoTool {
    fn name(&self) -> &str {
        "echo"
    }
    fn description(&self) -> &str {
        "Возвращает вход"
    }
    fn input_schema(&self) -> Value {
        json!({"type": "object"})
    }
    fn call(&self, params: Value) -> Result<Value, ToolError> {
        Ok(params)
    }
}

fn server() -> McpServer {
    let mut registry = Registry::new();
    registry.register_tool(ToolDefinition {
        name: "echo".to_string(),
        description: "Возвращает вход".to_string(),
        input_schema: json!({"type": "object"}),
        handler: Arc::new(EchoTool),
    });
    McpServer::new(registry)
}

#[test]
fn initialize_returns_protocol_version() {
    let server = server();
    let response = server.handle(RpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(1)),
        method: "initialize".into(),
        params: None,
    });
    let result = response.result.expect("result");
    assert_eq!(
        result["protocolVersion"],
        mcp_core::server::MCP_PROTOCOL_VERSION
    );
}

#[test]
fn tools_list_returns_registered_tools() {
    let server = server();
    let response = server.handle(RpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(2)),
        method: "tools/list".into(),
        params: None,
    });
    let result = response.result.expect("result");
    assert_eq!(result["tools"][0]["name"], "echo");
}

#[test]
fn tools_call_returns_result() {
    let server = server();
    let response = server.handle(RpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(3)),
        method: "tools/call".into(),
        params: Some(json!({"name": "echo", "arguments": {"text": "hi"}})),
    });
    let result = response.result.expect("result");
    assert_eq!(result["content"][0]["text"], "{\"text\":\"hi\"}");
}

#[test]
fn unknown_method_returns_error() {
    let server = server();
    let response = server.handle(RpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(4)),
        method: "ghost".into(),
        params: None,
    });
    let error = response.error.expect("error");
    assert_eq!(error.code, mcp_core::protocol::METHOD_NOT_FOUND);
}
