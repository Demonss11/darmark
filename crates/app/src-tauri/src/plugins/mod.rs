//! Плагинная подсистема GUI-хоста (TZ-H2).
//!
//! GUI-хост (`darmark`) **не** линкует `mlua` (D6/ADR-0021): Lua живёт в отдельном процессе
//! `darmark-plugin-host.exe`, который [`supervisor::Supervisor`] спавнит как внешний exe.
//! Здесь же — сканирование/настройки/манифесты, проверка permissions (до вызова host-функции)
//! и карантин per-plugin.

pub mod bus;
pub mod host;
pub mod manager;
pub mod scan;
pub mod services;
pub mod settings;
pub mod supervisor;
pub mod views;

#[cfg(test)]
mod tests;

use plugin_proto::envelope::PluginError;
use serde_json::Value;

/// Услуги хоста, которые плагин вызывает через host-call (range/delta документа, настройки,
/// экспорт). Проверку permissions делает [`supervisor::Supervisor`] **до** вызова; реализация
/// только исполняет операцию и остаётся без Tauri (тестируема на фейке).
pub trait HostServices: Send + Sync + 'static {
    /// Обрабатывает один host-call. `Err` возвращается плагину как значение, не паника.
    fn handle(&self, method: &str, args: Value) -> Result<Value, PluginError>;
}

// Типы контракта с фронтендом объявлены платформенно-нейтрально в `crate`: команда
// `list_plugins` должна компилироваться и на не-Windows.
pub use crate::{CommandInfo, PluginInfo, PluginStatus};
