# AGENTS.md

`darmark` — лёгкий Markdown-редактор/вьюер в духе Notepad++: слева редактор, справа HTML-предпросмотр.
Tauri 2 + vanilla TypeScript/Vite на фронте + чистое Rust-ядро Markdown. Целевая платформа — Windows.
Документация, комментарии и UI-строки в репозитории — **на русском**;
при правках придерживайся этого.

## Структура и границы

- `crates/md-core/` — чистое ядро markdown→HTML (pulldown-cmark 0.13). **Без Tauri/GUI-зависимостей**;
  должно оставаться переиспользуемым в CLI/тестах. Здесь `to_html()`/`to_html_with()`, XSS-санитайзер
  и юнит-тесты.
- `crates/app/src/` — фронтенд на чистом TS (без фреймворков): `main.ts` (композиционный корень:
  связывает store, редактор, предпросмотр, тулбар), `docStore.ts` (проекция Rust-стора: текст/rev/
  path/dirty, дебаунс IPC, защита от гонок), `editorView.ts` (textarea-представление),
  `renderIndex.ts` (единый индекс на ревизию), `viewRegistry.ts` (реестр тир-1/тир-2 + `ViewContext`),
  `previewView.ts` (тир-1 preview, единственное `preview.innerHTML`), `layout.ts` (дерево Pane/Split,
  `MAX_PANES=2`), `paneHost.ts` (монтирование панелей), `linkController.ts` (inspector + scrollsync),
  `statusBar.ts`/`fileActions.ts`/`shell.ts` (оболочка) + `sampleDocument.ts`, `ids.ts`
  (брендированные id §5.1), `tauri.ts` (IPC-обёртки), `tables.ts` (Excel-подобные таблицы),
  `inspector.ts` (двусторонняя подсветка блоков), `mapping.ts` (байты ↔ UTF-16), `scrollsync.ts`
  (синхронная прокрутка), `images.ts` (относительные src → asset-URL), `style.css`.
- `crates/app/src-tauri/` — тонкий Tauri-шелл. Крейт/пакет — `darmark`, lib — `darmark_lib`.
  `state.rs` — `DocumentStore` (D5: текст, rev, путь, кэш), `error.rs` — `CommandError { code, message }`.
  Команды документов `new_document`/`open_document`/`update_document`/`render_document`/
  `save_document`/`close_document` (рендер и кэш — в сторе, `render_markdown` удалён).
  Файловый доступ — модель Notepad++ (путь выбирает пользователь в нативном диалоге на фронте),
  единственная проверка — лимит 10 МБ.
- Задачи только по фронтенду не должны трогать `md-core` и `src-tauri`

## Команды

Из корня репозитория, если не указано иное:

- `cargo test -p md-core` — юнит-тесты ядра; быстро, без GUI. Запускать после любых правок Rust.
- `cargo check -p darmark` — компиляция Tauri-шелла.
- `cd crates/app; npm install; npm run build` — `tsc && vite build`; `tsc` **валит сборку на любой
  ошибке типов**. Запускать после правок TS (отдельного typecheck-скрипта нет).
- `npm run dev` — Vite dev-сервер на фиксированном `:5173` (`strictPort`, чтобы Tauri dev не «уплывал»).
- `npx tauri dev` — полноценный GUI; требует WebView2 + VS Build Tools (C++). Не запускать без запроса.
- `npx tauri build` — релизный NSIS-установщик в `src-tauri/target/release/bundle/nsis/`.
- `cd crates/app; npm run test:e2e` — GUI E2E (Cucumber + WebdriverIO поверх release-бинарника).
  Требует `cargo install tauri-driver --locked` и собранный `target/release/darmark.exe`
  (`npx tauri build --no-bundle`). Фичи/шаги — `crates/app/e2e/`.

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
- Уже существующие инструкции для агентов: `KODA.md` (подробная память проекта), `.kodarules`
  (жёсткие правила: не читать локи/игнорируемые каталоги, не коммитить/пушить без запроса),
  `.kodaignore` (файлы-шум: `Cargo.lock`, `package-lock.json` и т.п.). При расхождении доверяй коду,
  а не докам — часть утверждений `KODA.md` устарела после появления санитайзера и DOM-diff.

## Конвенции

- Комментарии на русском, объясняют **зачем**, а не «что». Отступ 2 пробела в TS; стандартный `rustfmt` в Rust.
- Не коммить и не пушить без прямого запроса.
