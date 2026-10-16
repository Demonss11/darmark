//! Обслуживание host-call'ов плагина: маршрутизация метода, применение прав и разбор
//! аргументов. Приватные хелперы разбора живут здесь же — рядом с единственным
//! пользователем (`handle`).

use plugin_core::envelope::PluginError;
use serde_json::{json, Value};

use crate::state::DocumentId;
use crate::MAX_FILE_SIZE;

use super::super::HostServices;
use super::{DocumentServices, MAX_RANGE_BYTES, MAX_TEXT_REPLY_ESCAPED};

impl DocumentServices {
    fn set_view_content(&self, args: &Value) -> Result<Value, PluginError> {
        let view_id = args.get("view_id").and_then(Value::as_str).ok_or_else(|| {
            PluginError::new("bad_args", "set_view_content: нет строкового view_id")
        })?;
        let html = args
            .get("html")
            .and_then(Value::as_str)
            .ok_or_else(|| PluginError::new("bad_args", "set_view_content: нет строкового html"))?;
        let plugin_id = args
            .get("_plugin_id")
            .and_then(Value::as_str)
            .ok_or_else(|| PluginError::new("bad_args", "set_view_content: нет _plugin_id"))?;
        if !view_id.starts_with(&format!("{plugin_id}:")) {
            return Err(PluginError::new(
                "bad_view_id",
                format!("view_id {view_id} не принадлежит плагину {plugin_id}"),
            ));
        }

        let views = self.views.as_ref().ok_or_else(|| {
            PluginError::new(
                "no_views",
                "реестр представлений недоступен в этом окружении",
            )
        })?;
        let changed = {
            let mut views = views
                .lock()
                .map_err(|_| PluginError::new("internal", "реестр представлений отравлен"))?;
            views.set_content(view_id, html)?
        };
        if changed {
            if let Some(notify) = &self.views_notify {
                notify();
            }
        }
        Ok(json!(true))
    }

    fn export_html(&self, args: &Value) -> Result<Value, PluginError> {
        let html = args
            .get("html")
            .and_then(Value::as_str)
            .ok_or_else(|| PluginError::new("bad_args", "export_html: нет строкового html"))?;
        let exporter = self.exporter.as_ref().ok_or_else(|| {
            PluginError::new("no_exporter", "экспорт недоступен в этом окружении")
        })?;
        match exporter(html) {
            Ok(saved) => Ok(json!(saved)),
            Err(message) => Err(PluginError::new("export_failed", message)),
        }
    }

    fn show_message(&self, args: &Value) -> Result<Value, PluginError> {
        let text = args
            .get("text")
            .and_then(Value::as_str)
            .ok_or_else(|| PluginError::new("bad_args", "show_message: нет строкового text"))?;
        let plugin_id = args
            .get("_plugin_id")
            .and_then(Value::as_str)
            .ok_or_else(|| PluginError::new("bad_args", "show_message: нет _plugin_id"))?;
        if let Some(sink) = &self.status_sink {
            sink(plugin_id, text);
        }
        Ok(json!(true))
    }

    fn log(&self, args: &Value) -> Result<Value, PluginError> {
        let level = args.get("level").and_then(Value::as_str).unwrap_or("info");
        let message = args
            .get("message")
            .and_then(Value::as_str)
            .ok_or_else(|| PluginError::new("bad_args", "log: нет строкового message"))?;
        if let Some(sink) = &self.log_sink {
            sink(level, "plugin", message);
        }
        Ok(json!(true))
    }

    fn get_setting(&self, args: &Value) -> Result<Value, PluginError> {
        let plugin_id = plugin_id_arg(args)?;
        let key = key_arg(args)?;
        let store = self.plugin_settings.as_ref().ok_or_else(|| {
            PluginError::new(
                "no_settings",
                "хранилище настроек недоступно в этом окружении",
            )
        })?;
        let mut store = store
            .lock()
            .map_err(|_| PluginError::new("internal", "хранилище настроек отравлено"))?;
        Ok(store.get(plugin_id, key)?.unwrap_or(Value::Null))
    }

    fn set_setting(&self, args: &Value) -> Result<Value, PluginError> {
        let plugin_id = plugin_id_arg(args)?;
        let key = key_arg(args)?;
        let value = args
            .get("value")
            .cloned()
            .ok_or_else(|| PluginError::new("bad_args", "set_setting: нет value"))?;
        let store = self.plugin_settings.as_ref().ok_or_else(|| {
            PluginError::new(
                "no_settings",
                "хранилище настроек недоступно в этом окружении",
            )
        })?;
        let mut store = store
            .lock()
            .map_err(|_| PluginError::new("internal", "хранилище настроек отравлено"))?;
        store.set(plugin_id, key, value)?;
        Ok(json!(true))
    }
}

impl HostServices for DocumentServices {
    fn handle(&self, method: &str, args: Value) -> Result<Value, PluginError> {
        match method {
            "set_view_content" => return self.set_view_content(&args),
            "export_html" => return self.export_html(&args),
            "show_message" => return self.show_message(&args),
            "log" => return self.log(&args),
            "get_setting" => return self.get_setting(&args),
            "set_setting" => return self.set_setting(&args),
            _ => {}
        }

        let mut store = self
            .store
            .lock()
            .map_err(|_| PluginError::new("internal", "стор документов отравлен"))?;

        match method {
            "get_document_len" => {
                let id = doc_id(&args)?;
                let doc = store.get(&id).ok_or_else(|| unknown(&id))?;
                Ok(json!(doc.text.len()))
            }
            "get_document_version" => {
                let id = doc_id(&args)?;
                let doc = store.get(&id).ok_or_else(|| unknown(&id))?;
                Ok(json!(doc.rev))
            }
            "get_document_range" => {
                let id = doc_id(&args)?;
                let start = usize_arg(&args, "start")?;
                let len = usize_arg(&args, "len")?.min(MAX_RANGE_BYTES);
                let doc = store.get(&id).ok_or_else(|| unknown(&id))?;
                if start > doc.text.len() || !doc.text.is_char_boundary(start) {
                    return Err(PluginError::new(
                        "invalid_range",
                        format!("get_document_range: start={start} вне текста"),
                    ));
                }
                let end = start.saturating_add(len).min(doc.text.len());
                let mut end = end;
                while end > start && !doc.text.is_char_boundary(end) {
                    end -= 1;
                }
                Ok(json!(&doc.text[start..end]))
            }
            "get_document_text" => {
                let id = doc_id(&args)?;
                let doc = store.get(&id).ok_or_else(|| unknown(&id))?;
                if doc.text.len() as u64 > MAX_FILE_SIZE {
                    return Err(PluginError::new(
                        "too_large",
                        format!("документ больше {} МБ", MAX_FILE_SIZE / (1024 * 1024)),
                    ));
                }
                if json_escaped_len(&doc.text) > MAX_TEXT_REPLY_ESCAPED {
                    return Err(PluginError::new(
                        "too_large",
                        "документ не умещается в кадр после JSON-экранирования; \
                         используйте get_document_range",
                    ));
                }
                Ok(json!(&doc.text))
            }
            "apply_edit" => {
                let id = doc_id(&args)?;
                let start = usize_arg(&args, "start")?;
                let stop = usize_arg(&args, "stop")?;
                let text = args
                    .get("text")
                    .and_then(Value::as_str)
                    .ok_or_else(|| PluginError::new("bad_args", "apply_edit: нет text"))?;
                let prev_rev = store.get(&id).map(|doc| doc.rev);
                let rev = store.apply_edit(&id, start, stop, text).ok_or_else(|| {
                    PluginError::new(
                        "invalid_range",
                        format!("apply_edit: диапазон {start}..{stop} невалиден"),
                    )
                })?;
                if prev_rev != Some(rev) {
                    if let Some(pending) = &self.pending_changed {
                        if let Ok(mut queue) = pending.lock() {
                            queue.push((id.as_str().to_string(), rev));
                        }
                    }
                    if let Some(sink) = &self.document_sink {
                        sink(id.as_str(), rev);
                    }
                }
                Ok(json!(true))
            }
            other => Err(PluginError::protocol(format!(
                "неизвестный host-call: {other}"
            ))),
        }
    }
}

fn json_escaped_len(text: &str) -> usize {
    text.bytes()
        .map(|b| match b {
            b'"' | b'\\' => 2,
            0x00..=0x1f => 6,
            _ => 1,
        })
        .sum()
}

fn doc_id(args: &Value) -> Result<DocumentId, PluginError> {
    let raw = args
        .get("doc_id")
        .and_then(Value::as_str)
        .ok_or_else(|| PluginError::new("bad_args", "нет строкового doc_id"))?;
    Ok(DocumentId::new(raw))
}

fn plugin_id_arg(args: &Value) -> Result<&str, PluginError> {
    args.get("_plugin_id")
        .and_then(Value::as_str)
        .ok_or_else(|| PluginError::new("bad_args", "нет _plugin_id"))
}

fn key_arg(args: &Value) -> Result<&str, PluginError> {
    args.get("key")
        .and_then(Value::as_str)
        .ok_or_else(|| PluginError::new("bad_args", "нет строкового key"))
}

fn usize_arg(args: &Value, name: &str) -> Result<usize, PluginError> {
    let value = args
        .get(name)
        .and_then(Value::as_i64)
        .ok_or_else(|| PluginError::new("bad_args", format!("нет числового {name}")))?;
    usize::try_from(value).map_err(|_| PluginError::new("bad_args", format!("{name} < 0")))
}

fn unknown(id: &DocumentId) -> PluginError {
    PluginError::new(
        "unknown_document",
        format!("документ не найден: {}", id.as_str()),
    )
}
