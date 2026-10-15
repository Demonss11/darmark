//! Consent как гейт прав: эффективные права, выдача/отзыв и персист согласия.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::plugins::host::PluginHost;
use crate::plugins::manager::{effective_permissions, load_plugins, PluginManager, PluginRuntime};
use crate::plugins::settings::SettingsStore;
use crate::plugins::PluginStatus;

use super::common::{
    child_exe, doc_text, fixture, serial, services, store_with_doc, valid_manifest, views,
    write_plugin,
};

/// Чистая функция пересечения: `manifest ∩ granted`, deny-by-default, порядок — из манифеста.
#[test]
fn effective_permissions_intersects_and_rejects_garbage() {
    let manifest = vec![
        "document:read".to_string(),
        "document:write".to_string(),
        "ui:statusbar".to_string(),
    ];
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
    assert!(effective_permissions(&manifest, &[]).is_empty());
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
    runtime.start().expect("старт с нулём прав");
    assert_eq!(
        doc_text(&store, "doc-1"),
        "",
        "deny-by-default: без согласия правка не применяется"
    );
    assert!(matches!(runtime.status, PluginStatus::Active));
    manager.register(runtime);
    manager
        .set_plugin_permissions("edit", vec!["document:write".to_string()])
        .expect("grant");
    assert_eq!(doc_text(&store, "doc-1"), "X", "grant → правка проходит");
    assert_eq!(
        manager.get("edit").unwrap().effective(),
        vec!["document:write"]
    );
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
        matches!(manager.get("edit").unwrap().status, PluginStatus::Active),
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
        matches!(info.status, PluginStatus::Stopped),
        "выключенный плагин не стартует при смене прав"
    );
    assert_eq!(info.granted_permissions, vec!["document:read"]);
    let _ = std::fs::remove_dir_all(dir.path());
}
