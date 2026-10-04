# KODA.md — память проекта mdedit

README **для AI-агентов**. Обязательно читать перед правками. Полное человеческое описание — в `README.md`.

## Что это

`mdedit` — лёгкий Markdown-редактор/вьюер (аналог Notepad++ для Markdown): слева редактор,
справа HTML-предпросмотр. **Без React/фреймворков** — чистый TypeScript + Vite, вся логика
Markdown на Rust. Целевая платформа — Windows (Tauri 2), разработка возможна на Linux.

## Архитектура (Cargo workspace)

```
crates/
├── md-core/            # ЧИСТОЕ ЯДРО: markdown → HTML (pulldown-cmark 0.13). Без UI-зависимостей.
│   └── src/lib.rs      #   to_html(), to_html_with(), DEFAULT_OPTIONS + юнит-тесты (GFM tables, tasklist, strike)
└── app/
    ├── src/            # Фронтенд (vanilla TS + Vite)
    │   ├── main.ts     #   тулбар, debounce-рендер 120 мс, lastRenderedHtml, файловые команды
    │   ├── tauri.ts    #   тонкие IPC-обёртки: renderMarkdown / readFile / writeFile
    │   ├── tables.ts   #   Excel-подобное поведение <table> в предпросмотре (сортировка/фильтры/поиск)
    │   └── style.css   #   CSS-переменные, light/dark через prefers-color-scheme
    ├── index.html      #   #toolbar, #editor (textarea), #preview (article.markdown-body), #statusbar
    └── src-tauri/      # Tauri-шелл: команды read_file / write_file / render_markdown
        └── src/lib.rs  #   IPC + проверка расширений (ALLOWED_EXTS)
```

**Главный принцип:** `md-core` НИЧЕГО не знает про Tauri/GUI — переиспользуется в CLI/TUI/тестах
без изменений. `src-tauri` — тонкая IPC-прослойка. Фронтенд не парсит Markdown — только рендерит
готовый HTML и украшает таблицы.

## Команды

```bash
# Rust-ядро (живёт и тестируется отдельно от GUI)
cargo test -p md-core          # юнит-тесты ядра — быстрая проверка без GUI
cargo check -p mdedit          # компиляция Tauri-шелла

# Фронтенд
cd crates/app
npm install
npm run dev                    # vite (dev-сервер на :5173)
npm run build                  # tsc && vite build  ← tsc падает на любых ошибках типов
npm run preview

# Всё приложение
npx tauri dev                  # разработка
npx tauri build                # релиз → NSIS-установщик в src-tauri/target/release/bundle/nsis/
```

Требования: Rust ≥ 1.80, Node.js ≥ 20. На Windows — WebView2 Runtime + VS Build Tools (C++).
Релизный профиль в корневом `Cargo.toml`: `opt-level="s"`, LTO, strip, panic=abort (мин. размер).

## Жёсткие ограничения (нарушение = баг или падение сборки)

1. **CSP строгая** (`tauri.conf.json`): `default-src 'self'; style-src 'self' 'unsafe-inline'`.
   Запрещены внешние библиотеки/скрипты, любой `eval`. Inline-стили для ширин колонок допустимы.
2. **TypeScript строгий** (`tsconfig.json`): `strict`, `noUnusedLocals`, `noUnusedParameters`.
   Новый код обязан это соблюдать, иначе `npm run build` упадёт на `tsc`.
3. **Ядро `md-core` не трогать** при фронтовых задачах (см. `tasks/TZ-*.md` — область только `crates/app/src/*`).
4. **Рендер идёт на Rust-стороне.** Фронт делает `preview.innerHTML = html` (`main.ts:doRender`)
   и не может «изнутри» отличить, изменилась ли конкретная таблица — для этого нужен стабильный ключ.
5. **Тестовой инфраструктуры для фронта нет**: в `package.json` только `dev`/`build`/`preview`,
   `vitest` не установлен, CI (`.github/`) нет. Rust-ядро тестируется через `cargo test`.
6. **Состояние таблиц** (`tables.ts`) исторически в `WeakMap` по DOM-узлу — теряется при `innerHTML`.
   Задачи по стабильному ключу/персистентности описаны в `tasks/TZ-excel-tables.md`.

## Конвенции

- Комментарии на русском, объясняют **зачем**, а не **что**.
- UI-строки сейчас захардкожены по-русски в `tables.ts`/`main.ts`; план i18n — в `tasks/TZ-excel-tables.md` (AC-17).
- Зависимости добавлять только после проверки, что они уместны и проходят CSP/размер-бюджет.
- Стиль: 2 пробела в TS, стандартный `rustfmt` в Rust.

## Текущие задачи (ТЗ в `tasks/`)

- `TZ-excel-tables.md` — таблицы «как в Excel»: стабильный ключ + sessionStorage-персистентность,
  точечный diff вместо `innerHTML`, предикатные фильтры, мультисортировка, `<mark>`-подсветка,
  sticky-первый столбец, копирование TSV/MD, печать, ресайз колонок, i18n, a11y, юнит-тесты ядра таблиц.
- `TZ-inspect-mode.md` — режим инспектора (hover/click подсветка md-блоков).
- `TZ-fixes.md` — точечные исправления.

Перед реализацией задачи — прочитать соответствующий `tasks/TZ-*.md` целиком.

## Дорожная карта (из README)

Вкладки · подсветка синтаксиса в редакторе · экспорт HTML/PDF · поиск/замена (Ctrl+F) ·
harness-прогоны CommonMark/GFM поверх `md-core`.
