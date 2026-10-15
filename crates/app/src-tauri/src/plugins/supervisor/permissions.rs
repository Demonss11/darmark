//! Гейт прав плагина: обязательное permission host-call'а, проверка выданного набора и
//! инъекция `_plugin_id` в аргументы (§4.4).
//!
//! Домен проверки прав отделён от автомата состояния `Supervisor`: и проверка, и
//! `_plugin_id`-инъекция нужны именно на границе host-call.

use plugin_core::envelope::PluginError;
use serde_json::Value;

/// Возвращает копию object-args с инжектированным `_plugin_id`.
///
/// Хост не доверяет идентификатору из плагина: `serve_host_call` подставляет
/// `self.plugin_id` перед передачей в [`HostServices`](super::super::HostServices). Не-object args остаются
/// как есть (host-call'ы с object-аргументами — контракт H2).
pub(super) fn with_plugin_id(args: &Value, plugin_id: &str) -> Value {
    let mut args = args.clone();
    if let Some(object) = args.as_object_mut() {
        object.insert(
            "_plugin_id".to_string(),
            Value::String(plugin_id.to_string()),
        );
    }
    args
}

/// Читает `PluginError` из `payload["error"]`; при сбое разбора — `protocol`.
pub(super) fn decode_error(payload: &Value) -> PluginError {
    match payload.get("error") {
        Some(value) => serde_json::from_value::<PluginError>(value.clone())
            .unwrap_or_else(|e| PluginError::protocol(format!("битый error-payload: {e}"))),
        None => PluginError::protocol("error-событие без поля error"),
    }
}

/// Разрешение, необходимое host-call'у (§4.4). `None` — вызов без permission.
pub fn required_permission(method: &str) -> Option<&'static str> {
    match method {
        "get_document_len"
        | "get_document_range"
        | "get_document_version"
        | "get_document_text" => Some("document:read"),
        "apply_edit" => Some("document:write"),
        "set_view_content" => Some("view:modify"),
        "show_message" => Some("ui:statusbar"),
        "export_html" => None,
        _ => None,
    }
}

/// Проверяет, что у плагина есть разрешение на host-call. Отказ — значение
/// [`PluginError::permission_denied`], а не паника.
pub fn check_permission(permissions: &[String], method: &str) -> Result<(), PluginError> {
    if let Some(required) = required_permission(method) {
        if !permissions.iter().any(|p| p == required) {
            return Err(PluginError::permission_denied(required));
        }
    }
    Ok(())
}
