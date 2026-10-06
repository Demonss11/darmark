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
}

/// Операция правки для undo/redo-стека (D5). Владелец стека — документ.
///
/// Поля закладываются в Фазе 1, стек наполняется вместе с `update_document` (Фаза 2).
#[allow(dead_code)] // поля начнёт читать update_document в Фазе 2
#[derive(Clone, Debug)]
pub struct EditOp {
    pub rev: u64,
    /// Диапазон в **байтовых** смещениях UTF-8 (как `data-md` в `md-core`).
    pub range: (usize, usize),
    pub text: String,
}

/// Кэш HTML. Ключ — `(mapped, rev)`, значение — готовый HTML.
///
/// `RenderResult.changed = false`, если запрошенная ревизия уже в кэше: TS тогда не трогает DOM.
#[allow(dead_code)] // кэш начнёт читать update_document/render_document в Фазе 2
#[derive(Clone, Debug)]
pub struct RenderCache {
    pub mapped: bool,
    pub rev: u64,
    pub html: String,
}

/// Документ: текст, ревизия, путь, кэш рендера и undo/redo.
#[allow(dead_code)] // cached/undo/redo наполняются в Фазах 2–3
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
}

/// Хранилище открытых документов. Порядок (`order`) сохраняется под будущие вкладки.
#[derive(Default)]
pub struct DocumentStore {
    docs: HashMap<DocumentId, Document>,
    order: Vec<DocumentId>,
    next_id: u64,
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

    /// Закрывает документ. Возвращает `false`, если такого id нет.
    pub fn close(&mut self, id: &DocumentId) -> bool {
        if self.docs.remove(id).is_some() {
            self.order.retain(|x| x != id);
            true
        } else {
            false
        }
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
}
