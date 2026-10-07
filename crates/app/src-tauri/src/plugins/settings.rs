//! `SettingsStore` — конфиг приложения и плагинов (§12 DESIGN_DOC, Фаза 3 TZ-H2).
//!
//! Файл `%APPDATA%/darmark/config.json`. Хранит вкл/выкл плагинов, согласие на permissions и
//! список недавних файлов (путь + время, 20). Создание каталога `%APPDATA%/darmark/` здесь же
//! закрывает хвост `tasks/архив/TZ-H1.md` п.8.
//!
//! Чтение терпимо к отсутствию/битому файлу (GUI продолжает работу), но **не** молча к реальной
//! IO-ошибке и не без ограничения размера; запись атомарна (временный файл + `rename`), чтобы
//! прерывание не усекло конфиг.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Сколько недавних файлов хранить.
pub const RECENT_FILES_LIMIT: usize = 20;

/// Лимит размера `config.json` (защита от гигантского файла при старте).
pub const MAX_CONFIG_BYTES: u64 = 4 * 1024 * 1024;

/// Настройки одного плагина.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PluginSettings {
    /// Включён ли плагин. По умолчанию — да (выключение явное).
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Разрешения, на которые пользователь дал согласие (§8/§12). Фильтрация по согласию —
    /// менеджер UI (Фаза 5).
    #[serde(default)]
    pub granted_permissions: Vec<String>,
}

impl Default for PluginSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            granted_permissions: Vec::new(),
        }
    }
}

/// Запись недавнего файла: путь + время открытия (unix-секунды), §12.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentFile {
    pub path: String,
    pub opened_at: u64,
}

/// Корневой конфиг приложения.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SettingsStore {
    /// Настройки плагинов по id.
    #[serde(default)]
    pub plugins: HashMap<String, PluginSettings>,
    /// Недавние файлы, новейшие первыми (≤ [`RECENT_FILES_LIMIT`]).
    #[serde(default)]
    pub recent_files: Vec<RecentFile>,
}

impl SettingsStore {
    /// Путь конфига: `DARMARK_CONFIG_PATH` (override для тестов/E2E) либо
    /// `%APPDATA%/darmark/config.json`. `None`, если ни override, ни `APPDATA` не заданы.
    pub fn config_path() -> Option<PathBuf> {
        if let Some(path) = std::env::var_os("DARMARK_CONFIG_PATH") {
            return Some(PathBuf::from(path));
        }
        let base = std::env::var_os("APPDATA")?;
        Some(PathBuf::from(base).join("darmark").join("config.json"))
    }

    /// Читает конфиг. Отсутствие файла — пустые настройки; реальная IO-ошибка/битый JSON
    /// логируются (конфиг не должен ронять приложение). Список недавних нормализуется.
    pub fn load(path: &Path) -> Self {
        let mut settings = match std::fs::metadata(path) {
            Ok(meta) if meta.len() > MAX_CONFIG_BYTES => {
                eprintln!(
                    "config.json ({} Б) превышает лимит {MAX_CONFIG_BYTES} — беру дефолты",
                    meta.len()
                );
                Self::default()
            }
            Ok(_) => match std::fs::read(path) {
                Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
                    eprintln!("config.json не разобран ({}): {e}", path.display());
                    Self::default()
                }),
                Err(e) => {
                    eprintln!("config.json не прочитан ({}): {e}", path.display());
                    Self::default()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(e) => {
                eprintln!("config.json недоступен ({}): {e}", path.display());
                Self::default()
            }
        };
        settings.normalize_recent();
        settings
    }

    /// Сохраняет конфиг атомарно: временный файл в том же каталоге + `rename`. Создаёт каталог
    /// `%APPDATA%/darmark/` при необходимости. Прерывание записи не усекает рабочий конфиг.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let bytes = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, bytes)?;
        match std::fs::rename(&tmp, path) {
            Ok(()) => Ok(()),
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                Err(e)
            }
        }
    }

    /// Включён ли плагин (неизвестный — включён по умолчанию).
    pub fn plugin_enabled(&self, id: &str) -> bool {
        self.plugins.get(id).map(|p| p.enabled).unwrap_or(true)
    }

    /// Фиксирует тумблер вкл/выкл.
    pub fn set_plugin_enabled(&mut self, id: &str, enabled: bool) {
        self.plugins.entry(id.to_string()).or_default().enabled = enabled;
    }

    /// Согласие на разрешения плагина (§12). Пусто — согласие ещё не выдано.
    pub fn granted_permissions(&self, id: &str) -> Vec<String> {
        self.plugins
            .get(id)
            .map(|p| p.granted_permissions.clone())
            .unwrap_or_default()
    }

    /// Записывает согласие пользователя на разрешения плагина.
    pub fn set_granted_permissions(&mut self, id: &str, permissions: Vec<String>) {
        self.plugins
            .entry(id.to_string())
            .or_default()
            .granted_permissions = permissions;
    }

    /// Добавляет файл в начало списка недавних: дедупликация, лимит [`RECENT_FILES_LIMIT`].
    pub fn push_recent_file(&mut self, path: impl Into<String>) {
        let path = path.into();
        self.recent_files.retain(|recent| recent.path != path);
        self.recent_files.insert(
            0,
            RecentFile {
                path,
                opened_at: now_secs(),
            },
        );
        self.recent_files.truncate(RECENT_FILES_LIMIT);
    }

    /// Нормализует список недавних из файла: дедупликация по пути и обрезка до лимита.
    fn normalize_recent(&mut self) {
        let mut seen = std::collections::HashSet::new();
        self.recent_files
            .retain(|recent| seen.insert(recent.path.clone()));
        self.recent_files.truncate(RECENT_FILES_LIMIT);
    }
}

fn default_true() -> bool {
    true
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("darmark-settings-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("config.json")
    }

    #[test]
    fn missing_file_yields_defaults() {
        let settings = SettingsStore::load(Path::new("нет-такого-файла.json"));
        assert!(settings.plugin_enabled("any"));
        assert!(settings.recent_files.is_empty());
    }

    #[test]
    fn save_load_roundtrip_creates_parent_dir() {
        let path = temp_path("roundtrip");
        let mut settings = SettingsStore::default();
        settings.set_plugin_enabled("word-count", false);
        settings.set_granted_permissions("word-count", vec!["document:read".into()]);
        settings.push_recent_file("C:/tmp/a.md");
        settings.save(&path).unwrap();

        assert!(path.exists(), "каталог %APPDATA%/darmark создан");
        assert!(
            !path.with_extension("json.tmp").exists(),
            "временный файл убран после rename"
        );
        let loaded = SettingsStore::load(&path);
        assert!(!loaded.plugin_enabled("word-count"));
        assert_eq!(
            loaded.granted_permissions("word-count"),
            vec!["document:read"]
        );
        assert_eq!(loaded.recent_files.len(), 1);
        assert_eq!(loaded.recent_files[0].path, "C:/tmp/a.md");
        assert!(loaded.recent_files[0].opened_at > 0);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn corrupt_file_falls_back_to_defaults() {
        let path = temp_path("corrupt");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"{ not json").unwrap();
        let settings = SettingsStore::load(&path);
        assert!(settings.plugins.is_empty());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn oversized_config_falls_back_to_defaults() {
        let path = temp_path("oversized");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let f = std::fs::File::create(&path).unwrap();
        f.set_len(MAX_CONFIG_BYTES + 1).unwrap();
        drop(f);
        assert!(SettingsStore::load(&path).plugins.is_empty());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn recent_files_dedup_cap_and_normalize_on_load() {
        let mut settings = SettingsStore::default();
        for i in 0..25 {
            settings.push_recent_file(format!("file-{i}.md"));
        }
        assert_eq!(settings.recent_files.len(), RECENT_FILES_LIMIT);
        assert_eq!(
            settings.recent_files[0].path, "file-24.md",
            "новейший — первым"
        );

        // Повторное открытие поднимает файл наверх без дублей.
        let before = settings.recent_files.len();
        settings.push_recent_file("file-10.md");
        assert_eq!(settings.recent_files[0].path, "file-10.md");
        assert_eq!(settings.recent_files.len(), before);
        assert_eq!(
            settings
                .recent_files
                .iter()
                .filter(|r| r.path == "file-10.md")
                .count(),
            1
        );

        // Нормализация из «грязного» файла: дубли и переполнение убираются при load.
        let path = temp_path("normalize");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let raw: Vec<RecentFile> = (0..30)
            .map(|i| RecentFile {
                path: format!("f-{}.md", i % 5),
                opened_at: i,
            })
            .collect();
        let json = serde_json::to_vec(&serde_json::json!({ "recent_files": raw })).unwrap();
        std::fs::write(&path, json).unwrap();
        let loaded = SettingsStore::load(&path);
        assert_eq!(loaded.recent_files.len(), 5, "дубли схлопнуты");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
