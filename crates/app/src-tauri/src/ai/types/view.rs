//! Проекции настроек для фронтенда (`View`) и вход от него (`Input`).
//!
//! Секреты наружу не отдаются: в `ProviderSettingsView` вместо `api_key` — признак `has_key`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::settings::{resolve_api_key, AiSettings};


#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AiSettingsView {
    pub enabled: bool,
    pub active_provider: String,
    pub active_model: String,
    pub max_tokens: u32,
    pub temperature: f32,
    pub consent_document_send: bool,
    pub providers: Vec<ProviderView>,
}


#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ProviderView {
    pub id: String,
    pub name: String,
    pub env: Vec<String>,
    pub settings: ProviderSettingsView,
    pub headers: BTreeMap<String, String>,
    pub body: serde_json::Map<String, serde_json::Value>,
    pub models: Vec<ModelView>,
}


#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ProviderSettingsView {
    pub base_url: String,
    pub timeout: Option<u64>,
    pub has_key: bool,
}


#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModelView {
    pub id: String,
    pub name: String,
    pub model_id: Option<String>,
    pub disabled: Option<bool>,
}

impl From<&AiSettings> for AiSettingsView {
    fn from(settings: &AiSettings) -> Self {
        let providers = settings
            .providers
            .iter()
            .map(|(id, provider)| ProviderView {
                id: id.clone(),
                name: provider.name.clone(),
                env: provider.env.clone(),
                settings: ProviderSettingsView {
                    base_url: provider.settings.base_url.clone(),
                    timeout: provider.settings.timeout,
                    has_key: resolve_api_key(provider, |name| std::env::var(name).ok()).is_some(),
                },
                headers: provider.headers.clone(),
                body: provider.body.clone(),
                models: provider
                    .models
                    .iter()
                    .map(|(model_id, entry)| ModelView {
                        id: model_id.clone(),
                        name: entry.name.clone(),
                        model_id: entry.model_id.clone(),
                        disabled: entry.disabled,
                    })
                    .collect(),
            })
            .collect();
        Self {
            enabled: settings.enabled,
            active_provider: settings.active_provider.clone(),
            active_model: settings.active_model.clone(),
            max_tokens: settings.max_tokens,
            temperature: settings.temperature,
            consent_document_send: settings.consent_document_send,
            providers,
        }
    }
}


#[derive(Clone, Debug, Deserialize)]
pub struct AiSettingsInput {
    pub enabled: bool,
    pub active_provider: String,
    pub active_model: String,
    pub max_tokens: u32,
    pub temperature: f32,
    pub consent_document_send: bool,
    pub providers: Vec<ProviderInput>,
}


#[derive(Clone, Debug, Deserialize)]
pub struct ProviderInput {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub env: Vec<String>,
    pub settings: ProviderSettingsInput,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub body: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    pub models: Vec<ModelInput>,
}


#[derive(Clone, Debug, Deserialize)]
pub struct ProviderSettingsInput {
    pub base_url: String,
    #[serde(default)]
    pub timeout: Option<u64>,
}


#[derive(Clone, Debug, Deserialize)]
pub struct ModelInput {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub disabled: Option<bool>,
}
