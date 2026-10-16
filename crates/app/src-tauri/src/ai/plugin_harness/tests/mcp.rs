//! Прогон песочницы через MCP-инструмент `plugin_test` без Tauri-рантайма.
//!
//! Закрывает находку FINDINGS §1.5 п.2: `plugin_test` через MCP проверялся только схемой
//! (`tests/mcp_contract.rs`), а исполнение Lua в песочнице Rust-тестом не покрывалось.
//! Реестр собирается тем же путём, что и в приложении ([`crate::mcp::build_registry_with_sink`]),
//! но со собирающим стоком журнала вместо Tauri-события `log-entry` — тест не поднимает окно
//! и event-loop. Идём через реестр, а не через `PluginTestTool` напрямую: обработчик объявлен
//! `pub(super)` в `mcp::adapters`, а `pub(crate)`-видимость ради тестов запрещена (ADR-0039 §2.3).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use mcp_core::registry::Registry;
use serde_json::{json, Value};

use crate::mcp::adapters::LogSink;
use crate::mcp::build_registry_with_sink;

/// Корень workspace: `CARGO_MANIFEST_DIR` = `crates/app/src-tauri`, три `parent()` до репозитория.
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .expect("корень workspace")
        .to_path_buf()
}

/// Собирает child-хост `darmark-plugin-host.exe` debug-профилем.
///
/// `cargo test -p darmark` не тянет `plugin-host` (это не зависимость GUI-хоста, ADR-0021),
/// поэтому без сайдкара `validate_plugin` уходит в fallback и рапортует «хост не найден».
/// Сборка кладёт exe в `<target>/debug`, откуда `resolve_sidecar` находит его рядом с тестовым
/// бинарником (`<target>/debug/deps` → `<target>/debug`) — глобальные env-переменные не трогаем.
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

/// Реестр приложения с собирающим журналом — как в `mcp_contract.rs`, но без Tauri.
fn registry_with_journal() -> (Registry, Arc<Mutex<Vec<String>>>) {
    let journal = Arc::new(Mutex::new(Vec::new()));
    let collected = Arc::clone(&journal);
    let sink: LogSink = Arc::new(move |message: &str| {
        collected
            .lock()
            .expect("журнал теста не отравлен")
            .push(message.to_string());
    });
    (
        build_registry_with_sink(sink, &std::env::temp_dir()),
        journal,
    )
}

/// Каталог плагина в `temp` — он в белом списке путей `plugin_harness` (AC-MCP-12).
fn temp_plugin(manifest: &str, source: &str) -> tempfile::TempDir {
    let dir = tempfile::Builder::new()
        .prefix("darmark-mcp-plugin-test")
        .tempdir()
        .expect("временный каталог");
    std::fs::write(dir.path().join("plugin.json"), manifest).expect("plugin.json");
    std::fs::write(dir.path().join("main.lua"), source).expect("main.lua");
    dir
}

/// Вызывает `plugin_test` через реестр и возвращает payload инструмента.
///
/// `plugin_test` кодирует провал валидации в результат (`ok: false`), а не в `Err`: ошибка
/// вызова осталась бы только для неверных параметров или пути вне белого списка. Тот же вердикт
/// payload объявляет полем `isError`, и транспорт копирует его в конверт `tools/call` —
/// поэтому журнал и карточка вызова не могут разойтись (ADR-0041 §2.2). Здесь тест берёт
/// payload напрямую из реестра, минуя конверт: проверяется паритет данных с ядром, а сам
/// конверт проверяет контракт-тест `tests/mcp_contract.rs`.
fn call_plugin_test(registry: &Registry, dir: &Path) -> Value {
    registry
        .call_tool("plugin_test", json!({ "path": dir.to_string_lossy() }))
        .expect("plugin_test зарегистрирован, путь в белом списке")
}

/// Валидный манифест (форма — как у `examples/plugins`).
const VALID_MANIFEST: &str = r#"{
  "id": "mcp-plugin-test",
  "name": "MCP plugin test",
  "version": "1.0.0",
  "api_version": 1,
  "entry": "main.lua"
}"#;

/// Невалидный манифест: пустой `id` (`ManifestError::EmptyField`).
const INVALID_MANIFEST: &str = r#"{
  "id": "",
  "name": "broken",
  "version": "1.0.0",
  "api_version": 1,
  "entry": "main.lua"
}"#;

/// `plugin_test` обязан исполнить Lua в песочнице: valid-плагин с `print("ok")` в `on_activate`
/// даёт `ok: true` и пустой список ошибок (признак успешного прогона).
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

    // Журнал фиксирует вызов tool и не несёт полного пути (R2/AC-MCP-13).
    let journal = journal.lock().expect("журнал не отравлен").clone();
    assert_eq!(journal.len(), 1, "каждый вызов — запись: {journal:?}");
    assert!(journal[0].contains("plugin_test"), "{journal:?}");
    let full_path = dir.path().to_string_lossy().into_owned();
    assert!(
        !journal[0].contains(&full_path),
        "журнал без полного пути: {journal:?}"
    );
}

/// Невалидный манифест не запускает песочницу и возвращает ошибку валидации.
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
