//! Сигналы `plugin-views-changed` / `plugins-changed`: что и когда эмитится фронт-шине.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::json;

use crate::plugins::host::PluginHost;
use crate::plugins::manager::{PluginManager, PluginRuntime};
use crate::plugins::services::DocumentServices;
use crate::plugins::HostServices;

use super::common::{child_exe, fixture, push_log, serial, store_with_doc, view_contrib, views};

/// Хост с одним view-плагином (`view-activate.lua`, наполняет вкладку в `on_activate`) и
/// журналом раздельных сигналов `plugin-views-changed`/`plugins-changed` (F41/S2).
///
/// `DocumentServices` получает только view-сигнал; `plugins-changed` ему недоступен. Это
/// структурная гарантия того, что `set_view_content` не дёрнет менеджер: если бы сюда
/// подключили `plugins_notify`, тест «эмитится только `plugin-views-changed`» потерял бы смысл.
fn host_with_signal_log() -> (PluginHost, Arc<Mutex<Vec<&'static str>>>) {
    let store = store_with_doc("x");
    let views = views();
    let pending: Arc<Mutex<Vec<(String, u64)>>> = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::new(Mutex::new(Vec::<&'static str>::new()));
    let views_notify = push_log(&log, "plugin-views-changed");
    let plugins_notify = push_log(&log, "plugins-changed");
    let services: Arc<dyn HostServices> = Arc::new(
        DocumentServices::new(Arc::clone(&store))
            .with_views(Arc::clone(&views))
            .with_notify(Arc::clone(&views_notify))
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
    let host = PluginHost::new(manager, views, services, views_notify, pending)
        .with_plugins_notify(plugins_notify);
    (host, log)
}

/// F41/S2: `set_view_content` меняет только контент view → эмитится **лишь**
/// `plugin-views-changed`; менеджер плагинов не должен перечитывать `list_plugins`.
#[test]
fn set_view_content_emits_only_views_changed() {
    let store = store_with_doc("x");
    let views = views();
    views
        .lock()
        .unwrap()
        .register("p", &[view_contrib("main", "Main", 1)]);

    let log = Arc::new(Mutex::new(Vec::<&'static str>::new()));
    let views_notify = push_log(&log, "plugin-views-changed");
    let services: Arc<dyn HostServices> = Arc::new(
        DocumentServices::new(Arc::clone(&store))
            .with_views(Arc::clone(&views))
            .with_notify(Arc::clone(&views_notify)),
    );
    services
        .handle(
            "set_view_content",
            json!({ "view_id": "p:main", "html": "<b>x</b>", "_plugin_id": "p" }),
        )
        .unwrap();

    assert_eq!(
        log.lock().unwrap().clone(),
        vec!["plugin-views-changed"],
        "только view-сигнал; plugins-changed не эмитится"
    );
}

/// F41/S2: включение/перезагрузка меняют статус (и реестр view) → оба сигнала, включая
/// `plugins-changed` (менеджер обязан обновить состав/статусы).
#[test]
fn set_enabled_and_reload_emit_plugins_changed() {
    let _guard = serial();
    let (host, log) = host_with_signal_log();

    host.set_enabled("viewplugin", true).expect("включение");
    let after_enable = log.lock().unwrap().clone();
    assert!(
        after_enable.contains(&"plugins-changed"),
        "включение меняет состав/статус → plugins-changed: {after_enable:?}"
    );
    assert!(
        after_enable.contains(&"plugin-views-changed"),
        "включение регистрирует view → plugin-views-changed: {after_enable:?}"
    );

    log.lock().unwrap().clear();
    host.reload("viewplugin").expect("перезагрузка");
    assert!(
        log.lock().unwrap().contains(&"plugins-changed"),
        "reload меняет статус → plugins-changed: {:?}",
        log.lock().unwrap()
    );
}

/// F41/S2: отказ доставки события уводит статус плагина из `Active` (тот же путь, что при
/// накоплении серии ведёт к карантину) — менеджер обязан получить `plugins-changed` без
/// участия пользователя.
#[test]
fn pump_failure_emits_plugins_changed() {
    let _guard = serial();
    let store = store_with_doc("x");
    let views = views();
    let pending: Arc<Mutex<Vec<(String, u64)>>> = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::new(Mutex::new(Vec::<&'static str>::new()));
    let views_notify = push_log(&log, "plugin-views-changed");
    let plugins_notify = push_log(&log, "plugins-changed");
    let services: Arc<dyn HostServices> = Arc::new(
        DocumentServices::new(Arc::clone(&store))
            .with_views(Arc::clone(&views))
            .with_notify(Arc::clone(&views_notify))
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
    let host = PluginHost::new(manager, views, services, views_notify, pending)
        .with_plugins_notify(plugins_notify);

    host.run_plugin_command("test.command", Some("doc-1"));

    assert!(
        log.lock().unwrap().contains(&"plugins-changed"),
        "отказ доставки → смена статуса → plugins-changed: {:?}",
        log.lock().unwrap()
    );
}
