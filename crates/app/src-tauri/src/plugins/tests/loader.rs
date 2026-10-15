//! Скан манифестов, `load_plugins`, перезагрузка и contributed-views плагинов.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use crate::plugins::manager::{load_plugins, PluginManager, PluginRuntime};
use crate::plugins::scan::scan_plugins;
use crate::plugins::settings::SettingsStore;
use crate::plugins::PluginStatus;
use crate::state::DocumentId;

use super::common::{
    child_exe, doc_text, fixture, serial, services, store_with_doc, valid_manifest, views,
    workspace_root, write_plugin,
};

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
        matches!(list[0].status, PluginStatus::Stopped),
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
    assert!(matches!(list[0].status, PluginStatus::Active));

    manager.reload_plugin("edit").expect("перезагрузка");
    assert!(matches!(
        manager.get("edit").unwrap().status,
        PluginStatus::Active
    ));
}

#[test]
fn reference_plugins_validate() {
    let repo_root = workspace_root();
    let report = scan_plugins(&repo_root.join("examples").join("plugins"));
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
