# Техническое задание: таблицы «как в Excel» (доработка предпросмотра)

**Версия:** 1.0
**Дата:** 04.10.2026
**Целевая аудитория:** разработчик фичи (реализация), ревьюер (объём и приёмка)
**Область:** только фронтенд (`crates/app/src/*`), CSS и тесты. Rust-ядро `md-core` не трогается; `src-tauri` — не затрагивается (см. §4.6).

---

## 1. Постановка задачи

### 1.1. Пользовательская история

> Как автор markdown-документа с таблицами, я хочу, чтобы таблицы в предпросмотре вели себя как лист Excel: фильтры по условиям (>, between, «содержит», «после даты»), многоярусная сортировка, подсветка совпадений поиска, липкий первый столбец, сохранение состояния при наборе текста, копирование отфильтрованного диапазона в Excel и печать только видимых строк.

Сейчас реализовано (MVP): сортировка кликом по одному столбцу ↑/↓/без, автоопределение типа колонки, глобальный поиск по таблице, фильтр по **набору значений** колонки (воронка ▾ со списком чекбоксов), sticky-шапка, счётчик `N из M`, кнопка «Сбросить». Всё это живёт в `crates/app/src/tables.ts` (413 строк, чистый TS, без зависимостей).

### 1.2. Acceptance criteria

| # | Сценарий | Ожидаемый результат |
|---|----------|---------------------|
| AC-1 | Отсортировать таблицу по колонке, применить фильтр, начать печатать текст **в другой части документа** | Сортировка, фильтры, строка поиска и ширины колонок **сохраняются**; незадетые таблицы не пересоздаются (нет мигания, нет потери фокуса) |
| AC-2 | Изменить строку внутри самой отфильтрованной таблицы | Состояние сохраняется для всех таблиц, у которых markdown-исходник не изменился; изменённая таблица получает дефолтное состояние (или восстанавливается по ключу, если исходник вернулся к прежнему) |
| AC-3 | Перекрыть окно / свернуть приложение, открыть снова (тот же документ) | Состояние таблиц восстановлено из `sessionStorage` |
| AC-4 | Числовая колонка: воронка → «Числовые фильтры» → «Больше…» → `10` | Видны только строки со значением > 10; на воронке — индикатор активности; tooltip показывает активное условие |
| AC-5 | Текстовая колонка: «Текстовые фильтры» → «Содержит» → `при` | Работает регистронезависимо; пустые ячейки не проходят текстовый предикат |
| AC-6 | Колонка дат: «Фильтры дат» → «В этом месяце» | Фильтр считается от текущей даты локального часового пояса |
| AC-7 | Комбинация: значения из чекбокс-списка **+** предикат в одной колонке, плюс предикаты в двух колонках, плюс глобальный поиск | Все условия объединяются И; счётчик корректен; «Сбросить» снимает всё |
| AC-8 | Поиск по таблице (`main`) с запросом `ma` | Совпадения обёрнуты в `<mark>`; после сброса поиска `<mark>` сняты; сортировка той же колонки даёт тот же порядок, что и без подсветки |
| AC-9 | Клик по заголовку A, затем **Shift+клик** по заголовку B | Сортировка сначала по A, при равенстве — по B; у стрелок видны номера приоритета `¹`/`²`; обычный клик по C сбрасывает мультисортировку до одной колонки |
| AC-10 | Широкая таблица (≥8 колонок), горизонтальный скролл | Первый `th`/`td` остаётся на месте, между ним и скроллом видна тень-разделитель |
| AC-11 | Таблица на 3000 строк: сортировка + фильтр | Время применения ≤ 250 мс (замер в dev-консоли), прокрутка не «дёргается»; ниже лимита отображается кнопка «Показать ещё 500» |
| AC-12 | Кнопка «Копировать TSV» | В буфере — видимые строки и видимые колонки в порядке сортировки, `\t`-разделитель, табы/переноски/кавычки в значениях экранированы; вставка в Excel даёт правильную сетку |
| AC-13 | Кнопки «Скопировать MD» / «Сохранить таблицу…» | В буфер обмена (или, по второму клику — через нативный диалог сохранения, в файл) попадает GFM-таблица ровно так, как она отфильтрована/отсортирована; выравнивание колонок сохранено, `\|` экранирован |
| AC-14 | Печать (`Ctrl+P`) при активных фильтрах | На лист попадают только видимые строки; панель инструментов, воронки, стрелки сортировки и `<mark>`-фон не печатаются |
| AC-15 | Перетаскивание границы `th` | Ширина фиксируется, переживает ре-рендер (AC-1); двойной клик по границе сбрасывает ширину |
| AC-16 | Открыть меню фильтра, нажать `Esc`, затем стрелки ↑/↓ и `Space` | Меню закрылось по Esc; навигация по чекбокс-списку работает; «Все»/«Ничего» доступны с клавиатуры |
| AC-17 | Переключить язык приложения (en) | Подписи «Все», «Ничего», «Сбросить», плейсхолдеры, счётчик `N of M` переведены; в коде нет захардкоженных строк UI |
| AC-18 | Вызвать `enhanceTables(preview)` два раза на одном контейнере | Одна панель инструментов на таблицу, ноль дублей обработчиков, DOM идентичен однократному вызову |
| AC-19 | Инспектор (`TZ-inspect-mode.md`) активен + таблицы украшены | Hover по-прежнему подсвечивает весь табличный блок; клик по `th`/воронке не перехватывается инспектором |
| AC-20 | `npm run test` в CI | Юнит-тесты `tables-core` зелёные; полное покрытие чистых функций из `tables-core.ts` (§4.5) + ≥30 кейсов |

---

## 2. Контекст проекта (актуальное состояние кода)

Монорепозиторий Cargo workspace (`/workspace`):

```
crates/
  md-core/          # чистое ядро Markdown → HTML (pulldown-cmark 0.13), 6 юнит-тестов
    src/lib.rs      # to_html(), to_html_with(), DEFAULT_OPTIONS
  app/              # Tauri 2 + vanilla TS/Vite (без фреймворков)
    index.html      # #toolbar, #editor (textarea), #preview (article.markdown-body), #statusbar
    src/main.ts     # 220 строк: debounce-рендер 120 мс, lastRenderedHtml, innerHTML, enhanceTables
    src/tauri.ts    # 15 строк: renderMarkdown / readFile / writeFile
    src/tables.ts   # 413 строк: Excel-подобное поведение таблиц (см. разбор ниже)
    src/style.css   # 280 строк: CSS-переменные, light/dark через prefers-color-scheme
    src-tauri/src/lib.rs  # read_file, write_file, render_markdown (+ проверка расширений)
```

### 2.1. Что именно в `tables.ts` сегодня (факт, проверенный по коду)

| Сущность | Строки | Состояние |
|----------|--------|-----------|
| `registry = new WeakMap<HTMLTableElement, TableEntry>()` | 19 | **Ключ — DOM-узел.** После `preview.innerHTML = html` узлы новые → всё состояние теряется |
| `TableState` | 5–10 | `{ sortCol: number \| null, sortDir: 1 \| -1, global: string, colFilters: Map<number, Set<string>> }` — одна колонка сортировки, фильтр = набор разрешённых значений |
| `TableEntry` | 12–17 | `{ wrap, table, state, baseOrder }`; `baseOrder` — массив DOM-строк, тоже «привязан» к старому дереву |
| `parseNumber` / `parseDate` / `toCellValue` / `detectColumnKind` | 26–77 | Эвристики: NUM_RE/DATE_RE, порог 70 % непустых значений. Чистые функции, но не экспортированы и не тестируются |
| `compareCells(a, b, kind)` | 113–125 | `localeCompare(…, { numeric: true, sensitivity: "base" })` для текста; распознанные значения первыми, пустые последними |
| `apply(entry)` | 129–186 | Пересобирает типы **всех** колонок при каждом вызове; `r.hidden = !show` (146); сортировка через `tbody.appendChild(r)` построчно (160, 163) — **без DocumentFragment**; классы `sorted-asc/desc/filtered`; счётчик; disabled кнопки сброса |
| `makeTools()` | 190–224 | Поиск + счётчик + «Сбросить». Строки захардкожены по-русски (197, 210) |
| `openFilterMenu()` | 251–343 | Один режим: чекбоксы уникальных значений + мини-поиск + «Все»/«Ничего»; позиционирование `position: fixed`, ширина 260 px; закрытие по `mousedown` (capture) и по scroll (`attachMenuAutoClose`, 405) |
| `enhance(table)` | 349–402 | Ранний выход `if (registry.has(table)) return`; оборачивает в `.table-enhanced` + `.table-scroll`; навешивает клик/Enter/Space на `th`; `data-anchor-id` из глобального счётчика `anchorSeq` (347) — **не стабилен между рендерами** |
| `enhanceTables(root)` | 411–413 | Простой проход по `querySelectorAll("table")` |

### 2.2. Что в CSS (важно для плана)

- `.markdown-body table th` (style.css:141) имеет `background: var(--toolbar-bg)` — то есть **все** `th`, а не только внутри `.table-enhanced`.
- `.table-enhanced thead th { position: sticky; top: 0; z-index: 1 }` (180–188) — sticky-шапка уже есть.
- Zebra-строки включены всегда: `.markdown-body table tbody tr:nth-child(2n)` (142) — учитывай, что скрытые строки ломают чередование (см. T-14).
- `.col-filter-menu` (229–262) — фиксированная ширина 260 px, flex-колонка, `max-height: 320px`.
- `.table-scroll { overflow-x: auto; max-height: 60vh }` (126–129) — горизонтальный скролл-контейнер уже есть, нужен только sticky-левый столбец внутри него.
- `@media print` в проекте отсутствует полностью.

### 2.3. Ключевые ограничения дизайна

1. Рендер идёт на Rust-стороне; фронтенд делает `preview.innerHTML = html` (main.ts:42) и **не может** отличить «изменилась ли таблица» без собственного ключа. Отсюда — стабильный ключ таблицы (§4.1).
2. `lastRenderedHtml` (main.ts:23, 40) защищает от перезаписи DOM при неизменном HTML — этого недостаточно: правка в другом абзаце меняет весь HTML, и таблицы перетираются. Нужен точечный diff (T-3).
3. Ядро `md-core` возвращает HTML **без** обёрток блоков; совместимость с инспектором (`TZ-inspect-mode.md`, T-14) обязана сохраниться: `data-md` живёт на внешнем `div.md-block`, а `enhanceTables` переносит `<table>` внутрь `.table-enhanced`. Значит ключ таблицы должен вычисляться **до** мутирования DOM и не зависеть от обёрток.
4. CSP жёсткая: `default-src 'self'; style-src 'self' 'unsafe-inline'` (tauri.conf.json). Никаких внешних библиотек, никаких `eval`. Inline-стили для ширин колонок допустимы (`style-src 'unsafe-inline'`).
5. Тестовой инфраструктуры для фронта нет: в `package.json` только `dev`/`build`/`preview`; `node_modules/jsdom@29.1.1` установлен транзитивно, `vitest` — нет. CI-директории `.github/` нет вообще.
6. TypeScript строгий: `strict`, `noUnusedLocals`, `noUnusedParameters` (tsconfig.json) — новый код обязан это соблюдать, иначе `npm run build` (который вызывает `tsc`) упадёт.

---

## 3. Архитектура решения

```
┌──────────────────────────────────────────────────────────────────────┐
│ tables-core.ts  (НОВЫЙ, чистые функции, 0 обращений к document)      │
│   parseNumber · parseDate · toCellValue · detectColumnType           │
│   comparators · Predicate (num/text/date) + (де)сериализация         │
│   rowMatches(state) · multiSort(rows) · hashMd · toTSV · toMarkdown  │
│   types: TableKey, ColumnPredicate, SortLevel, PersistedTableState   │
├──────────────────────────────────────────────────────────────────────┤
│ tables-state.ts (НОВЫЙ)                                              │
│   Map<string, TableState> + sessionStorage (версия схемы, TTL, квота)│
│   computeTableKeys(html) — маппинг <table> → стабильный ключ         │
├──────────────────────────────────────────────────────────────────────┤
│ tables.ts       (РЕФАКТОРИНГ: только DOM-слой и события)             │
│   enhanceTables(root) идемпотентен · patchPreview(newHtml, root)     │
│   filter menu (2 режима) · highlight · copy/export · i18n · a11y     │
├──────────────────────────────────────────────────────────────────────┤
│ style.css: sticky-first-col, mark, print, zebra-var, col-resize      │
│ main.ts: 1 строка — patchPreview вместо innerHTML+enhanceTables      │
└──────────────────────────────────────────────────────────────────────┘
```

Правило зависимости: `tables.ts → tables-state.ts → tables-core.ts`; обратных импортов нет; `tables-core.ts` не знает про DOM и про Tauri — он единственный объект юнит-тестов.

### 3.1. Поток данных (после доработки)

```
[input в editor] → scheduleRender(120мс) → IPC render_markdown → newHtml
   → patchPreview(newHtml, preview):
        keys_new = computeTableKeys(newHtml)          // хэш исходника каждой таблицы
        diff old-vs-new по блокам/таблицам
        ├─ таблица не изменилась  → узел НЕ трогаем (состояние и DOM целые)
        ├─ таблица новая/изменилась → создаём/подменяем узел,
        │     state = store.get(key) ?? defaultState()
        └─ удалённая таблица → store.drop(key)
   → apply(entry) для затронутых таблиц (батч через DocumentFragment)
   → store.persist(sessionStorage, debounce 300мс)
[действие пользователя: сортировка/фильтр/поиск/ширина]
   → mutate state → apply(entry) → store.set(key, state) → persist()
```

---

## 4. Детальные требования

### 4.1. Приоритет 0 — корректность и долговечность состояния

**T-1. Стабильный ключ таблицы (замена WeakMap).**
- Новый модуль `crates/app/src/tables-state.ts`:
  ```ts
  export type TableKey = string;                 // "t:<fnv1a32hex>:<ordinal>"
  export function computeTableKeys(html: string): TableKey[];   // по порядку <table> в HTML
  export function hashSource(src: string): string;              // FNV-1a 32-bit → hex
  ```
- Алгоритм ключа: нормализовать HTML-фрагмент таблицы (`\s+` → один пробел, trim) → `hashSource`. порядковые номера (ordinal) нужны как тай-брейк для двух идентичных таблиц в одном документе: `t:<hash>:<i>`, где `i` — номер среди таблиц с тем же хэшем.
- Парсинг `html` для извлечения таблиц — через `new DOMParser().parseFromString(html, "text/html")` **один раз** за рендер, без записи в живой DOM.
- Реестр: `const registry = new Map<TableKey, TableEntry>()` (полная замена WeakMap, tables.ts:19). Дополнительно слабая ссылка `elToKey = new WeakMap<HTMLTableElement, TableKey>()` для быстрого поиска по узлу.
- **Инвариант:** ключ не зависит от позиции таблицы в DOM, от наличия обёрток `.table-enhanced`/`.md-block` и от содержимого других частей документа.

**T-2. Персистентность в `sessionStorage`.**
- Схема: `{ v: 1, doc: <hash редакторского текста или currentPath>, tables: Record<TableKey, PersistedTableState> }`, ключ хранения — `mdedit:tables:v1`.
- `PersistedTableState` — сериализуемое подмножество состояния: `sort: SortLevel[]`, `global: string`, `predicates: SerializedPredicate[]`, `widths: Record<number, number>`, `limit: number`. **DOM-узлов в состоянии быть не должно** (`baseOrder` хранится как `number[]` — исходные индексы строк).
- Запись — debounce 300 мс; чтение — синхронно при создании/восстановлении `TableEntry`.
- Защита: JSON.parse в try/catch, проверка `v === 1`, игнор повреждённых записей; при `QuotaExceededError` — сбросить `widths` и самые старые ключи, повторить одну попытку, затем молча деградировать до in-memory.
- Очистка: при смене документа (`currentPath`/`doc`-хэш отличается) — сброс хранилища; при удалении таблицы из markdown — `store.drop(key)`.
- Объём: хранить не более 50 таблиц и 200 predicates суммарно (защита от разрастания).

**T-3. Точечный diff-обновление вместо `innerHTML`.**
- Новая сигнатура в `tables.ts`:
  ```ts
  export function patchPreview(newHtml: string, root: HTMLElement): void;
  ```
- Логика: разобрать `newHtml` во «временный» документ, сопоставить блоки-предки таблиц (для обычного режима — все топ-узлы `body.children`; для mapped-режима — `div.md-block` по атрибуту `data-md`), далее:
  - узел-предок не изменился (сравнение `outerHTML`) → ничего не делаем;
  - изменился, но содержит таблицу с тем же ключом → обновляем только её внутренности (`replaceChildren` из нового узла) и **возвращаем** сохранённое состояние из `registry`/storage;
  - таблицы с новым ключом → создаём запись, применяем состояние по ключу (если есть) либо дефолт;
  - ключи, исчезшие из документа → удаляем запись и гасим подписку.
- `main.ts:42–43` (`preview.innerHTML = html; enhanceTables(preview);`) заменяется на `void patchPreview(html, preview)` — внутри нет await, функция синхронная; переменная `lastRenderedHtml` остаётся как быстрый путь «HTML не изменился → выходим» (главная защита от debounce-шторма).
- Обязан сохранять: `scrollTop`/`scrollLeft` контейнера `#preview` и `.table-scroll` (снять до диффа, вернуть после).

**T-4. Производительность на больших таблицах.**
- `apply()` переписать:
  - сборка нового порядка строк в `DocumentFragment` **одним** `appendChild(fragment)` вместо N `appendChild(row)` (сейчас tables.ts:160, 163);
  - видимость — класс `.row-hidden { display: none }` вместо атрибута `hidden` (tables.ts:146); переключение — через `classList.toggle("row-hidden", !show)`, при массовом применении — пакетно, без чтения layout-свойств в цикле;
  - типы колонок и значения ячеек кэшируются в `TableEntry.values: string[][]` и `TableEntry.kinds: ColumnType[]`, инвалидируются только при реальном изменении содержимого таблицы (флаг `dirtyValues`). Сейчас `apply()` пересчитывает `colValues`/`kinds` на каждый keystroke поиска (tables.ts:136–140) — убрать.
  - сортировка — по массиву индексов с предварительным извлечением ключей (Schwartzian transform), а не сравнением через `cellText()` внутри компаратора.
- Ленивая отрисовка хвоста: `entry.limit` (по умолчанию 500). Если видимых строк больше `limit` — показываем первые `limit` + строка-кнопка `.table-more` («Показать ещё 500»), счётчик пишет `${shown} из ${visible} (всего ${total})`. Порог включения — 2000 строк (ниже — рисуем всё).
- Бюджет (проверить `console.time` на фикстуре 3000×6): первичный `apply` ≤ 250 мс, повторный (только поиск) ≤ 60 мс. Виртуализация строк в объём **не** входит.

### 4.2. Приоритет 1 — паритет с Excel по интерактиву

**T-5. Предикатные фильтры (замена `Set<string>`).**
- Типизация в `tables-core.ts`:
  ```ts
  export type ColumnType = "num" | "date" | "str";
  export type Predicate =
    | { t: "in"; values: string[] }                                  // прежний чекбокс-режим
    | { t: "eq" | "gt" | "gte" | "lt" | "lte"; v: number }            // num
    | { t: "between"; a: number; b: number }                          // num
    | { t: "contains" | "startsWith" | "endsWith"; s: string }        // str
    | { t: "after" | "before"; d: number }                            // date (ms)
    | { t: "thisMonth" } | { t: "next30" };                           // date
  export type ColFilter = { op: "and"; preds: Predicate[] };
  // в TableState: predicates: Map<number, ColFilter>  (вместо colFilters)
  ```
- `rowMatches` (tables.ts:97–109) переезжает в `tables-core.ts` как чистая функция `matchesRow(values: string[], st: TableState, kinds: ColumnType[]): boolean`. Для чисел/дат сравнение идёт по распарсенному значению (`toCellValue`), если тип ячейки (`toCellValue().kind`) не совпадает с ожидаемым для предиката — строка не проходит (для `{ t: "in" }` сравнение остаётся строковым).
- Сериализация предикатов — плоский массив `{ col, ...predicate }`, пригодный для storage и для tooltip.
- UI (в `openFilterMenu`): над чекбокс-списком — раскрывающаяся секция (одна из трёх) «Числовые фильтры ▸» / «Текстовые фильтры ▸» / «Фильтры дат ▸» (показываемую определяет `detectColumnType` колонки). Внутри — `select` условия + 1–2 `input` (число/дата). Дата вводится в формате `dd.mm.yyyy`, разбирается тем же `parseDate`.
- Смешивание: `in` + предикат в одной колонке = И. Несколько предикатов одного столбца = И (Excel-совместимо).
- Обратная совместимость: при чтении storage версии `v: 0`/старого формата `Set` → конвертировать в `{ t: "in" }`.

**T-6. Подсветка совпадений поиска (`<mark>`).**
- Источник истины — `entry.values` (кэш исходного текста ячеек, T-4). DOM читается только при заполнении кэша, поэтому разметка `<mark>` никогда не попадает в данные для сортировки/фильтрации (прямое требование AC-8).
- `highlightCell(td, raw, query)`: собрать `textContent` из фрагментов, обернув все регистронезависимые вхождения в `<mark class="tbl-hit">`. Разметка только в текстовых узлах; содержимое `<code>`, `<a>`, `<strong>` не ломать — обходить childNodes рекурсивно, матчинг по конкатенированному тексту узла допустимо упростить до «размечаем только прямые текстовые узлы ячейки» (зафиксировать в README как ограничение v1).
- Снятие подсветки — сохранить оригинальный HTML ячейки в `Map<HTMLElement, string>` (или просто перечитать из `entry.values` и восстановить `td.textContent` для размеченных клеток; выбран второй вариант как дешёвый).
- Мульти-термин: запрос `a b` трактуется как И по всем словам. Это **изменение** текущего поведения (`text.includes(gq)` целиком, tables.ts:100–101) — вынесено в OQ-5; если OQ-5 решим «оставить как есть», подсветка ищутся только по полной подстроке.
- Стиль: `mark.tbl-hit { background: color-mix(in srgb, var(--accent) 35%, transparent); color: inherit; border-radius: 2px; }`.

**T-7. Многоярусная сортировка.**
- `TableState.sortCol/sortDir` → `sort: SortLevel[]`, `type SortLevel = { col: number; dir: 1 | -1 }`, максимум 4 уровня.
- Обычный клик по `th[i]`: цикл состояний как сейчас (без сортировки → ↑ → ↓ → без), но **сбрасывает** `sort` до одного уровня. Shift+клик: если `i` уже в `sort` — инвертировать его `dir`; иначе добавить уровень; если `i` — единственный и последний → удалить (возврат к исходному порядку).
- Компаратор: пройтись по `sort` слева направо, первый ненулевой результат решает; при полном равенстве — по исходному индексу строки (стабильность, см. tables.ts:158).
- Индикация: при `sort.length > 1` рядом со стрелкой выводить номер уровня (`sup`-элемент или CSS `content` с `attr(data-sort-rank)`), цвета accent; при одном уровне — только стрелка (текущее поведение).
- `aria-sort` ставить на все участвующие `th` (T-21).

**T-8. Sticky-первый столбец.**
```css
.table-enhanced .table-scroll.is-scrolled-x thead th:first-child,
.table-enhanced .table-scroll.is-scrolled-x tbody td:first-child { box-shadow: 6px 0 8px -6px rgba(0,0,0,.35); }
.table-enhanced thead th:first-child { left: 0; z-index: 2; }   /* выше остальных th (z-index:1) */
.table-enhanced tbody td:first-child { position: sticky; left: 0; background: var(--bg); }
```
- Фоновые цвета обязательны (иначе контент просвечивает); для zebra-строк использовать `color-mix(...)`-значение, совпадающее с рядом (T-14 — единый класс `.row-alt` + CSS-переменная `--row-bg`).
- Класс `.is-scrolled-x` навешивается на `.table-scroll` слушателем `scroll` (пассивным) при `scrollLeft > 0`, снимается при 0 — тень появляется только когда реально есть что перекрывать.
- Только первая колонка; включается исключительно внутри `.table-enhanced` (обычные таблицы не трогаем). Проверить, что `border-collapse: separate` (style.css:116) позволяет sticky без артефактов рамок.

### 4.3. Приоритет 2 — интеграция с приложением

**T-9. Копирование как TSV/CSV.**
- Кнопки в `.table-tools`: `⧉ TSV` и `⧉ CSV` (рядом со «Сбросить», справа от счётчика).
- Сериализация **видимых** строк (не скрытых фильтрами/поиском и не отсечённых `limit`) в текущем порядке сортировки; все колонки таблицы (скрытия колонок в v1 нет — см. §9).
- Экранирование обязательно: значение заключается в `"..."`, если содержит `"`/`\t`/`\n`/`\r`; внутренние `"` удваиваются; `\r\n` внутри значения запрещён (заменяется на ` `). Если хоть одно значение пришлось заключить в кавычки (содержит `\t`/`\n`/`"`) — показать `flash()` с тем же текстом. Для CSV разделитель задаётся параметром (`toDelimited(rows, delim)`), BOM не добавляем (Excel для `.csv` из буфера это не требует; при записи файла — см. T-10, там `.md`).
- Запись: `navigator.clipboard.writeText(text)` c fallback `document.execCommand("copy")` через временный `<textarea>` (WebView2 иногда режет clipboard без user-gesture — жалоба лечится flash «Не удалось скопировать»).
- Чистая функция `toDelimited(rows: string[][], delim: "\t" | ","): string` живёт в `tables-core.ts` и покрыта тестами (T-18): кейсы — таб внутри значения, перевод строки, кавычка, пустые ячейки, CRLF в конце.

**T-10. Экспорт отфильтрованного вида обратно в Markdown.**
- `toMarkdownTable(headers, rows, aligns): string` в `tables-core.ts`:
  - `|` в значениях → `\|`; переводы строк → `<br>`; ведущие/хвостовые пробелы trim;
  - выравнивание берём из inline-стилей ячеек: pulldown-cmark отдаёт `<td style="text-align: right">` (см. тест `renders_gfm_table` в `md-core/src/lib.rs`); в разделителе пишем `|:---|`, `|---:|`, `|:--:|`; если стиль не распознан — `|---|`;
  - колонки padding'ом до максимальной ширины значения (красивый diff в редакторе).
- Два действия: «Скопировать MD» (в буфер) и «Сохранить таблицу…» — диалог `save()` из `@tauri-apps/plugin-dialog` + `writeFile` (расширения `.md`/`.txt` уже разрешены `check_path_allowed`).
- Важно: экспорт не должен менять сам документ (нет `dirty = true`).

**T-11. Печать/PDF с учётом фильтров.**
```css
@media print {
  #toolbar, #editor, #statusbar, .table-tools, .col-filter-btn, .col-filter-menu,
  .table-more, .inspect-active { display: none !important; }
  #panes { display: block; }
  #preview { overflow: visible; padding: 0; }
  .table-scroll { max-height: none; overflow: visible; }
  .table-enhanced thead th, .table-enhanced tbody td:first-child { position: static; box-shadow: none; }
  .row-hidden { display: none !important; }
  mark.tbl-hit { background: none; }
  tr { break-inside: avoid; }
}
```
- Так как скрытие строк идёт классом (T-4), правило `.row-hidden` гарантирует печать только видимых строк. Перед `window.print()` временно снять `limit` (показать все видимые фильтром строки), после — восстановить (слушатель `afterprint`).
- Ctrl+P сейчас перехватывается (main.ts:171 → toggle предпросмотра). Решение: оставить Ctrl+P как есть (это переключатель панели), печать — через нативное меню WebView2; зафиксировать в OQ-6.

### 4.4. Приоритет 3 — UX-полировка

**T-12. Индикатор активности фильтра + tooltip.**
- Воронка при активных условиях: класс `.col-filter-btn.active` (жирная + точка `::after { content:"●" }`), `th.filtered` остаётся.
- `title` на воронке = человекочитаемая сводка: `Фильтр: > 10 И содержит "при"` (строки — из i18n-словаря, T-16). Обновляется в `apply()`.

**T-13. Запоминание ширины колонок.**
- Обработчик `pointerdown` на `th` в зоне 5 px от правой границы (курсор `col-resize`), `setPointerCapture`, drag переключает `table.style.tableLayout = "fixed"` и меняет ширину соответствующего `<col>` — при fixed layout этого достаточно, отдельные стили на `th`/`td` не нужны.
- Реализация через `<colgroup>` (создаётся лениво): `colgroup > col` по числу колонок, ширина пишется в `col.style.width` и в `state.widths[i]` → persistence (T-2).
- Двойной клик по границе — сброс (`widths[i]` удаляется, `tableLayout` обратно `auto`).
- Минимальная ширина 40 px, максимум 800 px; перетаскивание не должно запускать сортировку (`stopPropagation` + флаг `justResized`, проверяемый в click-хендлере `th`).

**T-14. Zebra-подсветка по видимому порядку + нумерация строк.**
- **Zebra (в объёме v1).** Сейчас `nth-child(2n)` (style.css:142) красит и строки, скрытые фильтром, поэтому после фильтрации чередование «съезжает». Решение: в `apply()` в том же проходе по строкам проставлять класс `.row-alt` только чётным **видимым** строкам; CSS — `.table-enhanced tbody tr.row-alt > td { background: var(--row-bg-alt) }`, где `--row-bg-alt: color-mix(in srgb, var(--toolbar-bg) 45%, transparent)` объявлена один раз и переиспользуется sticky-первым столбцом (T-8), иначе липкая ячейка рассинхронизируется по цвету с рядом. Для обычных (не-enhanced) таблиц `nth-child(2n)` оставляем как есть.
- Отключение zebra: `body.no-zebra .table-enhanced tbody tr.row-alt > td { background: var(--bg) }`, флаг из `localStorage["mdedit:zebra"]`.
- **Нумерация строк (v1.1, OQ-7).** Отдельный `<th class="row-no">`/`<td class="row-no">` не добавляем — сдвинет индексы колонок и сломает экспорт (T-10). Чистый CSS через `counter-increment` давал бы номера и по скрытым строкам (CSS-счётчик не знает про `display:none`). Корректный вариант: в том же проходе `apply()` писать номер в атрибут первой ячейки (`td:first-of-type.dataset.rowno`) и выводить его стилями `td[data-rowno]::before { content: attr(data-rowno) " "; opacity:.5 }`. Объём небольшой (+3 строки JS, +2 CSS), но трогает sticky-колонку и ширину первой ячейки — поэтому вынесено в v1.1.

**T-15. Клавиатура в меню фильтра.**
- `Esc` — закрыть меню (глобальный keydown, пока `openMenu !== null`; не конфликтует с инспектором: инспектор пропускает Esc, когда `.col-filter-menu` в DOM — ср. с обработкой Esc в `TZ-inspect-mode.md`, п. 4.7).
- `Tab` — цикл по control'ам меню (focus-trap, T-21); `↑/↓` — перемещение активной строки чекбокс-списка с прокруткой; `Space` — переключение; `Home/End` — крайние; `Enter` в полях предиката — применить.
- Убрать зависимость от автозакрытия по scroll: при скролле `#preview` меню не закрывать, а **пересчитывать** позицию (якорь же sticky в шапке таблицы). Метод: `requestAnimationFrame`-обновление `left/top` по `getBoundingClientRect()` якоря; закрытие — только по клику вне/Esc/повторному клику по воронке.

**T-16. i18n.**
- Новый файл `crates/app/src/i18n.ts`: `export const dict = { ru: {...}, en: {...} }; export function t(key: string): string;` — плоские ключи (`tools.searchPlaceholder`, `filter.all`, `filter.none`, `filter.reset`, `filter.num.*`, `filter.text.*`, `filter.date.*`, `count.of`, `count.rows`, `more.rows`, `copy.tsv`, …).
- Все строки из `tables.ts` (197, 210, 275, 303, 324–325) и новых элементов идут через `t()`. Язык: `navigator.language.startsWith("ru") ? "ru" : "en"` с возможностью переопределения через `localStorage["mdedit:lang"]`.
- Порядок слов в счётчике задаётся словарём (`"{n} из {m}"` / `"{n} of {m}"`), не конкатенацией.

### 4.5. Приоритет 4 — качество кода

**T-17. Разделение ответственности.**
- Создать `crates/app/src/tables-core.ts` (только чистые функции) и `crates/app/src/tables-state.ts` (registry + storage + ключи). `tables.ts` оставляет DOM, события, меню, тулбар.
- Переносимые в `tables-core.ts` чистые функции (сигнатуры фиксируем сразу, они же — объект тестов):
  ```ts
  parseNumber(text: string): number | null
  parseDate(text: string): number | null                 // epoch ms
  toCellValue(text: string): { kind: "num" | "date" | "str"; v: number | null; s: string }
  detectColumnType(values: string[]): ColumnType         // порог 0.7, как сейчас
  compareByKind(a: CellValue, b: CellValue, kind: ColumnType): number
  sortRowsIdx(rows: RowData[], levels: SortLevel[], kinds: ColumnType[]): number[]
  matchesRow(row: RowData, st: TableState, kinds: ColumnType[], now?: number): boolean
  hashNormalized(htmlFragment: string): string           // FNV-1a после нормализации whitespace
  toDelimited(rows: string[][], delim: "\t" | ","): string
  toMarkdownTable(headers: string[], rows: string[][], aligns: Align[]): string
  ```
  Текущий `detectColumnKind(colValues: string[][])` (tables.ts:64) переименовывается в `detectColumnType(values: string[])` — работает над колонкой, а не над матрицей; вызывающий код передаёт уже собранную колонку из кэша `entry.values`.
- Публичный API `tables.ts`: имена `enhanceTables(root)` и `attachMenuAutoClose(scroller)` сохраняются (вызываются из main.ts), добавляются `patchPreview(newHtml, root)` и `resetAllTables()`.

**T-18. Постоянные юнит-тесты (vitest + jsdom/happy-dom).**
- `crates/app/package.json`: devDependencies `vitest`, `jsdom` (закрепить явно установленную версию 29.1.1), scripts:
  ```json
  "test": "vitest run",
  "test:watch": "vitest",
  "coverage": "vitest run --coverage"
  ```
- `crates/app/vitest.config.ts`: `environment: "node"` по умолчанию (`tests/core/*` работают без DOM), для `tests/dom/*` — `// @vitest-environment jsdom` в шапке файла (проще, чем project-матрица). `include: ["tests/**/*.test.ts"]`.
- **Важно про jsdom:** он НЕ предоставляет `sessionStorage`/`localStorage` в среде `node` и не реализует `navigator.clipboard`; тесты `dom/state.test.ts` подменяют их ин-memory стабами (`Object.defineProperty(globalThis, "sessionStorage", { value: fakeStore() })`). Это же требует, чтобы продакшн-код обращался к хранилищу через маленькую обёртку `store.ts`-геттер, а не напрямую к глобалу.
- Каталог `crates/app/tests/` (в `tsconfig.json` добавить `"exclude": ["tests", "scripts"]`, иначе `npm run build` упадёт на тестах — в тестах используются глобалы jsdom и типы vitest):
  - `core/parse.test.ts` — числа: `1 234,56`, `1.234,56`, `-3,5`, `42%`, `₽`, `1,2.3`, пустая строка, `NaN`; даты: `01.10.2026`, `1.10.26`, `2026-10-01`, `2026-10-01T08:30`, невалидные `32.13.2026`.
  - `core/kind.test.ts` — пороги 70 %, пустые ячейки, смешанные колонки, полностью пустая колонка.
  - `core/compare.test.ts` — numeric-aware (`item2 < item10`), рус./англ. регистр, пустые в конце, распознанные первыми.
  - `core/predicates.test.ts` — все варианты `Predicate`, граничные `between`, `thisMonth` (подставить `now` через DI: `matchesRow(values, st, kinds, now)`).
  - `core/multisort.test.ts` — 2–3 уровня, стабильность, tie-break по исходному индексу.
  - `core/serialize.test.ts` — TSV/CSV экранирование, Markdown-экспорт (`,` в тексте, `|`, выравнивание, паддинг).
  - `core/hash.test.ts` — известное значение FNV-1a, инвариантность к `>\s+<`, разные таблицы → разные ключи.
  - `dom/enhance.test.ts` — AC-18 идемпотентность: два вызова → один `.table-tools`, один `.col-filter-btn` на `th`; отсутствие двойных listeners (проверка по одному клику = один переход состояния).
  - `dom/state.test.ts` — паттерн «симуляция ре-рендера»: сохранить состояние, заменить таблицу новым DOM с тем же исходником, вызвать `patchPreview` → состояние восстановлено (jsdom + `sessionStorage` mock).
- Не менее 30 тест-кейсов суммарно; `npm run test` обязан входить в CI.

**T-19. CI.**
- Новый `.github/workflows/ci.yml` (on: push/PR, две job'ы):
  - `rust`: `cargo test -p md-core` + `cargo clippy -p md-core -- -D warnings`. Компиляцию пакета `mdedit` (Tauri) **не** проверяем — на ubuntu-latest нет `libwebkit2gtk-4.1-dev`/`glib`; crate с Tauri-зависимостями остаётся на локальной машине разработчика.
  - `frontend`: `actions/setup-node@v4` (Node 20) → `npm ci` → `npm run build` (покрывает `tsc`) → `npm run test` → `npm run test:lint`, `working-directory: crates/app`.
  Кэш: `actions/cache` для `~/.cargo` и `crates/app/node_modules`.
- Это первый CI в репозитории (директории `.github/` нет) — учесть в плане файлов.

**T-20. Идемпотентность и mini-lint.**
- `enhance(table)` обязан корректно работать, если: таблица уже внутри `.table-enhanced` (выход без изменений), `thead` отсутствует (tables.ts:353 — выход), `thead` есть, но `th` уже украшены (не плодить `.col-filter-btn`).
- Тест-страховка (T-18) + проверка в рантайме: перед украшением `assert(th.querySelector(".col-filter-btn") === null)`.
- Mini-lint как скрипт `scripts/check-registry.mjs` (запускается из `npm run test:lint`): grep-правила — запрет `new WeakMap<HTMLTableElement` в `tables.ts` (сам `WeakMap<HTMLTableElement, TableKey>` в `tables-state.ts` разрешён и исключён из проверки), запрет литеральных русских строк в `tables.ts`/`inspector.ts` (требование i18n), запрет `innerHTML =` вне `patchPreview`. Реализация — 3 регулярки, без eslint-зависимостей (сохраняем минимализм проекта).

**T-21. A11y.**
- `th`: `aria-sort="ascending|descending|none"` проставляется **для всех уровней** `sort` (уровень 1 → его направление, остальные — тоже `ascending/descending`, SR читает их по порядку); `scope="col"`; `tabindex="0"` остаётся. `role="button"` c `th` **убрать** (табличный заголовок ≠ button, скринридер теряет семантику колонки) — достаточно `tabindex` + `aria-sort` + `title` с подсказкой «Сортировать / Shift+клик — добавить уровень».
- **Не** ставить `aria-label` на `th`: он перекроет видимый текст заголовка (требование WCAG 2.5.3 «Label in Name»). Имя колонки SR берёт из содержимого `th`, поэтому воронка `<span class="col-filter-btn" aria-hidden="true">▾</span>` обязана быть `aria-hidden`.
- У `th`, участвующих в мультисортировке, дополнительно `data-sort-rank` (для CSS-номера уровня, T-7) — атрибут не влияет на озвучку.
- Меню фильтра: `role="dialog"` + `aria-modal="true"` + `aria-label="Фильтр столбца {name}"`, focus-trap (первый/последний control замыкаются), возврат фокуса на воронку при закрытии.
- Счётчик: `aria-live="polite"` на `.table-count` (или отдельный visually-hidden live-region, чтобы не кричать на каждый keystroke — debounce 300 мс).
- Кнопки панелей: `aria-label`, различимый `:focus-visible` (в стиле существующего `outline: 2px solid var(--accent)`).
- `mark` не должен влиять на озвучку (ок по умолчанию).

### 4.6. IPC / Rust (минимальное вмешательство)

**T-22.** `md-core` **не изменяется**. Требуемые от ядра факты зафиксированы как есть: выравнивание колонок приходит как `<td style="text-align: right">` (см. тест `renders_gfm_table`), таблица — один `<table>` с `<thead>`; парсер детерминирован.
**T-23.** Опционально (только если решим писать экспорт в файл без plugin-dialog): новая команда `save_text(path, contents)` не нужна — достаточно существующего `write_file` + `plugin-dialog.save()`. Capability `default.json` расширять не требуется (`core:default` + `dialog:default` уже есть).
**T-24.** Clipboard: штатный `navigator.clipboard` доступен в WebView2; если Tauri потребует permission — добавить `core:clipboard-*` нельзя (их нет в core), значит остаётся execCommand-fallback из T-9. Проверить на целевой платформе вручную (см. §8, пункт 9).

### 4.7. Бюджеты производительности (критерии приёмки)

| Операция | Бюджет |
|----------|--------|
| `computeTableKeys` на документ 100 КБ / 20 таблиц | ≤ 15 мс |
| `patchPreview` при правке вне таблиц (0 изменённых таблиц) | ≤ 8 мс сверх IPC |
| `apply` на 3000×6 (сортировка + 2 фильтра) | ≤ 250 мс, без layout-thrashing (замер `performance.measure`) |
| Повторный `apply` только по поиску (кэш значений валиден) | ≤ 60 мс |
| Хранение состояния: 50 таблиц × 4 predicate | ≤ 32 КБ sessionStorage |
| Режим «выключен» (нет таблиц в документе) | 0 дополнительных слушателей, 0 записей в storage |

---

## 5. Ограничения, риски, открытые вопросы

### 5.1. Ограничения (зафиксировать в README)
- Гранулярность состояния — таблица целиком; ячейки не редактируются (это не spreadsheet-приложение).
- Нет скрытия/перестановки колонок drag'n'drop, нет формул, нет объединения ячеек, нет виртуализации строк.
- Подсветка `<mark>` — только внутри прямых текстовых узлов ячеек; ссылки/код внутри ячейки не размечаются (v1).
- Ширина колонок хранится per-document-session; между разными файлами не переносится.
- Экспорт Markdown не сохраняет форматирование ячеек (жирный/код) — только текст.
- Две идентичные таблицы в одном документе разделяют состояние (одинаковый ключ, см. T-1): отфильтровав одну, получаем ту же фильтрацию у другой при следующем появлении. Это осознанное решение, а не баг.

### 5.2. Риски

| Риск | Вероятность | Митигция |
|------|-------------|----------|
| Хэш таблицы нестабилен из-за незначительных изменений HTML (например, перенос строки внутри ячейки) | средняя | нормализация whitespace в T-1; ключ меняется → состояние сбрасывается, что приемлемо; тест на инвариантность |
| Две идентичные таблицы в документе конфликтуют по ключу | средняя | ordinal-тай-брейк `:i` (T-1); состояние разделяется осознанно (одинаковый исходник = одинаковое состояние) |
| Diff-патч ломает совместимость с инспектором (`data-md` на обёртке) | высокая | T-3 работает по блокам-предкам; E2E-проверка AC-19; при mapped-режиме ключ сопоставляется внутри `div.md-block` |
| `sessionStorage` недоступен/переполнен в WebView2 | низкая | try/catch + деградация в Map в памяти (T-2) |
| Sticky-первый столбец конфликтует с `border-collapse`/рамками | средняя | `border-collapse: separate` уже стоит; проверить зрительно в light/dark; при артефактах — рамки через `box-shadow` |
| Класс `.row-alt` вместо `nth-child` увеличит работу `apply` | низкая | проставляется в том же проходе, что и видимость; O(n) |
| Drag-ресайз случайно триггерит сортировку | средняя | флаг `justResized` + `stopPropagation` (T-13) |
| Перехват `Esc` конфликтует с инспектором | средняя | очерёдность: сначала закрываем меню фильтра, инспектор обрабатывает Esc только если меню нет (см. `TZ-inspect-mode.md`, п. 4.7) |
| Рост сложности `tables.ts` (сейчас 413 строк) | высокая | обязательное дробление на 3 модуля до начала работ по P1 (T-17 первым делом) |
| Тесты в jsdom не ловят реальные layout-проблемы sticky | средняя | ручной чек-лист §8; автотесты только для логики |

### 5.3. Открытые вопросы (решить до старта реализации)

| # | Вопрос | Варианты | Рекомендация | Блокит |
|---|--------|----------|--------------|--------|
| OQ-1 | Ключ таблицы: хэш HTML-фрагмента или markdown-исходника? | (a) HTML-фрагмент — чисто на фронте; (b) markdown-ranges — требует правки `md-core` (противоречит §4.6) | **(a)** + ordinal-тай-брейк | T-1 |
| OQ-2 | Неймспейс хранилища при появлении вкладок/нескольких открытых файлов | (a) один ключ на приложение; (b) `mdedit:tables:v1:<docHash>` | **(b)** — формат готовим сразу, иначе миграция схемы | T-2 |
| OQ-3 | Судьба состояния при изменении исходника той же таблицы | (a) новый хэш → дефолт; (b) применять состояние по ordinal игнорируя содержимое; (c) гибрид: хранить состояние с TTL 10 мин и восстанавливать, если хэш вернулся к прежнему | **(c)**, но TTL не больше 10 мин (иначе «призрачные» фильтры после отката текста) | T-1, T-2, AC-2 |
| OQ-4 | Параметры ленивой отрисовки | шаг 500 / порог 2000 строк vs шаг 1000 / порог 3000 | **500 / 2000**, константами в `tables-core.ts` (`TAIL_STEP`, `TAIL_THRESHOLD`) для тюнинга | T-4 |
| OQ-5 | Семантика глобального поиска | (a) текущая подстрока целиком; (b) AND по словам + подсветка каждого слова | **(b)** — иначе T-6 подсвечивает только длинный запрос и выглядит сломанной | T-5, T-6 |
| OQ-6 | Хоткей печати (Ctrl+P занят переключателем предпросмотра) | (a) оставить Ctrl+P, печать из меню WebView2; (b) Ctrl+Shift+P → `window.print()` | **(b)** + обновить `title` кнопки/подсказку в `index.html` | T-11 |
| OQ-7 | Нумерация строк в v1 или v1.1 | — | **v1.1** (затрагивает sticky-колонку и ширину первой ячейки) | T-14 |
| OQ-8 | Экспорт: буфер обмена и/или файл | (a) только буфер; (b) обе кнопки | **(b)**: «Скопировать MD» + «Сохранить таблицу…» через `plugin-dialog.save()` | T-10 |
| OQ-9 | Нужно ли показывать число скрытых поиском/фильтром строк отдельно | (a) одна строка счётчика; (b) `видно N · отфильтровано M · всего K` | **(a)** для v1, живёт в словаре i18n (T-16), расширять без смены разметки | T-16 |

---

## 6. План файлов

| Файл | Действие |
|------|----------|
| `crates/app/src/tables-core.ts` | **новый**: чистые функции (парсинг, типы, компараторы, предикаты, `matchesRow`, `sortRowsIdx`, `toDelimited`, `toMarkdownTable`, `hashNormalized`) ~300 строк |
| `crates/app/src/tables-state.ts` | **новый**: `TableKey`, `registry: Map<TableKey, TableEntry>`, `computeTableKeys`, `loadState/saveState` (sessionStorage-обёртка) ~150 строк |
| `crates/app/src/tables.ts` | рефакторинг: удаление WeakMap, `patchPreview`, предикатное меню, `<mark>`, мультисортировка, тулбар копи/экспорта, i18n, a11y, limit-строка (~600 строк) |
| `crates/app/src/i18n.ts` | **новый**: словарь ru/en + `t()` ~60 строк |
| `crates/app/src/main.ts` | `innerHTML`+`enhanceTables` → `patchPreview`; Ctrl+Shift+P; `store.resetOnDocChange()` (~15 строк diff) |
| `crates/app/src/style.css` | `.row-hidden`, `.row-alt`, `mark.tbl-hit`, sticky-first-col + тень, `.col-filter-btn.active`, resizer, `@media print`, `.table-more` (+~90 строк) |
| `crates/app/index.html` | без изменений (панель таблиц строится в JS). При решении OQ-6=(b): добавить подсказку «Ctrl+Shift+P — печать» в `title` кнопки предпросмотра и в `#statusbar`-подсказки |
| `crates/app/tests/**` | **новые**: `core/*.test.ts`, `dom/*.test.ts` (≥30 кейсов) |
| `crates/app/vitest.config.ts` | **новый** |
| `crates/app/package.json` | +`vitest`, +`jsdom` (devDep), scripts `test`/`test:watch`/`coverage`/`test:lint` |
| `crates/app/tsconfig.json` | `"exclude": ["tests", "scripts"]` (иначе `tsc` падает на тестах) |
| `crates/app/scripts/check-registry.mjs` | **новый**: mini-lint (T-20) |
| `.github/workflows/ci.yml` | **новый**: rust + frontend jobs |
| `README.md` | секция «Таблицы как в Excel»: список возможностей, ограничения, как запустить тесты |
| `crates/md-core/**`, `crates/app/src-tauri/**` | **без изменений** (кроме случаев, когда OQ-1/OQ-8 потребуют IPC — тогда отдельный RFC) |

Новые npm-зависимости: только dev (`vitest`, явно `jsdom`). Runtime-зависимостей нет. Cargo-зависимостей нет.

---

## 7. Спринты и трудоёмкость

| Спринт | Содержание | Почему именно так | Оценка |
|--------|-----------|-------------------|--------|
| 0 | T-17 (дробление на 3 модуля) + T-18 каркас vitest + T-19 CI | Иначе все дальнейшие правки падают в нечитаемый 400-строчный файл без тестов | 0.5 дня |
| 1 | T-1, T-2, T-3 (стабильный ключ + storage + точечный diff) | Фундамент: без него любые фичи теряются при наборе текста | 1–1.5 дня |
| 2 | T-4 (батчинг, кэш значений, `.row-hidden`, lazy tail) | Большие таблицы — источник жалоб; плюс ускоряет всё дальнейшее | 0.5–1 день |
| 3 | T-5 (предикатные фильтры) | Самый большой разрыв с Excel | 1–1.5 дня |
| 4 | T-6 + T-8 (подсветка, sticky-столбец) | Быстрые видимые улучшения | 0.5 дня |
| 5 | T-7 (мульти-сортировка) | Замыкает сортировку до паритета с Excel | 0.5–1 день |
| 6 | T-9, T-10, T-11 (TSV/CSV, экспорт MD, печать) + проверка clipboard/print в WebView2 (T-24) | Мост к реальному Excel | 1 день |
| 7 | T-12…T-16 (индикаторы, ширины, zebra/номера, клавиатура, i18n) | Полировка, растёт LTV фичи | 1–1.5 дня |
| 8 | T-20, T-21 (идемпотентность, mini-lint, a11y) + добор тестов + README | Качество и приёмка | 0.5–1 день |
| **Итого** | | | **≈ 7–9 рабочих дней** |

**Definition of Done спринта 1:** AC-1, AC-2, AC-3 проходятся вручную на стартовом документе main.ts (добавить в него вторую таблицу и абзац для проверки «правка вне таблицы не сбрасывает состояние»); тест `dom/state.test.ts` зелёный.
**Definition of Done всего:** все AC-1…AC-20, бюджеты из §4.7 замерены и записаны в PR, `npm run test` + `cargo test -p md-core` зелёные в CI.

---

## 8. Приёмочное тестирование (чеклист)

**Ручные сценарии (Windows/WebView2 + Linux/webkit2gtk):**
1. Стартовый документ: добавить строку текста в раздел «Прочее» → сортировка и фильтр таблицы не сбросились (AC-1).
2. Изменить число в таблице → состояние этой таблицы дефолтится согласно OQ-3, соседняя таблица цела (AC-2).
3. Свернуть/перекрыть окно, вернуть → фильтры на месте (AC-3).
4. Прогнать AC-4…AC-7 на таблице с колонками: текст, число с плавающей запятой (`3,5`), дата (`01.10.2026`), проценты.
5. Поиск `ma` → `<mark>`; очистить поле → `<mark>` нет; отсортировать ту же колонку → порядок идентичен тому, что был до подсветки (AC-8).
6. Shift+клик по трём заголовкам, проверить номера уровней и результат; Esc-сценарий меню (AC-9, AC-16).
7. Таблица 8+ колонок: горизонтальный скролл — первый столбец липкий, тень появляется/исчезает (AC-10).
8. Сгенерировать fixture 3000 строк (скрипт в `tests/fixtures/`), замерить `console.time` для сортировки/фильтра/поиска (AC-11, бюджеты из §4.7); проверить кнопку «Показать ещё».
9. «Копировать TSV» → вставка в Excel: колонки на местах, значения с табами/кавычками корректны (AC-12). Проверить clipboard в WebView2 отдельно.
10. «Экспорт Markdown» → вставить в редактор: таблица парсится pulldown-cmark, выравнивание сохранено, `|` внутри значений экранирован (AC-13).
11. Печать в PDF с активным фильтром: только видимые строки, без панелей и маркеров (AC-14).
12. Drag границы колонки, ре-рендер, двойной клик по границе (AC-15).
13. Включённый инспектор (`TZ-inspect-mode.md`): hover по ячейке подсвечивает табличный блок, клик по `th` сортирует, Esc закрывает меню раньше, чем выключает инспектор (AC-19).
14. Тёмная тема: sticky-первый столбец, `<mark>`, тень, zebra — читаемы.
15. Кириллица/эмодзи в значениях: сортировка, фильтры, экспорт, подсветка — без битых границ.
16. Документ без таблиц: ни одного лишнего слушателя/записи в storage (проверить Application → Storage).

**Автоматические:**
- `cd crates/app && npm run test` — ≥30 кейсов, включая `dom/enhance.test.ts` на идемпотентность (AC-18).
- `npm run test:lint` — mini-lint без нарушений.
- `npm run build` — `tsc` без ошибок (strict, noUnusedLocals).
- `cargo test -p md-core` — зелёный (регрессия: ядро не тронуто).
- CI workflow проходит на push (T-19).

---

## 9. Что сознательно НЕ входит в v1

- Виртуализация строк (окно отрисовки) и колонок; lazy tail из T-4 достаточен.
- Редактирование ячеек в предпросмотре и обратная запись в markdown (round-trip).
- Формулы, сводные таблицы, условное форматирование, закрепление областей (freeze panes) кроме первой колонки/шапки.
- Скрытие, показ и drag-перестановка колонок.
- Импорт XLSX/CSV (только экспорт).
- Сохранение состояния таблиц между разными документами и в самом `.md`-файле (метакомментарии вида `<!-- mdedit:table ... -->` — кандидаты в v2; неймспейс хранилища под это уже готовим, см. OQ-2).
- Полноценный ESLint/Prettier-конфиг (остаёмся на mini-lint, чтобы не раздувать dev-зависимости).
- Мультиязычность помимо ru/en.
