//! Тесты Фазы 2: Supervisor на фикстурах `crash`/`hang`/`chatty`/`edit` и карантин.
//!
//! Гоняют реальный `darmark-plugin-host.exe`. Если его нет, тест пытается собрать child
//! (`cargo build -p plugin-host`); это делает `cargo test -p darmark` самодостаточным.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime};

use serde_json::json;

use crate::state::{DocumentId, DocumentStore};

use super::manager::{PluginManager, PluginRuntime};
use super::services::DocumentServices;
use super::supervisor::{Supervisor, SupervisorParams};

/// Процессные тесты конкурируют за CPU (chatty/hang крутят цикл) и искажают замеры watchdog,
/// поэтому выполняем их строго по одному.
fn serial() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("plugins")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("фикстура {}: {e}", path.display()))
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("корень workspace")
        .to_path_buf()
}

/// Путь к child-бинарнику. Пересобирает его, если он отсутствует или **старше** исходников
/// `plugin-host`/`plugin-proto`/`md-core` — иначе тесты молча гоняли бы устаревший exe (§4.3).
fn child_exe() -> PathBuf {
    if let Ok(path) = std::env::var("DARMARK_PLUGIN_HOST_EXE") {
        return PathBuf::from(path);
    }
    let root = workspace_root();
    let stale = match Supervisor::resolve_child_exe() {
        Some(path) => !is_fresh(&path, &root),
        None => true,
    };
    if stale {
        let status = std::process::Command::new(env!("CARGO"))
            .args(["build", "-p", "plugin-host"])
            .current_dir(&root)
            .status()
            .expect("запуск cargo build -p plugin-host");
        assert!(status.success(), "не удалось собрать plugin-host");
    }
    Supervisor::resolve_child_exe().expect("child exe после сборки")
}

/// mtime child'а не старее самого нового исходника его зависимости.
fn is_fresh(exe: &Path, root: &Path) -> bool {
    let Ok(exe_time) = fs::metadata(exe).and_then(|m| m.modified()) else {
        return false;
    };
    let sources = [
        root.join("crates/plugin-host"),
        root.join("crates/plugin-proto"),
        root.join("crates/md-core"),
    ];
    match sources.iter().filter_map(|dir| newest_mtime(dir)).max() {
        Some(newest) => exe_time >= newest,
        None => false,
    }
}

fn newest_mtime(dir: &Path) -> Option<SystemTime> {
    let mut newest = None;
    for entry in fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        let time = if meta.is_dir() {
            newest_mtime(&path)
        } else {
            meta.modified().ok()
        };
        if let Some(time) = time {
            newest = Some(newest.map_or(time, |current: SystemTime| current.max(time)));
        }
    }
    newest
}

/// Стор с одним документом `doc-1` (id плагины адресуют строкой).
fn store_with_doc(text: &str) -> Arc<Mutex<DocumentStore>> {
    let mut store = DocumentStore::default();
    let snap = store.create(text.to_string());
    assert_eq!(snap.id.as_str(), "doc-1");
    Arc::new(Mutex::new(store))
}

fn services(store: &Arc<Mutex<DocumentStore>>) -> Arc<dyn super::HostServices> {
    Arc::new(DocumentServices::new(Arc::clone(store)))
}

fn sup(id: &str, file: &str, perms: &[&str], store: &Arc<Mutex<DocumentStore>>) -> Supervisor {
    let params = SupervisorParams::new(child_exe(), id, services(store))
        .with_permissions(perms.iter().map(|p| p.to_string()).collect());
    Supervisor::start(params, &fixture(file)).expect("start + load")
}

fn doc_rev(store: &Arc<Mutex<DocumentStore>>) -> u64 {
    store
        .lock()
        .unwrap()
        .get(&DocumentId::new("doc-1"))
        .unwrap()
        .rev
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
    // Abort-разбор Windows заметно медленнее дефолтного прогресс-бюджета — даём запас,
    // чтобы именно разрыв stdio (crashed), а не таймер, определил исход.
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
    // Прогресс не должен срабатывать: chatty постоянно шлёт host-call'ы.
    params.progress = Duration::from_secs(30);
    params.deadline = Duration::from_millis(500);
    let mut supervisor = Supervisor::start(params, &fixture("chatty.lua")).unwrap();

    let err = supervisor.activate().expect_err("chatty должен быть снят");
    assert_eq!(err.code, "timeout", "снятие по абсолютному дедлайну");
}

#[test]
fn permission_denied_is_handled_as_value() {
    let _guard = serial();
    // edit-denied.lua сам проверяет, что отказ пришёл значением ({code,permission}),
    // а on_activate не прервался (иначе Lua-assert упал бы и activate вернул ошибку).
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
    )
    .with_watchdog(Duration::from_millis(300), Duration::from_secs(30));

    runtime.start().expect("активация успешна");
    assert!(matches!(runtime.status, super::PluginStatus::Active));

    let err = runtime
        .invoke("event", json!({ "name": "x", "payload": null }))
        .expect_err("зависание в on_event");
    assert_eq!(err.code, "timeout");
    assert!(
        !matches!(runtime.status, super::PluginStatus::Active),
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

#[test]
fn manager_lists_and_reloads_plugin() {
    let _guard = serial();
    let store = store_with_doc("# A");
    let mut manager = PluginManager::default();
    manager.register(PluginRuntime::new(
        "edit",
        child_exe(),
        fixture("edit.lua"),
        vec!["document:write".to_string()],
        services(&store),
    ));

    manager.set_plugin_enabled("edit", true).expect("включение");
    let list = manager.list_plugins();
    assert_eq!(list.len(), 1);
    assert!(matches!(list[0].status, super::PluginStatus::Active));

    manager.reload_plugin("edit").expect("перезагрузка");
    assert!(matches!(
        manager.get("edit").unwrap().status,
        super::PluginStatus::Active
    ));
}
