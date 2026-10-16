---
version: alpha
name: darmark
description: Инструментальная идентичность редактора Markdown в духе Notepad++ — холодные глубокие поверхности, фиолетовый акцент, ролевые гарнитуры (JetBrains Mono / Inter / Source Serif 4), ручной тумблер тёмной/светлой темы.
colors:
  # тёмная тема (по умолчанию): акцент и семантические статусы (tokens.css :root)
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
  accent-hover: "#ab9fff" # hover-заливка акцента (тёмная тема)
  on-accent: "#17121f"
  on-error: "#2a0f0f"
  on-warning: "#1a1400" # светлая тема переопределяет все три на "#ffffff"
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
  # светлая тема — поверхности и границы (base.css :root[data-theme="light"])
  light-background: "#fafbfc"
  light-surface: "#f0f2f6"
  light-surface-raised: "#e3e7ee"
  light-surface-overlay: "#d4d9e3"
  light-gutter: "#d8dde6"
  light-popover: "#ffffff"
  light-border: "#b8bfcc"
  light-border-soft: "#d0d5e0"
  # светлая тема — текст
  light-foreground: "#1a1d24"
  light-foreground-muted: "#4a5068"
  light-foreground-subtle: "#545a6b"
  light-heading: "#0f1115"
  light-body-text: "#2d3140"
  # светлая тема — акцент и статусы: переопределяются, а не наследуются
  # из тёмной (пастельные тона не проходят по контрасту на светлом фоне)
  light-accent: "#5546d0"
  light-accent-hover: "#4a3bc4" # hover темнее: белый текст держит 7.7:1
  light-accent-2: "#1d4ed8"
  light-accent-soft: "rgba(85, 70, 208, 0.12)"
  light-code: "#6d28d9"
  light-success: "#046c4e"
  light-warning: "#92400e"
  light-error: "#b91c1c"
  light-disabled: "#5f6575"
  light-on-accent: "#ffffff"
  light-on-error: "#ffffff"
  light-on-warning: "#ffffff"
typography:
  ui:
    fontFamily: '-apple-system, "Segoe UI", system-ui, Roboto, Arial, sans-serif'
    fontSize: 13px
    fontWeight: 400
    lineHeight: 1.4
  editor:
    fontFamily: '"JetBrains Mono", ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace'
    fontSize: 14.5px
    fontWeight: 400
    lineHeight: 26px
  mono:
    fontFamily: '"JetBrains Mono", ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace'
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
  caption-strong: # бейджи/счётчики (drawer.css .badge, .ipbtn .cnt)
    fontFamily: '-apple-system, "Segoe UI", system-ui, sans-serif'
    fontSize: 10px
    fontWeight: 600
    lineHeight: 1.4
rounded:
  sm: 5px
  md: 7px
  lg: 12px
  pill: 999px
spacing:
  # реализованные метрики оболочки: у каждой есть CSS-переменная (tokens.css).
  # Числовая шкала отступов (2/4/6/8/12/16/24) — дизайн-интенция, а не токены:
  # см. §Layout, в CSS отступы задаются px по месту.
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
    backgroundColor: "{colors.accent-hover}"
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
    typography: "{typography.caption-strong}"
    rounded: "{rounded.md}"
    padding: "0 5px"
  badge-error:
    backgroundColor: "{colors.error}"
    textColor: "{colors.on-error}"
  badge-warning:
    backgroundColor: "{colors.warning}"
    textColor: "{colors.on-warning}"
  divider:
    backgroundColor: "{colors.border-soft}"
  # разделитель групп в тулбаре — .div-v (opacity .75)
  divider-strong:
    backgroundColor: "{colors.border}"
  line-numbers:
    backgroundColor: "{colors.gutter}"
    textColor: "{colors.foreground-subtle}"
    typography: "{typography.mono}"
  inline-code:
    backgroundColor: "{colors.surface-overlay}"
    textColor: "{colors.code}"
    rounded: "4px" # предпросмотр: локальная шкала радиусов 4/6/8 (code / blockquote / pre)
    padding: "2px 6px"
  link:
    textColor: "{colors.link}"
  heading:
    textColor: "{colors.heading}"
    typography: "{typography.heading-1}"
  blockquote:
    backgroundColor: "{colors.accent-soft}"
  # Семантика статусов живёт в контекстных селекторах, а не в едином классе
  # `status-dot`: статусбар — `.stat-dot` (6px), журнал — `.log-dot.*` (5px),
  # индикаторы AI — `.dot` / `#ai-dot` / `#ai-status-dot` (7px).
  status-dot:
    backgroundColor: "{colors.success}"
    size: 6px
    rounded: "{rounded.pill}"
  status-dot-warning: # .log-dot.warning, #ai-dot.state-warn, .tab .dirty
    backgroundColor: "{colors.warning}"
  status-dot-error: # .log-dot.error, #ai-dot.state-error
    backgroundColor: "{colors.error}"
  status-dot-off: # #ai-dot.state-off, #ai-status-dot.state-off, .dot
    backgroundColor: "{colors.disabled}"
  log-dot-document: # .log-dot.document
    backgroundColor: "{colors.info}"
---

## Overview

**PENAL — «Notepad++ для Markdown».** Десктопный инструмент для Windows. Интерфейс — vanilla TS + CSS, без фреймворков.

Принципы: минимум хрома, две темы (ручной тумблер), один фиолетовый акцент, структура = информация, честность отказов.

Логотип — `logo.svg`/`logo_w.svg`/`penal_text.svg` в `crates/app/public/`; в тулбар — белый знак 20px. Цвета логотипа в UI-палитру не переносятся.

## Colors

Палитра — холодные глубокие поверхности с синим подтоном и **единственным** фиолетовым акцентом. Две темы: тёмная (по умолчанию) и светлая (ручной тумблер).

### Тёмная тема (по умолчанию)

| Токен | Значение | Роль |
|-------|----------|------|
| `primary` | `#9d8fff` | акцент и действие (активная вкладка, каретка, ссылка, точка плагина) |
| `secondary` | `#6ea8fe` | информационный акцент (ссылки, инлайн-подсказки) |
| `success` | `#34d399` | успех, активное состояние |
| `warning` | `#fbbf24` | предупреждение, «изменено» |
| `error` | `#f87171` | ошибка, отказ, деструктивное действие |
| `background` → `surface-overlay` | `#111318` → `#242836` | фоны (каждый слой светлее) |
| `foreground` / `-muted` / `-subtle` | `#e8eaf0` / `#a0a6bd` / `#8a90b0` | тексты (основной / вторичный / третий) |
| `heading` / `body-text` | `#ffffff` / `#d0d3e0` | заголовки / текст предпросмотра |
| `code` | `#e9b3ff` | инлайн-код |
| `chrome` | `#0d0e12` | `theme-color` для WebView2; в светлой теме подменяется на `#fafbfc` (`theme.ts`, `theme-init.js`) |
| `border` / `border-soft` | `#2e3348` / `#232736` | границы / тихие швы |

`--disabled` — только для точек/иконок, не для текста (текст «выкл» → `--fg-mute`).

### Светлая тема (ручной тумблер)

Тумблер переключает не «инверсию» тёмной, а отдельный набор значений (`base.css`): слои разведены, границы усилены, gutter темнее рабочей области, все текстовые пары ≥ 4.5:1.

| Токен | Значение |
|-------|----------|
| `background` | `#fafbfc` |
| `surface` | `#f0f2f6` |
| `surface-raised` | `#e3e7ee` |
| `surface-overlay` | `#d4d9e3` |
| `border` / `border-soft` | `#b8bfcc` / `#d0d5e0` |
| `gutter` | `#d8dde6` |
| `popover` | `#ffffff` |
| `foreground` / `-muted` / `-subtle` | `#1a1d24` / `#4a5068` / `#545a6b` |
| `heading` / `body-text` | `#0f1115` / `#2d3140` |
| `accent` / `accent-2` | `#5546d0` / `#1d4ed8` |
| `accent-soft` | `rgba(85, 70, 208, 0.12)` |
| `success` / `warning` / `error` | `#046c4e` / `#92400e` / `#b91c1c` |
| `disabled` | `#5f6575` (только точки/иконки) |
| `code` | `#6d28d9` |
| `on-accent` / `on-error` / `on-warning` | `#ffffff` |

**Акцент и статусы светлая тема переопределяет, а не наследует буквально** — это и есть «докрутка»: пастельные тона (`#9d8fff`, `#fbbf24`, `#f87171`) на светлом фоне AA не проходят, поэтому светлая берёт глубокие варианты — акцент `#5546d0` вместо `#9d8fff`, статусы `#046c4e` / `#92400e` / `#b91c1c`. Вместе с заливками меняется и текст на них (`on-accent` / `on-error` / `on-warning` → `#ffffff`), поэтому пары «заливка + текст» держат AA в обеих темах. Из тёмной наследуются только метрики, радиусы и ролевые гарнитуры — стек не зависит от темы.

## Typography

Все шрифты встроены (`.woff2`, OFL 1.1). Четыре ролевые гарнитуры:

- **UI (системная):** `-apple-system, "Segoe UI", system-ui, Roboto, Arial, sans-serif` — 13px/1.4. Вся оболочка: тулбар, вкладки, панели, кнопки, статусбар.
- **Editor (моноширинная):** `"JetBrains Mono", ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace` (`--font-mono`) — 14.5px/26px. Редактор, номера строк, статусы, метрики, технические данные.
- **Headings (Source Serif 4):** h1 30px, h2 21px, h3 17px (все 700). h1 — `border-bottom: 1px solid var(--line)`, h2 — `border-bottom: 1px solid var(--line-soft)`.
- **Body (Inter):** `Inter, -apple-system, "Segoe UI", system-ui, Roboto, Arial, sans-serif` — 13px/1.7 в предпросмотре. Плотность предпросмотра — `max-width: 860px`.
- **Labels / Caption:** 11px и 10.5px — подписи, метрики; 10px/600 (`caption-strong`) — бейджи и счётчики. Метки форм — uppercase с трекингом (`pop-label`).

Числа всегда идут с `font-variant-numeric: tabular-nums`. Лигатуры: в редакторе/коде — `liga` 1, `calt` 0; в предпросмотре — `calt` включён.

## Layout

Оболочка — вертикальная flex-колонка: тулбар → рабочая область → нижний док → статусбар. Рабочая область — горизонтальный сплит из максимум двух панелей (`MAX_PANES = 2`) с разделителем 1px.

Ключевые метрики: `Toolbar 36px` / `Tabs 32px` / `Statusbar 24px` / `Dock 300px` (свёрнут 28px) / `Drawer 440px`. Раскрытие дока сжимает рабочую область, а не накрывает её (не overlay). Drawer при узком окне (< 900px) — на всю ширину.

Отступы — шкала 2/4/6/8/12/16/24px: это дизайн-интенция, а не токены (в CSS отступы заданы px по месту); крупные зазоры (34px/20px) — исключения для геометрии текста. Плотность высокая: это инструмент, а не витрина.

## Elevation & Depth

Глубина выражается **тональными слоями и границами**, а не тяжёлыми тенями. Иерархия поверхностей — см. Colors (каждый слой светлее).

Тени применяются точечно и только для реально всплывающих слоёв: `--shadow-lg` (`0 20px 60px rgba(0,0,0,0.55)`) — дровер, диалоги, поповеры; `--shadow-sm` (`0 6px 18px rgba(0,0,0,0.35)`) — тосты и мелкие поповеры.

Backdrop — полупрозрачный чёрный (0.42–0.45), не размытие. Вложенные элементы отделяются фоном и `border-soft`, без теней.

## Shapes

Радиусы: `sm 5px`, `md 7px`, `lg 12px`, `pill 999px`. Скругление никогда не смешивается с прямыми углами в одном элементе. В предпросмотре — своя шкала блочных элементов: инлайн-код 4px, `blockquote` 6px, `pre` 8px (не токены оболочки).

Точки-индикаторы и полоски плагинов (`--pc`) — круглые/2px; единственный «органический» акцент в остальном прямоугольном интерфейсе.

## Components

Все компоненты построены **только на семантических токенах**; хардкод hex допустим лишь там, где значение не вынесено в переменную.

| Компонент | Фон | Текст | Применение |
|-----------|------|-------|------------|
| `button-primary` | `primary` | `on-accent` | главное действие («Включить»); единственная крупная заливка акцентом |
| `button-secondary` | `surface-raised` | `foreground-muted` | базовый вид кнопок; hover светлит до `surface-overlay` |
| `button-danger` | transparent | `error` | деструктивные действия; hover — красная подложка 12% |
| `button-icon` | transparent | `foreground-muted` | иконка 15px в квадрате 28px; каждая несёт `aria-label` |
| `input` / `textarea` | `surface-raised` | `foreground` | фокус — граница `primary` |
| `tab` | `surface` | `foreground-subtle` | активная вкладка — `background` + полоска `primary` 2px сверху |
| `toolbar` | `surface` | `foreground` | 36px; иконки приглушены, активные — `accent-soft` + `primary` |
| `statusbar` | `surface` | `foreground-subtle` | 24px, `label` + `tabular-nums`; `aria-live="polite"` |
| `pane` | `background` | `body-text` | панель-редактор с gutter, панель-предпросмотр с `body-text` |
| `drawer` | `popover` | `foreground` | 440px справа, тень `shadow-lg`, модален |
| `dock` | `surface` | `foreground` | нижняя панель; режимы «Журнал / Мониторинг / AI»; сворачивается в 28px |
| `badge` | `surface-overlay` | `foreground-muted` | радиус 7px (`md`), `caption-strong` (10px/600); варианты `badge-error` (красный), `badge-warning` (янтарный) |

Мелкие атомы — токены frontmatter и их реальные селекторы:

| Токен | Селектор | Примечание |
|-------|----------|------------|
| `divider` | `.divider` (шов панелей) | 1px, `border-soft` |
| `divider-strong` | `.div-v` (тулбар) | 1px, `border` + `opacity: .75` |
| `line-numbers` | `.line-numbers` (`#gutter`) | фон `gutter`, текст `foreground-subtle`, `mono` |
| `inline-code` | `.markdown-body code` | фон `surface-overlay`, текст `code` |
| `link` | `.markdown-body a` | `link`, hover → `primary` |
| `heading` | `.markdown-body h1–h3` | `heading` + `heading-1…3`; линейка h1 — `border`, h2 — `border-soft` |
| `blockquote` | `.markdown-body blockquote` | фон `accent-soft`, полоса слева `primary` 3px |
| `status-dot` | `.stat-dot` (статусбар) | 6px, `success` |
| `status-dot-warning` | `.log-dot.warning`, `#ai-dot.state-warn`, `.tab .dirty` | 5–7px, `warning` |
| `status-dot-error` | `.log-dot.error`, `#ai-dot.state-error` | 5–7px, `error` |
| `status-dot-off` | `#ai-dot.state-off`, `.dot` | 7px, `disabled` |
| `log-dot-document` | `.log-dot.document` | 5px, `secondary` (`--accent-2`) |

`status-dot*` — **семантический контракт, а не имена классов**: у каждого статуса есть точка нужного цвета, но живёт она в контекстных селекторах (статусбар `.stat-dot`, журнал `.log-dot.…`, индикатор AI `.dot` / `#ai-dot` / `#ai-status-dot`). Единого класса `status-dot-warning` в продукте нет намеренно: переименование этих селекторов — отдельное решение по дизайн-документу, а не часть синхронизации документа.

## Do's and Don'ts

- **Do** нести смысл цветом **вместе со словом или формой**: статус плагина — точка **и** подпись, ошибка — цвет **и** текст.
- **Do** использовать `:focus-visible` (outline 2px `primary`, offset 1px) вместо `:focus`; `outline: none` — только когда есть замена.
- **Do** писать числа с `tabular-nums`, многоточие — символом `…`, кавычки — `«»`.
- **Don't** добавлять второй яркий акцент или красить им фоновые области.
- **Don't** подключать шрифты с CDN или внешних доменов — только встроенные same-origin `.woff2`.
- **Don't** заменять иконочные кнопки текстом без `aria-label`.
