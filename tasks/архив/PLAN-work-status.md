# План работ — статус по TZ-fixes (актуализировано 05.10.2026)

Обновлено: 2026-10-05. Основа: `tasks/TZ-fixes.md` v1.2, `reviews/TZ-fixes-review-v1.2.md`.
Предыдущая редакция плана была написана на `c4eb434`; с тех пор появились `bf7f7d6` («app»),
`dedfc21` («exit fix») и незакоммиченные правки 05.10.

## 1. Сделано

### 1.1 Замечания ревью v1.2

| # | Замечание | Статус |
|---|-----------|--------|
| 3.1 | Противоречие P2.3 (`union()` в `DEFAULT_OPTIONS`) | ✅ Устранено документационно: `union()` оставлен осознанно — в bitflags 2.13 `BitOr` не `const fn` |
| 3.2 | Статусы P0.1/P0.2 завышены (GUI не проверялся) | ✅ Закрыто: P0.1 отменён целиком, P0.2 частично автоматизирован (см. ниже) |

### 1.2 Код: дефект санитайзера (закоммичено)

`remove_event_handlers`/`sanitize_urls_in_tag` смешивали байтовые индексы и символьный
вывод → паника на кириллице в атрибуте. Заменены единой char-безопасной `rewrite_tag`
на `Vec<char>`. Побочно это закрыло и техдолг #6 (прежний `O(n²)` из `chars().nth(i)`).

### 1.3 Закрытие окна (05.10, незакоммичено)

Корневой баг: в `capabilities/default.json` не было `core:window:allow-destroy`, поэтому
JS-`destroy()` отклонялся ACL, а окно закрывал только аварийный Rust-поток `sleep(5)` →
`destroy` из `exit fix` (отсюда задержка 5 с и игнор отмены). Исправлено: добавлено
разрешение, убраны Rust-поток и fallback-таймеры, `onCloseRequested` опирается на штатный
`destroy()` (при `dirty` — подтверждение).

### 1.4 Техдолг #7: табличный XSS-harness (05.10, незакоммичено)

В `md-core` добавлены `table_no_event_handler_survives_any_context`,
`table_no_dangerous_uri_survives_any_context`, `table_benign_content_survives`.

### 1.4b Санитайзер v2 по белым спискам (05.10, незакоммичено)

Прежний санитайзер удалял только `on*` и `javascript:` — `<style>`, `<iframe>`, `<form>`,
`srcdoc` проходили. Переписан по идеям `ammonia` (без зависимости): белые списки тегов и
атрибутов, вырезание содержимого `script/style/iframe/form/svg/…`, схемы URL по белому списку,
декодирование сущностей и повторное экранирование, `rel="noopener noreferrer"` на `<a>`.
Итого 32 теста ядра. Полная строгость (настоящий парсер) — `ammonia`, +≈0,6 МБ.

### 1.5 `tauri-plugin-opener` (05.10, незакоммичено)

Пункт 2.1.3 закрыт: внешние ссылки открываются нативно через `tauri-plugin-opener`
(Rust-крат + npm-пакет), `main.ts` вызывает `openUrl`; permission `opener:default`.

### 1.6 Файловый доступ упрощён — модель Notepad++ (05.10, незакоммичено)

Отказались от «белых корней» (P0.1/P2.2): для локального редактора defense-in-depth не
оправдывает сложности. Теперь:

- `src-tauri/src/lib.rs` — три команды: `read_file` (лимит 10 МБ), `write_file`, `render_markdown`.
  Удалены `base_roots`/`SessionState`/`validate_path`/`is_allowed_ext`/`register_dir`/`resolve_start_dir`
  и команды `pick_open_file`/`pick_save_file`.
- Нативные диалоги открытия/сохранения — на фронтенде (`@tauri-apps/plugin-dialog`:
  `open`/`save`), фильтр `.md/.txt` в опциях; открытие начинается в папке текущего документа.
- Удалены тесты валидации путей; остались 3 теста IPC (чтение, запись, лимит).
- Удалены `e2e/features/readfile_boundary.feature` и стенд `tools/gen_readfile_probe.py`.

### 1.7 GUI E2E: Cucumber + WebdriverIO (05.10, незакоммичено)

BDD-стенд `crates/app/e2e/` поверх release-бинарника; драйвер `@wdio/tauri-service`
с внешним `tauri-driver`. Покрыто: оболочка, предпросмотр (кириллица, неисполнение
`onerror`, нейтрализация `javascript:`), таблицы (сортировка, фильтр + мини-поиск P1.4).
Нативные диалоги (лимит 10 МБ, Save As, внешняя ссылка, закрытие окна) — вручную, `@manual`.

### 1.8 Прогнанные проверки (05.10)

| Проверка | Результат |
|----------|-----------|
| `cargo test --workspace` | ✅ 35 тестов (32 md-core + 3 mdedit_lib) |
| `cargo clippy --workspace --all-targets -- -D warnings` | ✅ чисто |
| `cargo fmt --all -- --check` | ✅ чисто |
| `cargo check -p mdedit` | ✅ чисто |
| `npm run build` (`tsc && vite build`) | ✅ успешно |
| `npx tauri build --no-bundle` | ✅ пересобран `target/release/mdedit.exe` |
| `npm run test:e2e` (Cucumber + WebdriverIO) | ✅ 17 шагов зелёные (3 feature) |
| Нативные GUI-пункты (лимит/Save As/ссылка/закрытие) | ✅ вручную (WebDriver не автоматизирует) |

### 1.9 Незакоммиченное (`git status`)

- `M crates/app/src-tauri/src/lib.rs` — упрощён до read/write/render; закрытие окна; opener
- `M crates/app/src-tauri/capabilities/default.json` — `core:window:allow-destroy`, `opener:default`
- `M crates/app/src-tauri/Cargo.toml` — `tauri-plugin-opener`
- `M crates/app/src/main.ts` — диалоги через `plugin-dialog`, `openUrl`, упрощён `onCloseRequested`
- `M crates/app/src/tauri.ts` — убраны `pick_open_file`/`pick_save_file`
- `M crates/app/package.json` + `package-lock.json` — opener, WDIO/Cucumber devDeps, `test:e2e`
- `M crates/md-core/src/lib.rs` — табличный harness (3 теста)
- `M README.md`, `M AGENTS.md`, `M tasks/TZ-fixes.md`, `M tasks/PLAN-work-status.md` — документация
- `A crates/app/e2e/**` — Cucumber-фичи, шаги, `wdio.conf.js`
- `M crates/app/dist/*`, `M crates/app/src-tauri/gen/schemas/*` — артефакты сборки

> `tauri-driver` ставится вручную (`cargo install tauri-driver --locked`) — в репозиторий не входит.

## 2. Надо сделать

### 2.1 Обязательное (блокирует закрытие TZ-fixes)

1. **Ручной GUI-чек-лист** (WebDriver не автоматизирует):
   - [x] файл > 10 МБ → понятная ошибка, приложение не виснет;
   - [x] «Сохранить как» в произвольный каталог → сохраняется;
   - [x] внешняя ссылка в предпросмотре → открытие системным браузером (opener);
   - [x] закрытие окна: без правок — сразу, с правками — диалог, «Отмена» не закрывает;
2. **Коммит и push** — по явной команде пользователя

### 2.2 Технический долг (не блокирует)

3. **Санитайзер строковый, без настоящего HTML-парсера.** Усилен до v2 по белым спискам
   (теги/атрибуты/схемы URL, drop-content, декодирование сущностей) — см. журнал TZ-fixes.
   Остаточный риск: незакрытый/вложенный/экзотический HTML, CDATA. Полная строгость — переход
   на `ammonia` (Rust, +≈0,6 МБ к exe; замерено). Решение — отдельное ТЗ.
4. ~~`O(n²)` в `rewrite_tag`~~ — ✅ **закрыто** переписью на `Vec<char>`.
5. ~~Нет полного покрытия `preserves_*`/`on*`~~ — ✅ **закрыто** табличным harness.
   При добавлении правил расширять таблицы `*_VECTORS`, а не только точечные тесты.

## 3. Краткий чек-лист следующего шага

- [x] Пройти ручные GUI-пункты п. 2.1 (лимит, Save As, внешняя ссылка, закрытие окна)
- [x] Коммит (по запросу)
- [x] GUI-чек-лист автоматизирован через Cucumber E2E (`npm run test:e2e`)
- [x] `tauri-plugin-opener` вместо `window.open`
- [x] Файловый доступ упрощён до модели Notepad++ (сняты P0.1/P2.2)
- [x] XSS-harness (техдолг #7)
