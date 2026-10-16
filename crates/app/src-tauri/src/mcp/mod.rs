//! MCP-сервер darmark: in-memory транспорт, адаптеры к существующим командам/CLI.
//!
//! Размещение — в процессе GUI. MCP — только маршрутизация:
//! проверки (права, rev-guard, безопасность путей) остаются в существующих
//! командах (TZ-MCP R3/R4). Протокольная часть живёт в чистом крейте `mcp-core`;
//! здесь — тонкие адаптеры tools/resources и журналирование вызовов.

pub mod adapters;
pub mod resources;

#[cfg(test)]
mod tests;

use std::path::Path;
use std::sync::Arc;

use mcp_core::Registry;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};





pub fn build_registry(app: &AppHandle) -> Registry {
    let app_data_dir = app.path().app_data_dir().unwrap_or_default();
    build_registry_with_sink(app_log_sink(app.clone()), &app_data_dir)
}








pub fn build_registry_with_sink(sink: adapters::LogSink, app_data_dir: &Path) -> Registry {
    let repo = catalog_repo(app_data_dir);
    let mut registry = Registry::new();
    adapters::register_tools(&mut registry, &sink, &repo);
    resources::register_resources(&mut registry, &repo);
    registry
}






pub trait LogEmitter: Send + Sync + 'static {
    
    fn emit(&self, event: &str, payload: Value) -> Result<(), String>;
}

impl LogEmitter for AppHandle {
    fn emit(&self, event: &str, payload: Value) -> Result<(), String> {
        Emitter::emit(self, event, payload).map_err(|e| e.to_string())
    }
}






fn app_log_sink(emitter: impl LogEmitter) -> adapters::LogSink {
    Arc::new(move |message: &str| {
        let payload = serde_json::json!({
            "level": "info",
            "category": "system",
            "message": message,
        });
        if let Err(e) = emitter.emit("log-entry", payload) {
            eprintln!("не эмитить log-entry: {e}");
        }
    })
}







pub(crate) fn catalog_repo(app_data_dir: &Path) -> std::path::PathBuf {
    app_data_dir.join("plugins")
}
