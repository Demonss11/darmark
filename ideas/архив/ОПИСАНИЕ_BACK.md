# ОТЧЁТ: Rust-слой, контракты, документы (mdedit)

## 0. Наличие проектных документов — ПУСТО

Проверено отдельными поисками:
- `**/TZ-*.md` → **нет файлов**
- `**/.kodarules` → **нет**
- `**/{AGENTS,CLAUDE,.kodarules,koda*,*.rules}*` → **нет**
- `**/tasks/**` → **нет каталога**
- `**/*.md` → ровно 3 файла: `README.md`, `crates/app/e2e/fixtures/ac10-unicode.md`, `ac10-unicode-crlf.md`
- листинг корня с `respect_git_ignore:false`: только `Cargo.lock`, `Cargo.toml`, `README.md`, `crates/`

**Вывод: единственный проектным документ — `README.md`.** Имена ТЗ живут только как комментарии-ссылки в коде (сами файлы отсутствуют): `TZ-inspect-tables` (`md-core/src/lib.rs:173`, `:1631`; `inspect-tables.feature:3`; `helpers.js:567`; `inspect-tables.steps.js:1`), `TZ-scroll-sync-v2` (`scrollsync.feature:3`, `helpers.js:290`, `scrollsync.steps.js:1`), `TZ-inspect-mode-review-v1.1` (`inspect-tables.feature:4`), `TZ-excel-tables` (`tables.feature:5`).

---

## 1. `crates/md-core/src/lib.rs` — 1867 строк, 63 `#[test]`

### Публичный API (весь, без исключений)

| Строка | Сущность |
|---|---|
| `L14` | `pub const DEFAULT_OPTIONS: Options` |
| `L25` | `pub fn to_html(markdown: &str) -> String` |
| `L30` | `pub fn to_html_with(markdown: &str, options: Options) -> String` |
| `L64` | `pub fn to_html_mapped(markdown: &str) -> String` |

`pub struct` / `pub enum` — **нет ни одного**. Всё остальное приватно: `enum StructTag { Row, Th, Td }` (`L176-182`, `#[derive(Debug, Clone, Copy, PartialEq, Eq)]`), `struct StructRange { tag, start, end }` (`L184-188`, `#[derive(Debug, Clone, Copy)]`).

### Serde-типы во фронтенд — ОТСУТСТВУЮТ

- В `crates/md-core/Cargo.toml` **нет serde** вообще: единственная зависимость `pulldown-cmark = { version = "0.13", default-features = false, features = ["html"] }` (в lock — `0.13.4`).
- В `src-tauri` `serde`/`serde_json` объявлены в `Cargo.toml:21-22`, но grep по `*.rs` не нашёл **ни одного** `serde`, `#[derive(Serialize)]` — они не используются.
- Контракт с JS — **голый `String` (HTML-фрагмент)**. Единственная «структура», передаваемая во фронт — текстовый атрибут `data-md="{start},{end}"` (байтовые смещения UTF-8):
  - на обёртках `<div class="md-block" data-md="…">` (`L155-160`);
  - на структурных тегах таблиц `<tr>/<th>/<td>` (`push_with_data_md`, `L367`).

### Пайплайн

1. **Обычный режим:** `to_html` → `to_html_with(md, DEFAULT_OPTIONS)` → `Parser::new_ext` → `html::push_html` → **`sanitize_html`** (`L30-35`).
2. **Mapped-режим (`to_html_mapped`, `L64`):** один проход `Parser::new_ext(...).into_offset_iter()`; события группируются по топ-блокам по счётчику `depth` (`L74-116`); параллельно собираются `StructRange` для таблиц (`collect_struct_range`, `L197`); промежутки ссылочных определений приклеиваются к следующему блоку (`L118-137`); затем на блок: `trim_end_ws` → `html::push_html` → `sanitize_html` → **только после санитайзера** `inject_structural_data_md` (`L143-166`, комментарий: «Санитайзер срезал бы `data-*`, поэтому вложенную разметку таблицы добавляем строго после него»).
3. Расширения (`L14-20`): `ENABLE_TABLES | STRIKETHROUGH | TASKLISTS | FOOTNOTES | HEADING_ATTRIBUTES`.
4. Санитайзер v2 — строковый, по белым спискам: `ALLOWED_TAGS` (`L418`), `DROP_CONTENT_TAGS` (`L477`), `GLOBAL_ATTRS` (`L483`), `ALLOWED_SCHEMES = ["http","https","mailto","tel"]` (`L486`), `URL_ATTRS` (`L489`), `tag_attrs` (`L509`), `decode_entities` (`L539`), `url_is_safe` (`L603`), `style_is_safe` — только `text-align` (`L639`), `rewrite_tag` (`L778`, работает по `Vec<char>` — защита от кириллицы), `a` получает `rel="noopener noreferrer"` (`L880`).

### Кэш / состояние / мьютексы

**Нет.** Grep по `Mutex|RwLock|OnceLock|static |State<|manage\(` в `crates/**/*.rs` дал только `&'static [&'static str]` (`md-core/lib.rs:509`). Все функции чистые. Единственный кэш в проекте — фронтовый `lastRenderedHtml` (`src/main.ts:43`).

### Тесты (63 `#[test]`, все в `mod tests` с `L902`)

- **Рендер/расширения** (`L903-945`): заголовок+абзац, GFM-таблица с выравниванием, таблица без расширения = текст, strikethrough/tasklist, footnotes, пустой вход.
- **XSS-санитайзер v2** (`L951-1069`): `onerror`, `<script>`, `<style>`+содержимое, iframe/form/object, unknown-tag unwrap, `javascript:`/`data:text/html`/сущностные обфускации, attribute breakout через `&#34;`, `rel=noopener`, `style` вне ячеек, сохранение нормальных ссылок/картинок/атрибутов, HTML-комментарии.
- **Unicode-regression** (`L1077-1116`): кириллица в `title`/`alt`, эмодзи, `onerror` после кириллицы, `javascript:` в кириллическом контексте (ловит старый panic «not a char boundary»).
- **Табличный harness** (`L1123-1213`): `assert_clean` + таблицы `EVENT_HANDLER_VECTORS` (9 векторов) и `DANGEROUS_URL_VECTORS` (8), обратный инвариант benign-контента.
- **Внутренние функции** (`L1216-1244`): `decode_entities`, `url_is_safe`, `style_is_safe`.
- **`to_html_mapped` T-1…T-5** (`L1268-1374`): границы блоков, вложенные списки/цитаты, fenced code, footnote-def отдельным блоком, setext, пустой вход, trim хвоста, UTF-8-границы, CRLF-байты.
- **Инвариант «mapped == to_html»** (`L1499-1634`): `strip_mapped_wrappers` + `strip_all_data_md` на 11 документах; разрешение ссылочных определений/картинок/сносок; `<hr>` индексируется; комментарий не даёт пустого блока; покрытие непустых строк без наложений.
- **Гранулярность таблиц (TZ-inspect-tables)** (`L1637-1866`): порядок `th/tr/td` и их исходные фрагменты, вложенность в `.md-block`, char-boundaries, пустая ячейка, таблица в цитате и в списке, inline-разметка в ячейке (AC-9), экранированный `\|` (AC-11), CRLF (AC-10), несколько таблиц, выравнивание, большая таблица 500×8 (`L1832`).

`crates/md-core/tests/` — **каталог пуст** (интеграционных тестов нет).

---

## 2. `crates/app/src-tauri/src/lib.rs` — 179 строк

### Все `#[tauri::command]` (три, других нет)

```rust
// L34-51
#[tauri::command]
async fn read_file(path: PathBuf, app: tauri::AppHandle) -> Result<String, String>
```
`spawn_blocking(read_file_impl)` → при успехе `allow_asset_dir(app, path.parent())`.

```rust
// L52-71
#[tauri::command]
async fn write_file(path: PathBuf, contents: String, app: tauri::AppHandle) -> Result<(), String>
```
`spawn_blocking(write_file_impl)` → при успехе тоже расширяет asset-scope.

```rust
// L84-91
#[tauri::command]
fn render_markdown(markdown: String, mapped: Option<bool>) -> String
```
Синхронный; `mapped == true` → `md_core::to_html_mapped`, иначе `md_core::to_html`.

Приватные хелперы: `MAX_FILE_SIZE: u64 = 10 * 1024 * 1024` (`L10`), `read_file_impl` (`L13`), `write_file_impl` (`L25`), `allow_asset_dir` (`L73`, `app.asset_protocol_scope().allow_directory(dir, true)`, ошибка только в `eprintln!`).

### Managed state / события / плагины

- **`app.manage(...)` — нет. `State<...>` — нет.** Grep по `manage\(|State<` в `.rs` — ноль совпадений. Rust-слой полностью stateless.
- **Событий нет**: ни `emit`, ни `listen`, ни `app.emit` в Rust. На фронте слушается только window-событие `onCloseRequested` (`src/main.ts:281`) + `appWindow.destroy()` (`L290`).
- **Плагины Tauri** (`Cargo.toml:19-20`): `tauri-plugin-dialog = "2"` (lock 2.8.1), `tauri-plugin-opener = "2"` (lock 2.7.0); инициализация `L96-97`. Сам Tauri: `tauri = { version = "2", features = ["protocol-asset"] }` (lock `2.12.1`).
- `run()` (`L94-108`): `Builder::default().plugin(dialog).plugin(opener).setup(→ #[cfg(windows)] disable_browser_accelerator_keys).invoke_handler(generate_handler![read_file, write_file, render_markdown])`.
- Windows-only (`L117-142`): `webview2-com = "0.39"` (lock 0.39.1) + `windows-core = "0.62"` → `ICoreWebView2Settings3::SetAreBrowserAcceleratorKeysEnabled(false)` для окна `"main"`.
- `main.rs` (6 строк): `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` + `mdedit_lib::run()`. `build.rs`: только `tauri_build::build()`.

### Обработка файловых I/O-ошибок

Тип ошибки — **`String`** (не кастомный `serde`-enum). Формат — префикс пути:

```rust
// L14
let metadata = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
// L16-20
return Err(format!("Файл больше {} МБ — открытие отменено", MAX_FILE_SIZE / (1024 * 1024)));
// L22
std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
```
В JS это прилетает как **отклонённый промис с обычным текстовым сообщением** (Tauri сериализует `String` в error payload); фронт ловит через `catch` и показывает в статус (`main.ts`). Ошибка `JoinError` от `spawn_blocking` сворачивается в `e.to_string()` (`L42`, `L60`).

Тесты (`L144-179`, dev-dep `tempfile = "3"`): `reads_existing_file`, `rejects_file_over_size_limit` (проверяет `"МБ"` в тексте), `writes_file`.

---

## 3. `tauri.conf.json` + `capabilities/default.json`

**tauri.conf.json** (43 строки):
- `identifier: "dev.mdedit.app"`, `productName: "mdedit"`, `version: "0.1.0"`.
- `build`: `beforeDevCommand: "npm run dev"`, `devUrl: "http://localhost:5173"`, `beforeBuildCommand: "npm run build"`, `frontendDist: "../dist"`.
- Окно одно: `title "mdedit — Markdown editor"`, 1200×800, min 640×480. **`label` не задан явно → "main"** (на него опираются capabilities и `get_webview_window("main")`).
- CSP: `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: https: asset: http://asset.localhost`.
- `assetProtocol: { enable: true, scope: [] }` — пустой статический scope, расширяется рантаймом из Rust.
- `bundle`: `targets: ["nsis"]`, `webviewInstallMode: downloadBootstrapper`, иконка `icons/icon.ico`.

**capabilities/default.json** (13 строк): `windows: ["main"]`, permissions: `core:default`, `dialog:default`, `core:window:allow-destroy`, `opener:default`. Свои команды (`read_file`/`write_file`/`render_markdown`) перечислять не нужно — доступны по умолчанию (это зафиксировано в `description`). **Нет** `fs`-плагина, нет `shell`, нет `updater`.

---

## 4. `crates/app/package.json`

Скрипты: `dev: vite`, `build: "tsc && vite build"`, `preview`, `tauri: tauri`, `tauri:dev: tauri dev`, `tauri:build: tauri build`, `test:e2e: "wdio run e2e/wdio.conf.js"`.

dependencies: `@tauri-apps/api ^2.1.0`, `@tauri-apps/plugin-dialog ^2.2.0`, `@tauri-apps/plugin-opener ^2.7.0`.
devDependencies: `@tauri-apps/cli ^2.12.1`, `@wdio/cli ^9.32.0`, `@wdio/cucumber-framework ^9.32.0`, `@wdio/local-runner ^9.32.0`, `@wdio/tauri-service ^1.4.0`, `typescript ^5.6.0`, `vite ^5.4.0`, `webdriverio ^9.32.0`.

**Ничего кроме vite/typescript/wdio/tauri нет**: ни CodeMirror, ни парсеров, ни тест-раннеров JS, ни линтеров, ни prettier. `type: "module"`, `private: true`.

`tsconfig.json`: `target ES2021`, `module ESNext`, `moduleResolution "bundler"`, `strict: true`, `noUnusedLocals`, `noUnusedParameters`, `noFallthroughCasesInSwitch`, `noEmit`, `include: ["src"]`.

`vite.config.ts`: `clearScreen: false`, `server.port 5173` + `strictPort: true`, `server.watch.ignored: ["**/src-tauri/**"]`, `build.target ["es2021","chrome100","safari13"]`, `minify: "esbuild"`, `sourcemap: false`.

Корневой `Cargo.toml`: workspace `["crates/md-core", "crates/app/src-tauri"]`, `edition 2021`, `[profile.release] opt-level="s", lto=true, codegen-units=1, strip=true, panic="abort"`.

---

## 5. Содержание `README.md` (единственный документ, 216 строк)

1. **Позиционирование:** аналог Notepad++ для Markdown, редактор слева / HTML-предпросмотр справа; «Никакого React/фреймворков — чистый TypeScript + Vite, вся логика Markdown на Rust».
2. **Архитектура (буквально зафиксирована в дереве):** `md-core` — «ЧИСТОЕ ЯДРО: markdown → HTML (pulldown-cmark). Без UI-зависимостей»; `app/src` — фронт (main.ts, tauri.ts (IPC), tables.ts, inspector.ts, images.ts, style.css); `app/src-tauri` — «Tauri-шелл: команды read_file / write_file / render_markdown». Принцип: «`md-core` ничего не знает ни про Tauri, ни про GUI — его можно переиспользовать в будущем CLI/TUI/harness без изменений. Shell — тонкая прослойка IPC».
3. **Инспектор:** ядро экспортирует `to_html_mapped`, фронт (`inspector.ts`) использует метки «не парся markdown самостоятельно». Гранулярность — **топ-блок**; inline-элементы «сознательно вне v1». Диапазоны в байтах UTF-8, выделение textarea в UTF-16, конвертация через префиксные карты по `editor.value` (учёт нормализации CRLF в WebView).
4. **MVP-возможности:** Файл-операции + хоткеи; предпросмотр с дебаунсом 120 мс, Ctrl+P; GFM-таблицы; «Excel-подобное поведение таблиц» в `tables.ts` — «чистый TS, ядро не трогает» (сортировка с автоопределением типа, поиск, фильтры-воронки, липкая шапка, счётчик, сброс); task lists/strike/footnotes/heading attrs; инспектор (Ctrl+I, Esc); локальные картинки через asset-протокол (`convertFileSrc`, каталог файла резолвится на Rust-стороне); синхронная прокрутка (`scrollsync.ts`, по умолчанию вкл.; с инспектором — анкорная привязка по `data-md`, без — пропорция; карты в `mapping.ts`; rAF + окно игнорирования эха WebView2); индикатор несохранённых изменений; тёмная тема.
5. **Сборка:** Rust ≥ 1.80, Node ≥ 20; Windows — WebView2 + VS Build Tools; «Релиз одной командой `npm run tauri:build`»; **важно: «обычный `cargo build`/`cargo run` НЕ собирает единый exe с фронтом»**; Linux-пакеты для dev.
6. **Проверка ядра без GUI:** `cargo test -p md-core`, `cargo test -p mdedit` («3 теста IPC»).
7. **E2E:** Cucumber + WebdriverIO + `@wdio/tauri-service`, драйвер внешний `tauri-driver`; перечислено покрытое; ручные проверки помечаются `@manual`; предусловия (`cargo install tauri-driver`, собранный release, `MDEDIT_APP_BINARY`).
8. **Безопасность/модель угроз:** санитайзер v2 по белым спискам (идеи из `ammonia`, без `html5ever`), явное ограничение — «санитайзер строковый, без настоящего HTML-парсера», альтернатива `ammonia` «ценой +0.6 МБ к exe»; CSP; **«Файловый доступ. Модель Notepad++: … „белого списка“ каталогов нет, `read_file`/`write_file` читают/пишут любой выбранный путь. Единственная защита при чтении — лимит 10 МБ; не-UTF-8 файлы не читаются. (Осознанный отказ от defense-in-depth P0.1/P2.2 ради простоты)»**; scope asset-протокола = каталог открытого файла рекурсивно; внешние ссылки → `opener`.
9. **Ручной чек-лист безопасности** (2 пункта отмечены как авто).
10. **Сценарий P1.4** — пошаговая Excel-семантика «Все»/«Ничего»/«все выбраны = фильтр снят».
11. **Дорожная карта:** вкладки (несколько документов), подсветка синтаксиса (CodeMirror 6 или свой overlay), экспорт HTML/PDF, поиск/замена, harness поверх `md-core`.

**Запреты, явно зафиксированные в README:** не тащить UI-зависимости в `md-core`; не парсить markdown на фронте; не переносить логику таблиц в ядро; не использовать `cargo build` как способ сборки релиза; не строить React/фреймворк-слой.

**Про плагины/WASM/wasmi/JSON-RPC/stdio/Document/View/Pane в документации — НИЧЕГО НЕТ.** Ни одного упоминания (см. п.7). Ближайшее к «плагинам» — Tauri-плагины dialog/opener и дорожная карта про вкладки.

---

## 6. E2E-контракты (что захардкожено в шагах — нельзя ломать)

**Важный факт:** grep по `crates/app/e2e` **не нашёл ни одного** упоминания имён Rust-команд (`render_markdown`, `read_file`, `write_file`) и `invoke(`. Тесты идут **только через DOM** — контракт с Rust держится опосредованно через HTML-разметку ядра.

**DOM id (из `index.html`, используются в `helpers.js`/шагах):**
`editor` (textarea), `preview` (article), `btn-inspect`, `chk-sync`, `chk-preview`, `btn-new`, `btn-open`, `btn-save`, `btn-save-as`, `file-label`, `stat-pos`, `stat-size`, `stat-msg`, `stat-inspect`, `toolbar`, `panes`, `statusbar`, `toggle-sync`, `toggle-preview`, `app`.

**CSS-классы/селекторы, на которые опираются тесты:**
- `#preview .md-block[data-md]` — ядро (`helpers.js:146,181,205,461,480,547,579,609,639,665,759`)
- атрибут `data-md="start,end"` — формат «байты через запятую» (`helpers.js:186`, `inspector.steps.js:56`)
- `.table-enhanced`, `.table-enhanced tbody tr`, `.table-enhanced thead th`, `.table-enhanced td[data-md]`, `.table-enhanced thead th[data-md]`, `.table-enhanced tbody tr[data-md]` (`helpers.js:41,63,76,580,610,640,666,758`)
- `.table-count` (`helpers.js:52`), `.table-scroll` (`helpers.js:641`), `.col-filter-btn` (`helpers.js:66`), `.col-filter-menu` (`helpers.js:87,97,110`), `.col-filter-item` + `item.dataset.value` (`helpers.js:103-105`), `.col-filter-footer button` по тексту `"Все"`/`"Ничего"` (`helpers.js:112-118`)
- `.inspect-active` (`helpers.js:291,712`), `.inspect-col` (`helpers.js:720`), `tr.inspect-row` (`helpers.js:727`)
- `#preview .inspect-active` — активный элемент инспектора; сверка `tagName` (`TD`/`TR`/`TH`/`DIV`) — `inspect-tables.feature` + `helpers.js:712`
- `.markdown-body` (в `index.html`, тестами не читается)

**Атрибуты/свойства редактора:** `wrap="off"` и `white-space: pre` — проверяются явно (`scrollsync.steps.js:300-308`).

**Текстовые контракты:** стартовый документ обязан содержать заголовок `"Добро пожаловать в mdedit"` (`smoke.feature` ↔ `main.ts:297`); заголовки демо-таблицы `Файл | Размер | Строк | Изменён` и значения `README.md/Cargo.toml/main.rs/style.css` (README-чек-лист P1.4 + `tables.feature`); текст счётчика `"3 из 4"` / `"4 строк"` (формат генерируется в `tables.ts`).

**Поведенческие константы, зашитые в тесты:** дебаунс рендера **120 мс** (`main.ts:50`, паузы 250 мс в `helpers.js:14,357`); окно эха **ECHO_MS = 100** (`scrollsync.ts:52`) ↔ `ECHO_SLACK = 160` (`helpers.js:298`); throttle скролла инспектора 100 мс (`inspector.ts:121`); защита `lastRenderedHtml` обходится двойной установкой значения (`helpers.js:8-14`); хистерезис `activeBlockEl === el` сбрасывается синтетическим `mouseout` (`helpers.js:190,207`); инспектор на время теста получает `pointer-events: none` на `#preview` (`helpers.js:143`).

**Глобальные window-переменные (контракт с `hooks.js`):** `window.__xss` (XSS-ловушка, `preview.feature` + `preview.steps.js:19`), `window.__errors` / `window.__errorCapture` (сбор необработанных ошибок, `hooks.js:10-19` ↔ `helpers.js:capturedErrors`).

**Прочие контракты:** `data-anchor-id` на воронке и `data-anchor` на меню (`tables.ts:250,256,391`); классы `sorted-asc`/`sorted-desc`/`filtered`; `@manual`-тег (в `wdio.conf.js:60` `tags: "not @manual"`, в фичах сейчас используется только `@ac5`); бинарник по умолчанию `<repo>/target/release/mdedit.exe`, переопределение `MDEDIT_APP_BINARY` (`wdio.conf.js:12-15`); порт tauri-driver 4444.

---

## 7. Поиск заготовок PaneId / DocumentId / ViewId / ViewProvider / DocumentStore / plugin / WASM

Регистронезависимый поиск по всему дереву (включая `.feature`, `.md`, конфиги):

| Искомое | Результат |
|---|---|
| `PaneId`, `DocumentId`, `ViewId`, `ViewProvider`, `DocumentStore` (и варианты `_`/`-`) | **Ни одного совпадения** |
| `Pane`/`pane`, `Document`, `View`, `Store`, `Registry` (в `.ts/.js/.css/.html/.rs/.md/.feature`) | **Ни одного совпадения.** Единственное «похожее» — `id="panes"` в `index.html:24` и `#panes { flex: 1; display: flex; min-height: 0; }` в `style.css:64` — это layout-контейнер двух панелей, а не абстракция Pane |
| `wasmi` | **Нет** |
| `WASM`/`wasm` | **Нет** |
| `JSON-RPC`/`jsonrpc`/`RPC` | **Нет** |
| `stdio` | **Нет** |
| `plugin` | Только Tauri-плагины: `Cargo.toml:19-20` (`tauri-plugin-dialog`, `tauri-plugin-opener`), `src-tauri/src/lib.rs:96-97` (`.plugin(...init())`), `package.json:17-18` (`@tauri-apps/plugin-*`), `capabilities/default.json` (`dialog:default`, `opener:default`), плюс автогенерированные `gen/schemas/*.json` и собранный бандл `app/dist/assets/index-*.js`. **Системы плагинов приложения нет.** |
