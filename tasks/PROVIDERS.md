# Задача: Универсальная конфигурация провайдеров LLM (ADR-0035)

> **Статус:** Не реализовано (проверено 14.10.2026 через explore-агент)
> **Приоритет:** P2
> **Источник:** `docs/adr/0035-ai-provider-config.md` (нормативное)
> **Epic:** 2 из плана v2 (задачи 2.1–2.12)
> **Связанные:** ADR-0033 (HTTP-клиент), ADR-0034 (размещение рантайма), ADR-0038 (санитайзер), DESIGN_DOC §11/§13.2

---

## 1. Цель

Заменить плоскую секцию `ai` с одним провайдером на **map провайдеров** с активной парой
`active_provider`/`active_model` — по образцу `providers` в `opencode.json` (раздел **Custom**).

**Проблема:** текущая схема не позволяет держать несколько провайдеров, задавать произвольные
заголовки/тело, менять точку без потери прежней, выбирать модель как `provider/model`.

---

## 2. Ключевые инварианты (норматив)

| # | Инвариант | Источник |
|---|-----------|----------|
| 1 | **Имена JSON-полей:** `settings.baseURL`, `settings.apiKey`, `settings.timeout`, `models[].modelID`, `models[].disabled` — явные `#[serde(rename)]` + `alias` для legacy | ADR-0035 §2.1 |
| 2 | **Резолв по полям, не строке:** `active_model` обязан быть ключом в `providers[active_provider].models`; иначе `ai_bad_settings` без отправки запроса | ADR-0035 §2.2 |
| 3 | **Секреты:** ключ на провайдера (`settings.apiKey`), приоритет над `env`; наружу — только `has_key`; в `{:?}` — `<redacted>` | ADR-0035 §2.3 |
| 4 | **Валидация `baseURL`:** `https` — всегда; `http` — только loopback; иначе `ai_bad_settings` | ADR-0035 §2.6 |
| 5 | **Согласие по хосту:** диалог именует `name` + хост активного провайдера; смена хоста → сброс `consent_document_send` | ADR-0035 §2.7 |
| 6 | **Миграция:** legacy-секция превращается в первый провайдер, конфиг перезаписывается (идемпотентно) | ADR-0035 §2.8 |
| 7 | **Протокол:** только OpenAI-совместимый; `package` — маркер протокола, не npm-пакет | ADR-0035 §2.1 |
| 8 | **Замороженные контракты:** `#ai-*` id, §13.2, `HOST_API_VERSION` не поднимается | DESIGN_DOC §13.2 |
| 9 | **Три точки `innerHTML`:** рендер ответа — только через `safeHtml.setSanitizedHtml` (уже реализовано, Epic 1) | ADR-0038 §2.1 |

---

## 3. Схема конфигурации

```jsonc
{
  "ai": {
    "enabled": false,
    "active_provider": "routerai",
    "active_model": "deepseek/deepseek-v4.1-flash",
    "consent_document_send": false,
    "max_tokens": 32768,
    "temperature": 0.7,
    "providers": {
      "routerai": {
        "name": "RouterAI",
        "env": ["ROUTERAI_API_KEY"],
        "package": "openai-compatible",
        "settings": {
          "baseURL": "https://routerai.ru/api/v1",
          "apiKey": "",
          "timeout": 30000
        },
        "headers": {},
        "body": {},
        "models": {
          "deepseek/deepseek-v4.1-flash": { "name": "DeepSeek V4.1 Flash" }
        }
      }
    }
  }
}
```

---

## 4. Типы Rust

```rust
// ai/types.rs
pub struct ProviderSettings {
    #[serde(rename = "baseURL", alias = "base_url")]
    pub base_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
}

pub struct ModelEntry {
    pub name: String,
    #[serde(default, rename = "modelID", alias = "model_id", skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
}

pub struct ProviderConfig {
    pub name: String,
    #[serde(default)]
    pub env: Vec<String>,
    pub package: String,
    pub settings: ProviderSettings,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub body: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    pub models: BTreeMap<String, ModelEntry>,
}

pub struct AiSettings {
    pub enabled: bool,
    pub active_provider: String,
    pub active_model: String,
    pub consent_document_send: bool,
    pub max_tokens: u32,
    pub temperature: f32,
    pub providers: BTreeMap<String, ProviderConfig>,
    #[serde(skip)]
    pub migrated_from_legacy: bool,
}
```

---

## 5. Задачи (TDD: красный → зелёный → коммит)

Все задачи идут строго последовательно (2.1 блокирует 2.2 и т.д.).

### 2.1 Типы провайдеров и serde-имена
- `ai/types.rs`: `ProviderConfig`, `ProviderSettings`, `ModelEntry` с явными `#[serde(rename)]` + `alias`
- Тесты: `provider_settings_use_custom_field_names`, `provider_settings_accept_legacy_aliases`, `model_entry_serializes_model_id_and_disabled`

### 2.2 `AiSettings` в формате providers + legacy-чтение
- `ai/types.rs`: ручной `Deserialize for AiSettings` через `Helper` с `Option`-полями
- `Default`: провайдер `routerai`, `base_url = "https://routerai.ru/api/v1"`, `enabled = false`
- Тесты: `legacy_section_deserializes_into_providers`, `new_format_deserializes_without_migration`, `missing_fields_fall_back_to_defaults`

### 2.3 Миграция с перезаписью конфига
- `plugins/settings.rs`: `data_dir()` (уже есть, но не связана с AI)
- `ai/settings.rs`: `load_and_migrate(path)` — читает, при необходимости перезаписывает атомарно
- `ai/state.rs`: `AiState::init` → `load_and_migrate`
- Тесты: `data_dir_follows_config_override`, `load_and_migrate_rewrites_config`, `load_and_migration_is_idempotent`

### 2.4 Проекции `View`/`Input` без секреты
- `ai/types.rs`: `AiSettingsView`, `ProviderSettingsView` (с `has_key`, без `api_key`), `AiSettingsInput`, `AiProviderInput`
- Тест: `view_hides_secret_for_every_provider`

### 2.5 Резолв ключа: `settings.apiKey` → `env`
- `ai/types.rs`: `resolve_api_key<F>(provider, env: F) -> Option<String>` — чистая функция
- Тест: `resolve_api_key_prefers_settings_then_env`

### 2.6 Валидация и резолв активной пары
- `ai/state.rs`: `resolve_active(settings) -> Result<ResolvedProvider, CommandError>`
- `ai/gate.rs`: `check_active` — после `check_enabled`, до `check_key`
- Тесты: `resolve_active_*` (6 штук), `gate_reports_bad_settings_before_key`

### 2.7 Сброс согласия при смене хоста
- `ai/state.rs`: `active_host(settings) -> Option<String>`, правило в `update_settings`
- Тесты: `consent_reset_when_active_host_changes`, `consent_kept_when_active_host_unchanged`, `active_host_is_normalized`

### 2.8 `headers`/`body`/`timeout` через шов `AiTransport`
- `ai/transport.rs`: `ChatRequest` + `headers`, `extra_body`, `timeout`, `tools`
- `ai/backend/reqwest.rs`: `build_chat_body`, `apply_custom_headers`
- Тесты: `chat_body_merges_extra_body_without_overriding_protocol_fields`, `custom_headers_drop_reserved_names`

### 2.9 Команды под провайдеров
- `ai/commands.rs`: `ai_set_api_key(provider_id, key)`, `ai_test_connection` на активной паре
- `ai/state.rs`: `set_api_key(provider_id, key)`
- Тест: `set_api_key_is_scoped_to_provider`

### 2.10 Согласие и диалог — имя активного провайдера
- `aiPanel.ts`: `askConsent` использует `name` + хост из настроек (не хардкод `routerai.ru`)
- Проверка: `rg -n "routerai" crates/app/src` → пусто

### 2.11 TS-DTO и менеджер провайдеров в UI
- `tauri.ts`: `AiSettingsView`/`AiSettingsInput`/`aiSetApiKey(providerId, key)`
- `index.html` + `aiPanel.ts` + `styles.css`: менеджер в `#ai-settings-pop`
- Новые id: `#ai-providers`, `#ai-provider-add`, `#ai-provider-edit`, `#ai-provider-remove`, `#ai-provider-name`, `#ai-provider-base-url`, `#ai-provider-key`, `#ai-provider-env`, `#ai-provider-headers`, `#ai-provider-body`, `#ai-provider-timeout`, `#ai-model-select`

### 2.12 E2E под новую форму настроек
- `ai.steps.js`: переписать IPC-шаги, добавить сценарий менеджера провайдеров
- `ai.feature`: «Менеджер провайдеров: список, добавление, выбор активного»

---

## 6. Критерии готовности

| AC | Критерий | Задача |
|----|----------|--------|
| AC-P3-5 | Несколько провайдеров, активная пара по полям | 2.1, 2.2, 2.4, 2.6, 2.11 |
| AC-P3-6 | Поля провайдера + `modelID`/`disabled` | 2.1, 2.8, 2.9 |
| AC-P3-7 | Ключ на провайдера, приоритет `apiKey` > `env` | 2.5, 2.9 |
| AC-P3-8 | Согласие называет провайдера, сброс при смене хоста | 2.7, 2.10, 2.12 |
| AC-P3-9 | Валидация `baseURL` | 2.6 |
| AC-P3-10 | Миграция + перезапись конфига | 2.2, 2.3 |
| AC-P3-11 | UI менеджера провайдеров | 2.11, 2.12 |

---

## 7. Проверки перед сдачей

- `cargo test -p darmark` — зелёный
- `cd crates/app; npm run build` — зелёный (tsc strict)
- `cargo clippy -p darmark -- -D warnings` — чисто
- `cd crates/app; npm run test:e2e` — зелёный
- `cargo tree -p darmark | Select-String mlua` — пусто (ADR-0021)
