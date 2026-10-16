<picture>
  <source media="(prefers-color-scheme: dark)" srcset="crates/app/public/penal_text.svg">
  <img src="crates/app/public/penal_text.svg" alt="ПЕНАЛ" width="360" align="right">
</picture>

Markdown-редактор для Windows (10/11) с расширяемостью на лету: встроенный AI напишет, протестирует и подключит плагин без пересборки и перезапуска.

![CI](https://github.com/Demonss11/darmark/actions/workflows/ci.yml/badge.svg)
![Platform](https://img.shields.io/badge/platform-Windows%2010%2F11-0078D6)
![License](https://img.shields.io/badge/license-MIT-blue)

## Как работает AI

```mermaid
sequenceDiagram
    actor User as Пользователь
    participant AI as AI-ассистент
    participant Host as Редактор

    User->>AI: «Сделай плагин для TODO-подсветки»
    AI->>Host: генерация + тест + установка плагина
    Host-->>AI: плагин активен
    AI-->>User: функция работает
```

## Установка

**Через установщик:** [релизы](https://github.com/Demonss11/darmark/releases)
Зависимости: если не стоит WebView2, он автоматически скачается и установится.

**Из исходников** (Rust 1.99, Node 24):

```bash
git clone https://github.com/Demonss11/darmark.git
cd darmark/crates/app
npm install
npx tauri dev     # dev-режим
npx tauri build   # NSIS-установщик
```

## Лицензия

MIT