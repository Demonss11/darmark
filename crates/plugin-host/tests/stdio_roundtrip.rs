//! Интеграционный тест Фазы 1: round-trip `on_activate` через **реальный процесс**.
//!
//! Спавнит бинарник `darmark-plugin-host`, гоняет конверт `plugin-proto::envelope` поверх
//! назначенного stdio: `load` → `activate`, обработка `HostCall` → `Reply`, подтверждение
//! `Event{done}`. GUI не участвует — изоляция процесса проверяется как есть.

use std::io::Read;
use std::process::{Command, Stdio};

use plugin_proto::envelope::{self, ToChild, ToHost};
use serde_json::json;

fn read_one<R: Read>(reader: &mut R) -> ToHost {
    envelope::read_to_host(reader).unwrap().unwrap()
}

/// Ждёт `Event{kind:"done", payload.id == id}`. Паникует на `error` с тем же id.
fn expect_done<R: Read>(reader: &mut R, id: u32) {
    for _ in 0..16 {
        match read_one(reader) {
            ToHost::Event { kind, payload } if kind == "done" && payload["id"] == id => return,
            ToHost::Event { kind, payload } if kind == "error" && payload["id"] == id => {
                panic!("invoke #{id} завершился ошибкой: {payload}");
            }
            ToHost::Event { .. } => {}
            ToHost::Log { .. } => {}
            ToHost::HostCall {
                id: hid, method, ..
            } => {
                panic!("неожиданный HostCall #{hid} ({method}) до done #{id}");
            }
        }
    }
    panic!("не дождались done #{id}");
}

#[test]
fn stdio_load_activate_and_hostcall_roundtrip() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_darmark-plugin-host"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn darmark-plugin-host");
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();

    let source = r#"
        function on_activate(ctx)
            local window = host.get_document_range("d", 0, 5)
            host.log("info", "range=" .. window)
        end
    "#;

    envelope::write_to_child(
        &mut stdin,
        &ToChild::Invoke {
            id: 1,
            method: "load".into(),
            args: json!({ "source": source, "plugin_id": "itest" }),
        },
    )
    .unwrap();
    expect_done(&mut stdout, 1);

    envelope::write_to_child(
        &mut stdin,
        &ToChild::Invoke {
            id: 2,
            method: "activate".into(),
            args: json!(null),
        },
    )
    .unwrap();

    let mut saw_hostcall = false;
    let mut activate_ok = false;
    for _ in 0..16 {
        match read_one(&mut stdout) {
            ToHost::HostCall { id, method, args } => {
                assert_eq!(method, "get_document_range");
                assert_eq!(args["doc_id"], "d");
                assert_eq!(args["start"], 0);
                assert_eq!(args["len"], 5);
                saw_hostcall = true;
                envelope::write_to_child(
                    &mut stdin,
                    &ToChild::Reply {
                        id,
                        result: Ok(json!("hello")),
                    },
                )
                .unwrap();
            }
            ToHost::Event { kind, payload } if kind == "done" && payload["id"] == 2 => {
                activate_ok = true;
                break;
            }
            ToHost::Event { kind, payload } if kind == "error" && payload["id"] == 2 => {
                panic!("activate вернул ошибку: {payload}");
            }
            ToHost::Event { .. } | ToHost::Log { .. } => {}
        }
    }
    assert!(saw_hostcall, "child обязан запросить get_document_range");
    assert!(
        activate_ok,
        "activate обязан подтвердиться событием done #2"
    );

    drop(stdin);
    let _ = child.wait();
}

#[test]
fn stdio_unknown_method_reports_error() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_darmark-plugin-host"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn darmark-plugin-host");
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();

    envelope::write_to_child(
        &mut stdin,
        &ToChild::Invoke {
            id: 7,
            method: "nonsense".into(),
            args: json!(null),
        },
    )
    .unwrap();

    let mut got_error = false;
    for _ in 0..8 {
        match read_one(&mut stdout) {
            ToHost::Event { kind, payload } if kind == "error" && payload["id"] == 7 => {
                assert_eq!(payload["error"]["code"], "protocol");
                got_error = true;
                break;
            }
            ToHost::Event { .. } | ToHost::Log { .. } => {}
            ToHost::HostCall { .. } => {}
        }
    }
    assert!(got_error, "неизвестный метод должен вернуть protocol-error");

    drop(stdin);
    let _ = child.wait();
}
