//! Прогон песочницы через MCP-инструмент `plugin_test` без Tauri-рантайма, а также чтение
//! исходника установленного плагина `plugin_read` по `id` (TASK-003).
//!
//! Закрывает находку FINDINGS §1.5 п.2: `plugin_test` через MCP проверялся только схемой
//! (`tests/mcp_contract.rs`), а исполнение Lua в песочнице Rust-тестом не покрывалось.
//! Реестр собирается тем же путём, что и в приложении ([`crate::mcp::build_registry_with_sink`]),
//! но со собирающим стоком журнала вместо Tauri-события `log-entry` — тест не поднимает окно
//! и event-loop. Идём через реестр, а не через `PluginTestTool` напрямую: обработчик объявлен
//! `pub(super)` в `mcp::adapters`, а `pub(crate)`-видимость ради тестов запрещена (ADR-0039 §2.3).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use mcp_core::protocol::INVALID_PARAMS;
use mcp_core::registry::{CallToolError, Registry};
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

/// Валидный манифест (форма — как у `crates/plugin-catalog/examples/plugins`).
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

/// Исходник для `plugin_read` — не совпадает с манифестом, чтобы ответ нельзя было
/// спутать с эхом входа.
const READ_SOURCE: &str = "function on_activate(ctx)\n  print(\"read\")\nend\n";

/// Кладёт плагин `<root>/<id>/{plugin.json,main.lua}` (каталог создаётся).
fn plugin_fixture(root: &Path, id: &str, manifest: &str, source: &str) {
    let dir = root.join(id);
    std::fs::create_dir(&dir).expect("каталог плагина");
    std::fs::write(dir.join("plugin.json"), manifest).expect("plugin.json");
    std::fs::write(dir.join("main.lua"), source).expect("main.lua");
}

/// `plugin_read` строит путь сам из `plugins_root()` и отдаёт **ровно** манифест + entry.
///
/// `DARMARK_PLUGINS_DIR` — переменная процесса: ставим под общим env-локом `ai` и возвращаем
/// прежнее значение (как `paths.rs`), иначе параллельные тесты увидели бы промежуточный корень.
#[test]
fn mcp_plugin_read_returns_manifest_and_source_by_id() {
    use crate::ai::history::env_lock;

    let root = tempfile::tempdir().expect("корень плагинов");
    plugin_fixture(root.path(), "mcp-plugin-test", VALID_MANIFEST, READ_SOURCE);
    let (registry, journal) = registry_with_journal();

    let _guard = env_lock();
    let previous = std::env::var_os("DARMARK_PLUGINS_DIR");
    std::env::set_var("DARMARK_PLUGINS_DIR", root.path());
    let call = registry.call_tool("plugin_read", json!({ "id": "mcp-plugin-test" }));
    match previous {
        Some(value) => std::env::set_var("DARMARK_PLUGINS_DIR", value),
        None => std::env::remove_var("DARMARK_PLUGINS_DIR"),
    }

    let payload = call.expect("plugin_read отдаёт исходник установленного плагина по id");
    assert_eq!(payload["manifest"], json!(VALID_MANIFEST));
    assert_eq!(payload["source"], json!(READ_SOURCE));

    // Контракт ответа — ровно `manifest` + `source`: ни настроек, ни документа (TASK-003 §6.3).
    let object = payload.as_object().expect("payload — объект");
    assert_eq!(object.len(), 2, "ответ только manifest+source: {payload}");
    // ADR-0041 §2.2 п.15: `plugin_read` не верификатор — поля `isError` в payload нет.
    assert!(
        payload.get("isError").is_none(),
        "payload без isError: {payload}"
    );

    // Журнал фиксирует вызов tool и не несёт полного пути (R2/AC-MCP-13).
    let journal = journal.lock().expect("журнал не отравлен").clone();
    assert_eq!(journal.len(), 1, "каждый вызов — запись: {journal:?}");
    assert!(journal[0].contains("plugin_read"), "{journal:?}");
    let root_path = root.path().to_string_lossy().into_owned();
    assert!(
        !journal[0].contains(&root_path),
        "журнал без полного пути: {journal:?}"
    );
    assert!(!journal[0].contains(":\\"), "журнал без диска: {journal:?}");
}

/// Небезопасный и несуществующий `id` отвергаются **до** чтения файлов и без записи в журнал.
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
