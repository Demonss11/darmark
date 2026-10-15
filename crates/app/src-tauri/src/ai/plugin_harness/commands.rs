use serde_json::Value;

use super::crud::{
    create_plugin_at, delete_plugin_at, temp_validate_dir, update_plugin_at, write_plugin_files,
};
use super::paths::plugins_root;
use super::validate::{validate_plugin, validate_plugin_id};
use crate::ai::types::AiValidationResult;
use crate::error::CommandError;

/// Валидирует плагин во временном каталоге (запись → проверка), не трогая реальный профиль.
#[tauri::command(async)]
pub fn ai_validate_plugin(
    id: String,
    manifest: Value,
    source: String,
) -> Result<AiValidationResult, CommandError> {
    validate_plugin_id(&id)?;
    let dir = temp_validate_dir(&id);
    write_plugin_files(&dir, &manifest, &source)?;
    let result = validate_plugin(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    Ok(result)
}

/// Создаёт плагин: запись → валидация → регистрация. Эмитит `ai:plugin_created`.
#[tauri::command(async)]
pub fn ai_create_plugin(
    app: tauri::AppHandle,
    id: String,
    manifest: Value,
    source: String,
    state: tauri::State<'_, crate::PluginState>,
) -> Result<crate::PluginInfo, CommandError> {
    let root = plugins_root()?;
    let info = create_plugin_at(&state.host, &root, &id, &manifest, &source, validate_plugin)?;
    emit_plugin_event(&app, "ai:plugin_created", &id);
    Ok(info)
}

/// Обновляет плагин: перезапись → валидация → `reload_plugin`. Эмитит `ai:plugin_updated`.
#[tauri::command(async)]
pub fn ai_update_plugin(
    app: tauri::AppHandle,
    id: String,
    manifest: Value,
    source: String,
    state: tauri::State<'_, crate::PluginState>,
) -> Result<crate::PluginInfo, CommandError> {
    let root = plugins_root()?;
    let info = update_plugin_at(&state.host, &root, &id, &manifest, &source, validate_plugin)?;
    emit_plugin_event(&app, "ai:plugin_updated", &id);
    Ok(info)
}

/// Удаляет плагин через существующий `PluginHost::remove`. Эмитит `ai:plugin_deleted`.
#[tauri::command(async)]
pub fn ai_delete_plugin(
    app: tauri::AppHandle,
    id: String,
    state: tauri::State<'_, crate::PluginState>,
) -> Result<(), CommandError> {
    delete_plugin_at(&state.host, &id)?;
    emit_plugin_event(&app, "ai:plugin_deleted", &id);
    Ok(())
}

/// Эмитит событие harness'а `{ plugin_id }` (best-effort: сбой не роняет команду).
fn emit_plugin_event(app: &tauri::AppHandle, event: &str, id: &str) {
    use tauri::Emitter;
    if let Err(e) = app.emit(event, serde_json::json!({ "plugin_id": id })) {
        eprintln!("не эмитить {event}: {e}");
    }
}
