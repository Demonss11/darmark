//! Плагинная подсистема GUI-хоста (плагинная система).
//!
//! GUI-хост (`darmark`) **не** линкует `mlua` (D6): Lua живёт в отдельном процессе
//! `darmark-plugin-host.exe`, который [`supervisor::Supervisor`] спавнит как внешний exe.
//! Здесь же — сканирование/настройки/манифесты, проверка permissions (до вызова host-функции)
//! и карантин per-plugin.

pub mod bus;
pub mod host;
pub mod manager;
pub mod plugin_settings;
pub mod resource;
pub mod scan;
pub mod services;
pub mod settings;
pub mod supervisor;
pub mod views;

#[cfg(test)]
mod tests;

use plugin_core::envelope::PluginError;
use serde_json::Value;




pub trait HostServices: Send + Sync + 'static {
    
    fn handle(&self, method: &str, args: Value) -> Result<Value, PluginError>;
}

pub use crate::{CommandInfo, PluginInfo, PluginStatus};
