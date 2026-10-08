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

use super::views::PluginViews;
use super::HostServices;

/// Потолок одного окна `get_document_range`: основной путь чтения — небольшие окна; гигантский
/// `len` раздул бы `Reply` и мог бы заблокировать запись в stdin child (§3.2 ревью).
const MAX_RANGE_BYTES: usize = plugin_proto::frame::MAX_EVENT_FRAME_BYTES;

/// Очередь отложенных `document:changed` (плагинные `apply_edit`): `(doc_id, rev)`.
/// Общая между `DocumentServices` (кладёт) и `PluginHost` (дренит в EventBus).
pub type PendingChanged = Arc<Mutex<Vec<(String, u64)>>>;

/// Экспорт HTML в файл (Фаза 5). `Ok(true)` — сохранено, `Ok(false)` — пользователь
/// отменил нативный диалог, `Err` — IO-ошибка (вернётся плагину значением).
pub type Exporter = Arc<dyn Fn(&str) -> Result<bool, String> + Send + Sync>;

/// Канал уведомлений статусбара: `(plugin_id, text)`. Эмитит событие `plugin-message`.
pub type StatusSink = Arc<dyn Fn(&str, &str) + Send + Sync>;

/// Документные host-функции поверх общего [`DocumentStore`].
///
/// Держит `Arc<Mutex<DocumentStore>>` (а не `&`): host-call обслуживается синхронно в потоке
/// invocation. Инвариант — `handle` не вызывается, когда блокировка стора уже удержана
/// вызывающей стороной (§4.2 ревью).
pub struct DocumentServices {
    store: Arc<Mutex<DocumentStore>>,
    /// Реестр плагинных view (Фаза 4): нужен только для `set_view_content`.
    views: Option<Arc<Mutex<PluginViews>>>,
    /// Уведомление фронтенда об изменении представлений (эмит `plugin-views-changed`).
    notify: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Отложенные `document:changed` от плагинных `apply_edit`: `(doc_id, rev)`.
    ///
    /// Публиковать их в EventBus прямо здесь нельзя — `apply_edit` исполняется под
    /// блокировкой `Mutex<PluginManager>` (внутри `pump`/invoke), а публикация+повторный
    /// `pump` взяли бы тот же нереентрантный мьютекс (дедлок). Поэтому копим в очереди,
    /// а [`super::host::PluginHost::pump`] дренит её после отпускания manager.
    pending_changed: Option<PendingChanged>,
    /// Экспорт HTML через нативный диалог хоста (Фаза 5). Без него `export_html` — ошибка.
    exporter: Option<Exporter>,
    /// Уведомление статусбара (Фаза 5). Без него `show_message` — no-op.
    status_sink: Option<StatusSink>,
}

impl DocumentServices {
    pub fn new(store: Arc<Mutex<DocumentStore>>) -> Self {
        Self {
            store,
            views: None,
            notify: None,
            pending_changed: None,
            exporter: None,
            status_sink: None,
        }
    }

    /// Подключает реестр view (Фаза 4). Без него `set_view_content` возвращает ошибку.
    pub fn with_views(mut self, views: Arc<Mutex<PluginViews>>) -> Self {
        self.views = Some(views);
        self
    }

    /// Подключает колбэк уведомления фронтенда о смене контента view.
    pub fn with_notify(mut self, notify: Arc<dyn Fn() + Send + Sync>) -> Self {
        self.notify = Some(notify);
        self
    }

    /// Подключает очередь отложенных `document:changed` (плагинные `apply_edit`).
    pub fn with_pending_changed(mut self, pending: PendingChanged) -> Self {
        self.pending_changed = Some(pending);
        self
    }

    /// Подключает экспорт HTML (нативный диалог сохранения хоста, Фаза 5).
    pub fn with_exporter(mut self, exporter: Exporter) -> Self {
        self.exporter = Some(exporter);
        self
    }

    /// Подключает канал уведомлений статусбара (`plugin-message`, Фаза 5).
    pub fn with_status_sink(mut self, sink: StatusSink) -> Self {
        self.status_sink = Some(sink);
        self
    }

    /// `set_view_content`: санитизация и запись HTML в реестр представлений.
    ///
    /// Не трогает `DocumentStore` (важно для дедлок-безопасности: вызов приходит
    /// из `pump`, где блокировка стора уже отпущена). `_plugin_id` инжектится
    /// `Supervisor` — плагин не может подменить владельца view.
    fn set_view_content(&self, args: &Value) -> Result<Value, PluginError> {
        let view_id = args.get("view_id").and_then(Value::as_str).ok_or_else(|| {
            PluginError::new("bad_args", "set_view_content: нет строкового view_id")
        })?;
        let html = args
            .get("html")
            .and_then(Value::as_str)
            .ok_or_else(|| PluginError::new("bad_args", "set_view_content: нет строкового html"))?;

        // Проверка владения обязательна: `_plugin_id` инжектит `Supervisor` (плагин не может
        // подменить владельца view). Без инъекции вызов отвергается, а не «проходит без проверки».
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
        // Эмитим событие только при реальной смене HTML (§7 риски: избежать шторма IPC).
        if changed {
            if let Some(notify) = &self.notify {
                notify();
            }
        }
        Ok(json!(true))
    }

    /// `export_html`: HTML → нативный диалог сохранения (Фаза 5, §6.3).
    ///
    /// Не берёт блокировку стора: диалог может висеть до выбора пользователя, а
    /// замороженный `DocumentStore` заблокировал бы редактор. Permission не требуется —
    /// согласие даёт сам диалог (`filesystem:write` не нужен).
    fn export_html(&self, args: &Value) -> Result<Value, PluginError> {
        let html = args
            .get("html")
            .and_then(Value::as_str)
            .ok_or_else(|| PluginError::new("bad_args", "export_html: нет строкового html"))?;
        let exporter = self.exporter.as_ref().ok_or_else(|| {
            PluginError::new("no_exporter", "экспорт недоступен в этом окружении")
        })?;
        match exporter(html) {
            // Успех → true; отмена диалога → false (плагин различает исход).
            Ok(saved) => Ok(json!(saved)),
            Err(message) => Err(PluginError::new("export_failed", message)),
        }
    }

    /// `show_message`: уведомление в статусбар через событие `plugin-message`.
    ///
    /// `_plugin_id` инжектится `Supervisor` — плагин не может подменить автора сообщения.
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
}

impl HostServices for DocumentServices {
    fn handle(&self, method: &str, args: Value) -> Result<Value, PluginError> {
        // View-путь и UI-пути не берут блокировку стора: вызываются из pump под уже
        // снятым сторовым guard'ом (§4.2/Фаза 4). Экспорт открывает модальный диалог —
        // держать стор замороженным нельзя (Фаза 5). Все прочие host-call'ы — документные.
        match method {
            "set_view_content" => return self.set_view_content(&args),
            "export_html" => return self.export_html(&args),
            "show_message" => return self.show_message(&args),
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
                let prev_rev = store.get(&id).map(|doc| doc.rev);
                let rev = store.apply_edit(&id, start, stop, text).ok_or_else(|| {
                    PluginError::new(
                        "invalid_range",
                        format!("apply_edit: диапазон {start}..{stop} невалиден"),
                    )
                })?;
                // Правка изменила документ → откладываем `document:changed` для шины (§4.5/§7).
                // Публикует и доставляет его `PluginHost::pump` уже без manager-блокировки.
                if prev_rev != Some(rev) {
                    if let Some(pending) = &self.pending_changed {
                        if let Ok(mut queue) = pending.lock() {
                            queue.push((id.as_str().to_string(), rev));
                        }
                    }
                }
                Ok(json!(true))
            }
            // Настройки плагина (§12) в H2 не реализованы: скоуп по plugin_id и хранилище —
            // отдельная задача. Отвечаем явной ошибкой, а не тихим `nil` (чтобы автор плагина
            // не считал вызов успешным). Задокументировано в docs/PLUGIN_API.md/PLUGIN_GUIDE.md.
            "get_setting" | "set_setting" => Err(PluginError::new(
                "not_implemented",
                "настройки плагина (get_setting/set_setting) не реализованы в H2",
            )),
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
