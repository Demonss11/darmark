//! AI-ассистент: Rust-сетевой слой (OpenAI-совместимый API), настройки, скиллы и IPC-команды.
//!
//! Размещение — **в процессе GUI** (ADR-0034); сайдкар `darmark-ai.exe` отложен. Вызов
//! провайдера и передача текста спрятаны за трейтом шва [`transport::AiTransport`] (будущий
//! сайдкар — drop-in). Сеть — только здесь (CSP `default-src 'self'`, ADR-0032/0033).
//!
//! Модуль собирается на всех платформах (типы/скиллы/гейт/команды), но сетевой бэкенд —
//! только на Windows ([`backend`]); на не-Windows команды возвращают заглушку.

pub mod backend;
pub mod commands;
pub mod errors;
pub mod gate;
pub mod settings;
pub mod skills;
pub mod sse;
pub mod state;
pub mod transport;
pub mod types;

pub use types::{
    AiMessage, AiResponse, AiSettings, AiSettingsInput, AiSettingsView, AiSkill, AiTestResult,
    AiUsage,
};
