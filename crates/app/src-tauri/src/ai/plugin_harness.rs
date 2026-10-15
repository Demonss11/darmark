//! Harness плагинов (ADR-0034/ADR-0026): валидация через `darmark-plugin test --json` с fallback на
//! `plugin_core::manifest` + `darmark-plugin-host --self-test`, а также создание/обновление/
//! удаление плагинов через существующий [`crate::plugins::host::PluginHost`].
//!
//! Порядок строго **запись → валидация → регистрация** (ADR-0034 §2.4): невалидный плагин не попадает
//! в реестр. Сеть/LLM живут в AI-модуле, CLI используется как dev-инструмент (сеть в него не
//! вводится). Безопасность id проверяется **до** записи (нет traversal/спецсимволов).
//!
//! **Обновление не затирает рабочий плагин:** предложенный контент валидируется во временном
//! каталоге **до** перезаписи реальных файлов; при `!ok` реальные файлы не трогаются (ADR-0034/ADR-0026).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::ai::types::AiValidationResult;
use crate::error::{CommandError, ErrorCode};

/// Версия схемы JSON-отчёта CLI задаётся в `plugin-core` (ADR-0034 §2.4): писатель
/// (`plugin-cli`) и читатель (harness) обязаны свериться с одним значением.
use plugin_core::REPORT_SCHEMA_VERSION;

/// Максимальная длина id плагина (защита от гигантских имён каталогов).
const MAX_PLUGIN_ID_LEN: usize = 64;

/// Host-side таймаут запуска CLI/`--self-test` (ADR-0034 §2.4): при превышении — `ok=false`.
const VALIDATION_TIMEOUT: Duration = Duration::from_secs(15);

/// Имя файла манифеста в каталоге плагина.
const MANIFEST_FILE: &str = "plugin.json";

/// Имя entry-файла, который пишет harness (совпадает с шаблоном `plugin-cli init`).
const ENTRY_FILE: &str = "main.lua";

/// Env-override и имя сайдкара CLI разработчика.
const CLI_ENV: &str = "DARMARK_PLUGIN_CLI";
const CLI_EXE: &str = "darmark-plugin.exe";
/// Env-override и имя child-хоста песочницы.
const HOST_ENV: &str = "DARMARK_PLUGIN_HOST_EXE";
const HOST_EXE: &str = "darmark-plugin-host.exe";

/// Безопасный id плагина: непустой, ≤ [`MAX_PLUGIN_ID_LEN`], только `[a-z0-9-_]`.
///
/// Отклоняет traversal (`..`), разделители пути и спецсимволы — до любой записи на диск.
pub fn validate_plugin_id(id: &str) -> Result<(), CommandError> {
    let safe = !id.is_empty()
        && id.len() <= MAX_PLUGIN_ID_LEN
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_');
    if safe {
        Ok(())
    } else {
        Err(CommandError::new(
            ErrorCode::AiPluginInvalid,
            format!(
                "недопустимый id плагина: {id:?} (ожидается [a-z0-9-_], ≤ {MAX_PLUGIN_ID_LEN})"
            ),
        ))
    }
}

/// `manifest.id` обязан совпадать с параметром `id` (create/update): иначе плагин записался
/// бы под чужим/неожиданным id. Ошибка → `ai_plugin_invalid` **до** любой записи на диск.
pub fn validate_manifest_id(manifest: &Value, id: &str) -> Result<(), CommandError> {
    match manifest.get("id").and_then(Value::as_str) {
        Some(manifest_id) if manifest_id == id => Ok(()),
        Some(manifest_id) => Err(CommandError::new(
            ErrorCode::AiPluginInvalid,
            format!("id манифеста {manifest_id:?} не совпадает с id {id:?}"),
        )),
        None => Err(CommandError::new(
            ErrorCode::AiPluginInvalid,
            "в манифесте нет строкового поля id",
        )),
    }
}

/// Лимиты размера **до** записи на диск (ADR-0034 §2.4/ADR-0032): исходник `main.lua` и
/// сериализованный манифест. Нарушение → `ai_plugin_invalid` без записи файлов.
pub fn check_plugin_limits(manifest: &Value, source: &str) -> Result<(), CommandError> {
    if let Err(e) = plugin_core::manifest::validate_source_len(source.len() as u64) {
        return Err(CommandError::new(
            ErrorCode::AiPluginInvalid,
            format!("entry: {e}"),
        ));
    }
    let manifest_len = serde_json::to_vec(manifest)
        .map_err(|e| {
            CommandError::new(
                ErrorCode::AiPluginInvalid,
                format!("манифест не сериализуется: {e}"),
            )
        })?
        .len() as u64;
    let max = plugin_core::MAX_PLUGIN_SOURCE_BYTES;
    if manifest_len > max {
        return Err(CommandError::new(
            ErrorCode::AiPluginInvalid,
            format!("манифест больше {max} байт"),
        ));
    }
    Ok(())
}

/// Корень каталога плагинов: env `DARMARK_PLUGINS_DIR` → `%APPDATA%/darmark/plugins`.
///
/// Совпадает с `plugins_dir()` GUI-хоста, но без cfg-гейта: harness доступен и в тестах.
pub fn plugins_root() -> Result<PathBuf, CommandError> {
    if let Some(dir) = std::env::var_os("DARMARK_PLUGINS_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let base = std::env::var_os("APPDATA")
        .ok_or_else(|| CommandError::new(ErrorCode::AiIo, "APPDATA не задан"))?;
    Ok(PathBuf::from(base).join("darmark").join("plugins"))
}

/// Канонизирует корень белого списка; при неудаче — исходный путь.
///
/// `canonicalize` на Windows возвращает verbatim-путь (`\\?\…`), поэтому сравнивать
/// канонический путь надо с каноническим корнем. Если корень не канонизируется
/// (не существует) — исходный путь не совпадёт с verbatim-путём, т.е. ничего лишнего
/// не разрешаем (fail-closed).
#[cfg(windows)]
fn canonical_root(root: &Path) -> PathBuf {
    std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf())
}

/// Отказывает, если любой компонент пути **ниже** `root` — reparse point (symlink/junction).
///
/// `canonicalize` раскрывает ссылки, поэтому junction/symlink *внутри* разрешённого каталога
/// (например `allowed\junction\to\inside`) не виден на каноническом пути и containment его не
/// ловит. Обходим **исходный** путь от `root` (сам `root` доверенный — не проверяем) до листа.
/// Если путь лексически не под `root` (относительный/иная раскладка) — пропускаем: containment
/// по каноническому пути уже проверен вызывающим.
#[cfg(windows)]
fn reject_reparse_between(root: &Path, path: &Path) -> Result<(), CommandError> {
    let Ok(relative) = path.strip_prefix(root) else {
        return Ok(());
    };
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component);
        if is_reparse_point(&current) {
            return Err(CommandError::new(
                ErrorCode::AiPluginInvalid,
                "путь содержит symlink/junction — доступ запрещён",
            ));
        }
    }
    Ok(())
}

/// Отказывает, если `plugin.json`/`main.lua` внутри каталога — сами reparse point.
///
/// AC-MCP-12: symlink-файл манифеста/entry иначе читался бы вслед за ссылкой (эскейп за
/// пределы разрешённой области). `symlink_metadata` не следует за ссылкой, поэтому видит
/// reparse-атрибут самого файла. Отсутствие файла — не ошибка (обработается валидацией).
#[cfg(windows)]
fn reject_reparse_plugin_files(dir: &Path) -> Result<(), CommandError> {
    for name in [MANIFEST_FILE, ENTRY_FILE] {
        if is_reparse_point(&dir.join(name)) {
            return Err(CommandError::new(
                ErrorCode::AiPluginInvalid,
                format!("файл {name} — symlink/junction, чтение запрещено"),
            ));
        }
    }
    Ok(())
}

/// Проверяет, что путь лежит в разрешённом каталоге (`plugins/` или `temp/`).
///
/// Единая точка проверки путей для `plugin_validate`/`plugin_test` (TZ-MCP R3/AC-MCP-12):
/// принуждение живёт здесь, а не в MCP-адаптере. Defense-in-depth: отказ на reparse point
/// (symlink/junction) → `canonicalize` (раскрывает `..` и ссылки) → `starts_with` по
/// каноническим корням → повторный обход компонентов пути от корня до листа и проверка файлов
/// манифеста/entry. Возвращает канонический путь, чтобы дальнейшая работа шла уже с проверенным
/// значением.
#[cfg(windows)]
pub fn ensure_plugin_path_allowed(path: &Path) -> Result<PathBuf, CommandError> {
    // Ссылку/junction отклоняем даже если её цель внутри белого списка: цель может быть
    // переопределена позже (TOCTOU), и запись уйдёт за пределы разрешённой области.
    // Проверяем исходный путь — `canonicalize` ссылку уже раскрывает.
    if is_reparse_point(path) {
        return Err(CommandError::new(
            ErrorCode::AiPluginInvalid,
            "путь является symlink/junction — доступ запрещён",
        ));
    }

    let canonical = std::fs::canonicalize(path).map_err(|e| {
        CommandError::new(
            ErrorCode::AiPluginInvalid,
            format!("путь не существует: {e}"),
        )
    })?;

    let plugins = plugins_root().ok();
    // Весь системный temp — осознанно (TZ-MCP R3): плагины валидируются из временных рабочих
    // каталогов (в т.ч. `temp_validate_dir`), поэтому сужение до префикса `darmark-ai-validate-*`
    // сломало бы валидацию. Принуждение — здесь, вне MCP; отказ на reparse ниже сохраняется.
    let temp = std::env::temp_dir();

    let within_plugins = plugins
        .as_ref()
        .is_some_and(|root| canonical.starts_with(canonical_root(root)));
    let within_temp = canonical.starts_with(canonical_root(&temp));
    if !within_plugins && !within_temp {
        return Err(CommandError::new(
            ErrorCode::AiPluginInvalid,
            "путь вне разрешённого каталога (plugins/ или temp/)",
        ));
    }

    // Обход всех компонентов от разрешённого корня до листа (finding #1): родительский
    // junction/symlink, ведущий внутрь разрешённой области, иначе прошёл бы containment.
    if let Some(root) = &plugins {
        reject_reparse_between(root, path)?;
    }
    reject_reparse_between(&temp, path)?;

    // Файлы манифеста/entry не должны быть ссылками (finding #2, AC-MCP-12).
    reject_reparse_plugin_files(&canonical)?;

    Ok(canonical)
}

/// Разбирает JSON-отчёт `darmark-plugin test --json`.
///
/// `None` — отчёту нельзя доверять: нет `schema_version` или неизвестный мажор → harness
/// уходит в fallback (ADR-0034 §2.4). Иначе — `ok`/`errors` (проваленные `checks[]`) и сырой отчёт.
pub fn parse_cli_report(stdout: &str) -> Option<AiValidationResult> {
    let value: Value = serde_json::from_str(stdout.trim()).ok()?;
    let schema = value.get("schema_version")?.as_u64()?;
    if schema != u64::from(REPORT_SCHEMA_VERSION) {
        return None;
    }
    let ok = value.get("ok").and_then(Value::as_bool)?;
    let errors = value
        .get("checks")
        .and_then(Value::as_array)
        .map(|checks| {
            checks
                .iter()
                .filter(|check| check.get("status").and_then(Value::as_str) == Some("fail"))
                .map(|check| {
                    let name = check.get("name").and_then(Value::as_str).unwrap_or("check");
                    let message = check.get("message").and_then(Value::as_str).unwrap_or("");
                    format!("{name}: {message}")
                })
                .collect()
        })
        .unwrap_or_default();
    Some(AiValidationResult {
        ok,
        errors,
        report: Some(value),
    })
}

/// Валидация манифеста и исходника **без** CLI (fallback, ADR-0034/ADR-0026): те же проверки через
/// `plugin_core::manifest`. Возвращает понятные ошибки (пусто — валидно).
///
/// Только для тестов: продовый путь статической валидации идёт через [`validate_plugin_static`]
/// (TZ-MCP R4/AC-MCP-6), поэтому в не-тестовой сборке функция не нужна.
#[cfg(test)]
pub fn manifest_errors(manifest: &Value, source: &str) -> Vec<String> {
    let mut errors = Vec::new();
    let manifest_check = serde_json::to_vec(manifest)
        .map_err(|e| e.to_string())
        .and_then(|bytes| {
            plugin_core::manifest::Manifest::from_json(&bytes)
                .map(|_| ())
                .map_err(|e| e.to_string())
        });
    if let Err(message) = manifest_check {
        errors.push(format!("manifest: {message}"));
    }
    if let Err(e) = plugin_core::manifest::validate_source_len(source.len() as u64) {
        errors.push(format!("entry: {e}"));
    }
    errors
}

/// Полная валидация каталога плагина: CLI `test --json`, при недоступности/недоверии —
/// fallback (`manifest` + `plugin-host --self-test`). Без паник: всё кодируется в результат.
pub fn validate_plugin(dir: &Path) -> AiValidationResult {
    if let Some(cli) = resolve_sidecar(CLI_ENV, CLI_EXE) {
        let mut command = Command::new(cli);
        command.arg("test").arg(dir).arg("--json");
        if let Ok(output) = run_with_timeout(command, VALIDATION_TIMEOUT) {
            if let Some(result) = parse_cli_report(&output.stdout) {
                return result;
            }
        }
    }
    fallback_validate(dir)
}

/// Статическая валидация каталога плагина: манифест + entry **без** песочницы.
///
/// Единая точка проверки «манифест/entry» (TZ-MCP R4/AC-MCP-6): переиспользуется
/// fallback-валидацией и MCP-адаптером `plugin_validate`, поэтому логика не дублируется.
/// Без паник: отсутствие/невалидность файлов кодируются в `errors`.
pub fn validate_plugin_static(dir: &Path) -> AiValidationResult {
    let mut errors = Vec::new();

    match std::fs::read(dir.join(MANIFEST_FILE)) {
        Ok(bytes) => {
            if let Err(e) = plugin_core::manifest::Manifest::from_json(&bytes) {
                errors.push(format!("manifest: {e}"));
            }
        }
        Err(e) => errors.push(format!("manifest: {e}")),
    }
    match std::fs::read_to_string(dir.join(ENTRY_FILE)) {
        Ok(source) => {
            if let Err(e) = plugin_core::manifest::validate_source_len(source.len() as u64) {
                errors.push(format!("entry: {e}"));
            }
        }
        Err(e) => errors.push(format!("entry: {e}")),
    }

    AiValidationResult {
        ok: errors.is_empty(),
        errors,
        report: None,
    }
}

/// Fallback-валидация (ADR-0034 §2.4): манифест/entry через `plugin_core` + песочница child-хостом.
fn fallback_validate(dir: &Path) -> AiValidationResult {
    // Статические проверки — общие с MCP-адаптером `plugin_validate` (R4/AC-MCP-6).
    let mut result = validate_plugin_static(dir);
    if !result.ok {
        return result;
    }

    // Песочницу запускаем только при валидных манифесте/entry: без валидного входа она
    // бессмысленна (как и в `plugin-cli`).
    let source_path = dir.join(ENTRY_FILE);
    match resolve_sidecar(HOST_ENV, HOST_EXE) {
        Some(host) => {
            let mut command = Command::new(host);
            command.arg("--self-test").arg(&source_path);
            match run_with_timeout(command, VALIDATION_TIMEOUT) {
                Ok(output) if output.success => {}
                Ok(output) => result
                    .errors
                    .push(format!("sandbox: {}", tail(&output.stderr))),
                Err(e) => result.errors.push(format!("sandbox: {}", e.message)),
            }
        }
        None => result
            .errors
            .push("sandbox: darmark-plugin-host не найден рядом с darmark.exe".to_string()),
    }
    result.ok = result.errors.is_empty();
    result
}

/// Записывает `plugin.json` + `main.lua` в каталог (создавая его).
pub fn write_plugin_files(dir: &Path, manifest: &Value, source: &str) -> Result<(), CommandError> {
    std::fs::create_dir_all(dir).map_err(io_error)?;
    let manifest_bytes = serde_json::to_vec_pretty(manifest).map_err(|e| {
        CommandError::new(
            ErrorCode::AiPluginInvalid,
            format!("манифест не сериализуется: {e}"),
        )
    })?;
    std::fs::write(dir.join(MANIFEST_FILE), manifest_bytes).map_err(io_error)?;
    std::fs::write(dir.join(ENTRY_FILE), source.as_bytes()).map_err(io_error)?;
    Ok(())
}

/// Уникальный временный каталог для валидации (не трогает реальный профиль).
fn temp_validate_dir(id: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "darmark-ai-validate-{}-{id}-{sequence}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// Резолвит сайдкар: env-override → рядом с `darmark.exe` → соседний каталог
/// (тесты: `target/debug/deps` ↔ `target/debug`). Образец — `Supervisor::resolve_child_exe`.
fn resolve_sidecar(env: &str, name: &str) -> Option<PathBuf> {
    if let Ok(path) = std::env::var(env) {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    let current = std::env::current_exe().ok()?;
    let dir = current.parent()?;
    let candidates = [
        dir.join(name),
        dir.parent().map(|p| p.join(name)).unwrap_or_default(),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

/// Запуск процесса с host-side таймаутом. По истечении — процесс снимается, `Err`.
///
/// stdout/stderr читаются **параллельно** ожиданию (отдельные потоки): иначе «болтливый»
/// вывод переполняет буфер пайпа, процесс блокируется на записи и ложно срабатывает
/// таймаут (ADR-0034 §2.4).
fn run_with_timeout(
    mut command: Command,
    timeout: Duration,
) -> Result<ProcessOutput, CommandError> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Без консольного окна у сайдкара (как Supervisor::start): GUI не должен мелькать окном.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command.spawn().map_err(process_io_error)?;
    let stdout_thread = std::thread::spawn({
        let pipe = child.stdout.take();
        move || read_pipe(pipe)
    });
    let stderr_thread = std::thread::spawn({
        let pipe = child.stderr.take();
        move || read_pipe(pipe)
    });

    let started = Instant::now();
    let outcome = loop {
        match child.try_wait() {
            Ok(Some(status)) => break WaitOutcome::Exited(status),
            Ok(None) => {
                if started.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    break WaitOutcome::TimedOut;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                break WaitOutcome::Failed(process_io_error(e));
            }
        }
    };
    // Дожидаемся чтецов: после выхода/kill пайпы закрываются, потоки завершаются.
    let stdout = stdout_thread.join().unwrap_or_default();
    let stderr = stderr_thread.join().unwrap_or_default();
    match outcome {
        WaitOutcome::Exited(status) => Ok(ProcessOutput {
            success: status.success(),
            stdout,
            stderr,
        }),
        WaitOutcome::TimedOut => Err(CommandError::new(
            ErrorCode::AiPluginInvalid,
            "таймаут проверки плагина",
        )),
        WaitOutcome::Failed(error) => Err(error),
    }
}

/// Исход ожидания процесса: выход, таймаут или ошибка опроса.
enum WaitOutcome {
    Exited(std::process::ExitStatus),
    TimedOut,
    Failed(CommandError),
}

/// Результат запуска сайдкара.
struct ProcessOutput {
    success: bool,
    stdout: String,
    stderr: String,
}

fn read_pipe(pipe: Option<impl std::io::Read>) -> String {
    let mut buffer = String::new();
    if let Some(mut pipe) = pipe {
        let _ = pipe.read_to_string(&mut buffer);
    }
    buffer
}

fn process_io_error(e: std::io::Error) -> CommandError {
    CommandError::new(ErrorCode::AiIo, format!("процесс проверки плагина: {e}"))
}

fn io_error(e: std::io::Error) -> CommandError {
    CommandError::new(ErrorCode::AiIo, e.to_string())
}

/// Каталог — reparse point (symlink/junction)? Defense-in-depth (ADR-0034 §2.4): запись в такой
/// каталог может увести файлы за пределы `plugins/`. Проверяем атрибут `FILE_ATTRIBUTE_REPARSE_POINT`.
#[cfg(windows)]
fn is_reparse_point(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    std::fs::symlink_metadata(path)
        .map(|meta| meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
        .unwrap_or(false)
}

/// Хвост вывода процесса для сообщения об ошибке (не раздуваем статусбар).
fn tail(text: &str) -> String {
    const MAX: usize = 400;
    let trimmed = text.trim();
    if trimmed.chars().count() <= MAX {
        return trimmed.to_string();
    }
    let mut tail: String = trimmed.chars().rev().take(MAX).collect();
    tail = tail.chars().rev().collect();
    format!("…{tail}")
}

/// Сводка ошибок валидации для кода `ai_plugin_invalid`.
fn errors_summary(errors: &[String]) -> String {
    if errors.is_empty() {
        "плагин не прошёл валидацию".to_string()
    } else {
        errors.join("; ")
    }
}

// ---------- команды IPC (ADR-0034, «Фаза 2») ----------

/// Валидирует плагин во временном каталоге (запись → проверка), не трогая реальный профиль.
#[tauri::command(async)]
pub fn ai_validate_plugin(
    id: String,
    manifest: Value,
    source: String,
) -> Result<AiValidationResult, CommandError> {
    validate_plugin_id(&id)?;
    let dir = temp_validate_dir(&id);
    write_plugin_files(&dir, &manifest, &source)?;
    let result = validate_plugin(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    Ok(result)
}

/// Создаёт плагин: запись → валидация → регистрация. Эмитит `ai:plugin_created`.
#[tauri::command(async)]
pub fn ai_create_plugin(
    app: tauri::AppHandle,
    id: String,
    manifest: Value,
    source: String,
    state: tauri::State<'_, crate::PluginState>,
) -> Result<crate::PluginInfo, CommandError> {
    let root = plugins_root()?;
    let info = create_plugin_at(&state.host, &root, &id, &manifest, &source, validate_plugin)?;
    emit_plugin_event(&app, "ai:plugin_created", &id);
    Ok(info)
}

/// Обновляет плагин: перезапись → валидация → `reload_plugin`. Эмитит `ai:plugin_updated`.
#[tauri::command(async)]
pub fn ai_update_plugin(
    app: tauri::AppHandle,
    id: String,
    manifest: Value,
    source: String,
    state: tauri::State<'_, crate::PluginState>,
) -> Result<crate::PluginInfo, CommandError> {
    let root = plugins_root()?;
    let info = update_plugin_at(&state.host, &root, &id, &manifest, &source, validate_plugin)?;
    emit_plugin_event(&app, "ai:plugin_updated", &id);
    Ok(info)
}

/// Удаляет плагин через существующий `PluginHost::remove`. Эмитит `ai:plugin_deleted`.
#[tauri::command(async)]
pub fn ai_delete_plugin(
    app: tauri::AppHandle,
    id: String,
    state: tauri::State<'_, crate::PluginState>,
) -> Result<(), CommandError> {
    delete_plugin_at(&state.host, &id)?;
    emit_plugin_event(&app, "ai:plugin_deleted", &id);
    Ok(())
}

/// Эмитит событие harness'а `{ plugin_id }` (best-effort: сбой не роняет команду).
fn emit_plugin_event(app: &tauri::AppHandle, event: &str, id: &str) {
    use tauri::Emitter;
    if let Err(e) = app.emit(event, serde_json::json!({ "plugin_id": id })) {
        eprintln!("не эмитить {event}: {e}");
    }
}

// ---------- ядро CRUD (без Tauri — тестируемо) ----------

/// Создание: безопасный id/манифест/лимиты → каталог не существует → запись → валидация →
/// регистрация.
///
/// Невалидный плагин откатывается (каталог удаляется), чтобы не блокировать повторную попытку.
/// Ошибка регистрации (в т.ч. `no_host`) тоже откатывает каталог — «сиротских» каталогов нет.
#[cfg(windows)]
pub fn create_plugin_at(
    host: &crate::plugins::host::PluginHost,
    root: &Path,
    id: &str,
    manifest: &Value,
    source: &str,
    validate: impl Fn(&Path) -> AiValidationResult,
) -> Result<crate::PluginInfo, CommandError> {
    validate_plugin_id(id)?;
    validate_manifest_id(manifest, id)?;
    check_plugin_limits(manifest, source)?;
    let dir = root.join(id);
    if dir.exists() {
        return Err(CommandError::new(
            ErrorCode::AiPluginExists,
            format!("плагин уже существует: {id}"),
        ));
    }
    write_plugin_files(&dir, manifest, source)?;
    let result = validate(&dir);
    if !result.ok {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(CommandError::new(
            ErrorCode::AiPluginInvalid,
            errors_summary(&result.errors),
        ));
    }
    match host.register_plugin_dir(&dir) {
        Ok(info) => Ok(info),
        Err(error) => {
            // Регистрация не удалась — каталог не оставляем (можно повторить создание).
            let _ = std::fs::remove_dir_all(&dir);
            Err(map_register_error(error))
        }
    }
}

/// Обновление: безопасный id/манифест/лимиты → каталог существует (и не reparse point) →
/// валидация предложенного контента во временном каталоге → перезапись реальных файлов →
/// `reload_plugin`.
///
/// **Рабочий плагин не затирается:** при `!ok` реальные файлы не трогаются (ADR-0034/ADR-0026).
#[cfg(windows)]
pub fn update_plugin_at(
    host: &crate::plugins::host::PluginHost,
    root: &Path,
    id: &str,
    manifest: &Value,
    source: &str,
    validate: impl Fn(&Path) -> AiValidationResult,
) -> Result<crate::PluginInfo, CommandError> {
    validate_plugin_id(id)?;
    validate_manifest_id(manifest, id)?;
    check_plugin_limits(manifest, source)?;
    let dir = root.join(id);
    if !dir.exists() {
        return Err(CommandError::new(
            ErrorCode::AiPluginNotFound,
            format!("плагин не найден: {id}"),
        ));
    }
    // Defense-in-depth (ADR-0034 §2.4): не пишем в symlink/junction-каталог (эскейп за `plugins/`).
    if is_reparse_point(&dir) {
        return Err(CommandError::new(
            ErrorCode::AiPluginInvalid,
            format!("каталог плагина — symlink/junction, запись запрещена: {id}"),
        ));
    }

    // Сначала валидируем предложенный контент во временном каталоге: реальные файлы не
    // трогаем до подтверждённого `ok` (иначе невалидный контент затирал бы рабочий плагин).
    let temp = temp_validate_dir(id);
    let result = match write_plugin_files(&temp, manifest, source) {
        Ok(()) => validate(&temp),
        Err(e) => {
            let _ = std::fs::remove_dir_all(&temp);
            return Err(e);
        }
    };
    let _ = std::fs::remove_dir_all(&temp);
    if !result.ok {
        return Err(CommandError::new(
            ErrorCode::AiPluginInvalid,
            errors_summary(&result.errors),
        ));
    }

    // Контент валиден — только теперь перезаписываем реальные файлы и перезагружаем.
    write_plugin_files(&dir, manifest, source)?;
    host.reload(id)
        .map_err(|error| CommandError::new(ErrorCode::AiPluginNotFound, error.message))?;
    host.list_plugins()
        .into_iter()
        .find(|info| info.id == id)
        .ok_or_else(|| {
            CommandError::new(
                ErrorCode::AiPluginNotFound,
                format!("плагин не найден: {id}"),
            )
        })
}

/// Удаление через существующий `PluginHost::remove`.
#[cfg(windows)]
pub fn delete_plugin_at(
    host: &crate::plugins::host::PluginHost,
    id: &str,
) -> Result<(), CommandError> {
    validate_plugin_id(id)?;
    host.remove(id)
        .map_err(|error| CommandError::new(ErrorCode::AiPluginNotFound, error.message))
}

/// Маппинг ошибки регистрации на коды IPC: «уже существует» → `ai_plugin_exists`,
/// «нет сайдкара-хоста» → `plugin` (проблема окружения, не контента), «не появился в
/// реестре» → `ai_plugin_register_failed`, иначе → `ai_plugin_invalid`.
#[cfg(windows)]
fn map_register_error(error: plugin_core::envelope::PluginError) -> CommandError {
    let code = match error.code.as_str() {
        "plugin_exists" => ErrorCode::AiPluginExists,
        "no_host" => ErrorCode::Plugin,
        "register_failed" => ErrorCode::AiPluginRegisterFailed,
        _ => ErrorCode::AiPluginInvalid,
    };
    CommandError::new(code, error.message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_ids_are_accepted() {
        for id in ["word-count", "my_plugin", "a1", "x"] {
            assert!(validate_plugin_id(id).is_ok(), "принят: {id}");
        }
    }

    #[test]
    fn unsafe_ids_are_rejected() {
        for id in [
            "",
            "..",
            "../evil",
            "a/b",
            "a\\b",
            "A",
            "with space",
            "спец",
            "id!",
            "a.b",
        ] {
            let err = validate_plugin_id(id).unwrap_err();
            assert_eq!(err.code, ErrorCode::AiPluginInvalid, "отклонён: {id:?}");
        }
        let too_long = "a".repeat(MAX_PLUGIN_ID_LEN + 1);
        assert_eq!(
            validate_plugin_id(&too_long).unwrap_err().code,
            ErrorCode::AiPluginInvalid
        );
    }

    #[test]
    fn cli_report_with_schema_version_is_trusted() {
        let stdout = r#"{
            "schema_version": 1,
            "path": "C:/tmp/p",
            "ok": false,
            "checks": [
                {"name": "manifest", "status": "ok", "message": "ok"},
                {"name": "sandbox", "status": "fail", "message": "on_activate упал"}
            ],
            "sandbox_logs": []
        }"#;
        let result = parse_cli_report(stdout).expect("отчёт со schema_version=1 доверенный");
        assert!(!result.ok);
        assert_eq!(result.errors, vec!["sandbox: on_activate упал".to_string()]);
        assert!(result.report.is_some());
    }

    #[test]
    fn cli_report_without_or_unknown_schema_falls_back() {
        // Нет schema_version → не доверяем.
        assert!(parse_cli_report(r#"{"ok": true, "checks": []}"#).is_none());
        // Неизвестный мажор → не доверяем.
        assert!(parse_cli_report(r#"{"schema_version": 2, "ok": true, "checks": []}"#).is_none());
        // Не JSON → не доверяем.
        assert!(parse_cli_report("не json").is_none());
    }

    #[test]
    fn fallback_manifest_validation_flags_errors() {
        let good = serde_json::json!({
            "id": "word-count",
            "name": "Word Count",
            "version": "1.0.0",
            "api_version": 1,
            "entry": "main.lua"
        });
        assert!(manifest_errors(&good, "function on_activate(ctx) end").is_empty());

        let bad_perm = serde_json::json!({
            "id": "word-count",
            "name": "Word Count",
            "version": "1.0.0",
            "api_version": 1,
            "entry": "main.lua",
            "permissions": ["quantum:teleport"]
        });
        let errors = manifest_errors(&bad_perm, "function on_activate(ctx) end");
        assert!(
            errors.iter().any(|e| e.contains("quantum:teleport")),
            "{errors:?}"
        );

        let bad_id = serde_json::json!({
            "id": "../evil",
            "name": "X",
            "version": "1.0.0",
            "api_version": 1,
            "entry": "main.lua"
        });
        assert!(!manifest_errors(&bad_id, "x").is_empty());
    }

    /// Единая точка статической валидации (TZ-MCP R4/AC-MCP-6): принимает валидный
    /// каталог и сообщает `manifest`/`entry`-ошибки при отсутствии файлов.
    #[test]
    fn static_validation_accepts_valid_plugin_and_reports_missing_files() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("p");
        let manifest = serde_json::json!({
            "id": "p",
            "name": "P",
            "version": "1.0.0",
            "api_version": 1,
            "entry": "main.lua"
        });
        write_plugin_files(&dir, &manifest, "function on_activate(ctx) end").unwrap();
        let valid = validate_plugin_static(&dir);
        assert!(valid.ok, "валидный плагин: {:?}", valid.errors);
        assert!(valid.errors.is_empty());

        let empty = root.path().join("empty");
        std::fs::create_dir(&empty).unwrap();
        let missing = validate_plugin_static(&empty);
        assert!(!missing.ok);
        assert!(
            missing.errors.iter().any(|e| e.starts_with("manifest:")),
            "{:?}",
            missing.errors
        );
        assert!(
            missing.errors.iter().any(|e| e.starts_with("entry:")),
            "{:?}",
            missing.errors
        );
    }

    #[test]
    fn write_plugin_files_roundtrip() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("p");
        let manifest = serde_json::json!({
            "id": "p",
            "name": "P",
            "version": "1.0.0",
            "api_version": 1,
            "entry": "main.lua"
        });
        write_plugin_files(&dir, &manifest, "function on_activate(ctx) end").unwrap();
        assert!(dir.join("plugin.json").is_file());
        assert_eq!(
            std::fs::read_to_string(dir.join("main.lua")).unwrap(),
            "function on_activate(ctx) end"
        );
    }

    #[test]
    fn manifest_id_must_match_param() {
        let manifest = serde_json::json!({
            "id": "p",
            "name": "P",
            "version": "1.0.0",
            "api_version": 1,
            "entry": "main.lua"
        });
        assert!(validate_manifest_id(&manifest, "p").is_ok());
        let err = validate_manifest_id(&manifest, "q").unwrap_err();
        assert_eq!(err.code, ErrorCode::AiPluginInvalid, "id != параметр id");

        let missing = serde_json::json!({"name": "P"});
        assert_eq!(
            validate_manifest_id(&missing, "p").unwrap_err().code,
            ErrorCode::AiPluginInvalid,
            "нет строкового id"
        );
    }

    #[test]
    fn source_and_manifest_limits_are_checked() {
        let manifest = serde_json::json!({
            "id": "p",
            "name": "P",
            "version": "1.0.0",
            "api_version": 1,
            "entry": "main.lua"
        });
        assert!(check_plugin_limits(&manifest, "function on_activate(ctx) end").is_ok());

        let max = plugin_core::MAX_PLUGIN_SOURCE_BYTES as usize;
        let big_source = "a".repeat(max + 1);
        assert_eq!(
            check_plugin_limits(&manifest, &big_source)
                .unwrap_err()
                .code,
            ErrorCode::AiPluginInvalid,
            "исходник сверх лимита"
        );

        let big_manifest = serde_json::json!({
            "id": "p",
            "name": "P",
            "version": "1.0.0",
            "api_version": 1,
            "entry": "main.lua",
            "description": "x".repeat(max + 1)
        });
        assert_eq!(
            check_plugin_limits(&big_manifest, "x").unwrap_err().code,
            ErrorCode::AiPluginInvalid,
            "манифест сверх лимита"
        );
    }

    #[cfg(windows)]
    mod windows_crud {
        use super::*;
        use crate::plugins::host::PluginHost;
        use crate::plugins::manager::PluginManager;
        use crate::plugins::settings::SettingsStore;
        use crate::plugins::views::PluginViews;
        use std::sync::{Arc, Mutex};

        /// Валидатор-заглушка: тесты CRUD не зависят от наличия сайдкаров.
        fn always_ok(_dir: &Path) -> AiValidationResult {
            AiValidationResult {
                ok: true,
                errors: Vec::new(),
                report: None,
            }
        }

        /// Хост без плагинов; плагин `id` выключен в настройках, чтобы не спавнить child.
        fn host_with_disabled(id: &str) -> PluginHost {
            let mut settings = SettingsStore::default();
            settings.set_plugin_enabled(id, false);
            PluginHost::new(
                PluginManager::default(),
                Arc::new(Mutex::new(PluginViews::new())),
                Arc::new(NoopServices),
                Arc::new(|| {}),
                Arc::new(Mutex::new(Vec::new())),
            )
            .with_settings(settings, None)
        }

        struct NoopServices;
        impl crate::plugins::HostServices for NoopServices {
            fn handle(
                &self,
                _method: &str,
                _args: Value,
            ) -> Result<Value, plugin_core::envelope::PluginError> {
                Err(plugin_core::envelope::PluginError::new(
                    "noop",
                    "нет услуг в тесте",
                ))
            }
        }

        fn manifest(id: &str) -> Value {
            serde_json::json!({
                "id": id,
                "name": id,
                "version": "1.0.0",
                "api_version": 1,
                "entry": "main.lua"
            })
        }

        const SOURCE: &str = "function on_activate(ctx) end\nfunction on_deactivate(ctx) end\n";

        #[test]
        fn create_update_delete_roundtrip_on_temp_dir() {
            let root = tempfile::tempdir().unwrap();
            let host = host_with_disabled("ai-test");

            let info = create_plugin_at(
                &host,
                root.path(),
                "ai-test",
                &manifest("ai-test"),
                SOURCE,
                always_ok,
            )
            .expect("создание");
            assert_eq!(info.id, "ai-test");
            assert!(root.path().join("ai-test").join("main.lua").is_file());

            // Повторное создание → exists.
            let err = create_plugin_at(
                &host,
                root.path(),
                "ai-test",
                &manifest("ai-test"),
                SOURCE,
                always_ok,
            )
            .unwrap_err();
            assert_eq!(err.code, ErrorCode::AiPluginExists);

            // Обновление.
            let updated = update_plugin_at(
                &host,
                root.path(),
                "ai-test",
                &manifest("ai-test"),
                SOURCE,
                always_ok,
            )
            .expect("обновление");
            assert_eq!(updated.id, "ai-test");

            // Удаление.
            delete_plugin_at(&host, "ai-test").expect("удаление");
            assert!(!root.path().join("ai-test").exists(), "каталог удалён");
            assert!(host.list_plugins().iter().all(|p| p.id != "ai-test"));

            // Повторное удаление → not_found.
            let err = delete_plugin_at(&host, "ai-test").unwrap_err();
            assert_eq!(err.code, ErrorCode::AiPluginNotFound);
        }

        #[test]
        fn update_missing_plugin_reports_not_found() {
            let root = tempfile::tempdir().unwrap();
            let host = host_with_disabled("nope");
            let err = update_plugin_at(
                &host,
                root.path(),
                "nope",
                &manifest("nope"),
                SOURCE,
                always_ok,
            )
            .unwrap_err();
            assert_eq!(err.code, ErrorCode::AiPluginNotFound);
        }

        #[test]
        fn invalid_validation_rolls_back_create() {
            let root = tempfile::tempdir().unwrap();
            let host = host_with_disabled("bad");
            let err = create_plugin_at(
                &host,
                root.path(),
                "bad",
                &manifest("bad"),
                SOURCE,
                |_dir| AiValidationResult {
                    ok: false,
                    errors: vec!["sandbox: упал".to_string()],
                    report: None,
                },
            )
            .unwrap_err();
            assert_eq!(err.code, ErrorCode::AiPluginInvalid);
            assert!(
                !root.path().join("bad").exists(),
                "невалидный плагин откатывается"
            );
        }

        #[test]
        fn register_error_codes_are_separated() {
            use plugin_core::envelope::PluginError;
            let cases = [
                ("plugin_exists", ErrorCode::AiPluginExists),
                ("no_host", ErrorCode::Plugin),
                ("register_failed", ErrorCode::AiPluginRegisterFailed),
                ("invalid_plugin", ErrorCode::AiPluginInvalid),
            ];
            for (code, expected) in cases {
                let err = map_register_error(PluginError::new(code, "msg"));
                assert_eq!(err.code, expected, "код для {code}");
            }
        }

        #[test]
        fn update_does_not_touch_files_when_proposed_content_invalid() {
            let root = tempfile::tempdir().unwrap();
            let host = host_with_disabled("keep");
            let dir = root.path().join("keep");
            write_plugin_files(&dir, &manifest("keep"), SOURCE).unwrap();
            let before = std::fs::read(dir.join("main.lua")).unwrap();

            let err = update_plugin_at(
                &host,
                root.path(),
                "keep",
                &manifest("keep"),
                "-- невалидный исходник",
                |_dir| AiValidationResult {
                    ok: false,
                    errors: vec!["sandbox: упал".to_string()],
                    report: None,
                },
            )
            .unwrap_err();
            assert_eq!(err.code, ErrorCode::AiPluginInvalid);
            assert_eq!(
                std::fs::read(dir.join("main.lua")).unwrap(),
                before,
                "невалидный контент не должен затирать рабочие файлы"
            );
        }

        #[test]
        fn create_rolls_back_directory_when_registration_fails() {
            let root = tempfile::tempdir().unwrap();
            let host = host_with_disabled("dup");
            // Регистрируем «dup» из другого каталога → повторная регистрация даст `plugin_exists`.
            let other = tempfile::tempdir().unwrap();
            let other_dir = other.path().join("dup");
            write_plugin_files(&other_dir, &manifest("dup"), SOURCE).unwrap();
            host.register_plugin_dir(&other_dir)
                .expect("первичная регистрация");

            let err = create_plugin_at(
                &host,
                root.path(),
                "dup",
                &manifest("dup"),
                SOURCE,
                always_ok,
            )
            .unwrap_err();
            assert_eq!(err.code, ErrorCode::AiPluginExists);
            assert!(
                !root.path().join("dup").exists(),
                "каталог откатывается при ошибке регистрации"
            );
        }
    }

    /// «Болтливый» процесс: вывод больше буфера пайпа. Без параллельного чтения процесс
    /// заблокировался бы на записи и таймаут сработал бы ложно.
    #[cfg(windows)]
    #[test]
    fn run_with_timeout_reads_large_output_without_false_timeout() {
        let mut command = Command::new("cmd");
        command.args(["/C", "for /L %i in (1,1,20000) do @echo line-%i"]);
        let output =
            run_with_timeout(command, Duration::from_secs(30)).expect("процесс завершился");
        assert!(output.success, "процесс успешен");
        assert!(
            output.stdout.len() > 64 * 1024,
            "вывод прочитан полностью: {} байт",
            output.stdout.len()
        );
    }

    // ─── белый список директорий (TZ-MCP R3/AC-MCP-12) ─────────────────

    /// Создаёт junction (`mklink /J`, без прав администратора). `false` — окружение не
    /// позволило; вызывающий тест должен явно сообщить о пропуске.
    #[cfg(windows)]
    fn try_junction(link: &Path, target: &Path) -> bool {
        let status = Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .status();
        matches!(status, Ok(s) if s.success()) && std::fs::symlink_metadata(link).is_ok()
    }

    /// Создаёт symlink-файл (`mklink` без `/J`). Требует прав администратора или Developer
    /// Mode; `false` — недоступно (тест сообщает о пропуске).
    #[cfg(windows)]
    fn try_symlink_file(link: &Path, target: &Path) -> bool {
        let status = Command::new("cmd")
            .args(["/C", "mklink"])
            .arg(link)
            .arg(target)
            .status();
        matches!(status, Ok(s) if s.success()) && std::fs::symlink_metadata(link).is_ok()
    }

    #[cfg(windows)]
    #[test]
    fn ensure_plugin_path_allowed_rejects_path_outside_whitelist() {
        let err = ensure_plugin_path_allowed(Path::new("C:\\Windows\\System32"))
            .expect_err("абсолютный путь вне белого списка");
        assert_eq!(err.code, ErrorCode::AiPluginInvalid);
    }

    #[cfg(windows)]
    #[test]
    fn ensure_plugin_path_allowed_rejects_traversal() {
        // Поднимаемся из temp выше корня и уходим в `C:\Windows` — вне белого списка.
        // Избыточные `..` на корне игнорируются, поэтому тест устойчив к глубине temp.
        let mut escaped = std::env::temp_dir();
        for _ in 0..8 {
            escaped.push("..");
        }
        escaped.push("Windows");
        let err = ensure_plugin_path_allowed(&escaped).expect_err("path traversal");
        assert_eq!(err.code, ErrorCode::AiPluginInvalid);
    }

    #[cfg(windows)]
    #[test]
    fn ensure_plugin_path_allowed_accepts_path_inside_temp() {
        let dir = tempfile::tempdir().unwrap();
        let canonical = ensure_plugin_path_allowed(dir.path()).expect("temp разрешён");
        assert!(canonical.is_absolute(), "возвращается канонический путь");
    }

    #[cfg(windows)]
    #[test]
    fn ensure_plugin_path_allowed_resolves_traversal_inside_temp() {
        // `inner/..` лексически содержит `..`, но канонизируется в разрешённый `temp`.
        let base = tempfile::tempdir().unwrap();
        let inner = base.path().join("inner");
        std::fs::create_dir(&inner).unwrap();
        let via_parent = inner.join("..");
        ensure_plugin_path_allowed(&via_parent).expect("`..` внутри temp разрешён");
    }

    #[cfg(windows)]
    #[test]
    fn ensure_plugin_path_allowed_rejects_missing_path() {
        let missing = std::env::temp_dir().join(format!(
            "darmark-no-such-dir-{}-{}",
            std::process::id(),
            "abc"
        ));
        let err = ensure_plugin_path_allowed(&missing).expect_err("несуществующий путь");
        assert_eq!(err.code, ErrorCode::AiPluginInvalid);
    }

    #[cfg(windows)]
    #[test]
    fn ensure_plugin_path_allowed_rejects_junction() {
        let base = tempfile::tempdir().unwrap();
        let target = base.path().join("target");
        std::fs::create_dir(&target).unwrap();
        let link = base.path().join("link");
        if !try_junction(&link, &target) {
            eprintln!(
                "SKIP ensure_plugin_path_allowed_rejects_junction: mklink /J недоступен: {}",
                link.display()
            );
            return;
        }
        // Цель junction внутри temp, но сама ссылка — reparse point: должна быть отклонена.
        let err = ensure_plugin_path_allowed(&link).expect_err("junction отклонён");
        assert_eq!(err.code, ErrorCode::AiPluginInvalid);
    }

    #[cfg(windows)]
    #[test]
    fn ensure_plugin_path_allowed_rejects_parent_junction_outward() {
        // `allowed\junction\to\outside`: цель внутри temp (containment её пропустит), но
        // родительский компонент `junction` — reparse point → должен ловиться обходом.
        let base = tempfile::tempdir().unwrap();
        let allowed = base.path().join("allowed");
        let outside = base.path().join("outside");
        std::fs::create_dir(&allowed).unwrap();
        std::fs::create_dir(&outside).unwrap();
        let junction = allowed.join("junction");
        if !try_junction(&junction, &outside) {
            eprintln!(
                "SKIP ensure_plugin_path_allowed_rejects_parent_junction_outward: mklink /J недоступен: {}",
                junction.display()
            );
            return;
        }
        let child = junction.join("child");
        std::fs::create_dir(&child).unwrap();
        let err = ensure_plugin_path_allowed(&child).expect_err("родительский junction");
        assert_eq!(err.code, ErrorCode::AiPluginInvalid);
    }

    #[cfg(windows)]
    #[test]
    fn ensure_plugin_path_allowed_rejects_symlinked_manifest_file() {
        let base = tempfile::tempdir().unwrap();
        let dir = base.path().join("plugin");
        std::fs::create_dir(&dir).unwrap();
        let target = base.path().join("secret.json");
        std::fs::write(&target, b"{}").unwrap();
        let link = dir.join("plugin.json");
        if !try_symlink_file(&link, &target) {
            eprintln!(
                "SKIP ensure_plugin_path_allowed_rejects_symlinked_manifest_file: symlink недоступен: {}",
                link.display()
            );
            return;
        }
        let err = ensure_plugin_path_allowed(&dir).expect_err("symlink plugin.json отклонён");
        assert_eq!(err.code, ErrorCode::AiPluginInvalid);
    }

    #[cfg(windows)]
    #[test]
    fn ensure_plugin_path_allowed_rejects_symlinked_entry_file() {
        let base = tempfile::tempdir().unwrap();
        let dir = base.path().join("plugin");
        std::fs::create_dir(&dir).unwrap();
        let target = base.path().join("secret.lua");
        std::fs::write(&target, b"-- secret").unwrap();
        let link = dir.join("main.lua");
        if !try_symlink_file(&link, &target) {
            eprintln!(
                "SKIP ensure_plugin_path_allowed_rejects_symlinked_entry_file: symlink недоступен: {}",
                link.display()
            );
            return;
        }
        let err = ensure_plugin_path_allowed(&dir).expect_err("symlink main.lua отклонён");
        assert_eq!(err.code, ErrorCode::AiPluginInvalid);
    }

    #[cfg(windows)]
    #[test]
    fn ensure_plugin_path_allowed_rejects_reparse_entry_without_symlink_privilege() {
        // Детерминированная проверка finding #2 без прав на symlink: junction с именем
        // `main.lua` — тоже reparse point, и `reject_reparse_plugin_files` обязан его отклонить.
        let base = tempfile::tempdir().unwrap();
        let dir = base.path().join("plugin");
        let target = base.path().join("target");
        std::fs::create_dir(&dir).unwrap();
        std::fs::create_dir(&target).unwrap();
        let link = dir.join("main.lua");
        if !try_junction(&link, &target) {
            eprintln!(
                "SKIP ensure_plugin_path_allowed_rejects_reparse_entry_without_symlink_privilege: mklink /J недоступен: {}",
                link.display()
            );
            return;
        }
        let err = ensure_plugin_path_allowed(&dir).expect_err("reparse main.lua отклонён");
        assert_eq!(err.code, ErrorCode::AiPluginInvalid);
    }

    #[cfg(windows)]
    #[test]
    fn ensure_plugin_path_allowed_accepts_path_inside_plugins_root() {
        // Ветка `plugins_root()` (env-override) иначе не покрыта: temp-тесты её не задевают.
        let root = tempfile::tempdir().unwrap();
        let plugin_dir = root.path().join("my-plugin");
        std::fs::create_dir(&plugin_dir).unwrap();

        let previous = std::env::var_os("DARMARK_PLUGINS_DIR");
        std::env::set_var("DARMARK_PLUGINS_DIR", root.path());
        let result = ensure_plugin_path_allowed(&plugin_dir);
        match previous {
            Some(value) => std::env::set_var("DARMARK_PLUGINS_DIR", value),
            None => std::env::remove_var("DARMARK_PLUGINS_DIR"),
        }

        result.expect("путь внутри plugins_root разрешён");
    }

    #[cfg(windows)]
    #[test]
    fn ensure_plugin_path_allowed_accepts_root_and_normalized_layouts() {
        let temp = std::env::temp_dir();
        // Сам корень whitelist — разрешён.
        ensure_plugin_path_allowed(&temp).expect("сам корень temp разрешён");

        // Лишние разделители/`.`: канонизация нормализует путь.
        let dotted = temp.join(".").join("");
        ensure_plugin_path_allowed(&dotted).expect("нормализованная раскладка внутри temp");

        // Иная раскладка регистра: `canonicalize` приводит к реальному регистру.
        let upper = PathBuf::from(temp.to_string_lossy().to_uppercase());
        ensure_plugin_path_allowed(&upper).expect("регистр нормализуется канонизацией");
    }
}
