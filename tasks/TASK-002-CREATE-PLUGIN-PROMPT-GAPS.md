# TASK-002 — Допатч промта «Создать плагин» (самодостаточность)

> **Статус:** к исполнению
> **Дата создания:** 16.10.2026
> **Источник:** анализ покрытия `docs/PLUGIN_API.md` промтом скилла (сабагент plugin-developer)
> **Зависит от:** TASK-004 → TASK-005 → TASK-008 (порядок: сначала удаление `add-view` и `summarize`
> и фолбэка, потом допатч промта) — чтобы править уже единственный источник
> **Связан с:** TASK-001 (идёт после и ссылается на промт как на норму для моделей)
> **Оценка:** ~10 строк текста в 1 источнике

---

## 1. Контекст

Кнопка «Создать плагин» **не зависит** от `docs/PLUGIN_API.md`: весь API плагина вшит в промт
скилла. Анализ покрытия нашёл 4 пробела, из них один функциональный.

| § PLUGIN_API | Раздел | Статус |
|---|---|---|
| §1 | манифест / `contributes` | частично: нет полей `commands`; про `statusbar`/`toolbar`/`settings` не сказано (GUI их не рендерит) |
| §2 | точки входа, `ctx` | полностью |
| §3 | host-функции | частично: нет лимита окна 1 МиБ, нет `[start, stop)`, нет `export_html: true/false` |
| §4 | события | полностью |
| §5 | Lua 5.5 | частично: неполный список запрещённых библиотек |
| §6 | CLI | не нужен (у пользователя CLI нет) |
| §7 | цикл окон | **нет — функциональный пробел** |

**Единственный источник промта:** `crates/app/src-tauri/src/ai/skills/mod.rs` →
`CREATE_PLUGIN_PROMPT` (`mod.rs:40`). Фронтовая копия `crates/app/src/panels/aiPanel/skills.ts`
(`FALLBACK_SKILLS[create-plugin]`) **удаляется** по решению владельца от 16.10.2026 (TASK-008,
вариант A) — правок во втором месте больше нет, и рассинхрон невозможен.

> Номера строк — по состоянию на 16.10.2026. TASK-004 сливает уникальное из `add-view` в
> `CREATE_PLUGIN_PROMPT` и сдвигает строки ниже шапки: искать правки по тексту, не по номеру.

---

## 2. Шаги

### Шаг 1. Правка A — шапка промта (`mod.rs:40`)

```
- (Lua 5.5 в песочнице; нет os/io/require/load, нет host.document/host.ui/host.view/host.commands)
+ (Lua 5.5 в песочнице; нет os/io/require/load/loadfile/dofile/coroutine/debug/package,
+  нет host.document/host.ui/host.view/host.commands)
```

### Шаг 2. Правка B — семантика host-функций

После строки `host.log(level, message), host.get_setting(key), …` внутри блока
«Доступен ТОЛЬКО этот API» (`mod.rs:65`):

```
- Смещения start/stop/len — байты UTF-8; apply_edit(doc_id, start, stop, text) заменяет полуинтервал
  [start, stop) на text.
- get_document_range за вызов отдаёт максимум 1 МиБ: большой документ читай ЦИКЛОМ ОКОН
  (len = get_document_len(id); pos = 0; пока pos < len — chunk = get_document_range(id, pos, 65536),
  pos = pos + #chunk; в конце table.concat).
- export_html: true — файл записан, false — пользователь отменил диалог.
```

### Шаг 3. Правка C — поля команды

Заменить строку про `contributes.commands` (`mod.rs:75`):

```
- Команда: объяви в contributes.commands [{"id": "<id>.<команда>", "title": "…", "keybinding": "Ctrl+Alt+W"}]
  (отдельный permission не нужен) и обрабатывай событие command:invoked {command_id}.
```

### Шаг 4. Правка D — запрет галлюцинаций манифеста

После блока `permissions — только из набора: …` (`mod.rs:78`):

```
- contributes.statusbar / toolbar / settings не добавляй: статусбар — только host.show_message,
  toolbar и settings GUI не рендерит.
```

### Шаг 5. Сборка и тесты

- [ ] `cd crates/app; npm run build` — tsc strict зелёный
- [ ] `cargo test -p darmark ai::tests::skills` — зелёный
- [ ] `cargo test -p darmark` — зелёный

---

## 3. Что НЕ переносить в промт

Лимит исходника 1 МиБ (сверяет валидатор), лимиты настроек (ключ/значение/файл),
`plugins/index.json`, таблица `data-p-*` (см. TASK-004), `table.create`/`global x` (модель этим не
пользуется), коалесцирование событий по `rev`.

## 4. Правило «не раздувать»

Правка B не должна заставлять модель циклить окна там, где достаточно `get_document_text`.
Формулировка оставляет оба пути; если эвалы `skill-creator` покажут регресс — сузить до
«больше 1 МиБ — цикл окон».

## 5. Вопрос о копиях — закрыт

Копий промта больше нет, поэтому синхронизировать нечего и правило синхронизации копий **не
заносится** в ADR-0042 §2.1 (инверсия зависимости была: ADR-0042 создаётся в TASK-001, который идёт
после TASK-002). Закрыто решениями владельца от 16.10.2026:

- `add-view` удаляется (TASK-004) — блок API перестаёт дублироваться между промтами;
- фолбэк `skills.ts` удаляется (TASK-008, вариант A) — копия во фронте исчезает.

ADR-0042 §2.1 фиксирует перечень из двух промтов — `create-plugin` и `debug-plugin` — как
**единственный** источник host-API нормы для моделей.

---

## 6. Приёмка

- [ ] Правки A–D внесены в `CREATE_PLUGIN_PROMPT` (`crates/app/src-tauri/src/ai/skills/mod.rs`)
- [ ] `rg -c "циклом окон" crates/app/src-tauri/src/ai/skills/mod.rs` → 1
- [ ] `rg -n "Ты — генератор Lua-плагинов" crates/` → ровно одно совпадение,
      `crates/app/src-tauri/src/ai/skills/mod.rs` (фронтовой копии нет; `skillBar.ts` держит только
      `id` скилла в `PLUGIN_SKILL_IDS`, не текст промта)
- [ ] Промт вырос не более чем на 10 строк
- [ ] `npm run build` и `cargo test -p darmark` зелёные