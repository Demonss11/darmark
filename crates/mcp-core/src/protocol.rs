//! JSON-RPC 2.0 envelope — базовый транспортный слой MCP.

use serde::{Deserialize, Serialize};
use serde_json::Value;


pub const RPC_VERSION: &str = "2.0";


pub const PARSE_ERROR: i64 = -32700;

pub const INVALID_REQUEST: i64 = -32600;

pub const METHOD_NOT_FOUND: i64 = -32601;

pub const INVALID_PARAMS: i64 = -32602;

pub const INTERNAL_ERROR: i64 = -32603;

pub const RESOURCE_NOT_FOUND: i64 = -32002;


#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RpcRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}


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
    
    pub fn success(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: RPC_VERSION.to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    
    pub fn error(id: Option<Value>, error: RpcError) -> Self {
        Self {
            jsonrpc: RPC_VERSION.to_string(),
            id,
            result: None,
            error: Some(error),
        }
    }
}
