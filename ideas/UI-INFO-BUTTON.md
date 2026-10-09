# UI-INFO-BUTTON — кнопка информации о границах изоляции (F38-notices)

> **Статус:** non-normative проектный документ (design/analysis). Источник истины об интерфейсе — код `crates/app`.
> **Основание:** запрос на возврат удалённой информационной части (F38-notices) в виде отдельной кнопки-иконки у каждого плагина, перед `class="pl-toggle"`.
> **Автор анализа:** агент с навыками `frontend-design` + `senior-system-analyst` (read-only, без правок кода).
> **Связанные файлы:** `ideas/UX-spec.md` §2.1/§7, `crates/plugin-proto/src/notices.rs`, `crates/app/src/pluginManager.ts`, `crates/app/src/style.css`.

---

## 1. Цель и UX-решение

### Что за кнопка

Кнопка-иконка «инфо» в голове карточки плагина, расположенная **перед** `.pl-toggle` (т.е. первым элементом `.pl-item-head`). При клике показывает поповер с формулировками границы изоляции/доступа (`PluginInfo.notices`).

### Иконка (CSP-совместимость)

**Рекомендация: inline SVG.** Проект уже использует inline SVG для кнопок тулбара (`style.css:195` — `.tbtn svg { width: 15px; height: 15px; }`, `style.css:270` — `.rail-btn svg { width: 17px; height: 17px; }`). CSP `default-src 'self'` разрешает inline-контент, inline SVG — не внешний ресурс. Unicode-символ `ⓘ` (U+24D8) — допустимая альтернатива, но менее контролируемая (зависит от системных шрифтов, рендерится непредсказуемо на разных платформах).

Классическая info-иконка (круг с «i»):

```svg
<svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
  <circle cx="8" cy="8" r="7" fill="none" stroke="currentColor" stroke-width="1.5"/>
  <text x="8" y="11.5" text-anchor="middle" font-size="10" fill="currentColor" font-family="inherit">i</text>
</svg>
```

### Позиция в DOM

```
.pl-item-head
  ├── .pl-info          ← НОВАЯ кнопка (первая)
  ├── .pl-toggle         (label > input.pl-enabled)
  ├── .pl-name           (span)
  └── .pl-badge          (span)
```

### Закрытое/открытое состояние

- **Закрыто:** иконка в цвете `--fg-mute` (как `.rail-btn`), при hover — `--fg` + фон `--bg-3`.
- **Открыто:** иконка заливается `--accent-soft`, цвет `--accent` (паттерн `.rail-btn[aria-pressed="true"]` — `style.css:266-267`). Поповер виден под кнопкой.

### Сочетание с `.pl-badge` и `--pc`

- `--pc` (цвет плагина) задаётся на `.pl-item` (`pluginManager.ts:157`) и наследуется. Иконка может использовать `color: var(--pc)` в открытом состоянии — это визуально свяжет кнопку с цветом плагина, как это делает `.palette-origin::before` (`style.css:1038`).
- `.pl-badge` — независимый элемент справа, не конфликтует.

---

## 2. Модель взаимодействия

### Tooltip vs Popover vs Dialog

| Критерий | Tooltip | Popover | Dialog |
|----------|---------|---------|--------|
| Содержимое | 1 строка | 1–2 абзаца | Любой |
| Клавиатура | Только hover | Enter/Space | Focus-trap |
| Модальность | Нет | Нет | Да |
| Сложность | Минимальная | Средняя | Высокая |

**Рекомендация: Popover** (лёгкий, не модальный). Обоснование:

- Содержимое — 1–2 абзаца текста (`ISOLATION_NOTICE` + опционально `DOCUMENT_ACCESS_NOTICE`), это больше чем tooltip, но меньше чем диалог.
- `dialog.ts` (`crates/app/src/dialog.ts:84-195`) — тяжёлая модалка с `inert`, focus-trap, `aria-modal`. Для информационного текста без действий это overkill.
- Tooltip не подходит: не управляется с клавиатуры, не удерживается открытым при переводе фокуса.

### Поведение

| Действие | Реакция |
|----------|---------|
| Клик по кнопке | Toggle поповера |
| Enter/Space на кнопке | Toggle поповера (нативное поведение `<button>`) |
| Escape | Закрыть поповер, фокус на кнопку |
| Клик вне поповера | Закрыть |
| Повторный клик | Закрыть |
| Потеря фокуса (Tab) | Закрыть (опционально) |

### Управление фокусом и ARIA

- Кнопка: `aria-expanded="true|false"`, `aria-controls="<id поповера>"`, `aria-label="Информация о границах изоляции плагина <id>"`.
- Поповер: `role="tooltip"`, `id` уникален (например, `pl-info-pop-<plugin_id>`), `aria-hidden="true|false"`.
- Поповер **не** захватывает фокус (в отличие от диалога) — фокус остаётся на кнопке.

---

## 3. Data Flow

### Поток данных

```
Rust: PluginRuntime.permissions
  ↓ plugin_proto::notices::permission_notices()
  ↓ Vec<&'static str>
Rust: PluginInfo.notices: Vec<String>
  ↓ IPC list_plugins
TS: PluginInfo.notices: string[]  (crates/app/src/tauri.ts:118)
  ↓ renderItem(info)
  ↓ renderInfoPopover(info) → поповер с textContent
```

### Трансформации

| Шаг | Из | В | Трансформация |
|-----|-------|---|---------------|
| Rust `permission_notices` | `&[P]` | `Vec<&str>` | Добавляет `ISOLATION_NOTICE`; если есть `document*` — `DOCUMENT_ACCESS_NOTICE` |
| Rust `PluginInfo` | `Vec<&str>` | `Vec<String>` | `.map(str::to_string)` |
| TS `PluginInfo` | `string[]` | DOM | `textContent` каждого notice в отдельный `<div>` |

### Санитизация

**Не требуется дополнительной санитизации.** Строки идут через `textContent` (не `innerHTML`) — паттерн, уже используемый в `pluginManager.ts` (`node.textContent = notice`). Это защищает от XSS.

### Кэширование текста в DOM

Текст notices **статичен** — не меняется в рамках одного `list_plugins`. Рекомендация:

- Создавать поповер **один раз** при рендере карточки (в `renderItem`), скрытым.
- При открытии — просто показывать (`hidden = false`), при закрытии — скрывать.
- Пересоздание при каждом открытии — лишняя работа, но допустимо (1–2 элемента).

### Что если `notices` пуст

**Невозможно.** `permission_notices` всегда возвращает минимум `ISOLATION_NOTICE` (`crates/plugin-proto/src/notices.rs:20`). Но для защиты: если `info.notices.length === 0` — не создавать кнопку.

---

## 4. Контракты и совместимость

### Затронутые селекторы

| Селектор | Где используется | Влияние |
|----------|------------------|---------|
| `.pl-item[data-plugin]` | `e2e/steps/plugins.steps.js` | **Не затронут** — добавка аддитивная |
| `.pl-badge` | `plugins.steps.js` | **Не затронут** |
| `input.pl-enabled` | `plugins.steps.js` | **Не затронут** |
| `button.pl-reload` | `plugins.steps.js` | **Не затронут** |
| `.pl-cmd` | `plugins.steps.js` | **Не затронут** |
| `#plugin-manager` | `plugins.steps.js` | **Не затронут** |
| `.pl-item-head` | `style.css:308` | **Расширяется** — новый первый элемент |

### Почему аддитивно

Новая кнопка — **новый элемент** в `.pl-item-head`, не изменяющий существующие. Все замороженные контракты (`§13.2`, e2e) остаются валидными.

### `noUnusedLocals`/`noUnusedParameters`

**Критично:** если создать функцию `renderInfoPopover(info)` и не вызвать её — сборка упадёт (`tsconfig.json`: `strict`, `noUnusedLocals`). Всё, что объявлено, должно использоваться.

---

## 5. Состояния и edge cases

### Количество notices

| notices | Поведение |
|---------|-----------|
| 0 | Кнопка не создаётся (защита, хотя невозможно) |
| 1 | Один `<div>` в поповере (только `ISOLATION_NOTICE`) |
| 2 | Два `<div>` с разделителем (только `ISOLATION` + `DOCUMENT_ACCESS`) |

### Длинный текст

`ISOLATION_NOTICE` — ~100 символов, `DOCUMENT_ACCESS_NOTICE` — ~110 символов. В поповере шириной `min(280px, 80vw)` — 2–3 строки. Дополнительная защита: `max-width: min(300px, 70vw)`, `overflow-wrap: break-word`.

### Переполнение панели

`.pl-panel` имеет `overflow-y: auto` (`style.css:297`). Поповер должен быть `position: absolute` относительно `.pl-item-head`, чтобы не влиять на layout. При узком sidebar (224px, `--w-sidebar`) поповер может выходить за правый край — использовать `right: 0` для выравнивания по правому краю кнопки.

### Перерисовка списка (`replaceChildren`)

**Критичный момент.** `render()` вызывает `root.replaceChildren(...)` (`pluginManager.ts:230`), что **уничтожает все DOM-узлы**, включая открытый поповер.

**Решение:** перед `render(list)` закрывать все открытые поповеры. Варианты:

1. Глобальный `AbortController` — при `render()` вызвать `ac.abort()`, поповеры слушают `signal` и удаляются.
2. Явный вызов `closeAllInfoPopovers()` перед `render()`.

Рекомендуется вариант 2 — проще, явнее, не требует передачи `signal` через все функции.

### Клавиатурная навигация

- Tab: кнопка в порядке tabulation (первый элемент `.pl-item-head`).
- Enter/Space: toggle поповера (нативное поведение `<button>`).
- Escape: закрыть.

### Screen reader

- `aria-expanded` на кнопке сообщает состояние.
- `aria-controls` связывает кнопку с поповером.
- `role="tooltip"` на поповере — SR озвучит содержимое при открытии.
- Дополнительно: `aria-live="polite"` на поповере (опционально, т.к. tooltip обычно не live).

---

## 6. Стили/токены

### Переиспользуемые токены

| Токен | Значение | Применение |
|-------|----------|------------|
| `--popover` | `#1a1b22` | Фон поповера |
| `--pop-border` | `#2a2c36` | Рамка поповера |
| `--shadow-sm` | `0 6px 18px rgba(0,0,0,0.35)` | Тень поповера |
| `--r-md` | `7px` | Скругление поповера |
| `--r-sm` | `5px` | Скругление кнопки |
| `--pc` | `var(--accent)` | Цвет плагина (открытое состояние) |
| `--accent-soft` | `rgba(124,92,255,0.12)` | Фон кнопки в открытом состоянии |
| `--fg-mute` | `#8a90a4` | Цвет иконки в закрытом состоянии |
| `--fg` | `#e6e7ee` | Цвет иконки при hover |
| `--bg-3` | `#1f2028` | Фон кнопки при hover |

### Новые классы

```css
/* Кнопка информации о границах изоляции */
.pl-info {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  padding: 0;
  border: none;
  border-radius: var(--r-sm);
  background: transparent;
  color: var(--fg-mute);
  cursor: pointer;
  flex-shrink: 0;
  transition: background 0.12s, color 0.12s;
}
.pl-info:hover { background: var(--bg-3); color: var(--fg); }
.pl-info[aria-expanded="true"] { background: var(--accent-soft); color: var(--pc); }
.pl-info:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }

/* Поповер с формулировками */
.pl-info-pop {
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  z-index: 100;
  max-width: min(300px, 70vw);
  padding: 10px 12px;
  background: var(--popover);
  border: 1px solid var(--pop-border);
  border-radius: var(--r-md);
  box-shadow: var(--shadow-sm);
  color: var(--fg-dim);
  font-size: 11.5px;
  line-height: 1.5;
}
.pl-info-pop[hidden] { display: none; }
/* Это справка, а не предупреждение: не наследуем янтарный цвет `.pl-notice`. */
.pl-info-pop .pl-notice { color: inherit; }
.pl-info-pop .pl-notice + .pl-notice {
  margin-top: 6px;
  padding-top: 6px;
  border-top: 1px solid var(--line-soft);
}
```

### Тёмная тема

Проект использует одну тёмную тему (`style.css:1`). Все токены уже тёмные, дополнительных действий не требуется.

### `prefers-reduced-motion`

Кнопка использует `transition: background 0.12s, color 0.12s`. Нужно добавить:

```css
@media (prefers-reduced-motion: reduce) {
  .pl-info { transition: none; }
}
```

### Тач-таргеты

Кнопка `24×24px` — минимальный размер. Для соответствия рекомендации 44px — увеличить hit-area через `::before`:

```css
.pl-info { position: relative; }
.pl-info::before {
  content: "";
  position: absolute;
  inset: -10px -6px; /* hit-area до 44px; по горизонтали уже — справа вплотную `.pl-toggle` */
}
```

Альтернатива — `padding: 10px` с отрицательным `margin` для компенсации.

---

## 7. E2E

### Новый сценарий в `plugins.feature`

```gherkin
Сценарий: Кнопка информации показывает формулировки границы изоляции
  Given плагин "e2e-view" загружен
  When я открываю панель плагинов
  Then у плагина "e2e-view" есть кнопка информации
  When я кликаю по кнопке информации плагина "e2e-view"
  Then поповер информации плагина "e2e-view" виден
  And поповер содержит текст "изоляция отказов"
  When я нажимаю Escape
  Then поповер информации плагина "e2e-view" скрыт
```

### Что нельзя проверять в E2E

- **Наведение мышью** — WebdriverIO не может эмулировать hover стабильно. Если добавить hover-открытие — пометить `@manual`.
- **Позиционирование поповера** — точные координаты зависят от layout, проверять только видимость.
- **Переполнение при узком sidebar** — зависит от размера окна, нестабильно.

### Шаги в `plugins.steps.js`

Нужно добавить:

```javascript
Then("у плагина {string} есть кнопка информации", async (pluginId) => { ... });
When("я кликаю по кнопке информации плагина {string}", async (pluginId) => { ... });
Then("поповер информации плагина {string} виден", async (pluginId) => { ... });
Then("поповер содержит текст {string}", async (text) => { ... });
Then("поповер информации плагина {string} скрыт", async (pluginId) => { ... });
```

---

## 8. Риски и открытые вопросы

| # | Риск/вопрос | Решение |
|---|-------------|---------|
| 1 | **Позиционирование поповера при скролле** — панель `.pl-panel` скроллится, поповер «прилипнет» к кнопке только если `position: absolute` относительно `.pl-item-head` | `[ТРЕБУЕТ УТОЧНЕНИЯ]` — достаточно ли `absolute` или нужен `fixed` + JS-расчёт? |
| 2 | **Поповер выходит за правый край панели** — sidebar 224px, поповер 300px | `[ТРЕБУЕТ УТОЧНЕНИЯ]` — выравнивать по правому краю (`right: 0`) или уменьшать `max-width`? |
| 3 | **Hover-открытие** — нужно ли? | `[ТРЕБУЕТ УТОЧНЕНИЯ]` — рекомендуется только клик, для простоты и e2e |
| 4 | **Несколько открытых поповеров** — открыть у двух плагинов одновременно? | Рекомендация: закрывать предыдущий при открытии нового |
| 5 | **Перерисовка при `plugins-changed`** — поповер уничтожится без закрытия | Решение: явный вызов `closeAllInfoPopovers()` перед `render()` |
| 6 | **CSP и inline SVG** — inline SVG разрешён? | Да, `default-src 'self'` разрешает inline-контент |
| 7 | **`noUnusedLocals`** — забыть вызвать `renderInfoPopover` | Критично: всё объявленное должно использоваться |
| 8 | **Focus-trap** — нужен ли для поповера? | Нет, поповер не модальный, фокус остаётся на кнопке |

---

## 9. Итоговая рекомендация

### Файлы и точки правки

1. **`crates/app/src/pluginManager.ts`**
   - Добавить функцию `renderInfoButton(info: PluginInfo): HTMLElement` — создаёт кнопку с inline SVG.
   - Добавить функцию `renderInfoPopover(info: PluginInfo): HTMLElement` — создаёт скрытый поповер с `textContent` notices.
   - В `renderItem()` (`pluginManager.ts:154-214`):
     - Создать `infoButton` и `infoPopover`.
     - Вставить `infoButton` **перед** `toggleLabel` в `head.append(...)`.
     - Назначить `aria-controls`/`aria-expanded`.
     - Добавить обработчик клика на `infoButton` (toggle поповера).
     - Добавить обработчик Escape (закрыть).
   - В `render()` (`pluginManager.ts:216-224`) — вызвать закрытие всех поповеров перед `replaceChildren`.
   - В `onClick()` (`pluginManager.ts:374-388`) — добавить ветку для `.pl-info`.

2. **`crates/app/src/style.css`**
   - Добавить стили `.pl-info`, `.pl-info-pop`, `@media (prefers-reduced-motion: reduce)`.
   - Расширить `.pl-item-head` — без изменений, `flex` сам обработает новый элемент.

3. **`crates/app/e2e/features/plugins.feature`**
   - Добавить сценарий «Кнопка информации показывает формулировки границы изоляции».

4. **`crates/app/e2e/steps/plugins.steps.js`**
   - Добавить шаги для кнопки/поповера (см. §7).

### Что НЕ трогать

- Rust-код (`manager.rs`, `notices.rs`, `tauri.ts`) — `PluginInfo.notices` уже готов.
- `dialog.ts` — не используем, поповер проще.
- `toast.ts`, `palette.ts` — только референс.
- Существующие селекторы e2e — аддитивность.

### Порядок реализации

1. Стили (`.pl-info`, `.pl-info-pop`) → `npm run build` для проверки.
2. `renderInfoButton` + `renderInfoPopover` → `npm run build`.
3. Вставка в `renderItem` → `npm run build`.
4. Обработчики (click, Escape, закрытие при render) → `npm run build`.
5. E2E сценарий → `npm run test:e2e`.

---

**Итого:** задача локализована в `pluginManager.ts` (рендер + обработчики) и `style.css` (новых ~30 строк). Данные готовы, контракты не ломаются, CSP соблюдена. Основной риск — позиционирование поповера при скролле и переполнении; открытые вопросы помечены `[ТРЕБУЕТ УТОЧНЕНИЯ]` в §8.

---

## 10. Статус реализации (после ревью)

Реализовано по этому документу (принятые решения §8 применены).

| Артефакт | Состояние |
|----------|-----------|
| `crates/app/src/pluginManager.ts` | `infoPopoverId`/`infoIcon`/`renderInfoButton`/`renderInfoPopover`, `toggleInfoPopover`/`closeAllInfoPopovers`, ветка `.pl-info` в `onClick`, `onDocumentClick`/`onDocumentKeydown`, снятие слушателей в `dispose()` |
| `crates/app/src/style.css` | `.pl-info` (+ hit-area), `.pl-info-pop`, `@media (prefers-reduced-motion)`, `.pl-item-head { position: relative }` |
| `crates/app/e2e/features/plugins.feature` | Сценарий «Кнопка информации показывает формулировки границы изоляции» |
| `crates/app/e2e/steps/plugins.steps.js` | 5 шагов (кнопка/клик/видимость/текст/скрытие) |

**Проверки:** `npm run build` — зелёная; E2E `plugins.feature` — сценарий info-кнопки проходит (70 passing). Ревью (`code-reviewer`): блокеров и MAJOR нет.

**Правки по ревью (внесены):**
- hit-area: `inset: -10px -6px` — зона нажатия больше не заходит на `.pl-toggle` (устраняет мисклики);
- `.pl-info-pop .pl-notice { color: inherit; }` — информационный текст не выглядит янтарным «предупреждением».

**Принято как известное ограничение:** вертикальный клиппинг поповера у нижней карточки списка (`.pl-panel { overflow-y: auto }`). Горизонтальное вылезание за узкий sidebar снято `right: 0`. Если станет проблемой — решать порталом (`position: fixed` + JS-расчёт) отдельным шагом.

