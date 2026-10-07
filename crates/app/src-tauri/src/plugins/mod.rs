//! Плагинная подсистема GUI-хоста (Фаза 2 TZ-H2).
//!
//! GUI-хост (`darmark`) **не** линкует `mlua` (D6/ADR-0021): Lua живёт в отдельном процессе
//! `darmark-plugin-host.exe`, который [`supervisor::Supervisor`] спавнит как внешний exe.
//! Здесь же — проверка permissions (до вызова host-функции) и карантин per-plugin.

pub mod manager;
pub mod services;
pub mod supervisor;

#[cfg(test)]
mod tests;

use plugin_proto::envelope::PluginError;
use serde::Serialize;
use serde_json::Value;

/// Услуги хоста, которые плагин вызывает через host-call (range/delta документа, настройки,
/// экспорт). Проверку permissions делает [`supervisor::Supervisor`] **до** вызова; реализация
/// только исполняет операцию и остаётся без Tauri (тестируема на фейке).
pub trait HostServices: Send + Sync + 'static {
    /// Обрабатывает один host-call. `Err` возвращается плагину как значение, не паника.
    fn handle(&self, method: &str, args: Value) -> Result<Value, PluginError>;
}

/// Статус плагина для менеджера (§4.7 TZ-H2).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PluginStatus {
    /// Остановлен (выключен пользователем или ещё не запускался).
    Stopped,
    /// Загружен и активирован.
    Active,
    /// Отказ загрузки/активации; сообщение для UI.
    Failed { message: String },
    /// Автоотключён после серии падений (карантин).
    Quarantined,
}
