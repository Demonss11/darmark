//! Разделяемое состояние плагинной подсистемы GUI-хоста (Фаза 4 плагинной системы).
//!
//! `PluginHost` сводит воедино менеджер плагинов, событийную шину и реестр
//! представлений. Tauri-команды ([`crate::plugin_views`], [`crate::plugin_view_action`])
//! и документные команды обращаются к нему через [`crate::PluginState`].
//!
//! **Инвариант дедлок-безопасности:** [`PluginHost::pump`] вызывает плагинов
//! (а те — host-call'ы к [`HostServices`]). Вызывающая сторона обязана **отпустить**
//! блокировку `DocumentStore` до `pump` — host-call `set_view_content`/range идёт в
//! тот же стор. Здесь, в `pump`, `DocumentStore` не берётся вообще.

mod lifecycle;
mod listing;
mod publish;
mod pump;
mod views;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;

use plugin_core::envelope::PluginError;
use serde_json::Value;

use crate::PluginStatus;

use super::bus::EventBus;
use super::manager::PluginManager;
use super::plugin_settings::PluginSettingsStore;
use super::services::PendingChanged;
use super::settings::SettingsStore;
use super::views::PluginViews;
use super::HostServices;





fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}




fn status_state(status: &PluginStatus) -> &'static str {
    match status {
        PluginStatus::Stopped => "stopped",
        PluginStatus::Active => "active",
        PluginStatus::Failed { .. } => "failed",
        PluginStatus::Quarantined => "quarantined",
    }
}




#[derive(Clone)]
struct LastDoc {
    id: String,
    path: Option<String>,
}






pub type StatusChangedSink = Arc<dyn Fn(&str, &str, &str, Option<&str>) + Send + Sync>;




pub type DocumentEventSink = Arc<dyn Fn(&str, Option<&str>, Option<u64>) + Send + Sync>;


pub struct PluginHost {
    manager: Arc<Mutex<PluginManager>>,
    bus: Mutex<EventBus>,
    views: Arc<Mutex<PluginViews>>,
    
    pending_changed: PendingChanged,
    
    
    #[allow(dead_code)]
    services: Arc<dyn HostServices>,
    
    views_notify: Arc<dyn Fn() + Send + Sync>,
    
    
    
    
    plugins_notify: Arc<dyn Fn() + Send + Sync>,
    
    
    settings: Mutex<SettingsStore>,
    config_path: Option<PathBuf>,
    
    
    plugin_settings: Option<Arc<Mutex<PluginSettingsStore>>>,
    
    
    
    last_doc: Mutex<Option<LastDoc>>,
    
    status_changed_sink: StatusChangedSink,
    
    document_event_sink: DocumentEventSink,
    
    
    resource_stop: Arc<AtomicBool>,
    
    resource_thread: Option<JoinHandle<()>>,
}



const MAX_PUMP_ROUNDS: usize = 16;


struct NoopServices;

impl HostServices for NoopServices {
    fn handle(&self, method: &str, _args: Value) -> Result<Value, PluginError> {
        Err(PluginError::protocol(format!(
            "плагинная подсистема недоступна: {method}"
        )))
    }
}

impl PluginHost {
    
    
    pub fn empty() -> Self {
        Self {
            manager: Arc::new(Mutex::new(PluginManager::default())),
            bus: Mutex::new(EventBus::new()),
            views: Arc::new(Mutex::new(PluginViews::new())),
            pending_changed: Arc::new(Mutex::new(Vec::new())),
            services: Arc::new(NoopServices),
            views_notify: Arc::new(|| {}),
            plugins_notify: Arc::new(|| {}),
            settings: Mutex::new(SettingsStore::default()),
            config_path: None,
            plugin_settings: None,
            last_doc: Mutex::new(None),
            status_changed_sink: Arc::new(|_, _, _, _| {}),
            document_event_sink: Arc::new(|_, _, _| {}),
            resource_stop: Arc::new(AtomicBool::new(false)),
            resource_thread: None,
        }
    }

    pub fn new(
        manager: PluginManager,
        views: Arc<Mutex<PluginViews>>,
        services: Arc<dyn HostServices>,
        views_notify: Arc<dyn Fn() + Send + Sync>,
        pending_changed: PendingChanged,
    ) -> Self {
        Self {
            manager: Arc::new(Mutex::new(manager)),
            bus: Mutex::new(EventBus::new()),
            views,
            pending_changed,
            services,
            views_notify,
            plugins_notify: Arc::new(|| {}),
            settings: Mutex::new(SettingsStore::default()),
            config_path: None,
            plugin_settings: None,
            last_doc: Mutex::new(None),
            status_changed_sink: Arc::new(|_, _, _, _| {}),
            document_event_sink: Arc::new(|_, _, _| {}),
            resource_stop: Arc::new(AtomicBool::new(false)),
            resource_thread: None,
        }
    }

    
    
    pub fn with_settings(mut self, settings: SettingsStore, config_path: Option<PathBuf>) -> Self {
        self.settings = Mutex::new(settings);
        self.config_path = config_path;
        self
    }

    
    
    pub fn with_plugin_settings(mut self, settings: Arc<Mutex<PluginSettingsStore>>) -> Self {
        self.plugin_settings = Some(settings);
        self
    }

    
    
    
    
    
    pub fn with_plugins_notify(mut self, notify: Arc<dyn Fn() + Send + Sync>) -> Self {
        self.plugins_notify = notify;
        self
    }

    
    
    
    
    pub fn with_status_changed_sink(mut self, sink: StatusChangedSink) -> Self {
        self.status_changed_sink = sink;
        self
    }

    
    
    
    pub fn with_document_event_sink(mut self, sink: DocumentEventSink) -> Self {
        self.document_event_sink = sink;
        self
    }
}

impl Drop for PluginHost {
    
    
    
    
    fn drop(&mut self) {
        self.resource_stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.resource_thread.take() {
            let _ = thread.join();
        }
    }
}
