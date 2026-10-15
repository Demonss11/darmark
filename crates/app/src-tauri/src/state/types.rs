//! IPC-контракты состояния документов: структуры с публичными полями.
//!
//! Приватных полей здесь нет намеренно — поэтому модуль ничего не разделяет с
//! соседями и может стоять в любом месте иерархии `state`.

use serde::Serialize;

use super::DocumentId;

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

/// Кэш HTML. Ключ — `(mapped, rev)`, значение — готовый HTML.
///
/// `RenderResult.changed = false`, если запрошенная ревизия уже в кэше: TS тогда не трогает DOM.
#[derive(Clone, Debug)]
pub struct RenderCache {
    pub mapped: bool,
    pub rev: u64,
    pub html: String,
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
