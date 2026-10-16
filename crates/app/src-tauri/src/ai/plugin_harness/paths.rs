use std::path::{Path, PathBuf};

use crate::error::{CommandError, ErrorCode};

/// Имя файла манифеста в каталоге плагина.
pub(super) const MANIFEST_FILE: &str = "plugin.json";

/// Имя entry-файла, который пишет harness (совпадает с шаблоном `plugin-cli init`).
pub(super) const ENTRY_FILE: &str = "main.lua";

/// Корень каталога плагинов: env `DARMARK_PLUGINS_DIR` → `%APPDATA%/darmark/plugins`.
///
/// Совпадает с `plugins_dir()` GUI-хоста, но без cfg-гейта: harness доступен и в тестах.
pub fn plugins_root() -> Result<PathBuf, CommandError> {
    if let Some(dir) = std::env::var_os("DARMARK_PLUGINS_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let base = std::env::var_os("APPDATA")
        .ok_or_else(|| CommandError::new(ErrorCode::AiIo, "APPDATA не задан"))?;
    Ok(PathBuf::from(base).join("darmark").join("plugins"))
}

/// Канонизирует корень белого списка; при неудаче — исходный путь.
///
/// `canonicalize` на Windows возвращает verbatim-путь (`\\?\…`), поэтому сравнивать
/// канонический путь надо с каноническим корнем. Если корень не канонизируется
/// (не существует) — исходный путь не совпадёт с verbatim-путём, т.е. ничего лишнего
/// не разрешаем (fail-closed).
fn canonical_root(root: &Path) -> PathBuf {
    std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf())
}

/// Отказывает, если любой компонент пути **ниже** `root` — reparse point (symlink/junction).
///
/// `canonicalize` раскрывает ссылки, поэтому junction/symlink *внутри* разрешённого каталога
/// (например `allowed\junction\to\inside`) не виден на каноническом пути и containment его не
/// ловит. Обходим **исходный** путь от `root` (сам `root` доверенный — не проверяем) до листа.
/// Если путь лексически не под `root` (относительный/иная раскладка) — пропускаем: containment
/// по каноническому пути уже проверен вызывающим.
fn reject_reparse_between(root: &Path, path: &Path) -> Result<(), CommandError> {
    let Ok(relative) = path.strip_prefix(root) else {
        return Ok(());
    };
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component);
        if is_reparse_point(&current) {
            return Err(CommandError::new(
                ErrorCode::AiPluginInvalid,
                "путь содержит symlink/junction — доступ запрещён",
            ));
        }
    }
    Ok(())
}

/// Отказывает, если `plugin.json`/`main.lua` внутри каталога — сами reparse point.
///
/// AC-MCP-12: symlink-файл манифеста/entry иначе читался бы вслед за ссылкой (эскейп за
/// пределы разрешённой области). `symlink_metadata` не следует за ссылкой, поэтому видит
/// reparse-атрибут самого файла. Отсутствие файла — не ошибка (обработается валидацией).
/// Объявленный в манифесте `entry` здесь неизвестен — его проверяет [`ensure_plugin_entry_allowed`].
fn reject_reparse_plugin_files(dir: &Path) -> Result<(), CommandError> {
    for name in [MANIFEST_FILE, ENTRY_FILE] {
        if is_reparse_point(&dir.join(name)) {
            return Err(CommandError::new(
                ErrorCode::AiPluginInvalid,
                format!("файл {name} — symlink/junction, чтение запрещено"),
            ));
        }
    }
    Ok(())
}

/// Проверяет объявленный в манифесте `entry`: не reparse point и остаётся внутри каталога.
///
/// [`reject_reparse_plugin_files`] знает только `plugin.json`/`main.lua`, а манифест вправе
/// объявить другое имя (`entry: "init.lua"`). Гейт на объявленном имени обязателен: иначе
/// symlink-файл читался бы вслед за ссылкой за пределы разрешённой области (AC-MCP-12).
///
/// `dir` — **канонический** каталог плагина (как возвращает [`ensure_plugin_path_allowed`]):
/// containment проверяется по каноническому пути, поэтому `..`/разделители в `entry` не выводят
/// чтение за каталог даже при небезопасном имени; несуществующий файл — ошибка (fail-closed).
pub fn ensure_plugin_entry_allowed(dir: &Path, entry: &str) -> Result<PathBuf, CommandError> {
    let path = dir.join(entry);
    if is_reparse_point(&path) {
        return Err(CommandError::new(
            ErrorCode::AiPluginInvalid,
            format!("файл {entry} — symlink/junction, чтение запрещено"),
        ));
    }

    let canonical = std::fs::canonicalize(&path).map_err(|e| {
        CommandError::new(
            ErrorCode::AiPluginInvalid,
            format!("файл {entry} не читается: {e}"),
        )
    })?;
    if !canonical.starts_with(dir) {
        return Err(CommandError::new(
            ErrorCode::AiPluginInvalid,
            format!("файл {entry} вне каталога плагина"),
        ));
    }
    Ok(canonical)
}

/// Проверяет, что путь лежит в разрешённом каталоге (`plugins/` или `temp/`).
///
/// Единая точка проверки путей для `plugin_validate`/`plugin_test` (TZ-MCP R3/AC-MCP-12):
/// принуждение живёт здесь, а не в MCP-адаптере. Defense-in-depth: отказ на reparse point
/// (symlink/junction) → `canonicalize` (раскрывает `..` и ссылки) → `starts_with` по
/// каноническим корням → повторный обход компонентов пути от корня до листа и проверка файлов
/// манифеста/entry. Возвращает канонический путь, чтобы дальнейшая работа шла уже с проверенным
/// значением.
pub fn ensure_plugin_path_allowed(path: &Path) -> Result<PathBuf, CommandError> {
    if is_reparse_point(path) {
        return Err(CommandError::new(
            ErrorCode::AiPluginInvalid,
            "путь является symlink/junction — доступ запрещён",
        ));
    }

    let canonical = std::fs::canonicalize(path).map_err(|e| {
        CommandError::new(
            ErrorCode::AiPluginInvalid,
            format!("путь не существует: {e}"),
        )
    })?;

    let plugins = plugins_root().ok();
    let temp = std::env::temp_dir();

    let within_plugins = plugins
        .as_ref()
        .is_some_and(|root| canonical.starts_with(canonical_root(root)));
    let within_temp = canonical.starts_with(canonical_root(&temp));
    if !within_plugins && !within_temp {
        return Err(CommandError::new(
            ErrorCode::AiPluginInvalid,
            "путь вне разрешённого каталога (plugins/ или temp/)",
        ));
    }
    if let Some(root) = &plugins {
        reject_reparse_between(root, path)?;
    }
    reject_reparse_between(&temp, path)?;
    reject_reparse_plugin_files(&canonical)?;

    Ok(canonical)
}

/// Каталог — reparse point (symlink/junction)? Defense-in-depth (ADR-0034 §2.4): запись в такой
/// каталог может увести файлы за пределы `plugins/`. Проверяем атрибут `FILE_ATTRIBUTE_REPARSE_POINT`.
pub(super) fn is_reparse_point(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    std::fs::symlink_metadata(path)
        .map(|meta| meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
        .unwrap_or(false)
}
