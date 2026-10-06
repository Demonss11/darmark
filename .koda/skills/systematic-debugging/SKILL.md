---
name: systematic-debugging
description: Use when encountering any bug in mdedit (Rust md-core, TS фронт, Tauri шелл, IPC, WebView E2E), test failure, or unexpected behavior, before proposing fixes
---

# Systematic Debugging — mdedit

## Overview

**Core principle:** ALWAYS find root cause before attempting fixes. Symptom fixes are failure.

**Violating the letter of this process is violating the spirit of debugging.**

## The Iron Law

```
NO FIXES WITHOUT ROOT CAUSE INVESTIGATION FIRST
```

If you haven't completed Phase 1, you cannot propose fixes.

## Архитектура mdedit (для локализации сбоя)

```
WebView (crates/app/src/*.ts)
   │  window.__bridge / invoke IPC        ← контракт: аргументы и возврат
   ▼
Tauri шелл (crates/app/src-tauri/)        ← команды, scope, CSP, opener
   │
   ▼
Ядро md-core (crates/md-core/src/*.rs)    ← pulldown→строковый HTML,
                                             to_html_mapped, data-md, byte offsets
   │  строка HTML → innerHTML предпросмотра
   ▼
Предпросмотр + редактор <textarea>        ← UTF-16 code units в JS
```

**Три границы, на которых живут баги:**
1. **TS ↔ Rust (IPC):** рассогласование типов/имён команды → ловит `npm run build` (tsc strict)
2. **Byte offsets (Rust) ↔ UTF-16 code units (JS):** кириллица/эмодзи/CRLF → смещение подсветки
3. **Строка HTML → innerHTML:** CSP `unsafe-inline`, санитайзер, незакрытые теги

## When to Use

Use for ANY technical issue:
- Провал `cargo test -p md-core` / `cargo test -p mdedit` / E2E-сценария
- Баг GUI: предпросмотр, таблицы, инспектор, скролл-синхронизация, картинки
- Смещение/рассогласование подсветки блока (байты↔UTF-16)
- Ошибки чтения/записи файлов, лимит 10 МБ
- Build-сбои: `npm run build` (tsc), `tauri build`, `cargo`
- Проблемы CSP/scope: CSS не применился, картинка не резолвится, XSS-ловушка сработала

**Use this ESPECIALLY when:**
- Под давлением времени («быстрый фикс» манит)
- «Очевидно, что не так» — до воспроизведения
- Уже пробовали несколько фиксов
- Предыдущий фикс не помог
- Баг не воспроизводится стабильно (плавающая синхронизация скролла, Unicode-границы)

**Don't skip when:**
- Баг кажется простым (у простых багов тоже есть root cause)
- Спешите (систематика быстрее перебора)
- Нужно «прямо сейчас» (систематика — самый быстрый путь)

## The Four Phases

You MUST complete each phase before proceeding to the next.

### Phase 1: Root Cause Investigation

**BEFORE attempting ANY fix:**

1. **Read Error Messages Carefully**
   - `cargo test`: имя теста, ожидаемое/фактическое, файл:строка
   - `npm run build` (tsc): код TS2xxx — это контракт IPC, не «косметика»
   - E2E WebDriver: HTML-дамп страницы из отчёта + feature-сценарий;
     WebView **без devtools** — дамп страницы и есть ваш «консоль»
   - Читай стектрейс полностью, не пропускай warnings

2. **Reproduce Consistently**
   - Точные шаги? Документируй: какой документ (размер, кодировка, контент),
     какое действие (клик/hover/фильтр/прокрутка)
   - Плавающий баг (скролл, rAF) → увеличь документ/итерации, не угадывай
   - Unicode-баг → минимальный пример: кириллица / эмодзи / CRLF по отдельности,
     потом вместе; сравни ASCII-документ (там не проявляется?)
   - Не воспроизводится → собирай данные дальше, не предполагай

3. **Check Recent Changes**
   - `git diff`, последние коммиты
   - Правки контракта IPC (сигнатура команды ↔ `tauri.ts` ↔ `main.ts`)
   - Правки ТЗ `tasks/TZ-*.md` (особенно §3 — «как в ТЗ» додумывать нельзя)
   - Пересобран ли бинарь: E2E на `target/release/mdedit.exe` БЕЗ
     `tauri build` после правок фронта проверяют **старый код** — это не баг, это артефакт

4. **Gather Evidence in Multi-Component Systems**

   **mdedit = минимум 3 компоненты (WebView → шелл → ядро). BEFORE proposing fixes,
   добавь инструментирование на КАЖДОЙ границе:**

   ```
   Граница 1 (TS → invoke): логируй аргументы команды и возврат (JSON)
   Граница 2 (шелл → ядро):  логируй что передаётся в to_html_mapped / какой путь файла
   Граница 3 (ядро → HTML):  сними кусок output рядом с data-md, проверь byte offset
   Граница 4 (HTML → JS):    в предпросмотре сфоткай/дампни data-md и сравни
                             с offset из редактора (байты vs UTF-16!)
   ```

   **Пример (подсветка не на том блоке):**
   ```bash
   # Ядро: что вернул to_html_mapped
   cargo test -p md-core mapped -- --nocapture
   # Шелл: что дошло до команды
   cargo test -p mdedit -- --nocapture
   # WebView: дамп страницы из E2E-отчёта (data-md="17,42")
   npm run test:e2e -- --spec e2e/features/inspector.feature
   ```

   **Это показывает:** какая граница ломает (ядро ✓ отдаёт байты, JS ✗ трактует как UTF-16)

5. **Trace Data Flow**

   **WHEN error deep in call stack** — см. `root-cause-tracing.md` в этом каталоге.

   **Быстрая версия для mdedit:**
   - Плохое значение (offset/scope/путь) — откуда родом?
   - pulldown-узел → byte offset → data-md → парсинг в JS → getBoundingClientRect?
   - Тяни вверх до источника; фиксь у источника, а не у симптома
   - Типовые источники: `to_html_mapped` (ядро), `mapping.ts` (конвертация),
     `convertFileSrc`/scope (шелл), CSP (Tauri-конфиг)

### Phase 2: Pattern Analysis

**Find the pattern before fixing:**

1. **Find Working Examples**
   - Похожий работающий код в том же проекте: `tables.ts` работает, `inspector.ts` нет —
     в чём разница обработки событий?
   - Рабочий feature-файл ↔ падающий: сравни шаги и моки

2. **Compare Against References**
   - Если реализуешь по ТЗ — перечитай `tasks/TZ-*.md` целиkom (особенно §3),
     reference-доки целиkom, не по диагонали (см. `.kodarules`)
   - Pulldown/pulldown-cmark документация — читать полностью перед «адаптацией»

3. **Identify Differences**
   - Liste every difference, however small
   - Не предполагай «это не может влиять»: для mdedit «не может влиять» =
     CRLF в конце строки, BOM, trailing space в data-md, порядок `on*`-атрибутов

4. **Understand Dependencies**
   - IPC-контракт: обе стороны согласованы? (tsc + cargo test)
   - CSP: `default-src 'self'` + `style-src 'unsafe-inline'` — на месте?
   - Scope: `assetProtocol.scope` = `$APPDATA/**`?
   - Бинарь для E2E: пересобран (`tauri build`) после правок фронта?
   - WebView: нет devtools — закладывай диагностику в дамп страницы/логи

### Phase 3: Hypothesis and Testing

**Scientific method:**

1. **Form Single Hypothesis**
   - «Корень — X, потому что Y». Запиши. Конкретно, не расплывчато.
   - Пример: «Подсветка смещена, потому что `mapping.ts` трактует byte offsets
     из `data-md` как UTF-16 индексы без конвертации для кириллицы»

2. **Test Minimally**
   - НАИМЕНЬШЕЕ изменение для проверки гипотезы; одна переменная за раз
   - Для mdedit: фиксируй на СЛОЕ — ядро (`cargo test -p md-core`),
     конвертация (`mapping.ts` + юнит), GUI (E2E на пересобранном бинаре).
     Не меняй TS и Rust одновременно

3. **Verify Before Continuing**
   - Сработало → Phase 4. Не сработало → НОВАЯ гипотеза, НЕ наслаивай фикс

4. **When You Don't Know**
   - Скажи «я не понимаю X». Не делай вид. Спроси / исследуй дальше.

### Phase 4: Implementation

**Fix the root cause, not the symptom:**

1. **Create Failing Test Case**
   - Ядро → `#[test]` в `crates/md-core/src/to_html.rs` (unit-тест, red-green)
   - Контракт кодировок → E2E-сценарий `inspector.feature` (кириллица/эмодзи/CRLF)
   - GUI-фича → feature-файл на реальном бинаре
   - Шелл/IPC → тест в `crates/app/src-tauri/` (как 3 существующих)
   - MUST have before fixing; используй `superpowers:test-driven-development`

2. **Implement Single Fix**
   - ONE change at a time; никаких «заодно» улучшений и рефакторингов вперемешку
   - Правка контракта IPC = синхронно обе стороны (Rust-команда + TS-обёртка),
     но гипотеза по-прежнему одна

3. **Verify Fix**
   - `cargo test -p md-core` → 0 failed; `npm run build` → exit 0;
     E2E-сценарий → пройден на **пересобранном** бинаре; соседние сценарии не сломаны
   - Перед заявлением об успехе — `superpowers:verification-before-completion`

4. **If Fix Doesn't Work**
   - STOP. Сосчитай: сколько фиксов попробовано?
   - < 3 → в Phase 1 с новой информацией
   - **≥ 3 → STOP,question architecture (шаг 5)**
   - НЕ пытайся фикс №4 без обсуждения архитектуры

5. **If 3+ Fixes Failed: Question Architecture**

   **Паттерны архитектурной проблемы в mdedit:**
   - Каждый фикс подсветки вскрывает новую границу байты↔UTF-16 в новом месте
     → возможно, конвертацию надо инкапсулировать в один модуль (`mapping.ts`),
     а не латать по точкам
   - Каждый фикс скролла порождает новый гонка rAF ↔ WebDriver
     → возможно, нужен явный сигнал готовности вместо ожидания
   - Фиксы требуют «массивного рефакторинга» → вопрос «звучит ли паттерн фундаментально»

   **STOP и question fundamentals** (обсуди с человеком ДО фиксов №4).
   Это не неудачная гипотеза — это неправильная архитектура.

## Red Flags - STOP and Follow Process

- «Быстрый фикс сейчас, разберусь потом»
   - «Просто поменяю X и посмотрю»
- «Заменю data-md на другой формат и заработает» (не разобравшись, где ломается контракт)
- «Добавлю несколько правок, прогоню тесты» (TS + Rust + HTML разом)
- «Пропущу тест, проверю руками» (особенно для Unicode-границ — руками не поймаешь)
- «Наверное дело в CSP, пофиксию»
- «Не понимаю до конца, но может сработать»
- «§3 ТЗ — сделаю как в ТЗ» без перечитывания раздела (см. `.kodarules`)
- «E2E падают — увеличу timeout» (это маскирование, не фикс; см. `condition-based-waiting.md`)
- Предложение решений до трассировки потока данных
- **«Ещё одна попы фикса» (когда уже 2+)**
- **Каждый фикс вскрывает проблему в новом месте**

**ALL of these mean: STOP. Return to Phase 1.**

## your human partner's Signals You're Doing It Wrong

- «Это что, не происходит?» — ты предположил, не проверив (бинарь пересобран?)
- «Он покажет нам…?» — надо было добавить сбор доказательств на границах
- «Хватит гадать» — предлагаешь фиксы без понимания
- «Ultra-think» — question fundamentals, не симптомы
- «Мы застряли?» (раздражение) — подход не работает

**When you see these:** STOP. Return to Phase 1.

## Common Rationalizations

| Excuse | Reality |
|--------|---------|
| «Баг простой, процесс не нужен» | У простых багов есть root cause. Процесс быстр для простых. |
| «Срочно, нет времени» | Систематика БЫСТРЕЕ гадания и перебора. |
| «Сначала попробую фикс, потом разберусь» | Первый фикс задаёт паттерн. Делай правильно с самого начала. |
| «Тест напишу после подтверждения» | Нетестированные фиксы не держатся. Тест сначала — и он же проверка. |
| «Несколько правок разом — экономия» | Не изолировать что сработало. Порождает новые баги (TS↔Rsync рассинхрон). |
| «Reference (ТЗ/док) длинный, адаптирую» | Частичное понимание гарантирует баги. Читать целиkom. |
| «Вижу проблему, сейчас пофикшу» | Видеть симптом ≠ понимать root cause. |
| «Ещё одна попытка» (после 2+ провалов) | 3+ провала = архитектурная проблема. Question pattern, не фиксь. |
| «E2E зелёные → всё ок» | На пересобранном ли бинаре? Иначе проверяли старый код. |

## Quick Reference

| Phase | Key Activities | Success Criteria |
|-------|---------------|------------------|
| **1. Root Cause** | Прочитать ошибки, воспроизвести (ASCII vs Unicode), git diff, инструментировать границы TS↔Rust↔HTML | Понять ЧТО и ПОЧЕМУ |
| **2. Pattern** | Найти рабочий пример (tables↔inspector), перечитать ТЗ/референс целиkom, diff | Найти отличия |
| **3. Hypothesis** | Одна гипотеза, минимальная проверка на одном слое | Подтверждена или новая |
| **4. Implementation** | Falling test (unit/E2E), один фикс, верификация | Баг решён, тесты зелёные |

## When Process Reveals "No Root Cause"

Если расследование показало, что сбой действительно средовой/тайминговый
(плавающая скролл-синхронизация, особенности WebView-рендера):

1. Процесс завершён
2. Задокументируй что проверял (включая «бинарь пересобран», «ASCII воспроизводится?»)
3. Реализуй обработку: явный сигнал готовности вместо `setTimeout`,
   `waitForSelector` вместо таймаутов (см. `condition-based-waiting.md`)
4. Добавь логирование для будущих расследований

**But:** 95% «нет root cause» — это незавершённое расследование.

## Supporting Techniques

- **`root-cause-tracing.md`** — трассировка назад по стеку до источника (offset:
  pulldown-узел → data-md → парсинг JS → DOM-координаты)
- **`defense-in-depth.md`** — валидация на нескольких слоях ПОСЛЕ нахождения
  root cause (ядро отдаёт байты → `mapping.ts` конвертирует → UI валидирует границы)
- **`condition-based-waiting.md`** — заменить `setTimeout` в E2E на условия готовности
  (появление/изменение дамп-признака страницы)