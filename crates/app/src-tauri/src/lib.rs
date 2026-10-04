//! Tauri-шелл mdedit: тонкая обёртка над md-core.
//! Вся логика Markdown живёт в crate `md-core` — здесь только IPC, окно и
//! проверка путей файлового доступа.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tauri::path::PathResolver;
use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;

/// Максимальный размер файла, который разрешено открывать (10 МБ).
const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// Разрешённые расширения для открытия/сохранения.
const ALLOWED_EXTS: &[&str] = &["md", "markdown", "mdown", "mkd", "txt"];

/// Каталоги, явно выбранные пользователем через нативный диалог в этой сессии.
/// Даже после «запоминания» каждый путь всё равно проходит [`validate_path`].
#[derive(Default)]
struct AllowedDirs(Mutex<HashSet<PathBuf>>);

/// Проверяет, что у пути разрешённое расширение.
fn is_allowed_ext(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .map(|e| ALLOWED_EXTS.contains(&e.as_str()))
        .unwrap_or(false)
}

/// Стандартные разрешённые корни: Documents/Desktop/Downloads.
/// Если ни одного нет (например, Unix без XDG-каталогов) — fallback на домашний каталог.
fn base_roots<R: tauri::Runtime>(resolver: &PathResolver<R>) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for dir in [
        resolver.document_dir(),
        resolver.desktop_dir(),
        resolver.download_dir(),
    ] {
        if let Ok(p) = dir {
            if let Ok(canonical) = p.canonicalize() {
                roots.push(canonical);
            }
        }
    }
    if roots.is_empty() {
        if let Ok(home) = resolver.home_dir() {
            if let Ok(canonical) = home.canonicalize() {
                roots.push(canonical);
            }
        }
    }
    roots
}

/// Полный список разрешённых корней: стандартные + выбранные через диалог.
fn allowed_roots(app: &tauri::AppHandle, state: &AllowedDirs) -> Vec<PathBuf> {
    let mut roots = base_roots(app.path());
    if let Ok(extra) = state.0.lock() {
        roots.extend(extra.iter().cloned());
    }
    roots
}

/// Приводит путь к каноническому виду и проверяет, что он лежит внутри одного
/// из разрешённых корней. Для записи в ещё не существующий файл канонализируется
/// родительский каталог. `canonicalize` раскрывает симлинки, поэтому побег наружу
/// через ссылку отсекается сравнением уже канонических путей.
fn validate_path(path: &Path, roots: &[PathBuf], for_write: bool) -> Result<PathBuf, String> {
    if !is_allowed_ext(path) {
        return Err(format!("Расширение не разрешено: {}", path.display()));
    }

    let canonical = if for_write && !path.exists() {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .ok_or_else(|| "Не удалось определить каталог файла".to_string())?;
        let canonical_parent = parent
            .canonicalize()
            .map_err(|e| format!("{}: {e}", parent.display()))?;
        let name = path
            .file_name()
            .ok_or_else(|| "Некорректное имя файла".to_string())?;
        canonical_parent.join(name)
    } else {
        path.canonicalize()
            .map_err(|e| format!("{}: {e}", path.display()))?
    };

    if roots.iter().any(|root| canonical.starts_with(root)) {
        Ok(canonical)
    } else {
        Err(format!("Путь вне разрешённых каталогов: {}", path.display()))
    }
}

/// Чтение файла без обращения к Tauri (ядро проверки — для юнит-тестов).
fn read_file_impl(path: &Path, roots: &[PathBuf]) -> Result<String, String> {
    let canonical = validate_path(path, roots, false)?;
    let metadata = std::fs::metadata(&canonical)
        .map_err(|e| format!("{}: {e}", canonical.display()))?;
    if metadata.len() > MAX_FILE_SIZE {
        return Err(format!(
            "Файл больше {} МБ — открытие отменено",
            MAX_FILE_SIZE / (1024 * 1024)
        ));
    }
    std::fs::read_to_string(&canonical).map_err(|e| format!("{}: {e}", canonical.display()))
}

/// Запись файла без обращения к Tauri (ядро проверки — для юнит-тестов).
fn write_file_impl(path: &Path, contents: &str, roots: &[PathBuf]) -> Result<(), String> {
    let canonical = validate_path(path, roots, true)?;
    std::fs::write(&canonical, contents).map_err(|e| format!("{}: {e}", canonical.display()))
}

/// Запоминает каталог выбранного файла как разрешённый на текущую сессию.
fn register_dir(state: &AllowedDirs, path: &Path) {
    if let Some(parent) = path.parent() {
        if let Ok(canonical) = parent.canonicalize() {
            if let Ok(mut set) = state.0.lock() {
                set.insert(canonical);
            }
        }
    }
}

/// Читает файл с диска (UTF-8). Путь обязан лежать в разрешённых каталогах.
#[tauri::command]
async fn read_file(
    app: tauri::AppHandle,
    state: State<'_, AllowedDirs>,
    path: PathBuf,
) -> Result<String, String> {
    let roots = allowed_roots(&app, &state);
    tauri::async_runtime::spawn_blocking(move || read_file_impl(&path, &roots))
        .await
        .map_err(|e| e.to_string())?
}

/// Записывает файл на диск (UTF-8). Ограничения — см. [`read_file`].
#[tauri::command]
async fn write_file(
    app: tauri::AppHandle,
    state: State<'_, AllowedDirs>,
    path: PathBuf,
    contents: String,
) -> Result<(), String> {
    let roots = allowed_roots(&app, &state);
    tauri::async_runtime::spawn_blocking(move || write_file_impl(&path, &contents, &roots))
        .await
        .map_err(|e| e.to_string())?
}

/// Показывает нативный диалог открытия и запоминает выбранный каталог.
/// Выбор файла делается на Rust-стороне, чтобы фронтенд не мог сам «разрешить»
/// произвольный путь в обход [`validate_path`].
#[tauri::command]
async fn pick_open_file(
    app: tauri::AppHandle,
    state: State<'_, AllowedDirs>,
) -> Result<Option<String>, String> {
    let dialog_app = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        dialog_app
            .dialog()
            .file()
            .set_title("Открыть Markdown")
            .add_filter("Markdown", ALLOWED_EXTS)
            .blocking_pick_file()
    })
    .await
    .map_err(|e| e.to_string())?;

    match picked {
        Some(file_path) => {
            let path = file_path.into_path().map_err(|e| e.to_string())?;
            register_dir(&state, &path);
            Ok(Some(path.to_string_lossy().into_owned()))
        }
        None => Ok(None),
    }
}

/// Показывает нативный диалог сохранения и запоминает выбранный каталог.
#[tauri::command]
async fn pick_save_file(
    app: tauri::AppHandle,
    state: State<'_, AllowedDirs>,
    default_path: Option<String>,
) -> Result<Option<String>, String> {
    let dialog_app = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        let mut builder = dialog_app
            .dialog()
            .file()
            .set_title("Сохранить Markdown")
            .add_filter("Markdown", ALLOWED_EXTS);
        if let Some(full) = default_path {
            let p = Path::new(&full);
            if let Some(dir) = p.parent().filter(|d| !d.as_os_str().is_empty()) {
                builder = builder.set_directory(dir);
            }
            if let Some(name) = p.file_name() {
                builder = builder.set_file_name(name.to_string_lossy());
            }
        }
        builder.blocking_save_file()
    })
    .await
    .map_err(|e| e.to_string())?;

    match picked {
        Some(file_path) => {
            let path = file_path.into_path().map_err(|e| e.to_string())?;
            register_dir(&state, &path);
            Ok(Some(path.to_string_lossy().into_owned()))
        }
        None => Ok(None),
    }
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
        .manage(AllowedDirs::default())
        .invoke_handler(tauri::generate_handler![
            read_file,
            write_file,
            render_markdown,
            pick_open_file,
            pick_save_file
        ])
        .run(tauri::generate_context!())
        .expect("error while running mdedit");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Корни — канонический tempdir (на Windows без `\\?\`-префикса сравнение ломается).
    fn roots_of(dir: &Path) -> Vec<PathBuf> {
        vec![dir.canonicalize().unwrap()]
    }

    #[test]
    fn allows_file_inside_root() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("note.md");
        fs::write(&file, "hi").unwrap();

        assert!(validate_path(&file, &roots_of(dir.path()), false).is_ok());
        assert_eq!(read_file_impl(&file, &roots_of(dir.path())).unwrap(), "hi");
    }

    #[test]
    fn rejects_parent_traversal_outside_root() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("sub");
        fs::create_dir(&sub).unwrap();
        let secret = dir.path().join("secret.md");
        fs::write(&secret, "secret").unwrap();

        // sub/../secret.md канонализируется в secret.md — вне корня sub.
        let escaped = sub.join("..").join("secret.md");
        assert!(validate_path(&escaped, &roots_of(&sub), false).is_err());
        assert!(read_file_impl(&escaped, &roots_of(&sub)).is_err());
    }

    #[test]
    fn rejects_disallowed_extension() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("note.exe");
        fs::write(&file, "x").unwrap();

        assert!(validate_path(&file, &roots_of(dir.path()), false).is_err());
    }

    #[test]
    fn rejects_file_over_size_limit() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("huge.md");
        let f = fs::File::create(&file).unwrap();
        f.set_len(MAX_FILE_SIZE + 1).unwrap();
        drop(f);

        let err = read_file_impl(&file, &roots_of(dir.path())).unwrap_err();
        assert!(err.contains("МБ"), "unexpected error: {err}");
    }

    #[test]
    fn writes_new_file_inside_root() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("new.md");

        write_file_impl(&file, "content", &roots_of(dir.path())).unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "content");
    }

    #[test]
    fn rejects_write_outside_root() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("sub");
        fs::create_dir(&sub).unwrap();
        let outside = dir.path().join("outside.md");

        assert!(write_file_impl(&outside, "x", &roots_of(&sub)).is_err());
    }

    /// Симлинк наружу не должен обходить белый список. На Windows создание
    /// симлинков требует прав администратора — тест тихо пропускается без них.
    #[test]
    fn rejects_symlink_escaping_root() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("sub");
        fs::create_dir(&sub).unwrap();
        let target = dir.path().join("target.md");
        fs::write(&target, "secret").unwrap();
        let link = sub.join("link.md");

        if create_file_symlink(&target, &link).is_err() {
            return; // нет прав на симлинки — пропускаем
        }

        assert!(validate_path(&link, &roots_of(&sub), false).is_err());
    }

    #[cfg(windows)]
    fn create_file_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
        std::os::windows::fs::symlink_file(target, link)
    }

    #[cfg(unix)]
    fn create_file_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
        std::os::unix::fs::symlink(target, link)
    }
}
