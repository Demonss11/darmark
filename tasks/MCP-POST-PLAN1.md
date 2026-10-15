# Задача: MCP-сервер — доработка после Плана 1

> **Статус:** Не реализовано
> **Приоритет:** P2
> **Источник:** `docs/FINDINGS.md` §2.5 (открытые оговорки)
> **Связанные:** ADR-0036 (MCP-сервер), ADR-0021 (песочница), ADR-0026 (каталог плагинов)

---

## 1. Цель

Закрыть открытые оговорки MCP-сервера (План 1), чтобы удалить `docs/FINDINGS.md` §2.5 как полностью обработанный.

---

## 2. Задачи

### 2.1 HIGH: `create_link` — небезопасная передача путей в `cmd /c mklink`

**Проблема:** `Command::new("cmd").arg("/c").arg("mklink")…` идёт через `cmd.exe`, который разбирает аргументы как командную строку. Символы `&`, `|`, `^`, `%` в пути к каталогу плагина интерпретируются как операторы оболочки. Имя вида `x&calc.exe` — валидный `Component::Normal`, проходит `is_safe_relative`.

**Исправление:**
- Перейти на `CommandExt::raw_arg` (убирает передачу через командную строку целиком).

**Тест:** `create_link_rejects_shell_metacharacters_in_source`

### 2.2 `plugin_test` через MCP — прогон песочницы тестом

**Проблема:** Прогон песочницы через MCP проверяется только схемой, не  Rust-тестом.

**Задача:** Добавить Rust-тест, который вызывает `plugin_test` через MCP и проверяет, что Lua исполняется в host-песочнице.

**Тест:** `mcp_plugin_test_executes_lua_in_sandbox`

### 2.3 `app_log_sink` (prod-only) — покрыть тестом

**Проблема:** Конверт `log-entry {level,category,message}` проверяется лишь до границы стока; MCP-записи идут уровнем `info` — наблюдаемость R2 зависит от фильтра журнала.

**Задача:** Добавить тест на конверт `log-entry` для MCP-записей.

**Тests:** `mcp_log_entry_has_valid_level_category_message`

### 2.4 `catalog_repo()` зависит من CWD

**Проблема:** В упакованном приложении каталога может не быть → tool/resource вернут ошибку.

**Задача:** Заменить CWD на абсолютный путь через `SettingsStore::data_dir()` (единый каталог данных приложения, Вариант B).

### 2.5 JSON-RPC-упрощения

**Проблема:**
- Поле `jsonrpc` не валидируется.
- Notification получает ответ с `id: null`.
- `"arguments": null` не нормализуется в `{}`.

**Задача:** Добавить валидацию поля `jsonrpc`, пропуск ответа для notification, нормализацию `arguments`.

### 2.6 Гигиена/мелочи

- `manifest_errors` стало `#[cfg(test)]` — вернуть в прод-код.
- `sanitize_text` — prefix-replace без границ пути.
- Тесты мутируют глобальные env без лока.
- Дублирование `ToolError`/`ResourceError` — объединить или обосновать.

---

## 3. Не в scope (осознанные решения)

- **Белый список путей:** разрешён весь системный temp (TZ-MCP R3) — сужение сломало бы валидацию.
- **Шаблонный ресурс `darkmark://plugins/{id}/manifest`:** читается через `resources/read`; отдельный `resources/templates/list` не вводится.
- **Tool-loop ассистента (План 2):** в `tasks/MCP-TOOLLOOP.md`.

---

## 4. Критерии готовности

| AC | Критерий |
|----|----------|
| 1 | `create_link` использует `raw_arg`, тест на метасимволы зелёный |
| 2 | Rust-тест `plugin_test` через MCP зелёный |
| 3 | Тест на конверт `log-entry` зелёный |
| 4 | `catalog_repo()` не зависит от CWD |
| 5 | JSON-RPC: `jsonrpc` валидируется, notification без ответа, `arguments: null` → `{}` |
| 6 | Гигиена: `manifest_errors` в проде, `sanitize_text` с границами, дублирование ошибок устранено |
| 7 | `cargo test -p darmark` зелёный |
| 8 | `cargo clippy -p darmark -- -D warnings` чисто |

---

## 5. Проверки перед сдачей

- `cargo test -p darmark` — зелёный
- `cargo clippy -p darmark -- -D warnings` — чисто
- `cargo fmt --all -- --check` — чисто
