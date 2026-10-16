//! Доставка событий плагинам (`pump`) и сигналы об изменениях.
//!
//! `pump` — единственная точка доставки накопленных событий активным плагинам; здесь же
//! живут эмиссия переходов статуса и сигналы `plugin-views-changed`/`plugins-changed`.

use crate::PluginStatus;

use super::{lock, status_state, PluginHost, MAX_PUMP_ROUNDS};

impl PluginHost {
    
    pub(super) fn runtime_status(&self, id: &str) -> Option<PluginStatus> {
        lock(&self.manager)
            .get(id)
            .map(|runtime| runtime.status.clone())
    }

    
    
    pub(super) fn emit_status_transition(
        &self,
        id: &str,
        old: &PluginStatus,
        new: &PluginStatus,
        message: Option<&str>,
    ) {
        let fallback = match new {
            PluginStatus::Failed { message } => Some(message.as_str()),
            _ => None,
        };
        (self.status_changed_sink)(
            id,
            status_state(old),
            status_state(new),
            message.or(fallback),
        );
    }

    
    fn notify_views(&self) {
        (self.views_notify)();
    }

    
    pub(super) fn notify_plugins(&self) {
        (self.plugins_notify)();
    }

    
    pub(super) fn notify_all(&self) {
        self.notify_views();
        self.notify_plugins();
    }

    
    
    
    
    
    
    
    
    
    pub fn pump(&self) {
        let mut status_changed = false;
        let mut transitions: Vec<(String, PluginStatus, PluginStatus, Option<String>)> = Vec::new();
        for _ in 0..MAX_PUMP_ROUNDS {
            {
                let mut pending = lock(&self.pending_changed);
                if !pending.is_empty() {
                    let mut bus = lock(&self.bus);
                    for (doc_id, rev) in pending.drain(..) {
                        bus.publish_document_changed(&doc_id, rev);
                    }
                }
            }
            let events = lock(&self.bus).drain();
            if events.is_empty() {
                break;
            }
            let mut manager = lock(&self.manager);
            for event in &events {
                for runtime in manager.iter_active_mut() {
                    let old = runtime.status.clone();
                    if let Err(error) = runtime.dispatch_event(&event.name, event.payload.clone()) {
                        eprintln!(
                            "плагин {}: событие {} не доставлено: {}",
                            runtime.id, event.name, error.message
                        );
                        status_changed = true;
                        transitions.push((
                            runtime.id.clone(),
                            old,
                            runtime.status.clone(),
                            Some(error.message),
                        ));
                    }
                }
            }
        }
        for (id, old, new, message) in &transitions {
            self.emit_status_transition(id, old, new, message.as_deref());
        }
        if status_changed {
            self.notify_plugins();
        }
    }
}
