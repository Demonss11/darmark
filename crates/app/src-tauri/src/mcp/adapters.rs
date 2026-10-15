//! Адаптеры MCP tools: тонкие обёртки над существующими командами/CLI.
//!
//! Бизнес-логики здесь нет (TZ-MCP R3/R4): белый список путей и валидация плагина живут
//! в `plugin_harness`, рендер — в `md-core`. Адаптер только маршрутизирует вызов, убирает
//! полные пути из ответа/журнала и пишет итог в журнал (R2) — без содержимого документа.
//! Журнал идёт через инъецированный [`LogSink`]: адаптеры не знают про Tauri.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use mcp_core::handlers::{ToolError, ToolHandler};
use mcp_core::registry::{Registry, ToolDefinition};
use serde_json::{json, Value};

use crate::ai::types::AiValidationResult;

/// Префикс verbatim-пути Windows (`\\?\`), который возвращает `canonicalize`.
const VERBATIM_PREFIX: &str = r"\\?\";

/// Сток журнала MCP: текст записи → приёмник (в продакшне — Tauri-событие `log-entry`).
///
/// Инъекция вместо `AppHandle` держит адаптеры независимыми от Tauri: их контракт
/// «схема ↔ поведение» (AC-MCP-6) и приватность журнала (AC-MCP-13) проверяются
/// интеграционным тестом без окна, event-loop и mock-рантайма.
pub type LogSink = Arc<dyn Fn(&str) + Send + Sync>;

/// Регистрирует все MCP tools в реестре.
///
/// Инструменты плагинов — только Windows (целевая платформа; там же живёт белый список
/// путей `plugin_harness`). Без него `plugin_validate` читал бы произвольные пути.
pub fn register_tools(registry: &mut Registry, sink: &LogSink) {
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
    sink: LogSink,
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
        // не в адаптере. Возвращает канонический путь — с ним дальше и работаем.
        let canonical = crate::ai::plugin_harness::ensure_plugin_path_allowed(path)
            .map_err(|e| ToolError::invalid_params(e.message))?;

        let started = Instant::now();
        // Статическая валидация — общая с harness (R4/AC-MCP-6), адаптер только маршрутизирует.
        let mut result = crate::ai::plugin_harness::validate_plugin_static(&canonical);
        let duration = started.elapsed();

        // Приватность (R2/AC-MCP-13): из результата и журнала — без полного пути.
        sanitize_validation(&mut result, &canonical);
        log_mcp_call(
            &self.sink,
            &mcp_log_message(
                "plugin_validate",
                &path_name(&canonical),
                duration,
                result.ok,
                &result.errors,
            ),
        );

        serde_json::to_value(&result).map_err(|e| ToolError::internal(e.to_string()))
    }
}

// ─── plugin_test ───────────────────────────────────────────────────

struct PluginTestTool {
    sink: LogSink,
}

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
        let mut result = crate::ai::plugin_harness::validate_plugin(&canonical);
        let duration = started.elapsed();

        // Приватность (R2/AC-MCP-13): отчёт CLI и `errors` несут полный путь
        // (`…\plugin.json: …`) — убираем его из результата и журнала.
        sanitize_validation(&mut result, &canonical);
        log_mcp_call(
            &self.sink,
            &mcp_log_message(
                "plugin_test",
                &path_name(&canonical),
                duration,
                result.ok,
                &result.errors,
            ),
        );

        serde_json::to_value(&result).map_err(|e| ToolError::internal(e.to_string()))
    }
}

// ─── plugin_catalog_list ───────────────────────────────────────────

struct PluginCatalogListTool {
    sink: LogSink,
}

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
        let started = Instant::now();
        // Каталог читается через `plugin-catalog` (без `mlua`, ADR-0021) — как в CLI.
        let repo = super::catalog_repo();
        let catalog = match plugin_catalog::Catalog::load(&repo) {
            Ok(catalog) => catalog,
            Err(error) => {
                // Приватность (R2/AC-MCP-13): ошибки каталога содержат полный путь `index.json`.
                let message = sanitize_paths(&error, &repo);
                log_mcp_call(
                    &self.sink,
                    &mcp_log_message(
                        "plugin_catalog_list",
                        "-",
                        started.elapsed(),
                        false,
                        std::slice::from_ref(&message),
                    ),
                );
                return Err(ToolError::internal(message));
            }
        };
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
                })
            })
            .collect();

        // Журнал без пути (R2): только количество записей.
        log_mcp_call(
            &self.sink,
            &mcp_log_message(
                "plugin_catalog_list",
                &format!("{} плагинов", items.len()),
                started.elapsed(),
                true,
                &[],
            ),
        );
        Ok(json!({ "plugins": items }))
    }
}

// ─── markdown_render ───────────────────────────────────────────────

struct MarkdownRenderTool {
    sink: LogSink,
}

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

        let started = Instant::now();
        let html = if mapped {
            md_core::to_html_mapped(text)
        } else {
            md_core::to_html(text)
        };

        // Журнал без содержимого документа (R2): только объём входа.
        log_mcp_call(
            &self.sink,
            &mcp_log_message(
                "markdown_render",
                &format!("{} символов", text.chars().count()),
                started.elapsed(),
                true,
                &[],
            ),
        );
        Ok(json!({ "html": html }))
    }
}

// ─── Приватность путей ─────────────────────────────────────────────

/// Имя каталога (последний компонент пути) — приватность §4.6 (не полный путь).
fn path_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "-".to_string())
}

/// Варианты строкового представления каталога для замены (verbatim и «обычный»).
///
/// `canonicalize` на Windows отдаёт verbatim-путь (`\\?\C:\…`), но в отчётах/ошибках CLI
/// может встретиться и форма без префикса — чистим обе. Длинные варианты идут первыми:
/// иначе замена короткого оставила бы префикс `\\?\` в тексте.
fn dir_path_variants(dir: &Path) -> Vec<String> {
    let mut variants = vec![dir.to_string_lossy().into_owned()];
    if let Some(stripped) = variants[0].strip_prefix(VERBATIM_PREFIX) {
        variants.push(stripped.to_string());
    }
    variants.retain(|variant| !variant.is_empty());
    variants.sort_by_key(|variant| std::cmp::Reverse(variant.len()));
    variants
}

/// Заменяет все вхождения вариантов пути на `name`.
fn sanitize_text(text: &str, variants: &[String], name: &str) -> String {
    variants.iter().fold(text.to_string(), |acc, variant| {
        acc.replace(variant.as_str(), name)
    })
}

/// Убирает полный путь базового каталога из текста (приватность R2/AC-MCP-13).
fn sanitize_paths(text: &str, base: &Path) -> String {
    sanitize_text(text, &dir_path_variants(base), &path_name(base))
}

/// Убирает полный путь каталога из ошибок валидации.
fn sanitize_validation(result: &mut AiValidationResult, dir: &Path) {
    let variants = dir_path_variants(dir);
    let name = path_name(dir);
    for error in &mut result.errors {
        *error = sanitize_text(error, &variants, &name);
    }
    if let Some(report) = result.report.as_mut() {
        sanitize_value(report, &variants, &name);
    }
}

/// Рекурсивно чистит строки JSON-значения (отчёт CLI несёт путь в `path` и `checks[].message`).
fn sanitize_value(value: &mut Value, variants: &[String], name: &str) {
    match value {
        Value::String(text) => *text = sanitize_text(text, variants, name),
        Value::Array(items) => {
            for item in items.iter_mut() {
                sanitize_value(item, variants, name);
            }
        }
        Value::Object(map) => {
            for item in map.values_mut() {
                sanitize_value(item, variants, name);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

// ─── Журналирование ────────────────────────────────────────────────

/// Текст записи журнала для вызова MCP tool (TZ-MCP R2).
///
/// `subject` — имя каталога/файла либо иная краткая характеристика вызова
/// (количество плагинов, объём текста) — без полного пути и содержимого документа.
/// Чистая функция — тестируется без Tauri (AC-MCP-13: в записи нет полного пути).
fn mcp_log_message(
    tool: &str,
    subject: &str,
    duration: std::time::Duration,
    ok: bool,
    errors: &[String],
) -> String {
    if ok {
        format!("MCP {tool}: {subject} — ok ({} мс)", duration.as_millis())
    } else {
        format!(
            "MCP {tool}: {subject} — ошибка: {} ({} мс)",
            errors.join("; "),
            duration.as_millis()
        )
    }
}

/// Публикует запись журнала через инъецированный сток (best-effort, TZ-MCP R2).
///
/// Категория `system` и конверт события задаются стоком (`mcp::app_log_sink`): TS-тип
/// `LogCategory` (`plugin`/`document`/`system`) не трогаем.
fn log_mcp_call(sink: &LogSink, message: &str) {
    sink(message);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

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
    fn log_message_carries_counts_for_pathless_tools() {
        let catalog = mcp_log_message(
            "plugin_catalog_list",
            "4 плагинов",
            Duration::from_millis(1),
            true,
            &[],
        );
        assert!(catalog.contains("plugin_catalog_list"), "{catalog}");
        assert!(catalog.contains("4 плагинов"), "{catalog}");

        let render = mcp_log_message(
            "markdown_render",
            "120 символов",
            Duration::from_millis(1),
            true,
            &[],
        );
        assert!(render.contains("markdown_render"), "{render}");
        assert!(render.contains("120 символов"), "{render}");
    }

    #[test]
    fn path_schema_requires_path() {
        let schema = path_schema();
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["required"][0], "path");
    }

    /// Critical #1: ошибки CLI несут полный путь (`…\plugin.json: …`) — в журнал/результат
    /// он попадать не должен.
    #[test]
    fn sanitize_validation_strips_full_path_from_errors_and_report() {
        let dir = Path::new(r"C:\Users\Demon\AppData\Roaming\darmark\plugins\word-count");
        let mut result = AiValidationResult {
            ok: false,
            errors: vec![
                format!("manifest: {}\\plugin.json: нет id", dir.display()),
                format!("entry: {}\\main.lua: слишком большой", dir.display()),
            ],
            report: Some(json!({
                "path": dir.display().to_string(),
                "checks": [{
                    "name": "manifest",
                    "status": "fail",
                    "message": format!("{}: нет id", dir.join("plugin.json").display()),
                }],
            })),
        };

        sanitize_validation(&mut result, dir);

        let errors = result.errors.join("; ");
        assert!(
            errors.contains("word-count"),
            "имя каталога сохранено: {errors}"
        );
        assert!(!errors.contains(":\\"), "нет полного пути: {errors}");
        assert!(!errors.contains("AppData"), "нет полного пути: {errors}");

        let report = result.report.expect("отчёт сохранён");
        assert_eq!(report["path"], "word-count", "path → имя каталога");
        let rendered = report.to_string();
        assert!(
            !rendered.contains(":\\"),
            "в отчёте нет полного пути: {rendered}"
        );
        assert!(
            !rendered.contains("AppData"),
            "в отчёте нет полного пути: {rendered}"
        );
    }

    /// `canonicalize` отдаёт verbatim-путь (`\\?\…`); чистка обязана покрыть и его.
    #[test]
    fn sanitize_paths_handles_verbatim_prefix() {
        let dir = Path::new(r"\\?\C:\Users\Demon\AppData\Roaming\darmark\plugins\word-count");
        let text = format!("{}\\plugin.json: ошибка", dir.display());
        let sanitized = sanitize_paths(&text, dir);
        assert!(sanitized.contains("word-count"), "{sanitized}");
        assert!(!sanitized.contains(":\\"), "нет полного пути: {sanitized}");
        assert!(
            !sanitized.contains(r"\\?\"),
            "нет verbatim-префикса: {sanitized}"
        );
    }

    /// Ошибка каталога (`index.json`) тоже чистится перед журналом и результатом.
    #[test]
    fn sanitize_paths_replaces_repo_dir_with_name() {
        let repo = Path::new(r"C:\repo\examples\plugins");
        let text = format!("{}\\index.json: не разобран JSON", repo.display());
        let sanitized = sanitize_paths(&text, repo);
        assert!(sanitized.starts_with(r"plugins\index.json"), "{sanitized}");
        assert!(!sanitized.contains(":\\"), "нет полного пути: {sanitized}");
    }
}
