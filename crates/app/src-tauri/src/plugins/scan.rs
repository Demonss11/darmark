//! Сканирование каталога плагинов (Фаза 3 TZ-H2, §6.6 DESIGN_DOC).
//!
//! `scan(plugins_dir) → validate(manifest) → …`. Каждый подкаталог с `plugin.json` валидируется
//! (`plugin_proto::manifest`) и читается `.lua`-исходник. Ошибки одного плагина не мешают
//! остальным: они собираются в отчёт, а не роняют сканирование.
//!
//! Файлы читаются с проверкой размера **до** аллокации (гигантский `plugin.json`/`main.lua`
//! не должен уронить GUI-хост). Симлинки/junction не разворачиваются (не идём по reparse point).

use std::io::Read;
use std::path::{Path, PathBuf};

use plugin_proto::manifest::Manifest;
use plugin_proto::MAX_PLUGIN_SOURCE_BYTES;

/// Имя файла манифеста в каталоге плагина.
pub const MANIFEST_FILE: &str = "plugin.json";

/// Лимит размера `plugin.json`: манифест мал, всё сверх — мусор/атака на память хоста.
pub const MAX_MANIFEST_BYTES: u64 = 256 * 1024;

/// Валидный плагин, найденный на диске.
pub struct DiscoveredPlugin {
    pub dir: PathBuf,
    pub manifest: Manifest,
    /// Исходник, прочитанный при сканировании (кэш; `reload` перечитывает с диска).
    pub source: String,
    /// Путь к `.lua`-файлу — для перечитывания при перезагрузке.
    pub entry_path: PathBuf,
}

/// Ошибка одного каталога (не роняет сканирование).
#[derive(Debug)]
pub struct ScanError {
    pub dir: PathBuf,
    pub message: String,
}

/// Результат сканирования: валидные плагины + ошибки по остальным.
#[derive(Default)]
pub struct ScanReport {
    pub plugins: Vec<DiscoveredPlugin>,
    pub errors: Vec<ScanError>,
}

/// Сканирует каталог плагинов. Отсутствующий каталог — пустой отчёт (плагинов нет — не ошибка).
pub fn scan_plugins(dir: &Path) -> ScanReport {
    let mut report = ScanReport::default();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return report;
    };

    // Детерминированный порядок + пропуск симлинков/junction: `file_type()` не разворачивает
    // reparse points, поэтому фиктивные «каталоги» не попадают в скан.
    let mut subdirs: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| entry.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|entry| entry.path())
        .collect();
    subdirs.sort();

    for plugin_dir in subdirs {
        match load_plugin(&plugin_dir) {
            Ok(plugin) => report.plugins.push(plugin),
            Err(message) => report.errors.push(ScanError {
                dir: plugin_dir,
                message,
            }),
        }
    }
    report
}

/// Читает `.lua`-исходник с диска с проверкой размера. Общий путь для сканирования и `reload`.
pub fn read_source(entry_path: &Path) -> Result<String, String> {
    let bytes = read_capped(entry_path, MAX_PLUGIN_SOURCE_BYTES)?;
    String::from_utf8(bytes).map_err(|e| format!("{}: не UTF-8: {e}", entry_path.display()))
}

/// Читает и валидирует один каталог плагина.
fn load_plugin(dir: &Path) -> Result<DiscoveredPlugin, String> {
    let manifest_path = dir.join(MANIFEST_FILE);
    let manifest_bytes = read_capped(&manifest_path, MAX_MANIFEST_BYTES)?;
    let manifest = Manifest::from_json(&manifest_bytes)
        .map_err(|e| format!("{}: {e}", manifest_path.display()))?;

    // `manifest.entry` валидирован как безопасное имя `.lua` (без путей и `..`).
    let entry_path = dir.join(&manifest.entry);
    let source = read_source(&entry_path)?;

    Ok(DiscoveredPlugin {
        dir: dir.to_path_buf(),
        manifest,
        source,
        entry_path,
    })
}

/// Читает файл, но не более `max` байт: сначала быстрый `metadata().len()`, затем жёсткий
/// потолок через `Read::take(max + 1)` — даже если файл вырастет между проверкой и чтением,
/// аллокация ограничена `max + 1` (защита GUI от OOM).
fn read_capped(path: &Path, max: u64) -> Result<Vec<u8>, String> {
    let metadata = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if metadata.len() > max {
        return Err(format!(
            "{}: превышает лимит {max} Б ({} Б)",
            path.display(),
            metadata.len()
        ));
    }

    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut buf = Vec::new();
    file.take(max + 1)
        .read_to_end(&mut buf)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if buf.len() as u64 > max {
        return Err(format!("{}: превышает лимит {max} Б", path.display()));
    }
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_plugin(root: &Path, id: &str, manifest: &str, lua: Option<&str>) {
        let dir = root.join(id);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(MANIFEST_FILE), manifest).unwrap();
        if let Some(lua) = lua {
            fs::write(dir.join("main.lua"), lua).unwrap();
        }
    }

    fn manifest(id: &str) -> String {
        format!(
            r#"{{"id":"{id}","name":"{id}","version":"1.0.0","api_version":1,"entry":"main.lua"}}"#
        )
    }

    #[test]
    fn oversized_plugin_json_is_rejected_without_reading() {
        let dir = tempfile::tempdir().unwrap();
        let plugin_dir = dir.path().join("huge");
        fs::create_dir_all(&plugin_dir).unwrap();
        let f = fs::File::create(plugin_dir.join(MANIFEST_FILE)).unwrap();
        f.set_len(MAX_MANIFEST_BYTES + 1).unwrap();
        drop(f);

        let report = scan_plugins(dir.path());
        assert!(report.plugins.is_empty());
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].message.contains("лимит"));
    }

    #[test]
    fn oversized_lua_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        write_plugin(dir.path(), "big", &manifest("big"), None);
        let f = fs::File::create(dir.path().join("big").join("main.lua")).unwrap();
        f.set_len(MAX_PLUGIN_SOURCE_BYTES + 1).unwrap();
        drop(f);

        let report = scan_plugins(dir.path());
        assert!(report.plugins.is_empty());
        assert!(report.errors[0].message.contains("лимит"));
    }

    #[test]
    fn read_source_reads_valid_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("main.lua");
        fs::write(&path, "return 1").unwrap();
        assert_eq!(read_source(&path).unwrap(), "return 1");
    }
}
