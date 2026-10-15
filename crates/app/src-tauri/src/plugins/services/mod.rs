//! Реализация [`HostServices`](super::HostServices) поверх [`DocumentStore`] — range/delta host-API (§4.2 плагинной системы).
//!
//! Основной путь чтения — окно (`get_document_range`), а не полная копия: при модели
//! «child на плагин» копия 10 МБ умножалась бы на число процессов (F30). `apply_edit` идёт
//! единым путём `DocumentStore::apply_edit` → `rev++` (D5).

mod builder;
mod host_calls;

use std::sync::{Arc, Mutex};

use crate::state::DocumentStore;

use super::plugin_settings::PluginSettingsStore;
use super::views::PluginViews;

/// Потолок одного окна `get_document_range`: основной путь чтения — небольшие окна; гигантский
/// `len` раздул бы `Reply` и мог бы заблокировать запись в stdin child (§3.2 ревью).
const MAX_RANGE_BYTES: usize = plugin_core::frame::MAX_EVENT_FRAME_BYTES;

/// Бюджет ответа `get_document_text` **после JSON-экранирования** (F40/S1).
///
/// Гард `MAX_FILE_SIZE` смотрит сырой `text.len()`, но в кадре ответ — JSON-строка: `"`/`\`
/// дают 2 Б, управляющий символ — до 6 Б (`\u00XX`). Потолок кадра host→child —
/// [`plugin_core::frame::MAX_HOST_FRAME_BYTES`] (16 МиБ); вычитаем запас на обвязку конверта
/// (`{"msg":"reply","id":N,"result":…}`) и держим отказ ниже него.
///
/// Запас именован и намеренно консервативен: пересмотреть при изменении формата конверта
/// (добавлении полей ответа).
const REPLY_ENVELOPE_RESERVE: usize = 1024;
const MAX_TEXT_REPLY_ESCAPED: usize =
    plugin_core::frame::MAX_HOST_FRAME_BYTES - REPLY_ENVELOPE_RESERVE;

/// Очередь отложенных `document:changed` (плагинные `apply_edit`): `(doc_id, rev)`.
/// Общая между `DocumentServices` (кладёт) и `PluginHost` (дренит в EventBus).
pub type PendingChanged = Arc<Mutex<Vec<(String, u64)>>>;

/// Экспорт HTML в файл (Фаза 5). `Ok(true)` — сохранено, `Ok(false)` — пользователь
/// отменил нативный диалог, `Err` — IO-ошибка (вернётся плагину значением).
/// Сборка нативного диалога сохранения HTML. `Ok(false)` — пользователь закрыл диалог.
///
/// **Инвариант: реализация обязана блокировать не на главном потоке.** `blocking_save_file`
/// сам постит показ диалога в event-loop главного потока и ждёт, поэтому вызывать его
/// с главного потока — дедлок. Реализация в `crate::setup::plugins` диалог запускает в
/// отдельном потоке и ждёт результат через канал; плагинные команды помечены
/// `#[tauri::command(async)]`, так что host-call обслуживается вне main и event-loop
/// свободен для показа.
pub type Exporter = Arc<dyn Fn(&str) -> Result<bool, String> + Send + Sync>;

/// Канал уведомлений статусбара: `(plugin_id, text)`. Эмитит событие `plugin-message`.
pub type StatusSink = Arc<dyn Fn(&str, &str) + Send + Sync>;

/// Уведомление фронтенда о внешней (плагинной) правке: `(doc_id, rev)`. Эмитит событие
/// `document-updated`; фронт по нему тянет снапшот командой `document_snapshot`.
/// Уведомление фронтенда о внешней правке: Tauri-событие `document-updated {doc_id, rev}`.
///
/// **Инвариант: реализация не должна брать лок `DocumentStore` повторно.** Sink вызывается
/// из `handle` уже под удерживаемым guard'ом; повторный `.lock()` здесь дал бы дедлок
/// (мьютекс нереентрантен). Реализация только эмитит событие.
pub type DocumentSink = Arc<dyn Fn(&str, u64) + Send + Sync>;

/// Канал журнала (`log-entry`, S11): `(level, category, message)`. Источник — сообщения
/// `host.log`/`print` из Lua. Best-effort: без sink'а лог — no-op (не ошибка).
pub type LogSink = Arc<dyn Fn(&str, &str, &str) + Send + Sync>;

/// Документные host-функции поверх общего [`DocumentStore`].
///
/// Держит `Arc<Mutex<DocumentStore>>` (а не `&`): host-call обслуживается синхронно в потоке
/// invocation. Инвариант — `handle` не вызывается, когда блокировка стора уже удержана
/// вызывающей стороной (§4.2 ревью).
pub struct DocumentServices {
    store: Arc<Mutex<DocumentStore>>,
    /// Реестр плагинных view (Фаза 4): нужен только для `set_view_content`.
    views: Option<Arc<Mutex<PluginViews>>>,
    /// Сигнал `plugin-views-changed`: сменился контент представления. Отдельный от
    /// `plugins-changed` (F41/S2): `set_view_content` не меняет состав/статусы плагинов,
    /// поэтому не должен заставлять менеджер перечитывать `list_plugins`.
    views_notify: Option<Arc<dyn Fn() + Send + Sync>>,
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
    /// Уведомление фронтенда о внешней правке (`document-updated`). Без него — no-op:
    /// GUI-путь (без sink) не эмитит событие, чтобы не было петли IPC.
    document_sink: Option<DocumentSink>,
    /// Канал журнала (`log-entry`, S11): сообщения `host.log` из Lua. Без него — no-op.
    log_sink: Option<LogSink>,
    /// Постоянные настройки плагинов (F43/S5): per-plugin JSON-файл. Без него
    /// `get_setting`/`set_setting` возвращают `no_settings`.
    plugin_settings: Option<Arc<Mutex<PluginSettingsStore>>>,
}
