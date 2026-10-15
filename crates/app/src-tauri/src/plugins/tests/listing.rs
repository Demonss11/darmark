//! Проекция плагина для UI: состав `list_plugins` (команды, notices) и JSON-контракт `PluginInfo`.

use std::path::PathBuf;

use serde_json::json;

use crate::plugins::manager::load_plugins;
use crate::plugins::settings::SettingsStore;

use super::common::{services, store_with_doc, views, write_plugin};

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
            .any(|n| n == plugin_core::notices::ISOLATION_NOTICE),
        "notices: {:?}",
        list[0].notices
    );
    assert!(
        list[0]
            .notices
            .iter()
            .any(|n| n == plugin_core::notices::DOCUMENT_ACCESS_NOTICE),
        "notices: {:?}",
        list[0].notices
    );
}

#[test]
fn plugin_info_serializes_frontend_contract() {
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
