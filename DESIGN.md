---
version: alpha
name: darmark
description: Инструментальная идентичность редактора Markdown в духе Notepad++ — холодные глубокие поверхности, фиолетовый акцент, ролевые гарнитуры (JetBrains Mono / Inter / Source Serif 4), ручной тумблер тёмной/светлой темы.
colors:
  # акцент и семантические статусы (style.css :root)
  primary: "#9d8fff"
  secondary: "#6ea8fe"
  info: "#6ea8fe"
  success: "#34d399"
  warning: "#fbbf24"
  error: "#f87171"
  disabled: "#6b7280"
  link: "#6ea8fe"
  code: "#e9b3ff"
  accent-soft: "rgba(157, 143, 255, 0.14)"
  on-accent: "#17121f"
  on-error: "#2a0f0f"
  on-warning: "#1a1400"
  # тёмная тема — поверхности и границы (по умолчанию)
  background: "#111318"
  surface: "#161920"
  surface-raised: "#1c1f28"
  surface-overlay: "#242836"
  gutter: "#0f1014"
  popover: "#181b24"
  border: "#2e3348"
  border-soft: "#232736"
  chrome: "#0d0e12"
  # тёмная тема — текст
  foreground: "#e8eaf0"
  foreground-muted: "#a0a6bd"
  foreground-subtle: "#8a90b0"
  heading: "#ffffff"
  body-text: "#d0d3e0"
  # светлая тема — поверхности и границы
  light-background: "#fafbfc"
  light-surface: "#f0f2f6"
  light-surface-raised: "#e3e7ee"
  light-surface-overlay: "#d4d9e3"
  light-gutter: "#d8dde6"
  light-border: "#b8bfcc"
  light-border-soft: "#d0d5e0"
  # светлая тема — текст
  light-foreground: "#1a1d24"
  light-foreground-muted: "#4a5068"
  light-foreground-subtle: "#6b7194"
typography:
  ui:
    fontFamily: '-apple-system, "Segoe UI", system-ui, Roboto, Arial, sans-serif'
    fontSize: 13px
    fontWeight: 400
    lineHeight: 1.4
  editor:
    fontFamily: '"JetBrains Mono", "Cascadia Code", ui-monospace, Consolas, monospace'
    fontSize: 14.5px
    fontWeight: 400
    lineHeight: 26px
  mono:
    fontFamily: '"JetBrains Mono", "Cascadia Code", ui-monospace, Consolas, monospace'
    fontSize: 11px
    fontWeight: 400
    lineHeight: 1.5
  heading-1:
    fontFamily: '"Source Serif 4", Georgia, "Times New Roman", serif'
    fontSize: 30px
    fontWeight: 700
    lineHeight: 1.22
    letterSpacing: -0.01em
  heading-2:
    fontFamily: '"Source Serif 4", Georgia, "Times New Roman", serif'
    fontSize: 21px
    fontWeight: 700
    lineHeight: 1.3
  heading-3:
    fontFamily: '"Source Serif 4", Georgia, "Times New Roman", serif'
    fontSize: 17px
    fontWeight: 700
    lineHeight: 1.32
  body:
    fontFamily: 'Inter, -apple-system, "Segoe UI", system-ui, Roboto, Arial, sans-serif'
    fontSize: 13px
    fontWeight: 400
    lineHeight: 1.7
  label:
    fontFamily: '-apple-system, "Segoe UI", system-ui, sans-serif'
    fontSize: 11px
    fontWeight: 500
    lineHeight: 1.35
  caption:
    fontFamily: '-apple-system, "Segoe UI", system-ui, sans-serif'
    fontSize: 10.5px
    fontWeight: 400
    lineHeight: 1.4
rounded:
  sm: 5px
  md: 7px
  lg: 12px
  pill: 999px
spacing:
  xxs: 2px
  xs: 4px
  sm: 6px
  md: 8px
  lg: 12px
  xl: 16px
  xxl: 24px
  toolbar: 36px
  tabs: 32px
  statusbar: 24px
  dock-bar: 28px
  dock: 300px
  drawer: 440px
  editor-gutter: 20px
components:
  button-primary:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.on-accent}"
    typography: "{typography.label}"
    rounded: "{rounded.sm}"
    padding: "4px 10px"
  button-primary-hover:
    backgroundColor: "#ab9fff"
  button-secondary:
    backgroundColor: "{colors.surface-raised}"
    textColor: "{colors.foreground-muted}"
    typography: "{typography.label}"
    rounded: "{rounded.sm}"
    padding: "4px 10px"
  button-secondary-hover:
    backgroundColor: "{colors.surface-overlay}"
    textColor: "{colors.foreground}"
  button-danger:
    backgroundColor: "transparent"
    textColor: "{colors.error}"
    typography: "{typography.label}"
    rounded: "{rounded.sm}"
    padding: "4px 10px"
  button-danger-hover:
    backgroundColor: "rgba(248, 113, 113, 0.12)"
  button-icon:
    backgroundColor: "transparent"
    textColor: "{colors.foreground-muted}"
    rounded: "{rounded.sm}"
    size: 28px
  input:
    backgroundColor: "{colors.surface-raised}"
    textColor: "{colors.foreground}"
    typography: "{typography.ui}"
    rounded: "{rounded.sm}"
    padding: "6px 9px"
  textarea:
    backgroundColor: "{colors.surface-raised}"
    textColor: "{colors.foreground}"
    typography: "{typography.ui}"
    rounded: "{rounded.md}"
    padding: "6px 10px"
  tab:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.foreground-subtle}"
    typography: "{typography.label}"
    height: "{spacing.tabs}"
    padding: "0 12px"
  tab-active:
    backgroundColor: "{colors.background}"
    textColor: "{colors.foreground}"
  toolbar:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.foreground}"
    height: "{spacing.toolbar}"
    padding: "0 8px"
  statusbar:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.foreground-subtle}"
    typography: "{typography.label}"
    height: "{spacing.statusbar}"
    padding: "0 6px"
  pane:
    backgroundColor: "{colors.background}"
    textColor: "{colors.body-text}"
  drawer:
    backgroundColor: "{colors.popover}"
    textColor: "{colors.foreground}"
    width: "{spacing.drawer}"
  dock:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.foreground}"
    height: "{spacing.dock}"
  badge:
    backgroundColor: "{colors.surface-overlay}"
    textColor: "{colors.foreground-muted}"
    typography: "{typography.caption}"
    rounded: "{rounded.pill}"
    padding: "0 5px"
  badge-error:
    backgroundColor: "{colors.error}"
    textColor: "{colors.on-error}"
  badge-warning:
    backgroundColor: "{colors.warning}"
    textColor: "{colors.on-warning}"
  divider:
    backgroundColor: "{colors.border-soft}"
  divider-strong:
    backgroundColor: "{colors.border}"
  line-numbers:
    backgroundColor: "{colors.gutter}"
    textColor: "{colors.foreground-subtle}"
    typography: "{typography.mono}"
  inline-code:
    backgroundColor: "{colors.surface-overlay}"
    textColor: "{colors.code}"
    rounded: "{rounded.sm}"
    padding: "2px 6px"
  link:
    textColor: "{colors.link}"
  heading:
    textColor: "{colors.heading}"
    typography: "{typography.heading-1}"
  blockquote:
    backgroundColor: "{colors.accent-soft}"
  status-dot:
    backgroundColor: "{colors.success}"
    size: 6px
    rounded: "{rounded.pill}"
  status-dot-warning:
    backgroundColor: "{colors.warning}"
  status-dot-error:
    backgroundColor: "{colors.error}"
  status-dot-off:
    backgroundColor: "{colors.disabled}"
  log-dot-document:
    backgroundColor: "{colors.info}"
---

## Overview

**ПЕНАЛ — «Notepad++ для Markdown».** Это десктопный инструмент для Windows, а не веб-сайт: слева редактор, справа HTML-предпросмотр, между ними — оболочка приложения. Визуальная идентичность вырастает из предметной области (текст и код), поэтому интерфейс ощущается как рабочий инструмент, а не как лендинг.

Принципы, задающие каждое решение:

- **Минимум хрома.** Тулбар, вкладки, панели и статусбар — тихие, приглушённые, без декорации. Никаких фреймворков и тяжёлых визуальных эффектов; интерфейс — vanilla TS + CSS.
- **Две темы, ручной тумблер.** Тёмная тема — по умолчанию (норма для инструмента). Светлая тема вводится ручным переключателем (`data-theme="dark|light"`), без `prefers-color-scheme`; выбор сохраняется между сессиями (ADR-0037).
- **Один акцент.** Фиолетовый (`primary`) — единственный яркий цвет. Всё остальное — приглушённые серо-синие тона с холодным подтоном (ADR-0024).
- **Структура = информация.** Разделители (`border-bottom` секций, `border-left` панелей, `border-right` вкладок, `.div-v` между группами кнопок) несут смысл, а не только разделяют.
- **Честность отказов.** Ошибки уходят в статусбар и в журнал; окно не падает. Сбой плагина — это состояние (точка и слово), а не разрушение интерфейса.

Идентичность — рабочая, собранная, «редакторская»: плотная, но не тесная; контрастная там, где нужно читать код, и намеренно тихая во всём остальном.

**Логотип.** В `crates/app/public/` три файла: `logo.svg` — цветной знак (градиент `#fd7128 → #651d75`); `logo_w.svg` — белый моно-знак для тёмного фона; `penal_text.svg` — горизонтальный вордмарк «ПЕНАЛ» (буквы П, Е, Н, вместо А — знак, Л лежит на боку). Применение: тулбар — белый знак (`logo_w.svg`, высота 20px); иконка приложения и favicon — белый знак на плите `#0d0e12` (`crates/app/src-tauri/icons/`); README — вордмарк. Логотип не элемент UI-палитры и не второй акцент — его цвета внутрь интерфейса не переносятся (см. «Один акцент»).

Нормативная спецификация UI/layout — `docs/DESIGN_DOC.md` (§5 — layout/pane/view, §13.2 — замороженные e2e-контракты). Правила кода — `AGENTS.md`. Бэклог идей — `docs/BACKLOG.md`. Источник истины токенов — `crates/app/src/style.css`.

## Colors

Палитра построена на холодных глубоких поверхностях с синим подтоном и **единственном** фиолетовом акценте. Две темы: тёмная (по умолчанию) и светлая (ручной тумблер, ADR-0037).

### Тёмная тема (по умолчанию)

- **Primary (#9d8fff):** «ПЕНАЛ Violet» — единственный драйвер взаимодействия. Активная вкладка (полоска сверху), выбранная кнопка, каретка редактора, ссылки в предпросмотре, точка плагина (`--pc`). Только акцент и действие.
- **Secondary (#6ea8fe):** холодный синий — информационный акцент (ссылки, `--info`, инлайн-подсказки, origin-точка документа). Не конкурирует с фиолетовым за роль «главного действия».
- **Success (#34d399):** зелёный — успех и активное состояние (плагин активен, право выдано, задача выполнена).
- **Warning (#fbbf24):** янтарный — предупреждение и «изменено» (несохранённый документ, карантин, лимиты). Текст на янтарном — `on-warning` (#1a1400).
- **Error (#f87171):** красный — ошибка, отказ, деструктивное действие (сбой плагина, удаление, кнопка «Удалить»).
- **Background (#111318):** `--bg-0` — базовая поверхность рабочей области, редактора, предпросмотра.
- **Surface (#161920):** `--bg-1` — оболочка (тулбар, вкладки, статусбар, док, шапка дровера).
- **Surface-raised (#1c1f28):** `--bg-2` — приподнятые элементы (вторичные кнопки, поля ввода, шапки таблиц).
- **Surface-overlay (#242836):** `--bg-3` — hover-состояния, инлайн-код, бейджи.
- **Gutter (#0f1014):** фон колонки номеров строк — темнее рабочей области, чтобы отделить геометрию редактора.
- **Popover (#181b24):** поверхность поповеров, дровера, диалогов и тостов.
- **Border (#2e3348) / Border-soft (#232736):** `--line` / `--line-soft` — разделители и границы; `border-soft` — для тихих внутренних швов.
- **Foreground (#e8eaf0):** основной текст. **Foreground-muted (#a0a6bd):** вторичный (`--fg-dim`). **Foreground-subtle (#8a90b0):** третичный (`--fg-mute`, WCAG AA ≥ 4.5:1 на всех поверхностях `--bg-0..--bg-3`).
- **Heading (#ffffff):** заголовки и акцентные подписи. **Body-text (#d0d3e0):** текст предпросмотра Markdown (`--body`) — мягче основного.
- **Code (#e9b3ff):** инлайн-код в предпросмотре.
- **Chrome (#0d0e12):** `<meta name="theme-color">` в `index.html` — цвет системного хрома окна WebView2. Темнее `background`, потому что относится к рамке ОС, а не к рабочей области.

### Светлая тема (ручной тумблер)

Слои разведены, все текстовые пары ≥ 4.5:1 (WCAG AA), сохранён «холодный» подтон:

| Токен | Значение | Роль |
|-------|----------|------|
| `--bg-0` | `#fafbfc` | базовый фон приложения |
| `--bg-1` | `#f0f2f6` | панели, тулбар, вкладки |
| `--bg-2` | `#e3e7ee` | карточки, строки списков |
| `--bg-3` | `#d4d9e3` | hover-состояния, кнопки |
| `--line` | `#b8bfcc` | границы, разделители |
| `--line-soft` | `#d0d5e0` | тонкие границы, подложки |
| `--gutter` | `#d8dde6` | фон колонки номеров строк (темнее рабочей области) |
| `--fg` | `#1a1d24` | основной текст |
| `--fg-dim` | `#4a5068` | вторичный текст |
| `--fg-mute` | `#6b7194` | третий уровень, статус «выкл» |

Акцент и статусы сохраняются из тёмной темы (фиолетовый `--accent`, `--green`, `--amber`, `--red`, `--teal`).

### Правило `--disabled`

`--disabled` применяется **только к точкам/иконкам**, НЕ к цвету текста. Текст статуса «выкл» → `--fg-mute`. Класс `.sem.off` для точек-индикаторов: цвет фона/границы, не текст.

## Typography

Четыре ролевые гарнитуры с чётким разделением ролей (ADR-0037):

- **UI (системная):** `-apple-system, "Segoe UI", system-ui, Roboto, Arial, sans-serif` — 13px/1.4. Вся оболочка: тулбар, вкладки, панели, кнопки, статусбар. Системный шрифт делает интерфейс «нативным» для Windows.
- **Editor (моноширинная):** `"JetBrains Mono", "Cascadia Code", ui-monospace, Consolas, monospace` — 14.5px/26px. Редактор, номера строк, статусы, метрики, технические данные (id плагина, время, счётчики). Встроенный `.woff2` (OFL 1.1). Моноширинный шрифт — маркер предметной области: код и данные.
- **Headings (Source Serif 4):** засечки, визуально отделены от текста оболочки. Встроенный `.woff2` (OFL 1.1). Кегли:

  | Элемент | Кегль | line-height | Вес | Граница |
  |----------|-------|------------|-----|---------|
  | h1 | 30px | 1.22 | 700 | `border-bottom: 1px solid var(--line)` |
  | h2 | 21px | 1.30 | 700 | `border-bottom: 1px solid var(--line-soft)` |
  | h3 | 17px | 1.32 | 700 | — |

- **Body (Inter):** `Inter, -apple-system, "Segoe UI", system-ui, Roboto, Arial, sans-serif` — 13px/1.7 в предпросмотре — длинное чтение. Встроенный `.woff2` (OFL 1.1). Плотность предпросмотра — осознанная: `max-width: 860px`.
- **Labels / Caption:** 11px и 10.5px — подписи, метрики, вторичные данные. Метки форм могут быть uppercase с трекингом (`pop-label`).

Числа всегда идут с `font-variant-numeric: tabular-nums` (статусбар, метрики мониторинга, версии), чтобы значения не «плясали» при обновлении.

**Лигатуры:** в редакторе/коде — `font-feature-settings: "liga" 1, "calt" 0`; в предпросмотре — `calt` включён.

## Layout

Оболочка — вертикальная flex-колонка: тулбар → рабочая область → нижний док → статусбар. Рабочая область — горизонтальный сплит из максимум двух панелей (`MAX_PANES = 2`) с разделителем 1px.

Ключевые метрики (реальные токены оболочки):

- **Toolbar** — 36px, фон `surface`, нижняя граница `border-soft`.
- **Tabs** — 32px; шапка панели — та же высота.
- **Statusbar** — 24px, фон `surface`, верхняя граница `border-soft`.
- **Dock** — развёрнут 300px, свёрнут в полосу-шапку 28px (`dock-bar`). Раскрытие сжимает рабочую область, а не накрывает её (не overlay).
- **Drawer** — 440px справа; при узком окне (< 900px) — на всю ширину.

Ритм отступов держится на шкале 2/4/6/8/12/16/24px; крупные воздушные зазоры (34px по бокам предпросмотра, 20px по вертикали редактора) — исключения, привязанные к геометрии текста. Плотность высокая: это инструмент, а не витрина.

## Elevation & Depth

Глубина выражается **тональными слоями и границами**, а не тяжёлыми тенями. Иерархия поверхностей: `background` → `surface` → `surface-raised` → `surface-overlay`; каждое «выше» — светлее.

Тени применяются точечно и только для реально всплывающих слоёв:

- `--shadow-lg` (`0 20px 60px rgba(0,0,0,0.55)`) — дровер, диалоги, поповеры.
- `--shadow-sm` (`0 6px 18px rgba(0,0,0,0.35)`) — тосты и мелкие поповеры.

Backdrop'ы (дровер, модальные диалоги) — полупрозрачный чёрный (0.42–0.45), не размытие. Вложенные элементы (карточки плагинов, шапки таблиц) отделяются фоном и `border-soft`, без теней.

## Shapes

Форма — **сдержанная скруглённость**: радиусы заданы тремя токенами.

- **sm (5px)** — кнопки, поля ввода, мелкие чипы, строки списков.
- **md (7px)** — более крупные карточки, пузыри чата, кнопки плагинных view.
- **lg (12px)** — диалоги, палитра команд, поповеры, дровер.
- **pill (999px)** — бейджи-счётчики и чипы скиллов.

Скругление никогда не смешивается с прямыми углами в одном элементе. Точки-индикаторы и полоски плагинов (`--pc`) — круглые/со скруглением 2px; это единственный «органический» акцент в остальном прямоугольном интерфейсе.

## Components

Все компоненты построены **только на семантических токенах**; хардкод hex допустим лишь там, где значение не вынесено в переменную (например, hover-оттенок `button-primary-hover`).

- **button-primary** (`{components.button-primary}`) — фиолетовая заливка, тёмный текст (`on-accent`); главное действие (например, «Включить»). Единственная крупная заливка акцентом.
- **button-secondary** — фон `surface-raised`, текст `foreground-muted`; hover светлит до `surface-overlay` и текста `foreground`. Базовый вид кнопок в карточках и панелях.
- **button-danger** — прозрачный фон, текст `error`; hover добавляет красную подложку 12%. Только деструктивные действия (удаление плагина).
- **button-icon** — иконка 15px в квадрате 28px (в панелях — 24px); прозрачный фон, hover `surface-overlay`. Каждая несёт `aria-label`.
- **input / textarea** — фон `surface-raised`, текст `foreground`, граница `border`; фокус — граница `primary` (без внешнего кольца у полей) либо `:focus-visible`-outline у остальных контролов.
- **tab** — фон `surface`, текст `foreground-subtle`; активная вкладка — `background` + текст `foreground` + полоска `primary` сверху (2px).
- **toolbar** — 36px, фон `surface`; иконки приглушены, активные — `accent-soft` + `primary`. `.div-v` разделяет группы.
- **statusbar** — 24px, фон `surface`, `label` + `tabular-nums`; `aria-live="polite"` на `#stat-msg`. Слоты: состояние, сообщение, точки входа (журнал/мониторинг/AI), метрики позиции/размера.
- **pane** — фон `background`; панель-редактор с gutter, панель-предпросмотр с `body-text`.
- **drawer** — 440px справа, фон `popover`, тень `shadow-lg`; модален (backdrop + `inert` на `#app`).
- **dock** — нижняя панель на `surface`; режимы «Журнал | Мониторинг | AI-ассистент»; сворачивается в полосу 28px.
- **badge** — фон `surface-overlay`, текст `foreground-muted`, pill; вариант `badge-error` (красный, тёмный текст `on-error`) и `badge-warning` (янтарный, `on-warning`).

Мелкие атомы, тоже на семантических токенах: **divider / divider-strong** (`border-soft` / `border`), **line-numbers** (фон `gutter`, `mono`), **inline-code** (`surface-overlay` + `code`), **link** (`link`), **heading** (`heading`), **blockquote** (фон `accent-soft`), **status-dot** с вариантами warning/error/off (`success`/`warning`/`error`/`disabled`), **log-dot-document** (`info`).

## Do's and Don'ts

- **Do** брать значения только из семантических токенов (`{colors.primary}`, `{colors.surface-raised}`), а не хардкодить hex.
- **Do** использовать фиолетовый (`primary`) только для акцента и действия: активная вкладка, выбранная кнопка, каретка, ссылка, точка плагина.
- **Do** нести смысл цветом **вместе со словом или формой**: статус плагина — точка **и** подпись, ошибка — цвет **и** текст.
- **Do** использовать ручной тумблер тем (`data-theme="dark|light"`), тёмная по умолчанию; `prefers-reduced-motion` гасит анимации.
- **Do** использовать `:focus-visible` (outline 2px `primary`, offset 1px) вместо `:focus`; `outline: none` — только когда есть замена.
- **Do** писать числа с `tabular-nums`, многоточие — символом `…`, кавычки — `«»`.
- **Don't** использовать `prefers-color-scheme` для переключения темы — только ручной тумблер.
- **Don't** добавлять второй яркий акцент или красить им фоновые области.
- **Don't** применять `--disabled` к цвету текста — только к точкам/иконкам; текст «выкл» → `--fg-mute`.
- **Don't** подключать шрифты с CDN или внешних доменов — только встроенные same-origin `.woff2`.
- **Don't** менять замороженные id и селекторы (`docs/DESIGN_DOC.md` §13.2): `editor`, `preview`, `btn-*`, `chk-*`, `.md-block`, `.table-enhanced`, `.pl-item`, `.vtab.*` и прочие контракты e2e.
- **Don't** заменять иконочные кнопки текстом без `aria-label`; у иконочных кнопок метка обязательна.
