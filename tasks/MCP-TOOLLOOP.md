# Задача: Tool-loop MCP — встроенный ассистент вызывает инструменты (ADR-0036)

> **Статус:** Не реализовано (проверено 14.10.2026 через explore-агент)
> **Приоритет:** P2
> **Источник:** `docs/adr/0036-mcp-server-inmemory.md` (нормативное), `tasks/TZ-MCP.md` (AC-MCP-1…14)
> **Epic:** 6 из плана v2 (задачи 6.1–6.5)
> **Связанные:** ADR-0035 (провайдеры), ADR-0038 (санитайзер), DESIGN_DOC §11/§13.2

---

## 1. Цель

Дать встроенному AI-ассистенту возможность **вызывать инструменты** (tool-calling) в цикле
итераций через **in-memory MCP-сервер** (ADR-0036). Ассистент не просто отдаёт текст, а сам
вызывает `plugin_validate`/`plugin_test`/`markdown_render`/`plugin_catalog_list` — наблюдаемо,
с контрактом для агента.

**Главное — не сам сервер, а результат:** у агента появляется чёткий перечень команд и
ресурсов с схемами, каждый вызов логируется в журнал, результаты размечаются как «данные, не
инструкции».

---

## 2. Ключевые инварианты (норматив)

| # | Инвариант | Источник |
|---|-----------|----------|
| 1 | **In-memory транспорт:** JSON-RPC 2.0 + MCP поверх внутрипроцессного канала; ноль OS-поверхности (нет порта, child, бинарника) | ADR-0036 §2.1 |
| 2 | **Только внутренний клиент:** внешних клиентов, inbound-auth, аудита нет; outbound — вне scope | ADR-0036 §2 |
| 3 | **MCP — контракт, не принуждение:** права/rev-guard/безопасность путей остаются в существующих командах | ADR-0036 §2.2 |
| 4 | **Тонкие адаптеры:** tool = обёртка над командой/CLI, никакой бизнес-логики в MCP | ADR-0036 §2.2 |
| 5 | **Первый срез:** 6 resources + 4 tools, read-only, без мутаций | ADR-0036 §3 |
| 6 | **Наблюдаемость:** каждый вызов и результат в журнале (`#logs-panel`), без секретов, пути → только имена | TZ-MCP R2 |
| 7 | **Принуждение вне MCP:** `path` разрешён только в `%APPDATA%/darmark/plugins/` и temp; проверка в `plugin_harness` | TZ-MCP R3 |
| 8 | **R7 — «данные, не инструкции»:** результаты инструментов размечаются; поля манифеста — как враждебный текст | TZ-MCP R7 |
| 9 | **Tool-loop ограничен:** `MAX_TOOL_ITERATIONS = 4`, `MAX_TOOL_RESULT_CHARS = 64 КБ` | ADR-0036 |
| 10 | **Три точки `innerHTML`:** карточки вызовов — только `textContent` (ADR-0038 §2.3) | ADR-0038 |
| 11 | **Замороженные контракты:** `#ai-*`/`#logs-*`, §13.2, `HOST_API_VERSION` | DESIGN_DOC §13.2 |

---

## 3. Первый срез: инвентарь

### 3.1 Resources (read-only)

| URI | Что отдаёт | Источник |
|-----|------------|----------|
| `darmark://contracts/plugin-manifest` | схема `plugin.json` | `plugin-core::manifest`, PLUGIN_API.md §1 |
| `darmark://contracts/host-api` | контракт `host.*`/`md.*`/`json.*` | PLUGIN_API.md §3 |
| `darmark://contracts/events` | каталог событий | PLUGIN_API.md §4 |
| `darmark://contracts/markdown` | контракт рендера/санитайзера | `md-core` |
| `darmark://plugins/catalog` | индекс каталога | `examples/plugins/index.json` |
| `darmark://plugins/{id}/manifest` | манифест плагина | каталог |

### 3.2 Tools

| Tool | Оборачивает | Мутация | Примечание |
|------|-------------|---------|------------|
| `plugin_validate(path)` | `plugin-cli test <dir> --json` | нет | `path` → безопасность путей (R3) |
| `plugin_test(path)` | `plugin-cli test` (прогон в песочнице) | нет | исполняет Lua в host-песочнице; только через `plugin-cli` (ADR-0021) |
| `plugin_catalog_list()` | `plugin-cli list --json` | нет | |
| `markdown_render(text, mapped?)` | `md-core::to_html(_mapped)` | нет | |

---

## 3.3 Деградация: провайдер без поддержки tool-calling

> **Решение принято:** `docs/adr/0040-ai-tools-capability-degradation.md` (ADR-0040, дополнение
> к ADR-0036). Ниже — итоговая формулировка: сценарий (б) приведён в соответствие с кодом,
> Q1–Q4 закрыты.

Если MCP-сервер включён, но провайдер не поддерживает `tools` (OpenAI-совместимый шлюз,
кастомный эндпоинт), tool-loop не должен ломать чат — ассистент деградирует до обычного ответа.

**Сценарии:**

| # | Поведение провайдера | Решение |
|---|----------------------|---------|
| а | Отвечает 400 на поле `tools` | Детект по HTTP-статусу → **один** авто-повтор без `tools` в том же `run` (одна пауза rate limit, один `message_id`); вердикт кэшируется по `(base_url, model)`; тихий `warn` на stderr |
| б | Молча игнорирует `tools` (200, но `tool_calls` пустые) | **Повтор не выполняется:** пустые `tool_calls` — штатное конечное состояние цикла, возвращается обычный финальный ответ. Повтор продублировал бы уже отправленные дельты `ai:chunk` и срабатывал бы на каждом нормальном ходе |
| в | Возвращает `tool_calls` в нестандартной форме | Не парсить; считать что `tool_calls` пустые → тот же обычный ответ (реализовано: `ai/tool_calls.rs`) |

**Ответы на открытые вопросы (ADR-0040):**

1. Повтор без `tools` — **тихий `warn`** на stderr (`eprintln!`, префикс `darmark: ai:`), только
   метаданные (хост, модель). **Карточка в чате не вводится** (§2.6).
2. **Кэшировать — да**, в памяти `AiState` по `(base_url, model)`; ставится после успешного
   повтора, очищается при `ai_set_settings`, в `config.json` не персистится (§2.5).
3. **Повтор при 400 с непустым `tools`, если код не `AiContextLength` и остался свободный слот
   итерации** (на последнем слоте повтор не начинается — ошибка доходит до пользователя как есть;
   ADR-0040 §2.2 п.6).
   Разбор тела на слово «tools» не вводится: текст отказа не стандартизован, а промах этой
   эвристики означает вечную поломку чата; ложное срабатывание стоит одного лишнего запроса
   (§2.3).
4. **Локальный разбор вызова из текста ответа — не вводим** (R7 «данные, не инструкции»,
   prompt-injection). При появлении кейса — отдельный ADR (§2.8 п.37).

**Критерий успеха:** поведение для (а)/(б)/(в) зафиксировано ADR-0040 и закрыто тестами
`ai/tests/`; чат не ломается у провайдера без `tools`.

---

## 4. Типы Rust

```rust
// ai/tool_calls.rs
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

pub struct ToolCallAccumulator { /* … */ }
impl ToolCallAccumulator {
    pub fn push(&mut self, index: u32, id: Option<String>, name: Option<String>, arguments: Option<String>);
    pub fn finish(&self) -> Vec<ToolCall>;
}

pub fn tool_calls_from_payload(value: &serde_json::Value) -> Vec<ToolCall>;

// ai/tool_loop.rs
pub const MAX_TOOL_ITERATIONS: usize = 4;
pub const MAX_TOOL_RESULT_CHARS: usize = 64 * 1024;

pub struct ToolResult {
    pub ok: bool,
    pub text: String,
}

pub fn tool_specs_from_tools_list(tools: &serde_json::Value) -> Vec<ToolSpec>;
pub fn call_mcp_tool(server: &McpServer, name: &str, arguments: serde_json::Value) -> Result<ToolResult, CommandError>;
pub fn tool_message_content(tool: &str, text: &str) -> String; // R7: «данные, не инструкции»

pub async fn run<F, S>(
    transport: &dyn AiTransport,
    mcp: &McpServer,
    base: ChatRequest,
    on_delta: &mut (dyn FnMut(String) + Send),
    on_tool_call: &mut F,
    on_tool_result: &mut S,
) -> Result<AiResponse, CommandError>
where F: FnMut(&ToolCall), S: FnMut(&ToolResult);
```

---

## 5. Задачи (TDD: красный → зелёный → коммит)

Все задачи идут строго последовательно.

### 6.1 Разбор `tool_calls` в SSE и не-стриминг-конверте
- `ai/tool_calls.rs`: `ToolCall`, `ToolCallAccumulator`, `tool_calls_from_payload`
- `ai/sse.rs`: `SseEvent::ToolCallDelta { index, id, name, arguments }`
- `ai/mod.rs`: `pub mod tool_calls;`
- Тесты: `sse_parses_tool_call_delta_fragments`, `accumulator_merges_fragments_by_index`, `tool_calls_from_message_envelope`

### 6.2 Передача `tools` и сборка tool-calls в бэкенде
- `ai/transport.rs`: `ChatOutcome.tool_calls`
- `ai/backend/reqwest.rs`: `build_chat_body` + блок `tools`, `ToolCallAccumulator` в цикле SSE
- Тесты: `chat_body_includes_tools_when_present`, `chat_body_never_overrides_tools_with_extra_body`

### 6.3 Модуль `ai/tool_loop.rs`
- `ai/tool_loop.rs`: `tool_specs_from_tools_list`, `call_mcp_tool`, `tool_message_content`, `run`
- `ai/types.rs`: `AiMessage` + опциональные `name`, `tool_call_id`, `tool_calls`
- `ai/mod.rs`: `pub mod tool_loop;`
- Тесты: `tool_specs_from_tools_list_maps_mcp_registry`, `tool_message_content_marks_data_not_instructions`, `tool_loop_stops_at_iteration_limit`, `tool_loop_executes_tool_and_returns_final_answer`, `tool_loop_isolates_tool_failure`

### 6.4 Подключение tool-loop к `ai_chat` + эмиты
- `ai/commands.rs`: `ai_chat` → `tool_loop::run` при непустом реестре MCP
- `lib.rs`: `app.state::<McpServer>()`
- Новые аддитивные события: `ai:tool-call { message_id, call_id, tool, arguments }`, `ai:tool-result { message_id, call_id, tool, ok, duration_ms, summary }`
- `rate limit`: одна пауза на весь цикл
- Тест: `chat_emits_tool_lifecycle_events`

### 6.5 Карточки вызовов инструментов в UI + E2E
- `tauri.ts`: типы payload'ов событий
- `aiPanel.ts`: подписки, карточки «⚙ <tool> …» и «✓/✗ <tool> (12 мс)» (только `textContent`)
- `ai.steps.js`: шаг «я поднимаю мок-провайдера AI с вызовом инструмента»
- `ai.feature`: сценарий «Ассистент сам вызывает инструмент MCP»

---

## 6. Критерии готовности

| AC | Критерий | Задача |
|----|----------|--------|
| AC-MCP-1 | MCP-сервер отвечает на `initialize`/`tools/list`/`tools/call`/`resources/list`/`resources/read` | 6.3 |
| AC-MCP-2 | Первый срез: 6 resources + 4 tools, read-only | 6.3 |
| AC-MCP-3 | Схемы и описания у каждого tool/resource | 6.3 |
| AC-MCP-4 | Вызовы и результаты в журнале | 6.4, 6.5 |
| AC-MCP-5 | Tool-loop: ассистент сам вызывает инструменты | 6.1–6.5 |
| AC-MCP-6 | Адаптеры не дублируют логику | контракт-тест `mcp_contract` |
| AC-MCP-7 | CSP не нарушен | все |
| AC-MCP-8 | Приватность журнала | 6.4, 6.5 |
| AC-MCP-9 | Принуждение не в MCP | 6.3 |
| AC-MCP-10 | ≤6 МБ; `npm run build` и `cargo test -p darmark` зелёные | все |
| AC-MCP-11 | Аддитивность контрактов | 6.4 |
| AC-MCP-12 | Белый список директорий, path traversal → ошибка | в `plugin_harness` (существующий) |
| AC-MCP-13 | Журнал без путей, секретов, содержимого | 6.4, 6.5 |
| AC-MCP-14 | «Данные, не инструкции» | 6.3, 6.5 |

---

## 7. Проверки перед сдачей

- `cargo test -p darmark` — зелёный
- `cd crates/app; npm run build` — зелёный (tsc strict)
- `cargo clippy -p darmark -- -D warnings` — чисто
- `cd crates/app; npm run test:e2e` — зелёный
- `cargo tree -p darmark | Select-String mlua` — пусто (ADR-0021)
