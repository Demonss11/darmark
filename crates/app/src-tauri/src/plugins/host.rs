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

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use plugin_proto::envelope::PluginError;
use plugin_proto::manifest::ViewContrib;
use serde_json::{json, Value};

use crate::{PluginInfo, PluginViewInfo};

use super::bus::EventBus;
use super::manager::{effective_permissions, PluginManager};
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
    /// Настройки приложения (вкл/выкл плагинов, §12) и путь `config.json` для записи.
    /// Вкл/выкл из менеджера обязан переживать рестарт, поэтому сохраняется на диск.
    settings: Mutex<SettingsStore>,
    config_path: Option<PathBuf>,
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
            settings: Mutex::new(SettingsStore::default()),
            config_path: None,
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
            settings: Mutex::new(SettingsStore::default()),
            config_path: None,
        }
    }

    /// Подключает `SettingsStore` и путь `config.json`: вкл/выкл плагинов из менеджера
    /// сохраняется, чтобы переживать рестарт (§12). Без пути — только in-memory.
    pub fn with_settings(mut self, settings: SettingsStore, config_path: Option<PathBuf>) -> Self {
        self.settings = Mutex::new(settings);
        self.config_path = config_path;
        self
    }

    /// `plugin_views`: снимок реестра представлений для фронтенда.
    pub fn plugin_views(&self) -> Vec<PluginViewInfo> {
        lock(&self.views).snapshot()
    }

    /// `list_plugins`: снимок реестра плагинов для менеджера UI (Фаза 5).
    pub fn list_plugins(&self) -> Vec<PluginInfo> {
        lock(&self.manager).list_plugins()
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

        // `view_id` обязан быть зарегистрированным тир-1 представлением (§9.4): не даём
        // дёрнуть `on_action` с произвольным/чужим id (реестр — источник истины).
        if !lock(&self.views).contains(view_id) {
            return Err(PluginError::new(
                "unknown_view",
                format!("представление не зарегистрировано: {view_id}"),
            ));
        }

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
        // Отказ invocation увёл статус в failed/quarantined — сообщаем фронтенду.
        let failed = result.is_err();
        // `on_action` мог править документ (`apply_edit`) — дреним manager до pump.
        self.pump();
        if failed {
            (self.notify)();
        }
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
        // Отказ доставки уводит статус плагина в failed/quarantined — менеджер UI
        // должен узнать об этом сразу, а не после следующего действия пользователя.
        let mut status_changed = false;
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
                        status_changed = true;
                    }
                }
            }
        }
        if status_changed {
            (self.notify)();
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

    /// `set_plugin_enabled`: вкл/выкл плагина (IPC-команда менеджера, Фаза 5).
    ///
    /// **Порядок критичен:** при включении contributed view регистрируются в реестре
    /// **до** старта child — иначе `host.set_view_content` из `on_activate` вернул бы
    /// `unknown_view`. При выключении — наоборот: стоп, затем снятие view (нет «зомби»).
    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), PluginError> {
        let views_contrib = {
            let manager = lock(&self.manager);
            manager
                .get(id)
                .map(|runtime| runtime.views.clone())
                .ok_or_else(|| PluginError::new("unknown_plugin", format!("нет плагина {id}")))?
        };

        if enabled {
            let mut views = lock(&self.views);
            views.remove_plugin(id);
            views.register(id, &views_contrib);
        }

        let result = lock(&self.manager).set_plugin_enabled(id, enabled);

        // Неудачный старт или выключение — view плагина не должны висеть в реестре.
        if !enabled || result.is_err() {
            lock(&self.views).remove_plugin(id);
        }

        // Вкл/выкл должен переживать рестарт: фиксируем в config.json (§12).
        if result.is_ok() {
            if let Some(path) = &self.config_path {
                let mut settings = lock(&self.settings);
                settings.set_plugin_enabled(id, enabled);
                if let Err(e) = settings.save(path) {
                    eprintln!("не сохранить config.json: {e}");
                }
            }
        }

        // `on_activate` мог править документ; `notify` — статус/состав изменились.
        self.pump();
        (self.notify)();
        result
    }

    /// `remove_plugin`: полное удаление плагина.
    ///
    /// Порядок критичен: сначала снимаем views и останавливаем child, потом удаляем
    /// настройки и каталог с диска. Ошибка удаления файлов не откатывает удаление
    /// из реестра — плагин уже не запустится, а каталог можно удалить вручную.
    pub fn remove(&self, id: &str) -> Result<(), PluginError> {
        // Снимаем views до удаления из менеджера: иначе останутся «зомби»-вкладки.
        lock(&self.views).remove_plugin(id);

        // Удаляем из менеджера (останавливает child) и получаем путь к каталогу.
        let dir = lock(&self.manager).remove_plugin(id)?;

        // Удаляем настройки плагина из config.json.
        if let Some(path) = &self.config_path {
            let mut settings = lock(&self.settings);
            settings.remove_plugin(id);
            if let Err(e) = settings.save(path) {
                eprintln!("не сохранить config.json: {e}");
            }
        }

        // Удаляем каталог плагина с диска.
        if !dir.as_os_str().is_empty() && dir.exists() {
            if let Err(e) = std::fs::remove_dir_all(&dir) {
                return Err(PluginError::new(
                    "remove_failed",
                    format!("{}: {e}", dir.display()),
                ));
            }
        }

        self.pump();
        (self.notify)();
        Ok(())
    }

    /// `reload_plugin`: перезапуск без рестарта GUI (Фаза 5).
    ///
    /// Неудачная перезагрузка (синтаксис и т.п.) снимает view плагина: иначе остаётся
    /// «зомби»-вкладка со старым HTML при статусе «ошибка».
    pub fn reload(&self, id: &str) -> Result<(), PluginError> {
        // Для включённого плагина гарантируем contributed view в реестре ДО старта
        // child: предыдущий сбой мог снять view, а `on_activate` пишет HTML через
        // `set_view_content` — иначе `unknown_view` и потерянный контент.
        if let Some((true, contributes)) = self.enabled_views(id) {
            let mut views = lock(&self.views);
            views.remove_plugin(id);
            views.register(id, &contributes);
        }
        let result = lock(&self.manager).reload_plugin(id);
        if result.is_err() {
            lock(&self.views).remove_plugin(id);
        }
        self.pump();
        (self.notify)();
        result
    }

    /// Состояние плагина: `(enabled, contributes.views)`, если он зарегистрирован.
    fn enabled_views(&self, id: &str) -> Option<(bool, Vec<ViewContrib>)> {
        lock(&self.manager)
            .get(id)
            .map(|runtime| (runtime.is_enabled(), runtime.views.clone()))
    }

    /// `set_plugin_permissions`: согласие пользователя как **реальный** гейт прав
    /// (§8/§12, §11.1 п.4).
    ///
    /// Пересечение запрашиваемых манифестом прав и выданных пользователем
    /// (`manifest ∩ granted`, deny-by-default) сохраняется в `config.json` **до**
    /// применения — согласие фиксируется даже при сбое перезапуска. Включённый плагин
    /// перезапускается, чтобы новый набор вступил в силу (права захватываются при
    /// [`super::manager::PluginRuntime::start`]); выключенный — только запоминает набор (`reload` на
    /// выключенном = `start` ранним выходом). Карантин `reload` не сбрасывает.
    ///
    /// Сбой перезапуска снимает view плагина: иначе останется «зомби»-вкладка при
    /// статусе «ошибка» — как у [`Self::reload`].
    pub fn set_permissions(&self, id: &str, granted: Vec<String>) -> Result<(), PluginError> {
        // Запрашиваемые права + проверка существования: unknown id → ошибка, ничего не пишем.
        let (requested, enabled, contributes) = {
            let manager = lock(&self.manager);
            let runtime = manager
                .get(id)
                .ok_or_else(|| PluginError::new("unknown_plugin", format!("нет плагина {id}")))?;
            (
                runtime.permissions.clone(),
                runtime.is_enabled(),
                runtime.views.clone(),
            )
        };
        let effective = effective_permissions(&requested, &granted);

        // Персист пересечения до применения: согласие переживает сбой reload.
        if let Some(path) = &self.config_path {
            let mut settings = lock(&self.settings);
            settings.set_granted_permissions(id, effective.clone());
            if let Err(e) = settings.save(path) {
                eprintln!("не сохранить config.json: {e}");
            }
        }

        // Применение: enabled → reload (права захватываются при start), иначе no-op.
        // View гарантируем ДО перезапуска (как в [`Self::reload`]), иначе «восставший»
        // после сбоя плагин не сможет наполнить свою вкладку в `on_activate`.
        if enabled {
            let mut views = lock(&self.views);
            views.remove_plugin(id);
            views.register(id, &contributes);
        }
        let result = lock(&self.manager).set_plugin_permissions(id, effective);
        if result.is_err() {
            lock(&self.views).remove_plugin(id);
        }

        self.pump();
        (self.notify)();
        result
    }
}
