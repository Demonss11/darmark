# AGENTS.md

`darmark` (продуктовое название `ПЕНАЛ`) — Markdown-редактор/вьюер в духе Notepad++. Tauri 2 + vanilla TS/Vite + Rust-ядро. Платформа — Windows. Документация, комментарии и UI-строки — **на русском**.

## Крейты

| Крейт | Роль |
|---|---|
| `md-core` | ядро markdown→HTML + санитайзер |
| `app/src` | фронтенд (TS/Vite) |
| `app/src-tauri` | Tauri-шелл (Rust): документы, плагины, AI, MCP |
| `plugin-core` / `plugin-host` / `plugin-cli` / `plugin-catalog` | плагинная система |

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
| Guard: `mlua` вне GUI | `cargo tree -p darmark \| Select-String mlua` (пусто) |

Предусловие: сайдкары в `crates/app/src-tauri/binaries/` — после `npm run clean:tauri` требуется `npm run sidecars`.

## Сборка

Перед сборкой снять процессы E2E — они держат exe: `cd crates/app; npm run processes`.
Бинарник собирается **только** через `cd crates/app; npx tauri build` (фронт → sidecars → Rust-релиз → NSIS); `cargo build -p darmark --release` для приложения не использовать — сайдкары и ассеты не попадут в бинарник. Установщик: `target/release/bundle/nsis/`, size-gate ≤6 МБ.

## Жёсткие ограничения

- `innerHTML` — только через санитизированные хелперы (`setSanitizedHtml`); точек `innerHTML` на фронте — не больше трёх.
- Плагинный рантайм — `mlua` + Lua 5.5 в отдельном child-процессе; LuaJIT запрещён. GUI-хост без `mlua`.
- Плагины stateless по коду; состояние хранит хост. Перезагрузка — `unload` → `load` с диска.
- Юнит-тестов фронта нет (`vitest` нет) — проверка = `npm run build`.
- E2E не покрывает нативные диалоги/ОС — помечай сценарий `@manual`.
- E2E-сценарий > 5 с — подозрительно: либо сценарий тяжёлый, есть проблема.
- Тяжёлые зависимости — только по согласованию.
- Только Windows: CI — `windows-latest`; Windows-код (Job Object, WinAPI, SChannel, WebView2) пишется безусловно, зависимости — в `[target.'cfg(windows)'.dependencies]`.

## Конвенции

- Не коммить/пушить без запроса. Ветку коммитит исполнитель; в `main` не мержи сам.
- Правки файлов с кириллицей — файловыми инструментами или Python с `encoding="utf-8"` (PowerShell 5.1 портит кириллицу).

## Ссылки

| Документ | Что описывает |
|---|---|
| `DESIGN.md` | визуальная идентичность |
| Скилл `plugin-sandbox` | механика песочницы, permissions, watchdog, карантин |
| Скилл `md-core-domain` | ядро markdown→HTML, санитайзер, mapped-варианты |
| Скилл `tauri-ipc-conventions` | контракт фронт↔Rust, safeHtml, IPC |
| `crates/plugin-core` | манифест, envelope, лимиты, карантин |
| `crates/plugin-cli/templates/plugin` | гайд и примеры плагина |
