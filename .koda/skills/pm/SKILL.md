---
name: pm
description: Продуктовое управление — исследование задачи, определение метрик, декомпозиция в эпикосы и пользовательские истории. Используй, когда нужно исследовать продуктовую задачу, задать KPI/метрики, разбить фичу на эпикосы и истории, составить продуктовые требования, приоритизировать бэклог или разобрать продуктовую возможность.
---

# Product Manager

Систематическое исследование задачи, подготовка метрик и декомпозиция требований.

## Перед началом

Прочитай память проекта (`KODA.md`) и правила (`.kodarules`) — оттуда берутся архитектура,
границы модулей, ограничения и команды проверки. Здесь их не повторяю: при расхождении
доверяй коду и памяти проекта, а не этому скиллу.

Перед анализом задачи прочитай соответствующий `tasks/TZ-*.md` **целиком**. Если решение уже
зафиксировано в ТЗ — ссылайся на раздел, а не предлагай альтернативу.

## Output Requirements

Выдавай все разделы одним структурированным markdown-документом:

- [ ] Research findings (проблема, гипотеза, таблица целевых пользователей, конкурентный контекст, зависимости)
- [ ] Метрики (North Star, ≥ 3 success metrics с числовыми baseline и target, ≥ 2 guardrail, adoption)
- [ ] ≥ 2 эпикоса с ID `EP-[NNN]`, целью, гипотезой, приоритетом (P0/P1/P2)
- [ ] ≥ 4 пользовательские истории «Как [роль] / я хочу / чтобы» с критериями Given/When/Then
- [ ] В каждой истории — подраздел **Edge Cases**
- [ ] Матрица требований, связывающая требования с эпикосами и историями
- [ ] Нефункциональные требования с конкретными числовыми целями
- [ ] План релиза: Phase 1 (MVP) и Phase 2 (Enhancements)
- [ ] Таблица рисков и таблица открытых вопросов

## Шаг 1: Исследование задачи

1. Определи область: какой модуль затронут (границы — из `KODA.md`), это активное ТЗ,
   архивное или новый пункт дорожной карты.
2. Собери применимые архитектурные ограничения из памяти проекта и `.kodarules`.
3. Определи зависимости между модулями — направление зависимостей задаёт порядок работ.
4. Зафиксируй находки:

```
# Research Findings: [Задача]
**Date:** YYYY-MM-DD
**Module:** [модуль из KODA.md]
**Related TZ:** TZ-*.md

## Problem Statement
## Hypothesis
## Target Users
| Persona | Segment | Pain Point |
## Competitive Context
| Competitor | Approach | Gap/Opportunity |
## Dependencies & Constraints
## Conclusions
```

## Шаг 2: Метрики и KPI

Категории:
- **North Star Metric** — основной показатель ценности.
- **Success Metrics** — метрики успеха фичи.
- **Guardrail Metrics** — то, что не должно деградировать. Бери измеримые через команды
  сборки/тестов из `.kodarules`: время отклика, размер артефакта сборки, число ошибок типов,
  падающие тесты.
- **Adoption Metrics** — использование фичи.

```
# Metrics Definition: [Фича]

## North Star Metric
## Success Metrics
| Metric | Type | Baseline | Target | Measurement |
## Guardrail Metrics
| Metric | Current | Min Threshold | Alert |
## Adoption Metrics
| Metric | Week 1 | Month 1 | Measurement |
## Experiment Design (if applicable)
- **Hypothesis / Duration / Primary Metric**
```

Метрика обязана быть измеримой конкретной командой. «Код стал лучше» — не метрика.

## Шаг 3: Декомпозиция в эпикосы

Учитывай разделение модулей из `KODA.md`: эпикос не должен пересекать границу модулей
без явной необходимости. Следуй INVEST, сопоставляй с success metrics.

```
## EP-[NNN]: [Название]
**Module:** [модуль] | **Priority:** [P0/P1/P2] | **Estimate:** [XS/S/M/L/XL]
**Objective:** [бизнес-результат]
**Hypothesis:** [если сделаем X, то Y улучшится]
**Impact:** [High/Med/Low на метрику]
**Dependencies:** [другие эпикосы/модули]
**Risks:** [известные риски]
**Related TZ:** TZ-*.md
```

## Шаг 4: Пользовательские истории

```
## US-[NNN]: [Название]
**Module:** [модуль] | **Epic:** EP-[NNN] | **Priority:** P[P] | **Estimate:** [size]
**Related TZ:** TZ-*.md

As a [persona]
I want [capability]
So that [value]

**Acceptance Criteria:**
- Given [ctx] When [act] Then [res]

**Edge Cases:**
- [кейс и поведение]

**Technical Notes:**
- [применимые ограничения проекта — из .kodarules]

**Open Questions:** [неразрешённое]
```

Критерии приёма формулируй так, чтобы их проверял e2e-сценарий или наблюдаемый факт,
а не «код качественный». Каждый AC обязан соблюдать проектные ограничения.

## Шаг 5: PRD

Полный документ собирается из шагов 1–4:

```markdown
# Product Requirements: [Название]
**PRD-[NNN]** | **Version:** 1.0 | **Status:** Draft
**Date:** YYYY-MM-DD | **Owner:** [Имя]
**Module Scope:** [модули]

## 1. Context          — Problem / Opportunity / Strategic Alignment
## 2. Goals & Metrics  — | Objective | Key Result | Target |
## 3. Scope            — In Scope / Out of Scope
## 4. User Personas    — | Persona | Role | Primary Need |
## 5. User Flows
## 6. Epics & Stories
## 7. Detailed Stories
## 8. Requirements Matrix — | ID | Requirement | Priority | Epic | Module | Type |
## 9. Non-Functional Requirements — | Metric | Target | Verification |
## 10. Release Plan    — Phase 1 MVP / Phase 2 Enhance
## 11. Risks           — | Risk | Impact | Mitigation |
## 12. Open Questions  — | Question | Owner | Due |
```

## Шаг 6: Приоритизация бэклога

RICE:

```
| Story | Reach | Impact | Confidence | Effort | Score |
|-------|-------|--------|------------|--------|-------|
| US-001 | [N] | 1-3-9 | 10-50-25% | weeks | [calc] |
```

Порядок приоритетов:
- Утверждённое активное ТЗ — P0: декомпозиция не пересматривает его scope.
- Архивные ТЗ — вне бэклога, только как источник конвенций.
- Пункты дорожной карты из памяти проекта — оценивать по RICE.
- Если в ТЗ есть `Implementation Plan` — порядок работ задаёт он, а не RICE.

## Best Practices

- Сначала читай контекст: ТЗ целиком, память проекта, правила.
- Не дублируй ТЗ: зафиксированное решение — ссылкой на раздел.
- Связывай с модулями: каждая история и эпикос — с указанием модуля.
- Метрики — только измеримые существующими командами проверки.
- Комментарии в генерируемом коде — на русском (см. `.kodarules`).
- Версионируй PRD по мере уточнения понимания.

## Limitations

- Не заменяет пользовательские интервью.
- Техническая реализуемость требует проверки командами сборки/тестов.
- Финансовые прогнозы — вне скоупа.
- Не может игнорировать проектные ограничения.
- Не пересматривает scope утверждённого ТЗ — только декомпозирует его.
