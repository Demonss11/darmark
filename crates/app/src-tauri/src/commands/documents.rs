//! Команды документов (1:1 с host-функциями плагинов, §4.3).
//!
//! Здесь же `lock_store` — единственная точка лока стора: инвариант «guard отпущен до
//! `pump`» держится на том, что все команды берут стор только на время блока.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::error::CommandError;
use crate::fs::{allow_asset_dir, file_name, read_text, write_text};
use crate::state::{
    DocMeta, DocumentId, DocumentSnapshot, DocumentStore, RenderResult, UndoResult,
};
use crate::PluginState;

pub(super) fn lock_store<'a>(
    store: &'a tauri::State<'_, Arc<Mutex<DocumentStore>>>,
) -> std::sync::MutexGuard<'a, DocumentStore> {
    store
        .lock()
        .expect("стор документов не должен быть отравлен")
}


#[tauri::command(async)]
pub(crate) fn new_document(
    text: Option<String>,
    store: tauri::State<'_, Arc<Mutex<DocumentStore>>>,
    plugins: tauri::State<'_, PluginState>,
) -> DocumentSnapshot {
    let snapshot = lock_store(&store).create(text.unwrap_or_default());
    plugins.publish_opened(snapshot.id.as_str(), snapshot.path.as_deref());
    let name = file_name(snapshot.path.as_deref());
    plugins.publish_document_event("new", name.as_deref(), Some(snapshot.rev));
    snapshot
}


#[tauri::command]
pub(crate) async fn open_document(
    path: PathBuf,
    app: tauri::AppHandle,
    store: tauri::State<'_, Arc<Mutex<DocumentStore>>>,
    plugins: tauri::State<'_, PluginState>,
) -> Result<DocumentSnapshot, CommandError> {
    let read_path = path.clone();
    let text = tauri::async_runtime::spawn_blocking(move || read_text(&read_path))
        .await
        .map_err(|e| CommandError::io(e.to_string()))??;
    if let Some(dir) = path.parent() {
        allow_asset_dir(&app, dir);
    }
    let snapshot = lock_store(&store).insert_loaded(path, text);
    plugins.publish_opened(snapshot.id.as_str(), snapshot.path.as_deref());
    let name = file_name(snapshot.path.as_deref());
    plugins.publish_document_event("open", name.as_deref(), Some(snapshot.rev));
    Ok(snapshot)
}



#[tauri::command]
pub(crate) async fn save_document(
    id: DocumentId,
    path: Option<PathBuf>,
    app: tauri::AppHandle,
    store: tauri::State<'_, Arc<Mutex<DocumentStore>>>,
    plugins: tauri::State<'_, PluginState>,
) -> Result<DocMeta, CommandError> {
    let explicit_path = path;
    let (target, body) = {
        let guard = lock_store(&store);
        let doc = guard
            .get(&id)
            .ok_or_else(|| CommandError::unknown_document(id.as_str()))?;
        let target = match &explicit_path {
            Some(p) => p.clone(),
            None => doc
                .path
                .clone()
                .ok_or_else(|| CommandError::io("Не задан путь сохранения"))?,
        };
        (target, doc.text.clone())
    };

    let write_path = target.clone();
    tauri::async_runtime::spawn_blocking(move || write_text(&write_path, &body))
        .await
        .map_err(|e| CommandError::io(e.to_string()))??;
    if let Some(dir) = target.parent() {
        allow_asset_dir(&app, dir);
    }
    let meta = {
        let mut guard = lock_store(&store);
        let doc = guard
            .get_mut(&id)
            .ok_or_else(|| CommandError::unknown_document(id.as_str()))?;
        if let Some(p) = explicit_path {
            doc.path = Some(p);
        }
        doc.meta()
    };
    let name = file_name(meta.path.as_deref());
    plugins.publish_document_event("save", name.as_deref(), Some(meta.rev));
    Ok(meta)
}


#[tauri::command(async)]
pub(crate) fn close_document(
    id: DocumentId,
    store: tauri::State<'_, Arc<Mutex<DocumentStore>>>,
    plugins: tauri::State<'_, PluginState>,
) {
    let path = {
        let guard = lock_store(&store);
        guard
            .get(&id)
            .and_then(|doc| doc.path.as_ref().map(|p| p.to_string_lossy().into_owned()))
    };
    let closed = lock_store(&store).close(&id);
    if closed {
        plugins.publish_closed(id.as_str());
        let name = file_name(path.as_deref());
        plugins.publish_document_event("close", name.as_deref(), None);
    }
}










#[tauri::command(async)]
pub(crate) fn update_document(
    id: DocumentId,
    text: String,
    mapped: Option<bool>,
    store: tauri::State<'_, Arc<Mutex<DocumentStore>>>,
    plugins: tauri::State<'_, PluginState>,
) -> Result<RenderResult, CommandError> {
    let (result, rev_changed) = {
        let mut guard = lock_store(&store);
        let prev = guard.get(&id).map(|doc| doc.rev);
        let result = guard
            .update(&id, text, mapped.unwrap_or(false))
            .ok_or_else(|| CommandError::unknown_document(id.as_str()))?;
        let rev_changed = prev != Some(result.rev);
        (result, rev_changed)
    };
    if rev_changed {
        plugins.publish_document_changed(id.as_str(), result.rev);
    }
    Ok(result)
}






#[tauri::command(async)]
pub(crate) fn undo_document(
    id: DocumentId,
    store: tauri::State<'_, Arc<Mutex<DocumentStore>>>,
    plugins: tauri::State<'_, PluginState>,
) -> Result<UndoResult, CommandError> {
    let result = {
        let mut guard = lock_store(&store);
        if guard.get(&id).is_none() {
            return Err(CommandError::unknown_document(id.as_str()));
        }
        guard.undo(&id).ok_or_else(CommandError::nothing_to_undo)?
    };
    plugins.publish_document_changed(id.as_str(), result.rev);
    Ok(result)
}




#[tauri::command(async)]
pub(crate) fn redo_document(
    id: DocumentId,
    store: tauri::State<'_, Arc<Mutex<DocumentStore>>>,
    plugins: tauri::State<'_, PluginState>,
) -> Result<UndoResult, CommandError> {
    let result = {
        let mut guard = lock_store(&store);
        if guard.get(&id).is_none() {
            return Err(CommandError::unknown_document(id.as_str()));
        }
        guard.redo(&id).ok_or_else(CommandError::nothing_to_redo)?
    };
    plugins.publish_document_changed(id.as_str(), result.rev);
    Ok(result)
}




#[tauri::command]
pub(crate) fn render_document(
    id: DocumentId,
    mapped: Option<bool>,
    store: tauri::State<'_, Arc<Mutex<DocumentStore>>>,
) -> Result<RenderResult, CommandError> {
    lock_store(&store)
        .render(&id, mapped.unwrap_or(false))
        .ok_or_else(|| CommandError::unknown_document(id.as_str()))
}


#[tauri::command]
pub(crate) fn document_snapshot(
    id: DocumentId,
    store: tauri::State<'_, Arc<Mutex<DocumentStore>>>,
) -> Option<DocumentSnapshot> {
    lock_store(&store).get(&id).map(|doc| doc.snapshot())
}


#[tauri::command]
pub(crate) fn list_documents(store: tauri::State<'_, Arc<Mutex<DocumentStore>>>) -> Vec<DocMeta> {
    lock_store(&store).list()
}






#[tauri::command]
pub(crate) fn activate_document(
    id: DocumentId,
    store: tauri::State<'_, Arc<Mutex<DocumentStore>>>,
) -> Result<(), CommandError> {
    lock_store(&store)
        .activate(&id)
        .then_some(())
        .ok_or_else(|| CommandError::unknown_document(id.as_str()))
}
