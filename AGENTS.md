# AGENTS.md

`darmark` — лёгкий Markdown-редактор/вьюер в духе Notepad++: слева редактор, справа HTML-предпросмотр.
Tauri 2 + vanilla TypeScript/Vite + чистое Rust-ядро Markdown. Целевая платформа — Windows.
Документация, комментарии и UI-строки — **на русском**.

## Структура и границы

- `crates/md-core/` — чистое ядро markdown→HTML (pulldown-cmark 0.13), **без Tauri/GUI**; XSS-санитайзер, YAML-шапка, mapping байты↔UTF-16.
- `crates/app/src/` — фронтенд (vanilla TS): `main.ts` (композиция), `docStore.ts`, `editorView.ts`, `previewView.ts` (единственное `preview.innerHTML`), `viewRegistry.ts`/`paneHost.ts`/`layout.ts` (Pane/Split, `MAX_PANES=2`), `linkController.ts`, `logPanel.ts`/`aiPanel.ts`/`resourceMonitor.ts` (нижний док), `tauri.ts`, `style.css`.
- `crates/app/src-tauri/` — тонкий Tauri-шелл (`darmark`/`darmark_lib`): `state.rs` (DocumentStore, D5), `error.rs` (`CommandError { code, message }`), документные и плагинные команды; `src/plugins/` — подсистема плагинов (Windows).
- `crates/plugin-core/` — ядро плагинной системы (кадры, конверт, Job Object, карантин, манифест); **без `mlua`/Tauri**.
- `crates/plugin-host/` — child `darmark-plugin-host` (Lua 5.5, `mlua`); единственный крейт с `mlua`.
- `crates/plugin-cli/` — CLI `darmark-plugin` (`test`/`init`, каталог `plugins/index.json`).
- Задачи только по фронтенду **не трогают** `md-core` и `src-tauri`.

## Команды

Из корня, если не указано иное:

| Задача | Команда |
| --- | --- |
| Тесты ядра (без GUI) | `cargo test -p md-core` |
| Шелл: сборка/тесты | `cargo check -p darmark` / `cargo test -p darmark` |
| Стейджинг сайдкаров (предусловие шелла) | `cd crates/app; npm run sidecars:dev` |
| Плагинные крейты + CLI | `cargo test -p plugin-core -p plugin-host -p plugin-cli` |
| Прогон плагина / CLI | `cargo run -p plugin-host -- --self-test <plugin.lua>`; `cargo run -p plugin-cli -- test plugins/word-count` |
| Guard: `mlua` вне GUI | `cargo tree -p darmark \| Select-String mlua` (пусто) |
| Фронт (tsc strict, валит на ошибке типов) | `cd crates/app; npm install; npm run build` |
| Dev-сервер | `npm run dev` (`:5173`, strictPort) |
| GUI | `npx tauri dev` |
| Релиз NSIS | `npx tauri build` (стейджит сайдкары, size-gate ≤6 МБ) |
| E2E (нужен release-бинарник) | `npx tauri build --no-bundle`, затем `cd crates/app; npm run test:e2e` |

Предусловие `darmark`: `crates/app/src-tauri/binaries/` с сайдкарами (`tauri-build` падает без них). Требования: Rust ≥ 1.80, Node ≥ 20.

## Жёсткие ограничения

- CSP `default-src 'self'` (`tauri.conf.json`): без внешних скриптов и `eval`; inline-стили разрешены.
- Строгий TS (`strict`, `noUnusedLocals`, `noUnusedParameters`): неиспользуемые сущности ломают сборку.
- Рендер Markdown — на Rust-стороне; фронт только присваивает `preview.innerHTML`.
- Юнит-тестов фронта нет (`vitest` нет) — проверка = `npm run build`.
- CI (`.github/workflows/ci.yml`, windows-latest): `rustfmt`/`clippy`/`cargo test`/`npm run build` + size-gate.
- E2E не покрывает нативные диалоги/ОС — помечай сценарий `@manual`.
- Релизный профиль (`Cargo.toml`): `opt-level="s"`, LTO, `strip`, `panic=abort`. Тяжёлые зависимости — только по согласованию.

## Конвенции

- Комментарии на русском, объясняют **зачем**; отступ 2 пробела (TS), `rustfmt` (Rust).
- Не коммить и не пушить без прямого запроса.
- Перед реализацией прочитай соответствующий `tasks/TZ-*.md`.

## Ссылки

| Что | Файл |
| --- | --- |
| Визуальная идентичность (токены, правила UI) | `DESIGN.md` |
| Нормативный layout/контракты, реестр ADR | `docs/DESIGN_DOC.md` |
| Решения | `docs/adr/` |
| Бэклог идей | `docs/BACKLOG.md` |
| Замеры и открытые вопросы | `docs/FINDINGS.md` |
| API и гайд плагинов | `docs/PLUGIN_API.md`, `docs/PLUGIN_GUIDE.md` |
| Прототипы-референсы | `docs/proto/` |
| Производительность E2E | `docs/E2E-PERFORMANCE.md` |
