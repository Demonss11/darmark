//! `PluginSettingsStore` — постоянные настройки плагинов (§12 DESIGN_DOC, F43/S5).
//!
//! Один файл на плагин: `%APPDATA%/darmark/plugin-settings/<plugin_id>.json` с JSON-объектом
//! `{ "<key>": <json-value> }`. Базовый каталог переопределяется `DARMARK_PLUGIN_SETTINGS_DIR`
//! (тесты/E2E) — по образцу [`super::settings::SettingsStore`].
//!
//! Чтение терпимо к отсутствию/битому/гигантскому файлу (пустой объект), запись **атомарна**
//! (temp + `rename`). Лимиты (ключ, значение, файл) защищают от раздувания хранилища.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use plugin_core::envelope::PluginError;
use serde_json::{Map, Value};

/// Лимит размера файла настроек плагина: защита от раздувания при чтении/записи.
pub const MAX_SETTINGS_FILE_BYTES: u64 = 256 * 1024;

/// Лимит длины ключа настройки (байты).
pub const MAX_SETTING_KEY_BYTES: usize = 256;

/// Лимит сериализованного значения одной настройки (байты).
pub const MAX_SETTING_VALUE_BYTES: usize = 64 * 1024;

/// Хранилище постоянных настроек плагинов: per-plugin JSON-объект в отдельном файле.
///
/// Держится в `Arc<Mutex<…>>` (host-call'ы обслуживаются синхронно в потоке invocation);
/// кэш разобранных настроек заполняется лениво при первом обращении к плагину.
#[derive(Debug)]
pub struct PluginSettingsStore {
    base_dir: PathBuf,
    /// Кэш разобранных настроек по id плагина (ленивая загрузка с диска).
    cache: HashMap<String, Map<String, Value>>,
}

impl PluginSettingsStore {
    /// Базовый каталог: `DARMARK_PLUGIN_SETTINGS_DIR` (override для тестов/E2E) либо
    /// `%APPDATA%/darmark/plugin-settings`. `None`, если ни override, ни `APPDATA` не заданы.
    pub fn base_dir() -> Option<PathBuf> {
        if let Some(dir) = std::env::var_os("DARMARK_PLUGIN_SETTINGS_DIR") {
            return Some(PathBuf::from(dir));
        }
        let base = std::env::var_os("APPDATA")?;
        Some(PathBuf::from(base).join("darmark").join("plugin-settings"))
    }

    /// Создаёт стор с явным базовым каталогом (тесты подменяют путь здесь).
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
            cache: HashMap::new(),
        }
    }

    /// Путь файла настроек плагина.
    pub fn path_for(&self, plugin_id: &str) -> PathBuf {
        self.base_dir.join(format!("{plugin_id}.json"))
    }

    /// Читает настройку. Отсутствующий ключ/плагин/файл → `None`.
    ///
    /// Некорректный `plugin_id`/ключ (пустой/опасный/слишком длинный) → ошибка `bad_args`
    /// (значением, не паникой).
    pub fn get(&mut self, plugin_id: &str, key: &str) -> Result<Option<Value>, PluginError> {
        validate_id(plugin_id)?;
        validate_key(key)?;
        Ok(self.load(plugin_id).get(key).cloned())
    }

    /// Записывает настройку атомарно. Значение/файл сверх лимита → ошибка `too_large`.
    ///
    /// При превышении лимита файла изменение откатывается: кэш и диск остаются прежними.
    pub fn set(&mut self, plugin_id: &str, key: &str, value: Value) -> Result<(), PluginError> {
        validate_id(plugin_id)?;
        validate_key(key)?;

        let value_len = serde_json::to_vec(&value)
            .map_err(|e| PluginError::new("bad_args", format!("значение не сериализуется: {e}")))?
            .len();
        if value_len > MAX_SETTING_VALUE_BYTES {
            return Err(PluginError::new(
                "too_large",
                format!("значение настройки больше {MAX_SETTING_VALUE_BYTES} Б"),
            ));
        }

        let map = self.load(plugin_id);
        let previous = map.insert(key.to_string(), value);
        let bytes = serde_json::to_vec_pretty(&*map).map_err(|e| {
            PluginError::new("internal", format!("настройки не сериализуются: {e}"))
        })?;
        if bytes.len() as u64 > MAX_SETTINGS_FILE_BYTES {
            // Откат: файл настроек превышать лимит не должен.
            match previous {
                Some(old) => {
                    map.insert(key.to_string(), old);
                }
                None => {
                    map.remove(key);
                }
            }
            return Err(PluginError::new(
                "too_large",
                format!("файл настроек превышает {MAX_SETTINGS_FILE_BYTES} Б"),
            ));
        }

        self.write(plugin_id, &bytes)
    }

    /// Весь объект настроек плагина (из кэша/диска) как [`Value::Object`] для UI-формы (S5b).
    ///
    /// Отсутствующий файл → пустой объект. Некорректный `plugin_id` → `bad_args`: id
    /// подставляется в имя файла `<plugin_id>.json`, поэтому `../evil` увели бы чтение
    /// за пределы каталога настроек.
    pub fn all(&mut self, plugin_id: &str) -> Result<Value, PluginError> {
        validate_id(plugin_id)?;
        Ok(Value::Object(self.load(plugin_id).clone()))
    }

    /// Удаляет настройки плагина (файл + кэш). Вызывается при удалении плагина.
    ///
    /// Некорректный `plugin_id` → `bad_args`: иначе `remove_file` ушёл бы за пределы
    /// каталога настроек (`<plugin_id>.json`).
    pub fn remove_plugin(&mut self, plugin_id: &str) -> Result<(), PluginError> {
        validate_id(plugin_id)?;
        self.cache.remove(plugin_id);
        let _ = std::fs::remove_file(self.path_for(plugin_id));
        Ok(())
    }

    /// Настройки плагина из кэша; при первом обращении — читаются с диска.
    fn load(&mut self, plugin_id: &str) -> &mut Map<String, Value> {
        let path = self.path_for(plugin_id);
        self.cache
            .entry(plugin_id.to_string())
            .or_insert_with(|| read_map(&path))
    }

    /// Атомарная запись: temp-файл в том же каталоге + `rename`; каталог создаётся.
    fn write(&self, plugin_id: &str, bytes: &[u8]) -> Result<(), PluginError> {
        let path = self.path_for(plugin_id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| PluginError::new("io_error", format!("{}: {e}", parent.display())))?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, bytes)
            .map_err(|e| PluginError::new("io_error", format!("{}: {e}", tmp.display())))?;
        std::fs::rename(&tmp, &path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            PluginError::new("io_error", format!("{}: {e}", path.display()))
        })
    }
}

/// Проверяет `plugin_id`: непустой безопасный идентификатор (`[a-z0-9_-]+`).
///
/// `plugin_id` подставляется в имя файла `<plugin_id>.json`, поэтому `../evil`, `a/b`,
/// `..\evil` увели бы чтение/запись за пределы каталога настроек (traversal). Опасный id →
/// `bad_args` (значением, не паникой) — по образцу [`validate_key`].
fn validate_id(plugin_id: &str) -> Result<(), PluginError> {
    if plugin_id.is_empty() {
        return Err(PluginError::new("bad_args", "пустой id плагина"));
    }
    if !plugin_id
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    {
        return Err(PluginError::new(
            "bad_args",
            format!(
                "недопустимый id плагина {plugin_id:?}: только строчные латинские, цифры, '-' и '_'"
            ),
        ));
    }
    Ok(())
}

/// Проверяет ключ настройки: непустой и в пределах лимита.
fn validate_key(key: &str) -> Result<(), PluginError> {
    if key.is_empty() {
        return Err(PluginError::new("bad_args", "пустой ключ настройки"));
    }
    if key.len() > MAX_SETTING_KEY_BYTES {
        return Err(PluginError::new(
            "bad_args",
            format!("ключ настройки длиннее {MAX_SETTING_KEY_BYTES} Б"),
        ));
    }
    Ok(())
}

/// Читает JSON-объект настроек. Отсутствие/битый/oversized файл → пустой объект (как
/// [`super::settings::SettingsStore`]): настройки не должны ронять GUI.
fn read_map(path: &Path) -> Map<String, Value> {
    match std::fs::metadata(path) {
        Ok(meta) if meta.len() > MAX_SETTINGS_FILE_BYTES => {
            eprintln!(
                "{} ({} Б) превышает лимит {MAX_SETTINGS_FILE_BYTES} — беру пустые настройки",
                path.display(),
                meta.len()
            );
            Map::new()
        }
        Ok(_) => match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice::<Map<String, Value>>(&bytes).unwrap_or_else(|e| {
                eprintln!("{} не разобран: {e}", path.display());
                Map::new()
            }),
            Err(e) => {
                eprintln!("{} не прочитан: {e}", path.display());
                Map::new()
            }
        },
        Err(_) => Map::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_store() -> (tempfile::TempDir, PluginSettingsStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = PluginSettingsStore::new(dir.path());
        (dir, store)
    }

    #[test]
    fn missing_file_yields_empty() {
        let (_dir, mut store) = temp_store();
        assert_eq!(store.get("p", "k").unwrap(), None);
    }

    #[test]
    fn set_persists_and_reads_back() {
        let (dir, mut store) = temp_store();
        store.set("p", "n", json!(1)).unwrap();
        store.set("p", "o", json!({ "a": [true, null] })).unwrap();

        // Файл на диске — JSON-объект.
        let path = dir.path().join("p.json");
        assert!(path.exists(), "файл настроек создан");
        assert!(!dir.path().join("p.json.tmp").exists(), "temp убран");

        // Новый инстанс из того же пути видит значения (персист).
        let mut fresh = PluginSettingsStore::new(dir.path());
        assert_eq!(fresh.get("p", "n").unwrap(), Some(json!(1)));
        assert_eq!(
            fresh.get("p", "o").unwrap(),
            Some(json!({ "a": [true, null] }))
        );
    }

    #[test]
    fn plugins_are_isolated() {
        let (_dir, mut store) = temp_store();
        store.set("a", "k", json!("a")).unwrap();
        store.set("b", "k", json!("b")).unwrap();
        assert_eq!(store.get("a", "k").unwrap(), Some(json!("a")));
        assert_eq!(store.get("b", "k").unwrap(), Some(json!("b")));
    }

    #[test]
    fn empty_and_long_keys_are_rejected() {
        let (_dir, mut store) = temp_store();
        assert_eq!(store.get("p", "").unwrap_err().code, "bad_args");
        assert_eq!(store.set("p", "", json!(1)).unwrap_err().code, "bad_args");

        let long = "k".repeat(MAX_SETTING_KEY_BYTES + 1);
        assert_eq!(store.get("p", &long).unwrap_err().code, "bad_args");
    }

    #[test]
    fn oversized_value_is_rejected() {
        let (_dir, mut store) = temp_store();
        let big = "x".repeat(MAX_SETTING_VALUE_BYTES + 1);
        assert_eq!(
            store.set("p", "k", json!(big)).unwrap_err().code,
            "too_large"
        );
    }

    #[test]
    fn corrupt_and_oversized_files_fall_back_to_empty() {
        let dir = tempfile::tempdir().unwrap();

        std::fs::write(dir.path().join("corrupt.json"), b"{ not json").unwrap();
        let mut store = PluginSettingsStore::new(dir.path());
        assert_eq!(store.get("corrupt", "k").unwrap(), None);

        let f = std::fs::File::create(dir.path().join("huge.json")).unwrap();
        f.set_len(MAX_SETTINGS_FILE_BYTES + 1).unwrap();
        drop(f);
        let mut store = PluginSettingsStore::new(dir.path());
        assert_eq!(store.get("huge", "k").unwrap(), None);
    }

    #[test]
    fn remove_plugin_deletes_file() {
        let (dir, mut store) = temp_store();
        store.set("p", "k", json!(1)).unwrap();
        assert!(dir.path().join("p.json").exists());
        store.remove_plugin("p").unwrap();
        assert!(!dir.path().join("p.json").exists());
        assert_eq!(store.get("p", "k").unwrap(), None);
    }

    #[test]
    fn all_returns_saved_values_and_isolates_plugins() {
        let (_dir, mut store) = temp_store();
        store.set("a", "k", json!(1)).unwrap();
        store.set("a", "s", json!("x")).unwrap();
        store.set("b", "k", json!(2)).unwrap();

        assert_eq!(store.all("a").unwrap(), json!({ "k": 1, "s": "x" }));
        assert_eq!(store.all("b").unwrap(), json!({ "k": 2 }), "изоляция по id");
        assert_eq!(
            store.all("missing").unwrap(),
            json!({}),
            "нет файла → пустой объект"
        );
    }

    #[test]
    fn unsafe_plugin_id_is_rejected() {
        let (dir, mut store) = temp_store();
        for bad in ["../evil", "a/b", "a\\b", "", ".", "..", "A", "a b"] {
            assert_eq!(
                store.get(bad, "k").unwrap_err().code,
                "bad_args",
                "get {bad:?}"
            );
            assert_eq!(
                store.set(bad, "k", json!(1)).unwrap_err().code,
                "bad_args",
                "set {bad:?}"
            );
            assert_eq!(store.all(bad).unwrap_err().code, "bad_args", "all {bad:?}");
            assert_eq!(
                store.remove_plugin(bad).unwrap_err().code,
                "bad_args",
                "remove {bad:?}"
            );
        }
        // Traversal не сработал: вне и внутри каталога настроек ничего не создано.
        assert!(!dir.path().parent().unwrap().join("evil.json").exists());
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            0,
            "каталог настроек пуст"
        );
    }
}
