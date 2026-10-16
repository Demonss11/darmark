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

    
    
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}






#[derive(Clone, Debug)]
pub struct EditOp {
    pub start: usize,
    pub removed: String,
    pub inserted: String,
}




#[derive(Clone, Debug)]
pub struct RenderCache {
    pub mapped: bool,
    pub rev: u64,
    pub html: String,
}





#[derive(Debug, Serialize)]
pub struct DocumentSnapshot {
    pub id: DocumentId,
    pub path: Option<String>,
    pub rev: u64,
    pub text: String,
    pub dirty_hint: bool,
}


#[derive(Debug, Serialize)]
pub struct DocMeta {
    pub id: DocumentId,
    pub path: Option<String>,
    pub rev: u64,
    pub dirty_hint: bool,
}




#[derive(Debug, Serialize)]
pub struct RenderResult {
    pub html: String,
    pub rev: u64,
    pub changed: bool,
}






#[derive(Debug, Serialize)]
pub struct UndoResult {
    pub text: String,
    pub rev: u64,
    pub start: usize,
    pub end: usize,
}
