//! Тесты harness'а документа [`super::super::doc_harness`].

use crate::ai::doc_harness::apply::notify_applied;
use crate::ai::doc_harness::{apply_with_guard, checked_diff_ops, diff_ops, MAX_DIFF_LINES};
use crate::ai::types::AiEditOp;
use crate::error::ErrorCode;
use crate::state::{DocumentId, DocumentStore};

fn apply(original: &str, ops: &[AiEditOp]) -> String {
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
    let original = "привет\nмир\n";
    let modified = "привет\n🌍 мир\n";
    let ops = diff_ops(original, modified);
    assert_eq!(ops.len(), 1);
    let op = &ops[0];
    assert_eq!(op.start, "привет\n".len(), "start — граница строки");
    assert!(original.is_char_boundary(op.start));
    assert!(original.is_char_boundary(op.end));
    assert_eq!(apply(original, &ops), modified);
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
    let original = "a\nb\nc\nd\ne\n";
    let modified = "a\nB\nc\nD\ne\n";
    let ops = diff_ops(original, modified);
    assert_eq!(ops.len(), 1, "один оп на изменённый блок");
    assert_eq!(apply(original, &ops), modified);
}

#[test]
fn oversized_diff_block_is_rejected() {
    let original = "head\n";
    let mut modified = String::from("head\n");
    for _ in 0..(MAX_DIFF_LINES + 1) {
        modified.push_str("x\n");
    }
    let err = checked_diff_ops(original, &modified).unwrap_err();
    assert_eq!(err.code, ErrorCode::AiBadRequest);
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
