---
name: pm
description: Product Manager skill for task research, metric definition, and decomposition into epics and user stories. Specialized for mdedit project (Rust+TypeScript Markdown editor). Use when user wants to research a product task, define KPIs/metrics, break down features into epics and user stories, create product requirements, prioritize backlog, or analyze product opportunities. Make sure to use this skill whenever the user mentions product management, task decomposition, metrics definition, KPI tracking, epics, user stories, backlog grooming, feature analysis, roadmap planning, or product research.
---

# Product Manager — mdedit

Product management skill for systematic task research, metric preparation, and requirement decomposition, **specialized for the mdedit project** (Rust + TypeScript Markdown editor with Tauri).

## Проектный контекст

**mdedit** — лёгкий Markdown-редактор/вьюер (аналог Notepad++ для Markdown): слева редактор,
справа HTML-предпросмотр. Стек: Rust (`md-core` — pulldown-cmark 0.13) + TypeScript (Vite, без React) + Tauri 2.

**Архитектура:**
- `crates/md-core/` — чистое ядро: Markdown → HTML. Без UI-зависимостей. Тесты: `cargo test -p md-core`
- `crates/app/src/` — фронтенд: vanilla TS + Vite. Сборка: `npm run build` (tsc && vite build)
- `crates/app/src-tauri/` — Tauri-шелл: IPC (read_file / write_file / render_markdown)

**Жёсткие ограничения:**
1. **CSP:** `default-src 'self'; style-src 'self' 'unsafe-inline'`. Никаких внешних скриптов/библиотек, никакого `eval`.
2. **TypeScript strict mode:** `strict`, `noUnusedLocals`, `noUnusedParameters`.
3. **Разделение ответственности:** задачи по фронтенду (`crates/app/src/*`) не трогают `md-core` и `src-tauri`.
4. **Стиль:** 2 пробела в TS, стандартный `rustfmt` в Rust. Комментарии на русском.

**Активные задачи:**
- `tasks/TZ-excel-tables.md` — таблицы «как в Excel»: стабильный ключ + sessionStorage, точечный diff
  вместо `innerHTML`, предикатные фильтры, мультисортировка, `<mark>`-подсветка, sticky-первый столбец,
  копирование TSV/MD, печать, ресайз колонок, i18n, a11y

**Завершённые/архивные (в `tasks/архив/`):** `TZ-inspect-mode.md`, `TZ-inspect-tables.md`,
`TZ-fixes.md`, `TZ-scroll-sync-v2.md`, `TZ-inspect-mode-s1.md` — читать как источник
сложившихся решений и конвенций, не как бэклог.

**Дорожная карта (из `KODA.md`):** вкладки · подсветка синтаксиса в редакторе · экспорт HTML/PDF ·
поиск/замена (Ctrl+F) · harness-прогоны CommonMark/GFM поверх `md-core`.

**Команды сборки:**
```bash
cargo test -p md-core          # быстрая проверка ядра
cd crates/app && npm run build # tsc && vite build (tsc падает на ошибках типов)
npx tauri dev                  # разработка GUI (требует WebView2)
```

## Output Requirements

**MANDATORY:** Always produce ALL of the following in a single structured markdown document. Do NOT skip any section.

- [ ] Research findings (problem, hypothesis, target users table, competitive context, dependencies)
- [ ] Metrics definition (North Star, >= 3 success metrics with numeric baselines and targets, >= 2 guardrail metrics, adoption metrics)
- [ ] >= 2 epics with EP-[NNN] IDs, objectives, hypotheses, priorities (P0/P1/P2)
- [ ] >= 4 user stories in "As a / I want / So that" format with Given/When/Then acceptance criteria
- [ ] All user stories must include an **Edge Cases** subsection
- [ ] Requirements matrix table linking requirements to epics and stories
- [ ] Non-functional requirements with specific numeric targets
- [ ] Release plan with Phase 1 (MVP) and Phase 2 (Enhancements)
- [ ] Risks table and Open Questions table

## Instructions

### Step 1: Task Research & Analysis

1.1. **Understand the Task Context** (с учётом mdedit):
- Что за задача: фронтенд (`crates/app/src/*`), Rust-ядро (`crates/md-core/`), IPC (`src-tauri/`), или несколько?
- Это активное ТЗ (`tasks/TZ-*.md`), архивное (`tasks/архив/`) или новый пункт дорожной карты?
- Какие архитектурные ограничения применимы (CSP, TS strict, граница модулей)?

1.2. **Gather Context**:
- Текущее состояние кода: прочитай соответствующий `tasks/TZ-*.md` **целиком** перед анализом
- Проверь `KODA.md` и `.kodarules` на применимые ограничения и конвенции
- Определи зависимости между модулями: `md-core` не знает про UI, `src-tauri` — тонкая IPC-прослойка
- Не изобретай решение, уже описанное в ТЗ: в `TZ-excel-tables.md` есть готовые §3/§4.5/§6

1.3. **Document Research** as markdown (с проектным контекстом):
```
# Research Findings: [Task Name]
**Date:** YYYY-MM-DD
**Module:** md-core | app/src | src-tauri (выбрать)
**Related TZ:** TZ-*.md

## Problem Statement
[Clear description of the problem or opportunity]

## Hypothesis
[What we believe will happen and why]

## Target Users
| Persona | Segment | Pain Point |
|---------|---------|------------|
| [Name] | [Group] | [Specific pain] |

## Competitive Context
| Competitor | Approach | Gap/Opportunity |
|------------|----------|-----------------|
| [Name] | [How they solve it] | [What's missing] |

## Dependencies & Constraints
- [Technical constraint: CSP? TS strict? module boundary?]
- [Business constraint]

## Conclusions
[Key findings and recommendations]
```

### Step 2: Define Metrics & KPIs

2.1. **Identify Metric Categories** (с учётом специфики mdedit):
- **North Star Metric**: основной показатель ценности (например, скорость рендера Markdown → HTML)
- **Success Metrics**: метрики успеха фичи
- **Guardrail Metrics**: метрики, которые не должны деградировать (время рендера, размер бандла, покрытие `tsc`)
- **Adoption Metrics**: использование фичи пользователями

2.2. **Document Metrics** as markdown (с релевантными для mdedit метриками):
```
# Metrics Definition: [Feature Name]
**Date:** YYYY-MM-DD
**Module:** [md-core | app/src | both]

## North Star Metric
[Metric and target]

## Success Metrics
| Metric | Type | Baseline | Target | Measurement |
|--------|------|----------|--------|-------------|
| [Name] | Outcome | [Value] | [Goal] | [Tool/Command] |

## Guardrail Metrics
| Metric | Current | Min Threshold | Alert |
|--------|---------|---------------|-------|
| Время рендера HTML | [ms] | < X ms | debounce 120ms (main.ts) |
| Размер бандла | [KB] | < X KB | npm run build |
| tsc errors | 0 | 0 | npm run build |

## Adoption Metrics
| Metric | Week 1 | Month 1 | Measurement |
|--------|--------|---------|-------------|
| [Name] | [Value] | [Value] | [Tool] |

## Experiment Design (if applicable)
- **Hypothesis:** [If X, then Y changes by Z%]
- **Duration:** [Time period]
- **Primary Metric:** [Deciding metric]
```

### Step 3: Decompose into Epics

3.1. **Identify Epics** from research (с учётом модульной архитектуры):
- Каждый эпикус — значимая часть функциональности
- Учитывай разделение: md-core (Rust) ↔ app/src (TS) ↔ src-tauri (IPC)
- Следуй принципам INVEST
- Сопоставь с success metrics

3.2. **Structure Each Epic**:
```
## EP-[NNN]: [Epic Name]
**Module:** md-core | app/src | both
**Objective:** [Business outcome]
**Hypothesis:** [If we build X, then Y improves]
**Impact:** [High/Med/Low on metric]
**Priority:** [P0/P1/P2]
**Dependencies:** [Other epics/modules]
**Estimate:** [XS/S/M/L/XL]
**Risks:** [Known risks]
**Related TZ:** TZ-*.md (если применимо)
```

### Step 4: Write User Stories

4.1. **Format per Story**:
```
As a [persona]
I want [capability]
So that [value]
```

4.2. **Acceptance Criteria** (Given/When/Then) **с учётом проектных ограничений**:
- Проверь, что AC не нарушают CSP
- Проверь, что AC проходят TS strict (`tsc`)
- Для Rust-задач: проверь `cargo test -p md-core`

4.3. **Story Structure**:
```
## US-[NNN]: [Title]
**Module:** [md-core | app/src | src-tauri]
**Epic:** EP-[NNN] | **Priority:** P[P] | **Estimate:** [size]
**Related TZ:** TZ-*.md

As a [persona]
I want [capability]
So that [value/outcome]

**Acceptance Criteria:**
- Given [ctx] When [act] Then [res]

**Edge Cases:**
- [Edge case and behavior]

**Technical Notes:**
- [CSP constraint?]
- [TS strict constraint?]
- [Module boundary: md-core vs app?]

**Open Questions:** [Unresolved items]
```

### Step 5: Generate Product Requirements Document

Create comprehensive markdown document (с проектной спецификой):

```markdown
# Product Requirements: [Project Name]
**PRD-001** | **Version:** 1.0 | **Status:** Draft
**Date:** YYYY-MM-DD | **Owner:** [Name]
**Module Scope:** md-core | app/src | src-tauri

---
## 1. Context
### 1.1 Problem
[Problem description]

### 1.2 Opportunity
[Market or user opportunity]

### 1.3 Strategic Alignment
[How this aligns with mdedit goals: tabs, syntax highlighting, export, search/replace]

---
## 2. Goals & Metrics
| Objective | Key Result | Target |
|-----------|------------|--------|
| [Objective] | [KR with metric] | [Value] |

---
## 3. Scope
**In Scope:**
- [Feature 1] (module: ...)
- [Feature 2] (module: ...)

**Out of Scope:**
- [Deferred - target Q]

---
## 4. User Personas
| Persona | Role | Primary Need |
|---------|------|--------------|
| [Name] | [Title] | [Need] |

---
## 5. User Flows
1. User starts at [entry point]
2. [Step with response]
3. [End state]

---
## 6. Epics & Stories
### EP-001: [Name] [module, objective and impact]
- US-001: [title] (module)
- US-002: [title] (module)

---
## 7. Detailed Stories
### US-001: [Title]
**Module:** [md-core | app/src | src-tauri]
**Epic:** EP-001 | **Priority:** P0 | **Estimate:** M

As a [persona]
I want [capability]
So that [value]

**Acceptance Criteria:**
- Given [ctx] When [act] Then [res]

---
## 8. Requirements Matrix
| ID | Requirement | Priority | Epic | Module | Type |
|----|-------------|----------|------|--------|------|
| REQ-001 | [Desc] | P0 | EP-001 | [module] | Functional |

---
## 9. Non-Functional Requirements
| Metric | Target | Verification |
|--------|--------|--------------|
| Время рендера | < [X]ms | debounce 120ms |
| Размер бандла | < [X] KB | npm run build |
| tsc errors | 0 | npm run build |
| Cargo errors | 0 | cargo test -p md-core |

---
## 10. Release Plan
**Phase 1 - MVP:** [Features], [Date]
**Phase 2 - Enhance:** [Features], [Date]

---
## 11. Risks
| Risk | Impact | Mitigation |
|------|--------|------------|
| [Risk] | High/Low | [Action] |

---
## 12. Open Questions
| Question | Owner | Due |
|----------|-------|-----|
| [Q] | [Name] | [Date] |
```

### Step 6: Prioritize Backlog

6.1. **RICE Scoring** (с учётом проектных приоритетов):
```
| Story | Reach | Impact | Confidence | Effort | Score |
|-------|-------|--------|------------|--------|-------|
| US-001 | [N] | 1-3-9 | 10-50-25% | weeks | [calc] |
```

6.2. **Проектные приоритеты:**
- Активное ТЗ (`tasks/TZ-excel-tables.md`) — P0: оно уже утверждено, декомпозиция не пересматривает scope
- Архивные ТЗ (`tasks/архив/`) — вне бэклога; используются только как источник конвенций
- Пункты дорожной карты из `KODA.md` (вкладки, подсветка синтаксиса, экспорт HTML/PDF, Ctrl+F,
  CommonMark/GFM-harness) — оценивать по RICE
- Порядок работ определяется `Implementation Plan` из ТЗ (шаги 1–10 в `TZ-excel-tables.md` §9),
  а не RICE
- Направление зависимостей: `md-core` (не знает про UI) → `app/src` (vanilla TS) → `src-tauri` (IPC)

## Best Practices

- **Сначала читай контекст:** перед анализом задачи прочитай соответствующий `tasks/TZ-*.md` **целиком**
- **Не дублируй ТЗ:** если решение уже зафиксировано (стабильный ключ, предикаты вместо `data-*`,
  точечный diff), ссылайся на раздел ТЗ, а не предлагай альтернативу
- **Соблюдай ограничения:** CSP (`default-src 'self'`, без внешних библиотек), TS strict, граница модулей —
  всегда указывай в AC
- **Метрики должны быть измеримыми:** через `npm run build`, `cargo test -p md-core`, debounce-таймеры,
  размер бандла, число ошибок `tsc`
- **Связывай с модулями:** каждая история и эпикус — с указанием модуля (md-core | app/src | src-tauri)
- **AC ≠ дизайн:** критерии приёма формулируются так, чтобы их можно было проверить в e2e-сценариях ТЗ
- **Комментарии на русском:** если генерируешь код, комментарии должны быть на русском
- **Версионируй PRD:** обновляй версию и статус по мере эволюции понимания

## Limitations

- Не заменяет пользовательские интервью
- Техническая реализуемость требует валидации инженерии (проверка `tsc`, `cargo test`)
- Финансовые прогнозы требуют FP&A
- Compliance требует legal review
- **Не может игнорировать проектные ограничения** (CSP, TS strict, архитектура workspace)
- **Не пересматривает scope утверждённого ТЗ** — только декомпозирует его

## Примеры для mdedit

### Example 1: Активное ТЗ — таблицы «как в Excel» (`tasks/TZ-excel-tables.md`)
**User:** "Сделай таблицы в предпросмотре как в Excel: сортировка, фильтры, копирование TSV."

Output:
- Research: текущий `tables.ts`, состояние в `WeakMap` (губится при `innerHTML`), AC-1..AC-17 ТЗ
- Metrics: применение фильтра < 50 мс; 0 ошибок `tsc`; `cargo test -p md-core` — зелёные;
  guardrail — размер бандла не растёт за счёт внешних библиотек (их быть не должно)
- Epics (по Implementation Plan §9): EP-001 стабильный ключ + sessionStorage, EP-002 точечный diff
  вместо `innerHTML`, EP-003 предикатные фильтры + мультисортировка, EP-004 копирование TSV/MD
- Stories: 8–12, edge cases из §6 ТЗ: `innerHTML`-перезапись, `NaN`-ключ, sticky-первый столбец,
  склейка `<th`/`<td`, переполнение `<select>` (> `PREFILTER_MAX`)

### Example 2: Новый пункт дорожной карты — поиск/замена (Ctrl+F)
**User:** "Добавь поиск и замену по документу."

Output:
- Research: скользящее окно маппинга (`mapping.ts`), debounce рендера 120 мс, отсутствие готового
  layer-механизма выделения
- Metrics: поиск по 500 КБ < 100 мс; подсветка совпадений в 60 fps; guardrail — время рендера не деградирует
- Epics: EP-001 поиск + счётчик совпадений, EP-002 переходы F3/Shift+F3, EP-003 замена одна/все
- Stories: 6–8, edge cases: регистронезависимость, совпадения внутри кода/таблицы, скролл-синхронизация

### Example 3: Точечное исправление — рассинхрон скролла после загрузки изображений
**User:** "После подгрузки картинок предпросмотр уезжает относительно редактора."

Output:
- Research: `scrollsync.ts` (пропорция, не пиксели), `mapping.ts` (окно), жизненный цикл загрузки изображений
- Metrics: ошибка смещения < 2 пикселя после `load`; отсутствие layout thrashing
- Epics: EP-001 хук на загрузку изображений, EP-002 пересчёт пропорции
- Stories: 3–4, AC обязательно включают `npm run build` и `cargo test -p md-core`