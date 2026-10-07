//! Разделяемое состояние плагинной подсистемы GUI-хоста (Фаза 4 TZ-H2).
//!
//! `PluginHost` сводит воедино менеджер плагинов, событийную шину и реестр
//! представлений. Tauri-команды ([`crate::plugin_views`], [`crate::plugin_view_action`])
//! и документные команды обращаются к нему через [`crate::PluginState`].
//!
//! **Инвариант дедлок-безопасности:** [`PluginHost::pump`] вызывает плагинов
//! (а те — host-call'ы к [`HostServices`]). Вызывающая сторона обязана **отпустить**
//! блокировку `DocumentStore` до `pump` — host-call `set_view_content`/range идёт в
//! тот же стор. Здесь, в `pump`, `DocumentStore` не берётся вообще.

use std::sync::{Arc, Mutex, MutexGuard};

use plugin_proto::envelope::PluginError;
use serde_json::{json, Value};

use crate::PluginViewInfo;

use super::bus::EventBus;
use super::manager::PluginManager;
use super::services::PendingChanged;
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

/// Общее состояние плагинной подсистемы (один экземпляр на приложение).
pub struct PluginHost {
    manager: Mutex<PluginManager>,
    bus: Mutex<EventBus>,
    views: Arc<Mutex<PluginViews>>,
    /// Отложенные `document:changed` от плагинных `apply_edit` (общая с `DocumentServices`).
    pending_changed: PendingChanged,
    /// Услуги хоста: задел Фазы 5 (менеджер UI, экспорт). В H2 менеджер хранит
    /// собственную копию `Arc`, поэтому поле пока не читается.
    #[allow(dead_code)]
    services: Arc<dyn HostServices>,
    notify: Arc<dyn Fn() + Send + Sync>,
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
            manager: Mutex::new(PluginManager::default()),
            bus: Mutex::new(EventBus::new()),
            views: Arc::new(Mutex::new(PluginViews::new())),
            pending_changed: Arc::new(Mutex::new(Vec::new())),
            services: Arc::new(NoopServices),
            notify: Arc::new(|| {}),
        }
    }

    pub fn new(
        manager: PluginManager,
        views: Arc<Mutex<PluginViews>>,
        services: Arc<dyn HostServices>,
        notify: Arc<dyn Fn() + Send + Sync>,
        pending_changed: PendingChanged,
    ) -> Self {
        Self {
            manager: Mutex::new(manager),
            bus: Mutex::new(EventBus::new()),
            views,
            pending_changed,
            services,
            notify,
        }
    }

    /// `plugin_views`: снимок реестра представлений для фронтенда.
    pub fn plugin_views(&self) -> Vec<PluginViewInfo> {
        lock(&self.views).snapshot()
    }

    /// `plugin_view_action`: обратная маршрутизация из тир-1 view (§9.4).
    ///
    /// `plugin_id` берётся из `view_id` до `:`; вызывается `on_action` плагина.
    /// Возвращённый плагином HTML уходит через `host.set_view_content` и
    /// санитизируется на хосте.
    pub fn plugin_view_action(
        &self,
        view_id: &str,
        action: &str,
        payload: Option<Value>,
    ) -> Result<Value, PluginError> {
        let plugin_id = view_id
            .split_once(':')
            .map(|(plugin_id, _)| plugin_id)
            .filter(|plugin_id| !plugin_id.is_empty())
            .ok_or_else(|| {
                PluginError::new("bad_view_id", format!("view_id без plugin_id: {view_id}"))
            })?;

        let result = {
            let mut manager = lock(&self.manager);
            let runtime = manager.get_mut(plugin_id).ok_or_else(|| {
                PluginError::new("unknown_plugin", format!("нет плагина {plugin_id}"))
            })?;
            runtime.invoke(
                "action",
                json!({
                    "view_id": view_id,
                    "action": action,
                    "payload": payload.unwrap_or(Value::Null),
                }),
            )
        };
        // `on_action` мог править документ (`apply_edit`) — дреним manager до pump.
        self.pump();
        result
    }

    /// Публикует `document:changed` (коалесинг по `rev`). Доставка — [`Self::pump`].
    pub fn publish_document_changed(&self, doc_id: &str, rev: u64) {
        lock(&self.bus).publish_document_changed(doc_id, rev);
    }

    /// Публикует `document:opened`. Доставка — [`Self::pump`].
    pub fn publish_opened(&self, doc_id: &str, path: Option<&str>) {
        lock(&self.bus).publish_opened(doc_id, path);
    }

    /// Публикует `document:closed`. Доставка — [`Self::pump`].
    pub fn publish_closed(&self, doc_id: &str) {
        lock(&self.bus).publish_closed(doc_id);
    }

    /// Публикует `command:invoked` (задел Фазы 5: команды менеджера/тулбара).
    #[allow(dead_code)]
    pub fn publish_command(&self, command_id: &str) {
        lock(&self.bus).publish_command(command_id);
    }

    /// Доставляет накопленные события всем активным плагинам.
    ///
    /// Вызывать **без** удержания блокировки `DocumentStore` (host-call плагина
    /// берёт тот же мьютекс → дедлок). Ошибка плагина не паникует: он снимается
    /// менеджером, остальные продолжают получать события.
    ///
    /// Цикл нужен для плагинных правок: обработчик `document:changed` может сам
    /// вызвать `apply_edit`, породив новый `document:changed`; такие события лежат в
    /// [`Self::pending_changed`] и доставляются следующим раундом. Число раундов ограничено.
    pub fn pump(&self) {
        for _ in 0..MAX_PUMP_ROUNDS {
            // Переносим отложенные `document:changed` (от плагинных apply_edit) в шину.
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
                    if let Err(error) = runtime.dispatch_event(&event.name, event.payload.clone()) {
                        eprintln!(
                            "плагин {}: событие {} не доставлено: {}",
                            runtime.id, event.name, error.message
                        );
                    }
                }
            }
        }
    }

    /// Best-effort старт enabled-плагинов и уведомление фронтенда.
    ///
    /// Ошибка запуска одного плагина не мешает остальным и не роняет GUI (§4.7).
    pub fn start_enabled(&self) {
        {
            let mut manager = lock(&self.manager);
            for runtime in manager.iter_mut() {
                if runtime.is_enabled() {
                    if let Err(error) = runtime.start() {
                        eprintln!("плагин {}: не запущен: {}", runtime.id, error.message);
                    }
                }
            }
        }
        // on_activate мог править документ — доставляем отложенные события и уведомляем.
        self.pump();
        (self.notify)();
    }

    /// Вкл/выкл плагина (задел IPC-команд менеджера, Фаза 5).
    ///
    /// Выключение убирает представления плагина из реестра (иначе остаются «зомби»-вкладки),
    /// включение перерегистрирует их из манифеста.
    #[allow(dead_code)]
    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), PluginError> {
        let contributes = {
            let mut manager = lock(&self.manager);
            let contributes = manager.get(id).map(|runtime| runtime.views.clone());
            let result = manager.set_plugin_enabled(id, enabled);
            (contributes, result)
        };
        let (contributes, result) = contributes;
        if result.is_ok() {
            let contributes = contributes.unwrap_or_default();
            {
                let mut views = lock(&self.views);
                views.remove_plugin(id);
                if enabled {
                    views.register(id, &contributes);
                }
            }
            (self.notify)();
        }
        result
    }

    /// Перезагрузка плагина без рестарта GUI (задел Фазы 5).
    #[allow(dead_code)]
    pub fn reload(&self, id: &str) -> Result<(), PluginError> {
        let result = lock(&self.manager).reload_plugin(id);
        if result.is_ok() {
            self.pump();
            (self.notify)();
        }
        result
    }
}
