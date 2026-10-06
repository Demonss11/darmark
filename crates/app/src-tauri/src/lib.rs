//! Tauri-шелл darmark: тонкая обёртка над md-core + владение состоянием документов.
//!
//! Рендер Markdown живёт в crate `md-core`; здесь — IPC, файловый ввод-вывод и
//! `DocumentStore` (D5: текст, ревизия, путь и кэш рендера принадлежат хосту).
//! Путь выбирает пользователь в нативном диалоге на фронтенде (модель Notepad++:
//! «белого списка» каталогов нет — диалог и есть согласие).

mod error;
mod state;

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use error::CommandError;
use state::{DocMeta, DocumentId, DocumentSnapshot, DocumentStore};

/// Максимальный размер файла, который разрешено открывать (10 МБ).
/// Единственная проверка при чтении — защита от чтения гигантских файлов.
const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

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
    store: &'a tauri::State<'_, Mutex<DocumentStore>>,
) -> std::sync::MutexGuard<'a, DocumentStore> {
    // Под `panic = "abort"` отравление мьютекса невозможно: паника абортит процесс.
    store
        .lock()
        .expect("стор документов не должен быть отравлен")
}

// ---------- команды документов (1:1 с будущими host-функциями плагинов, §4.3) ----------

/// Создаёт безымянный документ (`text` — стартовый текст или пусто).
#[tauri::command]
fn new_document(
    text: Option<String>,
    store: tauri::State<'_, Mutex<DocumentStore>>,
) -> DocumentSnapshot {
    lock_store(&store).create(text.unwrap_or_default())
}

/// Открывает файл с диска и регистрирует его в сторе.
#[tauri::command]
async fn open_document(
    path: PathBuf,
    app: tauri::AppHandle,
    store: tauri::State<'_, Mutex<DocumentStore>>,
) -> Result<DocumentSnapshot, CommandError> {
    let read_path = path.clone();
    let text = tauri::async_runtime::spawn_blocking(move || read_text(&read_path))
        .await
        .map_err(|e| CommandError::io(e.to_string()))??;
    if let Some(dir) = path.parent() {
        allow_asset_dir(&app, dir);
    }
    Ok(lock_store(&store).insert_loaded(path, text))
}

/// Сохраняет документ в файл.
///
/// Владелец текста — Rust (D5), но до Фазы 2 правки принимает `update_document`,
/// поэтому текст приходит аргументом (временный мост, см. `tasks/TZ-H1.md` §3.1).
/// Явный `path` (Save As) приоритетнее запомненного в сторе.
#[tauri::command]
async fn save_document(
    id: DocumentId,
    text: String,
    path: Option<PathBuf>,
    app: tauri::AppHandle,
    store: tauri::State<'_, Mutex<DocumentStore>>,
) -> Result<DocMeta, CommandError> {
    let target = {
        let mut guard = lock_store(&store);
        let doc = guard
            .get_mut(&id)
            .ok_or_else(|| CommandError::unknown_document(id.as_str()))?;
        let target = match path {
            Some(p) => {
                doc.path = Some(p.clone());
                p
            }
            None => doc
                .path
                .clone()
                .ok_or_else(|| CommandError::io("Не задан путь сохранения"))?,
        };
        doc.text = text.clone();
        target
    };

    let write_path = target.clone();
    let body = text;
    tauri::async_runtime::spawn_blocking(move || write_text(&write_path, &body))
        .await
        .map_err(|e| CommandError::io(e.to_string()))??;
    if let Some(dir) = target.parent() {
        allow_asset_dir(&app, dir);
    }

    let guard = lock_store(&store);
    let doc = guard
        .get(&id)
        .ok_or_else(|| CommandError::unknown_document(id.as_str()))?;
    Ok(doc.meta())
}

/// Закрывает документ (появляется вместе с панелями; в UI пока не вызывается).
#[tauri::command]
fn close_document(id: DocumentId, store: tauri::State<'_, Mutex<DocumentStore>>) {
    lock_store(&store).close(&id);
}

/// Рендерит markdown → HTML через md-core (CommonMark + GFM tables/strike/tasklist).
///
/// `mapped` включает режим инспектора: топ-блоки оборачиваются в
/// `<div class="md-block" data-md="start,end">` (см. `md_core::to_html_mapped`).
/// В Фазе 2 переедет в `update_document`/`render_document` (владелец — стор).
#[tauri::command]
fn render_markdown(markdown: String, mapped: Option<bool>) -> String {
    if mapped.unwrap_or(false) {
        md_core::to_html_mapped(&markdown)
    } else {
        md_core::to_html(&markdown)
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(Mutex::new(DocumentStore::default()))
        .setup(|app| {
            #[cfg(windows)]
            disable_browser_accelerator_keys(app);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            new_document,
            open_document,
            save_document,
            close_document,
            render_markdown
        ])
        .run(tauri::generate_context!())
        .expect("error while running mdedit");
}

/// Отключает браузерные акселераторы WebView2 (Ctrl+P — печать, F5, Ctrl+F …).
///
/// По умолчанию WebView2 перехватывает их на уровне движка, и JS-обработчик
/// `keydown` их не получает: `Ctrl+P` открывал печать вместо переключения
/// предпросмотра. Обычные клавиши редактирования (Ctrl+C/V/X/A/Z) не затронуты.
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
}
