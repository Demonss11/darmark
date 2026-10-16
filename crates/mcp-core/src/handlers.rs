//! Трейты обработчиков инструментов и ресурсов.
//!
//! Синхронные трейты: in-memory транспорт не требует async. `plugin_test`
//! исполняет Lua через CLI с таймаутом 15 с — блокирующий вызов приемлем
//! (GUI не блокируется надолго, ADR-0034 §2.4).

use serde_json::Value;

use crate::protocol::{INTERNAL_ERROR, INVALID_PARAMS};

/// Ошибка обработчика MCP (инструмент или ресурс): код JSON-RPC и сообщение.
///
/// Единая структура вместо двух идентичных: контекст (tool/resource) на результат не влияет —
/// и [`ToolHandler`], и [`ResourceReader`] несут только `code`/`message`. Имена [`ToolError`]
/// и [`ResourceError`] сохранены алиасами ради читаемости сигнатур и совместимости вызовов.
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

/// Ошибка вызова инструмента ([`ToolHandler`]).
pub type ToolError = McpError;

/// Ошибка чтения ресурса ([`ResourceReader`]).
pub type ResourceError = McpError;

/// Обработчик инструмента (tool).
pub trait ToolHandler: Send + Sync {
    /// Имя инструмента (уникальное).
    fn name(&self) -> &str;
    /// Описание для агента.
    fn description(&self) -> &str;
    /// Схема входных параметров (JSON Schema object).
    fn input_schema(&self) -> Value;
    /// Вызов инструмента с параметрами.
    ///
    /// **Конвенция вердикта (ADR-0041 §2.2):** инструмент, результат которого сам является
    /// вердиктом («проверка прошла / не прошла»), объявляет его булевым полем `isError` в
    /// возвращаемом payload — транспорт [`crate::server::McpServer`] копирует это поле в
    /// конверт `tools/call`, и журнал с карточкой вызывают один и тот же вердикт. Payload при
    /// этом не переписывается: поле остаётся и в данных.
    ///
    /// Инструмент **без** такой семантики («успех» = «выполнилось») поля `isError` не
    /// возвращает — конверт остаётся ровно таким, каким был.
    fn call(&self, params: Value) -> Result<Value, ToolError>;
}

/// Читатель ресурса.
///
/// `read` получает запрошенный URI: часть ресурсов шаблонные
/// (например, `darmark://plugins/{id}/manifest`), поэтому URI нужен читателю.
pub trait ResourceReader: Send + Sync {
    /// Чтение ресурса по запрошенному URI.
    fn read(&self, uri: &str) -> Result<Value, ResourceError>;
}
