//! Инструмент `plugin_read`: исходник **установленного** плагина по `id` (TASK-003 §2).
//!
//! Отдельным модулем, а не соседом в [`super::tools`]: контракт крупнее прочих адаптеров —
//! два файла с потолками чтения, `entry` из манифеста и подгонка ответа под лимит контекста
//! модели, — а `tools.rs` с этими правками перешагнул бы порог 400 строк (ADR-0039 §2.1).

use std::path::{Path, PathBuf};
use std::time::Instant;

use mcp_core::handlers::{ToolError, ToolHandler};
use serde_json::{json, Value};

use super::journal::{log_mcp_call, mcp_log_message};
use super::privacy::sanitize_paths;
use super::{id_schema, LogSink};
use crate::ai::tool_loop::{truncate_chars, MAX_TOOL_RESULT_CHARS};
use crate::plugins::scan::{read_capped, read_source, MAX_MANIFEST_BYTES};


const READ_MANIFEST_FILE: &str = "plugin.json";


const READ_ENTRY_FILE: &str = "main.lua";



const READ_PAYLOAD_RESERVE_CHARS: usize = 512;



const READ_PAYLOAD_BUDGET: usize = MAX_TOOL_RESULT_CHARS - READ_PAYLOAD_RESERVE_CHARS;

















pub(super) struct PluginReadTool {
    pub(super) sink: LogSink,
}

impl ToolHandler for PluginReadTool {
    fn name(&self) -> &str {
        "plugin_read"
    }

    fn description(&self) -> &str {
        "Исходник установленного плагина по id: plugin.json и entry из манифеста"
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
        match read_payload(&dir) {
            Ok(payload) => {
                log_mcp_call(
                    &self.sink,
                    &mcp_log_message("plugin_read", id, started.elapsed(), true, &[]),
                );
                Ok(payload)
            }
            Err(message) => {
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
                Err(ToolError::internal(message))
            }
        }
    }
}




fn read_payload(dir: &Path) -> Result<Value, String> {
    read_payload_raw(dir).map_err(|error| sanitize_paths(&error, dir))
}


fn read_payload_raw(dir: &Path) -> Result<Value, String> {
    let manifest = read_manifest(dir)?;
    let entry_path = declared_entry_path(dir, &manifest)?;
    let source = read_source(&entry_path)?;
    fit_payload(&manifest, &source)
}





fn read_manifest(dir: &Path) -> Result<String, String> {
    let bytes = read_capped(&dir.join(READ_MANIFEST_FILE), MAX_MANIFEST_BYTES)?;
    String::from_utf8(bytes).map_err(|e| format!("{READ_MANIFEST_FILE}: не UTF-8: {e}"))
}











fn declared_entry_path(dir: &Path, manifest: &str) -> Result<PathBuf, String> {
    let entry = plugin_core::manifest::Manifest::from_json(manifest.as_bytes())
        .map(|manifest| manifest.entry)
        .unwrap_or_else(|_| READ_ENTRY_FILE.to_string());
    crate::ai::plugin_harness::ensure_plugin_entry_allowed(dir, &entry).map_err(|e| e.message)
}






fn fit_payload(manifest: &str, source: &str) -> Result<Value, String> {
    if payload_chars(manifest, source, false) <= READ_PAYLOAD_BUDGET {
        return Ok(json!({
            "manifest": manifest,
            "source": source,
            "truncated": false,
        }));
    }
    let Some(fitted) = fit_source(manifest, source) else {
        return Err(format!(
            "{READ_MANIFEST_FILE}: ответ не укладывается в лимит модели ({MAX_TOOL_RESULT_CHARS} символов)"
        ));
    };
    Ok(json!({
        "manifest": manifest,
        "source": fitted,
        "truncated": true,
    }))
}





fn fit_source(manifest: &str, source: &str) -> Option<String> {
    let fits = |chars: usize| {
        payload_chars(manifest, &truncate_chars(source, chars), true) <= READ_PAYLOAD_BUDGET
    };
    if !fits(0) {
        return None;
    }

    
    
    let mut low = 0;
    let mut high = source.chars().count();
    while low < high {
        let mid = low + (high - low).div_ceil(2);
        if fits(mid) {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    Some(truncate_chars(source, low))
}



fn payload_chars(manifest: &str, source: &str, truncated: bool) -> usize {
    json!({
        "manifest": manifest,
        "source": source,
        "truncated": truncated,
    })
    .to_string()
    .chars()
    .count()
}
