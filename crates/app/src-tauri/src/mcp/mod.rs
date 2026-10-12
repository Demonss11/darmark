//! MCP-сервер darmark: in-memory транспорт, адаптеры к существующим командам/CLI.
//!
//! Размещение — в процессе GUI (ADR-0036). MCP — только маршрутизация:
//! проверки (права, rev-guard, безопасность путей) остаются в существующих
//! командах (TZ-MCP R3/R4). Протокольная часть живёт в чистом крейте `mcp-core`;
//! здесь — тонкие адаптеры tools/resources и журналирование вызовов.

pub mod adapters;
pub mod resources;

use mcp_core::Registry;
use tauri::AppHandle;

/// Собирает реестр MCP-инструментов и ресурсов.
pub fn build_registry(app: &AppHandle) -> Registry {
    let mut registry = Registry::new();
    adapters::register_tools(&mut registry, app);
    resources::register_resources(&mut registry, app);
    registry
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
