//! Менеджер плагинов: жизненный цикл, карантин и статусы (Фаза 2 TZ-H2, §4.7).
//!
//! Пока внутренний API (без Tauri-команд): `list_plugins`/`set_plugin_enabled`/`reload_plugin`
//! как контракты H2 фиксируются здесь, IPC-обёртки появятся с UI менеджера (Фаза 5).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use plugin_proto::envelope::PluginError;
use plugin_proto::limits::{DEADLINE_MS, PROGRESS_TIMEOUT_MS};
use plugin_proto::quarantine::Quarantine;
use serde::Serialize;
use serde_json::Value;

use super::supervisor::{Supervisor, SupervisorParams};
use super::{HostServices, PluginStatus};

/// Порог карантина: 3 неудачи подряд (ADR-0021 §3).
pub const QUARANTINE_THRESHOLD: u32 = 3;

/// Один плагин и его child-процесс.
pub struct PluginRuntime {
    pub id: String,
    pub permissions: Vec<String>,
    pub status: PluginStatus,
    quarantine: Quarantine,
    exe: PathBuf,
    source: String,
    services: Arc<dyn HostServices>,
    supervisor: Option<Supervisor>,
    enabled: bool,
    mem_bytes: usize,
    progress: Duration,
    deadline: Duration,
}

impl PluginRuntime {
    pub fn new(
        id: impl Into<String>,
        exe: impl Into<PathBuf>,
        source: impl Into<String>,
        permissions: Vec<String>,
        services: Arc<dyn HostServices>,
    ) -> Self {
        Self {
            id: id.into(),
            permissions,
            status: PluginStatus::Stopped,
            quarantine: Quarantine::new(QUARANTINE_THRESHOLD),
            exe: exe.into(),
            source: source.into(),
            services,
            supervisor: None,
            enabled: true,
            mem_bytes: 64 * 1024 * 1024,
            progress: Duration::from_millis(PROGRESS_TIMEOUT_MS),
            deadline: Duration::from_millis(DEADLINE_MS),
        }
    }

    /// Переопределяет бюджеты watchdog (тесты/`config.json` в Фазе 3).
    pub fn with_watchdog(mut self, progress: Duration, deadline: Duration) -> Self {
        self.progress = progress;
        self.deadline = deadline;
        self
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn is_quarantined(&self) -> bool {
        self.quarantine.is_disabled()
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

        let mut params = SupervisorParams::new(&self.exe, &self.id, Arc::clone(&self.services))
            .with_permissions(self.permissions.clone())
            .with_mem_bytes(self.mem_bytes);
        params.progress = self.progress;
        params.deadline = self.deadline;
        let started = Supervisor::start(params, &self.source)
            .and_then(|mut supervisor| supervisor.activate().map(|_| supervisor));

        match started {
            Ok(supervisor) => {
                self.supervisor = Some(supervisor);
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
                // `Supervisor` уже снял child; не держим мёртвый процесс.
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

/// Проекция плагина для UI менеджера.
#[derive(Clone, Debug, Serialize)]
pub struct PluginInfo {
    pub id: String,
    pub status: PluginStatus,
    pub permissions: Vec<String>,
    pub enabled: bool,
}

/// Набор плагинов. Фаза 5 обернёт его Tauri-командами.
///
/// **Инвариант блокировок:** host-call'ы берут `Mutex<DocumentStore>` внутри `invoke`;
/// `std::sync::Mutex` не реентрантный, поэтому вызывать операции менеджера **нельзя** под уже
/// удерживаемой блокировкой стора (иначе дедлок). При IPC-подключении (Фаза 4/5) состояние
/// станет `Arc<Mutex<DocumentStore>>` (§4.2 ревью).
#[derive(Default)]
pub struct PluginManager {
    plugins: HashMap<String, PluginRuntime>,
}

impl PluginManager {
    pub fn register(&mut self, runtime: PluginRuntime) {
        self.plugins.insert(runtime.id.clone(), runtime);
    }

    /// `list_plugins`: снимок для UI.
    pub fn list_plugins(&self) -> Vec<PluginInfo> {
        let mut infos: Vec<PluginInfo> = self
            .plugins
            .values()
            .map(|p| PluginInfo {
                id: p.id.clone(),
                status: p.status.clone(),
                permissions: p.permissions.clone(),
                enabled: p.is_enabled(),
            })
            .collect();
        infos.sort_by(|a, b| a.id.cmp(&b.id));
        infos
    }

    /// `set_plugin_enabled`: вкл/выкл плагина (включение снимает карантин).
    pub fn set_plugin_enabled(&mut self, id: &str, enabled: bool) -> Result<(), PluginError> {
        let plugin = self
            .plugins
            .get_mut(id)
            .ok_or_else(|| PluginError::new("unknown_plugin", format!("нет плагина {id}")))?;
        plugin.set_enabled(enabled);
        if enabled {
            plugin.start()?;
        }
        Ok(())
    }

    /// `reload_plugin`: перезапуск без рестарта приложения.
    pub fn reload_plugin(&mut self, id: &str) -> Result<(), PluginError> {
        let plugin = self
            .plugins
            .get_mut(id)
            .ok_or_else(|| PluginError::new("unknown_plugin", format!("нет плагина {id}")))?;
        plugin.reload()
    }

    pub fn get(&self, id: &str) -> Option<&PluginRuntime> {
        self.plugins.get(id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut PluginRuntime> {
        self.plugins.get_mut(id)
    }
}
