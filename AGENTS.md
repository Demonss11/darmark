# AGENTS.md

`darmark` (продуктовое название `ПЕНАЛ`) — Markdown-редактор/вьюер в духе Notepad++. Tauri 2 + vanilla TS/Vite + Rust-ядро. Платформа — Windows. Документация, комментарии и UI-строки — **на русском**.

## Структура и границы

- Крейты: `md-core` (ядро markdown→HTML), `app/src` (фронтенд), `app/src-tauri` (Tauri-шелл), `plugin-core`/`plugin-host`/`plugin-cli`/`plugin-catalog` (плагинная система) — границы видны из `Cargo.toml`.

### Модули шелла (`crates/app/src-tauri/src`)

Карта, на которую отсылает `DESIGN_DOC.md` §5.5. Конвенции — ADR-0039.

| Модуль | Содержимое |
|---|---|
| `lib.rs` | только сборка: `mod`-декларации, `pub use` наружу, `run()`, `generate_handler!` |
| `contract.rs` | контрактные типы IPC: `PluginViewInfo`, `PluginStatus`, `CommandInfo`, `PluginInfo` |
| `fs.rs` | `MAX_FILE_SIZE`, чтение/запись, asset-scope, имя файла для журнала |
| `commands/documents.rs` | 7 команд документов + `lock_store` |
| `commands/plugins.rs` | 3 команды менеджера плагинов + `open_devtools` |
| `setup/plugins.rs` | `PluginState`, sinks плагинной подсистемы, `setup_plugins` |
| `setup/env.rs` | каталог настроек, отключение WebView2-акселераторов |
| `dev_plugin.rs` | dev-режим `--debug-plugin` |
| `state/` | владение документами: `types` / `store` / `history` / `tests` |
| `ai/` | ассистент: `commands`, `doc_harness`, `plugin_harness`, `history`, `skills`, `settings`, `backend` |
| `mcp/` | MCP-сервер: `adapters` (tools/plugin_read/privacy/journal/tests), `resources` |
| `plugins/` | плагинная подсистема: `manager`, `host`, `supervisor`, `services`, `views`, `scan`, `bus`, настройки |

Правила раскладки, порог 400 строк, видимость Tauri-команд и раскладка тестов — ADR-0039. Правки файлов с кириллицей — файловыми инструментами или Python с `encoding="utf-8"` (PowerShell 5.1 портит кириллицу).

## Платформа

Продукт — **только Windows**. Не-Windows ветки не поддерживаются и не тестируются: CI собирает только `windows-latest`.

Windows-специфичный код (Job Object, WinAPI, SChannel, WebView2) пишется безусловно; `#[cfg(windows)]`-гейты не делают крейт кроссплатформенным. Зависимости Windows остаются в `[target.'cfg(windows)'.dependencies]` — декларация платформы.

## Команды

Из корня, если не указано иное:

| Задача | Команда |
| --- | --- |
| Линт Rust | `cargo clippy -p md-core -p mcp-core -p plugin-catalog -p plugin-core -p plugin-host -p plugin-cli -p darmark --all-targets -- -D warnings` |
| Линт фронта | `cd crates/app; npm run lint` |
| Тесты | `cargo test -p md-core -p mcp-core -p plugin-core -p plugin-catalog -p plugin-host -p plugin-cli -p darmark` |
| Фронт (tsc strict) | `cd crates/app; npm run build` |
| GUI | `cd crates/app; npx tauri dev` |
| Релиз NSIS | `cd crates/app; npx tauri build` (size-gate ≤6 МБ) |
| Чистка Tauri-артефактов | `cd crates/app; npm run clean:tauri` |
| E2E | `cd crates/app; npm run test:e2e` |
| Guard: `mlua` вне GUI | `cargo tree -p darmark \| Select-String mlua` (пусто; ADR-0021) |

Предусловие: сайдкары в `crates/app/src-tauri/binaries/` — после `npm run clean:tauri` требуется `npm run sidecars`.

### Сборка приложения

```powershell
# перед сборкой — снять процессы E2E, они держат exe
cd crates/app; npm run processes

# релиз + NSIS
cd crates/app; npx tauri build
```

Бинарник для E2E собирается **только** через `cd crates/app; npm run tauri build` (внутри: фронт → sidecars → Rust-релиз → NSIS). `cargo build -p darmark --release` для сборки приложения не использовать — без `tauri build` сайдкары и ассеты не попадают в бинарник.

Установщик: `target/release/bundle/nsis/`, size-gate ≤6 МБ.

## Жёсткие ограничения

- `innerHTML` — только через санитизированные хелперы (ADR-0038, DESIGN_DOC §11.1).
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
| `docs/adr/0021-plugin-isolation-process.md`, `docs/adr/0023-plugin-runtime.md`, `docs/adr/0026-local-plugin-catalog.md`, `docs/adr/0027-host-api-version-policy.md` — плагинная система: изоляция, runtime, каталог, версии host-API |
