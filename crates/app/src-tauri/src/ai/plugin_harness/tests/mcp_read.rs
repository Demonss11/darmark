//! Тесты MCP-инструмента `plugin_read`: исходник установленного плагина по `id` (TASK-003).
//!
//! Отдельным файлом от `mcp.rs` (прогон песочницы через `plugin_test`): у инструментов разные
//! контракты, а общий файл перешагнул бы порог 400 строк. Идём через реестр,
//! а не через `PluginReadTool` напрямую: обработчик объявлен `pub(super)` в `mcp::adapters`,
//! а `pub(crate)`-видимость ради тестов запрещена.

use std::path::Path;

use mcp_core::protocol::{INTERNAL_ERROR, INVALID_PARAMS};
use mcp_core::registry::CallToolError;
use serde_json::{json, Value};

use super::common::{registry_with_journal, VALID_MANIFEST};
use crate::ai::tool_loop::MAX_TOOL_RESULT_CHARS;
use crate::plugins::scan::MAX_MANIFEST_BYTES;
use plugin_core::MAX_PLUGIN_SOURCE_BYTES;



const READ_SOURCE: &str = "function on_activate(ctx)\n  print(\"read\")\nend\n";


const READ_PLUGIN_ID: &str = "mcp-plugin-test";


fn plugin_fixture(root: &Path, id: &str, manifest: &str, source: &str) {
    plugin_dir(root, id, &[("plugin.json", manifest), ("main.lua", source)]);
}


fn plugin_dir(root: &Path, id: &str, files: &[(&str, &str)]) {
    let dir = root.join(id);
    std::fs::create_dir(&dir).expect("каталог плагина");
    for (name, content) in files {
        std::fs::write(dir.join(name), content).expect("файл плагина");
    }
}





fn plugin_read_at(root: &Path, id: &str) -> (Result<Value, CallToolError>, Vec<String>) {
    use crate::ai::history::env_lock;

    let (registry, journal) = registry_with_journal();
    let _guard = env_lock();
    let previous = std::env::var_os("DARMARK_PLUGINS_DIR");
    std::env::set_var("DARMARK_PLUGINS_DIR", root);
    let result = registry.call_tool("plugin_read", json!({ "id": id }));
    match previous {
        Some(value) => std::env::set_var("DARMARK_PLUGINS_DIR", value),
        None => std::env::remove_var("DARMARK_PLUGINS_DIR"),
    }
    let records = journal.lock().expect("журнал не отравлен").clone();
    (result, records)
}


#[test]
fn mcp_plugin_read_returns_manifest_and_source_by_id() {
    let root = tempfile::tempdir().expect("корень плагинов");
    plugin_fixture(root.path(), READ_PLUGIN_ID, VALID_MANIFEST, READ_SOURCE);

    let (call, journal) = plugin_read_at(root.path(), READ_PLUGIN_ID);

    let payload = call.expect("plugin_read отдаёт исходник установленного плагина по id");
    assert_eq!(payload["manifest"], json!(VALID_MANIFEST));
    assert_eq!(payload["source"], json!(READ_SOURCE));
    assert_eq!(
        payload["truncated"],
        json!(false),
        "исходник уложился в лимит модели — усечения нет: {payload}"
    );

    
    
    let object = payload.as_object().expect("payload — объект");
    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, vec!["manifest", "source", "truncated"], "{payload}");
    
    assert!(
        !object.contains_key("isError"),
        "payload без isError: {payload}"
    );

    
    assert_eq!(journal.len(), 1, "каждый вызов — запись: {journal:?}");
    assert!(journal[0].contains("plugin_read"), "{journal:?}");
    assert!(journal[0].contains("ok"), "{journal:?}");
    let root_path = root.path().to_string_lossy().into_owned();
    assert!(
        !journal[0].contains(&root_path),
        "журнал без полного пути: {journal:?}"
    );
    assert!(!journal[0].contains(":\\"), "журнал без диска: {journal:?}");
}


#[test]
fn mcp_plugin_read_reads_declared_entry() {
    const MANIFEST: &str = r#"{
      "id": "mcp-entry-test",
      "name": "MCP entry test",
      "version": "1.0.0",
      "api_version": 1,
      "entry": "init.lua"
    }"#;
    const ENTRY_SOURCE: &str = "function on_activate(ctx)\n  print(\"entry\")\nend\n";

    let root = tempfile::tempdir().expect("корень плагинов");
    
    plugin_dir(
        root.path(),
        "mcp-entry-test",
        &[
            ("plugin.json", MANIFEST),
            ("init.lua", ENTRY_SOURCE),
            ("main.lua", READ_SOURCE),
        ],
    );

    let (call, journal) = plugin_read_at(root.path(), "mcp-entry-test");

    let payload = call.expect("исходник объявленного entry отдаётся по id");
    assert_eq!(payload["source"], json!(ENTRY_SOURCE));
    assert_eq!(payload["truncated"], json!(false));
    assert_eq!(journal.len(), 1, "каждый вызов — запись: {journal:?}");
}



#[test]
fn mcp_plugin_read_missing_entry_writes_error_log() {
    let root = tempfile::tempdir().expect("корень плагинов");
    plugin_dir(
        root.path(),
        READ_PLUGIN_ID,
        &[("plugin.json", VALID_MANIFEST)],
    );

    let (call, journal) = plugin_read_at(root.path(), READ_PLUGIN_ID);

    let CallToolError::Execution(error) = call.expect_err("отсутствующий исходник — ошибка вызова")
    else {
        panic!("ожидалась ошибка исполнения, а не неизвестный tool");
    };
    assert_eq!(error.code, INTERNAL_ERROR, "{error:?}");
    assert!(error.message.contains("main.lua"), "{error:?}");

    assert_eq!(
        journal.len(),
        1,
        "ошибка исполнения журналируется: {journal:?}"
    );
    assert!(journal[0].contains("ошибка"), "{journal:?}");
    let root_path = root.path().to_string_lossy().into_owned();
    assert!(
        !journal[0].contains(&root_path),
        "журнал без полного пути: {journal:?}"
    );
    assert!(!journal[0].contains(":\\"), "журнал без диска: {journal:?}");
}



#[test]
fn mcp_plugin_read_rejects_files_over_read_limits() {
    let cases = [
        ("mcp-huge-manifest", "plugin.json", MAX_MANIFEST_BYTES),
        ("mcp-huge-source", "main.lua", MAX_PLUGIN_SOURCE_BYTES),
    ];

    for (id, oversized, limit) in cases {
        let root = tempfile::tempdir().expect("корень плагинов");
        plugin_dir(
            root.path(),
            id,
            &[("plugin.json", VALID_MANIFEST), ("main.lua", READ_SOURCE)],
        );
        
        let file = std::fs::File::create(root.path().join(id).join(oversized)).expect("файл");
        file.set_len(limit + 1).expect("размер над лимитом");
        drop(file);

        let (call, journal) = plugin_read_at(root.path(), id);

        let CallToolError::Execution(error) = call.expect_err("файл над лимитом — ошибка вызова")
        else {
            panic!("{id}: ожидалась ошибка исполнения, а не неизвестный tool");
        };
        assert_eq!(error.code, INTERNAL_ERROR, "{id}: {error:?}");
        assert!(error.message.contains("лимит"), "{id}: {error:?}");
        assert!(
            journal.iter().any(|record| record.contains("ошибка")),
            "{id}: ошибка обязана попасть в журнал: {journal:?}"
        );
    }
}



#[test]
fn mcp_plugin_read_truncates_source_over_model_budget() {
    let oversized = "x".repeat(MAX_TOOL_RESULT_CHARS + 1024);
    let root = tempfile::tempdir().expect("корень плагинов");
    plugin_dir(
        root.path(),
        READ_PLUGIN_ID,
        &[("plugin.json", VALID_MANIFEST), ("main.lua", &oversized)],
    );

    let (call, journal) = plugin_read_at(root.path(), READ_PLUGIN_ID);

    let payload = call.expect("усечение — не ошибка: инструмент исполнился");
    assert_eq!(payload["truncated"], json!(true), "{payload}");
    assert_eq!(
        payload["manifest"],
        json!(VALID_MANIFEST),
        "манифест отдаётся целиком: {payload}"
    );
    let fitted = payload["source"].as_str().expect("source — строка");
    assert!(
        fitted.chars().count() < oversized.chars().count(),
        "исходник урезан: {} из {}",
        fitted.chars().count(),
        oversized.chars().count()
    );
    assert!(
        fitted.chars().all(|character| character == 'x'),
        "усечение по границе UTF-8 (только целые символы исходника)"
    );

    
    
    let text = payload.to_string();
    assert!(
        text.chars().count() <= MAX_TOOL_RESULT_CHARS,
        "{} символов при лимите {MAX_TOOL_RESULT_CHARS}",
        text.chars().count()
    );
    serde_json::from_str::<Value>(&text).expect("payload остаётся валидным JSON");
    assert!(journal[0].contains("ok"), "{journal:?}");
}



#[test]
fn mcp_plugin_read_reports_manifest_over_model_budget() {
    let padding = "m".repeat(MAX_TOOL_RESULT_CHARS + 1024);
    let manifest = format!(
        r#"{{"id":"mcp-plugin-test","name":"big","version":"1.0.0","api_version":1,"entry":"main.lua","contributes":{{"settings":{{"pad":"{padding}"}}}}}}"#
    );
    let root = tempfile::tempdir().expect("корень плагинов");
    plugin_dir(
        root.path(),
        READ_PLUGIN_ID,
        &[("plugin.json", &manifest), ("main.lua", READ_SOURCE)],
    );

    let (call, journal) = plugin_read_at(root.path(), READ_PLUGIN_ID);

    let CallToolError::Execution(error) = call.expect_err("манифест над бюджетом — ошибка вызова")
    else {
        panic!("ожидалась ошибка исполнения, а не неизвестный tool");
    };
    assert_eq!(error.code, INTERNAL_ERROR, "{error:?}");
    assert!(error.message.contains("лимит модели"), "{error:?}");
    assert_eq!(journal.len(), 1, "ошибка журналируется: {journal:?}");
    assert!(journal[0].contains("ошибка"), "{journal:?}");
}


#[test]
fn mcp_plugin_read_rejects_unsafe_and_unknown_id() {
    let (registry, journal) = registry_with_journal();

    let traversal = registry
        .call_tool("plugin_read", json!({ "id": "../x" }))
        .expect_err("traversal в id отвергнут до построения пути");
    let CallToolError::Execution(traversal) = traversal else {
        panic!("ожидалась ошибка исполнения, а не неизвестный tool");
    };
    assert_eq!(
        traversal.code, INVALID_PARAMS,
        "небезопасный id — ошибка параметров: {traversal:?}"
    );

    let unknown = registry
        .call_tool("plugin_read", json!({ "id": "no-such-plugin-xyz" }))
        .expect_err("несуществующий id отвергнут");
    let CallToolError::Execution(unknown) = unknown else {
        panic!("ожидалась ошибка исполнения, а не неизвестный tool");
    };
    assert_eq!(
        unknown.code, INVALID_PARAMS,
        "несуществующий плагин — ошибка параметров: {unknown:?}"
    );

    let journal = journal.lock().expect("журнал не отравлен").clone();
    assert!(
        journal.is_empty(),
        "отказ до чтения файлов журнал не пишет: {journal:?}"
    );
}
