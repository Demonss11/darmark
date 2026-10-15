//! Адаптеры MCP tools: тонкие обёртки над существующими командами/CLI.
//!
//! Бизнес-логики здесь нет (TZ-MCP R3/R4): белый список путей и валидация плагина живут
//! в `plugin_harness`, рендер — в `md-core`. Адаптер только маршрутизирует вызов, убирает
//! полные пути из ответа/журнала и пишет итог в журнал (R2) — без содержимого документа.
//! Журнал идёт через инъецированный [`LogSink`]: адаптеры не знают про Tauri.

use std::path::Path;
use std::sync::Arc;

use mcp_core::registry::{Registry, ToolDefinition};
use serde_json::{json, Value};

mod journal;
mod privacy;
mod tools;

#[cfg(test)]
mod tests;

use tools::{MarkdownRenderTool, PluginCatalogListTool, PluginTestTool, PluginValidateTool};

/// Сток журнала MCP: текст записи → приёмник (в продакшне — Tauri-событие `log-entry`).
///
/// Инъекция вместо `AppHandle` держит адаптеры независимыми от Tauri: их контракт
/// «схема ↔ поведение» (AC-MCP-6) и приватность журнала (AC-MCP-13) проверяются
/// интеграционным тестом без окна, event-loop и mock-рантайма.
pub type LogSink = Arc<dyn Fn(&str) + Send + Sync>;

/// Регистрирует все MCP tools в реестре.
///
/// `repo` — уже разрешённый каталог плагинов ([`crate::mcp::catalog_repo`]): обработчики
/// не читают CWD, а работают с запечённым абсолютным путём.
///
/// Инструменты плагинов — только Windows (целевая платформа; там же живёт белый список
/// путей `plugin_harness`). Без него `plugin_validate` читал бы произвольные пути.
pub fn register_tools(registry: &mut Registry, sink: &LogSink, repo: &Path) {
    registry.register_tool(ToolDefinition {
        name: "plugin_validate".to_string(),
        description: "Валидация манифеста и entry плагина без запуска песочницы".to_string(),
        input_schema: path_schema(),
        handler: Arc::new(PluginValidateTool {
            sink: Arc::clone(sink),
        }),
    });

    registry.register_tool(ToolDefinition {
        name: "plugin_test".to_string(),
        description: "Полная проверка плагина с прогоном в песочнице".to_string(),
        input_schema: path_schema(),
        handler: Arc::new(PluginTestTool {
            sink: Arc::clone(sink),
        }),
    });

    registry.register_tool(ToolDefinition {
        name: "plugin_catalog_list".to_string(),
        description: "Список плагинов из локального каталога (index.json)".to_string(),
        input_schema: json!({"type": "object", "properties": {}}),
        handler: Arc::new(PluginCatalogListTool {
            sink: Arc::clone(sink),
            repo: repo.to_path_buf(),
        }),
    });

    registry.register_tool(ToolDefinition {
        name: "markdown_render".to_string(),
        description: "Рендер Markdown в HTML (с опциональным маппингом блоков)".to_string(),
        input_schema: markdown_schema(),
        handler: Arc::new(MarkdownRenderTool {
            sink: Arc::clone(sink),
        }),
    });
}

/// Схема параметров tools, принимающих путь к каталогу плагина (Windows-only).
pub(super) fn path_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "path": {"type": "string", "description": "Путь к каталогу плагина"}
        },
        "required": ["path"]
    })
}

/// Схема параметров `markdown_render`.
pub(super) fn markdown_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "text": {"type": "string", "description": "Markdown-текст"},
            "mapped": {"type": "boolean", "description": "Включить data-md маппинг"}
        },
        "required": ["text"]
    })
}
