use std::path::{Path, PathBuf};

use crate::error::{CommandError, ErrorCode};


pub(super) const MANIFEST_FILE: &str = "plugin.json";


pub(super) const ENTRY_FILE: &str = "main.lua";




pub fn plugins_root() -> Result<PathBuf, CommandError> {
    if let Some(dir) = std::env::var_os("DARMARK_PLUGINS_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let base = std::env::var_os("APPDATA")
        .ok_or_else(|| CommandError::new(ErrorCode::AiIo, "APPDATA не задан"))?;
    Ok(PathBuf::from(base).join("darmark").join("plugins"))
}







fn canonical_root(root: &Path) -> PathBuf {
    std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf())
}








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



pub(super) fn is_reparse_point(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    std::fs::symlink_metadata(path)
        .map(|meta| meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
        .unwrap_or(false)
}
