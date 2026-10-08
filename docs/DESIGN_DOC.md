# DESIGN_DOC — darmark

> **Тип: нормативная спецификация целевого состояния.** Документ описывает архитектуру, которой
> обязана следовать кодовая база; он не описывает текущее состояние, ход работ и результаты
> измерений. Продукт — лёгкий Markdown-редактор с плагинным UX «открыл `.lua` — поправил — работает».
>
> **Правила чистоты:**
> - трассировка решений и измерений — только §16 и `docs/adr/*` / `docs/proto/FINDINGS.md`;
> - в теле нет номеров фактов FINDINGS, чисел измерений, статусов принятия и процессного нарратива;
> - нумерация разделов §0–§17 **заморожена**: на неё ссылаются ADR (§7.1, §10, §11.4, §14, §16);
> - дорожная карта — `docs/ROADMAP.md`.

---

## 0. Ключевые ограничения и допущения

Краткая сводка несущих ограничений. Трассировка решений — §16.

| # | Ограничение / допущение | Обоснование |
|---|---|---|
| D1 | Плагинный рантайм — `mlua` 0.12 + vendored Lua 5.5; LuaJIT и wasmi не используются | Один лёгкий рантайм; текстовый скрипт правится без пересборки. Wasmi отклонён: платит размером и сложностью за экосистему языков, которая не нужна. LuaJIT запрещён: `ffi` даёт доступ к произвольному C — дыра в песочнице. |
| D2 | Preview — встроенный тир-1 провайдер на хосте, не плагин | Горячий путь (каждый debounce) недопустимо гнать через интерпретатор; плагинная система доказывается на холодных плагинах. |
| D3 | Протокол плагинов — вызовы host-функций с JSON-совместимым конвертом; между хостом и Lua-плагином транспорт — stdio (граница процесса) | JSON-RPC не нужен как обязательный транспорт; конверт `Request`/`Response`/`Error` — единый формат данных (§7). |
| D4 | `Pane = { views: ViewId[], active: ViewId }` закладывается сразу | Иначе вкладки потребуют переделки модели данных, а не только UI. |
| D5 | Владелец текста и undo-стека — Rust `Document` | Плагинный `apply_edit` требует единой точки правок с ревизиями. |
| D6 | Размер базы ≤ 6 МБ; «нулевая база»: ни одного встроенного `.lua`/`.wasm` в дистрибутиве | Размер — продуктовая метрика. Плагины не уменьшают базу, а ограничивают её будущий рост. |
| D7 | Единая точка санитизации на хосте; SVG — allowlist-подмножество; `data-p-<pluginid>-*` переживают санитизацию | Иначе диаграммные/интерактивные плагины невозможны. |
| D8 | Имя продукта `darmark`: identifier `dev.darmark.app`, productName `darmark`, каталог `%APPDATA%/darmark/` | Имя обязано быть согласовано во всех артефактах и e2e-фикстурах. |
| D9 | Лимиты памяти плагина — средствами рантайма/ОС; собственные аллокаторы и арены (`bumpalo`) не применяются | См. §10; отклонение арен — §16 (ADR-16). |
| D10 | Язык документации — русский (код, комментарии, ТЗ, ADR); публичный API-референс — русский, en-перевод отложен | — |
| D12 | Excel-таблицы не развиваются; существующий функционал `tables.ts` сохраняется и остаётся рабочим | Размер усилий не окупается; приоритет — плагинная платформа и Document/View/Pane. |
| D13 | Только Lua 5.5; сравнение с 5.4 не планируется | 5.5 даёт требуемые возможности (§6.1.1). |
| D16 | Изоляция плагинов — отдельный дочерний процесс; GUI-хост без `mlua`, на `panic = "abort"` | Убирает хрупкость in-process (panic-стратегия, оверхед hook, патч `pcall`, C-ABI) и даёт изоляцию отказов. In-process `unwind` отклонён (размер ломает D6), in-process C-hook — отклонён (хрупкость). См. §6, §10, §11.4. |
| D17 | `rawget`/`rawset` чистятся из `BASE` вместе с `load`/`collectgarbage` | Не нужны плагину и снимают обход санитизации метатаблицами. |

**Порядок принятия (D11).** Решения D1–D17 фиксируются этим документом, проверяются измерительным
стендом, затем оформляются как ADR; ADR предшествуют реализации H1–H3. Трассировка — §16, дорожная
карта — `docs/ROADMAP.md`.

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

```
┌─────────────────────────────────────────────────────────────┐
│ TS-оболочка (crates/app/src)                                │
│  layout.ts · paneHost.ts · viewRegistry.ts · main.ts(корень) │
│  тир-2 провайдеры: editorView, tablesView                    │
│  previewView — тир-1 контракт, но встроен на хосте (D2)      │
├─────────────────────────────────────────────────────────────┤
│ IPC-граница (Tauri commands)                                 │
│  документы · рендер · плагины · настройки                    │
├─────────────────────────────────────────────────────────────┤
│ Rust-хост (crates/app/src-tauri/src)                         │
│  DocumentStore · RenderCache · PluginSupervisor · Sanitizer  │
│  EventBus · SettingsStore                                    │
├─────────────────────────────────────────────────────────────┤
│ Child-хост плагина (отдельный процесс, stdio-RPC)            │
│  PluginHost(mlua) · Lua 5.5 · Job Object                     │
├─────────────────────────────────────────────────────────────┤
│ Rust-ядро (crates/md-core) — чистые функции, без UI          │
│  to_html · to_html_with · to_html_mapped · sanitize_html     │
└─────────────────────────────────────────────────────────────┘
```

Правила слоёв:
1. `md-core` **не знает** про Tauri, Lua, плагины, состояние. Только `&str -> String`.
2. Rust-хост владеет **данными** (текст, ревизии, кэш рендера, конфиг, реестр плагинов).
3. TS владеет **представлением и взаимодействием** (dirty, selection, scroll, фокус, layout).
4. TS не парсит Markdown и не санитизирует HTML — только рендерит готовый HTML и украшает DOM.
5. Плагины не видят DOM. Единственный канал — host-функции.
6. Плагин исполняется в отдельном процессе; хост не разделяет с ним адресное пространство (§10, §11.4).

---

## 3. Контракт ядра `md-core`

Публичный API ядра:

```rust
pub const DEFAULT_OPTIONS: Options;
pub fn to_html(markdown: &str) -> String;
pub fn to_html_with(markdown: &str, options: Options) -> String;
pub fn to_html_mapped(markdown: &str) -> String;
pub fn to_html_mapped_with(markdown: &str, options: Options) -> String;
```

`to_html_mapped` и `to_html_mapped_with` оборачивают топ-блоки в `<div class="md-block" data-md="start,end">` (байтовые смещения UTF-8). Это **замороженный контракт** — на нём держатся inspector и scrollsync. Вариант с `Options` нужен плагинам, чтобы управлять рендером, не форкая ядро.

---

## 4. DocumentStore (Rust) — владение состоянием

### 4.1. Структуры
```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DocumentId(String);

pub struct DocumentStore {
    docs: HashMap<DocumentId, Document>,
    order: Vec<DocumentId>,
}

pub struct Document {
    pub id: DocumentId,
    pub path: Option<PathBuf>,
    pub text: String,
    pub rev: u64,
    pub cached: Option<RenderCache>,      // (mapped, rev) -> html
    pub undo: Vec<EditOp>,                 // D5
    pub redo: Vec<EditOp>,
}

pub struct EditOp { pub rev: u64, pub range: (usize, usize), pub text: String }
pub struct RenderCache { pub mapped: bool, pub rev: u64, pub html: String }
```

### 4.2. Кто чем владеет

| Данные | Владелец | Обоснование |
|---|---|---|
| текст документа | **Rust** | плагин не видит DOM; текст — единственный общий знаменатель |
| `rev` (ревизия) | **Rust** | ключ кэша рендера и инвалидации индексов |
| `id ↔ path` | **Rust** | сохранение, recent files, плагинные host-функции |
| кэш HTML | **Rust** | ключ `(mapped, rev)` → html; TS отрендеренный HTML не хранит |
| undo/redo стек | **Rust** | единая точка правок для `apply_edit` |
| dirty-флаг | TS (проекция) | нужен синхронно на каждое нажатие |
| selection, scroll, фокус | TS | UI-состояние, IPC на клавишу недопустим |
| layout, panes, tabs | TS | UI-состояние |

**Правило:** TS-`docStore` — **проекция** Rust-стора, а не второй источник истины. Любая правка текста идёт через Rust.

### 4.3. Команды (IPC) — 1:1 к host-функциям плагинов

| Команда | Сигнатура |
|---|---|
| `new_document` | `(text?) -> DocMeta` |
| `open_document` | `(path) -> DocMeta` |
| `update_document` | `(id, text, mapped) -> RenderResult` |
| `render_document` | `(id, mapped) -> RenderResult` |
| `save_document` | `(id, path?) -> DocMeta` |
| `close_document` | `(id) -> ()` |

```rust
pub struct DocMeta { pub id: DocumentId, pub path: Option<String>, pub rev: u64, pub dirty_hint: bool }
pub struct RenderResult { pub html: String, pub rev: u64, pub changed: bool }
```
`changed: false` при неизменной `rev`; TS при `changed: false` **не трогает DOM**.

---

## 5. TS-слой: layout, pane, view

### 5.1. Идентификаторы
```ts
type Brand<T, K extends string> = T & { readonly __b: K };
export type DocumentId = Brand<string, "doc">;
export type PaneId     = Brand<string, "pane">;
export type ViewId     = Brand<string, "view">;
```
Строки, а не числа: единый формат с Rust (`#[serde(transparent)]`) и с Lua-плагинами (строковые ключи в таблицах).

### 5.2. Layout — дерево
```ts
export interface Pane {
  kind: "pane";
  id: PaneId;
  views: ViewId[];      // D4: модель под вкладки закладывается сразу
  active: ViewId;
}
export interface Split {
  kind: "split";
  id: string;
  dir: "row" | "column";
  children: [LayoutNode, LayoutNode];
}
export type LayoutNode = Pane | Split;
const MAX_PANES = 2;    // ограничение v1
```
Операции — чистая функция без DOM: `(layout, op) => layout | ValidationError`. Операции: `splitPane`, `closePane`, `focusPane`, `addView`, `removeView`, `activateView`.

### 5.3. Двухтировый ViewProvider (центральная идея)

**Тир 1 — plugin-safe** (JSON на входе, HTML на выходе):
```ts
export interface HtmlViewProvider {
  readonly kind: ViewKind;
  readonly title: string;
  createView(ctx: ViewContext, opts: Json): HtmlView;
}
export interface HtmlView {
  render(doc: DocumentSnapshot): Promise<void>;
  dispose(): void;
}
```

**Тир 2 — built-in only** (полный DOM: редактор, Excel-таблицы, инспектор):
```ts
export interface DomViewProvider {
  readonly kind: ViewKind;
  readonly title: string;
  createView(ctx: ViewContext, host: HTMLElement, opts: Json): DomView;
}
export interface DomView { activate(): void; deactivate(): void; dispose(): void }
```

Preview реализуется как тир-1 провайдер, **встроенный на хосте** (D2).

### 5.4. ViewContext — единственная «песочница»
```ts
export interface ViewContext {
  readonly paneId: PaneId;
  readonly viewId: ViewId;
  document(): DocumentSnapshot | null;
  edit(text: string): void;
  render(text: string, mapped: boolean): Promise<RenderResult>;
  status(msg: string): void;
  openExternal(url: string): void;
  onDocument(cb: (d: DocumentSnapshot) => void): () => void;
}
```
Никакого доступа к `document`, `window`, `fetch`, файловой системе. Только этот фасад.

### 5.5. Модульная карта

| Модуль | Ответственность |
|---|---|
| `main.ts` | композиционный корень: собрать всё, повесить тулбар |
| `ids.ts` | брендированные id + генераторы |
| `docStore.ts` | проекция Rust-стора, подписки, dirty |
| `layout.ts` | дерево, операции, валидация |
| `paneHost.ts` | монтирование панелей в DOM, фокус |
| `viewRegistry.ts` | реестр провайдеров тир-1/тир-2 |
| `editorView.ts` | тир-2 редактор (textarea) |
| `previewView.ts` | тир-1 preview |
| `linkController.ts` | inspector + scrollsync на общем `RenderIndex` |
| `mapping.ts` | карты «байты ↔ code units» |
| `tables.ts` | украшение таблиц (функционал заморожен, D12) |
| `tauri.ts` | IPC-обёртки |

`linkController` строит **один** `RenderIndex` на `rev` и раздаёт его inspector и scrollsync.

### 5.6. Ограничения TS-слоя
- Нативный undo textarea: программная смена `value` при переключении документа ломает undo-стек браузера; целевое решение — Rust-undo (D5).
- Элементы, вешаемые в `document.body` (например, меню фильтров таблиц), обязаны иметь per-view подписку и `dispose()`.
- E2E-контракты id: `id="editor"`/`id="preview"` — на элементах первичной панели; при двух панелях добавляются `data-pane`/`data-view`.

---

## 6. Плагинная система: Lua

### 6.1. Рантайм
```toml
mlua = { version = "0.12", features = ["lua55", "vendored"] }
```
- `vendored` — Lua 5.5 компилируется из исходников; C-тулчейн уже требуется для Tauri на Windows.
- **LuaJIT запрещён**: `ffi` даёт доступ к произвольному C — дыра в песочнице.
- Плагин компилируется `lua.load(src).set_name(name)`; на каждый плагин — **отдельный процесс и отдельное Lua-состояние** (изоляция глобальных). См. §10, §11.4.

#### 6.1.1. Версия Lua: 5.5

Требуемая версия — **Lua 5.5**. Она обязана давать:

| Возможность 5.5 | Зачем нужна |
|---|---|
| **Декларации глобальных переменных** (`global x`) | В скопе с явной декларацией неявные глобалы запрещены — опечатка становится ошибкой компиляции, а не тихим `nil`. |
| **Компактные массивы** | Крупные массивы заметно компактнее — плагин, читающий документ в массив строк, укладывается в лимит памяти. |
| **Внешние строки** (память не управляется Lua) | Текст документа отдаётся плагину без копирования в Lua-кучу. |

Также доступен `table.create`. API-референс плагинов **обязан** быть написан явно под 5.5, а не скопирован из материалов под 5.1/5.4.

**Набор стандартных библиотек** — задаётся через `Lua::new_with(StdLib, LuaOptions)` (перечисляются только включаемые библиотеки, см. уточнение F7 после таблицы):

| Библиотека | Включаем | Причина |
|---|---|---|
| `BASE` | да, **с чисткой** | `pairs`, `ipairs`, `type`, `tostring`, `error`, `pcall`, метатаблицы строк |
| `STRING`, `TABLE`, `MATH`, `UTF8` | да | базовые операции, `utf8` нужен для Markdown; `TABLE` даёт `table.create` |
| `COROUTINE` | **нет** | плагинный API синхронный; корутины вне контракта |
| `OS`, `IO` | **нет** | файлы и время — только через host-функции с permission |
| `PACKAGE` | **нет** | нет `require`; модули подключает хост |
| `DEBUG` | **нет** | библиотека не предоставляется плагину |

**Чистка после загрузки `BASE`:** удалить `load`, `loadfile`, `dofile`, `collectgarbage`, `rawget`, `rawset`; `print` перенаправить в `host.log("info", ...)`.

> **Уточнение (F7).** В `mlua` 0.12 отдельного флага `BASE` нет: базовая библиотека грузится **неявно**. Поэтому в `new_with` передаются только `STRING|TABLE|MATH|UTF8`, а опасные входы базовой библиотеки удаляются из глобалов после создания состояния (строка `BASE` выше и D17).

### 6.2. Модель: плагин = код, инстанс = привязка
```
plugin (код)         = манифест + .lua файл(ы)
instance (состояние) = привязка к view/подписке; слот состояния хранит ХОСТ
```
Хост хранит таблицу `PluginState` и передаёт её плагину как аргумент — плагины остаются **stateless по коду**, состояние живёт в хосте. Это упрощает `dispose()` и перезагрузку.

### 6.3. API плагина (host-функции)

**Документ:**
```lua
host.get_document_len(doc_id)                -> integer
host.get_document_range(doc_id, start, len)  -> string   -- основной путь чтения
host.get_document_version(doc_id)            -> integer
host.apply_edit(doc_id, start, stop, text)   -- только с permission document:write
host.get_document_text(doc_id)               -> string   -- вспомогательный, не основной
```

**View:**
```lua
host.get_view_state(view_id)     -> table {scroll, cursor}
host.set_view_content(view_id, html)        -- только для тир-1 view
host.on_event(view_id, event_name, handler) -- подписка на события view
```

**Хост:**
```lua
host.log(level, message)         -- level: "debug"|"info"|"warn"|"error" -> stderr хоста
host.show_message(text)          -- уведомление в статусбар (permission ui:statusbar)
host.export_html(text)           -- (ok, err): нативный диалог сохранения; filesystem:write не нужен
```

**Lua-нативные удобства** (регистрируются хостом, не требуют JSON-прослойки):
```lua
md.to_html(text, opts?)          -> string   -- обёртка над md-core
md.to_html_mapped(text, opts?)   -> string
json.encode(value) / json.decode(s)
```

> **Контракт ошибок host-функций.** Функции, зависящие от permission, возвращают в Lua два
> значения `(value, err)`: при успехе `err = nil`, при отказе `value = nil`, а `err` — таблица
> `{ code, message, permission? }`. `permission_denied` приходит плагину **значением**, а не
> исключением Lua (§8): обработчик продолжает работу и сам решает, что делать. `host.log`,
> `md.*`, `json.*` ошибок permission не несут и возвращают одно значение.

### 6.4. Точки расширения (`contributes`)
Манифест декларирует, что плагин добавляет:
```json
{
  "id": "word-count",
  "name": "Word Count",
  "version": "1.0.0",
  "api_version": 1,
  "entry": "main.lua",
  "permissions": ["document:read", "ui:statusbar"],
  "contributes": {
    "views":     [{ "kind": "wordcount", "title": "Статистика", "tier": 1 }],
    "commands":  [{ "id": "wordcount.count", "title": "Посчитать слова", "keybinding": "Ctrl+Alt+W" }],
    "statusbar": [{ "id": "wordcount.status", "text": "Слова: {n}" }],
    "toolbar":   [{ "id": "wordcount.btn", "title": "Статистика", "command": "wordcount.count" }],
    "settings":  { "type": "object", "properties": { "ignore_code": { "type": "boolean", "default": true } } }
  }
}
```

### 6.5. Пример плагина (эталон документации)

Подписчик получает `(name, payload)`; `doc_id` (и `rev`) лежат в `payload`, а не
передаются отдельным аргументом:

```lua
-- word-count/main.lua
local function count_words(text)
  local n = 0
  for _ in text:gmatch("%S+") do n = n + 1 end
  return n
end

function on_activate(ctx)
  ctx.subscribe("document:changed", function(name, payload)
    local doc_id = payload.doc_id
    local len = host.get_document_len(doc_id)
    local text = host.get_document_range(doc_id, 0, len)
    host.show_message("Слова: " .. count_words(text))
  end)
end

function on_deactivate(ctx) end
```

Команды из `contributes.commands` приходят тем же каналом подписки. Событие
`command:invoked` несёт `{command_id, doc_id}`: у плагина нет собственного
«активного» документа, поэтому текущий `doc_id` передаётся вместе с командой.

```lua
-- export-html/main.lua
function on_activate(ctx)
  ctx.subscribe("command:invoked", function(name, payload)
    if payload.command_id == "export-html.export" then
      local doc_id = payload.doc_id
      local len = host.get_document_len(doc_id)
      local text = host.get_document_range(doc_id, 0, len)
      host.export_html(md.to_html(text))
    end
  end)
end
```

### 6.6. Жизненный цикл
```
scan(plugins_dir) -> validate(manifest) -> load(src) -> on_activate(ctx)
                                                   -> [события/команды]
                                                   -> on_deactivate(ctx) -> unload
```
- Перезагрузка: `unload` старого инстанса → `load` нового файла с диска. **Без перезапуска приложения.**
- Ошибка загрузки (синтаксис, отсутствует `on_activate`) → плагин помечается `failed`, показывается сообщение, приложение работает.
- Автоперезагрузка по изменению файла (fs-watch) — опционально, off by default.

---

## 7. Протокол и транспорт

### 7.1. Уровни
| Уровень | Формат | Транспорт |
|---|---|---|
| **Данные** | serde-структуры `Request`/`Response`/`Error` (JSON-совместимые) | — |
| **Хост ↔ Lua-плагин** | тот же JSON-совместимый конверт | stdio, граница процесса |
| **Dev** | тот же конверт как JSON-строки | stdio (`darmark --debug-plugin <path>`) |

Конверт `Request`/`Response`/`Error` — единый формат данных; JSON-RPC не является обязательным транспортом. Детали и обоснование — §16 (ADR-0021).

### 7.2. Версионирование
Поле `api_version` в манифесте. Хост отвергает плагин с `api_version > HOST_API_VERSION`. Обратная совместимость host-функций — семантическое версионирование, удаление функций запрещено без мажора.

---

## 8. Модель разрешений

По образцу Tauri ACL: **всё запрещено по умолчанию**, плагин декларирует нужное, пользователь подтверждает при установке/первом запуске и может отозвать.

| Разрешение | Что даёт |
|---|---|
| `document:read` | `get_document_len`, `get_document_range`, `get_document_version`, `get_document_text` |
| `document:write` | `apply_edit` |
| `view:create` | регистрация тир-1 view |
| `view:modify` | `set_view_content`, `on_event` |
| `filesystem:read` | `host.read_file(path)` — со scope (какие пути) |
| `filesystem:write` | `host.write_file(path, data)` — со scope |
| `network` | `host.http_*` — по умолчанию **не предоставляется вообще** |
| `ui:menu` | пункт в меню |
| `ui:statusbar` | элемент статусбара |
| `ui:sidebar` | панель в сайдбаре |
| `ui:toolbar` | кнопка в тулбаре |

Проверка — **на стороне хоста, до вызова**. Плагин без `document:write` получает `error { code = "permission_denied", permission = "document:write" }`, а не исключение Lua.

**Согласие (H2):** пользователь выдаёт подмножество декларированных прав (`granted`), эффективный набор = `manifest ∩ granted` (deny-by-default). Согласие хранится в `%APPDATA%/darmark/config.json`; смена набора у запущенного плагина применяется перезапуском child-процесса.

---

## 9. События

### 9.1. Каталог событий
| Событие | Полезная нагрузка |
|---|---|
| `document:changed` | `doc_id`, `rev` |
| `document:opened` / `document:closed` | `doc_id`, `path` |
| `view:scroll` | `view_id`, `top` |
| `view:focus` | `view_id` |
| `pane:resized` | `pane_id`, `w`, `h` |
| `command:invoked` | `command_id`, `doc_id` |

### 9.2. Модель доставки: notify-only + pull
Событие **не несёт содержимое документа**. Оно несёт `doc_id` и `rev`. Плагин, если ему нужен текст, вызывает `get_document_range`/`get_document_text`. Причины:
1. Не дублировать большие строки в каждый подписчик.
2. Плагин может пропустить события (backpressure) без потери данных.

### 9.3. Коалесинг и порядок
- События по одному `doc_id` **коалесцируются по `rev`**: доставляется только последняя ревизия за тик.
- Порядок доставки: FIFO внутри одного типа события.
- Подписка возвращает `unsubscribe`-функцию; `dispose()` view/плагина **обязан** отписаться (иначе — утечка на закрытой DOM-ноде).

### 9.4. Обратная маршрутизация из тир-1 view
Как клик/ввод внутри HTML-плагина возвращается в плагин:
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
- Ошибка плагина не выходит за границу процесса: хост и плагин не разделяют адресное пространство.

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
| Атрибуты плагина | `data-p-<pluginid>-*` переживают санитизацию (единственная правка allowlist ядра в H2) |
| Классы | атрибут `class` входит в глобальный allowlist и сохраняется как есть; для плагинов зарезервирован префикс `p-<pluginid>-`. Ужесточение до «только `p-*`» в H2 **не выполняется** |
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
- **Плагины**: `%APPDATA%/darmark/plugins/<id>/` (манифест + `.lua`), настройки плагина — `settings.json` в той же папке, схема из `contributes.settings`.
- **Состояние окна**: размер/позиция окна, layout (дерево), открытые документы, активная панель — восстанавливается между сессиями.
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
- **DOM id**: `editor`, `preview`, `btn-inspect`, `chk-sync`, `chk-preview`, `btn-new`, `btn-open`, `btn-save`, `btn-save-as`, `file-label`, `stat-pos`, `stat-size`, `stat-msg`, `stat-inspect`, `toolbar`, `panes`, `statusbar`, `toggle-sync`, `toggle-preview`, `app`.
- **Селекторы**: `#preview .md-block[data-md]`, `.table-enhanced`, `.table-enhanced tbody tr`, `.table-enhanced td[data-md]`, `.table-count`, `.table-scroll`, `.col-filter-btn`, `.col-filter-menu`, `.col-filter-item`, `.inspect-active`, `.inspect-col`, `tr.inspect-row`.
- **Текстовые**: стартовый документ `"Добро пожаловать в darmark"`; заголовки демо-таблицы `Файл | Размер | Строк | Изменён`.
- **Константы**: debounce рендера **120 мс**, `ECHO_MS = 100`, throttle инспектора 100 мс.
- **Глобальные**: `window.__xss`, `window.__errors` / `window.__errorCapture`.
- При двух панелях: id остаются на **первичной** панели, добавляются `data-pane` / `data-view`.

> Имя продукта (D8) обязано быть согласовано в начале строки; стартовый текст, e2e-фикстуры и шаги правятся синхронно.

---

## 14. Бюджет размера

- **База (GUI-хост) — ≤ 6 МБ**; size-gate в CI: размер release-exe ≤ 6 МБ.
- GUI-хост **не содержит `mlua`**; Lua-рантайм — в отдельном child-бинарнике.
- Пользовательские `.lua` в дистрибутив не входят (нулевая база, D6).
- Собственные аллокаторы/арены не добавляются (D9).

Фактические измерения размеров и памяти child-процессов — §16 (ADR-0021) и `docs/proto/FINDINGS.md`.

---

## 15. Порядок работ

Дорожная карта H1–H3 — `docs/ROADMAP.md`. Порядок принятия решений — §0 (проверочный стенд → ADR → реализация).

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
