//! Типы контракта AI-ассистента (IPC + конфиг).
//!
//! Разбиты по домену: [`settings`] — конфигурация провайдеров и настройки, [`view`] —
//! проекции `View`/`Input` для фронтенда (без секретов), [`chat`] — сообщения чата,
//! ответ, скиллы, история и типы harness'а.
//!
//! Секрет живёт в `providers[id].settings.api_key` и пишется в `config.json` (секция `ai`,
//! D11), но **никогда** не попадает в проекции для фронтенда — наружу идёт лишь `has_key`.

pub mod chat;
pub mod settings;
pub mod view;

pub use chat::{
    AiDiff, AiEditOp, AiMessage, AiResponse, AiSkill, AiTestResult, AiUsage, AiValidationResult,
    HistoryView,
};
pub use settings::{
    resolve_api_key, AiSettings, ModelEntry, ProviderConfig, ProviderSettings, DEFAULT_BASE_URL,
    DEFAULT_MODEL, DEFAULT_PROVIDER_ID,
};
pub use view::{
    AiSettingsInput, AiSettingsView, ModelInput, ModelView, ProviderInput, ProviderSettingsInput,
    ProviderSettingsView, ProviderView,
};
