//! serde-конверт хост ↔ child (§4.1 плагинной системы, §7.1 DESIGN_DOC): JSON-in-frame.
//!
//! От ручной кодировки C-спайка отказываемся: единый формат данных `ToChild`/`ToHost`/`PluginError`.
//! Разбор входящего от child потока — **fallible** (F36): нарушение протокола превращается в
//! [`PluginError`] с кодом [`codes::PROTOCOL`], а не в панику.

use serde::{Deserialize, Serialize};

/// Коды [`PluginError`]. Строки — часть контракта для UI/Lua.
pub mod codes {
    pub const PERMISSION_DENIED: &str = "permission_denied";
    pub const TIMEOUT: &str = "timeout";
    pub const CRASHED: &str = "crashed";
    pub const PROTOCOL: &str = "protocol";
    pub const LUA_ERROR: &str = "lua_error";
}

/// Сообщение host → child: вызов метода плагина либо ответ на ранее присланный host-call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "msg", rename_all = "snake_case")]
pub enum ToChild {
    /// Вызов (`load`/`activate`/`event`/`action`/…) с идентификатором для сопоставления ответа.
    Invoke {
        id: u32,
        method: String,
        #[serde(default)]
        args: serde_json::Value,
    },
    /// Ответ на [`ToHost::HostCall`] с тем же `id`.
    Reply {
        id: u32,
        result: Result<serde_json::Value, PluginError>,
    },
}

/// Сообщение child → host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "msg", rename_all = "snake_case")]
pub enum ToHost {
    /// Запрос host-функции (range/delta документа, настройки, экспорт и т.п.).
    HostCall {
        id: u32,
        method: String,
        #[serde(default)]
        args: serde_json::Value,
    },
    /// Событие жизненного цикла (`ready`/`done`/…). Для строк лога используйте [`ToHost::Log`].
    Event {
        kind: String,
        #[serde(default)]
        payload: serde_json::Value,
    },
    /// Лог плагина — **канонический** канал строк лога (host.log/print). `Event{kind:"log"}`
    /// допустим, но не рекомендуется: два способа логирования рассинхронизируются.
    Log { level: String, message: String },
}

/// Ошибка, пересекающая границу процесса. Возвращается как **значение**, не Lua-исключение
/// (например `permission_denied` при отсутствии разрешения, §4.4/§8).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginError {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl PluginError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            permission: None,
            data: None,
        }
    }

    pub fn permission_denied(permission: impl Into<String>) -> Self {
        let permission = permission.into();
        Self {
            code: codes::PERMISSION_DENIED.into(),
            message: format!("нет разрешения {permission}"),
            permission: Some(permission),
            data: None,
        }
    }

    pub fn timeout(message: impl Into<String>) -> Self {
        Self::new(codes::TIMEOUT, message)
    }

    pub fn crashed(message: impl Into<String>) -> Self {
        Self::new(codes::CRASHED, message)
    }

    pub fn protocol(message: impl Into<String>) -> Self {
        Self::new(codes::PROTOCOL, message)
    }

    pub fn lua(message: impl Into<String>) -> Self {
        Self::new(codes::LUA_ERROR, message)
    }
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for PluginError {}

/// Сериализует сообщение в JSON-payload кадра.
pub fn encode_to_child(msg: &ToChild) -> serde_json::Result<Vec<u8>> {
    serde_json::to_vec(msg)
}

/// Сериализует сообщение child → host в JSON-payload кадра.
pub fn encode_to_host(msg: &ToHost) -> serde_json::Result<Vec<u8>> {
    serde_json::to_vec(msg)
}

/// Разбирает payload host → child. Ошибка разбора — [`PluginError`] `protocol` (fallible, без паники).
pub fn decode_to_child(payload: &[u8]) -> Result<ToChild, PluginError> {
    serde_json::from_slice(payload)
        .map_err(|e| PluginError::protocol(format!("конверт хост→child: {e}")))
}

/// Разбирает payload child → host. **Не паникует** на мусоре/обрезке (F36).
pub fn decode_to_host(payload: &[u8]) -> Result<ToHost, PluginError> {
    serde_json::from_slice(payload)
        .map_err(|e| PluginError::protocol(format!("конверт child→host: {e}")))
}

/// Пишет кадр host → child с JSON-конвертом (сторона хоста).
pub fn write_to_child<W: std::io::Write>(w: &mut W, msg: &ToChild) -> std::io::Result<()> {
    let bytes = encode_to_child(msg).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("сериализация: {e}"),
        )
    })?;
    crate::frame::write_frame(w, &bytes)
}

/// Читает кадр host → child (сторона child). Ошибка кадра — `io::Error`; нарушение формата —
/// `Ok(Err(PluginError protocol))`, чтобы вызывающий единообразно обработал сбой конверта.
pub fn read_to_child<R: std::io::Read>(r: &mut R) -> std::io::Result<Result<ToChild, PluginError>> {
    let payload = crate::frame::read_frame_host(r)?;
    Ok(decode_to_child(&payload))
}

/// Пишет кадр child → host с JSON-конвертом (сторона child).
pub fn write_to_host<W: std::io::Write>(w: &mut W, msg: &ToHost) -> std::io::Result<()> {
    let bytes = encode_to_host(msg).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("сериализация: {e}"),
        )
    })?;
    crate::frame::write_frame(w, &bytes)
}

/// Читает кадр child → host (сторона хоста). Ошибка кадра — `io::Error`; нарушение формата —
/// `Ok(Err(PluginError protocol))`, чтобы вызывающий единообразно обработал отказ плагина.
pub fn read_to_host<R: std::io::Read>(r: &mut R) -> std::io::Result<Result<ToHost, PluginError>> {
    let payload = crate::frame::read_frame(r)?;
    Ok(decode_to_host(&payload))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn roundtrip_to_child(msg: ToChild) {
        let bytes = encode_to_child(&msg).unwrap();
        assert_eq!(decode_to_child(&bytes).unwrap(), msg);
    }

    fn roundtrip_to_host(msg: ToHost) {
        let bytes = encode_to_host(&msg).unwrap();
        assert_eq!(decode_to_host(&bytes).unwrap(), msg);
    }

    #[test]
    fn to_child_invoke_roundtrip() {
        roundtrip_to_child(ToChild::Invoke {
            id: 7,
            method: "event".into(),
            args: json!({"name": "document:changed", "payload": {"rev": 3}}),
        });
    }

    #[test]
    fn to_child_reply_ok_and_err_roundtrip() {
        roundtrip_to_child(ToChild::Reply {
            id: 1,
            result: Ok(json!({"doc_len": 42})),
        });
        roundtrip_to_child(ToChild::Reply {
            id: 2,
            result: Err(PluginError::permission_denied("document:write")),
        });
    }

    #[test]
    fn to_host_variants_roundtrip() {
        roundtrip_to_host(ToHost::HostCall {
            id: 5,
            method: "get_document_range".into(),
            args: json!({"doc_id": "d", "start": 0, "len": 10}),
        });
        roundtrip_to_host(ToHost::Event {
            kind: "ready".into(),
            payload: json!(null),
        });
        roundtrip_to_host(ToHost::Log {
            level: "info".into(),
            message: "привет".into(),
        });
    }

    #[test]
    fn event_defaults_when_payload_omitted() {
        let msg: ToHost = serde_json::from_slice(br#"{"msg":"event","kind":"done"}"#).unwrap();
        assert_eq!(
            msg,
            ToHost::Event {
                kind: "done".into(),
                payload: json!(null)
            }
        );
    }

    #[test]
    fn to_host_garbage_is_protocol_error_not_panic() {
        let err = decode_to_host(b"not json").unwrap_err();
        assert_eq!(err.code, codes::PROTOCOL);
    }

    #[test]
    fn to_host_truncated_is_protocol_error_not_panic() {
        let full = encode_to_host(&ToHost::Log {
            level: "info".into(),
            message: "0123456789".into(),
        })
        .unwrap();
        let err = decode_to_host(&full[..full.len() / 2]).unwrap_err();
        assert_eq!(err.code, codes::PROTOCOL);
    }

    #[test]
    fn unknown_tag_is_protocol_error() {
        assert_eq!(
            decode_to_host(br#"{"msg":"nope"}"#).unwrap_err().code,
            codes::PROTOCOL
        );
    }

    #[test]
    fn permission_error_keeps_permission_field() {
        let err = PluginError::permission_denied("document:write");
        let bytes = serde_json::to_vec(&err).unwrap();
        let back: PluginError = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(back.permission.as_deref(), Some("document:write"));
        assert_eq!(back.code, codes::PERMISSION_DENIED);
    }

    #[test]
    fn framed_roundtrip_via_reader() {
        let mut buf = Vec::new();
        let msg = ToHost::Log {
            level: "warn".into(),
            message: "тест".into(),
        };
        // Пишем кадром так же, как это делает child.
        write_to_host(&mut buf, &msg).unwrap();
        let got = read_to_host(&mut buf.as_slice()).unwrap().unwrap();
        assert_eq!(got, msg);
    }

    #[test]
    fn framed_roundtrip_to_child() {
        let mut buf = Vec::new();
        let msg = ToChild::Invoke {
            id: 9,
            method: "activate".into(),
            args: json!({"plugin_id": "word-count"}),
        };
        write_to_child(&mut buf, &msg).unwrap();
        let got = read_to_child(&mut buf.as_slice()).unwrap().unwrap();
        assert_eq!(got, msg);
    }

    #[test]
    fn read_to_child_oversized_frame_is_io_error() {
        // Хост → child допускает крупные кадры (документ), но потолок всё же есть.
        let mut data = ((crate::frame::MAX_HOST_FRAME_BYTES as u32) + 1)
            .to_le_bytes()
            .to_vec();
        data.extend_from_slice(&[0u8; 8]);
        assert!(read_to_child(&mut data.as_slice()).is_err());
    }
}
