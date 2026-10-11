//! Шов `AiBackend`/`AiTransport` (ADR-0034 §2.2): точка «вызов провайдера + передача текста».
//!
//! В MVP — единственная реализация [`crate::ai::backend::reqwest::ReqwestBackend`] в процессе
//! GUI. Будущий сайдкар `darmark-ai.exe` станет drop-in заменой реализации трейта без правок
//! UI/IPC: `ai_chat` вызывает трейт, а не `reqwest` напрямую.

use std::future::Future;
use std::pin::Pin;

use crate::ai::types::{AiMessage, AiUsage};
use crate::error::CommandError;

/// Завернутый `Send`-фьючер с лайфтаймом — объектобезопасный async-метод трейта без `async-trait`.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Запрос к провайдеру (после гейта: включён, ключ есть, согласие выдано).
#[derive(Clone, Debug)]
pub struct ChatRequest {
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub messages: Vec<AiMessage>,
    pub max_tokens: u32,
    pub temperature: f32,
}

/// Лёгкая проверка соединения (`GET /models`); содержимое не отправляется.
#[derive(Clone, Debug)]
pub struct TestRequest {
    pub base_url: String,
    pub api_key: String,
}

/// Итог чата: склеенный текст ответа и (если провайдер прислал) `usage`.
#[derive(Clone, Debug, Default)]
pub struct ChatOutcome {
    pub content: String,
    pub usage: Option<AiUsage>,
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
