# skills-eval — инвентаризация и оценка скиллов (Этапы A–B)

Артефакт к `TZ-skills-integration.md`. Дисциплина: **адаптировать то, что реально пригодится;
остальное отбросить с причиной**. Ничего не интегрируется без пилота (Этап C).

Дата: 04.10.2026. Koda CLI 1.2.1, git 2.56.0.

---

## 1. Этап A — источник и доступ

Гипотеза первой редакции ТЗ («raw-URL отдаёт 404 → скиллы недоступны») **опровергнута**:
репозиторий открыт для клонирования, хотя raw-ссылки и HTML-страницы каталога не работают.

- Проверка: `git ls-remote https://gitverse.ru/gitverse/skills` → OK (`master` = `2e91430`).
- Клон: `git clone --depth 1 https://gitverse.ru/gitverse/skills` → OK, **73 каталога-скилла**.
- Вывод: **`BLOCKED` не применяется ни к одному из S1–S9.** Все 9 получены.

### 1.1. Инвентаризация S1–S9 (факт)

| # | Скилл | Файлы | Объём | Формат | Дефекты целостности |
|---|---|---|---|---|---|
| S1 | `typescript-advanced-types` | `SKILL.md`, `references/details.md` | 318 + 404 строк | SKILL.md + reference | — |
| S2 | `frontend-api-integration-patterns` | `SKILL.md` | ~60 строк | SKILL.md | React-центричен |
| S3 | `review` | `SKILL.md` | ~130 строк | SKILL.md | frontmatter `allowedTools` — поле **не из док. Koda** (только `name`/`description`); требует `task`-инструмент и `gh` CLI |
| S4 | `systematic-debugging` | `SKILL.md` | ~190 строк | SKILL.md | **dangling refs**: `root-cause-tracing.md`, `defense-in-depth.md`, `condition-based-waiting.md` — отсутствуют |
| S5 | `codebase-design` | `SKILL.md`, `DEEPENING.md`, `DESIGN-IT-TWICE.md`, `agents/openai.yaml` | 114 + 37 + 44 | SKILL.md + references | `agents/openai.yaml` — для другого рантайма (лишний) |
| S6 | `verification-before-completion` | `SKILL.md` | ~120 строк | SKILL.md | — |
| S7 | `test-driven-development` | `SKILL.md` | ~250 строк | SKILL.md | **dangling ref**: `writing-good-tests.md` — отсутствует |
| S8 | `webapp-testing` | `SKILL.md` | ~90 строк | SKILL.md | **dangling refs**: `scripts/with_server.py`, `examples/*` — отсутствуют; требует Python + Playwright |
| S9 | `hindsight-docs` | `SKILL.md` | 1 файл, 4 КБ | SKILL.md | **не тот артефакт**: документация продукта Hindsight (система памяти), не «актуализация README» |

**Важно:** дефекты целостности (dangling refs) означают, что часть скиллов при активации будет
ссылаться на несуществующие файлы — это надо учитывать (либо не интегрировать, либо форкнуть).

---

## 2. Этап B — карточки оценки

### S1 `typescript-advanced-types`
```
S1 typescript-advanced-types
  Класс:             B (сторонний)
  Источник/формат:   gitverse.ru/gitverse/skills · SKILL.md + references/details.md
  Доступ:            OK
  Соответствие стеку:частично (TS strict — да; но материал про generics/mapped/conditional/template literals)
  Конфликты:         нет (теоретический материал, без кода проекта)
  Польза (ожидаемая):низкая — TZ-excel-tables требует лишь discriminated union для Predicate
                     и простых типов (SortLevel, PersistedTableState). Это базовый TS, не «advanced».
  Стоимость внедрения:средняя (318+404 строк, редко триггерится на реальные задачи проекта)
  Вердикт B:         отклонить
```
**Обоснование:** 90 % материала (generic constraints, mapped types, template literal paths) не
встречается в задачах mdedit. Единственный нужный приём — discriminated union — уже применён
в проекте неявно и не требует скилла. Скилл будет мёртвым грузом (риск R4).

### S2 `frontend-api-integration-patterns`
```
S2 frontend-api-integration-patterns
  Класс:             B
  Источник/формат:   gitverse.ru/gitverse/skills · SKILL.md
  Доступ:            OK
  Соответствие стеку:нет (React Query/SWR/TanStack/Zustand/axios — не наш стек)
  Конфликты:         CSP запрещает внешние библиотеки; проект — vanilla TS + Tauri IPC
  Польза (ожидаемая):низкая — invoke() локальный, гонки уже закрыты renderSeq (main.ts:21,32)
  Стоимость внедрения:низкая, но польза ~0
  Вердикт B:         отклонить
```
**Обоснование:** вся ценность скилла — в React-экосистеме кэширования серверного состояния,
которой в mdedit нет. `AbortController` неприменим к Tauri `invoke` так, как описано.

### S3 `review`
```
S3 review
  Класс:             B
  Источник/формат:   gitverse.ru/gitverse/skills · SKILL.md
  Доступ:            OK (с дефектом frontmatter: allowedTools — неизвестное поле для Koda)
  Соответствие стеку:частично (TS+Rust — да; 4 параллельных субагента + gh CLI — нет)
  Конфликты:         требует subagents.enabled и инструмент `task`; проект на GitVerse, не GitHub (`gh`)
  Польза (ожидаемая):средняя — структура ревью (Correctness/Security/Quality/Performance + вердикт) ценна
  Стоимость внедрения:средняя (адаптация под один субагент + git diff вместо gh)
  Вердикт B:         доработать → собственная роль субагента `reviewer` (Этап E), без 4 агентов и gh
```
**Обоснование:** берём идею (многомерное ревью + severity + вердикт), но реализуем как роль
`<ws>/.koda/agents/reviewer.md` с read-only инструментами. Оригинальный SKILL.md не ставим:
неизвестный frontmatter-ключ + зависимость от `task`/`gh`.

### S4 `systematic-debugging`
```
S4 systematic-debugging
  Класс:             B
  Источник/формат:   gitverse.ru/gitverse/skills · SKILL.md
  Доступ:            OK (dangling refs на 3 файла)
  Соответствие стеку:да (стек-агностичная методология)
  Конфликты:         ссылается на отсутствующие файлы; упоминает `superpowers:*` скиллы (чужой неймспейс)
  Польза (ожидаемая):средняя — 4 фазы (root cause → pattern → hypothesis → fix) полезны при
                     отладке состояния таблиц (AC-1/AC-2) и debounce/innerHTML-перерисовки
  Стоимость внедрения:низкая (тело самодостаточно, refs не критичны)
  Вердикт B:         кандидат в пилот (при первом реальном баге состояния)
```

### S5 `codebase-design`
```
S5 codebase-design
  Класс:             B
  Источник/формат:   gitverse.ru/gitverse/skills · SKILL.md + DEEPENING.md + DESIGN-IT-TWICE.md
  Доступ:            OK
  Соответствие стеку:да (языко-агностичная методология проектирования модулей)
  Конфликты:         нет; `agents/openai.yaml` лишний для Koda (не ставим)
  Польза (ожидаемая):высокая — прямо ложится на T-17 TZ-excel-tables (дробление tables.ts на
                     tables-core / tables-state / DOM-адаптер) и T-18 («interface is the test surface»
                     → тесты tables-core без DOM)
  Стоимость внедрения:низкая (SKILL.md 114 строк + 2 reference)
  Вердикт B:         кандидат в пилот №1 (перекрывает изначальный P1)
```

### S6 `verification-before-completion`
```
S6 verification-before-completion
  Класс:             B
  Источник/формат:   gitverse.ru/gitverse/skills · SKILL.md
  Доступ:            OK
  Соответствие стеку:да (стек-агностично)
  Конфликты:         нет — усиливает существующее правило .kodarules (npm run build / cargo test)
  Польза (ожидаемая):высокая — «evidence before claims»: не заявлять AC выполненным без свежего
                     прогона `tsc` + `cargo test -p md-core` (прямо AC-8 ТЗ)
  Стоимость внедрения:низкая
  Вердикт B:         кандидат в пилот №2 (перекрывает изначальный P2)
```

### S7 `test-driven-development`
```
S7 test-driven-development
  Класс:             B
  Источник/формат:   gitverse.ru/gitverse/skills · SKILL.md
  Доступ:            OK (dangling ref: writing-good-tests.md)
  Соответствие стеку:частично (примеры на JS/TS; принцип применим к Rust md-core)
  Конфликты:         ref на отсутствующий файл; частично пересекается с S6
  Польза (ожидаемая):средняя — red-green-refactor для tables-core.ts (vitest, T-18) и md-core (cargo test)
  Стоимость внедрения:низкая
  Вердикт B:         кандидат в пилот №3 (для чистых модулей; не для DOM-слоя)
```

### S8 `webapp-testing`
```
S8 webapp-testing
  Класс:             B
  Источник/формат:   gitverse.ru/gitverse/skills · SKILL.md
  Доступ:            OK (dangling refs: scripts/, examples/)
  Соответствие стеку:нет — Python + Playwright, «local web applications»; mdedit — Tauri desktop
  Конфликты:         §6.4 ТЗ: зависимости только с согласия; проект выбрал vitest+jsdom (T-18), не Playwright
  Польза (ожидаемая):низкая/нулевая
  Стоимость внедрения:высокая (Python-рантайм + Playwright + scripts/)
  Вердикт B:         отклонить
```
**Обоснование:** проект уже зафиксировал стратегию тестирования фронта — vitest + jsdom (T-18).
Playwright для desktop-WebView2 не даёт выигрыша, а требует внешних зависимостей и скриптов,
которых в каталоге скилла нет.

### S9 `hindsight-docs`
```
S9 hindsight-docs
  Класс:             B
  Источник/формат:   gitverse.ru/gitverse/skills · SKILL.md
  Доступ:            OK
  Соответствие стеку:нет
  Конфликты:         скилл несёт документацию стороннего продукта (Hindsight), а не методологию
  Польза (ожидаемая):нулевая — исходная гипотеза («актуализация README/KODA.md») не подтвердилась
  Стоимость внедрения:—
  Вердикт B:         отклонить
```

---

## 3. Финализация Класса A (собственные скиллы P1–P3)

**Результат: P1–P3 отклоняются как дубли.** Причина — принцип «не плодить дубли» (§8, R8) и
запрет AC-5 на дублирование тел. Их назначение полностью покрывается уже готовыми S-скиллами:

| Изначальный P | Покрывается | Вывод |
|---|---|---|
| P1 `mdedit-tables-core` (процедура выноса логики таблиц) | **S5** `codebase-design` (методология deep module) + `TZ-excel-tables.md` §3, §4.5, §6 (проектная конкретика) | отклонён как дубль |
| P2 `mdedit-verify` (чек-лист AC → tsc → cargo test) | **S6** `verification-before-completion` + `.kodarules` (команды проекта) | отклонён как дубль |
| P3 `mdedit-rust-core` (TDD для md-core) | **S7** `test-driven-development` + существующие тесты `md-core/src/lib.rs` | отклонён как дубль |

**Правило разделения:** методология — в скилле (переиспользуемо, ставится из каталога);
факты проекта (команды, ограничения, пути) — в `.kodarules`/`KODA.md`/`tasks/`. Дублировать
одно в другое запрещено.

Новых собственных скиллов на данном этапе **не создаём**. Если пилот покажет пробел, не
покрытый S-скиллами, — только тогда создаём форк (вердикт «доработать»).

---

## 4. Предварительные вердикты (сводка)

| # | Скилл | Вердикт B | Действие |
|---|---|---|---|
| S1 | typescript-advanced-types | **отклонить** | — |
| S2 | frontend-api-integration-patterns | **отклонить** | — |
| S3 | review | **доработать** | роль `reviewer` (Этап E) |
| S4 | systematic-debugging | кандидат в пилот | пилот при первом баге состояния |
| S5 | codebase-design | **кандидат в пилот №1** | пилот на T-17 (`TZ-excel-tables`) |
| S6 | verification-before-completion | **кандидат в пилот №2** | пилот на любой подзадаче с AC |
| S7 | test-driven-development | кандидат в пилот №3 | пилот на `tables-core.ts` (T-18) |
| S8 | webapp-testing | **отклонить** | — |
| S9 | hindsight-docs | **отклонить** | — |
| P1–P3 | собственные | **отклонить (дубли)** | заменены S5/S6/S7 |

**Итог:** из 12 кандидатов отклонены 7 (S1, S2, S8, S9, P1, P2, P3), доработать 1 (S3),
в пилот идут 3 (S5, S6, S7). Это и есть «адаптировать нужное, отбросить лишнее».

---

## 5. Следующий шаг — Этап C (пилоты)

Порядок (по приоритету разблокировки `TZ-excel-tables.md`):

1. **S5 `codebase-design`** — пилот-задача: спринт 0 ТЗ (T-17, дробление `tables.ts`).
   Замер: время до каркаса `tables-core.ts` + `tables-state.ts`, число итераций, готовность к T-18.
2. **S6 `verification-before-completion`** — пилот-задача: приёмка любой подзадачи по AC.
   Замер: число ложных «готово» до/после.
3. **S7 `test-driven-development`** — пилот-задача: первые тесты `tests/core/parse.test.ts` (T-18).
   Замер: время до зелёного набора, найденные red-green дефекты.

Условие интеграции (Этап D): после пилота — `koda skills install <каталог> --scope workspace`,
затем правка `.kodarules`/`KODA.md` краткой ссылкой-правилом (AC-5) и проверка `tsc` +
`cargo test -p md-core` (AC-8).

Пилоты **не запускаются** до решения пользователя (они меняют `crates/app/src/*` и `package.json`).
