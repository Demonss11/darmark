//! Тесты модуля `mcp`: конверт события `log-entry` и разрешение каталога плагинов.
//!
//! `AppHandle` без Tauri-runtime не создать, поэтому прод-сток параметризован трейтом
//! [`LogEmitter`](super::LogEmitter): тест подставляет собирающий mock и проверяет ровно
//! тот конверт `{level, category, message}`, что уходит в продакшене (TZ-MCP R2/R4).
//! [`catalog_repo`](super::catalog_repo) тоже не зависит от Tauri: база каталога — аргумент.

use std::sync::{Arc, Mutex};

use serde_json::Value;

use super::catalog_repo;
use super::{app_log_sink, LogEmitter};





#[derive(Clone, Default)]
struct MockEmitter {
    entries: Arc<Mutex<Vec<(String, Value)>>>,
}

impl LogEmitter for MockEmitter {
    fn emit(&self, event: &str, payload: Value) -> Result<(), String> {
        self.entries
            .lock()
            .expect("журнал теста не отравлен")
            .push((event.to_string(), payload));
        Ok(())
    }
}

#[test]
fn mcp_log_entry_has_valid_level_category_message() {
    let emitter = MockEmitter::default();
    let sink = app_log_sink(emitter.clone());

    sink("MCP markdown_render: 12 символов — ok (1 мс)");

    let entries = emitter.entries.lock().expect("журнал не отравлен");
    assert_eq!(
        entries.len(),
        1,
        "каждый вызов стока — одна запись: {entries:?}"
    );
    let (event, payload) = &entries[0];
    assert_eq!(event, "log-entry", "имя события журнала");
    assert!(payload["level"].is_string(), "level — строка: {payload}");
    assert_eq!(
        payload["level"], "info",
        "MCP-записи идут уровнем info: {payload}"
    );
    assert!(
        payload["category"].is_string(),
        "category — строка: {payload}"
    );
    assert_eq!(
        payload["category"], "system",
        "категория MCP-записей: {payload}"
    );
    assert!(
        payload["message"].is_string(),
        "message — строка: {payload}"
    );
    assert_eq!(
        payload["message"], "MCP markdown_render: 12 символов — ok (1 мс)",
        "сообщение дословно из стока: {payload}"
    );
}


#[test]
fn catalog_repo_returns_app_data_dir_plugins() {
    let app_dir = tempfile::tempdir().unwrap();
    let resolved = catalog_repo(app_dir.path());
    assert_eq!(resolved, app_dir.path().join("plugins"));
}
