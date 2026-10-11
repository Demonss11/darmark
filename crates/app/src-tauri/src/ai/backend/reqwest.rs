//! `reqwest`-бэкенд шва `AiTransport` (ADR-0033): OpenAI-совместимый API + SSE-стриминг.
//!
//! HTTP — `POST {base_url}/chat/completions` с Bearer-ключом и `stream: true`; ответ читается
//! как SSE (`bytes_stream`) и разбирается [`SseParser`](crate::ai::sse::SseParser). TLS —
//! системный SChannel (`native-tls`, без OpenSSL). Таймаут — 30 с (TZ §10.1).

use std::time::Duration;

use futures_util::StreamExt;

use crate::ai::errors::map_status;
use crate::ai::sse::{SseEvent, SseParser};
use crate::ai::transport::{AiTransport, BoxFuture, ChatOutcome, ChatRequest, TestRequest};
use crate::error::{CommandError, ErrorCode};

/// Таймаут запроса к провайдеру (TZ §10.1, ADR-0033 §2.3 п.9).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// Число попыток для транзиентной 5xx (первая + один ретрай, ADR-0032 §2.5 п.13).
const MAX_ATTEMPTS: u32 = 2;
/// Пауза перед ретраем на 5xx.
const RETRY_BACKOFF: Duration = Duration::from_secs(1);

/// HTTP-клиент AI на `reqwest` + SChannel. `reqwest::Client` строится лениво (ADR-0034 §2.3).
pub struct ReqwestBackend {
    client: std::sync::OnceLock<reqwest::Client>,
}

impl ReqwestBackend {
    pub fn new() -> Self {
        Self {
            client: std::sync::OnceLock::new(),
        }
    }

    /// Возвращает общий клиент, создавая его при первом обращении (ленивая инициализация).
    fn client(&self) -> Result<&reqwest::Client, CommandError> {
        if let Some(client) = self.client.get() {
            return Ok(client);
        }
        let built = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|e| {
                CommandError::new(ErrorCode::AiOffline, format!("не создать HTTP-клиент: {e}"))
            })?;
        Ok(self.client.get_or_init(|| built))
    }
}

impl Default for ReqwestBackend {
    fn default() -> Self {
        Self::new()
    }
}

/// Склеивает базовую точку и путь, не допуская двойного слэша.
fn endpoint(base_url: &str, path: &str) -> String {
    format!("{}/{}", base_url.trim_end_matches('/'), path)
}

impl AiTransport for ReqwestBackend {
    fn chat<'a>(
        &'a self,
        request: ChatRequest,
        on_delta: &'a mut (dyn FnMut(String) + Send),
    ) -> BoxFuture<'a, Result<ChatOutcome, CommandError>> {
        Box::pin(async move {
            let client = self.client()?;
            let url = endpoint(&request.base_url, "chat/completions");
            let body = serde_json::json!({
                "model": request.model,
                "messages": request.messages,
                "max_tokens": request.max_tokens,
                "temperature": request.temperature,
                "stream": true,
            });
            let auth = format!("Bearer {}", request.api_key);

            // Транзиентная 5xx: одна повторная попытка с бэкоффом; сеть/таймаут — сразу офлайн.
            let mut attempt = 0;
            let response = loop {
                attempt += 1;
                let sent = client
                    .post(url.as_str())
                    .header(reqwest::header::AUTHORIZATION, &auth)
                    .header(reqwest::header::ACCEPT, "text/event-stream")
                    .json(&body)
                    .send()
                    .await;
                match sent {
                    Ok(response)
                        if response.status().is_server_error() && attempt < MAX_ATTEMPTS =>
                    {
                        std::thread::sleep(RETRY_BACKOFF);
                    }
                    Ok(response) => break response,
                    Err(e) => {
                        return Err(CommandError::new(
                            ErrorCode::AiOffline,
                            format!("нет соединения с провайдером: {e}"),
                        ));
                    }
                }
            };

            let status = response.status();
            if !status.is_success() {
                let retry_after = response
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_string);
                let error_body = response.text().await.unwrap_or_default();
                return Err(map_status(
                    status.as_u16(),
                    &error_body,
                    retry_after.as_deref(),
                ));
            }

            let mut parser = SseParser::new();
            let mut content = String::new();
            let mut usage = None;
            let mut stream = response.bytes_stream();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|e| {
                    CommandError::new(ErrorCode::AiOffline, format!("обрыв потока ответа: {e}"))
                })?;
                for event in parser.push(&chunk) {
                    match event {
                        SseEvent::Delta(delta) => {
                            content.push_str(&delta);
                            on_delta(delta);
                        }
                        SseEvent::Usage(value) => usage = Some(value),
                        SseEvent::Done => {}
                    }
                }
            }

            Ok(ChatOutcome { content, usage })
        })
    }

    fn test_connection(
        &self,
        request: TestRequest,
    ) -> BoxFuture<'_, Result<Option<String>, CommandError>> {
        Box::pin(async move {
            let client = self.client()?;
            let url = endpoint(&request.base_url, "models");
            let response = client
                .get(url.as_str())
                .header(
                    reqwest::header::AUTHORIZATION,
                    format!("Bearer {}", request.api_key),
                )
                .send()
                .await
                .map_err(|e| {
                    CommandError::new(
                        ErrorCode::AiOffline,
                        format!("нет соединения с провайдером: {e}"),
                    )
                })?;
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            if !status.is_success() {
                return Err(map_status(status.as_u16(), &body, None));
            }
            // Информативно: имя первой модели из `{ data: [{ id }] }`, если ответ распознан.
            let model = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|value| {
                    value
                        .get("data")?
                        .get(0usize)?
                        .get("id")?
                        .as_str()
                        .map(str::to_string)
                });
            Ok(model)
        })
    }
}
