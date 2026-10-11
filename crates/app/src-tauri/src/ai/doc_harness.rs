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

/// Верхний предел числа строк изменённого блока (§10.1): защита от гигантских правок.
pub const MAX_DIFF_LINES: usize = 10_000;

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

/// Дифф с проверкой лимита размера (§10.1): изменённый блок больше [`MAX_DIFF_LINES`]
/// строк → `ai_bad_request`. Дифф считается всегда **одним** [`AiEditOp`], поэтому
/// проверяется единственный блок (заменённый фрагмент и его новая версия).
pub fn checked_diff_ops(original: &str, modified: &str) -> Result<Vec<AiEditOp>, CommandError> {
    let ops = diff_ops(original, modified);
    if let Some(op) = ops.first() {
        let added = op.text.lines().count();
        let removed = original[op.start..op.end].lines().count();
        if added.max(removed) > MAX_DIFF_LINES {
            return Err(CommandError::new(
                ErrorCode::AiBadRequest,
                format!("изменённый блок больше {MAX_DIFF_LINES} строк"),
            ));
        }
    }
    Ok(ops)
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
    // Guard стора отпущен: доставка `document:changed` плагинам может обслужить host-call
    // к тому же `DocumentStore` (нереентрантный мьютекс → дедлок при удержании лока).
    // Порядок важен: сначала изменение документа (плагины + фронт), затем `ai:edit_applied`.
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

/// Порядок уведомлений после успешного применения правки: сначала изменение документа
/// (плагины + фронт), затем `ai:edit_applied` (карточка диффа). Выделено отдельно ради
/// тестируемости порядка (TZ §5.1).
fn notify_applied(
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
fn emit_document_updated(app: &AppHandle, doc_id: &str, rev: u64) {
    if let Err(e) = app.emit(
        "document-updated",
        serde_json::json!({ "doc_id": doc_id, "rev": rev }),
    ) {
        eprintln!("не эмитить document-updated: {e}");
    }
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
/// **Атомарность (§7):** сначала **все** операции проверяются против базового текста
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
        // Перекрытие (после сортировки по `start`): начало раньше конца предыдущей.
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

    #[test]
    fn disjoint_changes_still_yield_single_op() {
        // Дифф сознательно строится как **один** оп по общему префиксу/суффиксу строк:
        // две раздельные правки схлопываются в один блок (документированное поведение).
        let original = "a\nb\nc\nd\ne\n";
        let modified = "a\nB\nc\nD\ne\n";
        let ops = diff_ops(original, modified);
        assert_eq!(ops.len(), 1, "один оп на изменённый блок");
        assert_eq!(apply(original, &ops), modified);
    }

    #[test]
    fn oversized_diff_block_is_rejected() {
        // Изменённый блок длиннее MAX_DIFF_LINES строк → ai_bad_request (§10.1).
        let original = "head\n";
        let mut modified = String::from("head\n");
        for _ in 0..(MAX_DIFF_LINES + 1) {
            modified.push_str("x\n");
        }
        let err = checked_diff_ops(original, &modified).unwrap_err();
        assert_eq!(err.code, ErrorCode::AiBadRequest);

        // Ровно на границе — ещё допустимо.
        let mut ok = String::from("head\n");
        for _ in 0..MAX_DIFF_LINES {
            ok.push_str("x\n");
        }
        assert!(checked_diff_ops(original, &ok).is_ok());
    }

    #[test]
    fn invalid_op_aborts_without_partial_changes() {
        let mut store = DocumentStore::default();
        let id = store.create("hello world".into()).id;
        // Первая (валидная) правка + вторая вне границ: ничего не применяется.
        let ops = vec![
            AiEditOp {
                start: 0,
                end: 5,
                text: "HELLO".into(),
            },
            AiEditOp {
                start: 50,
                end: 51,
                text: "!".into(),
            },
        ];
        let err = apply_with_guard(&mut store, &id, 0, &ops).unwrap_err();
        assert_eq!(err.code, ErrorCode::AiApplyFailed);
        assert_eq!(
            store.get(&id).map(|d| d.text.as_str()),
            Some("hello world"),
            "частичных изменений быть не должно"
        );
    }

    #[test]
    fn overlapping_ops_are_rejected() {
        let mut store = DocumentStore::default();
        let id = store.create("abcdef".into()).id;
        let ops = vec![
            AiEditOp {
                start: 0,
                end: 4,
                text: "X".into(),
            },
            AiEditOp {
                start: 2,
                end: 6,
                text: "Y".into(),
            },
        ];
        let err = apply_with_guard(&mut store, &id, 0, &ops).unwrap_err();
        assert_eq!(err.code, ErrorCode::AiApplyFailed);
        assert_eq!(store.get(&id).map(|d| d.text.as_str()), Some("abcdef"));
    }

    #[test]
    fn notifications_are_document_then_applied() {
        use std::cell::RefCell;
        let calls = RefCell::new(Vec::new());
        notify_applied(
            "doc-1",
            7,
            |id, rev| calls.borrow_mut().push(format!("document:{id}:{rev}")),
            |id, rev| calls.borrow_mut().push(format!("applied:{id}:{rev}")),
        );
        assert_eq!(
            calls.into_inner(),
            vec![
                "document:doc-1:7".to_string(),
                "applied:doc-1:7".to_string()
            ],
            "сначала document:changed/updated, затем ai:edit_applied"
        );
    }
}
