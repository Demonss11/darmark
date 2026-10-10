---
name: release-checklist
description: >
  Пошаговая релизная процедура для darmark: что прогнать перед сборкой, как
  проверить бюджет размера установщика, как сверить релизный профиль Cargo.toml
  (LTO/strip/panic=abort) и где лежат артефакты. Используй перед релизом и при
  расследовании превышения бюджета размера (size-gate).
---

# Релизный чек-лист

Используй перед релизом, после правок Cargo.toml или зависимостей, а также при
превышении бюджета размера. Цель — локально прогнать тот же цикл проверок, что
и CI, и не пропустить релиз с сломанным билдом или раздутым exe.

## Пошаговая процедура

1. **Локальный прогон проверок (до сборки)**

   Гоняем всё, что гоняет CI, чтобы не тратить цикл CI на локальные ошибки:

   - `cargo fmt --all -- --check` — форматирование по rustfmt.
   - `cargo clippy --workspace -- -D warnings` — линтер без предупреждений.
   - `cargo test -p md-core` — тесты чистого ядра markdown→HTML.
   - `cargo test -p darmark` — тесты Tauri-шелла. **Перед этим застейджить сайдкары**
     (`cd crates/app; npm run sidecars:dev`) — build.rs (`tauri-build`) копирует
     `bundle.externalBin` рядом с бинарником и падает без файлов `binaries/*-<triple>.exe`.
   - `cd crates/app; npm run build` — сборка фронтенда (tsc + vite); tsc валит
     сборку на любой ошибке типов.
   - Дополнительно после правок Rust: `cargo test -p plugin-proto -p plugin-host -p plugin-cli`.

2. **Сверка релизного профиля**

   Открой корневой `Cargo.toml` и убедись, что в релизном профиле включены:

   - `lto = true` — link-time optimization для уменьшения размера.
   - `strip = true` (или `strip = "symbols"`) — вырезание отладочной информации.
   - `panic = "abort"` — обязательный инвариант ради размера.

   Если любой из флагов отключён — собирать нельзя, сначала верни профиль.

3. **Проверка зависимостей**

   Убедись, что в `Cargo.toml` не появились новые тяжёлые зависимости без
   согласования (инвариант проекта). Проверь guard-инвариант: `cargo tree -p darmark`
   не должен содержать `mlua`.

4. **Сборка релиза**

   - `npx tauri build` — релизный NSIS-установщик в
     `target/release/bundle/nsis/`.
   - Сборка идёт в релизном профиле (`opt-level="s"`, LTO, strip, panic=abort),
     поэтому может занять заметное время — это нормально.
   - **Сайдкары плагинной подсистемы.** `beforeBuildCommand` запускает
     `npm run sidecars` (`crates/app/scripts/prepare-sidecars.mjs`): собирает
     `plugin-host`/`plugin-cli` в release и раскладывает
     `src-tauri/binaries/<name>-<target-triple>.exe` (их подхватывает
     `bundle.externalBin`). В установщик рядом с `darmark.exe` попадают
     `darmark-plugin-host.exe` (child — без него плагины в релизе не работают) и
     `darmark-plugin.exe` (CLI разработчика плагинов). Стейджить вручную:
     `cd crates/app; node scripts/prepare-sidecars.mjs`.
   - **CI-релиз:** пуш тега `v*` запускает `.github/workflows/release.yml` — тот же
     `npx tauri build` (сайдкары + NSIS), guard `mlua`, артефакт и приложение к GitHub Release.

5. **Контроль бюджета размера (size-gate)**

   - Найди `darmark.exe` в `target/release/`.
   - Замерь его размер и сравни с лимитом **≤ 6 МБ** (инвариант D6, проверяется
     в CI size-gate).
   - **Установщик:** бюджет **≤ 4 МиБ** (`release.yml`, S9; сейчас ≈1.7 МиБ) — учитывает
     `darmark.exe` + сайдкары в одном NSIS-архиве.
   - Превышение — повод не выпускать релиз, а разобраться: что добавилось,
     почему LTO/strip не помогли, не появилась ли новая зависимость.

6. **Артефакты**

   - NSIS-установщик: `target/release/bundle/nsis/`.
   - Release exe: `target/release/darmark.exe`.
   - Сайдкары в бандле: `darmark-plugin-host.exe` (≈0.9 МБ) и `darmark-plugin.exe` (≈0.9 МБ).

## Что гонять перед tauri build

Обязательно (эквивалент CI):

- `cargo fmt --all -- --check`
- `cargo clippy --workspace -- -D warnings`
- `cargo test -p md-core`
- `cargo test -p darmark`
- `cd crates/app; npm run build`

Опционально (после правок плагинной системы):

- `cargo test -p plugin-proto -p plugin-host -p plugin-cli`
- `cargo tree -p darmark | Select-String mlua` — guard-инвариант, вывод пуст.

## Проверки бюджета размера

| Проверка | Лимит | Источник |
|----------|-------|----------|
| `darmark.exe` после release-сборки | ≤ 6 МБ | D6, CI size-gate |

Порядок действий при превышении:

1. Убедись, что релизный профиль не менялся (LTO/strip/panic=abort).
2. Проверь `cargo tree` на новые тяжёлые зависимости.
3. Сравни с предыдущим релизом — когда начало расти.
4. Если причина не очевидна — не подавляй проверку, а верни пользователю
   конкретный диагноз (что выросло, когда, при каких правках).

## Артефакты

- **NSIS-установщик**: `target/release/bundle/nsis/`
- **Release exe**: `src-tauri/target/release/darmark.exe`
- **Сайдкары рядом с exe** (в бандле): `darmark-plugin-host.exe`, `darmark-plugin.exe`

Генерируются командой `npx tauri build` и не коммитятся в репозиторий. Промежуточные
сайдкары-файлы `src-tauri/binaries/*` также генерируются (`prepare-sidecars.mjs`) и
в `git` не хранятся.
