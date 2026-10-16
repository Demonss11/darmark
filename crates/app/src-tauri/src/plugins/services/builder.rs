//! Конструктор `DocumentServices` и подключение услуг (view-реестр, сигналы, стор, сторы
//! настроек). Сами host-call'ы живут в `super::host_calls`.

use std::sync::{Arc, Mutex};

use crate::state::DocumentStore;

use super::super::plugin_settings::PluginSettingsStore;
use super::super::views::PluginViews;
use super::{DocumentServices, DocumentSink, Exporter, LogSink, PendingChanged, StatusSink};

impl DocumentServices {
    pub fn new(store: Arc<Mutex<DocumentStore>>) -> Self {
        Self {
            store,
            views: None,
            views_notify: None,
            pending_changed: None,
            exporter: None,
            status_sink: None,
            document_sink: None,
            log_sink: None,
            plugin_settings: None,
        }
    }

    
    pub fn with_views(mut self, views: Arc<Mutex<PluginViews>>) -> Self {
        self.views = Some(views);
        self
    }

    
    
    
    
    pub fn with_notify(mut self, notify: Arc<dyn Fn() + Send + Sync>) -> Self {
        self.views_notify = Some(notify);
        self
    }

    
    pub fn with_pending_changed(mut self, pending: PendingChanged) -> Self {
        self.pending_changed = Some(pending);
        self
    }

    
    pub fn with_exporter(mut self, exporter: Exporter) -> Self {
        self.exporter = Some(exporter);
        self
    }

    
    pub fn with_status_sink(mut self, sink: StatusSink) -> Self {
        self.status_sink = Some(sink);
        self
    }

    
    
    
    
    pub fn with_document_sink(mut self, sink: DocumentSink) -> Self {
        self.document_sink = Some(sink);
        self
    }

    
    pub fn with_log_sink(mut self, sink: LogSink) -> Self {
        self.log_sink = Some(sink);
        self
    }

    
    
    pub fn with_plugin_settings(mut self, settings: Arc<Mutex<PluginSettingsStore>>) -> Self {
        self.plugin_settings = Some(settings);
        self
    }
}
