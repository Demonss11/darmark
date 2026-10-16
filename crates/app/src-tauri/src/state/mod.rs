//! Состояние документов (D5): хост владеет текстом, ревизией, путём и кэшем рендера.
//!
//! Модуль живёт в Tauri-шелле, а не в `md-core`: ядро обязано оставаться чистым
//! (`&str -> String`) и переиспользуемым в CLI/тестах без GUI-состояния.
//!
//! Модуль разбит на подмодули: [`types`] — IPC-контракты (структуры с публичными
//! полями, без приватных), [`store`] — операции над документами, [`history`] —
//! undo/redo-стек. Объявления [`DocumentId`], [`Document`] и [`DocumentStore`]
//! остались здесь: приватные поля видны подмодулям только от модуля объявления,
//! а читают их все три подмодуля плюс тесты.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

mod history;
mod store;
mod types;

#[cfg(test)]
mod tests;

pub use types::{DocMeta, DocumentSnapshot, EditOp, RenderCache, RenderResult, UndoResult};



#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DocumentId(String);


pub struct Document {
    pub id: DocumentId,
    pub path: Option<PathBuf>,
    pub text: String,
    pub rev: u64,
    pub cached: Option<RenderCache>,
    pub undo: Vec<EditOp>,
    pub redo: Vec<EditOp>,
}


#[derive(Default)]
pub struct DocumentStore {
    docs: HashMap<DocumentId, Document>,
    pub(super) order: Vec<DocumentId>,
    next_id: u64,
    
    
    active_id: Option<DocumentId>,
}
