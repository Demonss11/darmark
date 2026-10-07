// Windows subsystem="windows" убирает консольное окно в релизе (как у Notepad++)
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Dev-режим Фазы 6 (`--debug-plugin`) разбирается до инициализации Tauri: при нём процесс
    // завершается кодом child-хоста; иначе — обычный запуск GUI. Прочие аргументы игнорируются.
    if let Some(code) = darmark_lib::maybe_run_debug_plugin() {
        std::process::exit(code);
    }
    darmark_lib::run()
}
