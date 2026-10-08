//! GUI-parent C-спайка (P9/M10): тот же round-trip, что `rpc-parent`, но сам parent
//! собран как GUI-приложение (`windows_subsystem = "windows"`, subsystem 2) — как `mdedit.exe`.
//!
//! Зачем отдельный бинарь: F21 показал, что спайк мерял пару «консоль+консоль», а продукт —
//! GUI-parent без консоли. Гейт M10: round-trip (в т.ч. кадр 10 МБ) работает без консоли, а при
//! обрыве child parent видит закрытие границы и не виснет.
//!
//! Отчёта в stdout нет (у GUI нет консоли) — результат пишется в `--report <path>` и в код выхода.
//! Пайпы создаёт `Stdio::piped()` (std): CreatePipe + STARTUPINFO с hStd* + `bInheritHandles=TRUE`.
//! `CREATE_NO_WINDOW` (флаг `--no-window`, по умолчанию вкл.) — child тоже без консоли: тест «без
//! консоли» жёстче, но stdio-хэндлы всё равно наследуются через STARTUPINFO.

// Сам parent — GUI-приложение (subsystem 2), как mdedit.exe: консоли нет.
#![cfg_attr(windows, windows_subsystem = "windows")]

use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use lua_rpc_spike::{
    parse_event, put_bytes, put_u32, read_frame, serve_host_call, write_frame, Event, CMD_REPLY,
    CMD_RUN,
};

/// CREATE_NO_WINDOW: child — консольный subsystem, но без окна/консоли.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

struct Args {
    child: PathBuf,
    plugin: PathBuf,
    doc_size: usize,
    timeout: Duration,
    report: PathBuf,
    /// "roundtrip" — обычный прогон; "break" — убить child и проверить, что parent не виснет.
    mode: String,
    no_window: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut child = None;
    let mut plugin = None;
    let mut doc_size = 1024usize;
    let mut timeout = Duration::from_millis(3000);
    let mut report = None;
    let mut mode = "roundtrip".to_string();
    let mut no_window = true;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--child" => child = Some(PathBuf::from(it.next().ok_or("--child требует путь")?)),
            "--plugin" => plugin = Some(PathBuf::from(it.next().ok_or("--plugin требует путь")?)),
            "--report" => report = Some(PathBuf::from(it.next().ok_or("--report требует путь")?)),
            "--mode" => mode = it.next().ok_or("--mode требует значение")?,
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
            "--no-window" => no_window = true,
            "--with-console" => no_window = false,
            other => return Err(format!("неизвестный аргумент: {other}")),
        }
    }
    Ok(Args {
        child: child.ok_or("не указан --child <exe>")?,
        plugin: plugin.ok_or("не указан --plugin <path>")?,
        doc_size,
        timeout,
        report: report.ok_or("не указан --report <path>")?,
        mode,
        no_window,
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

fn spawn_child(args: &Args) -> std::io::Result<Child> {
    let mut cmd = Command::new(&args.child);
    // Явные пайпы + NUL: std создаёт CreatePipe и отдаёт их через STARTUPINFO с bInheritHandles.
    // Ничего не наследуем из консоли parent — у GUI-parent её нет.
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    if args.no_window {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.spawn()
}

/// Пишет отчёт (GUI-parent не может в stdout).
fn write_report(path: &PathBuf, lines: &[String]) -> std::io::Result<()> {
    let mut s = lines.join("\n");
    s.push('\n');
    fs::write(path, s)
}

fn run(args: &Args) -> Result<Vec<String>, String> {
    let source = fs::read(&args.plugin)
        .map_err(|e| format!("не прочитать {}: {e}", args.plugin.display()))?;
    let document = build_document(args.doc_size);

    let mut log = vec![
        format!("mode={}", args.mode),
        format!("child={}", args.child.display()),
        format!("plugin={}", args.plugin.display()),
        format!("doc_size={}", args.doc_size),
        format!("no_window={}", args.no_window as u8),
        // subsystem GUI-parent'а: 2 = windows. Значение читается тестом-обёрткой из PE.
        "parent_subsystem=gui".to_string(),
    ];

    let spawn_t = Instant::now();
    let mut child = spawn_child(args).map_err(|e| format!("не запустить child: {e}"))?;
    let spawn_ms = spawn_t.elapsed().as_secs_f64() * 1000.0;
    log.push(format!("spawn_ms={spawn_ms:.2}"));

    // Забираем stdin себе: `Child::wait()` закрывает собственный stdin, а нам нужно записать в
    // уже мёртвого child и увидеть BrokenPipe (а не None). Свой handle при этом остаётся открыт.
    let mut to_child = child.stdin.take().ok_or("нет stdin child")?;
    let stdout = child.stdout.take().ok_or("нет stdout child")?;
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
                // EOF или ошибка чтения — граница закрыта; поток завершается.
                Err(_) => break,
            }
        }
    });

    let run_t = Instant::now();
    let mut run_msg = vec![CMD_RUN];
    put_bytes(&mut run_msg, &source);
    write_frame(&mut to_child, &run_msg).map_err(|e| format!("не отправить CMD_RUN: {e}"))?;

    let mut doc_bytes: u64 = 0;

    // Ждём READY. Если child не поднялся — это провал round-trip.
    match rx.recv_timeout(args.timeout) {
        Ok(p) => {
            match parse_event(&p) {
                Ok(Event::Ready) => {}
                Ok(_) => return Err("первый кадр — не EV_READY".into()),
                Err(e) => return Err(format!("нарушение протокола: {e}")),
            }
            let ms = run_t.elapsed().as_secs_f64() * 1000.0;
            log.push(format!("init_ms={ms:.2}"));
        }
        Err(RecvTimeoutError::Timeout) => {
            let _ = child.kill();
            log.push("roundtrip_ok=0".into());
            log.push("error=timeout_ready".into());
            let _ = child.wait();
            reader.join().ok();
            return Ok(log);
        }
        Err(RecvTimeoutError::Disconnected) => {
            log.push("roundtrip_ok=0".into());
            log.push("error=child_died_before_ready".into());
            let _ = child.wait();
            reader.join().ok();
            return Ok(log);
        }
    }

    // --- Режим обрыва: убить child, затем убедиться, что parent видит закрытие и не виснет. ---
    if args.mode == "break" {
        let kill_t = Instant::now();
        let _ = child.kill();
        let _ = child.wait();
        log.push(format!(
            "kill_ms={:.2}",
            kill_t.elapsed().as_secs_f64() * 1000.0
        ));

        // (1) Parent пытается писать крупный кадр в мёртвый child: ожидаем немедленную ошибку,
        //     а не вечную блокировку на заполненном буфере пайпа.
        let payload = vec![0u8; args.doc_size];
        let big = {
            let mut v = vec![CMD_REPLY];
            put_u32(&mut v, 1);
            v.push(0);
            put_bytes(&mut v, &payload);
            v
        };
        let wt = Instant::now();
        let werr = write_frame(&mut to_child, &big).err();
        let write_ms = wt.elapsed().as_secs_f64() * 1000.0;
        log.push(format!("write_after_kill_ms={write_ms:.2}"));
        log.push(format!(
            "write_after_kill_err={}",
            werr.as_ref()
                .map(|e| e.kind().to_string())
                .unwrap_or_else(|| "none".into())
        ));

        // (2) Reader должен получить EOF → канал закрыт → Disconnected без ожидания полного таймаута.
        let dt = Instant::now();
        let disconnected = loop {
            match rx.recv_timeout(args.timeout) {
                Ok(_) => continue,
                Err(RecvTimeoutError::Disconnected) => break true,
                Err(RecvTimeoutError::Timeout) => break false,
            }
        };
        let disconnect_ms = dt.elapsed().as_secs_f64() * 1000.0;
        log.push(format!("disconnect_detected={}", disconnected as u8));
        log.push(format!("disconnect_ms={disconnect_ms:.2}"));

        // Гейт: не виснет. Ошибка записи ожидаема, дисконнект обязателен, таймаут не исчерпан.
        let hung = !disconnected || disconnect_ms >= args.timeout.as_secs_f64() * 1000.0;
        let write_ok = werr.is_some();
        log.push(format!("hang={}", hung as u8));
        log.push(format!("roundtrip_ok={}", (!hung && write_ok) as u8));
        reader.join().ok();
        return Ok(log);
    }

    // --- Обрыв посреди крупной записи (N2): child жив, но не читает (spin.lua) — parent
    //     упирается в заполненный буфер пайпа. Watchdog убивает child; запись обязана
    //     разблокироваться ошибкой, а не висеть. Это и есть «большой кадр + обрыв». ---
    if args.mode == "break-write" {
        let payload = vec![0u8; args.doc_size];
        let big = {
            let mut v = vec![CMD_REPLY];
            put_u32(&mut v, 1);
            v.push(0);
            put_bytes(&mut v, &payload);
            v
        };

        // Watchdog владеет Child и убивает его, пока main заблокирован в записи.
        let watchdog = thread::spawn(move || {
            thread::sleep(Duration::from_millis(150));
            let _ = child.kill();
            let _ = child.wait();
        });

        let wt = Instant::now();
        let werr = write_frame(&mut to_child, &big).err();
        let write_ms = wt.elapsed().as_secs_f64() * 1000.0;
        watchdog.join().ok();
        log.push(format!("write_ms={write_ms:.2}"));
        log.push(format!(
            "write_err={}",
            werr.as_ref()
                .map(|e| e.kind().to_string())
                .unwrap_or_else(|| "none".into())
        ));

        // Reader должен получить EOF после гибели child.
        let dt = Instant::now();
        let disconnected = loop {
            match rx.recv_timeout(args.timeout) {
                Ok(_) => continue,
                Err(RecvTimeoutError::Disconnected) => break true,
                Err(RecvTimeoutError::Timeout) => break false,
            }
        };
        let disconnect_ms = dt.elapsed().as_secs_f64() * 1000.0;
        log.push(format!("disconnect_detected={}", disconnected as u8));
        log.push(format!("disconnect_ms={disconnect_ms:.2}"));

        let write_err = werr.is_some();
        let hung = !disconnected || write_ms >= args.timeout.as_secs_f64() * 1000.0;
        log.push(format!("hang={}", hung as u8));
        log.push(format!("roundtrip_ok={}", (!hung && write_err) as u8));
        reader.join().ok();
        return Ok(log);
    }

    // --- Обычный round-trip: обслуживаем host-вызовы, пока child не пришлёт EV_DONE. ---
    let outcome: Option<String> = loop {
        match rx.recv_timeout(args.timeout) {
            Ok(p) => {
                match parse_event(&p) {
                    Ok(Event::Log { level, message }) => {
                        log.push(format!("child_log[{level}]={message}"));
                    }
                    Ok(Event::HostCall { id, method, arg }) => {
                        // Основной сценарий M10 — кадр размером с документ (10 МБ) обратно в child.
                        let result = serve_host_call(method, arg, document.as_bytes());
                        doc_bytes += result.len() as u64;
                        let mut reply = vec![CMD_REPLY];
                        put_u32(&mut reply, id);
                        reply.push(0);
                        put_bytes(&mut reply, &result);
                        if let Err(e) = write_frame(&mut to_child, &reply) {
                            break Some(format!("ошибка записи ответа: {e}"));
                        }
                    }
                    Ok(Event::Done { status, summary }) => {
                        break Some(format!("status={status} {summary}"));
                    }
                    Ok(_) => {}
                    Err(e) => {
                        // F36: нарушение протокола — отказ, не паника.
                        let _ = child.kill();
                        break Some(format!("нарушение протокола: {e}"));
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                let _ = child.kill();
                break Some(format!("таймаут {} мс", args.timeout.as_millis()));
            }
            Err(RecvTimeoutError::Disconnected) => {
                break Some("child закрыл поток".into());
            }
        }
    };

    let run_ms = run_t.elapsed().as_secs_f64() * 1000.0;
    log.push(format!("run_ms={run_ms:.2}"));
    log.push(format!("doc_bytes={doc_bytes}"));
    let outcome = outcome.unwrap_or_else(|| "нет итога".into());
    log.push(format!("outcome={}", outcome.replace(['\n', '\r'], " ")));
    // Round-trip успешен, если child сам прислал DONE со status=0 (а не таймаут/обрыв).
    let ok = outcome.starts_with("status=0");
    log.push(format!("roundtrip_ok={}", ok as u8));

    let _ = child.wait();
    reader.join().ok();
    Ok(log)
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(_) => std::process::exit(2),
    };
    let report = args.report.clone();
    // GUI-процесс: панику не видно. Кладём её рядом с отчётом (до `abort` хук успевает).
    {
        let panic_path = report.with_extension("panic");
        std::panic::set_hook(Box::new(move |info| {
            let _ = fs::write(&panic_path, format!("{info}\n"));
        }));
    }
    let code = match run(&args) {
        Ok(log) => {
            let ok = log.iter().any(|l| l == "roundtrip_ok=1");
            let _ = write_report(&report, &log);
            if ok {
                0
            } else {
                1
            }
        }
        Err(e) => {
            let _ = write_report(&report, &[format!("error={e}"), "roundtrip_ok=0".into()]);
            1
        }
    };
    std::process::exit(code);
}
