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

## Команды

Из корня, если не указано иное:

| Задача | Команда |
| --- | --- |
| Тесты ядра | `cargo test -p md-core` |
| Тесты плагинных крейтов | `cargo test -p plugin-core -p plugin-catalog -p plugin-host -p plugin-cli` |
| Шелл: сборка/тесты | `cargo check -p darmark` / `cargo test -p darmark` |
| Фронт (tsc strict) | `cd crates/app; npm run build` |
| GUI | `cd crates/app; npx tauri dev` |
| Релиз NSIS | `cd crates/app; npx tauri build` (size-gate ≤6 МБ) |
| E2E | `cd crates/app; npm run test:e2e` |
| Guard: `mlua` вне GUI | `cargo tree -p darmark \| Select-String mlua` (пусто; ADR-0021) |

Предусловие: сайдкары в `crates/app/src-tauri/binaries/` (иначе `tauri-build` падает).

## Жёсткие ограничения

- `innerHTML` — только через санитизированные хелперы (ADR-0038, DESIGN_DOC §11.1). Четвёртая точка запрещена.
- Юнит-тестов фронта нет (`vitest` нет) — проверка = `npm run build`.
- E2E не покрывает нативные диалоги/ОС — помечай сценарий `@manual`.
- E2E-сценарий > 5 с — подозрительно: либо сценарий тяжёлый, есть проблема.
- Тяжёлые зависимости — только по согласованию.

## Конвенции

- Комментарии на русском, объясняют **зачем**.
- Не коммить/пушить без запроса. Ветку коммитит исполнитель; в `main` не мержи сам.

## Ссылки

| Документ |
| --- |
| `DESIGN.md` — визуальная идентичность |
| `docs/DESIGN_DOC.md` — архитектура, контракты, ADR |
| `docs/adr/` — решения |
| `docs/BACKLOG.md`, `docs/FINDINGS.md` — бэклог, замеры |
| `docs/PLUGIN_API.md`, `docs/PLUGIN_GUIDE.md` — API плагинов |
