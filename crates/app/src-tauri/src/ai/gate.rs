//! Гейт `ai_chat`: проверки **до** сети (ADR-0032, ADR-0034).
//!
//! Порядок важен и зафиксирован: `enabled → ключ → согласие`. Каждая проверка даёт свой
//! код ошибки, чтобы фронт показал правильное состояние (`#ai-dot`). Функции чистые —
//! тестируются без сети.

use crate::ai::types::AiSettings;
use crate::error::{CommandError, ErrorCode};

/// Фича выключена пользователем (`ai.enabled = false`).
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

/// Не задан непустой API-ключ.
pub fn check_key(settings: &AiSettings) -> Result<(), CommandError> {
    if settings.has_key() {
        Ok(())
    } else {
        Err(CommandError::new(ErrorCode::AiNoKey, "API-ключ не задан"))
    }
}

/// Нет согласия на отправку содержимого в облако.
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

/// Полный гейт перед `ai_chat`: включён → ключ → согласие.
pub fn check_chat(settings: &AiSettings) -> Result<(), CommandError> {
    check_enabled(settings)?;
    check_key(settings)?;
    check_consent(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Настройки, проходящие все проверки.
    fn ready() -> AiSettings {
        AiSettings {
            enabled: true,
            consent_document_send: true,
            api_key: Some("sk-test".to_string()),
            ..AiSettings::default()
        }
    }

    #[test]
    fn disabled_reports_ai_disabled() {
        let settings = AiSettings::default();
        assert_eq!(
            check_chat(&settings).unwrap_err().code,
            ErrorCode::AiDisabled
        );
    }

    #[test]
    fn missing_key_reports_ai_no_key() {
        let settings = AiSettings {
            enabled: true,
            api_key: None,
            ..AiSettings::default()
        };
        assert_eq!(check_chat(&settings).unwrap_err().code, ErrorCode::AiNoKey);
    }

    #[test]
    fn missing_consent_reports_ai_no_consent() {
        let settings = AiSettings {
            enabled: true,
            api_key: Some("sk-test".to_string()),
            consent_document_send: false,
            ..AiSettings::default()
        };
        assert_eq!(
            check_chat(&settings).unwrap_err().code,
            ErrorCode::AiNoConsent
        );
    }

    #[test]
    fn gate_order_is_enabled_key_consent() {
        let settings = AiSettings {
            enabled: true,
            ..AiSettings::default()
        };
        assert_eq!(check_chat(&settings).unwrap_err().code, ErrorCode::AiNoKey);
    }

    #[test]
    fn fully_configured_passes() {
        assert!(check_chat(&ready()).is_ok());
    }

    #[test]
    fn test_connection_does_not_require_consent() {
        let settings = AiSettings {
            enabled: true,
            api_key: Some("sk-test".to_string()),
            consent_document_send: false,
            ..AiSettings::default()
        };
        assert!(check_enabled(&settings).is_ok());
        assert!(check_key(&settings).is_ok());
    }
}
