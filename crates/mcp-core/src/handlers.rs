//! Трейты обработчиков инструментов и ресурсов.
//!
//! Синхронные трейты: in-memory транспорт не требует async. `plugin_test`
//! исполняет Lua через CLI с таймаутом 15 с — блокирующий вызов приемлем
//! (GUI не блокируется надолго).

use serde_json::Value;

use crate::protocol::{INTERNAL_ERROR, INVALID_PARAMS};






#[derive(Clone, Debug)]
pub struct McpError {
    pub code: i64,
    pub message: String,
}

impl McpError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(INTERNAL_ERROR, message)
    }

    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(INVALID_PARAMS, message)
    }
}


pub type ToolError = McpError;


pub type ResourceError = McpError;


pub trait ToolHandler: Send + Sync {
    
    fn name(&self) -> &str;
    
    fn description(&self) -> &str;
    
    fn input_schema(&self) -> Value;
    
    
    
    
    
    
    
    
    
    
    fn call(&self, params: Value) -> Result<Value, ToolError>;
}





pub trait ResourceReader: Send + Sync {
    
    fn read(&self, uri: &str) -> Result<Value, ResourceError>;
}
