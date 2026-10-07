# Отчёт по Фазе 0 и ядру Фазы 1 (TZ-H2)

## 1. Гейт: факты

| Проверка | Результат |
|---|---|
| `cargo fmt -p plugin-proto -p plugin-host -- --check` | ✅ чисто |
| `cargo clippy -p plugin-proto -p plugin-host --all-targets -- -D warnings` | ✅ без замечаний |
| `cargo test -p plugin-proto` | ✅ 32 passed |
| `cargo test -p plugin-host` | ✅ 10 passed |
| `cargo tree -p darmark \| grep mlua` | ✅ пусто (D6/ADR-0021 держится) |

Код компилируется, тесты зелёные, инвариант «GUI-хост без mlua» соблюдён. Но **гейт Фазы 1 по ТЗ не закрыт** — см. §3.

## 2. Что закрыто

**Фаза 0 — полностью.** `plugin-proto` собран из перенесённых блоков спайка: `frame.rs` (5 тестов F36), `envelope.rs` (10 тестов), `job.rs` (2 теста, FFI kernel32 вручную), `quarantine.rs` (4 теста F37), `manifest.rs` (9 тестов), `notices.rs` (3 теста F38). Конверт `ToChild`/`ToHost`/`PluginError` соответствует §4.1 ТЗ, разбор fallible. CI: job `plugin-checks` + mlua-guard + size-gate.

**Фаза 1 — частично.** `sandbox.rs` (`new_with(STRING|TABLE|MATH|UTF8)` + `harden_base` с `rawget`/`rawset` по D17), `print` → `host.log`, `md.to_html`/`to_html_mapped` нативно, self-test на фикстурах `hello.lua`/`no-activate.lua`.

## 3. Незакрытые пункты гейта Фазы 1

| # | Расхождение | Где |
|---|---|---|
| **1** | **Нет stdio-цикла.** ТЗ §5 Фаза 1 требует `read_frame_host` → `ToChild::Invoke` → `ToHost::Event{done}` + исходящие `HostCall`/`Reply`. Сейчас `main.rs` — только CLI self-test; `plugin-proto::envelope` в child не подключён ни разу | `plugin-host/src/main.rs` |
| **2** | **Нет `json.encode`/`json.decode`.** ТЗ Фаза 1: «`md.*`/`json.*` нативно». `json.*` отсутствует | `plugin-host/src/bindings.rs` |
| **3** | **Нет round-trip через реальный процесс.** Тест `self_test_runs_hello_fixture` зовёт `load_and_activate()` in-process. Гейт Фазы 1 («round-trip `on_activate`») не подтверждён | `plugin-host/tests/` |

## 4. Дефекты и расхождения с архитектурой

**Критичные для границ крейтов**

- **Watchdog-константы лежат в child.** `PROGRESS_TIMEOUT_MS`/`DEADLINE_MS` — в `plugin-host/src/limits.rs` под `#[allow(dead_code)]`, с комментарием «В child не используется». Потребляет их Supervisor из `src-tauri`, а он **не имеет права** зависеть от `plugin-host` (§2 ТЗ: «`src-tauri` знает про `plugin-proto`, но **не** про `plugin-host`/mlua»). Место этих констант — `plugin-proto`. Сейчас они недостижимы для потребителя.
- **`MAX_FRAME` в комментарии `limits.rs`** обещан, но в файле его нет — константы живут в `plugin-proto/frame.rs`. Комментарий описывает несуществующий код.

**Дефекты**

- **Баг в сообщении об ошибке:** `args.next().ok_or("{arg} требует путь")` — строка-литерал, `{arg}` не подставляется, пользователь увидит `{arg} требует путь` буквально. Нужен `format!`.
- **Мёртвая зависимость** `serde_json = "1"` в `plugin-host/Cargo.toml` — не используется ни одним модулем (задел под stdio-цикл, который не подключён).
- **TOCTOU в `load_and_activate`:** `fs::metadata` → проверка размера → `fs::read_to_string`. Между проверкой и чтением файл может вырасти. Для недоверенного ввода правильнее читать в буфер и проверять его длину.
- **`is_safe_entry` содержит мёртвое условие** `entry != ".."` после `!entry.contains("..")`.
- **`ctx` создаётся дважды** (`make_context` для activate и отдельно для deactivate) — подписки/состояние из `on_activate` не доживают до `on_deactivate`, что противоречит §6.2 DESIGN_DOC («состояние живёт в хосте»).
- **`plugin_id` берётся из `file_stem`**, а не из манифеста: для `word-count/main.lua` даёт `main`. Тест это закрепляет. Для Фазы 2 (id = поле манифеста) это неверная семантика.

**Стиль и конвенции (скилл `rust-coder` требует `// SAFETY:`)**

- `job.rs`: `unsafe impl Send for Job {}` и ~10 блоков `unsafe` **без** `// SAFETY:`-обоснований.
- `job.rs`: `#![cfg(windows)]` внутри файла дублирует `#[cfg(windows)] pub mod job;` в `lib.rs`.
- `job.rs`: четыре почти идентичные функции `QueryInformationJobObject` (`peak_process_memory`/`peak_job_memory`/`stored_limits`/`total_user_time_ms`) — просится общий helper.

**Асимметрия API `envelope.rs`**

- Есть `write_to_child` + `read_from_host` (child-сторона), но **нет** парных `write_to_host` + `read_from_child` для host-стороны. Supervisor Фазы 2 упрётся в это. Имена к тому же путают (`read_from_host` читает то, что прислал host, а не «читает host»).
- Два способа логирования (`ToHost::Log` и `Event{kind:"log"}`) — ТЗ допускает, но нужен один канонический, иначе рассинхрон в Фазе 2.

**Не зафиксировано документально**

- Судьба `lua-proto`/`lua-rpc-spike` в `members`: ТЗ Фаза 0 требует «решение фиксируется в Фазе 0». Есть только комментарий в `Cargo.toml`; `[profile.proto]` живёт ради них.
- `KODA.md` и `AGENTS.md` не знают про `plugin-proto`/`plugin-host` — память проекта устарела.
- CI не собирает child в release (size-budget child — Фаза 6) и не проверяет `panic="abort"` в нём.

## 5. Предлагаемый порядок рефакторинга

1. **Границы:** перенести `PROGRESS_TIMEOUT_MS`/`DEADLINE_MS` в `plugin-proto` (например, `limits.rs` рядом с `frame.rs`); в `plugin-host/src/limits.rs` оставить только лимит `.lua` либо удалить модуль целиком.
2. **Долг Фазы 1:** подключить stdio-цикл в `main.rs` (`read_frame_host` → dispatch `load`/`activate`/`event` → `ToHost::Event{done}`; `HostCall` → ожидание `Reply`); реализовать `json.*`; добавить интеграционный тест round-trip через запуск бинарника.
3. **API `envelope`:** добавить симметричные `write_to_host`/`read_from_child`, унифицировать логирование.
4. **Мелкие баги:** `format!("{arg} …")`, TOCTOU (читать → проверять), мёртвое условие в `is_safe_entry`, убрать `serde_json` из `plugin-host` до подключения stdio.
5. **Гигиена `job.rs`:** `// SAFETY:`-комментарии, снять дубль `cfg(windows)`, свести четыре query-функции к одной.
6. **Документация:** зафиксировать решение по прототипным крейтам, обновить `KODA.md`/`AGENTS.md`.

## 6. Вопросы, требующие вашего решения

1. **Объём рефакторинга:** только §4 (дефекты/границы) или плюс §3 (дотянуть Фазу 1 до конца — stdio-цикл и `json.*`)? Второе — это уже не рефакторинг, а закрытие фазы.
2. **Прототипные крейты:** убираем `lua-proto`/`lua-rpc-spike` из `members` сейчас (перенос блоков завершён) или держим до Фазы 2 как источник фикстур `crash`/`hang`/`chatty`/`edit`?
3. **Где держать константы watchdog** — согласны на `plugin-proto` (единственный крейт, видимый и хосту, и child)?
