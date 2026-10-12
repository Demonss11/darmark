//! mcp-core — чистое ядро MCP-протокола без зависимостей от UI.
//!
//! JSON-RPC 2.0 envelope + семантика MCP (initialize, tools/list, tools/call,
//! resources/list, resources/read). Транспорт-агностично: сервер принимает
//! `RpcRequest` и отдаёт `RpcResponse`. Реестр tools/resources со схемами.

pub mod handlers;
pub mod protocol;
pub mod registry;
pub mod server;

pub use handlers::{ResourceError, ResourceReader, ToolError, ToolHandler};
pub use registry::{Registry, ResourceDefinition, ToolDefinition};
