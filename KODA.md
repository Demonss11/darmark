# KODA.md — память проекта darmark

README **для AI-агентов**. Обязательно читать перед правками.

## Что это

`darmark` — лёгкий Markdown-редактор/вьюер (аналог Notepad++ для Markdown): слева редактор,
справа HTML-предпросмотр. **Без React/фреймворков** — чистый TypeScript + Vite, вся логика
Markdown на Rust. Целевая платформа — Windows (Tauri 2).

## Архитектура (Cargo workspace)

```
crates/
├── md-core/            # ЧИСТОЕ ЯДРО: markdown → HTML (pulldown-cmark 0.13). Без UI-зависимостей.
│   └── src/lib.rs      #   to_html(), to_html_with(), DEFAULT_OPTIONS + юнит-тесты (GFM tables, tasklist, strike)
└── app/
    ├── src/            # Фронтенд (vanilla TS + Vite)
    │   ├── main.ts     #   композиционный корень: store + реестр view + предпросмотр/тулбар
    │   ├── docStore.ts #   проекция Rust-стора (текст/rev/path/dirty), подписки, дебаунс IPC
    │   ├── editorView.ts#  тир-2 редактор (textarea): ввод → store, внешний текст ← store
    │   ├── renderIndex.ts#  единый индекс рендера на ревизию (inspector + scrollsync)
    │   ├── viewRegistry.ts# реестр тир-1/тир-2 провайдеров + ViewContext-фасад (§5.3–5.4)
    │   ├── previewView.ts#  тир-1 preview: единственное место preview.innerHTML
    │   ├── layout.ts   #   дерево Pane/Split (MAX_PANES=2) + чистые операции (§5.2)
    │   ├── paneHost.ts #   монтирование layout в DOM: слоты, видимость, активная панель
    │   ├── linkController.ts# inspector + scrollsync над парой панелей
    │   ├── statusBar.ts#   статусбар/заголовок (читают проекцию стора)
    │   ├── fileActions.ts#  файловые команды (new/open/save/save-as, диалоги)
    │   ├── shell.ts    #   тулбар, хоткеи, тумблеры, закрытие окна
    │   ├── sidebar.ts  #   rail + сворачиваемый sidebar (idea5), explorer — заглушка
    │   ├── formatActions.ts# Markdown-обёртки над выделением textarea (bold/italic/code/heading/link)
    │   ├── gutter.ts   #   номера строк, синхронные прокрутке редактора
    │   ├── sampleDocument.ts# стартовый демо-документ
    │   ├── tauri.ts    #   IPC-обёртки: документы (new/open/update/render/save/close) + errorMessage
    │   ├── ids.ts      #   брендированные DocumentId / PaneId / ViewId (§5.1)
    │   ├── tables.ts   #   Excel-подобное поведение <table> в предпросмотре (сортировка/фильтры/поиск)
    │   ├── inspector.ts#   режим инспектора: двусторонняя подсветка блока предпросмотр ↔ исходник
    │   ├── mapping.ts  #   конвертация байтовых смещений data-md ↔ UTF-16 (единственная граница)
    │   ├── scrollsync.ts#  синхронная прокрутка: анкоровая по блокам + пропорциональный fallback
    │   ├── images.ts   #   относительные src картинок → asset-URL (convertFileSrc)
    │   └── style.css   #   одна тёмная тема (idea5): токены, сетка оболочки, gutter, компоненты
    ├── index.html      #   toolbar/rail/sidebar/tabs/panes/gutter/preview/statusbar
    └── src-tauri/      # Tauri-шелл: команды документов (с рендером)
        ├── src/lib.rs  #   IPC-команды + файловый ввод-вывод (лимит 10 МБ)
        ├── src/state.rs#   DocumentStore (D5): текст, rev, путь, кэш рендера
        └── src/error.rs#   CommandError { code, message }
```

**Главный принцип:** `md-core` НИЧЕГО не знает про Tauri/GUI — переиспользуется в CLI/TUI/тестах
без изменений. `src-tauri` — тонкая IPC-прослойка. Фронтенд не парсит Markdown — только рендерит
готовый HTML и украшает таблицы.

## Команды

```bash
# Rust-ядро (живёт и тестируется отдельно от GUI)
cargo test -p md-core          # юнит-тесты ядра — быстрая проверка без GUI
cargo check -p darmark          # компиляция Tauri-шелла
cargo test -p darmark           # тесты файлового ввода-вывода шелла

# Фронтенд
cd crates/app
npm install
npm run dev                    # vite (dev-сервер на :5173)
npm run build                  # tsc && vite build  ← tsc падает на любых ошибках типов
npm run preview

# Всё приложение
npx tauri dev                  # разработка
npx tauri build                # релиз → NSIS-установщик в src-tauri/target/release/bundle/nsis/

# GUI E2E (Cucumber + WebdriverIO) — только по явному запросу
cargo install tauri-driver --locked      # один раз
npx tauri build --no-bundle              # ОБЯЗАТЕЛЬНО после правок фронта: E2E гоняют по exe
cd crates/app && npm run test:e2e        # все фичи
npm run test:e2e -- --spec e2e/features/tables.feature   # один feature
```

Требования: Rust ≥ 1.80, Node.js ≥ 20. На Windows — WebView2 Runtime + VS Build Tools (C++).
Релизный профиль в корневом `Cargo.toml`: `opt-level="s"`, LTO, strip, panic=abort (мин. размер).

## Жёсткие ограничения (нарушение = баг или падение сборки)

1. **CSP строгая** (`tauri.conf.json`): `default-src 'self'; script-src 'self';
   style-src 'self' 'unsafe-inline'; img-src 'self' data: https: asset: http://asset.localhost`.
   Запрещены внешние библиотеки/скрипты, любой `eval`. Inline-стили допустимы.
2. **TypeScript строгий** (`tsconfig.json`): `strict`, `noUnusedLocals`, `noUnusedParameters`.
   Новый код обязан это соблюдать, иначе `npm run build` упадёт на `tsc`.
3. **Ядро `md-core` не трогать** при фронтовых задачах (см. `tasks/TZ-*.md` — область только `crates/app/src/*`).
4. **Рендер идёт на Rust-стороне.** Фронт применяет HTML только в `onRender` (`main.ts`),
   причём трогает `preview.innerHTML` лишь когда `RenderResult.changed === true` — иначе
   сбрасывались бы сортировка/фильтры таблиц. Гонки ответов гасит `docStore` (`rev`, сверка `id`).
   Стабильного ключа на таблицу пока нет — состояние таблиц в `WeakMap` гибнет при перерисовке.
5. **Юнит-тестов фронтенда нет** (`vitest` не установлен, CI `.github/` нет). Есть GUI E2E:
   Cucumber + WebdriverIO (`crates/app/e2e/`, `npm run test:e2e`) поверх release-бинарника —
   требует пересборки (`npx tauri build --no-bundle`), иначе проверяется старый код.
   Нативные диалоги/ОС E2E не покрываются — сценарии помечать `@manual`.
   Rust-ядро тестируется через `cargo test -p md-core`, шелл — `cargo test -p darmark`.

## Конвенции

- Комментарии на русском, объясняют **зачем**, а не **что**.
- Зависимости добавлять только после проверки, что они уместны и проходят CSP/размер-бюджет.
- Стиль: 2 пробела в TS, стандартный `rustfmt` в Rust.

## Текущие задачи (ТЗ в `tasks/`)

Перед реализацией задачи — прочитать соответствующий `tasks/TZ-*.md` целиком.
`tasks/TZ-H1.md` — ядро Document/View/Pane: **H1 закрыт** (Фазы 0–6, переименование в `darmark`,
CI + size-gate). Следующая веха — H2 (плагинная система, `docs/ROADMAP.md`).

## Скиллы (`.koda/skills/`)

Скиллы описывают **методологию**, а не этот проект: архитектура, команды и ограничения
берутся отсюда и из `.kodarules`. При расхождении доверяй коду и этому файлу.

| Скилл | Когда |
|---|---|
| `frontend-dev` | любая правка `crates/app/src/**`, включая IPC-слой |
| `frontend-design` | правка `style.css`/`index.html`, выбор цвета/отступа/состояния |
| `review` | `/review` — ревью диффа или файла |
| `codebase-design` | разбор модуля на слои, выделение чистого ядра |
| `senior-system-analyst` | контракт интеграции, data flow, sequence-диаграмма |
| `pm` | декомпозиция в эпикосы/истории, метрики, PRD |
| `senior-discovery-product` | конкурентный анализ, валидация гипотезы фичи |
| `systematic-debugging` | любой баг — до предложения фикса |
| `verification-before-completion` | перед любым «готово / работает / проходит» |
| `writing-style` | правка прозы: ТЗ, комментарии, UI-строки, тело коммита |

## Дорожная карта (из README)

Вкладки · подсветка синтаксиса в редакторе · экспорт HTML/PDF · поиск/замена (Ctrl+F) ·
harness-прогоны CommonMark/GFM поверх `md-core`.
