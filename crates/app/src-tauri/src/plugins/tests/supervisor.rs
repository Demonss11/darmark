//! Процессный Supervisor: спавн child, таймауты прогресса и дедлайна, крах, карантин.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::json;

use crate::plugins::manager::PluginRuntime;
use crate::plugins::supervisor::{Supervisor, SupervisorParams};
use crate::plugins::PluginStatus;
use crate::state::{DocumentId, DocumentStore};

use super::common::{child_exe, doc_rev, fixture, serial, services, store_with_doc};

fn sup(id: &str, file: &str, perms: &[&str], store: &Arc<Mutex<DocumentStore>>) -> Supervisor {
    let params = SupervisorParams::new(child_exe(), id, services(store))
        .with_permissions(perms.iter().map(|p| p.to_string()).collect());
    Supervisor::start(params, &fixture(file)).expect("start + load")
}

#[test]
fn edit_bumps_rev_through_host_call() {
    let _guard = serial();
    let store = store_with_doc("# A");
    let mut supervisor = sup("edit", "edit.lua", &["document:write"], &store);

    supervisor.activate().expect("activate");
    drop(supervisor);

    assert_eq!(doc_rev(&store), 1, "плагинная правка обязана поднять rev");
    assert_eq!(
        store
            .lock()
            .unwrap()
            .get(&DocumentId::new("doc-1"))
            .unwrap()
            .text,
        "X# A"
    );
}

#[test]
fn crash_is_isolated_and_returns_crashed() {
    let _guard = serial();
    let store = store_with_doc("x");
    let mut params = SupervisorParams::new(child_exe(), "crash", services(&store));
    params.progress = Duration::from_secs(15);
    params.deadline = Duration::from_secs(30);
    let mut supervisor = Supervisor::start(params, &fixture("crash.lua")).unwrap();

    let err = supervisor.activate().expect_err("activate должен упасть");
    assert_eq!(err.code, "crashed", "крах child → code=crashed, GUI жив");
}

#[test]
fn hang_is_killed_by_progress_timeout() {
    let _guard = serial();
    let store = store_with_doc("x");
    let mut params = SupervisorParams::new(child_exe(), "hang", services(&store))
        .with_permissions(vec!["document:read".to_string()]);
    params.progress = Duration::from_millis(300);
    params.deadline = Duration::from_secs(30);
    let mut supervisor = Supervisor::start(params, &fixture("hang.lua")).unwrap();

    let err = supervisor.activate().expect_err("hang должен быть снят");
    assert_eq!(err.code, "timeout", "снятие по прогресс-таймауту");
}

#[test]
fn chatty_is_killed_by_deadline() {
    let _guard = serial();
    let store = store_with_doc("x");
    let mut params = SupervisorParams::new(child_exe(), "chatty", services(&store))
        .with_permissions(vec!["document:read".to_string()]);
    params.progress = Duration::from_secs(30);
    params.deadline = Duration::from_millis(500);
    let mut supervisor = Supervisor::start(params, &fixture("chatty.lua")).unwrap();

    let err = supervisor.activate().expect_err("chatty должен быть снят");
    assert_eq!(err.code, "timeout", "снятие по абсолютному дедлайну");
}

#[test]
fn permission_denied_is_handled_as_value() {
    let _guard = serial();
    let store = store_with_doc("# A");
    let mut supervisor = sup("edit-denied", "edit-denied.lua", &[], &store);

    supervisor
        .activate()
        .expect("on_activate обязан завершиться без исключения");
    assert_eq!(doc_rev(&store), 0, "правка не должна примениться");
}

#[test]
fn invocation_failure_clears_supervisor() {
    let _guard = serial();
    let store = store_with_doc("x");
    let mut runtime = PluginRuntime::new(
        "event-hang",
        child_exe(),
        fixture("event-hang.lua"),
        Vec::new(),
        services(&store),
        Vec::new(),
        Vec::new(),
    )
    .with_watchdog(Duration::from_millis(300), Duration::from_secs(30));

    runtime.start().expect("активация успешна");
    assert!(matches!(runtime.status, PluginStatus::Active));

    let err = runtime
        .invoke("event", json!({ "name": "x", "payload": null }))
        .expect_err("зависание в on_event");
    assert_eq!(err.code, "timeout");
    assert!(
        !matches!(runtime.status, PluginStatus::Active),
        "после отказа invocation статус не Active"
    );

    let again = runtime
        .invoke("event", json!({}))
        .expect_err("supervisor уже снят");
    assert_eq!(again.code, "not_running");
}

#[test]
fn quarantine_after_three_failures() {
    let _guard = serial();
    let store = store_with_doc("x");
    let mut runtime = PluginRuntime::new(
        "crash",
        child_exe(),
        fixture("crash.lua"),
        Vec::new(),
        services(&store),
        Vec::new(),
        Vec::new(),
    )
    .with_watchdog(Duration::from_secs(15), Duration::from_secs(30));

    for attempt in 1..=3 {
        let err = runtime.start().expect_err("crash должен падать");
        assert_eq!(err.code, "crashed", "попытка {attempt}");
    }
    assert!(runtime.is_quarantined(), "3 падения подряд → карантин");

    let err = runtime.start().expect_err("в карантине не спавним");
    assert_eq!(err.code, "quarantined");
}

#[test]
fn manual_enable_clears_quarantine() {
    let _guard = serial();
    let store = store_with_doc("x");
    let mut runtime = PluginRuntime::new(
        "crash",
        child_exe(),
        fixture("crash.lua"),
        Vec::new(),
        services(&store),
        Vec::new(),
        Vec::new(),
    )
    .with_watchdog(Duration::from_secs(15), Duration::from_secs(30));
    for _ in 0..3 {
        let _ = runtime.start();
    }
    assert!(runtime.is_quarantined());

    runtime.set_enabled(true);
    assert!(
        !runtime.is_quarantined(),
        "ручное включение снимает карантин"
    );
}
