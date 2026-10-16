//! Проекция реестра для UI и регистрация плагина из каталога на диске (ADR-0034 §2.4).

use std::path::Path;
use std::sync::Arc;

use plugin_core::envelope::PluginError;

use crate::PluginInfo;

use super::super::manager::{effective_permissions, PluginRuntime};
use super::super::supervisor::Supervisor;
use super::{lock, PluginHost};

impl PluginHost {
    
    
    
    
    
    pub fn list_plugins(&self) -> Vec<PluginInfo> {
        let mut infos = lock(&self.manager).list_plugins();
        if let Some(settings) = &self.plugin_settings {
            let mut store = lock(settings);
            for info in &mut infos {
                if let Ok(values) = store.all(&info.id) {
                    info.settings_values = values;
                }
            }
        }
        infos
    }

    
    
    
    
    
    pub fn register_plugin_dir(&self, dir: &Path) -> Result<PluginInfo, PluginError> {
        let discovered = super::super::scan::load_single(dir)
            .map_err(|m| PluginError::new("invalid_plugin", m))?;
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
                    super::super::supervisor::CHILD_EXE_NAME
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
}
