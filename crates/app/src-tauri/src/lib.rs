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
#[tauri::command]
async fn read_file(path: PathBuf) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || read_file_impl(&path))
        .await
        .map_err(|e| e.to_string())?
}

/// Записывает файл на диск (UTF-8).
#[tauri::command]
async fn write_file(path: PathBuf, contents: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || write_file_impl(&path, &contents))
        .await
        .map_err(|e| e.to_string())?
}

/// Рендерит markdown → HTML через md-core (CommonMark + GFM tables/strike/tasklist).
#[tauri::command]
fn render_markdown(markdown: String) -> String {
    md_core::to_html(&markdown)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            read_file,
            write_file,
            render_markdown
        ])
        .run(tauri::generate_context!())
        .expect("error while running mdedit");
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
