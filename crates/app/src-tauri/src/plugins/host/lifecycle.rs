//! Жизненный цикл плагина на стороне хоста: автозапуск, вкл/выкл, удаление, перезагрузка,
//! права, настройки и поток ресурс-монитора.

use std::sync::Arc;

use plugin_core::envelope::PluginError;
use serde_json::Value;

use crate::PluginStatus;

use super::super::manager::effective_permissions;
use super::super::resource::{ProcessMetrics, ResourceMonitor, ResourceSink};
use super::{lock, PluginHost};

impl PluginHost {
    
    
    
    
    
    pub fn start_resource_monitor<S: ResourceSink + 'static>(
        &mut self,
        provider: Arc<dyn ProcessMetrics>,
        sink: Arc<S>,
        mem_limit_bytes: usize,
    ) {
        let manager = Arc::clone(&self.manager);
        let stop = Arc::clone(&self.resource_stop);
        let monitor = ResourceMonitor::new(provider, sink, std::process::id(), mem_limit_bytes);
        let children = move || match manager.try_lock() {
            Ok(guard) => Some(guard.active_pids()),
            Err(_) => None,
        };
        self.resource_thread = Some(monitor.spawn(children, stop));
    }

    
    
    
    pub fn start_enabled(&self) {
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
        self.pump();
        self.notify_all();
    }

    
    
    
    
    
    
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
        if enabled {
            lock(&self.views).sync(id, &views_contrib, &effective);
        }

        let result = lock(&self.manager).set_plugin_enabled(id, enabled);
        if !enabled || result.is_err() {
            lock(&self.views).remove_plugin(id);
        }
        if result.is_ok() && enabled {
            self.reannounce_opened(id);
        }
        if result.is_ok() {
            if let Some(path) = &self.config_path {
                let mut settings = lock(&self.settings);
                settings.set_plugin_enabled(id, enabled);
                if let Err(e) = settings.save(path) {
                    eprintln!("не сохранить config.json: {e}");
                }
            }
        }
        if let Some(new_status) = self.runtime_status(id) {
            let message = result.as_ref().err().map(|e| e.message.as_str());
            self.emit_status_transition(id, &old_status, &new_status, message);
        }
        self.pump();
        self.notify_all();
        result
    }

    
    
    
    
    
    pub fn remove(&self, id: &str) -> Result<(), PluginError> {
        let old_status = self.runtime_status(id);
        lock(&self.views).remove_plugin(id);
        let dir = lock(&self.manager).remove_plugin(id)?;
        if let Some(path) = &self.config_path {
            let mut settings = lock(&self.settings);
            settings.remove_plugin(id);
            if let Err(e) = settings.save(path) {
                eprintln!("не сохранить config.json: {e}");
            }
        }
        if let Some(settings) = &self.plugin_settings {
            if let Err(e) = lock(settings).remove_plugin(id) {
                eprintln!("не удалить настройки плагина {id}: {}", e.message);
            }
        }
        if !dir.as_os_str().is_empty() && dir.exists() {
            if let Err(e) = std::fs::remove_dir_all(&dir) {
                return Err(PluginError::new(
                    "remove_failed",
                    format!("{}: {e}", dir.display()),
                ));
            }
        }

        self.pump();
        if let Some(old) = old_status {
            self.emit_status_transition(id, &old, &PluginStatus::Stopped, Some("плагин удалён"));
        }
        self.notify_all();
        Ok(())
    }

    
    
    
    
    pub fn reload(&self, id: &str) -> Result<(), PluginError> {
        let old_status = self.runtime_status(id);
        if let Some((true, contributes, effective)) = self.runtime_view_state(id) {
            lock(&self.views).sync(id, &contributes, &effective);
        }
        let result = lock(&self.manager).reload_plugin(id);
        if result.is_err() {
            lock(&self.views).remove_plugin(id);
        } else {
            self.reannounce_opened(id);
        }
        if let (Some(old), Some(new)) = (old_status, self.runtime_status(id)) {
            let message = result.as_ref().err().map(|e| e.message.as_str());
            self.emit_status_transition(id, &old, &new, message);
        }
        self.pump();
        self.notify_all();
        result
    }

    
    
    
    
    
    
    
    
    
    
    
    
    pub fn set_permissions(&self, id: &str, granted: Vec<String>) -> Result<(), PluginError> {
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
        if let Some(path) = &self.config_path {
            let mut settings = lock(&self.settings);
            settings.set_granted_permissions(id, effective.clone());
            if let Err(e) = settings.save(path) {
                eprintln!("не сохранить config.json: {e}");
            }
        }
        if enabled {
            lock(&self.views).sync(id, &contributes, &effective);
        }
        let result = lock(&self.manager).set_plugin_permissions(id, effective);
        if result.is_err() {
            lock(&self.views).remove_plugin(id);
        } else if enabled {
            self.reannounce_opened(id);
        }
        if let Some(new_status) = self.runtime_status(id) {
            if new_status != old_status {
                let message = result.as_ref().err().map(|e| e.message.as_str());
                self.emit_status_transition(id, &old_status, &new_status, message);
            }
        }
        self.pump();
        self.notify_all();
        result
    }
}
