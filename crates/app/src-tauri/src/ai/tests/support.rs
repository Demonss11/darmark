//! Двойники для тестов команд AI: эмиттер событий, MCP-сервер, транспорт, кэш возможностей и
//! исполнитель фьючеров.
//!
//! `AppHandle` без Tauri-runtime не создать, а `#[tokio::test]` недоступен (в графе нет фичи
//! `macros`), поэтому команда проверяется через шов [`AiEmitter`](crate::ai::commands::AiEmitter)
//! и двойники, завершающиеся за один опрос. Видимость — `pub(super)`: за пределы дерева тестов
//! `ai` двойники не выходят.

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
use crate::ai::tool_loop::{ToolsCapability, MAX_TOOL_RESULT_CHARS};
use crate::ai::transport::{AiTransport, BoxFuture, ChatOutcome, ChatRequest, TestRequest};
use crate::ai::types::{AiSettings, AiUsage};
use crate::error::{CommandError, ErrorCode};




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
    
    pub(super) fn entries(&self) -> Vec<(String, Value)> {
        self.entries
            .lock()
            .expect("события теста не отравлены")
            .clone()
    }

    
    pub(super) fn payload(&self, event: &str) -> Value {
        self.entries()
            .into_iter()
            .find(|(name, _)| name == event)
            .map(|(_, payload)| payload)
            .unwrap_or_else(|| panic!("событие {event} не эмитилось"))
    }
}


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


pub(super) fn empty_server() -> McpServer {
    McpServer::new(Registry::new())
}


struct FailingTool;

impl ToolHandler for FailingTool {
    fn name(&self) -> &str {
        "fake_tool"
    }

    fn description(&self) -> &str {
        "падающий тестовый инструмент"
    }

    fn input_schema(&self) -> Value {
        json!({"type": "object", "properties": {}})
    }

    fn call(&self, _params: Value) -> Result<Value, ToolError> {
        Err(ToolError::internal("сбой инструмента"))
    }
}


pub(super) fn failing_server() -> McpServer {
    let mut registry = Registry::new();
    registry.register_tool(ToolDefinition {
        name: "fake_tool".to_string(),
        description: "падающий тестовый инструмент".to_string(),
        input_schema: json!({"type": "object", "properties": {}}),
        handler: Arc::new(FailingTool),
    });
    McpServer::new(registry)
}



struct LongTextTool;

impl ToolHandler for LongTextTool {
    fn name(&self) -> &str {
        "long_tool"
    }

    fn description(&self) -> &str {
        "длинный результат"
    }

    fn input_schema(&self) -> Value {
        json!({"type": "object", "properties": {}})
    }

    fn call(&self, _params: Value) -> Result<Value, ToolError> {
        Ok(Value::String("я".repeat(MAX_TOOL_RESULT_CHARS + 100)))
    }
}


pub(super) fn long_text_server() -> McpServer {
    let mut registry = Registry::new();
    registry.register_tool(ToolDefinition {
        name: "long_tool".to_string(),
        description: "длинный результат".to_string(),
        input_schema: json!({"type": "object", "properties": {}}),
        handler: Arc::new(LongTextTool),
    });
    McpServer::new(registry)
}



pub(super) fn tools_rejected() -> CommandError {
    CommandError::provider(400, ErrorCode::AiBadRequest, "tools is not supported")
}



pub(super) fn context_length_exceeded() -> CommandError {
    CommandError::provider(
        400,
        ErrorCode::AiContextLength,
        "This model's maximum context length is 8192 tokens",
    )
}




pub(super) struct ScriptedTransport {
    script: Mutex<VecDeque<Result<ChatOutcome, CommandError>>>,
    requests: Mutex<Vec<ChatRequest>>,
    fallback: Result<ChatOutcome, CommandError>,
}

impl ScriptedTransport {
    
    pub(super) fn new(
        script: Vec<Result<ChatOutcome, CommandError>>,
        fallback: Result<ChatOutcome, CommandError>,
    ) -> Self {
        Self {
            script: Mutex::new(script.into()),
            requests: Mutex::new(Vec::new()),
            fallback,
        }
    }

    
    pub(super) fn ok(script: Vec<ChatOutcome>, fallback: ChatOutcome) -> Self {
        Self::new(script.into_iter().map(Ok).collect(), Ok(fallback))
    }

    
    pub(super) fn requests(&self) -> Vec<ChatRequest> {
        self.requests.lock().expect("запросы не отравлены").clone()
    }

    
    pub(super) fn request_count(&self) -> usize {
        self.requests.lock().expect("запросы не отравлены").len()
    }
}



pub(super) fn clone_error(err: &CommandError) -> CommandError {
    match err.http_status {
        Some(status) => CommandError::provider(status, err.code, err.message.clone()),
        None => CommandError::new(err.code, err.message.clone()),
    }
}



fn clone_step(step: &Result<ChatOutcome, CommandError>) -> Result<ChatOutcome, CommandError> {
    match step {
        Ok(outcome) => Ok(outcome.clone()),
        Err(err) => Err(clone_error(err)),
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
            let step = self
                .script
                .lock()
                .expect("сценарий не отравлен")
                .pop_front()
                .map(|step| clone_step(&step))
                .unwrap_or_else(|| clone_step(&self.fallback));
            match step {
                
                
                Ok(outcome) => {
                    on_delta(outcome.content.clone());
                    Ok(outcome)
                }
                Err(err) => Err(err),
            }
        })
    }

    fn test_connection(
        &self,
        _request: TestRequest,
    ) -> BoxFuture<'_, Result<Option<String>, CommandError>> {
        Box::pin(async { Ok(None) })
    }
}



pub(super) struct MockCapability {
    unsupported: Vec<(String, String)>,
    marks: Mutex<Vec<(String, String)>>,
}

impl MockCapability {
    
    pub(super) fn empty() -> Self {
        Self {
            unsupported: Vec::new(),
            marks: Mutex::new(Vec::new()),
        }
    }

    
    pub(super) fn with_unsupported(base_url: &str, model: &str) -> Self {
        Self {
            unsupported: vec![(base_url.to_string(), model.to_string())],
            marks: Mutex::new(Vec::new()),
        }
    }

    
    pub(super) fn marks(&self) -> Vec<(String, String)> {
        self.marks.lock().expect("вердикты не отравлены").clone()
    }
}

impl ToolsCapability for MockCapability {
    fn tools_supported(&self, base_url: &str, model: &str) -> bool {
        !self
            .unsupported
            .iter()
            .any(|(url, name)| url == base_url && name == model)
    }

    fn mark_tools_unsupported(&self, base_url: &str, model: &str) {
        self.marks
            .lock()
            .expect("вердикты не отравлены")
            .push((base_url.to_string(), model.to_string()));
    }
}


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





pub(super) fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut context) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("тестовый фьючер обязан завершиться за один опрос"),
    }
}
