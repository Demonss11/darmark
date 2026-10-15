//! Тесты гейта [`super::super::gate`].

use crate::ai::gate::{check_active, check_chat, check_enabled, check_key};
use crate::ai::types::AiSettings;
use crate::error::ErrorCode;

/// Настройки, проходящие все проверки.
fn ready() -> AiSettings {
    let mut settings = AiSettings {
        enabled: true,
        consent_document_send: true,
        ..AiSettings::default()
    };
    settings
        .providers
        .get_mut("routerai")
        .unwrap()
        .settings
        .api_key = Some("sk-test".to_string());
    settings
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
fn bad_active_reports_bad_settings_before_key() {
    let settings = AiSettings {
        enabled: true,
        active_model: "ghost/model".to_string(),
        ..AiSettings::default()
    };
    assert_eq!(
        check_chat(&settings).unwrap_err().code,
        ErrorCode::AiBadSettings
    );
}

#[test]
fn missing_key_reports_ai_no_key() {
    let mut settings = AiSettings {
        enabled: true,
        ..AiSettings::default()
    };
    settings
        .providers
        .get_mut("routerai")
        .unwrap()
        .settings
        .api_key = None;
    assert_eq!(check_chat(&settings).unwrap_err().code, ErrorCode::AiNoKey);
}

#[test]
fn missing_consent_reports_ai_no_consent() {
    let mut settings = ready();
    settings.consent_document_send = false;
    assert_eq!(
        check_chat(&settings).unwrap_err().code,
        ErrorCode::AiNoConsent
    );
}

#[test]
fn fully_configured_passes() {
    assert!(check_chat(&ready()).is_ok());
}

#[test]
fn test_connection_does_not_require_consent() {
    let mut settings = ready();
    settings.consent_document_send = false;
    assert!(check_enabled(&settings).is_ok());
    assert!(check_active(&settings).is_ok());
    assert!(check_key(&settings).is_ok());
}
