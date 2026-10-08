//! Parent M11 (P9): лимиты child'а через **Job Object** (RSS/CPU/время), без ручного kill.
//!
//! Гейт M11: `spin.lua`/`hog.lua` останавливаются **детерминированно** лимитом ОС, parent жив.
//! В отличие от C-спайка (kill по таймауту) здесь не задаётся «сколько ждать»: CPU-лимит
//! завершает процесс сам, а memory-лимит заставляет аллокации падать. Parent только наблюдает
//! закрытие границы и читает учёт Job'а.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use lua_rpc_spike::job::{parent_is_in_job, Job};
use lua_rpc_spike::{
    parse_event, put_bytes, put_u32, read_frame, write_frame, Event, CMD_REPLY, CMD_RUN,
};

struct Args {
    child: PathBuf,
    plugin: PathBuf,
    cpu_ms: u64,
    mem_mb: u64,
    timeout: Duration,
    /// CREATE_BREAKAWAY_FROM_JOB: выйти из внешнего Job (если он разрешает), чтобы наш Job был
    /// не вложенным. Диагностика лага лимита времени во вложенном Job.
    breakaway: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut child = None;
    let mut plugin = None;
    let mut cpu_ms = 500u64;
    let mut mem_mb = 64u64;
    let mut timeout = Duration::from_millis(10000);
    let mut breakaway = false;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--child" => child = Some(PathBuf::from(it.next().ok_or("--child требует путь")?)),
            "--plugin" => plugin = Some(PathBuf::from(it.next().ok_or("--plugin требует путь")?)),
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
            "--timeout-ms" => {
                let ms: u64 = it
                    .next()
                    .ok_or("--timeout-ms требует число")?
                    .parse()
                    .map_err(|_| "--timeout-ms: целое".to_string())?;
                timeout = Duration::from_millis(ms);
            }
            "--breakaway" => breakaway = true,
            other => return Err(format!("неизвестный аргумент: {other}")),
        }
    }
    Ok(Args {
        child: child.ok_or("не указан --child <exe>")?,
        plugin: plugin.ok_or("не указан --plugin <path>")?,
        cpu_ms,
        mem_mb,
        timeout,
        breakaway,
    })
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

    let mem_bytes = (args.mem_mb as usize) * 1024 * 1024;
    println!(
        "CONFIG cpu_ms={} mem_mb={} timeout_ms={} plugin={}",
        args.cpu_ms,
        args.mem_mb,
        args.timeout.as_millis(),
        args.plugin.display()
    );

    println!("NESTED parent_in_job={}", parent_is_in_job() as u8);
    // Job создаём до spawn, чтобы child попал под лимиты до CMD_RUN.
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

    let mut cmd = Command::new(&args.child);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if args.breakaway {
        use std::os::windows::process::CommandExt;
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
        cmd.creation_flags(CREATE_BREAKAWAY_FROM_JOB);
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            println!("RESULT error=spawn:{e}");
            std::process::exit(2);
        }
    };

    let pid = child.id();
    if let Err(e) = job.assign_pid(pid) {
        println!("RESULT error=assign_pid:{e}");
        let _ = child.kill();
        std::process::exit(2);
    }
    let (lf, ppt, pjt) = job.stored_limits();
    println!("JOB_LIMITS flags=0x{lf:08X} per_process_100ns={ppt} per_job_100ns={pjt}");
    println!("ASSIGNED pid={pid}");

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

    let run_t = Instant::now();
    let mut run_msg = vec![CMD_RUN];
    put_bytes(&mut run_msg, &source);
    if let Err(e) = write_frame(&mut to_child, &run_msg) {
        println!("RESULT error=send_run:{e}");
    }

    let mut ready = false;
    let mut done: Option<(u8, String)> = None;
    let mut timeout_hit = false;

    loop {
        match rx.recv_timeout(args.timeout) {
            Ok(p) => {
                match parse_event(&p) {
                    Ok(Event::Ready) => {
                        ready = true;
                        println!(
                            "READY after {:.2}ms",
                            run_t.elapsed().as_secs_f64() * 1000.0
                        );
                    }
                    Ok(Event::Log { level, message }) => {
                        println!("LOG[{level}] {message}");
                    }
                    Ok(Event::HostCall { id, .. }) => {
                        let mut reply = vec![CMD_REPLY];
                        put_u32(&mut reply, id);
                        reply.push(0);
                        put_bytes(&mut reply, b"");
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
                        break;
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                timeout_hit = true;
                break;
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }

    let wall_ms = run_t.elapsed().as_secs_f64() * 1000.0;
    // Ручного kill() нет: child либо уже снят лимитом ОС, либо снимается Job'ом.
    // ВАЖНО: terminate до wait — иначе wait() на живом (без сработавшего лимита) child виснет.
    if timeout_hit {
        job.terminate(1); // уборка, если лимит не сработал
    }
    let status = child.wait().ok();
    reader.join().ok();

    let peak = job.peak_process_memory();
    let cpu_used = job.total_user_time_ms();
    let child_gone = status.is_some();

    let exit_code = status.map(|s| s.code().unwrap_or(-1)).unwrap_or(-1);
    println!("STATUS done={}", done.is_some() as u8);
    if let Some((st, summary)) = &done {
        println!("CHILD_SUMMARY status={st} {summary}");
    }
    println!("EXIT code={exit_code}");
    println!("WALL_MS {wall_ms:.2}");
    println!("JOB_CPU_MS {cpu_used:.2}");
    println!("JOB_PEAK_MEM {peak}");
    println!("MEM_LIMIT {mem_bytes}");

    // Гейт M11: лимит сработал без ручного таймаута, child мёртв, parent жив.
    let mem_ok = mem_bytes == 0 || peak <= mem_bytes;
    let pass = !timeout_hit && child_gone && mem_ok && ready;
    println!(
        "RESULT ready={} timeout={} child_gone={} peek_mem_ok={} pass={}",
        ready as u8, timeout_hit as u8, child_gone as u8, mem_ok as u8, pass as u8
    );
    std::process::exit(if pass { 0 } else { 1 });
}
