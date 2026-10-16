use std::path::{Path, PathBuf};
use std::time::Instant;

use mcp_core::handlers::{ToolError, ToolHandler};
use serde_json::{json, Value};

use super::journal::{log_mcp_call, mcp_log_message};
use super::privacy::{path_name, sanitize_paths, sanitize_validation};
use super::{id_schema, markdown_schema, path_schema, LogSink};

pub(super) struct PluginValidateTool {
    pub(super) sink: LogSink,
}

/// Сериализует результат верификатора и объявляет его вердикт вызову.
///
/// Вердикт — единственный источник для обоих наблюдателей: журнал пишется из `result.ok`,
/// конверт `tools/call` получает `isError: !ok` отсюда же (ADR-0041 §2.4 п.13). Вызывается
/// строго **после** `sanitize_validation`: добавление булева поля не может ослабить приватность
/// payload (AC-MCP-13), а `AiValidationResult` не получает транспортное поле — тип обслуживает
/// ещё и команду `ai_validate_plugin` (ADR-0041 §2.3 п.10–11).
fn verification_payload(result: &crate::ai::types::AiValidationResult) -> Result<Value, ToolError> {
    let payload = serde_json::to_value(result).map_err(|e| ToolError::internal(e.to_string()))?;
    let Value::Object(mut map) = payload else {
        return Err(ToolError::internal("результат валидации — не JSON-объект"));
    };
    map.insert("isError".to_string(), json!(!result.ok));
    Ok(Value::Object(map))
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
        let canonical = crate::ai::plugin_harness::ensure_plugin_path_allowed(path)
            .map_err(|e| ToolError::invalid_params(e.message))?;

        let started = Instant::now();
        let mut result = crate::ai::plugin_harness::validate_plugin_static(&canonical);
        let duration = started.elapsed();
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

        verification_payload(&result)
    }
}

pub(super) struct PluginTestTool {
    pub(super) sink: LogSink,
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
        let canonical = crate::ai::plugin_harness::ensure_plugin_path_allowed(path)
            .map_err(|e| ToolError::invalid_params(e.message))?;

        let started = Instant::now();
        let mut result = crate::ai::plugin_harness::validate_plugin(&canonical);
        let duration = started.elapsed();
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

        verification_payload(&result)
    }
}

/// Имя манифеста в каталоге установленного плагина (контракт `plugin_read`).
const READ_MANIFEST_FILE: &str = "plugin.json";

/// Имя entry-файла в каталоге установленного плагина (контракт `plugin_read`).
const READ_ENTRY_FILE: &str = "main.lua";

/// Инструмент чтения исходника **установленного** плагина по `id`.
///
/// Принимает только `id`: путь собирается здесь из доверенного [`plugins_root`](crate::ai::plugin_harness::plugins_root)
/// и проходит тот же белый список [`ensure_plugin_path_allowed`](crate::ai::plugin_harness::ensure_plugin_path_allowed),
/// что `plugin_validate`/`plugin_test` — произвольный путь от модели не принимается ни прямо,
/// ни внутри `id`. Отдаёт **ровно** `plugin.json` + `main.lua`: ни настроек, ни документа
/// (TASK-003 §6.3). Не верификатор: вердикта не несёт, поэтому payload без `isError`
/// (ADR-0041 §2.2 п.15).
pub(super) struct PluginReadTool {
    pub(super) sink: LogSink,
}

/// Читает файл каталога плагина; ошибка чистится тем же `sanitize_paths`, что у соседей —
/// полного пути в ответе нет даже если он попадёт в текст ошибки ввода-вывода.
fn read_plugin_file(dir: &Path, name: &str) -> Result<String, ToolError> {
    std::fs::read_to_string(dir.join(name))
        .map_err(|e| ToolError::internal(sanitize_paths(&format!("{name}: {e}"), dir)))
}

impl ToolHandler for PluginReadTool {
    fn name(&self) -> &str {
        "plugin_read"
    }

    fn description(&self) -> &str {
        "Исходник установленного плагина по id: plugin.json и main.lua"
    }

    fn input_schema(&self) -> Value {
        id_schema()
    }

    fn call(&self, params: Value) -> Result<Value, ToolError> {
        let Some(id) = params.get("id").and_then(Value::as_str) else {
            return Err(ToolError::invalid_params("требуется id"));
        };
        crate::ai::plugin_harness::validate_plugin_id(id)
            .map_err(|e| ToolError::invalid_params(e.message))?;
        let root = crate::ai::plugin_harness::plugins_root()
            .map_err(|e| ToolError::internal(e.message))?;
        let dir = crate::ai::plugin_harness::ensure_plugin_path_allowed(&root.join(id))
            .map_err(|e| ToolError::invalid_params(e.message))?;

        let started = Instant::now();
        let manifest = read_plugin_file(&dir, READ_MANIFEST_FILE)?;
        let source = read_plugin_file(&dir, READ_ENTRY_FILE)?;
        if let Err(e) = plugin_core::manifest::validate_source_len(source.len() as u64) {
            let message = format!("{READ_ENTRY_FILE}: {e}");
            log_mcp_call(
                &self.sink,
                &mcp_log_message(
                    "plugin_read",
                    id,
                    started.elapsed(),
                    false,
                    std::slice::from_ref(&message),
                ),
            );
            return Err(ToolError::internal(message));
        }
        log_mcp_call(
            &self.sink,
            &mcp_log_message("plugin_read", id, started.elapsed(), true, &[]),
        );
        Ok(json!({ "manifest": manifest, "source": source }))
    }
}

pub(super) struct PluginCatalogListTool {
    pub(super) sink: LogSink,
    /// Уже разрешённый каталог плагинов (абсолютный, без CWD) — см. `mcp::catalog_repo`.
    pub(super) repo: PathBuf,
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
        let repo = &self.repo;
        let catalog = match plugin_catalog::Catalog::load(repo) {
            Ok(catalog) => catalog,
            Err(error) => {
                let message = sanitize_paths(&error, repo);
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

pub(super) struct MarkdownRenderTool {
    pub(super) sink: LogSink,
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
