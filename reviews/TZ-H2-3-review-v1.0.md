# Отчёт по Фазе 3 (TZ-H2): манифесты, сканирование, настройки, permissions

Ревизия незакоммиченных правок рабочего дерева (HEAD `9b9b74b`, ветка `h2-2-1`). Область —
`crates/app/src-tauri/src/plugins/scan.rs` (новый), `settings.rs` (новый), `manager.rs`,
`supervisor.rs`, `mod.rs`, `tests.rs`. Изменения `.opencode/agents/code-reviewer.md` и `AGENTS.md`
в область не входят. Ревью выполнено агентом `code-reviewer` (read-only) + независимый прогон
гейта.

## 1. Гейт: факты

| Проверка | Результат |
|---|---|
| `cargo test -p darmark` | ✅ **30 passed, 0 failed** (включая `scan_finds_valid_and_reports_errors`, `permission_denied_is_handled_as_value`, `settings_*`, `loader_*`, `quarantine_after_three_failures`). Ненулевой код команды в PowerShell — артефакт stderr-перенаправления, не провал тестов. |
| `cargo clippy -p darmark --all-targets -- -D warnings` | ✅ без замечаний |
| `cargo fmt -p darmark -- --check` | ✅ чисто |

Гейт Фазы 3 зелёный, но тесты **не покрывают** перечитывание `.lua` с диска при `reload`
(см. §2.1) — поэтому зелёный гейт не подтверждает приёмку §1.2/§6.

> В сессии агента `shell` был запрещён, а `rust-analyzer` MCP падал на всех запросах; тесты/линт
> прогонялись отдельно в сессии-инициаторе — результаты выше фактические.

## 2. MAJOR / HIGH — до мержа

### 2.1. `reload()` не перечитывает `.lua` с диска — «Перезагрузить» отдаёт старую версию

`manager.rs:31,44,98,131-134`. `PluginRuntime` хранит `source: String`, полученный один раз в
`new()` из `DiscoveredPlugin.source`. `reload()` (строка 131, комментарий обещает «загрузить файл
заново») просто вызывает `start()`, а `start()` передаёт в `Supervisor::start(params, &self.source)`
**in-memory копию**. Директория плагина при этом теряется: `load_plugins`, принимая `discovered`,
использует только `id`, `source`, `permissions`; `discovered.dir` (`scan.rs:15`) никуда не
сохраняется. После `load_plugins` файловая система больше не читается.

**Риск:** прямое нарушение §4.7 ТЗ («Перезагрузка = terminate + respawn + **load с диска**») и
приёмки §1.2/§6: правка `word-count.lua` + «Перезагрузить» не вступит в силу до рестарта GUI.
Флагманский сценарий вехи H2 не работает.

**Фикс:** хранить в `PluginRuntime` директорию (`dir: PathBuf`) и перед `Supervisor::start`
перечитывать исходник с диска (с обязательным гейтом размера, см. §2.2), например через общий
хелпер `scan::read_source(dir, entry)`.

### 2.2. Чтение `plugin.json` и `.lua` до проверки размера — обход гейта размера, OOM хоста

`scan.rs:65-77`: `std::fs::read(&manifest_path)` (строка 66) и `std::fs::read(&entry_path)`
(строка 72) читают файл целиком, и только затем `validate_source_len(entry_bytes.len())`
(строка 74). Для `plugin.json` лимита нет вообще. `MAX_PLUGIN_SOURCE_BYTES` (`limits.rs:18`,
1 МиБ) защищает загрузку в Lua, но **не** выделение памяти на стороне GUI-хоста.

**Риск:** плагин с `main.lua` в гигабайты (или битым/гигантским `plugin.json`) → попытка
аллокации в процессе GUI → OOM/аварийное завершение до валидации.

**Фикс:** проверять `std::fs::metadata(path)?.len()` **до** чтения (или `File::open` +
`Read::take(max + 1)`); для манифеста ввести отдельный лимит (64–256 КБ).

### 2.3. `SettingsStore::load` читает `config.json` без ограничения размера

`settings.rs:56-64`: `std::fs::read(path)` читает конфиг целиком без проверки размера, затем
`serde_json::from_slice`. `serde_json` ограничивает рекурсию (стек не переполнится), но огромный
плоский JSON аллоцируется полностью.

**Риск:** `%APPDATA%/darmark/config.json` доступен любому процессу пользователя; гигантский файл
→ рост памяти/зависание при старте.

**Фикс:** до чтения проверять `metadata().len()` (разумный лимит 1–5 МиБ); при превышении —
`Self::default()` + запись в лог.

## 3. MEDIUM

### 3.1. Неатомарная запись конфига + молчаливый откат на дефолты → потеря настроек и согласия

`settings.rs:56-73`: `save()` пишет прямо в `config.json` (`std::fs::write`); `load()` трактует
**любую** ошибку чтения/разбора как пустые настройки и не различает NotFound и реальную IO-ошибку.
Прерванная запись (краш/`panic=abort`, нехватка места) оставит усечённый файл; следующий запуск
тихо подставит `Default`, а последующий `save` затрёт остатки.

**Риск:** потеря enable/disable, `granted_permissions` (согласие!) и recent files.

**Фикс:** атомарная запись (временный файл в том же каталоге + `std::fs::rename`, замена атомарна
на NTFS); NotFound отделять от прочих ошибок и логировать.

### 3.2. Синхронная запись в stdin child под watchdog — возможен неостанавливаемый висел GUI

`supervisor.rs:204-211,248-257`: ответ на host-call (`ToChild::Reply`) и любой `ToChild::Invoke`
пишутся синхронно (`write_to_child`) в том же потоке, что и `await_result`; таймеры проверяются
только в начале цикла. Если child перестанет читать stdin, буфер пайпа заполнится и `write_all`
заблокируется навсегда — watchdog не сработает. Достижимо: `get_document_range`
(`services.rs:53-71`) не ограничивает `len` и через `Reply` может вернуть до ~10 МБ; reader-поток
(F27) обслуживает только направление child→host.

**Риск:** недоверенный плагин вешает поток GUI-хоста (против F27/ADR-0021).

**Фикс:** ограничить размер `Reply` (например потолком `MAX_EVENT_FRAME_BYTES`) и/или капнуть
`len` в `get_document_range`; запись с контролем бюджета. Замечание унаследовано из Фазы 2, но
`supervisor.rs` в области ревью.

### 3.3. Дубликаты `manifest.id` молча перезаписываются в реестре

`manager.rs:201-203,271-283`: `register` делает `insert` по `id`, `load_plugins` регистрирует все
`report.plugins`. Два подкаталога с одинаковым `id` (имя каталога и `id` не сверяются) → один
плагин бесследно исчезает, а настройки по `id` начинают делиться.

**Фикс:** при коллизии не перезаписывать, а добавлять `ScanError` («дублирующийся id …»).

## 4. MINOR / NIT

- **`recent_files` без времени и без нормализации при `load`** (`settings.rs:42-44,102-107`):
  DESIGN_DOC §12 — «пути **+ время**, лимит 20»; реализовано `Vec<String>`, лимит/дедуп только в
  `push_recent_file`, не в `load`.
- **Сканирование следует симлинкам/junction** (`scan.rs:43-48`): `path.is_dir()` идёт по reparse
  points; junction создаётся без прав администратора. Мера: `symlink_metadata()` /
  `FILE_ATTRIBUTE_REPARSE_POINT` → пропуск.
- **Дублирование констант/дефолтов и устаревшие комментарии:** `MAX_DOCUMENT_BYTES`
  (`services.rs:18`) = `MAX_FILE_SIZE` (`lib.rs:22`); дефолты watchdog/mem продублированы в
  `manager.rs:58-60` и `supervisor.rs:67-69`; комментарии «`SettingsStore` появится в Фазе 3»
  (`services.rs:101`) и «Фаза 2» (`plugins/mod.rs:1`) уже неактуальны.
- **`plugin-proto/manifest.rs:167`:** `api_version == 0` принимается (проверка только
  `> HOST_API_VERSION`); по букве §7.2 верно, но `0` бессмыслен — стоит отвергать `< 1`.
- **`manifest.rs:253`:** `entry != ".."` избыточно после `!entry.contains("..")`.
- **`tests.rs:214-226`:** гейт `permission_denied` подтверждается косвенно (через Lua-фикстуру),
  прямого Rust-ассерта `err.code == "permission_denied"` из `serve_host_call` нет.
- **`SettingsStore::save`/`scan_plugins` в рантайме никем не вызываются** — каталог
  `%APPDATA%/darmark/` фактически не создаётся в Phase 3; закрытие хвоста TZ-H1 п.8 пока только
  на уровне API (создание каталога тестами). Нужна точка вызова.
- **`host.get_setting`/`set_setting`** всё ещё заглушки (`services.rs:102`) и не скоупаются по
  `plugin_id`; при реализации в Фазе 5 следить за изоляцией настроек между плагинами.

## 5. Проверено и подтверждено корректным

- **Path traversal закрыт:** `is_safe_id` (строчные/цифры/`-`/`_`) и `is_safe_entry` (`*.lua`,
  без `/`, `\`, `:`, `..`) вызываются в `Manifest::validate`, который отрабатывает **до**
  `dir.join(&manifest.entry)` (`scan.rs:67-71`). Namespace/drive-relative (`C:`) не пройдут.
- **Permissions соответствуют §4.4/§8:** `H2_PERMISSIONS` = `document:read|write`,
  `view:create|modify`, `ui:statusbar`; `ui:menu|sidebar|toolbar` → `PermissionNotInH2`
  («не реализовано в H2»); `network`/`filesystem:*` → `UnsupportedPermission`; прочее → `Unknown`.
- **Отказ `api_version > HOST_API_VERSION`** есть и покрыт тестом (`manifest.rs:167-172`).
- **`permission_denied` как значение, не исключение:** `bindings.rs::defuse` возвращает
  `(nil, {code,message,permission?})`; хост отдаёт `ToChild::Reply{result: Err}`; фикстура
  `edit-denied.lua` + `tests.rs:214-226` это закрепляют.
- **Недоверенный ввод без паник:** в production-коде `scan.rs`/`settings.rs`/`manager.rs`/
  `supervisor.rs`/`mod.rs` нет `unwrap`/`expect`/`panic!`; битый config → `Default`, отсутствующий
  каталог плагинов → пустой отчёт, не-UTF8 `.lua`/битый JSON → `ScanReport.errors` без падения.
- **Изоляция отказа одного плагина:** ошибки сканирования собираются per-plugin, порядок
  детерминирован (`subdirs.sort()`).
- **Жизненный цикл:** ошибка invocation снимает supervisor и копит карантин (`manager.rs:161-177`),
  ручное включение сбрасывает карантин (`manager.rs:137-143`), N=3 проверен тестами.
- **Разделение слоёв:** `plugin-proto` без Tauri/mlua; `plugins` под `#[cfg(windows)]`; child —
  внешний exe; guard `cargo tree` в CI сохранён.

## 6. Резюме

- **Всего находок:** 10 (BLOCKER: 0, MAJOR/HIGH: 3, MEDIUM: 3, MINOR: 3, NIT: 1 группа).
- **Статус:** ⚠️ **Требует доработки до мержа.** Приоритет №1 — §2.1 (`reload` без чтения с
  диска): ломает приёмку §1.2/§4.7. Затем §2.2/§2.3 (гейты размера перед чтением).
- **Ключевые риски:** (1) «Перезагрузить» не подхватывает правки `.lua`; (2) OOM GUI-хоста на
  гигантских `main.lua`/`plugin.json`/`config.json`; (3) потеря настроек/согласия из-за
  неатомарного `save`; (4) потенциальное зависание на синхронной записи в stdin child.
