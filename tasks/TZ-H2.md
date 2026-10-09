# TZ-H2 — плагинная система darmark (Lua в дочернем процессе)

> **Статус:** к реализации (после закрытия H1).
> **Нормативная архитектура:** `docs/DESIGN_DOC.md` §6–§12, §14; решения — `docs/adr/0021-plugin-isolation-process.md` (D16), `docs/adr/0022-viewprovider-contract.md` (два тира).
> **Дорожная карта:** `docs/ROADMAP.md`, раздел H2. **Замеры/факты:** `docs/proto/FINDINGS.md` (F1–F38), `reviews/архив/TZ-proto-lua-host-*.md`.
> **Область:** новые продуктовые крейты `crates/plugin-proto`, `crates/plugin-host`; `crates/app/src-tauri/*` (Supervisor, permissions, события, settings); `crates/app/src/*` (плагинные view, менеджер); `crates/md-core/src/lib.rs` — **только** публичный вход санитайзера фрагмента + allowlist `data-p-*` (D7). `lua-proto`/`lua-rpc-spike` — источник переноса, в продукт не линкуются. (ROADMAP H2 п.1 называет крейт `plugin-api`; в H2 он разделён на `plugin-proto`/`plugin-host`.)
>
> Перед правкой прочитать целиком: `DESIGN_DOC` §6–§12, ADR-0021, ADR-0022, `docs/proto/FINDINGS.md` §1 (строки F16–F38).

---

## 0. Цель и рамки

Превратить доказанный в прототипе C-спайк (`crates/lua-rpc-spike`, изоляция отказов, stdio, Job Object, watchdog, range/delta, карантин — F16–F38) в **продуктовую плагинную систему**: пользователь кладёт `word-count.lua` в `%APPDATA%/darmark/plugins/word-count/`, приложение его видит и запускает; правка файла + «Перезагрузить» — работает новая версия, без пересборки (§1.2 DESIGN_DOC).

**Главный инвариант (D6/§14/ADR-0021):** GUI-хост `darmark` **не линкует `mlua`** и остаётся на `panic = "abort"`. Lua живёт **только** в отдельном child-бинарнике. Нарушение инварианта = провал size-gate и откат архитектурного решения.

**Не делаем в H2:**
- вкладки и UI нескольких документов (стор держит N, UI — 1; как в H1);
- сеть плагинов (`network` не предоставляется вообще, §8 DESIGN_DOC);
- подпись/реестр/магазин плагинов и `darmark plugin test` — это H3;
- Rust-undo (`EditOp` остаётся заглушкой), редактор с подсветкой синтаксиса, drag-resize разделителя;
- правки парсинга/рендера Markdown в `md-core` (трогаем **только** allowlist санитайзера под `data-p-*` и публичный вход санитизации фрагмента);
- `fs-watch` автоперезагрузку (только ручной «Перезагрузить»);
- новые npm-зависимости и любой JS-рантайм для плагинов;
- «общий child на все плагины» (отвергнут ADR-0021 §2.3: один `crash`/`spin` валит все — F31);
- события `view:scroll`/`view:focus`/`pane:resized` и permissions `ui:menu`/`ui:sidebar`/`ui:toolbar` (есть в DESIGN_DOC, но вне H2, см. §4.4/§4.5).

**Соответствие ROADMAP H2:** п.1 → Фазы 1–2 · п.2 → Фаза 3 · п.3 → Фаза 4 · п.4–5 → Фаза 5 · п.6 → Фаза 6.

---

## 1. Замороженные контракты

### 1.1. Унаследованные из H1 (нельзя ломать)
Полный список — `DESIGN_DOC` §13.2 и `tasks/архив/TZ-H1.md` §1. Ключевое: DOM-id (`editor`, `preview`, `panes`, `statusbar`, `app`, `file-label`, `chk-sync`, `chk-preview`, `stat-*`, `btn-*`, `toggle-*`), селекторы (`#preview .md-block[data-md]`, `.table-enhanced*`, `.inspect-*`), стартовый текст `«Добро пожаловать в darmark»`, debounce 120 мс, `ECHO_MS = 100`, `window.__errors`/`__xss`, ошибка лимита содержит `«МБ»`. Любая фаза H2 обязана держать `npm run test:e2e` (6 спеков) зелёным.

### 1.2. Новые контракты H2 (фиксируются и не меняются без мажора `api_version`)
- **`HOST_API_VERSION = 1`**; манифест с `api_version > HOST_API_VERSION` отвергается (DESIGN §7.2).
- **Имена host-функций** (§6.3 DESIGN_DOC) — строковый контракт между `plugin-host` и Lua; удаление/переименование = мажор. Новые функции H2 (`host.export_html`, см. §5 Фаза 5) добавляются в §6.3 DESIGN_DOC до реализации.
- **Каталоги (D8):** конфиг `%APPDATA%/darmark/config.json`; плагины `%APPDATA%/darmark/plugins/<id>/` (манифест + `.lua` + `settings.json`).
- **Атрибуты плагина:** `data-p-<pluginid>-*` добавляются в allowlist санитайзера (единственная правка allowlist ядра, D7/§11.2); именно на них опирается обратная маршрутизация (§9.4). Атрибут `class` **уже** входит в глобальный allowlist `md-core` и сохраняется как есть; `p-<pluginid>-` — зарезервированный префикс для классов плагина, ужесточение до «только `p-*`» в H2 **не выполняется** (см. §11.2 DESIGN_DOC).
- **Канонические строки границы изоляции:** `ISOLATION_NOTICE` и `DOCUMENT_ACCESS_NOTICE` (лежат в `lua-rpc-spike/src/lib.rs`, F38) — единый источник формулировок для UI/магазина.

---

## 2. Crate-структура и перенос из прототипа

Спайк доказал механику, но не годится в продукт как есть (ручная кодировка payload, `CMD_RUN_N` «общий child», harness-`main`-ы, `[profile.proto] unwind`). Product-коды — новые чистые крейты; проверенные блоки переносятся **как есть с тестами**, harness'ы остаются в спайке.

```
crates/
├── plugin-proto/   # НОВЫЙ, БЕЗ mlua. Линкуется и в GUI-хост, и в child.
│   └── src/
│       ├── frame.rs    # write_frame / read_frame / read_frame_host / read_frame_capped (F36) — из lua-rpc-spike/lib.rs
│       ├── envelope.rs # serde-конверт Request/Response/PluginError (§7.1 DESIGN_DOC): JSON-in-frame
│       ├── job.rs      # Job Object RSS/CPU/time + KILL_ON_JOB_CLOSE — из lua-rpc-spike/job.rs
│       ├── quarantine.rs # Quarantine N=3 (сейчас в lua-rpc-spike/src/lib.rs, F37) — вынести в отдельный модуль + юнит-тесты
│       ├── manifest.rs # Manifest/contributes/permissions + валидация (§6.4/§8)
│       └── notices.rs  # ISOLATION_NOTICE / DOCUMENT_ACCESS_NOTICE / permission_notices (F38) — из lua-rpc-spike/lib.rs
└── plugin-host/    # НОВЫЙ, СОДЕРЖИТ mlua. Отдельный child-бинарник (subsystem console, но запускается с CREATE_NO_WINDOW).
    └── src/
        ├── main.rs   # цикл: read_frame_host → dispatch → host-call обратно; EV-поток
        ├── sandbox.rs# Lua::new_with(STRING|TABLE|MATH|UTF8) + harden_base: load/loadfile/dofile/collectgarbage + rawget/rawset (D17) + print→host.log — из lua-proto/src/sandbox.rs
        ├── bindings.rs # регистрация таблицы host.* и md.* / json.* — часть в плагин-процессе, часть RPC к GUI-хосту
        └── limits.rs # НОВЫЕ константы: бюджеты watchdog (F33/F35), MAX_FRAME (из lua-rpc-spike/lib.rs, F36), лимит размера .lua.
                      # In-process лимиты lua-proto/src/limits.rs (memory-limit + instruction-hook, путь A/B, F9/F14) НЕ переносятся:
                      # в child их роль выполняют Job Object + watchdog (ADR-0021 §2.4).
```

**Правило границ:** `plugin-proto` **не** знает про Tauri и про mlua. `plugin-host` **не** знает про Tauri. Только `crates/app/src-tauri` (GUI-хост) знает про `plugin-proto` (транспорт/Job/карантин/манифест), но **не** про `plugin-host`/mlua (child спавнится как внешний exe, а не как зависимость crate).

**Работа Фазы 0:** создать два crate, добавить в `members` корневого `Cargo.toml`; `lua-proto` и `lua-rpc-spike` оставить в `members` до завершения переноса и затем убрать (или пометить как прототипные — решение фиксируется в Фазе 0). CI (`ci.yml`) собирает `plugin-proto`/`plugin-host` отдельно и **проверяет**, что `cargo tree -p darmark` не содержит `mlua`.

---

## 3. Сводная таблица фаз

| # | Срез | Область | Гейт |
|---|---|---|---|
| 0 | Crate-каркас `plugin-proto`/`plugin-host`; перенос frame/job/quarantine/notices + тесты; контракт конверта `Request/Response/Error`; фиксация `[profile.release]` для child | новые крейты | `cargo test -p plugin-proto -p plugin-host`, `cargo tree -p darmark` без `mlua` |
| 1 | `plugin-host`: sandbox (D17), `host.log`/`print`, цикл stdio, self-test round-trip на `hello.lua` | `plugin-host` | unit sandbox + `cargo run -p plugin-host` с фикстурой: `on_activate` вызван, sandbox голый (`os`/`io`/`require`/`load`/`coroutine`/`rawget` = nil) |
| 2 | `PluginSupervisor` в GUI-хосте: spawn child на плагин, stdio, Job Object, **прогресс-таймаут + абсолютный дедлайн**, карантин N=3, range/delta host-API к `DocumentStore` | `src-tauri`, `plugin-proto` | `cargo test -p darmark` (supervisor на `crash`/`hang`/`chatty`-фикстурах) + ручной запуск: `crash` не роняет GUI, карантин после 3 |
| 3 | Манифест + сканирование `plugins/` + валидация + `SettingsStore` (`%APPDATA%/darmark/`) + permissions (проверка на хосте до вызова) | `src-tauri`, `plugin-proto` | `cargo test -p darmark` (валидация манифеста, отказ `api_version`, `permission_denied`) |
| 4 | Событийная шина (notify-only + pull, коалесинг по `rev`) + плагинный тир-1 view + обратная маршрутизация `data-p-*` | `src-tauri`, `src/*` | `cargo test -p darmark` + e2e-спек `plugins` (view рендерится, клик→хендлер) |
| 5 | Эталонные плагины `word-count`/`export-html`/`format-selection` + менеджер плагинов в UI (вкл/выкл/перезагрузить/разрешения + notices F38) | `src/*`, `plugins/` | `npm run build` + e2e `plugins` + ручная перезагрузка без рестарта |
| 6 | `darmark --debug-plugin <path>` (stdio dev) + CI (сборка child, size-budget child) + ADR-0023 + README | `src-tauri`, `.github`, docs | CI зелёный; size-gate: GUI ≤ 6 МБ; `--debug-plugin` гоняет фикстуру из stdin |

**Гейт каждой фазы:** рабочее приложение + `cargo test -p md-core -p darmark` + `npm run build` + соответствующие e2e.

---

## 4. Ключевые контракты H2 (детали)

### 4.1. Конверт хост ↔ child (`plugin-proto/envelope.rs`)
Кадр = `u32 LE длина || JSON` (перенос `write_frame`/`read_frame`/`read_frame_host` с потолками `MAX_EVENT_FRAME_BYTES = 1 MiB`, `MAX_HOST_FRAME_BYTES = 16 MiB`, F36). От ручной кодировки спайка отказываемся в пользу serde-конверта (§7.1):

```rust
pub enum ToChild  { Invoke { id: u32, method: String, args: serde_json::Value },
                    Reply  { id: u32, result: Result<serde_json::Value, PluginError> } }
pub enum ToHost   { HostCall { id: u32, method: String, args: serde_json::Value },
                    Event    { kind: String, payload: serde_json::Value },   // ready/done/…; log — частный случай kind:"log"
                    Log      { level: String, message: String } }
pub struct PluginError {
  pub code: String,                       // permission_denied / timeout / crashed / protocol / lua_error
  pub message: String,
  pub permission: Option<String>,         // заполняется при code = "permission_denied" (§4.4/§8)
  pub data: Option<serde_json::Value>,    // опциональная диагностика
}
```
`ToHost` парсится **только fallible** (`serde_json::from_slice` → `PluginError{protocol}`), паники нет (F36). Нарушение протокола/кадр-сверх-лимита = отказ плагина, не краш GUI. (`ToHost::Log` — удобный частный случай `ToHost::Event{kind:"log"}`; при желании схлопывается в `Event`.)

### 4.2. Host-API — range/delta (обязательно по памяти, F22/F30/ADR-0021 §2.5)
```lua
host.get_document_len(doc_id)               -> integer
host.get_document_range(doc_id, start, len) -> string   -- ОСНОВНОЙ путь чтения
host.get_document_version(doc_id)           -> integer   -- == rev стора
host.apply_edit(doc_id, start, stop, text)  -- permission document:write; ведёт в DocumentStore.update → rev++
host.get_document_text(doc_id)              -> string    -- вспомогательный; guard-rail ≤ MAX_FILE_SIZE
host.log(level, message)
host.show_message(text)                     -- permission ui:statusbar
host.get_setting(key) / host.set_setting(key, value)  -- скоуп = только свой плагин (§12)
md.to_html(text, opts?) / md.to_html_mapped(text, opts?)  -- нативные, без RPC
json.encode(v) / json.decode(s)
```
`get_document_text` **не** основной путь: при «child на плагин» полная копия 10 МБ = 20.7 МиБ × N (F30); range/len = ~0.7 МиБ (F34). Supervisor обслуживает host-calls **асинхронно** (отдельный reader-поток), иначе блокируется на заполненном пайпе (F27).

### 4.3. Watchdog — два таймера (ADR-0021 §3, F33/F35)
- **Прогресс-таймаут** (`--progress-ms`): нет **любого** события от child за бюджет → `TerminateJobObject` → `PluginError{timeout}`. Сбрасывается каждым кадром child.
- **Абсолютный дедлайн invocation** (`--deadline-ms`): wall-clock на весь вызов, **не** сбрасывается событиями — ловит `chatty`-плагин, бесконечно дёргающий быстрые host-calls (F35).
Оба уже реализованы в `watchdog_parent.rs` спайка — переносятся как чистая функция-политика + интеграция в Supervisor. Значения по умолчанию фиксируются в Фазе 2 (референс: прогресс 300–500 мс, дедлайн 1500 мс из прогонов F33/F35).

### 4.4. Permissions (§8 DESIGN_DOC, проверка на хосте до вызова)
Разрешения манифеста → фильтр в Supervisor перед диспатчем host-call. Нет разрешения → `PluginError{code:"permission_denied", permission, message}` как **значение**, не Lua-исключение. Набор H2: `document:read`, `document:write`, `view:create`, `view:modify`, `ui:statusbar`. Permissions `ui:menu`/`ui:sidebar`/`ui:toolbar` (объявлены в §8 DESIGN_DOC, но вне H2) распознаются как **известные**, но отклоняются валидатором с сообщением «не реализовано в H2» (это не «неизвестное permission»). `filesystem:*`/`network` — **не реализуются**: декларация отвергается валидатором с явным сообщением (§8 DESIGN_DOC).

### 4.5. События (§9): notify-only + pull + коалесинг
Каталог H2: `document:changed{doc_id,rev}`, `document:opened/closed{doc_id,path}`, `command:invoked{command_id}`. Событие **не несёт текст**. По одному `doc_id` — коалесинг по `rev` (доставляется только последний за тик). Доставка в child — кадр `ToChild::Invoke{method:"event", args:{name,payload}}`. Подписка/`dispose` — на стороне плагина (`ctx.subscribe` → unsubscribe при деактивации). События `view:scroll`/`view:focus`/`pane:resized` (§9.1 DESIGN_DOC) в H2 **не реализуются** (отложены).

### 4.6. Плагинный тир-1 view + обратная маршрутизация (§5.3/§9.4, ADR-0022)
- Плагин с `contributes.views[].tier==1` даёт `HtmlViewProvider`: `createView(ctx, opts)` возвращает view, HTML для которого приходит из child (`host.set_view_content(view_id, html)`).
- HTML плагина проходит **ту же** санитизацию (md-core), что и preview (§11.1) — с расширенным allowlist `data-p-*` (Фаза 0/4; `class` уже разрешён глобально, см. §1.2). Для этого ядру нужен **публичный вход** санитизации фрагмента (`sanitize_html` сейчас приватная).
- Клик внутри view: делегированный слушатель контейнера → `{view_id, action, payload}` из `data-p-<pluginid>-action/payload` → Lua-хендлер → новый HTML/патч → `set_view_content`. **Единственный** канал «из DOM в плагин».
- `ViewContext` плагина строго уже: `document()/edit()/render()/status()/openExternal()/onDocument()` — без DOM.

### 4.7. Жизненный цикл и менеджер (§6.6)
`scan → validate(manifest) → spawn child → load(src) → on_activate(ctx) → [events/commands] → on_deactivate → terminate child`. Перезагрузка = terminate + respawn + load с диска (без рестарта приложения). Ошибка загрузки/`on_activate` → статус `failed` + сообщение, GUI жив. 3 падения подряд → карантин (`Quarantine`, F37) → автоотключение до ручного включения. UI менеджера (Фаза 5): список плагинов, тумблер вкл/выкл, «Перезагрузить», показ `permissions` + `permission_notices` (F38), статус (active/failed/quarantined).

---

## 5. Детализация фаз

### Фаза 0 — каркас и контракты
- Создать `crates/plugin-proto`, `crates/plugin-host`; в `members`. Зависимости: `mlua` **только** у `plugin-host`; `serde`/`serde_json` — у обоих; `windows`-FFI Job Object — в `plugin-proto` (перенос `job.rs`, `#[cfg(windows)]`).
- Перенести с тестами: `frame.rs` (F36-тесты уже есть в спайке), `quarantine.rs` (3 теста F37), `notices.rs` (F38).
- `envelope.rs`: конверт §4.1 + round-trip/property-тесты (в т.ч. укороченный/огромный кадр → ошибка, не panic).
- `manifest.rs`: структура манифеста (§6.4) + `validate()` (обязательные поля, `api_version ≤ HOST_API_VERSION`, неизвестное permission → ошибка, `.lua`-размер ≤ лимита).
- Release-профиль child: `[profile.release]` наследуется (opt-level="s", LTO, strip, `panic="abort"` — в child допустим, т.к. изоляция процессом).
- CI: job сборки `plugin-proto`/`plugin-host`; проверка `cargo tree -p darmark` без `mlua`.
- **Гейт:** `cargo test -p plugin-proto -p plugin-host` зелёный; `cargo tree -p darmark | Select-String mlua` пусто.

### Фаза 1 — child-хост (`plugin-host`)
- `sandbox.rs`: `Lua::new_with(STRING|TABLE|MATH|UTF8)` + `harden_base` — в `mlua` 0.12 флага `BASE` нет, базовая библиотека грузится **неявно** (F7), поэтому `new_with` перечисляет только эти четыре, а `harden_base` чистит уже загруженную базу: удалить `load`/`loadfile`/`dofile`/`collectgarbage` **и `rawget`/`rawset`** (D17); `print` → `host.log("info", …)` (F6).
- `bindings.rs`: `host.log`, `plugin_id`; `md.*`/`json.*` нативно (md-core линкуется в child, **не** в GUI-хост). (`lua-proto/src/host.rs` сейчас содержит только `host.log`+`print` — остальное новое.)
- `main.rs`: цикл `read_frame_host` → `ToChild::Invoke` (load/activate/event) → ответ `ToHost::Event{done}`; исходящие host-calls (`get_document_range` и т.п.) как `ToHost::HostCall{id}` с ожиданием `Reply`.
- Self-test: `cargo run -p plugin-host -- <hello.lua>` в режиме «эхо-хост» (встроенный `serve_host_call` из спайка, перенесённый в `plugin-host` под `#[cfg(test)]`/dev-флаг).
- **Гейт:** sandbox-тесты (все запрещённые глобалы nil); round-trip `on_activate`; лимит-тест (`md.to_html` работает).

### Фаза 2 — Supervisor в GUI-хосте
- `src-tauri/src/plugins/supervisor.rs`: `spawn_child(exe, plugin_id)` (Job Object, `CREATE_NO_WINDOW`, stdio-пайпы), reader-поток → канал; `invoke(plugin_id, method, args)` с прогресс-таймаутом + дедлайном; обработка `HostCall` → range/delta к `DocumentStore` (переиспользовать `state.rs`).
- Карантин per-plugin (перенос `Quarantine`), статусы плагина.
- IPC-команды (фасад для UI, не для плагинов): `list_plugins`, `set_plugin_enabled`, `reload_plugin` — **новые контракты H2** (в `DESIGN_DOC` §4.3 их нет; фиксируются здесь до Фазы 5); в Фазе 2 — внутренний API Supervisor + тесты.
- **Решение (зафиксировать):** где берётся путь child-exe. Продукт — `darmark-plugin-host.exe` рядом с `darmark.exe` (resolves из `std::env::current_exe`). Для dev — переменная окружения/override. Тесты Supervisor'а гоняют child из `target/debug`.
- **Гейт:** `cargo test -p darmark` на фикстурах `crash`/`hang`/`chatty`/`edit` (перенос `.lua` из спайка): GUI жив, карантин после 3, `chatty` снят по дедлайну, `apply_edit` поднимает `rev`.

### Фаза 3 — манифесты, сканирование, настройки, permissions
- `SettingsStore` (Rust, §12): `%APPDATA%/darmark/config.json` — enable/disable плагинов, согласие на permissions, recent files (20). Здесь же закрывается хвост `tasks/архив/TZ-H1.md` п.8 («каталог `%APPDATA%/darmark/` при появлении SettingsStore»).
- `scan(plugins_dir)`: подкаталоги с манифестом → `validate` → реестр.
- Permissions: диспатчер Supervisor сверяет `permissions` манифеста (Фаза 2 держит заглушку — здесь наполняется; известные-но-вне-H2 permissions отклоняются отдельным сообщением, см. §4.4).
- **Гейт:** `cargo test -p darmark` — валидация манифеста, отказ `api_version>1`, неизвестное permission, `permission_denied` для хоста без `document:write`.

### Фаза 4 — события + плагинный тир-1 view + маршрутизация
- `src-tauri/src/plugins/bus.rs`: EventBus (notify+pull, коалесинг по `rev`); источник — `DocumentStore` (rev++ в `update`) и shell-команды.
- TS: `pluginViews.ts` — реестр плагинных `HtmlViewProvider` поверх существующего `viewRegistry.ts` (тир-1, ADR-0022); рендер через `paneHost`/`pane-head` `view-switch` (задел из H1-F5).
- Обратная маршрутизация: делегат на контейнере view (`data-p-*`) → IPC → Supervisor → Lua-хендлер.
- md-core: **публичный вход** санитизации фрагмента (сейчас `sanitize_html` приватная) + allowlist `data-p-*` в нём + тесты (единственная правка ядра; `to_html_mapped` инварианты не трогать; классы `class` уже разрешены глобально).
- **Гейт:** `cargo test -p md-core -p darmark` + новый e2e-спек `plugins.feature`: плагин-вью появляется в панели, клик по `data-p-*-action` вызывает хендлер (детерминированный маркер в DOM/statusbar).

### Фаза 5 — эталонные плагины + менеджер UI
- `plugins/word-count` (`document:read`,`ui:statusbar` — §6.5 эталон), `export-html` (`document:read` → записать HTML через host-команду экспорта `host.export_html(text)` — **новая** host-функция H2, добавить в §6.3 DESIGN_DOC; идёт через нативный диалог хоста, как `save_as`, поэтому `filesystem:write` **не требуется**), `format-selection` (`document:write` — обёртки над выделением, аналог `formatActions.ts`).
- Менеджер плагинов в существующем `sidebar` (панель `plugins` — заглушка из H1): список, тумблер вкл/выкл, «Перезагрузить», блок `permissions` + `permission_notices` (F38), статус (active/failed/quarantined).
- **Гейт:** `npm run build` + e2e `plugins` + ручная проверка: положить `.lua` → виден; правка + «Перезагрузить» → новая версия; карантин отображается.

### Фаза 6 — dev-режим + CI + документация
- `darmark --debug-plugin <path>`: тот же конверт поверх **назначенного** stdio (запуск плагина без помещения в `%APPDATA%`; для отладки, §7.1 «Dev»).
- CI: артефакт `darmark-plugin-host.exe`, отдельный size-budget child (фиксируется в Фазе 6; референс F3/F20: child ≈ 430–500 КБ + Lua); GUI size-gate ≤ 6 МБ остаётся.
- ADR-0023 «Плагинный runtime: child-хост и Supervisor» (ссылка на ADR-0021 как основание), README «Плагины», API-референс под Lua 5.5 (§6.1.1: `global`, компактные массивы, внешние строки, `table.create`).
- **Гейт:** CI зелёный; `--debug-plugin` прогоняет фикстуру.

---

## 6. Гейт всей вехи H2

(Windows/PowerShell; `Select-String` — PS-команда.)

```text
cargo fmt -p md-core -p darmark -p plugin-proto -p plugin-host -- --check
cargo clippy -p md-core -p darmark -p plugin-proto -p plugin-host --all-targets -- -D warnings
cargo test  -p md-core -p darmark -p plugin-proto -p plugin-host
cargo tree -p darmark | Select-String mlua        # пусто — GUI-хост без mlua (D6/ADR-0021)
cd crates/app
npm ci
npm run build
npx tauri build --no-bundle
npm run test:e2e    # все прежние 6 спеков + новый plugins
# size-gate: target/release/darmark.exe ≤ 6 МБ (GUI без mlua)
```

Приёмка UX (§1.2): положить `word-count.lua` в `%APPDATA%/darmark/plugins/` → работает; правка + «Перезагрузить» без рестарта → новая версия; `crash`/`hang` плагина не роняют и не вешают GUI; 3 падения → карантин в менеджере.

---

## 7. Риски и меры

- **Протекание `mlua` в GUI-хост.** Риск: случайно добавить `plugin-host` как зависимость `src-tauri`. Мера: CI-проверка `cargo tree -p darmark` без `mlua` (Фаза 0).
- **Блокировка Supervisor на пайпе.** Запись большого ответа при нечитающем child вешает поток (F27). Мера: отдельный reader-поток + асинхронные host-calls + watchdog (Фаза 2).
- **Двойная ревизия двух бинарей.** `darmark.exe` и `darmark-plugin-host.exe` обязаны сходиться по `api_version`. Мера: общий `plugin-proto::HOST_API_VERSION`, child сверяет при handshake; релиз собирает оба из одного workspace.
- **Карантин ложносрабатывает** на плагины с дорогим первым вызовом. Мера: бюджеты watchdog из замеров (F33/F35), настраиваемые в `config.json`; карантин снимается вручную (тест `reset`).
- **Санитайзер vs `data-p-*`.** Расширение allowlist может ослабить XSS-защиту. Мера: префикс `data-p-<pluginid>-` (не произвольные `data-*`), запрет событий/`javascript:`/внешних ссылок; тесты на payload с `onerror`/`javascript:` в `data-p-*` (Фаза 4).
- **`apply_edit` из плагина vs echo-цикл редактора.** Правка плагина меняет `rev` → событие → подписчики. Мера: единый путь через `DocumentStore.update` (D5), эхо-защита как в редакторе (`ECHO_MS`); плагинные правки не должны зациклить `document:changed`.
- **Утечки view при перезагрузке.** Плагинный view обязан `dispose()` (unsubscribe + DOM) — иначе висят слушатели/меню в `body` (риск H1-F5). Мера: контракт `dispose()` (ADR-0022) + проверка e2e после перезагрузки.
- **Размер child'а при N плагинах.** Каждый child ≈ 0.6 МиБ commit + Lua-куча (F30). Мера: лимит RSS Job Object per-child; документ — только range/delta (не полная копия).
- **`noUnusedLocals`/strict** при росте TS (`pluginViews.ts`, менеджер) — держать `npm run build` зелёным на каждой фронтовой фазе.

---

## 8. Что переносим / что НЕ переносим из прототипа

| Переносим (готово, F-подтверждено) | Не переносим / переписываем |
|---|---|
| `frame.rs` (read/write + лимиты, F36) | ручная кодировка payload → serde-конверт (§4.1) |
| `job.rs` (Job Object RSS/CPU/time, KILL_ON_JOB_CLOSE, F28/F29) | `CMD_RUN_N` «общий child» (отвергнут ADR-0021 §2.3) |
| `Quarantine` N=3 (сейчас в `lua-rpc-spike/src/lib.rs`, F37) | harness-`main`-ы (`*-parent.rs`) остаются в спайке |
| `ISOLATION_NOTICE`/`DOCUMENT_ACCESS_NOTICE`/`permission_notices` (`lua-rpc-spike/src/lib.rs`, F38) | `[profile.proto] unwind` — product child на `abort` |
| политика watchdog (прогресс + дедлайн, F33/F35) | `serve_host_call`-заглушка → реальный range/delta к `DocumentStore` |
| sandbox `create`/`harden_base` (`lua-proto`) | + чистка `rawget`/`rawset` (D17, в прототипе не было) |
| — | in-process лимиты `lua-proto/src/limits.rs` (memory-limit + instruction-hook, путь A/B, F9/F14) — заменены Job Object + watchdog |

После переноса `lua-proto`/`lua-rpc-spike` помечаются прототипными (остаются в ветке/`members` до решения в Фазе 0; CI их не гоняет — как сейчас).
