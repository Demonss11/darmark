---
name: senior-discovery-product
description: Evidence-based product research specialized for mdedit (Rust+TypeScript Markdown editor with Tauri). Use for competitor analysis of Markdown editors, user pain-point research, hypothesis validation on editor features, market direction for lightweight desktop tools, or discovery for roadmap items (tabs, syntax highlighting, export, search/replace). Research first with current sources, then synthesize evidence into insights and recommendations tailored to the mdedit constraints (vanilla TS, no React, strict CSP, Rust core).
---

# Senior Discovery Product — mdedit

Evidence-based product research skill, **adapted for the mdedit project** (Rust + TypeScript Markdown editor with Tauri 2).

## Проектный контекст

**mdedit** — лёгкий Markdown-редактор/вьюер (аналог Notepad++ для Markdown): слева редактор,
справа HTML-предпросмотр. Стек: Rust (`md-core` — pulldown-cmark 0.13) + TypeScript (Vite,
без React) + Tauri 2.

**Текущая MVP-функциональность:**
- Открытие/сохранение файлов, реалтайм-предпросмотр с debounce 120 мс
- GFM-таблицы со стилями, сортировка, фильтры, поиск по колонкам
- Режим инспектора: двусторонняя подсветка блоков предпросмотра ↔ исходника
- Локальные картинки через asset-протокол Tauri
- Синхронная прокрутка (анкорная / пропорциональная)
- Тёмная тема, индикатор несохранённых изменений

**Завершённые ТЗ:** `TZ-inspect-mode.md`, `TZ-scroll-sync-v2.md`, `TZ-inspect-tables.md`
(в `tasks/архив/`).

**Активное ТЗ:** `TZ-excel-tables.md` — стабильный ключ + sessionStorage, точечный diff,
предикатные фильтры, мультисортировка, `<mark>`-подсветка, sticky-колонка, копирование TSV/MD,
печать, ресайз, i18n, a11y.

**Дорожная карта (не реализована):** вкладки · подсветка синтаксиса · экспорт HTML/PDF ·
поиск/замена (Ctrl+F) · CommonMark/GFM-harness.

**Жёсткие ограничения:**
- **CSP:** `default-src 'self'` — никаких внешних скриптов/CDN/библиотек
- **Без React/фреймворков:** vanilla TS + Vite
- **Rust-ядро:** `md-core` без UI-зависимостей, переиспользуемое
- **Размер:** релиз с `opt-level="s"`, LTO, strip — бинарь должен оставаться лёгким

## Research Rules

- Search before concluding.
- One source is a weak signal; multiple independent sources form a pattern.
- Separate fact from interpretation.
- Challenge the framing if the proposed solution may target the wrong problem.
- **Always map findings to mdedit's constraints:** no external scripts (CSP), no React,
  Rust core boundary, vanilla TS only. If a recommendation requires an npm package,
  justify its size impact and CSP compliance.

## Workflow

1. **Read local context.** Check `README.md`, `KODA.md`, `.kodarules`, `tasks/`,
   `ideas/` — mdedit уже закрывает часть потребностей. Не переисследуй то, что известно.
2. **Define 4-6 search angles** aligned with mdedit's domain:
   - Lightweight Markdown editors (desktop): Notepad++, VS Code, Zettlr, Mark Text, Obsidian
   - Tauri vs Electron desktop apps: bundle size, startup time, memory footprint
   - Rust markdown libraries: pulldown-cmark vs comrak vs pulldown-cmark-escape
   - Markdown feature gaps: GFM tables, task lists, callouts, math, mermaid
   - UX patterns for split-pane editors: scroll sync, inspector, live preview
3. **Browse current sources** and fetch the strongest ones (benchmarks, GitHub stars,
   user reviews, Reddit/HN discussions about Markdown editors).
4. **Rate each insight:** weak signal, hypothesis, pattern, or evidence.
5. **Synthesize:** patterns, contradictions, white space, next steps — **always with
   mdedit relevance**.

## Common Research Tracks (mdedit-специфичные)

| Трек | Примеры вопросов | Применяется к |
|------|-----------------|---------------|
| **Конкуренты Markdown-редакторов** | Что умеют Notepad++/Zettlr/Obsidian? Где дыры? | Вкладки, подсветка синтаксиса |
| **Tauri vs Electron для лёгких утилит** | Размер бандла, старт, память | Архитектура mdedit |
| **Rust-парсеры Markdown** | pulldown-cmark vs comrak: скорость, GFM, расширенные фичи | `md-core` roadmap |
| **UX split-pane редакторов** | Scroll sync, inspector, live preview — паттерны | `scrollsync.ts`, `inspector.ts` |
| **Гэки Markdown-фич** | Callouts, math, mermaid, YAML frontmatter | Приоритизация дорожной карты |
| **Валидация гипотез** | Нужны ли вкладки пользователям mdedit? | `TZ-*` ТЗ, приоритизация |
| **Бенчмарки и метрики** | Время рендера, размер бандла, FPS scroll sync | Guardrail metrics |

## Output Structure

### 1. Research Goal
[Чёткая формулировка: что исследуем и зачем для mdedit]

### 2. Key Findings with Confidence Labels
Для каждого findings:
- **[Finding title]** — `pattern` | `evidence` | `hypothesis` | `weak signal`
- Краткое описание (2-3 предложения)
- Связь с mdedit: как влияет на архитектуру, фичи, приоритизацию

### 3. Evidence and Source Links
| # | Source | Type | URL | Relevance |
|---|--------|------|-----|-----------|
| 1 | [Title] | benchmark | URL | [Как влияет на mdedit] |

### 4. Contradictions or Missing Data
- Где данные расходятся между источниками?
- Чего не хватает для принятия решения?
- Что нужно протестировать в mdedit (прототип/bench)?

### 5. Recommendation
[Конкретное действие для mdedit с обоснованием]

### 6. One Concrete Next Step
[Что сделать в проекте прямо сейчас: прототип в `crates/app/src/`, бенч `md-core`,
новое ТЗ в `tasks/`, или отказ от фичи]

## mdedit-специфичные шаблоны

### Шаблон: Анализ конкурента Markdown-редактора

```
## [Конкурент]: [Название]

| Параметр | Значение | Вывод для mdedit |
|----------|----------|-----------------|
| Стек | [Electron/Rust/WebView] | [Сравнение с Tauri] |
| Размер бандла | [X MB] | [Ожидание mdedit: < X MB] |
| Markdown-фичи | [GFM, math, callouts...] | [Что добавить/не добавлять] |
| Split-pane UX | [scroll sync, inspector...] | [Что заимствовать] |
| Вкладки | [да/нет] | [Приоритет для дорожной карты] |

**Вердикт:** [Keep / Watch / Skip] для mdedit
**Обоснование:** [Почему это важно для CSP/TS strict/Rust core]
```

### Шаблон: Валидация гипотезы фичи

```
## Гипотеза: [Название фичи]

**Гипотеза:** If we add [feature], then [metric] improves by [X%].

| Сигнал | Тип | Источник | Сила |
|--------|-----|----------|------|
| [Finding] | pattern/evidence | [Source] | высокая/средняя/низкая |

**Требуется для проверки:**
- [ ] Прототип в `crates/app/src/` (TS)
- [ ] Бенч в `md-core` (Rust)
- [ ] Опрос пользователей / GitHub issues

**Риск для CSP/размера:** [низкий/средний/высокий]
**Влияние на md-core:** [нет / требует новых функций / требует API change]
```

### Шаблон: Анализ Rust-парсера Markdown

```
## [Библиотека]: [pulldown-cmark / comrak / ...]

| Параметр | Значение | Влияние на mdedit |
|----------|----------|------------------|
| Скорость парсинга | [ms / MB] | [Сравнение с текущим] |
| GFM coverage | [% spec] | [Нужны ли патчи] |
| Расширенные фичи | [callouts, math...] | [Стоит ли брать] |
| Размер crate | [KB] | [Влияние на бинарь] |
| ZST / unsafe | [да/нет] | [Безопасность ядра] |

**Вердикт:** [Остаться на pulldown-cmark] | [Перейти] | [Гибрид]
```

## Best Practices для mdedit

- **Не предлагай фичи, уже в ТЗ.** Если исследование подтверждает `TZ-excel-tables.md` —
  ссылайся на него, не предлагай альтернативный дизайн.
- **CSP — святое.** Любая рекомендация с «подключи внешнюю библиотеку» должна быть
  `reject` для mdedit. Предпочитай vanilla TS решения.
- **Rust core — это актив.** Парсеры и трансформации Markdown должны идти в `md-core`,
  а не на TS-сторону.
- **Размер бандла — метрика успеха.** Любое новое npm-зависимое решение должно оцениваться
  по вкладу в размер финального NSIS-установщика.
- **Проверяй на реальных корпусах.** CommonMark spec, GFM spec, реальные `.md` файлы —
  используй их как бенчмарк.
- **Связывай с модулями.** Каждый вывод: md-core | app/src | src-tauri.
- **Русские комментарии.** Если рекомендация предполагает код — комментарии на русском.

## Limitations

- Не заменяет пользовательские интервью и usability-тесты
- Технические бенчмарки требуют запуска на целевой платформе (Windows + WebView2)
- Финансовые прогнозы требуют FP&A
- Compliance требует legal review (особенно для лицензий Rust-кратов)
- **Не может рекомендовать внешние скрипты или библиотеки** — это нарушает CSP mdedit