//! Бенчмарк латентности range/delta-API против N1 (полный документ).
//!
//! Спавнит реальный child-процесс и измеряет round-trip латентность:
//! - N1: один вызов get_document_text (один большой ответ ~1 МБ)
//! - range/delta: N вызовов get_document_range (окна по 64 КБ) для того же объёма
//!
//! Ответы host-функции генерирует сам тест (как это делает DocumentServices::handle).

use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};
use std::time::Instant;

use plugin_core::envelope::{self, ToChild, ToHost};
use serde_json::json;

const DOC_SIZE: usize = 1024 * 1024; // 1 МБ
const WINDOW: usize = 65536; // 64 КБ, как WINDOW в примерах плагинов

type Spawn = (Child, std::process::ChildStdout, std::process::ChildStdin);

fn spawn_child() -> Spawn {
    let mut child = Command::new(env!("CARGO_BIN_EXE_darmark-plugin-host"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn darmark-plugin-host");
    let stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    (child, stdout, stdin)
}

fn read_one<R: Read>(reader: &mut R) -> ToHost {
    envelope::read_to_host(reader).unwrap().unwrap()
}

fn expect_event<R: Read>(reader: &mut R, id: u32) {
    loop {
        match read_one(reader) {
            ToHost::Event { kind, payload } if kind == "done" && payload["id"] == id => return,
            ToHost::Event { kind, payload } if kind == "error" && payload["id"] == id => {
                panic!("invoke #{id} заверсился ошибкой: {payload}");
            }
            ToHost::Event { .. } | ToHost::Log { .. } => {}
            ToHost::HostCall {
                id: hid, method, ..
            } => {
                panic!("неожиданный HostCall #{hid} ({method}) до done #{id}");
            }
        }
    }
}

/// Загружает плагин и активирует его. Возвращает (child, stdout, stdin).
fn load_and_activate(source: &str) -> Spawn {
    let (child, mut stdout, mut stdin) = spawn_child();

    envelope::write_to_child(
        &mut stdin,
        &ToChild::Invoke {
            id: 1,
            method: "load".into(),
            args: json!({ "source": source, "plugin_id": "bench" }),
        },
    )
    .unwrap();
    expect_event(&mut stdout, 1);

    envelope::write_to_child(
        &mut stdin,
        &ToChild::Invoke {
            id: 2,
            method: "activate".into(),
            args: json!(null),
        },
    )
    .unwrap();
    expect_event(&mut stdout, 2);

    (child, stdout, stdin)
}

/// Ответ `get_document_text` — имитация DocumentServices::handle: JSON с полным текстом.
fn reply_full<W: Write>(stdin: &mut W, id: u32, text: &str) {
    envelope::write_to_child(
        stdin,
        &ToChild::Reply {
            id,
            result: Ok(json!(text)),
        },
    )
    .unwrap();
}

/// Ответ на `get_document_range` — имитация DocumentServices::handle: JSON с окном.
fn reply_range<W: Write>(stdin: &mut W, id: u32, text: &str, start: usize, len: usize) {
    let end = (start + len).min(text.len());
    envelope::write_to_child(
        stdin,
        &ToChild::Reply {
            id,
            result: Ok(json!(&text[start..end])),
        },
    )
    .unwrap();
}

/// Генерирует "документ" ровно 1 МБ — ASCII-паттерн длиной 256 байт (делит DOC_SIZE нацело).
fn make_doc() -> String {
    let unit: String = (0..256u16)
        .map(|i| (b'a' + (i % 26) as u8) as char)
        .collect();
    let repeats = DOC_SIZE / unit.len();
    unit.repeat(repeats)
}

/// Замеряет латентность N1: один вызов get_document_text на документ 1 МБ.
fn bench_n1(doc: &str) -> std::time::Duration {
    let source = r#"
        function on_activate(ctx)
        end

        function on_event(name, payload)
            if name == "document:changed" then
                local text, err = host.get_document_text(payload.doc_id)
                if err then
                    host.log("error", "get_document_text failed: " .. err.message)
                end
            end
        end
    "#;

    let (mut child, mut stdout, mut stdin) = load_and_activate(source);

    let start = Instant::now();

    envelope::write_to_child(
        &mut stdin,
        &ToChild::Invoke {
            id: 10,
            method: "event".into(),
            args: json!({ "name": "document:changed", "payload": { "doc_id": "d", "rev": 1 } }),
        },
    )
    .unwrap();

    // Ждём HostCall → отвечаем → ждём done
    match read_one(&mut stdout) {
        ToHost::HostCall { id, method, .. } => {
            assert_eq!(method, "get_document_text");
            reply_full(&mut stdin, id, doc);
        }
        other => panic!("ожидался HostCall get_document_text, получено: {other:?}"),
    }
    expect_event(&mut stdout, 10);

    let elapsed = start.elapsed();

    drop(stdin);
    let _ = child.wait();

    elapsed
}

/// Замеряет латентность range/delta: N вызовов get_document_range для документа 1 МБ.
fn bench_range_delta(doc: &str) -> std::time::Duration {
    let source = r#"
        function on_activate(ctx)
        end

        function on_event(name, payload)
            if name == "document:changed" then
                local pos = 0
                while true do
                    local chunk, err = host.get_document_range(payload.doc_id, pos, 65536)
                    if err then
                        host.log("error", "get_document_range failed at " .. pos .. ": " .. err.message)
                        break
                    end
                    if #chunk == 0 then break end
                    pos = pos + #chunk
                    if pos >= 1048576 then break end
                end
            end
        end
    "#;

    let (mut child, mut stdout, mut stdin) = load_and_activate(source);

    let start = Instant::now();

    envelope::write_to_child(
        &mut stdin,
        &ToChild::Invoke {
            id: 20,
            method: "event".into(),
            args: json!({ "name": "document:changed", "payload": { "doc_id": "d", "rev": 1 } }),
        },
    )
    .unwrap();

    // Отвечаем на каждый HostCall get_document_range, потом ждём done
    let mut pos = 0;
    loop {
        match read_one(&mut stdout) {
            ToHost::HostCall { id, method, .. } if method == "get_document_range" => {
                reply_range(&mut stdin, id, doc, pos, WINDOW);
                pos += WINDOW;
            }
            ToHost::Event { kind, payload } if kind == "done" && payload["id"] == 20 => break,
            ToHost::Event { kind, payload } if kind == "error" && payload["id"] == 20 => {
                panic!("event #20 заверсился ошибкой: {payload}");
            }
            ToHost::Event { .. } | ToHost::Log { .. } => {}
            ToHost::HostCall { id, method, .. } => {
                panic!("неожиданный HostCall #{id} ({method})");
            }
        }
    }

    let elapsed = start.elapsed();

    drop(stdin);
    let _ = child.wait();

    elapsed
}

#[test]
fn latency_range_vs_n1() {
    let doc = make_doc();
    assert_eq!(doc.len(), DOC_SIZE);

    // Прогрев (инициализация процессов)
    let _ = bench_n1(&doc);
    let _ = bench_range_delta(&doc);

    // Основные прогоны
    let mut n1_times = Vec::new();
    let mut range_times = Vec::new();

    for _ in 0..5 {
        n1_times.push(bench_n1(&doc));
        range_times.push(bench_range_delta(&doc));
    }

    let n1_median = n1_times[n1_times.len() / 2];
    let range_median = range_times[range_times.len() / 2];

    let windows = DOC_SIZE / WINDOW;

    eprintln!("=== Латентность range/delta vs N1 (документ 1 МБ) ===");
    eprintln!("N1 (get_document_text):       {:?}", n1_median);
    eprintln!("range/delta ({windows} окон по 64 КБ): {:?}", range_median);
    eprintln!(
        "Отношение range/N1:           {:.2}",
        range_median.as_secs_f64() / n1_median.as_secs_f64()
    );
}
