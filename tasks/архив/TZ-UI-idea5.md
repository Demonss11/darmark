# TZ-UI-idea5 — воссоздание интерфейса по `front_idea5`

> **Статус:** план (реализация не начата).
> **Источник идей:** `ideas/front_idea5.html` (интерактивный прототип), `ideas/UI_CONCEPT.md`
> (принципы линии idea2→idea3). **Архитектура:** `docs/DESIGN_DOC.md` §5.2–5.5, §13.2.
> **Область:** только `crates/app/src/*` + `crates/app/index.html` + `crates/app/src/style.css`.
> `md-core`/`crates/app/src-tauri/*` **не трогаем** (UI-only).
>
> Итог H1 закрыт (`tasks/TZ-H1.md`); этот ТЗ — UI-срез **до H2**, независимый от плагинной системы.

---

## 0. Цель

Воссоздать визуальный каркас `front_idea5`: тёмная тема, icon-toolbar, rail + сворачиваемый sidebar,
панели с шапками view, gutter с номерами строк, статусбар. Там, где функционал относится к H2
(плагины, права, журнал, палитра) — **заглушка** («скоро»), но каркас готов принять его без переделки.

**Решение по теме:** одна **тёмная** тема, без тумблера (light-палитра и `prefers-color-scheme`
удаляются). UI-строки и комментарии — на русском.

**Не теряем функционал.** Всё, что есть сейчас, обязано работать: файловые команды, рендер
`md-core`+санитайзер, таблицы (сортировка/фильтр/поиск), **инспектор-подсветка** (наш режим, не
путать с «инспектором»-дровером idea5), синхронная прокрутка, локальные картинки, внешние ссылки,
dirty, подтверждение закрытия, хоткеи.

---

## 1. Рамки

**Делаем:** тёмные токены; icon-toolbar (brand, file-chip, format-group, инспектор); rail + sidebar
(заглушка Explorer «скоро»); панели с `pane-head`/`view-switch`; gutter; статусбар; мелкие UX
(сворачивание sidebar, активная панель).

**Не делаем (заглушки):** plugin-strip, панель «Плагины», журнал/события, палитра команд, вкладки
документов (один плейсхолдер-таб), слоты плагинов в статусбаре, инфо-дровер idea5, resize-разделителя
(отложен ещё с Фазы 5).

**Не берём из idea5:** её JS-рендерер markdown и мок-состояние; `⌘`-хоткеи (у нас Windows → `Ctrl`);
обёртки `.editor-scroll`/`.preview-scroll` (ломают скроллеры e2e).

---

## 2. Инварианты (замороженные контракты §13.2)

- **id/разметка:** `app, toolbar, panes, statusbar, editor, preview, btn-new, btn-open, btn-save,
  btn-save-as, file-label, toggle-sync, toggle-preview, chk-sync, chk-preview, btn-inspect,
  stat-pos, stat-size, stat-msg, stat-inspect`.
- **Селекторы:** `#preview .md-block[data-md]`, `.table-enhanced`, `.table-enhanced tbody tr`,
  `.table-enhanced td[data-md]`, `.table-count`, `.table-scroll`, `.col-filter-btn`, `.col-filter-menu`,
  `.col-filter-item`, `.inspect-active`, `.inspect-col`, `tr.inspect-row`.
- **Константы/тексты:** debounce 120 мс, `ECHO_MS = 100`, throttle инспектора 100 мс; стартовый
  документ «Добро пожаловать в darmark»; глобалы `window.__xss`/`window.__errors`.
- **Скроллеры:** `#editor` и `#preview` — **сами** скролл-элементы (`overflow`), обёрток-скроллеров нет.
- **При двух панелях:** id на элементах пары, плюс `data-pane`/`data-view` (уже есть с Фазы 5).

Инспектор (`inspector.ts`) и `scrollsync.ts` **не изменяются**. `md-core`/`src-tauri` — тоже.

---

## 3. Целевой каркас (`index.html`)

```
#app
├─ header#toolbar.toolbar
│    .brand («d» + darmark)
│    .file-chip  → #file-label + маркер dirty (dot)
│    .spacer
│    #format-group (.tbtn): B · I · ` · H · link     ← icon-only
│    .div-v
│    #plugin-strip .plugin-strip                     ← ЗАГЛУШКА (H2)
│    .div-v
│    #palette-trigger.palette-trigger (icon + «Команды» + kbd Ctrl+K)  ← ЗАГЛУШКА
│    #btn-inspect.tbtn                               ← наш инспектор
├─ .workspace
│    nav.rail
│      #rail-explorer.tbtn (active) · #rail-plugins.tbtn (disabled, title «скоро»)
│      .spacer · #rail-logs.tbtn (title «скоро»)
│    aside#sidebar.sidebar
│      #panel-explorer: .side-head «Файлы» + #tree   → «скоро»
│      #panel-plugins (hidden)                       ← ЗАГЛУШКА (H2)
│    main.main
│      .tabrow > #tab-strip                          ← ЗАГЛУШКА: один таб текущего файла
│      #panes.panes
│        section#pane-a.pane.pane-editor[data-pane="pane-editor"][data-view="editor"]
│           .pane-head > #view-switch-a (.vtab «Редактор» T2, static)
│           .editor-wrap > #gutter + #editor
│        div#divider.divider
│        section#pane-b.pane.pane-preview[data-pane="pane-preview"][data-view="preview"]
│           .pane-head > #view-switch (.vtab «Предпросмотр» T1, static)
│           #preview
└─ footer#statusbar.statusbar
     #stat-state («Готов») · #stat-msg · .spacer · #stat-block («—») · #stat-pos · #stat-size ·
     (слоты плагинов — H2)
```

Примечания:
- `#toggle-sync`/`#toggle-preview` (label+checkbox) сохраняются в toolbar (переносим из старого),
  пусть компактно/иконками — id обязателен.
- `#stat-inspect` (`hidden`) остаётся (показывает режим инспектора).
- `.editor-scroll`/`.preview-scroll` из idea5 **не** используем.

---

## 4. Функционально сейчас vs заглушки

| Поверхность | Сейчас | Примечание |
|---|---|---|
| Toolbar: файл/inspector | работает | привязка в `shell.ts` (id сохраняются) |
| Toolbar: format-group (B/I/code/H/link) | **новое, работает** | `formatActions.ts`, операции над выделением |
| Toolbar: palette-trigger | заглушка | клик/`Ctrl+K` → `flash("Палитра — скоро")` |
| Toolbar: plugin-strip | заглушка | пусто (H2 заполнит) |
| rail / sidebar | **новое, работает** | `sidebar.ts`: сворачивание, переключение панели |
| Explorer `#tree` | заглушка | текст «скоро» |
| Панель «Плагины» | заглушка | «скоро» (H2) |
| `#rail-logs` / журнал | заглушка | `flash("Журнал — скоро")` |
| `#tab-strip` | заглушка | один таб текущего файла (имя + dirty) |
| panes / pane-head / view-switch | работает | модель из Фазы 5, добавить шапку редактора |
| gutter (номера строк) | **новое, работает** | `gutter.ts`, синхрон с `#editor` |
| `#preview` + таблицы + инспектор + sync | без изменений | ядро не трогаем |
| statusbar | рестайл | + `#stat-state`, `#stat-block` («—»), слоты плагинов — H2 |
| Инфо-дровер idea5 | не делаем | отложено (частью завязано на H2) |

---

## 5. Токены и CSS

- Один набор токенов (idea5, dark) в `:root`: `--bg-0/1/2/3`, `--line`, `--line-soft`, `--fg`,
  `--fg-dim`, `--fg-mute`, `--accent`, `--accent-soft`, статусные `--green/--amber/--red`.
- Удалить `@media (prefers-color-scheme: dark)` и light-значения.
- Тёмные скроллбары, `::selection`, `:focus-visible`; системные шрифты (без CDN), код — mono-fallback.
- Оболочка: `.workspace{display:flex}`, `.rail` (фикс. ширина), `.sidebar` (переход ширины, `.collapsed`),
  `.main{flex:1;display:flex;flex-direction:column;min-width:0}`, `.panes{flex:1;display:flex}`.
- Gutter: та же `font-size`/`line-height`/`padding-block`, что у `#editor` (иначе номера «поедут»).
- Сохранить `#editor{white-space:pre; overflow:auto; resize:none}` и `#preview{overflow-y:auto}`.

---

## 6. Модули и контракты

### 6.1 Новые

```ts
// sidebar.ts — rail + сворачиваемый sidebar.
export interface Sidebar {
  setPanel(panel: "explorer" | "plugins"): void; // переключить контент панели
  toggle(panel?: "explorer" | "plugins"): void;  // повторный клик по активной — свернуть
  dispose(): void;
}
export function createSidebar(opts: { sidebar: HTMLElement; panels: Record<string, HTMLElement> }): Sidebar;
```

```ts
// formatActions.ts — Markdown-обёртки над выделением textarea (идея-подсветка не задета).
export interface FormatActions {
  bold(): void; italic(): void; code(): void; heading(): void; link(): void;
}
export function createFormatActions(host: HTMLTextAreaElement): FormatActions;
// Каждое действие меняет host.value вокруг selection, восстанавливает selection
// и dispatchEvent(new Event("input")) — единственный путь обновления стора (editorView).
```

```ts
// gutter.ts — номера строк, синхронные прокрутке редактора.
export interface Gutter {
  update(): void;        // перерисовать номера (число строк = host.value)
  syncScroll(): void;    // host.scrollTop = editor.scrollTop
  dispose(): void;
}
export function createGutter(editor: HTMLTextAreaElement, host: HTMLElement): Gutter;
```

### 6.2 Изменяемые

- **`shell.ts`** — расширить `ShellCommands`: `palette(): void` (заглушка), `panel(panel)`; привязать
  `#format-group` → `FormatActions`, `#palette-trigger`, rail-кнопки (через `Sidebar`). Хоткеи: добавить
  `Ctrl+B/I/K` (формат), `Ctrl+K` (палитра-заглушка). Существующие Ctrl+N/O/S/P/I и Esc — без изменений.
- **`statusBar.ts`** — оставить как есть; `#stat-state`/`#stat-block` — статические из HTML (обновление
  `#stat-block` из `RenderIndex` — опционально позже).
- **`editorView.ts`** — без изменения API; `main.ts` подписывает `Gutter` на `store.subscribe` (текст)
  и на `scroll` редактора.
- **`main.ts`** — собрать `createSidebar`, `createGutter`, `createFormatActions`; прокинуть в `createShell`;
  подписать gutter на смену текста и скролл.

`md-core`, `src-tauri`, `inspector.ts`, `scrollsync.ts`, `tables.ts`, `renderIndex.ts`, `viewRegistry.ts`
— **не меняются**.

---

## 7. Порядок работ (чек-лист)

- [ ] `style.css`: тёмные токены + сетка оболочки + компоненты (toolbar/rail/sidebar/tabs/panes/
      pane-head/gutter/statusbar); убрать light/`prefers-color-scheme`.
- [ ] `index.html`: новый каркас по §3 (id и скроллеры сохранены; `#toggle-*` на месте).
- [ ] `sidebar.ts`, `formatActions.ts`, `gutter.ts`.
- [ ] `shell.ts`: формат-группа, палитра-заглушка, rail↔sidebar, хоткеи формата.
- [ ] `main.ts`: композиция новых модулей; gutter на store+scroll.
- [ ] Прогон: `npm run build` → `npx tauri build --no-bundle` → `npm run test:e2e` (6 спеков).
- [ ] Ручная проверка: файл-команды, таблицы, инспектор, sync, формат, gutter, сворачивание sidebar.
- [ ] Обновить `KODA.md`/`AGENTS.md` (карта модулей: `sidebar`/`formatActions`/`gutter`).

---

## 8. Гейт

```bash
cd crates/app && npm run build
npx tauri build --no-bundle && npm run test:e2e   # все 6 спеков
```

Приёмка: интерфейс соответствует `front_idea5` (тёмная тема, icon-toolbar, rail/sidebar, panes,
gutter, статусбар); заглушки помечены/не мешают; **все текущие функции и e2e-контракты сохранены**;
`md-core`/`src-tauri` не тронуты.

---

## 9. Риски

- **Скроллеры.** Любая обёртка-скроллер вокруг `#editor`/`#preview` сломает `scrollTop` и e2e-синк.
- **Геометрия и sync.** Rail/sidebar/tabrow/gutter меняют размеры панелей → перепроверить scrollsync
  e2e (расчёты относительные, но прогнать обязательно).
- **Gutter.** Рассинхрон номеров при несовпадении `line-height`/`padding` с `#editor` — завязать на
  те же значения.
- **Формат-действия.** Меняют текст программно → сбрасывают нативный undo textarea (известное
  ограничение; чинится Rust-undo, отдельная задача). Диспатчить `input`, иначе стор не обновится.
- **Контраст тёмной темы.** Таблицы/инспектор/код должны остаться читаемыми (`.inspect-*`, `th`).
- **e2e кликает по id**, но тексты кнопок уходят в `title` — проверить, что шаги не ищут текст кнопок.

---

## 10. Что дальше (после среза)

H2 наполняет заглушки: plugin-strip, панель «Плагины» (карточки/права/установка), журнал/события,
плагинные view (через уже готовый `viewRegistry`/`ViewContext`), слоты статусбара. Инфо-дровер
idea5 — опционально поверх `RenderIndex`.
