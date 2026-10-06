# TZ-H1-F4 — Фаза 4: тир-1 preview, реестр представлений и общий RenderIndex

> **Статус:** сделана (Фаза 4 закрыта). Исходный черновик в дереве доведён до зелёной сборки и e2e.
> **Родитель:** `tasks/TZ-H1.md` §2 (Фаза 4), §4. **Архитектура:** `docs/DESIGN_DOC.md` §5.3–§5.5, §9.4, §13.2.
> **Область:** только `crates/app/src/*` (TypeScript). `md-core` и `src-tauri` **не трогаем**.

---

## 0. Цель

Вынести предпросмотр в тир-1 контракт (`HtmlViewProvider`) через реестр представлений,
ввести `ViewContext`-фасад и **единый `RenderIndex`** на ревизию: `inspector` и `scrollsync`
читают один снимок вместо независимого построения карт/блоков. `preview.innerHTML` живёт
ровно в одном месте — `previewView`.

**Не делаем:** `layout`/`paneHost`/вторая панель (Фаза 5) · `linkController` (Фаза 5) ·
ребрендинг (ROADMAP п. 8) · правки `md-core`/`src-tauri`.

---

## 1. Текущее состояние (черновик в рабочем дереве)

Уже созданы/изменены (не мной; не затирать):

| Файл | Статус | Содержимое |
|---|---|---|
| `renderIndex.ts` | новый | `RenderIndex { current/set/clear }`, снимок `{ source, rev, maps, lineStarts, blocks }` |
| `viewRegistry.ts` | новый | `ViewContext`, `HtmlView`/`HtmlViewProvider`, `DomView`/`DomViewProvider`, `createViewRegistry()` |
| `previewView.ts` | новый | `previewViewProvider` (тир 1): `applyRender`, `invalidate`, `setMapped`, `refreshImages`, `render`, `dispose` |
| `inspector.ts` | изменён | принимает `index: RenderIndex`; `onRendered(md?)` → `onIndexChanged()` |
| `scrollsync.ts` | изменён | принимает `index`; `onRendered(md?)` → `onIndexChanged()` |
| `tables.ts` | изменён | `attachMenuAutoClose` возвращает `unsubscribe` (§5.6) |
| `main.ts` | изменён | собирает реестр + `ViewContext` + `previewView` + `renderIndex` |

**Сборка падала** (`npm run build`) — это и было предметом работы (исправлено, см. §3):

1. `viewRegistry.ts:103,108` — **TS7006**: у generic-методов `createHtmlView`/`createDomView`
   параметры (`kind`, `ctx`, `host`) теряют контекстный тип в объектном литерале → неявный `any`.
2. `main.ts:110` — **TS2345**: `createHtmlView<PreviewView>("preview", ctx, { previewEl, index, onIndexed })`
   — `opts` типизирован как `Json`, а встроенный на хосте провайдер (D2) передаёт `HTMLElement`/`RenderIndex`/callback.
3. `main.ts` вызывает у `docStore` методы, которых **нет** в `docStore.ts`:
   `snapshot()`, `renderText(text, mapped)`, `onDocument(cb)`.

---

## 2. Контракты (целевые)

### 2.1 `renderIndex.ts` (есть; сверить)

```ts
export interface RenderIndexSnapshot {
  readonly source: string;   // текст, ушедший в рендер (по нему посчитаны data-md)
  readonly rev: number;
  readonly maps: UnitMaps;
  readonly lineStarts: Int32Array;
  readonly blocks: Block[];
}
export interface RenderIndex {
  current(): RenderIndexSnapshot | null;        // null между правкой и рендером
  set(source: string, rev: number, root: ParentNode): void;
  clear(): void;
}
```
Инвариант: `set` вызывается `previewView` **сразу после** `preview.innerHTML = html`; `clear` — при
смене текста. Потребители читают `current()` и не строят карты сами.

### 2.2 `viewRegistry.ts` (есть; исправить типы)

- `ViewContext` (§5.4): `paneId`, `viewId`, `document()`, `edit(text)`, `render(text, mapped)`,
  `status(msg)`, `openExternal(url)`, `onDocument(cb)`.
- Тир-1 `HtmlView`/`HtmlViewProvider`; тир-2 `DomView`/`DomViewProvider`.
- `createViewRegistry()` — реестр `kind → provider`, `createHtmlView<T>`/`createDomView<T>`.

**Правка:** у методов `createHtmlView`/`createDomView` в объектном литерале задать явные типы
параметров (иначе неявный `any`). Тип `opts` — см. §2.3/§4.

### 2.3 `previewView.ts` (есть; сверить)

- Единственное место `preview.innerHTML = html`.
- `applyRender(res, source)`: при `res.changed` — `innerHTML` + `enhanceTables`; всегда —
  `resolveLocalImages` + `index.set(source, res.rev, previewEl)` + `onIndexed()`.
- `invalidate()` → `index.clear()` + `onIndexed()`.
- `setMapped`, `refreshImages`, `render(doc)` (тир-1 путь через `ctx.render`), `dispose()`
  (снять click + `detachMenuAutoClose`).
- **Решение по типам:** `createHtmlView` принимает `opts?: Json`; встроенному провайдеру нужны
  не-Json значения. Вариант — сделать `opts` дженериком `O = Json` и в `previewViewProvider`
  `createView(ctx, opts)` кастовать внутри (`opts as unknown as PreviewViewOptions`, как уже
  сделано), а в реестре типизировать `createHtmlView<T, O = Json>(kind, ctx, opts?: O)`. Тогда
  `main.ts` не будет ломаться на TS2345.

### 2.4 `docStore.ts` — расширить (ключевая доработка)

Добавить к текущему API (`state/subscribe/setText/flush/reload/newDocument/open/save`):

```ts
/** Снимок документа для ViewContext.document(): null, если документ не создан. */
snapshot(): DocumentSnapshot | null;

/** Подписка на снимок документа (для ViewContext.onDocument); возвращает unsubscribe. */
onDocument(cb: (d: DocumentSnapshot) => void): () => void;

/**
 * Рендер произвольного текста для тир-1 view (ViewContext.render).
 * Тонкая обёртка `update_document(id, text, mapped)`: возвращает RenderResult,
 * обновляет `rev`, но НЕ запускает основной конвейер (`onRender`), иначе preview
 * применит HTML дважды — результат применяет сам view.
 */
renderText(text: string, mapped: boolean): Promise<RenderResult>;
```

- `snapshot()`: `{ id, path, rev, text, dirty_hint: dirty }` или `null`.
- `onDocument`: вызывается на каждый `notify` с текущим снимком (не немедленно; текущий — через
  `snapshot()`).
- `renderText`: `ipcUpdate(id, text, mapped)`; при отсутствии `id` — `throw`. Конвейер дебаунса и
  `onRender` не трогает.

### 2.5 `main.ts` (есть; довести)

- `renderIndex` создаётся первым; `inspector`/`scrollsync` получают `index`.
- `store` с `onRender → previewView.applyRender`.
- `registry.registerHtml(previewViewProvider)`; `createHtmlView<PreviewView>("preview", ctx, {...})`.
- `ViewContext` (`makeContext()`) — единственный доступ view к хосту: `document/edit/render/status/
  openExternal/onDocument`.
- `onIndexed` → `inspector.onIndexChanged()` + `scrollSync.onIndexChanged()`.
- Подписка стора на смену текста → `previewView.invalidate()`.
- `toggleInspector` → `previewView.setMapped(...)` + `store.reload()`.
- `saveAs` → `previewView.refreshImages()`.

### 2.6 `editorView.ts` — вопрос регистрации

Сейчас `editorView` создаётся напрямую (`createEditorView`). Тир-2 `DomViewProvider` для редактора
в Фазе 4 **опционально**: интерфейс закладывается, но регистрация переносится в Фазу 5 (панели
решают, какой view активен). Решение по умолчанию — оставить прямое создание, отметить в ТЗ.

---

## 3. Что осталось сделать (delta)

- [x] `viewRegistry.ts`: явные типы параметров generic-методов (TS7006).
- [x] `viewRegistry.ts`/`previewView.ts`: типизация `opts` (generic `O=Json`) — TS2345 убрана,
      вызов в `main.ts` — `createHtmlView<PreviewView, PreviewViewOptions>(...)`.
- [x] `docStore.ts`: `snapshot()`, `onDocument()`, `renderText()` (уже были в черновике).
- [x] `main.ts`: сборка проходит; `preview.innerHTML` встречается **только** в `previewView.ts` (grep).
- [x] Завершающие переводы строк в `inspector.ts`/`scrollsync.ts`.
- [x] `editorView` как DomViewProvider — отложено в Фазу 5 (прямое создание сохранено, §2.6).
- [x] `npm run build` (24 модуля) + release + `npm run test:e2e` — **6 спеков** ✅.

**Результат:** Фаза 4 закрыта. Rust не менялся.

---

## 4. Ключевые решения

1. **`preview.innerHTML` — ровно одно место** (`previewView.applyRender`); `enhanceTables`/
   `resolveLocalImages`/`index.set` переехали туда же.
2. **Один `RenderIndex` на ревизию.** `inspector` и `scrollsync` больше не строят карты: читают
   `index.current()` по событию `onIndexChanged()`. Текст для индекса — `source` рендера, не буфер.
3. **`opts` контракта — `Json`**, но встроенному провайдеру (D2) разрешены живые зависимости
   (`previewEl`, `index`, колбэк). Оформляем generic-параметром, сохраняя JSON-форму для плагинов.
4. **`renderText` не дублирует применение.** `ViewContext.render` рендерит текст и возвращает
   `RenderResult`; применение — на стороне view (иначе двойной `innerHTML`).
5. **`mapped`**: основной (дебаунсный) путь по-прежнему берёт `renderMapped()` из `docStore`;
   `previewView.setMapped` — для тир-1 пути `render(doc)`. Дубль осознанный, сведём в Фазе 5.

---

## 5. Замороженные контракты (§13.2)

- DOM id/селекторы, debounce 120 мс, `ECHO_MS`, стартовый текст, `window.__errors` — не меняются.
- `helpers.js:setMarkdown` (установка `""`, затем документа) продолжает работать: смена текста →
  `invalidate()` (индекс `null`) → новый рендер → `set(...)`.
- toggle инспектора: `previewView.setMapped(true)` + `store.reload()` → mapped-рендер с `data-md`.

---

## 6. Гейт Фазы 4

```bash
cd crates/app && npm run build
npx tauri build --no-bundle && npm run test:e2e   # все 6 спеков
```

Rust не меняется; `cargo test -p md-core -p darmark` — опционально.

---

## 7. Риски

- **Порядок `index.set` и подписчиков.** `onIndexed()` вызывается после `set`/`clear`; иначе
  инспектор прочитает старый снимок.
- **Двойное применение HTML**, если `renderText` пойдёт через общий `onRender` — потому он его не
  трогает (§2.4).
- **TS-строгость** (`noUnusedLocals`, неявный `any`): правки типов в реестре обязательны, иначе
  сборка не проходит.
- **Чужой черновик.** В дереве уже есть незакоммиченные файлы Фазы 4 — реализацию вести поверх
  них, не пересоздавая с нуля.
