//! Интеграционный тест Фазы 6: dev-режим `--serve-plugin` через **реальный процесс**.
//!
//! Child читает каталог плагина (или `.lua`), делает `load` + `on_activate` и входит в штатный
//! stdio-цикл конверта. Роль разработчика-хоста играет тест: он шлёт `Invoke` и разбирает `ToHost`.
//! Фикстуры в `on_activate` не делают host-call, поэтому активация не требует ответов.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use plugin_proto::envelope::{self, ToChild, ToHost};
use serde_json::json;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn spawn_serve(path: &Path) -> Child {
    Command::new(env!("CARGO_BIN_EXE_darmark-plugin-host"))
        .arg("--serve-plugin")
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn darmark-plugin-host --serve-plugin")
}

fn read_one<R: Read>(reader: &mut R) -> ToHost {
    envelope::read_to_host(reader).unwrap().unwrap()
}

/// Шлёт `event` (плагин уже активирован) и ждёт `done` с тем же id, затем закрывает stdin
/// и проверяет штатный выход 0. `ready`/`log` пропускаются.
fn event_roundtrip_and_exit(child: &mut Child) {
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();

    envelope::write_to_child(
        &mut stdin,
        &ToChild::Invoke {
            id: 1,
            method: "event".into(),
            args: json!({ "name": "document:changed", "payload": { "doc_id": "d", "rev": 1 } }),
        },
    )
    .unwrap();

    let mut done = false;
    for _ in 0..16 {
        match read_one(&mut stdout) {
            ToHost::Event { kind, payload } if kind == "done" && payload["id"] == 1 => {
                done = true;
                break;
            }
            ToHost::Event { kind, payload } if kind == "error" && payload["id"] == 1 => {
                panic!("event завершился ошибкой: {payload}");
            }
            ToHost::HostCall {
                id: hid, method, ..
            } => panic!("неожиданный host-call #{hid} ({method}): фикстура не должна его делать"),
            ToHost::Event { .. } | ToHost::Log { .. } => {}
        }
    }
    assert!(done, "не дождались done #1");

    // EOF: разработчик закрыл пайп — child завершается штатно.
    drop(stdin);
    let status = child.wait().unwrap();
    assert!(
        status.success(),
        "child обязан выйти с кодом 0, а не {status:?}"
    );
}

#[test]
fn serve_plugin_directory_activates_and_handles_event() {
    let mut child = spawn_serve(&fixture("serve-dir"));
    event_roundtrip_and_exit(&mut child);
}

#[test]
fn serve_plugin_lua_file_activates_and_handles_event() {
    let mut child = spawn_serve(&fixture("hello.lua"));
    event_roundtrip_and_exit(&mut child);
}

#[test]
fn serve_plugin_missing_path_exits_nonzero() {
    let output = Command::new(env!("CARGO_BIN_EXE_darmark-plugin-host"))
        .arg("--serve-plugin")
        .arg(fixture("no-such-plugin"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .expect("spawn darmark-plugin-host --serve-plugin <битый путь>");

    assert!(
        !output.status.success(),
        "битый путь обязан дать ненулевой код, а не {:?}",
        output.status
    );
}
