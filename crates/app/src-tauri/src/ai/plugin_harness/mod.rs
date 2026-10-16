//! Harness плагинов (ADR-0034/ADR-0026): валидация через `darmark-plugin test --json` с fallback на
//! `plugin_core::manifest` + `darmark-plugin-host --self-test`, а также создание/обновление/
//! удаление плагинов через существующий [`crate::plugins::host::PluginHost`].
//!
//! Порядок строго **запись → валидация → регистрация** (ADR-0034 §2.4): невалидный плагин не попадает
//! в реестр. Сеть/LLM живут в AI-модуле, CLI используется как dev-инструмент (сеть в него не
//! вводится). Безопасность id проверяется **до** записи (нет traversal/спецсимволов).
//!
//! **Обновление не затирает рабочий плагин:** предложенный контент валидируется во временном
//! каталоге **до** перезаписи реальных файлов; при `!ok` реальные файлы не трогаются (ADR-0034/ADR-0026).

mod commands;
mod crud;
mod paths;
mod proc;
mod validate;

#[cfg(test)]
mod tests;

pub use commands::{ai_create_plugin, ai_delete_plugin, ai_update_plugin, ai_validate_plugin};
pub use crud::{create_plugin_at, delete_plugin_at, update_plugin_at, write_plugin_files};
pub use paths::{ensure_plugin_entry_allowed, ensure_plugin_path_allowed, plugins_root};
pub use validate::{
    check_plugin_limits, parse_cli_report, validate_manifest_id, validate_plugin,
    validate_plugin_id, validate_plugin_static,
};

pub use commands::{
    __cmd__ai_create_plugin, __cmd__ai_delete_plugin, __cmd__ai_update_plugin,
    __cmd__ai_validate_plugin, __tauri_command_name_ai_create_plugin,
    __tauri_command_name_ai_delete_plugin, __tauri_command_name_ai_update_plugin,
    __tauri_command_name_ai_validate_plugin,
};
