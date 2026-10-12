//! Адаптеры MCP tools: тонкие обёртки над существующими командами/CLI.
//!
//! Бизнес-логики здесь нет (TZ-MCP R3/R4): белый список путей и валидация плагина живут
//! в `plugin_harness`, рендер — в `md-core`. Адаптер только маршрутизирует вызов и пишет
//! итог в журнал (R2) — без полного пути и содержимого документа.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use mcp_core::handlers::{ToolError, ToolHandler};
use mcp_core::registry::{Registry, ToolDefinition};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use crate::ai::types::AiValidationResult;
use crate::error::{CommandError, ErrorCode};

/// Регистрирует все MCP tools в реестре.
pub fn register_tools(registry: &mut Registry, app: &AppHandle) {
    registry.register_tool(ToolDefinition {
        name: "plugin_validate".to_string(),
        description: "Валидация манифеста и entry плагина без запуска песочницы".to_string(),
        input_schema: path_schema(),
        handler: Arc::new(PluginValidateTool { app: app.clone() }),
    });

    #[cfg(windows)]
    registry.register_tool(ToolDefinition {
        name: "plugin_test".to_string(),
        description: "Полная проверка плагина с прогоном в песочнице".to_string(),
        input_schema: path_schema(),
        handler: Arc::new(PluginTestTool { app: app.clone() }),
    });

    registry.register_tool(ToolDefinition {
        name: "plugin_catalog_list".to_string(),
        description: "Список плагинов из локального каталога (index.json)".to_string(),
        input_schema: json!({"type": "object", "properties": {}}),
        handler: Arc::new(PluginCatalogListTool),
    });

    registry.register_tool(ToolDefinition {
        name: "markdown_render".to_string(),
        description: "Рендер Markdown в HTML (с опциональным маппингом блоков)".to_string(),
        input_schema: markdown_schema(),
        handler: Arc::new(MarkdownRenderTool),
    });
}

/// Схема параметров tools, принимающих путь к каталогу плагина.
fn path_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "path": {"type": "string", "description": "Путь к каталогу плагина"}
        },
        "required": ["path"]
    })
}

/// Схема параметров `markdown_render`.
fn markdown_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "text": {"type": "string", "description": "Markdown-текст"},
            "mapped": {"type": "boolean", "description": "Включить data-md маппинг"}
        },
        "required": ["text"]
    })
}

// ─── plugin_validate ───────────────────────────────────────────────

struct PluginValidateTool {
    app: AppHandle,
}

impl ToolHandler for PluginValidateTool {
    fn name(&self) -> &str {
        "plugin_validate"
    }

    fn description(&self) -> &str {
        "Валидация манифеста и entry плагина без запуска песочницы"
    }

    fn input_schema(&self) -> Value {
        path_schema()
    }

    fn call(&self, params: Value) -> Result<Value, ToolError> {
        let Some(path_str) = params.get("path").and_then(Value::as_str) else {
            return Err(ToolError::invalid_params("требуется path"));
        };
        let path = Path::new(path_str);

        // Белый список директорий (TZ-MCP R3): принуждение живёт в `plugin_harness`,
        // не в адаптере. На не-Windows проверки нет — валидация идёт без неё.
        #[cfg(windows)]
        let canonical = crate::ai::plugin_harness::ensure_plugin_path_allowed(path)
            .map_err(|e| ToolError::invalid_params(e.message))?;
        #[cfg(not(windows))]
        let canonical = path.to_path_buf();

        let started = Instant::now();
        let result = validate_static(&canonical);
        let duration = started.elapsed();

        // Журналирование (TZ-MCP R2): только имя каталога, не полный путь.
        let (ok, errors) = match &result {
            Ok(validation) => (validation.ok, validation.errors.clone()),
            Err(error) => (false, vec![error.message.clone()]),
        };
        log_mcp_call(
            &self.app,
            &mcp_log_message("plugin_validate", &path_name(path), duration, ok, &errors),
        );

        match result {
            Ok(validation) => {
                serde_json::to_value(&validation).map_err(|e| ToolError::internal(e.to_string()))
            }
            Err(error) => Err(ToolError::invalid_params(error.message)),
        }
    }
}

/// Статическая валидация: манифест + entry без песочницы.
fn validate_static(dir: &Path) -> Result<AiValidationResult, CommandError> {
    let manifest_bytes = std::fs::read(dir.join("plugin.json"))
        .map_err(|e| CommandError::new(ErrorCode::NotFound, format!("plugin.json: {e}")))?;
    let manifest: Value = serde_json::from_slice(&manifest_bytes).map_err(|e| {
        CommandError::new(ErrorCode::Plugin, format!("plugin.json не парсится: {e}"))
    })?;
    let source = std::fs::read_to_string(dir.join("main.lua"))
        .map_err(|e| CommandError::new(ErrorCode::NotFound, format!("main.lua: {e}")))?;
    let errors = crate::ai::plugin_harness::manifest_errors(&manifest, &source);
    Ok(AiValidationResult {
        ok: errors.is_empty(),
        errors,
        report: None,
    })
}

// ─── plugin_test ───────────────────────────────────────────────────

#[cfg(windows)]
struct PluginTestTool {
    app: AppHandle,
}

#[cfg(windows)]
impl ToolHandler for PluginTestTool {
    fn name(&self) -> &str {
        "plugin_test"
    }

    fn description(&self) -> &str {
        "Полная проверка плагина с прогоном в песочнице"
    }

    fn input_schema(&self) -> Value {
        path_schema()
    }

    fn call(&self, params: Value) -> Result<Value, ToolError> {
        let Some(path_str) = params.get("path").and_then(Value::as_str) else {
            return Err(ToolError::invalid_params("требуется path"));
        };
        let path = Path::new(path_str);

        // Белый список директорий (TZ-MCP R3) — до прогона Lua в песочнице.
        let canonical = crate::ai::plugin_harness::ensure_plugin_path_allowed(path)
            .map_err(|e| ToolError::invalid_params(e.message))?;

        let started = Instant::now();
        let result = crate::ai::plugin_harness::validate_plugin(&canonical);
        let duration = started.elapsed();

        // Журналирование (TZ-MCP R2): только имя каталога, не полный путь.
        log_mcp_call(
            &self.app,
            &mcp_log_message(
                "plugin_test",
                &path_name(path),
                duration,
                result.ok,
                &result.errors,
            ),
        );

        serde_json::to_value(&result).map_err(|e| ToolError::internal(e.to_string()))
    }
}

// ─── plugin_catalog_list ───────────────────────────────────────────

struct PluginCatalogListTool;

impl ToolHandler for PluginCatalogListTool {
    fn name(&self) -> &str {
        "plugin_catalog_list"
    }

    fn description(&self) -> &str {
        "Список плагинов из локального каталога (index.json)"
    }

    fn input_schema(&self) -> Value {
        json!({"type": "object", "properties": {}})
    }

    fn call(&self, _params: Value) -> Result<Value, ToolError> {
        // Каталог читается через `plugin-catalog` (без `mlua`, ADR-0021) — как в CLI.
        let catalog =
            plugin_catalog::Catalog::load(&super::catalog_repo()).map_err(ToolError::internal)?;
        let items: Vec<Value> = catalog
            .list()
            .iter()
            .map(|entry| {
                json!({
                    "id": entry.id,
                    "name": entry.name,
                    "version": entry.version,
                    "api_version": entry.api_version,
                    "permissions": entry.permissions,
                    "description": entry.description,
                    "path": entry.path,
                })
            })
            .collect();
        Ok(json!({ "plugins": items }))
    }
}

// ─── markdown_render ───────────────────────────────────────────────

struct MarkdownRenderTool;

impl ToolHandler for MarkdownRenderTool {
    fn name(&self) -> &str {
        "markdown_render"
    }

    fn description(&self) -> &str {
        "Рендер Markdown в HTML (с опциональным маппингом блоков)"
    }

    fn input_schema(&self) -> Value {
        markdown_schema()
    }

    fn call(&self, params: Value) -> Result<Value, ToolError> {
        let Some(text) = params.get("text").and_then(Value::as_str) else {
            return Err(ToolError::invalid_params("требуется text"));
        };
        let mapped = params
            .get("mapped")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let html = if mapped {
            md_core::to_html_mapped(text)
        } else {
            md_core::to_html(text)
        };
        Ok(json!({ "html": html }))
    }
}

// ─── Журналирование ────────────────────────────────────────────────

/// Имя каталога (последний компонент пути) — приватность §4.6 (не полный путь).
fn path_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "-".to_string())
}

/// Текст записи журнала для вызова MCP tool (TZ-MCP R2).
///
/// Чистая функция — тестируется без Tauri (AC-MCP-13: в записи нет полного пути).
fn mcp_log_message(
    tool: &str,
    path_name: &str,
    duration: std::time::Duration,
    ok: bool,
    errors: &[String],
) -> String {
    if ok {
        format!("MCP {tool}: {path_name} — ok ({} мс)", duration.as_millis())
    } else {
        format!(
            "MCP {tool}: {path_name} — ошибка: {} ({} мс)",
            errors.join("; "),
            duration.as_millis()
        )
    }
}

/// Публикует запись журнала (best-effort, TZ-MCP R2).
///
/// Категория `system`: TS-тип `LogCategory` (`plugin`/`document`/`system`) не трогаем.
fn log_mcp_call(app: &AppHandle, message: &str) {
    if let Err(e) = app.emit(
        "log-entry",
        json!({ "level": "info", "category": "system", "message": message }),
    ) {
        eprintln!("не эмитить log-entry: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[cfg(windows)]
    #[test]
    fn path_name_returns_last_component() {
        let path = Path::new(r"C:\Users\Demon\AppData\Roaming\darmark\plugins\word-count");
        assert_eq!(path_name(path), "word-count");
    }

    #[test]
    fn path_name_falls_back_without_component() {
        assert_eq!(path_name(Path::new("")), "-");
    }

    #[test]
    fn log_message_has_no_full_path_and_reports_ok() {
        let message = mcp_log_message(
            "plugin_test",
            "word-count",
            Duration::from_millis(12),
            true,
            &[],
        );
        assert!(message.contains("word-count"), "msg: {message}");
        assert!(!message.contains(":\\"), "нет полного пути: {message}");
        assert!(message.contains("ok"), "msg: {message}");
    }

    #[test]
    fn log_message_lists_errors_without_full_path() {
        let errors = vec!["manifest: missing id".to_string()];
        let message = mcp_log_message(
            "plugin_validate",
            "broken",
            Duration::from_millis(3),
            false,
            &errors,
        );
        assert!(message.contains("broken"), "msg: {message}");
        assert!(message.contains("missing id"), "msg: {message}");
        assert!(!message.contains(":\\"), "нет полного пути: {message}");
    }

    #[test]
    fn path_schema_requires_path() {
        let schema = path_schema();
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["required"][0], "path");
    }
}
