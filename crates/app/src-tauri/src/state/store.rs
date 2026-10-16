//! Операции над документами и хранилищем: создание, правка, рендер, список, вкладки.

use std::path::PathBuf;

use super::history::{minimal_diff, push_op};
use super::{
    DocMeta, Document, DocumentId, DocumentSnapshot, DocumentStore, EditOp, RenderCache,
    RenderResult,
};

impl Document {
    pub(crate) fn snapshot(&self) -> DocumentSnapshot {
        DocumentSnapshot {
            id: self.id.clone(),
            path: self.path_string(),
            rev: self.rev,
            text: self.text.clone(),
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

impl DocumentStore {
    fn fresh_id(&mut self) -> DocumentId {
        self.next_id += 1;
        DocumentId(format!("doc-{}", self.next_id))
    }

    
    pub fn create(&mut self, text: String) -> DocumentSnapshot {
        self.insert(None, text)
    }

    
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

    
    pub fn render(&mut self, id: &DocumentId, mapped: bool) -> Option<RenderResult> {
        Some(self.docs.get_mut(id)?.render(mapped))
    }

    
    
    
    
    pub fn last_id(&self) -> Option<DocumentId> {
        self.order.last().cloned()
    }

    
    pub fn close(&mut self, id: &DocumentId) -> bool {
        if self.docs.remove(id).is_some() {
            self.order.retain(|x| x != id);
            if self.active_id.as_ref() == Some(id) {
                self.active_id = None;
            }
            true
        } else {
            false
        }
    }

    
    
    
    
    pub fn list(&self) -> Vec<DocMeta> {
        self.order
            .iter()
            .filter_map(|id| self.docs.get(id).map(|doc| doc.meta()))
            .collect()
    }

    
    
    
    
    
    pub fn activate(&mut self, id: &DocumentId) -> bool {
        if self.docs.contains_key(id) {
            self.active_id = Some(id.clone());
            true
        } else {
            false
        }
    }

    
    pub fn active_id(&self) -> Option<&DocumentId> {
        self.active_id.as_ref()
    }

    
    
    
    
    
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
}
