//! Чтение/запись секции `ai` в `%APPDATA%/darmark/config.json` через `SettingsStore` (D11).
//!
//! На Windows настройки лежат в существующем `plugins::settings::SettingsStore` (секция `ai`).
//! На не-Windows сетевой слой AI не собирается (ADR-0033 §2.1) — читаются дефолты, запись
//! не выполняется (заглушка для сборки).
//!
//! Путь конфига передаётся явно (`_from`/`_to`) — так `AiState` инъектируется в тестах и не
//! трогает реальный профиль пользователя.

use std::path::{Path, PathBuf};

use crate::ai::types::AiSettings;
use crate::error::CommandError;

/// Путь конфига приложения. `None` — сохранять некуда (нет `APPDATA`/override).
pub fn config_path() -> Option<PathBuf> {
    crate::plugins::settings::SettingsStore::config_path()
}

/// Читает секцию `ai` из конфига по пути (нет пути/файла/секции → дефолты).
pub fn load_from(path: Option<&Path>) -> AiSettings {
    path.map(|p| crate::plugins::settings::SettingsStore::load(p).ai)
        .unwrap_or_default()
}

/// Пишет секцию `ai`, **не затирая** прочие секции (плагины/недавние файлы). Атомарно.
///
/// Нет пути — no-op (не ошибка команды).
pub fn persist_to(path: Option<&Path>, settings: &AiSettings) -> Result<(), CommandError> {
    let Some(path) = path else {
        return Ok(());
    };
    let mut store = crate::plugins::settings::SettingsStore::load(path);
    store.ai = settings.clone();
    store
        .save(path)
        .map_err(|e| CommandError::io(format!("не сохранить настройки AI: {e}")))
}

/// Читает настройки из конфига по пути по умолчанию.
pub fn load() -> AiSettings {
    load_from(config_path().as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_config_yields_defaults() {
        let path =
            std::env::temp_dir().join(format!("darmark-ai-missing-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let settings = load_from(Some(&path));
        assert_eq!(settings.base_url, "https://routerai.ru/api/v1");
        assert_eq!(settings.model, "deepseek/deepseek-v4.1-flash");
        assert!(!settings.enabled);
    }

    #[test]
    fn none_path_yields_defaults() {
        assert_eq!(load_from(None).model, "deepseek/deepseek-v4.1-flash");
    }

    #[cfg(windows)]
    #[test]
    fn roundtrip_keeps_secret_and_other_sections() {
        use crate::plugins::settings::SettingsStore;

        let dir = std::env::temp_dir().join(format!("darmark-ai-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("config.json");

        let mut store = SettingsStore::default();
        store.set_plugin_enabled("word-count", false);
        store.save(&path).expect("сохранение конфига");

        let settings = AiSettings {
            enabled: true,
            model: "custom/model".to_string(),
            api_key: Some("sk-test-roundtrip".to_string()),
            ..AiSettings::default()
        };
        persist_to(Some(&path), &settings).expect("запись секции ai");

        let loaded = SettingsStore::load(&path);
        assert!(loaded.ai.enabled);
        assert_eq!(loaded.ai.model, "custom/model");
        assert_eq!(loaded.ai.api_key.as_deref(), Some("sk-test-roundtrip"));
        // Секция плагинов не затёрта — persist меняет только `ai`.
        assert!(!loaded.plugin_enabled("word-count"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
