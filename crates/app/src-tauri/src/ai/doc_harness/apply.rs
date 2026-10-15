use tauri::{AppHandle, Emitter};

use crate::ai::types::AiEditOp;
use crate::error::{CommandError, ErrorCode};
use crate::state::{DocumentId, DocumentStore, UndoResult};

/// Rev-guard + применение операций: ядро команды [`ai_apply_edit`](super::ai_apply_edit)
/// (без Tauri — тестируемо).
///
/// `base_rev != rev` → `ai_stale_edit` (правка не применяется). Неизвестный документ →
/// `unknown_document`. Невалидные смещения → `ai_apply_failed`.
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

/// Применяет операции в порядке убывания `start`, чтобы более ранние смещения не «поехали».
///
/// **Атомарность (ADR-0034):** сначала **все** операции проверяются против базового текста
/// ([`validate_ops`]); при любой невалидной — `ai_apply_failed` **без частичных изменений**.
/// Возвращает [`UndoResult`] по первой (нижней по смещению) операции — фронт по нему
/// выставляет выделение.
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

/// Проверяет все операции против базового текста **до** применения: диапазоны в границах,
/// обе границы — на границе UTF-8, операции не перекрываются. Любая невалидная →
/// `ai_apply_failed` (ни одна правка не применена).
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

/// Ошибка невалидной операции правки (`ai_apply_failed`).
fn apply_failed(op: &AiEditOp) -> CommandError {
    CommandError::new(
        ErrorCode::AiApplyFailed,
        format!("невалидная правка [{}, {})", op.start, op.end),
    )
}

/// Порядок уведомлений после успешного применения правки: сначала изменение документа
/// (плагины + фронт), затем `ai:edit_applied` (карточка диффа). Выделено отдельно ради
/// тестируемости порядка (ADR-0034).
pub(super) fn notify_applied(
    doc_id: &str,
    rev: u64,
    mut document_changed: impl FnMut(&str, u64),
    mut edit_applied: impl FnMut(&str, u64),
) {
    document_changed(doc_id, rev);
    edit_applied(doc_id, rev);
}

/// Эмитит фронту `document-updated { doc_id, rev }` — тот же контракт, что `DocumentSink`
/// плагинной подсистемы (см. `setup_plugins`): фронт по событию тянет снапшот и обновляет
/// `#editor`/`#preview`. Best-effort: сбой эмита не роняет команду.
pub(super) fn emit_document_updated(app: &AppHandle, doc_id: &str, rev: u64) {
    if let Err(e) = app.emit(
        "document-updated",
        serde_json::json!({ "doc_id": doc_id, "rev": rev }),
    ) {
        eprintln!("не эмитить document-updated: {e}");
    }
}
