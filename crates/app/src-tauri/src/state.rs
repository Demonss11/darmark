//! Состояние документов (D5): хост владеет текстом, ревизией, путём и кэшем рендера.
//!
//! Модуль живёт в Tauri-шелле, а не в `md-core`: ядро обязано оставаться чистым
//! (`&str -> String`) и переиспользуемым в CLI/тестах без GUI-состояния.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Идентификатор документа. Строка, а не число: единый формат с TS и Lua-плагинами
/// (у плагинов — строковые ключи таблиц), плюс генерацию id контролирует хост.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DocumentId(String);

impl DocumentId {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Конструирует id из строки: плагины адресуют документ строкой в host-call'ах.
    /// Несуществующий id просто не найдётся в сторе.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

/// Операция правки для undo/redo-стека (D5). Владелец стека — документ.
///
/// Смещения — **байтовые** UTF-8 (как `data-md` в `md-core`): `removed` — прежнее
/// содержимое `[start, start+removed.len())`, `inserted` — пришедшее ему на смену.
/// Undo меняет `inserted` обратно на `removed`, redo — наоборот.
#[derive(Clone, Debug)]
pub struct EditOp {
    pub start: usize,
    pub removed: String,
    pub inserted: String,
}

/// Потолок undo/redo-стека: при переполнении отбрасывается самый старый оп.
///
/// Ограничивает память: редактор шлёт правку на каждое нажатие, а каждый оп держит
/// две строки. 500 шагов с запасом покрывают типичную сессию редактирования.
const UNDO_LIMIT: usize = 500;

/// Кладёт оп в стек, отбрасывая самый старый при переполнении [`UNDO_LIMIT`].
///
/// Применяется к обоим стекам сообразно: `undo` растёт на новых правках, `redo` —
/// на отменах; каждый держится в границах лимита.
fn push_op(stack: &mut Vec<EditOp>, op: EditOp) {
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
fn minimal_diff(old: &str, new: &str) -> EditOp {
    let old_bytes = old.as_bytes();
    let new_bytes = new.as_bytes();

    // Общий префикс по байтам, затем выравнивание вниз до границы символа.
    let max_prefix = old_bytes.len().min(new_bytes.len());
    let mut start = 0;
    while start < max_prefix && old_bytes[start] == new_bytes[start] {
        start += 1;
    }
    while start > 0 && !old.is_char_boundary(start) {
        start -= 1;
    }

    // Общий суффикс по байтам, не заходя за отрезанный префикс, затем выравнивание.
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

/// Кэш HTML. Ключ — `(mapped, rev)`, значение — готовый HTML.
///
/// `RenderResult.changed = false`, если запрошенная ревизия уже в кэше: TS тогда не трогает DOM.
#[derive(Clone, Debug)]
pub struct RenderCache {
    pub mapped: bool,
    pub rev: u64,
    pub html: String,
}

/// Документ: текст, ревизия, путь, кэш рендера и undo/redo.
pub struct Document {
    pub id: DocumentId,
    pub path: Option<PathBuf>,
    pub text: String,
    pub rev: u64,
    pub cached: Option<RenderCache>,
    pub undo: Vec<EditOp>,
    pub redo: Vec<EditOp>,
}

/// Проекция документа для фронтенда: надмножество [`DocMeta`] с текстом.
///
/// Возвращается `new_document`/`open_document`; именно её принимает
/// `ViewContext.document()` (§5.4) — вид получает текст и ревизию одной структурой.
#[derive(Debug, Serialize)]
pub struct DocumentSnapshot {
    pub id: DocumentId,
    pub path: Option<String>,
    pub rev: u64,
    pub text: String,
    pub dirty_hint: bool,
}

/// Метаданные документа без текста — ответ `save_document`.
#[derive(Debug, Serialize)]
pub struct DocMeta {
    pub id: DocumentId,
    pub path: Option<String>,
    pub rev: u64,
    pub dirty_hint: bool,
}

/// Результат рендера: готовый HTML, ревизия и признак «HTML изменился с прошлого раза».
///
/// `changed: false` — TS не трогает DOM (переезд `lastRenderedHtml` из фронтенда в хост).
#[derive(Debug, Serialize)]
pub struct RenderResult {
    pub html: String,
    pub rev: u64,
    pub changed: bool,
}

/// Результат undo/redo: текст после операции и байтовый диапазон изменения.
///
/// `start`/`end` — границы изменённого фрагмента в **байтовых** смещениях UTF-8:
/// при undo это восстановленный `removed`, при redo — применённый `inserted`.
/// Фронт по ним выставляет выделение/каретку в редакторе.
#[derive(Debug, Serialize)]
pub struct UndoResult {
    pub text: String,
    pub rev: u64,
    pub start: usize,
    pub end: usize,
}

impl Document {
    pub(crate) fn snapshot(&self) -> DocumentSnapshot {
        DocumentSnapshot {
            id: self.id.clone(),
            path: self.path_string(),
            rev: self.rev,
            text: self.text.clone(),
            // Владелец dirty — TS (§4.2); заглушка до Фазы 3.
            dirty_hint: false,
        }
    }

    pub(crate) fn meta(&self) -> DocMeta {
        DocMeta {
            id: self.id.clone(),
            path: self.path_string(),
            rev: self.rev,
            dirty_hint: false,
        }
    }

    fn path_string(&self) -> Option<String> {
        self.path.as_ref().map(|p| p.to_string_lossy().into_owned())
    }

    /// Рендерит документ через `md-core` и обновляет кэш.
    ///
    /// Если `(mapped, rev)` уже в кэше — рендер не запускается, `changed: false`.
    /// Иначе `changed` = отличается ли HTML от последнего отданного (переезд
    /// `lastRenderedHtml` из TS в хост).
    pub(crate) fn render(&mut self, mapped: bool) -> RenderResult {
        if let Some(cached) = &self.cached {
            if cached.mapped == mapped && cached.rev == self.rev {
                return RenderResult {
                    html: cached.html.clone(),
                    rev: self.rev,
                    changed: false,
                };
            }
        }
        let html = if mapped {
            md_core::to_html_mapped(&self.text)
        } else {
            md_core::to_html(&self.text)
        };
        let changed = match &self.cached {
            Some(cached) => cached.html != html,
            None => true,
        };
        self.cached = Some(RenderCache {
            mapped,
            rev: self.rev,
            html: html.clone(),
        });
        RenderResult {
            html,
            rev: self.rev,
            changed,
        }
    }
}

/// Хранилище открытых документов. Порядок (`order`) сохраняется под будущие вкладки.
#[derive(Default)]
pub struct DocumentStore {
    docs: HashMap<DocumentId, Document>,
    order: Vec<DocumentId>,
    next_id: u64,
    /// Активный документ (для мультивкладок, п.11.1.1). Пока дублирует состояние
    /// на TS-стороне, но контракт закладывается на будущее.
    active_id: Option<DocumentId>,
}

impl DocumentStore {
    fn fresh_id(&mut self) -> DocumentId {
        self.next_id += 1;
        DocumentId(format!("doc-{}", self.next_id))
    }

    /// Создаёт безымянный документ с заданным текстом; возвращает снимок.
    pub fn create(&mut self, text: String) -> DocumentSnapshot {
        self.insert(None, text)
    }

    /// Создаёт документ, привязанный к файлу на диске; возвращает снимок.
    pub fn insert_loaded(&mut self, path: PathBuf, text: String) -> DocumentSnapshot {
        self.insert(Some(path), text)
    }

    fn insert(&mut self, path: Option<PathBuf>, text: String) -> DocumentSnapshot {
        let id = self.fresh_id();
        let doc = Document {
            id: id.clone(),
            path,
            text,
            rev: 0,
            cached: None,
            undo: Vec::new(),
            redo: Vec::new(),
        };
        let snapshot = doc.snapshot();
        self.order.push(id.clone());
        self.docs.insert(id, doc);
        snapshot
    }

    pub fn get(&self, id: &DocumentId) -> Option<&Document> {
        self.docs.get(id)
    }

    pub fn get_mut(&mut self, id: &DocumentId) -> Option<&mut Document> {
        self.docs.get_mut(id)
    }

    /// Применяет правку текста и рендерит. `rev` растёт только при реальной смене
    /// текста, поэтому повторный вызов с тем же текстом даёт `changed: false`.
    ///
    /// Реальная правка кладёт **минимальный** оп в `undo` (D5) и очищает `redo`:
    /// новая ветка истории делает прежнее «вперёд» недостижимым.
    pub fn update(&mut self, id: &DocumentId, text: String, mapped: bool) -> Option<RenderResult> {
        let doc = self.docs.get_mut(id)?;
        if doc.text != text {
            let op = minimal_diff(&doc.text, &text);
            push_op(&mut doc.undo, op);
            doc.redo.clear();
            doc.text = text;
            doc.rev += 1;
        }
        Some(doc.render(mapped))
    }

    /// Рендерит без правки текста (смена `mapped`, первый рендер после open/new).
    pub fn render(&mut self, id: &DocumentId, mapped: bool) -> Option<RenderResult> {
        Some(self.docs.get_mut(id)?.render(mapped))
    }

    /// id последнего открытого/созданного документа (UI H1 держит один активный).
    ///
    /// Плагины не имеют собственного «активного» документа: команда `command:invoked`
    /// несёт его с собой (Фаза 5). `None`, если документов нет.
    pub fn last_id(&self) -> Option<DocumentId> {
        self.order.last().cloned()
    }

    /// Закрывает документ. Возвращает `false`, если такого id нет.
    pub fn close(&mut self, id: &DocumentId) -> bool {
        if self.docs.remove(id).is_some() {
            self.order.retain(|x| x != id);
            // Если закрыли активный документ — сбрасываем указатель.
            if self.active_id.as_ref() == Some(id) {
                self.active_id = None;
            }
            true
        } else {
            false
        }
    }

    /// Возвращает метаданные всех документов в порядке открытия.
    ///
    /// Нужен для мультивкладок (п.11.1.1): фронт получает список вкладок
    /// без необходимости отслеживать порядок на своей стороне.
    pub fn list(&self) -> Vec<DocMeta> {
        self.order
            .iter()
            .filter_map(|id| self.docs.get(id).map(|doc| doc.meta()))
            .collect()
    }

    /// Устанавливает активный документ (для мультивкладок, п.11.1.1).
    ///
    /// Возвращает `true`, если документ найден и установлен активным.
    /// Пока активный документ также хранится на TS-стороне — эта функция
    /// закладывает контракт для будущего перехода состояния в Rust.
    pub fn activate(&mut self, id: &DocumentId) -> bool {
        if self.docs.contains_key(id) {
            self.active_id = Some(id.clone());
            true
        } else {
            false
        }
    }

    /// Возвращает id активного документа, если он установлен.
    pub fn active_id(&self) -> Option<&DocumentId> {
        self.active_id.as_ref()
    }

    /// Правка диапазона в **байтовых** смещениях UTF-8: заменяет `[start, stop)` на `text`.
    ///
    /// Единый путь изменений (D5): `rev` растёт только при реальной смене текста, поэтому
    /// эхо-правка тем же текстом не зацикливает подписчиков (`document:changed`, §7).
    /// `None` — документ не найден или смещения невалидны (не на границе символа / вне текста).
    pub fn apply_edit(
        &mut self,
        id: &DocumentId,
        start: usize,
        stop: usize,
        text: &str,
    ) -> Option<u64> {
        let doc = self.docs.get_mut(id)?;
        if start > stop || stop > doc.text.len() {
            return None;
        }
        if !doc.text.is_char_boundary(start) || !doc.text.is_char_boundary(stop) {
            return None;
        }
        if start == stop && text.is_empty() {
            return Some(doc.rev);
        }
        let mut new_text = String::with_capacity(doc.text.len() - (stop - start) + text.len());
        new_text.push_str(&doc.text[..start]);
        new_text.push_str(text);
        new_text.push_str(&doc.text[stop..]);
        if doc.text != new_text {
            // Точный оп (не минимальный diff): диапазон задан вызывающей стороной явно.
            let op = EditOp {
                start,
                removed: doc.text[start..stop].to_string(),
                inserted: text.to_string(),
            };
            push_op(&mut doc.undo, op);
            doc.redo.clear();
            doc.text = new_text;
            doc.rev += 1;
        }
        Some(doc.rev)
    }

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

#[cfg(test)]
mod tests {
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

        // Закрытие последнего возвращает предыдущий открытый.
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

        // Заменяем "world" (байты 6..11) на "there".
        assert_eq!(store.apply_edit(&id, 6, 11, "there"), Some(1));
        assert_eq!(store.get(&id).unwrap().text, "hello there");

        // Правка тем же текстом ревизию не двигает (эхо-защита).
        assert_eq!(store.apply_edit(&id, 6, 11, "there"), Some(1));

        // Вставка без удаления.
        assert_eq!(store.apply_edit(&id, 0, 0, ">> "), Some(2));
        assert_eq!(store.get(&id).unwrap().text, ">> hello there");
    }

    #[test]
    fn apply_edit_rejects_invalid_offsets() {
        let mut store = DocumentStore::default();
        // "ё" — двухбайтовый символ: смещение в 1 не является границей символа.
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
        // Корректные границы проходят.
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

        // Активный документ можно установить после неудачной попытки.
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

    // ---------- undo/redo (D5) ----------

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

        // Новая правка после отката делает «вперёд» недостижимым.
        store.update(&id, "aX".into(), false).unwrap();
        assert!(store.redo(&id).is_none(), "новая правка сбрасывает redo");
    }

    #[test]
    fn diff_does_not_split_multibyte_prefix() {
        let mut store = DocumentStore::default();
        // «ёж» → «ёк»: различается только «ж»/«к» (по 2 байта), префикс «ё» — 2 байта.
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
        // «é» (C3 A9) и «ө» (D3 A9): общий суффикс-байт A9 лежит в середине символа —
        // без выравнивания вниз срез порвал бы UTF-8 и запаниковал.
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

        // Откатываем весь стек: самые старые оп'ы отброшены, поэтому undo
        // останавливается на состоянии после 10-й правки (текст «9»), а не на пустом.
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

        // Эхо-правка (тот же текст) не плодит оп и не двигает ревизию.
        let before = store.get(&id).unwrap();
        let (ops_before, rev_before) = (before.undo.len(), before.rev);
        store.update(&id, "b".into(), false).unwrap();
        let after = store.get(&id).unwrap();
        assert_eq!(after.undo.len(), ops_before, "эхо не плодит оп");
        assert_eq!(after.rev, rev_before, "эхо не двигает ревизию");
    }
}
