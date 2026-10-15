//! MCP-сервер darmark: in-memory транспорт, адаптеры к существующим командам/CLI.
//!
//! Размещение — в процессе GUI (ADR-0036). MCP — только маршрутизация:
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

/// Собирает реестр MCP-инструментов и ресурсов для запущенного приложения.
///
/// Каталог плагинов разрешается от `app_data_dir` (а не от CWD): в упакованном приложении
/// рабочий каталог произволен, а источник каталога должен быть детерминированным.
pub fn build_registry(app: &AppHandle) -> Registry {
    let app_data_dir = app.path().app_data_dir().unwrap_or_default();
    build_registry_with_sink(app_log_sink(app.clone()), &app_data_dir)
}

/// Собирает реестр с заданным стоком журнала и каталогом данных — без Tauri.
///
/// Единственная точка сборки инвентаря MCP (4 tools + 6 resources, TZ-MCP §4):
/// [`build_registry`] передаёт сюда продакшн-сток и `app_data_dir`, контракт-тесты —
/// собирающий сток (AC-MCP-6/AC-MCP-13), поэтому тест проверяет тот же состав, что и
/// приложение. Каталог плагинов вычисляется здесь один раз ([`catalog_repo`]) и
/// «запекается» в обработчики: зависимости от CWD на пути чтения нет.
pub fn build_registry_with_sink(sink: adapters::LogSink, app_data_dir: &Path) -> Registry {
    let repo = catalog_repo(app_data_dir);
    let mut registry = Registry::new();
    adapters::register_tools(&mut registry, &sink, &repo);
    resources::register_resources(&mut registry, &repo);
    registry
}

/// Эмиттер событий журнала — граница между стоком MCP и транспортом (Tauri-событие в проде).
///
/// Инъекция вместо жёсткой зависимости от `AppHandle` делает конверт события `log-entry`
/// (уровень/категория/сообщение) проверяемым Rust-тестом: прод отдаёт [`AppHandle`], тест —
/// собирающий mock. Прод-поведение не меняется.
pub trait LogEmitter: Send + Sync + 'static {
    /// Отправляет событие `event` с JSON-конвертом `payload`; `Err` — транспорт недоступен.
    fn emit(&self, event: &str, payload: Value) -> Result<(), String>;
}

impl LogEmitter for AppHandle {
    fn emit(&self, event: &str, payload: Value) -> Result<(), String> {
        Emitter::emit(self, event, payload).map_err(|e| e.to_string())
    }
}

/// Сток журнала MCP → Tauri-событие `log-entry` (TZ-MCP R2, best-effort).
///
/// Сбой `emit` логируется и не роняет вызов инструмента: журнал — наблюдаемость,
/// а не критичный путь. Конверт несёт уровень `info` и категорию `system` (TS-тип
/// `LogCategory` не трогаем, §4.6).
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

/// Каталог-источник плагинов относительно каталога данных приложения: env
/// `DARMARK_PLUGIN_REPO` → автопоиск от `app_data_dir` → `app_data_dir/plugins`.
///
/// `app_data_dir` передаётся явно (в проде — `app.path().app_data_dir()`), поэтому функция не
/// зависит от рабочего каталога процесса: в упакованном приложении CWD произволен, а источник
/// каталога должен быть детерминированным. Env-override сохраняется для dev/E2E (ADR-0026);
/// автопоиск находит репозиторный `examples/plugins/`. Если каталога нет — вызывающий
/// получает понятную ошибку от `Catalog::load`.
pub(crate) fn catalog_repo(app_data_dir: &Path) -> std::path::PathBuf {
    if let Some(dir) = std::env::var_os("DARMARK_PLUGIN_REPO") {
        return std::path::PathBuf::from(dir);
    }
    plugin_catalog::discover_repo(app_data_dir).unwrap_or_else(|| app_data_dir.join("plugins"))
}
