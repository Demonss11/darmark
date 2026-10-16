//! Команды плагинов: тир-1 представления (Фаза 4) и менеджер плагинов (Фаза 5),
//! плюс dev-only вход в DevTools.
//!
//! Все команды работают через [`PluginState`] — обёртку над `plugins::host::PluginHost`,
//! которую поднимает [`crate::setup::setup_plugins`].

use std::sync::{Arc, Mutex};

use crate::contract::{PluginInfo, PluginViewInfo};
use crate::error::{self, CommandError};
use crate::state::DocumentStore;
use crate::PluginState;

use super::documents::lock_store;


#[tauri::command]
pub(crate) fn plugin_views(state: tauri::State<'_, PluginState>) -> Vec<PluginViewInfo> {
    state.host.plugin_views()
}



#[tauri::command(async)]
pub(crate) fn plugin_view_action(
    view_id: String,
    action: String,
    payload: Option<serde_json::Value>,
    state: tauri::State<'_, PluginState>,
) -> Result<serde_json::Value, CommandError> {
    state
        .host
        .plugin_view_action(&view_id, &action, payload)
        .map_err(|error| CommandError::new(error::ErrorCode::Plugin, error.message))
}


#[tauri::command]
pub(crate) fn list_plugins(state: tauri::State<'_, PluginState>) -> Vec<PluginInfo> {
    state.host.list_plugins()
}


#[tauri::command(async)]
pub(crate) fn set_plugin_enabled(
    id: String,
    enabled: bool,
    state: tauri::State<'_, PluginState>,
) -> Result<(), CommandError> {
    state
        .host
        .set_enabled(&id, enabled)
        .map_err(|error| CommandError::new(error::ErrorCode::Plugin, error.message))
}


#[tauri::command(async)]
pub(crate) fn reload_plugin(
    id: String,
    state: tauri::State<'_, PluginState>,
) -> Result<(), CommandError> {
    state
        .host
        .reload(&id)
        .map_err(|error| CommandError::new(error::ErrorCode::Plugin, error.message))
}



#[tauri::command(async)]
pub(crate) fn remove_plugin(
    id: String,
    state: tauri::State<'_, PluginState>,
) -> Result<(), CommandError> {
    state
        .host
        .remove(&id)
        .map_err(|error| CommandError::new(error::ErrorCode::Plugin, error.message))
}





#[tauri::command(async)]
pub(crate) fn set_plugin_permissions(
    id: String,
    granted: Vec<String>,
    state: tauri::State<'_, PluginState>,
) -> Result<(), CommandError> {
    state
        .host
        .set_permissions(&id, granted)
        .map_err(|error| CommandError::new(error::ErrorCode::Plugin, error.message))
}







#[tauri::command(async)]
pub(crate) fn set_plugin_setting(
    id: String,
    key: String,
    value: serde_json::Value,
    state: tauri::State<'_, PluginState>,
) -> Result<(), CommandError> {
    state
        .host
        .plugin_setting_set(&id, &key, value)
        .map_err(|error| CommandError::new(error::ErrorCode::Plugin, error.message))
}





#[tauri::command(async)]
pub(crate) fn run_plugin_command(
    command_id: String,
    store: tauri::State<'_, Arc<Mutex<DocumentStore>>>,
    state: tauri::State<'_, PluginState>,
) -> Result<(), CommandError> {
    let doc_id = lock_store(&store)
        .last_id()
        .map(|id| id.as_str().to_string());
    state
        .host
        .run_plugin_command(&command_id, doc_id.as_deref());
    Ok(())
}







#[tauri::command]
pub(crate) fn open_devtools(window: tauri::WebviewWindow) {
    #[cfg(debug_assertions)]
    {
        window.open_devtools();
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = window;
    }
}
