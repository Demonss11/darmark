//! Типы чата AI-ассистента и harness'а (ADR-0032/0034): сообщения, ответ, скиллы,
//! история, диффы и валидация плагинов.

use serde::{Deserialize, Serialize};

use crate::ai::tool_calls::ToolCall;

/// Сообщение чата. `role` — `"system" | "user" | "assistant" | "tool"`.
///
/// `name`/`tool_call_id`/`tool_calls` заполняются только в tool-loop: assistant-сообщение
/// несёт вызовы инструментов, `tool`-сообщение — их результат и id вызова. `Eq` не выводится:
/// `tool_calls` содержит `serde_json::Value`, который `Eq` не реализует.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AiMessage {
    pub role: String,
    pub content: String,
    /// Имя инструмента для `tool`-сообщения (опционально, по контракту OpenAI).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub name: Option<String>,
    /// Идентификатор вызова, на который отвечает `tool`-сообщение.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tool_call_id: Option<String>,
    /// Вызовы инструментов, которые запросила модель (assistant-сообщение).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tool_calls: Option<Vec<ToolCall>>,
}

/// Токены использования ответа (показ расходов, ADR-0032).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

/// Ответ модели: итоговое сообщение, (если провайдер прислал) `usage` и причина завершения.
#[derive(Clone, Debug, Serialize)]
pub struct AiResponse {
    pub message: AiMessage,
    pub usage: Option<AiUsage>,
    /// Причина завершения стрима (`"stop"`, `"length"` и т.д.); `None`, если не пришла.
    pub finish_reason: Option<String>,
}

/// Готовый скилл (промпт) из встроенной библиотеки (ADR-0032).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AiSkill {
    pub id: String,
    pub title: String,
    pub description: String,
    /// Шаблон промта; подстановки — `{{doc}}` (текст документа) и `{{input}}` (описание из чата).
    pub prompt_template: String,
    /// Категория: `document` или `plugins`.
    pub category: String,
}

/// Представление истории чата для фронтенда (`ai_history_load`).
///
/// Секретов и абсолютных путей не содержит: только идентификатор сессии,
/// метка времени, сообщения и опциональное предупреждение о проблеме загрузки.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HistoryView {
    /// Идентификатор сессии чата.
    pub session_id: String,
    /// Время последнего обновления (unix-секунды).
    pub updated_at: u64,
    /// Сообщения чата (последние первыми — новые в конце).
    pub messages: Vec<AiMessage>,
    /// Предупреждение о проблеме загрузки (битый файл, превышение лимита и т.п.).
    pub warning: Option<String>,
}

/// Результат лёгкой проверки соединения (`ai_test_connection`).
#[derive(Clone, Debug, Serialize)]
pub struct AiTestResult {
    pub ok: bool,
    /// Модель, если провайдер вернул список (`GET /models`).
    pub model: Option<String>,
    /// Понятное сообщение об ошибке (ключ/сеть/статус); `None` при успехе.
    pub error: Option<String>,
}

/// Одна операция правки документа (harness, ADR-0034): замена `[start, end)` на `text`.
///
/// Смещения — **байтовые** UTF-8 (как `data-md` и `DocumentStore::apply_edit`, D5).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiEditOp {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

/// Предложенный дифф документа: исходный/новый текст, операции и база для rev-guard.
#[derive(Clone, Debug, Serialize)]
pub struct AiDiff {
    pub doc_id: String,
    /// Ревизия документа, на которой посчитан дифф (проверяется в `ai_apply_edit`).
    pub base_rev: u64,
    pub original: String,
    pub modified: String,
    pub ops: Vec<AiEditOp>,
}

/// Результат валидации плагина (harness, ADR-0034/ADR-0026): статус, понятные ошибки и сырой отчёт CLI.
#[derive(Clone, Debug, Serialize)]
pub struct AiValidationResult {
    pub ok: bool,
    pub errors: Vec<String>,
    /// Сырой JSON-отчёт `darmark-plugin test --json`, если CLI доступен и отчёт версионирован;
    /// `None` — результат получен fallback-путём.
    pub report: Option<serde_json::Value>,
}
