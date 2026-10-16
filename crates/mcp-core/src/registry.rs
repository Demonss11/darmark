//! Реестр инструментов и ресурсов.

use std::sync::Arc;

use serde_json::{json, Value};

use crate::handlers::{ResourceError, ResourceReader, ToolError, ToolHandler};
use crate::protocol;

/// Определение инструмента в реестре.
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub handler: Arc<dyn ToolHandler>,
}

/// Определение ресурса в реестре.
///
/// `name` обязателен по спецификации MCP (поле `Resource.name`) и отдаётся
/// агенту в `resources/list` — по нему агент понимает назначение ресурса.
pub struct ResourceDefinition {
    pub uri: String,
    pub name: String,
    pub description: String,
    pub reader: Arc<dyn ResourceReader>,
}

/// Результат `call_tool`: различает protocol-ошибку и ошибку исполнения.
///
/// «Инструмент не зарегистрирован» — это ошибка протокола (JSON-RPC error),
/// а не результат работы инструмента. Смешивать их нельзя: агент по `isError`
/// судит о сбое самого вызова, а не о неверном имени (спека MCP).
///
/// `isError` в конверте вызыва несёт не только этот тип: инструмент-верификатор вправе сам
/// объявить отрицательный вердикт полем `isError` в payload — транспорт копирует его в
/// конверт, сохраняя структурированные данные (ADR-0041 §2.2). Различение protocol-ошибки
/// и ошибки исполнения этим не отменяется: обе они остаются `Err`.
#[derive(Clone, Debug)]
pub enum CallToolError {
    /// Инструмент не зарегистрирован (protocol error, JSON-RPC `-32602`).
    UnknownTool(String),
    /// Ошибка исполнения инструмента (отдаётся как результат с `isError: true`).
    Execution(ToolError),
}

/// Реестр инструментов и ресурсов.
#[derive(Default)]
pub struct Registry {
    tools: Vec<ToolDefinition>,
    resources: Vec<ResourceDefinition>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_tool(&mut self, def: ToolDefinition) {
        self.tools.push(def);
    }

    pub fn register_resource(&mut self, def: ResourceDefinition) {
        self.resources.push(def);
    }

    /// Список инструментов для `tools/list`.
    pub fn tools_list(&self) -> Value {
        let tools: Vec<Value> = self
            .tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "inputSchema": t.input_schema,
                })
            })
            .collect();
        json!({ "tools": tools })
    }

    /// Список ресурсов для `resources/list`.
    pub fn resources_list(&self) -> Value {
        let resources: Vec<Value> = self
            .resources
            .iter()
            .map(|r| {
                json!({
                    "uri": r.uri,
                    "name": r.name,
                    "description": r.description,
                })
            })
            .collect();
        json!({ "resources": resources })
    }

    /// Вызов инструмента по имени.
    ///
    /// Неизвестное имя — `CallToolError::UnknownTool` (protocol error);
    /// ошибка обработчика — `CallToolError::Execution`.
    pub fn call_tool(&self, name: &str, params: Value) -> Result<Value, CallToolError> {
        let tool = self
            .tools
            .iter()
            .find(|t| t.name == name)
            .ok_or_else(|| CallToolError::UnknownTool(name.to_string()))?;
        tool.handler.call(params).map_err(CallToolError::Execution)
    }

    /// Чтение ресурса по URI.
    ///
    /// Сначала точное совпадение (статические ресурсы), затем — шаблонные регистрации
    /// (`darmark://plugins/{id}/manifest`): без этого шаблонный ресурс недостижим, так как
    /// запрос всегда несёт конкретный URI, а не шаблон. Читателю передаётся **фактический**
    /// URI запроса (не шаблон) — он сам извлекает из него параметры.
    pub fn read_resource(&self, uri: &str) -> Result<Value, ResourceError> {
        if let Some(resource) = self.resources.iter().find(|r| r.uri == uri) {
            return resource.reader.read(uri);
        }
        if let Some(resource) = self
            .resources
            .iter()
            .find(|r| uri_matches_template(&r.uri, uri))
        {
            return resource.reader.read(uri);
        }
        Err(ResourceError::new(
            protocol::RESOURCE_NOT_FOUND,
            format!("ресурс не найден: {uri}"),
        ))
    }
}

/// Сопоставляет запрошенный URI с шаблоном ресурса.
///
/// Сегментный матч по `/`: сегмент шаблона вида `{name}` матчит **любой непустой**
/// сегмент запроса; остальные сегменты должны совпасть буквально. Число сегментов
/// обязано совпадать, поэтому `{id}` не «съедает» лишние `/`. Шаблон без `{…}` —
/// обычный точный URI.
fn uri_matches_template(template: &str, actual: &str) -> bool {
    if !template.contains('{') {
        return template == actual;
    }
    let template_segments: Vec<&str> = template.split('/').collect();
    let actual_segments: Vec<&str> = actual.split('/').collect();
    if template_segments.len() != actual_segments.len() {
        return false;
    }
    template_segments
        .iter()
        .zip(actual_segments.iter())
        .all(|(expected, got)| match placeholder_name(expected) {
            Some(name) => !name.is_empty() && !got.is_empty(),
            None => expected == got,
        })
}

/// Имя параметра шаблонного сегмента (`{id}` → `Some("id")`), иначе `None`.
fn placeholder_name(segment: &str) -> Option<&str> {
    segment.strip_prefix('{')?.strip_suffix('}')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_uri_matches_only_itself() {
        assert!(uri_matches_template("darmark://test", "darmark://test"));
        assert!(!uri_matches_template("darmark://test", "darmark://other"));
        // URI без `{…}` — не шаблон: лишний сегмент не допускается.
        assert!(!uri_matches_template(
            "darmark://test",
            "darmark://test/extra"
        ));
    }

    #[test]
    fn placeholder_matches_any_non_empty_segment() {
        let template = "darmark://plugins/{id}/manifest";
        assert!(uri_matches_template(
            template,
            "darmark://plugins/word-count/manifest"
        ));
        // Пустой сегмент на месте `{id}` не матчится (id обязателен).
        assert!(!uri_matches_template(
            template,
            "darmark://plugins//manifest"
        ));
        // Число сегментов должно совпадать.
        assert!(!uri_matches_template(
            template,
            "darmark://plugins/word-count/manifest/extra"
        ));
        assert!(!uri_matches_template(
            template,
            "darmark://plugins/word-count"
        ));
        // Литеральные сегменты — буквально.
        assert!(!uri_matches_template(
            template,
            "darmark://plugins/word-count/meta"
        ));
    }

    #[test]
    fn placeholder_name_extracts_inner_text() {
        assert_eq!(placeholder_name("{id}"), Some("id"));
        assert_eq!(placeholder_name("plugins"), None);
        assert_eq!(placeholder_name("{}"), Some(""));
    }
}
