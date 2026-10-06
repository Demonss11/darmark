# TZ-H1 — ядро приложения darmark (Document / View / Pane)

> **Статус:** **H1 закрыт** (пп. 1–9 ROADMAP). Дальше — H2 (плагинная система).
> **Нормативная архитектура:** `docs/DESIGN_DOC.md` (разделы §3–§5, §13.2).
> **Дорожная карта:** `docs/ROADMAP.md` §H1. **Исходный разбор:** `ideas/ОПИСАНИЕ_ПЛАН.md`.
> **Область:** `crates/app/src-tauri/*` и `crates/app/src/*`. **`crates/md-core` не трогаем.**
>
> Перед любой правкой прочитать целиком `docs/DESIGN_DOC.md` §4, §5 и раздел «Замороженные E2E-контракты» (§13.2).

---

## 0. Цель и рамки

Перенести владение состоянием документа в Rust (`DocumentStore`, D5) и разложить монолитный
фронтенд на модули `Document / View / Pane`, не ломая ни одного замороженного контракта.
Плагинной системы (H2) в H1 нет.

**Не делаем в H1:** вкладки и UI нескольких документов (стор держит N, UI — 1) · `mlua`/Lua ·
JSON-RPC/stdio · wasm-таргеты · drag-resize разделителя · новые npm/cargo-зависимости ·
перенос таблиц/парсинга в ядро · любые правки `md-core`.

---

## 1. Замороженные контракты (нельзя ломать)

Полный список — `docs/DESIGN_DOC.md` §13.2. Ключевое:

- **DOM id:** `editor`, `preview`, `btn-inspect`, `chk-sync`, `chk-preview`, `btn-new`, `btn-open`,
  `btn-save`, `btn-save-as`, `file-label`, `stat-pos`, `stat-size`, `stat-msg`, `stat-inspect`,
  `toolbar`, `panes`, `statusbar`, `toggle-sync`, `toggle-preview`, `app`.
- **Селекторы:** `#preview .md-block[data-md]`, `.table-enhanced*`, `.inspect-*`, `tr.inspect-row`.
- **Тексты:** стартовый документ `«Добро пожаловать в darmark»`;
  заголовки демо-таблицы `Файл | Размер | Строк | Изменён`.
- **Константы:** debounce рендера **120 мс**, `ECHO_MS = 100`, throttle инспектора 100 мс.
- **Глобальные:** `window.__xss`, `window.__errors` / `window.__errorCapture`.
- **Ошибка лимита файла** содержит подстроку `«МБ»` (сообщение сохраняем байт-в-байт).
- При двух панелях id остаются на **первичной** панели, добавляются `data-pane` / `data-view`.

---

## 2. Фазы H1

| # | Шаг | Срез | Гейт |
|---|---|---|---|
| 0 | Контракты (§1) + `DESIGN_DOC` | докс | — |
| 1 | ✅ Rust `state.rs`/`error.rs` + `new/open/save/close_document`; TS `ids.ts` + `tauri.ts`; `read_file`/`write_file` удалены | +220 / −60 | `cargo test -p md-core`, `cargo test -p darmark`, `npm run build` |
| 2 | ✅ `update_document`/`render_document`; `renderSeq`/`lastRenderedHtml` удалены из `main.ts` | −25 / +40 | build + e2e `preview`, `tables` |
| 3 | ✅ TS `docStore.ts` (без DOM) + `editorView.ts`; `editor.value` перестаёт быть источником истины — детали в `tasks/TZ-H1-F3.md` | +200 / −120 | build + e2e `smoke`, `inspector` |
| 4 | ✅ `viewRegistry.ts`, `ViewContext`-фасад, `previewView.ts` (тир 1); единый `RenderIndex` для inspector+scrollsync — детали в `tasks/TZ-H1-F4.md` | +350 / −200 | build + **все** e2e |
| 5 | ✅ `layout.ts` + `paneHost.ts` (1–2 панели); `chk-preview` → видимость панели; `tables.ts` → экземпляр-контроллер; inspector/scrollsync → `linkController`; каркас панелей из `front_idea5` — детали в `tasks/TZ-H1-F5.md` | +400 / −150 | build + e2e + ручная 1↔2 |
| 6 | ✅ `main.ts` = композиционный корень; модули `statusBar`/`fileActions`/`shell`/`sampleDocument`; README «Архитектура» + ADR-0022 «Контракт ViewProvider» | −150 | build |
| 8 | ✅ Переименование `mdedit` → `darmark` (D8): `tauri.conf.json` (identifier `dev.darmark.app`, productName), `Cargo.toml`/lib, `package.json`, e2e-фикстуры, стартовый текст, доки. Каталог `%APPDATA%/darmark/` — при появлении SettingsStore (H2) | — | build + e2e |
| 9 | ✅ CI + size-gate: `.github/workflows/ci.yml` (fmt/clippy/tests/frontend + release-exe ≤ 6 МБ) | — | CI зелёный |

**Гейт каждой фазы:** рабочее приложение + `cargo test` + `npm run build` + соответствующие e2e.

---

## 3. Фаза 1 — детально (текущий шаг)

### 3.1 Решения и уточнения к DESIGN_DOC

1. **`DocumentId` — `String`, `#[serde(transparent)]`.** `Copy` в §4.1 невозможен для `String`;
   реализуем `Clone`, не `Copy`. Идентификаторы генерирует Rust (`doc-1`, `doc-2`, …).
2. **`DocumentSnapshot`.** §4.3 возвращает `DocMeta` без текста, но фронтенду при открытии нужен
   текст для `editor.value`. Вводим `DocumentSnapshot { id, path, rev, text, dirty_hint }` —
   надмножество `DocMeta`; его возвращают `new_document`/`open_document` и принимает
   `ViewContext.document()` (§5.4). `save_document` возвращает `DocMeta`.
3. **Мост `save_document(id, text, path?)`.** До Фазы 2 Rust ещё не принимает правки
   (`update_document`), поэтому текст при сохранении приходит аргументом. После Фазы 2 сигнатура
   станет `(id, path?)`, а владельцем текста станет стор.
4. **`dirty_hint` в Rust** в Фазе 1 — заглушка `false` (владелец dirty — TS, §4.2); наполняется
   в Фазе 3.
5. **Ошибки IPC** — `CommandError { code, message }` вместо `String`. Коды: `not_found`, `io`,
   `too_large`, `not_utf8`, `unknown_document`. `message` сохраняем (e2e-контракт «МБ»).

### 3.2 Rust (`crates/app/src-tauri/src/`)

- **`state.rs`** (новый): `DocumentId`, `EditOp`, `RenderCache`, `Document`, `DocumentSnapshot`,
  `DocMeta`, `DocumentStore` (map + order + счётчик id). Юнит-тесты стора.
- **`error.rs`** (новый): `ErrorCode`, `CommandError`, конструкторы, `Display`/`Error`.
- **`lib.rs`**: команды `new_document`, `open_document`, `save_document`, `close_document`;
  `render_markdown` пока остаётся (уйдёт в Фазе 2). `read_file`/`write_file` и их обёртки
  удаляются. `app.manage(Mutex::new(DocumentStore::default()))`. Файловый ввод-вывод —
  `read_text`/`write_text` с лимитом 10 МБ и `CommandError`.

### 3.3 TS (`crates/app/src/`)

- **`ids.ts`** (новый): брендированные `DocumentId`/`PaneId`/`ViewId` + конструкторы/генераторы.
- **`tauri.ts`**: типизованные обёртки `newDocument`/`openDocument`/`saveDocument`/`closeDocument`/
  `renderMarkdown`; типы `DocumentSnapshot`/`DocMeta`; `errorMessage(e)` для `{ code, message }`.
- **`main.ts`**: `currentPath` → `currentPath` + `currentId`; `newFile`/`openFile`/`saveFile`/
  `saveAs` ходят через стор; стартовый документ создаётся `newDocument(START_TEXT)`.

### 3.4 Гейт Фазы 1

```bash
cargo test -p md-core      # ядро не тронуто, тесты зелёные
cargo test -p darmark       # DocumentStore + IPC
npm run build              # tsc && vite build (strict)
```

Приёмка: приложение запускается, стартовый документ и предпросмотр на месте, открыть/сохранить
работают, e2e (`npm run test:e2e`) зелёный.

---

## 4. Фаза 2 — детально (план)

**Цель:** перенести владение кэшем рендера в Rust. Команды `update_document`/`render_document`
заменяют stateless `render_markdown`; `RenderResult.changed` заменяет TS-сравнение
`lastRenderedHtml`, а монотонная `rev` — счётчик `renderSeq`. Из `main.ts` исчезают
`lastRenderedHtml` и `renderSeq`.

### 4.1 Семантика `RenderResult`

```rust
pub struct RenderResult { pub html: String, pub rev: u64, pub changed: bool }
```

- **`rev`** — ревизия документа, инкремент **только при реальной смене текста** (`update_document`
  сравнивает переданный текст с `doc.text`). `render_document` (текст не меняет) `rev` не трогает.
- **`changed`** — изменился ли HTML относительно **последнего отданного** клиенту. Это переезд
  `lastRenderedHtml` в Rust: `changed = last_html != new_html`. `changed: false` → TS **не трогает
  DOM** (иначе сбросились бы сортировка/фильтры/фокус в таблицах на каждом дебаунсе).
- Кэш `RenderCache { mapped, rev, html }` хранит последний результат. Если `(mapped, rev)`
  совпали — рендер не запускается, `changed: false`, отдаётся кэш.

Алгоритм (`Document::render(&mut self, mapped) -> RenderResult`, в `state.rs`):

1. если `cached.mapped == mapped && cached.rev == self.rev` → вернуть кэш, `changed: false`;
2. иначе `html = if mapped { to_html_mapped } else { to_html }`;
3. `changed = cached.map_or(true, |c| c.html != html)`;
4. `cached = { mapped, rev, html.clone() }`; вернуть `{ html, rev, changed }`.

### 4.2 Rust (`crates/app/src-tauri/src/`)

- **`state.rs`**: `RenderResult` (Serialize); `Document::render(mapped)`; методы стора
  `update(&mut self, id, text, mapped) -> Option<RenderResult>` (при смене текста `rev += 1`) и
  `render(&mut self, id, mapped) -> Option<RenderResult>`. `md_core` вызывается здесь — `state.rs`
  живёт в шелле, а не в ядре, слой не нарушается. Юнит-тесты на `changed`/`rev`.
- **`lib.rs`**: команды `update_document(id, text, mapped?) -> RenderResult` и
  `render_document(id, mapped?) -> RenderResult` (обе возвращают `CommandError::unknown_document`
  на чужой id); **`render_markdown` удаляется**; из `generate_handler!` — тоже.
- **`save_document` теряет мост `text`** → `save_document(id, path?)`: пишет текст **из стора**.
  Чтобы избежать отставания стора от редактора (дебаунс 120 мс), `saveFile`/`saveAs` перед
  сохранением вызывают `update_document` (flush, §4.4). Требование §3.1 (снять мост после Фазы 2)
  закрывается здесь.

### 4.3 TS (`crates/app/src/`)

- **`tauri.ts`**: тип `RenderResult { html, rev, changed }`; обёртки `updateDocument(id, text, mapped)`,
  `renderDocument(id, mapped)`; **`renderMarkdown` удаляется**; `saveDocument(id, path?)`.
- **`main.ts`**:
  - удалить `let renderSeq` и `let lastRenderedHtml`;
  - добавить `let lastRev = -1` (защита от устаревших ответов IPC вместо `renderSeq`);
  - `doRender()` → `render_document`: читает `mapped = inspector.isActive()`, применяет результат;
  - `doUpdate()` → `update_document(id, editor.value, mapped)`, вызывается из debounce 120 мс;
  - общий `applyRenderResult(res, source)`: если `res.rev < lastRev` — отбросить (устаревший);
    иначе `lastRev = res.rev`; при `res.changed` — `preview.innerHTML = res.html` + `enhanceTables`;
    всегда — `resolveLocalImages`, `inspector.onRendered(source)`, `scrollSync.onRendered(source)`
    (как в старом `doRender`, где DOM трогался условно, а индексация — всегда);
  - `source` — **текст, ушедший в рендер** (не текущий `editor.value`): `data-md`-смещения должны
    соответствовать именно отрендеренному HTML;
  - `resetRenderState()` → сброс `debounceTimer` и `lastRev = -1` (вместо `renderSeq++` и
    `lastRenderedHtml = ""`);
  - `toggleInspector()`: убрать `lastRenderedHtml = ""`; вызвать `void doRender()` (`render_document`
    сам увидит смену `mapped` и вернёт `changed: true`);
  - `newFile`/`openFile`/`bootstrap`: после `applySnapshot` — `void doRender()` (текст уже в сторе);
  - `saveFile`/`saveAs`: перед `saveDocument` — `await updateDocument(currentId, editor.value,
    inspector.isActive())` (flush текста в стор).

### 4.4 Порядок работ (чек-лист)

- [x] `state.rs`: `RenderResult`, `Document::render`, `DocumentStore::update/render` + тесты
      (`changed` при смене HTML; `changed: false` на повторном `render`; `rev` инкремент только при
      смене текста).
- [x] `lib.rs`: `update_document`/`render_document`, удалить `render_markdown`, `save_document(id, path?)`.
- [x] `tauri.ts`: `RenderResult`, `updateDocument`/`renderDocument`, `saveDocument(id, path?)`.
- [x] `main.ts`: удалить `renderSeq`/`lastRenderedHtml`, ввести `lastRev` + `applyRenderResult`,
      перевести `doRender`/debounce/`toggleInspector`/команды файла.
- [x] `saveFile`/`saveAs`: flush через `update_document`.
- [x] Обновить упоминание `render_markdown` в комментарии e2e-фич и в `KODA.md`/`AGENTS.md`/`README.md`.

**Статус:** Фаза 2 закрыта. Гейт: md-core 63, darmark 11 (было 8, +3), clippy/fmt чисто,
`npm run build` ✅, release + e2e — 6 спеков ✅.

### 4.5 E2E и контракты

- `helpers.js:setMarkdown` устанавливает `""`, затем документ — это по-прежнему проходит: смена
  текста двигает `rev`, кэш не совпадает, `changed: true`, DOM таблиц перестраивается.
- `setInspectorActive` жмёт `#btn-inspect` → `render_document` со сменой `mapped` → `changed: true`
  → появляются `.md-block[data-md]`. Контракты §13.2 (id, debounce 120 мс, `ECHO_MS`,
  стартовый текст, `window.__errors`) не меняются.
- Проверяемые e2e-наборы: `smoke`, `preview`, `tables`, `inspector`, `inspect-tables`, `scrollsync`.

### 4.6 Гейт Фазы 2

```bash
cargo test -p md-core -p darmark
cargo clippy -p darmark --all-targets -- -D warnings
cargo fmt -p darmark -- --check
cd crates/app && npm run build
npx tauri build --no-bundle && npm run test:e2e   # все 6 спеков
```

Приёмка: приложение работает; при неизменном тексте DOM не перерисовывается (таблицы сохраняют
сортировку/фильтры); инспектор и синхронная прокрутка по-прежнему зелёные; `render_markdown`
отсутствует.

### 4.7 Риски

- **Порядок ответов IPC.** Синхронные команды сериализуются мьютексом, но ответы могут прийти не в
  порядке отправки — защита `res.rev < lastRev`. Если этого мало, добавить сравнение по id документа.
- **`source` для inspector/scrollsync.** Передавать именно отрендеренный текст, иначе `data-md`
  разъедется с HTML (риск из `ОПИСАНИЕ_ФРОНТ.md`).
- **Клонирование html.** `RenderCache` + возврат дают клон большого HTML; для 10 МБ заметно.
  Приемлемо для Фазы 2; оптимизация (владение/`Arc`/`Bytes`) — по факту, отдельным шагом.
- **Flush перед save.** `update_document` возвращает `RenderResult`; если рендер упадёт, сохранение
  прервётся с сообщением — это допустимо и лучше тихой записи устаревшего текста.
