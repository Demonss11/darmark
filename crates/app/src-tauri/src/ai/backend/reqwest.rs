//! `reqwest`-бэкенд шва `AiTransport` (ADR-0033): OpenAI-совместимый API + SSE-стриминг.
//!
//! HTTP — `POST {base_url}/chat/completions` с Bearer-ключом и `stream: true`; ответ читается
//! как SSE (`bytes_stream`) и разбирается парсером `crate::ai::sse`. TLS — системный SChannel
//! (`native-tls`, без OpenSSL). Таймаут — 30 с по умолчанию (ADR-0032); провайдер может
//! переопределить его через `settings.timeout` (ADR-0035 §2.5).

use std::time::Duration;

use futures_util::StreamExt;
use reqwest::header::{HeaderName, HeaderValue};

use crate::ai::errors::map_status;
use crate::ai::sse::{SseEvent, SseParser};
use crate::ai::tool_calls::ToolCallAccumulator;
use crate::ai::transport::{AiTransport, BoxFuture, ChatOutcome, ChatRequest, TestRequest};
use crate::error::{CommandError, ErrorCode};

/// Таймаут запроса к провайдеру по умолчанию (ADR-0032, ADR-0033 §2.3 п.9).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// Число попыток для транзиентной 5xx (первая + один ретрай, ADR-0032 §2.5 п.13).
const MAX_ATTEMPTS: u32 = 2;
/// Пауза перед ретраем на 5xx.
const RETRY_BACKOFF: Duration = Duration::from_secs(1);
/// Лимит накопленного текста ответа: недоверенное тело не должно исчерпать память под `panic=abort`.
pub(in crate::ai) const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
/// Лимит чтения тела ошибки/списка моделей перед разбором (защита от гигантского тела).
const MAX_ERROR_BODY_BYTES: usize = 64 * 1024;

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
            // Редиректы запрещены: тело (документ) не должно уходить на хост, не названный
            // в согласии (H2).
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| {
                CommandError::new(
                    ErrorCode::AiOffline,
                    format!("не создать HTTP-клиент: {e}"),
                )
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
pub(in crate::ai) fn endpoint(base_url: &str, path: &str) -> String {
    format!("{}/{}", base_url.trim_end_matches('/'), path)
}

/// Проверяет, что накопленный ответ вместе с новой дельтой не превышает лимит (H1).
pub(in crate::ai) fn ensure_within_response_limit(
    current: usize,
    incoming: usize,
) -> Result<(), CommandError> {
    if current.saturating_add(incoming) > MAX_RESPONSE_BYTES {
        Err(CommandError::new(
            ErrorCode::AiBadRequest,
            "ответ провайдера превысил допустимый размер",
        ))
    } else {
        Ok(())
    }
}

/// Читает тело ответа, но не более `limit` байт (недоверенное тело не читается целиком).
async fn read_body_limited(response: reqwest::Response, limit: usize) -> String {
    let mut stream = response.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
    while buf.len() < limit {
        let Some(chunk) = stream.next().await else {
            break;
        };
        let Ok(chunk) = chunk else {
            break;
        };
        let remaining = limit - buf.len();
        let take = chunk.len().min(remaining);
        buf.extend_from_slice(&chunk[..take]);
    }
    String::from_utf8_lossy(&buf).into_owned()
}

/// Собирает тело Chat Completions, вливая пользовательский `extra_body` как **надстройку**:
/// обязательные поля протокола (`model`/`messages`/…) не переопределяются (ADR-0035 §2.4).
pub(in crate::ai) fn build_chat_body(request: &ChatRequest) -> serde_json::Value {
    let mut body = serde_json::Map::new();
    body.insert(
        "model".to_string(),
        serde_json::Value::String(request.model.clone()),
    );
    body.insert(
        "messages".to_string(),
        serde_json::to_value(&request.messages).unwrap_or(serde_json::Value::Null),
    );
    body.insert(
        "max_tokens".to_string(),
        serde_json::json!(request.max_tokens),
    );
    body.insert(
        "temperature".to_string(),
        serde_json::json!(request.temperature),
    );
    body.insert("stream".to_string(), serde_json::Value::Bool(true));
    body.insert(
        "stream_options".to_string(),
        serde_json::json!({ "include_usage": true }),
    );
    if let Some(tools) = &request.tools {
        body.insert("tools".to_string(), tools.clone());
    }
    for (key, value) in &request.extra_body {
        body.entry(key.clone()).or_insert_with(|| value.clone());
    }
    serde_json::Value::Object(body)
}

/// Заголовки, которые формирует клиент: пользовательские `headers` их не переопределяют,
/// иначе источник авторизации стал бы неоднозначным (ADR-0035 §2.4 п.11).
fn is_reserved_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "authorization" | "content-type" | "accept" | "host" | "content-length"
    )
}

/// Отбирает из пользовательских `headers` валидные и не зарезервированные пары.
pub(in crate::ai) fn custom_headers(
    headers: &std::collections::BTreeMap<String, String>,
) -> Vec<(HeaderName, HeaderValue)> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            if is_reserved_header(name) {
                return None;
            }
            let name = HeaderName::from_bytes(name.as_bytes()).ok()?;
            let value = HeaderValue::from_str(value).ok()?;
            Some((name, value))
        })
        .collect()
}

/// Навешивает пользовательские заголовки на запрос (уже отфильтрованные [`custom_headers`]).
fn apply_custom_headers(
    mut builder: reqwest::RequestBuilder,
    headers: &std::collections::BTreeMap<String, String>,
) -> reqwest::RequestBuilder {
    for (name, value) in custom_headers(headers) {
        builder = builder.header(name, value);
    }
    builder
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
            let body = build_chat_body(&request);
            let auth = format!("Bearer {}", request.api_key);
            let mut attempt = 0;
            let response = loop {
                attempt += 1;
                let mut builder = client
                    .post(url.as_str())
                    .header(reqwest::header::AUTHORIZATION, &auth)
                    .header(reqwest::header::ACCEPT, "text/event-stream")
                    .json(&body);
                builder = apply_custom_headers(builder, &request.headers);
                if let Some(timeout) = request.timeout {
                    builder = builder.timeout(timeout);
                }
                match builder.send().await {
                    Ok(response)
                        if response.status().is_server_error() && attempt < MAX_ATTEMPTS =>
                    {
                        tokio::time::sleep(RETRY_BACKOFF).await;
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
                let error_body = read_body_limited(response, MAX_ERROR_BODY_BYTES).await;
                return Err(map_status(
                    status.as_u16(),
                    &error_body,
                    retry_after.as_deref(),
                ));
            }

            let mut parser = SseParser::new();
            let mut content = String::new();
            let mut usage = None;
            let mut finish_reason = None;
            let mut tool_acc = ToolCallAccumulator::default();
            let mut stream = response.bytes_stream();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|e| {
                    CommandError::new(
                        ErrorCode::AiOffline,
                        format!("обрыв потока ответа: {e}"),
                    )
                })?;
                for event in parser.push(&chunk) {
                    match event {
                        SseEvent::Delta(delta) => {
                            ensure_within_response_limit(content.len(), delta.len())?;
                            content.push_str(&delta);
                            on_delta(delta);
                        }
                        SseEvent::ToolCallDelta {
                            index,
                            id,
                            name,
                            arguments,
                        } => {
                            tool_acc.push(index, id, name, arguments);
                        }
                        SseEvent::Usage(value) => usage = Some(value),
                        SseEvent::Finish(reason) => finish_reason = Some(reason),
                        SseEvent::Done => {}
                    }
                }
            }

            Ok(ChatOutcome {
                content,
                usage,
                finish_reason,
                tool_calls: tool_acc.finish(),
            })
        })
    }

    fn test_connection(
        &self,
        request: TestRequest,
    ) -> BoxFuture<'_, Result<Option<String>, CommandError>> {
        Box::pin(async move {
            let client = self.client()?;
            let url = endpoint(&request.base_url, "models");
            let mut builder = client.get(url.as_str()).header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", request.api_key),
            );
            builder = apply_custom_headers(builder, &request.headers);
            if let Some(timeout) = request.timeout {
                builder = builder.timeout(timeout);
            }
            let response = builder.send().await.map_err(|e| {
                CommandError::new(
                    ErrorCode::AiOffline,
                    format!("нет соединения с провайдером: {e}"),
                )
            })?;
            let status = response.status();
            let body = read_body_limited(response, MAX_ERROR_BODY_BYTES).await;
            if !status.is_success() {
                return Err(map_status(status.as_u16(), &body, None));
            }
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
