//! Библиотечный API child-хоста плагинов darmark.
//!
//! Бинарник `darmark-plugin-host` (`src/main.rs`) — тонкая оболочка над этим крейтом:
//! stdio-цикл конверта, режимы `--self-test` и `--serve-plugin`. Библиотека даёт внешним
//! инструментам доступ к песочнице (`sandbox`), Lua-биндингам (`bindings`) и самопроверке
//! (`selftest`) — например, CLI `plugin-cli` (mlua остаётся вне GUI-хоста, D6/ADR-0021).

pub mod bindings;
pub mod sandbox;
pub mod selftest;

use std::path::Path;


pub const DEFAULT_PLUGIN_ID: &str = "plugin";


pub fn plugin_id_of(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| DEFAULT_PLUGIN_ID.to_string())
}


pub fn read_plugin_source(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("не читается {}: {e}", path.display()))?;
    if bytes.len() as u64 > plugin_core::MAX_PLUGIN_SOURCE_BYTES {
        return Err(format!(
            "плагин больше {} КБ ({} байт)",
            plugin_core::MAX_PLUGIN_SOURCE_BYTES / 1024,
            bytes.len()
        ));
    }
    String::from_utf8(bytes).map_err(|e| format!("{}: не UTF-8: {e}", path.display()))
}






pub fn read_plugin_manifest_source(path: &Path) -> Result<(String, String), String> {
    if path.is_dir() {
        let manifest_path = path.join("plugin.json");
        let bytes = std::fs::read(&manifest_path)
            .map_err(|e| format!("не читается {}: {e}", manifest_path.display()))?;
        let manifest = plugin_core::manifest::Manifest::from_json(&bytes)
            .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
        
        let entry_path = path.join(&manifest.entry);
        let source = read_plugin_source(&entry_path)?;
        Ok((manifest.id, source))
    } else if path.is_file() {
        let source = read_plugin_source(path)?;
        Ok((plugin_id_of(path), source))
    } else {
        Err(format!(
            "{}: ожидается каталог плагина с plugin.json или .lua-файл",
            path.display()
        ))
    }
}
