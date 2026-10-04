# mdedit — лёгкий Markdown-редактор/вьюер (Tauri 2 + Rust + TS)

Аналог Notepad++ для Markdown: окно с редактором слева и HTML-предпросмотром справа.
Никакого React/фреймворков — чистый TypeScript + Vite, вся логика Markdown на Rust.

## Архитектура

```
crates/
├── md-core/          # ЧИСТОЕ ЯДРО: markdown → HTML (pulldown-cmark). Без UI-зависимостей.
│   └── src/lib.rs    #   to_html(), to_html_with() + юнит-тесты (таблицы GFM, tasklist, strike…)
└── app/
    ├── src/          # Фронтенд (TS + Vite): main.ts, tauri.ts (IPC), style.css
    └── src-tauri/    # Tauri-шелл: команды read_file / write_file / render_markdown
```

Принцип: `md-core` ничего не знает ни про Tauri, ни про GUI — его можно переиспользовать
в будущем CLI/TUI/harness без изменений. Shell (`src-tauri`) — тонкая прослойка IPC.

## Возможности (MVP)

- **Новый / Открыть / Сохранить / Сохранить как** — кнопки тулбара + Ctrl+N/O/S/Shift+S.
- **Предпросмотр в реальном времени** (дебаунс 120 мс), Ctrl+P — скрыть/показать.
- **Таблицы GitHub-Flavored Markdown** с выравниванием колонок (`|---|---:|`) —
  рендерятся через pulldown-cmark (CommonMark-compliant) со стилями и горизонтальным скроллом.
- **Excel-подобное поведение таблиц в предпросмотре** (`src/tables.ts`, чистый TS, ядро не трогает):
  сортировка кликом по заголовку ↑/↓ с автоопределением типа колонки (число/дата/текст),
  глобальный поиск по таблице, фильтры значений по каждой колонке (воронка ▾, как в Excel),
  «липкая» шапка, счётчик отфильтрованных строк, кнопка сброса.
- Strikethrough, task lists, footnotes, heading attributes.
- Индикатор несохранённых изменений (● в заголовке), диалог подтверждения при закрытии.
- Тёмная тема через `prefers-color-scheme`.

## Сборка и запуск

Требуется Rust ≥ 1.80 и Node.js ≥ 20.

### Windows (целевая платформа)
1. Установить [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/) (обычно уже есть в Win 10/11) и Visual Studio Build Tools (C++).
2. `npm install -g @tauri-apps/cli` (или использовать локальный dev-зависимость).
3. Dev: `cd crates/app && npm install && npx tauri dev`
4. Релиз: `npx tauri build` → NSIS-установщик в `src-tauri/target/release/bundle/nsis/`.
   Профиль release в корневом `Cargo.toml`: `opt-level="s"`, LTO, strip, panic=abort — ради минимального размера.

### Linux (для разработки/тестов)
```
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev \
  libssl-dev libayatana-appindicator3-dev librsvg2-dev
cargo test -p md-core            # ядро живёт и тестируется отдельно от GUI
cd crates/app && npm install && npm run build   # фронтенд
cargo check -p mdedit             # компиляция шелла
```

## Быстрая проверка ядра без GUI

```
cargo test -p md-core        # 14 тестов ядра (GFM-таблицы + XSS-санитайзер)
cargo test -p mdedit         # 7 тестов путей/IPC Tauri-шелла
```

## Безопасность и модель угроз

- **Предпросмотр.** `md-core::to_html` прогоняет HTML через простой строковый санитайзер:
  вырезает `<script>…</script>`, удаляет `on*=`-атрибуты, нейтрализует `javascript:` и
  `data:text/html` в `href`/`src`. Это **минимальная** защита, слабее контекстного парсера
  (`ammonia`/DOMPurify): экзотические мутации (`<svg><style>` и т.п.) не покрыты. При росте
  требований — переходить на ammonia (ценой размера бинарника).
- **CSP** (`src-tauri/tauri.conf.json`): `default-src 'self'; script-src 'self'; style-src 'self'
  'unsafe-inline'; img-src 'self' data: https:`. Внешние скрипты, `eval` и `http:`-картинки запрещены.
- **Файловый доступ.** Команды `read_file`/`write_file` принимают только пути внутри
  Documents/Desktop/Downloads (или домашнего каталога, если стандартных нет) и расширения
  `md/markdown/mdown/mkd/txt`; проверяется канонизированный путь (симлинки наружу отсекаются),
  размер чтения ≤ 10 МБ. Каталог, выбранный пользователем в нативном диалоге, разрешается на
  текущую сессию — диалог показывается на Rust-стороне (`pick_open_file`/`pick_save_file`),
  поэтому фронтенд не может сам «разрешить» произвольный путь.
- **Внешние ссылки** в предпросмотре перехватываются и открываются через `window.open`
  (best-effort без `tauri-plugin-opener`).

### Ручной чек-лист безопасности

- [ ] `read_file` с путём вне разрешённых каталогов → ошибка.
- [ ] Файл > 10 МБ → понятная ошибка, приложение не виснет.
- [ ] XSS-документ (`<img src=x onerror=…>`, `[x](javascript:…)`) → в предпросмотре нет
  `onerror`/`javascript:`.
- [ ] Клик по http(s)-ссылке в предпросмотре → открытие во внешнем браузере.
- [ ] «Сохранить как» в каталог вне белых корней → файл сохраняется (каталог запоминается).
- [ ] Фильтр колонки при активном мини-поиске: «Ничего»/«Все» меняют только видимые значения,
  а «все выбраны = фильтр снят» считается по фактическому набору.

## Дорожная карта

- [ ] Вкладки (несколько документов)
- [ ] Подсветка синтаксиса в редакторе (CodeMirror 6 или свой overlay)
- [ ] Экспорт HTML/PDF
- [ ] Поиск/замена (Ctrl+F)
- [ ] Harness поверх `md-core` (прогон корпусов CommonMark/GFM spec)
