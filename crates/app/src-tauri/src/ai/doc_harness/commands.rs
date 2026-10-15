use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, State};

use crate::ai::types::{AiDiff, AiEditOp};
use crate::error::CommandError;
use crate::state::{DocumentId, DocumentStore, UndoResult};

use super::apply::{apply_with_guard, emit_document_updated, notify_applied};
use super::diff::checked_diff_ops;

/// Предлагает дифф для документа `doc_id` относительно текущего текста стора.
///
/// Эмитит `ai:edit_proposed { doc_id, diff }` (best-effort). Неизвестный документ →
/// `unknown_document`.
#[tauri::command]
pub fn ai_diff(
    app: AppHandle,
    doc_id: String,
    proposed: String,
    store: State<'_, Arc<Mutex<DocumentStore>>>,
) -> Result<AiDiff, CommandError> {
    let (original, base_rev) = {
        let guard = lock_store(&store);
        let doc = guard
            .get(&DocumentId::new(doc_id.clone()))
            .ok_or_else(|| CommandError::unknown_document(&doc_id))?;
        (doc.text.clone(), doc.rev)
    };
    let ops = checked_diff_ops(&original, &proposed)?;
    let diff = AiDiff {
        doc_id: doc_id.clone(),
        base_rev,
        original,
        modified: proposed,
        ops,
    };
    if let Err(e) = app.emit(
        "ai:edit_proposed",
        serde_json::json!({ "doc_id": doc_id, "diff": &diff }),
    ) {
        eprintln!("не эмитить ai:edit_proposed: {e}");
    }
    Ok(diff)
}

/// Применяет предложенную правку через `DocumentStore::apply_edit` (D5).
///
/// **Rev-guard:** если `base_rev != текущая rev` — правка устарела (`ai_stale_edit`), ничего
/// не применяется. При успехе уведомляет плагины (`document:changed`, как
/// `update_document`/`undo_document`) и фронт (`document-updated` → pull-снапшот, AC-4),
/// затем эмитит `ai:edit_applied { doc_id, rev }`.
///
/// **Guard стора обязан быть отпущен до уведомлений:** доставка `document:changed` плагинам
/// может обслужить host-call к тому же `DocumentStore`, а мьютекс нереентрантен — удержание
/// лока дало бы дедлок. Поэтому `lock_store` живёт во внутреннем блоке, а `notify_applied`
/// вызывается после него. Порядок важен: сначала изменение документа (плагины + фронт),
/// затем `ai:edit_applied`.
#[tauri::command]
pub fn ai_apply_edit(
    app: AppHandle,
    doc_id: String,
    base_rev: u64,
    ops: Vec<AiEditOp>,
    store: State<'_, Arc<Mutex<DocumentStore>>>,
    plugins: State<'_, crate::PluginState>,
) -> Result<UndoResult, CommandError> {
    let result = {
        let mut guard = lock_store(&store);
        let id = DocumentId::new(doc_id.clone());
        apply_with_guard(&mut guard, &id, base_rev, &ops)?
    };
    notify_applied(
        &doc_id,
        result.rev,
        |id, rev| {
            plugins.publish_document_changed(id, rev);
            emit_document_updated(&app, id, rev);
        },
        |id, rev| {
            if let Err(e) = app.emit(
                "ai:edit_applied",
                serde_json::json!({ "doc_id": id, "rev": rev }),
            ) {
                eprintln!("не эмитить ai:edit_applied: {e}");
            }
        },
    );
    Ok(result)
}

/// Блокировка стора без паники (под `panic = "abort"` отравление невозможно).
fn lock_store<'a>(
    store: &'a State<'_, Arc<Mutex<DocumentStore>>>,
) -> std::sync::MutexGuard<'a, DocumentStore> {
    store
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
