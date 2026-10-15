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

/// Снимок плагинных тир-1 представлений.
#[tauri::command]
pub(crate) fn plugin_views(state: tauri::State<'_, PluginState>) -> Vec<PluginViewInfo> {
    state.host.plugin_views()
}

/// Обратная маршрутизация из тир-1 view: `{view_id, action, payload}` → `on_action`
/// плагина. Возвращаемое значение — ответ Lua-хендлера (обычно `null`).
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

/// Снимок реестра плагинов.
#[tauri::command]
pub(crate) fn list_plugins(state: tauri::State<'_, PluginState>) -> Vec<PluginInfo> {
    state.host.list_plugins()
}

/// Включает/выключает плагин; включение снимает карантин (§4.7).
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

/// Перезагружает плагин с диска без рестарта приложения (§4.7).
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

/// Удаляет плагин: останавливает child, снимает views, удаляет настройки и каталог
/// с диска.
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

/// Фиксирует согласие пользователя на права плагина (consent как реальный гейт, §11.1 п.4).
///
/// Сохраняется только `manifest ∩ granted` (deny-by-default); активный плагин
/// перезапускается, чтобы новый набор прав вступил в силу.
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

/// Записывает значение настройки плагина из UI-формы (S5b).
///
/// В отличие от host-call `set_setting` (scope — внедрённый `Supervisor` `_plugin_id`),
/// здесь значение приходит от пользователя из интерфейса, поэтому владелец задаётся
/// параметром `id` напрямую. Запись идёт через `PluginSettingsStore` хоста; менеджер
/// получает `plugins-changed` и перечитывает `list_plugins`, показывая новое значение.
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

/// Исполняет команду плагина: публикует `command:invoked{command_id, doc_id}` в шину.
///
/// `doc_id` — текущий документ: у плагина нет собственного доступа к «активному»
/// документу, поэтому команда несёт его с собой (Фаза 5).
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

/// Dev-only: открыть DevTools WebView2 программно.
///
/// Акселераторы браузера отключены настройкой WebView2 (см.
/// `disable_browser_accelerator_keys`), поэтому DevTools-хоткеи (F12 / Ctrl+Shift+C)
/// не работают — даём свой вход (Ctrl+Shift+I) с фронтенда. Метод доступен в
/// debug-сборке; в release — no-op, чтобы не тянуть фичу `devtools` и не растить бюджет D6.
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
