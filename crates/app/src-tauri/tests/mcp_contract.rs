//! Контракт-тесты MCP: инвентарь, схема ↔ поведение (AC-MCP-2/3/6) и приватность
//! журнала (AC-MCP-13) — на **реальных** адаптерах приложения, не на заглушках.
//!
//! Реестр собирается тем же путём, что и в приложении
//! (`darmark_lib::mcp::build_registry_with_sink`), но с собирающим стоком журнала
//! вместо Tauri-события `log-entry`: тест не поднимает окно, event-loop и mock-рантайм.
//!
//! Проверяется совпадение результата адаптера с прямым вызовом ядра (`md_core`,
//! `plugin_harness`, `plugin_catalog`) — именно это означает «адаптеры не дублируют
//! логику» (TZ-MCP R4/AC-MCP-6): разойдись они, тест упадёт.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[cfg(windows)]
use std::path::Path;

use darmark_lib::mcp::adapters::LogSink;
use darmark_lib::mcp::build_registry_with_sink;
use mcp_core::protocol::{RpcRequest, RpcResponse};
use mcp_core::server::McpServer;
use serde_json::{json, Value};

// ─── Инфраструктура теста ──────────────────────────────────────────

/// Сервер с реальным инвентарём приложения и собирающим журналом.
///
/// Возвращает и сток записей: приватность (AC-MCP-13) проверяется по факту эмиссии.
fn server() -> (McpServer, Arc<Mutex<Vec<String>>>) {
    let records = Arc::new(Mutex::new(Vec::new()));
    let collected = Arc::clone(&records);
    let sink: LogSink = Arc::new(move |message: &str| {
        collected
            .lock()
            .expect("журнал теста не отравлен")
            .push(message.to_string());
    });
    (McpServer::new(build_registry_with_sink(sink)), records)
}

fn request(method: &str, params: Option<Value>) -> RpcRequest {
    RpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(1)),
        method: method.into(),
        params,
    }
}

/// Успешный `result` ответа — с проверкой, что протокольной ошибки нет.
fn result_of(response: &RpcResponse) -> Value {
    assert!(
        response.error.is_none(),
        "протокольная ошибка: {:?}",
        response.error
    );
    response.result.clone().expect("result")
}

/// Вызов tool через MCP: возвращает конверт `{content, isError?}`.
fn call_tool(server: &McpServer, name: &str, arguments: Value) -> Value {
    let response = server.handle(request(
        "tools/call",
        Some(json!({ "name": name, "arguments": arguments })),
    ));
    result_of(&response)
}

/// JSON-значение, которое tool вернул в `content[0].text`.
fn tool_payload(envelope: &Value) -> Value {
    let text = envelope["content"][0]["text"]
        .as_str()
        .expect("content[0].text — строка");
    serde_json::from_str(text).expect("payload инструмента — JSON")
}

fn is_error(envelope: &Value) -> bool {
    envelope["isError"].as_bool().unwrap_or(false)
}

/// Записи журнала, снятые со собирающего стока.
fn records_of(records: &Arc<Mutex<Vec<String>>>) -> Vec<String> {
    records.lock().expect("журнал теста не отравлен").clone()
}

/// Имена tools, которые приложение обязано зарегистрировать (TZ-MCP §4).
fn expected_tools() -> Vec<&'static str> {
    #[cfg(windows)]
    {
        vec![
            "plugin_validate",
            "plugin_test",
            "plugin_catalog_list",
            "markdown_render",
        ]
    }
}

/// URI ресурсов, которые приложение обязано зарегистрировать (TZ-MCP §4).
fn expected_resources() -> Vec<&'static str> {
    vec![
        "darmark://contracts/plugin-manifest",
        "darmark://contracts/host-api",
        "darmark://contracts/events",
        "darmark://contracts/markdown",
        "darmark://plugins/catalog",
        "darmark://plugins/{id}/manifest",
    ]
}

// ─── AC-MCP-1/2/3: инвентарь и схемы ───────────────────────────────

#[test]
fn initialize_reports_protocol_version_and_capabilities() {
    let (server, _records) = server();

    let result = result_of(&server.handle(request("initialize", None)));
    assert_eq!(
        result["protocolVersion"],
        mcp_core::server::MCP_PROTOCOL_VERSION
    );
    assert!(result["capabilities"]["tools"].is_object(), "{result}");
    assert!(result["capabilities"]["resources"].is_object(), "{result}");
    assert_eq!(result["serverInfo"]["name"], "darmark", "{result}");
}

#[test]
fn tools_list_declares_inventory_with_usable_schemas() {
    let (server, _records) = server();

    let result = result_of(&server.handle(request("tools/list", None)));
    let tools = result["tools"].as_array().expect("tools — массив");
    let names: Vec<&str> = tools
        .iter()
        .map(|tool| tool["name"].as_str().expect("name — строка"))
        .collect();
    assert_eq!(names, expected_tools(), "инвентарь tools");

    for tool in tools {
        let name = tool["name"].as_str().expect("name");
        let description = tool["description"].as_str().unwrap_or_default();
        assert!(
            !description.is_empty(),
            "{name}: пустое описание (AC-MCP-3)"
        );
        assert_eq!(
            tool["inputSchema"]["type"], "object",
            "{name}: схема входа — JSON Schema object (AC-MCP-3)"
        );
        assert!(
            tool["inputSchema"]["properties"].is_object(),
            "{name}: в схеме нет properties"
        );
    }
}

#[test]
fn resources_list_declares_inventory_with_descriptions() {
    let (server, _records) = server();

    let result = result_of(&server.handle(request("resources/list", None)));
    let resources = result["resources"].as_array().expect("resources — массив");
    let uris: Vec<&str> = resources
        .iter()
        .map(|resource| resource["uri"].as_str().expect("uri — строка"))
        .collect();
    assert_eq!(uris, expected_resources(), "инвентарь resources (AC-MCP-2)");

    for resource in resources {
        let uri = resource["uri"].as_str().expect("uri");
        assert!(
            !resource["name"].as_str().unwrap_or_default().is_empty(),
            "{uri}: нет name (спека MCP требует Resource.name)"
        );
        assert!(
            !resource["description"]
                .as_str()
                .unwrap_or_default()
                .is_empty(),
            "{uri}: нет описания (AC-MCP-3)"
        );
    }
}

#[test]
fn static_contract_resource_reads_through_server() {
    let (server, _records) = server();

    let result = result_of(&server.handle(request(
        "resources/read",
        Some(json!({ "uri": "darmark://contracts/plugin-manifest" })),
    )));
    let text = result["contents"][0]["text"]
        .as_str()
        .expect("contents[0].text — строка");
    let contract: Value = serde_json::from_str(text).expect("контракт — JSON");
    assert!(
        contract["fields"]["id"].is_string(),
        "контракт манифеста перечисляет поля: {contract}"
    );
    assert_eq!(
        result["contents"][0]["uri"],
        "darmark://contracts/plugin-manifest"
    );
}

#[test]
fn unknown_tool_is_protocol_error_not_tool_result() {
    let (server, _records) = server();

    let response = server.handle(request(
        "tools/call",
        Some(json!({ "name": "ghost", "arguments": {} })),
    ));
    let error = response.error.expect("неизвестный tool — ошибка протокола");
    assert_eq!(error.code, mcp_core::protocol::INVALID_PARAMS);
    assert!(response.result.is_none(), "нет result при ошибке протокола");
}

// ─── AC-MCP-6: схема == поведение, результат == прямой вызов ядра ──

#[test]
fn markdown_render_matches_md_core_directly() {
    let (server, records) = server();
    let text = "# Заголовок\n\n- пункт\n\n`код`\n";

    let payload = tool_payload(&call_tool(
        &server,
        "markdown_render",
        json!({ "text": text }),
    ));
    assert_eq!(payload["html"], json!(md_core::to_html(text)));

    let mapped = tool_payload(&call_tool(
        &server,
        "markdown_render",
        json!({ "text": text, "mapped": true }),
    ));
    assert_eq!(mapped["html"], json!(md_core::to_html_mapped(text)));
    assert_ne!(
        mapped["html"], payload["html"],
        "флаг mapped обязан менять рендер (иначе схема лжёт о поведении)"
    );

    // Журнал фиксирует вызов без содержимого документа (R2).
    let log = records_of(&records);
    assert_eq!(log.len(), 2, "каждый вызов — запись: {log:?}");
    assert!(log[0].contains("markdown_render"), "{log:?}");
    assert!(!log[0].contains("Заголовок"), "нет содержимого: {log:?}");
}

#[test]
fn markdown_render_requires_text_per_schema() {
    let (server, _records) = server();

    let envelope = call_tool(&server, "markdown_render", json!({}));
    assert!(is_error(&envelope), "без text — ошибка вызова: {envelope}");
    let message = envelope["content"][0]["text"].as_str().unwrap_or_default();
    assert!(
        message.contains("text"),
        "сообщение называет параметр: {message}"
    );
}

#[cfg(windows)]
#[test]
fn plugin_validate_matches_static_validation_and_hides_full_path() {
    let plugin = temp_plugin_dir(
        "darmark-mcp-contract-ok",
        VALID_MANIFEST,
        "function on_activate(ctx) end\n",
    );
    let (server, records) = server();

    let payload = tool_payload(&call_tool(
        &server,
        "plugin_validate",
        json!({ "path": plugin.path().to_string_lossy() }),
    ));

    // Прямой вызов той же проверки ядра (AC-MCP-6: адаптер не дублирует логику).
    let direct = darmark_lib::ai::plugin_harness::validate_plugin_static(plugin.path());
    assert!(direct.ok, "фикстура обязана быть валидной: {direct:?}");
    assert_eq!(payload["ok"], direct.ok);
    assert_eq!(payload["errors"], json!(direct.errors));

    // Приватность (AC-MCP-13): ни в результате, ни в журнале нет полного пути.
    let full_path = plugin.path().to_string_lossy().into_owned();
    assert!(
        !payload.to_string().contains(&full_path),
        "результат без полного пути: {payload}"
    );
    let log = records_of(&records);
    assert_eq!(log.len(), 1, "{log:?}");
    assert!(log[0].contains("plugin_validate"), "{log:?}");
    assert!(
        !log[0].contains(&full_path),
        "журнал без полного пути: {log:?}"
    );
    assert!(!log[0].contains(":\\"), "журнал без диска: {log:?}");
    assert!(
        log[0].contains(&plugin_dir_name(&full_path)),
        "журнал несёт имя каталога: {log:?}"
    );
}

/// Паритет адаптера с ядром на **непустой** ветке `errors` (AC-MCP-6).
///
/// Валидная фикстура даёт пустой `errors`, поэтому совпадение двух пустых массивов не
/// доказывает, что адаптер не теряет/не переписывает ошибки (`sanitize_validation` как раз
/// правит эти строки). Здесь манифест заведомо невалиден, а его текст ошибки **не содержит**
/// полного пути (`ManifestError` пути не несёт) — значит паритет с прямым ядром обязан
/// сохраняться и на непустом `errors`.
#[cfg(windows)]
#[test]
fn plugin_validate_reports_nonempty_errors_matching_static_validation() {
    let plugin = temp_plugin_dir(
        "darmark-mcp-contract-bad",
        INVALID_MANIFEST,
        "function on_activate(ctx) end\n",
    );
    let (server, records) = server();

    let payload = tool_payload(&call_tool(
        &server,
        "plugin_validate",
        json!({ "path": plugin.path().to_string_lossy() }),
    ));

    // Прямой вызов той же проверки ядра (AC-MCP-6: адаптер не дублирует логику).
    let direct = darmark_lib::ai::plugin_harness::validate_plugin_static(plugin.path());
    assert!(!direct.ok, "фикстура обязана быть невалидной: {direct:?}");
    assert!(!direct.errors.is_empty(), "ядро вернуло ошибку: {direct:?}");
    assert_eq!(payload["ok"], direct.ok);
    assert_eq!(payload["errors"], json!(direct.errors));
    assert!(
        !payload["errors"]
            .as_array()
            .expect("errors — массив")
            .is_empty(),
        "адаптер не теряет errors: {payload}"
    );

    // Приватность (AC-MCP-13): ни в результате, ни в журнале нет полного пути.
    let full_path = plugin.path().to_string_lossy().into_owned();
    assert!(
        !payload.to_string().contains(&full_path),
        "результат без полного пути: {payload}"
    );
    let log = records_of(&records);
    assert_eq!(log.len(), 1, "{log:?}");
    assert!(log[0].contains("plugin_validate"), "{log:?}");
    assert!(
        !log[0].contains(&full_path),
        "журнал без полного пути: {log:?}"
    );
    assert!(!log[0].contains(":\\"), "журнал без диска: {log:?}");
    assert!(
        log[0].contains(&plugin_dir_name(&full_path)),
        "журнал несёт имя каталога: {log:?}"
    );
}

#[cfg(windows)]
#[test]
fn plugin_validate_rejects_path_outside_whitelist() {
    let (server, _records) = server();

    // Белый список (AC-MCP-12) применяется и через MCP: `C:\Windows` — вне plugins/temp.
    let envelope = call_tool(&server, "plugin_validate", json!({ "path": r"C:\Windows" }));
    assert!(is_error(&envelope), "путь вне белого списка: {envelope}");
    let message = envelope["content"][0]["text"].as_str().unwrap_or_default();
    assert!(
        !message.contains(r"C:\Windows"),
        "в сообщении нет проверяемого пути: {message}"
    );
}

#[cfg(windows)]
#[test]
fn plugin_validate_requires_path_per_schema() {
    let (server, _records) = server();

    let envelope = call_tool(&server, "plugin_validate", json!({}));
    assert!(is_error(&envelope), "без path — ошибка вызова: {envelope}");
}

#[cfg(windows)]
#[test]
fn plugin_test_requires_path_per_schema() {
    let (server, _records) = server();

    // Песочница здесь не запускается: вызов отсекается на схеме (до канонизации пути),
    // поэтому тест не зависит от наличия сайдкаров рядом с тестовым бинарником.
    let envelope = call_tool(&server, "plugin_test", json!({}));
    assert!(is_error(&envelope), "без path — ошибка вызова: {envelope}");
}

#[test]
fn catalog_tool_and_resource_match_plugin_catalog_directly() {
    let repo = examples_plugins_repo();
    // Каталог читается из `DARMARK_PLUGIN_REPO` (единственный источник и у tool, и у ресурса).
    std::env::set_var("DARMARK_PLUGIN_REPO", &repo);

    let (server, records) = server();

    let direct = plugin_catalog::Catalog::load(&repo).expect("эталонный каталог читается");
    let expected: Vec<(String, String)> = direct
        .list()
        .iter()
        .map(|entry| (entry.id.clone(), entry.version.clone()))
        .collect();
    assert!(!expected.is_empty(), "каталог пуст — фикстура не найдена");

    // tool: те же записи, что и при прямом чтении каталога (AC-MCP-6).
    let payload = tool_payload(&call_tool(&server, "plugin_catalog_list", json!({})));
    let via_tool: Vec<(String, String)> = payload["plugins"]
        .as_array()
        .expect("plugins — массив")
        .iter()
        .map(|item| {
            (
                item["id"].as_str().expect("id").to_string(),
                item["version"].as_str().expect("version").to_string(),
            )
        })
        .collect();
    assert_eq!(via_tool, expected);

    // R2-симметрия: tool, как и ресурс `darmark://plugins/catalog`, не отдаёт `path`
    // (относительный путь агенту не нужен — «пути → только имена»).
    assert!(
        payload["plugins"]
            .as_array()
            .expect("plugins — массив")
            .iter()
            .all(|item| item.get("path").is_none()),
        "tool не должен отдавать path: {payload}"
    );

    // resource: тот же каталог через `resources/read`.
    let read = result_of(&server.handle(request(
        "resources/read",
        Some(json!({ "uri": "darmark://plugins/catalog" })),
    )));
    let catalog: Value =
        serde_json::from_str(read["contents"][0]["text"].as_str().expect("text")).expect("JSON");
    let via_resource: Vec<&str> = catalog["plugins"]
        .as_array()
        .expect("plugins — массив")
        .iter()
        .map(|item| item["id"].as_str().expect("id"))
        .collect();
    let expected_ids: Vec<&str> = expected.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(via_resource, expected_ids);

    // Шаблонный ресурс читает манифест конкретного плагина.
    let first = &expected[0].0;
    let manifest = result_of(&server.handle(request(
        "resources/read",
        Some(json!({ "uri": format!("darmark://plugins/{first}/manifest") })),
    )));
    let manifest: Value =
        serde_json::from_str(manifest["contents"][0]["text"].as_str().expect("text"))
            .expect("JSON");
    assert_eq!(
        manifest["id"],
        first.as_str(),
        "манифест из шаблонного ресурса"
    );

    // Журнал вызова tool — без полного пути (R2).
    let log = records_of(&records);
    assert_eq!(log.len(), 1, "{log:?}");
    assert!(!log[0].contains(":\\"), "журнал без полного пути: {log:?}");
}

// ─── Фикстуры ──────────────────────────────────────────────────────
// (фикстуры валидации плагина нужны только Windows-тестам: там живёт белый список путей)

/// Валидный манифест плагина (совпадает по форме с `examples/plugins`).
#[cfg(windows)]
const VALID_MANIFEST: &str = r#"{
  "id": "mcp-contract",
  "name": "MCP contract",
  "version": "1.0.0",
  "api_version": 1,
  "entry": "main.lua",
  "permissions": ["document:read"]
}"#;

/// Невалидный манифест: пустой `id` (`ManifestError::EmptyField` не несёт пути).
#[cfg(windows)]
const INVALID_MANIFEST: &str = r#"{
  "id": "",
  "name": "x",
  "version": "1.0.0",
  "api_version": 1,
  "entry": "main.lua"
}"#;

/// Каталог плагина в `temp` — он в белом списке `plugin_harness` (AC-MCP-12).
#[cfg(windows)]
fn temp_plugin_dir(prefix: &str, manifest: &str, source: &str) -> tempfile::TempDir {
    let dir = tempfile::Builder::new()
        .prefix(prefix)
        .tempdir()
        .expect("временный каталог");
    std::fs::write(dir.path().join("plugin.json"), manifest).expect("plugin.json");
    std::fs::write(dir.path().join("main.lua"), source).expect("main.lua");
    dir
}

/// Последний компонент пути — то, что журнал обязан показать вместо полного пути.
#[cfg(windows)]
fn plugin_dir_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Репозиторный каталог-источник плагинов (`examples/plugins`).
fn examples_plugins_repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .join("examples")
        .join("plugins")
}
