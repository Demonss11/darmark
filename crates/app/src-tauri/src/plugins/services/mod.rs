//! Реализация [`HostServices`](super::HostServices) поверх [`DocumentStore`] — range/delta host-API (§4.2 плагинной системы).
//!
//! Основной путь чтения — окно (`get_document_range`), а не полная копия: при модели
//! «child на плагин» копия 10 МБ умножалась бы на число процессов. `apply_edit` идёт
//! единым путём `DocumentStore::apply_edit` → `rev++` (D5).

mod builder;
mod host_calls;

use std::sync::{Arc, Mutex};

use crate::state::DocumentStore;

use super::plugin_settings::PluginSettingsStore;
use super::views::PluginViews;

const MAX_RANGE_BYTES: usize = plugin_core::frame::MAX_EVENT_FRAME_BYTES;

const REPLY_ENVELOPE_RESERVE: usize = 1024;
const MAX_TEXT_REPLY_ESCAPED: usize =
    plugin_core::frame::MAX_HOST_FRAME_BYTES - REPLY_ENVELOPE_RESERVE;

pub type PendingChanged = Arc<Mutex<Vec<(String, u64)>>>;

pub type Exporter = Arc<dyn Fn(&str) -> Result<bool, String> + Send + Sync>;

pub type StatusSink = Arc<dyn Fn(&str, &str) + Send + Sync>;

pub type DocumentSink = Arc<dyn Fn(&str, u64) + Send + Sync>;

pub type LogSink = Arc<dyn Fn(&str, &str, &str) + Send + Sync>;

pub struct DocumentServices {
    store: Arc<Mutex<DocumentStore>>,

    views: Option<Arc<Mutex<PluginViews>>>,

    views_notify: Option<Arc<dyn Fn() + Send + Sync>>,

    pending_changed: Option<PendingChanged>,

    exporter: Option<Exporter>,

    status_sink: Option<StatusSink>,

    document_sink: Option<DocumentSink>,

    log_sink: Option<LogSink>,

    plugin_settings: Option<Arc<Mutex<PluginSettingsStore>>>,
}
