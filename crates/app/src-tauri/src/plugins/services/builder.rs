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

    /// Подключает реестр view (Фаза 4). Без него `set_view_content` возвращает ошибку.
    pub fn with_views(mut self, views: Arc<Mutex<PluginViews>>) -> Self {
        self.views = Some(views);
        self
    }

    /// Подключает сигнал `plugin-views-changed` (смена контента view, F41/S2).
    ///
    /// Отдельный канал: этот колбэк **не** эмитит `plugins-changed` — `set_view_content`
    /// не меняет состав/статусы плагинов.
    pub fn with_notify(mut self, notify: Arc<dyn Fn() + Send + Sync>) -> Self {
        self.views_notify = Some(notify);
        self
    }

    /// Подключает очередь отложенных `document:changed` (плагинные `apply_edit`).
    pub fn with_pending_changed(mut self, pending: PendingChanged) -> Self {
        self.pending_changed = Some(pending);
        self
    }

    /// Подключает экспорт HTML (нативный диалог сохранения хоста, Фаза 5).
    pub fn with_exporter(mut self, exporter: Exporter) -> Self {
        self.exporter = Some(exporter);
        self
    }

    /// Подключает канал уведомлений статусбара (`plugin-message`, Фаза 5).
    pub fn with_status_sink(mut self, sink: StatusSink) -> Self {
        self.status_sink = Some(sink);
        self
    }

    /// Подключает уведомление фронтенда о внешней правке (`document-updated`).
    ///
    /// Эмитится только здесь — при плагинном `apply_edit`. Фронтовый `update_document`
    /// событие не шлёт, иначе получилась бы петля «правка → событие → перерисовка → правка».
    pub fn with_document_sink(mut self, sink: DocumentSink) -> Self {
        self.document_sink = Some(sink);
        self
    }

    /// Подключает канал журнала (`log-entry`, S11): сообщения `host.log`/`print` из Lua.
    pub fn with_log_sink(mut self, sink: LogSink) -> Self {
        self.log_sink = Some(sink);
        self
    }

    /// Подключает хранилище постоянных настроек плагинов (F43/S5). Без него
    /// `get_setting`/`set_setting` возвращают `no_settings`.
    pub fn with_plugin_settings(mut self, settings: Arc<Mutex<PluginSettingsStore>>) -> Self {
        self.plugin_settings = Some(settings);
        self
    }
}
