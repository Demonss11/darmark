//! Персистентность истории чата: файл `ai-history.json` в каталоге данных.
//!
//! Лимит — [`MAX_HISTORY_MESSAGES`] сообщений (хвост отбрасывается при сохранении).
//! Битый JSON / файл больше [`MAX_HISTORY_BYTES`] / неизвестная версия схемы → пустая
//! сессия + предупреждение (приложение стартует). Запись атомарна (`*.json.tmp` + `rename`).

use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use crate::ai::types::AiMessage;
use crate::error::CommandError;

/// Текущая версия схемы файла истории.
pub(crate) const SCHEMA_VERSION: u32 = 1;

/// Сколько сообщений хранить в истории (последние N).
pub const MAX_HISTORY_MESSAGES: usize = 40;

/// Максимальный размер файла истории в байтах (защита от гигантского файла при старте).
pub const MAX_HISTORY_BYTES: u64 = 2 * 1024 * 1024;

/// Белый список ролей сообщений. Роль вне списка отбрасывается при загрузке.
const ALLOWED_ROLES: &[&str] = &["system", "user", "assistant", "tool"];

/// Файл истории чата: метаданные + сообщения.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryFile {
    /// Версия схемы (для будущей миграции).
    pub schema_version: u32,
    /// Идентификатор сессии чата.
    pub session_id: String,
    /// Время последнего обновления (unix-секунды).
    pub updated_at: u64,
    /// Сообщения чата (последние первыми — новые в конце).
    pub messages: Vec<AiMessage>,
}

impl HistoryFile {
    /// Пустая сессия (нет файла / битый файл / неизвестная схема).
    pub fn empty() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            session_id: String::new(),
            updated_at: 0,
            messages: Vec::new(),
        }
    }
}

/// Путь к файлу истории: `<base>/ai-history.json`. `None` — сохранять некуда.
pub fn history_path(base: Option<&Path>) -> Option<PathBuf> {
    base.map(|dir| dir.join("ai-history.json"))
}

/// Читает историю из файла. Возвращает `(HistoryFile, Option<предупреждение>)`.
///
/// Предупреждение — короткая строка без содержимого сообщений (не логируется).
/// Паники нет: любая проблема → пустая сессия + предупреждение.
pub fn load(base: Option<&Path>) -> (HistoryFile, Option<String>) {
    let Some(path) = history_path(base) else {
        return (HistoryFile::empty(), None);
    };

    match std::fs::metadata(&path) {
        Ok(meta) if meta.len() > MAX_HISTORY_BYTES => {
            let msg = format!(
                "ai-history.json ({} Б) превышает лимит {MAX_HISTORY_BYTES} — начинаю с пустой сессии",
                meta.len()
            );
            (HistoryFile::empty(), Some(msg))
        }
        Ok(_) => match std::fs::read(&path) {
            Ok(bytes) => parse_history(&bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (HistoryFile::empty(), None),
            Err(e) => (
                HistoryFile::empty(),
                Some(format!("ai-history.json не прочитан: {e}")),
            ),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (HistoryFile::empty(), None),
        Err(e) => (
            HistoryFile::empty(),
            Some(format!("ai-history.json недоступен: {e}")),
        ),
    }
}

/// Парсит JSON в `HistoryFile`. Битый JSON / неизвестная схема → пустая сессия + предупреждение.
fn parse_history(bytes: &[u8]) -> (HistoryFile, Option<String>) {
    let file: HistoryFile = match serde_json::from_slice(bytes) {
        Ok(f) => f,
        Err(_) => {
            // Текст serde-ошибки может содержать фрагмент содержимого файла — не пробрасываем
            // его наружу: сообщения чата не должны попадать в логи/предупреждения (приватность).
            return (
                HistoryFile::empty(),
                Some("ai-history.json не разобран (битый JSON)".to_string()),
            );
        }
    };

    if file.schema_version != SCHEMA_VERSION {
        return (
            HistoryFile::empty(),
            Some(format!(
                "ai-history.json: неизвестная версия схемы {}",
                file.schema_version
            )),
        );
    }

    // Фильтруем сообщения по белому списку ролей.
    let messages: Vec<AiMessage> = file
        .messages
        .into_iter()
        .filter(|m| ALLOWED_ROLES.contains(&m.role.as_str()))
        .collect();

    (
        HistoryFile {
            schema_version: file.schema_version,
            session_id: file.session_id,
            updated_at: file.updated_at,
            messages,
        },
        None,
    )
}

/// Сохраняет историю атомарно: временный файл + `rename`.
///
/// Перед записью хвост старых сообщений отбрасывается, пока не останется
/// ≤ [`MAX_HISTORY_MESSAGES`] (последнее сообщение всегда сохраняется).
pub fn save(base: Option<&Path>, history: &HistoryFile) -> Result<(), CommandError> {
    let Some(path) = history_path(base) else {
        return Ok(());
    };

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| CommandError::io(format!("не создать каталог: {e}")))?;
    }

    // Обрезаем до лимита: последнее сообщение всегда сохраняется. Роли вне белого списка
    // отбрасываем симметрично `load`, чтобы файл не содержал недопустимых ролей.
    let allowed: Vec<AiMessage> = history
        .messages
        .iter()
        .filter(|m| ALLOWED_ROLES.contains(&m.role.as_str()))
        .cloned()
        .collect();
    let messages = if allowed.len() > MAX_HISTORY_MESSAGES {
        allowed[allowed.len() - MAX_HISTORY_MESSAGES..].to_vec()
    } else {
        allowed
    };

    let trimmed = HistoryFile {
        schema_version: history.schema_version,
        session_id: history.session_id.clone(),
        updated_at: history.updated_at,
        messages,
    };

    let bytes = serde_json::to_vec_pretty(&trimmed)
        .map_err(|e| CommandError::io(format!("не сериализовать историю: {e}")))?;
    // Инвариант «сохранённый файл всегда загружаем»: при превышении лимита байт отбрасываем
    // старые сообщения с начала, пока файл не уложится (последнее сообщение всегда остаётся).
    let bytes = fit_within_byte_limit(&trimmed, bytes)?;
    let tmp = path.with_extension("json.tmp");
    if let Err(e) = std::fs::write(&tmp, bytes) {
        let _ = std::fs::remove_file(&tmp);
        return Err(CommandError::io(format!("не записать временный файл: {e}")));
    }
    match std::fs::rename(&tmp, &path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(CommandError::io(format!("не сохранить историю: {e}")))
        }
    }
}

/// Укладывает сериализованную историю в [`MAX_HISTORY_BYTES`], отбрасывая старые сообщения
/// с начала, пока файл не уместится; последнее сообщение сохраняется всегда.
///
/// Инвариант: сохранённый файл обязан загружаться `load` без срабатывания лимита размера —
/// иначе сессия «переживала» бы запись и терялась при следующем запуске.
fn fit_within_byte_limit(
    history: &HistoryFile,
    mut bytes: Vec<u8>,
) -> Result<Vec<u8>, CommandError> {
    if bytes.len() as u64 <= MAX_HISTORY_BYTES {
        return Ok(bytes);
    }
    let mut messages = history.messages.clone();
    while messages.len() > 1 {
        messages.remove(0);
        let candidate = HistoryFile {
            schema_version: history.schema_version,
            session_id: history.session_id.clone(),
            updated_at: history.updated_at,
            messages: messages.clone(),
        };
        bytes = serde_json::to_vec_pretty(&candidate)
            .map_err(|e| CommandError::io(format!("не сериализовать историю: {e}")))?;
        if bytes.len() as u64 <= MAX_HISTORY_BYTES {
            return Ok(bytes);
        }
    }
    // Одно сообщение всё ещё больше лимита (недостижимо при лимите текста запроса 100 КиБ):
    // пишем как есть — `load` отклонит файл с понятным предупреждением, тихой потери нет.
    Ok(bytes)
}

/// Мьютекс для тестов, меняющих переменные окружения (параллельные тесты не должны конфликтовать).
/// Общий на весь модуль `ai`: `commands` и `history` сериализуют доступ к `DARMARK_CONFIG_PATH`.
#[cfg(test)]
pub(crate) fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Временный каталог для теста (уникальный на процесс + имя).
    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("darmark-history-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn make_messages(count: usize) -> Vec<AiMessage> {
        (0..count)
            .map(|i| AiMessage {
                role: if i % 2 == 0 {
                    "user".to_string()
                } else {
                    "assistant".to_string()
                },
                content: format!("сообщение {i}"),
            })
            .collect()
    }

    #[test]
    fn history_roundtrip_trims_to_limit() {
        let dir = temp_dir("roundtrip");

        let history = HistoryFile {
            schema_version: SCHEMA_VERSION,
            session_id: "test-session".to_string(),
            updated_at: 1234567890,
            messages: make_messages(41),
        };
        save(Some(&dir), &history).expect("сохранение");

        let (loaded, warn) = load(Some(&dir));
        assert!(warn.is_none(), "предупреждения быть не должно: {warn:?}");
        assert_eq!(
            loaded.messages.len(),
            MAX_HISTORY_MESSAGES,
            "обрезано до лимита"
        );
        assert_eq!(
            loaded.messages.last().unwrap().content,
            "сообщение 40",
            "последнее сообщение сохранено"
        );
        assert_eq!(loaded.session_id, "test-session");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn history_survives_corrupt_file() {
        let dir = temp_dir("corrupt");
        let _path = history_path(Some(&dir)).unwrap();
        std::fs::write(&_path, b"{ not json").unwrap();

        let (loaded, warn) = load(Some(&dir));
        assert!(warn.is_some(), "должно быть предупреждение");
        assert!(loaded.messages.is_empty(), "пустая сессия");
        assert_eq!(loaded.session_id, "");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn history_rejects_oversized_file() {
        let dir = temp_dir("oversized");
        let path = history_path(Some(&dir)).unwrap();
        let f = std::fs::File::create(&path).unwrap();
        f.set_len(MAX_HISTORY_BYTES + 1).unwrap();
        drop(f);

        let (loaded, warn) = load(Some(&dir));
        assert!(warn.is_some(), "должно быть предупреждение");
        assert!(loaded.messages.is_empty(), "пустая сессия");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn history_write_is_atomic() {
        let dir = temp_dir("atomic");
        let path = history_path(Some(&dir)).unwrap();
        let tmp_path = path.with_extension("json.tmp");

        let history = HistoryFile {
            schema_version: SCHEMA_VERSION,
            session_id: "atomic".to_string(),
            updated_at: 0,
            messages: make_messages(3),
        };
        save(Some(&dir), &history).expect("сохранение");

        assert!(path.exists(), "файл истории создан");
        assert!(!tmp_path.exists(), "временный файл убран после rename");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn history_path_follows_data_dir() {
        let _guard = env_lock();

        let dir = temp_dir("path");
        let config_path = dir.join("config.json");

        // Устанавливаем override через DARMARK_CONFIG_PATH.
        std::env::set_var("DARMARK_CONFIG_PATH", &config_path);

        let data_dir =
            crate::plugins::settings::SettingsStore::data_dir().expect("data_dir через override");
        let history = history_path(Some(&data_dir)).expect("путь истории");

        // Восстанавливаем переменную окружения.
        std::env::remove_var("DARMARK_CONFIG_PATH");

        assert_eq!(
            history,
            dir.join("ai-history.json"),
            "путь истории — в каталоге данных"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_filters_disallowed_roles() {
        let dir = temp_dir("roles");

        let history = HistoryFile {
            schema_version: SCHEMA_VERSION,
            session_id: "roles".to_string(),
            updated_at: 0,
            messages: vec![
                AiMessage {
                    role: "user".to_string(),
                    content: "ok".to_string(),
                },
                AiMessage {
                    role: "evil".to_string(),
                    content: "bad".to_string(),
                },
                AiMessage {
                    role: "assistant".to_string(),
                    content: "ok2".to_string(),
                },
            ],
        };
        save(Some(&dir), &history).expect("сохранение");

        let (loaded, _) = load(Some(&dir));
        assert_eq!(loaded.messages.len(), 2, "роль вне списка отброшена");
        assert!(loaded.messages.iter().all(|m| m.role != "evil"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_unknown_schema_returns_empty() {
        let dir = temp_dir("schema");
        let path = history_path(Some(&dir)).unwrap();

        let raw = serde_json::json!({
            "schema_version": 999,
            "session_id": "test",
            "updated_at": 0,
            "messages": [{"role": "user", "content": "hi"}]
        });
        std::fs::write(&path, serde_json::to_vec(&raw).unwrap()).unwrap();

        let (loaded, warn) = load(Some(&dir));
        assert!(warn.is_some(), "должно быть предупреждение");
        assert!(loaded.messages.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_fits_within_byte_limit() {
        let dir = temp_dir("bytes");

        // Три сообщения примерно по 0.8 МБ: вместе не влезают в лимит 2 МиБ, значит `save`
        // обязан отбросить старые и сохранить хвост, уложившись в лимит.
        let chunk = "я".repeat(400_000);
        let history = HistoryFile {
            schema_version: SCHEMA_VERSION,
            session_id: "bytes".to_string(),
            updated_at: 0,
            messages: vec![
                AiMessage {
                    role: "user".to_string(),
                    content: chunk.clone(),
                },
                AiMessage {
                    role: "assistant".to_string(),
                    content: chunk.clone(),
                },
                AiMessage {
                    role: "user".to_string(),
                    content: format!("{chunk}последнее"),
                },
            ],
        };
        save(Some(&dir), &history).expect("сохранение");

        let path = history_path(Some(&dir)).unwrap();
        let size = std::fs::metadata(&path).expect("метаданные").len();
        assert!(
            size <= MAX_HISTORY_BYTES,
            "файл уложен в лимит байт: {size} > {MAX_HISTORY_BYTES}"
        );

        // Инвариант «сохранённый файл всегда загружаем»: без предупреждения об oversized.
        let (loaded, warn) = load(Some(&dir));
        assert!(
            warn.is_none(),
            "файл в лимите грузится без предупреждения: {warn:?}"
        );
        assert!(!loaded.messages.is_empty(), "хвост сохранён");
        assert!(
            loaded
                .messages
                .last()
                .unwrap()
                .content
                .ends_with("последнее"),
            "последнее сообщение сохраняется всегда"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_filters_disallowed_roles() {
        let dir = temp_dir("save-roles");

        let history = HistoryFile {
            schema_version: SCHEMA_VERSION,
            session_id: "save-roles".to_string(),
            updated_at: 0,
            messages: vec![
                AiMessage {
                    role: "system".to_string(),
                    content: "ok".to_string(),
                },
                AiMessage {
                    role: "root".to_string(),
                    content: "не должно попасть на диск".to_string(),
                },
            ],
        };
        save(Some(&dir), &history).expect("сохранение");

        // Проверяем сам файл: недопустимой роли нет даже на диске.
        let path = history_path(Some(&dir)).unwrap();
        let raw = std::fs::read_to_string(&path).expect("файл истории");
        assert!(
            !raw.contains("не должно попасть"),
            "роль отброшена при сохранении"
        );

        let (loaded, _) = load(Some(&dir));
        assert_eq!(loaded.messages.len(), 1);
        assert_eq!(loaded.messages[0].role, "system");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
