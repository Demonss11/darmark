use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::json;

use crate::plugins::host::{DocumentEventSink, PluginHost, StatusChangedSink};
use crate::plugins::manager::{PluginManager, PluginRuntime};
use crate::plugins::services::{DocumentServices, LogSink};
use crate::plugins::HostServices;

use super::common::{child_exe, fixture, serial, store_with_doc, view_contrib, views};


type StatusTransition = (String, String, String, Option<String>);



fn record_status_changes() -> (StatusChangedSink, Arc<Mutex<Vec<StatusTransition>>>) {
    let seen = Arc::new(Mutex::new(Vec::<StatusTransition>::new()));
    let seen_for = Arc::clone(&seen);
    let sink: StatusChangedSink = Arc::new(
        move |plugin_id: &str, old_state: &str, new_state: &str, message: Option<&str>| {
            seen_for.lock().unwrap().push((
                plugin_id.to_string(),
                old_state.to_string(),
                new_state.to_string(),
                message.map(str::to_string),
            ));
        },
    );
    (sink, seen)
}



fn host_with_status_changes() -> (PluginHost, Arc<Mutex<Vec<StatusTransition>>>) {
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
    let (sink, seen) = record_status_changes();
    let host =
        PluginHost::new(manager, views, services, notify, pending).with_status_changed_sink(sink);
    (host, seen)
}




#[test]
fn plugin_status_changed_on_enable_and_disable() {
    let _guard = serial();
    let (host, seen) = host_with_status_changes();

    host.set_enabled("viewplugin", true).expect("включение");
    let after_enable = seen.lock().unwrap().clone();
    assert!(
        after_enable
            .iter()
            .any(|(id, old, new, _)| id == "viewplugin" && old == "stopped" && new == "active"),
        "остановлен → активен: {after_enable:?}"
    );

    seen.lock().unwrap().clear();
    host.set_enabled("viewplugin", false).expect("выключение");
    let after_disable = seen.lock().unwrap().clone();
    assert!(
        after_disable
            .iter()
            .any(|(id, old, new, _)| id == "viewplugin" && old == "active" && new == "stopped"),
        "активен → остановлен: {after_disable:?}"
    );
}



#[test]
fn plugin_status_changed_on_reload() {
    let _guard = serial();
    let (host, seen) = host_with_status_changes();
    host.set_enabled("viewplugin", true).expect("включение");
    seen.lock().unwrap().clear();

    host.reload("viewplugin").expect("перезагрузка");
    let transitions = seen.lock().unwrap().clone();
    assert!(
        transitions
            .iter()
            .any(|(id, old, new, _)| id == "viewplugin" && old == "active" && new == "active"),
        "reload без смены статуса обязан дать active → active: {transitions:?}"
    );
}



#[test]
fn plugin_status_changed_on_remove() {
    let _guard = serial();
    let (host, seen) = host_with_status_changes();
    host.set_enabled("viewplugin", true).expect("включение");
    seen.lock().unwrap().clear();

    host.remove("viewplugin").expect("удаление");
    let transitions = seen.lock().unwrap().clone();
    assert!(
        transitions.iter().any(|(id, old, new, message)| {
            id == "viewplugin"
                && old == "active"
                && new == "stopped"
                && message.as_deref().is_some_and(|m| m.contains("удал"))
        }),
        "удаление: active → stopped с сообщением: {transitions:?}"
    );
}



#[test]
fn set_permissions_emits_status_change_on_restart() {
    let _guard = serial();
    let (host, seen) = host_with_status_changes();

    host.set_permissions("viewplugin", vec!["view:create".to_string()])
        .expect("смена прав");
    let transitions = seen.lock().unwrap().clone();
    assert!(
        transitions
            .iter()
            .any(|(id, old, new, _)| id == "viewplugin" && old == "stopped" && new == "active"),
        "перезапуск при смене прав: stopped → active: {transitions:?}"
    );
}



#[test]
fn set_permissions_same_set_emits_no_extra_event() {
    let _guard = serial();
    let (host, seen) = host_with_status_changes();
    host.set_enabled("viewplugin", true).expect("включение");
    seen.lock().unwrap().clear();
    host.set_permissions(
        "viewplugin",
        vec!["view:create".to_string(), "view:modify".to_string()],
    )
    .expect("повторная установка прав");

    assert!(
        seen.lock().unwrap().is_empty(),
        "тот же набор прав не меняет статус → событий быть не должно: {:?}",
        seen.lock().unwrap()
    );
}



#[test]
fn start_enabled_emits_stopped_to_active() {
    let _guard = serial();
    let (host, seen) = host_with_status_changes();

    host.start_enabled();
    let transitions = seen.lock().unwrap().clone();
    assert!(
        transitions
            .iter()
            .any(|(id, old, new, _)| id == "viewplugin" && old == "stopped" && new == "active"),
        "автозапуск: stopped → active: {transitions:?}"
    );
}



#[test]
fn plugin_status_changed_on_pump_failure() {
    let _guard = serial();
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
    let mut runtime = PluginRuntime::new(
        "event-hang",
        child_exe(),
        fixture("event-hang.lua"),
        Vec::new(),
        Arc::clone(&services),
        Vec::new(),
        Vec::new(),
    )
    .with_watchdog(Duration::from_millis(300), Duration::from_secs(30));
    runtime.start().expect("активация");
    manager.register(runtime);

    let (sink, seen) = record_status_changes();
    let host =
        PluginHost::new(manager, views, services, notify, pending).with_status_changed_sink(sink);

    host.run_plugin_command("test.command", Some("doc-1"));

    let transitions = seen.lock().unwrap().clone();
    assert!(
        transitions.iter().any(|(id, old, new, message)| {
            id == "event-hang"
                && old == "active"
                && new == "failed"
                && message.as_deref().is_some_and(|m| !m.is_empty())
        }),
        "отказ доставки: active → failed с непустым сообщением: {transitions:?}"
    );
}



#[test]
fn document_event_sink_receives_all_action_types() {
    let seen = Arc::new(Mutex::new(
        Vec::<(String, Option<String>, Option<u64>)>::new(),
    ));
    let seen_for = Arc::clone(&seen);
    let sink: DocumentEventSink =
        Arc::new(move |kind: &str, path: Option<&str>, rev: Option<u64>| {
            seen_for
                .lock()
                .unwrap()
                .push((kind.to_string(), path.map(str::to_string), rev));
        });
    let host = PluginHost::empty().with_document_event_sink(sink);

    host.publish_document_event("new", None, None);
    host.publish_document_event("open", Some("note.md"), Some(0));
    host.publish_document_event("save", Some("note.md"), Some(3));
    host.publish_document_event("close", Some("note.md"), None);

    assert_eq!(
        *seen.lock().unwrap(),
        vec![
            ("new".to_string(), None, None),
            ("open".to_string(), Some("note.md".to_string()), Some(0)),
            ("save".to_string(), Some("note.md".to_string()), Some(3)),
            ("close".to_string(), Some("note.md".to_string()), None),
        ],
        "тип действия и path/rev должны дойти без искажений"
    );
}



#[test]
fn log_entry_calls_log_sink() {
    let store = store_with_doc("x");
    let seen = Arc::new(Mutex::new(Vec::<(String, String, String)>::new()));
    let seen_for = Arc::clone(&seen);
    let sink: LogSink = Arc::new(move |level: &str, category: &str, message: &str| {
        seen_for.lock().unwrap().push((
            level.to_string(),
            category.to_string(),
            message.to_string(),
        ));
    });
    let services = DocumentServices::new(store).with_log_sink(sink);

    let result = services
        .handle(
            "log",
            json!({ "level": "warn", "message": "событие не доставлено", "_plugin_id": "word-count" }),
        )
        .unwrap();
    assert_eq!(result, json!(true), "успех log — значение true");

    let entries = seen.lock().unwrap().clone();
    assert_eq!(
        entries.len(),
        1,
        "одна запись на один host-call: {entries:?}"
    );
    assert_eq!(entries[0].0, "warn", "уровень сохранён: {entries:?}");
    assert_eq!(
        entries[0].1, "plugin",
        "логи плагина — категория plugin: {entries:?}"
    );
    assert!(
        entries[0].2.contains("событие не доставлено"),
        "текст сообщения сохранён: {entries:?}"
    );
}



#[test]
fn log_without_sink_is_noop() {
    let store = store_with_doc("x");
    let services = DocumentServices::new(store);

    let result = services
        .handle(
            "log",
            json!({ "level": "info", "message": "x", "_plugin_id": "p" }),
        )
        .expect("без sink — no-op, не ошибка");
    assert_eq!(result, json!(true));
}
