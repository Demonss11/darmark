//! Undo/redo-стек (D5): потолок стека, раскладка минимальной правки и сами операции.

use super::{DocumentId, DocumentStore, EditOp, UndoResult};

/// Потолок undo/redo-стека: при переполнении отбрасывается самый старый оп.
///
/// Ограничивает память: редактор шлёт правку на каждое нажатие, а каждый оп держит
/// две строки. 500 шагов с запасом покрывают типичную сессию редактирования.
pub(super) const UNDO_LIMIT: usize = 500;

/// Кладёт оп в стек, отбрасывая самый старый при переполнении [`UNDO_LIMIT`].
///
/// Применяется к обоим стекам сообразно: `undo` растёт на новых правках, `redo` —
/// на отменах; каждый держится в границах лимита.
pub(super) fn push_op(stack: &mut Vec<EditOp>, op: EditOp) {
    stack.push(op);
    if stack.len() > UNDO_LIMIT {
        stack.remove(0);
    }
}

/// Минимальная правка `old → new`: общий префикс и общий суффикс по байтам.
///
/// Границы усекаются **вниз до границ UTF-8**: общий префикс/суффикс может разрезать
/// многочленный символ (совпавшие байты внутри разных символов), и без выравнивания
/// срез `&str` запаниковал бы. Возвращает оп с байтовыми `start`, `removed`, `inserted`.
pub(super) fn minimal_diff(old: &str, new: &str) -> EditOp {
    let old_bytes = old.as_bytes();
    let new_bytes = new.as_bytes();
    let max_prefix = old_bytes.len().min(new_bytes.len());
    let mut start = 0;
    while start < max_prefix && old_bytes[start] == new_bytes[start] {
        start += 1;
    }
    while start > 0 && !old.is_char_boundary(start) {
        start -= 1;
    }
    let mut suffix = 0;
    while suffix < old_bytes.len() - start
        && suffix < new_bytes.len() - start
        && old_bytes[old_bytes.len() - 1 - suffix] == new_bytes[new_bytes.len() - 1 - suffix]
    {
        suffix += 1;
    }
    while suffix > 0
        && (!old.is_char_boundary(old_bytes.len() - suffix)
            || !new.is_char_boundary(new_bytes.len() - suffix))
    {
        suffix -= 1;
    }

    EditOp {
        start,
        removed: old[start..old_bytes.len() - suffix].to_string(),
        inserted: new[start..new_bytes.len() - suffix].to_string(),
    }
}

impl DocumentStore {
    /// Отменяет последнюю правку (LIFO). `None` — документа нет или нечего отменять.
    ///
    /// Меняет `[start, start+inserted.len())` обратно на `removed`, переносит оп в
    /// `redo`, растит `rev` (кэш рендера инвалидируется по ревизии). Возвращает текст
    /// и байтовый диапазон восстановленного фрагмента.
    pub fn undo(&mut self, id: &DocumentId) -> Option<UndoResult> {
        let doc = self.docs.get_mut(id)?;
        let op = doc.undo.pop()?;
        let mut new_text = String::with_capacity(doc.text.len() + op.removed.len());
        new_text.push_str(&doc.text[..op.start]);
        new_text.push_str(&op.removed);
        new_text.push_str(&doc.text[op.start + op.inserted.len()..]);
        doc.text = new_text;
        doc.rev += 1;
        let end = op.start + op.removed.len();
        let result = UndoResult {
            text: doc.text.clone(),
            rev: doc.rev,
            start: op.start,
            end,
        };
        push_op(&mut doc.redo, op);
        Some(result)
    }

    /// Повторяет отменённую правку (LIFO). `None` — документа нет или нечего повторять.
    ///
    /// Меняет `[start, start+removed.len())` на `inserted`, переносит оп назад в
    /// `undo`, растит `rev`. Возвращает текст и байтовый диапазон применённого фрагмента.
    pub fn redo(&mut self, id: &DocumentId) -> Option<UndoResult> {
        let doc = self.docs.get_mut(id)?;
        let op = doc.redo.pop()?;
        let mut new_text = String::with_capacity(doc.text.len() + op.inserted.len());
        new_text.push_str(&doc.text[..op.start]);
        new_text.push_str(&op.inserted);
        new_text.push_str(&doc.text[op.start + op.removed.len()..]);
        doc.text = new_text;
        doc.rev += 1;
        let end = op.start + op.inserted.len();
        let result = UndoResult {
            text: doc.text.clone(),
            rev: doc.rev,
            start: op.start,
            end,
        };
        push_op(&mut doc.undo, op);
        Some(result)
    }
}
