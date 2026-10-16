//! Ошибки IPC-команд. Заменяют голый `String`: у ошибки есть код (для логики фронта)
//! и текст сообщения (для статус-бара). Формат сериализуется в `{ code, message }`;
//! `http_status` добавляется только у ошибок провайдера (ADR-0040 §2.1).

use std::fmt;

use serde::Serialize;


#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    
    NotFound,
    
    Io,
    
    TooLarge,
    
    NotUtf8,
    
    UnknownDocument,
    
    NothingToUndo,
    
    NothingToRedo,
    
    Plugin,
    
    AiDisabled,
    
    AiNoKey,
    
    AiNoConsent,
    
    AiBadKey,
    
    AiRateLimited,
    
    AiBadRequest,
    
    AiContextLength,
    
    AiServerError,
    
    AiOffline,
    
    AiBadSettings,
    
    AiStaleEdit,
    
    AiApplyFailed,
    
    AiPluginInvalid,
    
    AiPluginExists,
    
    AiPluginNotFound,
    
    AiPluginRegisterFailed,
    
    AiIo,
}


#[derive(Debug, Serialize)]
pub struct CommandError {
    pub code: ErrorCode,
    pub message: String,
    
    
    
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_status: Option<u16>,
}

impl CommandError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            http_status: None,
        }
    }

    
    
    
    
    
    pub fn provider(status: u16, code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            http_status: Some(status),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotFound, message)
    }

    pub fn io(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Io, message)
    }

    pub fn too_large(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::TooLarge, message)
    }

    pub fn not_utf8(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotUtf8, message)
    }

    pub fn unknown_document(id: &str) -> Self {
        Self::new(
            ErrorCode::UnknownDocument,
            format!("Документ не найден: {id}"),
        )
    }

    pub fn nothing_to_undo() -> Self {
        Self::new(ErrorCode::NothingToUndo, "Нечего отменять")
    }

    pub fn nothing_to_redo() -> Self {
        Self::new(ErrorCode::NothingToRedo, "Нечего повторять")
    }
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CommandError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carries_code_and_message() {
        let err = CommandError::unknown_document("doc-7");
        assert_eq!(err.code, ErrorCode::UnknownDocument);
        assert_eq!(err.message, "Документ не найден: doc-7");
    }

    #[test]
    fn undo_redo_errors_have_distinct_codes() {
        assert_eq!(
            CommandError::nothing_to_undo().code,
            ErrorCode::NothingToUndo
        );
        assert_eq!(
            CommandError::nothing_to_redo().code,
            ErrorCode::NothingToRedo
        );
    }

    
    
    
    #[test]
    fn local_errors_serialize_without_http_status() {
        let cases = [
            ("unknown_document", CommandError::unknown_document("doc-7")),
            ("not_found", CommandError::not_found("файл отсутствует")),
            ("io", CommandError::io("диск недоступен")),
            ("too_large", CommandError::too_large("файл слишком большой")),
            ("not_utf8", CommandError::not_utf8("некорректный UTF-8")),
            ("nothing_to_undo", CommandError::nothing_to_undo()),
            ("nothing_to_redo", CommandError::nothing_to_redo()),
        ];

        for (label, err) in cases {
            assert_eq!(err.http_status, None, "у {label} нет HTTP-ответа");
            let value = serde_json::to_value(&err).expect("ошибка сериализуется");
            let object = value.as_object().expect("payload — объект");
            assert_eq!(
                object.keys().collect::<Vec<_>>(),
                vec!["code", "message"],
                "payload {label} не должен содержать http_status: {value}"
            );
        }
    }

    
    
    #[test]
    fn provider_error_serializes_http_status() {
        let err = CommandError::provider(400, ErrorCode::AiBadRequest, "Некорректный запрос");
        let value = serde_json::to_value(&err).expect("ошибка сериализуется");

        assert_eq!(value["code"], "ai_bad_request");
        assert_eq!(value["http_status"], 400);
    }
}
