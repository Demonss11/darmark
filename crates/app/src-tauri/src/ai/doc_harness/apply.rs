use tauri::{AppHandle, Emitter};

use crate::ai::types::AiEditOp;
use crate::error::{CommandError, ErrorCode};
use crate::state::{DocumentId, DocumentStore, UndoResult};






pub fn apply_with_guard(
    store: &mut DocumentStore,
    id: &DocumentId,
    base_rev: u64,
    ops: &[AiEditOp],
) -> Result<UndoResult, CommandError> {
    let current_rev = store
        .get(id)
        .map(|doc| doc.rev)
        .ok_or_else(|| CommandError::unknown_document(id.as_str()))?;
    if current_rev != base_rev {
        return Err(CommandError::new(
            ErrorCode::AiStaleEdit,
            "документ изменился с момента предложения правки",
        ));
    }
    apply_ops(store, id, ops)
}







fn apply_ops(
    store: &mut DocumentStore,
    id: &DocumentId,
    ops: &[AiEditOp],
) -> Result<UndoResult, CommandError> {
    {
        let doc = store
            .get(id)
            .ok_or_else(|| CommandError::unknown_document(id.as_str()))?;
        validate_ops(&doc.text, ops)?;
    }
    let mut ordered: Vec<&AiEditOp> = ops.iter().collect();
    ordered.sort_by_key(|op| std::cmp::Reverse(op.start));
    for op in ordered {
        store
            .apply_edit(id, op.start, op.end, &op.text)
            .ok_or_else(|| apply_failed(op))?;
    }
    let doc = store
        .get(id)
        .ok_or_else(|| CommandError::unknown_document(id.as_str()))?;
    let (start, end) = ops
        .first()
        .map(|op| (op.start, op.start + op.text.len()))
        .unwrap_or((0, 0));
    Ok(UndoResult {
        text: doc.text.clone(),
        rev: doc.rev,
        start,
        end,
    })
}




fn validate_ops(text: &str, ops: &[AiEditOp]) -> Result<(), CommandError> {
    let mut ordered: Vec<&AiEditOp> = ops.iter().collect();
    ordered.sort_by_key(|op| op.start);
    let mut prev_end = 0usize;
    for op in ordered {
        if op.start > op.end || op.end > text.len() {
            return Err(apply_failed(op));
        }
        if !text.is_char_boundary(op.start) || !text.is_char_boundary(op.end) {
            return Err(apply_failed(op));
        }
        if op.start < prev_end {
            return Err(CommandError::new(
                ErrorCode::AiApplyFailed,
                format!("правки перекрываются: [{}, {})", op.start, op.end),
            ));
        }
        prev_end = op.end;
    }
    Ok(())
}


fn apply_failed(op: &AiEditOp) -> CommandError {
    CommandError::new(
        ErrorCode::AiApplyFailed,
        format!("невалидная правка [{}, {})", op.start, op.end),
    )
}




pub(in crate::ai) fn notify_applied(
    doc_id: &str,
    rev: u64,
    mut document_changed: impl FnMut(&str, u64),
    mut edit_applied: impl FnMut(&str, u64),
) {
    document_changed(doc_id, rev);
    edit_applied(doc_id, rev);
}




pub(super) fn emit_document_updated(app: &AppHandle, doc_id: &str, rev: u64) {
    if let Err(e) = app.emit(
        "document-updated",
        serde_json::json!({ "doc_id": doc_id, "rev": rev }),
    ) {
        eprintln!("не эмитить document-updated: {e}");
    }
}
