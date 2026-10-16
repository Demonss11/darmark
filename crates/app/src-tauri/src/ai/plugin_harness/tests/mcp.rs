//! Прогон песочницы через MCP-инструмент `plugin_test` без Tauri-рантайма.
//!
//! Закрывает пробел: `plugin_test` через MCP проверялся только схемой
//! (`tests/mcp_contract.rs`), а исполнение Lua в песочнице Rust-тестом не покрывалось.
//! Реестр собирается тем же путём, что и в приложении ([`crate::mcp::build_registry_with_sink`]),
//! но со собирающим стоком журнала вместо Tauri-события `log-entry` — тест не поднимает окно
//! и event-loop. Идём через реестр, а не через `PluginTestTool` напрямую: обработчик объявлен
//! `pub(super)` в `mcp::adapters`, а `pub(crate)`-видимость ради тестов запрещена.

use std::path::{Path, PathBuf};

use mcp_core::registry::Registry;
use serde_json::{json, Value};

use super::common::{registry_with_journal, VALID_MANIFEST};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .expect("корень workspace")
        .to_path_buf()
}

fn ensure_plugin_host() {
    if std::env::var_os("DARMARK_PLUGIN_HOST_EXE").is_some() {
        return;
    }
    let status = std::process::Command::new(env!("CARGO"))
        .args(["build", "-p", "plugin-host"])
        .current_dir(workspace_root())
        .status()
        .expect("запуск cargo build -p plugin-host");
    assert!(status.success(), "не удалось собрать plugin-host");
}

fn temp_plugin(manifest: &str, source: &str) -> tempfile::TempDir {
    let dir = tempfile::Builder::new()
        .prefix("darmark-mcp-plugin-test")
        .tempdir()
        .expect("временный каталог");
    std::fs::write(dir.path().join("plugin.json"), manifest).expect("plugin.json");
    std::fs::write(dir.path().join("main.lua"), source).expect("main.lua");
    dir
}

fn call_plugin_test(registry: &Registry, dir: &Path) -> Value {
    registry
        .call_tool("plugin_test", json!({ "path": dir.to_string_lossy() }))
        .expect("plugin_test зарегистрирован, путь в белом списке")
}

const INVALID_MANIFEST: &str = r#"{
  "id": "",
  "name": "broken",
  "version": "1.0.0",
  "api_version": 1,
  "entry": "main.lua"
}"#;

#[test]
fn mcp_plugin_test_executes_lua_in_sandbox() {
    ensure_plugin_host();
    let dir = temp_plugin(
        VALID_MANIFEST,
        "function on_activate(ctx)\n  print(\"ok\")\nend\n",
    );
    let (registry, journal) = registry_with_journal();

    let payload = call_plugin_test(&registry, dir.path());

    assert_eq!(
        payload["ok"],
        json!(true),
        "песочница обязана исполнить Lua и вернуть ok: {payload}"
    );
    assert_eq!(
        payload["errors"],
        json!([]),
        "успешный прогон без ошибок песочницы: {payload}"
    );

    let journal = journal.lock().expect("журнал не отравлен").clone();
    assert_eq!(journal.len(), 1, "каждый вызов — запись: {journal:?}");
    assert!(journal[0].contains("plugin_test"), "{journal:?}");
    let full_path = dir.path().to_string_lossy().into_owned();
    assert!(
        !journal[0].contains(&full_path),
        "журнал без полного пути: {journal:?}"
    );
}

#[test]
fn mcp_plugin_test_reports_invalid_manifest() {
    let dir = temp_plugin(INVALID_MANIFEST, "function on_activate(ctx) end\n");
    let (registry, _journal) = registry_with_journal();

    let payload = call_plugin_test(&registry, dir.path());

    assert_eq!(
        payload["ok"],
        json!(false),
        "невалидный манифест не проходит проверку: {payload}"
    );
    let errors = payload["errors"].as_array().expect("errors — массив");
    assert!(!errors.is_empty(), "ядро вернуло ошибку: {payload}");
}
