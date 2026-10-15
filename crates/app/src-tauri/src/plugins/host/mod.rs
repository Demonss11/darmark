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

/// Блокировка мьютекса с восстановлением после отравления.
///
/// Под `panic = "abort"` отравление невозможно; в тестах (unwind) — берём данные,
/// чтобы один упавший тест не ломал остальные.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Состояние плагина как snake_case-строка. Держим синхронно с сериализацией
/// [`PluginStatus`] (`#[serde(rename_all = "snake_case")]`): журнал (S11) ждёт строки
/// `stopped`/`active`/`failed`/`quarantined`.
fn status_state(status: &PluginStatus) -> &'static str {
    match status {
        PluginStatus::Stopped => "stopped",
        PluginStatus::Active => "active",
        PluginStatus::Failed { .. } => "failed",
        PluginStatus::Quarantined => "quarantined",
    }
}

/// Последний документ, для которого публиковались `document:opened`/`changed`.
/// Нужен, чтобы после (ре)старта view-плагина переиздать ему `document:opened`:
/// иначе вкладка, наполняемая по подписке, останется пустой.
#[derive(Clone)]
struct LastDoc {
    id: String,
    path: Option<String>,
}

/// Канал `plugin-status-changed` (S11): `(plugin_id, old_state, new_state, message)`.
///
/// Состояния — snake_case [`PluginStatus`] (`stopped`/`active`/`failed`/`quarantined`).
/// По умолчанию no-op (тесты, [`PluginHost::empty`]); подключается
/// [`PluginHost::with_status_changed_sink`].
pub type StatusChangedSink = Arc<dyn Fn(&str, &str, &str, Option<&str>) + Send + Sync>;

/// Канал `document-event` (S11): `(type, path, rev)`. `type` — `new`/`open`/`save`/`close`.
///
/// По умолчанию no-op; подключается [`PluginHost::with_document_event_sink`].
pub type DocumentEventSink = Arc<dyn Fn(&str, Option<&str>, Option<u64>) + Send + Sync>;

/// Общее состояние плагинной подсистемы (один экземпляр на приложение).
pub struct PluginHost {
    manager: Arc<Mutex<PluginManager>>,
    bus: Mutex<EventBus>,
    views: Arc<Mutex<PluginViews>>,
    /// Отложенные `document:changed` от плагинных `apply_edit` (общая с `DocumentServices`).
    pending_changed: PendingChanged,
    /// Услуги хоста: задел Фазы 5 (менеджер UI, экспорт). В H2 менеджер хранит
    /// собственную копию `Arc`, поэтому поле пока не читается.
    #[allow(dead_code)]
    services: Arc<dyn HostServices>,
    /// Сигнал `plugin-views-changed`: сменился контент/реестр представлений.
    views_notify: Arc<dyn Fn() + Send + Sync>,
    /// Сигнал `plugins-changed`: сменился состав/статус/карантин/включение (F41/S2).
    /// Отдельный от view-сигнала: `set_view_content` не должен дёргать `list_plugins`.
    /// По умолчанию no-op (тесты, [`PluginHost::empty`]); подключается
    /// [`PluginHost::with_plugins_notify`].
    plugins_notify: Arc<dyn Fn() + Send + Sync>,
    /// Настройки приложения (вкл/выкл плагинов, §12) и путь `config.json` для записи.
    /// Вкл/выкл из менеджера обязан переживать рестарт, поэтому сохраняется на диск.
    settings: Mutex<SettingsStore>,
    config_path: Option<PathBuf>,
    /// Постоянные настройки плагинов (F43/S5): общий с `DocumentServices` стор, чтобы
    /// удаление плагина подчищало и его `plugin-settings/<id>.json`. Без него — `None`.
    plugin_settings: Option<Arc<Mutex<PluginSettingsStore>>>,
    /// Последний документ, для которого публиковались `document:opened`/`changed`.
    /// После (ре)старта view-плагина переиздаём ему `document:opened`, чтобы
    /// вкладка, наполняемая по подписке, не осталась пустой.
    last_doc: Mutex<Option<LastDoc>>,
    /// Канал `plugin-status-changed` (S11): переходы статуса плагина для журнала.
    status_changed_sink: StatusChangedSink,
    /// Канал `document-event` (S11): действия пользователя над документом (`new`/`open`/…).
    document_event_sink: DocumentEventSink,
    /// Флаг остановки потока ресурс-монитора (`Arc<AtomicBool>`, ADR-0031 §2.3 п.9).
    /// Взводится в [`Drop`]; поток завершается, не мешая выходу.
    resource_stop: Arc<AtomicBool>,
    /// Дескриптор потока опроса ресурсов (ADR-0031). `None` — монитор не запущен.
    resource_thread: Option<JoinHandle<()>>,
}

/// Максимум раундов `pump`: обработчик события может сам править документ (`apply_edit`)
/// и порождать новый `document:changed`. Ограничение страхует от лавинообразного цикла.
const MAX_PUMP_ROUNDS: usize = 16;

/// Заглушка услуг хоста для [`PluginHost::empty`] (плагинная подсистема не поднялась).
struct NoopServices;

impl HostServices for NoopServices {
    fn handle(&self, method: &str, _args: Value) -> Result<Value, PluginError> {
        Err(PluginError::protocol(format!(
            "плагинная подсистема недоступна: {method}"
        )))
    }
}

impl PluginHost {
    /// Пустой хост без плагинов (child-exe не найден и т.п.): команды отвечают
    /// пустым списком/ошибкой, GUI продолжает работу (§4.7).
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

    /// Подключает `SettingsStore` и путь `config.json`: вкл/выкл плагинов из менеджера
    /// сохраняется, чтобы переживать рестарт (§12). Без пути — только in-memory.
    pub fn with_settings(mut self, settings: SettingsStore, config_path: Option<PathBuf>) -> Self {
        self.settings = Mutex::new(settings);
        self.config_path = config_path;
        self
    }

    /// Подключает хранилище постоянных настроек плагинов (F43/S5). Тот же `Arc`, что у
    /// `DocumentServices`: `remove` подчищает файл настроек удаляемого плагина.
    pub fn with_plugin_settings(mut self, settings: Arc<Mutex<PluginSettingsStore>>) -> Self {
        self.plugin_settings = Some(settings);
        self
    }

    /// Подключает сигнал `plugins-changed` (F41/S2): состав/статус/карантин/включение.
    ///
    /// Отдельный от `plugin-views-changed` (передаётся в [`PluginHost::new`]): `set_view_content`
    /// меняет лишь контент view и не должен заставлять фронтенд-менеджер лишний раз звать
    /// `list_plugins`. По умолчанию (без вызова) сигнал — no-op.
    pub fn with_plugins_notify(mut self, notify: Arc<dyn Fn() + Send + Sync>) -> Self {
        self.plugins_notify = notify;
        self
    }

    /// Подключает канал `plugin-status-changed` (S11): переходы статуса плагина.
    ///
    /// По умолчанию (без вызова) — no-op. Подключается в `setup_plugins` для эмиссии
    /// события `plugin-status-changed` в журнал фронтенда.
    pub fn with_status_changed_sink(mut self, sink: StatusChangedSink) -> Self {
        self.status_changed_sink = sink;
        self
    }

    /// Подключает канал `document-event` (S11): действия пользователя над документом.
    ///
    /// По умолчанию (без вызова) — no-op. Подключается в `setup_plugins`.
    pub fn with_document_event_sink(mut self, sink: DocumentEventSink) -> Self {
        self.document_event_sink = sink;
        self
    }
}

impl Drop for PluginHost {
    /// Останавливает поток ресурс-монитора при выходе из приложения (ADR-0031 §2.3 п.9).
    ///
    /// Флаг взводится, затем поток присоединяется: поток проверяет флаг в цикле сна чанками
    /// (≤50 мс), поэтому выход не «зависает» на 2-секундном периоде.
    fn drop(&mut self) {
        self.resource_stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.resource_thread.take() {
            let _ = thread.join();
        }
    }
}
