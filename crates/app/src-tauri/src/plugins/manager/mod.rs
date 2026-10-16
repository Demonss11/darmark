//! Менеджер плагинов: жизненный цикл, карантин и статусы (Фаза 2 плагинной системы, §4.7).
//!
//! Пока внутренний API (без Tauri-команд): `list_plugins`/`set_plugin_enabled`/`reload_plugin`
//! как контракты H2 фиксируются здесь, IPC-обёртки появятся с UI менеджера (Фаза 5).

mod runtime;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use plugin_core::envelope::PluginError;
use plugin_core::manifest::{CommandContrib, ViewContrib};
use plugin_core::quarantine::Quarantine;
use serde_json::Value;

use super::settings::SettingsStore;
use super::supervisor::Supervisor;
use super::views::PluginViews;
use super::{HostServices, PluginStatus};
use crate::{CommandInfo, PluginInfo};


pub const QUARANTINE_THRESHOLD: u32 = 3;







pub fn effective_permissions(manifest: &[String], granted: &[String]) -> Vec<String> {
    manifest
        .iter()
        .filter(|permission| granted.iter().any(|grant| grant == *permission))
        .cloned()
        .collect()
}


pub struct PluginRuntime {
    pub id: String,
    
    pub name: String,
    
    pub version: String,
    
    pub permissions: Vec<String>,
    
    
    granted: Vec<String>,
    pub status: PluginStatus,
    
    
    #[allow(dead_code)]
    pub views: Vec<ViewContrib>,
    
    
    #[allow(dead_code)]
    pub commands: Vec<CommandContrib>,
    
    
    pub settings: Option<Value>,
    quarantine: Quarantine,
    exe: PathBuf,
    
    source: String,
    
    entry_path: Option<PathBuf>,
    
    dir: PathBuf,
    services: Arc<dyn HostServices>,
    supervisor: Option<Supervisor>,
    enabled: bool,
    mem_bytes: usize,
    progress: Duration,
    deadline: Duration,
}







#[derive(Default)]
pub struct PluginManager {
    plugins: HashMap<String, PluginRuntime>,
}

impl PluginManager {
    pub fn register(&mut self, runtime: PluginRuntime) {
        self.plugins.insert(runtime.id.clone(), runtime);
    }

    
    pub fn list_plugins(&self) -> Vec<PluginInfo> {
        let mut infos: Vec<PluginInfo> = self
            .plugins
            .values()
            .map(|p| PluginInfo {
                id: p.id.clone(),
                name: p.name.clone(),
                version: p.version.clone(),
                status: p.status.clone(),
                permissions: p.permissions.clone(),
                granted_permissions: p.granted().to_vec(),
                enabled: p.is_enabled(),
                commands: p
                    .commands
                    .iter()
                    .map(|c| CommandInfo {
                        id: c.id.clone(),
                        title: c.title.clone(),
                        keybinding: c.keybinding.clone(),
                    })
                    .collect(),
                notices: plugin_core::notices::permission_notices(&p.permissions)
                    .into_iter()
                    .map(str::to_string)
                    .collect(),
                settings_schema: p.settings.clone(),
                settings_values: serde_json::json!({}),
            })
            .collect();
        infos.sort_by(|a, b| a.id.cmp(&b.id));
        infos
    }

    
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

    
    pub fn reload_plugin(&mut self, id: &str) -> Result<(), PluginError> {
        let plugin = self
            .plugins
            .get_mut(id)
            .ok_or_else(|| PluginError::new("unknown_plugin", format!("нет плагина {id}")))?;
        plugin.reload()
    }

    
    
    
    
    
    pub fn remove_plugin(&mut self, id: &str) -> Result<PathBuf, PluginError> {
        let mut plugin = self
            .plugins
            .remove(id)
            .ok_or_else(|| PluginError::new("unknown_plugin", format!("нет плагина {id}")))?;
        plugin.stop();
        Ok(plugin.dir)
    }

    
    
    
    
    
    
    
    pub fn set_plugin_permissions(
        &mut self,
        id: &str,
        granted: Vec<String>,
    ) -> Result<(), PluginError> {
        let plugin = self
            .plugins
            .get_mut(id)
            .ok_or_else(|| PluginError::new("unknown_plugin", format!("нет плагина {id}")))?;
        let effective = effective_permissions(&plugin.permissions, &granted);
        plugin.set_granted(effective);
        if plugin.is_enabled() {
            plugin.reload()?;
        }
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&PluginRuntime> {
        self.plugins.get(id)
    }

    
    
    
    
    
    
    pub fn active_pids(&self) -> Vec<(String, u32)> {
        let mut pids: Vec<(String, u32)> = self
            .plugins
            .values()
            .filter_map(|plugin| plugin.pid().map(|pid| (plugin.id.clone(), pid)))
            .collect();
        pids.sort_by(|a, b| a.0.cmp(&b.0));
        pids
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut PluginRuntime> {
        self.plugins.get_mut(id)
    }

    
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut PluginRuntime> {
        self.plugins.values_mut()
    }

    
    pub fn iter_active_mut(&mut self) -> impl Iterator<Item = &mut PluginRuntime> {
        self.plugins
            .values_mut()
            .filter(|p| matches!(p.status, PluginStatus::Active))
    }
}


pub struct LoadResult {
    pub manager: PluginManager,
    pub errors: Vec<super::scan::ScanError>,
}







pub fn load_plugins(
    plugins_dir: &Path,
    exe: &Path,
    services: Arc<dyn HostServices>,
    settings: &SettingsStore,
    views: Arc<Mutex<PluginViews>>,
) -> LoadResult {
    let report = super::scan::scan_plugins(plugins_dir);
    let mut manager = PluginManager::default();
    let mut errors = report.errors;
    let mut seen = std::collections::HashSet::new();
    for discovered in report.plugins {
        let id = discovered.manifest.id.clone();
        if !seen.insert(id.clone()) {
            errors.push(super::scan::ScanError {
                dir: discovered.dir,
                message: format!("дублирующийся id плагина: {id}"),
            });
            continue;
        }
        let enabled = settings.plugin_enabled(&id);
        let granted = effective_permissions(
            &discovered.manifest.permissions,
            &settings.granted_permissions(&id),
        );
        let contributes = discovered.manifest.contributes.views.clone();
        let commands = discovered.manifest.contributes.commands.clone();
        let settings_schema = discovered.manifest.contributes.settings.clone();
        let name = discovered.manifest.name.clone();
        let version = discovered.manifest.version.clone();
        if let Ok(mut views) = views.lock() {
            if enabled {
                views.sync(&id, &contributes, &granted);
            } else {
                views.remove_plugin(&id);
            }
        }
        let mut runtime = PluginRuntime::new(
            id,
            exe,
            discovered.source,
            discovered.manifest.permissions.clone(),
            Arc::clone(&services),
            contributes,
            commands,
        )
        .with_identity(name, version)
        .with_entry_path(discovered.entry_path)
        .with_dir(discovered.dir)
        .with_granted(granted)
        .with_settings_schema(settings_schema);
        runtime.set_enabled(enabled);
        manager.register(runtime);
    }
    LoadResult { manager, errors }
}
