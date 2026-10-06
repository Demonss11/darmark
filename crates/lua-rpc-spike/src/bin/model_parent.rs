//! Parent M12 (P9): модель процессов — «один общий child на все плагины» vs «child на плагин».
//!
//! Метрики (F23/F24):
//!  - commit-память пустого child, child с 10 МБ документом, N×child;
//!  - время активации (spawn → READY всех);
//!  - изоляция отказа: crash/spin одного плагина — кого уносит.
//!
//! Все child'ы одного прогона назначаются в один Job Object; `PeakJobMemoryUsed` даёт aggregate-пик
//! (совпадает с суммой при одновременной жизни). Ручных таймаутов-убийц нет — только наблюдение.

use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use lua_rpc_spike::job::Job;
use lua_rpc_spike::{
    parse_event, put_bytes, put_str, put_u32, read_frame, serve_host_call, write_frame, Event,
    CMD_REPLY, CMD_RUN, CMD_RUN_N,
};

struct Args {
    child: PathBuf,
    plugins: PathBuf, // каталог с фикстурами
    model: String,    // shared | per-plugin
    scenario: String, // empty | doc | stress | fault
    n: usize,
    doc_size: usize,
    timeout: Duration,
}

fn parse_args() -> Result<Args, String> {
    let mut child = None;
    let mut plugins = PathBuf::from("crates/lua-rpc-spike/plugins");
    let mut model = "shared".to_string();
    let mut scenario = "empty".to_string();
    let mut n = 5usize;
    let mut doc_size = 10 * 1024 * 1024;
    let mut timeout = Duration::from_millis(15000);
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--child" => child = Some(PathBuf::from(it.next().ok_or("--child требует путь")?)),
            "--plugins" => plugins = PathBuf::from(it.next().ok_or("--plugins требует путь")?),
            "--model" => model = it.next().ok_or("--model требует значение")?,
            "--scenario" => scenario = it.next().ok_or("--scenario требует значение")?,
            "--n" => {
                n = it
                    .next()
                    .ok_or("--n требует число")?
                    .parse()
                    .map_err(|_| "--n: целое".to_string())?
            }
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
        plugins,
        model,
        scenario,
        n,
        doc_size,
        timeout,
    })
}

struct Conn {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<Vec<u8>>,
    reader: JoinHandle<()>,
}

fn spawn_conn(exe: &Path, job: &Job) -> std::io::Result<Conn> {
    let mut child = Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    job.assign_pid(child.id())?;
    let stdin = child.stdin.take().unwrap();
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
    Ok(Conn {
        child,
        stdin,
        rx,
        reader,
    })
}

impl Conn {
    fn send(&mut self, payload: &[u8]) -> std::io::Result<()> {
        write_frame(&mut self.stdin, payload)
    }
}

enum Outcome {
    Done(u8, String),
    Disconnected,
    Timeout,
}

struct DriveResult {
    outcome: Outcome,
    /// Метки `plugin#N done`, полученные до завершения/обрыва.
    done_markers: Vec<String>,
    ready: bool,
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

/// Обслуживает один child до DONE/обрыва, отвечая на host-вызовы документом-заглушкой.
fn drive(conn: &mut Conn, document: &[u8], timeout: Duration) -> DriveResult {
    let mut done_markers = Vec::new();
    let mut ready = false;
    loop {
        match conn.rx.recv_timeout(timeout) {
            Ok(p) => {
                match parse_event(&p) {
                    Ok(Event::Ready) => ready = true,
                    Ok(Event::Log { message, .. }) => {
                        if message.contains("done") || message.contains("hello") {
                            done_markers.push(message.to_string());
                        }
                    }
                    Ok(Event::HostCall { id, method, arg }) => {
                        let result = serve_host_call(method, arg, document);
                        let mut reply = vec![CMD_REPLY];
                        put_u32(&mut reply, id);
                        reply.push(0);
                        put_bytes(&mut reply, &result);
                        if conn.send(&reply).is_err() {
                            return DriveResult {
                                outcome: Outcome::Disconnected,
                                done_markers,
                                ready,
                            };
                        }
                    }
                    Ok(Event::Done { status, summary }) => {
                        return DriveResult {
                            outcome: Outcome::Done(status, summary.to_string()),
                            done_markers,
                            ready,
                        };
                    }
                    Ok(_) => {}
                    Err(_) => {
                        // F36: нарушение протокола трактуем как обрыв плагина.
                        return DriveResult {
                            outcome: Outcome::Disconnected,
                            done_markers,
                            ready,
                        };
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                return DriveResult {
                    outcome: Outcome::Timeout,
                    done_markers,
                    ready,
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                return DriveResult {
                    outcome: Outcome::Disconnected,
                    done_markers,
                    ready,
                }
            }
        }
    }
}

fn read_plugin(dir: &Path, name: &str) -> Vec<u8> {
    let p = dir.join(name);
    match std::fs::read(&p) {
        Ok(b) => b,
        Err(e) => {
            println!("RESULT error=read {}:{e}", p.display());
            std::process::exit(2);
        }
    }
}

/// CMD_RUN_N: count + (name, source)×count.
fn build_run_n(items: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut msg = vec![CMD_RUN_N];
    put_u32(&mut msg, items.len() as u32);
    for (name, src) in items {
        put_str(&mut msg, name);
        put_bytes(&mut msg, src);
    }
    msg
}

fn finish(conn: Conn) {
    let mut conn = conn;
    let _ = conn.child.wait();
    conn.reader.join().ok();
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
    println!(
        "SCENARIO model={} scenario={} n={} doc_size={}",
        args.model, args.scenario, args.n, args.doc_size
    );

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

    let document = build_document(args.doc_size);
    let t0 = Instant::now();
    let mut conns: Vec<Conn> = Vec::new();
    let mut outcomes: Vec<DriveResult> = Vec::new();

    let result = match (args.model.as_str(), args.scenario.as_str()) {
        // ---- commit пустого child ----
        ("shared", "empty") | ("per-plugin", "empty") => {
            let src = read_plugin(&args.plugins, "empty.lua");
            let c = spawn_conn(&args.child, &job).expect("spawn");
            conns.push(c);
            let mut msg = vec![CMD_RUN];
            put_bytes(&mut msg, &src);
            conns[0].send(&msg).expect("send");
            let d = drive(&mut conns[0], document.as_bytes(), args.timeout);
            outcomes.push(d);
            Ok(())
        }
        // ---- commit child с 10 МБ документом ----
        ("shared", "doc") | ("per-plugin", "doc") => {
            let src = read_plugin(&args.plugins, "large.lua");
            let c = spawn_conn(&args.child, &job).expect("spawn");
            conns.push(c);
            let mut msg = vec![CMD_RUN];
            put_bytes(&mut msg, &src);
            conns[0].send(&msg).expect("send");
            let d = drive(&mut conns[0], document.as_bytes(), args.timeout);
            outcomes.push(d);
            Ok(())
        }
        // ---- N плагинов: один общий child vs child на плагин ----
        ("shared", "stress") => {
            let src = read_plugin(&args.plugins, "empty.lua");
            let items: Vec<(String, Vec<u8>)> = (0..args.n)
                .map(|i| (format!("empty#{i}"), src.clone()))
                .collect();
            let c = spawn_conn(&args.child, &job).expect("spawn");
            conns.push(c);
            let msg = build_run_n(&items);
            conns[0].send(&msg).expect("send");
            let d = drive(&mut conns[0], document.as_bytes(), args.timeout);
            outcomes.push(d);
            Ok(())
        }
        ("per-plugin", "stress") => {
            let src = read_plugin(&args.plugins, "empty.lua");
            for _ in 0..args.n {
                let c = spawn_conn(&args.child, &job).expect("spawn");
                conns.push(c);
            }
            for c in conns.iter_mut() {
                let mut msg = vec![CMD_RUN];
                put_bytes(&mut msg, &src);
                c.send(&msg).expect("send");
            }
            for c in conns.iter_mut() {
                let d = drive(c, document.as_bytes(), args.timeout);
                outcomes.push(d);
            }
            Ok(())
        }
        // ---- изоляция отказа: hello, crash, hello ----
        ("shared", "fault") => {
            let hello = read_plugin(&args.plugins, "hello.lua");
            let crash = read_plugin(&args.plugins, "crash.lua");
            let items = vec![
                ("hello#0".to_string(), hello.clone()),
                ("crash#1".to_string(), crash),
                ("hello#2".to_string(), hello),
            ];
            let c = spawn_conn(&args.child, &job).expect("spawn");
            conns.push(c);
            let msg = build_run_n(&items);
            conns[0].send(&msg).expect("send");
            let d = drive(&mut conns[0], document.as_bytes(), args.timeout);
            outcomes.push(d);
            Ok(())
        }
        ("per-plugin", "fault") => {
            let hello = read_plugin(&args.plugins, "hello.lua");
            let crash = read_plugin(&args.plugins, "crash.lua");
            let sources = [hello.clone(), crash, hello];
            for src in sources.iter() {
                let c = spawn_conn(&args.child, &job).expect("spawn");
                conns.push(c);
                let mut msg = vec![CMD_RUN];
                put_bytes(&mut msg, src);
                conns.last_mut().unwrap().send(&msg).expect("send");
            }
            for c in conns.iter_mut() {
                let d = drive(c, document.as_bytes(), args.timeout);
                outcomes.push(d);
            }
            Ok(())
        }
        // ---- зависание: hello, spin, hello ----
        ("shared", "fault-spin") => {
            let hello = read_plugin(&args.plugins, "hello.lua");
            let spin = read_plugin(&args.plugins, "spin.lua");
            let items = vec![
                ("hello#0".to_string(), hello.clone()),
                ("spin#1".to_string(), spin),
                ("hello#2".to_string(), hello),
            ];
            let c = spawn_conn(&args.child, &job).expect("spawn");
            conns.push(c);
            let msg = build_run_n(&items);
            conns[0].send(&msg).expect("send");
            let d = drive(&mut conns[0], document.as_bytes(), args.timeout);
            outcomes.push(d);
            Ok(())
        }
        ("per-plugin", "fault-spin") => {
            let hello = read_plugin(&args.plugins, "hello.lua");
            let spin = read_plugin(&args.plugins, "spin.lua");
            let sources = [hello.clone(), spin, hello];
            for src in sources.iter() {
                let c = spawn_conn(&args.child, &job).expect("spawn");
                conns.push(c);
                let mut msg = vec![CMD_RUN];
                put_bytes(&mut msg, src);
                conns.last_mut().unwrap().send(&msg).expect("send");
            }
            for c in conns.iter_mut() {
                let d = drive(c, document.as_bytes(), args.timeout);
                outcomes.push(d);
            }
            Ok(())
        }
        (m, s) => Err(format!("неизвестная комбинация model={m} scenario={s}")),
    };

    let wall_ms = t0.elapsed().as_secs_f64() * 1000.0;
    if let Err(e) = result {
        println!("RESULT error={e}");
        std::process::exit(2);
    }

    let peak = job.peak_job_memory();
    println!("CHILDREN {}", conns.len());
    println!("WALL_MS {wall_ms:.2}");
    println!("JOB_PEAK_BYTES {peak}");
    println!("JOB_PEAK_MIB {:.2}", mib(peak));

    let mut survivors = 0usize;
    let mut disconnected = 0usize;
    let mut timeouts = 0usize;
    let mut done_markers = 0usize;
    let mut ready_children = 0usize;
    for d in &outcomes {
        done_markers += d.done_markers.len();
        if d.ready {
            ready_children += 1;
        }
        match &d.outcome {
            Outcome::Done(st, sum) => {
                if *st == 0 {
                    survivors += 1;
                }
                if outcomes.len() == 1 {
                    println!("CHILD_SUMMARY status={st} {sum}");
                }
            }
            Outcome::Disconnected => disconnected += 1,
            Outcome::Timeout => timeouts += 1,
        }
    }
    println!("READY_CHILDREN {ready_children}");
    println!("DONE_MARKERS {done_markers}");
    println!("SURVIVORS {survivors}");
    println!("DISCONNECTED {disconnected}");
    println!("TIMEOUTS {timeouts}");

    // Гейт M12: для fault важно, что модель «общий child» теряет все плагины при одном crash,
    // а «на плагин» — только один. Для прочих сценариев фиксируем числа.
    let pass = match (args.model.as_str(), args.scenario.as_str()) {
        (_, "fault") if args.model == "shared" => done_markers < 3 && disconnected == 1,
        (_, "fault") => survivors == 2 && disconnected == 1,
        (_, "fault-spin") if args.model == "shared" => done_markers < 3 && timeouts == 1,
        (_, "fault-spin") => survivors == 2 && timeouts == 1,
        _ => timeouts == 0 && peak > 0,
    };
    println!("RESULT pass={}", pass as u8);

    // Уборка: закрываем job (KILL_ON_JOB_CLOSE снимет всё, что ещё живо) и join'им reader'ы.
    drop(job);
    for c in conns {
        finish(c);
    }
    std::process::exit(if pass { 0 } else { 1 });
}
