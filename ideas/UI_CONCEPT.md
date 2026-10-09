# UI_CONCEPT — интерфейс darmark (актуально на H2)

> **Статус:** non-normative, описание **реализованного** интерфейса H2.
> **Нормативная спецификация:** `docs/DESIGN_DOC.md` (§5 — layout/pane/view, §13.2 — замороженные e2e-контракты).
> **Плагинный UX (H2):** `tasks/TZ-UX-SPEC-CLEANUP.md` §11.
> **Референс-прототип:** `ideas/front_idea7.html` (вдохновение, не контракт).
> **Бэклог идей развития UI:** `ideas/UI-IDEAS.md`.
> Источник истины об интерфейсе — **код** `crates/app` (`index.html`, `src/*`, `src/style.css`).

---

## 0. Принципы

| Принцип | Что это значит |
|---|---|
| Простота | «Notepad++ для Markdown»: слева редактор, справа превью; минимум хрома, никаких фреймворков на фронте. |
| Одна тема | Единая тёмная тема без тумблера; light-палитра не поддерживается. |
| Модель видна | Pane/View/тиры — в шапках панелей; при двух панелях id и `data-pane`/`data-view` сохраняются (§5.2–5.3). |
| Видимость плагинов | Плагинные поверхности реализованы (H2): панель плагинов, палитра, статусбар, тир-1 view. |
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
│    .div-v · #palette-trigger «Команды · Ctrl+K»                    (работает)
│    .div-v · #toggle-sync (chk-sync) · #toggle-preview (chk-preview) (работают)
│    .div-v · #btn-inspect                                           (режим подсветки)
├─ .workspace
│    nav.rail
│      #rail-plugins (active) · .spacer · #rail-logs («Журнал — скоро»)
│    aside#sidebar (сворачивается)
│      #panel-plugins: панель плагинов (H2)                          (работает)
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

---

## 2. Что работает (H2)

- **Файлы:** Новый/Открыть/Сохранить/Сохранить как (нативные диалоги), dirty-маркер `●` в `#file-label`, подтверждение закрытия окна при несохранённых правках.
- **Редактор:** `#editor` — сам скролл-элемент, `white-space: pre`, `wrap="off"`; **gutter** с номерами строк, синхронный прокрутке (`gutter.ts`).
- **Форматирование** (`formatActions.ts`): жирный / курсив / код / заголовок / ссылка над выделением; обновление стора через событие `input`.
- **Предпросмотр:** рендер на Rust (`md-core`) + санитайзер; таблицы (сортировка/фильтр/поиск), локальные картинки, внешние ссылки.
- **Инспектор** (`#btn-inspect`, `inspector.ts`): двусторонняя подсветка блоков/ячеек превью ↔ исходник; закрепление выделения кликом.
- **Синхронизация скролла** (`#toggle-sync`): анкорная (по `data-md`) и пропорциональная связка редактора и превью.
- **Оболочка** (`sidebar.ts`): rail переключает/сворачивает sidebar (Explorer по умолчанию); `#toggle-preview` скрывает вторичную панель.
- **Палитра команд** (`palette.ts`, `Ctrl+K`): core + плагинные команды, навигация ↑/↓/Home/End, `aria-activedescendant`.
- **Плагины** (H2): панель плагинов (`pluginManager.ts`) с карточками, аккордеоном, согласием на права; per-plugin статусбар (`pluginStatusBar.ts`); тир-1 view (`pluginViews.ts`); карантин и перезагрузка.
- **Диалоги** (`dialog.ts`): модальная инфраструктура с focus-trap, Escape, возвратом фокуса.
- **Хоткеи:** `Ctrl+N/O/S`, `Ctrl+Shift+S`, `Ctrl+P`, `Ctrl+I` (инспектор), `Ctrl+B` (жирный), `Ctrl+K` (палитра), `Ctrl+R` (перезагрузка плагинов), `Esc` (выход из инспектора/палитры).

Модули фронта: `main.ts` (композиция), `sidebar.ts`, `gutter.ts`, `formatActions.ts`, `shell.ts`, `statusBar.ts`, `fileActions.ts`, `docStore.ts`, `editorView.ts`, `previewView.ts`, `viewRegistry.ts`, `layout.ts`, `paneHost.ts`, `linkController.ts`, `renderIndex.ts`, `tables.ts`, `inspector.ts`, `mapping.ts`, `scrollsync.ts`, `images.ts`, `ids.ts`, `tauri.ts`, `sampleDocument.ts`, `palette.ts`, `dialog.ts`, `toast.ts`, `pluginManager.ts`, `pluginViews.ts`, `pluginStatusBar.ts`, `pluginColor.ts`, `style.css`.

---

## 3. Заглушки и отложенное

| Поверхность | Сейчас | План |
|---|---|---|
| `#rail-logs` / журнал и события | «Журнал — скоро» | H3: нижняя панель |
| `#tab-strip` | один таб текущего файла | H3: несколько документов |
| Инфо-дровер | не делаем | отложено |
| Resize-разделитель | отложен | отдельная задача |

---

## 4. Замороженные e2e-контракты

Полный список замороженных контрактов — `tasks/TZ-UX-SPEC-CLEANUP.md §1`.

Ключевое: id (`app`, `toolbar`, `panes`, `statusbar`, `editor`, `preview`, `btn-*`, `toggle-*`, `chk-*`, `stat-*`), селекторы (`#preview .md-block[data-md]`, `.table-enhanced*`, `.inspect-*`, `.pl-item[data-plugin]`, `.pl-badge`, `input.pl-enabled`, `button.pl-reload`), инварианты (`#editor`/`#preview` — скролл-контейнеры; переключение view — `hidden`-toggle).

---

## 5. Дальше

Идеи развития интерфейса — в бэклоге `ideas/UI-IDEAS.md`. Нормативно по плагинной системе — `tasks/TZ-UX-SPEC-CLEANUP.md` §11.

---

## 6. Файлы

| Файл | Роль |
|---|---|
| `ideas/front_idea7.html` | референс-прототип (single-file, офлайн) |
| `ideas/UI_CONCEPT.md` | этот документ: принципы, карта экрана, реалии H2 |
| `ideas/UI-IDEAS.md` | бэклог идей развития UI |
| `ideas/UI-BUGS.md` | баг-бэклог интерфейса |
| `docs/DESIGN_DOC.md` | нормативная спецификация |
| `tasks/TZ-UX-SPEC-CLEANUP.md` | замена TZ-H2/UX-spec: контракты, открытые пункты, план |
