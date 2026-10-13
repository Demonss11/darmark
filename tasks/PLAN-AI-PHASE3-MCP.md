# План работ: AI-ассистент Фаза 3 + MCP Tool-loop

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Завершить AI-ассистент (Фаза 3) и tool-loop MCP: универсальная конфигурация провайдеров, Rust-санитайзер ответа, остальные скиллы, персистентность истории, пользовательские скиллы, tool-loop.

**Architecture:** Универсальная конфигурация провайдеров (ADR-0035) — map провайдеров + активная пара `provider/model`; Rust-санитайзер ответа через `md-core` (единственная точка `innerHTML`); персистентность истории — файл на сессию в `%APPDATA%/darmark/`; пользовательские скиллы — JSON `ai-skills.json` в `%APPDATA%/darmark/`; tool-loop — цикл вызова MCP-инструментов ассистентом (провайдер поддерживает tool-calling).

**Tech Stack:** Rust (Tauri 2), vanilla TypeScript (strict), `md-core` (pulldown-cmark 0.13 + санитайзер), `plugin-cli`, `mcp-core` (JSON-RPC 2.0, протокол 2024-11-05).

**Spec:** `tasks/TZ-AI-PHASE3.md`, `tasks/TZ-MCP.md`, `docs/adr/0035-ai-provider-config.md`, `docs/adr/0036-mcp-server-inmemory.md`

---

## Global Constraints

- **CSP:** `default-src 'self'` — без внешних скриптов и `eval`; сеть — только Rust-сторона.
- **Строгий TS:** `strict`, `noUnusedLocals`, `noUnusedParameters` — неиспользуемые сущности ломают сборку.
- **Рендер Markdown ответа AI** — через `md-core` (`to_html` + `sanitize_html`); сырой вывод LLM в `innerHTML` не попадает.
- **Приватность:** ключи не в TS/логах/событиях; наружу — только `has_key`; в `{:?}` — `<redacted>`.
- **Размер release-exe:** ≤6 МБ (D6); AI-клиент ≤1.0 МиБ (ADR-0033).
- **Аддитивность контрактов:** `HOST_API_VERSION` не поднимается; существующие id `#ai-*`, `#logs-*` сохраняются.
- **mlua вне GUI** (ADR-0021): `plugin_test` идёт только через `plugin-cli`.
- **Комментарии на русском**, объясняют **зачем**; отступ 2 пробела (TS), `rustfmt` (Rust).
- **Проверки перед релизом:** `cargo test -p darmark`, `cd crates/app; npm run build`, `cargo test -p md-core -p plugin-core -p plugin-catalog -p plugin-host -p plugin-cli`.

---

## File Structure

### Универсальная конфигурация провайдеров

| Файл | Ответственность |
|------|-----------------|
| `crates/app/src-tauri/src/ai/types.rs` | Расширение `AiSettings` → `providers: HashMap<String, ProviderConfig>`, `active_provider`, `active_model` |
| `crates/app/src-tauri/src/ai/settings.rs` | Миграция legacy-секции `ai` в новый формат (idempotent) |
| `crates/app/src-tauri/src/ai/state.rs` | Резолв активной пары, сборка запроса из `ProviderConfig` |
| `crates/app/src-tauri/src/ai/commands.rs` | `ai_set_api_key(provider_id, key)` — ключ на провайдера |
| `crates/app/src/aiPanel.ts` | UI менеджера провайдеров (список, добавить/править/удалить, активный) |

### Rust-санитайзер ответа

| Файл | Ответственность |
|------|-----------------|
| `crates/app/src-tauri/src/ai/commands.rs` | Новая команда `ai_render_markdown(text) -> String` (вызов `md-core`) |
| `crates/app/src/aiPanel.ts` | Замена `textContent` на `innerHTML` с санитизированным HTML от `ai_render_markdown` |

### Остальные скиллы

| Файл | Ответственность |
|------|-----------------|
| `crates/app/src-tauri/src/ai/skills.rs` | Добавление 5 скиллов: переструктурировать, конспект, объяснить синтаксис, отладить плагин, добавить view |
| `crates/app/src/aiPanel.ts` | Карточки новых скиллов (аддитивно) |

### Персистентность истории

| Файл | Ответственность |
|------|-----------------|
| `crates/app/src-tauri/src/ai/history.rs` | Новый модуль: загрузка/сохранение истории в `%APPDATA%/darmark/ai-history.json` |
| `crates/app/src-tauri/src/ai/commands.rs` | Команды `ai_history_load` / `ai_history_save` |
| `crates/app/src/aiPanel.ts` | Восстановление истории при старте, сохранение при отправке |

### Пользовательские скиллы

| Файл | Ответственность |
|------|-----------------|
| `crates/app/src-tauri/src/ai/skills.rs` | Загрузка пользовательских скиллов из `%APPDATA%/darmark/ai-skills.json` |
| `crates/app/src-tauri/src/ai/commands.rs` | CRUD-команды: `ai_skills_list`, `ai_skill_create`, `ai_skill_update`, `ai_skill_delete` |
| `crates/app/src/aiPanel.ts` | UI редактора скиллов (создание/правка/удаление) |

### Tool-loop (MCP)

| Файл | Ответственность |
|------|-----------------|
| `crates/app/src-tauri/src/ai/tool_loop.rs` | Новый модуль: цикл вызова MCP-инструментов ассистентом |
| `crates/app/src-tauri/src/ai/commands.rs` | Команда `ai_chat_with_tools` — чат с tool-calling |
| `crates/app/src/aiPanel.ts` | Отображение вызовов инструментов в чате |

---

## Epic 1: Универсальная конфигурация провайдеров (ADR-0035)

### Task 1.1: Расширение типов AiSettings

**Files:**
- Modify: `crates/app/src-tauri/src/ai/types.rs`

**Interfaces:**
- Consumes: —
- Produces: `ProviderConfig { name, env: Vec<String>, package, settings: ProviderSettings, headers: HashMap<String,String>, body: serde_json::Value, models: HashMap<String, ModelEntry> }`, `ProviderSettings { base_url, api_key: Option<String>, timeout: Option<u64> }`, `ModelEntry { name, model_id: Option<String>, disabled: Option<bool> }`, `AiSettings { enabled, active_provider, active_model, consent_document_send, max_tokens, temperature, providers: HashMap<String, ProviderConfig> }`

- [ ] **Step 1: Добавить структуры ProviderConfig, ProviderSettings, ModelEntry в types.rs**

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProviderSettings {
    pub base_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelEntry {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    #[serde(default)]
    pub env: Vec<String>,
    pub package: String,
    pub settings: ProviderSettings,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub body: serde_json::Value,
    #[serde(default)]
    pub models: HashMap<String, ModelEntry>,
}
```

- [ ] **Step 2: Расширить AiSettings новыми полями**

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AiSettings {
    pub enabled: bool,
    pub active_provider: String,
    pub active_model: String,
    pub consent_document_send: bool,
    pub max_tokens: u32,
    pub temperature: f32,
    pub providers: HashMap<String, ProviderConfig>,
}
```

- [ ] **Step 3: Обновить AiSettingsView (без секретов)**

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct AiSettingsView {
    pub enabled: bool,
    pub active_provider: String,
    pub active_model: String,
    pub consent_document_send: bool,
    pub max_tokens: u32,
    pub temperature: f32,
    pub providers: HashMap<String, ProviderConfigView>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProviderConfigView {
    pub name: String,
    pub env: Vec<String>,
    pub package: String,
    pub settings: ProviderSettingsView,
    pub headers: HashMap<String, String>,
    pub body: serde_json::Value,
    pub models: HashMap<String, ModelEntry>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProviderSettingsView {
    pub base_url: String,
    pub has_key: bool,
    pub timeout: Option<u64>,
}
```

- [ ] **Step 4: Проверить компиляцию**

Run: `cargo check -p darmark`
Expected: OK

- [ ] **Step 5: Commit**

```bash
git add crates/app/src-tauri/src/ai/types.rs
git commit -m "feat(ai): добавить типы ProviderConfig для универсальной конфигурации провайдеров"
```

---

### Task 1.2: Миграция legacy-секции ai

**Files:**
- Modify: `crates/app/src-tauri/src/ai/settings.rs`

**Interfaces:**
- Consumes: `AiSettings` (новый формат)
- Produces: `fn migrate_legacy_ai(legacy: &LegacyAiSettings) -> AiSettings`

- [ ] **Step 1: Добавить структуру LegacyAiSettings для чтения старого формата**

```rust
#[derive(Debug, Clone, serde::Deserialize)]
pub struct LegacyAiSettings {
    pub enabled: Option<bool>,
    pub provider: Option<String>,
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub api_key: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub consent_document_send: Option<bool>,
}
```

- [ ] **Step 2: Реализовать функцию миграции**

```rust
pub fn migrate_legacy_ai(legacy: &LegacyAiSettings) -> AiSettings {
    let provider_id = legacy.provider.clone().unwrap_or_else(|| "routerai".to_string());
    let model_id = legacy.model.clone().unwrap_or_else(|| "deepseek/deepseek-v4.1-flash".to_string());

    let mut models = HashMap::new();
    models.insert(model_id.clone(), crate::ai::types::ModelEntry {
        name: model_id.clone(),
        model_id: None,
        disabled: None,
    });

    let mut providers = HashMap::new();
    providers.insert(provider_id.clone(), crate::ai::types::ProviderConfig {
        name: legacy.provider.clone().unwrap_or_else(|| "RouterAI".to_string()),
        env: vec![],
        package: "openai-compatible".to_string(),
        settings: crate::ai::types::ProviderSettings {
            base_url: legacy.base_url.clone().unwrap_or_else(|| "https://routerai.ru/api/v1".to_string()),
            api_key: legacy.api_key.clone(),
            timeout: None,
        },
        headers: HashMap::new(),
        body: serde_json::Value::Object(serde_json::Map::new()),
        models,
    });

    AiSettings {
        enabled: legacy.enabled.unwrap_or(false),
        active_provider: provider_id,
        active_model: model_id,
        consent_document_send: legacy.consent_document_send.unwrap_or(false),
        max_tokens: legacy.max_tokens.unwrap_or(32768),
        temperature: legacy.temperature.unwrap_or(0.7),
        providers,
    }
}
```

- [ ] **Step 3: Интегрировать миграцию в load_from (SettingsStore)**

В `settings.rs` при загрузке: если `providers` отсутствует, а есть legacy-поля — вызвать `migrate_legacy_ai`.

- [ ] **Step 4: Написать тест миграции**

```rust
#[test]
fn test_migrate_legacy_ai() {
    let json = r#"{"enabled":true,"provider":"openai-compatible","base_url":"https://routerai.ru/api/v1","model":"deepseek/deepseek-v4.1-flash","api_key":"secret"}"#;
    let legacy: LegacyAiSettings = serde_json::from_str(json).unwrap();
    let settings = migrate_legacy_ai(&legacy);
    assert_eq!(settings.active_provider, "openai-compatible");
    assert_eq!(settings.active_model, "deepseek/deepseek-v4.1-flash");
    assert!(settings.providers.contains_key("openai-compatible"));
}
```

- [ ] **Step 5: Запустить тест**

Run: `cargo test -p darmark migrate_legacy_ai`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/app/src-tauri/src/ai/settings.rs
git commit -m "feat(ai): миграция legacy-секции ai в формат providers"
```

---

### Task 1.3: Резолв активной пары и сборка запроса

**Files:**
- Modify: `crates/app/src-tauri/src/ai/state.rs`

**Interfaces:**
- Consumes: `AiSettings`, `ProviderConfig`
- Produces: `fn resolve_active_provider(settings: &AiSettings) -> Result<ProviderConfig, CommandError>`, `fn build_chat_request(provider: &ProviderConfig, model: &str, messages: &[AiMessage]) -> ChatRequest`

- [ ] **Step 1: Добавить функцию резолва активного провайдера**

```rust
pub fn resolve_active_provider(settings: &AiSettings) -> Result<ProviderConfig, CommandError> {
    let provider = settings.providers.get(&settings.active_provider)
        .ok_or_else(|| CommandError::new("ai_bad_settings", "Активный провайдер не найден"))?;
    Ok(provider.clone())
}
```

- [ ] **Step 2: Добавить функцию сборки запроса**

```rust
pub fn build_chat_request(provider: &ProviderConfig, model: &str, messages: &[AiMessage]) -> ChatRequest {
    let mut body = serde_json::Map::new();
    body.insert("model".to_string(), serde_json::Value::String(model.to_string()));
    body.insert("messages".to_string(), serde_json::to_value(messages).unwrap());
    // Сливаем body из конфига поверх дефолтных полей
    if let serde_json::Value::Object(extra) = &provider.body {
        for (k, v) in extra {
            body.insert(k.clone(), v.clone());
        }
    }
    ChatRequest { body: serde_json::Value::Object(body) }
}
```

- [ ] **Step 3: Проверить компиляцию**

Run: `cargo check -p darmark`
Expected: OK

- [ ] **Step 4: Commit**

```bash
git add crates/app/src-tauri/src/ai/state.rs
git commit -m "feat(ai): резолв активного провайдера и сборка запроса"
```

---

### Task 1.4: Команда ai_set_api_key с id провайдера

**Files:**
- Modify: `crates/app/src-tauri/src/ai/commands.rs`

**Interfaces:**
- Consumes: `AiSettings`
- Produces: `#[tauri::command] async fn ai_set_api_key(provider_id: String, key: String) -> Result<(), CommandError>`

- [ ] **Step 1: Изменить сигнатуру ai_set_api_key**

```rust
#[tauri::command]
pub async fn ai_set_api_key(provider_id: String, key: String) -> Result<(), CommandError> {
    let mut settings = crate::ai::settings::load_from(&state).await?;
    let provider = settings.providers.get_mut(&provider_id)
        .ok_or_else(|| CommandError::new("ai_bad_settings", "Провайдер не найден"))?;
    let trimmed = key.trim();
    if trimmed.is_empty() {
        provider.settings.api_key = None;
    } else {
        provider.settings.api_key = Some(trimmed.to_string());
    }
    crate::ai::settings::persist_to(&state, &settings).await?;
    Ok(())
}
```

- [ ] **Step 2: Проверить компиляцию**

Run: `cargo check -p darmark`
Expected: OK

- [ ] **Step 3: Commit**

```bash
git add crates/app/src-tauri/src/ai/commands.rs
git commit -m "feat(ai): ai_set_api_key с указанием id провайдера"
```

---

### Task 1.5: UI менеджера провайдеров

**Files:**
- Modify: `crates/app/src/aiPanel.ts`

**Interfaces:**
- Consumes: `ai_get_settings`, `ai_set_settings`, `ai_set_api_key`
- Produces: Элементы `#ai-providers`, `#ai-provider-add`, `#ai-provider-edit`, `#ai-provider-remove`, `#ai-provider-active`, `#ai-provider-name`, `#ai-provider-base-url`, `#ai-provider-key`, `#ai-provider-env`, `#ai-provider-headers`, `#ai-provider-body`, `#ai-provider-timeout`

- [ ] **Step 1: Добавить HTML менеджера провайдеров в поповер `#ai-settings-pop`**

```html
<div id="ai-providers">
  <div id="ai-provider-list"></div>
  <button id="ai-provider-add">Добавить</button>
  <div id="ai-provider-editor" hidden>
    <input id="ai-provider-name" placeholder="Имя">
    <input id="ai-provider-base-url" placeholder="https://...">
    <input id="ai-provider-key" type="password" placeholder="API ключ">
    <input id="ai-provider-env" placeholder="ENV1,ENV2">
    <textarea id="ai-provider-headers" placeholder='{"X-Custom":"value"}'></textarea>
    <textarea id="ai-provider-body" placeholder='{"metadata":"value"}'></textarea>
    <input id="ai-provider-timeout" type="number" placeholder="30000">
    <button id="ai-provider-save">Сохранить</button>
    <button id="ai-provider-remove">Удалить</button>
  </div>
</div>
```

- [ ] **Step 2: Добавить логику загрузки/сохранения провайдеров**

```ts
async function loadProviders() {
  const settings = await invoke('ai_get_settings');
  const list = document.getElementById('ai-provider-list')!;
  list.innerHTML = '';
  for (const [id, p] of Object.entries(settings.providers)) {
    const div = document.createElement('div');
    div.className = 'ai-provider-item';
    div.innerHTML = `<span>${p.name}</span> <button data-id="${id}">Выбрать</button>`;
    list.appendChild(div);
  }
}
```

- [ ] **Step 3: Проверить сборку**

Run: `cd crates/app; npm run build`
Expected: OK (tsc strict)

- [ ] **Step 4: Commit**

```bash
git add crates/app/src/aiPanel.ts
git commit -m "feat(ai): UI менеджера провайдеров"
```

---

## Epic 2: Rust-санитайзер ответа AI

### Task 2.1: Команда ai_render_markdown

**Files:**
- Modify: `crates/app/src-tauri/src/ai/commands.rs`

**Interfaces:**
- Consumes: `md_core::to_html`, `md_core::sanitize_html`
- Produces: `#[tauri::command] async fn ai_render_markdown(text: String) -> Result<String, CommandError>`

- [ ] **Step 1: Добавить команду рендера Markdown**

```rust
#[tauri::command]
pub async fn ai_render_markdown(text: String) -> Result<String, CommandError> {
    let html = md_core::to_html(&text);
    let safe = md_core::sanitize_html(&html);
    Ok(safe)
}
```

- [ ] **Step 2: Проверить компиляцию**

Run: `cargo check -p darmark`
Expected: OK

- [ ] **Step 3: Commit**

```bash
git add crates/app/src-tauri/src/ai/commands.rs
git commit -m "feat(ai): команда ai_render_markdown через md-core"
```

---

### Task 2.2: Замена textContent на innerHTML в aiPanel.ts

**Files:**
- Modify: `crates/app/src/aiPanel.ts`

**Interfaces:**
- Consumes: `ai_render_markdown`
- Produces: Ответ AI рендерится как Markdown через `innerHTML` с санитизированным HTML

- [ ] **Step 1: Добавить функцию рендера ответа**

```ts
async function renderAiResponse(text: string): Promise<string> {
  return await invoke('ai_render_markdown', { text });
}
```

- [ ] **Step 2: Заменить textContent на innerHTML в функции отображения ответа**

```ts
// Было:
// responseEl.textContent = text;
// Стало:
responseEl.innerHTML = await renderAiResponse(text);
```

- [ ] **Step 3: Проверить сборку**

Run: `cd crates/app; npm run build`
Expected: OK

- [ ] **Step 4: Commit**

```bash
git add crates/app/src/aiPanel.ts
git commit -m "feat(ai): рендер ответа AI как Markdown через санитайзер"
```

---

## Epic 3: Остальные скиллы

### Task 3.1: Добавление 5 скиллов в skills.rs

**Files:**
- Modify: `crates/app/src-tauri/src/ai/skills.rs`

**Interfaces:**
- Consumes: —
- Produces: Массив `AiSkill` с элементами `id`, `name`, `description`, `prompt`

- [ ] **Step 1: Добавить 5 скиллов**

```rust
pub fn skills() -> Vec<AiSkill> {
    vec![
        AiSkill { id: "explain".into(), name: "Объяснить".into(), description: "Объяснить выделенный текст".into(), prompt: "Объясни выделенный текст...".into() },
        AiSkill { id: "create-plugin".into(), name: "Создать плагин".into(), description: "Создать Lua-плагин".into(), prompt: "Создай плагин...".into() },
        AiSkill { id: "restructure".into(), name: "Переструктурировать".into(), description: "Переструктурировать документ".into(), prompt: "Переструктурируй документ...".into() },
        AiSkill { id: "summary".into(), name: "Конспект".into(), description: "Сжатое изложение документа".into(), prompt: "Составь конспект...".into() },
        AiSkill { id: "explain-syntax".into(), name: "Объяснить синтаксис".into(), description: "Разбор разметки Markdown".into(), prompt: "Объясни синтаксис...".into() },
        AiSkill { id: "debug-plugin".into(), name: "Отладить плагин".into(), description: "Диагностика ошибок песочницы".into(), prompt: "Отладь плагин...".into() },
        AiSkill { id: "add-view".into(), name: "Добавить view".into(), description: "Генерация команды представления".into(), prompt: "Добавь view...".into() },
    ]
}
```

- [ ] **Step 2: Проверить компиляцию**

Run: `cargo check -p darmark`
Expected: OK

- [ ] **Step 3: Commit**

```bash
git add crates/app/src-tauri/src/ai/skills.rs
git commit -m "feat(ai): добавить 5 скиллов (переструктурировать, конспект, синтаксис, отладка, view)"
```

---

### Task 3.2: Карточки новых скиллов в UI

**Files:**
- Modify: `crates/app/src/aiPanel.ts`

**Interfaces:**
- Consumes: `ai_list_skills`
- Produces: Карточки скиллов в режиме AI

- [ ] **Step 1: Удалить заглушку "Ещё скиллы — скоро"**

- [ ] **Step 2: Проверить, что карточки скиллов рендерятся динамически из `ai_list_skills`**

- [ ] **Step 3: Проверить сборку**

Run: `cd crates/app; npm run build`
Expected: OK

- [ ] **Step 4: Commit**

```bash
git add crates/app/src/aiPanel.ts
git commit -m "feat(ai): отображение новых скиллов в UI"
```

---

## Epic 4: Персистентность истории чата

### Task 4.1: Модуль истории

**Files:**
- Create: `crates/app/src-tauri/src/ai/history.rs`

**Interfaces:**
- Consumes: `AiMessage`
- Produces: `fn load_history() -> Result<Vec<AiMessage>, CommandError>`, `fn save_history(messages: &[AiMessage]) -> Result<(), CommandError>`

- [ ] **Step 1: Создать модуль history.rs**

```rust
use crate::ai::types::AiMessage;
use crate::error::CommandError;
use std::path::PathBuf;

fn history_path() -> PathBuf {
    let mut path = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("darmark");
    path.push("ai-history.json");
    path
}

pub fn load_history() -> Result<Vec<AiMessage>, CommandError> {
    let path = history_path();
    if !path.exists() {
        return Ok(vec![]);
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| CommandError::new("io_error", &format!("Ошибка чтения истории: {}", e)))?;
    serde_json::from_str(&content)
        .map_err(|e| CommandError::new("parse_error", &format!("Ошибка парсинга истории: {}", e)))
}

pub fn save_history(messages: &[AiMessage]) -> Result<(), CommandError> {
    let path = history_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| CommandError::new("io_error", &format!("Ошибка создания каталога: {}", e)))?;
    }
    let content = serde_json::to_string_pretty(messages)
        .map_err(|e| CommandError::new("serialize_error", &format!("Ошибка сериализации: {}", e)))?;
    std::fs::write(&path, content)
        .map_err(|e| CommandError::new("io_error", &format!("Ошибка записи истории: {}", e)))?;
    Ok(())
}
```

- [ ] **Step 2: Подключить модуль в mod.rs**

```rust
pub mod history;
```

- [ ] **Step 3: Проверить компиляцию**

Run: `cargo check -p darmark`
Expected: OK

- [ ] **Step 4: Commit**

```bash
git add crates/app/src-tauri/src/ai/history.rs crates/app/src-tauri/src/ai/mod.rs
git commit -m "feat(ai): модуль персистентности истории чата"
```

---

### Task 4.2: Команды истории

**Files:**
- Modify: `crates/app/src-tauri/src/ai/commands.rs`

**Interfaces:**
- Consumes: `history::load_history`, `history::save_history`
- Produces: `#[tauri::command] async fn ai_history_load() -> Result<Vec<AiMessage>, CommandError>`, `#[tauri::command] async fn ai_history_save(messages: Vec<AiMessage>) -> Result<(), CommandError>`

- [ ] **Step 1: Добавить команды**

```rust
#[tauri::command]
pub async fn ai_history_load() -> Result<Vec<AiMessage>, CommandError> {
    crate::ai::history::load_history()
}

#[tauri::command]
pub async fn ai_history_save(messages: Vec<AiMessage>) -> Result<(), CommandError> {
    crate::ai::history::save_history(&messages)
}
```

- [ ] **Step 2: Проверить компиляцию**

Run: `cargo check -p darmark`
Expected: OK

- [ ] **Step 3: Commit**

```bash
git add crates/app/src-tauri/src/ai/commands.rs
git commit -m "feat(ai): команды ai_history_load/ai_history_save"
```

---

### Task 4.3: Интеграция истории в aiPanel.ts

**Files:**
- Modify: `crates/app/src/aiPanel.ts`

**Interfaces:**
- Consumes: `ai_history_load`, `ai_history_save`
- Produces: Восстановление истории при старте, сохранение при отправке

- [ ] **Step 1: Загрузить историю при инициализации**

```ts
async function initHistory() {
  const history = await invoke('ai_history_load');
  for (const msg of history) {
    appendMessage(msg.role, msg.content);
  }
}
```

- [ ] **Step 2: Сохранять историю при отправке**

```ts
async function saveHistory() {
  const messages = history.map(m => ({ role: m.role, content: m.content }));
  await invoke('ai_history_save', { messages });
}
```

- [ ] **Step 3: Проверить сборку**

Run: `cd crates/app; npm run build`
Expected: OK

- [ ] **Step 4: Commit**

```bash
git add crates/app/src/aiPanel.ts
git commit -m "feat(ai): персистентность истории чата в UI"
```

---

## Epic 5: Пользовательские скиллы

### Task 5.1: Загрузка пользовательских скиллов

**Files:**
- Modify: `crates/app/src-tauri/src/ai/skills.rs`

**Interfaces:**
- Consumes: —
- Produces: `fn load_user_skills() -> Result<Vec<AiSkill>, CommandError>`, `fn save_user_skills(skills: &[AiSkill]) -> Result<(), CommandError>`

- [ ] **Step 1: Добавить функции загрузки/сохранения**

```rust
fn user_skills_path() -> std::path::PathBuf {
    let mut path = dirs::data_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    path.push("darmark");
    path.push("ai-skills.json");
    path
}

pub fn load_user_skills() -> Result<Vec<AiSkill>, CommandError> {
    let path = user_skills_path();
    if !path.exists() {
        return Ok(vec![]);
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| CommandError::new("io_error", &format!("Ошибка чтения скиллов: {}", e)))?;
    serde_json::from_str(&content)
        .map_err(|e| CommandError::new("parse_error", &format!("Ошибка парсинга скиллов: {}", e)))
}

pub fn save_user_skills(skills: &[AiSkill]) -> Result<(), CommandError> {
    let path = user_skills_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| CommandError::new("io_error", &format!("Ошибка создания каталога: {}", e)))?;
    }
    let content = serde_json::to_string_pretty(skills)
        .map_err(|e| CommandError::new("serialize_error", &format!("Ошибка сериализации: {}", e)))?;
    std::fs::write(&path, content)
        .map_err(|e| CommandError::new("io_error", &format!("Ошибка записи скиллов: {}", e)))?;
    Ok(())
}
```

- [ ] **Step 2: Объединить встроенные и пользовательские скиллы в `skills()`**

```rust
pub fn skills() -> Vec<AiSkill> {
    let mut all = builtin_skills();
    if let Ok(user) = load_user_skills() {
        all.extend(user);
    }
    all
}
```

- [ ] **Step 3: Проверить компиляцию**

Run: `cargo check -p darmark`
Expected: OK

- [ ] **Step 4: Commit**

```bash
git add crates/app/src-tauri/src/ai/skills.rs
git commit -m "feat(ai): загрузка пользовательских скиллов из ai-skills.json"
```

---

### Task 5.2: CRUD-команды для скиллов

**Files:**
- Modify: `crates/app/src-tauri/src/ai/commands.rs`

**Interfaces:**
- Consumes: `skills::load_user_skills`, `skills::save_user_skills`
- Produces: `ai_skills_list`, `ai_skill_create`, `ai_skill_update`, `ai_skill_delete`

- [ ] **Step 1: Добавить команды**

```rust
#[tauri::command]
pub async fn ai_skills_list() -> Result<Vec<AiSkill>, CommandError> {
    crate::ai::skills::load_user_skills()
}

#[tauri::command]
pub async fn ai_skill_create(skill: AiSkill) -> Result<(), CommandError> {
    let mut skills = crate::ai::skills::load_user_skills()?;
    skills.push(skill);
    crate::ai::skills::save_user_skills(&skills)
}

#[tauri::command]
pub async fn ai_skill_update(id: String, skill: AiSkill) -> Result<(), CommandError> {
    let mut skills = crate::ai::skills::load_user_skills()?;
    if let Some(pos) = skills.iter().position(|s| s.id == id) {
        skills[pos] = skill;
    }
    crate::ai::skills::save_user_skills(&skills)
}

#[tauri::command]
pub async fn ai_skill_delete(id: String) -> Result<(), CommandError> {
    let mut skills = crate::ai::skills::load_user_skills()?;
    skills.retain(|s| s.id != id);
    crate::ai::skills::save_user_skills(&skills)
}
```

- [ ] **Step 2: Проверить компиляцию**

Run: `cargo check -p darmark`
Expected: OK

- [ ] **Step 3: Commit**

```bash
git add crates/app/src-tauri/src/ai/commands.rs
git commit -m "feat(ai): CRUD-команды для пользовательских скиллов"
```

---

### Task 5.3: UI редактора скиллов

**Files:**
- Modify: `crates/app/src/aiPanel.ts`

**Interfaces:**
- Consumes: `ai_skills_list`, `ai_skill_create`, `ai_skill_update`, `ai_skill_delete`
- Produces: Кнопка "Свои скиллы", форма создания/редактирования

- [ ] **Step 1: Добавить кнопку "Свои скиллы" в поповер настроек**

- [ ] **Step 2: Добавить форму создания/редактирования скилла**

- [ ] **Step 3: Проверить сборку**

Run: `cd crates/app; npm run build`
Expected: OK

- [ ] **Step 4: Commit**

```bash
git add crates/app/src/aiPanel.ts
git commit -m "feat(ai): UI редактора пользовательских скиллов"
```

---

## Epic 6: Tool-loop (MCP)

### Task 6.1: Модуль tool_loop.rs

**Files:**
- Create: `crates/app/src-tauri/src/ai/tool_loop.rs`

**Interfaces:**
- Consumes: `mcp_core::server::McpServer`, `AiTransport`
- Produces: `pub async fn chat_with_tools(transport: &dyn AiTransport, messages: Vec<AiMessage>, max_iterations: usize) -> Result<Vec<AiMessage>, CommandError>`

- [ ] **Step 1: Создать модуль tool_loop.rs**

```rust
use crate::ai::types::AiMessage;
use crate::ai::transport::AiTransport;
use crate::error::CommandError;

pub async fn chat_with_tools(
    transport: &dyn AiTransport,
    messages: Vec<AiMessage>,
    max_iterations: usize,
) -> Result<Vec<AiMessage>, CommandError> {
    let mut all_messages = messages;
    for _ in 0..max_iterations {
        let response = transport.chat(&all_messages).await?;
        all_messages.push(AiMessage::assistant(response.content.clone()));
        if let Some(tool_calls) = response.tool_calls {
            for call in tool_calls {
                // Вызов MCP-инструмента через mcp_core
                let result = call_mcp_tool(&call).await?;
                all_messages.push(AiMessage::tool(call.name, result));
            }
        } else {
            break;
        }
    }
    Ok(all_messages)
}

async fn call_mcp_tool(call: &ToolCall) -> Result<String, CommandError> {
    // Интеграция с mcp_core::server::McpServer
    // ...
}
```

- [ ] **Step 2: Подключить модуль в mod.rs**

```rust
pub mod tool_loop;
```

- [ ] **Step 3: Проверить компиляцию**

Run: `cargo check -p darmark`
Expected: OK

- [ ] **Step 4: Commit**

```bash
git add crates/app/src-tauri/src/ai/tool_loop.rs crates/app/src-tauri/src/ai/mod.rs
git commit -m "feat(ai): модуль tool-loop для вызова MCP-инструментов"
```

---

### Task 6.2: Команда ai_chat_with_tools

**Files:**
- Modify: `crates/app/src-tauri/src/ai/commands.rs`

**Interfaces:**
- Consumes: `tool_loop::chat_with_tools`
- Produces: `#[tauri::command] async fn ai_chat_with_tools(messages: Vec<AiMessage>) -> Result<Vec<AiMessage>, CommandError>`

- [ ] **Step 1: Добавить команду**

```rust
#[tauri::command]
pub async fn ai_chat_with_tools(messages: Vec<AiMessage>) -> Result<Vec<AiMessage>, CommandError> {
    let transport = crate::ai::backend::create_transport().await?;
    crate::ai::tool_loop::chat_with_tools(&*transport, messages, 5).await
}
```

- [ ] **Step 2: Проверить компиляцию**

Run: `cargo check -p darmark`
Expected: OK

- [ ] **Step 3: Commit**

```bash
git add crates/app/src-tauri/src/ai/commands.rs
git commit -m "feat(ai): команда ai_chat_with_tools"
```

---

### Task 6.3: Отображение вызовов инструментов в UI

**Files:**
- Modify: `crates/app/src/aiPanel.ts`

**Interfaces:**
- Consumes: `ai_chat_with_tools`
- Produces: Карточки вызовов инструментов в чате

- [ ] **Step 1: Добавить отображение tool-calls в чате**

- [ ] **Step 2: Проверить сборку**

Run: `cd crates/app; npm run build`
Expected: OK

- [ ] **Step 3: Commit**

```bash
git add crates/app/src/aiPanel.ts
git commit -m "feat(ai): отображение вызовов инструментов в чате"
```

---

## Порядок выполнения

| # | Epic | Задачи | Зависимости |
|---|------|--------|-------------|
| 1 | Универсальная конфигурация провайдеров | 1.1–1.5 | Нет |
| 2 | Rust-санитайзер ответа | 2.1–2.2 | Нет |
| 3 | Остальные скиллы | 3.1–3.2 | Нет |
| 4 | Персистентность истории | 4.1–4.3 | Скиллы (контекст) |
| 5 | Пользовательские скиллы | 5.1–5.3 | Персистентность |
| 6 | Tool-loop (MCP) | 6.1–6.3 | Провайдер (tool-calling) |

---

## Критерии готовности (сводка)

| AC | Критерий | Epic |
|----|----------|------|
| AC-P3-5…12 | Универсальная конфигурация провайдеров | Epic 1 |
| AC-P3-4 | Rust-санитайзер ответа | Epic 2 |
| AC-P3-1 | Остальные скиллы | Epic 3 |
| AC-P3-3 | Персистентность истории | Epic 4 |
| AC-P3-2 | Пользовательские скиллы | Epic 5 |
| AC-MCP-5 | Tool-loop | Epic 6 |

---

## Ссылки

- `tasks/TZ-AI-PHASE3.md` — ТЗ AI-ассистента Фаза 3
- `tasks/TZ-MCP.md` — ТЗ MCP-сервера
- `docs/adr/0035-ai-provider-config.md` — ADR универсальной конфигурации провайдеров
- `docs/adr/0036-mcp-server-inmemory.md` — ADR in-memory MCP-сервера
- `docs/DESIGN_DOC.md` — нормативный layout/контракты
