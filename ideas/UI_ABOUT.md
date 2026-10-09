# UI_ABOUT — дизайн-концепции и чек-лист доступности

> **Статус:** non-normative, обновлено на H2.
> **Расхождения с кодом:** `tasks/TZ-UX-SPEC-CLEANUP.md §4`.
> **Описание реализованного UI:** `ideas/UI_CONCEPT.md`.

---

## 1. Дизайн-концепции

### 1.1. Дизайн вырастает из предметной области

**darmark — это инструмент для работы с текстом и кодом.** Значит:
- Моноширинный шрифт для кода и данных (`Cascadia Code`)
- Тёмная тема по умолчанию (как в редакторах кода)
- Акцентный цвет — фиолетовый (ассоциация с кодом, креативностью)
- Минимум декорации, максимум функциональности

### 1.2. Типографика несёт личность

**Выбор шрифтов:**
- Основной: `-apple-system, Segoe UI, Inter, Roboto` — системный, чистый, профессиональный
- Моноширинный: `Cascadia Code, JetBrains Mono` — для кода, данных, статусов

**Типографическая шкала:**
- `h1`: 26px, bold, с нижней границей
- `h2`: 19px, semibold
- `h3`: 15px, semibold
- `body`: 13px, regular
- `small`: 11.5px, regular
- `mono`: 12.5px, monospace

### 1.3. Визуальная структура = информация

**Разделители несут смысл:**
- `border-bottom` — разделение секций (toolbar, context-bar, statusbar)
- `border-left` — разделение панелей (rail, sidebar)
- `border-right` — разделение вкладок
- `.div-v` — визуальный разделитель между группами кнопок

**Цветовые индикаторы:**
- Зелёный — активен/успех
- Жёлтый — предупреждение/изменено
- Красный — ошибка/карантин
- Фиолетовый — акцент/выбран

### 1.4. Один запоминающийся элемент

**Фиолетовый акцент (`--accent: #7c5cff`)** — единственный яркий цвет:
- Активные вкладки (полоска сверху)
- Выбранные кнопки (фон `accent-soft`)
- Каретка редактора
- Ссылки в предпросмотре
- Точки плагинов (`--pc`)

Всё остальное — приглушённые серые тона.

### 1.5. Два тира представлений

**Tier 1 (plugin-safe HTML):**
- Предпросмотр Markdown
- Представления плагинов
- Рендерится через `innerHTML`
- Ограниченный HTML (санитайзер)

**Tier 2 (полный DOM):**
- Редактор (textarea)
- Вкладки
- Тулбар
- Полный доступ к DOM

### 1.6. Плагины как расширения

**Плагины могут:**
- Объявлять команды (кнопки «Выполнить» в панели «Плагины»)
- Создавать представления (plugin-view)
- Подписаться на события
- Показывать уведомления

**Плагины не могут:**
- Менять тулбар
- Менять rail
- Менять статусбар
- Получать полный доступ к DOM

---

## 2. Чек-лист доступности (web-design-guidelines)

### 2.1. Accessibility

| Правило | Статус | Комментарий |
|---|---|---|
| Icon-only buttons need `aria-label` | OK | Все кнопки имеют `aria-label` |
| Form controls need `<label>` | OK | Чекбоксы обёрнуты в `<label>` |
| Interactive elements need keyboard handlers | OK | Все интерактивные элементы фокусируемы |
| `<button>` for actions | OK | Все действия — `<button>` |
| Decorative icons need `aria-hidden` | OK | SVG не имеют текстового эквивалента |
| Async updates need `aria-live` | OK | Статусбар обновляется |
| Use semantic HTML | OK | `<header>`, `<nav>`, `<main>`, `<footer>` |
| Headings hierarchical | OK | `h1`–`h6` иерархически |
| `scroll-margin-top` on heading anchors | OK | `.md-block { scroll-margin-top: 12px }` |

### 2.2. Focus States

| Правило | Статус | Комментарий |
|---|---|---|
| Visible focus | OK | `:focus-visible { outline: 2px solid var(--accent) }` |
| Never `outline: none` without replacement | OK | `:focus { outline: none }` + `:focus-visible` |
| Use `:focus-visible` over `:focus` | OK | Используется `:focus-visible` |
| Group focus with `:focus-within` | OK | `.format-group:focus-within` |

### 2.3. Animation

| Правило | Статус | Комментарий |
|---|---|---|
| Honor `prefers-reduced-motion` | OK | `@media (prefers-reduced-motion: reduce)` |
| Animate `transform`/`opacity` only | OK | Все анимации — transform/opacity |
| Never `transition: all` | OK | Все transition явные |
| Set correct `transform-origin` | OK | Используется по умолчанию |

### 2.4. Typography

| Правило | Статус | Комментарий |
|---|---|---|
| `…` not `...` | OK | Используется `…` |
| Curly quotes | OK | Используются `«»` |
| Non-breaking spaces | OK | `10&nbsp;MB`, `Ctrl+K` |
| Loading states end with `…` | OK | «Загрузка…» |
| `font-variant-numeric: tabular-nums` | OK | Для чисел в статусбаре |
| `text-wrap: balance` | — | Отсутствует в `style.css` (см. `TZ-UX-SPEC-CLEANUP.md §4`) |

### 2.5. Content Handling

| Правило | Статус | Комментарий |
|---|---|---|
| Text containers handle long content | OK | `text-overflow: ellipsis`, `white-space: nowrap` |
| Flex children need `min-w-0` | OK | `.file-chip`, `.tab`, `.pl-name` |
| Handle empty states | OK | Пустые состояния обработаны |
| User-generated content | OK | Санитайзер в md-core |

### 2.6. Navigation & State

| Правило | Статус | Комментарий |
|---|---|---|
| URL reflects state | N/A | Десктопное приложение |
| Links use `<a>` | OK | Все ссылки — `<a>` |
| Deep-link all stateful UI | N/A | Десктопное приложение |
| Destructive actions need confirmation | OK | Диалог при закрытии с несохранёнными изменениями |

### 2.7. Touch & Interaction

| Правило | Статус | Комментарий |
|---|---|---|
| `touch-action: manipulation` | — | Отсутствует в `style.css` (см. `TZ-UX-SPEC-CLEANUP.md §4`) |
| `-webkit-tap-highlight-color` | — | Отсутствует в `style.css` (см. `TZ-UX-SPEC-CLEANUP.md §4`) |
| `overscroll-behavior: contain` | — | Отсутствует в `style.css` (см. `TZ-UX-SPEC-CLEANUP.md §4`) |
| Drag/swipe need tap/click alternative | OK | Все жесты имеют альтернативы |

### 2.8. Dark Mode & Theming

| Правило | Статус | Комментарий |
|---|---|---|
| `color-scheme: dark` | OK | `<meta name="color-scheme" content="dark">` |
| `<meta name="theme-color">` | OK | `<meta name="theme-color" content="#0d0e12">` |
| Native `<select>` | N/A | Не используется |

### 2.9. Hover & Interactive States

| Правило | Статус | Комментарий |
|---|---|---|
| Buttons need `hover:` state | OK | Все кнопки имеют `:hover` |
| Interactive states increase contrast | OK | hover/active/focus ярки |

### 2.10. Content & Copy

| Правило | Статус | Комментарий |
|---|---|---|
| Active voice | OK | «Сохранить», «Открыть» |
| Title Case for headings | OK | «Новый файл», «Сохранить как» |
| Numerals for counts | OK | «3 активных», «0 слов» |
| Specific button labels | OK | «Сохранить как…», не «Сохранить» |
| Error messages include fix | OK | Понятные сообщения об ошибках |

### 2.11. Anti-patterns

| Правило | Статус | Комментарий |
|---|---|---|
| `user-scalable=no` | OK | Не используется |
| `onPaste` with `preventDefault` | OK | Не блокируется |
| `transition: all` | OK | Не используется |
| `outline-none` without focus-visible | OK | Не используется |
| Inline `onClick` navigation | OK | Не используется |
| `<div>` with click handlers | OK | Все кликабельные — `<button>` |
| Images without dimensions | N/A | Нет изображений |
| Large arrays `.map()` without virtualization | N/A | Маленькие списки |
| Form inputs without labels | OK | Все имеют метки |
| Icon buttons without `aria-label` | OK | Все имеют |
| Hardcoded date/number formats | OK | Используется `Intl` |
| `autoFocus` without justification | OK | Не используется |
| Animated GIF | N/A | Не используется |
| Gesture-only action | OK | Все имеют альтернативы |

---

## 3. Итог

**Соответствие рекомендациям:** высокое (большинство применимых правил соблюдены).

**Известные расхождения:** см. `tasks/TZ-UX-SPEC-CLEANUP.md §4` (5 пунктов: `text-wrap: balance`, `touch-action`, `-webkit-tap-highlight-color`, `overscroll-behavior`, `color-scheme`).

**Ключевые концепты:**
1. Дизайн из предметной области (код/текст)
2. Типографика как личность (моноширинный + системный)
3. Структура как информация (разделители, цвета)
4. Один акцентный элемент (фиолетовый)
5. Два тира представлений (plugin-safe vs full DOM)
6. Плагины как расширения (панель плагинов, plugin-view, статусбар)
