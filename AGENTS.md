# AGENTS.md

`darmark` (продуктовое название `ПЕНАЛ`) — Markdown-редактор/вьюер в духе Notepad++. Tauri 2 + vanilla TS/Vite + Rust-ядро. Платформа — Windows. Документация, комментарии и UI-строки — **на русском**.

## Структура и границы

- `crates/md-core/` — чистое ядро markdown→HTML (без Tauri/GUI).
- `crates/app/src/` — фронтенд (vanilla TS).
- `crates/app/src-tauri/` — тонкий Tauri-шелл (`darmark`/`darmark_lib`).
- `crates/plugin-core/` — ядро плагинной системы (без `mlua`/Tauri).
- `crates/plugin-host/` — child `darmark-plugin-host` (Lua 5.5, `mlua`).
- `crates/plugin-cli/` — CLI `darmark-plugin`.
- `crates/plugin-catalog/` — каталог плагинов (без `mlua`/Tauri).

### Модули шелла (`crates/app/src-tauri/src`)

Карта, на которую отсылает `DESIGN_DOC.md` §5.5. Конвенции — ADR-0039.

| Модуль | Содержимое |
|---|---|
| `lib.rs` | только сборка: `mod`-декларации, `pub use` наружу, `run()`, `generate_handler!` |
| `contract.rs` | контрактные типы IPC: `PluginViewInfo`, `PluginStatus`, `CommandInfo`, `PluginInfo` |
| `fs.rs` | `MAX_FILE_SIZE`, чтение/запись, asset-scope, имя файла для журнала |
| `commands/documents.rs` | 11 команд документов + `lock_store` |
| `commands/plugins.rs` | 9 команд менеджера плагинов + `open_devtools` |
| `setup/plugins.rs` | `PluginState`, sinks плагинной подсистемы, `setup_plugins` |
| `setup/env.rs` | каталог настроек, отключение WebView2-акселераторов |
| `dev_plugin.rs` | dev-режим `--debug-plugin` |
| `state/` | владение документами: `types` / `store` / `history` / `tests` |
| `ai/` | ассистент: `commands`, `doc_harness`, `plugin_harness`, `history`, `skills`, `settings`, `backend` |
| `mcp/` | MCP-сервер: `adapters` (tools/plugin_read/privacy/journal/tests), `resources` |
| `plugins/` | плагинная подсистема: `manager`, `host`, `supervisor`, `services`, `views`, `scan`, `bus`, настройки |

Правила раскладки:

- **Файл > 400 строк — повод разбить по домену.** Исключения обосновываются в ADR.
- **Структура с полями объявляется в модуле-родителе, `impl`-блоки — по подмодулям.** Приватность поля не распространяется на соседей, поэтому объявление обязано быть выше всех, кто это поле читает (`PluginHost` → `host/mod.rs`, `DocumentStore` → `state/mod.rs`).
- **Команда Tauri вне корня крейта обязана быть `pub(crate) fn` или `pub fn`.** На `fn` макрос `#[tauri::command]` не получает `#[macro_export]`, и `generate_handler!` из `lib.rs` падает с `E0603`. При переносе команды реэкспортируй и сами макросы `__cmd__*`, иначе `E0433`.
- **Тесты домена — отдельным деревом** (`plugins/tests/`, `ai/plugin_harness/tests/`), не внутрь модулей-владельцев. Приватные детали для тестов расширяй до `pub(super)`, не до `pub(crate)`.
- **Правки файлов с кириллицей — файловыми инструментами или через Python с `encoding="utf-8"`.** PowerShell 5.1 читает/пишет в CP1251 и портит кириллицу необратимо.

## Платформа

Продукт — **только Windows**. Не-Windows ветки не поддерживаются и не тестируются: CI собирает только `windows-latest`.

Атрибуты `#[cfg(windows)]` / `#[cfg(not(windows))]` в коде не ставятся. Windows-специфичный код (Job Object, WinAPI, SChannel, WebView2) пишется безусловно. Гейт над модулем не делает крейт кроссплатформенным — он лишь создаёт иллюзию: потребители Windows-модулей используются без гейта, поэтому крейт на не-Windows и так не собирается. Зависимости Windows при этом остаются в `[target.'cfg(windows)'.dependencies]` — это декларация платформы, а не мёртвый код.

## Команды

Из корня, если не указано иное:

| Задача | Команда |
| --- | --- |
| Формат Rust | `cargo fmt --all -- --check` |
| Линт Rust | `cargo clippy -p md-core -p mcp-core -p plugin-catalog -p plugin-core -p plugin-host -p plugin-cli -p darmark --all-targets -- -D warnings` |
| Линт фронта | `cd crates/app; npm run lint` |
| Мёртвый код фронта | `cd crates/app; npm run knip` |
| Аудит зависимостей | `cargo audit` (RUSTSEC; джоб `audit` в CI) |
| Неиспользуемые зависимости | `cargo machete` |
| Тесты ядра | `cargo test -p md-core` |
| Тесты MCP-ядра | `cargo test -p mcp-core` |
| Тесты плагинных крейтов | `cargo test -p plugin-core -p plugin-catalog -p plugin-host -p plugin-cli` |
| Шелл: сборка/тесты | `cargo check -p darmark` / `cargo test -p darmark` |
| Фронт (tsc strict) | `cd crates/app; npm run build` |
| GUI | `cd crates/app; npx tauri dev` |
| Релиз NSIS | `cd crates/app; npx tauri build` (size-gate ≤6 МБ) |
| Чистка Tauri-артефактов | `cd crates/app; npm run clean:tauri` (удаляет `src-tauri/gen`, `src-tauri/binaries`, `dist`; `target/` не трогает) |
| E2E | `cd crates/app; npm run test:e2e` |
| Guard: `mlua` вне GUI | `cargo tree -p darmark \| Select-String mlua` (пусто; ADR-0021) |

Предусловие: сайдкары в `crates/app/src-tauri/binaries/` (иначе `tauri-build` падает). `npm run clean:tauri` их удаляет — после него `cargo check -p darmark` требует `npm run sidecars`.

### Сборка приложения

```powershell
# перед сборкой — снять процессы E2E, они держат exe
taskkill /IM darmark.exe /F /T; taskkill /IM darmark-plugin-host.exe /F /T; taskkill /IM tauri-driver.exe /F /T; taskkill /IM msedgedriver.exe /F /T
Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -like "*wdio*" } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force }

# релиз + NSIS
cd crates/app; npx tauri build
```

Бинарник для E2E собирается **только** через `cd crates/app; npm run tauri build` (внутри: фронт → sidecars → Rust-релиз → NSIS). Команду `cargo build -p darmark --release` для сборки приложения использовать нельзя — без шага `tauri build` сайдкары и ассеты не попадают в бинарник, и E2E падает по инфраструктурным причинам, а не функциональным.

Установщик: `target/release/bundle/nsis/`, size-gate ≤6 МБ.
Отмена `npm run test:e2e` оставляет живое дерево процессов — снимать `taskkill` выше.

## Жёсткие ограничения

- `innerHTML` — только через санитизированные хелперы (ADR-0038, DESIGN_DOC §11.1). Четвёртая точка запрещена.
- Юнит-тестов фронта нет (`vitest` нет) — проверка = `npm run build`.
- E2E не покрывает нативные диалоги/ОС — помечай сценарий `@manual`.
- E2E-сценарий > 5 с — подозрительно: либо сценарий тяжёлый, есть проблема.
- Тяжёлые зависимости — только по согласованию.

## Конвенции

- Комментарии — **только doc-комментарии** `///` (к элементу) и `//!` (к модулю), на русском, объясняют **зачем**. Обычных `//` в коде нет: пересказ кода, подписи фаз и разделители вычищены (ADR-0039 §5). Doc-комментарий не дублирует сигнатуру — он несёт контракт, инвариант или ссылку на ADR.
- Не коммить/пушить без запроса. Ветку коммитит исполнитель; в `main` не мержи сам.

## Ссылки

| Документ |
| --- |
| `DESIGN.md` — визуальная идентичность |
| `docs/DESIGN_DOC.md` — архитектура, контракты, ADR |
| `docs/adr/` — решения |
| `docs/adr/0039-shell-module-map-and-test-layout.md` — карта модулей шелла, порог 400 строк, раскладка тестов, видимость Tauri-команд |
| `docs/FINDINGS.md` — замеры |
| `docs/PLUGIN_GUIDE.md` — гайд по плагинам |
| `docs/adr/0021-plugin-isolation-process.md`, `docs/adr/0023-plugin-runtime.md`, `docs/adr/0026-local-plugin-catalog.md`, `docs/adr/0027-host-api-version-policy.md` — плагинная система: изоляция, runtime, каталог, версии host-API |
| `docs/adr/0042-plugin-host-api-norm-carriers.md` — носители нормы host-API (ADR + код + промты скиллов) |
