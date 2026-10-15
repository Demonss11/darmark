//! Жизненный цикл одного плагина: запуск/стоп/перезагрузка, права и вызовы child'а.
//!
//! Поля [`PluginRuntime`] объявлены в родительском `mod.rs`: к приватным полям обращается
//! и этот impl, и [`PluginManager`](super::PluginManager) — а приватность поля действует на
//! модуль объявления и его потомков, но не наверх.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use plugin_core::envelope::PluginError;
use plugin_core::limits::{DEADLINE_MS, DEFAULT_MEM_BYTES, PROGRESS_TIMEOUT_MS};
use plugin_core::manifest::{CommandContrib, ViewContrib};
use plugin_core::quarantine::Quarantine;
use serde_json::Value;

use super::super::supervisor::{Supervisor, SupervisorParams};
use super::{effective_permissions, PluginRuntime, QUARANTINE_THRESHOLD};
use crate::plugins::{HostServices, PluginStatus};

impl PluginRuntime {
    pub fn new(
        id: impl Into<String>,
        exe: impl Into<PathBuf>,
        source: impl Into<String>,
        permissions: Vec<String>,
        services: Arc<dyn HostServices>,
        views: Vec<ViewContrib>,
        commands: Vec<CommandContrib>,
    ) -> Self {
        Self {
            id: id.into(),
            name: String::new(),
            version: String::new(),
            permissions,
            granted: Vec::new(),
            status: PluginStatus::Stopped,
            views,
            commands,
            settings: None,
            quarantine: Quarantine::new(QUARANTINE_THRESHOLD),
            exe: exe.into(),
            source: source.into(),
            entry_path: None,
            dir: PathBuf::new(),
            services,
            supervisor: None,
            enabled: true,
            mem_bytes: DEFAULT_MEM_BYTES,
            progress: Duration::from_millis(PROGRESS_TIMEOUT_MS),
            deadline: Duration::from_millis(DEADLINE_MS),
        }
    }

    /// Задаёт человекочитаемые `name`/`version` из манифеста (IPC-проекция `PluginInfo`, S12a).
    pub fn with_identity(mut self, name: impl Into<String>, version: impl Into<String>) -> Self {
        self.name = name.into();
        self.version = version.into();
        self
    }

    /// Переопределяет бюджеты watchdog (тесты/`config.json` в Фазе 3).
    pub fn with_watchdog(mut self, progress: Duration, deadline: Duration) -> Self {
        self.progress = progress;
        self.deadline = deadline;
        self
    }

    /// Задаёт путь `.lua` на диске: с ним `start`/`reload` читают свежий исходник (§4.7).
    pub fn with_entry_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.entry_path = Some(path.into());
        self
    }

    /// Задаёт каталог плагина на диске — для удаления (`remove_plugin`).
    pub fn with_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.dir = dir.into();
        self
    }

    /// Задаёт согласованные пользователем права (обычно из
    /// [`SettingsStore`](super::super::settings::SettingsStore)).
    pub fn with_granted(mut self, granted: Vec<String>) -> Self {
        self.granted = granted;
        self
    }

    /// Задаёт схему настроек из манифеста (`contributes.settings`) — для UI-формы (S5b).
    /// `None`, если плагин настроек не объявил.
    pub fn with_settings_schema(mut self, schema: Option<Value>) -> Self {
        self.settings = schema;
        self
    }

    /// Согласованные пользователем права (может содержать мусор из конфига — его отсекает
    /// [`Self::effective`] при старте).
    pub fn granted(&self) -> &[String] {
        &self.granted
    }

    /// Обновляет согласованные права. Ортогонально [`Self::is_enabled`]: пустой набор
    /// не мешает старту — плагин поднимается с нулём прав (гейт срабатывает на host-call).
    pub fn set_granted(&mut self, granted: Vec<String>) {
        self.granted = granted;
    }

    /// Эффективные права: `manifest ∩ granted` (deny-by-default). Именно они уходят в
    /// разрешения child'а при старте, поэтому неподтверждённое манифестом разрешение
    /// до child'а не доходит.
    pub fn effective(&self) -> Vec<String> {
        effective_permissions(&self.permissions, &self.granted)
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn is_quarantined(&self) -> bool {
        self.quarantine.is_disabled()
    }

    /// PID живого child'а — только у `Active`-плагина, владеющего `Supervisor` (ADR-0031 §2.6).
    ///
    /// `Stopped`/`Failed`/`Quarantined` процессом не владеют → `None` (границы §2.2 п.7).
    pub fn pid(&self) -> Option<u32> {
        if matches!(self.status, PluginStatus::Active) {
            self.supervisor.as_ref().map(Supervisor::pid)
        } else {
            None
        }
    }

    /// `spawn → load → on_activate`. Ошибка запуска копит серию карантина.
    pub fn start(&mut self) -> Result<(), PluginError> {
        self.stop();
        if !self.enabled {
            return Ok(());
        }
        if self.quarantine.is_disabled() {
            self.status = PluginStatus::Quarantined;
            return Err(PluginError::new(
                "quarantined",
                format!("{} в карантине — включите вручную", self.id),
            ));
        }
        let source = match &self.entry_path {
            Some(path) => match super::super::scan::read_source(path) {
                Ok(source) => source,
                Err(message) => {
                    let error = PluginError::new("load_error", message);
                    self.quarantine.record_failure();
                    self.status = if self.quarantine.is_disabled() {
                        PluginStatus::Quarantined
                    } else {
                        PluginStatus::Failed {
                            message: error.message.clone(),
                        }
                    };
                    return Err(error);
                }
            },
            None => self.source.clone(),
        };

        let mut params = SupervisorParams::new(&self.exe, &self.id, Arc::clone(&self.services))
            .with_permissions(self.effective())
            .with_mem_bytes(self.mem_bytes);
        params.progress = self.progress;
        params.deadline = self.deadline;
        let started = Supervisor::start(params, &source)
            .and_then(|mut supervisor| supervisor.activate().map(|_| supervisor));

        match started {
            Ok(supervisor) => {
                self.supervisor = Some(supervisor);
                self.source = source;
                self.quarantine.record_success();
                self.status = PluginStatus::Active;
                Ok(())
            }
            Err(error) => {
                self.supervisor = None;
                self.quarantine.record_failure();
                self.status = if self.quarantine.is_disabled() {
                    PluginStatus::Quarantined
                } else {
                    PluginStatus::Failed {
                        message: error.message.clone(),
                    }
                };
                Err(error)
            }
        }
    }

    /// Останавливает child (best-effort `on_deactivate`, затем снятие Job).
    pub fn stop(&mut self) {
        if let Some(mut supervisor) = self.supervisor.take() {
            let _ = supervisor.deactivate();
        }
        self.status = PluginStatus::Stopped;
    }

    /// Ручная перезагрузка: снять старый child и загрузить файл заново (без рестарта GUI).
    pub fn reload(&mut self) -> Result<(), PluginError> {
        self.start()
    }

    /// Включение/выключение. Ручное включение снимает карантин (§7 риски).
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if enabled {
            self.quarantine.reset();
        }
        self.stop();
    }

    /// Доставляет событие шины плагину (`event`). Отказ invocation снимает child и
    /// копит серию карантина — тот же путь, что у [`PluginRuntime::invoke`].
    pub fn dispatch_event(&mut self, name: &str, payload: Value) -> Result<Value, PluginError> {
        self.invoke(
            "event",
            serde_json::json!({ "name": name, "payload": payload }),
        )
    }

    /// Вызов метода активного плагина.
    ///
    /// Отказ invocation (краш/дедлайн/прогресс/протокол) снимает child: supervisor
    /// обнуляется, серия неудач копится, статус уходит из `Active`. Иначе остаётся
    /// «зомби»-supervisor с мёртвым stdin (§3.2 ревью).
    pub fn invoke(&mut self, method: &str, args: Value) -> Result<Value, PluginError> {
        let result = match self.supervisor.as_mut() {
            Some(supervisor) => supervisor.invoke(method, args),
            None => {
                return Err(PluginError::new(
                    "not_running",
                    format!("{} не запущен", self.id),
                ))
            }
        };

        match result {
            Ok(value) => Ok(value),
            Err(error) => {
                self.supervisor = None;
                self.quarantine.record_failure();
                self.status = if self.quarantine.is_disabled() {
                    PluginStatus::Quarantined
                } else {
                    PluginStatus::Failed {
                        message: error.message.clone(),
                    }
                };
                Err(error)
            }
        }
    }
}
