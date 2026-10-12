//! MCP-сервер darmark: in-memory транспорт, адаптеры к существующим командам/CLI.
//!
//! Размещение — в процессе GUI (ADR-0036). MCP — только маршрутизация:
//! проверки (права, rev-guard, безопасность путей) остаются в существующих
//! командах (TZ-MCP R3/R4). Протокольная часть живёт в чистом крейте `mcp-core`;
//! здесь — тонкие адаптеры tools/resources и журналирование вызовов.

pub mod adapters;
pub mod resources;

use std::sync::Arc;

use mcp_core::Registry;
use tauri::{AppHandle, Emitter};

/// Собирает реестр MCP-инструментов и ресурсов для запущенного приложения.
pub fn build_registry(app: &AppHandle) -> Registry {
    build_registry_with_sink(app_log_sink(app))
}

/// Собирает реестр с заданным стоком журнала — без Tauri.
///
/// Единственная точка сборки инвентаря MCP (4 tools + 6 resources, TZ-MCP §4):
/// [`build_registry`] передаёт сюда продакшн-сток, контракт-тесты — собирающий
/// (AC-MCP-6/AC-MCP-13), поэтому тест проверяет тот же состав, что и приложение.
pub fn build_registry_with_sink(sink: adapters::LogSink) -> Registry {
    let mut registry = Registry::new();
    adapters::register_tools(&mut registry, &sink);
    resources::register_resources(&mut registry);
    registry
}

/// Сток журнала MCP → Tauri-событие `log-entry` (TZ-MCP R2, best-effort).
///
/// Сбой `emit` логируется и не роняет вызов инструмента: журнал — наблюдаемость,
/// а не критичный путь.
fn app_log_sink(app: &AppHandle) -> adapters::LogSink {
    let handle = app.clone();
    Arc::new(move |message: &str| {
        if let Err(e) = handle.emit(
            "log-entry",
            serde_json::json!({ "level": "info", "category": "system", "message": message }),
        ) {
            eprintln!("не эмитить log-entry: {e}");
        }
    })
}

/// Каталог-источник плагинов: env `DARMARK_PLUGIN_REPO` → автопоиск от CWD → `<cwd>/plugins`.
///
/// В dev-сборке находит репозиторный `examples/plugins/` (ADR-0026); в дистрибутиве каталога
/// может не быть — тогда ресурс/инструмент вернёт понятную ошибку (открытый вопрос TZ-MCP §8).
pub(crate) fn catalog_repo() -> std::path::PathBuf {
    if let Some(dir) = std::env::var_os("DARMARK_PLUGIN_REPO") {
        return std::path::PathBuf::from(dir);
    }
    let cwd = std::env::current_dir().unwrap_or_default();
    plugin_catalog::discover_repo(&cwd).unwrap_or_else(|| cwd.join("plugins"))
}
