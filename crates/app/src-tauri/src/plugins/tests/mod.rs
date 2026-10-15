//! Тесты Фазы 2: Supervisor на фикстурах `crash`/`hang`/`chatty`/`edit` и карантин.
//!
//! Гоняют реальный `darmark-plugin-host.exe`. Если его нет, тест пытается собрать child
//! (`cargo build -p plugin-host`); это делает `cargo test -p darmark` самодостаточным.

#[cfg(not(debug_assertions))]
compile_error!("тесты плагинов гонят debug-бинарник child; cargo test только без --release");

mod bus;
mod common;
mod listing;
mod loader;
mod notify;
mod permissions;
mod resource;
mod services;
mod settings;
mod status;
mod supervisor;
mod views;
