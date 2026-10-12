use mcp_core::protocol::{RpcError, RpcResponse, METHOD_NOT_FOUND};

#[test]
fn success_response_serializes_with_jsonrpc_version() {
    let response =
        RpcResponse::success(Some(serde_json::json!(1)), serde_json::json!({"ok": true}));
    let json = serde_json::to_value(&response).expect("сериализуется");
    assert_eq!(json["jsonrpc"], "2.0");
    assert_eq!(json["id"], 1);
    assert_eq!(json["result"]["ok"], true);
    assert!(json.get("error").is_none());
}

#[test]
fn error_response_carries_code_and_message() {
    let response = RpcResponse::error(
        Some(serde_json::json!(2)),
        RpcError::new(METHOD_NOT_FOUND, "метод не найден: foo"),
    );
    let json = serde_json::to_value(&response).expect("сериализуется");
    assert_eq!(json["error"]["code"], METHOD_NOT_FOUND);
    assert_eq!(json["error"]["message"], "метод не найден: foo");
    assert!(json.get("result").is_none());
}
