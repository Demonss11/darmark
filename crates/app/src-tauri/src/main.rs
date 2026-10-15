#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Some(code) = darmark_lib::maybe_run_debug_plugin() {
        std::process::exit(code);
    }
    darmark_lib::run()
}
