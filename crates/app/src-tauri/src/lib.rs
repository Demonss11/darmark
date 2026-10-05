//! Tauri-шелл mdedit: тонкая обёртка над md-core.
//! Вся логика Markdown живёт в crate `md-core` — здесь только IPC: чтение/запись
//! файла и рендер. Путь выбирает пользователь в нативном диалоге на фронтенде
//! (модель Notepad++: «белого списка» каталогов нет — диалог и есть согласие).

use std::path::{Path, PathBuf};

/// Максимальный размер файла, который разрешено открывать (10 МБ).
/// Единственная проверка при чтении — защита от чтения гигантских файлов.
const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// Чтение файла (UTF-8) без обращения к Tauri — ядро для юнит-тестов.
fn read_file_impl(path: &Path) -> Result<String, String> {
    let metadata = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if metadata.len() > MAX_FILE_SIZE {
        return Err(format!(
            "Файл больше {} МБ — открытие отменено",
            MAX_FILE_SIZE / (1024 * 1024)
        ));
    }
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Запись файла (UTF-8) без обращения к Tauri — ядро для юнит-тестов.
fn write_file_impl(path: &Path, contents: &str) -> Result<(), String> {
    std::fs::write(path, contents).map_err(|e| format!("{}: {e}", path.display()))
}

/// Читает файл с диска (UTF-8). Лимит размера — см. [`MAX_FILE_SIZE`].
///
/// При успехе каталог файла разрешается в scope asset-протокола: относительные
/// картинки в предпросмотре (`./img/pic.png`) резолвятся на фронтенде в asset-URL,
/// а протокол отдаёт файл, только если его путь разрешён.
#[tauri::command]
async fn read_file(path: PathBuf, app: tauri::AppHandle) -> Result<String, String> {
    let result = tauri::async_runtime::spawn_blocking({
        let path = path.clone();
        move || read_file_impl(&path)
    })
    .await
    .map_err(|e| e.to_string())?;
    if result.is_ok() {
        if let Some(dir) = path.parent() {
            allow_asset_dir(&app, dir);
        }
    }
    result
}

/// Записывает файл на диск (UTF-8). Каталог тоже попадает в scope asset-протокола
/// (после «Сохранить как» картинки рядом с новым файлом должны отображаться).
#[tauri::command]
async fn write_file(path: PathBuf, contents: String, app: tauri::AppHandle) -> Result<(), String> {
    let result = tauri::async_runtime::spawn_blocking({
        let path = path.clone();
        move || write_file_impl(&path, &contents)
    })
    .await
    .map_err(|e| e.to_string())?;
    if result.is_ok() {
        if let Some(dir) = path.parent() {
            allow_asset_dir(&app, dir);
        }
    }
    result
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

/// Рендерит markdown → HTML через md-core (CommonMark + GFM tables/strike/tasklist).
///
/// `mapped` включает режим инспектора: топ-блоки оборачиваются в
/// `<div class="md-block" data-md="start,end">` (см. `md_core::to_html_mapped`).
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
        .setup(|app| {
            #[cfg(windows)]
            disable_browser_accelerator_keys(app);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            read_file,
            write_file,
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
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings3;
    use tauri::Manager;
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
    use std::fs;

    #[test]
    fn reads_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("note.md");
        fs::write(&file, "hi").unwrap();

        assert_eq!(read_file_impl(&file).unwrap(), "hi");
    }

    #[test]
    fn rejects_file_over_size_limit() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("huge.md");
        let f = fs::File::create(&file).unwrap();
        f.set_len(MAX_FILE_SIZE + 1).unwrap();
        drop(f);

        let err = read_file_impl(&file).unwrap_err();
        assert!(err.contains("МБ"), "unexpected error: {err}");
    }

    #[test]
    fn writes_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("new.md");

        write_file_impl(&file, "content").unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "content");
    }
}
