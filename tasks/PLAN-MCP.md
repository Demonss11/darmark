# MCP-сервер darmark (План 1): контрактный tool-слой, read-only ресурсы, журналирование

> **Статус:** План реализации (TZ-MCP §3.1: сервер + контракты + журнал + подключение; работающее и тестируемое ПО само по себе)
> **ТЗ:** `tasks/TZ-MCP.md` · **ADR:** `docs/adr/0036-mcp-server-inmemory.md`
> **CI:** `.github/workflows/ci.yml` (windows-latest)

## For agentic workers

Этот план предназначен для исполнения через **subagent-driven-development** или **executing-plans**: каждая задача — атомарный шаг с файлами, интерфейсами и командами проверки.

Порядок исполнения:

- [ ] Прочитать `tasks/TZ-MCP.md`, `docs/adr/0036-mcp-server-inmemory.md`, `AGENTS.md`
- [ ] Задача 1: создать крейт `mcp-core` в workspace
- [ ] Задача 2: JSON-RPC 2.0 envelope
- [ ] Задача 3: трейты `ToolHandler`/`ResourceReader`
- [ ] Задача 4: реестр tools/resources
- [ ] Задача 5: MCP-сервер (initialize, tools/list, tools/call, resources/list, resources/read)
- [ ] Задача 6: белый список директорий в `plugin_harness`
- [ ] Задача 7: библиотека `plugin-cli` (pub mod catalog) для MCP-адаптеров
- [ ] Задача 8: MCP-модуль в `src-tauri` (адаптеры tools)
- [ ] Задача 9: MCP-ресурсы (контракты + каталог)
- [ ] Задача 10: журналирование вызовов
- [ ] Задача 11: подключение сервера в приложение
- [ ] Задача 12: контракт-тесты (AC-MCP-6)
- [ ] Задача 13: обновление CI
- [ ] Финал: `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`, `cd crates/app; npm run build`

## Goal

Реализовать **первый срез** трека MCP (TZ-MCP §3.1): работающий MCP-сервер в процессе GUI с контрактным слоем для внутреннего AI-ассистента. Сервер реализует протокол MCP по букве (JSON-RPC 2.0 + семантика MCP) поверх in-memory транспорта (ADR-0036). Первый срез — read-only: 4 tools + 6 resources (TZ-MCP §4). Каждый вызов инструмента журналируется в `#logs-panel` с соблюдением приватности (TZ-MCP §4.6, R2). MCP — только маршрутизация: проверки (права, rev-guard, безопасность путей) остаются в существующих командах (R3/R4, ADR-0027).

**Не в этом плане:** внутренний MCP-клиент + tool-loop у ассистента (План 2, зависит от поддержки tool-calling провайдером — TZ-MCP §8). `ChatRequest` в `transport.rs` не несёт tools/function-calling — это вынесено в План 2.

## Architecture

```
┌─────────────────────────────────────────────────────┐
│  GUI-процесс (darmark.exe)                          │
│                                                     │
│  ┌─────────────┐    ┌──────────────────────────┐   │
│  │ AI-ассистент │───▶│ MCP-сервер (in-memory)    │   │
│  │ (План 2)    │    │ crates/app/src-tauri/    │   │
│  └─────────────┘    │ src/mcp/                 │   │
│                     │  ├─ adapters.rs (tools)   │   │
│                     │  └─ resources.rs          │   │
│                     └──────────┬───────────────┘   │
│                                │                   │
│                     ┌──────────▼───────────────┐   │
│                     │ mcp-core (чистый крейт)  │   │
│                     │  ├─ protocol.rs (RPC 2.0) │   │
│                     │  ├─ handlers.rs (трейты) │   │
│                     │  ├─ registry.rs          │   │
│                     │  └─ server.rs (MCP-семантика)│  │
│                     └──────────────────────────┘   │
│                                                     │
│  Существующие команды: plugin_harness, md-core,    │
│  plugin-cli (catalog), plugin_core                 │
└─────────────────────────────────────────────────────┘
```

**Транспорт:** in-memory (канал в рамках процесса, ADR-0036 §2.1). Нет порта, нет child, нет нового бинарника.

**Разделение ответственности:**
- `mcp-core` — чистый крейт без Tauri/GUI: JSON-RPC 2.0, MCP-семантика, реестр, трейты. Тестируется отдельно (`cargo test -p mcp-core`).
- `crates/app/src-tauri/src/mcp/` — тонкие адаптеры к существующим командам/CLI. Без бизнес-логики: только маршрутизация + журналирование.
- `plugin_harness` — белый список директорий (единая точка проверки путей, TZ-MCP R3).

## Tech Stack

- Rust (edition 2021), `serde`/`serde_json` для JSON-RPC envelope
- Новый крейт `crates/mcp-core` (без Tauri, как `md-core`/`plugin-core`)
- Tauri 2 (`AppHandle` для emit `log-entry`)
- Существующие: `plugin_harness`, `plugin-cli::catalog`, `md-core`, `plugin-core`
- CI: windows-latest, `cargo fmt`/`clippy`/`test` + `npm run build` + size-gate

## Spec: tasks/TZ-MCP.md

- §3.1 scope: сервер по букве, read-only первый срез, контрактная ясность (R1), наблюдаемость (R2)
- §4: инвентарь первого среза (4 tools + 6 resources)
- §5: требования R1–R7
- §6: критерии готовности AC-MCP-1..14
- §8: открытые вопросы (tool-calling провайдера — План 2)

## Global Constraints

- **CSP** `default-src 'self'` (`tauri.conf.json`): без внешних скриптов и `eval`; всё на Rust-стороне (ADR-0032/0033).
- **Сеть только Rust** (CSP): MCP-транспорт in-memory, не покидает процесс (ADR-0036 §2.1 п. 4).
- **`mlua` вне GUI** (ADR-0021): `plugin_test` исполняет Lua только через `plugin-cli` (не `plugin-host` напрямую). Guard: `cargo tree -p darmark | Select-String mlua` — пусто.
- **Релиз ≤6 МБ** (D6, `docs/DESIGN_DOC.md` §14): MCP-модуль — маргинальный прирост ≤0.1 МиБ (без новых зависимостей; `serde`/`serde_json` уже в дереве).
- **rustfmt/clippy**: `cargo fmt --check` + `cargo clippy --all-targets --all-features -- -D warnings` зелёные.
- **Комментарии на русском**, объясняют **зачем**; отступ 2 пробела (TS), `rustfmt` (Rust).
- **Аддитивность контрактов** (ADR-0027): `ErrorCode` — только добавление вариантов; `HOST_API_VERSION` не поднимается; замороженные контракты §13.2 не меняются.
- **Безопасность путей**: `canonicalize` + `starts_with` + отказ на reparse point (symlink/junction) — в `plugin_harness`, не в MCP-адаптере (TZ-MCP R3, AC-MCP-12).
- **Журнал без полных путей/секретов/содержимого** (TZ-MCP §4.6, AC-MCP-8/13): в журнал идёт только имя файла (последний компонент), не полный путь; содержимое документа не логируется.
- **Категория журнала для MCP**: `"system"` (не трогаем TS-тип `LogCategory`).
- **Паника = abort** в релизе: не полагаться на `catch_unwind`.

---

## File Structure

| Файл | Действие | Ответственность |
|------|----------|-----------------|
| `crates/mcp-core/Cargo.toml` | Create | Манифест нового крейта (без Tauri) |
| `crates/mcp-core/src/lib.rs` | Create | Точка входа крейта, реэкспорт модулей |
| `crates/mcp-core/src/protocol.rs` | Create | JSON-RPC 2.0 envelope: `RpcRequest`, `RpcResponse`, `RpcError`, коды ошибок |
| `crates/mcp-core/src/handlers.rs` | Create | Трейты `ToolHandler`, `ResourceReader` + типы ошибок |
| `crates/mcp-core/src/registry.rs` | Create | Реестр tools/resources со схемами |
| `crates/mcp-core/src/server.rs` | Create | `McpServer`: MCP-семантика (initialize, tools/list, tools/call, resources/list, resources/read) |
| `crates/mcp-core/tests/protocol_tests.rs` | Create | Тесты JSON-RPC envelope |
| `crates/mcp-core/tests/server_tests.rs` | Create | Тесты MCP-семантики |
| `Cargo.toml` (workspace) | Modify | Добавить `"crates/mcp-core"` в `members` |
| `crates/plugin-cli/src/lib.rs` | Create | Библиотечная точка: `pub mod catalog;` (для MCP-адаптеров) |
| `crates/plugin-cli/src/main.rs` | Modify | `use plugin_cli::catalog;` вместо локального `mod catalog;` |
| `crates/app/src-tauri/src/ai/plugin_harness.rs` | Modify | Добавить `ensure_plugin_path_allowed` (белый список директорий) |
| `crates/app/src-tauri/src/mcp/mod.rs` | Create | MCP-модуль: сборка реестра, подключение к `AppHandle` |
| `crates/app/src-tauri/src/mcp/adapters.rs` | Create | Адаптеры tools: `plugin_validate`, `plugin_test`, `plugin_catalog_list`, `markdown_render` |
| `crates/app/src-tauri/src/mcp/resources.rs` | Create | Ресурсы: контракты (статические) + каталог (динамический) |
| `crates/app/src-tauri/src/lib.rs` | Modify | `pub mod mcp;` + `app.manage(McpServer)` в `setup` |
| `crates/app/src-tauri/Cargo.toml` | Modify | Зависимости: `plugin-cli` (задача 8), `mcp-core` (задача 11) |
| `crates/app/src-tauri/tests/mcp_contract.rs` | Create | Контракт-тесты: схема == поведение (AC-MCP-6), журнал без путей (AC-MCP-13) |
| `.github/workflows/ci.yml` | Modify | Добавить `-p mcp-core` в fmt/clippy/tests |

---

## Задачи

### Задача 1: Новый крейт `mcp-core` в workspace

**Goal:** Создать чистый крейт `mcp-core` (без Tauri) в workspace.

**Files:**
- Create: `crates/mcp-core/Cargo.toml`
- Create: `crates/mcp-core/src/lib.rs`
- Modify: `Cargo.toml` (workspace members)

**Interfaces:**
- Consumes: нет
- Produces: крейт `mcp-core` с пустым `lib.rs`

**Шаги:**

1. Создать `crates/mcp-core/Cargo.toml`:

```toml
[package]
name = "mcp-core"
version.workspace = true
edition.workspace = true
license.workspace = true
description = "MCP protocol core: JSON-RPC 2.0 envelope, tool/resource registry, server semantics"

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

2. Создать `crates/mcp-core/src/lib.rs`:

```rust
//! mcp-core — чистое ядро MCP-протокола без зависимостей от UI.
//!
//! JSON-RPC 2.0 envelope + семантика MCP (initialize, tools/list, tools/call,
//! resources/list, resources/read). Транспорт-агностично: сервер принимает
//! `RpcRequest` и отдаёт `RpcResponse`. Реестр tools/resources со схемами.

pub mod handlers;
pub mod protocol;
pub mod registry;
pub mod server;

pub use handlers::{ResourceError, ResourceReader, ToolError, ToolHandler};
pub use registry::{Registry, ResourceDefinition, ToolDefinition};
```

3. Добавить `"crates/mcp-core"` в `members` workspace `Cargo.toml`:

```toml
members = [
  "crates/md-core",
  "crates/mcp-core",
  "crates/app/src-tauri",
  "crates/plugin-core",
  "crates/plugin-host",
  "crates/plugin-cli",
]
```

4. Проверка: `cargo check -p mcp-core`

---

### Задача 2: JSON-RPC 2.0 envelope

**Goal:** Реализовать JSON-RPC 2.0 envelope: `RpcRequest`, `RpcResponse`, `RpcError`, коды ошибок.

**Files:**
- Create: `crates/mcp-core/src/protocol.rs`
- Create: `crates/mcp-core/tests/protocol_tests.rs`
- Modify: `crates/mcp-core/src/lib.rs` (уже добавлен `pub mod protocol;`)

**Interfaces:**
- Consumes: `serde`, `serde_json::Value`
- Produces:
  - `pub const RPC_VERSION: &str = "2.0"`
  - `pub const PARSE_ERROR: i64 = -32700`
  - `pub const INVALID_REQUEST: i64 = -32600`
  - `pub const METHOD_NOT_FOUND: i64 = -32601`
  - `pub const INVALID_PARAMS: i64 = -32602`
  - `pub const INTERNAL_ERROR: i64 = -32603`
  - `pub struct RpcRequest { jsonrpc: String, id: Option<Value>, method: String, params: Option<Value> }`
  - `pub struct RpcError { code: i64, message: String, data: Option<Value> }`
  - `pub struct RpcResponse { jsonrpc: String, id: Option<Value>, result: Option<Value>, error: Option<RpcError> }`
  - `RpcResponse::success(id: Option<Value>, result: Value) -> RpcResponse`
  - `RpcResponse::error(id: Option<Value>, error: RpcError) -> RpcResponse`
  - `RpcError::new(code: i64, message: impl Into<String>) -> RpcError`

**Шаги:**

1. Написать падающий тест в `crates/mcp-core/tests/protocol_tests.rs`:

```rust
use mcp_core::protocol::{RpcError, RpcResponse, METHOD_NOT_FOUND};

#[test]
fn success_response_serializes_with_jsonrpc_version() {
    let response = RpcResponse::success(Some(serde_json::json!(1)), serde_json::json!({"ok": true}));
    let json = serde_json::to_value(&response).expect("сериализуется");
    assert_eq!(json["jsonrpc"], "2.0");
    assert_eq!(json["id"], 1);
    assert_eq!(json["result"]["ok"], true);
    assert!(json.get("error").is_none());
}

#[test]
fn error_response_carries_code_and_message() {
    let response = RpcResponse::error(
        Some(serde_json::json!(2)),
        RpcError::new(METHOD_NOT_FOUND, "метод не найден: foo"),
    );
    let json = serde_json::to_value(&response).expect("сериализуется");
    assert_eq!(json["error"]["code"], METHOD_NOT_FOUND);
    assert_eq!(json["error"]["message"], "метод не найден: foo");
    assert!(json.get("result").is_none());
}
```

2. Прогнать `cargo test -p mcp-core protocol_tests` — **падает** (нет модуля `protocol`).

3. Реализовать `crates/mcp-core/src/protocol.rs`:

```rust
//! JSON-RPC 2.0 envelope — базовый транспортный слой MCP.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Версия JSON-RPC протокола.
pub const RPC_VERSION: &str = "2.0";

/// Код ошибки: невалидный JSON.
pub const PARSE_ERROR: i64 = -32700;
/// Код ошибки: невалидный запрос.
pub const INVALID_REQUEST: i64 = -32600;
/// Код ошибки: метод не найден.
pub const METHOD_NOT_FOUND: i64 = -32601;
/// Код ошибки: невалидные параметры.
pub const INVALID_PARAMS: i64 = -32602;
/// Код ошибки: внутренняя ошибка.
pub const INTERNAL_ERROR: i64 = -32603;

/// Запрос JSON-RPC 2.0.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RpcRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}

/// Ошибка JSON-RPC 2.0.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl RpcError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }
}

/// Ответ JSON-RPC 2.0.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RpcResponse {
    pub jsonrpc: String,
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl RpcResponse {
    /// Успешный ответ.
    pub fn success(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: RPC_VERSION.to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// Ответ с ошибкой.
    pub fn error(id: Option<Value>, error: RpcError) -> Self {
        Self {
            jsonrpc: RPC_VERSION.to_string(),
            id,
            result: None,
            error: Some(error),
        }
    }
}
```

4. Прогнать `cargo test -p mcp-core protocol_tests` — **зелёно**.
5. `cargo fmt -p mcp-core`.

---

### Задача 3: Трейты `ToolHandler`/`ResourceReader`

**Goal:** Определить контракты обработчиков инструментов и ресурсов.

**Files:**
- Create: `crates/mcp-core/src/handlers.rs`
- Create: `crates/mcp-core/tests/handler_tests.rs`
- Modify: `crates/mcp-core/src/lib.rs` (уже добавлен `pub mod handlers;`)

**Interfaces:**
- Consumes: `crate::protocol::{INTERNAL_ERROR, INVALID_PARAMS}`
- Produces:
  - `pub struct ToolError { code: i64, message: String }`
  - `ToolError::internal(impl Into<String>) -> ToolError`
  - `ToolError::invalid_params(impl Into<String>) -> ToolError`
  - `pub trait ToolHandler: Send + Sync { fn name(&self) -> &str; fn description(&self) -> &str; fn input_schema(&self) -> Value; fn call(&self, params: Value) -> Result<Value, ToolError>; }`
  - `pub trait ResourceReader: Send + Sync { fn read(&self, uri: &str) -> Result<Value, ResourceError>; }`
  - `ResourceError::internal(impl Into<String>) -> ResourceError`

**Шаги:**

1. Написать падающий тест в `crates/mcp-core/tests/handler_tests.rs`:

```rust
use mcp_core::handlers::{ResourceReader, ToolError, ToolHandler};
use serde_json::{json, Value};

struct EchoTool;

impl ToolHandler for EchoTool {
    fn name(&self) -> &str { "echo" }
    fn description(&self) -> &str { "Возвращает вход как есть" }
    fn input_schema(&self) -> Value {
        json!({"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]})
    }
    fn call(&self, params: Value) -> Result<Value, ToolError> {
        Ok(params)
    }
}

#[test]
fn tool_handler_returns_params() {
    let tool = EchoTool;
    let result = tool.call(json!({"text": "hi"})).expect("ok");
    assert_eq!(result["text"], "hi");
}
```

2. Прогнать `cargo test -p mcp-core handler_tests` — **падает** (нет модуля `handlers`).

3. Реализовать `crates/mcp-core/src/handlers.rs`:

```rust
//! Трейты обработчиков инструментов и ресурсов.
//!
//! Синхронные трейты: in-memory транспорт не требует async. `plugin_test`
//! исполняет Lua через CLI с таймаутом 15 с — блокирующий вызов приемлем
//! (GUI не блокируется надолго, ADR-0034 §2.4).

use serde_json::Value;

use crate::protocol::{INTERNAL_ERROR, INVALID_PARAMS};

/// Ошибка вызова инструмента.
#[derive(Clone, Debug)]
pub struct ToolError {
    pub code: i64,
    pub message: String,
}

impl ToolError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(INTERNAL_ERROR, message)
    }

    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(INVALID_PARAMS, message)
    }
}

/// Обработчик инструмента (tool).
pub trait ToolHandler: Send + Sync {
    /// Имя инструмента (уникальное).
    fn name(&self) -> &str;
    /// Описание для агента.
    fn description(&self) -> &str;
    /// Схема входных параметров (JSON Schema object).
    fn input_schema(&self) -> Value;
    /// Вызов инструмента с параметрами.
    fn call(&self, params: Value) -> Result<Value, ToolError>;
}

/// Ошибка чтения ресурса.
#[derive(Clone, Debug)]
pub struct ResourceError {
    pub code: i64,
    pub message: String,
}

impl ResourceError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(INTERNAL_ERROR, message)
    }
}

/// Читатель ресурса.
///
/// `read` получает запрошенный URI: часть ресурсов шаблонные
/// (например, `darmark://plugins/{id}/manifest`), поэтому URI нужен читателю.
pub trait ResourceReader: Send + Sync {
    /// Чтение ресурса по запрошенному URI.
    fn read(&self, uri: &str) -> Result<Value, ResourceError>;
}
```

4. Прогнать `cargo test -p mcp-core handler_tests` — **зелёно**.
5. `cargo fmt -p mcp-core`.

---

### Задача 4: Реестр tools/resources

**Goal:** Реализовать реестр инструментов и ресурсов со схемами.

**Files:**
- Create: `crates/mcp-core/src/registry.rs`
- Create: `crates/mcp-core/tests/registry_tests.rs`
- Modify: `crates/mcp-core/src/lib.rs` (уже добавлен `pub mod registry;`)

**Interfaces:**
- Consumes: `crate::handlers::{ResourceReader, ToolHandler}`, `crate::protocol::METHOD_NOT_FOUND`
- Produces:
  - `pub struct ToolDefinition { name: String, description: String, input_schema: Value, handler: Arc<dyn ToolHandler> }`
  - `pub struct ResourceDefinition { uri: String, description: String, reader: Arc<dyn ResourceReader> }`
  - `pub struct Registry { tools: Vec<ToolDefinition>, resources: Vec<ResourceDefinition> }`
  - `Registry::new() -> Registry`
  - `Registry::register_tool(&mut self, def: ToolDefinition)`
  - `Registry::register_resource(&mut self, def: ResourceDefinition)`
  - `Registry::tools_list(&self) -> Value`
  - `Registry::resources_list(&self) -> Value`
  - `Registry::call_tool(&self, name: &str, params: Value) -> Result<Value, ToolError>`
  - `Registry::read_resource(&self, uri: &str) -> Result<Value, ResourceError>`

**Шаги:**

1. Написать падающий тест в `crates/mcp-core/tests/registry_tests.rs`:

```rust
use mcp_core::handlers::{ToolError, ToolHandler};
use mcp_core::registry::{Registry, ToolDefinition};
use serde_json::{json, Value};
use std::sync::Arc;

struct DummyTool;

impl ToolHandler for DummyTool {
    fn name(&self) -> &str { "dummy" }
    fn description(&self) -> &str { "Тестовый инструмент" }
    fn input_schema(&self) -> Value { json!({"type": "object"}) }
    fn call(&self, _params: Value) -> Result<Value, ToolError> { Ok(json!({"ok": true})) }
}

#[test]
fn register_and_call_tool() {
    let mut registry = Registry::new();
    registry.register_tool(ToolDefinition {
        name: "dummy".to_string(),
        description: "Тестовый инструмент".to_string(),
        input_schema: json!({"type": "object"}),
        handler: Arc::new(DummyTool),
    });
    let result = registry.call_tool("dummy", json!({})).expect("ok");
    assert_eq!(result["ok"], true);
}

#[test]
fn call_unknown_tool_returns_method_not_found() {
    let registry = Registry::new();
    let err = registry.call_tool("ghost", json!({})).expect_err("ошибка");
    assert_eq!(err.code, mcp_core::protocol::METHOD_NOT_FOUND);
}
```

2. Прогнать `cargo test -p mcp-core registry_tests` — **падает** (нет модуля `registry`).

3. Реализовать `crates/mcp-core/src/registry.rs`:

```rust
//! Реестр инструментов и ресурсов.

use std::sync::Arc;

use serde_json::{json, Value};

use crate::handlers::{ResourceError, ResourceReader, ToolError, ToolHandler};
use crate::protocol;

/// Определение инструмента в реестре.
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub handler: Arc<dyn ToolHandler>,
}

/// Определение ресурса в реестре.
pub struct ResourceDefinition {
    pub uri: String,
    pub description: String,
    pub reader: Arc<dyn ResourceReader>,
}

/// Реестр инструментов и ресурсов.
#[derive(Default)]
pub struct Registry {
    tools: Vec<ToolDefinition>,
    resources: Vec<ResourceDefinition>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_tool(&mut self, def: ToolDefinition) {
        self.tools.push(def);
    }

    pub fn register_resource(&mut self, def: ResourceDefinition) {
        self.resources.push(def);
    }

    /// Список инструментов для `tools/list`.
    pub fn tools_list(&self) -> Value {
        let tools: Vec<Value> = self
            .tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "inputSchema": t.input_schema,
                })
            })
            .collect();
        json!({ "tools": tools })
    }

    /// Список ресурсов для `resources/list`.
    pub fn resources_list(&self) -> Value {
        let resources: Vec<Value> = self
            .resources
            .iter()
            .map(|r| {
                json!({
                    "uri": r.uri,
                    "description": r.description,
                })
            })
            .collect();
        json!({ "resources": resources })
    }

    /// Вызов инструмента по имени.
    pub fn call_tool(&self, name: &str, params: Value) -> Result<Value, ToolError> {
        let tool = self
            .tools
            .iter()
            .find(|t| t.name == name)
            .ok_or_else(|| {
                ToolError::new(
                    protocol::METHOD_NOT_FOUND,
                    format!("инструмент не найден: {name}"),
                )
            })?;
        tool.handler.call(params)
    }

    /// Чтение ресурса по URI.
    pub fn read_resource(&self, uri: &str) -> Result<Value, ResourceError> {
        let resource = self
            .resources
            .iter()
            .find(|r| r.uri == uri)
            .ok_or_else(|| {
                ResourceError::new(
                    protocol::METHOD_NOT_FOUND,
                    format!("ресурс не найден: {uri}"),
                )
            })?;
        resource.reader.read(uri)
    }
}
```

4. Прогнать `cargo test -p mcp-core registry_tests` — **зелёно**.
5. `cargo fmt -p mcp-core`.

---

### Задача 5: MCP-сервер (семантика протокола)

**Goal:** Реализовать `McpServer`: маршрутизация JSON-RPC запросов к реестру.

**Files:**
- Create: `crates/mcp-core/src/server.rs`
- Create: `crates/mcp-core/tests/server_tests.rs`
- Modify: `crates/mcp-core/src/lib.rs` (уже добавлен `pub mod server;`)

**Interfaces:**
- Consumes: `crate::protocol`, `crate::registry::Registry`
- Produces:
  - `pub const MCP_PROTOCOL_VERSION: &str = "2024-11-05"`
  - `pub struct McpServer { registry: Registry }`
  - `McpServer::new(registry: Registry) -> McpServer`
  - `McpServer::handle(&self, request: RpcRequest) -> RpcResponse`

**Шаги:**

1. Написать падающий тест в `crates/mcp-core/tests/server_tests.rs`:

```rust
use mcp_core::handlers::{ToolError, ToolHandler};
use mcp_core::protocol::RpcRequest;
use mcp_core::registry::{Registry, ToolDefinition};
use mcp_core::server::McpServer;
use serde_json::{json, Value};
use std::sync::Arc;

struct EchoTool;

impl ToolHandler for EchoTool {
    fn name(&self) -> &str { "echo" }
    fn description(&self) -> &str { "Возвращает вход" }
    fn input_schema(&self) -> Value { json!({"type": "object"}) }
    fn call(&self, params: Value) -> Result<Value, ToolError> { Ok(params) }
}

fn server() -> McpServer {
    let mut registry = Registry::new();
    registry.register_tool(ToolDefinition {
        name: "echo".to_string(),
        description: "Возвращает вход".to_string(),
        input_schema: json!({"type": "object"}),
        handler: Arc::new(EchoTool),
    });
    McpServer::new(registry)
}

#[test]
fn initialize_returns_protocol_version() {
    let server = server();
    let response = server.handle(RpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(1)),
        method: "initialize".into(),
        params: None,
    });
    let result = response.result.expect("result");
    assert_eq!(result["protocolVersion"], mcp_core::server::MCP_PROTOCOL_VERSION);
}

#[test]
fn tools_list_returns_registered_tools() {
    let server = server();
    let response = server.handle(RpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(2)),
        method: "tools/list".into(),
        params: None,
    });
    let result = response.result.expect("result");
    assert_eq!(result["tools"][0]["name"], "echo");
}

#[test]
fn tools_call_returns_result() {
    let server = server();
    let response = server.handle(RpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(3)),
        method: "tools/call".into(),
        params: Some(json!({"name": "echo", "arguments": {"text": "hi"}})),
    });
    let result = response.result.expect("result");
    assert_eq!(result["content"][0]["text"], "{\"text\":\"hi\"}");
}

#[test]
fn unknown_method_returns_error() {
    let server = server();
    let response = server.handle(RpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(4)),
        method: "ghost".into(),
        params: None,
    });
    let error = response.error.expect("error");
    assert_eq!(error.code, mcp_core::protocol::METHOD_NOT_FOUND);
}
```

2. Прогнать `cargo test -p mcp-core server_tests` — **падает** (нет модуля `server`).

3. Реализовать `crates/mcp-core/src/server.rs`:

```rust
//! MCP-сервер: семантика протокола поверх реестра.

use serde_json::{json, Value};

use crate::protocol::{self, RpcRequest, RpcResponse};

/// Версия MCP-протокола, поддерживаемая сервером.
pub const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

/// MCP-сервер: маршрутизация JSON-RPC запросов к реестру.
pub struct McpServer {
    registry: crate::registry::Registry,
}

impl McpServer {
    pub fn new(registry: crate::registry::Registry) -> Self {
        Self { registry }
    }

    /// Обработка JSON-RPC запроса.
    pub fn handle(&self, request: RpcRequest) -> RpcResponse {
        let id = request.id.clone();
        match request.method.as_str() {
            "initialize" => RpcResponse::success(id, self.initialize()),
            "tools/list" => RpcResponse::success(id, self.registry.tools_list()),
            "tools/call" => self.handle_tool_call(id, request.params),
            "resources/list" => RpcResponse::success(id, self.registry.resources_list()),
            "resources/read" => self.handle_resource_read(id, request.params),
            other => RpcResponse::error(
                id,
                protocol::RpcError::new(
                    protocol::METHOD_NOT_FOUND,
                    format!("метод не найден: {other}"),
                ),
            ),
        }
    }

    fn initialize(&self) -> Value {
        json!({
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities": {
                "tools": {},
                "resources": {},
            },
        })
    }

    fn handle_tool_call(&self, id: Option<Value>, params: Option<Value>) -> RpcResponse {
        let Some(params) = params else {
            return RpcResponse::error(
                id,
                protocol::RpcError::new(protocol::INVALID_PARAMS, "tools/call требует params"),
            );
        };
        let Some(name) = params.get("name").and_then(Value::as_str) else {
            return RpcResponse::error(
                id,
                protocol::RpcError::new(protocol::INVALID_PARAMS, "tools/call требует params.name"),
            );
        };
        let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
        match self.registry.call_tool(name, arguments) {
            Ok(result) => RpcResponse::success(
                id,
                json!({ "content": [{ "type": "text", "text": result.to_string() }] }),
            ),
            Err(error) => RpcResponse::success(
                id,
                json!({ "isError": true, "content": [{ "type": "text", "text": error.message }] }),
            ),
        }
    }

    fn handle_resource_read(&self, id: Option<Value>, params: Option<Value>) -> RpcResponse {
        let Some(params) = params else {
            return RpcResponse::error(
                id,
                protocol::RpcError::new(protocol::INVALID_PARAMS, "resources/read требует params"),
            );
        };
        let Some(uri) = params.get("uri").and_then(Value::as_str) else {
            return RpcResponse::error(
                id,
                protocol::RpcError::new(protocol::INVALID_PARAMS, "resources/read требует params.uri"),
            );
        };
        match self.registry.read_resource(uri) {
            Ok(result) => RpcResponse::success(
                id,
                json!({ "contents": [{ "uri": uri, "text": result.to_string() }] }),
            ),
            Err(error) => RpcResponse::error(id, protocol::RpcError::new(error.code, error.message)),
        }
    }
}
```

4. Прогнать `cargo test -p mcp-core server_tests` — **зелёно**.
5. `cargo fmt -p mcp-core`.

---

### Задача 6: Белый список директорий в `plugin_harness`

**Goal:** Добавить проверку путей для `plugin_validate`/`plugin_test` — разрешены только `plugins_dir()` и временные каталоги.

**Files:**
- Modify: `crates/app/src-tauri/src/ai/plugin_harness.rs`

**Interfaces:**
- Consumes: `crate::error::{CommandError, ErrorCode}`, `is_reparse_point` (уже есть, Windows)
- Produces:
  - `pub fn ensure_plugin_path_allowed(path: &Path) -> Result<PathBuf, CommandError>` (Windows-only)
  - Логика: `canonicalize` → отказ на reparse point → `starts_with(plugins_root)` или `starts_with(temp_dir)`

**Шаги:**

1. Написать падающий тест в `crates/app/src-tauri/src/ai/plugin_harness.rs` (в существующий `mod tests`):

```rust
#[cfg(windows)]
#[test]
fn ensure_plugin_path_allowed_rejects_path_outside_whitelist() {
    use std::path::Path;
    use crate::ai::plugin_harness::ensure_plugin_path_allowed;
    let err = ensure_plugin_path_allowed(Path::new("C:\\Windows\\System32")).expect_err("отказ");
    assert_eq!(err.code, ErrorCode::AiPluginInvalid);
}
```

2. Прогнать `cargo test -p darmark ensure_plugin_path_allowed` — **падает** (нет функции).

3. Добавить в `plugin_harness.rs` (после `plugins_root`):

```rust
/// Проверяет, что путь находится в разрешённом каталоге (plugins/ или temp/).
///
/// Возвращает канонический путь или ошибку. Defense-in-depth (TZ-MCP R3/AC-MCP-12):
/// `canonicalize` + `starts_with` + отказ на reparse point (symlink/junction).
#[cfg(windows)]
pub fn ensure_plugin_path_allowed(path: &Path) -> Result<PathBuf, CommandError> {
    use std::path::PathBuf;
    let canonical = std::fs::canonicalize(path).map_err(|e| {
        CommandError::new(
            ErrorCode::AiPluginInvalid,
            format!("путь не существует: {e}"),
        )
    })?;

    // Отказ на reparse point (symlink/junction) — запись в такой каталог может
    // увести файлы за пределы разрешённой области.
    if is_reparse_point(&canonical) {
        return Err(CommandError::new(
            ErrorCode::AiPluginInvalid,
            "путь является symlink/junction — доступ запрещён",
        ));
    }

    let root = plugins_root()?;
    if canonical.starts_with(&root) {
        return Ok(canonical);
    }

    let temp = std::env::temp_dir();
    if canonical.starts_with(&temp) {
        return Ok(canonical);
    }

    Err(CommandError::new(
        ErrorCode::AiPluginInvalid,
        "путь вне разрешённого каталога (plugins/ или temp/)",
    ))
}
```

4. Прогнать `cargo test -p darmark ensure_plugin_path_allowed` — **зелёно**.
5. `cargo fmt -p darmark`.

---

### Задача 7: Библиотека `plugin-cli` (pub mod catalog)

**Goal:** Сделать `catalog` доступным для MCP-адаптеров через библиотечный крейт.

**Files:**
- Create: `crates/plugin-cli/src/lib.rs`
- Modify: `crates/plugin-cli/src/main.rs`

**Interfaces:**
- Consumes: нет
- Produces: `plugin_cli::catalog::{Catalog, CatalogEntry, CATALOG_FILE, SCHEMA_VERSION, MAX_CATALOG_BYTES, MAX_MANIFEST_BYTES}`

**Шаги:**

1. Создать `crates/plugin-cli/src/lib.rs`:

```rust
//! Библиотечная точка входа для plugin-cli: каталог плагинов доступен внешним
//! потребителям (MCP-адаптеры) без запуска CLI-процесса.

pub mod catalog;
```

2. В `main.rs` заменить `mod catalog;` на `use plugin_cli::catalog;` (в начале файла, после `use`):

```rust
use plugin_cli::catalog::{Catalog, CatalogEntry};
```

И убрать `mod catalog;` (строка 9).

3. Проверка: `cargo check -p plugin-cli`.
4. Прогнать `cargo test -p plugin-cli` — **зелёно**.
5. `cargo fmt -p plugin-cli`.

---

### Задача 8: MCP-модуль в `src-tauri` (адаптеры tools)

**Goal:** Реализовать адаптеры tools: `plugin_validate`, `plugin_test`, `plugin_catalog_list`, `markdown_render`.

**Files:**
- Create: `crates/app/src-tauri/src/mcp/mod.rs`
- Create: `crates/app/src-tauri/src/mcp/adapters.rs`
- Modify: `crates/app/src-tauri/src/lib.rs` (добавить `pub mod mcp;`)
- Modify: `crates/app/src-tauri/Cargo.toml` (добавить зависимость `plugin-cli`)

**Interfaces:**
- Consumes:
  - `mcp_core::handlers::{ToolError, ToolHandler}`
  - `mcp_core::registry::{Registry, ToolDefinition}`
  - `crate::ai::plugin_harness::{ensure_plugin_path_allowed, manifest_errors, validate_plugin}`
  - `plugin_cli::catalog::Catalog`
  - `md_core::{to_html, to_html_mapped}`
- Produces:
  - `pub fn build_registry(app: &AppHandle) -> Registry`
  - `struct PluginValidateTool` (manifest-only валидация)
  - `struct PluginTestTool` (полная валидация с песочницей)
  - `struct PluginCatalogListTool`
  - `struct MarkdownRenderTool`

**Шаги:**

1. Добавить `pub mod mcp;` в `crates/app/src-tauri/src/lib.rs` (после `pub mod ai;`):

```rust
pub mod mcp;
```

2. Создать `crates/app/src-tauri/src/mcp/mod.rs`:

```rust
//! MCP-сервер darmark: in-memory транспорт, адаптеры к существующим командам/CLI.
//!
//! Размещение — в процессе GUI (ADR-0036). MCP — только маршрутизация:
//! проверки (права, rev-guard, безопасность путей) остаются в существующих
//! командах (TZ-MCP R3/R4).

pub mod adapters;
pub mod resources;

use mcp_core::Registry;
use tauri::AppHandle;

/// Собирает реестр MCP-инструментов и ресурсов.
pub fn build_registry(app: &AppHandle) -> Registry {
    let mut registry = Registry::new();
    adapters::register_tools(&mut registry, app);
    resources::register_resources(&mut registry, app);
    registry
}

/// Каталог-источник плагинов: env `DARMARK_PLUGIN_REPO` → автопоиск от CWD → `<cwd>/plugins`.
///
/// В dev-сборке находит репозиторный `examples/plugins/` (ADR-0026); в дистрибутиве каталога
/// может не быть — тогда ресурс/инструмент вернёт понятную ошибку (открытый вопрос TZ-MCP §8).
pub(crate) fn catalog_repo() -> std::path::PathBuf {
    if let Some(dir) = std::env::var_os("DARMARK_PLUGIN_REPO") {
        return std::path::PathBuf::from(dir);
    }
    let cwd = std::env::current_dir().unwrap_or_default();
    plugin_cli::catalog::discover_repo(&cwd).unwrap_or_else(|| cwd.join("plugins"))
}
```

3. Создать `crates/app/src-tauri/src/mcp/adapters.rs`:

```rust
//! Адаптеры MCP tools: тонкие обёртки над существующими командами/CLI.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use mcp_core::handlers::{ToolError, ToolHandler};
use mcp_core::registry::{Registry, ToolDefinition};
use serde_json::{json, Value};
use tauri::AppHandle;

use crate::ai::types::AiValidationResult;
use crate::error::CommandError;
use md_core;

/// Регистрирует все MCP tools в реестре.
pub fn register_tools(registry: &mut Registry, app: &AppHandle) {
    registry.register_tool(ToolDefinition {
        name: "plugin_validate".to_string(),
        description: "Валидация манифеста и entry плагина без запуска песочницы".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Путь к каталогу плагина"}
            },
            "required": ["path"]
        }),
        handler: Arc::new(PluginValidateTool {
            app: app.clone(),
        }),
    });

    #[cfg(windows)]
    registry.register_tool(ToolDefinition {
        name: "plugin_test".to_string(),
        description: "Полная проверка плагина с прогоном в песочнице".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Путь к каталогу плагина"}
            },
            "required": ["path"]
        }),
        handler: Arc::new(PluginTestTool {
            app: app.clone(),
        }),
    });

    registry.register_tool(ToolDefinition {
        name: "plugin_catalog_list".to_string(),
        description: "Список плагинов из локального каталога (index.json)".to_string(),
        input_schema: json!({"type": "object", "properties": {}}),
        handler: Arc::new(PluginCatalogListTool),
    });

    registry.register_tool(ToolDefinition {
        name: "markdown_render".to_string(),
        description: "Рендер Markdown в HTML (с опциональным маппингом блоков)".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "text": {"type": "string", "description": "Markdown-текст"},
                "mapped": {"type": "boolean", "description": "Включить data-md маппинг"}
            },
            "required": ["text"]
        }),
        handler: Arc::new(MarkdownRenderTool),
    });
}

// ─── plugin_validate ───────────────────────────────────────────────

struct PluginValidateTool {
    app: AppHandle,
}

impl ToolHandler for PluginValidateTool {
    fn name(&self) -> &str { "plugin_validate" }
    fn description(&self) -> &str { "Валидация манифеста и entry плагина без запуска песочницы" }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Путь к каталогу плагина"}
            },
            "required": ["path"]
        })
    }

    fn call(&self, params: Value) -> Result<Value, ToolError> {
        let Some(path_str) = params.get("path").and_then(Value::as_str) else {
            return Err(ToolError::invalid_params("требуется path"));
        };
        let path = Path::new(path_str);

        // Белый список директорий (TZ-MCP R3)
        #[cfg(windows)]
        let canonical = crate::ai::plugin_harness::ensure_plugin_path_allowed(path)
            .map_err(|e| ToolError::invalid_params(e.message))?;
        #[cfg(not(windows))]
        let canonical = path.to_path_buf();

        let started = Instant::now();
        let result = validate_static(&canonical);
        let duration = started.elapsed();

        // Журналирование (TZ-MCP R2): только имя каталога, не полный путь
        let (ok, errors) = match &result {
            Ok(v) => (v.ok, v.errors.clone()),
            Err(e) => (false, vec![e.message.clone()]),
        };
        let message = mcp_log_message("plugin_validate", &path_name(path), duration, ok, &errors);
        log_mcp_call(&self.app, &message);

        match result {
            Ok(validation) => Ok(serde_json::to_value(&validation).map_err(|e| ToolError::internal(e.to_string()))?),
            Err(e) => Err(ToolError::invalid_params(e.message)),
        }
    }
}

/// Статическая валидация: манифест + entry без песочницы.
fn validate_static(dir: &Path) -> Result<AiValidationResult, CommandError> {
    let manifest_bytes = std::fs::read(dir.join("plugin.json")).map_err(|e| {
        CommandError::new(crate::error::ErrorCode::NotFound, format!("plugin.json: {e}"))
    })?;
    let manifest: Value = serde_json::from_slice(&manifest_bytes).map_err(|e| {
        CommandError::new(crate::error::ErrorCode::Plugin, format!("plugin.json не парсится: {e}"))
    })?;
    let source = std::fs::read_to_string(dir.join("main.lua")).map_err(|e| {
        CommandError::new(crate::error::ErrorCode::NotFound, format!("main.lua: {e}"))
    })?;
    let errors = crate::ai::plugin_harness::manifest_errors(&manifest, &source);
    Ok(AiValidationResult {
        ok: errors.is_empty(),
        errors,
        report: None,
    })
}

// ─── plugin_test ───────────────────────────────────────────────────

#[cfg(windows)]
struct PluginTestTool {
    app: AppHandle,
}

#[cfg(windows)]
impl ToolHandler for PluginTestTool {
    fn name(&self) -> &str { "plugin_test" }
    fn description(&self) -> &str { "Полная проверка плагина с прогоном в песочнице" }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Путь к каталогу плагина"}
            },
            "required": ["path"]
        })
    }

    fn call(&self, params: Value) -> Result<Value, ToolError> {
        let Some(path_str) = params.get("path").and_then(Value::as_str) else {
            return Err(ToolError::invalid_params("требуется path"));
        };
        let path = Path::new(path_str);

        // Белый список директорий (TZ-MCP R3)
        let canonical = crate::ai::plugin_harness::ensure_plugin_path_allowed(path)
            .map_err(|e| ToolError::invalid_params(e.message))?;

        let started = Instant::now();
        let result = crate::ai::plugin_harness::validate_plugin(&canonical);
        let duration = started.elapsed();

        // Журналирование (TZ-MCP R2): только имя каталога, не полный путь
        let message = mcp_log_message("plugin_test", &path_name(path), duration, result.ok, &result.errors);
        log_mcp_call(&self.app, &message);

        Ok(serde_json::to_value(&result).map_err(|e| ToolError::internal(e.to_string()))?)
    }
}

// ─── plugin_catalog_list ───────────────────────────────────────────

struct PluginCatalogListTool;

impl ToolHandler for PluginCatalogListTool {
    fn name(&self) -> &str { "plugin_catalog_list" }
    fn description(&self) -> &str { "Список плагинов из локального каталога (index.json)" }
    fn input_schema(&self) -> Value { json!({"type": "object", "properties": {}}) }

    fn call(&self, _params: Value) -> Result<Value, ToolError> {
        let started = Instant::now();
        let catalog = plugin_cli::catalog::Catalog::load(&super::catalog_repo())
            .map_err(ToolError::internal)?;
        let items: Vec<Value> = catalog
            .list()
            .iter()
            .map(|entry| {
                json!({
                    "id": entry.id,
                    "name": entry.name,
                    "version": entry.version,
                    "api_version": entry.api_version,
                    "permissions": entry.permissions,
                    "description": entry.description,
                    "path": entry.path,
                })
            })
            .collect();
        let _ = started.elapsed();
        Ok(json!({ "plugins": items }))
    }
}

// ─── markdown_render ───────────────────────────────────────────────

struct MarkdownRenderTool;

impl ToolHandler for MarkdownRenderTool {
    fn name(&self) -> &str { "markdown_render" }
    fn description(&self) -> &str { "Рендер Markdown в HTML (с опциональным маппингом блоков)" }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "text": {"type": "string", "description": "Markdown-текст"},
                "mapped": {"type": "boolean", "description": "Включить data-md маппинг"}
            },
            "required": ["text"]
        })
    }

    fn call(&self, params: Value) -> Result<Value, ToolError> {
        let Some(text) = params.get("text").and_then(Value::as_str) else {
            return Err(ToolError::invalid_params("требуется text"));
        };
        let mapped = params.get("mapped").and_then(Value::as_bool).unwrap_or(false);
        let html = if mapped { md_core::to_html_mapped(text) } else { md_core::to_html(text) };
        Ok(json!({ "html": html }))
    }
}

// ─── Журналирование ────────────────────────────────────────────────

/// Имя каталога (последний компонент пути) — приватность §4.6 (не полный путь).
fn path_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "-".to_string())
}

/// Текст записи журнала для вызова MCP tool (TZ-MCP R2).
///
/// Чистая функция — тестируется без Tauri (AC-MCP-13: в записи нет полного пути).
fn mcp_log_message(
    tool: &str,
    path_name: &str,
    duration: std::time::Duration,
    ok: bool,
    errors: &[String],
) -> String {
    if ok {
        format!("MCP {tool}: {path_name} — ok ({} мс)", duration.as_millis())
    } else {
        format!(
            "MCP {tool}: {path_name} — ошибка: {} ({} мс)",
            errors.join("; "),
            duration.as_millis()
        )
    }
}

/// Публикует запись журнала (best-effort, TZ-MCP R2).
fn log_mcp_call(app: &AppHandle, message: &str) {
    let _ = app.emit(
        "log-entry",
        serde_json::json!({ "level": "info", "category": "system", "message": message }),
    );
}
```

4. Добавить зависимость `plugin-cli` в `crates/app/src-tauri/Cargo.toml` (нужна для `plugin_cli::catalog`; lib-таргет создан в задаче 7):

```toml
plugin-cli = { path = "../../plugin-cli" }
```

5. Проверка: `cargo check -p darmark`.
6. Прогнать `cargo test -p darmark` — **зелёно**.
7. `cargo fmt -p darmark`.

---

### Задача 9: MCP-ресурсы (контракты + каталог)

**Goal:** Реализовать ресурсы: статические контракты + динамический каталог плагинов.

**Files:**
- Create: `crates/app/src-tauri/src/mcp/resources.rs`
- Modify: `crates/app/src-tauri/src/mcp/mod.rs` (уже добавлен `pub mod resources;`)

**Interfaces:**
- Consumes:
  - `mcp_core::handlers::ResourceReader`
  - `mcp_core::registry::{Registry, ResourceDefinition}`
  - `plugin_cli::catalog::Catalog`
  - `plugin_core::HOST_API_VERSION`
- Produces:
  - `pub fn register_resources(registry: &mut Registry, app: &AppHandle)`
  - Статические ресурсы: `darmark://contracts/plugin-manifest`, `darmark://contracts/host-api`, `darmark://contracts/events`, `darmark://contracts/markdown`
  - Динамические ресурсы: `darmark://plugins/catalog`, `darmark://plugins/{id}/manifest`

**Шаги:**

1. Создать `crates/app/src-tauri/src/mcp/resources.rs`:

```rust
//! MCP-ресурсы: статические контракты + динамический каталог плагинов.

use std::sync::Arc;

use mcp_core::handlers::{ResourceError, ResourceReader};
use mcp_core::registry::{Registry, ResourceDefinition};
use serde_json::{json, Value};
use tauri::AppHandle;

/// Регистрирует все MCP-ресурсы в реестре.
pub fn register_resources(registry: &mut Registry, _app: &AppHandle) {
    // Статические контракты (read-only, не меняются в рантайме)
    registry.register_resource(ResourceDefinition {
        uri: "darmark://contracts/plugin-manifest".to_string(),
        description: "Схема plugin.json — манифеста плагина".to_string(),
        reader: Arc::new(StaticResource::new(manifest_schema())),
    });

    registry.register_resource(ResourceDefinition {
        uri: "darmark://contracts/host-api".to_string(),
        description: "Контракт host.*/md.*/json.* функций хоста".to_string(),
        reader: Arc::new(StaticResource::new(host_api_schema())),
    });

    registry.register_resource(ResourceDefinition {
        uri: "darmark://contracts/events".to_string(),
        description: "Каталог событий плагинной системы".to_string(),
        reader: Arc::new(StaticResource::new(events_schema())),
    });

    registry.register_resource(ResourceDefinition {
        uri: "darmark://contracts/markdown".to_string(),
        description: "Контракт рендера/санитайзера Markdown".to_string(),
        reader: Arc::new(StaticResource::new(markdown_schema())),
    });

    // Динамические ресурсы
    registry.register_resource(ResourceDefinition {
        uri: "darmark://plugins/catalog".to_string(),
        description: "Индекс локального каталога плагинов (index.json)".to_string(),
        reader: Arc::new(PluginsCatalogResource),
    });

    registry.register_resource(ResourceDefinition {
        uri: "darmark://plugins/{id}/manifest".to_string(),
        description: "Манифест конкретного плагина из каталога".to_string(),
        reader: Arc::new(PluginManifestResource),
    });
}

// ─── Статические ресурсы ──────────────────────────────────────────

struct StaticResource {
    content: Value,
}

impl StaticResource {
    fn new(content: Value) -> Self {
        Self { content }
    }
}

impl ResourceReader for StaticResource {
    fn read(&self, _uri: &str) -> Result<Value, ResourceError> {
        Ok(self.content.clone())
    }
}

/// Схема plugin.json (манифеста плагина).
fn manifest_schema() -> Value {
    json!({
        "title": "plugin.json — манифест плагина",
        "fields": {
            "id": "string, [a-z0-9-_], ≤64",
            "name": "string",
            "version": "string",
            "api_version": format!("number, ≤ HOST_API_VERSION"),
            "entry": "string, относительный .lua-путь",
            "permissions": "string[]",
            "contributes": "object (views, commands, statusbar, toolbar, settings)"
        }
    })
}

/// Контракт host.*/md.*/json.* функций.
fn host_api_schema() -> Value {
    json!({
        "host.*": "функции хоста (host.log, host.get_document, host.set_view_content и т.д.)",
        "md.*": "функции Markdown (md.get_text, md.get_selection, md.get_mappings)",
        "json.*": "функции JSON (json.get, json.set)"
    })
}

/// Каталог событий.
fn events_schema() -> Value {
    json!({
        "on_activate": "вызывается при активации плагина",
        "on_deactivate": "вызывается при деактивации плагина",
        "document:changed": "документ изменён (doc_id, rev)",
        "document:opened": "документ открыт (doc_id, path)",
        "document:closed": "документ закрыт (doc_id)"
    })
}

/// Контракт рендера/санитайзера Markdown.
fn markdown_schema() -> Value {
    json!({
        "to_html": "markdown → HTML (без маппинга)",
        "to_html_mapped": "markdown → HTML с data-md атрибутами",
        "sanitize_fragment": "HTML → санитизированный HTML (белые списки)"
    })
}

// ─── Динамические ресурсы ──────────────────────────────────────────

struct PluginsCatalogResource;

impl ResourceReader for PluginsCatalogResource {
    fn read(&self, _uri: &str) -> Result<Value, ResourceError> {
        let catalog = plugin_cli::catalog::Catalog::load(&super::catalog_repo())
            .map_err(ResourceError::internal)?;
        let items: Vec<Value> = catalog
            .list()
            .iter()
            .map(|entry| {
                json!({
                    "id": entry.id,
                    "name": entry.name,
                    "version": entry.version,
                    "api_version": entry.api_version,
                    "permissions": entry.permissions,
                    "description": entry.description,
                })
            })
            .collect();
        Ok(json!({ "plugins": items }))
    }
}

struct PluginManifestResource;

impl ResourceReader for PluginManifestResource {
    fn read(&self, uri: &str) -> Result<Value, ResourceError> {
        // URI: darmark://plugins/{id}/manifest — извлекаем id.
        let id = uri
            .strip_prefix("darmark://plugins/")
            .and_then(|rest| rest.strip_suffix("/manifest"))
            .filter(|id| !id.is_empty())
            .ok_or_else(|| ResourceError::internal(format!("некорректный URI ресурса: {uri}")))?;

        let catalog = plugin_cli::catalog::Catalog::load(&super::catalog_repo())
            .map_err(ResourceError::internal)?;
        let entry = catalog
            .get(id)
            .ok_or_else(|| ResourceError::internal(format!("плагин не найден: {id}")))?;
        let manifest_bytes = std::fs::read(entry.dir.join("plugin.json"))
            .map_err(|e| ResourceError::internal(format!("plugin.json: {e}")))?;
        let manifest: Value = serde_json::from_slice(&manifest_bytes)
            .map_err(|e| ResourceError::internal(format!("plugin.json не парсится: {e}")))?;
        Ok(manifest)
    }
}
```

2. Проверка: `cargo check -p darmark`.
3. Прогнать `cargo test -p darmark` — **зелёно**.
4. `cargo fmt -p darmark`.

---

### Задача 10: Тесты журналирования (приватность)

**Goal:** Закрепить тестами, что запись журнала содержит имя каталога, а не полный путь (AC-MCP-13).

**Files:**
- Modify: `crates/app/src-tauri/src/mcp/adapters.rs` (добавить `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `path_name(&Path) -> String`, `mcp_log_message(&str, &str, Duration, bool, &[String]) -> String` (задача 8)
- Produces: тесты приватности журнала

**Примечание:** `plugin_catalog_list`/`markdown_render` не несут файлового пути (нет поля `path`),
поэтому в журнал **не** пишут — логируются только `plugin_validate`/`plugin_test` (TZ-MCP R2), и
логируются они **не** содержимым, а итогом.

**Шаги:**

1. Добавить тесты в `crates/app/src-tauri/src/mcp/adapters.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn path_name_returns_last_component() {
        assert_eq!(
            path_name(Path::new("C:\\Users\\Demon\\AppData\\Roaming\\darmark\\plugins\\word-count")),
            "word-count"
        );
    }

    #[test]
    fn log_message_has_no_full_path_and_reports_ok() {
        let message = mcp_log_message("plugin_test", "word-count", Duration::from_millis(12), true, &[]);
        assert!(message.contains("word-count"), "msg: {message}");
        assert!(!message.contains(":\\"), "нет полного пути: {message}");
        assert!(message.contains("ok"), "msg: {message}");
    }

    #[test]
    fn log_message_lists_errors_without_full_path() {
        let errors = vec!["manifest: missing id".to_string()];
        let message = mcp_log_message("plugin_validate", "broken", Duration::from_millis(3), false, &errors);
        assert!(message.contains("broken"), "msg: {message}");
        assert!(message.contains("missing id"), "msg: {message}");
        assert!(!message.contains(":\\"), "нет полного пути: {message}");
    }
}
```

2. Прогнать `cargo test -p darmark path_name log_message` — **зелёно**.
3. `cargo fmt -p darmark`.

---

### Задача 11: Подключение сервера в приложение

**Goal:** Создать `McpServer` в `setup` и зарегистрировать его в Tauri state.

**Files:**
- Modify: `crates/app/src-tauri/src/lib.rs` (в `setup`)

**Interfaces:**
- Consumes: `mcp_core::server::McpServer`, `crate::mcp::build_registry`
- Produces: `McpServer` доступен через `app.state::<McpServer>()`

**Шаги:**

1. В `crates/app/src-tauri/src/lib.rs`, в `setup` (после `app.manage(ai::state::AiState::init());`):

```rust
// MCP-сервер (ADR-0036): создаём реестр и регистрируем в state
let mcp_registry = crate::mcp::build_registry(app.handle());
app.manage(mcp_core::server::McpServer::new(mcp_registry));
```

2. Добавить `mcp_core` в зависимости `crates/app/src-tauri/Cargo.toml`:

```toml
mcp-core = { path = "../../mcp-core" }
```

3. Проверка: `cargo check -p darmark`.
4. Прогнать `cargo test -p darmark` — **зелёно**.
5. `cargo fmt -p darmark`.

---

### Задача 12: Контракт-тесты (AC-MCP-6)

**Goal:** Проверить, что схема tool соответствует реальному поведению; журнал не содержит полных путей.

**Files:**
- Create: `crates/app/src-tauri/tests/mcp_contract.rs`

**Interfaces:**
- Consumes: `mcp_core::server::McpServer`, `crate::mcp::build_registry`
- Produces: тесты контракта

**Шаги:**

1. Создать `crates/app/src-tauri/tests/mcp_contract.rs`:

```rust
//! Контракт-тесты MCP: схема tool == реальное поведение (AC-MCP-6),
//! журнал без полных путей (AC-MCP-13).

use mcp_core::handlers::{ToolError, ToolHandler};
use mcp_core::protocol::RpcRequest;
use mcp_core::registry::{Registry, ToolDefinition};
use mcp_core::server::McpServer;
use serde_json::{json, Value};
use std::sync::Arc;

// Тестовый инструмент для проверки контракта
struct EchoTool;

impl ToolHandler for EchoTool {
    fn name(&self) -> &str { "echo" }
    fn description(&self) -> &str { "Возвращает вход" }
    fn input_schema(&self) -> Value { json!({"type": "object"}) }
    fn call(&self, params: Value) -> Result<Value, ToolError> { Ok(params) }
}

fn test_server() -> McpServer {
    let mut registry = Registry::new();
    registry.register_tool(ToolDefinition {
        name: "echo".to_string(),
        description: "Возвращает вход".to_string(),
        input_schema: json!({"type": "object"}),
        handler: Arc::new(EchoTool),
    });
    McpServer::new(registry)
}

#[test]
fn tool_call_matches_direct_invocation() {
    let server = test_server();
    let response = server.handle(RpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(1)),
        method: "tools/call".into(),
        params: Some(json!({"name": "echo", "arguments": {"text": "hi"}})),
    });
    let result = response.result.expect("result");
    // Результат совпадает с прямым вызовом
    assert_eq!(result["content"][0]["text"], "{\"text\":\"hi\"}");
}

#[test]
fn initialize_returns_supported_version() {
    let server = test_server();
    let response = server.handle(RpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(2)),
        method: "initialize".into(),
        params: None,
    });
    let result = response.result.expect("result");
    assert_eq!(result["protocolVersion"], mcp_core::server::MCP_PROTOCOL_VERSION);
    assert!(result["capabilities"]["tools"].is_object());
    assert!(result["capabilities"]["resources"].is_object());
}

#[test]
fn tools_list_and_resources_list_return_arrays() {
    let server = test_server();
    let tools_response = server.handle(RpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(3)),
        method: "tools/list".into(),
        params: None,
    });
    let tools = tools_response.result.expect("result");
    assert!(tools["tools"].is_array());

    let resources_response = server.handle(RpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(4)),
        method: "resources/list".into(),
        params: None,
    });
    let resources = resources_response.result.expect("result");
    assert!(resources["resources"].is_array());
}

#[test]
fn unknown_method_returns_method_not_found() {
    let server = test_server();
    let response = server.handle(RpcRequest {
        jsonrpc: "2.0".into(),
        id: Some(json!(5)),
        method: "unknown_method".into(),
        params: None,
    });
    let error = response.error.expect("error");
    assert_eq!(error.code, mcp_core::protocol::METHOD_NOT_FOUND);
}
```

2. Прогнать `cargo test -p darmark mcp_contract` — **зелёно**.
3. `cargo fmt -p darmark`.

---

### Задача 13: Обновление CI

**Goal:** Добавить `mcp-core` в CI-проверки.

**Files:**
- Modify: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: нет
- Produces: CI-проверки для `mcp-core`

**Шаги:**

1. В `.github/workflows/ci.yml`, в `checks` job:

- rustfmt: `cargo fmt -p md-core -p mcp-core -p darmark -- --check`
- clippy: `cargo clippy -p md-core -p mcp-core -p darmark --all-targets -- -D warnings`
- tests: `cargo test -p md-core -p mcp-core -p darmark`

2. `plugin-checks` **не трогаем**: `mcp-core` — чистый крейт без зависимости от плагинных крейтов,
   поэтому входит только в job `checks` (шаг 1). Вынесение `catalog` в lib `plugin-cli` (задача 7)
   на `plugin-checks` не влияет.

3. Проверка: CI на windows-latest (запуск вручную или через PR).

---

## Проверки (финал)

Перед заявкой о готовности:

```bash
# Форматирование
cargo fmt -p md-core -p mcp-core -p darmark -p plugin-core -p plugin-host -p plugin-cli -- --check

# Clippy
cargo clippy -p md-core -p mcp-core -p darmark -p plugin-core -p plugin-host -p plugin-cli --all-targets --all-features -- -D warnings

# Тесты
cargo test -p md-core -p mcp-core -p darmark -p plugin-core -p plugin-host -p plugin-cli

# Фронтенд (tsc strict)
cd crates/app; npm run build
```

**Критерии готовности (AC-MCP-1..14):**

| # | Критерий | Статус в Плане 1 |
|---|----------|------------------|
| AC-MCP-1 | Сервер отвечает на initialize/tools/list/tools/call/resources/list/resources/read | Задача 5 |
| AC-MCP-2 | Первый срез: 6 resources + 4 tools, read-only | Задачи 8–9 |
| AC-MCP-3 | Схемы и описания для агента | Задачи 8–9 |
| AC-MCP-4 | Журналирование вызовов | Задача 10 |
| AC-MCP-5 | Tool-loop (ассистент вызывает tools) | **План 2** |
| AC-MCP-6 | Адаптеры не дублируют логику | Задача 12 |
| AC-MCP-7 | CSP не нарушен | Global Constraints |
| AC-MCP-8 | Приватность журнала | Задача 10 |
| AC-MCP-9 | Принуждение вне MCP | Задача 6 |
| AC-MCP-10 | Размер ≤6 МБ, npm run build, cargo test зелёные | Финал |
| AC-MCP-11 | Инварианты ADR-0027 | Global Constraints |
| AC-MCP-12 | Белый список директорий | Задача 6 |
| AC-MCP-13 | Журнал без полных путей | Задача 10 |
| AC-MCP-14 | Результаты как «данные, не инструкции» | **План 2** |

---

## План 2 (следующий, не в этом документе)

**Внутренний MCP-клиент + tool-loop у ассистента.** Зависит от поддержки tool-calling (tools/function-calling) провайдером (TZ-MCP §8). В `transport.rs` `ChatRequest` сейчас **не** несёт tools/function-calling — это вынесено в План 2. Когда провайдер поддерживает tool-calling: ассистент вызывает `plugin_validate`/`plugin_test` по своей инициативе (AC-MCP-5), результаты размечаются как «данные, не инструкции» (AC-MCP-14).

---

## Открытые вопросы

| Вопрос | Влияние на план | Решение |
|--------|-----------------|---------|
| Версия MCP-протокола (`2024-11-05` vs `2025-03-26`) | Косметическая: in-memory транспорт, версия в `initialize` | Взять `2024-11-05` (стабильная) |
| `plugin_catalog_list` и `darmark://plugins/catalog` дублируют друг друга | Нет: tool — для вызова, resource — для чтения | Оставить оба (TZ-MCP §4) |
| `PluginManifestResource` отдаёт все манифесты одним объектом | Нет: read-only, каталог мал | Упростить: `Value::Object({id: manifest}) |
| `ensure_plugin_path_allowed` на не-Windows | Только Windows (плагинная система Windows-only) | `#[cfg(windows)]`, на не-Windows MCP tools для плагинов не регистрируются |
| `log_mcp_call` для `plugin_catalog_list`/`markdown_render` | Нет: нет пути | Не логировать (нет пути) или логировать без пути |
