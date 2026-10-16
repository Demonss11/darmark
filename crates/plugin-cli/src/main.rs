//! CLI разработчика плагинов darmark: проверка (`test`), шаблон (`init`) и работа с каталогом.
//!
//! Бинарник `darmark-plugin` — отдельный крейт, чтобы GUI-хост `darmark` не линковал mlua
//! (D6): песочницу даёт `plugin-host` как библиотека.
//!
//! Коды выхода: `0` — успех/проверки пройдены, `1` — ошибка операции/проверок, `2` — ошибка
//! аргументов (неизвестная команда/флаг).

mod args;
mod commands;
mod report;
mod templates;

use std::path::PathBuf;
use std::process::ExitCode;

use args::{parse_init_args, parse_install_args, parse_link_args, parse_list_args};
use args::{parse_remove_args, parse_test_args, parse_update_args};
use commands::{run_init, run_install, run_link, run_list, run_remove, run_test, run_update};
use report::{print_report_json, print_report_text};


#[derive(Debug, PartialEq, Eq)]
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
            Err(message) => op_error(message),
        },
        Ok(Action::List { repo, json }) => match run_list(repo, json) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => op_error(message),
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
            Err(message) => op_error(message),
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
            Err(message) => op_error(message),
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
            Err(message) => op_error(message),
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
            Err(message) => op_error(message),
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


fn op_error(message: String) -> ExitCode {
    eprintln!("ошибка: {message}");
    ExitCode::FAILURE
}


const USAGE: &str = "\
darmark-plugin — CLI разработчика плагинов ПЕНАЛ

Использование:
  darmark-plugin test <path> [--json] [--verbose]
      Проверить плагин: манифест, entry, песочница.
      --json     — вывести отчёт одним JSON-объектом
      --verbose  — подробности в stderr
  darmark-plugin init <dir> [--id <id>] [--name <name>] [--force]
      Создать шаблон плагина (plugin.json, main.lua, README.md).
      --id/--name — явные id и имя (иначе из имени каталога)
      --force     — перезаписать существующие файлы

Каталог плагинов (plugins/index.json):
  darmark-plugin list [--repo <dir>] [--json]
      Показать доступные в каталоге плагины.
      --json      — массив объектов вместо таблицы
  darmark-plugin install <id> [--repo <dir>] [--dest <dir>] [--force]
      Скопировать плагин из каталога в каталог приложения.
  darmark-plugin link <id> [--repo <dir>] [--dest <dir>] [--force]
      Привязать каталог плагина ссылкой (junction) — для разработки.
  darmark-plugin remove <id> [--dest <dir>]
      Удалить установленный плагин (ссылку — не трогая источник).
  darmark-plugin update [<id>] [--repo <dir>] [--dest <dir>]
      Обновить установленные копии до версии из каталога (ссылки пропускаются).
      --repo      — каталог с index.json (env DARMARK_PLUGIN_REPO,
                    иначе поиск вверх (plugins/ или examples/plugins/), иначе <cwd>/plugins)
      --dest      — каталог плагинов приложения (env DARMARK_PLUGINS_DIR,
                    иначе %APPDATA%/darmark/plugins)
      --force     — перезаписать существующую установку

  darmark-plugin --help, -h    — эта справка
  darmark-plugin --version, -V — версия

Примеры:
  darmark-plugin test crates/plugin-catalog/examples/plugins/word-count
  darmark-plugin test . --json
  darmark-plugin init my-plugin --name \"Мой плагин\"
  darmark-plugin list --repo crates/plugin-catalog/examples/plugins
  darmark-plugin install word-count --repo crates/plugin-catalog/examples/plugins
  darmark-plugin link my-plugin --repo crates/plugin-catalog/examples/plugins
  darmark-plugin update";

fn print_usage() {
    println!("{USAGE}");
}
