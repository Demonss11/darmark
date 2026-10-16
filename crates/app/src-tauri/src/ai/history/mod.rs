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


pub(crate) const SCHEMA_VERSION: u32 = 1;


pub const MAX_HISTORY_MESSAGES: usize = 40;


pub const MAX_HISTORY_BYTES: u64 = 2 * 1024 * 1024;


const ALLOWED_ROLES: &[&str] = &["system", "user", "assistant", "tool"];


#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryFile {
    
    pub schema_version: u32,
    
    pub session_id: String,
    
    pub updated_at: u64,
    
    pub messages: Vec<AiMessage>,
}

impl HistoryFile {
    
    pub fn empty() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            session_id: String::new(),
            updated_at: 0,
            messages: Vec::new(),
        }
    }
}


pub fn history_path(base: Option<&Path>) -> Option<PathBuf> {
    base.map(|dir| dir.join("ai-history.json"))
}





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





fn parse_history(bytes: &[u8]) -> (HistoryFile, Option<String>) {
    let file: HistoryFile = match serde_json::from_slice(bytes) {
        Ok(f) => f,
        Err(_) => {
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





pub fn save(base: Option<&Path>, history: &HistoryFile) -> Result<(), CommandError> {
    let Some(path) = history_path(base) else {
        return Ok(());
    };

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| CommandError::io(format!("не создать каталог: {e}")))?;
    }
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
    Ok(bytes)
}



#[cfg(test)]
pub(crate) fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}
