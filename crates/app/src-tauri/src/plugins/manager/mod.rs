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

/// Порог карантина: 3 неудачи подряд (ADR-0021 §3).
pub const QUARANTINE_THRESHOLD: u32 = 3;

/// Эффективные права плагина: пересечение запрашиваемых манифестом (`manifest`) и выданных
/// пользователем (`granted`) — deny-by-default, §8/§12.
///
/// Порядок — как в манифесте (детерминирован), дубли не плодятся. «Мусор» в `granted`
/// (разрешение, которого нет в манифесте) отсекается: подпись пользователя не расширяет
/// декларацию плагина. Чистая функция — без зависимостей от процессов/IO, потому и тестируема.
pub fn effective_permissions(manifest: &[String], granted: &[String]) -> Vec<String> {
    manifest
        .iter()
        .filter(|permission| granted.iter().any(|grant| grant == *permission))
        .cloned()
        .collect()
}

/// Один плагин и его child-процесс.
pub struct PluginRuntime {
    pub id: String,
    /// Человекочитаемое имя из манифеста (`plugin.json`) — для менеджера UI (S12a).
    pub name: String,
    /// Версия из манифеста (`plugin.json`) — для менеджера UI (S12a).
    pub version: String,
    /// Запрашиваемые плагином права из манифеста (а не фактические — их даёт [`Self::effective`]).
    pub permissions: Vec<String>,
    /// Согласованные пользователем права (подмножество `permissions`, §8/§12, Фаза 5).
    /// Пусто — согласие не выдано; при старте child получает [`Self::effective`] (deny-by-default).
    granted: Vec<String>,
    pub status: PluginStatus,
    /// Тир-1 view из манифеста (регистрируются в общем [`PluginViews`] при загрузке;
    /// здесь — для менеджера UI Фазы 5: перерегистрация при перезагрузке).
    #[allow(dead_code)]
    pub views: Vec<ViewContrib>,
    /// Команды из манифеста (`contributes.commands`): менеджер UI показывает их кнопками,
    /// `run_plugin_command` публикует `command:invoked` с этим `command_id`.
    #[allow(dead_code)]
    pub commands: Vec<CommandContrib>,
    /// Схема настроек из манифеста (`contributes.settings`) для UI-формы (S5b).
    /// `None`, если плагин не объявил `contributes.settings`.
    pub settings: Option<Value>,
    quarantine: Quarantine,
    exe: PathBuf,
    /// Кэш исходника; используется, если [`PluginRuntime::entry_path`] не задан (тесты).
    source: String,
    /// Путь к `.lua` на диске: `reload`/`start` перечитывают его, чтобы подхватить правки (§4.7).
    entry_path: Option<PathBuf>,
    /// Каталог плагина на диске — для удаления (`remove_plugin`).
    dir: PathBuf,
    services: Arc<dyn HostServices>,
    supervisor: Option<Supervisor>,
    enabled: bool,
    mem_bytes: usize,
    progress: Duration,
    deadline: Duration,
}

/// Набор плагинов. Фаза 5 оборачивает его Tauri-командами.
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

    /// Удаляет плагин из реестра: останавливает child и возвращает путь к каталогу
    /// на диске для последующего удаления файлов.
    ///
    /// Каталог не удаляется здесь — это ответственность вызывающего (`PluginHost`),
    /// который сначала снимает views и настройки, а потом удаляет файлы.
    pub fn remove_plugin(&mut self, id: &str) -> Result<PathBuf, PluginError> {
        let mut plugin = self
            .plugins
            .remove(id)
            .ok_or_else(|| PluginError::new("unknown_plugin", format!("нет плагина {id}")))?;
        plugin.stop();
        Ok(plugin.dir)
    }

    /// `set_plugin_permissions`: фиксирует согласие пользователя на права плагина.
    ///
    /// Пересечение с манифестом (`manifest ∩ granted`) — deny-by-default: выданное
    /// пользователем разрешение, которого нет в декларации, отсекается ещё до записи.
    /// Включённый плагин перезапускается, чтобы новый набор вступил в силу (права
    /// захватываются при [`PluginRuntime::start`]); выключенный — только запоминает набор.
    /// `reload` **не** снимает карантин: плагин в нём вернёт `quarantined`.
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

    /// Активные плагины с PID живого child'а (`(plugin_id, pid)`), детерминированно по id.
    ///
    /// Источник списка процессов для ресурс-монитора (ADR-0031 §2.2/§2.6): только `Active`
    /// с живым `Supervisor`. Вызывается под **коротким** локом менеджера — снимок PID;
    /// WinAPI-замеры и эмит идут вне блокировки (§2.3 п.10).
    ///
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

    /// Изменяемый обход всех плагинов (старт enabled-плагинов, Фаза 4).
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut PluginRuntime> {
        self.plugins.values_mut()
    }

    /// Изменяемый обход только активных плагинов (доставка событий, §9).
    pub fn iter_active_mut(&mut self) -> impl Iterator<Item = &mut PluginRuntime> {
        self.plugins
            .values_mut()
            .filter(|p| matches!(p.status, PluginStatus::Active))
    }
}

/// Результат загрузки реестра: менеджер + ошибки сканирования (битые манифесты и т.п.).
pub struct LoadResult {
    pub manager: PluginManager,
    pub errors: Vec<super::scan::ScanError>,
}

/// Сканирует каталог и строит реестр плагинов (**без запуска** процессов).
///
/// Запрашиваемые права берутся из манифеста, согласованные — из [`SettingsStore`]
/// (deny-by-default: нет записи — нет прав). Вкл/выкл — тоже из настроек. Фактический
/// набор (`manifest ∩ granted`) вычисляет [`PluginRuntime::effective`] и передаёт child'у
/// при старте; автозапуск enabled-плагинов — `PluginHost::start_enabled`.
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
