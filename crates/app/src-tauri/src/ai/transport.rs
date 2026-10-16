//! Шов `AiBackend`/`AiTransport`: точка «вызов провайдера + передача текста».
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


pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;





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
    
    pub tools: Option<serde_json::Value>,
}



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


#[derive(Clone)]
pub struct TestRequest {
    pub base_url: String,
    pub api_key: String,
    pub headers: BTreeMap<String, String>,
    pub timeout: Option<Duration>,
}


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


#[derive(Clone, Debug, Default)]
pub struct ChatOutcome {
    pub content: String,
    pub usage: Option<AiUsage>,
    
    
    pub finish_reason: Option<String>,
    
    pub tool_calls: Vec<ToolCall>,
}






pub trait AiTransport: Send + Sync + 'static {
    
    fn chat<'a>(
        &'a self,
        request: ChatRequest,
        on_delta: &'a mut (dyn FnMut(String) + Send),
    ) -> BoxFuture<'a, Result<ChatOutcome, CommandError>>;

    
    fn test_connection(
        &self,
        request: TestRequest,
    ) -> BoxFuture<'_, Result<Option<String>, CommandError>>;
}
