//! Событийная шина GUI-хоста (Фаза 4 TZ-H2, §9 DESIGN_DOC).
//!
//! Модель **notify-only + pull**: событие не несёт текст документа, только
//! `doc_id`/`rev` (или путь/команду). Плагин, которому нужен текст, сам читает
//! его через `host.get_document_range` (§9.2).
//!
//! Коалесинг по `rev` для `document:changed`: за один [`EventBus::drain`] по
//! каждому `doc_id` доставляется **только последняя** ревизия (§9.3). Порядок
//! детерминирован: событие сохраняет позицию первого появления в тике, а
//! прочие события (opened/closed/command) — в порядке публикации.

use std::collections::HashMap;

use serde_json::{json, Value};

/// Событие шины: имя (`document:changed`, …) и JSON-payload.
#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub name: String,
    pub payload: Value,
}

/// Накопитель событий между тиками доставки.
///
/// Очередь снимается целиком через [`EventBus::drain`]; `document:changed`
/// коалесцируется по `doc_id` на месте (последняя ревизия, позиция первого
/// появления — детерминированный порядок).
#[derive(Default)]
pub struct EventBus {
    pending: Vec<Event>,
    changed_at: HashMap<String, usize>,
}

impl EventBus {
    pub fn new() -> Self {
        Self::default()
    }

    /// Публикует `document:changed{doc_id,rev}` с коалесингом по `doc_id`.
    pub fn publish_document_changed(&mut self, doc_id: &str, rev: u64) {
        let event = Event {
            name: "document:changed".to_string(),
            payload: json!({ "doc_id": doc_id, "rev": rev }),
        };
        match self.changed_at.get(doc_id).copied() {
            // Уже есть событие по этому документу — обновляем ревизию на месте.
            Some(index) => self.pending[index] = event,
            None => {
                self.changed_at
                    .insert(doc_id.to_string(), self.pending.len());
                self.pending.push(event);
            }
        }
    }

    /// Публикует `document:opened{doc_id,path}`.
    pub fn publish_opened(&mut self, doc_id: &str, path: Option<&str>) {
        self.push("document:opened", json!({ "doc_id": doc_id, "path": path }));
    }

    /// Публикует `document:closed{doc_id}` (путь закрытого документа хосту неизвестен).
    pub fn publish_closed(&mut self, doc_id: &str) {
        self.push(
            "document:closed",
            json!({ "doc_id": doc_id, "path": Value::Null }),
        );
    }

    /// Публикует `command:invoked{command_id}`.
    pub fn publish_command(&mut self, command_id: &str) {
        self.push("command:invoked", json!({ "command_id": command_id }));
    }

    fn push(&mut self, name: &str, payload: Value) {
        self.pending.push(Event {
            name: name.to_string(),
            payload,
        });
    }

    /// Забирает все накопленные события и очищает очередь (в т.ч. карту коалесинга).
    pub fn drain(&mut self) -> Vec<Event> {
        self.changed_at.clear();
        std::mem::take(&mut self.pending)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_changed_coalesces_to_latest_rev() {
        let mut bus = EventBus::new();
        bus.publish_document_changed("d1", 1);
        bus.publish_document_changed("d1", 2);
        bus.publish_document_changed("d1", 7);

        let events = bus.drain();
        assert_eq!(events.len(), 1, "одна ревизия на doc_id");
        assert_eq!(events[0].name, "document:changed");
        assert_eq!(events[0].payload["doc_id"], "d1");
        assert_eq!(events[0].payload["rev"], 7);
    }

    #[test]
    fn coalescing_is_per_document_and_order_is_stable() {
        let mut bus = EventBus::new();
        bus.publish_document_changed("d1", 1);
        bus.publish_document_changed("d2", 1);
        bus.publish_document_changed("d1", 2);

        let events = bus.drain();
        assert_eq!(events.len(), 2, "по одному событию на документ");
        // Порядок — по первому появлению: d1, затем d2.
        assert_eq!(events[0].payload["doc_id"], "d1");
        assert_eq!(events[0].payload["rev"], 2);
        assert_eq!(events[1].payload["doc_id"], "d2");
        assert_eq!(events[1].payload["rev"], 1);
    }

    #[test]
    fn other_events_keep_publication_order() {
        let mut bus = EventBus::new();
        bus.publish_opened("d1", Some("C:/a.md"));
        bus.publish_document_changed("d1", 1);
        bus.publish_command("word-count.count");
        bus.publish_closed("d1");

        let events = bus.drain();
        let names: Vec<&str> = events.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "document:opened",
                "document:changed",
                "command:invoked",
                "document:closed"
            ]
        );
    }

    #[test]
    fn drain_clears_queue_and_coalescing_state() {
        let mut bus = EventBus::new();
        bus.publish_document_changed("d1", 1);
        assert_eq!(bus.drain().len(), 1);
        assert!(bus.drain().is_empty(), "повторный drain пуст");

        // После drain коалесинг начинается заново.
        bus.publish_document_changed("d1", 5);
        let events = bus.drain();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].payload["rev"], 5);
    }

    #[test]
    fn opened_carries_path() {
        let mut bus = EventBus::new();
        bus.publish_opened("d1", Some("C:/a.md"));
        let events = bus.drain();
        assert_eq!(events[0].payload["path"], "C:/a.md");
    }
}
