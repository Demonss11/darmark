# AGENTS.md

`mdedit` — лёгкий Markdown-редактор/вьюер в духе Notepad++: слева редактор, справа HTML-предпросмотр.
Tauri 2 + vanilla TypeScript/Vite на фронте + чистое Rust-ядро Markdown. Целевая платформа — Windows. 
Документация, комментарии и UI-строки в репозитории — **на русском**;
при правках придерживайся этого.

## Структура и границы

- `crates/md-core/` — чистое ядро markdown→HTML (pulldown-cmark 0.13). **Без Tauri/GUI-зависимостей**;
  должно оставаться переиспользуемым в CLI/тестах. Здесь `to_html()`/`to_html_with()`, XSS-санитайзер
  и юнит-тесты.
- `crates/app/src/` — фронтенд на чистом TS (без фреймворков): `main.ts` (тулбар, файловые команды,
  рендер с debounce 120 мс), `ids.ts` (брендированные id §5.1), `tauri.ts` (IPC-обёртки),
  `tables.ts` (Excel-подобные таблицы), `style.css`.
- `crates/app/src-tauri/` — тонкий Tauri-шелл. Крейт/пакет — `mdedit`, lib — `mdedit_lib`.
  `state.rs` — `DocumentStore` (D5: текст, rev, путь, кэш), `error.rs` — `CommandError { code, message }`.
  Команды документов `new_document`/`open_document`/`save_document`/`close_document` + `render_markdown`.
  Файловый доступ — модель Notepad++ (путь выбирает пользователь в нативном диалоге на фронте),
  единственная проверка — лимит 10 МБ.
- Задачи только по фронтенду не должны трогать `md-core` и `src-tauri`

## Команды

Из корня репозитория, если не указано иное:

- `cargo test -p md-core` — юнит-тесты ядра; быстро, без GUI. Запускать после любых правок Rust.
- `cargo check -p mdedit` — компиляция Tauri-шелла.
- `cd crates/app; npm install; npm run build` — `tsc && vite build`; `tsc` **валит сборку на любой
  ошибке типов**. Запускать после правок TS (отдельного typecheck-скрипта нет).
- `npm run dev` — Vite dev-сервер на фиксированном `:5173` (`strictPort`, чтобы Tauri dev не «уплывал»).
- `npx tauri dev` — полноценный GUI; требует WebView2 + VS Build Tools (C++). Не запускать без запроса.
- `npx tauri build` — релизный NSIS-установщик в `src-tauri/target/release/bundle/nsis/`.
- `cd crates/app; npm run test:e2e` — GUI E2E (Cucumber + WebdriverIO поверх release-бинарника).
  Требует `cargo install tauri-driver --locked` и собранный `target/release/mdedit.exe`
  (`npx tauri build --no-bundle`). Фичи/шаги — `crates/app/e2e/`.

Требования: Rust ≥ 1.80, Node ≥ 20.

## Жёсткие ограничения

- **Строгая CSP** (`tauri.conf.json`): `default-src 'self'`. Никаких внешних скриптов/библиотек,
  никакого `eval`. Inline-стили разрешены.
- **Строгий TS** (`tsconfig.json`): `strict`, `noUnusedLocals`, `noUnusedParameters`. Неиспользуемые
  сущности ломают сборку — не создавай их.
- **Рендер идёт на Rust-стороне.** Фронт только присваивает `preview.innerHTML` и не парсит Markdown.
  Состояние таблиц — `WeakMap` по DOM-узлу (`tables.ts`), теряется при перерисовке; `main.ts` защищает
  это сравнением `lastRenderedHtml`, но стабильного ключа на таблицу пока нет.
- **Юнит-тестов фронтенда и CI нет** (`.github/` отсутствует, `vitest` не установлен); проверка
  фронта = `npm run build`. Есть GUI E2E на Cucumber + WebdriverIO (`crates/app/e2e/`, `npm run test:e2e`)
  поверх release-бинарника; нативные диалоги/ОС им не покрываются — помечай `@manual`.
- Релизный профиль (корневой `Cargo.toml`): `opt-level="s"`, LTO, `strip`, `panic=abort` ради размера.
  Не добавляй тяжёлые зависимости без прямого согласования.

## Работа с задачами

- Перед реализацией **прочитай целиком** соответствующий `tasks/TZ-*.md
- Уже существующие инструкции для агентов: `KODA.md` (подробная память проекта), `.kodarules`
  (жёсткие правила: не читать локи/игнорируемые каталоги, не коммитить/пушить без запроса),
  `.kodaignore` (файлы-шум: `Cargo.lock`, `package-lock.json` и т.п.). При расхождении доверяй коду,
  а не докам — часть утверждений `KODA.md` устарела после появления санитайзера и DOM-diff.

## Конвенции

- Комментарии на русском, объясняют **зачем**, а не «что». Отступ 2 пробела в TS; стандартный `rustfmt` в Rust.
- Не коммить и не пушить без прямого запроса.
