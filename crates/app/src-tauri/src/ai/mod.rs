//! AI-ассистент: Rust-сетевой слой (OpenAI-совместимый API), настройки, скиллы и IPC-команды.
//!
//! Размещение — **в процессе GUI**; сайдкар `darmark-ai.exe` отложен. Вызов
//! провайдера и передача текста спрятаны за трейтом шва [`transport::AiTransport`] (будущий
//! сайдкар — drop-in). Сеть — только здесь (CSP `default-src 'self'`,).
//!
//! Сеть живёт только в [`backend`]; весь остальной модуль (типы, скиллы, гейт,
//! история, harness'ы, команды) платформенно-нейтрален.

pub mod backend;
pub mod commands;
pub mod doc_harness;
pub mod errors;
pub mod gate;
pub mod history;
pub mod plugin_harness;
pub mod settings;
pub mod skills;
pub mod sse;
pub mod state;
pub mod tool_calls;
pub mod tool_loop;
pub mod transport;
pub mod types;

#[cfg(test)]
mod tests;

pub use types::{
    AiDiff, AiEditOp, AiMessage, AiResponse, AiSettings, AiSettingsInput, AiSettingsView, AiSkill,
    AiTestResult, AiUsage, AiValidationResult, HistoryView, ModelView, ProviderSettingsView,
    ProviderView,
};
