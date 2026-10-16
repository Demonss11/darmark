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

    
    pub fn with_identity(mut self, name: impl Into<String>, version: impl Into<String>) -> Self {
        self.name = name.into();
        self.version = version.into();
        self
    }

    
    pub fn with_watchdog(mut self, progress: Duration, deadline: Duration) -> Self {
        self.progress = progress;
        self.deadline = deadline;
        self
    }

    
    pub fn with_entry_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.entry_path = Some(path.into());
        self
    }

    
    pub fn with_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.dir = dir.into();
        self
    }

    
    
    pub fn with_granted(mut self, granted: Vec<String>) -> Self {
        self.granted = granted;
        self
    }

    
    
    pub fn with_settings_schema(mut self, schema: Option<Value>) -> Self {
        self.settings = schema;
        self
    }

    
    
    pub fn granted(&self) -> &[String] {
        &self.granted
    }

    
    
    pub fn set_granted(&mut self, granted: Vec<String>) {
        self.granted = granted;
    }

    
    
    
    pub fn effective(&self) -> Vec<String> {
        effective_permissions(&self.permissions, &self.granted)
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn is_quarantined(&self) -> bool {
        self.quarantine.is_disabled()
    }

    
    
    
    pub fn pid(&self) -> Option<u32> {
        if matches!(self.status, PluginStatus::Active) {
            self.supervisor.as_ref().map(Supervisor::pid)
        } else {
            None
        }
    }

    
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

    
    pub fn stop(&mut self) {
        if let Some(mut supervisor) = self.supervisor.take() {
            let _ = supervisor.deactivate();
        }
        self.status = PluginStatus::Stopped;
    }

    
    pub fn reload(&mut self) -> Result<(), PluginError> {
        self.start()
    }

    
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if enabled {
            self.quarantine.reset();
        }
        self.stop();
    }

    
    
    pub fn dispatch_event(&mut self, name: &str, payload: Value) -> Result<Value, PluginError> {
        self.invoke(
            "event",
            serde_json::json!({ "name": name, "payload": payload }),
        )
    }

    
    
    
    
    
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
