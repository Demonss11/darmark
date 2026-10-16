//! Двойники для тестов команд AI: эмиттер событий, MCP-сервер, транспорт, кэш возможностей и
//! исполнитель фьючеров.
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
use crate::ai::tool_loop::{ToolsCapability, MAX_TOOL_RESULT_CHARS};
use crate::ai::transport::{AiTransport, BoxFuture, ChatOutcome, ChatRequest, TestRequest};
use crate::ai::types::{AiSettings, AiUsage};
use crate::error::{CommandError, ErrorCode};

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

/// Фейковый инструмент, всегда падающий: сбой исполнения приходит как `ok = false`.
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

/// MCP-сервер с [`FailingTool`] — изоляция сбоя исполнения в цикле.
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

/// Фейковый инструмент с длинным текстовым результатом — обрезка до
/// [`MAX_TOOL_RESULT_CHARS`].
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

/// MCP-сервер с [`LongTextTool`] — проверка обрезки результата по границе UTF-8.
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

/// Отказ провайдера принимать поле `tools` — HTTP 400 без маркера превышения контекста
/// (ADR-0040 §2.3: текст отказа не разбирается, решает статус).
pub(super) fn tools_rejected() -> CommandError {
    CommandError::provider(400, ErrorCode::AiBadRequest, "tools is not supported")
}

/// Ошибка `map_status` для превышения контекста: HTTP 400 **и** распознанный код — повтор без
/// `tools` по ADR-0040 §2.3 п.14 не выполняется.
pub(super) fn context_length_exceeded() -> CommandError {
    CommandError::provider(
        400,
        ErrorCode::AiContextLength,
        "This model's maximum context length is 8192 tokens",
    )
}

/// Транспорт провайдера с заданной последовательностью шагов; лишние запросы получают
/// `fallback`. Шаг — `Ok`/`Err`: ошибки нужны для проверки деградации (ADR-0040).
/// Каждый запрос сохраняется — по нему проверяется объявление `tools`.
pub(super) struct ScriptedTransport {
    script: Mutex<VecDeque<Result<ChatOutcome, CommandError>>>,
    requests: Mutex<Vec<ChatRequest>>,
    fallback: Result<ChatOutcome, CommandError>,
}

impl ScriptedTransport {
    /// Сценарий из шагов с ошибками; каждый запрос забирает следующий шаг.
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

    /// Сценарий, где провайдер всегда отвечает успехом.
    pub(super) fn ok(script: Vec<ChatOutcome>, fallback: ChatOutcome) -> Self {
        Self::new(script.into_iter().map(Ok).collect(), Ok(fallback))
    }

    /// Снимок запросов, ушедших провайдеру.
    pub(super) fn requests(&self) -> Vec<ChatRequest> {
        self.requests.lock().expect("запросы не отравлены").clone()
    }

    /// Сколько запросов ушло провайдеру (бюджет итераций цикла).
    pub(super) fn request_count(&self) -> usize {
        self.requests.lock().expect("запросы не отравлены").len()
    }
}

/// Копия ошибки для повторного использования в сценарии: [`CommandError`] не клонируется,
/// поэтому статус и код переносятся в свежую ошибку с тем же содержимым.
pub(super) fn clone_error(err: &CommandError) -> CommandError {
    match err.http_status {
        Some(status) => CommandError::provider(status, err.code, err.message.clone()),
        None => CommandError::new(err.code, err.message.clone()),
    }
}

/// Копия шага сценария транспорта: успешный ответ клонируется, ошибка — через
/// [`clone_error`].
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
                // Дельта приходит только от успешного шага: у отказа провайдера текста нет,
                // иначе в чат попадёт пустой пузырь от деградирующего запроса.
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

/// Двойник кэша возможностей инструментов: по умолчанию `tools` разрешены, вызовы
/// `mark_tools_unsupported` записываются для проверки момента вердикта (ADR-0040 §2.5 п.22).
pub(super) struct MockCapability {
    unsupported: Vec<(String, String)>,
    marks: Mutex<Vec<(String, String)>>,
}

impl MockCapability {
    /// Кэш без вердиктов: инструменты разрешены для любой пары `(base_url, model)`.
    pub(super) fn empty() -> Self {
        Self {
            unsupported: Vec::new(),
            marks: Mutex::new(Vec::new()),
        }
    }

    /// Кэш с готовым вердиктом: пара уже помечена провайдером без `tools` (кэш-хит).
    pub(super) fn with_unsupported(base_url: &str, model: &str) -> Self {
        Self {
            unsupported: vec![(base_url.to_string(), model.to_string())],
            marks: Mutex::new(Vec::new()),
        }
    }

    /// Пары, отмеченные как не поддерживающие `tools`, в порядке вызовов.
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
