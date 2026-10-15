//! Шов `AiBackend`/`AiTransport` (ADR-0034 §2.2): точка «вызов провайдера + передача текста».
//!
//! В MVP — единственная реализация [`crate::ai::backend::reqwest::ReqwestBackend`] в процессе
//! GUI. Будущий сайдкар `darmark-ai.exe` станет drop-in заменой реализации трейта без правок
//! UI/IPC: `ai_chat` вызывает трейт, а не `reqwest` напрямую.

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use crate::ai::tool_calls::ToolCall;
use crate::ai::types::{AiMessage, AiUsage};
use crate::error::CommandError;

/// Завернутый `Send`-фьючер с лайфтаймом — объектобезопасный async-метод трейта без `async-trait`.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Запрос к провайдеру (после гейта: включён, активная пара и ключ есть, согласие выдано).
///
/// `headers`/`extra_body` — надстройка запроса из `ai.providers[id]` (ADR-0035 §2.4);
/// `timeout` — провайдер-wide ограничение запроса (§2.5).
#[derive(Clone)]
pub struct ChatRequest {
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub messages: Vec<AiMessage>,
    pub max_tokens: u32,
    pub temperature: f32,
    pub headers: BTreeMap<String, String>,
    pub extra_body: serde_json::Map<String, serde_json::Value>,
    pub timeout: Option<Duration>,
    /// Готовый OpenAI-массив `tools` для запроса; `None` — инструменты не передаются.
    pub tools: Option<serde_json::Value>,
}

/// Ручной `Debug`: секрет (`api_key`) маскируется; заголовки/тело не разворачиваются (H4,
/// ADR-0035 §2.6) — печатаются только имена, чтобы надстройка не утекала в логи.
impl std::fmt::Debug for ChatRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatRequest")
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("api_key", &"<redacted>")
            .field("messages", &self.messages)
            .field("max_tokens", &self.max_tokens)
            .field("temperature", &self.temperature)
            .field("headers", &self.headers.keys().collect::<Vec<_>>())
            .field("extra_body", &self.extra_body.keys().collect::<Vec<_>>())
            .field("timeout", &self.timeout)
            .field("tools", &self.tools)
            .finish()
    }
}

/// Лёгкая проверка соединения (`GET /models`); содержимое документа не отправляется.
#[derive(Clone)]
pub struct TestRequest {
    pub base_url: String,
    pub api_key: String,
    pub headers: BTreeMap<String, String>,
    pub timeout: Option<Duration>,
}

/// Ручной `Debug`: секрет (`api_key`) и заголовки не разворачиваются (H4, ADR-0035 §2.6).
impl std::fmt::Debug for TestRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestRequest")
            .field("base_url", &self.base_url)
            .field("api_key", &"<redacted>")
            .field("headers", &self.headers.keys().collect::<Vec<_>>())
            .field("timeout", &self.timeout)
            .finish()
    }
}

/// Итог чата: склеенный текст ответа, (если провайдер прислал) `usage` и причина завершения.
#[derive(Clone, Debug, Default)]
pub struct ChatOutcome {
    pub content: String,
    pub usage: Option<AiUsage>,
    /// Последнее непустое `finish_reason` из стрима (`"stop"`, `"length"` и т.д.);
    /// `None`, если провайдер его не прислал.
    pub finish_reason: Option<String>,
    /// Вызовы инструментов, собранные из SSE-потока через `ToolCallAccumulator`.
    pub tool_calls: Vec<ToolCall>,
}

/// Трейт шва: вызов провайдера и передача текста.
///
/// Дельты стрима отдаются через `on_delta` синхронно по мере поступления — вызывающая сторона
/// (`ai_chat`) превращает их в событие `ai:chunk`. Трейт не знает про Tauri: это держит
/// реализацию тестируемой и позволяет вынести её в child-процесс позже.
pub trait AiTransport: Send + Sync + 'static {
    /// Отправляет чат-запрос; возвращает итоговый текст и `usage`.
    fn chat<'a>(
        &'a self,
        request: ChatRequest,
        on_delta: &'a mut (dyn FnMut(String) + Send),
    ) -> BoxFuture<'a, Result<ChatOutcome, CommandError>>;

    /// Проверяет соединение и (если возможно) возвращает имя модели.
    fn test_connection(
        &self,
        request: TestRequest,
    ) -> BoxFuture<'_, Result<Option<String>, CommandError>>;
}
