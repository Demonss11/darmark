//! Dev-режим `--debug-plugin` (Фаза 6): запуск Lua-плагина в debug-сборке без GUI.

use std::path::PathBuf;

use crate::plugins;









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
