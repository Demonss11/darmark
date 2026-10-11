//! Harness документа (TZ §5): минимальный дифф предложенного текста и его применение
//! через `DocumentStore` (D5, ADR-0028).
//!
//! Дифф считается **на Rust-стороне** (TS не парсит Markdown/текст): практичная контракция
//! по общему префиксу/суффиксу **строк** — изменённый блок становится одним [`AiEditOp`] с
//! байтовыми смещениями UTF-8. Применение защищено проверкой ревизии (rev-guard, §5.4):
//! устаревший `base_rev` → `ai_stale_edit`, правка не применяется.

use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, State};

use crate::ai::types::{AiDiff, AiEditOp};
use crate::error::{CommandError, ErrorCode};
use crate::state::{DocumentId, DocumentStore, UndoResult};

/// Считает минимальный дифф `original → modified` (байтовые смещения UTF-8).
///
/// Возвращает **один** [`AiEditOp`] на изменённый блок строк (или пустой список, если тексты
/// совпадают). Общий префикс/суффикс считается по строкам, поэтому смещения всегда лежат на
/// границах UTF-8 (символ не режется).
pub fn diff_ops(original: &str, modified: &str) -> Vec<AiEditOp> {
    if original == modified {
        return Vec::new();
    }
    let old_lines = line_spans(original);
    let new_lines = line_spans(modified);

    // Общий префикс по строкам.
    let mut prefix = 0;
    while prefix < old_lines.len()
        && prefix < new_lines.len()
        && line(original, old_lines[prefix]) == line(modified, new_lines[prefix])
    {
        prefix += 1;
    }

    // Общий суффикс по строкам, не заходя за префикс.
    let max_suffix = (old_lines.len() - prefix).min(new_lines.len() - prefix);
    let mut suffix = 0;
    while suffix < max_suffix
        && line(original, old_lines[old_lines.len() - 1 - suffix])
            == line(modified, new_lines[new_lines.len() - 1 - suffix])
    {
        suffix += 1;
    }

    let start = line_start(original, &old_lines, prefix);
    let end = if suffix == 0 {
        original.len()
    } else {
        old_lines[old_lines.len() - suffix].0
    };
    let new_start = line_start(modified, &new_lines, prefix);
    let new_end = if suffix == 0 {
        modified.len()
    } else {
        new_lines[new_lines.len() - suffix].0
    };

    vec![AiEditOp {
        start,
        end,
        text: modified[new_start..new_end].to_string(),
    }]
}

/// Байтовые границы строк `[start, end)` (end — за `\n`, если он есть).
///
/// Пустой текст даёт одну пустую строку; текст с завершающим `\n` — виртуальную пустую
/// строку в конце (симметрично для оригинала и правки — суффикс сходится корректно).
fn line_spans(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut spans = Vec::new();
    let mut start = 0;
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' {
            spans.push((start, index + 1));
            start = index + 1;
        }
    }
    spans.push((start, bytes.len()));
    spans
}

/// Начало строки с индексом `index` (или конец текста, если индекс за пределами).
fn line_start(text: &str, spans: &[(usize, usize)], index: usize) -> usize {
    if index < spans.len() {
        spans[index].0
    } else {
        text.len()
    }
}

/// Срез строки по её байтовым границам.
fn line(text: &str, span: (usize, usize)) -> &str {
    &text[span.0..span.1]
}

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
    let ops = diff_ops(&original, &proposed);
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
/// не применяется. При успехе эмитит `ai:edit_applied { doc_id, rev }`.
#[tauri::command]
pub fn ai_apply_edit(
    app: AppHandle,
    doc_id: String,
    base_rev: u64,
    ops: Vec<AiEditOp>,
    store: State<'_, Arc<Mutex<DocumentStore>>>,
) -> Result<UndoResult, CommandError> {
    let result = {
        let mut guard = lock_store(&store);
        let id = DocumentId::new(doc_id.clone());
        apply_with_guard(&mut guard, &id, base_rev, &ops)?
    };
    if let Err(e) = app.emit(
        "ai:edit_applied",
        serde_json::json!({ "doc_id": doc_id, "rev": result.rev }),
    ) {
        eprintln!("не эмитить ai:edit_applied: {e}");
    }
    Ok(result)
}

/// Rev-guard + применение операций: ядро команды [`ai_apply_edit`] (без Tauri — тестируемо).
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
/// Возвращает [`UndoResult`] по первой (нижней по смещению) операции — фронт по нему
/// выставляет выделение. Невалидные смещения `apply_edit` → `ai_apply_failed`.
fn apply_ops(
    store: &mut DocumentStore,
    id: &DocumentId,
    ops: &[AiEditOp],
) -> Result<UndoResult, CommandError> {
    let mut ordered: Vec<&AiEditOp> = ops.iter().collect();
    ordered.sort_by_key(|op| std::cmp::Reverse(op.start));
    for op in ordered {
        store
            .apply_edit(id, op.start, op.end, &op.text)
            .ok_or_else(|| {
                CommandError::new(
                    ErrorCode::AiApplyFailed,
                    format!("невалидная правка [{}, {})", op.start, op.end),
                )
            })?;
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

/// Блокировка стора без паники (под `panic = "abort"` отравление невозможно).
fn lock_store<'a>(
    store: &'a State<'_, Arc<Mutex<DocumentStore>>>,
) -> std::sync::MutexGuard<'a, DocumentStore> {
    store
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(original: &str, ops: &[AiEditOp]) -> String {
        // Применяем к строке теми же правилами, что и DocumentStore::apply_edit.
        let mut ordered: Vec<&AiEditOp> = ops.iter().collect();
        ordered.sort_by_key(|op| std::cmp::Reverse(op.start));
        let mut text = original.to_string();
        for op in ordered {
            text.replace_range(op.start..op.end, &op.text);
        }
        text
    }

    #[test]
    fn identical_texts_produce_empty_diff() {
        assert!(diff_ops("", "").is_empty());
        assert!(diff_ops("# A\nb", "# A\nb").is_empty());
    }

    #[test]
    fn middle_line_change_is_one_op() {
        let ops = diff_ops("a\nb\nc", "a\nX\nc");
        assert_eq!(ops.len(), 1);
        let op = &ops[0];
        assert_eq!((op.start, op.end), (2, 4), "заменяем строку «b\\n»");
        assert_eq!(op.text, "X\n");
        assert_eq!(apply("a\nb\nc", &ops), "a\nX\nc");
    }

    #[test]
    fn pure_append_and_prepend() {
        let appended = diff_ops("a\n", "a\nb\n");
        assert_eq!(apply("a\n", &appended), "a\nb\n");
        let prepended = diff_ops("b\n", "a\nb\n");
        assert_eq!(apply("b\n", &prepended), "a\nb\n");
    }

    #[test]
    fn full_replacement_is_single_op() {
        let ops = diff_ops("a\nb\n", "x\ny\n");
        assert_eq!(ops.len(), 1);
        assert_eq!((ops[0].start, ops[0].end), (0, 4));
        assert_eq!(apply("a\nb\n", &ops), "x\ny\n");
    }

    #[test]
    fn empty_to_text_and_back() {
        let ops = diff_ops("", "hello");
        assert_eq!(ops.len(), 1);
        assert_eq!((ops[0].start, ops[0].end), (0, 0));
        assert_eq!(ops[0].text, "hello");
        assert_eq!(apply("", &ops), "hello");

        let removed = diff_ops("hello", "");
        assert_eq!(apply("hello", &removed), "");
    }

    #[test]
    fn cyrillic_and_emoji_offsets_are_byte_correct() {
        // Кириллица (2 байта/символ) и эмодзи (4 байта) — смещения не должны резать символы.
        let original = "привет\nмир\n";
        let modified = "привет\n🌍 мир\n";
        let ops = diff_ops(original, modified);
        assert_eq!(ops.len(), 1);
        let op = &ops[0];
        assert_eq!(op.start, "привет\n".len(), "start — граница строки");
        assert!(original.is_char_boundary(op.start));
        assert!(original.is_char_boundary(op.end));
        assert_eq!(apply(original, &ops), modified);

        // Внутристрочная правка: строка целиком заменяется, смещения на границах.
        let ops = diff_ops("ёж\n", "ёк\n");
        assert_eq!(apply("ёж\n", &ops), "ёк\n");
        assert!(("ёж\n").is_char_boundary(ops[0].start));
    }

    #[test]
    fn every_op_boundary_is_char_boundary() {
        let original = "a\nёж\n🌍\nz";
        let modified = "a\nёк\n🌍!\nz";
        for op in diff_ops(original, modified) {
            assert!(original.is_char_boundary(op.start));
            assert!(original.is_char_boundary(op.end));
        }
        assert_eq!(apply(original, &diff_ops(original, modified)), modified);
    }

    #[test]
    fn apply_changes_text_and_bumps_rev() {
        let mut store = DocumentStore::default();
        let id = store.create("a\nb\nc".to_string()).id;
        let ops = diff_ops("a\nb\nc", "a\nX\nc");
        let result = apply_with_guard(&mut store, &id, 0, &ops).expect("правка применена");
        assert_eq!(result.text, "a\nX\nc");
        assert_eq!(result.rev, 1);
        assert_eq!(store.get(&id).map(|d| d.text.as_str()), Some("a\nX\nc"));
    }

    #[test]
    fn stale_base_rev_is_rejected_and_nothing_applied() {
        let mut store = DocumentStore::default();
        let id = store.create("a\nb\nc".to_string()).id;
        let ops = diff_ops("a\nb\nc", "a\nX\nc");
        // Пользователь успел отредактировать: rev ушла вперёд.
        store.apply_edit(&id, 0, 1, "A").unwrap();
        let err = apply_with_guard(&mut store, &id, 0, &ops).unwrap_err();
        assert_eq!(err.code, ErrorCode::AiStaleEdit);
        assert_eq!(
            store.get(&id).map(|d| d.text.as_str()),
            Some("A\nb\nc"),
            "устаревшая правка не применяется"
        );
    }

    #[test]
    fn unknown_document_is_reported() {
        let mut store = DocumentStore::default();
        let err = apply_with_guard(&mut store, &DocumentId::new("doc-999"), 0, &[]).unwrap_err();
        assert_eq!(err.code, ErrorCode::UnknownDocument);
    }
}
