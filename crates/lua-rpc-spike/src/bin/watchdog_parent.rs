//! Parent D16 (ADR-0021): watchdog над child-плагином — **прогресс-таймаут + абсолютный
//! дедлайн invocation** поверх Job Object.
//!
//! Зачем два бюджета (F32/F33 → F35):
//! - **прогресс-таймаут** (`--watchdog-ms`): нет события от child'а дольше бюджета → снятие.
//!   Ловит `while true do end`.
//! - **абсолютный дедлайн** (`--deadline-ms`): общий wall-clock invocation, **не сбрасываемый**
//!   событиями. Ловит плагин, который бесконечно дёргает дешёвые host-вызовы: прогресс есть,
//!   а invocation не заканчивается, и синхронный host занят навсегда.
//! - **`--overall-ms`** — страховочный потолок ожидания harness'а.
//!
//! Снятие — `TerminateJobObject`. Сравнение с чистым Job CPU-лимитом (`--cpu-ms`) показывает,
//! насколько грубее ОС-backstop (F29). F36: кадры child'а разбираются без паники.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use lua_rpc_spike::job::Job;
use lua_rpc_spike::{
    parse_event, put_bytes, put_u32, read_frame, serve_host_call, write_frame, Event, CMD_REPLY,
    CMD_RUN,
};

struct Args {
    child: PathBuf,
    plugin: PathBuf,
    doc_size: usize,
    /// Бюджет «без прогресса от child» до `TerminateJobObject`. 0 — выключен.
    watchdog_ms: u64,
    /// Абсолютный wall-clock дедлайн invocation (не сбрасывается событиями). 0 — выключен.
    deadline_ms: u64,
    /// Job CPU-лимит (грубый backstop). 0 — выключен.
    cpu_ms: u64,
    mem_mb: u64,
    /// Страховочный общий таймаут ожидания (чтобы не висеть вечно).
    overall_ms: u64,
}

fn parse_args() -> Result<Args, String> {
    let mut child = None;
    let mut plugin = None;
    let mut doc_size = 1024usize;
    let mut watchdog_ms = 300u64;
    let mut deadline_ms = 0u64;
    let mut cpu_ms = 0u64;
    let mut mem_mb = 64u64;
    let mut overall_ms = 30000u64;
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
            "--watchdog-ms" => {
                watchdog_ms = it
                    .next()
                    .ok_or("--watchdog-ms требует число")?
                    .parse()
                    .map_err(|_| "--watchdog-ms: целое".to_string())?
            }
            "--deadline-ms" => {
                deadline_ms = it
                    .next()
                    .ok_or("--deadline-ms требует число")?
                    .parse()
                    .map_err(|_| "--deadline-ms: целое".to_string())?
            }
            "--cpu-ms" => {
                cpu_ms = it
                    .next()
                    .ok_or("--cpu-ms требует число")?
                    .parse()
                    .map_err(|_| "--cpu-ms: целое".to_string())?
            }
            "--mem-mb" => {
                mem_mb = it
                    .next()
                    .ok_or("--mem-mb требует число")?
                    .parse()
                    .map_err(|_| "--mem-mb: целое".to_string())?
            }
            "--overall-ms" => {
                overall_ms = it
                    .next()
                    .ok_or("--overall-ms требует число")?
                    .parse()
                    .map_err(|_| "--overall-ms: целое".to_string())?
            }
            other => return Err(format!("неизвестный аргумент: {other}")),
        }
    }
    Ok(Args {
        child: child.ok_or("не указан --child <exe>")?,
        plugin: plugin.ok_or("не указан --plugin <path>")?,
        doc_size,
        watchdog_ms,
        deadline_ms,
        cpu_ms,
        mem_mb,
        overall_ms,
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

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            println!("RESULT error={e}");
            std::process::exit(2);
        }
    };
    let source = match std::fs::read(&args.plugin) {
        Ok(s) => s,
        Err(e) => {
            println!("RESULT error=read_plugin:{e}");
            std::process::exit(2);
        }
    };
    let document = build_document(args.doc_size);

    println!(
        "CONFIG watchdog_ms={} deadline_ms={} cpu_ms={} mem_mb={} plugin={}",
        args.watchdog_ms,
        args.deadline_ms,
        args.cpu_ms,
        args.mem_mb,
        args.plugin.display()
    );

    let mem_bytes = (args.mem_mb as usize) * 1024 * 1024;
    let job = match Job::new().and_then(|j| {
        j.set_limits(args.cpu_ms, mem_bytes)?;
        Ok(j)
    }) {
        Ok(j) => j,
        Err(e) => {
            println!("RESULT error=job_setup:{e}");
            std::process::exit(2);
        }
    };

    let mut child = match Command::new(&args.child)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            println!("RESULT error=spawn:{e}");
            std::process::exit(2);
        }
    };
    if let Err(e) = job.assign_pid(child.id()) {
        println!("RESULT error=assign_pid:{e}");
        let _ = child.kill();
        std::process::exit(2);
    }

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

    let mut run_msg = vec![CMD_RUN];
    put_bytes(&mut run_msg, &source);
    if let Err(e) = write_frame(&mut to_child, &run_msg) {
        println!("RESULT error=send_run:{e}");
    }

    let started = Instant::now();
    // Прогресс-таймаут сбрасывается на любое событие; абсолютный дедлайн — нет.
    let mut last_progress = started;
    let mut invocation_start: Option<Instant> = None;
    let mut progress_fired = false;
    let mut deadline_fired = false;
    let mut overall_timeout = false;
    let mut disconnected = false;
    let mut protocol_error = false;
    let mut done: Option<String> = None;

    let watchdog = Duration::from_millis(args.watchdog_ms);
    let deadline = Duration::from_millis(args.deadline_ms);
    let overall = Duration::from_millis(args.overall_ms);

    loop {
        let now = Instant::now();

        // Границы проверяются явно, а не через таймаут `recv_timeout`: при непрерывном
        // потоке событий (chatty-плагин) таймаут не наступает вовсе.
        if args.deadline_ms > 0 {
            if let Some(t0) = invocation_start {
                if now.duration_since(t0) >= deadline {
                    deadline_fired = true;
                    job.terminate(1);
                    break;
                }
            }
        }
        if args.watchdog_ms > 0 && now.duration_since(last_progress) >= watchdog {
            progress_fired = true;
            job.terminate(1);
            break;
        }
        if now.duration_since(started) >= overall {
            overall_timeout = true;
            job.terminate(1);
            break;
        }

        // Ближайшая из включённых границ — сколько спать до следующего события.
        let mut wait = overall.saturating_sub(now.duration_since(started));
        if args.deadline_ms > 0 {
            if let Some(t0) = invocation_start {
                wait = wait.min(deadline.saturating_sub(now.duration_since(t0)));
            }
        }
        if args.watchdog_ms > 0 {
            wait = wait.min(watchdog.saturating_sub(now.duration_since(last_progress)));
        }

        match rx.recv_timeout(wait) {
            Ok(p) => {
                let now = Instant::now();
                last_progress = now;
                if invocation_start.is_none() {
                    invocation_start = Some(now);
                    println!("READY after {:.2}ms", ms(now.duration_since(started)));
                }
                match parse_event(&p) {
                    Ok(Event::Ready) => {}
                    Ok(Event::Log { level, message }) => println!("LOG[{level}] {message}"),
                    Ok(Event::HostCall { id, method, arg }) => {
                        let result = serve_host_call(method, arg, document.as_bytes());
                        let mut reply = vec![CMD_REPLY];
                        put_u32(&mut reply, id);
                        reply.push(0);
                        put_bytes(&mut reply, &result);
                        if write_frame(&mut to_child, &reply).is_err() {
                            break;
                        }
                    }
                    Ok(Event::Done { status, summary }) => {
                        done = Some(format!("status={status} {summary}"));
                        break;
                    }
                    Ok(Event::Unknown(_)) => {}
                    Err(e) => {
                        // F36: нарушение протокола — отказ плагина, не паника.
                        protocol_error = true;
                        println!("PROTOCOL_ERROR {e}");
                        job.terminate(1);
                        break;
                    }
                }
            }
            // Событий нет — границы перепроверит начало следующей итерации.
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => {
                // Child исчез сам: его снял Job CPU-лимит (или он упал).
                disconnected = true;
                break;
            }
        }
    }

    let latency_ms = if deadline_fired {
        invocation_start
            .map(|t| ms(t.elapsed()))
            .unwrap_or_else(|| ms(started.elapsed()))
    } else if progress_fired {
        ms(last_progress.elapsed())
    } else {
        ms(started.elapsed())
    };

    let status = child.wait().ok();
    reader.join().ok();
    let child_gone = status.is_some();
    let exit_code = status.and_then(|s| s.code()).unwrap_or(-1);
    let cpu_used = job.total_user_time_ms();

    if let Some(d) = &done {
        println!("CHILD_SUMMARY {d}");
    }
    println!("LATENCY_MS {latency_ms:.2}");
    println!("PROGRESS_FIRED {}", progress_fired as u8);
    println!("DEADLINE_FIRED {}", deadline_fired as u8);
    println!("OVERALL_TIMEOUT {}", overall_timeout as u8);
    println!("DISCONNECTED {}", disconnected as u8);
    println!("PROTOCOL_ERROR {}", protocol_error as u8);
    println!("CHILD_GONE {}", child_gone as u8);
    println!("EXIT code={exit_code}");
    println!("JOB_CPU_MS {cpu_used:.2}");

    // Гейт выбирается по заданному бюджету: дедлайн → дедлайн; иначе прогресс-таймаут;
    // иначе (только CPU-лимит) — child в итоге снят (латентность грубая, F29).
    let pass = if args.deadline_ms > 0 {
        let budget = args.deadline_ms as f64 * 3.0 + 300.0;
        deadline_fired && child_gone && !protocol_error && latency_ms <= budget
    } else if args.watchdog_ms > 0 {
        let budget = args.watchdog_ms as f64 * 3.0 + 250.0;
        progress_fired && child_gone && latency_ms <= budget
    } else {
        child_gone && done.is_none()
    };
    println!("RESULT pass={}", pass as u8);
    std::process::exit(if pass { 0 } else { 1 });
}
