//! Tauri-шелл darmark: тонкая обёртка над md-core + владение состоянием документов.
//!
//! Рендер Markdown живёт в crate `md-core`; здесь — IPC, файловый ввод-вывод и
//! `DocumentStore` (D5: текст, ревизия, путь и кэш рендера принадлежат хосту).
//! Путь выбирает пользователь в нативном диалоге на фронтенде (модель Notepad++:
//! «белого списка» каталогов нет — диалог и есть согласие).

mod error;
// AI-ассистент: Rust-сетевой слой, настройки, скиллы (ADR-0032/0033/0034). Собирается на всех
// платформах; сетевой бэкенд — только на Windows (внутри модуля).
pub mod ai;
// MCP-сервер (ADR-0036): in-memory транспорт в процессе GUI, адаптеры к существующим
// командам/CLI. Только маршрутизация — проверки остаются в командах (TZ-MCP R3/R4).
pub mod mcp;
// Плагинная подсистема — только Windows: Job Object и CREATE_NO_WINDOW (плагинная система).
#[cfg(windows)]
pub mod plugins;
mod state;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use error::CommandError;
use state::{DocMeta, DocumentId, DocumentSnapshot, DocumentStore, RenderResult, UndoResult};

/// Максимальный размер файла, который разрешено открывать (10 МБ).
/// Единственная проверка при чтении — защита от чтения гигантских файлов.
pub(crate) const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// Чтение файла (UTF-8) без обращения к Tauri — ядро для юнит-тестов.
///
/// Отсутствие файла и битый UTF-8 — разные коды: фронт по ним решает, что показать.
fn read_text(path: &Path) -> Result<String, CommandError> {
    let metadata = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(CommandError::not_found(format!("{}: {e}", path.display())));
        }
        Err(e) => return Err(CommandError::io(format!("{}: {e}", path.display()))),
    };
    if metadata.len() > MAX_FILE_SIZE {
        return Err(CommandError::too_large(format!(
            "Файл больше {} МБ — открытие отменено",
            MAX_FILE_SIZE / (1024 * 1024)
        )));
    }
    let bytes =
        std::fs::read(path).map_err(|e| CommandError::io(format!("{}: {e}", path.display())))?;
    String::from_utf8(bytes).map_err(|_| {
        CommandError::not_utf8(format!(
            "{}: файл не является корректным UTF-8",
            path.display()
        ))
    })
}

/// Запись файла (UTF-8) без обращения к Tauri — ядро для юнит-тестов.
fn write_text(path: &Path, contents: &str) -> Result<(), CommandError> {
    std::fs::write(path, contents).map_err(|e| CommandError::io(format!("{}: {e}", path.display())))
}

/// Разрешает каталогу файла (и подкаталогам) читаться через asset-протокол.
///
/// Модель Notepad++: путь выбрал пользователь в нативном диалоге — это и есть
/// согласие. `recursive = true`, чтобы работали картинки в подпапках (`images/…`).
/// Ошибку логируем и не роняем команду: картинки — не критичный путь.
fn allow_asset_dir(app: &tauri::AppHandle, dir: &Path) {
    use tauri::Manager;
    if let Err(e) = app.asset_protocol_scope().allow_directory(dir, true) {
        eprintln!("asset scope для {}: {e}", dir.display());
    }
}

fn lock_store<'a>(
    store: &'a tauri::State<'_, Arc<Mutex<DocumentStore>>>,
) -> std::sync::MutexGuard<'a, DocumentStore> {
    // Под `panic = "abort"` отравление мьютекса невозможно: паника абортит процесс.
    store
        .lock()
        .expect("стор документов не должен быть отравлен")
}

/// Имя файла из пути для `document-event` (S11, ТЗ §4.6).
///
/// В журнал фронтенда идёт **только имя** файла, не полный путь (приватность).
/// Документ без пути (новый) → `None`.
fn file_name(path: Option<&str>) -> Option<String> {
    path.and_then(|p| {
        Path::new(p)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
    })
}

/// Информация о плагинном тир-1 представлении — контракт с фронтендом.
///
/// Объявлена платформенно-нейтрально (вне `plugins`): команда `plugin_views`
/// должна компилироваться и на не-Windows. `serde` — snake_case по умолчанию.
#[derive(Clone, Debug, serde::Serialize)]
pub struct PluginViewInfo {
    pub view_id: String,
    pub plugin_id: String,
    pub kind: String,
    pub title: String,
    pub html: String,
}

/// Статус плагина для менеджера UI (§4.7 плагинной системы).
///
/// Объявлен платформенно-нейтрально (вне `plugins`): команда `list_plugins` должна
/// компилироваться и на не-Windows. Сериализуется как `{"state":"active"}` и т.п.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PluginStatus {
    /// Остановлен (выключен пользователем или ещё не запускался).
    Stopped,
    /// Загружен и активирован.
    Active,
    /// Отказ загрузки/активации; сообщение для UI.
    Failed { message: String },
    /// Автоотключён после серии падений (карантин).
    Quarantined,
}

/// Команда, объявленная плагином (`contributes.commands`), — контракт с фронтендом.
#[derive(Clone, Debug, serde::Serialize)]
pub struct CommandInfo {
    pub id: String,
    pub title: String,
    /// Клавиатурная привязка; сериализуется как `null`, если не задана.
    pub keybinding: Option<String>,
}

/// Проекция плагина для менеджера UI (IPC-команда `list_plugins`, Фаза 5).
///
/// Платформенно-нейтральна: на не-Windows команда возвращает пустой список.
#[derive(Clone, Debug, serde::Serialize)]
pub struct PluginInfo {
    pub id: String,
    /// Человекочитаемое имя плагина из манифеста (S12a); фолбэк на `id` — на стороне фронта.
    pub name: String,
    /// Версия плагина из манифеста (S12a).
    pub version: String,
    pub status: PluginStatus,
    /// Запрашиваемые плагином права из манифеста (декларация).
    pub permissions: Vec<String>,
    /// Согласованные пользователем права фактически (подмножество `permissions`, Ф5).
    /// Пусто — согласие не выдано (deny-by-default).
    pub granted_permissions: Vec<String>,
    pub enabled: bool,
    pub commands: Vec<CommandInfo>,
    /// Формулировки границы изоляции/доступа к документу (F38).
    pub notices: Vec<String>,
    /// Схема настроек из манифеста (`contributes.settings`) для UI-формы (S5b);
    /// `null`, если плагин настроек не объявил.
    pub settings_schema: Option<serde_json::Value>,
    /// Текущие сохранённые значения настроек плагина (JSON-объект) для UI-формы (S5b);
    /// пустой объект, если настроек нет. Обогащается хостом из `PluginSettingsStore`.
    pub settings_values: serde_json::Value,
}

/// Платформенно-нейтральное состояние плагинной подсистемы для Tauri-команд.
///
/// На Windows содержит реальный `plugins::host::PluginHost`; на прочих
/// платформах — пустую заглушку, чтобы единый `generate_handler!` собирался
/// (модуль `plugins` включается только под Windows).
pub struct PluginState {
    #[cfg(windows)]
    host: plugins::host::PluginHost,
}

impl PluginState {
    #[cfg(not(windows))]
    fn new() -> Self {
        Self {}
    }

    /// Создаёт/изменяет документ, публикует событие и доставляет его плагинам.
    ///
    /// На не-Windows — no-op: плагинной подсистемы нет.
    fn publish_document_changed(&self, doc_id: &str, rev: u64) {
        #[cfg(windows)]
        {
            self.host.publish_document_changed(doc_id, rev);
            self.host.pump();
        }
        #[cfg(not(windows))]
        {
            let _ = (doc_id, rev);
        }
    }

    fn publish_opened(&self, doc_id: &str, path: Option<&str>) {
        #[cfg(windows)]
        {
            self.host.publish_opened(doc_id, path);
            self.host.pump();
        }
        #[cfg(not(windows))]
        {
            let _ = (doc_id, path);
        }
    }

    fn publish_closed(&self, doc_id: &str) {
        #[cfg(windows)]
        {
            self.host.publish_closed(doc_id);
            self.host.pump();
        }
        #[cfg(not(windows))]
        {
            let _ = doc_id;
        }
    }

    /// Публикует `document-event` (S11) — действие пользователя над документом.
    ///
    /// Событие идёт в журнал фронтенда (`logPanel`): `new`/`open`/`save`/`close`.
    /// Доставку в Tauri-событие `document-event` обеспечивает sink из [`setup_plugins`].
    /// На не-Windows — no-op: плагинной подсистемы нет.
    fn publish_document_event(&self, kind: &str, path: Option<&str>, rev: Option<u64>) {
        #[cfg(windows)]
        {
            self.host.publish_document_event(kind, path, rev);
        }
        #[cfg(not(windows))]
        {
            let _ = (kind, path, rev);
        }
    }
}

/// Sink ресурс-монитора: конвертирует снимок/событие завершения в Tauri-события
/// (`resource-metrics`/`plugin-process-exited`, ADR-0031 §2.4 п.15).
///
/// Best-effort: сбой `emit` логируется и не роняет монитор. Сам модуль `resource` про Tauri
/// не знает — адаптер живёт здесь, в `setup_plugins`.
#[cfg(windows)]
struct TauriResourceSink {
    handle: tauri::AppHandle,
}

#[cfg(windows)]
impl plugins::resource::ResourceSink for TauriResourceSink {
    fn metrics(&self, snapshot: &plugins::resource::ResourceSnapshot) {
        use tauri::Emitter;
        if let Err(e) = self.handle.emit("resource-metrics", snapshot) {
            eprintln!("не эмитить resource-metrics: {e}");
        }
    }

    fn process_exited(&self, plugin_id: &str, pid: u32, reason: Option<&str>) {
        use tauri::Emitter;
        let mut payload = serde_json::json!({ "plugin_id": plugin_id, "pid": pid });
        if let Some(reason) = reason {
            payload["reason"] = serde_json::Value::String(reason.to_string());
        }
        if let Err(e) = self.handle.emit("plugin-process-exited", payload) {
            eprintln!("не эмитить plugin-process-exited: {e}");
        }
    }
}

// ---------- команды документов (1:1 с будущими host-функциями плагинов, §4.3) ----------
/// Создаёт безымянный документ (`text` — стартовый текст или пусто).
#[tauri::command(async)]
fn new_document(
    text: Option<String>,
    store: tauri::State<'_, Arc<Mutex<DocumentStore>>>,
    plugins: tauri::State<'_, PluginState>,
) -> DocumentSnapshot {
    let snapshot = lock_store(&store).create(text.unwrap_or_default());
    // Guard стора уже отпущен: pump может обслужить host-call к DocumentStore.
    plugins.publish_opened(snapshot.id.as_str(), snapshot.path.as_deref());
    // §4.6: в журнал идёт только имя файла, не полный путь (приватность).
    let name = file_name(snapshot.path.as_deref());
    plugins.publish_document_event("new", name.as_deref(), Some(snapshot.rev));
    snapshot
}

/// Открывает файл с диска и регистрирует его в сторе.
#[tauri::command]
async fn open_document(
    path: PathBuf,
    app: tauri::AppHandle,
    store: tauri::State<'_, Arc<Mutex<DocumentStore>>>,
    plugins: tauri::State<'_, PluginState>,
) -> Result<DocumentSnapshot, CommandError> {
    let read_path = path.clone();
    let text = tauri::async_runtime::spawn_blocking(move || read_text(&read_path))
        .await
        .map_err(|e| CommandError::io(e.to_string()))??;
    if let Some(dir) = path.parent() {
        allow_asset_dir(&app, dir);
    }
    let snapshot = lock_store(&store).insert_loaded(path, text);
    plugins.publish_opened(snapshot.id.as_str(), snapshot.path.as_deref());
    // §4.6: в журнал идёт только имя файла, не полный путь (приватность).
    let name = file_name(snapshot.path.as_deref());
    plugins.publish_document_event("open", name.as_deref(), Some(snapshot.rev));
    Ok(snapshot)
}

/// Сохраняет документ в файл. Текст берётся из стора (владелец — Rust, D5);
/// явный `path` (Save As) приоритетнее запомненного.
#[tauri::command]
async fn save_document(
    id: DocumentId,
    path: Option<PathBuf>,
    app: tauri::AppHandle,
    store: tauri::State<'_, Arc<Mutex<DocumentStore>>>,
    plugins: tauri::State<'_, PluginState>,
) -> Result<DocMeta, CommandError> {
    // Сначала читаем путь и текст, не мутируя стор: при ошибке записи стор не «уплывёт».
    let explicit_path = path;
    let (target, body) = {
        let guard = lock_store(&store);
        let doc = guard
            .get(&id)
            .ok_or_else(|| CommandError::unknown_document(id.as_str()))?;
        let target = match &explicit_path {
            Some(p) => p.clone(),
            None => doc
                .path
                .clone()
                .ok_or_else(|| CommandError::io("Не задан путь сохранения"))?,
        };
        (target, doc.text.clone())
    };

    let write_path = target.clone();
    tauri::async_runtime::spawn_blocking(move || write_text(&write_path, &body))
        .await
        .map_err(|e| CommandError::io(e.to_string()))??;
    if let Some(dir) = target.parent() {
        allow_asset_dir(&app, dir);
    }

    // Запись удалась — только теперь фиксируем путь в сторе.
    let meta = {
        let mut guard = lock_store(&store);
        let doc = guard
            .get_mut(&id)
            .ok_or_else(|| CommandError::unknown_document(id.as_str()))?;
        if let Some(p) = explicit_path {
            doc.path = Some(p);
        }
        doc.meta()
    };
    // Guard отпущен: публикуем `document-event` (S11) для журнала.
    // §4.6: в журнал идёт только имя файла, не полный путь (приватность).
    let name = file_name(meta.path.as_deref());
    plugins.publish_document_event("save", name.as_deref(), Some(meta.rev));
    Ok(meta)
}

/// Закрывает документ (появляется вместе с панелями; в UI пока не вызывается).
#[tauri::command(async)]
fn close_document(
    id: DocumentId,
    store: tauri::State<'_, Arc<Mutex<DocumentStore>>>,
    plugins: tauri::State<'_, PluginState>,
) {
    // Путь нужен для `document-event` (S11): после `close` стор его уже не помнит.
    let path = {
        let guard = lock_store(&store);
        guard
            .get(&id)
            .and_then(|doc| doc.path.as_ref().map(|p| p.to_string_lossy().into_owned()))
    };
    let closed = lock_store(&store).close(&id);
    if closed {
        plugins.publish_closed(id.as_str());
        // §4.6: в журнал идёт только имя файла, не полный путь (приватность).
        let name = file_name(path.as_deref());
        plugins.publish_document_event("close", name.as_deref(), None);
    }
}

/// Применяет правку текста: обновляет стор (`rev` растёт только при смене), рендерит.
///
/// Заменяет stateless `render_markdown`: рендер и кэш `(mapped, rev)` — теперь в сторе,
/// а `RenderResult.changed` избавляет TS от собственного `lastRenderedHtml`.
#[tauri::command(async)]
fn update_document(
    id: DocumentId,
    text: String,
    mapped: Option<bool>,
    store: tauri::State<'_, Arc<Mutex<DocumentStore>>>,
    plugins: tauri::State<'_, PluginState>,
) -> Result<RenderResult, CommandError> {
    let (result, rev_changed) = {
        let mut guard = lock_store(&store);
        let prev = guard.get(&id).map(|doc| doc.rev);
        let result = guard
            .update(&id, text, mapped.unwrap_or(false))
            .ok_or_else(|| CommandError::unknown_document(id.as_str()))?;
        let rev_changed = prev != Some(result.rev);
        (result, rev_changed)
    };
    // Guard отпущен до pump: событие доставляется плагинам, их host-call'ы берут стор.
    // Событие шлём только при реальной смене ревизии (эхо-защита, §7 риски).
    if rev_changed {
        plugins.publish_document_changed(id.as_str(), result.rev);
    }
    Ok(result)
}

/// Отменяет последнюю правку документа (Rust-undo, D5).
///
/// `rev` растёт (кэш рендера инвалидируется по ревизии), поэтому плагины уведомляются
/// `document:changed` — как и при обычной правке. Неизвестный документ →
/// `unknown_document`; пустой undo-стек → `nothing_to_undo`.
#[tauri::command(async)]
fn undo_document(
    id: DocumentId,
    store: tauri::State<'_, Arc<Mutex<DocumentStore>>>,
    plugins: tauri::State<'_, PluginState>,
) -> Result<UndoResult, CommandError> {
    let result = {
        let mut guard = lock_store(&store);
        // Различаем «нет документа» и «нечего отменять»: `undo` возвращает `None` в обоих.
        if guard.get(&id).is_none() {
            return Err(CommandError::unknown_document(id.as_str()));
        }
        guard.undo(&id).ok_or_else(CommandError::nothing_to_undo)?
    };
    // Guard отпущен до pump: событие доставляется плагинам, их host-call'ы берут стор.
    plugins.publish_document_changed(id.as_str(), result.rev);
    Ok(result)
}

/// Повторяет отменённую правку документа (Rust-undo, D5).
///
/// Неизвестный документ → `unknown_document`; пустой redo-стек → `nothing_to_redo`.
#[tauri::command(async)]
fn redo_document(
    id: DocumentId,
    store: tauri::State<'_, Arc<Mutex<DocumentStore>>>,
    plugins: tauri::State<'_, PluginState>,
) -> Result<UndoResult, CommandError> {
    let result = {
        let mut guard = lock_store(&store);
        if guard.get(&id).is_none() {
            return Err(CommandError::unknown_document(id.as_str()));
        }
        guard.redo(&id).ok_or_else(CommandError::nothing_to_redo)?
    };
    plugins.publish_document_changed(id.as_str(), result.rev);
    Ok(result)
}

/// Рендерит документ без правки текста (смена `mapped`, первый рендер после open/new).
///
/// Событий не публикует: ревизия не меняется.
#[tauri::command]
fn render_document(
    id: DocumentId,
    mapped: Option<bool>,
    store: tauri::State<'_, Arc<Mutex<DocumentStore>>>,
) -> Result<RenderResult, CommandError> {
    lock_store(&store)
        .render(&id, mapped.unwrap_or(false))
        .ok_or_else(|| CommandError::unknown_document(id.as_str()))
}

/// Возвращает снапшот документа для фронтенда (pull после события `document-updated`).
#[tauri::command]
fn document_snapshot(
    id: DocumentId,
    store: tauri::State<'_, Arc<Mutex<DocumentStore>>>,
) -> Option<DocumentSnapshot> {
    lock_store(&store).get(&id).map(|doc| doc.snapshot())
}

/// Возвращает список всех открытых документов в порядке открытия (мультивкладки, п.11.1.1).
#[tauri::command]
fn list_documents(store: tauri::State<'_, Arc<Mutex<DocumentStore>>>) -> Vec<DocMeta> {
    lock_store(&store).list()
}

/// Устанавливает активный документ (мультивкладки, п.11.1.1).
///
/// Пока активный документ также хранится на TS-стороне — команда закладывает
/// контракт для будущего перехода состояния в Rust. Возвращает ошибку,
/// если документ не найден.
#[tauri::command]
fn activate_document(
    id: DocumentId,
    store: tauri::State<'_, Arc<Mutex<DocumentStore>>>,
) -> Result<(), CommandError> {
    lock_store(&store)
        .activate(&id)
        .then_some(())
        .ok_or_else(|| CommandError::unknown_document(id.as_str()))
}

// ---------- команды плагинных представлений (Фаза 4, контракт с фронтендом) ----------

/// Снимок плагинных тир-1 представлений (пусто, если плагинов нет/не-Windows).
#[tauri::command]
fn plugin_views(state: tauri::State<'_, PluginState>) -> Vec<PluginViewInfo> {
    #[cfg(windows)]
    {
        state.host.plugin_views()
    }
    #[cfg(not(windows))]
    {
        let _ = state;
        Vec::new()
    }
}

/// Обратная маршрутизация из тир-1 view: `{view_id, action, payload}` → `on_action`
/// плагина. Возвращаемое значение — ответ Lua-хендлера (обычно `null`).
#[tauri::command(async)]
fn plugin_view_action(
    view_id: String,
    action: String,
    payload: Option<serde_json::Value>,
    state: tauri::State<'_, PluginState>,
) -> Result<serde_json::Value, CommandError> {
    #[cfg(windows)]
    {
        state
            .host
            .plugin_view_action(&view_id, &action, payload)
            .map_err(|error| CommandError::new(error::ErrorCode::Plugin, error.message))
    }
    #[cfg(not(windows))]
    {
        let _ = (view_id, action, payload, state);
        Err(CommandError::new(
            error::ErrorCode::Plugin,
            "плагинная подсистема доступна только на Windows",
        ))
    }
}

// ---------- команды менеджера плагинов (Фаза 5, контракт с фронтендом) ----------

/// Снимок реестра плагинов (пусто, если плагинов нет/не-Windows).
#[tauri::command]
fn list_plugins(state: tauri::State<'_, PluginState>) -> Vec<PluginInfo> {
    #[cfg(windows)]
    {
        state.host.list_plugins()
    }
    #[cfg(not(windows))]
    {
        let _ = state;
        Vec::new()
    }
}

/// Включает/выключает плагин; включение снимает карантин (§4.7).
#[tauri::command(async)]
fn set_plugin_enabled(
    id: String,
    enabled: bool,
    state: tauri::State<'_, PluginState>,
) -> Result<(), CommandError> {
    #[cfg(windows)]
    {
        state
            .host
            .set_enabled(&id, enabled)
            .map_err(|error| CommandError::new(error::ErrorCode::Plugin, error.message))
    }
    #[cfg(not(windows))]
    {
        let _ = (id, enabled, state);
        Err(plugin_unavailable())
    }
}

/// Перезагружает плагин с диска без рестарта приложения (§4.7).
#[tauri::command(async)]
fn reload_plugin(id: String, state: tauri::State<'_, PluginState>) -> Result<(), CommandError> {
    #[cfg(windows)]
    {
        state
            .host
            .reload(&id)
            .map_err(|error| CommandError::new(error::ErrorCode::Plugin, error.message))
    }
    #[cfg(not(windows))]
    {
        let _ = (id, state);
        Err(plugin_unavailable())
    }
}

/// Удаляет плагин: останавливает child, снимает views, удаляет настройки и каталог
/// с диска.
#[tauri::command(async)]
fn remove_plugin(id: String, state: tauri::State<'_, PluginState>) -> Result<(), CommandError> {
    #[cfg(windows)]
    {
        state
            .host
            .remove(&id)
            .map_err(|error| CommandError::new(error::ErrorCode::Plugin, error.message))
    }
    #[cfg(not(windows))]
    {
        let _ = (id, state);
        Err(plugin_unavailable())
    }
}

/// Фиксирует согласие пользователя на права плагина (consent как реальный гейт, §11.1 п.4).
///
/// Сохраняется только `manifest ∩ granted` (deny-by-default); активный плагин
/// перезапускается, чтобы новый набор прав вступил в силу.
#[tauri::command(async)]
fn set_plugin_permissions(
    id: String,
    granted: Vec<String>,
    state: tauri::State<'_, PluginState>,
) -> Result<(), CommandError> {
    #[cfg(windows)]
    {
        state
            .host
            .set_permissions(&id, granted)
            .map_err(|error| CommandError::new(error::ErrorCode::Plugin, error.message))
    }
    #[cfg(not(windows))]
    {
        let _ = (id, granted, state);
        Err(plugin_unavailable())
    }
}

/// Записывает значение настройки плагина из UI-формы (S5b).
///
/// В отличие от host-call `set_setting` (scope — внедрённый `Supervisor` `_plugin_id`),
/// здесь значение приходит от пользователя из интерфейса, поэтому владелец задаётся
/// параметром `id` напрямую. Запись идёт через `PluginSettingsStore` хоста; менеджер
/// получает `plugins-changed` и перечитывает `list_plugins`, показывая новое значение.
#[tauri::command(async)]
fn set_plugin_setting(
    id: String,
    key: String,
    value: serde_json::Value,
    state: tauri::State<'_, PluginState>,
) -> Result<(), CommandError> {
    #[cfg(windows)]
    {
        state
            .host
            .plugin_setting_set(&id, &key, value)
            .map_err(|error| CommandError::new(error::ErrorCode::Plugin, error.message))
    }
    #[cfg(not(windows))]
    {
        let _ = (id, key, value, state);
        Err(plugin_unavailable())
    }
}

/// Исполняет команду плагина: публикует `command:invoked{command_id, doc_id}` в шину.
///
/// `doc_id` — текущий документ: у плагина нет собственного доступа к «активному»
/// документу, поэтому команда несёт его с собой (Фаза 5).
#[tauri::command(async)]
fn run_plugin_command(
    command_id: String,
    store: tauri::State<'_, Arc<Mutex<DocumentStore>>>,
    state: tauri::State<'_, PluginState>,
) -> Result<(), CommandError> {
    #[cfg(windows)]
    {
        // Guard стора отпущен до pump: доставка события может обслужить host-call к стору.
        let doc_id = lock_store(&store)
            .last_id()
            .map(|id| id.as_str().to_string());
        state
            .host
            .run_plugin_command(&command_id, doc_id.as_deref());
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (command_id, store, state);
        Err(plugin_unavailable())
    }
}

/// Dev-only: открыть DevTools WebView2 программно.
///
/// Акселераторы браузера отключены настройкой WebView2 (см.
/// `disable_browser_accelerator_keys`), поэтому DevTools-хоткеи (F12 / Ctrl+Shift+C)
/// не работают — даём свой вход (Ctrl+Shift+I) с фронтенда. Метод доступен в
/// debug-сборке; в release — no-op, чтобы не тянуть фичу `devtools` и не растить бюджет D6.
#[tauri::command]
fn open_devtools(window: tauri::WebviewWindow) {
    #[cfg(debug_assertions)]
    {
        window.open_devtools();
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = window;
    }
}

/// Ошибка «плагинная подсистема доступна только на Windows» (не-Windows заглушки).
#[cfg(not(windows))]
fn plugin_unavailable() -> CommandError {
    CommandError::new(
        error::ErrorCode::Plugin,
        "плагинная подсистема доступна только на Windows",
    )
}

// ---------- dev-режим `--debug-plugin` (Фаза 6) ----------

/// Разбирает аргументы процесса и, если передан `--debug-plugin <path>`, исполняет dev-режим.
///
/// Возвращает `Some(код)` только для dev-режима — `main` завершает процесс этим кодом.
/// `None` — обычный запуск GUI: любые другие аргументы (в т.ч. `--help`) игнорируются,
/// как и раньше, и не ломают запуск.
///
/// В релизе `darmark` — GUI-подсистема (`windows_subsystem = "windows"`), консоли нет,
/// поэтому dev-режим рассчитан на debug-сборку (`cargo run -p darmark -- --debug-plugin …`).
pub fn maybe_run_debug_plugin() -> Option<i32> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--debug-plugin" {
            // Следующий аргумент считается путём, только если не похож на другой флаг.
            let Some(path) = args.next().filter(|p| !p.starts_with('-')) else {
                eprintln!("darmark: --debug-plugin требует путь к каталогу плагина или .lua-файлу");
                return Some(2);
            };
            return Some(run_debug_plugin(PathBuf::from(path)));
        }
    }
    None
}

/// Запускает child-хост с `--serve-plugin <path>` и **унаследованным** stdio; ждёт завершения.
///
/// Host-call'ы плагина обслуживает разработчик (его stdio), GUI и Job Object не участвуют:
/// это отладочный прогон плагина без помещения в `%APPDATA%` (§7.1 «Dev»). В отличие от
/// [`plugins::supervisor::Supervisor`], здесь нет `CREATE_NO_WINDOW` и пайпов — консоль
/// нужна, чтобы видеть лог child'а.
#[cfg(windows)]
fn run_debug_plugin(path: PathBuf) -> i32 {
    use std::process::{Command, Stdio};

    let Some(exe) = plugins::supervisor::Supervisor::resolve_child_exe() else {
        eprintln!(
            "darmark: {} не найден рядом с darmark.exe",
            plugins::supervisor::CHILD_EXE_NAME
        );
        return 1;
    };

    match Command::new(&exe)
        .arg("--serve-plugin")
        .arg(&path)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
    {
        Ok(status) => status.code().unwrap_or(1),
        Err(e) => {
            eprintln!("darmark: не запустить {}: {e}", exe.display());
            1
        }
    }
}

/// Вне Windows плагинная подсистема недоступна — dev-режим сообщает об этом и падает.
#[cfg(not(windows))]
fn run_debug_plugin(_path: PathBuf) -> i32 {
    eprintln!("darmark: dev-режим --debug-plugin доступен только на Windows");
    1
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Стор в `Arc`: его же копию получает плагинная подсистема (host-call'ы к документам).
    let store = Arc::new(Mutex::new(DocumentStore::default()));
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(store)
        .setup(|app| {
            use tauri::Manager;
            #[cfg(windows)]
            {
                disable_browser_accelerator_keys(app);
                init_settings_dir();
                if let Err(e) = setup_plugins(app) {
                    // Ошибка плагинной подсистемы не роняет GUI (§4.7).
                    eprintln!("плагинная подсистема не запущена: {e}");
                    app.manage(PluginState {
                        host: plugins::host::PluginHost::empty(),
                    });
                }
            }
            #[cfg(not(windows))]
            {
                app.manage(PluginState::new());
            }
            // AI-состояние: настройки из конфига + бэкенд шва (ленивый, ADR-0034 §2.3).
            app.manage(ai::state::AiState::init());
            // MCP-сервер (ADR-0036): реестр адаптеров + протокольное ядро в state.
            // Клиент (План 2) обратится к нему через `app.state::<McpServer>()`.
            app.manage(mcp_core::server::McpServer::new(mcp::build_registry(
                app.handle(),
            )));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            new_document,
            open_document,
            update_document,
            undo_document,
            redo_document,
            render_document,
            document_snapshot,
            save_document,
            close_document,
            list_documents,
            activate_document,
            plugin_views,
            plugin_view_action,
            list_plugins,
            set_plugin_enabled,
            set_plugin_permissions,
            set_plugin_setting,
            reload_plugin,
            remove_plugin,
            run_plugin_command,
            open_devtools,
            ai::commands::ai_get_settings,
            ai::commands::ai_set_settings,
            ai::commands::ai_set_api_key,
            ai::commands::ai_list_skills,
            ai::commands::ai_chat,
            ai::commands::ai_test_connection,
            ai::doc_harness::ai_diff,
            ai::doc_harness::ai_apply_edit,
            ai::plugin_harness::ai_validate_plugin,
            ai::plugin_harness::ai_create_plugin,
            ai::plugin_harness::ai_update_plugin,
            ai::plugin_harness::ai_delete_plugin
        ])
        .run(tauri::generate_context!())
        .expect("error while running darmark");
}

/// Каталог плагинов: `%APPDATA%/darmark/plugins`, с override `DARMARK_PLUGINS_DIR`
/// для тестов/E2E (изоляция от реального профиля пользователя).
#[cfg(windows)]
fn plugins_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("DARMARK_PLUGINS_DIR") {
        return PathBuf::from(dir);
    }
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_default();
    base.join("darmark").join("plugins")
}

/// Поднимает плагинную подсистему: настройки → scan/load → notify → старт enabled.
///
/// Child-exe резолвится [`plugins::supervisor::Supervisor::resolve_child_exe`]; при
/// его отсутствии плагины не стартуют, а GUI продолжает работу.
#[cfg(windows)]
fn setup_plugins(app: &mut tauri::App) -> Result<(), String> {
    use tauri::{Emitter, Manager};
    use tauri_plugin_dialog::DialogExt;

    use plugins::host::{DocumentEventSink, PluginHost, StatusChangedSink};
    use plugins::manager::load_plugins;
    use plugins::resource::{ProcessMetrics, WinProcessMetrics};
    use plugins::services::{DocumentServices, DocumentSink, Exporter, LogSink, StatusSink};
    use plugins::settings::SettingsStore;
    use plugins::supervisor::Supervisor;
    use plugins::views::PluginViews;

    let exe = Supervisor::resolve_child_exe().ok_or_else(|| {
        format!(
            "{} не найден рядом с darmark.exe",
            plugins::supervisor::CHILD_EXE_NAME
        )
    })?;

    let config_path = SettingsStore::config_path();
    let settings = match &config_path {
        Some(path) => SettingsStore::load(path),
        None => SettingsStore::default(),
    };

    let store = app.state::<Arc<Mutex<DocumentStore>>>().inner().clone();
    let views = Arc::new(Mutex::new(PluginViews::new()));
    // Очередь отложенных document:changed от плагинных apply_edit (общая services ↔ host).
    let pending_changed: plugins::services::PendingChanged = Arc::new(Mutex::new(Vec::new()));
    // Постоянные настройки плагинов (F43/S5): per-plugin файл. Общий стор у services
    // (host-call'ы get/set_setting) и host (удаление плагина подчищает файл).
    let plugin_settings = plugins::plugin_settings::PluginSettingsStore::base_dir().map(|dir| {
        Arc::new(Mutex::new(
            plugins::plugin_settings::PluginSettingsStore::new(dir),
        ))
    });

    // Два независимых сигнала фронтенду (F41/S2): `plugin-views-changed` — сменился
    // контент/реестр представлений; `plugins-changed` — состав/статус/карантин/включение.
    // Раньше единый notify слал оба, и любой `set_view_content` заставлял менеджер
    // лишний раз звать `list_plugins`.
    let handle = app.handle().clone();
    let views_notify: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        if let Err(e) = handle.emit("plugin-views-changed", ()) {
            eprintln!("не эмитить plugin-views-changed: {e}");
        }
    });
    let plugins_handle = app.handle().clone();
    let plugins_notify: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        if let Err(e) = plugins_handle.emit("plugins-changed", ()) {
            eprintln!("не эмитить plugins-changed: {e}");
        }
    });

    // Экспорт HTML: нативный save-диалог — согласие пользователя; `filesystem:write`
    // не требуется (§6.3). Отмена → `Ok(false)`, IO-ошибка → `Err` (значением плагину).
    //
    // `blocking_save_file` сам постит показ диалога в event-loop главного потока и ждёт;
    // вызывать его нельзя с главного потока (дедлок). Поэтому диалог идёт в отдельном
    // потоке, а плагинные команды помечены `#[tauri::command(async)]` — host-call
    // обслуживается вне main, event-loop свободен и обслуживает показ.
    let export_handle = app.handle().clone();
    let exporter: Exporter = Arc::new(move |html: &str| -> Result<bool, String> {
        let handle = export_handle.clone();
        let html = html.to_string();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = match handle
                .dialog()
                .file()
                .add_filter("HTML", &["html"])
                .set_file_name("export.html")
                .blocking_save_file()
            {
                None => Ok(false), // пользователь закрыл диалог
                Some(file_path) => {
                    file_path
                        .into_path()
                        .map_err(|e| e.to_string())
                        .and_then(|path| {
                            std::fs::write(&path, &html)
                                .map(|_| true)
                                .map_err(|e| format!("{}: {e}", path.display()))
                        })
                }
            };
            let _ = tx.send(result);
        });
        rx.recv().map_err(|e| e.to_string())?
    });

    // Уведомление статусбара: событие `plugin-message{plugin_id,text}` для фронтенда.
    let message_handle = app.handle().clone();
    let status_sink: StatusSink = Arc::new(move |plugin_id: &str, text: &str| {
        if let Err(e) = message_handle.emit(
            "plugin-message",
            serde_json::json!({ "plugin_id": plugin_id, "text": text }),
        ) {
            eprintln!("не эмитить plugin-message: {e}");
        }
    });

    // Внешняя (плагинная) правка: событие `document-updated{doc_id,rev}` для фронтенда.
    // Фронт по нему тянет снапшот `document_snapshot`. Эмитится только здесь, не в
    // `update_document` — иначе правка из редактора зациклилась бы через событие.
    let document_handle = app.handle().clone();
    let document_sink: DocumentSink = Arc::new(move |doc_id: &str, rev: u64| {
        if let Err(e) = document_handle.emit(
            "document-updated",
            serde_json::json!({ "doc_id": doc_id, "rev": rev }),
        ) {
            eprintln!("не эмитить document-updated: {e}");
        }
    });

    // S11: переход статуса плагина → журнал фронтенда (`plugin-status-changed`).
    let status_handle = app.handle().clone();
    let status_changed_sink: StatusChangedSink = Arc::new(
        move |plugin_id: &str, old_state: &str, new_state: &str, message: Option<&str>| {
            let mut payload = serde_json::json!({
                "plugin_id": plugin_id,
                "old_state": old_state,
                "new_state": new_state,
            });
            if let Some(message) = message {
                payload["message"] = serde_json::Value::String(message.to_string());
            }
            if let Err(e) = status_handle.emit("plugin-status-changed", payload) {
                eprintln!("не эмитить plugin-status-changed: {e}");
            }
        },
    );

    // S11: действие пользователя над документом → `document-event` (журнал фронтенда).
    // Payload `{type, path?, rev?}`: путь/ревизия опускаются, если неизвестны.
    let document_event_handle = app.handle().clone();
    let document_event_sink: DocumentEventSink =
        Arc::new(move |kind: &str, path: Option<&str>, rev: Option<u64>| {
            let mut payload = serde_json::json!({ "type": kind });
            if let Some(path) = path {
                payload["path"] = serde_json::Value::String(path.to_string());
            }
            if let Some(rev) = rev {
                payload["rev"] = serde_json::Value::from(rev);
            }
            if let Err(e) = document_event_handle.emit("document-event", payload) {
                eprintln!("не эмитить document-event: {e}");
            }
        });

    // S11: сообщения `host.log`/`print` из Lua → `log-entry` (журнал фронтенда).
    let log_handle = app.handle().clone();
    let log_sink: LogSink = Arc::new(move |level: &str, category: &str, message: &str| {
        if let Err(e) = log_handle.emit(
            "log-entry",
            serde_json::json!({ "level": level, "category": category, "message": message }),
        ) {
            eprintln!("не эмитить log-entry: {e}");
        }
    });

    let mut services = DocumentServices::new(store)
        .with_views(Arc::clone(&views))
        .with_notify(Arc::clone(&views_notify))
        .with_pending_changed(Arc::clone(&pending_changed))
        .with_exporter(exporter)
        .with_status_sink(status_sink)
        .with_document_sink(document_sink)
        .with_log_sink(log_sink);
    if let Some(settings) = &plugin_settings {
        services = services.with_plugin_settings(Arc::clone(settings));
    }
    let services: Arc<dyn plugins::HostServices> = Arc::new(services);

    let result = load_plugins(
        &plugins_dir(),
        &exe,
        Arc::clone(&services),
        &settings,
        Arc::clone(&views),
    );
    for error in &result.errors {
        eprintln!("плагин {}: {}", error.dir.display(), error.message);
    }

    let mut host = PluginHost::new(
        result.manager,
        views,
        services,
        views_notify,
        pending_changed,
    )
    .with_plugins_notify(plugins_notify)
    .with_status_changed_sink(status_changed_sink)
    .with_document_event_sink(document_event_sink)
    .with_settings(settings, config_path);
    if let Some(settings) = &plugin_settings {
        host = host.with_plugin_settings(Arc::clone(settings));
    }
    // Best-effort старт enabled-плагинов; notify внутри.
    host.start_enabled();

    // Ресурс-монитор (ADR-0031, Фаза 2): поток опроса 2 с. Список активных PID
    // снимается коротким локом менеджера внутри потока; замеры/эмит — вне блокировок
    // (ADR-0031 §2.3 п.10). Лимит Job Object — из общего бюджета плагинов.
    let resource_provider: Arc<dyn ProcessMetrics> = Arc::new(WinProcessMetrics::new());
    let resource_sink = Arc::new(TauriResourceSink {
        handle: app.handle().clone(),
    });
    host.start_resource_monitor(
        resource_provider,
        resource_sink,
        plugin_core::limits::DEFAULT_MEM_BYTES,
    );

    app.manage(PluginState { host });
    Ok(())
}

/// Создаёт каталог `%APPDATA%/darmark/` и `config.json` при первом запуске (хвост TZ-H1 п.8).
///
/// Автозапуск enabled-плагинов и их представлений поднимает [`setup_plugins`] (Фаза 4);
/// менеджер UI — Фаза 5. Здесь важен лишь факт существования конфига, чтобы
/// `SettingsStore::load` работал предсказуемо.
#[cfg(windows)]
fn init_settings_dir() {
    use plugins::settings::SettingsStore;
    if let Some(path) = SettingsStore::config_path() {
        if !path.exists() {
            if let Err(e) = SettingsStore::default().save(&path) {
                eprintln!("не создать {}: {e}", path.display());
            }
        }
    }
}

/// Отключает браузерные акселераторы WebView2 (Ctrl+P — печать, F5, Ctrl+F …).
///
/// По умолчанию WebView2 перехватывает их на уровне движка, и JS-обработчик `keydown`
/// их не получает: `Ctrl+P` открывал печать вместо переключения предпросмотра.
/// Обычные клавиши редактирования (Ctrl+C/V/X/A/Z) не затронуты.
#[cfg(windows)]
fn disable_browser_accelerator_keys(app: &tauri::App) {
    use tauri::Manager;
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings3;
    use windows_core::Interface;

    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let _ = window.with_webview(|webview| {
        let controller = webview.controller();
        unsafe {
            // Controller → CoreWebView2 → Settings (на контроллере метода нет).
            let Ok(core) = controller.CoreWebView2() else {
                return;
            };
            let Ok(settings) = core.Settings() else {
                return;
            };
            let Ok(settings3) = settings.cast::<ICoreWebView2Settings3>() else {
                return;
            };
            let _ = settings3.SetAreBrowserAcceleratorKeysEnabled(false);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use error::ErrorCode;
    use std::fs;

    #[test]
    fn reads_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("note.md");
        fs::write(&file, "hi").unwrap();

        assert_eq!(read_text(&file).unwrap(), "hi");
    }

    #[test]
    fn rejects_file_over_size_limit() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("huge.md");
        let f = fs::File::create(&file).unwrap();
        f.set_len(MAX_FILE_SIZE + 1).unwrap();
        drop(f);

        let err = read_text(&file).unwrap_err();
        assert_eq!(err.code, ErrorCode::TooLarge);
        assert!(err.message.contains("МБ"), "unexpected error: {err}");
    }

    #[test]
    fn rejects_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let err = read_text(&dir.path().join("нет.md")).unwrap_err();
        assert_eq!(err.code, ErrorCode::NotFound);
    }

    #[test]
    fn writes_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("new.md");

        write_text(&file, "content").unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "content");
    }

    /// ТЗ §4.6: в `document-event` уходит только имя файла, не полный путь.
    #[test]
    fn file_name_strips_directory() {
        assert_eq!(file_name(None), None, "документ без пути → None");
        assert_eq!(
            file_name(Some(r"D:\notes\sub\note.md")).as_deref(),
            Some("note.md"),
            "полный путь урезается до имени файла"
        );
        assert_eq!(
            file_name(Some("note.md")).as_deref(),
            Some("note.md"),
            "имя без каталога сохраняется"
        );
    }
}
