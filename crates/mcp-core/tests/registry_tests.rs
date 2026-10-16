use mcp_core::handlers::{ResourceError, ResourceReader, ToolError, ToolHandler};
use mcp_core::protocol::RESOURCE_NOT_FOUND;
use mcp_core::registry::{CallToolError, Registry, ResourceDefinition, ToolDefinition};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

struct DummyTool;

impl ToolHandler for DummyTool {
    fn name(&self) -> &str {
        "dummy"
    }
    fn description(&self) -> &str {
        "Тестовый инструмент"
    }
    fn input_schema(&self) -> Value {
        json!({"type": "object"})
    }
    fn call(&self, _params: Value) -> Result<Value, ToolError> {
        Ok(json!({"ok": true}))
    }
}

#[test]
fn register_and_call_tool() {
    let mut registry = Registry::new();
    registry.register_tool(ToolDefinition {
        name: "dummy".to_string(),
        description: "Тестовый инструмент".to_string(),
        input_schema: json!({"type": "object"}),
        handler: Arc::new(DummyTool),
    });
    let result = registry.call_tool("dummy", json!({})).expect("ok");
    assert_eq!(result["ok"], true);
}

#[test]
fn call_unknown_tool_returns_unknown_tool_error() {
    let registry = Registry::new();
    let err = registry.call_tool("ghost", json!({})).expect_err("ошибка");
    assert!(matches!(err, CallToolError::UnknownTool(name) if name == "ghost"));
}



struct RecordingResource {
    seen: Arc<Mutex<Vec<String>>>,
}

impl ResourceReader for RecordingResource {
    fn read(&self, uri: &str) -> Result<Value, ResourceError> {
        self.seen
            .lock()
            .expect("мьютекс не отравлен")
            .push(uri.to_string());
        Ok(json!({ "uri": uri, "manifest": true }))
    }
}

fn registry_with_manifest_template(seen: Arc<Mutex<Vec<String>>>) -> Registry {
    let mut registry = Registry::new();
    registry.register_resource(ResourceDefinition {
        uri: "darmark://plugins/{id}/manifest".to_string(),
        name: "Манифест плагина".to_string(),
        description: "Манифест конкретного плагина из каталога".to_string(),
        reader: Arc::new(RecordingResource { seen }),
    });
    registry
}

#[test]
fn template_resource_reads_concrete_uri() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let registry = registry_with_manifest_template(Arc::clone(&seen));

    let value = registry
        .read_resource("darmark://plugins/word-count/manifest")
        .expect("шаблонный ресурс достижим по конкретному URI");
    assert_eq!(value["manifest"], true);
    assert_eq!(
        seen.lock().expect("мьютекс не отравлен").as_slice(),
        ["darmark://plugins/word-count/manifest"],
        "читатель получает фактический URI запроса"
    );
}

#[test]
fn template_resource_rejects_mismatched_segment_count() {
    let registry = registry_with_manifest_template(Arc::new(Mutex::new(Vec::new())));
    let err = registry
        .read_resource("darmark://plugins/word-count/manifest/extra")
        .expect_err("лишний сегмент — не совпадение");
    assert_eq!(err.code, RESOURCE_NOT_FOUND);
}

#[test]
fn exact_resource_takes_precedence_over_template() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut registry = registry_with_manifest_template(Arc::clone(&seen));
    
    registry.register_resource(ResourceDefinition {
        uri: "darmark://plugins/catalog".to_string(),
        name: "Каталог плагинов".to_string(),
        description: "Индекс каталога".to_string(),
        reader: Arc::new(StaticResource),
    });

    let value = registry
        .read_resource("darmark://plugins/catalog")
        .expect("точное совпадение");
    assert_eq!(value["ok"], true, "сработал статический ресурс");
    assert!(
        seen.lock().expect("мьютекс не отравлен").is_empty(),
        "шаблонный читатель не вызывался"
    );
}

struct StaticResource;

impl ResourceReader for StaticResource {
    fn read(&self, _uri: &str) -> Result<Value, ResourceError> {
        Ok(json!({ "ok": true }))
    }
}
