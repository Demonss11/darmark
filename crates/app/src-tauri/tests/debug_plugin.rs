//! Интеграционный тест Фазы 6: `darmark --debug-plugin <path>` (Windows).
//!
//! Гоняет реальный `darmark.exe` (debug) с закрытым stdin: dev-режим спавнит
//! `darmark-plugin-host.exe --serve-plugin plugins/word-count`, тот активируется, получает EOF
//! и завершается с кодом 0. Требует собранного child-бинарника — собираем его здесь же
//! (как `plugins/tests.rs`, чтобы `cargo test -p darmark` был самодостаточным).

#![cfg(windows)]

use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Корень репозитория: `CARGO_MANIFEST_DIR` = `crates/app/src-tauri`, три `parent()` до `<repo>`.
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .expect("корень workspace")
        .to_path_buf()
}

#[test]
fn debug_plugin_runs_reference_fixture_without_gui() {
    let root = workspace_root();

    // child резолвится рядом с darmark.exe (target/debug) — собираем из того же workspace.
    let build = Command::new(env!("CARGO"))
        .args(["build", "-p", "plugin-host"])
        .current_dir(&root)
        .status()
        .expect("запуск cargo build -p plugin-host");
    assert!(build.success(), "не удалось собрать plugin-host");

    let plugin_dir = root.join("plugins").join("word-count");
    let status = Command::new(env!("CARGO_BIN_EXE_darmark"))
        .arg("--debug-plugin")
        .arg(&plugin_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("spawn darmark --debug-plugin");

    assert!(
        status.success(),
        "dev-режим обязан прогнать фикстуру и выйти с кодом 0, а не {status:?}"
    );
}
