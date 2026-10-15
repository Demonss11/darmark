//! Стартовое наполнение приложения при `.setup()`.
//!
//! Модуль `env` — настройки каталога и WebView2, модуль `plugins` — подъём плагинной
//! подсистемы и `PluginState`, который дальше адресуют Tauri-команды.

mod env;
mod plugins;

pub(crate) use env::{disable_browser_accelerator_keys, init_settings_dir};
pub(crate) use plugins::setup_plugins;
pub use plugins::PluginState;
