//! Реализация [`HostServices`] поверх [`DocumentStore`] — range/delta host-API (§4.2 TZ-H2).
//!
//! Основной путь чтения — окно (`get_document_range`), а не полная копия: при модели
//! «child на плагин» копия 10 МБ умножалась бы на число процессов (F30). `apply_edit` идёт
//! единым путём `DocumentStore::apply_edit` → `rev++` (D5).

use std::sync::{Arc, Mutex};

use plugin_proto::envelope::PluginError;
use serde_json::{json, Value};

use crate::state::{DocumentId, DocumentStore};
use crate::MAX_FILE_SIZE;

use super::HostServices;

/// Потолок одного окна `get_document_range`: основной путь чтения — небольшие окна; гигантский
/// `len` раздул бы `Reply` и мог бы заблокировать запись в stdin child (§3.2 ревью).
const MAX_RANGE_BYTES: usize = plugin_proto::frame::MAX_EVENT_FRAME_BYTES;

/// Документные host-функции поверх общего [`DocumentStore`].
///
/// Держит `Arc<Mutex<DocumentStore>>` (а не `&`): host-call обслуживается синхронно в потоке
/// invocation. Инвариант — `handle` не вызывается, когда блокировка стора уже удержана
/// вызывающей стороной (§4.2 ревью).
pub struct DocumentServices {
    store: Arc<Mutex<DocumentStore>>,
}

impl DocumentServices {
    pub fn new(store: Arc<Mutex<DocumentStore>>) -> Self {
        Self { store }
    }
}

impl HostServices for DocumentServices {
    fn handle(&self, method: &str, args: Value) -> Result<Value, PluginError> {
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
                // Окно не больше потолка: защита от раздутого Reply (F27, §3.2 ревью).
                let len = usize_arg(&args, "len")?.min(MAX_RANGE_BYTES);
                let doc = store.get(&id).ok_or_else(|| unknown(&id))?;
                if start > doc.text.len() || !doc.text.is_char_boundary(start) {
                    return Err(PluginError::new(
                        "invalid_range",
                        format!("get_document_range: start={start} вне текста"),
                    ));
                }
                let end = start.saturating_add(len).min(doc.text.len());
                // End может попасть в середину символа — подрезаем вниз до границы.
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
                let _rev = store.apply_edit(&id, start, stop, text).ok_or_else(|| {
                    PluginError::new(
                        "invalid_range",
                        format!("apply_edit: диапазон {start}..{stop} невалиден"),
                    )
                })?;
                Ok(json!(true))
            }
            // Уведомление в статусбар/меню — UI-часть в Фазе 5; здесь принимаем и игнорируем.
            "show_message" => Ok(Value::Null),
            // Настройки плагина (§12) скоупируются по plugin_id; наполнение — Фаза 5.
            "get_setting" | "set_setting" => Ok(Value::Null),
            other => Err(PluginError::protocol(format!(
                "неизвестный host-call: {other}"
            ))),
        }
    }
}

fn doc_id(args: &Value) -> Result<DocumentId, PluginError> {
    let raw = args
        .get("doc_id")
        .and_then(Value::as_str)
        .ok_or_else(|| PluginError::new("bad_args", "нет строкового doc_id"))?;
    Ok(DocumentId::new(raw))
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
