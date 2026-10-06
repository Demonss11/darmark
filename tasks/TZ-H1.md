# TZ-H1 — ядро приложения darmark (Document / View / Pane)

> **Статус:** в работе (Фазы 0–1 сделаны; следующий шаг — Фаза 2).
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
- **Тексты:** стартовый документ `«Добро пожаловать в mdedit»` (переименование — п. 8 ROADMAP);
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
| 1 | ✅ Rust `state.rs`/`error.rs` + `new/open/save/close_document`; TS `ids.ts` + `tauri.ts`; `read_file`/`write_file` удалены | +220 / −60 | `cargo test -p md-core`, `cargo test -p mdedit`, `npm run build` |
| 2 | `update_document`/`render_document`; `renderSeq`/`lastRenderedHtml` удалены из `main.ts` | −25 / +40 | build + e2e `preview`, `tables` |
| 3 | TS `docStore.ts` (без DOM) + `editorView.ts`; `editor.value` перестаёт быть источником истины | +200 / −120 | build + e2e `smoke`, `inspector` |
| 4 | `viewRegistry.ts`, `ViewContext`, `previewView.ts` (тир 1); единый `RenderIndex` для inspector+scrollsync | +350 / −200 | build + **все** e2e |
| 5 | `layout.ts` + `paneHost.ts` (1–2 панели); `tables.ts` → экземпляр-контроллер; inspector/scrollsync → `linkController` | +400 / −150 | build + e2e + ручная 1↔2 |
| 6 | `main.ts` = композиционный корень (~120 строк); README «Архитектура» + ADR «контракт ViewProvider» | −150 | build |
| 8 | Переименование `mdedit` → `darmark` (D8): `tauri.conf.json`, `Cargo.toml`, `package.json`, `%APPDATA%`, e2e-фикстуры, стартовый текст | — | build + e2e |
| 9 | CI + size-gate (release-exe ≤ 6 МБ) | — | CI зелёный |

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
cargo test -p mdedit       # DocumentStore + IPC
npm run build              # tsc && vite build (strict)
```

Приёмка: приложение запускается, стартовый документ и предпросмотр на месте, открыть/сохранить
работают, e2e (`npm run test:e2e`) зелёный.
