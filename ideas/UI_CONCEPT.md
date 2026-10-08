# UI_CONCEPT — интерфейс darmark (актуально на H1)

> **Статус:** non-normative, черновик-описание **реализованного** интерфейса H1.
> **Нормативная спецификация:** `docs/DESIGN_DOC.md` (§5 — layout/pane/view, §13.2 — замороженные e2e-контракты).
> **Целевой UX плагинов (H2):** `tasks/TZ-H2.md`, Фаза 5.
> **Референс-прототип:** `ideas/front_idea5.html` (вдохновение, не контракт).
> **Бэклог идей развития UI:** `ideas/UI-IDEAS.md`.
> **История:** линия `front_idea` → `front_idea2` → `front_idea3` → `front_idea4` → `front_idea5` (архив — `ideas/архив/`).
> Источник истины об интерфейсе — **код** `crates/app` (`index.html`, `src/*`, `src/style.css`).

---

## 0. Принципы

| Принцип | Что это значит |
|---|---|
| Простота | «Notepad++ для Markdown»: слева редактор, справа превью; минимум хрома, никаких фреймворков на фронте. |
| Одна тема | Единая тёмная тема без тумблера; light-палитра не поддерживается. |
| Модель видна | Pane/View/тиры — в шапках панелей; при двух панелях id и `data-pane`/`data-view` сохраняются (§5.2–5.3). |
| Видимость плагинов | **Цель H2**, а не текущее состояние: сейчас плагинные поверхности — заглушки (§3). |
| Честность отказов | Ошибки идут в статусбар/журнал-заглушку, окно не падает. |

---

## 1. Карта экрана

```
#app
├─ header#toolbar.toolbar
│    brand «d darmark»
│    file-actions: #btn-new · #btn-open · #btn-save · #btn-save-as   (icon-only)
│    .file-chip → #file-label
│    .spacer
│    #format-group: B · I · ` · H · link                             (работает)
│    .div-v · #palette-trigger «Команды · Ctrl+K»                    ← заглушка
│    .div-v · #toggle-sync (chk-sync) · #toggle-preview (chk-preview) (работают)
│    .div-v · #btn-inspect                                           (наш режим подсветки)
├─ .workspace
│    nav.rail
│      #rail-explorer (active) · #rail-plugins (disabled, «скоро»)
│      .spacer · #rail-logs («Журнал — скоро»)
│    aside#sidebar (сворачивается)
│      #panel-explorer: «Файлы» + #tree («скоро»)
│      #panel-plugins (hidden)                                       ← заглушка H2
│    main.main
│      .tabrow > #tab-strip → один таб #tab-current (#tab-name/#tab-dirty)  ← заглушка
│      #panes.panes
│        #pane-a.pane-editor   .pane-head > #view-switch-a «Редактор»
│                              .editor-wrap > #gutter + #editor
│        #divider.divider
│        #pane-b.pane-preview  .pane-head > #view-switch «Предпросмотр»
│                              #preview.markdown-body
└─ footer#statusbar.statusbar
     #stat-state «Готов» · #stat-msg · .spacer · #stat-block «—» ·
     #stat-pos · #stat-size · #stat-inspect (hidden)
```

Инфо-дровер idea5 и нижняя панель журнала/событий **в H1 не реализованы** (см. §3).

---

## 2. Что работает (H1)

- **Файлы:** Новый/Открыть/Сохранить/Сохранить как (нативные диалоги), dirty-маркер `●` в `#file-label`, подтверждение закрытия окна при несохранённых правках.
- **Редактор:** `#editor` — сам скролл-элемент, `white-space: pre`, `wrap="off"`; **gutter** с номерами строк, синхронный прокрутке (`gutter.ts`).
- **Форматирование** (`formatActions.ts`): жирный / курсив / код / заголовок / ссылка над выделением; обновление стора через событие `input`.
- **Предпросмотр:** рендер на Rust (`md-core`) + санитайзер; таблицы (сортировка/фильтр/поиск), локальные картинки, внешние ссылки.
- **Инспектор** (`#btn-inspect`, `inspector.ts`): двусторонняя подсветка блоков/ячеек превью ↔ исходник (это **не** информационный дровер idea5).
- **Синхронизация скролла** (`#toggle-sync`): анкорная (по `data-md`) и пропорциональная связка редактора и превью.
- **Оболочка** (`sidebar.ts`): rail переключает/сворачивает sidebar (Explorer по умолчанию); `#toggle-preview` скрывает вторичную панель.
- **Хоткеи:** `Ctrl+N/O/S`, `Ctrl+Shift+S`, `Ctrl+P`, `Ctrl+I` (инспектор), `Ctrl+B` (жирный), `Ctrl+K` (палитра-заглушка), `Esc` (выход из инспектора).

Модули фронта: `main.ts` (композиция), `sidebar.ts`, `gutter.ts`, `formatActions.ts`, `shell.ts`, `statusBar.ts`, `fileActions.ts`, `docStore.ts`, `editorView.ts`, `previewView.ts`, `viewRegistry.ts`, `layout.ts`, `paneHost.ts`, `linkController.ts`, `renderIndex.ts`, `tables.ts`, `inspector.ts`, `mapping.ts`, `scrollsync.ts`, `images.ts`, `ids.ts`, `tauri.ts`, `sampleDocument.ts`, `style.css`.

---

## 3. Заглушки и отложенное

| Поверхность | Сейчас | План |
|---|---|---|
| `#plugin-strip` (тулбар) | удалён | Не возвращать (`BUG-004`); ограниченный доступ — `IDEA-005` |
| Панель «Плагины» `#panel-plugins` | `hidden`, «скоро» | H2: карточки/права/управление |
| `#rail-logs` / журнал и события | «Журнал — скоро» | H2: нижняя панель |
| `#palette-trigger` | «Палитра — скоро» | H2: палитра команд |
| `#tab-strip` | один таб текущего файла | H2/H3: несколько документов |
| Инфо-дровер idea5 | не делаем | отложено |
| Resize-разделитель | отложен | отдельная задача |

---

## 4. Отличия от idea5 (осознанные)

1. Одна тёмная тема, без тумблера и light-палитры.
2. Файловые команды (new/open/save/save-as) — иконки в тулбаре (в idea5 их не было).
3. Тумблеры sync/preview — `label + chk-*` в тулбаре, а не `aria-pressed`-кнопки в tab-actions.
4. `#btn-inspect` — режим подсветки блоков/таблиц, а не информационный дровер idea5.
5. Gutter + `white-space: pre` (в idea5 — `pre-wrap`, без контракта на gutter).
6. Нет resize-разделителя и вкладок документов (один плейсхолдер-таб).
7. Хоткеи: `Ctrl+I` — инспектор, `Ctrl+B` — жирный, `Ctrl+K` — палитра-заглушка.
8. Dirty-маркер — `●` в `#file-label` (statusBar), отдельного dot нет.

Причина отличий: idea5 — визуальный референс, а UI-срез (`tasks/архив/TZ-UI-idea5.md`) сохранял замороженные контракты и текущий функционал, а не копировал прототип.

---

## 5. Соответствие замороженным E2E-контрактам (§13.2)

id: `app, toolbar, panes, statusbar, editor, preview, btn-new, btn-open, btn-save, btn-save-as,
file-label, toggle-preview, toggle-sync, btn-inspect, chk-preview, chk-sync, stat-msg, stat-pos,
stat-size, stat-inspect`.

Селекторы: `#preview .md-block[data-md]`, `.table-enhanced`, `.table-enhanced tbody tr`,
`.table-enhanced td[data-md]`, `.table-count`, `.table-scroll`, `.col-filter-btn`,
`.col-filter-menu`, `.col-filter-item`, `.inspect-active`, `.inspect-col`, `tr.inspect-row`.

Прочее: глобалы `window.__xss`, `window.__errors`/`__errorCapture`; стартовый текст
«Добро пожаловать в darmark»; заголовки демо-таблицы `Файл | Размер | Строк | Изменён`;
константы debounce 120 мс, `ECHO_MS = 100`, throttle инспектора 100 мс.

При двух панелях id остаются на первичной панели, добавляются `data-pane`/`data-view`.
Атрибуты `data-p-<pluginid>-*` и классы `p-<pluginid>-` — **H2** (`tasks/TZ-H2.md`).

---

## 6. Дальше

Идеи развития интерфейса и целевой UX H2 — в бэклоге `ideas/UI-IDEAS.md`. Нормативно по плагинной системе — `tasks/TZ-H2.md` (Фаза 5).

---

## 7. Файлы

| Файл | Роль |
|---|---|
| `ideas/front_idea5.html` | референс-прототип (single-file, офлайн) |
| `ideas/UI_CONCEPT.md` | этот документ: принципы, карта экрана, реалии H1 |
| `docs/DESIGN_DOC.md` | нормативная спецификация |
| `ideas/архив/front_idea3.html` | предыдущая итерация (история) |
| `ideas/архив/front_idea2.html` | ещё раньше (история) |
