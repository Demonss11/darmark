# AGENTS.md

`mdedit` — лёгкий Markdown-редактор/вьюер в духе Notepad++: слева редактор, справа HTML-предпросмотр.
Tauri 2 + vanilla TypeScript/Vite на фронте + чистое Rust-ядро Markdown. Целевая платформа — Windows,
разработка возможна на Linux. Документация, комментарии и UI-строки в репозитории — **на русском**;
при правках придерживайся этого.

## Структура и границы

- `crates/md-core/` — чистое ядро markdown→HTML (pulldown-cmark 0.13). **Без Tauri/GUI-зависимостей**;
  должно оставаться переиспользуемым в CLI/тестах. Здесь `to_html()`/`to_html_with()`, XSS-санитайзер
  и юнит-тесты.
- `crates/app/src/` — фронтенд на чистом TS (без фреймворков): `main.ts` (тулбар, файловые команды,
  рендер с debounce 120 мс), `tauri.ts` (IPC-обёртки), `tables.ts` (Excel-подобные таблицы), `style.css`.
- `crates/app/src-tauri/` — тонкий Tauri-шелл. Крейт/пакет — `mdedit`, lib — `mdedit_lib`.
  Здесь команды `read_file`/`write_file`/`render_markdown` и проверка расширений `ALLOWED_EXTS`.
- Задачи только по фронтенду не должны трогать `md-core` и `src-tauri` (см. область в `tasks/TZ-excel-tables.md`).

## Команды

Из корня репозитория, если не указано иное:

- `cargo test -p md-core` — юнит-тесты ядра; быстро, без GUI. Запускать после любых правок Rust.
- `cargo check -p mdedit` — компиляция Tauri-шелла.
- `cd crates/app; npm install; npm run build` — `tsc && vite build`; `tsc` **валит сборку на любой
  ошибке типов**. Запускать после правок TS (отдельного typecheck-скрипта нет).
- `npm run dev` — Vite dev-сервер на фиксированном `:5173` (`strictPort`, чтобы Tauri dev не «уплывал»).
- `npx tauri dev` — полноценный GUI; требует WebView2 + VS Build Tools (C++). Не запускать без запроса.
- `npx tauri build` — релизный NSIS-установщик в `src-tauri/target/release/bundle/nsis/`.

Требования: Rust ≥ 1.80, Node ≥ 20.

## Жёсткие ограничения

- **Строгая CSP** (`tauri.conf.json`): `default-src 'self'`. Никаких внешних скриптов/библиотек,
  никакого `eval`. Inline-стили разрешены.
- **Строгий TS** (`tsconfig.json`): `strict`, `noUnusedLocals`, `noUnusedParameters`. Неиспользуемые
  сущности ломают сборку — не создавай их.
- **Рендер идёт на Rust-стороне.** Фронт только присваивает `preview.innerHTML` и не парсит Markdown.
  Состояние таблиц — `WeakMap` по DOM-узлу (`tables.ts`), теряется при перерисовке; `main.ts` защищает
  это сравнением `lastRenderedHtml`, но стабильного ключа на таблицу пока нет.
- **Тестовой инфраструктуры для фронта и CI нет** (`.github/` отсутствует). В `package.json` только
  `dev`/`build`/`preview`; `vitest` не установлен. Проверка фронта = `npm run build`.
- Релизный профиль (корневой `Cargo.toml`): `opt-level="s"`, LTO, `strip`, `panic=abort` ради размера.
  Не добавляй тяжёлые зависимости без прямого согласования.

## Работа с задачами

- Перед реализацией **прочитай целиком** соответствующий `tasks/TZ-*.md`: `TZ-excel-tables.md`
  (только фронтенд, таблицы), `TZ-inspect-mode.md`, `TZ-fixes.md` (аудит безопасности/надёжности со
  статус-таблицей), `TZ-skills-integration.md`.
- Уже существующие инструкции для агентов: `KODA.md` (подробная память проекта), `.kodarules`
  (жёсткие правила: не читать локи/игнорируемые каталоги, не коммитить/пушить без запроса),
  `.kodaignore` (файлы-шум: `Cargo.lock`, `package-lock.json` и т.п.). При расхождении доверяй коду,
  а не докам — часть утверждений `KODA.md` устарела после появления санитайзера и DOM-diff.

## Конвенции

- Комментарии на русском, объясняют **зачем**, а не «что». Отступ 2 пробела в TS; стандартный `rustfmt` в Rust.
- UI-строки захардкожены по-русски; i18n запланирован в `TZ-excel-tables.md` AC-17.
- Не коммить и не пушить без прямого запроса.
