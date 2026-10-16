//! Хранилище настроек плагинов (`PluginSettingsStore`) и команда `plugin_setting_set` (S5b).

use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde_json::json;

use crate::plugins::host::PluginHost;
use crate::plugins::manager::{load_plugins, PluginManager};
use crate::plugins::plugin_settings::{
    PluginSettingsStore, MAX_SETTING_KEY_BYTES, MAX_SETTING_VALUE_BYTES,
};
use crate::plugins::services::DocumentServices;
use crate::plugins::settings::SettingsStore;
use crate::plugins::HostServices;
use crate::state::DocumentStore;

use super::common::{push_log, services, store_with_doc, views, write_plugin};


fn services_with_plugin_settings(
    store: &Arc<Mutex<DocumentStore>>,
    dir: &Path,
) -> Arc<dyn HostServices> {
    Arc::new(
        DocumentServices::new(Arc::clone(store))
            .with_plugin_settings(Arc::new(Mutex::new(PluginSettingsStore::new(dir)))),
    )
}



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
    let mut fresh = PluginSettingsStore::new(settings_dir.path());
    assert_eq!(
        fresh.get("word-count", "n").unwrap(),
        Some(json!(7)),
        "настройка переживает пересоздание стора"
    );
}


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
