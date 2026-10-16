//! Ошибки IPC-команд. Заменяют голый `String`: у ошибки есть код (для логики фронта)
//! и текст сообщения (для статус-бара). Формат сериализуется в `{ code, message }`;
//! `http_status` добавляется только у ошибок провайдера (ADR-0040 §2.1).

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
    /// AI: фича выключена пользователем (`ai.enabled = false`, ADR-0034).
    AiDisabled,
    /// AI: не задан API-ключ провайдера.
    AiNoKey,
    /// AI: нет согласия на отправку содержимого в облако (ADR-0032 §2.3).
    AiNoConsent,
    /// AI: ключ отклонён провайдером (HTTP 401).
    AiBadKey,
    /// AI: превышен лимит запросов/квота (HTTP 429, ADR-0032 §2.5 п.13).
    AiRateLimited,
    /// AI: провайдер отклонил запрос (HTTP 400 и прочие 4xx).
    AiBadRequest,
    /// AI: запрос длиннее контекста модели (HTTP 400, context length exceeded).
    AiContextLength,
    /// AI: транзиентная ошибка провайдера (HTTP 5xx).
    AiServerError,
    /// AI: нет сети/таймаут/провайдер недоступен (без падения приложения).
    AiOffline,
    /// AI: некорректные настройки (напр. `base_url` не `https://` и не loopback).
    AiBadSettings,
    /// AI harness: предложенная правка устарела (ревизия документа изменилась, ADR-0034).
    AiStaleEdit,
    /// AI harness: правку не удалось применить (невалидные смещения/неизвестный документ).
    AiApplyFailed,
    /// AI harness: плагин не прошёл валидацию (манифест/entry/песочница, ADR-0034/ADR-0026).
    AiPluginInvalid,
    /// AI harness: плагин с таким id уже существует (ADR-0034/ADR-0026).
    AiPluginExists,
    /// AI harness: плагин с таким id не найден.
    AiPluginNotFound,
    /// AI harness: регистрация плагина в реестре не удалась (не по вине контента).
    AiPluginRegisterFailed,
    /// AI harness: ошибка ввода-вывода при работе с файлами плагина/временным каталогом.
    AiIo,
}

/// Ошибка команды: код + сообщение (сообщение видит статус-бар и e2e).
#[derive(Debug, Serialize)]
pub struct CommandError {
    pub code: ErrorCode,
    pub message: String,
    /// HTTP-статус ответа провайдера; `None` — ответа по HTTP не было (локальная проверка,
    /// ошибка протокола MCP, сеть). Писатель ровно один — [`CommandError::provider`], поэтому
    /// payload остальных команд сериализуется как `{ code, message }` (ADR-0040 §2.1).
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

    /// Ошибка, порождённая ответом провайдера: сохраняет его HTTP-статус.
    ///
    /// Нужен там, где без статуса нельзя отличить отказ провайдера от локальной ошибки с тем
    /// же кодом (ADR-0040 §2.1) — сейчас это политика деградации `tools` в
    /// [`crate::ai::tool_loop`].
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

    /// ADR-0040 §2.1 п.4: единственный барьер для замороженных контрактов §13.2 — payload всех
    /// команд, кроме AI-провайдера, остаётся ровно `{ code, message }`. Поле `http_status`
    /// обязано быть пропущено, даже если конструктор ошибки изменится.
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

    /// Обратная сторона инварианта §2.1: у ошибки провайдера статус виден — по нему tool-loop
    /// отличает отказ шлюза от локальной ошибки с тем же кодом.
    #[test]
    fn provider_error_serializes_http_status() {
        let err = CommandError::provider(400, ErrorCode::AiBadRequest, "Некорректный запрос");
        let value = serde_json::to_value(&err).expect("ошибка сериализуется");

        assert_eq!(value["code"], "ai_bad_request");
        assert_eq!(value["http_status"], 400);
    }
}
