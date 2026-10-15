//! Локальный каталог плагинов: чтение `index.json` и установка плагинов в каталог приложения.
//!
//! Запись каталога несёт только `id`, относительный `path` и `description`; метаданные
//! (`name`/`version`/`permissions`) не дублируются и берутся из `plugin.json` при загрузке.
//!
//! Файловые операции идут по явным `repo`/`dest` — без CWD и env, чтобы функции оставались
//! тестируемыми; разрешение путей по умолчанию живёт в `main.rs` плагинного CLI.

use std::io::Read;
use std::path::{Component, Path, PathBuf};

use plugin_core::manifest::Manifest;

/// Файл каталога в каталоге репозитория плагинов.
pub const CATALOG_FILE: &str = "index.json";
/// Файл манифеста каталога плагина (совпадает со сканированием GUI).
pub const MANIFEST_FILE: &str = "plugin.json";
/// Потолок размера `index.json`: каталог мал, всё сверх — мусор/атака на память.
pub const MAX_CATALOG_BYTES: u64 = 256 * 1024;
/// Потолок размера `plugin.json` (совпадает с GUI-сканированием).
pub const MAX_MANIFEST_BYTES: u64 = 256 * 1024;
/// Единственная поддерживаемая версия схемы каталога.
pub const SCHEMA_VERSION: u32 = 1;

/// Сырая запись каталога (до обогащения данными манифеста).
#[derive(Debug)]
struct RawEntry {
    id: String,
    path: String,
    description: String,
}

/// Разрешённая запись каталога: манифест прочитан и провалидирован.
/// `name`/`version`/`api_version`/`permissions` — из манифеста.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogEntry {
    /// id плагина; обязан совпадать с `manifest.id`.
    pub id: String,
    /// Относительный путь каталога плагина от `repo` (из `index.json`).
    pub path: String,
    /// Описание из `index.json` (может быть пустым).
    pub description: String,
    pub name: String,
    pub version: String,
    pub api_version: u32,
    pub permissions: Vec<String>,
    /// Каталог плагина (`<repo>/<path>`; абсолютный, если `repo` абсолютный).
    pub dir: PathBuf,
}

/// Загруженный каталог плагинов.
#[derive(Debug, Clone)]
pub struct Catalog {
    repo: PathBuf,
    entries: Vec<CatalogEntry>,
}

impl Catalog {
    /// Читает и валидирует `<repo>/index.json` вместе с манифестами всех записей.
    ///
    /// Ошибка на любой записи — ошибка всего каталога: молча пропускать битые записи нельзя.
    pub fn load(repo: &Path) -> Result<Self, String> {
        let index_path = repo.join(CATALOG_FILE);
        let bytes = read_capped(&index_path, MAX_CATALOG_BYTES)?;
        let raw_entries = parse_index(&bytes, &index_path)?;

        let mut entries = Vec::with_capacity(raw_entries.len());
        for raw_entry in raw_entries {
            let entry = resolve_entry(repo, raw_entry)?;
            if entries.iter().any(|e: &CatalogEntry| e.id == entry.id) {
                return Err(format!(
                    "{}: дублирующийся id {}",
                    index_path.display(),
                    entry.id
                ));
            }
            entries.push(entry);
        }
        Ok(Self {
            repo: repo.to_path_buf(),
            entries,
        })
    }

    /// Записи каталога в порядке объявления в `index.json`.
    pub fn list(&self) -> &[CatalogEntry] {
        &self.entries
    }

    /// Запись по id без проверки: id берётся из уже провалидированного каталога.
    pub fn get(&self, id: &str) -> Option<&CatalogEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// Копирует каталог плагина в `<dest>/<id>`.
    ///
    /// Существующая цель без `force` — ошибка; с `force` удаляется (для ссылки/junction —
    /// только точка монтирования) и копируется заново. Манифест источника перечитывается и
    /// валидируется перед копированием: он мог измениться после `load`.
    pub fn install(&self, id: &str, dest: &Path, force: bool) -> Result<PathBuf, String> {
        let entry = self.entry(id)?;
        read_manifest(&entry.dir)?;
        let target = dest.join(&entry.id);
        prepare_target(&target, force)?;
        copy_dir_all(&entry.dir, &target)?;
        Ok(target)
    }

    /// Привязывает каталог плагина к каталогу приложения ссылкой.
    ///
    /// Windows — junction, Unix — symlink. Источник общий: правки в репозитории сразу видны
    /// в приложении (удобно для разработки через `link`).
    pub fn link(&self, id: &str, dest: &Path, force: bool) -> Result<PathBuf, String> {
        let entry = self.entry(id)?;
        read_manifest(&entry.dir)?;
        std::fs::create_dir_all(dest)
            .map_err(|e| format!("{}: не создать каталог: {e}", dest.display()))?;
        let target = dest.join(&entry.id);
        prepare_target(&target, force)?;
        // F47: ссылка указывает на канонический путь источника — так junction/symlink остаётся
        // валидным независимо от формы пути, которой пользователь задал `--repo`.
        let source = std::fs::canonicalize(&entry.dir)
            .map_err(|e| format!("{}: {e}", entry.dir.display()))?;
        create_link(&source, &target)?;
        Ok(target)
    }

    /// Обновляет установленные копии до версии из репозитория.
    ///
    /// `None` — все установленные плагины каталога, `Some(id)` — только указанный. Ссылки
    /// (junction/symlink) пропускаются: источник общий. Возвращает id переустановленных.
    pub fn update(&self, id: Option<&str>, dest: &Path) -> Result<Vec<String>, String> {
        let ids: Vec<String> = match id {
            Some(id) => {
                let entry = self.entry(id)?;
                if std::fs::symlink_metadata(dest.join(&entry.id)).is_err() {
                    return Err(format!("{}: плагин {id} не установлен", dest.display()));
                }
                vec![entry.id.clone()]
            }
            None => installed_ids(dest),
        };

        let mut changed = Vec::new();
        for id in ids {
            if let Some(entry) = self.get(&id) {
                if update_one(entry, dest)? {
                    changed.push(id);
                }
            }
        }
        Ok(changed)
    }

    /// Запись по id с проверкой `is_safe_id` или понятная ошибка — точка валидации для
    /// install/link.
    fn entry(&self, id: &str) -> Result<&CatalogEntry, String> {
        if !is_safe_id(id) {
            return Err(format!(
                "недопустимый id плагина {id:?}: только строчные латинские, цифры, '-' и '_'"
            ));
        }
        self.get(id)
            .ok_or_else(|| format!("плагин {id:?} не найден в каталоге {}", self.repo.display()))
    }
}

/// Удаляет установленный плагин `<dest>/<id>`.
///
/// Ссылка/junction удаляется как ссылка (`fs::remove_dir`) — цель не затрагивается; обычный
/// каталог — целиком (`fs::remove_dir_all`).
///
/// Идемпотентна: отсутствие `<dest>/<id>` — не ошибка, а `Ok(false)`, чтобы повторный `remove`
/// в скрипте не валил пайплайн; `Ok(true)` — что-то удалено. `Err` — только для небезопасного `id`
/// или сбоя удаления.
pub fn remove(id: &str, dest: &Path) -> Result<bool, String> {
    if !is_safe_id(id) {
        return Err(format!(
            "недопустимый id плагина {id:?}: только строчные латинские, цифры, '-' и '_'"
        ));
    }
    let target = dest.join(id);
    if std::fs::symlink_metadata(&target).is_err() {
        return Ok(false);
    }
    remove_path(&target)?;
    Ok(true)
}

/// Ближайший к `start` (включая сам `start`) предок с каталогом плагинов: generic
/// `plugins/index.json` или репозиторный `examples/plugins/index.json` (эталонный каталог darmark).
/// Возвращает сам каталог плагинов (`<предок>/plugins`), а не корень репозитория.
pub fn discover_repo(start: &Path) -> Option<PathBuf> {
    let mut current: Option<&Path> = Some(start);
    while let Some(dir) = current {
        for candidate in [dir.join("plugins"), dir.join("examples").join("plugins")] {
            if candidate.join(CATALOG_FILE).is_file() {
                return Some(candidate);
            }
        }
        current = dir.parent();
    }
    None
}

/// Разбирает и валидирует `index.json` в список сырых записей.
///
/// Разбор через `serde_json::Value` (без прямой зависимости от `serde`): структура индекса мала и
/// фиксирована, а ошибки нужны с точным указанием файла.
fn parse_index(bytes: &[u8], index_path: &Path) -> Result<Vec<RawEntry>, String> {
    let value: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|e| format!("{}: не разобран JSON: {e}", index_path.display()))?;
    let object = value
        .as_object()
        .ok_or_else(|| format!("{}: ожидался JSON-объект", index_path.display()))?;

    let schema_version = object
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| format!("{}: нет числового schema_version", index_path.display()))?;
    if schema_version != u64::from(SCHEMA_VERSION) {
        return Err(format!(
            "{}: schema_version {schema_version} не поддерживается (ожидается {SCHEMA_VERSION})",
            index_path.display()
        ));
    }

    let plugins = object
        .get("plugins")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| format!("{}: нет массива plugins", index_path.display()))?;

    let mut entries = Vec::with_capacity(plugins.len());
    for item in plugins {
        let item = item
            .as_object()
            .ok_or_else(|| format!("{}: запись plugins не объект", index_path.display()))?;
        let id = item
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("{}: в записи нет строкового id", index_path.display()))?
            .to_string();
        let path = item
            .get("path")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                format!(
                    "{}: в записи {id} нет строкового path",
                    index_path.display()
                )
            })?
            .to_string();
        let description = item
            .get("description")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        entries.push(RawEntry {
            id,
            path,
            description,
        });
    }
    Ok(entries)
}

/// Сырая запись → разрешённая: безопасность пути + чтение и валидация манифеста.
fn resolve_entry(repo: &Path, raw: RawEntry) -> Result<CatalogEntry, String> {
    if !is_safe_id(&raw.id) {
        return Err(format!(
            "каталог: недопустимый id {:?}: только строчные латинские, цифры, '-' и '_'",
            raw.id
        ));
    }
    if !is_safe_relative(&raw.path) {
        return Err(format!(
            "каталог: плагин {}: небезопасный path {:?} (нужен относительный без '..', ':' и \
             абсолютных путей)",
            raw.id, raw.path
        ));
    }
    let dir = repo.join(&raw.path);
    if !dir.starts_with(repo) {
        return Err(format!(
            "каталог: плагин {}: path {:?} выходит за пределы каталога",
            raw.id, raw.path
        ));
    }
    let manifest = read_manifest(&dir).map_err(|e| format!("каталог: плагин {}: {e}", raw.id))?;
    if manifest.id != raw.id {
        return Err(format!(
            "каталог: id {} не совпадает с manifest.id {} ({})",
            raw.id,
            manifest.id,
            dir.display()
        ));
    }
    Ok(CatalogEntry {
        id: raw.id,
        path: raw.path,
        description: raw.description,
        name: manifest.name,
        version: manifest.version,
        api_version: manifest.api_version,
        permissions: manifest.permissions,
        dir,
    })
}

/// Обновляет один установленный плагин. `true` — копия переустановлена.
fn update_one(entry: &CatalogEntry, dest: &Path) -> Result<bool, String> {
    let target = dest.join(&entry.id);
    let meta =
        std::fs::symlink_metadata(&target).map_err(|e| format!("{}: {e}", target.display()))?;
    if meta.file_type().is_symlink() {
        // Ссылка указывает на общий источник — обновлять нечего.
        return Ok(false);
    }

    let source = read_manifest(&entry.dir)?;
    let installed_version = read_manifest(&target).ok().map(|m| m.version);
    if installed_version.as_deref() == Some(source.version.as_str()) {
        return Ok(false);
    }

    // Версия отличается либо манифест копии не читается (установка повреждена) — переустанавливаем.
    prepare_target(&target, true)?;
    copy_dir_all(&entry.dir, &target)?;
    Ok(true)
}

/// id каталогов в `dest`; ссылки отсеивает `update_one`.
fn installed_ids(dest: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dest) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = entries
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| is_safe_id(name))
        .collect();
    ids.sort();
    ids
}

/// Путь назначения: отсутствует — ок; существует без `force` — ошибка; с `force` — удаляется.
fn prepare_target(target: &Path, force: bool) -> Result<(), String> {
    if std::fs::symlink_metadata(target).is_err() {
        return Ok(());
    }
    if !force {
        return Err(format!(
            "{}: уже существует (используйте --force для перезаписи)",
            target.display()
        ));
    }
    remove_path(target)
}

/// Удаляет каталог-назначение: ссылка/junction — как ссылку, обычный каталог — целиком.
fn remove_path(target: &Path) -> Result<(), String> {
    let meta =
        std::fs::symlink_metadata(target).map_err(|e| format!("{}: {e}", target.display()))?;
    if meta.file_type().is_symlink() {
        std::fs::remove_dir(target)
            .map_err(|e| format!("{}: не удалить ссылку: {e}", target.display()))
    } else {
        std::fs::remove_dir_all(target)
            .map_err(|e| format!("{}: не удалить каталог: {e}", target.display()))
    }
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| format!("{}: не создать: {e}", dst.display()))?;
    let entries = std::fs::read_dir(src).map_err(|e| format!("{}: {e}", src.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("{}: {e}", src.display()))?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        let file_type = entry
            .file_type()
            .map_err(|e| format!("{}: {e}", from.display()))?;
        if file_type.is_dir() {
            copy_dir_all(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)
                .map_err(|e| format!("{} → {}: {e}", from.display(), to.display()))?;
        }
    }
    Ok(())
}

/// Junction через `mklink /J` — в отличие от symlink не требует прав администратора.
fn create_link(source: &Path, target: &Path) -> Result<(), String> {
    let output = std::process::Command::new("cmd")
        .arg("/c")
        .arg("mklink")
        .arg("/J")
        .arg(target)
        .arg(source)
        .output()
        .map_err(|e| format!("не запустить cmd /c mklink: {e}"))?;
    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "{}: не создать junction: {} {}",
            target.display(),
            stdout.trim(),
            stderr.trim()
        ));
    }
    Ok(())
}

fn read_manifest(dir: &Path) -> Result<Manifest, String> {
    let path = dir.join(MANIFEST_FILE);
    let bytes = read_capped(&path, MAX_MANIFEST_BYTES)?;
    Manifest::from_json(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// `path` — безопасный относительный путь внутри базы.
fn is_safe_relative(path: &str) -> bool {
    if path.is_empty() || path.contains(':') || Path::new(path).is_absolute() {
        return false;
    }
    let mut has_normal = false;
    for component in Path::new(path).components() {
        match component {
            Component::Normal(_) => has_normal = true,
            // `..`, `.`, корень или префикс диска.
            _ => return false,
        }
    }
    has_normal
}

/// Безопасный id плагина; правило общее с `plugin_core::manifest` — расхождение копий ломает
/// каталог.
fn is_safe_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// Чтение с жёстким потолком размера: проверка метаданных и повторная проверка после
/// `take(max + 1)` закрывают TOCTOU и OOM.
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

    fn manifest(id: &str, version: &str) -> String {
        format!(
            r#"{{"id":"{id}","name":"{id}","version":"{version}","api_version":1,"entry":"main.lua","permissions":["document:read"]}}"#
        )
    }

    /// Пишет каталог плагина `<repo>/<path>` с манифестом заданной версии.
    fn write_plugin(repo: &Path, path: &str, id: &str, version: &str) {
        let dir = repo.join(path);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(MANIFEST_FILE), manifest(id, version)).unwrap();
        fs::write(dir.join("main.lua"), "function on_activate(ctx) end\n").unwrap();
    }

    /// Пишет `index.json` по парам `(id, path, description)`.
    fn write_index(repo: &Path, entries: &[(&str, &str, &str)]) {
        fs::create_dir_all(repo).unwrap();
        let plugins: Vec<serde_json::Value> = entries
            .iter()
            .map(|(id, path, description)| {
                serde_json::json!({ "id": id, "path": path, "description": description })
            })
            .collect();
        let index = serde_json::json!({ "schema_version": 1, "plugins": plugins });
        fs::write(repo.join(CATALOG_FILE), serde_json::to_vec(&index).unwrap()).unwrap();
    }

    fn single_plugin_repo(root: &Path) -> PathBuf {
        let repo = root.join("plugins");
        write_plugin(&repo, "alpha", "alpha", "1.0.0");
        write_index(&repo, &[("alpha", "alpha", "тестовый плагин")]);
        repo
    }

    fn manifest_text(dir: &Path) -> String {
        fs::read_to_string(dir.join(MANIFEST_FILE)).unwrap()
    }

    #[test]
    fn load_resolves_entries_with_manifest_data() {
        let root = tempfile::tempdir().unwrap();
        let repo = single_plugin_repo(root.path());
        let catalog = Catalog::load(&repo).unwrap();

        assert_eq!(catalog.list().len(), 1);
        let entry = &catalog.list()[0];
        assert_eq!(entry.id, "alpha");
        assert_eq!(entry.name, "alpha");
        assert_eq!(entry.version, "1.0.0");
        assert_eq!(entry.api_version, 1);
        assert_eq!(entry.description, "тестовый плагин");
        assert_eq!(entry.permissions, vec!["document:read"]);
        assert!(entry.dir.join(MANIFEST_FILE).is_file());
    }

    #[test]
    fn load_rejects_id_mismatch_with_manifest() {
        let root = tempfile::tempdir().unwrap();
        let repo = root.path().join("plugins");
        write_plugin(&repo, "alpha", "beta", "1.0.0");
        write_index(&repo, &[("alpha", "alpha", "")]);

        let err = Catalog::load(&repo).unwrap_err();
        assert!(err.contains("не совпадает"), "err: {err}");
    }

    #[test]
    fn load_rejects_unsafe_paths() {
        for path in ["../evil", "/abs", "a/../../b", "C:\\evil", "a:b"] {
            let root = tempfile::tempdir().unwrap();
            let repo = root.path().join("plugins");
            write_index(&repo, &[("alpha", path, "")]);
            let err = Catalog::load(&repo).unwrap_err();
            assert!(
                err.contains("небезопасный path") || err.contains("недопустимый"),
                "path {path:?}: err: {err}"
            );
        }
    }

    #[test]
    fn load_rejects_missing_plugin_json() {
        let root = tempfile::tempdir().unwrap();
        let repo = root.path().join("plugins");
        write_index(&repo, &[("ghost", "ghost", "")]);

        let err = Catalog::load(&repo).unwrap_err();
        assert!(err.contains("plugin.json"), "err: {err}");
    }

    #[test]
    fn load_rejects_invalid_manifest() {
        let root = tempfile::tempdir().unwrap();
        let repo = root.path().join("plugins");
        let dir = repo.join("alpha");
        fs::create_dir_all(&dir).unwrap();
        // api_version = 2 новее поддерживаемого — манифест невалиден.
        fs::write(
            dir.join(MANIFEST_FILE),
            r#"{"id":"alpha","name":"Alpha","version":"1.0.0","api_version":2,"entry":"main.lua"}"#,
        )
        .unwrap();
        write_index(&repo, &[("alpha", "alpha", "")]);

        let err = Catalog::load(&repo).unwrap_err();
        assert!(err.contains("api_version"), "err: {err}");
    }

    #[test]
    fn load_rejects_wrong_schema_version() {
        let root = tempfile::tempdir().unwrap();
        let repo = root.path().join("plugins");
        fs::create_dir_all(&repo).unwrap();
        fs::write(
            repo.join(CATALOG_FILE),
            r#"{"schema_version": 2, "plugins": []}"#,
        )
        .unwrap();

        let err = Catalog::load(&repo).unwrap_err();
        assert!(err.contains("schema_version"), "err: {err}");
    }

    #[test]
    fn load_rejects_duplicate_ids() {
        let root = tempfile::tempdir().unwrap();
        let repo = single_plugin_repo(root.path());
        write_index(&repo, &[("alpha", "alpha", ""), ("alpha", "alpha", "")]);
        let err = Catalog::load(&repo).unwrap_err();
        assert!(err.contains("дублирующийся"), "err: {err}");
    }

    #[test]
    fn install_copies_then_refuses_and_force_overwrites() {
        let root = tempfile::tempdir().unwrap();
        let repo = single_plugin_repo(root.path());
        let dest = root.path().join("dest");
        let catalog = Catalog::load(&repo).unwrap();

        let target = catalog.install("alpha", &dest, false).unwrap();
        assert_eq!(target, dest.join("alpha"));
        assert!(target.join(MANIFEST_FILE).is_file());

        // Повтор без --force — ошибка, содержимое не тронуто.
        fs::write(target.join(MANIFEST_FILE), "изменено").unwrap();
        let err = catalog.install("alpha", &dest, false).unwrap_err();
        assert!(err.contains("--force"), "err: {err}");
        assert_eq!(manifest_text(&target), "изменено");

        // С --force — переустановка из источника.
        catalog.install("alpha", &dest, true).unwrap();
        assert!(manifest_text(&target).contains("1.0.0"));
    }

    #[test]
    fn install_unknown_id_fails() {
        let root = tempfile::tempdir().unwrap();
        let repo = single_plugin_repo(root.path());
        let catalog = Catalog::load(&repo).unwrap();
        let err = catalog
            .install("ghost", &root.path().join("dest"), false)
            .unwrap_err();
        assert!(err.contains("не найден"), "err: {err}");
    }

    #[test]
    fn remove_deletes_copy_and_keeps_source() {
        let root = tempfile::tempdir().unwrap();
        let repo = single_plugin_repo(root.path());
        let dest = root.path().join("dest");
        let catalog = Catalog::load(&repo).unwrap();
        let target = catalog.install("alpha", &dest, false).unwrap();

        assert!(remove("alpha", &dest).unwrap(), "копия удалена");
        assert!(!target.exists(), "копия должна быть удалена");
        assert!(repo.join("alpha").join(MANIFEST_FILE).is_file());
    }

    #[test]
    fn remove_missing_is_noop() {
        let root = tempfile::tempdir().unwrap();
        assert!(!remove("alpha", root.path()).unwrap());
    }

    #[test]
    fn remove_rejects_unsafe_id() {
        let root = tempfile::tempdir().unwrap();
        let err = remove("../evil", root.path()).unwrap_err();
        assert!(err.contains("недопустимый id"), "err: {err}");
    }

    #[test]
    fn link_then_remove_detaches_without_touching_source() {
        let root = tempfile::tempdir().unwrap();
        let repo = single_plugin_repo(root.path());
        let dest = root.path().join("dest");
        let catalog = Catalog::load(&repo).unwrap();

        let target = catalog.link("alpha", &dest, false).unwrap();
        let meta = fs::symlink_metadata(&target).unwrap();
        assert!(meta.file_type().is_symlink(), "ожидался junction");
        assert!(target.join(MANIFEST_FILE).is_file());

        assert!(remove("alpha", &dest).unwrap(), "junction удалён");
        assert!(fs::symlink_metadata(&target).is_err(), "junction удалён");
        assert!(
            repo.join("alpha").join(MANIFEST_FILE).is_file(),
            "источник не тронут"
        );
    }

    #[test]
    fn link_refuses_existing_without_force() {
        let root = tempfile::tempdir().unwrap();
        let repo = single_plugin_repo(root.path());
        let dest = root.path().join("dest");
        let catalog = Catalog::load(&repo).unwrap();

        catalog.link("alpha", &dest, false).unwrap();
        let err = catalog.link("alpha", &dest, false).unwrap_err();
        assert!(err.contains("--force"), "err: {err}");
        // С --force пересоздаётся без ошибки.
        catalog.link("alpha", &dest, true).unwrap();
    }

    #[test]
    fn update_reinstalls_copy_on_version_change() {
        let root = tempfile::tempdir().unwrap();
        let repo = single_plugin_repo(root.path());
        let dest = root.path().join("dest");
        let catalog = Catalog::load(&repo).unwrap();
        let target = catalog.install("alpha", &dest, false).unwrap();

        // Версия не менялась — ничего не обновляется.
        assert!(catalog.update(None, &dest).unwrap().is_empty());

        // Меняем версию в источнике.
        fs::write(
            repo.join("alpha").join(MANIFEST_FILE),
            manifest("alpha", "2.0.0"),
        )
        .unwrap();
        let changed = catalog.update(None, &dest).unwrap();
        assert_eq!(changed, vec!["alpha".to_string()]);
        assert!(manifest_text(&target).contains("2.0.0"));
    }

    #[test]
    fn update_specific_missing_id_fails() {
        let root = tempfile::tempdir().unwrap();
        let repo = single_plugin_repo(root.path());
        let dest = root.path().join("dest");
        let catalog = Catalog::load(&repo).unwrap();
        let err = catalog.update(Some("alpha"), &dest).unwrap_err();
        assert!(err.contains("не установлен"), "err: {err}");
    }

    #[test]
    fn update_skips_links() {
        let root = tempfile::tempdir().unwrap();
        let repo = single_plugin_repo(root.path());
        let dest = root.path().join("dest");
        let catalog = Catalog::load(&repo).unwrap();
        let target = catalog.link("alpha", &dest, false).unwrap();

        fs::write(
            repo.join("alpha").join(MANIFEST_FILE),
            manifest("alpha", "2.0.0"),
        )
        .unwrap();
        // Ссылка не переустанавливается; новая версия видна через неё.
        assert!(catalog.update(None, &dest).unwrap().is_empty());
        assert!(manifest_text(&target).contains("2.0.0"));
    }

    #[test]
    fn update_specific_link_is_skipped() {
        let root = tempfile::tempdir().unwrap();
        let repo = single_plugin_repo(root.path());
        let dest = root.path().join("dest");
        let catalog = Catalog::load(&repo).unwrap();
        let target = catalog.link("alpha", &dest, false).unwrap();

        fs::write(
            repo.join("alpha").join(MANIFEST_FILE),
            manifest("alpha", "2.0.0"),
        )
        .unwrap();
        // Явный id на ссылке: не ошибка и не переустановка — источник общий (S6).
        assert!(catalog.update(Some("alpha"), &dest).unwrap().is_empty());
        assert!(manifest_text(&target).contains("2.0.0"));
    }

    #[test]
    fn real_catalog_has_four_valid_entries() {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("examples")
            .join("plugins");
        let catalog = Catalog::load(&repo).unwrap();
        assert_eq!(catalog.list().len(), 4);
        for id in ["word-count", "export-html", "format-selection", "todo-lens"] {
            let entry = catalog.get(id).unwrap_or_else(|| panic!("нет записи {id}"));
            assert!(
                entry.dir.join(MANIFEST_FILE).is_file(),
                "{id}: нет манифеста"
            );
        }
    }

    #[test]
    fn discover_repo_walks_up_to_plugins_dir() {
        let root = tempfile::tempdir().unwrap();
        let repo = single_plugin_repo(root.path());
        let nested = root.path().join("a").join("b");
        fs::create_dir_all(&nested).unwrap();
        assert_eq!(discover_repo(&nested).unwrap(), repo);
        let empty = tempfile::tempdir().unwrap();
        assert!(discover_repo(empty.path()).is_none());
    }

    #[test]
    fn discover_repo_finds_examples_plugins_layout() {
        // Репозиторный каталог darmark: `examples/plugins/index.json` — тоже находится автопоиском.
        let root = tempfile::tempdir().unwrap();
        let repo = root.path().join("examples").join("plugins");
        write_plugin(&repo, "alpha", "alpha", "1.0.0");
        write_index(&repo, &[("alpha", "alpha", "тестовый плагин")]);
        let nested = root.path().join("a").join("b");
        fs::create_dir_all(&nested).unwrap();
        assert_eq!(discover_repo(&nested).unwrap(), repo);
    }
}
