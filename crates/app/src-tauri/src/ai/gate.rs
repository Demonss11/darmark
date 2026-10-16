//! Гейт `ai_chat`: проверки **до** сети (ADR-0032, ADR-0034, ADR-0035).
//!
//! Порядок важен и зафиксирован: `enabled → активная пара → ключ → согласие`. Каждая
//! проверка даёт свой код ошибки, чтобы фронт показал правильное состояние (`#ai-dot`).
//! Функции чистые по отношению к сети — тестируются без запросов.

use crate::ai::state::resolve_active;
use crate::ai::types::AiSettings;
use crate::error::{CommandError, ErrorCode};


pub fn check_enabled(settings: &AiSettings) -> Result<(), CommandError> {
    if settings.enabled {
        Ok(())
    } else {
        Err(CommandError::new(
            ErrorCode::AiDisabled,
            "AI-ассистент выключен",
        ))
    }
}


pub fn check_active(settings: &AiSettings) -> Result<(), CommandError> {
    resolve_active(settings).map(|_| ())
}


pub fn check_key(settings: &AiSettings) -> Result<(), CommandError> {
    if resolve_active(settings)?.api_key.is_some() {
        Ok(())
    } else {
        Err(CommandError::new(ErrorCode::AiNoKey, "API-ключ не задан"))
    }
}


pub fn check_consent(settings: &AiSettings) -> Result<(), CommandError> {
    if settings.consent_document_send {
        Ok(())
    } else {
        Err(CommandError::new(
            ErrorCode::AiNoConsent,
            "Нужно согласие на отправку содержимого в облако",
        ))
    }
}


pub fn check_chat(settings: &AiSettings) -> Result<(), CommandError> {
    check_enabled(settings)?;
    check_active(settings)?;
    check_key(settings)?;
    check_consent(settings)
}
