# TZ-H1-F5 — Фаза 5: layout, панели, linkController, таблицы-контроллер

> **Статус:** сделана (Фаза 5 закрыта). Каркас панелей взят из `ideas/front_idea5.html`.
> **Родитель:** `tasks/TZ-H1.md` §2 (Фаза 5), §4. **Архитектура:** `docs/DESIGN_DOC.md` §5.2, §5.5, §5.6, §13.2.
> **UI-референс:** `ideas/front_idea5.html` (источник идей каркаса панелей; берём частично — §2.7).
> **Область:** только `crates/app/src/*` + `index.html`/`style.css` (TS/CSS). `md-core`/`src-tauri` **не трогаем**.

---

## 0. Цель

Ввести модель `Document / View / Pane`: дерево layout (`MAX_PANES = 2`), хост панелей в DOM, пару
«редактор + предпросмотр» как две панели, и связать их общим `linkController` (inspector +
scrollsync). `tables.ts` из модульных синглтонов превращается в экземпляр-контроллер с `dispose()`.

**Не делаем:** вкладки и UI нескольких документов · drag-resize разделителя (`sizes` закладываем,
обработку откладываем) · ребрендинг (ROADMAP п. 8) · Rust-undo · правки `md-core`/`src-tauri`.

**Берём из `front_idea5` (см. §2.7):** каркас панелей `.pane`/`.divider`, шапку панели
`pane-head`/`view-switch`, атрибуты `data-pane`/`data-view`, и — как решение — переключение
видимости вторичной панели вместо её удаления из DOM.

---

## 1. Текущее состояние

- `index.html`: `#panes` — `flex`-контейнер с двумя прямыми детьми `#editor` (textarea) и
  `#preview` (article). Никакой модели `Pane`/`View` в DOM нет.
- `main.ts`: создаёт `renderIndex`, `scrollSync`, `inspector`, `store`, `previewView`, `editorView`
  напрямую; `chk-preview` просто `preview.style.display = "" / "none"`.
- `tables.ts`: **модульные** `openMenu`, `outsideHandler`, `anchorSeq`; меню фильтров вешается в
  `document.body` (строка 326). Со вторым предпросмотром это утечёт/«чужое» меню.
- `inspector.ts`/`scrollsync.ts`: по одному инстансу, оба читают общий `RenderIndex`.
- `renderIndex.ts`: один снимок `(source, rev, maps, lineStarts, blocks)`.

---

## 2. Целевая модель

### 2.1 `layout.ts` — дерево панелей (чистая логика, без DOM)

По DESIGN §5.2:

```ts
export interface Pane { kind: "pane"; id: PaneId; views: ViewId[]; active: ViewId }
export interface Split {
  kind: "split"; id: string; dir: "row" | "column";
  children: [LayoutNode, LayoutNode];
}
export type LayoutNode = Pane | Split;
export const MAX_PANES = 2;

export type LayoutOp =
  | { t: "splitPane"; pane: PaneId; dir: "row" | "column"; view: ViewId }
  | { t: "closePane"; pane: PaneId }
  | { t: "focusPane"; pane: PaneId }
  | { t: "addView"; pane: PaneId; view: ViewId }
  | { t: "removeView"; pane: PaneId; view: ViewId }
  | { t: "activateView"; pane: PaneId; view: ViewId };

export function applyLayout(node: LayoutNode, op: LayoutOp): LayoutNode | ValidationError;
export function countPanes(node: LayoutNode): number;   // для валидации MAX_PANES
```

- Чистая функция `(layout, op) → layout | ValidationError`, без DOM.
- v1: дерево глубины 1 — либо один `Pane`, либо `Split` из двух `Pane`.
- `closePane`: родительский `Split` схлопывается в сиблинга.

### 2.2 `paneHost.ts` — монтирование панелей в DOM

```ts
export interface PaneHost {
  mount(layout: LayoutNode): void;   // видимость слотов по дереву + разделитель + активная панель
  setActive(paneId: PaneId): void;   // фокус (класс .active)
}
export function createPaneHost(root: HTMLElement): PaneHost;
```

- `#panes` → контейнер; на каждый `Pane` — статичный слот `<section class="pane" data-pane="<id>">`
  с `data-view` (в v1 слоты заданы в `index.html`; `mount` применяет к ним модель layout).
- **Скроллеры сохраняются:** `#editor` остаётся сам textarea, `#preview` — сам article (у него
  `overflow`), они лишь вкладываются в слоты панелей. E2E читает `preview.scrollTop`/
  `editor.scrollTop` по id — не ломать. **Обёртку `.preview-scroll` из idea5 не берём.**
- **Шапка панели (`pane-head`/`view-switch`, из idea5):** у панели предпросмотра — шапка с одним
  активным view-табом (`.vtab`, «Предпросмотр»); логики переключения нет (задел под плагинные view, H2).
- `mount` скрывает слоты, отсутствующие в дереве, через `display:none`; **элемент и вид остаются
  в DOM** (см. §3). Разделитель `.divider` виден при >1 панели.
- `Split.dir` задаёт flex-направление контейнера (`row`/`column`); в v1 — фиксированные 2 панели.

### 2.3 `linkController.ts` — inspector + scrollsync на пару панелей

```ts
export interface LinkController {
  /** Инспектор: isActive/enable/disable/onEditorActivity. */
  readonly inspector: Inspector;
  onIndexChanged(): void;   // перестроен/сброшен индекс — обоим потребителям
  setSyncEnabled(on: boolean): void;
}
export function createLinkController(opts: {
  editor: HTMLTextAreaElement;
  preview: HTMLElement;
  index: RenderIndex;
  statusEl: HTMLElement;
}): LinkController;
```

- Внутри создаёт `createInspector`/`createScrollSync` и владеет ими; `beforeScrollIntoView`
  инспектора замыкается на `scrollSync.suspend()`.
- Хосты **статичны** (`editor`/`preview` живут всё время): `setEditor/setPreview/dispose` не нужны.
- **Скрытая панель — не ошибка:** `scrollsync` сам пропускает запись при `clientHeight === 0`
  (`writeScrollTop`) и при отсутствии скролла (`syncPair`) — AC-9 проходит без спецкода.

### 2.4 `tables.ts` → экземпляр-контроллер

```ts
export interface TablesController {
  enhance(root: ParentNode): void;   // бывший enhanceTables
  dispose(): void;                    // закрыть меню, снять слушатели из body
}
export function createTablesController(root: HTMLElement): TablesController;
```

- Модульные `openMenu`/`outsideHandler`/`anchorSeq` → замыкание экземпляра.
- `dispose()` **обязан** удалить меню из `document.body` и снять `outsideHandler` (§5.6) —
  иначе при закрытии панели остаётся висячий `fixed`-элемент.
- `previewView` создаёт свой контроллер на каждый экземпляр и вызывает `dispose()` в `dispose()`.

### 2.5 `previewView.ts` — привязка к панели

- `PreviewViewOptions` дополняется `tables: TablesController` (или previewView сам создаёт контроллер).
- `previewEl` теперь приходит от `paneHost` (динамически). При закрытии панели `previewView.dispose()`
  удаляет слушатели/меню; при открытии создаётся новый экземпляр.

### 2.6 `main.ts` — композиция

- Строит `layout` (по умолчанию `Split(row, [editorPane, previewPane])` или один editorPane),
  монтирует через `paneHost`.
- `editorView` → слот первичной панели; `previewView` → слот вторичной.
- `store.onRender` делегирует **текущему** `previewView` (индирекция: `let previewView: PreviewView | null`,
  guard — панель может быть закрыта).
- `chk-preview` (id сохраняется): снят → `paneHost.setVisible(previewPane, false)` (панель скрыта,
  `#preview` остаётся в DOM); установлен → `setVisible(true)`. Без удаления/пересоздания вида
  (решение из idea5 — см. §3).
- `chk-sync` → `linkController.setSyncEnabled(checked)`.

### 2.7 UI-каркас из `front_idea5` (что берём)

Из прототипа берём **только каркас панелей**, без переноса логики/движка:

| Берём | Почему |
|---|---|
| `.panes` → `.pane` (`.pane-editor`/`.pane-preview`) + `.divider` | чище нынешнего «голого» flex; задаёт направление split |
| `pane-head`/`view-switch`/`.vtab` (разметка) | задел под вкладки-view (H2), визуализация Pane/View |
| `data-pane`/`data-view` | единый контракт разметки (совпадает с DESIGN §5.2) |
| `chk-preview` = видимость панели | снимает риск пересоздания `#preview` (AC-9) |
| токены тем/статусов (визуально) | аккуратный вид без внешних зависимостей (CSP-safe) |

**Не берём в Фазу 5:** rail + sidebar (Проводник/Плагины), карточки плагинов, permissions-диалог,
установка, карантин, нижний журнал/события, палитра команд, тема-тумблер, gutter с номерами строк,
JS-рендерер Markdown, мульти-документные вкладки, обёртку `.preview-scroll`, `⌘`-хоткеи. Это
H2 / post-H1 либо прямо конфликтует с нашими нормами (рендер в Rust, e2e-скроллеры).

---

## 3. DOM и замороженные контракты (§13.2)

- id `editor`, `preview`, `panes`, `toolbar`, `statusbar`, `chk-preview`, `chk-sync`,
  `toggle-preview`, `toggle-sync`, `btn-*`, `stat-*`, `file-label` — **сохранить**.
- При одной панели id на элементах; при двух — те же id на элементах пары, плюс `data-pane`/`data-view`.
- debounce 120 мс, `ECHO_MS`, стартовый текст, `window.__errors`, селекторы
  `.md-block[data-md]`/`.table-enhanced*`/`.inspect-*` — без изменений.

**Особый случай — `chk-preview` и AC-9.** Сценарий «Скрытый предпросмотр не ломает синхронизацию»
снимает галочку и прокручивает редактор, ожидая **отсутствия ошибок** (`window.__errors`), а не
наличия `#preview`. **Решение (из `front_idea5`):** `chk-preview` переключает **видимость вторичной
панели** (`display`), а `#preview` **остаётся в DOM**. Тогда:
- `hooks.js` → `setPreviewVisible(true)` просто показывает панель обратно, **без** пересоздания и
  `store.reload()` — контент сохраняется;
- конвейер `store.onRender → previewView.applyRender` не прерывается: вид жив, меняется только
  видимость слота;
- `linkController` всё равно устроен устойчиво к скрытому/отсутствующему предпросмотру
  (defensive: `setPreview(null)` → no-op/пропорция, без исключений).

Это заменяет прежнюю (более хрупкую) идею «`split/close` + `dispose/reload`».

---

## 4. Порядок работ (чек-лист)

- [x] `layout.ts`: `Pane`/`Split`/`LayoutNode`, `MAX_PANES`, `applyLayout` (`splitPane`/`closePane`),
      `panes`/`countPanes`, `LayoutError` (чистые, без DOM).
- [x] `tables.ts`: `createTablesController(root)` (per-instance `openMenu`/`outsideHandler`/`anchorSeq`),
      `dispose()`; `enhanceTables`/`attachMenuAutoClose` заменены; `previewView` переведён на контроллер.
- [x] `linkController.ts`: владеет `inspector`+`scrollsync`; `onIndexChanged`/`suspend`/`setSyncEnabled`.
- [x] `paneHost.ts`: слоты `data-pane`/`data-view`, `mount` (видимость по layout), `setActive`;
      `#editor`/`#preview` — сами скроллеры.
- [x] `index.html`/`style.css`: каркас из idea5 — `.pane`(`#pane-a`/`#pane-b`) + `.divider`,
      `pane-head`/`view-switch` (разметка), стили.
- [x] `main.ts`: layout + paneHost + `chk-preview` → видимость панели (`setPreviewVisible`).
- [x] `npm run build` + release + `npm run test:e2e` — **6 спеков** ✅.

**Результат:** Фаза 5 закрыта. Rust не менялся. `chk-preview` скрывает вторичную панель, `#preview`
остаётся в DOM (AC-9). Resize разделителя — не реализован (отложено, решение §5).

---

## 5. Решения (по умолчанию)

1. **`chk-preview` = видимость вторичной панели** (`display`), id чекбокса неизменен; `#preview`
   остаётся в DOM — без `dispose`/пересоздания/`reload` (идея `front_idea5`).
2. **Панель = слот, вид — вложенный скроллер.** `#editor`/`#preview` остаются самостоятельными
   скролл-элементами (e2e читает их `scrollTop`), обёртка лишь задаёт направление split.
3. **Один `RenderIndex` на всё** (из Фазы 4); `linkController` — единственный владелец
   inspector/scrollsync.
4. **`tables` — на экземпляр предпросмотра**, `dispose()` чистит `body` и слушатели.
5. **`sizes`/drag-resize**: в модель закладываем, обработку **откладываем** (решение).
6. **Шапка панели `pane-head`/`view-switch`** — берём из idea5 как разметку-задел под view-вкладки
   (в Фазе 5 один вид на панель, логики переключения нет).
7. **Каркас `.pane`/`.divider` и токены тем/статусов** — визуально берём из idea5; остальной UI
   прототипа (rail/sidebar/плагины/журнал/палитра/тема/gutter) — H2/post-H1.

---

## 6. Гейт Фазы 5

```bash
cd crates/app && npm run build
npx tauri build --no-bundle && npm run test:e2e   # все 6 спеков
```

Плюс **ручная проверка**: переключение `chk-preview` туда-обратно; синхронизация и инспектор при
двух панелях; отсутствие висячего меню фильтров после закрытия предпросмотра.

Rust не меняется; `cargo test -p md-core -p mdedit` — опционально.

---

## 7. Риски

- **E2E-контракт `chk-preview`.** Теперь низкий: панель скрывается через `display`, `#preview` жив
  (решение §3). AC-9 и `hooks.js` проходят без пересоздания.
- **Утечки из `document.body`**: `tables.dispose()` обязателен, даже если панель только скрывается
  (вид остаётся живым; dispose нужен при будущем реальном закрытии в H2).
- **Индирекция `previewView`** в `store.onRender`: при `null` (если вид когда-либо снимут) рендер
  тихо пропускается.
- **Скроллеры:** не превратить `#preview` в обёртку (`.preview-scroll` из idea5 не берём) — иначе
  `scrollTop` предпросмотра изменит смысл и sync/инспектор-e2e разъедутся.
- **Шапка панели:** принять как **разметку-задел**; не тащить логику switch/тиров из idea5 в Фазу 5.
- **`noUnusedLocals`** при чистке модульных синглтонов `tables.ts`.
