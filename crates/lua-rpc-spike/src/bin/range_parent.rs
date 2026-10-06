//! Parent D16 (ADR-0021): **range/delta host-API** против полной передачи документа (F22/F30).
//!
//! При модели «child на плагин» каждый процесс, тянущий документ целиком, держит его копию
//! (10 МБ → ≈20.7 МиБ commit, замер F30). Здесь сравниваются два пути на одном и том же
//! документе: `full` (`get_document_text`) и `range`/`delta` (`get_document_range`,
//! `get_document_len`, `apply_edit`). Метрика — пиковая committed-память child'а по Job Object.

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
    timeout: Duration,
    label: String,
}

fn parse_args() -> Result<Args, String> {
    let mut child = None;
    let mut plugin = None;
    let mut doc_size = 10 * 1024 * 1024;
    let mut timeout = Duration::from_millis(15000);
    let mut label = "run".to_string();
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--child" => child = Some(PathBuf::from(it.next().ok_or("--child требует путь")?)),
            "--plugin" => plugin = Some(PathBuf::from(it.next().ok_or("--plugin требует путь")?)),
            "--label" => label = it.next().ok_or("--label требует значение")?,
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
        label,
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

fn mib(bytes: usize) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
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
        "SCENARIO label={} plugin={} doc_size={}",
        args.label,
        args.plugin.display(),
        args.doc_size
    );

    // Лимитов памяти нет: меряем фактический commit child'а на каждом пути.
    let job = match Job::new().and_then(|j| {
        j.set_limits(0, 0)?; // только KILL_ON_JOB_CLOSE
        Ok(j)
    }) {
        Ok(j) => j,
        Err(e) => {
            println!("RESULT error=job:{e}");
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

    let started = Instant::now();
    let mut run_msg = vec![CMD_RUN];
    put_bytes(&mut run_msg, &source);
    if write_frame(&mut to_child, &run_msg).is_err() {
        println!("RESULT error=send_run");
    }

    let mut done: Option<(u8, String)> = None;
    loop {
        match rx.recv_timeout(args.timeout) {
            Ok(p) => {
                match parse_event(&p) {
                    Ok(Event::Log { level, message }) => {
                        println!("LOG[{level}] {message}");
                    }
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
                        done = Some((status, summary.to_string()));
                        break;
                    }
                    Ok(_) => {}
                    Err(e) => {
                        // F36: нарушение протокола — отказ, не паника.
                        println!("PROTOCOL_ERROR {e}");
                        job.terminate(1);
                        break;
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                job.terminate(1);
                break;
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }

    let wall_ms = started.elapsed().as_secs_f64() * 1000.0;
    let _ = child.wait();
    reader.join().ok();

    let peak = job.peak_process_memory();
    if let Some((st, summary)) = &done {
        println!("CHILD_SUMMARY status={st} {summary}");
    }
    println!("WALL_MS {wall_ms:.2}");
    println!("JOB_PEAK_BYTES {peak}");
    println!("JOB_PEAK_MIB {:.2}", mib(peak));

    let ok = done.as_ref().map(|(st, _)| *st == 0).unwrap_or(false);
    println!(
        "RESULT label={} pass={} peak_mib={:.2}",
        args.label,
        (ok && peak > 0) as u8,
        mib(peak)
    );
    std::process::exit(if ok { 0 } else { 1 });
}
