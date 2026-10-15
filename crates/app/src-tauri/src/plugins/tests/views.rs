//! Регистрация и жизненный цикл view: enable/reload и выдача/отзыв `view:create`.

use std::sync::{Arc, Mutex};

use crate::plugins::host::PluginHost;
use crate::plugins::manager::{PluginManager, PluginRuntime};
use crate::plugins::services::DocumentServices;
use crate::plugins::HostServices;

use super::common::{child_exe, fixture, serial, store_with_doc, view_contrib, views};

#[test]
fn set_enabled_registers_view_before_activation() {
    let _guard = serial();
    let store = store_with_doc("x");
    let views = views();
    let pending: Arc<Mutex<Vec<(String, u64)>>> = Arc::new(Mutex::new(Vec::new()));
    let notify_calls = Arc::new(Mutex::new(0u32));
    let notify_calls_for = Arc::clone(&notify_calls);
    let notify: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        *notify_calls_for.lock().unwrap() += 1;
    });
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
    let host = PluginHost::new(
        manager,
        Arc::clone(&views),
        services,
        Arc::clone(&notify),
        pending,
    );
    host.set_enabled("viewplugin", true).expect("включение");

    let snapshot = views.lock().unwrap().snapshot();
    assert_eq!(snapshot.len(), 1, "view зарегистрирован");
    assert_eq!(snapshot[0].view_id, "viewplugin:main");
    assert!(
        !snapshot[0].html.is_empty(),
        "on_activate записал контент: {:?}",
        snapshot[0].html
    );
    assert!(
        *notify_calls.lock().unwrap() >= 1,
        "notify обязан сработать"
    );
}

#[test]
fn set_enabled_false_removes_views() {
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
    let host = PluginHost::new(
        manager,
        Arc::clone(&views),
        services,
        Arc::clone(&notify),
        pending,
    );

    host.set_enabled("viewplugin", true).expect("включение");
    assert_eq!(views.lock().unwrap().snapshot().len(), 1);

    host.set_enabled("viewplugin", false).expect("выключение");
    assert!(
        views.lock().unwrap().snapshot().is_empty(),
        "выключение снимает view плагина"
    );
    host.set_enabled("viewplugin", true)
        .expect("повторное включение");
    let snapshot = views.lock().unwrap().snapshot();
    assert_eq!(
        snapshot.len(),
        1,
        "view вернулся после повторного включения"
    );
    assert!(
        !snapshot[0].html.is_empty(),
        "on_activate повторно записал контент: {:?}",
        snapshot[0].html
    );
}

/// F44/S4: выдача `view:create` активному плагину регистрирует view, отзыв — снимает.
/// Доказывает, что регистрация следует за согласием (deny-by-default), а не за манифестом.
#[test]
fn granting_view_create_adds_and_revoking_removes_view() {
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
    manager.register(PluginRuntime::new(
        "viewplugin",
        child_exe(),
        fixture("view-activate.lua"),
        vec!["view:create".to_string(), "view:modify".to_string()],
        Arc::clone(&services),
        vec![view_contrib("main", "Main", 1)],
        Vec::new(),
    ));
    let host = PluginHost::new(manager, Arc::clone(&views), services, notify, pending);

    host.set_enabled("viewplugin", true).expect("включение");
    assert!(
        views.lock().unwrap().snapshot().is_empty(),
        "без согласия на view:create вкладки нет"
    );

    host.set_permissions(
        "viewplugin",
        vec!["view:create".to_string(), "view:modify".to_string()],
    )
    .expect("выдача прав");
    assert_eq!(
        views.lock().unwrap().snapshot().len(),
        1,
        "выдача view:create регистрирует вкладку"
    );

    host.set_permissions("viewplugin", vec!["view:modify".to_string()])
        .expect("отзыв view:create");
    assert!(
        views.lock().unwrap().snapshot().is_empty(),
        "отзыв view:create снимает вкладку"
    );
}

/// Хост с одним view-плагином `viewplugin`, наполняющим вкладку **только** по событию
/// (`view-event.lua`, не в `on_activate`); права `view:modify` выданы.
fn view_event_host() -> PluginHost {
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
            fixture("view-event.lua"),
            vec!["view:create".to_string(), "view:modify".to_string()],
            Arc::clone(&services),
            vec![view_contrib("main", "Main", 1)],
            Vec::new(),
        )
        .with_granted(vec!["view:create".to_string(), "view:modify".to_string()]),
    );
    PluginHost::new(manager, views, services, notify, pending)
}

/// После `reload` contributed view снимается и регистрируется заново с пустым HTML, а шина
/// уже пуста — плагин, наполняющий вкладку по подписке на `document:opened`, без
/// переиздания остался бы пустым. Хост обязан адресно переиздать ему последний документ.
#[test]
fn reload_reannounces_opened_to_view_plugin() {
    let _guard = serial();
    let host = view_event_host();

    host.set_enabled("viewplugin", true).expect("включение");
    assert_eq!(host.plugin_views().len(), 1, "view зарегистрирован");
    assert!(
        host.plugin_views()[0].html.is_empty(),
        "без события view пуст: {:?}",
        host.plugin_views()[0].html
    );
    host.publish_opened("doc-1", None);
    host.pump();
    host.reload("viewplugin").expect("перезагрузка");

    let html = host.plugin_views()[0].html.clone();
    assert!(
        !html.is_empty() && html.contains("e2e-marker"),
        "reload обязан переиздать document:opened и наполнить view: {html:?}"
    );
}

/// Тот же сценарий, но документ не публиковался: переиздавать нечего — reannounce no-op,
/// view остаётся пустым (хост не выдумывает документ).
#[test]
fn reload_without_document_leaves_view_empty() {
    let _guard = serial();
    let host = view_event_host();

    host.set_enabled("viewplugin", true).expect("включение");
    host.reload("viewplugin").expect("перезагрузка");

    let snapshot = host.plugin_views();
    assert_eq!(snapshot.len(), 1, "view зарегистрирован");
    assert!(
        snapshot[0].html.is_empty(),
        "без документа переиздание — no-op: {:?}",
        snapshot[0].html
    );
}
