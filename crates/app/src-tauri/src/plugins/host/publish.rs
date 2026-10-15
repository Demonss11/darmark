//! Публикация документных событий и команд в шину и их переиздание плагинам.
//!
//! Все методы только наполняют шину/кэш последнего документа — доставку выполняет
//! [`PluginHost::pump`].

use serde_json::json;

use crate::PluginStatus;

use super::{lock, LastDoc, PluginHost};

impl PluginHost {
    /// Публикует `document-event` (S11): действие пользователя над документом.
    ///
    /// `kind` — `new`/`open`/`save`/`close`; `path`/`rev` опциональны. Доставка — через
    /// [`DocumentEventSink`](super::DocumentEventSink) (Tauri-событие `document-event`), подключённый в `setup_plugins`.
    pub fn publish_document_event(&self, kind: &str, path: Option<&str>, rev: Option<u64>) {
        (self.document_event_sink)(kind, path, rev);
    }

    /// Публикует `document:changed` (коалесинг по `rev`). Доставка — [`Self::pump`].
    ///
    /// Если событие о другом документе, `last_doc` переключается на него (путь неизвестен):
    /// переиздание после (ре)старта должно адресовать именно последний документ.
    pub fn publish_document_changed(&self, doc_id: &str, rev: u64) {
        {
            let mut last = lock(&self.last_doc);
            if !last.as_ref().is_some_and(|doc| doc.id == doc_id) {
                *last = Some(LastDoc {
                    id: doc_id.to_string(),
                    path: None,
                });
            }
        }
        lock(&self.bus).publish_document_changed(doc_id, rev);
    }

    /// Публикует `document:opened`. Доставка — [`Self::pump`].
    ///
    /// Открытие фиксирует документ как последний: он будет переиздан подписчикам
    /// при (ре)старте view-плагина.
    pub fn publish_opened(&self, doc_id: &str, path: Option<&str>) {
        *lock(&self.last_doc) = Some(LastDoc {
            id: doc_id.to_string(),
            path: path.map(str::to_string),
        });
        lock(&self.bus).publish_opened(doc_id, path);
    }

    /// Публикует `document:closed`. Доставка — [`Self::pump`].
    ///
    /// Закрытие последнего документа сбрасывает `last_doc`: переиздавать нечего.
    pub fn publish_closed(&self, doc_id: &str) {
        {
            let mut last = lock(&self.last_doc);
            if last.as_ref().is_some_and(|doc| doc.id == doc_id) {
                *last = None;
            }
        }
        lock(&self.bus).publish_closed(doc_id);
    }

    /// Публикует `command:invoked{command_id, doc_id}` (команды менеджера/тулбара, Фаза 5).
    pub fn publish_command(&self, command_id: &str, doc_id: Option<&str>) {
        lock(&self.bus).publish_command(command_id, doc_id);
    }

    /// `run_plugin_command`: публикует команду в шину и доставляет плагинам.
    ///
    /// `doc_id` — текущий документ (плагин не имеет собственного «активного» документа).
    pub fn run_plugin_command(&self, command_id: &str, doc_id: Option<&str>) {
        self.publish_command(command_id, doc_id);
        self.pump();
    }

    /// Переиздаёт конкретному плагину `document:opened` для последнего документа.
    ///
    /// Нужен после (ре)старта view-плагина: contributed view уже в реестре, child снова
    /// активен, но его `on_activate` не наполнит вкладку, если плагин строит её по
    /// подписке на `document:opened`/`changed`. Адресно — только этому плагину, чтобы
    /// не дублировать событие остальным.
    pub(super) fn reannounce_opened(&self, plugin_id: &str) {
        let last = lock(&self.last_doc).clone();
        let Some(last) = last else { return };
        let payload = json!({ "doc_id": last.id, "path": last.path });
        let mut manager = lock(&self.manager);
        if let Some(runtime) = manager.get_mut(plugin_id) {
            if matches!(runtime.status, PluginStatus::Active) {
                let _ = runtime.dispatch_event("document:opened", payload);
            }
        }
    }
}
