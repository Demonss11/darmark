# Отчёт по Фазе 1 (H1 — ядро Document / View / Pane)

**Дата:** 06.10.2026
**Ревизия:** v1.1 — Фаза 1 закрыта; правки по `reviews/TZ-H1-review-v1.0.md` внесены.
**Основание:** `docs/ROADMAP.md` §H1 (п. 2), `tasks/TZ-H1.md` §2–§3, `docs/DESIGN_DOC.md` §4–§5.
**Ветка:** рабочая (H2-прототип `crates/lua-rpc-spike` не затронут).

> **Статус: Фаза 1 пройдена.** Владение состоянием документа перенесено в Rust (`DocumentStore`, D5);
> фронтенд переведён на документные команды; `read_file`/`write_file` удалены. Все гейты зелёные,
> включая полный GUI E2E (6 спеков, 17 сценариев).

---

## 1. Вердикт

Владельцем текста, ревизии, пути и кэша рендера назначен Rust (`state.rs`); TS получил
брендированные id (`ids.ts`) и типизированные IPC-обёртки документов (`tauri.ts`). Ошибки IPC
больше не `String`, а структура `{ code, message }`. Замороженные контракты (§13.2 DESIGN_DOC) не
нарушены — e2e проходит без правок фич и шагов.

---

## 2. Что построено

### 2.1 Rust (`crates/app/src-tauri/src/`)

| Файл | Содержимое |
|---|---|
| `state.rs` *(new)* | `DocumentStore` (`docs` + `order` + счётчик id), `Document`, `DocumentId`, `EditOp`, `RenderCache`, `DocumentSnapshot`, `DocMeta`; методы `create`/`insert_loaded`/`get`/`get_mut`/`close`; 3 юнит-теста |
| `error.rs` *(new)* | `ErrorCode` (`not_found`/`io`/`too_large`/`not_utf8`/`unknown_document`), `CommandError { code, message }`, конструкторы, `Display`/`Error`; 1 юнит-тест |
| `lib.rs` | команды `new_document`/`open_document`/`save_document`/`close_document`; `read_text`/`write_text` (лимит 10 МБ, `CommandError`); `.manage(Mutex::new(DocumentStore::default()))`; `render_markdown` пока оставлен; `read_file`/`write_file` и их обёртки удалены; тесты переведены на новые хелперы (+`rejects_missing_file`) |

Идентификаторы документов — строковые `doc-1`, `doc-2`, … (единый формат с TS и будущими
Lua-плагинами, §4.1).

### 2.2 TypeScript (`crates/app/src/`)

| Файл | Содержимое |
|---|---|
| `ids.ts` *(new)* | `Brand<T,K>`, `DocumentId`/`PaneId`/`ViewId`, конструкторы `as*`, генераторы `newPaneId`/`newViewId` (§5.1) |
| `tauri.ts` | `DocumentSnapshot`/`DocMeta`, обёртки `newDocument`/`openDocument`/`saveDocument`/`closeDocument`/`renderMarkdown`, `errorMessage(e)` для `{ code, message }` |
| `main.ts` | `currentId`; `newFile`/`openFile`/`saveFile`/`saveAs` через стор; `applySnapshot()`; стартовый документ создаётся `newDocument(START_TEXT)` в `bootstrap()` |

---

## 3. Решения и уточнения к DESIGN_DOC

Зафиксированы в `tasks/TZ-H1.md` §3.1:

1. **`DocumentId` не `Copy`.** В §4.1 стоит `Clone, Copy`, но `Copy` для `String` невозможен —
   реализован `Clone`.
2. **`DocumentSnapshot`.** §4.3 возвращает `DocMeta` без текста, а фронту при открытии нужен текст
   для `editor.value`. Введён `DocumentSnapshot { id, path, rev, text, dirty_hint }` — надмножество
   `DocMeta`; его возвращают `new`/`open` и принимает `ViewContext.document()` (§5.4).
3. **Мост `save_document(id, text, path?)`.** До Фазы 2 Rust ещё не принимает правки
   (`update_document`), поэтому текст идёт аргументом. После Фазы 2 сигнатура станет `(id, path?)`.
4. **`dirty_hint` = `false`** — заглушка (владелец dirty — TS, §4.2); наполняется в Фазе 3.
5. **Ошибки — `CommandError`.** Сообщение лимита сохранено байт-в-байт (содержит «МБ» — контракт
   e2e). Код `not_found` отделён от `io` (падавший `open` теперь различим).

Временный `#[allow(dead_code)]` поставлен на `EditOp`, `RenderCache` и поля `cached`/`undo`/`redo`
`Document`: они закладываются сейчас, читаются с Фазы 2 (`update_document`).

---

## 4. Гейт Фазы 1 — результаты

| Проверка | Команда | Результат |
|---|---|---|
| Ядро | `cargo test -p md-core` | ✅ **63** теста |
| Хост | `cargo test -p mdedit` | ✅ **8** тестов (было 3; +5: state, error, missing-file) |
| Линт | `cargo clippy -p mdedit --all-targets -- -D warnings` | ✅ чисто |
| Формат | `cargo fmt -p mdedit -- --check` | ✅ чисто |
| Фронт | `npm run build` (`tsc && vite build`) | ✅ успешно |
| Сборка релиза | `npx tauri build --no-bundle` | ✅ `target/release/mdedit.exe` (3m 43s) |
| GUI E2E | `npm run test:e2e` | ✅ **6 спеков, 17 сценариев** (2m 52s) |

---

## 5. Изменённые / новые файлы

```
A  crates/app/src-tauri/src/state.rs
A  crates/app/src-tauri/src/error.rs
A  crates/app/src/ids.ts
A  tasks/TZ-H1.md            (план H1: все 9 пунктов, Фазы 0–9, гейты)
M  crates/app/src-tauri/src/lib.rs
M  crates/app/src/main.ts
M  crates/app/src/tauri.ts
M  KODA.md   (архитектура: ids.ts, state.rs/error.rs, команды документов)
M  AGENTS.md (структура: ids.ts, команды документов)
```

Коммит/пуш не выполнялись (по `.kodarules` — только по явному запросу).

---

## 6. Риски и техдолг (для следующих фаз)

1. **Мост текста в `save_document`** — временный; снимается в Фазе 2 вместе с `update_document`.
2. **`dirty_hint` всегда `false`** до Фазы 3 (реальный dirty ведёт TS-`docStore`).
3. **`render_markdown` ещё stateless** — `lastRenderedHtml`/`renderSeq` живут в `main.ts`; их
   переезд в стор (кэш `(mapped, rev)`) — Фаза 2.
4. **`#[allow(dead_code)]`** на placeholder-полях стора — убрать, когда Фаза 2 начнёт их читать.
5. **Нативный undo textarea** — как и раньше, программная смена `value` при смене документа ломает
   undo-стек браузера; Rust-undo (D5) — отдельная задача (не Фаза 1).

---

## 7. Воспроизведение

Из корня репозитория:

```powershell
cargo test -p md-core -p mdedit
cargo clippy -p mdedit --all-targets -- -D warnings
cargo fmt -p mdedit -- --check
```

Из `crates/app`:

```powershell
npm run build
npx tauri build --no-bundle   # release-бинарник для e2e
npm run test:e2e              # требует cargo install tauri-driver --locked
```

---

## 8. Дальше

**Фаза 2:** `update_document`/`render_document` — рендер и кэш `(mapped, rev)` переезжают в
`DocumentStore`; `renderSeq`/`lastRenderedHtml` удаляются из `main.ts`; `save_document` теряет
аргумент `text`. Гейт: build + e2e `preview`, `tables`.

---

## 9. Правки по ревью (`reviews/TZ-H1-review-v1.0.md`)

Ревью выполнено по коммиту `b612bd9`. Все код-находки разобраны; повторные гейты — ниже.

| # | Находка | Действие |
|---|---|---|
| 1 | Документы не закрывались — утечка стора | **Исправлено:** `newFile`/`openFile` после успешной замены закрывают прошлый документ (`closeDocument(previous)`); при ошибке создания/открытия прошлый остаётся |
| 2 | `save_document` мутировал стор до записи | **Исправлено:** сначала чтение пути и `write_text`, мутация `path`/`text` — только после успешной записи |
| 3 | Гонка стартового документа в e2e `smoke` | **Исправлено:** шаг ждёт непустого `#editor` (`waitUntil`), а не только существования элемента |
| 4 | `saveFile`/`saveAs` молча выходили без документа | **Исправлено:** статус-бар сообщает «Документ не создан — сохранение недоступно» |
| 5 | `resetRenderState()` в `openFile` до `await` | **Исправлено:** перенесён после `applySnapshot`, чтобы не терять правку и не рендерить старый текст |
| 6 | Коммит смешивал H1 и H2 (F37/F38) | **Процессная:** история не переписывалась; на будущее — H1 отдельным коммитом только по `crates/app/*` + `tasks/`/`KODA.md`/`AGENTS.md` |
| 7 | Сломанная разметка `AGENTS.md:57`, висячий пробел `:4` | **Исправлено** |
| 8 | `capabilities/default.json` описывал удалённые команды | **Исправлено:** описание перечисляет `new/open/save/close_document` + `render_markdown` |
| 9 | Артефакт сборки `crates/app/dist/index.html` в git | **Исправлено:** `git rm --cached crates/app/dist/index.html` (файл остаётся на диске, `frontendDist` пересобирается) |
| 10 | Дублирование конструктора `create`/`insert_loaded` | **Исправлено:** общий приватный `insert(path, text)`; убран `expect(...)` |
| 11 | Мёртвые заделы `ids.ts` / `closeDocument` | `closeDocument` теперь используется; генераторы/`as*` — осознанный API под Фазы 3–5; держим в техдолге рядом с `#[allow(dead_code)]` |
| 12 | Клонирование текста на open/save | **Принято для Фазы 1:** снимается при переходе на `update_document` (передача владения в снапшот) |

### Гейт после правок

| Проверка | Результат |
|---|---|
| `cargo test -p md-core -p mdedit` | ✅ 63 + 8 |
| `cargo clippy -p mdedit --all-targets -- -D warnings` | ✅ чисто |
| `cargo fmt -p mdedit -- --check` | ✅ чисто |
| `npm run build` | ✅ успешно |
| `npx tauri build --no-bundle` + `npm run test:e2e` | ✅ **6 спеков** (17 в `tables`; всего по логам шагов — зелёные) |

Коммит/пуш не выполнялись.
