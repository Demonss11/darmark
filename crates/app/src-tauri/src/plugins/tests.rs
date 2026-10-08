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

use super::host::PluginHost;
use super::manager::{effective_permissions, load_plugins, PluginManager, PluginRuntime};
use super::scan::scan_plugins;
use super::services::{DocumentServices, DocumentSink, Exporter, StatusSink};
use super::settings::SettingsStore;
use super::supervisor::{Supervisor, SupervisorParams};
use super::HostServices;

/// Процессные тесты конкурируют за CPU (chatty/hang крутят цикл) и искажают замеры watchdog,
/// поэтому выполняем их строго по одному. Заодно глушим WER-диалоги Windows: иначе `abort`
/// child'а может висеть до ручного закрытия и «крах» превратится в `timeout`.
fn serial() -> std::sync::MutexGuard<'static, ()> {
    suppress_windows_error_dialogs();
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(windows)]
fn suppress_windows_error_dialogs() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        extern "system" {
            fn SetErrorMode(mode: u32) -> u32;
        }
        // SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX — не показывать WER-диалог о
        // падении потомков (режим наследуется child-процессами).
        // SAFETY: SetErrorMode не принимает указателей и не может привести к UB.
        unsafe {
            SetErrorMode(0x0001 | 0x0002);
        }
    });
}

#[cfg(not(windows))]
fn suppress_windows_error_dialogs() {}

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("plugins")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("фикстура {}: {e}", path.display()))
}

/// Корень репозитория: `CARGO_MANIFEST_DIR` = `crates/app/src-tauri`, три `parent()` до `<repo>`.
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
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

fn views() -> Arc<Mutex<super::views::PluginViews>> {
    Arc::new(Mutex::new(super::views::PluginViews::new()))
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
        Vec::new(),
        Vec::new(),
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

#[test]
fn manager_lists_and_reloads_plugin() {
    let _guard = serial();
    let store = store_with_doc("# A");
    let mut manager = PluginManager::default();
    manager.register(
        PluginRuntime::new(
            "edit",
            child_exe(),
            fixture("edit.lua"),
            vec!["document:write".to_string()],
            services(&store),
            Vec::new(),
            Vec::new(),
        )
        .with_granted(vec!["document:write".to_string()]),
    );

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

// ─── Фаза 3: манифест, сканирование, настройки ────────────────────────

fn valid_manifest(id: &str, api: u32, perms: &[&str]) -> String {
    let perms_json = serde_json::to_string(perms).unwrap();
    format!(
        r#"{{"id":"{id}","name":"{id}","version":"1.0.0","api_version":{api},"entry":"main.lua","permissions":{perms_json}}}"#
    )
}

fn write_plugin(root: &Path, id: &str, manifest: &str, lua: Option<&str>) {
    let dir = root.join(id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("plugin.json"), manifest).unwrap();
    if let Some(lua) = lua {
        fs::write(dir.join("main.lua"), lua).unwrap();
    }
}

#[test]
fn scan_finds_valid_and_reports_errors() {
    let dir = tempfile::tempdir().unwrap();
    write_plugin(
        dir.path(),
        "word-count",
        &valid_manifest("word-count", 1, &["document:read", "ui:statusbar"]),
        Some("function on_activate(ctx) end"),
    );
    write_plugin(
        dir.path(),
        "too-new",
        &valid_manifest("too-new", 2, &[]),
        Some("--"),
    );
    write_plugin(
        dir.path(),
        "bad-perm",
        &valid_manifest("bad-perm", 1, &["quantum:teleport"]),
        Some("--"),
    );
    write_plugin(
        dir.path(),
        "no-entry",
        &valid_manifest("no-entry", 1, &[]),
        None,
    );

    let report = scan_plugins(dir.path());
    assert_eq!(report.plugins.len(), 1, "валиден только word-count");
    assert_eq!(report.plugins[0].manifest.id, "word-count");
    assert_eq!(report.errors.len(), 3);

    let messages: Vec<&str> = report.errors.iter().map(|e| e.message.as_str()).collect();
    assert!(
        messages.iter().any(|m| m.contains("api_version")),
        "{messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("неизвестное разрешение")),
        "{messages:?}"
    );
    assert!(
        messages.iter().any(|m| m.contains("main.lua")),
        "{messages:?}"
    );
}

#[test]
fn loader_takes_manifest_permissions_and_settings_state() {
    let dir = tempfile::tempdir().unwrap();
    write_plugin(
        dir.path(),
        "word-count",
        &valid_manifest("word-count", 1, &["document:read"]),
        Some("function on_activate(ctx) end"),
    );

    let mut settings = SettingsStore::default();
    settings.set_plugin_enabled("word-count", false);

    let store = store_with_doc("x");
    let exe = PathBuf::from("darmark-plugin-host.exe");
    let result = load_plugins(dir.path(), &exe, services(&store), &settings, views());
    assert!(result.errors.is_empty());
    assert!(store
        .lock()
        .unwrap()
        .get(&DocumentId::new("doc-1"))
        .is_some());

    let list = result.manager.list_plugins();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, "word-count");
    assert_eq!(
        list[0].permissions,
        vec!["document:read"],
        "permissions из манифеста"
    );
    assert!(!list[0].enabled, "выключен через настройки");
    assert!(
        matches!(list[0].status, super::PluginStatus::Stopped),
        "не запускается автоматически"
    );
}

#[test]
fn loader_defaults_to_enabled() {
    let dir = tempfile::tempdir().unwrap();
    write_plugin(
        dir.path(),
        "range",
        &valid_manifest("range", 1, &["document:read"]),
        Some("function on_activate(ctx) end"),
    );

    let store = store_with_doc("x");
    let result = load_plugins(
        dir.path(),
        &PathBuf::from("darmark-plugin-host.exe"),
        services(&store),
        &SettingsStore::default(),
        views(),
    );
    let list = result.manager.list_plugins();
    assert!(list[0].enabled, "по умолчанию плагин включён");
}

#[test]
fn settings_plugin_enabled_defaults_true() {
    let mut settings = SettingsStore::default();
    assert!(settings.plugin_enabled("new-plugin"));
    settings.set_plugin_enabled("new-plugin", false);
    assert!(!settings.plugin_enabled("new-plugin"));
}

#[test]
fn reload_rereads_lua_from_disk() {
    let _guard = serial();
    let dir = tempfile::tempdir().unwrap();
    let plugin_dir = dir.path().join("edit");
    fs::create_dir_all(&plugin_dir).unwrap();
    fs::write(
        plugin_dir.join("plugin.json"),
        valid_manifest("edit", 1, &["document:write"]),
    )
    .unwrap();
    fs::write(
        plugin_dir.join("main.lua"),
        "function on_activate(ctx) host.apply_edit(\"doc-1\", 0, 0, \"A\") end",
    )
    .unwrap();

    let store = store_with_doc("");
    let mut settings = SettingsStore::default();
    // Согласие на запись выдано явно: иначе effective пуст и apply_edit вернёт permission_denied.
    settings.set_granted_permissions("edit", vec!["document:write".to_string()]);
    let result = load_plugins(
        dir.path(),
        &child_exe(),
        services(&store),
        &settings,
        views(),
    );
    assert!(
        result.errors.is_empty(),
        "ошибки скана: {:?}",
        result.errors
    );
    let mut manager = result.manager;
    manager.set_plugin_enabled("edit", true).unwrap();
    assert_eq!(doc_text(&store, "doc-1"), "A");

    // Правка `.lua` на диске + «Перезагрузить» → новая версия без рестарта (§4.7, §2.1 ревью).
    fs::write(
        plugin_dir.join("main.lua"),
        "function on_activate(ctx) host.apply_edit(\"doc-1\", 0, 0, \"B\") end",
    )
    .unwrap();
    manager.reload_plugin("edit").unwrap();
    assert_eq!(
        doc_text(&store, "doc-1"),
        "BA",
        "reload обязан перечитать файл"
    );
}

#[test]
fn duplicate_ids_are_reported_not_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    write_plugin(
        dir.path(),
        "dir-a",
        &valid_manifest("dup", 1, &[]),
        Some("function on_activate(ctx) end"),
    );
    write_plugin(
        dir.path(),
        "dir-b",
        &valid_manifest("dup", 1, &[]),
        Some("function on_activate(ctx) end"),
    );

    let store = store_with_doc("x");
    let result = load_plugins(
        dir.path(),
        &PathBuf::from("darmark-plugin-host.exe"),
        services(&store),
        &SettingsStore::default(),
        views(),
    );
    assert_eq!(
        result.manager.list_plugins().len(),
        1,
        "дубликат не регистрируется"
    );
    assert_eq!(result.errors.len(), 1);
    assert!(
        result.errors[0].message.contains("дублирующийся"),
        "{:?}",
        result.errors[0].message
    );
}

fn doc_text(store: &Arc<Mutex<DocumentStore>>, id: &str) -> String {
    store
        .lock()
        .unwrap()
        .get(&DocumentId::new(id))
        .map(|doc| doc.text.clone())
        .unwrap_or_default()
}

// ─── Фаза 4: view через DocumentServices, регистрация contributes ────────

fn view_contrib(kind: &str, title: &str, tier: u8) -> plugin_proto::manifest::ViewContrib {
    plugin_proto::manifest::ViewContrib {
        kind: kind.to_string(),
        title: title.to_string(),
        tier,
    }
}

#[test]
fn document_services_set_view_content_sanitizes_and_notifies() {
    let store = store_with_doc("x");
    let views = views();
    views
        .lock()
        .unwrap()
        .register("p", &[view_contrib("main", "Main", 1)]);

    let notified = Arc::new(Mutex::new(0u32));
    let notified_for = Arc::clone(&notified);
    let notify: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        *notified_for.lock().unwrap() += 1;
    });
    let services = DocumentServices::new(Arc::clone(&store))
        .with_views(Arc::clone(&views))
        .with_notify(notify);

    let result = services
        .handle(
            "set_view_content",
            json!({
                "view_id": "p:main",
                "html": "<img src=x onerror=alert(1)>",
                "_plugin_id": "p",
            }),
        )
        .unwrap();
    assert_eq!(result, json!(true));
    assert_eq!(*notified.lock().unwrap(), 1, "notify обязан сработать");
    let html = views.lock().unwrap().snapshot()[0].html.clone();
    assert!(!html.contains("onerror"), "html не санитизирован: {html}");
}

#[test]
fn set_view_content_rejects_foreign_plugin() {
    let store = store_with_doc("x");
    let views = views();
    views
        .lock()
        .unwrap()
        .register("p", &[view_contrib("main", "Main", 1)]);
    let services = DocumentServices::new(Arc::clone(&store)).with_views(Arc::clone(&views));

    let err = services
        .handle(
            "set_view_content",
            json!({ "view_id": "other:main", "html": "x", "_plugin_id": "p" }),
        )
        .unwrap_err();
    assert_eq!(err.code, "bad_view_id");
}

#[test]
fn set_view_content_requires_plugin_id() {
    let store = store_with_doc("x");
    let views = views();
    views
        .lock()
        .unwrap()
        .register("p", &[view_contrib("main", "Main", 1)]);
    let services = DocumentServices::new(Arc::clone(&store)).with_views(Arc::clone(&views));

    let err = services
        .handle(
            "set_view_content",
            json!({ "view_id": "p:main", "html": "x" }),
        )
        .unwrap_err();
    assert_eq!(err.code, "bad_args", "без _plugin_id вызов отвергается");
}

#[test]
fn apply_edit_defers_document_changed() {
    let store = store_with_doc("# A");
    let pending: Arc<Mutex<Vec<(String, u64)>>> = Arc::new(Mutex::new(Vec::new()));
    let services =
        DocumentServices::new(Arc::clone(&store)).with_pending_changed(Arc::clone(&pending));

    services
        .handle(
            "apply_edit",
            json!({ "doc_id": "doc-1", "start": 0, "stop": 0, "text": "X" }),
        )
        .unwrap();
    let queued = pending.lock().unwrap().clone();
    assert_eq!(
        queued,
        vec![("doc-1".to_string(), 1)],
        "правка → отложенный changed"
    );

    // Эхо-правка тем же результатом rev не двигает → новое событие не копится.
    services
        .handle(
            "apply_edit",
            json!({ "doc_id": "doc-1", "start": 0, "stop": 1, "text": "X" }),
        )
        .unwrap();
    assert_eq!(pending.lock().unwrap().len(), 1, "эхо не порождает событие");
}

#[test]
fn load_plugins_registers_contributed_views() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = r#"{"id":"word-count","name":"Word Count","version":"1.0.0","api_version":1,"entry":"main.lua","contributes":{"views":[{"kind":"stats","title":"Статистика","tier":1}]}}"#;
    write_plugin(
        dir.path(),
        "word-count",
        manifest,
        Some("function on_activate(ctx) end"),
    );

    let store = store_with_doc("x");
    let views = views();
    let result = load_plugins(
        dir.path(),
        &PathBuf::from("darmark-plugin-host.exe"),
        services(&store),
        &SettingsStore::default(),
        Arc::clone(&views),
    );
    assert!(result.errors.is_empty(), "{:?}", result.errors);

    let snapshot = views.lock().unwrap().snapshot();
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].view_id, "word-count:stats");
    assert_eq!(snapshot[0].title, "Статистика");
}

// ─── Фаза 5: экспорт, статусбар, команды, менеджер ─────────────────────

#[test]
fn export_html_success_returns_true() {
    let store = store_with_doc("x");
    let captured = Arc::new(Mutex::new(String::new()));
    let captured_for = Arc::clone(&captured);
    let exporter: Exporter = Arc::new(move |html: &str| {
        *captured_for.lock().unwrap() = html.to_string();
        Ok(true)
    });
    let services = DocumentServices::new(store).with_exporter(exporter);

    let result = services
        .handle("export_html", json!({ "html": "<p>x</p>" }))
        .unwrap();
    assert_eq!(result, json!(true));
    assert_eq!(*captured.lock().unwrap(), "<p>x</p>");
}

#[test]
fn export_html_cancel_returns_false() {
    let store = store_with_doc("x");
    let exporter: Exporter = Arc::new(|_html: &str| Ok(false));
    let services = DocumentServices::new(store).with_exporter(exporter);

    let result = services
        .handle("export_html", json!({ "html": "x" }))
        .unwrap();
    assert_eq!(result, json!(false), "отмена диалога → false, не ошибка");
}

#[test]
fn export_html_io_error_is_value() {
    let store = store_with_doc("x");
    let exporter: Exporter = Arc::new(|_html: &str| Err("диск полон".to_string()));
    let services = DocumentServices::new(store).with_exporter(exporter);

    let err = services
        .handle("export_html", json!({ "html": "x" }))
        .unwrap_err();
    assert_eq!(err.code, "export_failed");
    assert!(
        err.message.contains("диск полон"),
        "message: {}",
        err.message
    );
}

#[test]
fn export_html_without_exporter_is_error() {
    let store = store_with_doc("x");
    let services = DocumentServices::new(store);
    let err = services
        .handle("export_html", json!({ "html": "x" }))
        .unwrap_err();
    assert_eq!(err.code, "no_exporter");
}

#[test]
fn show_message_calls_status_sink() {
    let store = store_with_doc("x");
    let seen = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let seen_for = Arc::clone(&seen);
    let sink: StatusSink = Arc::new(move |plugin_id: &str, text: &str| {
        seen_for
            .lock()
            .unwrap()
            .push((plugin_id.to_string(), text.to_string()));
    });
    let services = DocumentServices::new(store).with_status_sink(sink);

    let result = services
        .handle(
            "show_message",
            json!({ "text": "Слова: 5", "_plugin_id": "word-count" }),
        )
        .unwrap();
    assert_eq!(result, json!(true), "успех show_message — значение true");
    assert_eq!(
        *seen.lock().unwrap(),
        vec![("word-count".to_string(), "Слова: 5".to_string())]
    );
}

#[test]
fn show_message_without_plugin_id_is_rejected() {
    let store = store_with_doc("x");
    let services = DocumentServices::new(store);
    let err = services
        .handle("show_message", json!({ "text": "x" }))
        .unwrap_err();
    assert_eq!(err.code, "bad_args");
}

#[test]
fn settings_functions_are_not_implemented() {
    let store = store_with_doc("x");
    let services = DocumentServices::new(store);
    for method in ["get_setting", "set_setting"] {
        let err = services.handle(method, json!({ "key": "k" })).unwrap_err();
        assert_eq!(
            err.code, "not_implemented",
            "host-call {method} в H2 не реализован"
        );
    }
}

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
            vec!["view:modify".to_string()],
            Arc::clone(&services),
            vec![view_contrib("main", "Main", 1)],
            Vec::new(),
        )
        .with_granted(vec!["view:modify".to_string()]),
    );
    let host = PluginHost::new(
        manager,
        Arc::clone(&views),
        services,
        Arc::clone(&notify),
        pending,
    );

    // Порядок критичен: view регистрируются ДО старта, иначе set_view_content из
    // on_activate вернул бы unknown_view и активация упала.
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
            vec!["view:modify".to_string()],
            Arc::clone(&services),
            vec![view_contrib("main", "Main", 1)],
            Vec::new(),
        )
        .with_granted(vec!["view:modify".to_string()]),
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

    // Повторное включение (e2e «тумблер вкл/выкл»): view регистрируется заново ДО старта,
    // on_activate снова пишет контент — вкладка обязана вернуться с непустым HTML.
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
            vec!["view:modify".to_string()],
            Arc::clone(&services),
            vec![view_contrib("main", "Main", 1)],
            Vec::new(),
        )
        .with_granted(vec!["view:modify".to_string()]),
    );
    let host = PluginHost::new(manager, views, services, notify, pending)
        .with_settings(SettingsStore::default(), Some(config.clone()));

    // Выключение из менеджера обязано сохраниться в config.json (§12) — переживает рестарт.
    host.set_enabled("viewplugin", false).expect("выключение");
    let loaded = SettingsStore::load(&config);
    assert!(
        !loaded.plugin_enabled("viewplugin"),
        "выключение не сохранилось в config.json"
    );
    let _ = std::fs::remove_dir_all(dir.path());
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

/// Собирает `DocumentServices` с записью `show_message` в общий журнал: позволяет
/// проверить, какие события дошли до плагина-наблюдателя (BUG-002).
fn services_with_status_log(
    store: &Arc<Mutex<DocumentStore>>,
    views: &Arc<Mutex<super::views::PluginViews>>,
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

/// BUG-002: `command:invoked` — широковещательное событие, его получают все активные
/// плагины, а не только владелец команды. Фильтрация по `command_id` — обязанность
/// плагина. Правка (`apply_edit`) чужого плагина порождает `document:changed`, который
/// тоже доставляется всем активным плагинам — отсюда «видимый эффект» word-count.
#[test]
fn command_invoked_is_broadcast_and_edit_notifies_other_plugin() {
    let _guard = serial();
    let store = store_with_doc("");
    let views = views();
    let pending: Arc<Mutex<Vec<(String, u64)>>> = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let services = services_with_status_log(&store, &views, &pending, &seen);

    let mut manager = PluginManager::default();
    // Владелец команды: по `test.command` правит документ.
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
    // Наблюдатель: команд не имеет, но подписан на оба события.
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

/// BUG-002 (контроль): выключенный плагин не получает события шины, хотя активный
/// сосед по той же команде продолжает работать. Подтверждает, что «эффект третьего»
/// даёт именно активный подписчик, а не доставка выключенному.
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
    // Наблюдатель выключен (не запускается): событий получать не должен.
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

/// BUG-002 (часть A): плагинный `apply_edit` обязан уведомить фронтенд через
/// `document_sink` ровно один раз — с `(doc_id, rev)` изменившейся ревизии. Эхо-правка
/// (тот же результат) ревизию не двигает и повторного уведомления не порождает.
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

    // Эхо-правка тем же результатом rev не двигает → новое уведомление не эмитится.
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
fn list_plugins_reports_commands_and_notices() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = r#"{"id":"export-html","name":"Export HTML","version":"1.0.0","api_version":1,"entry":"main.lua","permissions":["document:read"],"contributes":{"commands":[{"id":"export-html.export","title":"Экспорт в HTML","keybinding":"Ctrl+Alt+E"}]}}"#;
    write_plugin(
        dir.path(),
        "export-html",
        manifest,
        Some("function on_activate(ctx) end"),
    );

    let store = store_with_doc("x");
    let result = load_plugins(
        dir.path(),
        &PathBuf::from("darmark-plugin-host.exe"),
        services(&store),
        &SettingsStore::default(),
        views(),
    );
    assert!(result.errors.is_empty(), "{:?}", result.errors);

    let list = result.manager.list_plugins();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].commands.len(), 1);
    assert_eq!(list[0].commands[0].id, "export-html.export");
    assert_eq!(list[0].commands[0].title, "Экспорт в HTML");
    assert_eq!(
        list[0].commands[0].keybinding.as_deref(),
        Some("Ctrl+Alt+E")
    );
    assert!(
        list[0]
            .notices
            .iter()
            .any(|n| n == plugin_proto::notices::ISOLATION_NOTICE),
        "notices: {:?}",
        list[0].notices
    );
    assert!(
        list[0]
            .notices
            .iter()
            .any(|n| n == plugin_proto::notices::DOCUMENT_ACCESS_NOTICE),
        "notices: {:?}",
        list[0].notices
    );
}

#[test]
fn plugin_info_serializes_frontend_contract() {
    // Фронт (`tauri.ts`) ждёт `status.state`, сообщение в `status.message` и
    // `keybinding: null` для команды без привязки — проверяем форму JSON.
    let info = crate::PluginInfo {
        id: "p".to_string(),
        status: crate::PluginStatus::Failed {
            message: "boom".to_string(),
        },
        permissions: vec!["document:read".to_string()],
        granted_permissions: vec!["document:read".to_string()],
        enabled: false,
        commands: vec![crate::CommandInfo {
            id: "p.cmd".to_string(),
            title: "Cmd".to_string(),
            keybinding: None,
        }],
        notices: Vec::new(),
    };
    let value = serde_json::to_value(&info).unwrap();
    assert_eq!(value["status"]["state"], "failed");
    assert_eq!(value["status"]["message"], "boom");
    assert_eq!(value["enabled"], false);
    assert_eq!(value["permissions"][0], "document:read");
    assert_eq!(value["granted_permissions"][0], "document:read");
    assert!(
        value["commands"][0]["keybinding"].is_null(),
        "keybinding должен быть null, а не отсутствовать: {value}"
    );
}

// ─── Фаза 5: consent как реальный гейт прав (§11.1 п.4, F39) ─────────────

/// Чистая функция пересечения: `manifest ∩ granted`, deny-by-default, порядок — из манифеста.
#[test]
fn effective_permissions_intersects_and_rejects_garbage() {
    let manifest = vec![
        "document:read".to_string(),
        "document:write".to_string(),
        "ui:statusbar".to_string(),
    ];

    // Пересечение, сохранение порядка манифеста, чужое разрешение из `granted` отсекается.
    let granted = vec![
        "ui:statusbar".to_string(),
        "quantum:teleport".to_string(),
        "document:read".to_string(),
    ];
    assert_eq!(
        effective_permissions(&manifest, &granted),
        vec!["document:read", "ui:statusbar"],
        "пересечение, порядок манифеста, мусор отсечён"
    );

    // Пустое согласие — прав нет (deny-by-default).
    assert!(effective_permissions(&manifest, &[]).is_empty());

    // Согласие на всё, чего нет в манифесте, не даёт ничего.
    assert!(effective_permissions(&manifest, &["nope".to_string()]).is_empty());
}

/// Дефолт рантайма: права запрашиваются манифестом, но без согласия effective пуст — child
/// стартует с нулём прав (гейт на host-call, а не на старте).
#[test]
fn runtime_without_grant_has_empty_effective() {
    let store = store_with_doc("x");
    let runtime = PluginRuntime::new(
        "p",
        PathBuf::from("darmark-plugin-host.exe"),
        String::new(),
        vec!["document:write".to_string()],
        services(&store),
        Vec::new(),
        Vec::new(),
    );

    assert!(runtime.granted().is_empty(), "по умолчанию согласия нет");
    assert!(
        runtime.effective().is_empty(),
        "manifest без granted → effective пуст"
    );
    // Мусор в granted, которого нет в манифесте, не проходит в effective.
    let runtime = runtime.with_granted(vec!["evil:root".to_string()]);
    assert!(runtime.effective().is_empty(), "чужое право отсечено");
}

/// `PluginHost::set_permissions`: intersect-on-write, персист в `config.json` и отражение в
/// `list_plugins().granted_permissions` (запрашиваемые `permissions` остаются из манифеста).
#[test]
fn set_permissions_persists_and_updates_list() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.json");
    let store = store_with_doc("x");
    let views = views();
    let services = services(&store);
    let pending: Arc<Mutex<Vec<(String, u64)>>> = Arc::new(Mutex::new(Vec::new()));

    let mut manager = PluginManager::default();
    let mut runtime = PluginRuntime::new(
        "word-count",
        PathBuf::from("darmark-plugin-host.exe"),
        String::new(),
        vec!["document:read".to_string(), "ui:statusbar".to_string()],
        Arc::clone(&services),
        Vec::new(),
        Vec::new(),
    );
    // Выключен: смена прав не должна спавнить процесс.
    runtime.set_enabled(false);
    manager.register(runtime);
    let host = PluginHost::new(manager, views, services, Arc::new(|| {}), pending)
        .with_settings(SettingsStore::default(), Some(config.clone()));

    host.set_permissions(
        "word-count",
        vec!["document:read".to_string(), "quantum:teleport".to_string()],
    )
    .expect("согласие записано");

    let info = &host.list_plugins()[0];
    assert_eq!(
        info.permissions,
        vec!["document:read", "ui:statusbar"],
        "запрашиваемые — из манифеста"
    );
    assert_eq!(
        info.granted_permissions,
        vec!["document:read"],
        "согласованные — пересечение с манифестом"
    );

    let loaded = SettingsStore::load(&config);
    assert_eq!(
        loaded.granted_permissions("word-count"),
        vec!["document:read"],
        "согласие персистится"
    );

    // Повторный вызов перезаписывает набор, а не копит.
    host.set_permissions("word-count", vec!["ui:statusbar".to_string()])
        .expect("перезапись согласия");
    assert_eq!(
        host.list_plugins()[0].granted_permissions,
        vec!["ui:statusbar"]
    );
    assert_eq!(
        SettingsStore::load(&config).granted_permissions("word-count"),
        vec!["ui:statusbar"]
    );
    let _ = std::fs::remove_dir_all(dir.path());
}

/// Неизвестный id — ошибка (команда не должна молча писать в никуда).
#[test]
fn set_permissions_unknown_plugin_is_error() {
    let store = store_with_doc("x");
    let host = PluginHost::new(
        PluginManager::default(),
        views(),
        services(&store),
        Arc::new(|| {}),
        Arc::new(Mutex::new(Vec::new())),
    );

    let err = host
        .set_permissions("nope", vec!["document:read".to_string()])
        .unwrap_err();
    assert_eq!(err.code, "unknown_plugin");
}

/// Смена прав активного плагина перезапускает его (права захватываются при `start`):
/// grant → host-call проходит, revoke → `permission_denied` (правка не применяется).
#[test]
fn permission_change_restarts_active_plugin() {
    let _guard = serial();
    let store = store_with_doc("");
    let mut manager = PluginManager::default();
    let mut runtime = PluginRuntime::new(
        "edit",
        child_exe(),
        fixture("edit.lua"),
        vec!["document:write".to_string()],
        services(&store),
        Vec::new(),
        Vec::new(),
    );

    // Старт без согласия не блокируется, но гейт работает: apply_edit отклонён как значение.
    runtime.start().expect("старт с нулём прав");
    assert_eq!(
        doc_text(&store, "doc-1"),
        "",
        "deny-by-default: без согласия правка не применяется"
    );
    assert!(matches!(runtime.status, super::PluginStatus::Active));
    manager.register(runtime);

    // Выдача согласия → reload активного плагина → host-call проходит.
    manager
        .set_plugin_permissions("edit", vec!["document:write".to_string()])
        .expect("grant");
    assert_eq!(doc_text(&store, "doc-1"), "X", "grant → правка проходит");
    assert_eq!(
        manager.get("edit").unwrap().effective(),
        vec!["document:write"]
    );

    // Отзыв согласия → reload → host-call снова отклонён (permission_denied значением).
    manager
        .set_plugin_permissions("edit", Vec::new())
        .expect("revoke");
    assert_eq!(
        doc_text(&store, "doc-1"),
        "X",
        "revoke → правка не применилась"
    );
    assert!(
        manager.get("edit").unwrap().effective().is_empty(),
        "revoke → effective пуст"
    );
    assert!(
        matches!(
            manager.get("edit").unwrap().status,
            super::PluginStatus::Active
        ),
        "reload с новым набором не уронил плагин"
    );
}

/// `load_plugins` читает согласованные права из `SettingsStore`; при отсутствии записи — пусто.
#[test]
fn loader_reads_granted_permissions_from_settings() {
    let dir = tempfile::tempdir().unwrap();
    write_plugin(
        dir.path(),
        "word-count",
        &valid_manifest("word-count", 1, &["document:read", "ui:statusbar"]),
        Some("function on_activate(ctx) end"),
    );

    let store = store_with_doc("x");
    let exe = PathBuf::from("darmark-plugin-host.exe");

    // Нет записи в настройках → granted пуст (deny-by-default).
    let result = load_plugins(
        dir.path(),
        &exe,
        services(&store),
        &SettingsStore::default(),
        views(),
    );
    assert!(
        result.manager.list_plugins()[0]
            .granted_permissions
            .is_empty(),
        "default → согласия нет"
    );

    // Есть запись → granted переносится в рантайм.
    let mut settings = SettingsStore::default();
    settings.set_granted_permissions("word-count", vec!["document:read".to_string()]);
    let result = load_plugins(dir.path(), &exe, services(&store), &settings, views());
    assert_eq!(
        result.manager.list_plugins()[0].granted_permissions,
        vec!["document:read"],
        "granted из настроек"
    );
}

/// Выключенный плагин: смена прав только персистится (и не стартует), выключение сохраняется.
#[test]
fn set_permissions_on_disabled_plugin_only_persists() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.json");
    let store = store_with_doc("x");
    let services = services(&store);
    let pending: Arc<Mutex<Vec<(String, u64)>>> = Arc::new(Mutex::new(Vec::new()));

    let mut manager = PluginManager::default();
    let runtime = PluginRuntime::new(
        "word-count",
        PathBuf::from("darmark-plugin-host.exe"),
        String::new(),
        vec!["document:read".to_string()],
        Arc::clone(&services),
        Vec::new(),
        Vec::new(),
    );
    manager.register(runtime);
    let host = PluginHost::new(manager, views(), services, Arc::new(|| {}), pending)
        .with_settings(SettingsStore::default(), Some(config.clone()));

    host.set_enabled("word-count", false).expect("выключение");
    host.set_permissions("word-count", vec!["document:read".to_string()])
        .expect("согласие выключенного");

    let loaded = SettingsStore::load(&config);
    assert!(
        !loaded.plugin_enabled("word-count"),
        "выключение сохранилось"
    );
    assert_eq!(
        loaded.granted_permissions("word-count"),
        vec!["document:read"],
        "согласие персистится и для выключенного (ортогонально enabled)"
    );
    let info = &host.list_plugins()[0];
    assert!(!info.enabled);
    assert!(
        matches!(info.status, super::PluginStatus::Stopped),
        "выключенный плагин не стартует при смене прав"
    );
    assert_eq!(info.granted_permissions, vec!["document:read"]);
    let _ = std::fs::remove_dir_all(dir.path());
}

#[test]
fn reference_plugins_validate() {
    // Эталонные плагины лежат в корне репозитория (`plugins/`), в дистрибутив не входят (D6).
    let repo_root = workspace_root();
    let report = scan_plugins(&repo_root.join("plugins"));
    assert!(
        report.errors.is_empty(),
        "ошибки скана эталонных плагинов: {:?}",
        report.errors
    );
    let ids: Vec<&str> = report
        .plugins
        .iter()
        .map(|p| p.manifest.id.as_str())
        .collect();
    for expected in ["word-count", "export-html", "format-selection"] {
        assert!(ids.contains(&expected), "нет плагина {expected}: {ids:?}");
    }
}
