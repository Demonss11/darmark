---
name: verification-before-completion
description: Use when about to claim mdedit work is complete, fixed, or passing, before committing or creating PRs - requires running project verification commands (cargo test -p md-core, npm run build, npm run test:e2e, npx tauri build) and confirming output before making any success claims; evidence before assertions always
---

# Verification Before Completion — mdedit

## Overview

**Core principle:** Evidence before claims, always.

**Violating the letter of this rule is violating the spirit of this rule.**

## The Iron Law

```
NO COMPLETION CLAIMS WITHOUT FRESH VERIFICATION EVIDENCE
```

If you haven't run the verification command in this message, you cannot claim it passes.

## Проектный контур верификации

**mdedit** — Rust (`md-core`) + TypeScript (Vite, vanilla) + Tauri 2. Утверждение «работает»
бессмысленно без запуска команд, проверяющих именно затронутый слой.

### Карта: изменение → что запускать

| Что изменено | Минимальная верификация | Полная верификация |
|--------------|------------------------|--------------------|
| `crates/md-core/src/*.rs` (ядро, санитайзер, `to_html_mapped`) | `cargo test -p md-core` | + `cargo clippy -p md-core` |
| `crates/app/src/*.ts` (main, tables, inspector, scrollsync, mapping) | `cd crates/app && npm run build` (tsc strict) | + E2E (см. ниже) |
| `crates/app/src-tauri/` (IPC-команды, scope, CSP) | `cargo check -p mdedit` | + `cargo test -p mdedit` (3 теста IPC) |
| GUI-поведение (предпросмотр, таблицы, инспектор, скролл) | пересобрать бинарь + `npm run test:e2e` | все feature-файлы |
| Контракты байты↔UTF-16 (кириллица/эмодзи/CRLF) | E2E: `inspector.feature` (Unicode-границы) | + ручной чек-лист |
| CSP / безопасность / XSS | E2E: `preview.feature` (XSS-ловушка, `javascript:`) | ручной чек-лист из README |
| Финальная сборка (exе, установщик) | `npx tauri build --no-bundle` | `npm run tauri:build` → NSIS |
| Изменения в `tasks/TZ-*.md` (ТЗ) | перечитать ТЗ целиkom → чеклист соответствия коду | + верификация каждого § |

**Команды (из корня и `crates/app`):**
```bash
cargo test -p md-core            # ядро отдельно от GUI
cargo test -p mdedit             # IPC шелла (чтение/запись/лимит)
cd crates/app && npm run build   # tsc + vite build (строгие типы — ловит рассогласование IPC)
npm run test:e2e                 # BDD поверх РЕАЛЬНОГО собранного бинаря
npm run test:e2e -- --spec e2e/features/tables.feature   # один feature
npx tauri build --no-bundle      # единый exe с фронтом (для E2E обязателен после правок фронта)
```

> ⚠️ E2E работают с `target/release/mdedit.exe`. После правок фронта бинарь **нужно
> пересобрать**, иначе E2E проверяют старый код — это ложная верификация.
> Путь к бинарю переопределяется `MDEDIT_APP_BINARY`.

## The Gate Function

```
BEFORE claiming any status or expressing satisfaction:

1. IDENTIFY: What command proves this claim? (см. «Карту» выше)
2. RUN: Execute the FULL command (fresh, complete)
3. READ: Full output, check exit code, count failures
4. VERIFY: Does output confirm the claim?
   - If NO: State actual status with evidence
   - If YES: State claim WITH evidence
5. ONLY THEN: Make the claim

Skip any step = lying, not verifying
```

## Common Failures (mdedit-специфичные)

| Claim | Requires | Not Sufficient |
|-------|----------|----------------|
| Тесты ядра проходят | `cargo test -p md-core`: 0 failures | Прошлый запуск, «должно пройти» |
| Фронт компилируется | `npm run build`: exit 0 (tsc strict) | TS-файл «выглядит правильно» |
| IPC-контракт согласован | `npm run build` (типы) + `cargo test -p mdedit` | Только TS или только Rust |
| GUI-фича работает | E2E на **пересобранном** бинаре: сценарий проходит | `npm run build` прошёл |
| Таблицы (сортировка/фильтр) | `tables.feature`: семантика «Все»/«Ничего» пройдена | Код `tables.ts` изменён |
| Байты↔UTF-16 корректны | `inspector.feature`: Unicode-границы (кириллица/эмодзи/CRLF) | Тест на ASCII-документе |
| XSS/санитайзер | `preview.feature`: нет `onerror`/`javascript:` | «Я же удаляю on*» |
| Санитайзер строковый — полный HTML | ручной чек-лист README (экзотичный HTML) | Автотест XSS-документа |
| Скролл-синхронизация | `scrollsync.feature`: AC-1…AC-11 | Визуально «вроде синхронно» |
| Инспектор подсвечивает блок | E2E клик/hover + ручной прогон | `data-md` присутствует в HTML |
| Картинки резолвятся | ручной чек-лист: `./pic.png` в scope, вне scope — нет | `convertFileSrc` вызывается |
| Bug fixed | Воспроизвести исходный симптом → он не проявляется | Код изменён, «предположительно исправлено» |
| Regression test работает | Red-green цикл проверен | Тест прошёл один раз |
| Требования ТЗ выполнены | `tasks/TZ-*.md` перечитан → построчный чеклист | Тесты проходят |
| Релиз собирается | `npx tauri build` → exe + NSIS существуют | `cargo build` прошёл (NOT единый exe!) |
| Agent завершил | VCS diff показывает изменения | Agent рапортует «успешно» |

> ⚠️ **Ловушка mdedit:** обычный `cargo build`/`cargo run` НЕ собирает единый exe с фронтом —
> он компилирует только Rust. Встраивание `dist/` делает именно `tauri build`. Утверждать
> «сборка прошла» после `cargo build` — ложь.

## Red Flags - STOP

- «Должно», «вероятно», «кажется», «по идее работает»
- Удовлетворение до верификации («Отлично!», «Готово!», «Работает!»)
- Коммит/PR без свежего прогона команд
- Доверие рапортами суб-агента («agent said success»)
- `cargo build` вместо `tauri build` при проверке exe
- E2E на **несобранном** после правок бинаре (проверяют старый код)
- Утверждение о GUI-фиче без E2E или ручного прогона
- Утверждение «требования ТЗ выполнены» без перечитывания `tasks/TZ-*.md` целиkom
- Ссылка на решение «как в §3 ТЗ» без перечитывания раздела (см. `.kodarules`)
- Частичная верификация (только TS или только Rust при IPC-контракте)
- Усталость и желание закончить
- **ЛЮБАЯ формулировка об успехе без запуска проверки**

## Rationalization Prevention

| Excuse | Reality |
|--------|---------|
| «Должно работать» | ЗАПУСТИ верификацию |
| «Я уверен» | Уверенность ≠ доказательства |
| «Только в этот раз» | Исключений нет |
| «`npm run build` прошёл» | tsc ≠ GUI; E2E на реальном бинаре обязателен |
| «`cargo test -p md-core` прошёл» | Ядро ≠ фронт ≠ шелл; проверяй затронутый слой |
| «Агент сказал успех» | Проверяй VCS diff независимо |
| «Я устал» | Истощение ≠ оправдание |
| «Частичной проверки хватит» | Частичное ничего не доказывает |
| «Другими словами — правило не про это» | Дух важнее буквы |
| «cargo build прошёл — exe готов» | Единый exe делает `tauri build`, не cargo |
| «E2E зелёные» | На пересобранном ли бинаре? Иначе ложь |
| «Решение есть в §3 ТЗ» | Перечитай раздел целиkom, не додумывай |

## Key Patterns (mdedit)

**Тесты ядра:**
```
✅ [cargo test -p md-core] [See: N/N pass, 0 failed] "Тесты ядра проходят"
❌ "Должно пройти" / "Выглядит корректно"
```

**IPC-контракт (обе стороны):**
```
✅ [npm run build] exit 0 (tsc) + [cargo test -p mdedit] 3/3 → "IPC согласован"
❌ Только TS или только Rust при изменении контракта команды
```

**GUI-фича (E2E на реальном бинаре):**
```
✅ [npx tauri build --no-bundle] → [npm run test:e2e -- --spec .../tables.feature]
   [See: сценарий пройден] → "Сортировка/фильтр работает"
❌ "npm run build прошёл" (tsc не проверяет GUI) / E2E на старом бинаре
```

**Кодировки (байты↔UTF-16):**
```
✅ [inspector.feature] Unicode-границы (кириллица/эмодзи/CRLF) пройдены → "Маппинг корректен"
❌ Тест только на ASCII-документе
```

**Regression tests (TDD Red-Green):**
```
✅ Написать → Запуск (pass) → Откатить фикс → Запуск (MUST FAIL) → Вернуть → Запуск (pass)
❌ "Я написал регрессионный тест" (без red-green)
```

**Релиз:**
```
✅ [npx tauri build] → проверить существование mdedit.exe и bundle/nsis/*-setup.exe
❌ "cargo build прошёл" (NOT единый exe с фронтом)
```

**Требования ТЗ:**
```
✅ Перечитать tasks/TZ-*.md целиkom → чеклист по § → верифицировать каждый → сообщить пробелы
❌ "Тесты проходят, фаза завершена" / ссылка на § без перечитывания
```

**Делегирование агенту:**
```
✅ Агент рапортует успех → VCS diff → проверить изменения → фактическое состояние
❌ Доверять рапорту
```

## Manual-Only Checks (WebDriver не автоматизирует)

Эти пункты README проверяются **только вручную** — автотест их не доказывает:
- [ ] Файл > 10 МБ → понятная ошибка, приложение не виснет
- [ ] Клик по http(s)-ссылке → внешний браузер (opener)
- [ ] «Сохранить как» в произвольный каталог (модель Notepad++)
- [ ] `./pic.png` в scope виден; вне scope — не подгружается
- [ ] Экзотичный незакрытый/вложенный HTML (ограничение строкового санитайзера)
- [ ] Диалог при закрытии с несохранёнными изменениями

Сценарии помечаются `@manual` и по умолчанию пропускаются — «E2E зелёные» НЕ означает
покрытие этих пунктов.

## When To Apply

**ALWAYS before:**
- ЛЮБАЯ вариация утверждений об успехе/завершённости
- ЛЮБОЕ выражение удовлетворения
- ЛЮБОЕ позитивное высказывание о состоянии работы
- Коммит, PR, завершение задачи
- Переход к следующей задаче
- Делегирование агентам
- Утверждение о GUI/сборке/кодировках/безопасности/соответствии ТЗ

**Rule applies to:**
- Точные фразы
- Пересказ и синонимы
- Подразумевание успеха
- ЛЮБУЮ коммуникацию, намекающую на завершённость/корректность