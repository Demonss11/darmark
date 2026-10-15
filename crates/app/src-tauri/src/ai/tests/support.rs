//! Двойники для тестов команд AI: эмиттер событий, MCP-сервер, транспорт и исполнитель фьючеров.
//!
//! `AppHandle` без Tauri-runtime не создать, а `#[tokio::test]` недоступен (в графе нет фичи
//! `macros`), поэтому команда проверяется через шов [`AiEmitter`](crate::ai::commands::AiEmitter)
//! и двойники, завершающиеся за один опрос. Видимость — `pub(super)`: за пределы дерева тестов
//! `ai` двойники не выходят (ADR-0039 §2.3).

use std::collections::VecDeque;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

use serde_json::{json, Value};

use mcp_core::handlers::ToolError;
use mcp_core::server::McpServer;
use mcp_core::{Registry, ToolDefinition, ToolHandler};

use crate::ai::commands::AiEmitter;
use crate::ai::tool_calls::ToolCall;
use crate::ai::transport::{AiTransport, BoxFuture, ChatOutcome, ChatRequest, TestRequest};
use crate::ai::types::{AiSettings, AiUsage};
use crate::error::CommandError;

/// Mock-эмиттер событий ассистента: копит пары `(event, payload)` в порядке эмиссии.
///
/// Прод-эмиттер — [`AppHandle`](tauri::AppHandle), не собираемый в тесте.
#[derive(Clone, Default)]
pub(super) struct MockEmitter {
    entries: Arc<Mutex<Vec<(String, Value)>>>,
}

impl AiEmitter for MockEmitter {
    fn emit(&self, event: &str, payload: Value) -> Result<(), String> {
        self.entries
            .lock()
            .expect("события теста не отравлены")
            .push((event.to_string(), payload));
        Ok(())
    }
}

impl MockEmitter {
    /// Снимок событий `(event, payload)` в порядке эмиссии.
    pub(super) fn entries(&self) -> Vec<(String, Value)> {
        self.entries
            .lock()
            .expect("события теста не отравлены")
            .clone()
    }

    /// Payload первого события с именем `event`; паника, если событие не эмитилось.
    pub(super) fn payload(&self, event: &str) -> Value {
        self.entries()
            .into_iter()
            .find(|(name, _)| name == event)
            .map(|(_, payload)| payload)
            .unwrap_or_else(|| panic!("событие {event} не эмитилось"))
    }
}

/// Фейковый MCP-инструмент: детерминированный успешный результат, без I/O и таймингов.
struct FakeTool;

impl ToolHandler for FakeTool {
    fn name(&self) -> &str {
        "fake_tool"
    }

    fn description(&self) -> &str {
        "тестовый инструмент"
    }

    fn input_schema(&self) -> Value {
        json!({"type": "object", "properties": {}})
    }

    fn call(&self, _params: Value) -> Result<Value, ToolError> {
        Ok(json!({"ok": true}))
    }
}

/// MCP-сервер с одним [`FakeTool`] — реестр, из которого команда объявляет `tools` провайдеру.
pub(super) fn fake_server() -> McpServer {
    let mut registry = Registry::new();
    registry.register_tool(ToolDefinition {
        name: "fake_tool".to_string(),
        description: "тестовый инструмент".to_string(),
        input_schema: json!({"type": "object", "properties": {}}),
        handler: Arc::new(FakeTool),
    });
    McpServer::new(registry)
}

/// MCP-сервер без инструментов — проверка деградации §3.3 ТЗ (запрос без поля `tools`).
pub(super) fn empty_server() -> McpServer {
    McpServer::new(Registry::new())
}

/// Транспорт провайдера с заданной последовательностью ответов; лишние запросы получают
/// `fallback`. Каждый запрос сохраняется — по нему проверяется объявление `tools`.
pub(super) struct ScriptedTransport {
    script: Mutex<VecDeque<ChatOutcome>>,
    requests: Mutex<Vec<ChatRequest>>,
    fallback: ChatOutcome,
}

impl ScriptedTransport {
    pub(super) fn new(script: Vec<ChatOutcome>, fallback: ChatOutcome) -> Self {
        Self {
            script: Mutex::new(script.into()),
            requests: Mutex::new(Vec::new()),
            fallback,
        }
    }

    /// Снимок запросов, ушедших провайдеру.
    pub(super) fn requests(&self) -> Vec<ChatRequest> {
        self.requests.lock().expect("запросы не отравлены").clone()
    }
}

impl AiTransport for ScriptedTransport {
    fn chat<'a>(
        &'a self,
        request: ChatRequest,
        on_delta: &'a mut (dyn FnMut(String) + Send),
    ) -> BoxFuture<'a, Result<ChatOutcome, CommandError>> {
        Box::pin(async move {
            self.requests
                .lock()
                .expect("запросы не отравлены")
                .push(request);
            let outcome = self
                .script
                .lock()
                .expect("сценарий не отравлен")
                .pop_front()
                .unwrap_or_else(|| self.fallback.clone());
            on_delta(outcome.content.clone());
            Ok(outcome)
        })
    }

    fn test_connection(
        &self,
        _request: TestRequest,
    ) -> BoxFuture<'_, Result<Option<String>, CommandError>> {
        Box::pin(async { Ok(None) })
    }
}

/// Настройки, проходящие гейт чата: включён, ключ у активного провайдера, согласие выдано.
pub(super) fn ready_settings() -> AiSettings {
    let mut settings = AiSettings {
        enabled: true,
        consent_document_send: true,
        ..AiSettings::default()
    };
    settings
        .providers
        .get_mut("routerai")
        .expect("провайдер по умолчанию")
        .settings
        .api_key = Some("sk-test".to_string());
    settings
}

/// Ответ провайдера с одним вызовом инструмента — первый шаг цикла.
pub(super) fn outcome_with_tool_call() -> ChatOutcome {
    ChatOutcome {
        content: String::new(),
        usage: None,
        finish_reason: Some("tool_calls".to_string()),
        tool_calls: vec![ToolCall {
            id: "call-1".to_string(),
            name: "fake_tool".to_string(),
            arguments: json!({"path": "plugins"}),
        }],
    }
}

/// Финальный текстовый ответ провайдера без вызовов — последний шаг цикла.
pub(super) fn final_outcome(text: &str) -> ChatOutcome {
    ChatOutcome {
        content: text.to_string(),
        usage: Some(AiUsage {
            prompt_tokens: 3,
            completion_tokens: 5,
        }),
        finish_reason: Some("stop".to_string()),
        tool_calls: Vec::new(),
    }
}

/// Минимальный исполнитель для `#[test]`: двойники не делают I/O, поэтому фьючер команды
/// завершается за один опрос (`#[tokio::test]` недоступен — в графе нет фичи `macros`).
///
/// `AiState` в тестах свежий, поэтому `enforce_rate_limit` не уходит в `tokio::time::sleep`.
pub(super) fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut context) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("тестовый фьючер обязан завершиться за один опрос"),
    }
}
