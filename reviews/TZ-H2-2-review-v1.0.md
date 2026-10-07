# Отчёт по Фазе 2 (TZ-H2): Supervisor в GUI-хосте

Ревизия коммита `fd48072` («H2-2»). Область — `crates/app/src-tauri/src/plugins/*`,
`crates/app/src-tauri/src/state.rs` (`apply_edit`), `crates/plugin-host/src/bindings.rs`
(`host.crash`), `ci.yml`.

## 1. Гейт: факты

| Проверка | Результат |
|---|---|
| `cargo clippy -p darmark --all-targets -- -D warnings` | ✅ без замечаний |
| `cargo fmt -p darmark -- --check` | ✅ чисто |
| `cargo tree -p darmark \| Select-String mlua` | ✅ пусто (D6/ADR-0021 держится) |
| `cargo test -p darmark plugins::tests::` | ✅ 8/8 зелёные — **после** `cargo build -p plugin-host` (см. §4.5) |

Архитектура фазы выдержана: транспорт stdio, Job Object (`KILL_ON_JOB_CLOSE` + лимит памяти),
два таймера watchdog (прогресс + абсолютный дедлайн), range/delta host-API к `DocumentStore`,
карантин per-plugin. Разделение слоёв по §2 ТЗ: `plugin-proto` без Tauri/mlua, `plugins` в
`src-tauri` только под `#[cfg(windows)]`, child спавнится как внешний exe.

## 2. Что закрыто

- `supervisor.rs`: `spawn_child` (Job Object, `CREATE_NO_WINDOW`, пайпы), reader-поток → канал,
  `invoke` с прогресс-таймаутом и дедлайном, обслуживание `HostCall` в `await_result`,
  `resolve_child_exe` (env → рядом с exe → соседний каталог).
- `services.rs`: range/delta host-API поверх `DocumentStore`, `apply_edit` единым путём (D5),
  guard-rail `MAX_DOCUMENT_BYTES` на `get_document_text`.
- `manager.rs`: жизненный цикл `start/stop/reload`, карантин N=3, `list_plugins`/
  `set_plugin_enabled`/`reload_plugin` как внутренний API, `PluginStatus`.
- `state.rs`: `DocumentStore::apply_edit` (байтовый диапазон, эхо-защита `rev++` только при
  смене текста, проверка `char_boundary`) + `DocumentId::new`.
- `tests.rs`: фикстуры `crash`/`hang`/`chatty`/`edit`, permission_denied, карантин, manager.

## 3. High — до мержа/Фазы 4

### 3.1. `permission_denied` приходит в Lua исключением, а не значением

`crates/plugin-host/src/bindings.rs:50-52` (`host_err` → `mlua::Error::RuntimeError`),
применяется во всех `host.*` (строки 94, 105, 130, 141, 156, 169, 177, 180). Тест это
поведение **закрепляет**:

- `crates/app/src-tauri/src/plugins/tests.rs:151-155` — `assert_eq!(err.code, "lua_error")`.

Спека говорит обратное:

- `docs/DESIGN_DOC.md:430`: «Плагин без `document:write` получает
  `error { code = "permission_denied", permission = "document:write" }`, а **не** исключение Lua».
- TZ-H2 §4.4: «`PluginError{code:"permission_denied", …}` как **значение**, не Lua-исключение».

Сейчас `host.apply_edit` без permission роняет весь `on_activate` (callback прерывается,
`host.log` после вызова не выполняется). Хост-сторона (`supervisor.rs:278-285` →
`ToChild::Reply{result: Err}`) отдаёт ошибку как значение корректно — её портит именно binding.
**Фикс:** возвращать в Lua структурированную ошибку (например `nil, {code=…, permission=…}` или
таблицу-ошибку) вместо `host_err`; тест переписать на `permission_denied` + продолжение callback.

### 3.2. Отказ invocation в рантайме не меняет состояние плагина — «зомби»-supervisor

`crates/app/src-tauri/src/plugins/manager.rs:145-153`. Если `supervisor.invoke` вернёт ошибку
**после** успешного старта (дедлайн/прогресс/краш при событии), `Supervisor::terminate()` уже
убил child, но `PluginRuntime`:

- оставляет `self.supervisor = Some(мёртвый)`;
- не делает `quarantine.record_failure()`;
- не меняет `status` (остаётся `Active`).

Следующий `invoke` упрётся в закрытый stdin (`crashed`), UI/шина не узнают о падении.
Для Фазы 4 (EventBus дёргает `invoke` на каждом событии) это станет реальным багом.
**Фикс:** в `PluginRuntime::invoke` по `Err` — `self.supervisor = None`, `record_failure()`,
`Failed`/`Quarantined`.

## 4. Medium / Low

### 4.1. `host.crash` — недокументированная продуктовая host-функция (Medium)

`crates/plugin-host/src/bindings.rs:185-190`. Регистрируется **безусловно**, уезжает в релизный
child. Её нет в `DESIGN_DOC` §6.3, а TZ-H2 §1.2 требует заносить новые host-функции в §6.3 до
реализации. Плюс любой плагин может намеренно себя уронить.
**Фикс:** спрятать за `#[cfg(debug_assertions)]`/dev-флаг либо задокументировать.

### 4.2. Риск дедлока: host-call под удержанной блокировкой стора (Medium)

`crates/app/src-tauri/src/plugins/services.rs:33-36` берёт `Mutex<DocumentStore>` на каждый
host-call синхронно в потоке invocation. Стор в приложении — `manage(Mutex::new(DocumentStore))`
(`lib.rs:189`), не `Arc<Mutex<…>>`. Когда в Фазе 4 IPC-команда, держа `lock_store`, вызовет
`supervisor.invoke`, вложенный `handle` повторно возьмёт тот же `std::sync::Mutex` → deadlock
(мьютекс не реентрантный). Также тип состояния придётся сменить на `Arc<Mutex<DocumentStore>>`,
иначе `DocumentServices::new` не собрать.
**Мера:** зафиксировать инвариант «`invoke` никогда не вызывается под блокировкой стора».

### 4.3. Тесты молча используют устаревший child-бинарник (Low)

`crates/app/src-tauri/src/plugins/tests.rs:34-50`: `child_exe()` берёт любой существующий
`target/debug/darmark-plugin-host.exe` и собирает child **только если его нет**, а не если он
устарел. Воспроизведено: без `cargo build -p plugin-host` полный прогон `plugins::tests::` падал
на `quarantine_after_three_failures` (`timeout` вместо `crashed`); после пересборки — зелено.
То есть `cargo test -p darmark` не самодостаточен, вопреки doc-комментарию. CI не страдает
(шаг `build plugin-host` стоит прямо перед тестами), но локально это ловушка.
**Фикс:** проверять свежесть (mtime/hash) либо собирать всегда.

### 4.4. `poll_event` теряет признак «child умер» (Low)

`supervisor.rs:269-275`: `.try_recv().ok()` превращает `Disconnected` в `None` — неотличимо от
«кадров нет». Шине Фазы 4 это понадобится для детекта смерти child.
**Фикс:** возвращать `Result`/3-состояние.

### 4.5. Краш может быть классифицирован как `timeout` (Low)

`limits.rs:11` (прогресс 400 мс) + `supervisor.rs:218-224`. Разбор abort/WER на Windows иногда
дольше бюджета, тогда реальный краш даст `timeout`, а не `crashed` (карантин в обоих случаях, но
сообщение вводит в заблуждение). Комментарий `tests.rs:104-105` сам признаёт медлительность abort.

### 4.6. Прочее (Low)

- `supervisor.rs:288-291` `terminate()` не помечает supervisor мёртвым; повторный `invoke` пишет
  в закрытый stdin (перекрывается фиксом §3.2).
- `supervisor.rs:226` возможен один холостой `recv_timeout(0)`, если оба бюджета истекли на
  границе цикла (верхние проверки обычно гасят; некритично).
- `services.rs:96-98` `show_message`/`get_setting`/`set_setting` — заглушки (`Null`), ожидаемо до
  фаз 3/5.
- `manager.rs:172-174` `register` молча перезаписывает плагин с тем же id.

## 5. Что понравилось

- Разделение слоёв точно по §2 ТЗ; `plugins` под `#[cfg(windows)]`; child — внешний exe.
- Обработка недоверенного child-вывода fallible без паник (`frame.rs`, `envelope.rs`).
- `apply_edit` (`state.rs:237-263`) — эхо-защита (`rev++` только при реальной смене), проверка
  `char_boundary`, единый путь через стор.
- Watchdog разделён корректно: `wait = min(остаток прогресса, остаток дедлайна)`, прогресс
  сбрасывается каждым кадром, дедлайн — нет.
- `resolve_child_exe` с dev-override и поиском в `target/debug` (тесты из `deps`).

## 6. Вопросы, требующие решения

1. **§3.1:** исправлять контракт `permission_denied` сейчас (и переписать тест) или оставить
   долгом до Фазы 3 с отметкой в тесте?
2. **§3.2:** закрывать жизненный цикл invocation сейчас или отложить до Фазы 4, когда появится
   вызывающая сторона?
3. **§4.1:** `host.crash` — оставить в проде задокументированным или спрятать в dev-сборку?
