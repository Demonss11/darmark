//! Команды CLI. Каждая команда — свой модуль; логика разбора аргументов живёт в `args`.
//!
//! Модули приватные: наружу торчат только `run_*`, которые диспетчерит `main`.

mod catalog;
mod init;
mod test;

pub use catalog::{run_install, run_link, run_list, run_remove, run_update};
pub use init::run_init;
pub use test::run_test;
