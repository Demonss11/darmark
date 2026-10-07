# Карта фронтенда mdedit

## 1. Обзор модулей

### `main.ts` — 318 строк
**Ответственность:** точка входа, инициализация, управление файловой моделью (new/open/save/saveAs), event-хуки, статус-бар, заголовок окна.

**Глобальные переменные (top-level `let`/`const`):**

| Имя | Тип | Строка |
|-----|-----|--------|
| `editor` | `HTMLTextAreaElement` | 19 |
| `preview` | `HTMLElement` | 20 |
| `fileLabel` | `HTMLElement` | 21 |
| `statPos` | `HTMLElement` | 22 |
| `statSize` | `HTMLElement` | 23 |
| `statMsg` | `HTMLElement` | 24 |
| `statInspect` | `HTMLElement` | 25 |
| `btnInspect` | `HTMLButtonElement` | 26 |
| `chkPreview` | `HTMLInputElement` | 27 |
| `chkSync` | `HTMLInputElement` | 28 |
| `scrollSync` | `ScrollSync` (интерфейс) | 30 |
| `inspector` | `Inspector` (интерфейс) | 32 |
| `currentPath` | `string \| null` | 40 |
| `dirty` | `boolean` | 41 |
| `renderSeq` | `number` | 42 |
| `lastRenderedHtml` | `string` | 43 |
| `debounceTimer` | `number` (handle таймера) | 47 |
| `appWindow` | `Window` (Tauri API) | 278 |

**Экспортирует:** ничего.

**Вызывает:**
- `createScrollSync({ editor, preview })` → строка 30
- `createInspector({ editor, preview, statusEl, beforeScrollIntoView })` → строка 32
- `renderMarkdown(source, inspector.isActive())` → строка 68 (через `tauri.ts`)
- `enhanceTables(preview)` → строка 75
- `resolveLocalImages(preview, currentPath)` → строки 84, 216
- `inspector.onRendered(source)` → строки 88, 163
- `scrollSync.onRendered(source)` → строки 92, 164
- `inspector.enable()/disable()` → строка 100
- `inspector.isActive()` → строки 68, 101

**Зависимости:** `tauri.ts` → `tables.ts` → `inspector.ts` ← `mapping.ts` ← `scrollsync.ts` ← `mapping.ts` → `images.ts` → `style.css`

**Циклических зависимостей нет.**

---

### `tauri.ts` — 16 строк
**Ответственность:** тонкие обёртки над `invoke()` Tauri IPC.

**Экспортирует 3 функции:**

```typescript
export function renderMarkdown(markdown: string, mapped = false): Promise<string>
// Команда Rust: "render_markdown"
// Аргументы: { markdown: string, mapped: boolean }
// Возврат: HTML-строка

export function readFile(path: string): Promise<string>
// Команда Rust: "read_file"
// Аргументы: { path: string }
// Возврат: содержимое файла как строка

export function writeFile(path: string, contents: string): Promise<void>
// Команда Rust: "write_file"
// Аргументы: { path: string, contents: string }
// Возврат: void
```

**Обработка ошибок:** ошибки не перехватываются на уровне обёрток — промисы.reject-ятся напрямую. Вызывающий код (`main.ts`) оборачивает вызовы в `try/catch` или `.catch()`.

**Сериализуемые типы, общие с Rust:** `renderMarkdown` принимает `mapped: boolean` (по умолчанию `false`), который передаётся как `Option<bool>` в Rust и переключает между `md_core::to_html()` и `md_core::to_html_mapped()`.

---

### `mapping.ts` — 144 строки
**Ответственность:** чистая логика конвертации байтовых смещений UTF-8 ↔ UTF-16 code units, поиск блоков по смещению. Не зависит от DOM (кроме `collectBlocks`/`parseRange`, которые читают `data-md` атрибуты).

**Экспортирует:**

```typescript
export interface Block {
  start: number;  // байтовый offset (inclusive)
  end: number;    // байтовый offset (exclusive)
  el: HTMLElement;
}

export interface UnitMaps {
  b2u: Int32Array;  // байтовый индекс → UTF-16 code units
  u2b: Int32Array;  // UTF-16 code units → байтовый индекс
}

export function buildUnitMaps(text: string): UnitMaps
export function bytesToUnits(maps: UnitMaps, b: number): number
export function unitsToBytes(maps: UnitMaps, u: number): number
export function buildLineStarts(text: string): Int32Array
export function findLineAt(lineStarts: Int32Array, unit: number): number
export function parseRange(el: HTMLElement): { start: number; end: number } | null
export function collectBlocks(root: ParentNode): Block[]
export function findBlock(blocks: Block[] | null, bytePos: number): Block | null
export function findBlockContaining(blocks: Block[], bytePos: number): Block | null
export function nextBlockAfter(blocks: Block[], bytePos: number): Block | null
```

**Вызывают:** `inspector.ts` (строки 12–21), `scrollsync.ts` (строки 22–33).

---

### `scrollsync.ts` — 270 строк
**Ответственность:** синхронизация вертикальной прокрутки редактора ↔ предпросмотра. Два режима: анкорная привязка (когда есть `data-md` блоки) и пропорциональная (fallback).

**Экспортирует интерфейс:**

```typescript
export interface ScrollSync {
  isEnabled(): boolean;
  setEnabled(enabled: boolean): void;
  onRendered(markdown?: string): void;
  suspend(): void;
}
```

**Экспортирует функцию-фабрику:**

```typescript
export function createScrollSync(opts: {
  editor: HTMLTextAreaElement;
  preview: HTMLElement;
}): ScrollSync
```

**Внутреннее состояние:**
- `enabled: boolean` — включена ли синхронизация
- `syncing: boolean` — флаг защиты от обратной связи
- `blocks: Block[] | null` — блоки из DOM (анкоры)
- `maps: UnitMaps | null` — byte↔UTF-16 карты
- `lineStarts: Int32Array | null` — начала логических строк
- `lineH: number` — высота строки в px
- `previewPadTop: number` — верхний padding preview
- `lastWriteAt: number`, `lastWriteTarget: HTMLElement | null` — защита от эха WebView2

**Зависимости:** `mapping.ts` (все экспорты кроме `findBlock`).

---

### `inspector.ts` — 310 строк
**Ответственность:** режим инспектора — двусторонняя подсветка «блок предпросмотра ↔ фрагмент исходника». Подсветка блоков, строк, столбцов таблиц.

**Экспортирует интерфейс:**

```typescript
export interface Inspector {
  isActive(): boolean;
  enable(): void;
  disable(): void;
  onRendered(markdown?: string): void;
  onEditorActivity(): void;
}
```

**Экспортирует функцию-фабрику:**

```typescript
export function createInspector(opts: {
  editor: HTMLTextAreaElement;
  preview: HTMLElement;
  statusEl: HTMLElement;
  beforeScrollIntoView?: () => void;
}): Inspector
```

**Внутреннее состояние:**
- `active: boolean` — режим активен
- `blocks: Block[] | null` — блоки из DOM
- `maps: UnitMaps | null` — byte↔UTF-16 карты
- `activeBlockEl: HTMLElement | null` — подсвеченный блок
- `bandEls: HTMLElement[]` — элементы с транзиентными классами inspect-col/row
- `lastHover: Element | null`, `lastHoverShift: boolean` — для пересчёта при Shift
- `programmaticSel: { start: number; end: number }` — программное выделение (защита от перебивания)
- `savedSelection`, `savedScrollTop`, `savedFocus` — состояние редактора до включения режима

**Типы подсветки:** `InspectMode = "block" | "cell" | "row" | "col"`

**Зависимости:** `mapping.ts` (все экспорты).

---

### `tables.ts` — 422 строки
**Ответственность:** Excel-подобное поведение таблиц в preview: сортировка по клику на заголовок, фильтры по значениям колонок, глобальный поиск.

**Экспортирует:**

```typescript
export function enhanceTables(root: ParentNode): void
export function attachMenuAutoClose(scroller: HTMLElement): void
```

**Внутреннее состояние:**
- `registry: WeakMap<HTMLTableElement, TableEntry>` — состояние каждой украшенной таблицы
- `openMenu: HTMLElement | null` — открытое меню фильтра (глобальная переменная)
- `outsideHandler: ((e: MouseEvent) => void) | null` — обработчик закрытия меню
- `anchorSeq: number` — счётчик уникальных anchor-id для меню фильтров

**Интерфейсы состояния:**

```typescript
interface TableState {
  sortCol: number | null;
  sortDir: 1 | -1;
  global: string;
  colFilters: Map<number, Set<string>>;
}

interface TableEntry {
  wrap: HTMLElement;
  table: HTMLTableElement;
  state: TableState;
  baseOrder: HTMLTableRowElement[];
  kinds: ("num" | "date" | "str")[];
}
```

**Меняет preview.innerHTML напрямую:** НЕТ. Не вызывает `preview.innerHTML = ...`. Работает с DOM-элементами через `insertBefore`, `appendChild`, `classList`, `hidden`, `textContent`.

---

### `images.ts` — 72 строки
**Ответственность:** замена относительных `src` картинок на asset-URL Tauri (`convertFileSrc`).

**Экспортирует:**

```typescript
export function resolveLocalImages(root: HTMLElement, docPath: string | null): void
```

**Внутреннее состояние:** нет. Чистая функция.

---

### `style.css` — 340 строк
**Ответственность:** стилизация всего UI. CSS-переменные `:root` для light/dark тем.

**CSS-переменные `:root` (light):**
- `--bg: #ffffff`, `--fg: #1f2328`, `--border: #d6dae0`
- `--toolbar-bg: #f3f5f7`, `--accent: #0b57d0`, `--code-bg: #f3f4f6`

**Dark (через `@media (prefers-color-scheme: dark)`):**
- `--bg: #17191c`, `--fg: #e6e9ef`, `--border: #3a3f46`
- `--toolbar-bg: #212429`, `--accent: #6ea8fe`, `--code-bg: #262a30`

**Селекторы layout:** `#app` (flex column, 100vh), `#toolbar` (flex row), `#panes` (flex 1, flex row), `#editor` (flex 1), `#preview` (flex 1), `#statusbar` (flex row).

---

## 2. main.ts — полная картина

### Инициализация (порядок вызовов, строки):

1. **Строка 19–28:** получение DOM-элементов (`editor`, `preview`, `fileLabel`, `statPos`, `statSize`, `statMsg`, `statInspect`, `btnInspect`, `chkPreview`, `chkSync`)
2. **Строка 30:** `scrollSync = createScrollSync({ editor, preview })` — создаёт синхронизацию, подписывается на `scroll` события
3. **Строка 32–38:** `inspector = createInspector({ ... })` — создаёт инспектор, передаёт `beforeScrollIntoView: () => scrollSync.suspend()`
4. **Строка 40–43:** инициализация состояния: `currentPath = null`, `dirty = false`, `renderSeq = 0`, `lastRenderedHtml = ""`
5. **Строка 104:** `attachMenuAutoClose(preview)` — закрытие меню фильтров при прокрутке
6. **Строка 113:** `preview.addEventListener("click", ...)` — перехват внешних ссылок `http(s)://`
7. **Строка 225–229:** event listeners на кнопки toolbar (new, open, save, save-as, inspect)
8. **Строка 231–239:** `editor.addEventListener("input", ...)` — dirty flag, scheduleRender, сброс подсветки
9. **Строка 240–243:** `editor.addEventListener("keyup", "click", "select")` — updateStatus + inspector.onEditorActivity
10. **Строка 246–247:** `chkPreview.addEventListener("change", ...)` — toggle preview display
11. **Строка 250–251:** `chkSync.addEventListener("change", ...)` — toggle scroll sync
12. **Строка 254–275:** `window.addEventListener("keydown", ...)` — глобальные хоткеи (Ctrl+N/O/S/Shift+S/P/I, Esc)
13. **Строка 278–293:** `appWindow.onCloseRequested(...)` — подтверждение при закрытии с несохранёнными изменениями
14. **Строка 296–316:** стартовый документ — `editor.value = "..."` (демо-текст с таблицей)
15. **Строка 317:** `void doRender()` — первый рендер
16. **Строка 318–319:** `updateStatus()`, `updateTitle()`, `editor.focus()`

### Таймеры:

| Таймер | Строка | Описание |
|--------|--------|----------|
| `debounceTimer` (setTimeout) | 50 | Дебаунс рендера: 120мс после ввода |
| `setTimeout` в `flash()` | 147 | Очистка сообщения статус-бара через 4000мс |

### Debounce:

```typescript
// Строка 48–51
let debounceTimer = 0;
function scheduleRender() {
  clearTimeout(debounceTimer);
  debounceTimer = window.setTimeout(doRender, 120);
}
```

Сброс при смене документа (строка 55–60):
```typescript
function resetRenderState() {
  clearTimeout(debounceTimer);
  renderSeq++;
  lastRenderedHtml = "";
  scrollSync.onRendered(); // анкоры предыдущего документа устарели
}
```

---

## 3. Поток данных рендера: нажатие клавиши → preview.innerHTML

**Цепочка:**

```
editor "input" event (строка 231)
  → dirty = true (232)
  → updateTitle() (233)
  → updateStatus() (234)
  → inspector.onRendered() (235) — сброс подсветки
  → scrollSync.onRendered() (236) — сброс анкоров
  → scheduleRender() (237) — debounce 120мс
    → setTimeout → doRender() (строка 62)
```

**doRender() (строки 62–96):**

```typescript
async function doRender() {
  const seq = ++renderSeq;                    // строка 63: инкремент счётчика
  const source = editor.value;                // строка 65: фиксируем текст
  try {
    const html = await renderMarkdown(source, inspector.isActive()); // строка 68: IPC в Rust
    if (seq !== renderSeq) return;            // строка 70: защита от гонки — если новый рендер уже в полёте, выходим
    if (html !== lastRenderedHtml) {          // строка 72: не перетирать DOM без изменений
      lastRenderedHtml = html;                // строка 73
      preview.innerHTML = html;               // строка 74: ТОЧКА ВСТАВКИ HTML
      try {
        enhanceTables(preview);               // строка 75: Excel-поведение таблиц
      } catch (e) { ... }
    }
    resolveLocalImages(preview, currentPath); // строка 84: asset-URL картинок
    inspector.onRendered(source);             // строка 88: переиндексация блоков
    scrollSync.onRendered(source);            // строка 92: обновление анкорных карт
  } catch (e) {
    if (seq === renderSeq) {
      lastRenderedHtml = "";
      preview.textContent = `Ошибка рендера: ${String(e)}`; // строка 94
    }
  }
}
```

**Защита от гонок:**
1. **`renderSeq`** (строка 42, 63, 70): каждый вызов `doRender` инкрементирует счётчик. Если асинхронный рендер вернулся с устаревшим seq — игнорируется.
2. **`lastRenderedHtml`** (строка 43, 72–73): если HTML не изменился — DOM не трогается, чтобы не сбрасывать состояние таблиц (сортировку, фильтры, фокус).
3. **`resetRenderState()`** (строка 55–60): при смене документа сбрасывает `debounceTimer`, инкрементирует `renderSeq`, обнуляет `lastRenderedHtml`.

---

## 4. Понятия «документ», «панель», «view», id файлов

**Документ:**
- `currentPath: string | null` (строка 40) — путь к текущему файлу. `null` = «безымянный».
- `dirty: boolean` (строка 41) — флаг несохранённых изменений.
- **Никакого `DocumentStore` нет.** Состояние документа (`currentPath`, `dirty`, `editor.value`) — это просто top-level переменные в `main.ts`.

**Панели / view:**
- Понятия «панель» как сущности нет. Есть два фиксированных DOM-элемента: `#editor` (textarea) и `#preview` (article).
- Нет концепции вкладок, нескольких файлов одновременно, ID панелей.
- `chkPreview.checked` (строка 247) — единственное «переключение вида»: скрывает/показывает preview через `style.display = "none"`.

**Хранение пути:**
- `currentPath` — просто строка или null.
- `baseName()` (строка 134–136) — извлекает имя файла из пути для отображения.
- `fileLabel.textContent` (строка 142) — показывает имя + `●` если dirty.
- `getCurrentWindow().setTitle()` (строка 141) — заголовок окна.

**Несколько файлов:** НЕ поддерживается. Модель — один файл за раз, как Notepad++.

---

## 5. scrollsync + mapping: связь редактор ↔ preview

### Данные маппинга из Rust

Rust-команда `render_markdown` с аргументом `mapped: true` (когда `inspector.isActive()`) вызывает `md_core::to_html_mapped()`, который оборачивает каждый блок в:

```html
<div class="md-block" data-md="start,end">...</div>
```

где `start` и `end` — байтовые смещения в исходном markdown.

### DOM-атрибуты

- **`.md-block[data-md]`** — топ-блоки (используют `scrollsync.ts` и `inspector.ts`)
- **`tr[data-md]`, `th[data-md]`, `td[data-md]`** — элементы таблиц (использует только `inspector.ts` для гранулярной подсветки)

### Как строится маппинг (scrollsync.ts, строки 220–230):

```typescript
onRendered(markdown?: string) {
  if (markdown === undefined) {
    blocks = null; maps = null; lineStarts = null;
    if (enabled) syncPair(editor, preview);
    return;
  }
  measure();
  maps = buildUnitMaps(markdown);        // байт ↔ UTF-16
  lineStarts = buildLineStarts(markdown); // начала строк
  blocks = collectBlocks(preview);        // читает data-md из DOM
  alignEditorToPreview();
}
```

`collectBlocks()` (mapping.ts, строка 105–115):
```typescript
export function collectBlocks(root: ParentNode): Block[] {
  const list: Block[] = [];
  for (const el of root.querySelectorAll<HTMLElement>(".md-block[data-md]")) {
    const range = parseRange(el);
    if (range) list.push({ start: range.start, end: range.end, el });
  }
  list.sort((a, b) => a.start - b.start);
  return list;
}
```

### Два режима синхронизации:

1. **Анкорная** (когда `hasAnchors()` = true): редактор → логическая строка → байтовая позиция → блок → `scrollIntoView` в preview. И наоборот.
2. **Пропорциональная** (fallback): `scrollTop / maxScroll` × maxScroll целевой панели.

### Защита от обратной связи (scrollsync.ts, строки 52–53):

```typescript
const ECHO_MS = 100;
// Двухуровневая: флаг syncing + окно ECHO_MS
```

---

## 6. inspector.ts и tables.ts — взаимодействие с DOM preview

### inspector.ts

**Меняет preview.innerHTML напрямую:** НЕТ. Только через `classList.add/remove`:
- `inspect-active` (строка 155) — подсветка блока
- `inspect-row` (строка 177) — подсветка строки таблицы
- `inspect-col` (строка 184) — подсветка столбца таблицы

**Своё состояние:**
- `active: boolean` — режим включён
- `blocks: Block[] | null` — блоки из DOM
- `maps: UnitMaps | null` — byte↔UTF-16
- `activeBlockEl: HTMLElement | null` — текущий подсвеченный блок
- `bandEls: HTMLElement[]` — элементы с транзиентными классами
- `lastHover`, `lastHoverShift` — для пересчёта при Shift
- `programmaticSel` — защита от перебивания выделения
- `savedSelection`, `savedScrollTop`, `savedFocus` — для восстановления при выходе

### tables.ts

**Меняет preview.innerHTML напрямую:** НЕТ.

**Меняет DOM preview напрямую через:**
- `table.parentNode?.insertBefore(wrap, table)` (строка 360) — обёртка вокруг таблицы
- `wrap.append(tools, scrollWrap)` (строка 361) — добавление панели инструментов
- `scrollWrap.appendChild(table)` (строка 362) — перемещение таблицы
- `r.hidden = !show` (строка 139) — скрытие строк
- `tbody.appendChild(r)` (строка 152) — переупорядочивание строк
- `th.classList.toggle(...)` (строки 158–160) — индикаторы сортировки
- `funnel.addEventListener("click", ...)` (строка 383) — воронка фильтра
- `menu.appendChild(...)` (строка 300) — добавление выпадающего меню в `document.body`

**Своё состояние:**
- `registry: WeakMap<HTMLTableElement, TableEntry>` — состояние каждой таблицы
- `openMenu: HTMLElement | null` — открытое меню фильтра (глобальное)
- `anchorSeq: number` — счётчик уникальных ID

---

## 7. index.html — структура DOM

```html
<body>
  <div id="app">                          <!-- flex column, 100vh -->
    <header id="toolbar">                 <!-- flex row, gap 6px -->
      <button id="btn-new">Новый</button>
      <button id="btn-open">Открыть</button>
      <button id="btn-save">Сохранить</button>
      <button id="btn-save-as">Сохранить как…</button>
      <span id="file-label">безымянный</span>
      <button id="btn-inspect">⌕ Инспектор</button>
      <label id="toggle-sync">
        <input type="checkbox" id="chk-sync" checked /> синхронно
      </label>
      <label id="toggle-preview">
        <input type="checkbox" id="chk-preview" checked /> предпросмотр
      </label>
    </header>

    <main id="panes">                     <!-- flex 1, flex row -->
      <textarea id="editor" ...></textarea> <!-- flex 1, white-space: pre -->
      <article id="preview" class="markdown-body"></article> <!-- flex 1 -->
    </main>
    <!-- НЕТ resize-бара между панелями -->

    <footer id="statusbar">               <!-- flex row, gap 16px -->
      <span id="stat-pos">Стр 1, Кол 1</span>
      <span id="stat-size">0 симв.</span>
      <span id="stat-inspect" hidden>Режим инспектора</span>
      <span id="stat-msg"></span>
    </footer>
  </div>
</body>
```

**Layout:** 3 горизонтальные зоны (toolbar / panes / statusbar). Внутри panes — две панели 50/50 (flex: 1), **без resize-бара**.

---

## 8. СПИСОК БОЛЕЙ — что мешает выделить DocumentStore

### Боль 1: Состояние документа перемешано с DOM-хуками (main.ts, строки 19–43)

`currentPath`, `dirty`, `editor`, `preview`, `fileLabel`, `statPos` — все top-level переменные в одном файле. Нет разделения на «модель данных» и «представление».

```typescript
// Строки 19–28: DOM-хуки
const editor = document.getElementById("editor") as HTMLTextAreaElement;
const preview = document.getElementById("preview") as HTMLElement;
// ...

// Строки 40–43: состояние документа — в том же scope
let currentPath: string | null = null;
let dirty = false;
```

### Боль 2: Логика панели перемешана с рендером (main.ts, строки 62–96)

`doRender()` — это смесь файловой логики, IPC-вызова, обновления DOM, вызовов enhancement-таблиц, resolve картинок и обновления inspector/scrollsync. Один монолитный async-функция на 35 строк.

```typescript
// Строки 62–96: один асинхронный метод делает ВСЁ
async function doRender() {
  const seq = ++renderSeq;
  const source = editor.value;
  const html = await renderMarkdown(source, inspector.isActive());
  if (seq !== renderSeq) return;
  if (html !== lastRenderedHtml) {
    lastRenderedHtml = html;
    preview.innerHTML = html;
    enhanceTables(preview);
  }
  resolveLocalImages(preview, currentPath);
  inspector.onRendered(source);
  scrollSync.onRendered(source);
}
```

### Боль 3: Жёстко зашит один редактор и один preview (main.ts, строки 19–20)

```typescript
const editor = document.getElementById("editor") as HTMLTextAreaElement;
const preview = document.getElementById("preview") as HTMLElement;
```

Все модули (`scrollsync`, `inspector`, `tables`, `images`) получают эти конкретные DOM-элементы через параметры. Нет абстракции «панель» или «view» — каждый модуль работает с конкретными `HTMLTextAreaElement` и `HTMLElement`.

### Боль 4: Состояние dirty и currentPath используется в 8+ местах (main.ts)

```typescript
// Строки 141–142: updateTitle
// Строки 155, 159, 170, 183, 193, 196, 213, 232, 282: newFile, openFile, saveFile, saveAs, input handler, onCloseRequested
```

Нет единого места, где состояние документа инкапсулировано.

### Боль 5: tables.ts хранит состояние таблиц в WeakMap, но не привязан к «документу» (tables.ts, строка 20)

```typescript
const registry = new WeakMap<HTMLTableElement, TableEntry>();
```

Состояние таблиц привязано к DOM-элементам `<table>`, а не к документу. При смене документа (`newFile`, `openFile`) DOM preview полностью пересоздаётся (`preview.innerHTML = ""` / `preview.innerHTML = html`), и WeakMap автоматически очищается. Но нет явного жизненного цикла «документ → таблицы».

### Боль 6: inspector.ts и scrollsync.ts дублируют построение карт (mapping.ts)

```typescript
// inspector.ts onRendered (строка ~280):
blocks = collectBlocks(preview);
maps = buildUnitMaps(markdown);

// scrollsync.ts onRendered (строка ~224):
maps = buildUnitMaps(markdown);
lineStarts = buildLineStarts(markdown);
blocks = collectBlocks(preview);
```

Оба модуля независимо строят `maps` и `blocks` из одного и того же `markdown`. Нет единого источника маппинга, привязанного к документу.

### Боль 7: Нет концепции «несколько документов» — нет массива/карты документов

```typescript
// Строка 40: один путь
let currentPath: string | null = null;
```

Нет `documents: Map<string, DocumentState>`, нет `activeDocId`, нет вкладок. Всё заточено под один файл.

### Боль 8: `lastRenderedHtml` и `renderSeq` — защитные переменные, размазанные по main.ts (строки 42–43, 63, 70, 73, 93)

```typescript
let renderSeq = 0;           // строка 42
let lastRenderedHtml = "";   // строка 43
// ...
const seq = ++renderSeq;     // строка 63
if (seq !== renderSeq) return; // строка 70
lastRenderedHtml = html;     // строка 73
```