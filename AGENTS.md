# AGENTS.md

`darmark` (продуктовое название `ПЕНАЛ`) — Markdown-редактор/вьюер в духе Notepad++. Tauri 2 + vanilla TS/Vite + Rust-ядро. Платформа — Windows. Документация, комментарии и UI-строки — **на русском**.

## Структура и границы

- `crates/md-core/` — чистое ядро markdown→HTML (без Tauri/GUI).
- `crates/app/src/` — фронтенд (vanilla TS).
- `crates/app/src-tauri/` — тонкий Tauri-шелл (`darmark`/`darmark_lib`).
- `crates/plugin-core/` — ядро плагинной системы (без `mlua`/Tauri).
- `crates/plugin-host/` — child `darmark-plugin-host` (Lua 5.5, `mlua`).
- `crates/plugin-cli/` — CLI `darmark-plugin`.
- `crates/plugin-catalog/` — каталог плагинов (без `mlua`/Tauri).

## Платформа

Продукт — **только Windows**. Не-Windows ветки не поддерживаются и не тестируются: CI собирает только `windows-latest`.

Атрибуты `#[cfg(windows)]` / `#[cfg(not(windows))]` в коде не ставятся. Windows-специфичный код (Job Object, WinAPI, SChannel, WebView2) пишется безусловно. Гейт над модулем не делает крейт кроссплатформенным — он лишь создаёт иллюзию: потребители Windows-модулей используются без гейта, поэтому крейт на не-Windows и так не собирается. Зависимости Windows при этом остаются в `[target.'cfg(windows)'.dependencies]` — это декларация платформы, а не мёртвый код.

## Команды

Из корня, если не указано иное:

| Задача | Команда |
| --- | --- |
| Формат Rust | `cargo fmt --all -- --check` |
| Линт Rust | `cargo clippy -p md-core -p mcp-core -p plugin-catalog -p plugin-core -p plugin-host -p plugin-cli -p darmark --all-targets -- -D warnings` |
| Линт фронта | `cd crates/app; npm run lint` |
| Мёртвый код фронта | `cd crates/app; npm run knip` |
| Аудит зависимостей | `cargo audit` (RUSTSEC; джоб `audit` в CI) |
| Неиспользуемые зависимости | `cargo machete` |
| Тесты ядра | `cargo test -p md-core` |
| Тесты плагинных крейтов | `cargo test -p plugin-core -p plugin-catalog -p plugin-host -p plugin-cli` |
| Шелл: сборка/тесты | `cargo check -p darmark` / `cargo test -p darmark` |
| Фронт (tsc strict) | `cd crates/app; npm run build` |
| GUI | `cd crates/app; npx tauri dev` |
| Релиз NSIS | `cd crates/app; npx tauri build` (size-gate ≤6 МБ) |
| Чистка Tauri-артефактов | `cd crates/app; npm run clean:tauri` (удаляет `src-tauri/gen`, `src-tauri/binaries`, `dist`; `target/` не трогает) |
| E2E | `cd crates/app; npm run test:e2e` |
| Guard: `mlua` вне GUI | `cargo tree -p darmark \| Select-String mlua` (пусто; ADR-0021) |

Предусловие: сайдкары в `crates/app/src-tauri/binaries/` (иначе `tauri-build` падает). `npm run clean:tauri` их удаляет — после него `cargo check -p darmark` требует `npm run sidecars`.

### Сборка приложения

```powershell
# перед сборкой — снять процессы E2E, они держат exe
taskkill /IM darmark.exe /F /T; taskkill /IM darmark-plugin-host.exe /F /T; taskkill /IM tauri-driver.exe /F /T; taskkill /IM msedgedriver.exe /F /T
Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -like "*wdio*" } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force }

# релиз + NSIS
cd crates/app; npx tauri build

# бинарник для E2E — в этом порядке, все три шага обязательны
cd crates/app; npm run build
npm run sidecars
cargo build -p darmark --release
```

Установщик: `target/release/bundle/nsis/`, size-gate ≤6 МБ.
Отмена `npm run test:e2e` оставляет живое дерево процессов — снимать `taskkill` выше.

## Жёсткие ограничения

- `innerHTML` — только через санитизированные хелперы (ADR-0038, DESIGN_DOC §11.1). Четвёртая точка запрещена.
- Юнит-тестов фронта нет (`vitest` нет) — проверка = `npm run build`.
- E2E не покрывает нативные диалоги/ОС — помечай сценарий `@manual`.
- E2E-сценарий > 5 с — подозрительно: либо сценарий тяжёлый, есть проблема.
- Тяжёлые зависимости — только по согласованию.

## Конвенции

- Комментарии на русском, объясняют **зачем**.
- Не коммить/пушить без запроса. Ветку коммитит исполнитель; в `main` не мержи сам.

## Ссылки

| Документ |
| --- |
| `DESIGN.md` — визуальная идентичность |
| `docs/DESIGN_DOC.md` — архитектура, контракты, ADR |
| `docs/adr/` — решения |
| `docs/BACKLOG.md`, `docs/FINDINGS.md` — бэклог, замеры |
| `docs/PLUGIN_API.md`, `docs/PLUGIN_GUIDE.md` — API плагинов |
