//! Трейты обработчиков инструментов и ресурсов.
//!
//! Синхронные трейты: in-memory транспорт не требует async. `plugin_test`
//! исполняет Lua через CLI с таймаутом 15 с — блокирующий вызов приемлем
//! (GUI не блокируется надолго, ADR-0034 §2.4).

use serde_json::Value;

use crate::protocol::{INTERNAL_ERROR, INVALID_PARAMS};

/// Ошибка вызова инструмента.
#[derive(Clone, Debug)]
pub struct ToolError {
    pub code: i64,
    pub message: String,
}

impl ToolError {
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

/// Обработчик инструмента (tool).
pub trait ToolHandler: Send + Sync {
    /// Имя инструмента (уникальное).
    fn name(&self) -> &str;
    /// Описание для агента.
    fn description(&self) -> &str;
    /// Схема входных параметров (JSON Schema object).
    fn input_schema(&self) -> Value;
    /// Вызов инструмента с параметрами.
    fn call(&self, params: Value) -> Result<Value, ToolError>;
}

/// Ошибка чтения ресурса.
#[derive(Clone, Debug)]
pub struct ResourceError {
    pub code: i64,
    pub message: String,
}

impl ResourceError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(INTERNAL_ERROR, message)
    }
}

/// Читатель ресурса.
///
/// `read` получает запрошенный URI: часть ресурсов шаблонные
/// (например, `darmark://plugins/{id}/manifest`), поэтому URI нужен читателю.
pub trait ResourceReader: Send + Sync {
    /// Чтение ресурса по запрошенному URI.
    fn read(&self, uri: &str) -> Result<Value, ResourceError>;
}
