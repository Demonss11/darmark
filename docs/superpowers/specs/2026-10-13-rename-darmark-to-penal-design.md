# Спецификация: переименование darmark → ПЕНАЛ (PENAL)

- **Дата:** 2026-10-13
- **Статус:** утверждена к реализации
- **ADR:** ADR-0038
- **Затрагивает:** весь workspace, идентичность крейтов, пути данных, env-контракт, MCP-схему, UI и документацию

## 1. Контекст

Проект стартовал под рабочим именем `darmark`. Продуктовое название — **ПЕНАЛ**
(«Notepad++ для Markdown»), логотип — петля пера с чернильницей. Переименование
начато, но не закончено: `tauri.conf.json` называет продукт `darmark`, тогда как
заголовок окна — «ПЕНАЛ — Markdown editor», а `DESIGN.md` и вордмарк
`penal_text.svg` используют ПЕНАЛ.

Итог сегодня: два разных имени в одном проекте, причём видимое пользователю имя
(`productName`) — старое.

## 2. Решения

| # | Решение | Обоснование |
|---|---------|-------------|
| Р1 | Полное переименование: крейты, бинарники, `identifier`, каталог данных, env, MCP-схема, доки, репозиторий | Частичное переименование оставляет две сущности навсегда |
| Р2 | `productName = "PENAL"` латиницей | NSIS с не-ASCII `productName` ведёт себя по-разному в локалях Windows; не-ASCII в имени `.exe` бьёт по путям и spawn. Кириллица остаётся там, где её видит человек: заголовок окна, About, вордмарк |
| Р3 | Чистый разрыв: **ни** миграционного кода, **ни** deprecated-алиасов `DARMARK_*` | Приложение не установлено ни у кого, релизов не было, тегов нет. Обратная совместимость не нужна и была бы постоянным долгом |
| Р4 | Переименование данных — разовое действие вручную, без кода в приложении | Следствие Р3 |
| Р5 | Прошлые ADR не переписываются; добавляется ADR-0038 + пометки «действует ADR-0038» | ADR — историческая запись решений. Переписывание обесценивает привязку к коммитам того времени |
| Р6 | Абсолютные пути в тестах `mcp/adapters.rs` чинятся в этом же проходе | Не связаны с переименованием, но лежат в тех же файлах; оставить — значит оставить тесты, работающие только на машине автора |

## 3. Границы

**Не меняется** (сознательно):

- `HOST_API_VERSION = 1` и политика версий API плагинов (ADR-0027) — целое число, имя в нём не участвует.
- Поле `entry` в `plugin.json` — это `main.lua`, а не путь к хосту. Манифесты плагинов не ломаются.
- `permission` в `plugin.json`, схемы кадров/конверта, `plugin-core` без `mlua`.
- Логотипы (`logo.svg`, `logo_w.svg`, `penal_text.svg`) и иконки — уже ПЕНАЛ.
- Плоский стиль знака, палитра интерфейса.

**Затрагивается вопреки ожиданиям:** схема MCP-ресурсов `darmark://` — это публичная
поверхность, её видят MCP-клиенты. Разрыв осознанный (Р3), фиксируется в ADR-0038.

## 4. Карта соответствий

### 4.1 Крейты и бинарники

| Было | Стало |
|---|---|
| крейт `darmark` (`crates/app/src-tauri/Cargo.toml`) | `penal` |
| либа `darmark_lib` | `penal_lib` |
| `darmark.exe` | `penal.exe` |
| `darmark-plugin-host.exe` | `penal-plugin-host.exe` |
| `darmark-plugin.exe` | `penal-plugin.exe` |
| `CHILD_EXE_NAME` (`plugins/supervisor.rs:27`) | `"penal-plugin-host.exe"` |
| `CLI_EXE` (`ai/plugin_harness.rs:38`) | `"penal-plugin.exe"` |
| `HOST_EXE` (`ai/plugin_harness.rs:41`) | `"penal-plugin-host.exe"` |

### 4.2 Конфиг Tauri (`crates/app/src-tauri/tauri.conf.json`)

| Ключ | Было | Стало |
|---|---|---|
| `productName` | `darmark` | `PENAL` |
| `identifier` | `dev.darmark.app` | `dev.penal.app` |
| `bundle.externalBin` | `binaries/darmark-plugin-host`, `binaries/darmark-plugin` | `binaries/penal-plugin-host`, `binaries/penal-plugin` |
| `app.windows[0].title` | `ПЕНАЛ — Markdown editor` | без изменений |

### 4.3 Пути данных

Каталог данных **захардкожен литералом в четырёх местах** и не выводится из
`identifier`. Смена `identifier` его не переименовывает — это отдельная работа.

| Место | Было | Стало |
|---|---|---|
| `src/lib.rs:869` | `base.join("darmark").join("plugins")` | `base.join("penal").join("plugins")` |
| `plugins/settings.rs:75` | `base.join("darmark").join("config.json")` | `base.join("penal").join("config.json")` |
| `plugins/plugin_settings.rs:44` | `base.join("darmark").join("plugin-settings")` | `base.join("penal").join("plugin-settings")` |
| `plugin-cli/src/main.rs:513` | `base.join("darmark").join("plugins")` | `base.join("penal").join("plugins")` |

На машине разработчика до перехода существует `%APPDATA%\darmark\` (`config.json`
с ключом AI и разрешениями плагинов, 4 установленных плагина) и
`%LOCALAPPDATA%\dev.darmark.app\` (WebView2, localStorage). Обе папки
переименовываются вручную один раз:

```
%APPDATA%\darmark            → %APPDATA%\penal
%LOCALAPPDATA%\dev.darmark.app → %LOCALAPPDATA%\dev.penal.app
```

В `penal.theme` переносится только выбор темы. Потеря этого значения означает
лишний клик пользователя и ничего больше.

### 4.4 Env-переменные

Алиасы не оставляются (Р3). Тесты и E2E обновляются в том же изменении.

**Продакшн-код — 6 переменных:**

| Было | Стало | Где используется |
|---|---|---|
| `DARMARK_CONFIG_PATH` | `PENAL_CONFIG_PATH` | `plugins/settings.rs:68,71`, `ai/settings.rs:23` |
| `DARMARK_PLUGINS_DIR` | `PENAL_PLUGINS_DIR` | `src/lib.rs:859,863`, `ai/plugin_harness.rs:107,111`, `plugin-catalog/src/lib.rs`, `plugin-cli/src/main.rs` |
| `DARMARK_PLUGIN_SETTINGS_DIR` | `PENAL_PLUGIN_SETTINGS_DIR` | `plugins/plugin_settings.rs:37,40` |
| `DARMARK_PLUGIN_HOST_EXE` | `PENAL_PLUGIN_HOST_EXE` | `plugins/supervisor.rs:351,354`, `ai/plugin_harness.rs:40`, `plugins/tests.rs:75` |
| `DARMARK_PLUGIN_CLI` | `PENAL_PLUGIN_CLI` | `ai/plugin_harness.rs:37` |
| `DARMARK_PLUGIN_REPO` | `PENAL_PLUGIN_REPO` | `mcp/mod.rs:49,54`, `plugin-catalog/src/lib.rs`, `plugin-cli/src/main.rs`, `tests/mcp_contract.rs` |

**E2E — 2 переменные:**

| Было | Стало | Где используется |
|---|---|---|
| `DARMARK_APP_BINARY` | `PENAL_APP_BINARY` | `e2e/wdio.conf.js:6,20` |
| `DARMARK_E2E_TIMING` | `PENAL_E2E_TIMING` | `e2e/steps/timing.js:4,12`, `e2e/timing-report.mjs:5,6` |

`DARMARK_CONFIG_PATH` и `DARMARK_PLUGINS_DIR` дополнительно встречаются в
`e2e/features/ai.feature` и `e2e/steps/ai.steps.js` — они попадают в тот же
коммит 3.

Плюс переименование temp-префиксов: `darmark-settings-*` → `penal-settings-*`,
`darmark-ai-settings-*` → `penal-ai-settings-*`, `darmark-ai-missing-*` →
`penal-ai-missing-*`, `darmark-ai-validate-*` → `penal-ai-validate-*`.

### 4.5 MCP

| Было | Стало | Файл |
|---|---|---|
| `darmark://contracts/…` | `penal://contracts/…` | `mcp/resources.rs:19,26,33,40` |
| `darmark://plugins/…` | `penal://plugins/…` | `mcp/resources.rs:48,55,153,161` |

### 4.6 Фронт

| Было | Стало | Файл |
|---|---|---|
| `THEME_STORAGE_KEY = "darmark.theme"` | `"penal.theme"` | `src/main.ts:608` |
| `localStorage.getItem("darmark.theme")` | `"penal.theme"` | `public/theme-init.js:14` |
| подсказки `%APPDATA%/darmark/config.json` | `%APPDATA%/penal/config.json` | `index.html:444,492`, `src/pluginManager.ts:672` |

### 4.7 Прочее

| Что | Стало |
|---|---|
| `crates/app/package.json` → `name` | `penal` |
| stderr-префикс `darmark: …` (`src/lib.rs`) | `penal: …` |
| GitHub-репозиторий | `Demonss11/darmark` → `Demonss11/penal` |
| README: бейджи CI/Release + `releases` + `git clone` (4 URL) | новые URL |

## 5. Разбивка на коммиты

Каждый коммит собирается и не роняет CI. Крейт `darmark` и CI ссылаются друг на
друга, поэтому идентичность меняется атомарно.

| # | Содержимое | Почему отдельно |
|---|-----------|-----------------|
| 1 | ADR-0038 + пометки «действует ADR-0038» в прошлых ADR | Docs-only, ничего не ломает; фиксирует решение до кода |
| 2 | Идентичность: `Cargo.toml` крейтов, `tauri.conf.json`, `supervisor.rs`, `plugin_harness.rs`, `prepare-sidecars.mjs` (`sidecars` на строках 55–56), `wdio.conf.js`, `ci.yml`, `release.yml`, doc-комментарии крейтов, **таблица команд в `AGENTS.md`** | Атомарно: `externalBin` связан с именами бинарников, CI — с именем крейта. Таблица команд `AGENTS.md` (`-p darmark`, строки 28, 32) едет сюда же: агенты читают этот файл и выполняют команды из него — оставить `-p darmark` после переименования крейта значит отдать им нерабочую команду |
| 3 | Данные и env: §4.3, §4.4, §4.5, §4.6, stderr-префикс, `package.json` | Логически отдельный слой: «где лежат данные», а не «как называется продукт» |
| 4 | Фикс абсолютных путей в `mcp/adapters.rs` → temp-директория (Р6) | Не связан с переименованием. Вынесен, чтобы ревьюился как самостоятельный багфикс и не маскировался внутри массовой замены. **Исключается из замены коммита 3** |
| 5 | Доки и пользовательский текст: `AGENTS.md` (кроме таблицы команд — она в коммите 2), `DESIGN.md`, `DESIGN_DOC.md`, `PLUGIN_API.md`, `PLUGIN_GUIDE.md`, `BACKLOG`, `FINDINGS`, `README`, `docs/proto/`, `tasks/` | Массовая замена в прозе ревьюится отдельно от кода |

## 6. Проверка

| Проверка | Команда |
|---|---|
| Ядро | `cargo test -p md-core -p mcp-core -p plugin-catalog -p penal` |
| Формат | `cargo fmt -p md-core -p mcp-core -p plugin-catalog -p penal -- --check` |
| Clippy | `cargo clippy -p md-core -p mcp-core -p plugin-catalog -p penal --all-targets -- -D warnings` |
| Фронт | `cd crates/app; npm run build` (tsc strict) |
| Guard `mlua` | `cargo tree -p penal \| Select-String mlua` (пусто) |
| **Остаточный grep** | `rg -i darmark -g '!target' -g '!node_modules'` — допустимы **только**: ADR-0001…0037 (историческая запись), ADR-0038 (таблица соответствий) |
| E2E | `npx tauri build --no-bundle`, затем `cd crates/app; npm run test:e2e` |

Остаточный grep — главный гейт: он доказывает, что переименование полное, а не
выборочное. Без него легко пропустить один env или одну строку в UI.

## 7. Риски и как они снимаются

| Риск | Как снимается |
|---|---|
| Пропущено имя, приложение падает в рантайме, а не в сборке | Остаточный grep (§6) + полный `cargo test` + E2E |
| Протухший бинарник подхватывается как child-хост | `cargo clean` + удаление старых сайдкаров из `crates/app/src-tauri/binaries/`, затем `npm run sidecars:dev` (§8, шаги 2–3) |
| Кириллица в имени бинарника ломает NSIS/spawn | Р2: `productName` латиницей |
| Плагины «исчезнут» после смены каталога данных | Р4: ручное переименование `%APPDATA%\darmark` до первого запуска новой сборки; проверяется вручную |
| Массовая замена заделает исторические ADR | Р5: ADR-0001…0037 исключены из замены, только пометки |
| Тесты `mcp/adapters.rs` зелёные только на машине автора | Р6: коммит 4 |

## 8. Порядок действий пользователя

1. Выполнить все пять коммитов.
2. Удалить протухшие сайдкары из `crates/app/src-tauri/binaries/`:
   `darmark-plugin-host-x86_64-pc-windows-msvc.exe`,
   `darmark-plugin-x86_64-pc-windows-msvc.exe`.
   **`cargo clean` их не трогает** — каталог вне `target/`, а `prepare-sidecars.mjs`
   только копирует файлы под новыми именами и старые не удаляет. Если оставить оба
   набора, `tauri-build` может подхватить старый бинарник по совпадению префикса.
3. `npm run sidecars:dev` — застейджить сайдкары под новыми именами
   (предусловие `tauri-build`: без них крейт `penal` не компилируется).
4. Переименовать `%APPDATA%\darmark` → `%APPDATA%\penal` **до** первого запуска
   новой сборки, иначе создастся пустой каталог и настройки будут потеряны.
5. Переименовать `%LOCALAPPDATA%\dev.darmark.app` → `%LOCALAPPDATA%\dev.penal.app`
   (необязательно — сохраняет только выбор темы).
6. `npx tauri dev`, затем `npx tauri build --no-bundle` и `npm run test:e2e`.
7. Переименовать репозиторий на GitHub: `Demonss11/darmark` → `Demonss11/penal`.
   Новые URL уже проставлены в README коммитом 5; GitHub сохраняет редирект
   со старого имени.
