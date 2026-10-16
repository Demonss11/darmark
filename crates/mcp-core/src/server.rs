//! MCP-сервер: семантика протокола поверх реестра.

use serde_json::{json, Value};

use crate::protocol::{self, RpcRequest, RpcResponse};
use crate::registry::CallToolError;

/// Версия MCP-протокола, поддерживаемая сервером.
pub const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

/// MCP-сервер: маршрутизация JSON-RPC запросов к реестру.
pub struct McpServer {
    registry: crate::registry::Registry,
}

impl McpServer {
    pub fn new(registry: crate::registry::Registry) -> Self {
        Self { registry }
    }

    /// Обработка JSON-RPC запроса.
    ///
    /// `None` — валидный notification (запрос без `id`): по JSON-RPC 2.0 ответ на него
    /// не отправляется. Невалидный `jsonrpc` получает ошибку с `id` из запроса (или `null`),
    /// поскольку обнаружить в нём notification нельзя.
    pub fn handle(&self, request: RpcRequest) -> Option<RpcResponse> {
        if request.jsonrpc != protocol::RPC_VERSION {
            return Some(RpcResponse::error(
                request.id.clone(),
                protocol::RpcError::new(
                    protocol::INVALID_REQUEST,
                    format!("jsonrpc must \"{}\"", protocol::RPC_VERSION),
                ),
            ));
        }
        // Notification — запрос без `id`: ответ не отправляется.
        request.id.as_ref()?;
        let id = request.id.clone();
        Some(match request.method.as_str() {
            "initialize" => RpcResponse::success(id, self.initialize()),
            "tools/list" => RpcResponse::success(id, self.registry.tools_list()),
            "tools/call" => self.handle_tool_call(id, request.params),
            "resources/list" => RpcResponse::success(id, self.registry.resources_list()),
            "resources/read" => self.handle_resource_read(id, request.params),
            other => RpcResponse::error(
                id,
                protocol::RpcError::new(
                    protocol::METHOD_NOT_FOUND,
                    format!("метод не найден: {other}"),
                ),
            ),
        })
    }

    fn initialize(&self) -> Value {
        json!({
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities": {
                "tools": {},
                "resources": {},
            },
            "serverInfo": {
                "name": "darmark",
                "version": env!("CARGO_PKG_VERSION"),
            },
        })
    }

    /// Вызов инструмента и сборка конверта `tools/call`.
    ///
    /// Конверт собирается из двух независимых источников: `content` — всегда текст payload
    /// обработчика, `isError` — либо из [`CallToolError::Execution`], либо из одноимённого
    /// булева поля payload, если обработчик его объявил (ADR-0041 §2.2). Payload обработчика
    /// при этом не переписывается: сервер копирует признак, а не забирает его из данных.
    fn handle_tool_call(&self, id: Option<Value>, params: Option<Value>) -> RpcResponse {
        let Some(params) = params else {
            return RpcResponse::error(
                id,
                protocol::RpcError::new(protocol::INVALID_PARAMS, "tools/call требует params"),
            );
        };
        let Some(name) = params.get("name").and_then(Value::as_str) else {
            return RpcResponse::error(
                id,
                protocol::RpcError::new(protocol::INVALID_PARAMS, "tools/call требует params.name"),
            );
        };
        // `"arguments": null` и отсутствие `arguments` эквивалентны пустому объекту:
        // обработчики получают JSON-объект, а не `Value::Null`.
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let arguments = if arguments.is_null() {
            json!({})
        } else {
            arguments
        };
        match self.registry.call_tool(name, arguments) {
            // Признак вердикта вызова копируется из payload обработчика: инструмент-верификатор
            // возвращает `isError` рядом со структурированными данными (`ok`, `errors`, `report`),
            // иначе журнал и карточка показывали бы противоположное (ADR-0041 §2.2 п.5).
            // Payload не мутируется: поле остаётся и в `content[0].text` (п.6), а без него
            // конверт собирается ровно как прежде — побайтово (п.8).
            Ok(result) => {
                let mut envelope =
                    json!({ "content": [{ "type": "text", "text": result.to_string() }] });
                if let Some(is_error) = result.get("isError").and_then(Value::as_bool) {
                    envelope["isError"] = json!(is_error);
                }
                RpcResponse::success(id, envelope)
            }
            // Неизвестный инструмент — ошибка протокола, а не результат вызова.
            Err(CallToolError::UnknownTool(name)) => RpcResponse::error(
                id,
                protocol::RpcError::new(protocol::INVALID_PARAMS, format!("Unknown tool: {name}")),
            ),
            // Ошибка исполнения инструмента — результат с `isError`.
            Err(CallToolError::Execution(error)) => RpcResponse::success(
                id,
                json!({ "isError": true, "content": [{ "type": "text", "text": error.message }] }),
            ),
        }
    }

    fn handle_resource_read(&self, id: Option<Value>, params: Option<Value>) -> RpcResponse {
        let Some(params) = params else {
            return RpcResponse::error(
                id,
                protocol::RpcError::new(protocol::INVALID_PARAMS, "resources/read требует params"),
            );
        };
        let Some(uri) = params.get("uri").and_then(Value::as_str) else {
            return RpcResponse::error(
                id,
                protocol::RpcError::new(
                    protocol::INVALID_PARAMS,
                    "resources/read требует params.uri",
                ),
            );
        };
        match self.registry.read_resource(uri) {
            Ok(result) => RpcResponse::success(
                id,
                json!({ "contents": [{ "uri": uri, "text": result.to_string() }] }),
            ),
            Err(error) => {
                RpcResponse::error(id, protocol::RpcError::new(error.code, error.message))
            }
        }
    }
}
