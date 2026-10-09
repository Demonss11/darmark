# TZ-PLUGIN-DRAWER — Управление плагинами: триггер в тулбаре + правый дровер (вариант C)

> **Статус:** Запланировано
> **Приоритет:** P1 (после закрытия H2 — Фазы 5–6 `tasks/TZ-UX-SPEC-CLEANUP.md`)
> **Область:** Фронтенд (`crates/app/**`) + документация
> **Основание:** прототип `ideas/front_plugins-drawer-c.html` (C-v2); анализы дизайнера,
> системного аналитика и проектировщика модулей; `ideas/front_plugins-placement.html` (вариант C)
> **Предшественники:** `ideas/UI-IDEAS.md` (IDEA-005), `tasks/TZ-UX-SPEC-CLEANUP.md` §1, §9, §11

---

## 1. Контекст и цель

После удаления `panel-explorer` левый край занимают только `nav.rail` (44px) и `aside#sidebar`
(224px) под панель плагинов. Вариант C выносит управление плагинами в **правый выдвижной дровер**,
а триггер — в верхний тулбар; левые rail и sidebar упраздняются.

**Цель:**
- чистый левый край (редактор получает +268px хрома);
- карточкам плагинов — нормальная ширина (440px против текущих 224px);
- **не слабее** текущего состояния (вариант A): сохранить идентичность плагина, состояния,
  плотность карточек и доступность;
- не трогать protected-функционал (§2.1 `TZ-UX-SPEC-CLEANUP.md`): инспектор-подсветку,
  `#view-switch`, плагинные view.

**Визуальный эталон:** `ideas/front_plugins-drawer-c.html` (переключатель 6 сценариев:
Обычный / Пусто / Загрузка / Ошибка загрузки / Карантин / Сбой).

---

## 2. Зафиксированные решения

Решения приняты по прототипу дизайнера (C-v2).

| # | Вопрос | **Решение** |
|---|--------|-------------|
| 1 | Модальность дровера | **Модальный.** `role="dialog"`, `aria-modal="true"`; backdrop закрывает; focus-trap; возврат фокуса на триггер |
| 2 | Ширина дровера | **`--w-drawer: 440px`** (на узком окне — 100vw, см. §7) |
| 3 | Судьба `#rail-logs` (Журнал) | **Перенести точку входа в статусбар** (`#sb-logs` + бейдж `#log-badge`). Полноценная панель журнала — вне scope (H3) |
| 4 | ARIA-роль дровера | **`dialog`** |
| 5 | `#side-count` | **Сохранить замороженный id** (переносится в шапку дровера). В прототипе элемент назван `#drawer-count`; в продакшене id **не** переименовываем — e2e читает его (`plugins.steps.js:658`, `plugin-panel.feature:45`) |

> **Примечание к п.5.** Прототип использует `#drawer-count` как локальное имя. В ТЗ
> фиксируется сохранение `#side-count` (минимальный риск, без миграции шага счётчика).
> Класс оформления может быть новым (`.drawer-count`), id — прежний.

---

## 3. Целевой layout

```
┌───────────────────────────────────────────────────────────────┐
│ #toolbar  … #palette-trigger · #tb-plugins · тумблеры · инспектор│
├───────────────────────────────────────────────────────────────┤
│ .workspace                                                     │
│   main (.panes)            ← rail и sidebar удалены            │
│   ┌───────────────────────┬───────────────────────┐           │
│   │  pane-a (редактор)    │  pane-b (превью)      │           │
│   └───────────────────────┴───────────────────────┘           │
├───────────────────────────────────────────────────────────────┤
│ #statusbar  … #sb-logs (Журнал) · #plugin-status · stat-*      │
└───────────────────────────────────────────────────────────────┘

  открыт дровер (модальный overlay справа):
┌───────────────────────────────────────────────────────────────┐
│ #toolbar                                                       │
├───────────────────────────────┬───────────────────────────────┤
│                               │ #backdrop                     │
│   main (.panes)               │ #drawer (fixed, 440px)        │
│   (перекрыт backdrop'ом)      │   .drawer-head + #drawer-body │
│                               │     #plugin-manager           │
├───────────────────────────────┴───────────────────────────────┤
│ #statusbar                                                     │
└───────────────────────────────────────────────────────────────┘
```

`#backdrop` и `#drawer` — **прямые дети `body`** (рядом с `#modal-root`/`#toast-host`),
вне `#app`, чтобы `inert` на `#app` (модальная палитра) не глушил сам дровер. Дровер
модальный — сам накладывает `inert` на `#app` (см. §9).

---

## 4. Контракты и миграция

### 4.1. Замороженные id — судьба

| id / селектор | Заморожен | Судьба в C |
|---------------|-----------|------------|
| `#rail-plugins` | Да (§1) | **Удалить** → триггер `#tb-plugins` |
| `#rail-logs` | де-факто (a11y) | **Удалить** → `#sb-logs` + `#log-badge` |
| `#panel-plugins` | Да (§1) | **Удалить** (контейнер не нужен) |
| `#plugin-manager` | Да (§1, §9.1) | **Сохранить**, переместить в `#drawer-body` |
| `#side-count` | Да (§1) | **Сохранить**, переместить в `.drawer-head` |
| `#palette-trigger` | Да (§1) | **Сохранить** без изменений |
| `#plugin-status` | Да (§9.5) | **Сохранить** (статусбар) |
| `#plugin-view`, `#view-switch` | Да (§1) | **Сохранить** (правая панель, не трогаем) |
| `#stat-msg`, `#stat-pos`, `#stat-size`, `#stat-inspect` | Да (§1) | **Сохранить** |

### 4.2. Новые id (добавить в §1 `TZ-UX-SPEC-CLEANUP.md`)

`#tb-plugins` (триггер), `#tb-plugins-cnt` (счётчик активных), `#tb-plugins-dot` (точка
проблемы), `#drawer`, `#drawer-title`, `#drawer-body`, `#drawer-close`, `#drawer-reload-all`,
`#drawer-install`, `#backdrop`, `#sb-logs`, `#log-badge`.

### 4.3. Инварианты (§9 `TZ-UX-SPEC-CLEANUP.md`) — сохранить

- `.pl-item-body` **всегда** в DOM, скрывается только CSS `display:none` (без `hidden`).
- Состояние раскрытия — `Set<string>` в замыкании `createPluginManager`, не в DOM.
- Переключение view — `hidden`-toggle смонтированных узлов, не пересоздание DOM.
- `#editor` и `#preview` — сами скролл-контейнеры.

---

## 5. Модуль `drawer.ts` (глубокий модуль)

`sidebar.ts` **удаляется целиком**; вместо него — один контроллер видимости дровера.

```ts
// drawer.ts — правый модальный дровер (вариант C).
// Глубокий модуль: маленький интерфейс, много скрытого поведения
// (фокус-трап, Esc, backdrop, синхронизация триггера, возврат фокуса).
export interface Drawer {
  open(): void;
  close(): void;
  toggle(): void;
  isOpen(): boolean;
  /** Контейнер содержимого; менеджер монтируется сюда один раз. */
  readonly content: HTMLElement;
  dispose(): void;
}

export interface DrawerOptions {
  root: HTMLElement;        // #drawer
  trigger: HTMLElement;     // #tb-plugins
  closeButton: HTMLElement; // #drawer-close
  backdrop: HTMLElement;    // #backdrop
  title: string;            // "Плагины"
}
```

Скрыто внутри (не протекает в вызывающих):
- синхронизация `aria-expanded`/класс `active` на триггере;
- `inert` на `#app` при открытии, снятие при закрытии;
- focus-trap (Tab/Shift+Tab), начальный фокус на `#drawer-close`, возврат фокуса на триггер;
- закрытие по `Esc`, клику по `#backdrop`, кнопке `#drawer-close`, повторному клику по триггеру;
- класс `.open` на `#drawer` (e2e проверяет класс, **не** `transform`/`transition`).

**Переиспользование:** примитивы focus-trap/inert из `dialog.ts` вынести в общий хелпер и
переиспользовать; если расширение `dialog.ts` рискованно — локальная реализация по образцу
прототипа. Зафиксировать выбор при реализации.

**Не обобщать** с `sidebar.ts` (`side: left|right`): левая панель — `collapsed`+rail, правая —
`fixed`+backdrop; обобщение усложнит интерфейс.

### 5.1. Точки композиции в `main.ts` (порядок)

1. `createDrawer({ root, trigger, closeButton, backdrop, title })` — контейнер.
2. `createPluginManager({ root: drawer.content, … })` — содержимое (шов: менеджер не знает о дровере).
3. `createPluginStatusBar({ …, onShowPanel: () => drawer.open() })`.
4. `createShell({ …, drawer })` — привязка триггера (`#tb-plugins` → `drawer.toggle()`).

---

## 6. Целевой DOM (`index.html`)

```html
<!-- тулбар: сразу после #palette-trigger -->
<button class="ipbtn" id="tb-plugins" type="button"
        aria-haspopup="dialog" aria-expanded="false" aria-controls="drawer"
        aria-label="Плагины">
  <svg …></svg>
  <span class="cnt" id="tb-plugins-cnt">0</span>
  <span class="pdot" id="tb-plugins-dot" aria-hidden="true"></span>
</button>

<!-- вне #app, рядом с #modal-root -->
<div class="drawer-backdrop" id="backdrop"></div>
<aside class="drawer" id="drawer" role="dialog" aria-modal="true"
       aria-labelledby="drawer-title" hidden>
  <div class="drawer-head">
    <span class="side-title" id="drawer-title">Плагины</span>
    <span class="side-count" id="side-count"></span>
    <button class="tbtn tbtn-sm" id="drawer-reload-all" …></button>
    <button class="tbtn tbtn-sm" id="drawer-install" …></button>
    <button class="tbtn tbtn-sm" id="drawer-close" …></button>
  </div>
  <div class="drawer-body" id="drawer-body">
    <div id="plugin-manager"></div>
  </div>
</aside>

<!-- статусбар: точка входа «Журнал» -->
<button class="stat clickable" id="sb-logs" title="Журнал и события" aria-label="Журнал и события">
  <svg …></svg><span>Журнал</span><span class="badge" id="log-badge" hidden>0</span>
</button>
```

**Удалить:** `nav.rail` (целиком), `aside#sidebar` (целиком), `#panel-plugins`, `#rail-plugins`,
`#rail-logs`.

---

## 7. Стили и токены

- Заменить `--w-sidebar` → `--w-drawer: 440px`; `--w-rail` удалить (или пометить мёртвым).
- `.drawer`: `position: fixed; top/right/bottom: 0; width: var(--w-drawer); z-index: 70;`
  `transform: translateX(100%)`; `transition: transform 220ms cubic-bezier(.2,.9,.3,1)`;
  `.drawer.open { transform: none; }`; `background: var(--popover)`; `box-shadow: var(--shadow-lg)`.
- `.drawer-backdrop`: `position: fixed; inset: 0; background: rgba(0,0,0,.45)`.
- `.drawer-head` фиксирован, `.drawer-body` — `overflow-y: auto`.
- Триггер `.ipbtn`: icon-only (≈28×28), счётчик `.cnt`, точка `.pdot` (`--pc`; красная при
  проблемах), активное состояние — фон `--accent-soft`.
- `@media (max-width: 900px)`: дровер `width: 100vw`.
- `@media (prefers-reduced-motion: reduce)`: без анимации (мгновенно).
- Правило: новые токены — только через `crates/app/src/style.css`.

---

## 8. Содержимое и состояния

Карточки плагинов — **не беднее A**: монограмма (`--pc`), имя, версия, статус-бейдж, тумблер
вкл/выкл, `button.pl-reload`, аккордеон (permissions-чипы, notice, crash), инфо-поповер
(`button.pl-info` → `.pl-info-pop`, см. §11 `TZ-UX-SPEC-CLEANUP.md`).

Состояния (по прототипу): Обычный · Пусто · Загрузка (skeleton) · Ошибка загрузки · Карантин ·
Сбой (трейсбек + «Перезапустить») · Согласие на permissions.

Триггер отражает состояние: счётчик активных + точка `--pc`; при `failed`/`quarantined`/`crashed` —
красная точка, счётчик становится янтарным.

---

## 9. Доступность

- Триггер: `aria-haspopup="dialog"`, `aria-expanded`, `aria-label="Плагины"` (+ счётчики при проблемах).
- Дровер: `role="dialog"`, `aria-modal="true"`, `aria-labelledby="drawer-title"`.
- Focus-trap; начальный фокус — `#drawer-close`; возврат фокуса на `#tb-plugins` при закрытии.
- Закрытие: `Esc`, `#backdrop`, `#drawer-close`, повторный клик по триггеру.
- `inert` на `#app`, пока дровер открыт.
- Порядок Tab: close → reload-all → install → карточки по порядку.

---

## 10. Data flow

| Действие | Путь |
|----------|------|
| Клик по `#tb-plugins` | `shell.ts` → `drawer.toggle()` |
| `.ps-*` в статусбаре (`onShowPanel`) | `main.ts` → `drawer.open()` |
| Ctrl+R (reload) | без изменений (`pluginManager.reloadForDocument`) |
| Ctrl+K (палитра) | **взаимоисключение:** открытие палитры закрывает дровер и наоборот (не два модальных слоя одновременно) |

---

## 11. E2E-миграция

| Файл | Правка |
|------|--------|
| `e2e/steps/plugins.steps.js` | `openPluginsPanel()`: вместо `#panel-plugins`/`#rail-plugins` — `#drawer`/`#tb-plugins` (проверка `#drawer.classList.contains("open")`) |
| `e2e/steps/plugins.steps.js` | Шаг счётчика: **без изменений** (`#side-count` сохранён) |
| `e2e/features/a11y.feature` | Сценарий M2: `#rail-plugins`/`#rail-logs` → `#tb-plugins`/`#sb-logs` |
| `e2e/features/plugin-panel.feature` | Обновить комментарий про расположение `#side-count` (`.drawer-head`) |
| `e2e/features/*` | Новые сценарии: открытие/закрытие дровера (Esc, backdrop, триггер), возврат фокуса, запись «Журнал» в статусбаре |
| `e2e/features/palette.feature` | Без изменений (взаимоисключение — новый сценарий при необходимости) |

Инвариант для новых тестов: проверять класс `.open` и `isOpen()`, **не** `getComputedStyle`
(`transform`/`transition`).

---

## 12. Фазы и объём

| Фаза | Объём | Содержание |
|------|-------|-----------|
| **0. Документация** | S | ADR-**0024** (см. §13); обновить `DESIGN_DOC.md §5.5` (`sidebar.ts` → `drawer.ts`); обновить §1 `TZ-UX-SPEC-CLEANUP.md` (удалить `#rail-plugins`/`#panel-plugins`, добавить новые id) |
| **1. Модуль и разметка** | M | `drawer.ts`; `index.html` (удалить rail/sidebar, добавить триггер/дровер/backdrop/журнал); `style.css`; `main.ts`; `shell.ts`; удалить `sidebar.ts` |
| **2. E2E** | M | Миграция шагов/фич, прогон `npm run test:e2e`, доведение до зелёного |
| **3. Журнал** | S | Точка входа `#sb-logs` (+ `#log-badge`) в статусбаре (заглушка/навигация; панель — H3) |

**Итого:** M.

---

## 13. ADR и документация

- **ADR-0024** «Размещение управления плагинами: триггер в тулбаре + правый дровер» —
  номер свободен (`docs/adr/0021–0023` заняты). Содержание: обоснование overlay-модальности,
  ширина 440px, миграция замороженных контрактов, взаимоисключение с палитрой, судьба журнала.
- `docs/DESIGN_DOC.md`: §5.5 (модульная карта), §13.2 (контракты).
- `tasks/TZ-UX-SPEC-CLEANUP.md` §1, §9, §11 — синхронизировать.

---

## 14. Критерии приёмки

- [ ] Левые `nav.rail` и `aside#sidebar` удалены; `sidebar.ts` удалён.
- [ ] `#tb-plugins` открывает/закрывает дровер; `aria-expanded` синхронизирован.
- [ ] Дровер модальный: backdrop, focus-trap, `inert` на `#app`, возврат фокуса, Esc.
- [ ] `#plugin-manager` работает в `#drawer-body`; `#side-count` сохранён в `.drawer-head`.
- [ ] `#plugin-status.onShowPanel` открывает дровер.
- [ ] Ctrl+R работает; Ctrl+K взаимоисключён с дровером.
- [ ] Триггер показывает счётчик активных и индикацию проблем.
- [ ] «Журнал» доступен из статусбара (`#sb-logs`).
- [ ] `npm run build` (strict TS) без ошибок; `npm run test:e2e` зелёный.
- [ ] §1 `TZ-UX-SPEC-CLEANUP.md` обновлён; ADR-0024 добавлен.

---

## 15. Риски и обратимость

| Риск | Митигация |
|------|-----------|
| Сломаны замороженные контракты | Атомарная миграция кода + e2e в одном коммите |
| Два модальных слоя (дровер + палитра) | Взаимоисключение (§10) |
| Дублирование менеджера | Перемещение `#plugin-manager`, не второй экземпляр |
| Хрупкость e2e из-за анимации | Проверять класс `.open`, не `getComputedStyle` |
| Переполнение тулбара | Icon-only триггер; проверка на 1280px |

**Обратимость:** все изменения в одном коммите → `git revert`; `drawer.ts` можно оставить,
скрыв триггер.

---

## 16. Вне scope

- Полноценная панель журнала/событий (H3) — здесь только точка входа в статусбаре.
- Удаление плагина с confirm/undo (пункт #9 `TZ-UX-SPEC-CLEANUP.md`).
- IDEA-005 (компактный триггер + попап) — при выборе C не требуется; закрыть/отложить.
- Плагинные тир-1 view (`#view-switch`) и инспектор — без изменений.

---

## 17. Ссылки

- Прототип: `ideas/front_plugins-drawer-c.html`, `ideas/front_plugins-placement.html`.
- `tasks/TZ-UX-SPEC-CLEANUP.md` §1 (контракты), §9 (панель плагинов), §11 (инфо-кнопка).
- `docs/DESIGN_DOC.md` §5.5, §13.2; `ideas/UI-IDEAS.md` (IDEA-005).
- `docs/adr/` — ADR-0024 (создать).
