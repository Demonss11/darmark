//! Жизненный цикл плагина на стороне хоста: автозапуск, вкл/выкл, удаление, перезагрузка,
//! права, настройки и поток ресурс-монитора (ADR-0031).

use std::sync::Arc;

use plugin_core::envelope::PluginError;
use serde_json::Value;

use crate::PluginStatus;

use super::super::manager::effective_permissions;
use super::super::resource::{ProcessMetrics, ResourceMonitor, ResourceSink};
use super::{lock, PluginHost};

impl PluginHost {
    /// Запускает поток ресурс-монитора (ADR-0031 §2.3 п.9).
    ///
    /// Список активных PID снимается коротким локом менеджера **внутри** потока
    /// ([`PluginManager::active_pids`](super::super::manager::PluginManager::active_pids)); WinAPI-замеры и эмит событий идут уже без блокировки
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
        let children = move || match manager.try_lock() {
            Ok(guard) => Some(guard.active_pids()),
            Err(_) => None,
        };
        self.resource_thread = Some(monitor.spawn(children, stop));
    }

    /// Best-effort старт enabled-плагинов и уведомление фронтенда.
    ///
    /// Ошибка запуска одного плагина не мешает остальным и не роняет GUI (§4.7).
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

    /// `remove_plugin`: полное удаление плагина.
    ///
    /// Порядок критичен: сначала снимаем views и останавливаем child, потом удаляем
    /// настройки и каталог с диска. Ошибка удаления файлов не откатывает удаление
    /// из реестра — плагин уже не запустится, а каталог можно удалить вручную.
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

    /// `reload_plugin`: перезапуск без рестарта GUI (Фаза 5).
    ///
    /// Неудачная перезагрузка (синтаксис и т.п.) снимает view плагина: иначе остаётся
    /// «зомби»-вкладка со старым HTML при статусе «ошибка».
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

    /// `set_plugin_permissions`: согласие пользователя как **реальный** гейт прав
    /// (§8/§12, §11.1 п.4).
    ///
    /// Пересечение запрашиваемых манифестом прав и выданных пользователем
    /// (`manifest ∩ granted`, deny-by-default) сохраняется в `config.json` **до**
    /// применения — согласие фиксируется даже при сбое перезапуска. Включённый плагин
    /// перезапускается, чтобы новый набор вступил в силу (права захватываются при
    /// [`super::super::manager::PluginRuntime::start`]); выключенный — только запоминает набор (`reload` на
    /// выключенном = `start` ранним выходом). Карантин `reload` не сбрасывает.
    ///
    /// Сбой перезапуска снимает view плагина: иначе останется «зомби»-вкладка при
    /// статусе «ошибка» — как у [`Self::reload`].
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
