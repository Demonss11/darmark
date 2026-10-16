use mcp_core::handlers::{ResourceError, ResourceReader, ToolError, ToolHandler};
use mcp_core::protocol::{
    RpcRequest, RpcResponse, INVALID_PARAMS, INVALID_REQUEST, RESOURCE_NOT_FOUND,
};
use mcp_core::registry::{Registry, ResourceDefinition, ToolDefinition};
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

/// Инструмент, чей обработчик падает — проверяем `isError`-результат.
struct FailingTool;

impl ToolHandler for FailingTool {
    fn name(&self) -> &str {
        "failing"
    }
    fn description(&self) -> &str {
        "Всегда возвращает ошибку исполнения"
    }
    fn input_schema(&self) -> Value {
        json!({"type": "object"})
    }
    fn call(&self, _params: Value) -> Result<Value, ToolError> {
        Err(ToolError::internal("boom"))
    }
}

/// Инструмент-верификатор: обработчик возвращает `Ok` с отрицательным вердиктом, объявленным
/// полем `isError` в payload (ADR-0041 §2.2 п.5).
struct RejectingTool;

impl ToolHandler for RejectingTool {
    fn name(&self) -> &str {
        "rejecting"
    }
    fn description(&self) -> &str {
        "Проверка не прошла: вердикт в payload"
    }
    fn input_schema(&self) -> Value {
        json!({"type": "object"})
    }
    fn call(&self, _params: Value) -> Result<Value, ToolError> {
        Ok(json!({"ok": false, "isError": true, "errors": ["поле id пустое"]}))
    }
}

struct StaticResource;

impl ResourceReader for StaticResource {
    fn read(&self, _uri: &str) -> Result<Value, ResourceError> {
        Ok(json!({"ok": true}))
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
    registry.register_tool(ToolDefinition {
        name: "failing".to_string(),
        description: "Всегда возвращает ошибку исполнения".to_string(),
        input_schema: json!({"type": "object"}),
        handler: Arc::new(FailingTool),
    });
    registry.register_tool(ToolDefinition {
        name: "rejecting".to_string(),
        description: "Проверка не прошла: вердикт в payload".to_string(),
        input_schema: json!({"type": "object"}),
        handler: Arc::new(RejectingTool),
    });
    registry.register_resource(ResourceDefinition {
        uri: "darmark://test".to_string(),
        name: "test".to_string(),
        description: "Тестовый ресурс".to_string(),
        reader: Arc::new(StaticResource),
    });
    McpServer::new(registry)
}

fn request(id: i64, method: &str, params: Option<Value>) -> RpcRequest {
    RpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(id)),
        method: method.into(),
        params,
    }
}

/// Запрос без `id` — JSON-RPC notification.
fn notification(method: &str, params: Option<Value>) -> RpcRequest {
    RpcRequest {
        jsonrpc: "2.0".into(),
        id: None,
        method: method.into(),
        params,
    }
}

/// Ответ на запрос с `id`: сервер обязан ответить, `None` здесь — ошибка.
fn respond(server: &McpServer, request: RpcRequest) -> RpcResponse {
    server
        .handle(request)
        .expect("запрос с id обязан получить ответ")
}

#[test]
fn initialize_returns_protocol_version() {
    let server = server();
    let response = respond(&server, request(1, "initialize", None));
    let result = response.result.expect("result");
    assert_eq!(
        result["protocolVersion"],
        mcp_core::server::MCP_PROTOCOL_VERSION
    );
}

#[test]
fn initialize_returns_server_info() {
    let server = server();
    let response = respond(&server, request(1, "initialize", None));
    let result = response.result.expect("result");
    assert_eq!(result["serverInfo"]["name"], "darmark");
    assert_eq!(result["serverInfo"]["version"], env!("CARGO_PKG_VERSION"));
}

#[test]
fn tools_list_returns_registered_tools() {
    let server = server();
    let response = respond(&server, request(2, "tools/list", None));
    let result = response.result.expect("result");
    assert_eq!(result["tools"][0]["name"], "echo");
}

#[test]
fn tools_call_returns_result() {
    let server = server();
    let response = respond(
        &server,
        request(
            3,
            "tools/call",
            Some(json!({"name": "echo", "arguments": {"text": "hi"}})),
        ),
    );
    let result = response.result.expect("result");
    assert_eq!(result["content"][0]["text"], "{\"text\":\"hi\"}");
}

#[test]
fn tools_call_unknown_tool_returns_invalid_params() {
    let server = server();
    let response = respond(
        &server,
        request(4, "tools/call", Some(json!({"name": "ghost"}))),
    );
    let error = response.error.expect("error");
    assert_eq!(error.code, INVALID_PARAMS);
    assert_eq!(error.message, "Unknown tool: ghost");
}

#[test]
fn tools_call_execution_error_returns_is_error_result() {
    let server = server();
    let response = respond(
        &server,
        request(
            5,
            "tools/call",
            Some(json!({"name": "failing", "arguments": {}})),
        ),
    );
    assert!(response.error.is_none());
    let result = response.result.expect("result");
    assert_eq!(result["isError"], true);
    assert_eq!(result["content"][0]["type"], "text");
    assert_eq!(result["content"][0]["text"], "boom");
}

/// ADR-0041 §2.2 п.5: `isError` из payload обработчика копируется в конверт.
#[test]
fn tools_call_copies_handler_is_error_into_envelope() {
    let server = server();
    let response = respond(
        &server,
        request(
            16,
            "tools/call",
            Some(json!({"name": "rejecting", "arguments": {}})),
        ),
    );
    assert!(response.error.is_none());
    let result = response.result.expect("result");
    assert_eq!(result["isError"], true);
    // Payload не переписывается (п.6): структурированный вердикт остаётся в тексте.
    let text = result["content"][0]["text"].as_str().expect("text");
    let payload: Value = serde_json::from_str(text).expect("payload — JSON");
    assert_eq!(payload["ok"], false);
    assert_eq!(payload["isError"], true);
    assert_eq!(payload["errors"], json!(["поле id пустое"]));
}

/// ADR-0041 §2.2 п.8: без поля `isError` в payload конверт побайтово прежний — признака нет.
#[test]
fn tools_call_without_handler_is_error_has_no_is_error() {
    let server = server();
    let response = respond(
        &server,
        request(
            17,
            "tools/call",
            Some(json!({"name": "echo", "arguments": {"text": "hi"}})),
        ),
    );
    assert!(response.error.is_none());
    let result = response.result.expect("result");
    assert!(
        result.get("isError").is_none(),
        "конверт без isError: {result}"
    );
    assert_eq!(result["content"][0]["text"], "{\"text\":\"hi\"}");
}

#[test]
fn tools_call_without_params_returns_invalid_params() {
    let server = server();
    let response = respond(&server, request(6, "tools/call", None));
    let error = response.error.expect("error");
    assert_eq!(error.code, INVALID_PARAMS);
}

#[test]
fn tools_call_without_name_returns_invalid_params() {
    let server = server();
    let response = respond(&server, request(7, "tools/call", Some(json!({}))));
    let error = response.error.expect("error");
    assert_eq!(error.code, INVALID_PARAMS);
}

#[test]
fn resources_list_returns_registered_resources() {
    let server = server();
    let response = respond(&server, request(8, "resources/list", None));
    let result = response.result.expect("result");
    let resources = result["resources"].as_array().expect("массив resources");
    assert_eq!(resources.len(), 1);
    assert_eq!(resources[0]["uri"], "darmark://test");
    assert_eq!(resources[0]["name"], "test");
    assert_eq!(resources[0]["description"], "Тестовый ресурс");
}

#[test]
fn resources_read_returns_contents() {
    let server = server();
    let response = respond(
        &server,
        request(9, "resources/read", Some(json!({"uri": "darmark://test"}))),
    );
    let result = response.result.expect("result");
    assert_eq!(result["contents"][0]["uri"], "darmark://test");
    assert_eq!(result["contents"][0]["text"], "{\"ok\":true}");
}

#[test]
fn resources_read_unknown_uri_returns_resource_not_found() {
    let server = server();
    let response = respond(
        &server,
        request(
            10,
            "resources/read",
            Some(json!({"uri": "darmark://ghost"})),
        ),
    );
    let error = response.error.expect("error");
    assert_eq!(error.code, RESOURCE_NOT_FOUND);
}

#[test]
fn resources_read_without_params_returns_invalid_params() {
    let server = server();
    let response = respond(&server, request(11, "resources/read", None));
    let error = response.error.expect("error");
    assert_eq!(error.code, INVALID_PARAMS);
}

#[test]
fn resources_read_without_uri_returns_invalid_params() {
    let server = server();
    let response = respond(&server, request(12, "resources/read", Some(json!({}))));
    let error = response.error.expect("error");
    assert_eq!(error.code, INVALID_PARAMS);
}

#[test]
fn unknown_method_returns_error() {
    let server = server();
    let response = respond(&server, request(13, "ghost", None));
    let error = response.error.expect("error");
    assert_eq!(error.code, mcp_core::protocol::METHOD_NOT_FOUND);
}

#[test]
fn rpc_invalid_jsonrpc_version_rejected() {
    let server = server();
    let mut req = request(14, "initialize", None);
    req.jsonrpc = "1.0".into();
    let response = respond(&server, req);
    let error = response.error.expect("невалидный jsonrpc — ошибка");
    assert_eq!(error.code, INVALID_REQUEST);
    assert_eq!(error.message, "jsonrpc must \"2.0\"");
    assert!(response.result.is_none());
}

#[test]
fn rpc_notification_does_not_receive_response() {
    let server = server();
    assert!(server.handle(notification("initialize", None)).is_none());
    assert!(server
        .handle(notification("tools/call", Some(json!({"name": "echo"}))))
        .is_none());
}

#[test]
fn rpc_arguments_null_normalized_to_empty_object() {
    let server = server();
    let response = respond(
        &server,
        request(
            15,
            "tools/call",
            Some(json!({"name": "echo", "arguments": null})),
        ),
    );
    let result = response.result.expect("result");
    assert_eq!(result["content"][0]["text"], "{}");
}
