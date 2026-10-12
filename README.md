<img src="crates/app/src-tauri/icons/icon.png" alt="darmark" width="88" align="right">

# darmark

**Markdown-редактор для Windows.** Слева исходник, справа HTML-предпросмотр — как в Notepad++, только с плагинами.

![CI](https://github.com/Demonss11/darmark/actions/workflows/ci.yml/badge.svg)
![Release](https://github.com/Demonss11/darmark/actions/workflows/release.yml/badge.svg)
![Platform](https://img.shields.io/badge/platform-Windows%2010%2F11-0078D6)
![License](https://img.shields.io/badge/license-MIT-blue)

Tauri 2, vanilla TypeScript и Rust-ядро Markdown. Одна тёмная тема, один акцент, установщик около 2 МБ.

## Возможности

**Редактор и предпросмотр**

- GFM-рендер на Rust (`pulldown-cmark`), YAML-шапка, санитизация HTML, картинки и ссылки.
- Номера строк, форматирование выделения, undo/redo целиком в Rust — стек переживает переключение вкладок.
- Таблицы «как в Excel»: сортировка колонок, фильтры, клавиатурная навигация.
- Инспектор структуры документа и синхронная прокрутка: один индекс рендера на ревизию, mapping байты↔UTF-16.
- Мультивкладки, сплит до двух панелей, палитра команд по `Ctrl+K`.

**Нижний док**

- **Журнал** — события документов, плагинов и MCP-вызовов.
- **Мониторинг** — потребление ресурсов приложением и плагинными процессами.
- **AI-ассистент** — чат с OpenAI-совместимым провайдером, стриминг ответа, скиллы «объясни документ» и «создай плагин». Правки приходят диффом с предпросмотром и откатом.

**Lua-плагины без пересборки**

Кладёшь `.lua` в `%APPDATA%\darmark\plugins\`, правишь в блокноте, жмёшь «Перезагрузить» — работает новая версия. Плагин живёт в отдельном процессе: его падение, зависание или утечка памяти не роняют редактор. Сбойный плагин — это состояние в панели «Плагины», а не упавшее окно.

- Песочница: без `os`, `io`, `require`, `load`; лимиты памяти и времени через Job Object; после трёх падений подряд — карантин.
- Разрешения по образцу deny-by-default: `document:read`, `document:write`, `view:create`, `view:modify`, `ui:statusbar`. Сети и файловой системы у плагина нет.
- Свои вкладки в предпросмотре, команды с хоткеями, слоты статусбара, постоянные настройки.

## Установка

Установщик собирается по тегу `v*` и попадает в [релизы](https://github.com/Demonss11/darmark/releases). WebView2 установщик подтянет сам.

Сборка из исходников (Rust ≥ 1.80, Node ≥ 20):

```bash
cd crates/app
npm install
npx tauri dev          # запуск в dev-режиме
npx tauri build        # NSIS-установщик в target/release/bundle/nsis/
```

## Первый плагин

```bash
cargo run -p plugin-cli -- init examples/plugins/hello   # заготовка plugin.json + main.lua
cargo run -p plugin-cli -- test examples/plugins/hello   # проверка в песочнице
```

```lua
function on_activate(ctx)
  host.show_message("Привет из плагина hello!")
end

function on_deactivate(ctx) end
```

Разрешения объявляет манифест, пользователь выдаёт подмножество. Полный разбор — [docs/PLUGIN_GUIDE.md](docs/PLUGIN_GUIDE.md), референс функций — [docs/PLUGIN_API.md](docs/PLUGIN_API.md).

## Горячие клавиши

| Клавиши | Действие | Клавиши | Действие |
| --- | --- | --- | --- |
| `Ctrl+N` / `Ctrl+O` | Новый / открыть | `Ctrl+K` | Палитра команд |
| `Ctrl+S` / `Ctrl+Shift+S` | Сохранить / сохранить как | `Ctrl+Shift+A` | AI-ассистент |
| `Ctrl+Z` / `Ctrl+Y` | Undo / redo | `Ctrl+I` | Инспектор |
| `Ctrl+B` | Жирный | `Ctrl+P` | Показать/скрыть предпросмотр |
| `Ctrl+R` | Перезагрузить плагин открытого `.lua` | `Ctrl+Shift+K` | Вставить ссылку |

Остальные команды (курсив, код, заголовок, синхронная прокрутка) — в палитре `Ctrl+K`.

## Устройство

| Крейт | Роль |
| --- | --- |
| `md-core` | Чистое ядро `markdown → HTML`: рендер, санитайзер, YAML-шапка, mapping. Без Tauri и GUI |
| `app/src` | Фронтенд: layout, панели, редактор, предпросмотр, палитра, док. Vanilla TS + Vite |
| `app/src-tauri` | Тонкий Tauri-шелл: `DocumentStore`, undo, IPC-команды, подсистема плагинов, AI, MCP |
| `plugin-core` | Плагинная механика без Lua: кадры, конверт, манифест, карантин |
| `plugin-host` | Child-процесс с Lua 5.5 — единственный крейт, линкующий `mlua` |
| `plugin-cli` | CLI `darmark-plugin`: `init`, `test`, `install`, `link` |
| `plugin-catalog` | Локальный каталог плагинов (`index.json`) |
| `mcp-core` | Ядро MCP-протокола: JSON-RPC, реестр tools/resources со схемами |

Границы жёсткие: рендер Markdown и санитизация — только в Rust, TS присваивает готовый HTML. Lua-рантайм не попадает в GUI-процесс — CI это проверяет. Размер под контролем: `darmark.exe` ≤ 6 МБ, `darmark-plugin-host.exe` ≤ 1.5 МБ, установщик ≤ 4 МБ.

## Разработка

```bash
cargo test -p md-core                                            # ядро, без GUI
cargo test -p plugin-core -p plugin-catalog -p plugin-host -p plugin-cli
cd crates/app && npm run build                                   # tsc strict + vite
npx tauri build --no-bundle && cd crates/app && npm run test:e2e # E2E, 21 сценарий
```

CI (windows-latest) гоняет `rustfmt`, `clippy -D warnings`, `cargo test`, сборку фронтенда, smoke-прогон плагина и size-gate.

> [!NOTE]
> Целевая платформа — Windows; Linux используется только для разработки. Документация, комментарии и UI-строки — на русском.

## Документация

| Что | Где |
| --- | --- |
| Нормативная спецификация архитектуры | [docs/DESIGN_DOC.md](docs/DESIGN_DOC.md) |
| Визуальная идентичность: токены, правила UI | [DESIGN.md](DESIGN.md) |
| Архитектурные решения | [docs/adr/](docs/adr/) |
| Плагины: API и гайд | [docs/PLUGIN_API.md](docs/PLUGIN_API.md), [docs/PLUGIN_GUIDE.md](docs/PLUGIN_GUIDE.md) |
| Правила работы с кодом | [AGENTS.md](AGENTS.md) |
| Бэклог идей | [docs/BACKLOG.md](docs/BACKLOG.md) |
| Замеры размеров и производительности | [docs/FINDINGS.md](docs/FINDINGS.md) |

Лицензия — MIT.
