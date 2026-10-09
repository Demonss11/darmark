# ADR-0023. Плагинный runtime: child-хост и Supervisor

- **Статус:** **Принят**
- **Дата:** 07.10.2026
- **Основание:** ADR-0021 (изоляция процесса), ADR-0022 (контракт ViewProvider);
  реализация H2 — плагинная система (Фазы 0–6); факты — `docs/proto/FINDINGS.md` (F16–F42)
- **Связанные:** `docs/DESIGN_DOC.md` §6–§12, §14, §16; `docs/PLUGIN_API.md`;
  `crates/plugin-proto`, `crates/plugin-host`, `crates/app/src-tauri/src/plugins`

---

## 1. Контекст

ADR-0021 выбрал изоляцию плагина отдельным процессом (вариант C): GUI-хост не линкует `mlua`,
Lua живёт в дочернем процессе. H2 довёл это решение до продуктового runtime: пользователь
кладёт `word-count.lua` в `%APPDATA%/darmark/plugins/`, правит файл и нажимает «Перезагрузить»
(§1.2 DESIGN_DOC). Прототип доказал механику (transport F16/F26/F27, Job Object F28/F29,
watchdog F33/F35, карантин F37, range/delta F34, устойчивость протокола F36), но продуктовые
крейты, события, представления и UI надо было построить заново.

Нужна зафиксированная архитектура runtime: какие крейты, где проходит граница, как устроены
транспорт, надзор, жизненный цикл, события, представления, permissions и dev-режим.

---

## 2. Решение

### 2.1. Крейты и правило границ

| Крейт | Знает про | **Не** знает про | Роль |
|---|---|---|---|
| `md-core` | Markdown | Tauri, Lua, плагины | ядро `&str → String`; публичный `sanitize_fragment` |
| `plugin-proto` | транспорт/конверт/Job/карантин/манифест | Tauri, `mlua` | общий крейт хоста и child |
| `plugin-host` | `mlua`/Lua 5.5 | Tauri | child-бинарник `darmark-plugin-host.exe` |
| `crates/app/src-tauri` | `plugin-proto` | `plugin-host`/`mlua` | GUI-хост: Supervisor, события, представления, команды |

**Инвариант (D6/ADR-0021):** `cargo tree -p darmark` не содержит `mlua`; child спавнится как
внешний exe, а не как зависимость. Проверяется в CI.

### 2.2. Транспорт и конверт

- Транспорт — stdio с **явными пайпами**; child запускается с `CREATE_NO_WINDOW`.
- Кадр — `u32 LE длина || payload`; потолки `MAX_EVENT_FRAME_BYTES` (1 МиБ, child→host) и
  `MAX_HOST_FRAME_BYTES` (16 МиБ, host→child). Разбор входящего — **fallible**, нарушение
  протокола = отказ плагина, не паника (F36).
- Данные — serde-конверт `ToChild`/`ToHost`/`PluginError` (§4.1 DESIGN_DOC); от ручной
  кодировки прототипа отказались.

### 2.3. Supervisor — надзор за одним процессом

Один `Supervisor` на плагин (модель «child на плагин», F30/F31):
- **spawn + Job Object**: лимит памяти (RSS, `KILL_ON_JOB_CLOSE`) и грубый CPU/время-лимит;
- **два таймера invocation** (F33/F35): прогресс-таймаут (сбрасывается любым кадром child'а) и
  абсолютный дедлайн (не сбрасывается, ловит «chatty»); превышение → `TerminateJobObject`;
- **reader-поток** (child→host) в канал, **writer-поток** (host→child) из канала: основной поток
  не блокируется на заполненном пайпе (F27);
- **асинхронные host-call'ы**: `ToHost::HostCall` обслуживаются в цикле ожидания результата
  (range/delta, настройки, экспорт); время обслуживания host-call исключается из дедлайна
  (нативный диалог `export_html` не должен считаться «chatty»);
- **permissions** проверяются **до** host-call (`required_permission`); отказ возвращается
  значением `PluginError{permission_denied}`, а не Lua-исключением;
- в object-args инжектится `_plugin_id` (хост не доверяет плагину автора view/сообщения).

### 2.4. Жизненный цикл и карантин

`scan → validate(manifest) → spawn → load → on_activate(ctx) → [events/commands] →
on_deactivate → terminate`. Ошибка загрузки/активации → `failed` (GUI жив). Серия из 3 падений
подряд → **карантин** (автоотключение до ручного включения); успех сбрасывает серию.
Перезагрузка = `terminate + respawn + load с диска` без рестарта приложения.

### 2.5. События и представления

- **Событийная шина** (`bus.rs`): notify-only + pull, коалесинг `document:changed` по `rev` на
  `doc_id`; каталог H2 — `document:changed/opened/closed`, `command:invoked`. Событие не несёт
  текст; плагин читает окном `get_document_range`. Публикация — вне удержания `DocumentStore`,
  доставка — `pump` без сторожевых блокировок (дедлок-безопасность).
- **Тир-1 view** (`views.rs` + `pluginViews.ts`): плагин с `contributes.views[].tier==1`
  становится `HtmlViewProvider` в `viewRegistry` (ADR-0022); HTML приходит `host.set_view_content`,
  санитизируется `md_core::sanitize_fragment` (allowlist `data-p-*`, D7) и рендерится в контейнер.
  Обратная маршрутизация: `data-p-<plugin_id>-action/-payload` → `plugin_view_action` → `on_action`.
- **Команды** (`contributes.commands`) исполняются событием `command:invoked{command_id, doc_id}`;
  в H2 кнопки «Выполнить» в менеджере, палитра/тулбар — вне H2.

### 2.6. Хост-функции

Документ: `get_document_len`/`get_document_range` (основной путь чтения)/`get_document_version`/
`get_document_text`/`apply_edit` (`document:write`). View: `set_view_content` (`view:modify`).
Хост: `log`, `show_message` (`ui:statusbar`), `export_html` (нативный диалог, без `filesystem:write`),
`get_setting`/`set_setting` (задел; в H2 возвращают `not_implemented`). Нативно в child:
`md.to_html`/`to_html_mapped`, `json.encode/decode`.
Полный референс — `docs/PLUGIN_API.md`.

### 2.7. Dev-режим

`darmark --debug-plugin <path>` (debug-сборка): запускает `darmark-plugin-host --serve-plugin <path>`
с **унаследованным** stdio, не помещая плагин в `%APPDATA%`. Разработчик общается с child тем же
конвертом (host-call'ы обслуживает он сам). Это §7.1 «Dev»: тот же конверт поверх назначенного stdio.

### 2.8. Бюджеты размера

- GUI-хост (`darmark.exe`) ≤ **6 МБ** (D6) — CI size-gate; фактически ≈3.8 МиБ.
- Child (`darmark-plugin-host.exe`) — отдельный бюджет ≤ **1,5 МиБ** (референс F3/F20: Lua +
  каркас); фактически ≈0,86 МиБ. Артефакт child собирается в CI и выкладывается.

---

## 3. Последствия

**Положительные:**
- Сбой/зависание/OOM плагина не роняет и (при независимых вызовах) не вешает GUI; данные
  приложения не разделяются с плагином адресно.
- Плагинные представления используют тот же `HtmlViewProvider`/`ViewContext`, что и встроенный
  preview (ADR-0022): единый контракт, единая точка санитизации.
- «Открыл `.lua` — поправил — `Перезагрузить` — работает» без пересборки и без Lua в базе.

**Цена / ограничения (см. §7 FINDINGS):**
- Хост обязан оставаться тонким диспетчером; тяжёлая логика — в child.
- Синхронные Tauri-команды, доходящие до invoke плагина, помечены `async` (иначе блокируют
  main-поток и нативный диалог `export_html`); host-call может быть долгим — время его
  обслуживания исключается из дедлайна.
- Согласие на permissions пока не собирается (F39); `get_document_text` ограничен кадром (F40);
  единый `notify` даёт лишний `list_plugins` при смене view (F41).
- Постоянные настройки плагина (`get_setting`/`set_setting`, `contributes.settings`) не
  реализованы — возвращают `not_implemented`; `view:create` не гейтит регистрацию представления
  (декларативное разрешение).

---

## 4. Отклонённые альтернативы

- **In-process Lua** — отклонён ADR-0021 (F9/F10: ошибки из Rust-callback под `abort`).
- **«Общий child» на все плагины** — отклонён ADR-0021 §2.3 (F31: один сбой валит/блокирует все).
- **Сетевой/шифрованный транспорт, named pipes** — не нужны: stdio подтверждён в GUI-конфигурации
  (F26/F27).
- **Событие с текстом документа** — отклонено (§9.2): дублирование больших строк и потеря данных
  при backpressure; notify-only + pull.

---

## 5. Ссылки

- Плагинная система (Фазы 0–6), `docs/DESIGN_DOC.md` §6–§12/§14, `docs/adr/0021-*`, `docs/adr/0022-*`
- `docs/proto/FINDINGS.md` F16–F42, `docs/PLUGIN_API.md`
- Код: `crates/plugin-proto`, `crates/plugin-host`, `crates/app/src-tauri/src/plugins/*`
