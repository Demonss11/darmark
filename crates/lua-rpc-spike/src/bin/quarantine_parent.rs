//! Parent ADR-0021 §3: **карантин N=3** и **формулировка границы изоляции**.
//!
//! Симулирует жизненный цикл супервизора: плагин запускается повторно; неудачный запуск
//! (краш/зависание/отказ) копит серию, успешный — сбрасывает. После `--threshold` неудач
//! **подряд** плагин автоотключается (карантин) и больше не перезапускается. Плюс печатает
//! канонические предупреждения для UI/магазина (`permission_notices`, §11.4).
//!
//! Гейт: `crash` → карантин ровно после 3 попыток; `hello` → не карантинится; серия
//! `crash,crash,hello,crash` показывает сброс счётчика успехом.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use lua_rpc_spike::job::Job;
use lua_rpc_spike::{
    parse_event, permission_notices, put_bytes, put_u32, read_frame, serve_host_call, write_frame,
    Event, Quarantine, CMD_REPLY, CMD_RUN,
};

struct Args {
    child: PathBuf,
    plugins: PathBuf,
    /// Имена фикстур через запятую — порядок запусков.
    sequence: Vec<String>,
    threshold: u32,
    timeout: Duration,
    permissions: Vec<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut child = None;
    let mut plugins = PathBuf::from("crates/lua-rpc-spike/plugins");
    let mut sequence = Vec::new();
    let mut threshold = 3u32;
    let mut timeout = Duration::from_millis(5000);
    let mut permissions: Vec<String> = Vec::new();
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--child" => child = Some(PathBuf::from(it.next().ok_or("--child требует путь")?)),
            "--plugins" => plugins = PathBuf::from(it.next().ok_or("--plugins требует путь")?),
            "--sequence" => {
                let s = it.next().ok_or("--sequence требует список")?;
                sequence = s.split(',').map(|x| x.trim().to_string()).collect();
            }
            "--permissions" => {
                let s = it.next().ok_or("--permissions требует список")?;
                permissions = s.split(',').map(|x| x.trim().to_string()).collect();
            }
            "--threshold" => {
                threshold = it
                    .next()
                    .ok_or("--threshold требует число")?
                    .parse()
                    .map_err(|_| "--threshold: целое".to_string())?
            }
            "--timeout-ms" => {
                let ms: u64 = it
                    .next()
                    .ok_or("--timeout-ms требует число")?
                    .parse()
                    .map_err(|_| "--timeout-ms: целое".to_string())?;
                timeout = Duration::from_millis(ms);
            }
            other => return Err(format!("неизвестный аргумент: {other}")),
        }
    }
    if sequence.is_empty() {
        return Err("--sequence пуст".into());
    }
    Ok(Args {
        child: child.ok_or("не указан --child <exe>")?,
        plugins,
        sequence,
        threshold,
        timeout,
        permissions,
    })
}

fn build_document(size: usize) -> String {
    const UNIT: &str = "lorem ipsum dolor sit amet ";
    let mut s = String::with_capacity(size + UNIT.len());
    while s.len() < size {
        s.push_str(UNIT);
    }
    s.truncate(size);
    s
}

enum Outcome {
    Success,
    Failure(&'static str),
}

fn read_plugin(dir: &Path, name: &str) -> Result<Vec<u8>, String> {
    let p = dir.join(name);
    std::fs::read(&p).map_err(|e| format!("не прочитать {}: {e}", p.display()))
}

/// Один запуск плагина в отдельном child-процессе (Job с `KILL_ON_JOB_CLOSE`).
fn run_once(
    exe: &Path,
    source: &[u8],
    document: &[u8],
    timeout: Duration,
) -> std::io::Result<Outcome> {
    let job = Job::new()?;
    job.set_limits(0, 0)?; // только KILL_ON_JOB_CLOSE
    let mut child = Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    job.assign_pid(child.id())?;

    let mut to_child = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let reader = thread::spawn(move || {
        let mut r = stdout;
        while let Ok(p) = read_frame(&mut r) {
            if tx.send(p).is_err() {
                break;
            }
        }
    });

    let mut msg = vec![CMD_RUN];
    put_bytes(&mut msg, source);
    write_frame(&mut to_child, &msg)?;

    let outcome = loop {
        match rx.recv_timeout(timeout) {
            Ok(p) => match parse_event(&p) {
                Ok(Event::HostCall { id, method, arg }) => {
                    let result = serve_host_call(method, arg, document);
                    let mut reply = vec![CMD_REPLY];
                    put_u32(&mut reply, id);
                    reply.push(0);
                    put_bytes(&mut reply, &result);
                    if write_frame(&mut to_child, &reply).is_err() {
                        break Outcome::Failure("write");
                    }
                }
                Ok(Event::Done { status, .. }) => {
                    break if status == 0 {
                        Outcome::Success
                    } else {
                        Outcome::Failure("lua-error")
                    };
                }
                Ok(_) => {}
                Err(_) => {
                    job.terminate(1);
                    break Outcome::Failure("protocol");
                }
            },
            Err(RecvTimeoutError::Timeout) => {
                job.terminate(1);
                break Outcome::Failure("timeout");
            }
            Err(RecvTimeoutError::Disconnected) => break Outcome::Failure("crash"),
        }
    };

    let _ = child.wait();
    reader.join().ok();
    Ok(outcome)
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            println!("RESULT error={e}");
            std::process::exit(2);
        }
    };
    let document = build_document(4096);

    // Формулировка границы изоляции (§11.4): единый источник для UI/магазина.
    let perms: Vec<&str> = args.permissions.iter().map(|s| s.as_str()).collect();
    for notice in permission_notices(&perms) {
        println!("NOTICE {notice}");
    }

    let mut q = Quarantine::new(args.threshold);
    let mut runs = 0usize;
    let mut skipped = 0usize;
    let mut success = 0usize;
    let mut failures = 0usize;

    for name in &args.sequence {
        if q.is_disabled() {
            skipped += 1;
            println!("SKIP name={name} reason=quarantined");
            continue;
        }
        let source = match read_plugin(&args.plugins, name) {
            Ok(s) => s,
            Err(e) => {
                println!("RESULT error={e}");
                std::process::exit(2);
            }
        };
        runs += 1;
        match run_once(&args.child, &source, document.as_bytes(), args.timeout) {
            Ok(Outcome::Success) => {
                q.record_success();
                success += 1;
                println!(
                    "STEP name={name} outcome=success failures={} disabled={}",
                    q.consecutive_failures(),
                    q.is_disabled() as u8
                );
            }
            Ok(Outcome::Failure(kind)) => {
                q.record_failure();
                failures += 1;
                println!(
                    "STEP name={name} outcome=failure({kind}) failures={} disabled={}",
                    q.consecutive_failures(),
                    q.is_disabled() as u8
                );
                if q.is_disabled() {
                    println!("QUARANTINE name={name} after={} failures", args.threshold);
                }
            }
            Err(e) => {
                println!("RESULT error=run:{e}");
                std::process::exit(2);
            }
        }
    }

    println!(
        "RESULT threshold={} runs={} success={} failures={} skipped={} disabled={}",
        args.threshold,
        runs,
        success,
        failures,
        skipped,
        q.is_disabled() as u8
    );
}
