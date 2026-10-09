# TZ-UI-PANEL — Перенос вида панели плагинов idea8 (строка-объект + аккордеон)

> **Статус:** Запланировано (референс утверждён, реализация не начата)
> **Приоритет:** Средний
> **Область:** Только фронт (`crates/app/**`); Rust/`md-core`/`src-tauri` не трогаем
> **Источник истины:** `ideas/front_idea8.html` (утверждённый референс нового вида)

---

## Контекст

Вид `.pl-panel` (ширина `--w-sidebar: 224px`) переработан в референсе `ideas/front_idea8.html`:
карточка-строка высотой ~36px с цветовой полоской `--pc` + раскрываемое тело-аккордеон.
Принятая концепция — гибрид «строка-объект + аккордеон» (диагностика и рекомендации —
в анализе `.pl-panel`, см. «Ссылки»).

Задача — перенести вид в продукт: `crates/app/src/style.css` + `crates/app/src/pluginManager.ts`
(точечно `index.html`, `main.ts`), **сохранив все замороженные e2e-селекторы**.

### Замороженные e2e-контракты (нельзя ломать)

| Селектор | Проверка в e2e |
|---|---|
| `#plugin-manager .pl-item[data-plugin="<id>"]` | контейнер карточки (`plugins.steps.js:117,137,173,195,249,266,414,428,444,475`) |
| `.pl-badge` + класс `active\|failed\|quarantined\|stopped` | `classList.contains` (`:119-120,197-198`) |
| `input.pl-enabled` | `.checked`/`.click()` (`:139`) |
| `button.pl-reload` | `disabled`-свойство (`:175-176`) |
| `.pl-cmd[data-command]` | синтетический `.click()` (`:213,298`) |
| `input.pl-grant[data-permission]` | `checked`/`disabled` (`:251,268`) |
| `button.pl-info[aria-controls]`, `.pl-info-pop[hidden]`, `aria-expanded` | (`:415,428,446-448,477-479`) |
| `.pl-perms-bulk button` | (`:502`) |

**Ключевой инвариант:** все e2e-шаги кликают через `browser.execute(el => el.click())`
(синтетический клик, работает для `display:none`). Поэтому:

> **`.pl-item-body` обязан ВСЕГДА рендериться в DOM и скрываться только CSS
> (`display:none`), без атрибута `hidden` и без условного создания узлов.**

---

## Фаза 1 — CSS quick wins (безопасно на текущем DOM)

**Файл:** `crates/app/src/style.css` (блок «Менеджер плагинов (H2, Фаза 5)», ~строки 300-459).

- [x] `.side-title` (~291-295): `display:inline-flex; align-items:baseline; gap:6px`; добавить
      `.side-count` и `.side-count.warn` (по референсу `front_idea8.html:177-179`).
- [x] `.pl-badge` (~320-334): pill → «точка + слово» (`::before`-кружок 6px `currentColor`,
      убрать border/background/padding); **классы состояний сохранить** (`front_idea8.html:241-263`).
- [x] `.pl-info-pop` (~365-380): `max-width: min(300px, calc(var(--w-sidebar) - 24px))` —
      фикс обрезки в 224px.
- [x] Секция согласия `.pl-perms*` (~394-436): строки `.pl-perm` без рамки (transparent,
      hover → `--bg-3`); `.pl-perms-head` — `flex-wrap:wrap`; `.pl-perms-title` — `nowrap +
      ellipsis`; bulk-кнопки — `margin-left:auto` (`front_idea8.html:370-411`).
- [x] `.pl-actions` (~438-458): убрать `margin-top`/`padding-top`/`border-top` (лишний
      двойной разделитель); кнопки: база `--bg-2`, **hover `--bg-3`** (инверсия — осветляет,
      как `.tbtn`), `:disabled { opacity:.6 }`, `min-height:24px`, кегль ≥11px.
- [x] `.pl-empty` (~459): классы `.pl-empty-title`/`.pl-empty-hint` заведены; замена рендера —
      Фаза 3.
- [x] Добавить **заготовки под Фазу 2**: `.pl-item::before` (полоска `--pc` — видна уже
      сейчас, выравнивание уточнится в Фазе 2), `.pl-item-expand`, `.pl-item-body { display:none }`,
      `.pl-item.open .pl-item-body`, `.pl-item.open .pl-item-expand svg { rotate }`.
- [x] НЕ менять пока padding/gap `.pl-item` и min-height головы (структурная часть — Фаза 2).
- [x] НЕ переносить референс-only блоки: `.pl-panel.legacy`, `.ref-toggle`.

**Проверка фазы:** `cd crates/app && npm run build` — зелёно ✅ (tsc + vite, 42 модуля).
**Ревью `@code-reviewer`:** 0 blocker/major; 2 minor (комментарий у `.pl-item::before` — исправлен;
выравнивание полоски под будущую 36px-голову — принято сознательно), 2 nit (мелкие расхождения
с референсом — отложены в полировку Фазы 2/3).

---

## Фаза 2 — Структура карточки и аккордеон (TS + структурный CSS)

**Файлы:** `crates/app/src/style.css`, `crates/app/src/pluginManager.ts`.

### style.css

- [x] `.pl-item` (~301-307): `position:relative; padding:0; gap:0;
      border-bottom:1px solid var(--line-soft)`; `:last-child { border-bottom:none }`;
      `::before`-полоска `top:7px; height:22px` из `--pc`; `:hover { background: var(--bg-2) }`.
- [x] `.pl-item-head` (~308): `min-height:36px; padding:0 10px 0 12px` (остаётся `<div>`,
      **не** `<button>` — внутри живут `input.pl-enabled` и `button.pl-info`).
- [x] `.pl-name` (~311-319): `font-weight:600`.
- [x] `@media (prefers-reduced-motion: reduce)`: погасить поворот шеврона.

### pluginManager.ts

- [x] **Состояние раскрытия:** `const expanded = new Set<string>();` внутри
      `createPluginManager` (замыкание, рядом с `openInfoButton`, ~стр. 147) — переживает
      `replaceChildren`, не течёт, `expanded.clear()` в `dispose()`.
- [x] Хелпер `bodyId(id)` → `pl-body-<id>` (рядом с `infoPopoverId`, ~70-72).
- [x] Иконка-шеврон `chevronIcon()` по образцу `infoIcon` (`createElementNS`, polyline
      `9 6 15 12 9 18`, `aria-hidden`).
- [x] `renderExpandButton(info, open)`: `button.pl-item-expand`, `type=button`,
      `dataset.expand`, `aria-expanded`, `aria-controls=bodyId(id)`,
      `aria-label="Развернуть/Свернуть карточку <id>"`.
- [x] **`renderItem` (~226-295) пересобрать:**
  - порядок head: **тумблер → имя → info (если notices) → бейдж → шеврон**;
    `infoPopover` — последним ребёнком head (позиционируется абсолютно, порядок не важен);
  - `const body = el("div", "pl-item-body"); body.id = bodyId(info.id);`
  - в body порядок **fail-notice → actions → consent** (критичное выше рутины);
  - `item.classList.toggle("open", expanded.has(info.id))`; `item.append(head, body)`;
  - сохранить `infoPopovers.set(...)` и защиту «пустой notices → без info-кнопки».
- [x] **`toggleExpand(btn)`** — переключение на месте, **без refresh и без IPC**:
      правка `expanded`, `classList.toggle("open")`, `aria-expanded`, `aria-label`.
      Фокус остаётся на кнопке (узлы не пересоздаются).
- [x] **`onClick` (~485-504)**: ветка `.pl-item-expand` — после `.pl-info` и до `.pl-reload`
      (ветки различаются классами, конфликтов нет; контракт поповера не меняется).
- [x] `render`/`refresh` (~325-335): `renderItem` читает `expanded` → раскрытие
      восстанавливается после каждой перерисовки; доп. правок нет.
- [x] *(опц.)* Прунинг: удалять из `expanded` id, отсутствующие в `list`.
- [x] Бонус Фазы 2: кнопки команд подписаны `command.title` (было «Выполнить»).

**Проверка фазы:** `npm.cmd run build` — зелёно ✅ (tsc + vite, 42 модуля).
**Ревью `@code-reviewer`:** 0 blocker/major; 1 minor (hit-area `.pl-info` выходила за
36px-строку → `inset:-6px` — исправлено), 4 nit (устаревшие комментарии e2e «первая в head» —
исправлены; JSDoc шеврона и комментарий hit-area — исправлены; `expanded.clear()` в
`dispose()` — добавлено).

---

## Фаза 3 — Голова панели, счётчик, пустое состояние

**Файлы:** `crates/app/index.html`, `crates/app/src/main.ts`, `crates/app/src/pluginManager.ts`.

- [ ] `index.html` (~185): в `.side-head` панели плагинов добавить
      `<span class="side-count" id="side-count"></span>` внутри `.side-title`.
      Кнопки `reload-all`/`install` из референса НЕ добавлять (в продукте нет IPC install).
- [ ] `main.ts` `onPlugins` (~151-154): функция `updatePluginCount(list)` — число плагинов,
      либо `«N с проблемой»` + класс `warn` (при `failed`/`quarantined`); вызывать из
      `onPlugins` (композиционный корень владеет chrome панели).
- [ ] `pluginManager.ts` empty-ветка (~328-333): вместо строки — `.pl-empty-title` +
      `.pl-empty-hint` с путём `%APPDATA%/darmark/plugins/<id>/` (путь подтверждён в
      `src-tauri/src/lib.rs:625,628`; сборка узлов через `textContent`, не `innerHTML`).

**Проверка фазы:** `npm run build`; счётчик и пустое состояние — визуально.

---

## Фаза 4 — Полировка a11y и сборка

- [ ] `aria-controls` шеврона указывает на существующий `id`; `aria-expanded` синхронен
      и при `render` (из `expanded.has`), и при `toggleExpand`.
- [ ] `noUnusedLocals`/`noUnusedParameters`: `bodyId`, `chevronIcon`, `renderExpandButton`,
      `expanded` — все используются; старые хелперы не осиротели.
- [ ] Комментарии на русском, объясняют **зачем** (в т.ч. «тело всегда в DOM — так e2e
      кликает скрытые контролы»).
- [ ] `cd crates/app && npm run build` — финально зелёно.
- [ ] Ручной чек: Tab-порядок, `Escape` закрывает поповер и возвращает фокус,
      `prefers-reduced-motion` гасит поворот шеврона.

---

## Технические решения (зафиксированы)

- **(а)** `Set<string>` expanded — в замыкании `createPluginManager` (единственный владелец
  состояния, `dispose()` не оставляет остатков).
- **(б)** Раскрытие переключается на месте (только `classList`/`aria-*`), без refresh и IPC —
  фокус не теряется, гонок нет. `refresh()` дёргается только после реальных операций.
- **(в)** Тело **всегда** в DOM, скрывается только CSS; при `render` раскрытие
  восстанавливается из `Set`.
- **(г)** Счётчик — `index.html` + `main.ts` (`PluginManagerOptions` не расширяем).
- **(д)** Порядок делегирования `onClick`: `.pl-info` → `.pl-item-expand` → `.pl-reload` →
  `.pl-cmd` → `.pl-perms-bulk button`. Поповер остаётся в `.pl-item-head` (e2e ищет
  `item.querySelector(".pl-info-pop")`), «не более одного поповера» сохраняется.

---

## Тестирование и регрессия

### Матрица регрессии (существующие фичи)

| Сценарий → проверка | Риск | Ожидание |
|---|---|---|
| `plugins.feature` «Менеджер плагинов» → `.pl-badge.classList.contains(st)` | pill→точка классы не трогает | **Зелёный** |
| тот же → `input.pl-enabled` `.checked` | тумблер в head | **Зелёный** |
| тот же → `button.pl-reload` `.disabled` | кнопка в `.pl-item-body`; synth-click работает | **Зелёный при «always in DOM»**; иначе `btn = null` → падение |
| «Плагинная правка» / «Согласие» → `.pl-cmd`, `input.pl-grant` | внутри тела | **Зелёный при «always in DOM»** |
| «Кнопка информации» → `.pl-info-pop`, `hidden`, `aria-expanded` | порядок head тест не проверяет | **Зелёный**; обновить комментарии `plugins.feature:115`, `plugins.steps.js:403` («первая в head» устареет) |
| `a11y.feature` prefers-reduced-motion → наличие `@media` | новые правила добавляют | **Зелёный** |
| Прочие фичи (редактор, таблицы, инспектор) | не пересекаются | **Зелёный** |

Заведомых падений нет — при обязательном инварианте «тело всегда в DOM» (зафиксирован выше).

### Новые сценарии (создать `e2e/features/plugin-panel.feature`, шаги — в `plugins.steps.js`)

- [ ] **С1 (высший приоритет).** Раскрытие/сворачивание: `.pl-item.open` есть/нет,
      `aria-expanded="true"/"false"`, `getComputedStyle(.pl-item-body).display`.
- [ ] **С2. Красный тест на инвариант:** тело присутствует в DOM при свёрнутой карточке;
      `aria-controls` указывает на существующий элемент. Если референс начнёт удалять тело —
      упадёт здесь, а не разрозненно в 4 сценариях.
- [ ] **С3.** Подпись команды = `command.title` (проверка `textContent`) — входит в scope
      Фазы 2.
- [ ] **С4.** Счётчик `#side-count` равен числу загруженных плагинов.
- [ ] **С5.** Порядок `fail-notice → actions → consent` — **не добавлять**: нет фикстуры
      со сбоем; пометить как осознанно непокрытое.
- [ ] Новые сценарии в TDD-фазе помечать `@wip` (фильтр `not @manual and not @wip`).

### Ручные проверки (`@manual`)

- [ ] Визуал в 224px: имя не обрезается, строка ~36px, ничего не выдавливается.
- [ ] Поповер `.pl-info-pop` не обрезается краем sidebar (позиционирование в WebdriverIO
      нестабильно — уже задокументировано в проекте).
- [ ] Hover-инверсия кнопок и `:hover` строки.
- [ ] Контраст статусов «точка+слово» (WCAG AA для мелкого текста).
- [ ] `focus-visible` на `.pl-item-expand`/`.pl-info` при реальном Tab.
- [ ] `prefers-reduced-motion`: поворот шеврона мгновенный.
- [ ] Пустое состояние с путём (в E2E недостижимо — фикстуры всегда грузятся).

### Порядок верификации

- [ ] `cd crates/app; npm run build` — `tsc` валит на любой ошибке типов (юнитов фронта нет).
- [ ] `npx tauri build --no-bundle` — свежий `target/release/darmark.exe` для E2E.
- [ ] Разово: `cargo install tauri-driver --locked`; совместимый `msedgedriver.exe` в PATH.
- [ ] Целевой прогон: `cd crates/app; npx wdio run e2e/wdio.conf.js --spec e2e/features/plugins.feature --spec e2e/features/plugin-panel.feature`.
- [ ] Полный прогон: `cd crates/app; npm run test:e2e`.
- [ ] После реализации — ревью `@code-reviewer` перед мержем.

### Риски тестирования

- **Скрытый клик ≠ видимость:** synth-click обходит `display:none`, поэтому для аккордеона
  обязана быть отдельная проверка `getComputedStyle`.
- **`replaceChildren` после каждого действия:** состояние раскрытия — только вне DOM (`Set`),
  иначе все проверки аккордеона станут flaky.
- **Строгий `strict: true` в WDIO:** неопределённый шаг валит прогон — объявлять до запуска.
- **Асинхронность `plugins-changed`:** ждать через `browser.waitUntil`, не паузами.
- **Недостижимые состояния в E2E:** `failed`/`quarantined` (нет crash-фикстуры), пустой
  каталог — не гоняться, помечать `@manual`.
- **Не трогать `pluginStatusBar.ts`** — его селекторы `.ps-item[data-plugin]` проверяются
  той же фичей, вне scope задачи.

---

## Оценка объёма

| Файл | Объём |
|---|---|
| `style.css` | ~150-170 строк замены в блоке 300-459 + ~15 строк `.side-title`/`.side-count` + reduced-motion |
| `pluginManager.ts` | ~15 строк хелперов + ~30 строк `renderItem` + ~12 строк `toggleExpand` + `Set` + ~4 строки `onClick` + ~12 строк empty |
| `index.html` | 1 строка (счётчик) |
| `main.ts` | ~10 строк (`updatePluginCount`) |
| `e2e/` | 1 фича + ~6 шагов (С1-С4) |

**НЕ трогать:** `tauri.ts`, `pluginViews.ts`, `pluginStatusBar.ts`, Rust-крейты,
общие `.tbtn`/`.rail-btn`, референс-only блоки (`.pl-panel.legacy`).

---

## Критерии готовности

- [ ] `npm run build` зелёно (strict TS, `noUnusedLocals`).
- [ ] Все замороженные селекторы присутствуют в реальном рендере (сверка с `plugins.steps.js`).
- [ ] `npm run test:e2e` — `plugins.feature` + новая `plugin-panel.feature` зелёные.
- [ ] Шеврон раскрывает/сворачивает без перерисовки; после `refresh` раскрытие сохраняется.
- [ ] Тело всегда в DOM (`display:none` при свёрнутом, без `hidden`).
- [ ] Статус — точка+слов; классы и `STATUS_LABELS` сохранены.
- [ ] Согласие без рамок; «Доступ: N из M» не растрёпан при 4 правах; hover осветляет.
- [ ] Счётчик: число или `N с проблемой` (амбер); пустое состояние — с путём к каталогу.
- [ ] Кнопки команд: `textContent = command.title`, `aria-label = «Выполнить: <title>»`.
- [ ] a11y: Tab-обход, `Escape` + возврат фокуса, reduced-motion.
- [ ] `git diff --name-only` не содержит `crates/md-core/**`, `**/src-tauri/**`,
      `crates/plugin-*/**` (только фронт).
- [ ] Ревью `@code-reviewer` пройдено.

---

## Ссылки

- Референс (источник истины): `ideas/front_idea8.html` — CSS `:196-421`, разметка/логика
  `:761-905`, счётчик `:860-876`, пустое состояние `:852-857`, заморозка селекторов `:32-44`.
- Текущий CSS: `crates/app/src/style.css:291-295` (`.side-title`), `:300-459` (блок менеджера).
- Контроллер: `crates/app/src/pluginManager.ts:41-56` (опции/лейблы), `:70-135` (id/икконки),
  `:137-149` (состояние), `:151-223` (бейдж/согласие), `:226-295` (`renderItem`),
  `:325-351` (`render`/`refresh`), `:485-525` (`onClick`/делегирование), `:536-544` (`dispose`).
- Обвязка: `crates/app/index.html:184-190`, `crates/app/src/main.ts:145-155`.
- Типы: `crates/app/src/tauri.ts:94-119`; каталог плагинов: `src-tauri/src/lib.rs:625,628`.
- e2e: `crates/app/e2e/features/plugins.feature`, `crates/app/e2e/steps/plugins.steps.js`.
- Прежние артефакты: анализ `.pl-panel` (системный аналитик + UI/UX-дизайнер) — в диалоге;
  `ideas/UI-INFO-BUTTON.md` (поповер), `ideas/front_idea5.html`/`front_idea7.html` (прототипы).
- Правила репозитория: `AGENTS.md` (строгий TS, проверка `npm run build`, только фронт,
  русские комментарии/UI-строки).
