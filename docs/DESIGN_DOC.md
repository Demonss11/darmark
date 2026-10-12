# DESIGN_DOC — darmark

> **Тип: нормативная спецификация целевого состояния.** Описывает архитектуру, которой обязана следовать кодовая база; не описывает текущее состояние, ход работ и измерения. Продукт — лёгкий Markdown-редактор с плагинным UX «открыл `.lua` — поправил — работает».
>
> **Правила чистоты:** трассировка решений и измерений — только §16 и `docs/adr/*` / `docs/FINDINGS.md`; в теле нет номеров фактов FINDINGS, чисел измерений, статусов принятия и процессного нарратива; нумерация §0–§17 **заморожена** (ссылки ADR: §7.1, §10, §11.4, §14, §16); бэклог идей — `docs/BACKLOG.md`.
>
> **Детали вынесены:** host-API, манифест, CLI — `docs/PLUGIN_API.md`; гайд и примеры — `docs/PLUGIN_GUIDE.md`; обоснования — `docs/adr/*`; механика песочницы — навык `plugin-sandbox`.

---

## 0. Ключевые ограничения и допущения

Краткая сводка несущих ограничений. Трассировка решений — §16.

| # | Ограничение / допущение | Обоснование |
|---|---|---|
| D1 | Плагинный рантайм — `mlua` 0.12 + vendored Lua 5.5; LuaJIT и wasmi не используются | один лёгкий рантайм; LuaJIT (`ffi` → произвольный C) и wasmi отклонены |
| D2 | Preview — встроенный тир-1 провайдер на хосте, не плагин | горячий путь (debounce) не гнать через интерпретатор |
| D3 | Протокол — вызовы host-функций с JSON-совместимым конвертом; транспорт хост↔Lua — stdio (граница процесса) | JSON-RPC не обязателен; конверт `Request`/`Response`/`Error` — единый формат (§7) |
| D4 | `Pane = { views: ViewId[], active: ViewId }` закладывается сразу | иначе вкладки потребуют переделки модели данных |
| D5 | Владелец текста и undo-стека — Rust `Document` | `apply_edit` требует единой точки правок с ревизиями |
| D6 | База ≤ 6 МБ; «нулевая база»: ни одного встроенного `.lua`/`.wasm` в дистрибутиве | размер — продуктовая метрика |
| D7 | Единая точка санитизации на хосте; SVG — allowlist-подмножество; `data-p-<pluginid>-*` переживают санитизацию | иначе диаграммные/интерактивные плагины невозможны |
| D8 | Имя `darmark`: identifier `dev.darmark.app`, productName `darmark`, каталог `%APPDATA%/darmark/` | имя согласовано во всех артефактах и e2e-фикстурах |
| D9 | Лимиты памяти плагина — средствами рантайма/ОС; собственные аллокаторы и арены (`bumpalo`) не применяются | см. §10; ADR-16 |
| D10 | Язык документации — русский; публичный API-референс — русский, en-перевод отложен | — |
| D11 | Порядок принятия: решения фиксируются документом → проверяются стендом → оформляются как ADR | ADR предшествуют реализации; трассировка — §16 |
| D12 | Excel-таблицы не развиваются; функционал `tables.ts` сохраняется рабочим | приоритет — плагинная платформа и Document/View/Pane |
| D13 | Только Lua 5.5; сравнение с 5.4 не планируется | 5.5 даёт требуемые возможности (§6.1.1) |
| D14 | Транспорт host↔child — stdio с JSON-конвертом; кадр `u32 LE длина \|\| JSON` | единый конверт; без доп. библиотеки поверх пайпа |
| D15 | Результаты измерений (размер, память, производительность) — в `docs/FINDINGS.md` | спецификация без чисел; трассировка — §16 |
| D16 | Изоляция плагинов — отдельный дочерний процесс; GUI-хост без `mlua`, на `panic = "abort"` | изоляция отказов; in-process `unwind`/C-hook отклонены (§6, §10, §11.4) |
| D17 | `rawget`/`rawset` чистятся из `BASE` вместе с `load`/`collectgarbage` | не нужны плагину и снимают обход санитизации метатаблицами |

**Порядок принятия (D11).** Решения D1–D17 фиксируются этим документом, проверяются измерительным стендом, затем оформляются как ADR; ADR предшествуют реализации. Трассировка — §16, бэклог идей — `docs/BACKLOG.md`.

---

## 1. Назначение и рамки

### 1.1. Продукт
Лёгкий Markdown-редактор/вьюер (аналог Notepad++ для Markdown): слева редактор, справа HTML-предпросмотр, Excel-подобные таблицы, инспектор, синхронная прокрутка. Расширяется **пользовательскими Lua-плагинами без пересборки приложения**.

### 1.2. Плагинный UX (главная продуктовая фича)
> Пользователь кладёт `word-count.lua` в `%APPDATA%/darmark/plugins/`, приложение его видит, плагин работает. Пользователь правит файл в блокноте, сохраняет, нажимает «Перезагрузить плагин» — работает новая версия. **Никакого компилятора, никакого `wasm32`, никакой пересборки.**

### 1.3. Вне рамок (не-цели)
- Мобильные платформы. Windows — продукт, Linux — dev.
- Собственный редактор кода (CodeMirror) — отдельный ADR.
- CPU-тяжёлые плагины (собственный парсер, LSP-клиент, mermaid-рендер) — сознательно не поддерживаем.
- Сетевой доступ плагинов по умолчанию запрещён.
- WIT / Component Model.
- Hard-isolation уровня ОС (sandbox) — см. §11.4.

---

## 2. Слои системы

| Слой | Содержимое |
|---|---|
| TS-оболочка (`crates/app/src`) | композиция, layout/pane/view, редактор (тир-2), предпросмотр (тир-1, встроен на хосте — D2); полная карта модулей — §5.5 |
| IPC-граница (Tauri commands) | документы · рендер · плагины · настройки |
| Rust-хост (`src-tauri/src`) | `DocumentStore`·`RenderCache`·`Sanitizer`·`EventBus`·`SettingsStore`; `plugins/`: supervisor·manager·services·bus·scan·settings·views |
| Child-хост плагина (отдельный процесс, stdio) | `PluginHost`(mlua)·Lua 5.5·Job Object |
| Rust-ядро (`crates/md-core`) | чистые функции без UI: `to_html`·`to_html_with`·`to_html_mapped`·`sanitize_html` |

Правила слоёв: (1) `md-core` **не знает** про Tauri, Lua, плагины, состояние — только `&str -> String`; (2) Rust-хост владеет **данными** (текст, ревизии, кэш рендера, конфиг, реестр плагинов); (3) TS владеет **представлением и взаимодействием** (dirty, selection, scroll, фокус, layout); (4) TS не парсит Markdown и не санитизирует HTML — только рендерит готовый HTML и украшает DOM; (5) плагины не видят DOM, единственный канал — host-функции; (6) плагин исполняется в отдельном процессе, хост не разделяет с ним адресное пространство (§10, §11.4).

---

## 3. Контракт ядра `md-core`

```rust
pub const DEFAULT_OPTIONS: Options;
pub fn to_html(markdown: &str) -> String;
pub fn to_html_with(markdown: &str, options: Options) -> String;
pub fn to_html_mapped(markdown: &str) -> String;
pub fn to_html_mapped_with(markdown: &str, options: Options) -> String;
```

`to_html_mapped`/`to_html_mapped_with` оборачивают топ-блоки в `<div class="md-block" data-md="start,end">` (байтовые смещения UTF-8). Это **замороженный контракт** — на нём держатся inspector и scrollsync. Вариант с `Options` нужен плагинам, чтобы управлять рендером, не форкая ядро.

---

## 4. DocumentStore (Rust) — владение состоянием

### 4.1. Структуры
```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)] #[serde(transparent)]
pub struct DocumentId(String);

pub struct DocumentStore { docs: HashMap<DocumentId, Document>, order: Vec<DocumentId> }

pub struct Document {
    pub id: DocumentId, pub path: Option<PathBuf>, pub text: String, pub rev: u64,
    pub cached: Option<RenderCache>,      // (mapped, rev) -> html
    pub undo: Vec<EditOp>, pub redo: Vec<EditOp>,   // D5
}
// D5 (ADR-0028): операция — замена байтового диапазона UTF-8.
pub struct EditOp { pub start: usize, pub removed: String, pub inserted: String }
pub struct RenderCache { pub mapped: bool, pub rev: u64, pub html: String }
// Результат undo/redo: текст после операции + диапазон результата для выделения.
pub struct UndoResult { pub text: String, pub rev: u64, pub start: usize, pub end: usize }
```

### 4.2. Кто чем владеет

| Данные | Владелец | Данные | Владелец |
|---|---|---|---|
| текст документа | **Rust** | dirty-флаг | TS (проекция) |
| `rev` (ревизия) | **Rust** | selection, scroll, фокус | TS |
| `id ↔ path` | **Rust** | layout, panes, tabs | TS |
| кэш HTML, undo/redo стек | **Rust** | | |

**Правило:** TS-`docStore` — **проекция** Rust-стора, а не второй источник истины; любая правка текста идёт через Rust.

### 4.3. Команды (IPC) — 1:1 к host-функциям плагинов

| Команда | Сигнатура | Команда | Сигнатура |
|---|---|---|---|
| `new_document` | `(text?) -> DocMeta` | `save_document` | `(id, path?) -> DocMeta` |
| `open_document` | `(path) -> DocMeta` | `close_document` | `(id) -> ()` |
| `update_document` | `(id, text, mapped) -> RenderResult` | `undo_document` | `(id) -> UndoResult` |
| `render_document` | `(id, mapped) -> RenderResult` | `redo_document` | `(id) -> UndoResult` |

```rust
pub struct DocMeta { pub id: DocumentId, pub path: Option<String>, pub rev: u64, pub dirty_hint: bool }
pub struct RenderResult { pub html: String, pub rev: u64, pub changed: bool }
// UndoResult определён в §4.1.
```
`changed: false` при неизменной `rev`; TS при `changed: false` **не трогает DOM**.

---

## 5. TS-слой: layout, pane, view

### 5.1. Идентификаторы
```ts
type Brand<T, K extends string> = T & { readonly __b: K };
export type DocumentId = Brand<string, "doc">;  export type PaneId = Brand<string, "pane">;
export type ViewId = Brand<string, "view">;
```
Строки, а не числа: единый формат с Rust (`#[serde(transparent)]`) и с Lua-плагинами (строковые ключи в таблицах).

### 5.2. Layout — дерево
```ts
export interface Pane { kind: "pane"; id: PaneId; views: ViewId[]; active: ViewId }  // D4: модель под вкладки сразу
export interface Split { kind: "split"; id: string; dir: "row" | "column"; children: [LayoutNode, LayoutNode] }
export type LayoutNode = Pane | Split;
const MAX_PANES = 2;    // ограничение v1
```
Операции — чистая функция без DOM: `(layout, op) => layout | ValidationError`. Операции: `splitPane`, `closePane`, `focusPane`, `addView`, `removeView`, `activateView`.

### 5.3. Двухтировый ViewProvider (центральная идея)

**Тир 1 — plugin-safe** (JSON на входе, HTML на выходе):
```ts
export interface HtmlViewProvider { readonly kind: ViewKind; readonly title: string;
  createView(ctx: ViewContext, opts: Json): HtmlView }
export interface HtmlView { render(doc: DocumentSnapshot): Promise<void>; dispose(): void }
```

**Тир 2 — built-in only** (полный DOM: редактор, Excel-таблицы, инспектор):
```ts
export interface DomViewProvider { readonly kind: ViewKind; readonly title: string;
  createView(ctx: ViewContext, host: HTMLElement, opts: Json): DomView }
export interface DomView { activate(): void; deactivate(): void; dispose(): void }
```

Preview реализуется как тир-1 провайдер, **встроенный на хосте** (D2). Детали контракта — ADR-0022.

### 5.4. ViewContext — единственная «песочница»
```ts
export interface ViewContext {
  readonly paneId: PaneId; readonly viewId: ViewId;
  document(): DocumentSnapshot | null; edit(text: string): void;
  render(text: string, mapped: boolean): Promise<RenderResult>;
  status(msg: string): void; openExternal(url: string): void;
  onDocument(cb: (d: DocumentSnapshot) => void): () => void;
}
```
Никакого доступа к `document`, `window`, `fetch`, файловой системе. Только этот фасад.

### 5.5. Модульная карта

| Группа | Модули |
|---|---|
| Композиция/оболочка | `main.ts`(корень), `shell.ts`(палитра, диалоги, toast), `palette.ts`(Ctrl+K), `dialog.ts`, `toast.ts`, `statusBar.ts`, `fileActions.ts`, `sampleDocument.ts` |
| Идентификаторы/состояние/IPC | `ids.ts`, `docStore.ts`(проекция Rust-стора, подписки, dirty), `tauri.ts` |
| Layout/pane/view | `layout.ts`, `paneHost.ts`, `viewRegistry.ts`, `editorView.ts`(тир-2 textarea), `previewView.ts`(тир-1) |
| Редактор/рендер/связка | `formatActions.ts`, `gutter.ts`, `mapping.ts`, `renderIndex.ts`, `linkController.ts`(inspector+scrollsync), `inspector.ts`, `scrollsync.ts`, `images.ts`, `tables.ts`(заморожен, D12) |
| Плагины | `drawer.ts`, `pluginManager.ts`(карточки, согласие), `pluginViews.ts`, `pluginStatusBar.ts`, `pluginColor.ts` |
| Журнал/док/AI/мониторинг | `logPanel.ts`(ADR-0029/0030), `bottomDock.ts`(ADR-0030), `aiPanel.ts`(чат, скиллы, диффы, согласие, настройки `provider/model` — ADR-0030/0032/0033/0034/0035), `resourceMonitor.ts`(ADR-0031) |

`linkController` строит **один** `RenderIndex` на `rev` и раздаёт его inspector и scrollsync.

### 5.6. Ограничения TS-слоя
- Нативный undo textarea: программная смена `value` при переключении документа ломает undo-стек браузера; целевое решение — Rust-undo (D5).
- Элементы в `document.body` (например, меню фильтров таблиц) обязаны иметь per-view подписку и `dispose()`.
- E2E-контракты id: `id="editor"`/`id="preview"` — на элементах первичной панели; при двух панелях добавляются `data-pane`/`data-view`.

---

## 6. Плагинная система: Lua

### 6.1. Рантайм
`mlua = { version = "0.12", features = ["lua55", "vendored"] }` (vendored — Lua 5.5 из исходников). **LuaJIT запрещён** (`ffi` → произвольный C — дыра в песочнице). На каждый плагин — **отдельный процесс и отдельное Lua-состояние** (§10, §11.4). Рантайм и его обоснование — ADR-0021, ADR-0023; особенности Lua 5.5 — `docs/PLUGIN_API.md` §5.

#### 6.1.1. Версия Lua: 5.5

Обязательна **Lua 5.5**; API-референс плагинов **обязан** быть под 5.5, не копия 5.1/5.4. **Библиотеки** (`Lua::new_with`): включаются `STRING`, `TABLE`, `MATH`, `UTF8`; `BASE` грузится **неявно, с чисткой** (флага `BASE` в `mlua` 0.12 нет — опасные входы удаляются из глобалов после создания состояния); **не** включаются `COROUTINE` (API синхронный), `OS`/`IO` (только через host-функции с permission), `PACKAGE` (нет `require`), `DEBUG`. Чистка: удалить `load`, `loadfile`, `dofile`, `collectgarbage`, `rawget`, `rawset`; `print` → `host.log("info", ...)` (D17). Детали — `docs/PLUGIN_API.md` §5.

### 6.2. Модель: плагин = код, инстанс = привязка
`plugin = манифест + .lua`; `instance = привязка к view/подписке`. Состояние хранит хост (таблица `PluginState` передаётся плагину аргументом) — плагины **stateless по коду**, что упрощает `dispose()` и перезагрузку.

### 6.3. API плагина (host-функции)

Единственный канал плагина к приложению — host-функции (плюс события, §9). Функции с permission возвращают `(value, err)`: успех — `err = nil`, отказ — `value = nil`, `err = { code, message, permission? }`; `permission_denied` приходит **значением**, не исключением Lua (§8). `host.log`, `md.*`, `json.*` ошибок permission не несут. Группы и сигнатуры — `docs/PLUGIN_API.md` §3.

### 6.4. Точки расширения (`contributes`)
Манифест декларирует `views` (тир-1), `commands` (с `keybinding`), `statusbar`, `toolbar`, `settings` (JSON-схема); формат и пример — `docs/PLUGIN_API.md` §1–§2. `contributes.toolbar` объявляется, но `ui:toolbar` не реализуется (§8).

### 6.5. Пример плагина (эталон документации)
Примеры (`word-count`, `export-html`) и разбор — `docs/PLUGIN_GUIDE.md` §7, `docs/PLUGIN_API.md` §7. Норма: подписчик получает `(name, payload)`; `doc_id` (и `rev`) — в `payload`; команды из `contributes.commands` приходят тем же каналом, `command:invoked` несёт `{command_id, doc_id}`.

### 6.6. Жизненный цикл
`scan(plugins_dir) → validate(manifest) → load(src) → on_activate(ctx) → [события/команды] → on_deactivate(ctx) → unload`. Перезагрузка — `unload` → `load` нового файла с диска, **без перезапуска приложения**. Ошибка загрузки (синтаксис, нет `on_activate`) → плагин `failed` + сообщение, приложение работает. Автоперезагрузка (fs-watch) — off by default. Детали — ADR-0023 §2.4.

---

## 7. Протокол и транспорт

### 7.1. Уровни
Единый JSON-совместимый конверт `Request`/`Response`/`Error` (serde-структуры); **основной транспорт host ↔ Lua — stdio** (граница процесса). Dev-режим (`darmark --debug-plugin <path>`) — тот же конверт поверх назначенного stdio. JSON-RPC не обязателен. Детали — ADR-0021 §2.2, ADR-0023 §2.2/§2.7.

### 7.2. Версионирование
Поле `api_version` в манифесте. Хост отвергает плагин с `api_version > HOST_API_VERSION`. Обратная совместимость host-функций — семантическое версионирование, удаление функций запрещено без мажора. Политика совместимости (двухступенчатая депрекация) — ADR-0027.

---

## 8. Модель разрешений

По образцу Tauri ACL: **всё запрещено по умолчанию** (deny-by-default); плагин декларирует нужное в `permissions`, пользователь выдаёт подмножество (`granted`), эффективный набор = **`manifest ∩ granted`**. Проверка — **на стороне хоста, до вызова**; плагин без права получает `permission_denied` **значением**, а не исключением Lua (§6.3).

- Поддержаны: `document:read`, `document:write`, `view:create`, `view:modify`, `ui:statusbar`. `filesystem:*` и `network` **не предоставляются**; `ui:menu`/`ui:sidebar`/`ui:toolbar` отложены. Привязка прав к функциям — `docs/PLUGIN_API.md` §1; политика версий контракта — ADR-0027.
- **Согласие:** хранится в `%APPDATA%/darmark/config.json`; смена набора у запущенного плагина применяется перезапуском child-процесса. Предупреждения границы (`ISOLATION_NOTICE`, `DOCUMENT_ACCESS_NOTICE`, `permission_notices`) — ADR-0021 §3; магазин **обязан** показать их в UI (§11.4).

---

## 9. События

### 9.1. Каталог событий
| Событие | Полезная нагрузка |
|---|---|
| `document:changed` | `doc_id`, `rev` |
| `document:opened` / `document:closed` | `doc_id`, `path` |
| `view:scroll` | `view_id`, `top` |
| `view:focus` | `view_id` |
| `command:invoked` | `command_id`, `doc_id` |

> **События фронт-шины (оболочка), аддитивно:** `plugin-status-changed`, `document-event`, `log-entry` (ADR-0029 §2.4); `resource-metrics`, `plugin-process-exited` (ADR-0031 §2.4). Не входят в контракт host↔child и `HOST_API_VERSION`; канал host → front, мажор не поднимают.

### 9.2. Модель доставки: notify-only + pull
Событие **не несёт содержимое документа** — только `doc_id` и `rev`; плагин при необходимости вызывает `get_document_range`/`get_document_text` (нет дублирования больших строк в подписчиков; события можно пропустить без потери данных).

### 9.3. Коалесинг и порядок
- События по одному `doc_id` **коалесцируются по `rev`**: доставляется только последняя ревизия за тик.
- Порядок доставки: FIFO внутри одного типа события.
- Подписка возвращает `unsubscribe`-функцию; `dispose()` view/плагина **обязан** отписаться (иначе — утечка на закрытой DOM-ноде).
- При (ре)старте плагина хост **адресно** переиздаёт `document:opened` для последнего документа, о котором публиковались `document:opened`/`document:changed` — иначе тир-1 view, наполняемый по подписке, после `reload`/`set_plugin_enabled(true)`/`set_plugin_permissions` остался бы пустым.

### 9.4. Обратная маршрутизация из тир-1 view
```
клик по [data-p-<pluginid>-action="foo"]
  -> делегированный слушатель на контейнере view (один на view)
  -> сериализация {view_id, action, payload} (payload из data-p-*-payload)
  -> вызов Lua-хендлера плагина
  -> плагин возвращает новый HTML или патч
  -> host.set_view_content / точечный патч
```
`data-p-<pluginid>-*` — **единственный** класс атрибутов, который переживает санитизацию (D7).

---

## 10. Ресурсные лимиты и отказоустойчивость

### 10.1. Лимиты
Плагин исполняется в отдельном дочернем процессе; лимиты задаёт ОС через Job Object.

| Лимит | Механизм |
|---|---|
| Память плагина | RSS-лимит Job Object — жёсткий, детерминированный |
| CPU/время | Job Object как грубый backstop + **per-call wall-clock watchdog** (`TerminateJobObject`) |
| Время на вызов | per-call watchdog: превышение бюджета → снятие child'а, отказ плагина |
| Размер `.lua` | проверка перед загрузкой |
| Корутины | не предоставляются плагину |

Собственные аллокаторы и арены (`bumpalo` и др.) для песочницы не применяются — см. §16 (ADR-16).

### 10.2. Отказоустойчивость
- Ошибка плагина (Lua `error`, `MemoryError`, stack overflow) — отказ плагина, не приложения.
- Падение child-процесса (OOM, эксплойт, паника плагина): хост видит закрытие stdio-границы / ненулевой exit-код, перезапускает child; повторные падения идут в карантин.
- **Карантин:** после 3 падений подряд плагин автоматически отключается до ручного включения.

Граница процесса: ошибка плагина не выходит наружу — хост и плагин не разделяют адресное пространство (§11.4). Детали — ADR-0021 §3, ADR-0023 §2.4.

### 10.3. Три уровня доверия (для трезвости ожиданий)

| Уровень | Что даёт | Чем обеспечен |
|---|---|---|
| **Не сломает приложение** | ошибка/лимит → сообщение, а не краш | граница процесса + карантин |
| **Не съест ресурсы** | лимиты памяти и времени | Job Object + per-call watchdog |
| **Не украдёт данные** | нет ФС/сети без permission, нет `os`/`io`/`require` | allowlist библиотек + проверка permission в host-функциях |

---

## 11. Санитайзер и границы безопасности

### 11.1. Единая точка
Санитизация — **только на хосте**, ровно один раз, в конце конвейера рендера. Фронтенд `preview.innerHTML = html` доверяет результату. HTML плагинных view санитизируется тем же ядром: ядро предоставляет **публичный вход** санитизации фрагмента (расширенный allowlist `data-p-*`, см. §11.2).

### 11.2. Политика санитизации
| Аспект | Правило |
|---|---|
| SVG | allowlist-подмножество: `svg`, `g`, `path`, `rect`, `circle`, `line`, `polyline`, `text`, `defs`, `use` — без событий и внешних ссылок; иначе растровый fallback |
| Атрибуты плагина | `data-p-<pluginid>-*` переживают санитизацию (единственная правка allowlist ядра) |
| Классы | `class` входит в глобальный allowlist и сохраняется как есть; для плагинов зарезервирован префикс `p-<pluginid>-`. Ужесточение до «только `p-*`» **не выполняется** |
| Схемы URL | `http`, `https`, `mailto`, `tel`; плагинам `http` в `src`/`href` дополнительно запрещён без permission `network` |

### 11.3. CSP
`default-src 'self'; style-src 'self' 'unsafe-inline'` — сохраняется. Плагины **не могут** внедрять скрипты: только HTML + инлайн-стили. Внешние библиотеки/`eval` запрещены.

### 11.4. Границы безопасности
- **Изоляция отказов:** краш, OOM и паника плагина ограничены child-процессом; GUI-хост жив, child перезапускается (§10.2).
- **Изоляция C-эксплойта:** эксплойт получает память child'а (в т.ч. легально прочитанный текст документа), но не память GUI-хоста. Полной sandbox-изоляции уровня ОС нет — это осознанный отказ.
- **Изоляция данных** определяется моделью permissions (§8), а не границей процесса: плагин читает документ легально, через `host.get_document*` с permission.
- Магазин плагинов **обязан** предупредить в UI установки: плагин с permission `document` видит содержимое документа полностью.

---

## 12. Настройки, состояние, recent files

- **Конфиг приложения**: `%APPDATA%/darmark/config.json`, владелец — Rust `SettingsStore`.
- **AI-провайдеры**: секция `ai` — map `ai.providers` (ключ — id провайдера; поля `name`/`env`/`package`/`settings` (`baseURL`/`apiKey`/`timeout`)/`headers`/`body`/`models`) + активная пара `ai.active_provider`/`ai.active_model`; ссылка на модель — `provider/model` (ADR-0035; структура — раздел **Custom** `opencode.json`, <https://opencode.ai/v2/docs/providers/#custom>).
- **Плагины**: `%APPDATA%/darmark/plugins/<id>/` (манифест + `.lua`); постоянные настройки плагина — `%APPDATA%/darmark/plugin-settings/<id>.json` (per-plugin JSON-объект; UI-схема `contributes.settings` рендерится в менеджере — секция «Настройки», S5b).
- **Состояние окна**: размер/позиция, layout (дерево), открытые документы, активная панель — восстанавливается между сессиями.
- **Recent files**: список путей + время, лимит 20.
- Доступ плагина к настройкам: `host.get_setting(key)`, `host.set_setting(key, value)` — только своего плагина.

---

## 13. Тестирование

### 13.1. Уровни
| Уровень | Инструмент | Что проверяет |
|---|---|---|
| Ядро | `cargo test -p md-core` | рендер, санитайзер, mapped-инварианты |
| Хост | `cargo test -p darmark` | DocumentStore, ревизии, кэш, permissions |
| Плагинный хост | Rust unit + фикстуры `.lua` | загрузка, sandbox (нет `os`/`io`/`coroutine`/`require`), лимиты (Job Object, watchdog), карантин |
| Фронтенд | `npm run build` (`tsc && vite build`) | strict-типы |
| E2E | Cucumber + WebdriverIO | замороженные контракты (см. ниже) |
| Дифференциальные | harness CommonMark/GFM | `to_html` vs `to_html_mapped` на корпусе спецификаций |

### 13.2. Замороженные E2E-контракты (нельзя ломать)

> Источник истины — этот раздел (`DESIGN_DOC §13.2`).
- **DOM id**: `editor`, `preview`, `btn-inspect`, `chk-sync`, `chk-preview`, `btn-new`, `btn-open`, `btn-save`, `btn-save-as`, `file-label`, `stat-pos`, `stat-size`, `stat-msg`, `stat-inspect`, `toolbar`, `panes`, `statusbar`, `toggle-sync`, `toggle-preview`, `app`.
- **Де-факто id** (используются спеками): `view-switch`, `plugin-view`, `format-group`, `e2e-marker`, `gutter`, `tab-name`, `tab-dirty`.
- **Селекторы**: `#preview .md-block[data-md]`, `.table-enhanced`, `.table-enhanced tbody tr`, `.table-enhanced td[data-md]`, `.table-count`, `.table-scroll`, `.col-filter-btn`, `.col-filter-menu`, `.col-filter-item`, `.inspect-active`, `.inspect-col`, `tr.inspect-row`.
- **Селекторы плагинной панели**: `.vtab.core`, `.vtab.plugin[data-view]`, `.pl-item[data-plugin]`, `.pl-badge`, `input.pl-enabled`, `button.pl-reload`, `.pl-cmd[data-command]`, `input.pl-grant[data-permission]`, `button.pl-info`, `.pl-info-pop`.
- **Текстовые**: стартовый документ `"Добро пожаловать в darmark"`; заголовки демо-таблицы `Файл | Размер | Строк | Изменён`.
- **Константы**: debounce рендера **120 мс**, `ECHO_MS = 100`, throttle инспектора 100 мс.
- **Глобальные**: `window.__xss`, `window.__errors` / `window.__errorCapture`.
- При двух панелях: id остаются на **первичной** панели, добавляются `data-pane` / `data-view`.
- **Плагинные контракты (миграция ADR-0024):** `#plugin-manager` и `#side-count` сохраняются
  (`#plugin-manager` переезжает в `#drawer-body`, `#side-count` — в `.drawer-head`); удаляются
  `#rail-plugins`, `#panel-plugins`, `#rail-logs`; добавляются `#tb-plugins`, `#tb-plugins-cnt`,
  `#tb-plugins-dot`, `#drawer`, `#drawer-title`, `#drawer-body`, `#drawer-close`, `#backdrop`,
  `#sb-logs`, `#log-badge`.
- **Контракты панели журнала (ADR-0029):** слот `#sb-logs` и бейдж `#log-badge` в статусбаре
  сохраняются; аддитивно добавляются `#logs-panel`, `#logs-resize`, `#logs-collapse`,
  `#logs-list`, `#logs-clear`.
- **Контракты нижнего дока (ADR-0030, доп. 11.10.2026):** `#logs-panel`, `#logs-resize`,
  `#logs-collapse`, `#logs-list`, `#logs-clear`, `#sb-logs`, `#log-badge` сохраняются;
  `#bottom-dock` не вводится. Состояния дока — `hidden`/`collapsed`/`expanded`; скрытие — кнопка
  `#logs-close` (×) и прогрессивный Esc. Режимы-табы: `#mode-logs-btn`/`#mode-mon-btn`/`#mode-ai-btn`.
  Аддитивные id: `#sb-mon`, `#mode-mon-btn`, `#logs-close`, `#mon-body`, `#mon-rows`, `#mon-summary`,
  `#sb-ai`, `#tb-ai`, `#dock-modes`, `#mode-ai-btn`, `#ai-mode`, `#ai-skills`, `#ai-chat`,
  `#ai-input`, `#ai-send`, `#ai-settings`, `#ai-clear`, `#ai-model`.
- **Контракты AI-ассистента (ADR-0030, провайдер — ADR-0032/0035, сетевой слой — ADR-0033):** режим AI дока и аддитивные id
  `#sb-ai`/`#tb-ai`/`#ai-*` сохраняются; **провайдер LLM — OpenAI-совместимый API, задаётся
  пользователем в `config.json` (`ai.providers`, ADR-0035)** (провайдер по умолчанию — `routerai.ru`),
  HTTP-клиент — `reqwest` (native-tls/SChannel) + SSE-стриминг (ADR-0033),
  сеть — только на
  Rust-стороне; отправка содержимого документа — **только после явного согласия** (модальный диалог,
  ADR-0030 §2.4); согласие называет **активный хост** (ADR-0035 §2.7).
- **Контракты мониторинга ресурсов (ADR-0031, доп. 11.10.2026):** `#logs-resources` (`role="note"`)
  сохраняется — **внутри вкладки** `#mon-body[role="tabpanel"]`; аддитивно `#mon-rows` (строки,
  `.mon-row.mon-main`/`.mon-child[data-plugin-id]`), `#mon-summary` (тихая сводка в шапке), классы
  `.mon-*`. События `resource-metrics`, `plugin-process-exited` — аддитивные фронт-события.
- **Инварианты скролла и переключения view:** `#editor` и `#preview` — сами скролл-контейнеры
  (e2e читает их `scrollTop`); переключение view — `hidden`-toggle уже смонтированных узлов,
  не пересоздание DOM.
- **Защищённый функционал инспектора:** `#btn-inspect`, `Ctrl+I`, `#stat-inspect`, `data-md`,
  `.inspect-*` — не переименовывать, не перевязывать, не подменять.
- **ARIA-паттерн инфо-кнопки плагина:** кнопка имеет `aria-expanded="true|false"`,
  `aria-controls="<id поповера>"`, `aria-label="Информация о границах изоляции плагина <id>"`;
  поповер — `role="tooltip"`, уникальный `id` (`pl-info-pop-<plugin_id>`),
  `aria-hidden="true|false"`; поповер не захватывает фокус (фокус остаётся на кнопке).

> Имя продукта (D8) обязано быть согласовано в начале строки; стартовый текст, e2e-фикстуры и шаги правятся синхронно.

---

## 14. Бюджет размера

- **База (GUI-хост) — ≤ 6 МБ**; size-gate в CI: размер release-exe ≤ 6 МБ.
- **AI-клиент (ADR-0033):** ориентир — **маргинальный прирост ≤1.0 МБ** (пересмотрен с ≤0.5 МБ по F55–F58: `reqwest`+SChannel +0.71 МиБ standalone); общий gate ≤6 МБ не меняется.
- GUI-хост **не содержит `mlua`**; Lua-рантайм — в отдельном child-бинарнике.
- Пользовательские `.lua` в дистрибутив не входят (нулевая база, D6).

Фактические измерения размеров и памяти child-процессов — §16 (ADR-0021) и `docs/FINDINGS.md`.

---

## 15. Порядок работ

Бэклог идей — `docs/BACKLOG.md`. Порядок принятия решений — §0 (проверочный стенд → ADR → реализация).

---

## 16. Реестр ADR

> **Трассировка.** Номера фактов и метрики измерений указываются только здесь и в `docs/adr/*`;
> тело документа на них не ссылается. Каждый ADR — отдельным файлом `docs/adr/NNNN-<slug>.md`.

| # | ADR | Раздел DESIGN_DOC | Трассировка |
|---|---|---|---|
| 1 | Имя продукта: **darmark** | §0, §1 | D8 |
| 2 | Preview: встроенный тир-1 | §5.3 | D2 |
| 3 | База: нулевая | §0, §14 | D6 |
| 4 | Панель: `views[] + active` | §5.2 | D4 |
| 5 | Undo: владелец — Rust | §4.1, §4.2 | D5 |
| 6 | Редактор: textarea; CodeMirror — отдельный ADR | §5.3, §15 | — |
| 7 | Санитайзер: единая точка, SVG-allowlist, `data-p-*` | §11 | D7 |
| 8 | Транспорт | §7 | stdio; F16–F22, F26–F27 |
| 9 | Лимиты: Job Object + per-call watchdog + карантин | §10 | F28–F29, F32–F33, F35 |
| 10 | События: notify+pull, коалесинг по rev | §9 | — |
| 11 | Настройки: Rust-store + JSON в appdata | §12 | — |
| 12 | Размер: база ≤6 МБ + size-gate | §14 | F3, F10, F20, F30 |
| 13 | Рантайм: `mlua`/Lua 5.5 (wasmi отклонён) | §6 | D1, D13 |
| 14 | Язык документации: русский | §0, весь документ | D10 |
| 15 | Платформы: Windows — продукт, Linux — dev | §1.3 | — |
| 16 | Аллокатор: без `bumpalo`/арен | §10 | D9 |
| 17 | Изоляция отказов ≠ защита данных | §11.4 | F25 |
| 18 | Порядок принятия: проверка → ADR → реализация | §0, §15 | D11 |
| 19 | Excel-таблицы не развиваются | §1.3, §5.5 | D12 |
| 20 | Результат измерений — таблица FINDINGS | §16 | D15 |
| 21 | Изоляция плагина: отдельный child-процесс | §0, §6, §10, §11.4, §14 | D16; F18, F21, F22–F24, F30–F31, F34, F36 |
| 22 | Контракт ViewProvider: два тира (тир-1 `HtmlViewProvider` plugin-safe, тир-2 `DomViewProvider`) | §5.3, §5.4, §9.4, §11.2 | — |
| 23 | Плагинный runtime: child-хост + Supervisor (stdio, Job Object, watchdog, карантин) | §6–§12, §14 | ADR-0023 |
| 24 | Плагины: триггер в тулбаре + правый дровер (вариант C) | §5.5, §13.2 | ADR-0024 |
| 25 | Ре-дизайн палитры и типографики: семантические токены, WCAG AA | §5, §13.2 | ADR-0025 |
| 26 | Локальный каталог плагинов: `plugins/index.json` + CLI `install/link/remove/update` (Фаза 2a H3) | §7.1, §10 | ADR-0026 |
| 27 | Политика совместимости `HOST_API_VERSION`: мажор — только несовместимые изменения, двухступенчатая депрекация (Фаза 3 H3, S15) | §7.2, §8 | ADR-0027 |
| 28 | Rust-undo: diff-модель `EditOp {start, removed, inserted}`, единый стек ввода/`apply_edit`, команды `undo_document`/`redo_document` (D5, S16) | §4.1, §4.2, §4.3 | ADR-0028 |
| 29 | Журнал: сворачивающаяся панель-консоль над `#statusbar` (исключение из паттерна дровера) | §5.5, §13.2 | ADR-0029 |
| 30 | Нижний док: «Журнал» + AI-ассистент, два режима (расширение ADR-0029; исключение из ADR-0024) | §5.5, §13.2 | ADR-0030 |
| 31 | Источник и границы мониторинга ресурсов: WinAPI (`windows`-crate), главный + child'ы Job Object, 2 с, без авто-kill | §5.5, §10, §14 | F49–F54 |
| 32 | AI-провайдер в MVP: OpenAI-совместимый API (routerai.ru); локальная LLM отложена (вне scope) | §5.5, §13.2 | ADR-0032 |
| 33 | AI: HTTP-клиент `reqwest` (native-tls/SChannel) + SSE-стриминг; бюджет AI-клиента пересмотрен | §5.5, §10, §14 | F55–F58 |
| 34 | Размещение AI-рантайма: в процессе GUI (шов `AiBackend`); сайдкар отложен (триггеры) | §5.5, §10, §13.2 | ADR-0034 |
| 35 | Универсальная конфигурация провайдеров LLM (map провайдеров/моделей, идея opencode.json) | §5.5, §12, §13.2 | ADR-0035 |

---

## 17. Глоссарий

| Термин | Значение |
|---|---|
| **Document** | Данные файла: текст, `rev`, путь, кэш рендера, undo-стек. Владелец — Rust. |
| **View** | Способ отображения документа. Два тира: HtmlView (JSON→HTML) и DomView (полный DOM). |
| **Pane** | Слот в layout. Содержит `views[]` и активный `ViewId`. |
| **Layout** | Бинарное дерево разбиений. `MAX_PANES = 2` в v1. |
| **ViewContext** | Единственный фасад доступа view/плагина к хосту. |
| **rev** | Ревизия документа, инкремент на каждую правку. Ключ кэша и инвалидации. |
| **RenderIndex** | Карты «байты ↔ code units» + блоки. Строится один раз на `rev`. |
| **Плагин** | Код: манифест + `.lua`. |
| **Инстанс** | Привязка плагина к view/подписке; состояние хранит хост. |
| **Карантин** | Автоотключение плагина после 3 падений подряд. |
| **notify-only + pull** | Событие несёт `doc_id`+`rev`, содержимое плагин запрашивает сам. |
| **Child-процесс плагина** | Отдельный процесс с Lua 5.5 и `mlua`; общается с хостом по stdio; лимиты — Job Object. |
| **per-call watchdog** | Wall-clock бюджет на вызов плагина; превышение → `TerminateJobObject`. |
| **Песочница плагина** | Child-процесс + allowlist библиотек + лимиты Job Object + permissions. Даёт изоляцию отказов и контроль ресурсов/доступа; не является sandbox-уровнем ОС и не решает изоляцию данных. |
