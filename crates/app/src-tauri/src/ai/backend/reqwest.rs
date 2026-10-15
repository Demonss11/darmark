//! `reqwest`-Р±СЌРєРµРЅРґ С€РІР° `AiTransport` (ADR-0033): OpenAI-СЃРѕРІРјРµСЃС‚РёРјС‹Р№ API + SSE-СЃС‚СЂРёРјРёРЅРі.
//!
//! HTTP вЂ” `POST {base_url}/chat/completions` СЃ Bearer-РєР»СЋС‡РѕРј Рё `stream: true`; РѕС‚РІРµС‚ С‡РёС‚Р°РµС‚СЃСЏ
//! РєР°Рє SSE (`bytes_stream`) Рё СЂР°Р·Р±РёСЂР°РµС‚СЃСЏ РїР°СЂСЃРµСЂРѕРј `crate::ai::sse`. TLS вЂ” СЃРёСЃС‚РµРјРЅС‹Р№ SChannel
//! (`native-tls`, Р±РµР· OpenSSL). РўР°Р№РјР°СѓС‚ вЂ” 30 СЃ РїРѕ СѓРјРѕР»С‡Р°РЅРёСЋ (ADR-0032); РїСЂРѕРІР°Р№РґРµСЂ РјРѕР¶РµС‚
//! РїРµСЂРµРѕРїСЂРµРґРµР»РёС‚СЊ РµРіРѕ С‡РµСЂРµР· `settings.timeout` (ADR-0035 В§2.5).

use std::time::Duration;

use futures_util::StreamExt;
use reqwest::header::{HeaderName, HeaderValue};

use crate::ai::errors::map_status;
use crate::ai::sse::{SseEvent, SseParser};
use crate::ai::transport::{AiTransport, BoxFuture, ChatOutcome, ChatRequest, TestRequest};
use crate::error::{CommandError, ErrorCode};

/// РўР°Р№РјР°СѓС‚ Р·Р°РїСЂРѕСЃР° Рє РїСЂРѕРІР°Р№РґРµСЂСѓ РїРѕ СѓРјРѕР»С‡Р°РЅРёСЋ (ADR-0032, ADR-0033 В§2.3 Рї.9).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// Р§РёСЃР»Рѕ РїРѕРїС‹С‚РѕРє РґР»СЏ С‚СЂР°РЅР·РёРµРЅС‚РЅРѕР№ 5xx (РїРµСЂРІР°СЏ + РѕРґРёРЅ СЂРµС‚СЂР°Р№, ADR-0032 В§2.5 Рї.13).
const MAX_ATTEMPTS: u32 = 2;
/// РџР°СѓР·Р° РїРµСЂРµРґ СЂРµС‚СЂР°РµРј РЅР° 5xx.
const RETRY_BACKOFF: Duration = Duration::from_secs(1);
/// Р›РёРјРёС‚ РЅР°РєРѕРїР»РµРЅРЅРѕРіРѕ С‚РµРєСЃС‚Р° РѕС‚РІРµС‚Р°: РЅРµРґРѕРІРµСЂРµРЅРЅРѕРµ С‚РµР»Рѕ РЅРµ РґРѕР»Р¶РЅРѕ РёСЃС‡РµСЂРїР°С‚СЊ РїР°РјСЏС‚СЊ РїРѕРґ `panic=abort`.
pub(in crate::ai) const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
/// Р›РёРјРёС‚ С‡С‚РµРЅРёСЏ С‚РµР»Р° РѕС€РёР±РєРё/СЃРїРёСЃРєР° РјРѕРґРµР»РµР№ РїРµСЂРµРґ СЂР°Р·Р±РѕСЂРѕРј (Р·Р°С‰РёС‚Р° РѕС‚ РіРёРіР°РЅС‚СЃРєРѕРіРѕ С‚РµР»Р°).
const MAX_ERROR_BODY_BYTES: usize = 64 * 1024;

/// HTTP-РєР»РёРµРЅС‚ AI РЅР° `reqwest` + SChannel. `reqwest::Client` СЃС‚СЂРѕРёС‚СЃСЏ Р»РµРЅРёРІРѕ (ADR-0034 В§2.3).
pub struct ReqwestBackend {
    client: std::sync::OnceLock<reqwest::Client>,
}

impl ReqwestBackend {
    pub fn new() -> Self {
        Self {
            client: std::sync::OnceLock::new(),
        }
    }

    /// Р’РѕР·РІСЂР°С‰Р°РµС‚ РѕР±С‰РёР№ РєР»РёРµРЅС‚, СЃРѕР·РґР°РІР°СЏ РµРіРѕ РїСЂРё РїРµСЂРІРѕРј РѕР±СЂР°С‰РµРЅРёРё (Р»РµРЅРёРІР°СЏ РёРЅРёС†РёР°Р»РёР·Р°С†РёСЏ).
    fn client(&self) -> Result<&reqwest::Client, CommandError> {
        if let Some(client) = self.client.get() {
            return Ok(client);
        }
        let built = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            // Р РµРґРёСЂРµРєС‚С‹ Р·Р°РїСЂРµС‰РµРЅС‹: С‚РµР»Рѕ (РґРѕРєСѓРјРµРЅС‚) РЅРµ РґРѕР»Р¶РЅРѕ СѓС…РѕРґРёС‚СЊ РЅР° С…РѕСЃС‚, РЅРµ РЅР°Р·РІР°РЅРЅС‹Р№
            // РІ СЃРѕРіР»Р°СЃРёРё (H2).
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| {
                CommandError::new(
                    ErrorCode::AiOffline,
                    format!("РЅРµ СЃРѕР·РґР°С‚СЊ HTTP-РєР»РёРµРЅС‚: {e}"),
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

/// РЎРєР»РµРёРІР°РµС‚ Р±Р°Р·РѕРІСѓСЋ С‚РѕС‡РєСѓ Рё РїСѓС‚СЊ, РЅРµ РґРѕРїСѓСЃРєР°СЏ РґРІРѕР№РЅРѕРіРѕ СЃР»СЌС€Р°.
pub(in crate::ai) fn endpoint(base_url: &str, path: &str) -> String {
    format!("{}/{}", base_url.trim_end_matches('/'), path)
}

/// РџСЂРѕРІРµСЂСЏРµС‚, С‡С‚Рѕ РЅР°РєРѕРїР»РµРЅРЅС‹Р№ РѕС‚РІРµС‚ РІРјРµСЃС‚Рµ СЃ РЅРѕРІРѕР№ РґРµР»СЊС‚РѕР№ РЅРµ РїСЂРµРІС‹С€Р°РµС‚ Р»РёРјРёС‚ (H1).
pub(in crate::ai) fn ensure_within_response_limit(
    current: usize,
    incoming: usize,
) -> Result<(), CommandError> {
    if current.saturating_add(incoming) > MAX_RESPONSE_BYTES {
        Err(CommandError::new(
            ErrorCode::AiBadRequest,
            "РѕС‚РІРµС‚ РїСЂРѕРІР°Р№РґРµСЂР° РїСЂРµРІС‹СЃРёР» РґРѕРїСѓСЃС‚РёРјС‹Р№ СЂР°Р·РјРµСЂ",
        ))
    } else {
        Ok(())
    }
}

/// Р§РёС‚Р°РµС‚ С‚РµР»Рѕ РѕС‚РІРµС‚Р°, РЅРѕ РЅРµ Р±РѕР»РµРµ `limit` Р±Р°Р№С‚ (РЅРµРґРѕРІРµСЂРµРЅРЅРѕРµ С‚РµР»Рѕ РЅРµ С‡РёС‚Р°РµС‚СЃСЏ С†РµР»РёРєРѕРј).
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

/// РЎРѕР±РёСЂР°РµС‚ С‚РµР»Рѕ Chat Completions, РІР»РёРІР°СЏ РїРѕР»СЊР·РѕРІР°С‚РµР»СЊСЃРєРёР№ `extra_body` РєР°Рє **РЅР°РґСЃС‚СЂРѕР№РєСѓ**:
/// РѕР±СЏР·Р°С‚РµР»СЊРЅС‹Рµ РїРѕР»СЏ РїСЂРѕС‚РѕРєРѕР»Р° (`model`/`messages`/`stream`/вЂ¦) РЅРµ РїРµСЂРµРѕРїСЂРµРґРµР»СЏСЋС‚СЃСЏ (ADR-0035 В§2.4).
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
    for (key, value) in &request.extra_body {
        body.entry(key.clone()).or_insert_with(|| value.clone());
    }
    serde_json::Value::Object(body)
}

/// Р—Р°РіРѕР»РѕРІРєРё, РєРѕС‚РѕСЂС‹Рµ С„РѕСЂРјРёСЂСѓРµС‚ РєР»РёРµРЅС‚: РїРѕР»СЊР·РѕРІР°С‚РµР»СЊСЃРєРёРµ `headers` РёС… РЅРµ РїРµСЂРµРѕРїСЂРµРґРµР»СЏСЋС‚,
/// РёРЅР°С‡Рµ РёСЃС‚РѕС‡РЅРёРє Р°РІС‚РѕСЂРёР·Р°С†РёРё СЃС‚Р°Р» Р±С‹ РЅРµРѕРґРЅРѕР·РЅР°С‡РЅС‹Рј (ADR-0035 В§2.4 Рї.11).
fn is_reserved_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "authorization" | "content-type" | "accept" | "host" | "content-length"
    )
}

/// РћС‚Р±РёСЂР°РµС‚ РёР· РїРѕР»СЊР·РѕРІР°С‚РµР»СЊСЃРєРёС… `headers` РІР°Р»РёРґРЅС‹Рµ Рё РЅРµ Р·Р°СЂРµР·РµСЂРІРёСЂРѕРІР°РЅРЅС‹Рµ РїР°СЂС‹.
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

/// РќР°РІРµС€РёРІР°РµС‚ РїРѕР»СЊР·РѕРІР°С‚РµР»СЊСЃРєРёРµ Р·Р°РіРѕР»РѕРІРєРё РЅР° Р·Р°РїСЂРѕСЃ (СѓР¶Рµ РѕС‚С„РёР»СЊС‚СЂРѕРІР°РЅРЅС‹Рµ [`custom_headers`]).
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
                            format!("РЅРµС‚ СЃРѕРµРґРёРЅРµРЅРёСЏ СЃ РїСЂРѕРІР°Р№РґРµСЂРѕРј: {e}"),
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
            let mut stream = response.bytes_stream();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|e| {
                    CommandError::new(
                        ErrorCode::AiOffline,
                        format!("РѕР±СЂС‹РІ РїРѕС‚РѕРєР° РѕС‚РІРµС‚Р°: {e}"),
                    )
                })?;
                for event in parser.push(&chunk) {
                    match event {
                        SseEvent::Delta(delta) => {
                            ensure_within_response_limit(content.len(), delta.len())?;
                            content.push_str(&delta);
                            on_delta(delta);
                        }
                        SseEvent::ToolCallDelta { .. } => {}
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
                    format!("РЅРµС‚ СЃРѕРµРґРёРЅРµРЅРёСЏ СЃ РїСЂРѕРІР°Р№РґРµСЂРѕРј: {e}"),
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
