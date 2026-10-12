<img src="crates/app/src-tauri/icons/icon.png" alt="darmark" width="88" align="right">

Markdown-редактор для Windows (10/11), который расширяет себя сам.
Слева исходник, справа HTML-предпросмотр.

Нужна новая функция? Встроенный AI напишет, протестирует и подключит плагин к редактору. Без пересборки, без перезапуска, без ручной возни.

![CI](https://github.com/Demonss11/darmark/actions/workflows/ci.yml/badge.svg)
![Release](https://github.com/Demonss11/darmark/actions/workflows/release.yml/badge.svg)
![Platform](https://img.shields.io/badge/platform-Windows%2010%2F11-0078D6)
![License](https://img.shields.io/badge/license-MIT-blue)

## Что внутри

- **Ядро на Rust** — рендеринг GFM, санитизация HTML, YAML-шапка; всё, что касается Markdown, живёт на бэкенде
- **Изолированные плагины** — Lua в отдельном процессе: песочница без `os`, `io`, `require`, лимиты памяти и времени через Job Object, карантин после трёх падений подряд
- **AI-ассистент** — чат с агентом, который п

## Как работает AI

```mermaid
sequenceDiagram
    actor User as Пользователь
    participant AI as AI-ассистент
    participant LLM as LLM
    participant CLI as Plugin CLI
    participant Host as Plugin Host

    User->>AI: «Сделай плагин для TODO-подсветки»
    AI->>LLM: промпт
    LLM-->>AI: Lua-код + манифест
    AI->>CLI: test
    CLI->>Host: запуск в песочнице
    Host-->>CLI: OK
    AI->>CLI: install
    CLI-->>AI: плагин активен
    AI-->>User: функция работает
```

## Установка

**Через установщик:** [релизы](https://github.com/Demonss11/darmark/releases)

- **Windows 11** — быстро, WebView2 уже предустановлен
- **Windows 10** — дольше: установщик подтянет WebView2

**Из исходников** (Rust ≥ 1.80, Node ≥ 20):

```bash
git clone https://github.com/Demonss11/darmark.git
cd darmark/crates/app
npm install
npx tauri dev     # dev-режим
npx tauri build   # NSIS-установщик в target/release/bundle/nsis/
```

## Документация

- [API плагинов](docs/PLUGIN_API.md)
- [Руководство по плагинам](docs/PLUGIN_GUIDE.md)
- [Архитектурные решения (ADR)](docs/adr/)
- [Дизайн-система](docs/DESIGN_DOC.md)

## Лицензия

MIT