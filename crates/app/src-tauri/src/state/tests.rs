use super::history::UNDO_LIMIT;
use super::*;

#[test]
fn creates_with_unique_ids_and_order() {
    let mut store = DocumentStore::default();
    let a = store.create(String::new());
    let b = store.create("текст".into());

    assert_ne!(a.id, b.id);
    assert_eq!(store.order.len(), 2);
    assert_eq!(store.get(&b.id).map(|d| d.text.as_str()), Some("текст"));
}

#[test]
fn loaded_document_knows_path_and_text() {
    let mut store = DocumentStore::default();
    let snap = store.insert_loaded(PathBuf::from("C:/tmp/note.md"), "hi".into());

    assert_eq!(snap.text, "hi");
    assert_eq!(snap.path.as_deref(), Some("C:/tmp/note.md"));
    assert_eq!(snap.rev, 0);
}

#[test]
fn close_removes_document_and_is_idempotent() {
    let mut store = DocumentStore::default();
    let snap = store.create("x".into());

    assert!(store.close(&snap.id));
    assert!(store.get(&snap.id).is_none());
    assert!(!store.close(&snap.id));
    assert!(store.order.is_empty());
}

#[test]
fn update_bumps_rev_only_on_text_change() {
    let mut store = DocumentStore::default();
    let snap = store.create("# A".into());

    let first = store.update(&snap.id, "# B".into(), false).unwrap();
    assert_eq!(first.rev, 1, "смена текста двигает ревизию");

    let same = store.update(&snap.id, "# B".into(), false).unwrap();
    assert_eq!(same.rev, 1, "тот же текст ревизию не двигает");
    assert!(!same.changed, "(mapped, rev) в кэше → DOM не трогаем");
}

#[test]
fn render_reports_changed_on_new_html_and_mapped_switch() {
    let mut store = DocumentStore::default();
    let snap = store.create("# A".into());

    let plain = store.render(&snap.id, false).unwrap();
    assert!(plain.changed, "первый рендер всегда changed");

    let repeat = store.render(&snap.id, false).unwrap();
    assert!(!repeat.changed, "повторный рендер того же (mapped, rev)");

    let mapped = store.render(&snap.id, true).unwrap();
    assert!(mapped.changed, "смена mapped меняет HTML");
    assert!(
        mapped.html.contains("data-md"),
        "mapped-рендер оборачивает блоки"
    );

    let back = store.render(&snap.id, false).unwrap();
    assert!(back.changed, "возврат к plain — снова другой HTML");
}

#[test]
fn last_id_tracks_most_recent_open() {
    let mut store = DocumentStore::default();
    assert!(store.last_id().is_none(), "пустой стор — нет активного");

    let first = store.create("a".into()).id;
    let second = store.create("b".into()).id;
    assert_eq!(store.last_id(), Some(second.clone()), "последний созданный");
    store.close(&second);
    assert_eq!(store.last_id(), Some(first));
}

#[test]
fn unknown_document_yields_none() {
    let mut store = DocumentStore::default();
    let id = store.create("x".into()).id;
    store.close(&id);

    assert!(store.update(&id, "y".into(), false).is_none());
    assert!(store.render(&id, false).is_none());
    assert!(store.apply_edit(&id, 0, 0, "z").is_none());
}

#[test]
fn apply_edit_splices_and_bumps_rev() {
    let mut store = DocumentStore::default();
    let id = store.create("hello world".into()).id;
    assert_eq!(store.apply_edit(&id, 6, 11, "there"), Some(1));
    assert_eq!(store.get(&id).unwrap().text, "hello there");
    assert_eq!(store.apply_edit(&id, 6, 11, "there"), Some(1));
    assert_eq!(store.apply_edit(&id, 0, 0, ">> "), Some(2));
    assert_eq!(store.get(&id).unwrap().text, ">> hello there");
}

#[test]
fn apply_edit_rejects_invalid_offsets() {
    let mut store = DocumentStore::default();
    let id = store.create("ё".into()).id;

    assert!(
        store.apply_edit(&id, 1, 2, "x").is_none(),
        "середина символа"
    );
    assert!(
        store.apply_edit(&id, 5, 6, "x").is_none(),
        "за пределами текста"
    );
    assert!(store.apply_edit(&id, 2, 1, "x").is_none(), "start > stop");
    assert_eq!(store.apply_edit(&id, 0, 2, "e"), Some(1));
}

#[test]
fn list_returns_all_documents_in_open_order() {
    let mut store = DocumentStore::default();
    let a = store.create("a".into()).id;
    let b = store.create("b".into()).id;
    let c = store.create("c".into()).id;

    let list = store.list();
    assert_eq!(list.len(), 3);
    assert_eq!(list[0].id, a);
    assert_eq!(list[1].id, b);
    assert_eq!(list[2].id, c);
}

#[test]
fn list_excludes_closed_documents() {
    let mut store = DocumentStore::default();
    let a = store.create("a".into()).id;
    let b = store.create("b".into()).id;
    store.close(&a);

    let list = store.list();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, b);
}

#[test]
fn activate_sets_active_id() {
    let mut store = DocumentStore::default();
    let a = store.create("a".into()).id;
    let b = store.create("b".into()).id;

    assert!(store.activate(&a));
    assert_eq!(store.active_id(), Some(&a));

    assert!(store.activate(&b));
    assert_eq!(store.active_id(), Some(&b));
}

#[test]
fn activate_rejects_unknown_id() {
    let mut store = DocumentStore::default();
    let a = store.create("a".into()).id;
    let unknown = DocumentId::new("doc-999");

    assert!(!store.activate(&unknown));
    assert_eq!(store.active_id(), None, "не сбрасываем активный при ошибке");
    assert!(store.activate(&a));
}

#[test]
fn close_resets_active_id() {
    let mut store = DocumentStore::default();
    let a = store.create("a".into()).id;
    store.activate(&a);

    store.close(&a);
    assert_eq!(store.active_id(), None);
}

#[test]
fn update_records_minimal_op_and_undo_redo_roundtrip() {
    let mut store = DocumentStore::default();
    let id = store.create("# A".into()).id;

    store.update(&id, "# B".into(), false).unwrap();
    {
        let doc = store.get(&id).unwrap();
        assert_eq!(doc.undo.len(), 1, "реальная правка кладёт один оп");
        let op = &doc.undo[0];
        assert_eq!(op.start, 2, "общий префикс «# » — 2 байта");
        assert_eq!(op.removed, "A");
        assert_eq!(op.inserted, "B");
    }

    let undone = store.undo(&id).unwrap();
    assert_eq!(undone.text, "# A");
    assert_eq!(
        (undone.start, undone.end),
        (2, 3),
        "выделение результата undo"
    );
    assert_eq!(store.get(&id).unwrap().text, "# A");

    let redone = store.redo(&id).unwrap();
    assert_eq!(redone.text, "# B");
    assert_eq!(
        (redone.start, redone.end),
        (2, 3),
        "выделение результата redo"
    );
}

#[test]
fn apply_edit_records_exact_op_and_undo_redo_roundtrip() {
    let mut store = DocumentStore::default();
    let id = store.create("hello world".into()).id;

    assert_eq!(store.apply_edit(&id, 6, 11, "there"), Some(1));
    {
        let op = &store.get(&id).unwrap().undo[0];
        assert_eq!(op.start, 6);
        assert_eq!(op.removed, "world");
        assert_eq!(op.inserted, "there");
    }

    let undone = store.undo(&id).unwrap();
    assert_eq!(undone.text, "hello world");
    assert_eq!((undone.start, undone.end), (6, 11));

    let redone = store.redo(&id).unwrap();
    assert_eq!(redone.text, "hello there");
    assert_eq!((redone.start, redone.end), (6, 11));
}

#[test]
fn undo_is_lifo_and_new_edit_clears_redo() {
    let mut store = DocumentStore::default();
    let id = store.create("a".into()).id;

    store.update(&id, "ab".into(), false).unwrap();
    store.update(&id, "abc".into(), false).unwrap();

    assert_eq!(
        store.undo(&id).unwrap().text,
        "ab",
        "сначала последняя правка"
    );
    assert_eq!(store.undo(&id).unwrap().text, "a", "затем предыдущая");
    assert_eq!(store.get(&id).unwrap().text, "a");
    store.update(&id, "aX".into(), false).unwrap();
    assert!(store.redo(&id).is_none(), "новая правка сбрасывает redo");
}

#[test]
fn diff_does_not_split_multibyte_prefix() {
    let mut store = DocumentStore::default();
    let id = store.create("ёж".into()).id;
    store.update(&id, "ёк".into(), false).unwrap();

    let op = &store.get(&id).unwrap().undo[0];
    assert_eq!(op.start, 2, "граница префикса не режет «ё»");
    assert_eq!(op.removed, "ж");
    assert_eq!(op.inserted, "к");
    assert_eq!(store.undo(&id).unwrap().text, "ёж");
    assert_eq!(store.redo(&id).unwrap().text, "ёк");
}

#[test]
fn diff_suffix_alignment_keeps_utf8_boundaries() {
    let mut store = DocumentStore::default();
    let id = store.create("aé".into()).id;
    store.update(&id, "aө".into(), false).unwrap();

    let op = &store.get(&id).unwrap().undo[0];
    assert_eq!(op.start, 1);
    assert_eq!(op.removed, "é");
    assert_eq!(op.inserted, "ө");
    assert_eq!(store.undo(&id).unwrap().text, "aé");
    assert_eq!(store.redo(&id).unwrap().text, "aө");
}

#[test]
fn empty_stacks_yield_none_without_bumping_rev() {
    let mut store = DocumentStore::default();
    let id = store.create("x".into()).id;

    assert!(store.undo(&id).is_none());
    assert!(store.redo(&id).is_none());
    assert_eq!(
        store.get(&id).unwrap().rev,
        0,
        "пустые стеки ревизию не двигают"
    );
}

#[test]
fn unknown_document_undo_redo_yield_none() {
    let mut store = DocumentStore::default();
    let id = store.create("x".into()).id;
    store.close(&id);

    assert!(store.undo(&id).is_none());
    assert!(store.redo(&id).is_none());
}

#[test]
fn undo_limit_drops_oldest() {
    let mut store = DocumentStore::default();
    let id = store.create(String::new()).id;

    for i in 0..(UNDO_LIMIT + 10) {
        store.update(&id, format!("{i}"), false).unwrap();
    }
    assert_eq!(
        store.get(&id).unwrap().undo.len(),
        UNDO_LIMIT,
        "стек ограничен UNDO_LIMIT"
    );
    let mut last = String::new();
    while let Some(result) = store.undo(&id) {
        last = result.text;
    }
    assert_eq!(last, "9", "самые старые оп'ы отброшены");
}

#[test]
fn rev_grows_on_undo_redo_and_echo_update_is_noop() {
    let mut store = DocumentStore::default();
    let id = store.create("a".into()).id;

    store.update(&id, "b".into(), false).unwrap();
    let rev_after_update = store.get(&id).unwrap().rev;

    let undone = store.undo(&id).unwrap();
    assert!(undone.rev > rev_after_update, "undo двигает ревизию");
    let redone = store.redo(&id).unwrap();
    assert!(redone.rev > undone.rev, "redo двигает ревизию");
    let before = store.get(&id).unwrap();
    let (ops_before, rev_before) = (before.undo.len(), before.rev);
    store.update(&id, "b".into(), false).unwrap();
    let after = store.get(&id).unwrap();
    assert_eq!(after.undo.len(), ops_before, "эхо не плодит оп");
    assert_eq!(after.rev, rev_before, "эхо не двигает ревизию");
}
