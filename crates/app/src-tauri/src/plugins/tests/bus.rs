//! Шина событий и доставка команд: `command:invoked`, `document:changed`, персист `enabled`.

use std::sync::{Arc, Mutex};

use serde_json::json;

use crate::plugins::host::PluginHost;
use crate::plugins::manager::{PluginManager, PluginRuntime};
use crate::plugins::services::{DocumentServices, DocumentSink, StatusSink};
use crate::plugins::settings::SettingsStore;
use crate::plugins::HostServices;
use crate::state::DocumentStore;

use super::common::{
    child_exe, doc_rev, doc_text, fixture, serial, store_with_doc, view_contrib, views,
};



fn services_with_status_log(
    store: &Arc<Mutex<DocumentStore>>,
    views: &Arc<Mutex<crate::plugins::views::PluginViews>>,
    pending: &Arc<Mutex<Vec<(String, u64)>>>,
    seen: &Arc<Mutex<Vec<(String, String)>>>,
) -> Arc<dyn HostServices> {
    let notify: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {});
    let seen_for = Arc::clone(seen);
    let sink: StatusSink = Arc::new(move |plugin_id: &str, text: &str| {
        seen_for
            .lock()
            .unwrap()
            .push((plugin_id.to_string(), text.to_string()));
    });
    Arc::new(
        DocumentServices::new(Arc::clone(store))
            .with_views(Arc::clone(views))
            .with_notify(notify)
            .with_pending_changed(Arc::clone(pending))
            .with_status_sink(sink),
    )
}

#[test]
fn run_plugin_command_delivers_doc_id_and_edits() {
    let _guard = serial();
    let store = store_with_doc("");
    let views = views();
    let pending: Arc<Mutex<Vec<(String, u64)>>> = Arc::new(Mutex::new(Vec::new()));
    let notify: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {});
    let services: Arc<dyn HostServices> = Arc::new(
        DocumentServices::new(Arc::clone(&store))
            .with_views(Arc::clone(&views))
            .with_notify(Arc::clone(&notify))
            .with_pending_changed(Arc::clone(&pending)),
    );
    let mut manager = PluginManager::default();
    let mut runtime = PluginRuntime::new(
        "cmd",
        child_exe(),
        fixture("command-edit.lua"),
        vec!["document:write".to_string()],
        Arc::clone(&services),
        Vec::new(),
        Vec::new(),
    )
    .with_granted(vec!["document:write".to_string()]);
    runtime.start().expect("активация");
    manager.register(runtime);
    let host = PluginHost::new(manager, views, services, notify, pending);

    host.run_plugin_command("test.command", Some("doc-1"));

    assert_eq!(
        doc_text(&store, "doc-1"),
        "C",
        "command:invoked дошёл с doc_id и применился"
    );
    assert_eq!(doc_rev(&store), 1);
}





#[test]
fn command_invoked_is_broadcast_and_edit_notifies_other_plugin() {
    let _guard = serial();
    let store = store_with_doc("");
    let views = views();
    let pending: Arc<Mutex<Vec<(String, u64)>>> = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let services = services_with_status_log(&store, &views, &pending, &seen);

    let mut manager = PluginManager::default();
    let mut commander = PluginRuntime::new(
        "commander",
        child_exe(),
        fixture("command-edit.lua"),
        vec!["document:write".to_string()],
        Arc::clone(&services),
        Vec::new(),
        Vec::new(),
    )
    .with_granted(vec!["document:write".to_string()]);
    commander.start().expect("активация commander");
    manager.register(commander);
    let mut observer = PluginRuntime::new(
        "observer",
        child_exe(),
        fixture("event-observer.lua"),
        vec!["ui:statusbar".to_string()],
        Arc::clone(&services),
        Vec::new(),
        Vec::new(),
    )
    .with_granted(vec!["ui:statusbar".to_string()]);
    observer.start().expect("активация observer");
    manager.register(observer);

    let host = PluginHost::new(manager, views, services, Arc::new(|| {}), pending);
    host.run_plugin_command("test.command", Some("doc-1"));

    let messages = seen.lock().unwrap().clone();
    assert!(
        messages
            .iter()
            .any(|(id, text)| id == "observer" && text == "cmd:test.command"),
        "command:invoked обязан дойти до плагина без такой команды (broadcast): {messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|(id, text)| id == "observer" && text == "changed:1"),
        "apply_edit чужого плагина порождает document:changed для остальных: {messages:?}"
    );
    assert_eq!(
        doc_text(&store, "doc-1"),
        "C",
        "команда применена своим плагином"
    );
}




#[test]
fn disabled_plugin_does_not_receive_bus_events() {
    let _guard = serial();
    let store = store_with_doc("");
    let views = views();
    let pending: Arc<Mutex<Vec<(String, u64)>>> = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let services = services_with_status_log(&store, &views, &pending, &seen);

    let mut manager = PluginManager::default();
    let mut commander = PluginRuntime::new(
        "commander",
        child_exe(),
        fixture("command-edit.lua"),
        vec!["document:write".to_string()],
        Arc::clone(&services),
        Vec::new(),
        Vec::new(),
    )
    .with_granted(vec!["document:write".to_string()]);
    commander.start().expect("активация commander");
    manager.register(commander);
    let mut observer = PluginRuntime::new(
        "observer",
        child_exe(),
        fixture("event-observer.lua"),
        vec!["ui:statusbar".to_string()],
        Arc::clone(&services),
        Vec::new(),
        Vec::new(),
    )
    .with_granted(vec!["ui:statusbar".to_string()]);
    observer.set_enabled(false);
    manager.register(observer);

    let host = PluginHost::new(manager, views, services, Arc::new(|| {}), pending);
    host.run_plugin_command("test.command", Some("doc-1"));

    assert!(
        seen.lock().unwrap().is_empty(),
        "выключенному плагину события не доставляются: {:?}",
        seen.lock().unwrap()
    );
    assert_eq!(
        doc_text(&store, "doc-1"),
        "C",
        "активный сосед по команде сработал"
    );
}




#[test]
fn plugin_apply_edit_notifies_document_sink_with_doc_and_rev() {
    let _guard = serial();
    let store = store_with_doc("");
    let views = views();
    let pending: Arc<Mutex<Vec<(String, u64)>>> = Arc::new(Mutex::new(Vec::new()));
    let notify: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {});
    let updated = Arc::new(Mutex::new(Vec::<(String, u64)>::new()));
    let updated_for = Arc::clone(&updated);
    let sink: DocumentSink = Arc::new(move |doc_id: &str, rev: u64| {
        updated_for.lock().unwrap().push((doc_id.to_string(), rev));
    });
    let services: Arc<dyn HostServices> = Arc::new(
        DocumentServices::new(Arc::clone(&store))
            .with_views(Arc::clone(&views))
            .with_notify(Arc::clone(&notify))
            .with_pending_changed(Arc::clone(&pending))
            .with_document_sink(sink),
    );

    let mut manager = PluginManager::default();
    let mut runtime = PluginRuntime::new(
        "cmd",
        child_exe(),
        fixture("command-edit.lua"),
        vec!["document:write".to_string()],
        Arc::clone(&services),
        Vec::new(),
        Vec::new(),
    )
    .with_granted(vec!["document:write".to_string()]);
    runtime.start().expect("активация");
    manager.register(runtime);
    let host = PluginHost::new(manager, views, Arc::clone(&services), notify, pending);

    host.run_plugin_command("test.command", Some("doc-1"));

    assert_eq!(
        *updated.lock().unwrap(),
        vec![("doc-1".to_string(), 1)],
        "плагинный apply_edit обязан уведомить document_sink с (doc_id, rev)"
    );
    services
        .handle(
            "apply_edit",
            json!({ "doc_id": "doc-1", "start": 0, "stop": 1, "text": "C" }),
        )
        .unwrap();
    assert_eq!(
        updated.lock().unwrap().len(),
        1,
        "эхо не порождает повторного уведомления"
    );
}

#[test]
fn set_enabled_persists_to_config() {
    let _guard = serial();
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.json");

    let store = store_with_doc("x");
    let views = views();
    let pending: Arc<Mutex<Vec<(String, u64)>>> = Arc::new(Mutex::new(Vec::new()));
    let notify: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {});
    let services: Arc<dyn HostServices> = Arc::new(
        DocumentServices::new(Arc::clone(&store))
            .with_views(Arc::clone(&views))
            .with_notify(Arc::clone(&notify))
            .with_pending_changed(Arc::clone(&pending)),
    );
    let mut manager = PluginManager::default();
    manager.register(
        PluginRuntime::new(
            "viewplugin",
            child_exe(),
            fixture("view-activate.lua"),
            vec!["view:create".to_string(), "view:modify".to_string()],
            Arc::clone(&services),
            vec![view_contrib("main", "Main", 1)],
            Vec::new(),
        )
        .with_granted(vec!["view:create".to_string(), "view:modify".to_string()]),
    );
    let host = PluginHost::new(manager, views, services, notify, pending)
        .with_settings(SettingsStore::default(), Some(config.clone()));
    host.set_enabled("viewplugin", false).expect("выключение");
    let loaded = SettingsStore::load(&config);
    assert!(
        !loaded.plugin_enabled("viewplugin"),
        "выключение не сохранилось в config.json"
    );
    let _ = std::fs::remove_dir_all(dir.path());
}
