//! Tauri-команды шелла, сгруппированные по контракту с фронтендом.
//!
//! Команды помечены `#[tauri::command]` и адресуются из `generate_handler!` в
//! [`crate::run`] по коротким именам, поэтому модуль реэкспортирует их всех через `*`.

mod documents;
mod plugins;

pub(crate) use documents::*;
pub(crate) use plugins::*;
