//! CLI разработчика плагинов darmark: проверка (`test`) и создание шаблона (`init`).
//!
//! Бинарник `darmark-plugin` — отдельный крейт, чтобы GUI-хост `darmark` не линковал mlua
//! (D6/ADR-0021): песочницу даёт `plugin-host` как библиотека.
//!
//! Коды выхода: `0` — успех/проверки пройдены, `1` — ошибка операции/проверок, `2` — ошибка
//! аргументов (неизвестная команда/флаг).

mod catalog;

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use catalog::{Catalog, CatalogEntry};
use plugin_core::manifest::Manifest;
use plugin_host::selftest;

/// Лимит размера `plugin.json` (совпадает с GUI-сканированием и самопроверкой child).
const MAX_MANIFEST_BYTES: u64 = 256 * 1024;

/// Версия схемы JSON-отчёта `test --json` (корневое поле `schema_version`).
///
/// Аддитивные поля не повышают мажор; harness AI доверяет отчёту только с этой версией
/// (нет поля / неизвестный мажор → fallback, ADR-0034 §2.4).
const REPORT_SCHEMA_VERSION: u32 = 1;

/// Имя файла манифеста в каталоге плагина.
const MANIFEST_FILE: &str = "plugin.json";

/// Шаблоны, встроенные в бинарник.
const TEMPLATE_PLUGIN_JSON: &str = include_str!("../templates/plugin/plugin.json");
const TEMPLATE_MAIN_LUA: &str = include_str!("../templates/plugin/main.lua");
const TEMPLATE_README: &str = include_str!("../templates/plugin/README.md");

/// Статус одной проверки.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CheckStatus {
    Ok,
    Fail,
}

/// Одна проверка плагина.
#[derive(Debug, Clone)]
struct Check {
    name: String,
    status: CheckStatus,
    message: String,
}

impl Check {
    fn ok(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Ok,
            message: message.into(),
        }
    }

    fn fail(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Fail,
            message: message.into(),
        }
    }
}

/// Отчёт команды `test`.
#[derive(Debug, Clone)]
struct TestReport {
    path: PathBuf,
    checks: Vec<Check>,
    sandbox_logs: Vec<String>,
}

impl TestReport {
    fn ok(&self) -> bool {
        self.checks.iter().all(|c| c.status == CheckStatus::Ok)
    }

    fn failed(&self) -> usize {
        self.checks
            .iter()
            .filter(|c| c.status == CheckStatus::Fail)
            .count()
    }
}

/// Действие CLI, разобранное из аргументов.
enum Action {
    Test {
        path: PathBuf,
        json: bool,
        verbose: bool,
    },
    Init {
        dir: PathBuf,
        id: Option<String>,
        name: Option<String>,
        force: bool,
    },
    List {
        repo: Option<PathBuf>,
        json: bool,
    },
    Install {
        id: String,
        repo: Option<PathBuf>,
        dest: Option<PathBuf>,
        force: bool,
    },
    Link {
        id: String,
        repo: Option<PathBuf>,
        dest: Option<PathBuf>,
        force: bool,
    },
    Remove {
        id: String,
        dest: Option<PathBuf>,
    },
    Update {
        id: Option<String>,
        repo: Option<PathBuf>,
        dest: Option<PathBuf>,
    },
    Help,
    Version,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse_args(&args) {
        Ok(Action::Test {
            path,
            json,
            verbose,
        }) => {
            let report = run_test(&path, verbose);
            if json {
                print_report_json(&report);
            } else {
                print_report_text(&report);
            }
            if report.ok() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Ok(Action::Init {
            dir,
            id,
            name,
            force,
        }) => match run_init(&dir, id.as_deref(), name.as_deref(), force) {
            Ok(created) => {
                for path in &created {
                    println!("создан: {}", path.display());
                }
                println!();
                println!(
                    "Готово. Проверьте плагин: darmark-plugin test {}",
                    dir.display()
                );
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("ошибка: {message}");
                ExitCode::FAILURE
            }
        },
        Ok(Action::List { repo, json }) => match run_list(repo, json) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("ошибка: {message}");
                ExitCode::FAILURE
            }
        },
        Ok(Action::Install {
            id,
            repo,
            dest,
            force,
        }) => match run_install(&id, repo, dest, force) {
            Ok(target) => {
                println!("установлен: {}", target.display());
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("ошибка: {message}");
                ExitCode::FAILURE
            }
        },
        Ok(Action::Link {
            id,
            repo,
            dest,
            force,
        }) => match run_link(&id, repo, dest, force) {
            Ok(target) => {
                println!("привязан: {}", target.display());
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("ошибка: {message}");
                ExitCode::FAILURE
            }
        },
        Ok(Action::Remove { id, dest }) => match run_remove(&id, dest) {
            Ok((target, true)) => {
                println!("удалён: {}", target.display());
                ExitCode::SUCCESS
            }
            Ok((target, false)) => {
                println!("не установлен: {} (нечего удалять)", target.display());
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("ошибка: {message}");
                ExitCode::FAILURE
            }
        },
        Ok(Action::Update { id, repo, dest }) => match run_update(id.as_deref(), repo, dest) {
            Ok(changed) => {
                if changed.is_empty() {
                    println!("обновлений нет");
                } else {
                    for id in &changed {
                        println!("обновлён: {id}");
                    }
                }
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("ошибка: {message}");
                ExitCode::FAILURE
            }
        },
        Ok(Action::Help) => {
            print_usage();
            ExitCode::SUCCESS
        }
        Ok(Action::Version) => {
            println!("darmark-plugin {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("ошибка: {message}");
            print_usage();
            ExitCode::from(2)
        }
    }
}

/// Разбирает аргументы командной строки вручную (без `clap`, как в `plugin-host`).
fn parse_args(args: &[String]) -> Result<Action, String> {
    let Some(command) = args.first() else {
        return Err("не указана команда".to_string());
    };
    match command.as_str() {
        "test" => parse_test_args(&args[1..]),
        "init" => parse_init_args(&args[1..]),
        "list" => parse_list_args(&args[1..]),
        "install" => parse_install_args(&args[1..]),
        "link" => parse_link_args(&args[1..]),
        "remove" => parse_remove_args(&args[1..]),
        "update" => parse_update_args(&args[1..]),
        "--help" | "-h" => Ok(Action::Help),
        "--version" | "-V" => Ok(Action::Version),
        other => Err(format!("неизвестная команда: {other}")),
    }
}

/// Разбирает аргументы `test <path> [--json] [--verbose]`.
fn parse_test_args(args: &[String]) -> Result<Action, String> {
    let mut path: Option<PathBuf> = None;
    let mut json = false;
    let mut verbose = false;
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            "--verbose" => verbose = true,
            other if other.starts_with('-') => {
                return Err(format!("неизвестный аргумент: {other}"))
            }
            other => {
                if path.is_some() {
                    return Err("укажите ровно один путь".to_string());
                }
                path = Some(PathBuf::from(other));
            }
        }
    }
    let path = path.ok_or_else(|| "test требует путь <path>".to_string())?;
    Ok(Action::Test {
        path,
        json,
        verbose,
    })
}

/// Разбирает аргументы `init <dir> [--id <id>] [--name <name>] [--force]`.
fn parse_init_args(args: &[String]) -> Result<Action, String> {
    let mut dir: Option<PathBuf> = None;
    let mut id: Option<String> = None;
    let mut name: Option<String> = None;
    let mut force = false;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--force" => force = true,
            "--id" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--id требует значение".to_string())?;
                id = Some(value.clone());
            }
            "--name" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--name требует значение".to_string())?;
                name = Some(value.clone());
            }
            other if other.starts_with('-') => {
                return Err(format!("неизвестный аргумент: {other}"))
            }
            other => {
                if dir.is_some() {
                    return Err("укажите ровно один каталог".to_string());
                }
                dir = Some(PathBuf::from(other));
            }
        }
    }
    let dir = dir.ok_or_else(|| "init требует каталог <dir>".to_string())?;
    Ok(Action::Init {
        dir,
        id,
        name,
        force,
    })
}

/// Забирает значение следующего аргумента (для флагов вида `--repo <dir>`).
fn take_value(iter: &mut std::slice::Iter<'_, String>, flag: &str) -> Result<String, String> {
    iter.next()
        .cloned()
        .ok_or_else(|| format!("{flag} требует значение"))
}

/// Разбирает аргументы `list [--repo <dir>] [--json]`.
fn parse_list_args(args: &[String]) -> Result<Action, String> {
    let mut repo: Option<PathBuf> = None;
    let mut json = false;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--repo" => repo = Some(PathBuf::from(take_value(&mut iter, "--repo")?)),
            other if other.starts_with('-') => {
                return Err(format!("неизвестный аргумент: {other}"))
            }
            other => return Err(format!("лишний аргумент: {other}")),
        }
    }
    Ok(Action::List { repo, json })
}

/// Разбирает общие для `install`/`link` аргументы `<id> [--repo <dir>] [--dest <dir>] [--force]`.
fn parse_transfer_args(
    args: &[String],
    command: &str,
) -> Result<(String, Option<PathBuf>, Option<PathBuf>, bool), String> {
    let mut id: Option<String> = None;
    let mut repo: Option<PathBuf> = None;
    let mut dest: Option<PathBuf> = None;
    let mut force = false;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--force" => force = true,
            "--repo" => repo = Some(PathBuf::from(take_value(&mut iter, "--repo")?)),
            "--dest" => dest = Some(PathBuf::from(take_value(&mut iter, "--dest")?)),
            other if other.starts_with('-') => {
                return Err(format!("неизвестный аргумент: {other}"))
            }
            other => {
                if id.is_some() {
                    return Err(format!("{command}: укажите ровно один id"));
                }
                id = Some(other.to_string());
            }
        }
    }
    let id = id.ok_or_else(|| format!("{command} требует id <id>"))?;
    Ok((id, repo, dest, force))
}

/// Разбирает аргументы `install <id> [--repo <dir>] [--dest <dir>] [--force]`.
fn parse_install_args(args: &[String]) -> Result<Action, String> {
    let (id, repo, dest, force) = parse_transfer_args(args, "install")?;
    Ok(Action::Install {
        id,
        repo,
        dest,
        force,
    })
}

/// Разбирает аргументы `link <id> [--repo <dir>] [--dest <dir>] [--force]`.
fn parse_link_args(args: &[String]) -> Result<Action, String> {
    let (id, repo, dest, force) = parse_transfer_args(args, "link")?;
    Ok(Action::Link {
        id,
        repo,
        dest,
        force,
    })
}

/// Разбирает аргументы `remove <id> [--dest <dir>]`.
fn parse_remove_args(args: &[String]) -> Result<Action, String> {
    let mut id: Option<String> = None;
    let mut dest: Option<PathBuf> = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--dest" => dest = Some(PathBuf::from(take_value(&mut iter, "--dest")?)),
            other if other.starts_with('-') => {
                return Err(format!("неизвестный аргумент: {other}"))
            }
            other => {
                if id.is_some() {
                    return Err("remove: укажите ровно один id".to_string());
                }
                id = Some(other.to_string());
            }
        }
    }
    let id = id.ok_or_else(|| "remove требует id <id>".to_string())?;
    Ok(Action::Remove { id, dest })
}

/// Разбирает аргументы `update [<id>] [--repo <dir>] [--dest <dir>]`.
fn parse_update_args(args: &[String]) -> Result<Action, String> {
    let mut id: Option<String> = None;
    let mut repo: Option<PathBuf> = None;
    let mut dest: Option<PathBuf> = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--repo" => repo = Some(PathBuf::from(take_value(&mut iter, "--repo")?)),
            "--dest" => dest = Some(PathBuf::from(take_value(&mut iter, "--dest")?)),
            other if other.starts_with('-') => {
                return Err(format!("неизвестный аргумент: {other}"))
            }
            other => {
                if id.is_some() {
                    return Err("update: укажите не более одного id".to_string());
                }
                id = Some(other.to_string());
            }
        }
    }
    Ok(Action::Update { id, repo, dest })
}

/// Разрешает каталог репозитория: явный аргумент → env `DARMARK_PLUGIN_REPO` → ближайший
/// предок (включая CWD) с `plugins/index.json` → `<cwd>/plugins` (ошибку даст загрузка индекса).
fn resolve_repo(explicit: Option<PathBuf>) -> PathBuf {
    if let Some(dir) = explicit {
        return normalize_repo(dir);
    }
    if let Some(dir) = std::env::var_os("DARMARK_PLUGIN_REPO") {
        return normalize_repo(PathBuf::from(dir));
    }
    let cwd = std::env::current_dir().unwrap_or_default();
    if let Some(found) = catalog::discover_repo(&cwd) {
        return found;
    }
    cwd.join("plugins")
}

/// Терпит и каталог-репозиторий (`<dir>/plugins/index.json`), и сам каталог плагинов
/// (`<dir>/index.json`): пользователь может указать любой из них.
fn normalize_repo(dir: PathBuf) -> PathBuf {
    if dir.join(catalog::CATALOG_FILE).is_file() {
        return dir;
    }
    let nested = dir.join("plugins");
    if nested.join(catalog::CATALOG_FILE).is_file() {
        return nested;
    }
    dir
}

/// Разрешает каталог установки: явный аргумент → env `DARMARK_PLUGINS_DIR` →
/// `%APPDATA%/darmark/plugins` (как `plugins_dir()` в GUI-хосте).
fn resolve_dest(explicit: Option<PathBuf>) -> Result<PathBuf, String> {
    if let Some(dir) = explicit {
        return Ok(dir);
    }
    if let Some(dir) = std::env::var_os("DARMARK_PLUGINS_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let base = std::env::var_os("APPDATA")
        .ok_or_else(|| "APPDATA не задан: укажите --dest или DARMARK_PLUGINS_DIR".to_string())?;
    Ok(PathBuf::from(base).join("darmark").join("plugins"))
}

/// `list`: загружает каталог и печатает его (таблицей или JSON).
fn run_list(repo: Option<PathBuf>, json: bool) -> Result<(), String> {
    let catalog = Catalog::load(&resolve_repo(repo))?;
    if json {
        print_catalog_json(&catalog);
    } else {
        print_catalog_text(&catalog);
    }
    Ok(())
}

/// `install`: копирует каталог плагина в каталог приложения.
fn run_install(
    id: &str,
    repo: Option<PathBuf>,
    dest: Option<PathBuf>,
    force: bool,
) -> Result<PathBuf, String> {
    let catalog = Catalog::load(&resolve_repo(repo))?;
    let dest = resolve_dest(dest)?;
    catalog.install(id, &dest, force)
}

/// `link`: привязывает каталог плагина к каталогу приложения ссылкой (junction/symlink).
fn run_link(
    id: &str,
    repo: Option<PathBuf>,
    dest: Option<PathBuf>,
    force: bool,
) -> Result<PathBuf, String> {
    let catalog = Catalog::load(&resolve_repo(repo))?;
    let dest = resolve_dest(dest)?;
    catalog.link(id, &dest, force)
}

/// `remove`: удаляет установленный плагин. Возвращает путь-цель и признак: `true` — удалён,
/// `false` — нечего удалять (идемпотентно, код выхода всё равно `0`).
fn run_remove(id: &str, dest: Option<PathBuf>) -> Result<(PathBuf, bool), String> {
    let dest = resolve_dest(dest)?;
    let removed = catalog::remove(id, &dest)?;
    Ok((dest.join(id), removed))
}

/// `update`: обновляет установленные копии до версии из репозитория.
fn run_update(
    id: Option<&str>,
    repo: Option<PathBuf>,
    dest: Option<PathBuf>,
) -> Result<Vec<String>, String> {
    let catalog = Catalog::load(&resolve_repo(repo))?;
    let dest = resolve_dest(dest)?;
    catalog.update(id, &dest)
}

/// Текстовая таблица каталога (id, version, permissions, description).
fn print_catalog_text(catalog: &Catalog) {
    let entries = catalog.list();
    let permissions = |entry: &CatalogEntry| {
        if entry.permissions.is_empty() {
            "-".to_string()
        } else {
            entry.permissions.join(",")
        }
    };
    let id_w = entries
        .iter()
        .map(|e| e.id.len())
        .chain(std::iter::once("id".len()))
        .max()
        .unwrap_or(0);
    let version_w = entries
        .iter()
        .map(|e| e.version.len())
        .chain(std::iter::once("version".len()))
        .max()
        .unwrap_or(0);
    let permissions_w = entries
        .iter()
        .map(|e| permissions(e).len())
        .chain(std::iter::once("permissions".len()))
        .max()
        .unwrap_or(0);

    println!(
        "{:<id_w$}  {:<version_w$}  {:<permissions_w$}  description",
        "id", "version", "permissions"
    );
    for entry in entries {
        println!(
            "{:<id_w$}  {:<version_w$}  {:<permissions_w$}  {}",
            entry.id,
            entry.version,
            permissions(entry),
            entry.description
        );
    }
}

/// JSON-вывод каталога (массив объектов).
fn print_catalog_json(catalog: &Catalog) {
    let items: Vec<serde_json::Value> = catalog
        .list()
        .iter()
        .map(|entry| {
            serde_json::json!({
                "id": entry.id,
                "name": entry.name,
                "version": entry.version,
                "api_version": entry.api_version,
                "permissions": entry.permissions,
                "description": entry.description,
                "path": entry.path,
            })
        })
        .collect();
    match serde_json::to_string_pretty(&serde_json::Value::Array(items)) {
        Ok(text) => println!("{text}"),
        Err(error) => eprintln!("ошибка сериализации JSON: {error}"),
    }
}

/// Выполняет проверки плагина: манифест (каталог), entry, песочница.
///
/// Проверки идут по порядку; при провале ранней (манифест/entry) последующие не выполняются —
/// без валидного входа песочницу запускать бессмысленно.
fn run_test(path: &Path, verbose: bool) -> TestReport {
    let mut checks = Vec::new();
    let mut sandbox_logs = Vec::new();

    if verbose {
        eprintln!("путь: {}", path.display());
        eprintln!(
            "тип: {}",
            if path.is_dir() {
                "каталог плагина"
            } else {
                ".lua-файл"
            }
        );
    }

    if !path.exists() {
        checks.push(Check::fail(
            "path",
            format!("{}: путь не существует", path.display()),
        ));
        return TestReport {
            path: path.to_path_buf(),
            checks,
            sandbox_logs,
        };
    }

    let is_dir = path.is_dir();

    // Шаг 1: манифест (только для каталога).
    if is_dir {
        match check_manifest(path) {
            Ok(message) => checks.push(Check::ok("manifest", message)),
            Err(message) => {
                checks.push(Check::fail("manifest", message));
                return TestReport {
                    path: path.to_path_buf(),
                    checks,
                    sandbox_logs,
                };
            }
        }
    }

    // Шаг 2: entry.
    match check_entry(path) {
        Ok(message) => checks.push(Check::ok("entry", message)),
        Err(message) => {
            checks.push(Check::fail("entry", message));
            return TestReport {
                path: path.to_path_buf(),
                checks,
                sandbox_logs,
            };
        }
    }

    // Шаг 3: песочница (реальный load + on_activate + on_deactivate).
    match selftest::self_test(path) {
        Ok(report) => {
            sandbox_logs = report.logs;
            checks.push(Check::ok(
                "sandbox",
                format!(
                    "{}: активирован ({} байт)",
                    report.plugin_id, report.source_bytes
                ),
            ));
        }
        Err(message) => checks.push(Check::fail("sandbox", message)),
    }

    TestReport {
        path: path.to_path_buf(),
        checks,
        sandbox_logs,
    }
}

/// Проверяет манифест каталога: чтение с потолком 256 КиБ + валидация `Manifest::from_json`.
fn check_manifest(dir: &Path) -> Result<String, String> {
    let manifest_path = dir.join(MANIFEST_FILE);
    let bytes = read_capped(&manifest_path, MAX_MANIFEST_BYTES)?;
    let manifest =
        Manifest::from_json(&bytes).map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    let permissions = if manifest.permissions.is_empty() {
        "нет".to_string()
    } else {
        manifest.permissions.join(", ")
    };
    Ok(format!(
        "id={}, version={}, api_version={}, permissions: {}",
        manifest.id, manifest.version, manifest.api_version, permissions
    ))
}

/// Путь к entry-файлу: для каталога — из манифеста, для `.lua` — сам файл.
fn entry_path_of(path: &Path) -> Result<PathBuf, String> {
    if path.is_dir() {
        let manifest_path = path.join(MANIFEST_FILE);
        let bytes = read_capped(&manifest_path, MAX_MANIFEST_BYTES)?;
        let manifest =
            Manifest::from_json(&bytes).map_err(|e| format!("{}: {e}", manifest_path.display()))?;
        Ok(path.join(&manifest.entry))
    } else {
        Ok(path.to_path_buf())
    }
}

/// Проверяет entry: файл существует, читается, ≤ `MAX_PLUGIN_SOURCE_BYTES`, UTF-8 и объявляет
/// `on_activate`.
fn check_entry(path: &Path) -> Result<String, String> {
    let entry_path = entry_path_of(path)?;
    if !entry_path.is_file() {
        return Err(format!("{}: файл не найден", entry_path.display()));
    }

    let bytes = std::fs::read(&entry_path)
        .map_err(|e| format!("{}: не читается: {e}", entry_path.display()))?;
    plugin_core::manifest::validate_source_len(bytes.len() as u64)
        .map_err(|e| format!("{}: {e}", entry_path.display()))?;
    let source =
        String::from_utf8(bytes).map_err(|e| format!("{}: не UTF-8: {e}", entry_path.display()))?;

    if !has_on_activate(&source) {
        return Err(format!(
            "{}: в исходнике не найдена функция on_activate",
            entry_path.display()
        ));
    }

    Ok(format!(
        "{}: {} байт, UTF-8, on_activate найден",
        entry_path.display(),
        source.len()
    ))
}

/// Эвристика объявления **глобальной** точки входа `on_activate`.
///
/// Признаются только две формы:
/// - `function on_activate(...)` — голое имя, `function` и `on_activate` разделены
///   любыми whitespace (≥1);
/// - `on_activate = ...` — голое имя, между `on_activate` и `=` любые whitespace.
///
/// Отвергаются: поля/методы (`M.on_activate = ...`, `function M.on_activate(...)`),
/// локальные объявления (`local function on_activate(...)`, `local on_activate = ...`),
/// а также `on_activate` как значение или часть другого идентификатора.
///
/// Известные ограничения:
/// - Lua-комментарии (`--`, `--[[ ]]`) не обрабатываются;
/// - явные глобальные формы `_G.on_activate = ...` / `_ENV.on_activate = ...`
///   отвергаются (перед именем стоит `.`), хотя семантически они глобальные.
fn has_on_activate(source: &str) -> bool {
    const NEEDLE: &str = "on_activate";

    fn is_ws(c: char) -> bool {
        matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{0B}' | '\u{0C}')
    }

    fn is_word_char(c: char) -> bool {
        c.is_alphanumeric() || c == '_'
    }

    /// Проверяет, что перед позицией `pos` (после отбрасывания trailing whitespace)
    /// нет слова `local` — чтобы отличить глобальное объявление от локального.
    fn no_local_before(source: &str, pos: usize) -> bool {
        let before = source[..pos].trim_end_matches(is_ws);
        if before.ends_with("local") {
            let local_start = before.len() - "local".len();
            if local_start == 0
                || source[..local_start]
                    .chars()
                    .next_back()
                    .is_none_or(|c| !is_word_char(c))
            {
                return false;
            }
        }
        true
    }

    /// Проверяет, что перед позицией `pos` (после отбрасывания trailing whitespace)
    /// нет маркера поля/метода `.` или `:` — чтобы отличить глобальное присваивание
    /// от `M.on_activate = ...`.
    fn no_field_marker_before(source: &str, pos: usize) -> bool {
        let before = source[..pos].trim_end_matches(is_ws);
        !before.ends_with('.') && !before.ends_with(':')
    }

    let mut from = 0;
    while let Some(offset) = source[from..].find(NEEDLE) {
        let start = from + offset;
        let end = start + NEEDLE.len();

        // Границы слова: рядом с on_activate не должно быть букв/цифр/подчёркиваний.
        // Это отсекает случаи вида on_activate2, my_on_activate и т.п.
        let before_ok = source[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !is_word_char(c));
        let after_ok = source[end..]
            .chars()
            .next()
            .is_none_or(|c| !is_word_char(c));

        if before_ok && after_ok {
            // Присваивание: on_activate [ws]* =
            // Отсекаем поля/методы (M.on_activate = ...) и локальные (local on_activate = ...).
            if no_field_marker_before(source, start) && no_local_before(source, start) {
                let after = source[end..].trim_start_matches(is_ws);
                if after.starts_with('=') {
                    return true;
                }
            }

            // Объявление функции: function [ws]+ on_activate.
            // Отсекаем local function и function M.on_activate (между function и on_activate
            // не только whitespace — between_ok это проверяет).
            let before = &source[..start];
            if let Some(func_rel) = before.rfind("function") {
                let func_start = func_rel;
                let func_end = func_start + "function".len();
                let func_before_ok = source[..func_start]
                    .chars()
                    .next_back()
                    .is_none_or(|c| !is_word_char(c));
                let between = &before[func_end..];
                let between_ok = !between.is_empty() && between.chars().all(is_ws);
                if func_before_ok && between_ok && no_local_before(source, func_start) {
                    return true;
                }
            }
        }

        from = end;
    }
    false
}

/// Читает файл с жёстким потолком размера (защита от TOCTOU и OOM).
fn read_capped(path: &Path, max: u64) -> Result<Vec<u8>, String> {
    let metadata = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if metadata.len() > max {
        return Err(format!(
            "{}: превышает лимит {max} Б ({} Б)",
            path.display(),
            metadata.len()
        ));
    }
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut buf = Vec::new();
    file.take(max + 1)
        .read_to_end(&mut buf)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if buf.len() as u64 > max {
        return Err(format!("{}: превышает лимит {max} Б", path.display()));
    }
    Ok(buf)
}

/// Создаёт шаблон плагина: `plugin.json`, `main.lua`, `README.md`.
///
/// Без `--force` не перезаписывает существующие файлы. Возвращает созданные пути.
fn run_init(
    dir: &Path,
    id: Option<&str>,
    name: Option<&str>,
    force: bool,
) -> Result<Vec<PathBuf>, String> {
    let dir_name = dir
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();

    let id = match id {
        Some(value) => value.to_string(),
        None => {
            let sanitized = sanitize_id(&dir_name);
            if sanitized.is_empty() {
                return Err(format!(
                    "не удалось определить id из имени каталога {dir_name:?}: используйте --id"
                ));
            }
            sanitized
        }
    };
    let name = name.map(|s| s.to_string()).unwrap_or(dir_name);

    let targets = [
        (dir.join(MANIFEST_FILE), TEMPLATE_PLUGIN_JSON),
        (dir.join("main.lua"), TEMPLATE_MAIN_LUA),
        (dir.join("README.md"), TEMPLATE_README),
    ];

    if !force {
        for (path, _) in &targets {
            if path.exists() {
                return Err(format!(
                    "{}: файл уже существует (используйте --force для перезаписи)",
                    path.display()
                ));
            }
        }
    }

    std::fs::create_dir_all(dir)
        .map_err(|e| format!("{}: не создать каталог: {e}", dir.display()))?;

    let mut created = Vec::with_capacity(targets.len());
    for (path, template) in &targets {
        let content = template.replace("{{ID}}", &id).replace("{{NAME}}", &name);
        std::fs::write(path, content)
            .map_err(|e| format!("{}: не записать: {e}", path.display()))?;
        created.push(path.clone());
    }
    Ok(created)
}

/// Приводит имя каталога к безопасному id: нижний регистр, вне `[a-z0-9_-]` → `-`,
/// схлопывает повторы `-`, обрезает края.
fn sanitize_id(name: &str) -> String {
    let mapped: String = name
        .to_lowercase()
        .chars()
        .map(|ch| {
            if ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect();

    let mut collapsed = String::with_capacity(mapped.len());
    let mut prev_dash = false;
    for ch in mapped.chars() {
        if ch == '-' {
            if !prev_dash {
                collapsed.push('-');
            }
            prev_dash = true;
        } else {
            collapsed.push(ch);
            prev_dash = false;
        }
    }
    collapsed.trim_matches('-').to_string()
}

/// Текстовый отчёт `test`.
fn print_report_text(report: &TestReport) {
    for check in &report.checks {
        let status = match check.status {
            CheckStatus::Ok => "OK",
            CheckStatus::Fail => "FAIL",
        };
        println!("[{status}] {}: {}", check.name, check.message);
    }
    if !report.sandbox_logs.is_empty() {
        println!("Лог песочницы:");
        for line in &report.sandbox_logs {
            println!("  {line}");
        }
    }
    println!(
        "Итого: {} проверок, ошибок {}",
        report.checks.len(),
        report.failed()
    );
}

/// JSON-отчёт `test` как значение (один объект) — отделён от печати ради тестов формы.
fn report_to_json(report: &TestReport) -> serde_json::Value {
    let checks: Vec<serde_json::Value> = report
        .checks
        .iter()
        .map(|check| {
            serde_json::json!({
                "name": check.name,
                "status": match check.status {
                    CheckStatus::Ok => "ok",
                    CheckStatus::Fail => "fail",
                },
                "message": check.message,
            })
        })
        .collect();
    serde_json::json!({
        "schema_version": REPORT_SCHEMA_VERSION,
        "path": report.path.to_string_lossy(),
        "ok": report.ok(),
        "checks": checks,
        "sandbox_logs": report.sandbox_logs,
    })
}

/// JSON-отчёт `test` (один объект).
fn print_report_json(report: &TestReport) {
    match serde_json::to_string_pretty(&report_to_json(report)) {
        Ok(text) => println!("{text}"),
        Err(error) => eprintln!("ошибка сериализации JSON: {error}"),
    }
}

fn print_usage() {
    println!("darmark-plugin — CLI разработчика плагинов darmark");
    println!();
    println!("Использование:");
    println!("  darmark-plugin test <path> [--json] [--verbose]");
    println!("      Проверить плагин: манифест, entry, песочница.");
    println!("      --json     — вывести отчёт одним JSON-объектом");
    println!("      --verbose  — подробности в stderr");
    println!("  darmark-plugin init <dir> [--id <id>] [--name <name>] [--force]");
    println!("      Создать шаблон плагина (plugin.json, main.lua, README.md).");
    println!("      --id/--name — явные id и имя (иначе из имени каталога)");
    println!("      --force     — перезаписать существующие файлы");
    println!();
    println!("Каталог плагинов (plugins/index.json):");
    println!("  darmark-plugin list [--repo <dir>] [--json]");
    println!("      Показать доступные в каталоге плагины.");
    println!("      --json      — массив объектов вместо таблицы");
    println!("  darmark-plugin install <id> [--repo <dir>] [--dest <dir>] [--force]");
    println!("      Скопировать плагин из каталога в каталог приложения.");
    println!("  darmark-plugin link <id> [--repo <dir>] [--dest <dir>] [--force]");
    println!("      Привязать каталог плагина ссылкой (junction) — для разработки.");
    println!("  darmark-plugin remove <id> [--dest <dir>]");
    println!("      Удалить установленный плагин (ссылку — не трогая источник).");
    println!("  darmark-plugin update [<id>] [--repo <dir>] [--dest <dir>]");
    println!("      Обновить установленные копии до версии из каталога (ссылки пропускаются).");
    println!("      --repo      — каталог с index.json (env DARMARK_PLUGIN_REPO,");
    println!("                    иначе поиск вверх от CWD, иначе <cwd>/plugins)");
    println!("      --dest      — каталог плагинов приложения (env DARMARK_PLUGINS_DIR,");
    println!("                    иначе %APPDATA%/darmark/plugins)");
    println!("      --force     — перезаписать существующую установку");
    println!();
    println!("  darmark-plugin --help, -h    — эта справка");
    println!("  darmark-plugin --version, -V — версия");
    println!();
    println!("Примеры:");
    println!("  darmark-plugin test examples/plugins/word-count");
    println!("  darmark-plugin test . --json");
    println!("  darmark-plugin init my-plugin --name \"Мой плагин\"");
    println!("  darmark-plugin list --repo examples/plugins");
    println!("  darmark-plugin install word-count --repo examples/plugins");
    println!("  darmark-plugin link my-plugin --repo examples/plugins");
    println!("  darmark-plugin update");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Готовит каталог с манифестом и (опционально) исходником.
    fn write_plugin(dir: &Path, manifest: &str, lua: Option<&str>) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join(MANIFEST_FILE), manifest).unwrap();
        if let Some(lua) = lua {
            fs::write(dir.join("main.lua"), lua).unwrap();
        }
    }

    fn manifest(id: &str) -> String {
        format!(
            r#"{{"id":"{id}","name":"{id}","version":"1.0.0","api_version":1,"entry":"main.lua"}}"#
        )
    }

    fn find<'a>(report: &'a TestReport, name: &str) -> &'a Check {
        report
            .checks
            .iter()
            .find(|c| c.name == name)
            .unwrap_or_else(|| panic!("нет проверки {name}: {:?}", report.checks))
    }

    #[test]
    fn test_valid_plugin_ok() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("valid");
        write_plugin(
            &dir,
            &manifest("valid"),
            Some("function on_activate(ctx) end\nfunction on_deactivate(ctx) end\n"),
        );
        let report = run_test(&dir, false);
        assert!(report.ok(), "checks: {:?}", report.checks);
        assert_eq!(find(&report, "manifest").status, CheckStatus::Ok);
        assert_eq!(find(&report, "entry").status, CheckStatus::Ok);
        assert_eq!(find(&report, "sandbox").status, CheckStatus::Ok);
    }

    #[test]
    fn test_unknown_permission_fails() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("bad-perm");
        write_plugin(
            &dir,
            r#"{"id":"bad-perm","name":"Bad","version":"1.0.0","api_version":1,"entry":"main.lua","permissions":["quantum:teleport"]}"#,
            Some("function on_activate(ctx) end"),
        );
        let report = run_test(&dir, false);
        assert!(!report.ok());
        let check = find(&report, "manifest");
        assert_eq!(check.status, CheckStatus::Fail);
        assert!(
            check.message.contains("quantum:teleport"),
            "msg: {}",
            check.message
        );
    }

    #[test]
    fn test_missing_entry_fails() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("no-entry");
        write_plugin(&dir, &manifest("no-entry"), None);
        let report = run_test(&dir, false);
        assert!(!report.ok());
        let check = find(&report, "entry");
        assert_eq!(check.status, CheckStatus::Fail);
        assert!(check.message.contains("main.lua"), "msg: {}", check.message);
    }

    #[test]
    fn test_source_without_on_activate_fails() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("no-activate");
        write_plugin(
            &dir,
            &manifest("no-activate"),
            Some("host.log(\"info\", \"нет активации\")"),
        );
        let report = run_test(&dir, false);
        assert!(!report.ok());
        let check = find(&report, "entry");
        assert_eq!(check.status, CheckStatus::Fail);
        assert!(
            check.message.contains("on_activate"),
            "msg: {}",
            check.message
        );
    }

    #[test]
    fn test_oversized_source_fails() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("big");
        // MAX_PLUGIN_SOURCE_BYTES = 1 МиБ; пишем заведомо больше.
        let big = "function on_activate(ctx) end\n".repeat(70_000);
        write_plugin(&dir, &manifest("big"), Some(&big));
        let report = run_test(&dir, false);
        assert!(!report.ok());
        let check = find(&report, "entry");
        assert_eq!(check.status, CheckStatus::Fail);
        assert!(check.message.contains("лимит"), "msg: {}", check.message);
    }

    #[test]
    fn test_smoke_word_count_plugin() {
        // plugin-cli → crates → root → examples/plugins/word-count.
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("examples")
            .join("plugins")
            .join("word-count");
        let report = run_test(&path, false);
        assert!(report.ok(), "checks: {:?}", report.checks);
        assert_eq!(find(&report, "sandbox").status, CheckStatus::Ok);
    }

    #[test]
    fn report_json_has_schema_version_and_shape() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("valid");
        write_plugin(
            &dir,
            &manifest("valid"),
            Some("function on_activate(ctx) end\n"),
        );
        let report = run_test(&dir, false);
        let json = report_to_json(&report);
        assert_eq!(
            json["schema_version"], 1,
            "версионированный отчёт (ADR-0034 §2.4)"
        );
        assert_eq!(json["ok"], true);
        assert!(json["checks"].is_array(), "checks[] присутствует");
        assert!(
            json["sandbox_logs"].is_array(),
            "sandbox_logs[] присутствует"
        );
        assert!(json["path"].is_string(), "path присутствует");
    }

    #[test]
    fn init_creates_three_files_with_substitution() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("my-plugin");
        let created = run_init(&dir, None, None, false).unwrap();
        assert_eq!(created.len(), 3);
        for path in &created {
            assert!(path.exists(), "{}: не создан", path.display());
        }

        let manifest_text = fs::read_to_string(dir.join(MANIFEST_FILE)).unwrap();
        assert!(
            manifest_text.contains("\"id\": \"my-plugin\""),
            "manifest: {manifest_text}"
        );
        assert!(
            manifest_text.contains("\"name\": \"my-plugin\""),
            "manifest: {manifest_text}"
        );
        let readme = fs::read_to_string(dir.join("README.md")).unwrap();
        assert!(readme.contains("my-plugin"), "readme: {readme}");
        let lua = fs::read_to_string(dir.join("main.lua")).unwrap();
        assert!(lua.contains("my-plugin"), "lua: {lua}");
    }

    #[test]
    fn init_honours_explicit_id_and_name() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("Директория С Пробелами");
        run_init(&dir, Some("custom-id"), Some("Моё имя"), false).unwrap();
        let manifest_text = fs::read_to_string(dir.join(MANIFEST_FILE)).unwrap();
        assert!(
            manifest_text.contains("\"id\": \"custom-id\""),
            "{manifest_text}"
        );
        assert!(
            manifest_text.contains("\"name\": \"Моё имя\""),
            "{manifest_text}"
        );
    }

    #[test]
    fn init_refuses_to_overwrite_without_force() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("exists");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(MANIFEST_FILE), "оригинал").unwrap();

        let err = run_init(&dir, None, None, false).unwrap_err();
        assert!(err.contains("--force"), "err: {err}");
        assert_eq!(
            fs::read_to_string(dir.join(MANIFEST_FILE)).unwrap(),
            "оригинал",
            "файл не должен быть перезаписан"
        );

        // С --force файл перезаписывается.
        run_init(&dir, None, None, true).unwrap();
        assert_ne!(
            fs::read_to_string(dir.join(MANIFEST_FILE)).unwrap(),
            "оригинал"
        );
    }

    #[test]
    fn sanitize_id_collapses_and_trims() {
        assert_eq!(sanitize_id("My Plugin!"), "my-plugin");
        assert_eq!(sanitize_id("--Hello__World--"), "hello__world");
        assert_eq!(sanitize_id("a..b"), "a-b");
        assert_eq!(sanitize_id("!!!"), "");
    }

    #[test]
    fn has_on_activate_variants() {
        // Базовые случаи.
        assert!(has_on_activate("function on_activate(ctx) end"));
        assert!(has_on_activate("on_activate = function(ctx) end"));
        assert!(has_on_activate("on_activate   = function(ctx) end"));
        assert!(has_on_activate("on_activate=function"));
        assert!(!has_on_activate("function on_event() end"));

        // Разные варианты whitespace между function и on_activate.
        assert!(has_on_activate("function  on_activate(ctx) end"));
        assert!(has_on_activate("function\ton_activate(ctx) end"));
        assert!(has_on_activate("function\non_activate(ctx) end"));
        assert!(has_on_activate("function\r\non_activate(ctx) end"));

        // Локальные объявления — не глобальная точка входа.
        assert!(!has_on_activate("local function on_activate(ctx) end"));
        assert!(!has_on_activate("local on_activate = function(ctx) end"));

        // Границы слова: on_activate2 и my_on_activate — не on_activate.
        assert!(!has_on_activate("on_activate2"));
        assert!(!has_on_activate("my_on_activate"));
        assert!(!has_on_activate("local on_activate_handler = 1"));

        // `on_activate` как значение, а не точка входа — не объявление.
        assert!(!has_on_activate("local f = on_activate"));
        assert!(!has_on_activate("return on_activate"));
        assert!(!has_on_activate("print(on_activate)"));
        assert!(!has_on_activate("host.log('info', on_activate)"));

        // Известное ограничение: комментарии не обрабатываются.
        // Следующая строка будет ложно распознана как объявление:
        // assert!(has_on_activate("-- function on_activate"));
    }

    #[test]
    fn has_on_activate_word_boundaries_around_function() {
        // `function` как часть более длинного идентификатора — не объявление.
        assert!(!has_on_activate("notfunction on_activate(ctx) end"));
        assert!(!has_on_activate("myfunction on_activate(ctx) end"));
        // Продолжение имени после on_activate — другой идентификатор.
        assert!(!has_on_activate("function on_activatex(ctx) end"));
        assert!(!has_on_activate("function on_activate_v2(ctx) end"));
        assert!(!has_on_activate("on_activate_v2 = function(ctx) end"));
    }

    #[test]
    fn has_on_activate_mixed_whitespace_variants() {
        // Смешанные пробелы и переводы строк между function и on_activate.
        assert!(has_on_activate("function \t\n on_activate(ctx) end"));
        assert!(has_on_activate("function\n\non_activate(ctx) end"));
        // Whitespace перед `=`, включая перевод строки.
        assert!(has_on_activate("on_activate   \n = function(ctx) end"));
        assert!(has_on_activate("on_activate\r\n= function(ctx) end"));
    }

    #[test]
    fn has_on_activate_declaration_without_parens() {
        // Эвристика не парсит синтаксис Lua: `function on_activate` без скобок
        // и с пробелом перед `(` распознаётся — невалидный синтаксис поймает песочница.
        assert!(has_on_activate("function on_activate end"));
        assert!(has_on_activate("function on_activate (ctx) end"));
    }

    #[test]
    fn has_on_activate_unicode_spaces_are_not_separators() {
        // Lua-лексер считает разделителями только ASCII-пробелы, \t, \n, \r:
        // NBSP и другие юникод-пробелы не разрывают токены, эвристика это учитывает.
        assert!(!has_on_activate("function\u{00A0}on_activate(ctx) end"));
        assert!(!has_on_activate("on_activate\u{00A0}= function(ctx) end"));
        assert!(!has_on_activate("function\u{2028}on_activate(ctx) end"));
    }

    #[test]
    fn has_on_activate_vertical_tab_and_form_feed() {
        // Lua допускает \v и \f как разделители токенов — эвристика их учитывает.
        assert!(has_on_activate("function\u{0B}on_activate(ctx) end"));
        assert!(has_on_activate("function\u{0C}on_activate(ctx) end"));
        // Форма с \v перед `=`.
        assert!(has_on_activate("on_activate\u{0B}= function(ctx) end"));
    }

    #[test]
    fn has_on_activate_method_form_is_not_global_entry() {
        // plugin-host берёт on_activate из globals (selftest.rs: `globals().get`):
        // `function M.on_activate` создаёт метод таблицы M, а не глобальную точку
        // входа. Между `function` и `on_activate` стоит `M.` — не чистый whitespace,
        // поэтому эвристика его уже отвергает (фиксируем корректное поведение).
        assert!(!has_on_activate("function M.on_activate(ctx) end"));
        assert!(!has_on_activate("function M:on_activate(ctx) end"));
    }

    #[test]
    fn has_on_activate_field_assignment_is_not_global_entry() {
        // plugin-host берёт on_activate из globals (selftest.rs: `globals().get`):
        // `M.on_activate = function` создаёт поле таблицы M, а не глобальную точку
        // входа, — эвристика не должна его принимать.
        assert!(!has_on_activate("M.on_activate = function(ctx) end"));
    }

    #[test]
    fn check_entry_accepts_double_space_declaration() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("double-space");
        write_plugin(
            &dir,
            &manifest("double-space"),
            Some("function  on_activate(ctx) end\n"),
        );
        let message = check_entry(&dir).expect("двойной пробел допускается");
        assert!(message.contains("on_activate найден"), "msg: {message}");
    }

    #[test]
    fn check_entry_rejects_method_form() {
        // `function M.on_activate` не создаёт глобальную on_activate,
        // которую ждёт plugin-host, — entry должен быть отклонён.
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("method-form");
        write_plugin(
            &dir,
            &manifest("method-form"),
            Some("function M.on_activate(ctx) end\n"),
        );
        let err = check_entry(&dir).unwrap_err();
        assert!(err.contains("on_activate"), "err: {err}");
    }

    #[test]
    fn check_entry_rejects_field_assignment_form() {
        // `M.on_activate = function` создаёт поле таблицы M, а не глобальную
        // on_activate, которую ждёт plugin-host, — entry должен быть отклонён.
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("field-assignment");
        write_plugin(
            &dir,
            &manifest("field-assignment"),
            Some("M.on_activate = function(ctx) end\n"),
        );
        let err = check_entry(&dir).unwrap_err();
        assert!(err.contains("on_activate"), "err: {err}");
    }

    /// F45: `normalize_repo` намеренно терпит обе формы — и сам каталог плагинов с `index.json`,
    /// и корень репозитория с `plugins/index.json` (разработчику удобнее указывать любой из них);
    /// пустой каталог возвращается как есть — понятную ошибку даст загрузка индекса.
    #[test]
    fn normalize_repo_accepts_catalog_dir_and_repo_root() {
        let root = tempfile::tempdir().unwrap();

        // Каталог с index.json — возвращается как есть.
        let plugins = root.path().join("plugins");
        fs::create_dir_all(&plugins).unwrap();
        fs::write(plugins.join(catalog::CATALOG_FILE), "{}").unwrap();
        assert_eq!(normalize_repo(plugins.clone()), plugins);

        // Корень репозитория с plugins/index.json — нормализуется в <dir>/plugins.
        assert_eq!(normalize_repo(root.path().to_path_buf()), plugins);

        // Пустой каталог без index.json — возвращается как есть.
        let empty = root.path().join("empty");
        fs::create_dir_all(&empty).unwrap();
        assert_eq!(normalize_repo(empty.clone()), empty);
    }
}
