//! Ошибки IPC-команд. Заменяют голый `String`: у ошибки есть код (для логики фронта)
//! и текст сообщения (для статус-бара). Формат сериализуется в `{ code, message }`.

use std::fmt;

use serde::Serialize;

/// Машиночитаемый код ошибки. Строковые значения — контракт с TS.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// Файл не найден.
    NotFound,
    /// Ошибка ввода-вывода при чтении/записи.
    Io,
    /// Файл превышает лимит размера.
    TooLarge,
    /// Файл не является корректным UTF-8.
    NotUtf8,
    /// Команда ссылается на неизвестный документ.
    UnknownDocument,
    /// Ошибка плагинной подсистемы (неизвестный плагин/view, отказ invocation).
    Plugin,
}

/// Ошибка команды: код + сообщение (сообщение видит статус-бар и e2e).
#[derive(Debug, Serialize)]
pub struct CommandError {
    pub code: ErrorCode,
    pub message: String,
}

impl CommandError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
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
}
