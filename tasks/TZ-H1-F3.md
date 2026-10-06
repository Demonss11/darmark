# TZ-H1-F3 — Фаза 3: TS-проекция стора и редактор

> **Статус:** сделана (Фаза 3 закрыта).
> **Родитель:** `tasks/TZ-H1.md` §2 (Фаза 3), §4. **Архитектура:** `docs/DESIGN_DOC.md` §4.2, §5.4–§5.6.
> **Область:** только `crates/app/src/*` (TypeScript). `md-core` и `crates/app/src-tauri/*` **не трогаем**.

---

## 0. Цель и рамки

Вынести состояние документа на фронтенде в `docStore.ts` (**чистый, без DOM** — проекция Rust-стора,
подписки, dirty) и редактор — в `editorView.ts`. После фазы `editor.value` **перестаёт быть
источником истины**: заголовок, статус-бар и dirty читают проекцию стора.

**Не делаем:** `ViewContext`/`viewRegistry`/`previewView` (Фаза 4) · `layout`/`paneHost` (Фаза 5) ·
Rust-undo (D5, отдельная задача) · правки `md-core`/`src-tauri`.

---

## 1. Текущее состояние (после Фазы 2)

- `main.ts` держит `currentId`/`currentPath`/`dirty`/`lastRev`/`debounceTimer` и читает
  `editor.value` в `updateStatus`/`flushToStore`.
- Рендер-конвейер (`scheduleRender`/`doUpdate`/`doRender`/`applyRenderResult`/`flushToStore`) —
  целиком в `main.ts`.
- Rust-стор — владелец текста/ревизии/кэша, но на фронте документ де-факто = `editor.value`.

---

## 2. Целевая модель

### 2.1 `docStore.ts` — проекция Rust-стора (без DOM)

Состояние (проекция):

```ts
export interface DocState {
  id: DocumentId | null;
  path: string | null;
  rev: number;
  text: string;   // актуальный буфер редактора (синхронен вводу)
  dirty: boolean;
}
```

Инъекция зависимостей (никакого `document`/`window` внутри модуля):

```ts
export interface DocStoreOptions {
  renderMapped(): boolean;                           // инспектор активен? — читается в момент рендера
  onRender(res: RenderResult, source: string): void; // применить HTML/переиндексацию (main.ts → preview)
  onStatus(msg: string): void;                       // статус-бар/ошибки
}

export interface DocStore {
  state(): DocState;
  subscribe(cb: (s: DocState) => void): () => void;  // возвращает unsubscribe (§5.6)

  setText(text: string): void;      // в проекцию сразу + dirty=true + debounce 120 мс → update_document
  flush(): Promise<void>;           // отменить debounce и отправить текст в Rust (перед save); ошибку пробрасывает
  reload(): void;                   // render_document с текущим mapped (первый рендер, toggle инспектора)

  newDocument(text?: string): Promise<void>;  // text — стартовый документ bootstrap
  open(path: string): Promise<void>;
  save(path?: string): Promise<void>;
}

export function createDocStore(opts: DocStoreOptions): DocStore;
```

> `remoteText`/`close` в Фазе 3 не вводятся — не используются до Фазы 5 (мультипанель); добавим тогда.

Семантика:

- **`setText`** обновляет `text` синхронно (до IPC), ставит `dirty = true`, уведомляет подписчиков,
  планирует дебаунс 120 мс → `update_document(id, text, mapped)`. Ответ → `rev = res.rev`,
  `onRender(res, text)`. Ошибка → `onStatus`, `lastRev = -1` (сообщение об ошибке переживает
  следующий `changed:false` — Low-2 из ревью Фазы 2).
- **`flush`** = отмена дебаунса + немедленный `update_document` + `onRender`; ошибку **пробрасывает**
  (нужно для save). Дублирует безопасность: повторный `update_document` с тем же текстом даёт
  `rev` без изменений.
- **`reload`** = `render_document(id, mapped)`; ответ → `onRender`.
- **`newDocument`/`open`**: создают документ, закрывают прошлый (`closeDocument`), сбрасывают
  `lastRev`, `dirty = false`, уведомляют (editorView подхватит текст), затем `reload()`.
- **`save`**: `flush()` → `save_document(id, path?)` → `dirty = false`, уведомление.
- Внутри стора живут: `lastRev` (защита от гонки ответов IPC вместо `renderSeq`) и таймер дебаунса.

### 2.2 `editorView.ts` — тир-2 редактор (textarea, DOM)

```ts
export interface EditorView {
  focus(): void;
  dispose(): void;
}

export function createEditorView(
  host: HTMLTextAreaElement,   // #editor
  store: DocStore,
  onSelection: () => void      // статус-бар: курсор/размер
): EditorView;
```

- `input` → `store.setText(host.value)`.
- `keyup`/`click`/`select` → `onSelection()`.
- Подписка на стор: если `state.text !== host.value` (внешняя смена при open/new) — присвоить
  `host.value = state.text` **без** генерации `input`. Сравнение значений исключает цикл.
- `dispose()` снимает DOM-слушатели и отписку. Ограничение §5.6 (программная смена `value` ломает
  нативный undo) сохраняется как известный риск, не решается здесь.

### 2.3 `main.ts` — композиция

- Создаёт `store` с `renderMapped: () => inspector.isActive()`, `onRender: applyRenderResult`,
  `onStatus: flash`; создаёт `editorView`. `#editor` в `main.ts` больше не читается как документ.
- `updateTitle()`/`updateStatus()` читают `store.state()`; подписка `store.subscribe(...)`
  обновляет заголовок/`file-label`/`stat-size`; позиция курсора — из `onSelection`.
- Команды: `newFile → store.newDocument`, `openFile → store.open`, `saveFile`/`saveAs → store.save`.
- `applyRenderResult(res, source)` остаётся в `main.ts` (в Фазе 4 переедет в `previewView`); `source`
  приходит из стора, а не из DOM.
- Удалить из `main.ts`: `currentId`/`currentPath`/`dirty`/`lastRev`, `scheduleRender`/
  `resetRenderState`/`doUpdate`/`doRender`/`flushToStore`/`applySnapshot` (уходят в стор),
  прямые `editor.addEventListener`.

---

## 3. Ключевые решения

1. **Буфер редактора проецируется синхронно** (`setText`), дебаунсится только IPC-рендер — иначе
   `save` снова читал бы DOM (мост, снятый в Фазе 2, вернулся бы).
2. **Дебаунс — в сторе**, не в `editorView`: это документ-уровневая забота (render scheduling).
3. **`mapped` стор не «знает»** — получает через `renderMapped()` (инспектор остаётся UI).
4. **`onRender` — единственная точка применения HTML**. В Фазе 3 живёт в `main.ts`, в Фазе 4
   становится `HtmlView.render` (`previewView`), интерфейс не меняется.
5. **`dispose()`/unsubscribe обязательны** (§5.6) — закладываемся под несколько панелей (Фаза 5).

---

## 4. Поведение и замороженные контракты (§13.2 DESIGN_DOC)

- E2E правят `#editor` напрямую + `dispatchEvent(new Event("input"))` → `editorView` → `store.setText`
  → debounce 120 мс → `update_document` → `onRender` → `preview`. Контракты (id, debounce 120 мс,
  стартовый текст, `window.__errors`) сохраняются.
- `helpers.js:setMarkdown` (установка `""`, затем документа) продолжает работать: смена текста
  двигает `rev`.
- Toggle инспектора → `store.reload()` (`render_document`, смена `mapped`) → `changed: true`.
- `dirty` («●» в title/`file-label`) — из стора; open/new/save его чистят.

---

## 5. Гейт Фазы 3

```bash
cd crates/app && npm run build
npx tauri build --no-bundle && npm run test:e2e   # smoke, inspector (+ полный прогон 6 спеков)
```

Rust в этой фазе не меняется; `cargo test -p md-core -p mdedit` — опционально, для страховки.

---

## 6. Шаги (чек-лист)

- [x] `docStore.ts`: `DocState`, `subscribe`, `setText`/`flush`/`reload`,
      `newDocument`/`open`/`save`, дебаунс, `lastRev`.
- [x] `editorView.ts`: `input`/selection, синхронизация текста из стора, `dispose`.
- [x] `main.ts`: собрать `store` + `editorView`; перевести `updateTitle`/`updateStatus` и команды
      файла; `onRender`-sink; удалить старое состояние и конвейер.
- [x] Полный `npm run build` + e2e; обновить `KODA.md` (карта модулей) и статус `tasks/TZ-H1.md`.

**Результат:** `npm run build` ✅; release + `npm run test:e2e` — 6 спеков ✅. Rust не менялся.
`remoteText`/`close` не вводились (не нужны до Фазы 5).

---

## 7. Риски

- **Нативный undo textarea** при смене `value` (open/new) — как и раньше; чинится Rust-undo (D5),
  отдельная задача.
- **Цикл** store → editorView → store: защита сравнением `state.text !== host.value`.
- **Порядок ответов IPC** — `lastRev` в сторе.
- **`noUnusedLocals`** при удалении старого кода из `main.ts` — чистить импорты вместе с кодом.
