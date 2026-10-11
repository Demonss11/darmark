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

use super::host::{DocumentEventSink, PluginHost, StatusChangedSink};
use super::manager::{effective_permissions, load_plugins, PluginManager, PluginRuntime};
use super::plugin_settings::{PluginSettingsStore, MAX_SETTING_KEY_BYTES, MAX_SETTING_VALUE_BYTES};
use super::scan::scan_plugins;
use super::services::{DocumentServices, DocumentSink, Exporter, LogSink, StatusSink};
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

/// S12a: `list_plugins` проецирует человекочитаемые `name`/`version` из манифеста, а не
/// выводит их из `id` (фикстура намеренно даёт имя и версию, отличные от id).
#[test]
fn loader_projects_name_and_version_from_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = r#"{"id":"word-count","name":"Word Count","version":"2.3.1","api_version":1,"entry":"main.lua"}"#;
    write_plugin(
        dir.path(),
        "word-count",
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
    assert_eq!(list[0].id, "word-count");
    assert_eq!(list[0].name, "Word Count", "name из манифеста");
    assert_eq!(list[0].version, "2.3.1", "version из манифеста");
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

/// Колбэк, пишущий имя сигнала в общий журнал (F41/S2): позволяет проверить, какой именно
/// канал эмитится в каждом сценарии.
fn push_log(
    log: &Arc<Mutex<Vec<&'static str>>>,
    event: &'static str,
) -> Arc<dyn Fn() + Send + Sync> {
    let log = Arc::clone(log);
    Arc::new(move || log.lock().unwrap().push(event))
}

/// Хост с одним view-плагином (`view-activate.lua`, наполняет вкладку в `on_activate`) и
/// журналом раздельных сигналов `plugin-views-changed`/`plugins-changed` (F41/S2).
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

    // DocumentServices получает только view-сигнал; `plugins-changed` ему недоступен —
    // это структурная гарантия того, что set_view_content не дёрнет менеджер.
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
    // `event-hang.lua` зависает на доставке события → invocation снимается (статус ≠ Active).
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

/// F40/S1: ответ `get_document_text` — JSON-строка; при обилии управляющих символов
/// экранирование раздувает кадр host→child (до 6×) сверх `MAX_HOST_FRAME_BYTES`. Вместо
/// `protocol`-сбоя хост обязан вернуть **явный** `too_large` (основной путь — окна).
#[test]
fn get_document_text_rejects_escaping_overflow() {
    // ~3 МиБ управляющих символов 0x01 (каждый → `\u0001`, 6 Б) ⇒ > 16 МиБ в кадре.
    let dense = "\u{0001}".repeat(3 * 1024 * 1024);
    let store = store_with_doc(&dense);
    let services = services(&store);

    let err = services
        .handle("get_document_text", json!({ "doc_id": "doc-1" }))
        .unwrap_err();
    assert_eq!(
        err.code, "too_large",
        "плотный текст → явный отказ, не protocol"
    );
}

/// F40/S1 (контроль): текст, умещающийся в кадр даже после экранирования, отдаётся целиком —
/// гард не должен резать легитимные документы.
#[test]
fn get_document_text_allows_text_within_frame() {
    // 1 МиБ кавычек → ~2 МиБ после экранирования: заметно меньше 16 МиБ.
    let text = "\"".repeat(1024 * 1024);
    let store = store_with_doc(&text);
    let services = services(&store);

    let value = services
        .handle("get_document_text", json!({ "doc_id": "doc-1" }))
        .unwrap();
    assert_eq!(
        value.as_str().map(str::len),
        Some(text.len()),
        "текст в пределах кадра отдаётся целиком"
    );
}

#[test]
fn load_plugins_registers_contributed_views() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = r#"{"id":"word-count","name":"Word Count","version":"1.0.0","api_version":1,"entry":"main.lua","permissions":["view:create"],"contributes":{"views":[{"kind":"stats","title":"Статистика","tier":1}]}}"#;
    write_plugin(
        dir.path(),
        "word-count",
        manifest,
        Some("function on_activate(ctx) end"),
    );

    let store = store_with_doc("x");
    let views = views();
    // F44/S4: регистрация view гейтится `view:create` — выдаём согласие явно.
    let mut settings = SettingsStore::default();
    settings.set_granted_permissions("word-count", vec!["view:create".to_string()]);
    let result = load_plugins(
        dir.path(),
        &PathBuf::from("darmark-plugin-host.exe"),
        services(&store),
        &settings,
        Arc::clone(&views),
    );
    assert!(result.errors.is_empty(), "{:?}", result.errors);

    let snapshot = views.lock().unwrap().snapshot();
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].view_id, "word-count:stats");
    assert_eq!(snapshot[0].title, "Статистика");
}

/// F44/S4: тот же манифест, но без согласия на `view:create` → вкладка не регистрируется
/// (deny-by-default, как `document:*`).
#[test]
fn load_plugins_without_view_create_does_not_register_view() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = r#"{"id":"word-count","name":"Word Count","version":"1.0.0","api_version":1,"entry":"main.lua","permissions":["view:create"],"contributes":{"views":[{"kind":"stats","title":"Статистика","tier":1}]}}"#;
    write_plugin(
        dir.path(),
        "word-count",
        manifest,
        Some("function on_activate(ctx) end"),
    );

    let store = store_with_doc("x");
    let views = views();
    // Согласие не выдано: эффективный набор пуст → view не регистрируется, но и не падаем.
    let result = load_plugins(
        dir.path(),
        &PathBuf::from("darmark-plugin-host.exe"),
        services(&store),
        &SettingsStore::default(),
        Arc::clone(&views),
    );
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert!(
        views.lock().unwrap().snapshot().is_empty(),
        "без view:create вкладки быть не должно"
    );
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

/// F43/S5: `DocumentServices` с подключённым хранилищем настроек плагинов (подменяемый путь).
fn services_with_plugin_settings(
    store: &Arc<Mutex<DocumentStore>>,
    dir: &Path,
) -> Arc<dyn HostServices> {
    Arc::new(
        DocumentServices::new(Arc::clone(store))
            .with_plugin_settings(Arc::new(Mutex::new(PluginSettingsStore::new(dir)))),
    )
}

/// F43/S5: round-trip разных JSON-типов (число, строка, вложенный объект, `null`) и
/// отсутствующий ключ → `nil` (`Value::Null`, без ошибки).
#[test]
fn settings_roundtrip_types_and_missing_key() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_doc("x");
    let services = services_with_plugin_settings(&store, dir.path());

    for (key, value) in [
        ("num", json!(42)),
        ("str", json!("привет")),
        ("obj", json!({ "a": 1, "b": [true, null] })),
        ("nil", json!(null)),
    ] {
        let ok = services
            .handle(
                "set_setting",
                json!({ "key": key, "value": value, "_plugin_id": "p" }),
            )
            .unwrap();
        assert_eq!(ok, json!(true), "set_setting {key}");
        let got = services
            .handle("get_setting", json!({ "key": key, "_plugin_id": "p" }))
            .unwrap();
        assert_eq!(got, value, "round-trip {key}");
    }

    let missing = services
        .handle("get_setting", json!({ "key": "nope", "_plugin_id": "p" }))
        .unwrap();
    assert_eq!(missing, serde_json::Value::Null, "нет ключа → nil");
}

/// F43/S5: изоляция по `plugin_id` — плагины не видят настройки друг друга.
#[test]
fn settings_are_isolated_per_plugin() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_doc("x");
    let services = services_with_plugin_settings(&store, dir.path());

    services
        .handle(
            "set_setting",
            json!({ "key": "k", "value": "a", "_plugin_id": "p1" }),
        )
        .unwrap();
    services
        .handle(
            "set_setting",
            json!({ "key": "k", "value": "b", "_plugin_id": "p2" }),
        )
        .unwrap();

    assert_eq!(
        services
            .handle("get_setting", json!({ "key": "k", "_plugin_id": "p1" }))
            .unwrap(),
        json!("a")
    );
    assert_eq!(
        services
            .handle("get_setting", json!({ "key": "k", "_plugin_id": "p2" }))
            .unwrap(),
        json!("b")
    );
}

/// F43/S5: настройки переживают пересоздание стора из того же каталога (персист на диск).
#[test]
fn settings_persist_across_store_instances() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_doc("x");

    {
        let services = services_with_plugin_settings(&store, dir.path());
        services
            .handle(
                "set_setting",
                json!({ "key": "k", "value": { "n": 1 }, "_plugin_id": "p" }),
            )
            .unwrap();
    }
    let fresh = services_with_plugin_settings(&store, dir.path());
    assert_eq!(
        fresh
            .handle("get_setting", json!({ "key": "k", "_plugin_id": "p" }))
            .unwrap(),
        json!({ "n": 1 }),
        "настройка должна перечитаться с диска"
    );
}

/// F43/S5: некорректный ключ → `bad_args`, значение сверх лимита → `too_large`, паники нет.
#[test]
fn setting_limits_and_bad_key_are_errors() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_doc("x");
    let services = services_with_plugin_settings(&store, dir.path());

    let empty = services
        .handle(
            "set_setting",
            json!({ "key": "", "value": 1, "_plugin_id": "p" }),
        )
        .unwrap_err();
    assert_eq!(empty.code, "bad_args", "пустой ключ");

    let long = "k".repeat(MAX_SETTING_KEY_BYTES + 1);
    let long_err = services
        .handle("get_setting", json!({ "key": long, "_plugin_id": "p" }))
        .unwrap_err();
    assert_eq!(long_err.code, "bad_args", "слишком длинный ключ");

    let big = "x".repeat(MAX_SETTING_VALUE_BYTES + 1);
    let big_err = services
        .handle(
            "set_setting",
            json!({ "key": "k", "value": big, "_plugin_id": "p" }),
        )
        .unwrap_err();
    assert_eq!(big_err.code, "too_large", "значение сверх лимита");

    // Файл тоже ограничен: несколько крупных значений упираются в лимит файла.
    let chunk = "y".repeat(MAX_SETTING_VALUE_BYTES - 10);
    let mut saw_too_large = false;
    for i in 0..8 {
        let result = services.handle(
            "set_setting",
            json!({ "key": format!("f{i}"), "value": chunk.clone(), "_plugin_id": "p" }),
        );
        if let Err(e) = result {
            assert_eq!(e.code, "too_large", "файл настроек переполнен");
            saw_too_large = true;
            break;
        }
    }
    assert!(saw_too_large, "хранилище обязано упереться в лимит файла");
}

/// F43/S5: без подключённого хранилища host-call отвечает `no_settings`, а не падает.
#[test]
fn settings_without_store_is_error() {
    let store = store_with_doc("x");
    let services = DocumentServices::new(store);
    for method in ["get_setting", "set_setting"] {
        let err = services
            .handle(method, json!({ "key": "k", "value": 1, "_plugin_id": "p" }))
            .unwrap_err();
        assert_eq!(err.code, "no_settings", "host-call {method}");
    }
}

/// F43/S5: без `_plugin_id` (инъекции Supervisor) вызов отвергается — scope обязателен.
#[test]
fn setting_without_plugin_id_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_doc("x");
    let services = services_with_plugin_settings(&store, dir.path());
    let err = services
        .handle("get_setting", json!({ "key": "k" }))
        .unwrap_err();
    assert_eq!(err.code, "bad_args");
}

// ─── S5b: UI-форма настроек плагина (команда set_plugin_setting) ─────────

/// S5b: `PluginHost::plugin_setting_set` персистит значение из UI-формы; `list_plugins`
/// отдаёт его в `settings_values`, а схема приходит из манифеста (`contributes.settings`).
#[test]
fn plugin_setting_set_persists_and_shows_in_list() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = r#"{"id":"word-count","name":"Word Count","version":"1.0.0","api_version":1,"entry":"main.lua","contributes":{"settings":{"type":"object","properties":{"n":{"type":"integer"}}}}}"#;
    write_plugin(
        dir.path(),
        "word-count",
        manifest,
        Some("function on_activate(ctx) end"),
    );

    let settings_dir = tempfile::tempdir().unwrap();
    let store = store_with_doc("x");
    let result = load_plugins(
        dir.path(),
        &PathBuf::from("darmark-plugin-host.exe"),
        services(&store),
        &SettingsStore::default(),
        views(),
    );
    assert!(result.errors.is_empty(), "{:?}", result.errors);

    let plugin_settings = Arc::new(Mutex::new(PluginSettingsStore::new(settings_dir.path())));
    let host = PluginHost::new(
        result.manager,
        views(),
        services(&store),
        Arc::new(|| {}),
        Arc::new(Mutex::new(Vec::new())),
    )
    .with_plugin_settings(Arc::clone(&plugin_settings));

    // Схема — из манифеста; до записи значения пусты.
    let info = &host.list_plugins()[0];
    assert_eq!(
        info.settings_schema,
        Some(json!({ "type": "object", "properties": { "n": { "type": "integer" } } })),
        "settings_schema из contributes.settings"
    );
    assert_eq!(info.settings_values, json!({}), "значений ещё нет");

    host.plugin_setting_set("word-count", "n", json!(7))
        .expect("запись настройки");

    assert_eq!(
        host.list_plugins()[0].settings_values,
        json!({ "n": 7 }),
        "значение видно в list_plugins"
    );

    // Персист на диск: новый стор из того же каталога видит значение.
    let mut fresh = PluginSettingsStore::new(settings_dir.path());
    assert_eq!(
        fresh.get("word-count", "n").unwrap(),
        Some(json!(7)),
        "настройка переживает пересоздание стора"
    );
}

/// S5b: без подключённого `PluginSettingsStore` команда отвечает `no_settings`, не падает.
#[test]
fn plugin_setting_set_without_store_is_error() {
    let store = store_with_doc("x");
    let host = PluginHost::new(
        PluginManager::default(),
        views(),
        services(&store),
        Arc::new(|| {}),
        Arc::new(Mutex::new(Vec::new())),
    );

    let err = host.plugin_setting_set("p", "k", json!(1)).unwrap_err();
    assert_eq!(err.code, "no_settings");
}

/// S5b: запись настройки меняет видимое состояние менеджера → эмитится `plugins-changed`
/// (фронт перечитает `list_plugins` и покажет новое значение).
#[test]
fn plugin_setting_set_emits_plugins_changed() {
    let settings_dir = tempfile::tempdir().unwrap();
    let log = Arc::new(Mutex::new(Vec::<&'static str>::new()));
    let plugins_notify = push_log(&log, "plugins-changed");
    let store = store_with_doc("x");

    let host = PluginHost::new(
        PluginManager::default(),
        views(),
        services(&store),
        Arc::new(|| {}),
        Arc::new(Mutex::new(Vec::new())),
    )
    .with_plugin_settings(Arc::new(Mutex::new(PluginSettingsStore::new(
        settings_dir.path(),
    ))))
    .with_plugins_notify(plugins_notify);

    host.plugin_setting_set("p", "k", json!(1)).expect("запись");

    assert!(
        log.lock().unwrap().contains(&"plugins-changed"),
        "запись настройки → plugins-changed: {:?}",
        log.lock().unwrap()
    );
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
    // Манифест декларирует `view:create`, но согласие не выдано.
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
    // На старте события шины нет: плагин наполняет view только по подписке.
    assert_eq!(host.plugin_views().len(), 1, "view зарегистрирован");
    assert!(
        host.plugin_views()[0].html.is_empty(),
        "без события view пуст: {:?}",
        host.plugin_views()[0].html
    );

    // Последний документ — doc-1; шину дреним, чтобы её событие не подменило reannounce.
    host.publish_opened("doc-1", None);
    host.pump();

    // reload снимает view (пустой HTML), перезапускает child и обязан переиздать
    // `document:opened` — иначе вкладка по подписке осталась бы пустой.
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
        name: "Плагин".to_string(),
        version: "1.2.3".to_string(),
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
        settings_schema: None,
        settings_values: json!({}),
    };
    let value = serde_json::to_value(&info).unwrap();
    assert_eq!(value["status"]["state"], "failed");
    assert_eq!(value["status"]["message"], "boom");
    assert_eq!(value["name"], "Плагин", "name — в IPC-контракт (S12a)");
    assert_eq!(value["version"], "1.2.3", "version — в IPC-контракт (S12a)");
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

// ─── S11 (Фаза 2): новые события журнала ────────────────────────────────
//
// ТРИ новых события (TZ-S11 §5.2, ADR-0029 §2.4): `plugin-status-changed`,
// `document-event`, `log-entry`. Реализации (эмиттеры/sink'и) ещё нет — тесты
// фиксируют контракт по TDD и сейчас падают на компиляции. Ожидаемый API:
//
// * `host::StatusChangedSink = Arc<dyn Fn(&str, &str, &str, Option<&str>)>` +
//   `PluginHost::with_status_changed_sink` — (plugin_id, old_state, new_state, message),
//   состояния — snake_case `PluginStatus` (`stopped`/`active`/`failed`/`quarantined`);
// * `host::DocumentEventSink = Arc<dyn Fn(&str, Option<&str>, Option<u64>)>` +
//   `PluginHost::with_document_event_sink`/`publish_document_event` — (type, path, rev),
//   тип — `new`/`open`/`save`/`close`; команды `lib.rs` зовут это через `PluginState`;
// * `services::LogSink = Arc<dyn Fn(&str, &str, &str)>` + `DocumentServices::with_log_sink` —
//   (level, category, message), host-call `log` (сообщения `host.log` из Lua).

/// Одна запись `plugin-status-changed`: `(plugin_id, old_state, new_state, message)`.
type StatusTransition = (String, String, String, Option<String>);

/// Колбэк сбора `plugin-status-changed`, переиспользующий приём `push_log` (общий
/// `Arc<Mutex<Vec<…>>>`): тест читает реальные переходы, а не считает вызовы.
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

/// Хост с одним активируемым view-плагином и подключённым каналом `plugin-status-changed`
/// (по образцу `host_with_signal_log`).
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

/// S11: включение/выключение меняет статус → `plugin-status-changed` несёт
/// `(plugin_id, old_state, new_state)`. Проверяем оба перехода: `stopped → active`
/// и `active → stopped` (состояния — как в сериализации `PluginStatus`).
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

/// S11: `reload` — точка смены статуса (TZ §5.2) → событие обязано эмититься,
/// даже если итоговое состояние осталось `active` (сценарий «плагин перезагружен»).
#[test]
fn plugin_status_changed_on_reload() {
    let _guard = serial();
    let (host, seen) = host_with_status_changes();
    host.set_enabled("viewplugin", true).expect("включение");
    seen.lock().unwrap().clear();

    host.reload("viewplugin").expect("перезагрузка");
    let transitions = seen.lock().unwrap().clone();
    // Ключевой сценарий: статус не изменился (`active → active`), но reload — точка
    // смены статуса, поэтому событие всё равно обязано эмититься.
    assert!(
        transitions
            .iter()
            .any(|(id, old, new, _)| id == "viewplugin" && old == "active" && new == "active"),
        "reload без смены статуса обязан дать active → active: {transitions:?}"
    );
}

/// S11: `remove` — плагин исчезает из реестра → событие `active → stopped` с сообщением
/// об удалении (точка смены статуса `remove`, TZ §5.2).
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

/// S11 (а): смена прав на включённом, но ещё не запущенном плагине перезапускает его
/// (`reload` при `set_permissions`) → переход `stopped → active` попадает в ленту.
#[test]
fn set_permissions_emits_status_change_on_restart() {
    let _guard = serial();
    // Плагин зарегистрирован (enabled), но не запущен: старт происходит при смене прав.
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

/// S11 (б): установка на активный плагин **того же** набора прав не эмитит лишнего —
/// инвариант «эмитим только при смене статуса» (перезапуск `active → active` молчит).
#[test]
fn set_permissions_same_set_emits_no_extra_event() {
    let _guard = serial();
    let (host, seen) = host_with_status_changes();
    host.set_enabled("viewplugin", true).expect("включение");
    seen.lock().unwrap().clear();

    // Тот же набор, что уже выдан (view:create/view:modify) → reload остаётся active.
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

/// S11: `start_enabled` — автозапуск включённого, но не запущенного плагина →
/// переход `stopped → active` попадает в `plugin-status-changed`.
#[test]
fn start_enabled_emits_stopped_to_active() {
    let _guard = serial();
    // Реестр после `load_plugins`: плагин enabled, статус Stopped (старта ещё не было).
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

/// S11: отказ доставки события уводит плагин из `Active` → в ленту должна попасть
/// смена `active → failed` с диагностическим сообщением (сценарий 1 ТЗ).
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
    // `event-hang.lua` активируется, но зависает на доставке события → статус ≠ Active.
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

/// S11: команды документов эмитят `document-event` с типом действия и опциональными
/// `path`/`rev` (TZ §5.2). Проверяем все четыре типа: `new`/`open`/`save`/`close`.
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

/// S11: host-call `log` (сообщения `host.log` из Lua) пишется в `LogSink` с
/// `(level, category, message)`; логи плагина получают категорию `plugin` (TZ §5.2).
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

/// S11: без подключённого `LogSink` host-call `log` не паникует и не ломает плагин
/// (best-effort observability, как `show_message` без sink).
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
