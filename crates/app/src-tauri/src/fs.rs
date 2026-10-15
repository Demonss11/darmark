//! Файловый ввод-вывод шелла без обращения к Tauri: чтение/запись текста, asset-scope
//! и имя файла для `document-event`.
//!
//! Функции не знают про `tauri::App` и `tauri::State` — их ядро юнит-тестируется напрямую,
//! поэтому проверки лимита и коды ошибок живут здесь, а не в командах.

use std::path::Path;

use crate::error::CommandError;

/// Максимальный размер файла, который разрешено открывать (10 МБ).
/// Единственная проверка при чтении — защита от чтения гигантских файлов.
pub(crate) const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// Чтение файла (UTF-8) без обращения к Tauri — ядро для юнит-тестов.
///
/// Отсутствие файла и битый UTF-8 — разные коды: фронт по ним решает, что показать.
pub(crate) fn read_text(path: &Path) -> Result<String, CommandError> {
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
pub(crate) fn write_text(path: &Path, contents: &str) -> Result<(), CommandError> {
    std::fs::write(path, contents).map_err(|e| CommandError::io(format!("{}: {e}", path.display())))
}

/// Разрешает каталогу файла (и подкаталогам) читаться через asset-протокол.
///
/// Модель Notepad++: путь выбрал пользователь в нативном диалоге — это и есть
/// согласие. `recursive = true`, чтобы работали картинки в подпапках (`images/…`).
/// Ошибку логируем и не роняем команду: картинки — не критичный путь.
pub(crate) fn allow_asset_dir(app: &tauri::AppHandle, dir: &Path) {
    use tauri::Manager;
    if let Err(e) = app.asset_protocol_scope().allow_directory(dir, true) {
        eprintln!("asset scope для {}: {e}", dir.display());
    }
}

/// Имя файла из пути для `document-event` (S11, ТЗ §4.6).
///
/// В журнал фронтенда идёт **только имя** файла, не полный путь (приватность).
/// Документ без пути (новый) → `None`.
pub(crate) fn file_name(path: Option<&str>) -> Option<String> {
    path.and_then(|p| {
        Path::new(p)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorCode;
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
