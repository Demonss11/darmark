//! Parent C-спайка: запускает child, обслуживает его host-вызовы (заглушка-документ
//! в памяти) и меряет spawn/init/round-trip/передачу большого документа,
//! а также изоляцию (kill по таймауту, крах child).

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use lua_rpc_spike::{
    parse_event, put_bytes, put_u32, read_frame, serve_host_call, write_frame, Event, CMD_REPLY,
    CMD_RUN,
};

struct Args {
    child: PathBuf,
    plugin: PathBuf,
    doc_size: usize,
    timeout: Duration,
}

fn parse_args() -> Result<Args, String> {
    let mut child = None;
    let mut plugin = None;
    let mut doc_size = 1024usize;
    let mut timeout = Duration::from_millis(3000);
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--child" => child = Some(PathBuf::from(it.next().ok_or("--child требует путь")?)),
            "--plugin" => plugin = Some(PathBuf::from(it.next().ok_or("--plugin требует путь")?)),
            "--doc-size" => {
                doc_size = it
                    .next()
                    .ok_or("--doc-size требует число")?
                    .parse()
                    .map_err(|_| "--doc-size: целое".to_string())?
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
    Ok(Args {
        child: child.ok_or("не указан --child <exe>")?,
        plugin: plugin.ok_or("не указан --plugin <path>")?,
        doc_size,
        timeout,
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

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("[parent] ошибка: {e}");
            std::process::exit(2);
        }
    };

    let source = match std::fs::read(&args.plugin) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[parent] не прочитать {}: {e}", args.plugin.display());
            std::process::exit(2);
        }
    };
    let document = build_document(args.doc_size);
    eprintln!(
        "[parent] child={} plugin={} doc_size={} timeout={}ms",
        args.child.display(),
        args.plugin.display(),
        args.doc_size,
        args.timeout.as_millis()
    );

    let spawn_t = Instant::now();
    let mut child = Command::new(&args.child)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap_or_else(|e| {
            eprintln!("[parent] не запустить child: {e}");
            std::process::exit(2);
        });
    let spawn_ms = spawn_t.elapsed().as_secs_f64() * 1000.0;

    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let reader = thread::spawn(move || {
        let mut r = stdout;
        loop {
            match read_frame(&mut r) {
                Ok(p) => {
                    if tx.send(p).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let run_t = Instant::now();
    let mut run_msg = vec![CMD_RUN];
    put_bytes(&mut run_msg, &source);
    if let Err(e) = write_frame(child.stdin.as_mut().unwrap(), &run_msg) {
        eprintln!("[parent] не отправить CMD_RUN: {e}");
    }

    let mut init_ms: Option<f64> = None;
    let outcome: String;

    loop {
        match rx.recv_timeout(args.timeout) {
            Ok(payload) => {
                match parse_event(&payload) {
                    Ok(Event::Ready) => {
                        let ms = run_t.elapsed().as_secs_f64() * 1000.0;
                        init_ms = Some(ms);
                        eprintln!("[parent] child готов (init) за {ms:.2} мс");
                    }
                    Ok(Event::Log { level, message }) => {
                        eprintln!("[child:{level}] {message}");
                    }
                    Ok(Event::HostCall { id, method, arg }) => {
                        let result = serve_host_call(method, arg, document.as_bytes());
                        let mut reply = vec![CMD_REPLY];
                        put_u32(&mut reply, id);
                        reply.push(0);
                        put_bytes(&mut reply, &result);
                        if let Err(e) = write_frame(child.stdin.as_mut().unwrap(), &reply) {
                            outcome = format!("ошибка записи ответа: {e}");
                            break;
                        }
                    }
                    Ok(Event::Done { status, summary }) => {
                        outcome = format!("status={status} {summary}");
                        break;
                    }
                    Ok(Event::Unknown(tag)) => eprintln!("[parent] неизвестный тег {tag}"),
                    Err(e) => {
                        // F36: нарушение протокола — отказ плагина, не паника.
                        outcome = format!("нарушение протокола: {e}");
                        let _ = child.kill();
                        break;
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                outcome = format!("таймаут {} мс → kill child", args.timeout.as_millis());
                let _ = child.kill();
                break;
            }
            Err(RecvTimeoutError::Disconnected) => {
                outcome = "child закрыл поток (вероятно, упал/абортнул) → parent жив".to_string();
                break;
            }
        }
    }

    let run_ms = run_t.elapsed().as_secs_f64() * 1000.0;
    let _ = child.wait();
    reader.join().ok();

    eprintln!("[parent] ── итог ──");
    eprintln!("[parent] spawn_ms={spawn_ms:.2}");
    eprintln!(
        "[parent] init_ms={}",
        init_ms
            .map(|v| format!("{v:.2}"))
            .unwrap_or_else(|| "n/a".into())
    );
    eprintln!("[parent] run_ms={run_ms:.2}");
    eprintln!("[parent] outcome: {outcome}");
}
