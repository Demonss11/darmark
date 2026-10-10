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
    /// Нечего отменять (пустой undo-стек).
    NothingToUndo,
    /// Нечего повторять (пустой redo-стек).
    NothingToRedo,
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
}
