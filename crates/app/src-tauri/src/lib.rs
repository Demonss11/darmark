//! Tauri-шелл mdedit: тонкая обёртка над md-core.
//! Вся логика Markdown живёт в crate `md-core` — здесь только IPC и окно.

use std::path::PathBuf;

/// Разрешённые расширения для открытия/сохранения.
const ALLOWED_EXTS: &[&str] = &["md", "markdown", "mdown", "mkd", "txt"];

/// Проверяет, что путь можно открыть как текстовый markdown-файл.
fn check_path_allowed(path: &PathBuf) -> Result<(), String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    if ALLOWED_EXTS.contains(&ext.as_str()) {
        Ok(())
    } else {
        Err(format!("Extension not allowed: '{ext}'"))
    }
}

/// Читает файл с диска (UTF-8). Путь ограничен capability `file-md`:
/// только пользовательские директории (docs/desktop/downloads) и расширения *.md/*.txt.
#[tauri::command]
fn read_file(path: PathBuf) -> Result<String, String> {
    check_path_allowed(&path)?;
    std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Записывает файл на диск (UTF-8). Ограничения — см. read_file.
#[tauri::command]
fn write_file(path: PathBuf, contents: String) -> Result<(), String> {
    check_path_allowed(&path)?;
    std::fs::write(&path, contents).map_err(|e| format!("{}: {e}", path.display()))
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
        .invoke_handler(tauri::generate_handler![
            read_file,
            write_file,
            render_markdown
        ])
        .run(tauri::generate_context!())
        .expect("error while running mdedit");
}
