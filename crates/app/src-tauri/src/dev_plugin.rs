//! Dev-режим `--debug-plugin` (Фаза 6): запуск Lua-плагина в debug-сборке без GUI.

use std::path::PathBuf;

use crate::plugins;

/// Разбирает аргументы процесса и, если передан `--debug-plugin <path>`, исполняет dev-режим.
///
/// Возвращает `Some(код)` только для dev-режима — `main` завершает процесс этим кодом.
/// `None` — обычный запуск GUI: любые другие аргументы (в т.ч. `--help`) игнорируются,
/// как и раньше, и не ломают запуск.
///
/// В релизе `darmark` — GUI-подсистема (`windows_subsystem = "windows"`), консоли нет,
/// поэтому dev-режим рассчитан на debug-сборку (`cargo run -p darmark -- --debug-plugin …`).
pub fn maybe_run_debug_plugin() -> Option<i32> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--debug-plugin" {
            let Some(path) = args.next().filter(|p| !p.starts_with('-')) else {
                eprintln!("darmark: --debug-plugin требует путь к каталогу плагина или .lua-файлу");
                return Some(2);
            };
            return Some(run_debug_plugin(PathBuf::from(path)));
        }
    }
    None
}

/// Запускает child-хост с `--serve-plugin <path>` и **унаследованным** stdio; ждёт завершения.
///
/// Host-call'ы плагина обслуживает разработчик (его stdio), GUI и Job Object не участвуют:
/// это отладочный прогон плагина без помещения в `%APPDATA%` (§7.1 «Dev»). В отличие от
/// [`plugins::supervisor::Supervisor`], здесь нет `CREATE_NO_WINDOW` и пайпов — консоль
/// нужна, чтобы видеть лог child'а.
fn run_debug_plugin(path: PathBuf) -> i32 {
    use std::process::{Command, Stdio};

    let Some(exe) = plugins::supervisor::Supervisor::resolve_child_exe() else {
        eprintln!(
            "darmark: {} не найден рядом с darmark.exe",
            plugins::supervisor::CHILD_EXE_NAME
        );
        return 1;
    };

    match Command::new(&exe)
        .arg("--serve-plugin")
        .arg(&path)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
    {
        Ok(status) => status.code().unwrap_or(1),
        Err(e) => {
            eprintln!("darmark: не запустить {}: {e}", exe.display());
            1
        }
    }
}
