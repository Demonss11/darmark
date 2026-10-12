//! JSON-RPC 2.0 envelope — базовый транспортный слой MCP.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Версия JSON-RPC протокола.
pub const RPC_VERSION: &str = "2.0";

/// Код ошибки: невалидный JSON.
pub const PARSE_ERROR: i64 = -32700;
/// Код ошибки: невалидный запрос.
pub const INVALID_REQUEST: i64 = -32600;
/// Код ошибки: метод не найден.
pub const METHOD_NOT_FOUND: i64 = -32601;
/// Код ошибки: невалидные параметры.
pub const INVALID_PARAMS: i64 = -32602;
/// Код ошибки: внутренняя ошибка.
pub const INTERNAL_ERROR: i64 = -32603;
/// Код ошибки: запрошенный ресурс не найден (серверный диапазон MCP).
pub const RESOURCE_NOT_FOUND: i64 = -32002;

/// Запрос JSON-RPC 2.0.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RpcRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}

/// Ошибка JSON-RPC 2.0.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl RpcError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }
}

/// Ответ JSON-RPC 2.0.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RpcResponse {
    pub jsonrpc: String,
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl RpcResponse {
    /// Успешный ответ.
    pub fn success(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: RPC_VERSION.to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// Ответ с ошибкой.
    pub fn error(id: Option<Value>, error: RpcError) -> Self {
        Self {
            jsonrpc: RPC_VERSION.to_string(),
            id,
            result: None,
            error: Some(error),
        }
    }
}
