//! Чтение/запись секции `ai` в `%APPDATA%/darmark/config.json` через `SettingsStore` (D11).
//!
//! Настройки лежат в общем `plugins::settings::SettingsStore` (секция `ai`) — тот же
//! конфиг, что и у плагинов. Путь конфига передаётся явно (`_from`/`_to`) — так `AiState`
//! инъектируется в тестах и не трогает реальный профиль пользователя.

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

/// Читает секцию `ai` и перезаписывает конфиг, если разобрана legacy-схема (ADR-0035 §2.8).
///
/// Идемпотентно: после перезаписи в конфиге уже новый формат (`providers`), и повторная
/// загрузка миграцию не запускает. Сбой перезаписи не роняет приложение (в памяти остаются
/// разобранные настройки).
pub fn load_and_migrate(path: Option<&Path>) -> AiSettings {
    let Some(path) = path else {
        return AiSettings::default();
    };
    let store = crate::plugins::settings::SettingsStore::load(path);
    if !store.ai.migrated_from_legacy {
        return store.ai;
    }
    let migrated = store.ai;
    if let Err(error) = persist_to(Some(path), &migrated) {
        eprintln!(
            "не перезаписать конфиг после миграции AI ({}): {}",
            path.display(),
            error.message
        );
    }
    migrated
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
    use crate::ai::history::env_lock;

    #[test]
    fn missing_config_yields_defaults() {
        let path =
            std::env::temp_dir().join(format!("darmark-ai-missing-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let settings = load_from(Some(&path));
        assert_eq!(settings.active_provider, "routerai");
        assert_eq!(settings.active_model, "deepseek/deepseek-v4.1-flash");
        assert!(!settings.enabled);
    }

    #[test]
    fn none_path_yields_defaults() {
        assert_eq!(load_from(None).active_model, "deepseek/deepseek-v4.1-flash");
    }

    #[test]
    fn data_dir_follows_config_override() {
        let _guard = env_lock();
        let dir = std::env::temp_dir().join(format!("darmark-ai-data-dir-{}", std::process::id()));
        let config_path = dir.join("config.json");
        std::env::set_var("DARMARK_CONFIG_PATH", &config_path);
        assert_eq!(
            crate::plugins::settings::SettingsStore::data_dir().as_deref(),
            Some(dir.as_path())
        );
        std::env::remove_var("DARMARK_CONFIG_PATH");
    }

    #[test]
    fn roundtrip_keeps_secret_and_other_sections() {
        use crate::plugins::settings::SettingsStore;

        let dir = std::env::temp_dir().join(format!("darmark-ai-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("config.json");

        let mut store = SettingsStore::default();
        store.set_plugin_enabled("word-count", false);
        store.save(&path).expect("сохранение конфига");

        let mut settings = AiSettings {
            enabled: true,
            active_model: "custom/model".to_string(),
            ..AiSettings::default()
        };
        settings
            .providers
            .get_mut("routerai")
            .unwrap()
            .settings
            .api_key = Some("sk-test-roundtrip".to_string());
        persist_to(Some(&path), &settings).expect("запись секции ai");

        let loaded = SettingsStore::load(&path);
        assert!(loaded.ai.enabled);
        assert_eq!(loaded.ai.active_model, "custom/model");
        assert_eq!(
            loaded.ai.providers["routerai"].settings.api_key.as_deref(),
            Some("sk-test-roundtrip")
        );
        assert!(!loaded.plugin_enabled("word-count"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_and_migrate_rewrites_config() {
        let dir = std::env::temp_dir().join(format!("darmark-ai-migrate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        let legacy = r#"{
            "ai": {
                "enabled": true,
                "provider": "openai-compatible",
                "base_url": "https://legacy.test/v1",
                "model": "legacy/model",
                "api_key": "sk-legacy"
            }
        }"#;
        std::fs::write(&path, legacy).unwrap();

        let settings = load_and_migrate(Some(&path));
        assert!(settings.enabled);
        assert_eq!(settings.active_provider, "openai-compatible");
        assert_eq!(settings.active_model, "legacy/model");

        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(
            raw.contains("\"providers\""),
            "конфиг перезаписан в новый формат: {raw}"
        );
        assert!(
            raw.contains("\"baseURL\""),
            "использованы имена полей Custom: {raw}"
        );

        let reloaded = load_and_migrate(Some(&path));
        assert!(
            !reloaded.migrated_from_legacy,
            "повторная загрузка не мигрирует"
        );
        assert_eq!(reloaded.active_provider, "openai-compatible");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
