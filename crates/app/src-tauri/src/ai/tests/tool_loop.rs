//! Тесты tool-loop [`super::super::tool_loop`]: спецификации MCP, вызов, разметка R7, цикл.
//!
//! Транспорт провайдера и MCP-сервер подменяются фейками: `ScriptedTransport` отдаёт заранее
//! заданную последовательность ответов, `FakeTool` — детерминированный результат вызова.
//! Так проверяется поведение цикла (число запросов, состав `tool`-сообщений, изоляция сбоя),
//! а не реализация сети или реестра.

use std::collections::{BTreeMap, VecDeque};
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

use serde_json::{json, Value};

use mcp_core::handlers::ToolError;
use mcp_core::protocol::{RpcRequest, RPC_VERSION};
use mcp_core::server::McpServer;
use mcp_core::{Registry, ToolDefinition, ToolHandler};

use crate::ai::tool_calls::ToolCall;
use crate::ai::tool_loop::{
    call_mcp_tool, run, tool_message_content, tool_specs_from_tools_list, ToolExecution,
    MAX_TOOL_ITERATIONS, MAX_TOOL_RESULT_CHARS,
};
use crate::ai::transport::{AiTransport, BoxFuture, ChatOutcome, ChatRequest, TestRequest};
use crate::ai::types::{AiMessage, AiUsage};
use crate::error::{CommandError, ErrorCode};

/// Фейковый инструмент MCP: по флагу `fail` отдаёт успех или ошибку исполнения.
struct FakeTool {
    name: String,
    fail: bool,
}

impl ToolHandler for FakeTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        "тестовый инструмент"
    }

    fn input_schema(&self) -> Value {
        json!({"type": "object", "properties": {}})
    }

    fn call(&self, _params: Value) -> Result<Value, ToolError> {
        if self.fail {
            Err(ToolError::internal("сбой инструмента"))
        } else {
            Ok(json!({"ok": true}))
        }
    }
}

/// MCP-сервер с одним [`FakeTool`] — минимальный реестр для проверки спецификаций и вызова.
fn fake_server(name: &str, fail: bool) -> McpServer {
    let mut registry = Registry::new();
    registry.register_tool(ToolDefinition {
        name: name.to_string(),
        description: "тестовый инструмент".to_string(),
        input_schema: json!({"type": "object", "properties": {}}),
        handler: Arc::new(FakeTool {
            name: name.to_string(),
            fail,
        }),
    });
    McpServer::new(registry)
}

/// Инструмент с длинным текстовым результатом — проверка обрезки до [`MAX_TOOL_RESULT_CHARS`].
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

/// MCP-сервер с [`LongTextTool`].
fn long_server() -> McpServer {
    let mut registry = Registry::new();
    registry.register_tool(ToolDefinition {
        name: "long_tool".to_string(),
        description: "длинный результат".to_string(),
        input_schema: json!({"type": "object", "properties": {}}),
        handler: Arc::new(LongTextTool),
    });
    McpServer::new(registry)
}

/// Результат `tools/list` из реестра — тот же JSON-RPC путь, что и у провайдера.
fn tools_list_json(server: &McpServer) -> Value {
    server
        .handle(RpcRequest {
            jsonrpc: RPC_VERSION.to_string(),
            id: Some(json!(1)),
            method: "tools/list".to_string(),
            params: None,
        })
        .expect("tools/list возвращает ответ")
        .result
        .expect("tools/list возвращает result")
}

/// Транспорт провайдера с заданной последовательностью ответов; лишние запросы получают
/// `fallback`. Каждый запрос сохраняется — по нему проверяется число итераций и состав диалога.
struct ScriptedTransport {
    script: Mutex<VecDeque<ChatOutcome>>,
    requests: Mutex<Vec<ChatRequest>>,
    fallback: ChatOutcome,
}

impl ScriptedTransport {
    fn new(script: Vec<ChatOutcome>, fallback: ChatOutcome) -> Self {
        Self {
            script: Mutex::new(script.into()),
            requests: Mutex::new(Vec::new()),
            fallback,
        }
    }

    fn request_count(&self) -> usize {
        self.requests.lock().expect("запросы не отравлены").len()
    }

    fn requests(&self) -> Vec<ChatRequest> {
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

/// Базовый запрос без инструментов: сообщения дополняет `run`.
fn base_request() -> ChatRequest {
    ChatRequest {
        base_url: "https://example.test/v1".to_string(),
        model: "test-model".to_string(),
        api_key: "sk-test".to_string(),
        messages: vec![AiMessage {
            role: "user".to_string(),
            content: "привет".to_string(),
            ..AiMessage::default()
        }],
        max_tokens: 128,
        temperature: 0.0,
        headers: BTreeMap::new(),
        extra_body: serde_json::Map::new(),
        timeout: None,
        tools: None,
    }
}

/// Ответ провайдера с одним вызовом инструмента.
fn outcome_with_tool_call(name: &str) -> ChatOutcome {
    ChatOutcome {
        content: String::new(),
        usage: None,
        finish_reason: Some("tool_calls".to_string()),
        tool_calls: vec![ToolCall {
            id: "call-1".to_string(),
            name: name.to_string(),
            arguments: json!({}),
        }],
    }
}

/// Финальный текстовый ответ провайдера без вызовов.
fn final_outcome(text: &str) -> ChatOutcome {
    ChatOutcome {
        content: text.to_string(),
        usage: Some(AiUsage {
            prompt_tokens: 1,
            completion_tokens: 2,
        }),
        finish_reason: Some("stop".to_string()),
        tool_calls: Vec::new(),
    }
}

/// Минимальный исполнитель для `#[test]`: фейковый транспорт не делает I/O, поэтому фьючер
/// `run` завершается за один опрос. `#[tokio::test]` недоступен (в графе нет фичи `macros`),
/// а тянуть рантайм ради трёх тестов не оправдано.
fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut context) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("тестовый фьючер обязан завершиться за один опрос"),
    }
}

#[test]
fn tool_specs_from_tools_list_maps_mcp_registry() {
    let server = fake_server("fake_tool", false);
    let list = tools_list_json(&server);

    let specs = tool_specs_from_tools_list(&list);

    assert_eq!(specs.len(), 1, "одна спека на инструмент: {specs:?}");
    assert_eq!(specs[0]["function"]["name"], "fake_tool");
    assert_eq!(specs[0]["type"], "function");
}

#[test]
fn tool_message_content_marks_data_not_instructions() {
    let text = tool_message_content("plugin_validate", "ok");

    assert!(
        text.contains("plugin_validate"),
        "имя инструмента видно: {text}"
    );
    assert!(
        text.to_lowercase().contains("не инструкци"),
        "результат размечается как данные, не инструкции: {text}"
    );
}

#[test]
fn tool_loop_executes_tool_and_returns_final_answer() {
    let server = fake_server("fake_tool", false);
    let transport = ScriptedTransport::new(
        vec![
            outcome_with_tool_call("fake_tool"),
            final_outcome("готовый ответ"),
        ],
        final_outcome("готовый ответ"),
    );
    let mut called: Vec<String> = Vec::new();
    let mut results: Vec<(String, bool)> = Vec::new();

    let response = block_on(run(
        &transport,
        &server,
        base_request(),
        &mut |_delta: String| {},
        &mut |call: &ToolCall| called.push(call.name.clone()),
        &mut |exec: &ToolExecution| results.push((exec.call.id.clone(), exec.result.ok)),
    ))
    .expect("tool-loop завершается финальным ответом");

    assert_eq!(response.message.role, "assistant");
    assert_eq!(response.message.content, "готовый ответ");
    assert_eq!(called, vec!["fake_tool".to_string()], "инструмент вызван");
    assert_eq!(
        results,
        vec![("call-1".to_string(), true)],
        "колбэк получает id вызова и успешный результат"
    );
    assert_eq!(
        transport.request_count(),
        2,
        "запрос до вызова инструмента и после"
    );

    let requests = transport.requests();
    let tool_messages: Vec<&AiMessage> = requests[1]
        .messages
        .iter()
        .filter(|message| message.role == "tool")
        .collect();
    assert_eq!(tool_messages.len(), 1, "в диалог добавлен tool-результат");
    assert_eq!(
        tool_messages[0].tool_call_id.as_deref(),
        Some("call-1"),
        "tool-сообщение связано с вызовом по id"
    );
}

#[test]
fn tool_loop_stops_at_iteration_limit() {
    let server = fake_server("fake_tool", false);
    let transport = ScriptedTransport::new(Vec::new(), outcome_with_tool_call("fake_tool"));
    let mut calls = 0usize;

    let response = block_on(run(
        &transport,
        &server,
        base_request(),
        &mut |_delta: String| {},
        &mut |_call: &ToolCall| calls += 1,
        &mut |_exec: &ToolExecution| {},
    ))
    .expect("исчерпание лимита не роняет цикл");

    assert_eq!(
        transport.request_count(),
        MAX_TOOL_ITERATIONS,
        "не более MAX_TOOL_ITERATIONS запросов"
    );
    assert_eq!(
        calls, MAX_TOOL_ITERATIONS,
        "на каждой итерации инструмент вызывается"
    );
    assert_eq!(response.message.role, "assistant");
}

#[test]
fn tool_loop_isolates_tool_failure() {
    let server = fake_server("fake_tool", true);
    let transport = ScriptedTransport::new(
        vec![
            outcome_with_tool_call("fake_tool"),
            final_outcome("ответ после сбоя"),
        ],
        final_outcome("ответ после сбоя"),
    );
    let mut results: Vec<bool> = Vec::new();

    let response = block_on(run(
        &transport,
        &server,
        base_request(),
        &mut |_delta: String| {},
        &mut |_call: &ToolCall| {},
        &mut |exec: &ToolExecution| results.push(exec.result.ok),
    ))
    .expect("сбой инструмента не ломает цикл");

    assert_eq!(
        results,
        vec![false],
        "isError приходит как неуспешный результат, а не ошибка"
    );
    assert_eq!(
        response.message.content, "ответ после сбоя",
        "цикл продолжается до финального ответа"
    );
}

#[test]
fn call_mcp_tool_rejects_unknown_tool() {
    let server = fake_server("fake_tool", false);

    let error = call_mcp_tool(&server, "ghost", json!({}))
        .expect_err("неизвестный инструмент — ошибка протокола");

    assert_eq!(error.code, ErrorCode::AiBadRequest);
}

#[test]
fn call_mcp_tool_truncates_long_result_on_char_boundary() {
    let server = long_server();
    let full = Value::String("я".repeat(MAX_TOOL_RESULT_CHARS + 100)).to_string();

    let result = call_mcp_tool(&server, "long_tool", json!({})).expect("вызов инструмента");

    assert!(result.ok, "успешный вызов: {result:?}");
    assert_eq!(
        result.text.chars().count(),
        MAX_TOOL_RESULT_CHARS,
        "текст обрезан ровно до лимита символов"
    );
    assert!(
        full.starts_with(&result.text),
        "обрезка прошла по границе UTF-8 (многобайтный символ не разорван)"
    );
}
