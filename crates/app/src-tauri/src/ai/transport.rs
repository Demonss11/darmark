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
#[derive(Clone)]
pub struct ChatRequest {
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub messages: Vec<AiMessage>,
    pub max_tokens: u32,
    pub temperature: f32,
}

/// Ручной `Debug`: секрет (`api_key`) маскируется и не попадает в `{:?}`/логи (H4).
impl std::fmt::Debug for ChatRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatRequest")
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("api_key", &"<redacted>")
            .field("messages", &self.messages)
            .field("max_tokens", &self.max_tokens)
            .field("temperature", &self.temperature)
            .finish()
    }
}

/// Лёгкая проверка соединения (`GET /models`); содержимое не отправляется.
#[derive(Clone)]
pub struct TestRequest {
    pub base_url: String,
    pub api_key: String,
}

/// Ручной `Debug`: секрет (`api_key`) маскируется и не попадает в `{:?}`/логи (H4).
impl std::fmt::Debug for TestRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestRequest")
            .field("base_url", &self.base_url)
            .field("api_key", &"<redacted>")
            .finish()
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_request_debug_masks_secret() {
        let request = ChatRequest {
            base_url: "https://example.test/v1".to_string(),
            model: "m".to_string(),
            api_key: "sk-secret-1234".to_string(),
            messages: vec![AiMessage {
                role: "user".to_string(),
                content: "hi".to_string(),
            }],
            max_tokens: 16,
            temperature: 0.0,
        };
        let rendered = format!("{request:?}");
        assert!(!rendered.contains("sk-secret"), "секрет скрыт: {rendered}");
        assert!(
            rendered.contains("<redacted>"),
            "секрет маскируется: {rendered}"
        );
    }

    #[test]
    fn test_request_debug_masks_secret() {
        let request = TestRequest {
            base_url: "https://example.test/v1".to_string(),
            api_key: "sk-secret-1234".to_string(),
        };
        let rendered = format!("{request:?}");
        assert!(!rendered.contains("sk-secret"), "секрет скрыт: {rendered}");
        assert!(
            rendered.contains("<redacted>"),
            "секрет маскируется: {rendered}"
        );
    }
}
