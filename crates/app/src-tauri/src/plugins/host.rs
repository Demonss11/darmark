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

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;

use plugin_core::envelope::PluginError;
use plugin_core::manifest::ViewContrib;
use serde_json::{json, Value};

use crate::{PluginInfo, PluginStatus, PluginViewInfo};

use super::bus::EventBus;
use super::manager::{effective_permissions, PluginManager, PluginRuntime};
use super::plugin_settings::PluginSettingsStore;
use super::resource::{ProcessMetrics, ResourceMonitor, ResourceSink};
use super::services::PendingChanged;
use super::settings::SettingsStore;
use super::supervisor::Supervisor;
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

    /// Запускает поток ресурс-монитора (ADR-0031 §2.3 п.9).
    ///
    /// Список активных PID снимается коротким локом менеджера **внутри** потока
    /// ([`PluginManager::active_pids`]); WinAPI-замеры и эмит событий идут уже без блокировки
    /// (§2.3 п.10 — дедлок-инвариант). Поток останавливается флагом в [`Drop`].
    pub fn start_resource_monitor<S: ResourceSink + 'static>(
        &mut self,
        provider: Arc<dyn ProcessMetrics>,
        sink: Arc<S>,
        mem_limit_bytes: usize,
    ) {
        let manager = Arc::clone(&self.manager);
        let stop = Arc::clone(&self.resource_stop);
        let monitor = ResourceMonitor::new(provider, sink, std::process::id(), mem_limit_bytes);
        // Снимок PID берём **неблокирующим** `try_lock` (ADR-0031 §2.3 п.10): пока лок
        // менеджера держит, например, диалог, поток не ждёт его, а пропускает тик
        // (`None`). Иначе блокировка менеджера могла бы задержать опрос на неопределённо
        // долгий срок. `Err` покрывает и `WouldBlock`, и отравление (в тестах).
        let children = move || match manager.try_lock() {
            Ok(guard) => Some(guard.active_pids()),
            Err(_) => None,
        };
        self.resource_thread = Some(monitor.spawn(children, stop));
    }

    /// Публикует `document-event` (S11): действие пользователя над документом.
    ///
    /// `kind` — `new`/`open`/`save`/`close`; `path`/`rev` опциональны. Доставка — через
    /// [`DocumentEventSink`] (Tauri-событие `document-event`), подключённый в `setup_plugins`.
    pub fn publish_document_event(&self, kind: &str, path: Option<&str>, rev: Option<u64>) {
        (self.document_event_sink)(kind, path, rev);
    }

    /// Снимок статуса плагина (для сравнения «до/после» при эмиссии `plugin-status-changed`).
    fn runtime_status(&self, id: &str) -> Option<PluginStatus> {
        lock(&self.manager)
            .get(id)
            .map(|runtime| runtime.status.clone())
    }

    /// Эмитит `plugin-status-changed` для одного перехода. `message` — диагностический
    /// текст (при `failed` фолбэк на сообщение из [`PluginStatus::Failed`]).
    fn emit_status_transition(
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

    /// Уведомляет фронтенд о смене реестра/контента представлений (`plugin-views-changed`).
    fn notify_views(&self) {
        (self.views_notify)();
    }

    /// Уведомляет менеджер о смене состава/статуса/карантина (`plugins-changed`).
    fn notify_plugins(&self) {
        (self.plugins_notify)();
    }

    /// Оба сигнала: реестр view и статусы изменились вместе (enable/reload/remove/permissions).
    fn notify_all(&self) {
        self.notify_views();
        self.notify_plugins();
    }

    /// `plugin_views`: снимок реестра представлений для фронтенда.
    pub fn plugin_views(&self) -> Vec<PluginViewInfo> {
        lock(&self.views).snapshot()
    }

    /// `list_plugins`: снимок реестра плагинов для менеджера UI (Фаза 5).
    ///
    /// Схема настроек (`settings_schema`) приходит из манифеста (менеджер); текущие
    /// значения (`settings_values`) обогащаются из `PluginSettingsStore`, если он подключён
    /// (S5b). Без стора значения остаются пустым объектом.
    pub fn list_plugins(&self) -> Vec<PluginInfo> {
        let mut infos = lock(&self.manager).list_plugins();
        if let Some(settings) = &self.plugin_settings {
            let mut store = lock(settings);
            for info in &mut infos {
                // Небезопасный id в реестр не попадает (валидирован манифестом), но
                // ошибку `all` не разносим: значение остаётся пустым объектом.
                if let Ok(values) = store.all(&info.id) {
                    info.settings_values = values;
                }
            }
        }
        infos
    }

    /// Регистрирует один плагин из каталога на диске (harness AI, ADR-0034 §2.4).
    ///
    /// Читает каталог тем же контрактом, что и [`super::manager::load_plugins`] (манифест +
    /// entry), добавляет плагин в реестр и, если он включён в настройках, запускает child.
    /// `Err` — каталог невалиден, плагин с таким id уже в реестре или child-exe не найден.
    pub fn register_plugin_dir(&self, dir: &Path) -> Result<PluginInfo, PluginError> {
        let discovered =
            super::scan::load_single(dir).map_err(|m| PluginError::new("invalid_plugin", m))?;
        let id = discovered.manifest.id.clone();
        if lock(&self.manager).get(&id).is_some() {
            return Err(PluginError::new(
                "plugin_exists",
                format!("плагин уже зарегистрирован: {id}"),
            ));
        }
        let exe = Supervisor::resolve_child_exe().ok_or_else(|| {
            PluginError::new(
                "no_host",
                format!(
                    "{} не найден рядом с darmark.exe",
                    super::supervisor::CHILD_EXE_NAME
                ),
            )
        })?;

        let (enabled, granted) = {
            let settings = lock(&self.settings);
            (
                settings.plugin_enabled(&id),
                effective_permissions(
                    &discovered.manifest.permissions,
                    &settings.granted_permissions(&id),
                ),
            )
        };
        let contributes = discovered.manifest.contributes.views.clone();
        let commands = discovered.manifest.contributes.commands.clone();
        let settings_schema = discovered.manifest.contributes.settings.clone();
        let name = discovered.manifest.name.clone();
        let version = discovered.manifest.version.clone();

        // Регистрация view — только для включённого плагина (deny-by-default, как load_plugins).
        if enabled {
            lock(&self.views).sync(&id, &contributes, &granted);
        } else {
            lock(&self.views).remove_plugin(&id);
        }

        let mut runtime = PluginRuntime::new(
            id.clone(),
            exe,
            discovered.source,
            discovered.manifest.permissions.clone(),
            Arc::clone(&self.services),
            contributes,
            commands,
        )
        .with_identity(name, version)
        .with_entry_path(discovered.entry_path)
        .with_dir(discovered.dir)
        .with_granted(granted)
        .with_settings_schema(settings_schema);
        runtime.set_enabled(enabled);
        if enabled {
            // Best-effort: ошибка старта отразится в статусе плагина (как start_enabled).
            let _ = runtime.start();
        }
        lock(&self.manager).register(runtime);

        self.pump();
        self.notify_all();
        lock(&self.manager)
            .list_plugins()
            .into_iter()
            .find(|info| info.id == id)
            .ok_or_else(|| PluginError::new("register_failed", "плагин не появился в реестре"))
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
        // Отказ invocation увёл статус в failed/quarantined — сообщаем менеджеру.
        // Реестр view не менялся: только `plugins-changed` (F41/S2).
        let failed = result.is_err();
        // `on_action` мог править документ (`apply_edit`) — дреним manager до pump.
        self.pump();
        if failed {
            self.notify_plugins();
        }
        result
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
        // Переходы статуса копим и эмитим после отпускания блокировки `manager` (S11):
        // sink — это Tauri-emit, под мьютексом менеджера его вызывать не следует.
        let mut transitions: Vec<(String, PluginStatus, PluginStatus, Option<String>)> = Vec::new();
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
            // Сменился статус/карантин, но не реестр view → только `plugins-changed` (F41/S2).
            self.notify_plugins();
        }
    }

    /// Переиздаёт конкретному плагину `document:opened` для последнего документа.
    ///
    /// Нужен после (ре)старта view-плагина: contributed view уже в реестре, child снова
    /// активен, но его `on_activate` не наполнит вкладку, если плагин строит её по
    /// подписке на `document:opened`/`changed`. Адресно — только этому плагину, чтобы
    /// не дублировать событие остальным.
    fn reannounce_opened(&self, plugin_id: &str) {
        let last = lock(&self.last_doc).clone();
        let Some(last) = last else { return };
        let payload = json!({ "doc_id": last.id, "path": last.path });
        let mut manager = lock(&self.manager);
        if let Some(runtime) = manager.get_mut(plugin_id) {
            if matches!(runtime.status, super::PluginStatus::Active) {
                // Отказ доставки уводит статус в failed/quarantined — это уже обработает
                // последующий pump/notify владельца вызова.
                let _ = runtime.dispatch_event("document:opened", payload);
            }
        }
    }

    /// Best-effort старт enabled-плагинов и уведомление фронтенда.
    ///
    /// Ошибка запуска одного плагина не мешает остальным и не роняет GUI (§4.7).
    pub fn start_enabled(&self) {
        // Переходы статуса при автозапуске (S11) — копим и эмитим после manager-блокировки.
        let mut transitions: Vec<(String, PluginStatus, PluginStatus, Option<String>)> = Vec::new();
        {
            let mut manager = lock(&self.manager);
            for runtime in manager.iter_mut() {
                if runtime.is_enabled() {
                    let old = runtime.status.clone();
                    match runtime.start() {
                        Ok(()) => {
                            if runtime.status != old {
                                transitions.push((
                                    runtime.id.clone(),
                                    old,
                                    runtime.status.clone(),
                                    None,
                                ));
                            }
                        }
                        Err(error) => {
                            eprintln!("плагин {}: не запущен: {}", runtime.id, error.message);
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
        }
        for (id, old, new, message) in &transitions {
            self.emit_status_transition(id, old, new, message.as_deref());
        }
        // on_activate мог править документ — доставляем отложенные события и уведомляем.
        // Состав/статусы изменились (старт) — оба сигнала.
        self.pump();
        self.notify_all();
    }

    /// `set_plugin_setting`: записывает настройку плагина из UI-формы (S5b).
    ///
    /// В отличие от host-call `set_setting` (scope — внедрённый `Supervisor` `_plugin_id`),
    /// здесь значение приходит от пользователя: владелец (`id`) задаётся явно. Ошибка
    /// `no_settings`, если хранилище не подключено. После записи — [`Self::notify_plugins`]:
    /// менеджер перечитает `list_plugins` и покажет новое значение.
    pub fn plugin_setting_set(&self, id: &str, key: &str, value: Value) -> Result<(), PluginError> {
        let store = self.plugin_settings.as_ref().ok_or_else(|| {
            PluginError::new(
                "no_settings",
                "хранилище настроек недоступно в этом окружении",
            )
        })?;
        {
            let mut guard = lock(store);
            guard.set(id, key, value)?;
        }
        self.notify_plugins();
        Ok(())
    }

    /// `set_plugin_enabled`: вкл/выкл плагина (IPC-команда менеджера, Фаза 5).
    ///
    /// **Порядок критичен:** при включении contributed view регистрируются в реестре
    /// **до** старта child (и только при выданном `view:create`, F44/S4) — иначе
    /// `host.set_view_content` из `on_activate` вернул бы `unknown_view`. При выключении —
    /// наоборот: стоп, затем снятие view (нет «зомби»).
    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), PluginError> {
        let (views_contrib, effective, old_status) = {
            let manager = lock(&self.manager);
            let runtime = manager
                .get(id)
                .ok_or_else(|| PluginError::new("unknown_plugin", format!("нет плагина {id}")))?;
            (
                runtime.views.clone(),
                runtime.effective(),
                runtime.status.clone(),
            )
        };

        // View регистрируем ДО старта child (иначе `host.set_view_content` из `on_activate`
        // вернул бы `unknown_view`), но только при выданном `view:create` (F44/S4).
        if enabled {
            lock(&self.views).sync(id, &views_contrib, &effective);
        }

        let result = lock(&self.manager).set_plugin_enabled(id, enabled);

        // Неудачный старт или выключение — view плагина не должны висеть в реестре.
        if !enabled || result.is_err() {
            lock(&self.views).remove_plugin(id);
        }

        // Успешный (ре)старт: view снова в реестре, child активен — переиздаём
        // последний `document:opened`, иначе наполняемая по подписке вкладка пуста.
        if result.is_ok() && enabled {
            self.reannounce_opened(id);
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

        // `on_activate` мог править документ; вкл/выкл меняет состав/статус и реестр view.
        // Диагностика (S11): переход статуса (в т.ч. отказ старта → failed/quarantined).
        if let Some(new_status) = self.runtime_status(id) {
            let message = result.as_ref().err().map(|e| e.message.as_str());
            self.emit_status_transition(id, &old_status, &new_status, message);
        }
        self.pump();
        self.notify_all();
        result
    }

    /// `remove_plugin`: полное удаление плагина.
    ///
    /// Порядок критичен: сначала снимаем views и останавливаем child, потом удаляем
    /// настройки и каталог с диска. Ошибка удаления файлов не откатывает удаление
    /// из реестра — плагин уже не запустится, а каталог можно удалить вручную.
    pub fn remove(&self, id: &str) -> Result<(), PluginError> {
        let old_status = self.runtime_status(id);
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

        // Удаляем постоянные настройки плагина (F43/S5): отдельный файл per-plugin.
        if let Some(settings) = &self.plugin_settings {
            if let Err(e) = lock(settings).remove_plugin(id) {
                eprintln!("не удалить настройки плагина {id}: {}", e.message);
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
        // Удаление меняет и реестр view, и состав плагинов — оба сигнала.
        // Диагностика (S11): плагин исчез из реестра — фиксируем переход в `stopped`.
        if let Some(old) = old_status {
            self.emit_status_transition(id, &old, &PluginStatus::Stopped, Some("плагин удалён"));
        }
        self.notify_all();
        Ok(())
    }

    /// `reload_plugin`: перезапуск без рестарта GUI (Фаза 5).
    ///
    /// Неудачная перезагрузка (синтаксис и т.п.) снимает view плагина: иначе остаётся
    /// «зомби»-вкладка со старым HTML при статусе «ошибка».
    pub fn reload(&self, id: &str) -> Result<(), PluginError> {
        let old_status = self.runtime_status(id);
        // Для включённого плагина гарантируем contributed view в реестре ДО старта
        // child: предыдущий сбой мог снять view, а `on_activate` пишет HTML через
        // `set_view_content` — иначе `unknown_view` и потерянный контент. Регистрация —
        // только при выданном `view:create` (F44/S4).
        if let Some((true, contributes, effective)) = self.runtime_view_state(id) {
            lock(&self.views).sync(id, &contributes, &effective);
        }
        let result = lock(&self.manager).reload_plugin(id);
        if result.is_err() {
            lock(&self.views).remove_plugin(id);
        } else {
            // View перерегистрирован выше, child перезапущен: переиздаём последний
            // `document:opened`, чтобы вкладка по подписке не осталась пустой.
            self.reannounce_opened(id);
        }
        // reload — точка смены статуса (TZ §5.2): эмитим **всегда**, даже если итоговое
        // состояние осталось `active` (сценарий «плагин перезагружен»).
        if let (Some(old), Some(new)) = (old_status, self.runtime_status(id)) {
            let message = result.as_ref().err().map(|e| e.message.as_str());
            self.emit_status_transition(id, &old, &new, message);
        }
        self.pump();
        // reload меняет статус и перерегистрирует view — оба сигнала.
        self.notify_all();
        result
    }

    /// Состояние плагина: `(enabled, contributes.views, effective permissions)`, если он
    /// зарегистрирован. Эффективный набор нужен для гейта регистрации view (`view:create`).
    fn runtime_view_state(&self, id: &str) -> Option<(bool, Vec<ViewContrib>, Vec<String>)> {
        lock(&self.manager).get(id).map(|runtime| {
            (
                runtime.is_enabled(),
                runtime.views.clone(),
                runtime.effective(),
            )
        })
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
        let (requested, enabled, contributes, old_status) = {
            let manager = lock(&self.manager);
            let runtime = manager
                .get(id)
                .ok_or_else(|| PluginError::new("unknown_plugin", format!("нет плагина {id}")))?;
            (
                runtime.permissions.clone(),
                runtime.is_enabled(),
                runtime.views.clone(),
                runtime.status.clone(),
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
        // View синхронизируем с новым эффективным набором ДО перезапуска: выдача
        // `view:create` регистрирует вкладку, отзыв — снимает (F44/S4). Иначе «восставший»
        // после сбоя плагин не сможет наполнить свою вкладку в `on_activate`.
        if enabled {
            lock(&self.views).sync(id, &contributes, &effective);
        }
        let result = lock(&self.manager).set_plugin_permissions(id, effective);
        if result.is_err() {
            lock(&self.views).remove_plugin(id);
        } else if enabled {
            // Права применились и плагин перезапущен: переиздаём последний
            // `document:opened`, чтобы view по подписке не сбросился в пустой.
            self.reannounce_opened(id);
        }

        // Диагностика (S11): смена прав перезапускает плагин — фиксируем переход статуса,
        // если он случился.
        if let Some(new_status) = self.runtime_status(id) {
            if new_status != old_status {
                let message = result.as_ref().err().map(|e| e.message.as_str());
                self.emit_status_transition(id, &old_status, &new_status, message);
            }
        }
        self.pump();
        // Смена прав меняет статус (перезапуск) и перерегистрирует view — оба сигнала.
        self.notify_all();
        result
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
