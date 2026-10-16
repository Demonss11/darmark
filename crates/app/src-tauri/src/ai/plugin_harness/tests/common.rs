//! Общие фикстуры MCP-тестов плагинного harness: реестр приложения с собирающим журналом и
//! валидный манифест.
//!
//! Отдельным модулем, чтобы тесты `plugin_test` и `plugin_read` не разошлись в представлении
//! о том, что считается валидным плагином, и чтобы оба файла не перешагнули порог 400 строк
//!.

use std::sync::{Arc, Mutex};

use mcp_core::registry::Registry;

use crate::mcp::adapters::LogSink;
use crate::mcp::build_registry_with_sink;


pub(super) fn registry_with_journal() -> (Registry, Arc<Mutex<Vec<String>>>) {
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


pub(super) const VALID_MANIFEST: &str = r#"{
  "id": "mcp-plugin-test",
  "name": "MCP plugin test",
  "version": "1.0.0",
  "api_version": 1,
  "entry": "main.lua"
}"#;
