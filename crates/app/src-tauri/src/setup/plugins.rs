//! Подъём плагинной подсистемы на старте приложения.
//!
//! Здесь живут `PluginState` — состояние, которым адресуются все Tauri-команды плагинов,
//! и адаптеры `plugins::*` к Tauri-событиям (единственное место, где они встречаются).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::plugins;
use crate::state::DocumentStore;

/// Платформенно-нейтральное состояние плагинной подсистемы для Tauri-команд.
///
/// На Windows содержит реальный `plugins::host::PluginHost`; на прочих
/// платформах — пустую заглушку, чтобы единый `generate_handler!` собирался
/// (модуль `plugins` включается только под Windows).
pub struct PluginState {
    pub(crate) host: plugins::host::PluginHost,
}

impl PluginState {
    /// Создаёт/изменяет документ, публикует событие и доставляет его плагинам.
    pub(crate) fn publish_document_changed(&self, doc_id: &str, rev: u64) {
        self.host.publish_document_changed(doc_id, rev);
        self.host.pump();
    }

    pub(crate) fn publish_opened(&self, doc_id: &str, path: Option<&str>) {
        self.host.publish_opened(doc_id, path);
        self.host.pump();
    }

    pub(crate) fn publish_closed(&self, doc_id: &str) {
        self.host.publish_closed(doc_id);
        self.host.pump();
    }

    /// Публикует `document-event` (S11) — действие пользователя над документом.
    ///
    /// Событие идёт в журнал фронтенда (`logPanel`): `new`/`open`/`save`/`close`.
    /// Доставку в Tauri-событие `document-event` обеспечивает sink из [`setup_plugins`].
    pub(crate) fn publish_document_event(&self, kind: &str, path: Option<&str>, rev: Option<u64>) {
        self.host.publish_document_event(kind, path, rev);
    }
}

/// Sink ресурс-монитора: конвертирует снимок/событие завершения в Tauri-события
/// (`resource-metrics`/`plugin-process-exited`, ADR-0031 §2.4 п.15).
///
/// Best-effort: сбой `emit` логируется и не роняет монитор. Сам модуль `resource` про Tauri
/// не знает — адаптер живёт здесь, в `setup_plugins`.
struct TauriResourceSink {
    handle: tauri::AppHandle,
}

impl plugins::resource::ResourceSink for TauriResourceSink {
    fn metrics(&self, snapshot: &plugins::resource::ResourceSnapshot) {
        use tauri::Emitter;
        if let Err(e) = self.handle.emit("resource-metrics", snapshot) {
            eprintln!("не эмитить resource-metrics: {e}");
        }
    }

    fn process_exited(&self, plugin_id: &str, pid: u32, reason: Option<&str>) {
        use tauri::Emitter;
        let mut payload = serde_json::json!({ "plugin_id": plugin_id, "pid": pid });
        if let Some(reason) = reason {
            payload["reason"] = serde_json::Value::String(reason.to_string());
        }
        if let Err(e) = self.handle.emit("plugin-process-exited", payload) {
            eprintln!("не эмитить plugin-process-exited: {e}");
        }
    }
}

/// Каталог плагинов: `%APPDATA%/darmark/plugins`, с override `DARMARK_PLUGINS_DIR`
/// для тестов/E2E (изоляция от реального профиля пользователя).
fn plugins_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("DARMARK_PLUGINS_DIR") {
        return PathBuf::from(dir);
    }
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_default();
    base.join("darmark").join("plugins")
}

/// Поднимает плагинную подсистему: настройки → scan/load → notify → старт enabled.
///
/// Child-exe резолвится [`plugins::supervisor::Supervisor::resolve_child_exe`]; при
/// его отсутствии плагины не стартуют, а GUI продолжает работу.
pub(crate) fn setup_plugins(app: &mut tauri::App) -> Result<(), String> {
    use tauri::{Emitter, Manager};
    use tauri_plugin_dialog::DialogExt;

    use plugins::host::{DocumentEventSink, PluginHost, StatusChangedSink};
    use plugins::manager::load_plugins;
    use plugins::resource::{ProcessMetrics, WinProcessMetrics};
    use plugins::services::{DocumentServices, DocumentSink, Exporter, LogSink, StatusSink};
    use plugins::settings::SettingsStore;
    use plugins::supervisor::Supervisor;
    use plugins::views::PluginViews;

    let exe = Supervisor::resolve_child_exe().ok_or_else(|| {
        format!(
            "{} не найден рядом с darmark.exe",
            plugins::supervisor::CHILD_EXE_NAME
        )
    })?;

    let config_path = SettingsStore::config_path();
    let settings = match &config_path {
        Some(path) => SettingsStore::load(path),
        None => SettingsStore::default(),
    };

    let store = app.state::<Arc<Mutex<DocumentStore>>>().inner().clone();
    let views = Arc::new(Mutex::new(PluginViews::new()));
    let pending_changed: plugins::services::PendingChanged = Arc::new(Mutex::new(Vec::new()));
    let plugin_settings = plugins::plugin_settings::PluginSettingsStore::base_dir().map(|dir| {
        Arc::new(Mutex::new(
            plugins::plugin_settings::PluginSettingsStore::new(dir),
        ))
    });
    let handle = app.handle().clone();
    let views_notify: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        if let Err(e) = handle.emit("plugin-views-changed", ()) {
            eprintln!("не эмитить plugin-views-changed: {e}");
        }
    });
    let plugins_handle = app.handle().clone();
    let plugins_notify: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        if let Err(e) = plugins_handle.emit("plugins-changed", ()) {
            eprintln!("не эмитить plugins-changed: {e}");
        }
    });
    let export_handle = app.handle().clone();
    let exporter: Exporter = Arc::new(move |html: &str| -> Result<bool, String> {
        let handle = export_handle.clone();
        let html = html.to_string();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = match handle
                .dialog()
                .file()
                .add_filter("HTML", &["html"])
                .set_file_name("export.html")
                .blocking_save_file()
            {
                None => Ok(false),
                Some(file_path) => {
                    file_path
                        .into_path()
                        .map_err(|e| e.to_string())
                        .and_then(|path| {
                            std::fs::write(&path, &html)
                                .map(|_| true)
                                .map_err(|e| format!("{}: {e}", path.display()))
                        })
                }
            };
            let _ = tx.send(result);
        });
        rx.recv().map_err(|e| e.to_string())?
    });
    let message_handle = app.handle().clone();
    let status_sink: StatusSink = Arc::new(move |plugin_id: &str, text: &str| {
        if let Err(e) = message_handle.emit(
            "plugin-message",
            serde_json::json!({ "plugin_id": plugin_id, "text": text }),
        ) {
            eprintln!("не эмитить plugin-message: {e}");
        }
    });
    let document_handle = app.handle().clone();
    let document_sink: DocumentSink = Arc::new(move |doc_id: &str, rev: u64| {
        if let Err(e) = document_handle.emit(
            "document-updated",
            serde_json::json!({ "doc_id": doc_id, "rev": rev }),
        ) {
            eprintln!("не эмитить document-updated: {e}");
        }
    });
    let status_handle = app.handle().clone();
    let status_changed_sink: StatusChangedSink = Arc::new(
        move |plugin_id: &str, old_state: &str, new_state: &str, message: Option<&str>| {
            let mut payload = serde_json::json!({
                "plugin_id": plugin_id,
                "old_state": old_state,
                "new_state": new_state,
            });
            if let Some(message) = message {
                payload["message"] = serde_json::Value::String(message.to_string());
            }
            if let Err(e) = status_handle.emit("plugin-status-changed", payload) {
                eprintln!("не эмитить plugin-status-changed: {e}");
            }
        },
    );
    let document_event_handle = app.handle().clone();
    let document_event_sink: DocumentEventSink =
        Arc::new(move |kind: &str, path: Option<&str>, rev: Option<u64>| {
            let mut payload = serde_json::json!({ "type": kind });
            if let Some(path) = path {
                payload["path"] = serde_json::Value::String(path.to_string());
            }
            if let Some(rev) = rev {
                payload["rev"] = serde_json::Value::from(rev);
            }
            if let Err(e) = document_event_handle.emit("document-event", payload) {
                eprintln!("не эмитить document-event: {e}");
            }
        });
    let log_handle = app.handle().clone();
    let log_sink: LogSink = Arc::new(move |level: &str, category: &str, message: &str| {
        if let Err(e) = log_handle.emit(
            "log-entry",
            serde_json::json!({ "level": level, "category": category, "message": message }),
        ) {
            eprintln!("не эмитить log-entry: {e}");
        }
    });

    let mut services = DocumentServices::new(store)
        .with_views(Arc::clone(&views))
        .with_notify(Arc::clone(&views_notify))
        .with_pending_changed(Arc::clone(&pending_changed))
        .with_exporter(exporter)
        .with_status_sink(status_sink)
        .with_document_sink(document_sink)
        .with_log_sink(log_sink);
    if let Some(settings) = &plugin_settings {
        services = services.with_plugin_settings(Arc::clone(settings));
    }
    let services: Arc<dyn plugins::HostServices> = Arc::new(services);

    let result = load_plugins(
        &plugins_dir(),
        &exe,
        Arc::clone(&services),
        &settings,
        Arc::clone(&views),
    );
    for error in &result.errors {
        eprintln!("плагин {}: {}", error.dir.display(), error.message);
    }

    let mut host = PluginHost::new(
        result.manager,
        views,
        services,
        views_notify,
        pending_changed,
    )
    .with_plugins_notify(plugins_notify)
    .with_status_changed_sink(status_changed_sink)
    .with_document_event_sink(document_event_sink)
    .with_settings(settings, config_path);
    if let Some(settings) = &plugin_settings {
        host = host.with_plugin_settings(Arc::clone(settings));
    }
    host.start_enabled();
    let resource_provider: Arc<dyn ProcessMetrics> = Arc::new(WinProcessMetrics::new());
    let resource_sink = Arc::new(TauriResourceSink {
        handle: app.handle().clone(),
    });
    host.start_resource_monitor(
        resource_provider,
        resource_sink,
        plugin_core::limits::DEFAULT_MEM_BYTES,
    );

    app.manage(PluginState { host });
    Ok(())
}
