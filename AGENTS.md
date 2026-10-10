# AGENTS.md

`darmark` — лёгкий Markdown-редактор/вьюер в духе Notepad++: слева редактор, справа HTML-предпросмотр.
Tauri 2 + vanilla TypeScript/Vite на фронте + чистое Rust-ядро Markdown. Целевая платформа — Windows.
Документация, комментарии и UI-строки в репозитории — **на русском**;
при правках придерживайся этого.

## Структура и границы

- `crates/md-core/` — чистое ядро markdown→HTML (pulldown-cmark 0.13). **Без Tauri/GUI-зависимостей**;
  должно оставаться переиспользуемым в CLI/тестах. Здесь `to_html()`/`to_html_with()`/`to_html_mapped()`,
  XSS-санитайзер, рендер ведущей YAML-шапки в «таблицу-таблиц» (`frontmatter.rs`) и юнит-тесты.
- `crates/app/src/` — фронтенд на чистом TS (без фреймворков): `main.ts` (композиционный корень:
  связывает store, редактор, предпросмотр, тулбар), `docStore.ts` (проекция Rust-стора: текст/rev/
  path/dirty, дебаунс IPC, защита от гонок), `editorView.ts` (textarea-представление),
  `renderIndex.ts` (единый индекс на ревизию), `viewRegistry.ts` (реестр тир-1/тир-2 + `ViewContext`),
  `previewView.ts` (тир-1 preview, единственное `preview.innerHTML`), `layout.ts` (дерево Pane/Split,
  `MAX_PANES=2`), `paneHost.ts` (монтирование панелей), `linkController.ts` (inspector + scrollsync),
  `statusBar.ts`/`fileActions.ts`/`shell.ts` (оболочка) + `sidebar.ts` (rail + сворачиваемый
  sidebar idea5), `formatActions.ts` (Markdown-обёртки над выделением), `gutter.ts` (номера
  строк редактора), `sampleDocument.ts`, `ids.ts`
  (брендированные id §5.1), `tauri.ts` (IPC-обёртки), `tables.ts` (Excel-подобные таблицы),
  `inspector.ts` (двусторонняя подсветка блоков), `mapping.ts` (байты ↔ UTF-16), `scrollsync.ts`
  (синхронная прокрутка), `images.ts` (относительные src → asset-URL), `style.css`.
- `crates/app/src-tauri/` — тонкий Tauri-шелл. Крейт/пакет — `darmark`, lib — `darmark_lib`.
  `state.rs` — `DocumentStore` (D5: текст, rev, путь, кэш), `error.rs` — `CommandError { code, message }`.
  Команды документов `new_document`/`open_document`/`update_document`/`render_document`/
  `save_document`/`close_document` (рендер и кэш — в сторе, `render_markdown` удалён).
  Файловый доступ — модель Notepad++ (путь выбирает пользователь в нативном диалоге на фронте),
  единственная проверка — лимит 10 МБ. Подсистема плагинов (H2, `src/plugins/`, только Windows):
  `supervisor.rs` — spawn child `darmark-plugin-host.exe`, Job Object, watchdog (прогресс+дедлайн),
  проверка permissions и обслуживание host-call'ов; `services.rs` — range/delta к `DocumentStore`;
  `scan.rs` — `scan(plugins/)` + валидация манифеста; `settings.rs` — `SettingsStore`
  (`%APPDATA%/darmark/config.json`: вкл/выкл, согласие на permissions, recent files);
  `manager.rs` — жизненный цикл, карантин N=3, `load_plugins` (scan+settings→реестр),
  `list/set_enabled/reload`. `PluginManager` линкует `plugin-proto`, но **не** `plugin-host`/`mlua`
  (D6/ADR-0021).
- Задачи только по фронтенду не должны трогать `md-core` и `src-tauri`
- `crates/plugin-proto/` — продуктовое ядро плагинной системы (H2): кадры транспорта
  (`frame.rs`), serde-конверт хост↔child (`envelope.rs`), Job Object (`job.rs`), карантин
  (`quarantine.rs`), манифест и валидация (`manifest.rs`), формулировки границы (`notices.rs`),
  бюджеты watchdog (`limits.rs`). **Без mlua и без Tauri** — линкуется и в GUI-хост, и в child.
- `crates/plugin-host/` — child-процесс `darmark-plugin-host` с Lua 5.5 (mlua, vendored). Единственный
  крейт с `mlua`; `src-tauri` его **не** линкует (D6/ADR-0021, проверяется CI: `cargo tree -p darmark`
  без `mlua`). Здесь песочница (D17), `host.*`/`md.*`/`json.*` и stdio-цикл конверта.
- `crates/lua-proto/`, `crates/lua-rpc-spike/` — **прототип** (источник переноса блоков и фикстур
  `crash`/`hang`/`chatty`/`edit`), в продукт не линкуются; держатся в workspace до Фазы 2.

## Команды

Из корня репозитория, если не указано иное:

- `cargo test -p md-core` — юнит-тесты ядра; быстро, без GUI. Запускать после любых правок Rust.
- `cargo check -p darmark` — компиляция Tauri-шелла.
- `cargo test -p plugin-proto -p plugin-host` — плагинные крейты (H2). `plugin-host` тянет `mlua`
  (vendored) — первая сборка долгая. `cargo run -p plugin-host -- --self-test <plugin.lua>` — прогон
  плагина в песочнице без GUI. Guard инварианта: `cargo tree -p darmark | Select-String mlua` пусто.
- `cd crates/app; npm install; npm run build` — `tsc && vite build`; `tsc` **валит сборку на любой
  ошибке типов**. Запускать после правок TS (отдельного typecheck-скрипта нет).
- `npm run dev` — Vite dev-сервер на фиксированном `:5173` (`strictPort`, чтобы Tauri dev не «уплывал»).
- `npx tauri dev` — полноценный GUI; требует WebView2 + VS Build Tools (C++). Не запускать без запроса.
- `npx tauri build` — релизный NSIS-установщик в `src-tauri/target/release/bundle/nsis/`.
- `cd crates/app; npm run test:e2e` — GUI E2E (Cucumber + WebdriverIO поверх release-бинарника).
  Требует `cargo install tauri-driver --locked` и собранный `target/release/darmark.exe`
  (`npx tauri build --no-bundle`). Фичи/шаги — `crates/app/e2e/` (в т.ч. `frontmatter.feature` —
  YAML-шапка как таблица; таблицы шапки не украшаются `tables.ts`).

Требования: Rust ≥ 1.80, Node ≥ 20.

## Жёсткие ограничения

- **Строгая CSP** (`tauri.conf.json`): `default-src 'self'`. Никаких внешних скриптов/библиотек,
  никакого `eval`. Inline-стили разрешены.
- **Строгий TS** (`tsconfig.json`): `strict`, `noUnusedLocals`, `noUnusedParameters`. Неиспользуемые
  сущности ломают сборку — не создавай их.
- **Рендер идёт на Rust-стороне.** Фронт только присваивает `preview.innerHTML` и не парсит Markdown.
  Состояние таблиц — `WeakMap` по DOM-узлу (`tables.ts`), теряется при перерисовке; хост защищает
  это флагом `RenderResult.changed`, но стабильного ключа на таблицу пока нет.
- **Юнит-тестов фронтенда нет** (`vitest` не установлен); проверка фронта = `npm run build`.
  **CI** — `.github/workflows/ci.yml` (windows-latest): `rustfmt`/`clippy`/`cargo test`
  (`md-core`+`darmark`)/`npm run build` + size-gate (release-exe ≤ 6 МБ, D6). GUI E2E —
  Cucumber + WebdriverIO (`crates/app/e2e/`, `npm run test:e2e`) поверх release-бинарника;
  нативные диалоги/ОС им не покрываются — помечай `@manual`.
- Релизный профиль (корневой `Cargo.toml`): `opt-level="s"`, LTO, `strip`, `panic=abort` ради размера.
  Не добавляй тяжёлые зависимости без прямого согласования.

## Работа с задачами

- Перед реализацией **прочитай целиком** соответствующий `tasks/TZ-*.md`.

## Конвенции

- Комментарии на русском, объясняют **зачем**, а не «что». Отступ 2 пробела в TS; стандартный `rustfmt` в Rust.
- Не коммить и не пушить без прямого запроса.

## Агенты проекта

- `@code-reviewer` (read-only ревью диффов/PR до мержа: баги, безопасность, производительность, поддерживаемость),
- `@rust-developer` (написание/рефакторинг/отладка Rust),
- `@frontend-developer` (vanilla TS/DOM, состояние, IPC, строгая типизация, a11y),
- `@rust-tester` (поиск важных непокрытых мест в Rust-коде и покрытие их тестами; умеет работать по TDD — тест до кода),
- `@frontend-tester` (поиск непокрытых пользовательских сценариев и покрытие их E2E-сценариями Cucumber; нативные диалоги/ОС помечает `@manual`),
- `@plugin-developer` (написание/сопровождение Lua-плагинов: main.lua + plugin.json под песочницу darmark),
- `@security-reviewer` (read-only ревью безопасности: песочница, permissions, протокол host↔child, watchdog, XSS, CSP),
- `@architect` (дизайн-ревью до кодинга: согласованность с DESIGN_DOC/ADR, оформление новых ADR),
- `@release-engineer` (релизный процесс: прогон проверок, size-gate, сборка NSIS, сверка релизного профиля),
- `@ui-designer` (визуальные направления и прототипы-референсы интерфейса; не пишет продуктовый код).

## Навыки проекта

- `coding-discipline` (дисциплина изменений: минимальный дифф, правки строго по месту),
- `rust-testing` (механика написания Rust-тестов и поиска непокрытых мест),
- `cucumber-e2e` (механика E2E-тестирования интерфейса на Gherkin + Cucumber),
- `plugin-sandbox` (механика песочницы плагинов: plugin.json, permissions, watchdog, карантин),
- `tauri-ipc-conventions` (инварианты IPC и фронтенда: контракт фронт↔Rust, брендированные id, защита от гонок),
- `md-core-domain` (домен ядра Markdown: pulldown-cmark, XSS-санитайзер, YAML-шапка, mapping),
- `release-checklist` (пошаговая релизная процедура, проверки бюджета размера),
- `tooling-audit` (аудит агентов/навыков: frontmatter, permission, висячие ссылки, дубли, хардкод),
- `ui-prototype` (механика прототипирования: где лежат прототипы, CSS-токены, CSP-safe, чек-лист хендоффа).
