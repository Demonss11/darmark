# UX-spec — darmark: мост `front_idea7` → основная программа

> **Статус:** non-normative проектный документ (design/UX-spec). Нормативная спецификация — `docs/DESIGN_DOC.md`; замороженные контракты — §13.2; плагинный UX — `tasks/TZ-H2.md`.
> **Основание:** результат 4 параллельных аудитов (`front_idea4`, `front_idea5`, `UI_ABOUT.md`) + карта основной программы (`crates/app`).
> **Артефакты этого шага:** `ideas/UX-spec.md` (этот файл) и `ideas/front_idea7.html` (прототип-мост).
> **История прототипов:** `front_idea` → … → `front_idea4` → `front_idea5` → `front_idea6` (текущий) → `front_idea7`.
> **Источник истины об интерфейсе — код** `crates/app` (`index.html`, `src/*`, `src/style.css`), а не HTML-прототипы.

---

## 0. Ключевой тезис

Основная программа **уже реализована** (H1 закрыт, H2 Фаза 4 сделана): есть `viewRegistry` (два тира), `paneHost`, `pluginViews.ts` (тир-1 плагинные view + маршрутизация `data-p-*`), `pluginManager.ts`, 8 e2e-спеков, замороженные id/селекторы.

HTML-прототипы — **референс, не контракт** (`UI_CONCEPT.md:3-9`). Значит:

- **Нельзя** дальше изобретать новую структуру DOM, которую приложение не сможет взять: `.pane-body`, `.view-host`, `#preview-scroll`, `#plugin-view-scroll`, `#plugin-view-host` в проде **не существуют** и вводить их как контракт нельзя (сломает `paneHost`, scrollsync, inspector, e2e).
- **Нужно** свести прототип к реальному DOM и на базе него принять решения, которые переносятся в `crates/app` точечно (CSS-токены, визуальный язык, недостающие поверхности-заглушки).

**`front_idea7` — это визуальный мост**: прототип, который использует **те же id/классы/токены, что и основная программа**, плюс отобранные UX/a11y-улучшения. Он нужен не «ещё на итерацию», а чтобы решить: **что дорабатывать в `crates/app`** (см. §11).

---

## 1. Что уже есть в основной программе (истина)

| Область | Реализовано | Файл |
|---|---|---|
| Композиция | ~239 строк, без доменной логики | `crates/app/src/main.ts` |
| Layout/Panes | Pane/Split-три, `MAX_PANES=2`, `splitPane`/`closePane` | `layout.ts`, `paneHost.ts` |
| View-модель | тир-1 `HtmlViewProvider` + тир-2 `DomViewProvider`, `ViewContext`-фасад | `viewRegistry.ts` |
| Редактор / превью | tier-2 `editorView`, tier-1 `previewView` | `editorView.ts`, `previewView.ts` |
| Плагинные view | тир-1, kind = `view_id`, `unregisterHtml`, `#plugin-view`, `data-p-*` | `pluginViews.ts` (284 стр.) |
| Менеджер плагинов | `#plugin-manager`, статусы `active/failed/quarantined/stopped` | `pluginManager.ts` (244 стр.) |
| Инспектор-подсветка / scrollsync | общий `RenderIndex`, `data-md`, двусторонняя подсветка блок/ячейка ↔ исходник | `linkController.ts`, `inspector.ts`, `scrollsync.ts`, `renderIndex.ts`, `mapping.ts` — **ЗАЩИЩЁННЫЙ функционал, см. §2.1** |
| Таблицы | сорт/поиск/фильтры колонок/popover | `tables.ts` (459 стр.) |
| Токены | единый `:root`, тёмная тема, без light | `style.css:4-39` |
| Тесты | 8 feature-спеков + wdio/cucumber | `crates/app/e2e` |

**Заглушки (сюда можно вкладывать UI без ломки контрактов):** `#palette-trigger`, `#rail-logs`/`#log-badge`, `#tab-strip`/`#tab-current`, мёртвый drawer `#logs`/`#backdrop` (inline-хендлеры не определены нигде — безопасно удалить или реализовать).

> **Расхождение спеки с кодом (§6, #8/#11):** `#plugin-strip` в этой таблице больше не заглушка — удалён намеренно (`BUG-004`, см. §6 п.11); `--pc` уже частично портирован (`pluginColor.ts` → `palette.ts`/`pluginStatusBar.ts`). Актуальный статус — §6.

---

## 2. Замороженные контракты (нельзя ломать)

Из `DESIGN_DOC §13.2` (строки 577-583) + де-факто из e2e:

- **id:** `app, toolbar, panes, statusbar, editor, preview, btn-new, btn-open, btn-save, btn-save-as, file-label, toggle-preview, toggle-sync, btn-inspect, chk-preview, chk-sync, stat-msg, stat-pos, stat-size, stat-inspect`.
- **де-факто id (используются спеками):** `view-switch`, `plugin-view`, `plugin-manager`, `panel-plugins`, `rail-plugins`, `format-group`, `e2e-marker`, `gutter`, `tab-name`, `tab-dirty`.
- **селекторы:** `#preview .md-block[data-md]`, `.table-enhanced`, `.table-enhanced tbody tr`, `.table-enhanced td[data-md]`, `.table-count`, `.table-scroll`, `.col-filter-btn`, `.col-filter-menu`, `.col-filter-item`, `.inspect-active`, `.inspect-col`, `tr.inspect-row`, `.vtab.core`, `.vtab.plugin[data-view]`, `.pl-item[data-plugin]`, `.pl-badge`, `input.pl-enabled`, `button.pl-reload`.
- **прочее:** текст «Добро пожаловать в darmark»; заголовки таблицы `Файл | Размер | Строк | Изменён`; debounce 120 мс; `ECHO_MS=100`; throttle инспектора 100 мс; `window.__xss` / `window.__errors` / `__errorCapture`.
- **инвариант scroll:** `#editor` и `#preview` — **сами скролл-контейнеры** (e2e читает их `scrollTop`). Оборачивать их в новые `.editor-scroll`/`.preview-scroll` **нельзя**.
- **инвариант переключения view:** переключение — `hidden`-toggle уже смонтированных узлов, не пересоздание DOM (`paneHost.ts`, `pluginViews.ts`).

> Итог: §2 — это «красные линии». Всё, что предлагает §4 и далее, обязано их не нарушать.

---

## 2.1. Защищённый функционал: НЕ терять

Критично: слово «инспектор» обозначает в спеке/прототипах **две разные сущности**. Их нельзя смешивать — это прямой риск потери рабочего функционала.

| | **Инспектор-подсветка (Highlight / Link mode)** — ТЕКУЩИЙ, ЗАЩИЩЁН | **Инфо-панель метаданных (Infopanel)** — предлагаемая НОВАЯ |
|---|---|---|
| Что делает | Двусторонняя подсветка «блок/ячейка предпросмотра ↔ фрагмент исходника» | Пассивный справочник: id/rev/размер/строка-колонка/блок/RenderIndex/список плагинов |
| Триггер | `#btn-inspect`, `Ctrl+I`, `#stat-inspect` | отдельный `#btn-info` (или пункт меню) |
| Стейт | режим (toggle) | открыт/закрыт (dialog) |
| Ядро | `inspector.ts`, `linkController.ts`, `scrollsync.ts`, `renderIndex.ts`, `mapping.ts`, `data-md`, `.inspect-active/.inspect-col/tr.inspect-row` | новые узлы, **без** `data-md`-логики |
| Статус | **нормативный, e2e `inspector`/`inspect-tables`, менять нельзя** | additive, не обязателен |

**Жёсткие правила:**
1. `#btn-inspect`, `#stat-inspect`, `Ctrl+I`, режим подсветки, `data-md`, `.inspect-*` — **не переименовывать, не перевязывать, не подменять** дровером. E2E `inspector.feature`/`inspect-tables.feature` должны остаться зелёными.
2. Классы и id инфо-панели **не должны** начинаться с `inspect`/`inspector` (чтобы не сталкиваться с режимом подсветки). Прототипный `#inspector`/`--w-inspector` = инфо-панель; при переносе переименовать в `#infopanel`/`--w-infopanel`.
3. Инфо-панель **никогда** не заменяет и не «улучшает» подсветку — она существует рядом. Если места/времени нет — инфо-панель откладывается, подсветка не трогается.
4. Тот же принцип — для **таблиц**: у прода есть сорт/поиск/фильтры/липкая шапка и granular `th/tr/td[data-md]` (`tables.ts`); прототипная упрощённая таблица и ее `col-filter-menu` **не переносятся** поверх продовой логики.

---

## 3. Реconciliation компонентной модели (prototype ↔ real)

Концептуально аудит прав: **pane = структура, view = данные, один персистентный хост на view, переключение через toggle**. Но имена и вложенность в проде уже другие. Маппинг:

| Концепт idea4/idea5 | В основной программе | Решение для idea7 |
|---|---|---|
| `.pane-head` | `.pane-head` (есть!) | **Использовать как есть.** В idea7 — всегда видна (не `display:none`) |
| `.view-switch` | `#view-switch` (есть) | Использовать; добавить корневую вкладку «Предпросмотр» + плагинные |
| `.pane-body` (нет в проде) | тело `.pane` напрямую | **Не вводить как контракт**; в idea7 допустимо как визуальная обёртка, но не переносить |
| `.view-host[data-view]` (нет) | `#preview` + `#plugin-view` (toggle) | Концепт «хост на view» **сохранить логически**; физически — уже смонтированные контейнеры с toggle. В idea7 отразить `#preview` + `#plugin-view` |
| `#plugin-view-host` / `.preview-scroll` (idea5/6) | не существуют | **Не переносить.** Убрать в idea7 |
| `.plugin-view` | `#plugin-view.plugin-view` (есть) | Использовать как есть |
| `.resize-handle` | отложен (`UI_CONCEPT.md §3`) | В idea7 показать, но помечать «отложено в проде» |
| `.divider` | `#divider` (есть, в `.panes`) | Использовать |

**Инварианты (для idea7 и для порта):**
1. Один персистентный контейнер на view; switch = `hidden`, не innerHTML-пересборка.
2. `.pane-head` всегда отрисован; per-view действия меняются внутри `.pane-head-actions`.
3. `#editor`/`#preview` остаются скролл-контейнерами.
4. Состояние view per-pane (не глобальная `ui.view`-строка), чтобы `MAX_PANES=2` и >1 view на плагин были выразимы.
5. Маршрутизация кликов — namespaced `data-p-<pluginId>-action` на делегированном контейнере, pluginId из атрибута, viewId из ближайшего хоста (не из глобального «активного view»).

---

## 4. Токены

**Канонические имена = текущие `crates/app/src/style.css`** (idea5-производные). **Не переименовывать** `--line`/`--line-soft`/`--fg*` (idea4-имена `--border`/`--text` не использовать).

- Текущие: `--bg-0..3, --line, --line-soft, --fg, --fg-dim, --fg-mute, --head, --body, --accent, --accent-2, --accent-soft, --green, --amber, --red, --teal, --gutter`, `--h-toolbar/tabs/status`, `--w-rail/sidebar`, `--r-sm/md`, `--ed-fs/lh/pad-y`.
- **Добавить (из idea5, нужны для дроверов/поповеров):** `--popover`, `--pop-border`, `--shadow-lg`, `--shadow-sm`, `--r-lg`. Токен ширины инфо-панели — **`--w-infopanel`** (в прототипе назван `--w-inspector`; при переносе переименовать, чтобы не путать с режимом подсветки, §2.1).
- **Добавить (инфраструктура):** `--accent-rgb` (убрать литералы `rgba(124,92,255,…)` — 19 в idea4 / 13 в idea5), `--code-fg` (в idea5 регресс — захардкожен `#e9b3ff`), `--pc` (цвет плагина, §5).
- **Правило:** идея7 не вводит новых токенов сверх этого списка; расширение — через `crates/app/src/style.css`.

---

## 5. Идентичность плагина (`--pc`)

Самый ценный и оригинальный элемент idea4 — **цвет плагина**, идущий через тулбар-стрип → карточку → лог → статусбар → view-вкладку → палитру. В idea5 потерян (заменён моно-монограммой).

**Решение:** вернуть `--pc` как первоклассную систему и **концентрировать boldness в ней** (вместо статичного фиолетового «акцента»):
- цвет плагина — стабильный (детерминированная HSL из `plugin_id` или из манифеста, если задан);
- `--pc` применяется к: monogram, полосе карточки, `origin`-точке view-вкладки, бейджу источника в логе, статусбар-элементу, бейджу origin в палитре;
- хью **сфокусированного** плагина может подсвечивать акцент панели (опционально, не обязательно).

Это одновременно решает задачу a11y — состояние плагина перестаёт кодироваться только цветом (см. §7, состояние дублируется словом).

---

## 6. Сводный бэклог — что уже закрыто, что открыто

| # | Приор. | Проблема | Статус в проде | Где видно |
|---|---|---|---|---|
| 1 | P0 | Install игнорирует снятые чекбоксы прав | ✅ **Закрыто** | `pluginManager.ts` `renderConsent` + `handleGrant`/`handleGrantBulk` — чекбоксы реальный гейт; `setPluginPermissions` |
| 2 | P0 | `closeDoc` без подтверждения | ✅ **Закрыто** | `fileActions.ts` `confirmDiscard()` перед `newFile`/`openFile`; `shell.ts` `onCloseRequested` |
| 3 | P0 | Конфликт `⌘K` vs `⌘⇧K` | ✅ **Закрыто** | `shell.ts`: `k&&shift→link` проверяется ДО `k→palette`; `palette.ts` `stopPropagation` от переоткрытия |
| 4 | P0 | Reload-хоткей без обработчика | ✅ **Закрыто** | `shell.ts` `Ctrl+R`→`reloadPlugins()`, всегда `preventDefault`; `pluginManager.reloadForDocument` |
| 5 | P0 | Нет `:focus-visible` / `prefers-reduced-motion` | ✅ **Закрыто** | `style.css` `:focus-visible{outline}`, `@media (prefers-reduced-motion)` |
| 6 | P0 | Диалоги без `role="dialog"`/focus-trap | ✅ **Закрыто** | `dialog.ts` + `palette.ts` (combobox/listbox/`aria-activedescendant`), `toast.ts` — по §11.1 п.3 |
| 7 | P0 | `UI_ABOUT.md` 100% compliance | ⬜ **docs** — вне `crates/app/src`, не трогали |
| 8 | P1 | Потеряна идентичность `--pc` | ✅ **Закрыто** | `pluginColor.ts` применяется в `pluginManager.ts` (`.pl-item`), `pluginViews.ts` (`.vtab.plugin .origin`), `palette.ts`, `pluginStatusBar.ts` (`.pdot`) |
| 9 | P1 | Удаление плагина без confirm/undo | ⬜ **Отложено в H3** (нет операции удаления) — см. §11.1 п.4 |
| 10 | P1 | Крах-рампа «падений N/3» не видна | ✅ **Закрыто** — решение: счётчик N/3 **не нужен**, достаточно статусов `failed`/`quarantined`. Оба уже видны: бейдж `pl-badge.failed` (красный) + текст ошибки и `pl-badge.quarantined` (янтарный) в `pluginManager.ts` (`STATUS_LABELS`/`renderBadge`/`statusMessage`), подписи в `pluginStatusBar.ts`; карантин после 3 падений — `Quarantine::record_failure` (`manager.rs`). Промежуточный прогресс «1/3→2/3» сознательно не показываем |
| 11 | P1 | Плагины без view невидимы; >1 view | ✅ **Закрыто** | `pluginViews.ts` рендерит **несколько** вкладок на плагин (`view_id` уникален) — «>1 view» решён. Плагин без view виден в менеджере и статусбаре, но **не** в `view-switch`. `#plugin-strip` убран намеренно (`BUG-004`) |
| 12 | P1 | Схлопывание per-plugin статусбара в «N активных» | ✅ **Закрыто** | `pluginStatusBar.ts`: элементы с `--pc`+текст, лимит `MAX_VISIBLE_PS=3` + «+N» |
| 13 | P1 | Палитра-заглушка, нет origin/группировки | ✅ **Закрыто** | `palette.ts`: группы+счётчики, `origin`-бейдж с `--pc`, seq-гейт |
| 14 | P1 | Разнобой `⌘` vs `Ctrl` | ✅ **Закрыто** | grep `⌘` — 0 совпадений; везде `Ctrl` (Windows-first) |
| 15 | P1 | Жаргон в пользовательских строках | ✅ **Закрыто** | `tier/DomView/RenderIndex/MAX_PANES` встречаются **только в комментариях**, в `textContent`/копирайтите — нет |
| 16 | P1 | Нет per-view `pane-head-actions` (copy HTML / render time / rev) | ✅ **Закрыто** | `index.html` + `style.css` — `.pane-head-actions` с кнопкой `#btn-copy-html` (Copy HTML, автономные стили для буфера). Render Time и Rev сознательно отклонены как избыточные |
| 17 | P2 | Табличные регрессы | ✅ **Закрыто** | `tables.ts` — сорт/поиск/фильтры/`th`/`tr`/`td[data-md]` (защищено §2.1) |
| 18 | P2 | `transition: all`, нет `tabular-nums` | ✅ **Закрыто** | grep `transition: all` — 0; `tabular-nums` в `.statusbar` и `.palette-count` |
| 19 | P2 | Glow-точки `0 0 Npx currentColor` | ✅ **Закрыто** | grep `box-shadow: 0 0` — 0 |
| 20 | P2 | Спец-ссылки в UI-копирайтите | ✅ **Закрыто** (совпало с #15) |

---

## 7. Доступность — обязательный минимум для idea7

- `aria-label` на всех icon-only кнопках (в idea6 уже есть — сохранить).
- Интерактивные `<div>`/`<span>` → `<button>` (view-вкладки, tree-row, `.pl-head`, todo-item, palette-item, status-item). Где button нельзя — `role="button"`, `tabindex="0"`, `onKeyDown` (Enter/Space).
- `role="dialog"`, `aria-modal="true"`, `aria-labelledby`, focus-trap и возврат фокуса — для палитры, модалки, инфо-панели метаданных и логов. Режим подсветки (`#btn-inspect`) — **не** диалог и этим правилам не подчиняется (§2.1).
- `aria-live="polite"` на статусбаре/status-message и toast-host; `role="status"`/`role="alert"`.
- `@media (prefers-reduced-motion: reduce)` — отключить slide/pop/toast/flash.
- `:focus-visible` — видимый ring; `outline:none` только с заменой.
- `<meta name="color-scheme">`, `<meta name="theme-color">`, `color-scheme`.
- Состояние — не только цветом (текстовая метка статуса рядом с точкой `--pc`).
- `font-variant-numeric: tabular-nums` на числовых полях статусбара.
- Тач-таргеты ≥ 24×24 CSS-visual, hit-area до 44px.
- Заменить `transition: all` на явные свойства; анимировать `transform`/`opacity`.

---

## 8. Визуальное направление (решение)

Аудит арт-дирекшна: «near-black + один фиолетовый» — типовой AI/дев-тулз дефолт; присутствуют штампы (middle-dot строки, UPPERCASE tracked микро-лейблы, градиентный лого-чип, glow-точки, утечка жаргона).

**Решение (консервативное, чтобы не ломать бренд проды):**
1. Сохранить текущую палитру проды (она уже принята в `style.css`) — не ре-дизайнить с нуля на этом шаге.
2. **Убрать штампы:** middle-dot мета-строки → пробелы/вторичные строки; UPPERCASE tracked лейблы → sentence case + вес; внутренний жаргон → в «Разработчику».
3. **Сконцентрировать boldness в `--pc`** (§5) и в одном сдержанном акцентном движении (например, подсветка блока под курсором), а не размазывать по точкам/градиентам.
4. Градиентный логотип — заменить на типографический моно-знак (в прототипе; для проды — отдельное решение).
5. Новые предложения палитры (printer's-ink/galley, Lua-navy) — **не** в этом шаге; зафиксировать как IDEA в `UI-IDEAS.md` при желании.

---

## 9. Scope `front_idea7.html`

Собрать как **standalone визуальный мост**, база — `front_idea6.html` (в нём уже a11y-фиксы и `renderViewSwitch`), с правками:

1. **DOM-имена как в проде.** Использовать `#panes`, `.pane-head`, `#view-switch`, `#preview`, `#plugin-view.plugin-view`, `#editor`, `#gutter`, `#statusbar`, `#panel-plugins`, `#plugin-manager`, `#plugin-strip`, `#palette-trigger`, `#rail-*`, `#tab-strip`. **Удалить** `#plugin-view-scroll`, `#plugin-view-host`, `.preview-scroll` как отдельный слой; переключение — `hidden` на `#preview`/`#plugin-view`.
2. **`.pane-head` всегда виден** в обеих панелях; per-view действия (`rev`, render time, copy HTML) — в `.pane-head-actions`.
3. **`--pc`** для плагинов + текстовые статусы (не только цвет).
4. **P0-фиксы:** consent в install, confirm при закрытии/удалении, согласованные хоткеи (+ реальный reload), `:focus-visible`, `prefers-reduced-motion`, dialog-семантика, `aria-live`.
5. **Паритет фич проды:** таблицы (фильтры/инспекция ячеек), todo «к строке», non-md note, breadcrumb-сигнал (или явный путь в pane-head), корневая вкладка «Предпросмотр» в `#view-switch`.
6. **Заглушки-поверхности** (полезны для будущего порта): `#plugin-strip` (кнопки с `--pc`), палитра (origin-бейджи, группировка, счётчики), журнал/события как **нижняя панель** (в проде — drawer-заглушка), вкладки документов.
7. **Токены:** использовать канонические (`--line`/`--fg`) + добавить недостающие (§4).
8. В `<title>`/комментарии — пометка «idea7 · visual bridge to crates/app», чтобы не был принят за контракт.
9. **Инфо-панель метаданных** в прототипе (`#inspector`) — это *additive* справочник; она **не** является режимом подсветки и не претендует на id `#btn-inspect`/`#stat-inspect` (см. §2.1). При переносе — `#infopanel`, `--w-infopanel`.

---

## 10. Критерии приёмки idea7

- Открывается автономно, без ошибок в консоли; `window.__errors` пуст.
- Все id/классы §2 присутствуют и ведут себя как в проде (риск регрессии контракта = 0 при переносе).
- a11y-минимум §7 выполнен (проверяется статически + вручную Tab/Enter/Esc).
- `--pc` виден на ≥3 поверхностях; состояние плагина читаемо словами.
- P0-фиксы §6 воспроизводимы в прототипе.
- Прототип **не вводит** `.pane-body`/`.view-host`/`#plugin-view-host` как обязательные контракты.

---

## 11. Дальнейшие шаги: чинить прототип или программу?

**Решение: дальше дорабатывать ОСНОВНУЮ ПРОГРАММУ.** Прототип-цепочка достигла точки насыщения — идея7 нужен как финальный визуальный референс, после которого продолжать `front_ideaN` неэффективно (каждый новый прототип всё дальше от реального DOM и не проверяется e2e).

Переходный алгоритм:

1. **Идея7 принят** → из него извлекаются **два конкретных дельта-артефакта**: (а) CSS/токен-патч, (б) список DOM/JS-правок поверх `crates/app`, каждый — со ссылкой на контракт §2, который он не ломает.
2. **Пилотный перенос без риска** — заполнить **заглушки** `#plugin-strip`, `#palette-trigger`, log-панель, `#view-switch`-полиш. Это не трогает замороженные контракты, но даёт максимум видимого эффекта.
3. **P0-баги** (consent, confirm, hotkeys, a11y диалогов) — в `crates/app` + покрыть e2e; это **не** прототипная задача.
4. **`--pc`-идентичность** — сначала как CSS-переменная + проброс `plugin_id`→hue в `pluginViews`/`pluginManager`, затем порт.
5. **Ре-дизайн палитры/типографики** (§8 п.4-5) — только отдельным ADR/IDEA после H2; не смешивать с переносом.

**Признак, что нужно вернуться к прототипу:** обнаружено архитектурное несоответствие, которое нельзя проверить e2e в проде (новая модель view/pane, MAX_PANES>2, движки-плагины). Тогда — отдельный спайк (`crates/*-spike`) + ADR, а не новый HTML.

**Итог:** `front_idea7` = последний визуальный прототип в линии; далее — `tasks/TZ-H2.md` Фазы 5–6 и точечные UI-правки `crates/app`, при необходимости с новыми IDEA в `ideas/UI-IDEAS.md`.

### 11.1. Проверенная очерёдность (после приёмки idea7)

Проверено на `front_idea7.html` (Playwright, headless Chromium; `consoleErrors/pageErrors/__errors` пусты). Порядок — от безопасного к требующему e2e:

1. **[SAFE] Токен/типографика-патч `crates/app/src/style.css` — ВЫПОЛНЕНО**: добавить `--accent-rgb`, `--code-fg`, `--popover`, `--pop-border`, `--shadow-lg/sm`, `--w-inspector`, `--r-lg`, `--pc`; убрать литералы `rgba(124,92,255,…)`; de-stamp (`.brand-mark`, `.side-title`, `.pl-badge`, `.stat-dot`, `transition: all`). Проверка: `npm run build` + `smoke`/`preview`. Контракты не задеты, scroll-правила `#editor`/`#preview` не трогать.
2. ~~**[SAFE→1 e2e] `#plugin-strip` с `--pc`**: `pluginColor()` + наполнение стрипа из `PluginInfo`. Аддитивная заглушка-поверхность.~~ **ОТМЕНЁН** (`BUG-004`, `ideas/UI-BUGS.md`): полоса разрастается O(N) по числу плагинов и убрана; идентичность/быстрый доступ — `IDEA-005` в `ideas/UI-IDEAS.md`.
3. **[E2E] Dialog-инфраструктура — ВЫПОЛНЕНО**: `dialog.ts` (focus-trap/Escape/возврат фокуса/`inert`), `toast.ts` (`#toast-host`), `palette.ts` (палитра `Ctrl+K`/`#palette-trigger`, core + плагинные команды), `#modal-root`; заглушки `main.ts`/`shell.ts` заменены (хоткеи глушатся при открытом модале, `Ctrl+Shift+K` → ссылка). Покрыто `e2e/features/palette.feature` (+ навигация `↑/↓`/`Home`/`End` с `aria-activedescendant`, закрытие кликом по подложке, `role="alert"` у тоста ошибки). **Найденный баг исправлен:** повторный `Ctrl+K` переоткрывал палитру (событие всплывало до `window`, где `shell.ts` при `isModalOpen() === false` вызывал `toggle()`); в `palette.ts` всплытие гасится `stopPropagation`, сценарий «Повторный Ctrl+K закрывает палитру» зелёный (не `@wip`). **Инфо-панель метаданных** (в прототипе `#inspector`) — остаётся отдельной сущностью `#infopanel`; **не** переиспользовать `#btn-inspect`/`Ctrl+I`/`#stat-inspect` под неё и не трогать режим подсветки (§2.1).
4. **[E2E] Consent — ВЫПОЛНЕНО** (uninstall-confirm → H3): Rust IPC-слой поверх `settings.rs` — команда `set_plugin_permissions(id, granted)` + `PluginInfo.granted_permissions`; эффективные права = `manifest ∩ granted` (deny-by-default), child при старте получает `effective()`; `load_plugins` читает согласие из `config.json`, смена прав у включённого плагина применяется перезапуском. UI: инлайн-чекбоксы согласия в `.pl-item` менеджера (read-only chips заменены). **Uninstall-confirm не делаем** — вне H2 (нет операции удаления), уходит в H3/реестр. Сохранены `.pl-item`/`.pl-badge`/`input.pl-enabled`/`button.pl-reload`. Покрыто `e2e/features/plugins.feature` («Согласие на разрешения — реальный гейт»).
5. **[E2E] Reload-хоткей (`Ctrl+R` для `.lua`) + per-plugin статусбар — ВЫПОЛНЕНО** (фронт): `shell.ts` — `Ctrl+R` (и `Ctrl+Shift+R`) всегда `preventDefault` (WebView не перезагружается), при открытом модале хоткей заглушён; `pluginTarget.ts` резолвит владельца открытого `.lua` (родительский каталог = id), иначе перезагружаются все включённые (fallback, reload не разрушителен; ветка владельца — `@manual`, путь E2E не выставить). Пер-плагинный статусбар `#plugin-status` (`pluginStatusBar.ts`): включённые плагины с выданным `ui:statusbar`, точка `--pc` + последнее `host.show_message`/статус, лимит 3 (+`N`), клик — представление плагина; отдельный polite-аннонсер; `pluginColor` вынесен в общий модуль. Расширен `plugins.feature`. Rust не менялся.

**Не делать сейчас:** мультивкладки документов, инфо-панель метаданных (отдельная от режима подсветки, §2.1), click-to-inspect, ре-дизайн палитры/типографики (§8 п.4-5) — до закрытия пунктов 1–5 с покрытием. **Режим подсветки (`#btn-inspect`) остаётся как есть на всех шагах.**

---

## 12. Файлы

| Файл | Роль |
|---|---|
| `ideas/front_idea7.html` | прототип-мост (этот шаг) |
| `ideas/UX-spec.md` | этот документ |
| `ideas/front_idea6.html` | база для idea7 (последний прототип с a11y-фиксами) |
| `ideas/front_idea4.html` | источник идей `pane-head`/`pane-body`/`view-host`/`plugin-view`, `--pc` |
| `crates/app` | источник истины об интерфейсе |
| `docs/DESIGN_DOC.md` §5/§13.2 | нормативная модель и контракты |
| `tasks/TZ-H2.md` | плагинный UX (Фазы 5–6) |
| `ideas/UI-IDEAS.md` | бэклог идей (сюда попадают отложенные: палитра, типографика) |
