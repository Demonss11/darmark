# ADR-0022. Контракт ViewProvider: два тира представлений

- **Статус:** **Принят**
- **Дата:** 07.10.2026
- **Основание:** реализация H1 (Фазы 3–5): `crates/app/src/`
- **Связанные:** `docs/DESIGN_DOC.md` §2, §5.2–§5.4, §9.4, §11.2

---

## 1. Контекст

Продукт расширяется Lua-плагинами (H2) без пересборки (§1.2). Плагину нужно показывать разметку
в панели, но он исполняется в отдельном процессе и **не видит DOM** (§2, §11.4). Нужен контракт,
который:

- одинаково работает для встроенных view и для будущих плагинных;
- не гонит горячий путь предпросмотра (каждый debounce) через интерпретатор (D2).

## 2. Решение

Вводится **два тира** провайдеров представлений (§5.3):

1. **Тир 1 — plugin-safe** (`HtmlViewProvider`): JSON на входе, HTML на выходе.
   ```ts
   interface HtmlView { render(doc: DocumentSnapshot): Promise<void>; dispose(): void }
   interface HtmlViewProvider { kind; title; createView(ctx: ViewContext, opts: Json): HtmlView }
   ```

2. **Тир 2 — built-in only** (`DomViewProvider`): полный DOM для встроенных контроллеров.
   ```ts
   interface DomView { activate(); deactivate(); dispose() }
   interface DomViewProvider { kind; title; createView(ctx: ViewContext, host: HTMLElement, opts: Json): DomView }
   ```

**Единственная «песочница» — `ViewContext` (§5.4):** `document()`, `edit()`, `render()`,
`status()`, `openExternal()`, `onDocument()`. `innerHTML` пишет только реализация view.

**Реализация H1:**

- `previewView` — тир-1, но встроен на хосте (D2): `createView` получает живые DOM-зависимости; плагин получит строгий `Json`. Единственное место `preview.innerHTML = html`.
- `editorView` — тир-2 (textarea, полный DOM); регистрация `DomViewProvider` — H2.
- Контракт уже использует `dispose()`/unsubscribe (§5.6) — готовность к нескольким панелям.
- `data-p-<pluginid>-*` — единственный класс атрибутов, переживающий санитизацию (D7, §11.2).

## 3. Последствия

**Положительные:**

- Плагинные view (H2) подключаются через тот же `createView`/`render`; горячий путь остаётся на хосте (D2).
- Единая точка санитизации: HTML плагина идёт через тот же `sanitize_html`, что и `render_markdown` (§11.1–§11.2).

**Отрицательные / цена:**

- Две поверхности вместо одной: провайдер должен выбрать тир (или реализовать оба).
- Встроенному тир-1 провайдеру (`previewView`) приходится расширять `opts` за пределы `Json`; компенсируется generic-параметром в реестре.

## 4. Отклонённые альтернативы

- **Один тир (только DOM или только HTML).** Невозможен: плагину не нужен DOM, редактору нужен (§1).
- **Preview как плагин.** Отклонён D2: горячий путь недопустимо гнать через интерпретатор.

## 5. Ссылки

- `docs/DESIGN_DOC.md` §5.2–§5.4 (модель Pane/View/ViewContext), §9.4 (обратная маршрутизация),
  §11.2 (санитизация).
