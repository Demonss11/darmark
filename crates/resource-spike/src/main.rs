//! Спайк (проверочный стенд) для `tasks/TZ-RESOURCE-MON.md`.
//!
//! **Спайк, в продукт не входит, удалить после ADR-0031.**
//!
//! Сравнивает два источника метрик ресурсов процесса (ОЗУ/CPU) для будущего монитора
//! главного процесса и child'ов `darmark-plugin-host`:
//!
//! * **WinAPI** через `windows`-crate (`K32GetProcessMemoryInfo` + `GetProcessTimes` +
//!   `GetProcessHandleCount`), открывая процесс по PID на каждый сэмпл;
//! * **`sysinfo`** — кроссплатформенный крейт, обновление набора PID.
//!
//! Что измеряется (вывод — строки `[TABLE]`/`[COST]`/`[POLL]`/`[EDGE]`):
//! 1. сырые значения метрик на одних и тех же PID — для проверки соответствия;
//! 2. стоимость одного сэмпла (µs): медиана/перцентили на ~1000 итераций;
//! 3. CPU самого опроса за 2-секундный интервал (для набора 1/5/20 PID);
//! 4. краевые случаи: мёртвый/несуществующий/недоступный PID, PID reuse (best-effort).
//!
//! Размер релизного бинарника по бэкендам замеряется отдельными сборками
//! (`--no-default-features --features winapi|sysinfo`) — см. отчёт/`FINDINGS`.
//!
//! Запуск: `cargo run -p resource-spike --release [-- --iters 1000 --children 20 --poll-secs 10]`.
//! Дочерний режим (внутренний): `resource-spike --sleep <ms>` — простаивающий процесс-цель.

use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Собственный замер CPU процесса (kernel+user) без зависимости от `windows`-crate:
/// нужен для честного «CPU самого опроса» в любом наборе фич. Копия FFI из
/// `plugin-proto/src/job.rs` (минимум kernel32).
#[cfg(windows)]
mod own_cpu {
    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub struct FileTime {
        pub lo: u32,
        pub hi: u32,
    }

    // SAFETY: сигнатуры соответствуют Win32; GetCurrentProcess возвращает псевдо-хэндл
    // (закрывать не нужно), GetProcessTimes пишет в валидные указатели.
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn GetProcessTimes(
            process: isize,
            creation: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
    }

    /// CPU-время текущего процесса в единицах 100 нс (kernel + user).
    pub fn process_cpu_100ns() -> u64 {
        let to_u64 = |t: FileTime| ((t.hi as u64) << 32) | (t.lo as u64);
        unsafe {
            let (mut c, mut e, mut k, mut u) = (
                FileTime::default(),
                FileTime::default(),
                FileTime::default(),
                FileTime::default(),
            );
            if GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u) == 0 {
                return 0;
            }
            to_u64(k) + to_u64(u)
        }
    }
}

/// WinAPI-провайдер на `windows`-crate (только при включённой фиче `winapi`).
#[cfg(feature = "winapi")]
mod win {
    use windows::Win32::Foundation::{CloseHandle, FILETIME, HANDLE};
    use windows::Win32::System::ProcessStatus::{
        K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
    };
    use windows::Win32::System::Threading::{
        GetProcessHandleCount, GetProcessTimes, OpenProcess, PROCESS_ACCESS_RIGHTS,
        PROCESS_QUERY_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    /// Сырые метрики процесса (WinAPI).
    #[derive(Debug, Clone, Copy, Default)]
    pub struct Sample {
        pub working_set: usize,
        pub private_bytes: usize,
        pub peak_working_set: usize,
        /// kernel+user CPU, единицы 100 нс.
        pub cpu_100ns: u64,
        pub handles: u32,
    }

    fn ft_to_u64(t: FILETIME) -> u64 {
        ((t.dwHighDateTime as u64) << 32) | (t.dwLowDateTime as u64)
    }

    /// Память процесса (`K32GetProcessMemoryInfo`, EX-структура) по открытому хэндлу.
    ///
    /// SAFETY: `h` — валидный хэндл с правом запроса; `c` — локальная EX-структура
    /// нужного размера; K32GetProcessMemoryInfo принимает указатель на неё.
    unsafe fn memory(h: HANDLE) -> Option<(usize, usize, usize)> {
        let mut c = PROCESS_MEMORY_COUNTERS_EX::default();
        let ok = K32GetProcessMemoryInfo(
            h,
            &mut c as *mut PROCESS_MEMORY_COUNTERS_EX as *mut PROCESS_MEMORY_COUNTERS,
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
        );
        // K32GetProcessMemoryInfo возвращает BOOL (не Result): true — метрики заполнены.
        if ok.as_bool() {
            Some((c.WorkingSetSize, c.PrivateUsage, c.PeakWorkingSetSize))
        } else {
            None
        }
    }

    /// CPU-время (kernel+user) по открытому хэндлу.
    ///
    /// SAFETY: `h` — валидный хэндл; все 4 FILETIME — локальные.
    unsafe fn cpu_100ns(h: HANDLE) -> Option<u64> {
        let (mut c, mut e, mut k, mut u) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        GetProcessTimes(h, &mut c, &mut e, &mut k, &mut u)
            .ok()
            .map(|_| ft_to_u64(k) + ft_to_u64(u))
    }

    /// Горячий путь монитора: интерес к памяти и CPU. Открытие — по
    /// `PROCESS_QUERY_LIMITED_INFORMATION` (минимум прав для метрик).
    pub fn sample_mem_cpu(pid: u32) -> Result<Sample, u32> {
        let h = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }
            .map_err(|e| e.code().0 as u32)?;
        let mut s = Sample::default();
        unsafe {
            if let Some((ws, priv_, peak)) = memory(h) {
                s.working_set = ws;
                s.private_bytes = priv_;
                s.peak_working_set = peak;
            }
            s.cpu_100ns = cpu_100ns(h).unwrap_or(0);
            let _ = CloseHandle(h);
        }
        Ok(s)
    }

    /// Хендлы требуют `PROCESS_QUERY_INFORMATION` — отдельное открытие; в UI показываются
    /// только при превышении порога, поэтому не входят в горячий путь.
    pub fn sample_handles(pid: u32) -> Result<u32, u32> {
        let h = unsafe { OpenProcess(PROCESS_QUERY_INFORMATION, false, pid) }
            .map_err(|e| e.code().0 as u32)?;
        let mut n: u32 = 0;
        let ok = unsafe { GetProcessHandleCount(h, &mut n) };
        unsafe {
            let _ = CloseHandle(h);
        }
        ok.map_err(|e| e.code().0 as u32)?;
        Ok(n)
    }

    /// Полный сэмпл одним открытием (для проверки значений): память + CPU + хендлы.
    pub fn sample_all(pid: u32) -> Result<Sample, u32> {
        let access: PROCESS_ACCESS_RIGHTS =
            PROCESS_QUERY_INFORMATION | PROCESS_QUERY_LIMITED_INFORMATION;
        let h = unsafe { OpenProcess(access, false, pid) }.map_err(|e| e.code().0 as u32)?;
        let mut s = Sample::default();
        unsafe {
            if let Some((ws, priv_, peak)) = memory(h) {
                s.working_set = ws;
                s.private_bytes = priv_;
                s.peak_working_set = peak;
            }
            s.cpu_100ns = cpu_100ns(h).unwrap_or(0);
            let mut n: u32 = 0;
            if GetProcessHandleCount(h, &mut n).is_ok() {
                s.handles = n;
            }
            let _ = CloseHandle(h);
        }
        Ok(s)
    }

    /// Только открыть процесс (проверка доступности) — для краевых случаев.
    pub fn try_open(pid: u32) -> Result<(), u32> {
        match unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) } {
            Ok(h) => {
                unsafe {
                    let _ = CloseHandle(h);
                }
                Ok(())
            }
            Err(e) => Err(e.code().0 as u32),
        }
    }
}

/// `sysinfo`-провайдер (только при включённой фиче `sysinfo`).
#[cfg(feature = "sysinfo")]
mod si {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

    /// Обёртка над `System` для повторного обновления одного и того же набора PID.
    pub struct Sampler {
        sys: System,
        pids: Vec<Pid>,
    }

    impl Sampler {
        pub fn new(pids: &[u32]) -> Self {
            Self {
                sys: System::new(),
                pids: pids.iter().copied().map(Pid::from_u32).collect(),
            }
        }

        /// Обновляет память+CPU заданного набора PID (аналог одного «тика» монитора).
        pub fn refresh(&mut self) {
            self.sys.refresh_processes_specifics(
                ProcessesToUpdate::Some(&self.pids),
                false,
                ProcessRefreshKind::nothing().with_memory().with_cpu(),
            );
        }

        /// Обновляет **все** процессы (худший случай для sysinfo).
        pub fn refresh_all(&mut self) {
            self.sys.refresh_processes_specifics(
                ProcessesToUpdate::All,
                false,
                ProcessRefreshKind::nothing().with_memory().with_cpu(),
            );
        }

        /// `(rss_bytes, virtual_bytes, cpu_percent)` для PID, если он есть.
        pub fn get(&self, pid: u32) -> Option<(u64, u64, f32)> {
            self.sys
                .process(Pid::from_u32(pid))
                .map(|p| (p.memory(), p.virtual_memory(), p.cpu_usage()))
        }
    }
}

/// Статистика по выборке длительностей (в наносекундах), печатается в µs.
struct Stats {
    n: usize,
    min: f64,
    p50: f64,
    p90: f64,
    p99: f64,
    max: f64,
    mean: f64,
}

impl Stats {
    fn from_nanos(mut v: Vec<u128>) -> Self {
        v.sort_unstable();
        let n = v.len();
        let at = |q: f64| -> f64 {
            if n == 0 {
                return 0.0;
            }
            let idx = ((q * (n as f64 - 1.0)).round() as usize).min(n - 1);
            v[idx] as f64 / 1000.0
        };
        let mean = if n == 0 {
            0.0
        } else {
            v.iter().sum::<u128>() as f64 / n as f64 / 1000.0
        };
        Self {
            n,
            min: at(0.0),
            p50: at(0.5),
            p90: at(0.9),
            p99: at(0.99),
            max: at(1.0),
            mean,
        }
    }

    fn print(&self, label: &str) {
        println!(
            "[COST] {label:<26} n={:<5} min={:>8.2} p50={:>8.2} p90={:>8.2} p99={:>8.2} max={:>9.2} mean={:>8.2} us",
            self.n, self.min, self.p50, self.p90, self.p99, self.max, self.mean
        );
    }
}

/// Прогоняет `iters` итераций замыкания, возвращая статистику стоимости.
fn time_iters<F: FnMut()>(iters: usize, mut f: F) -> Stats {
    let mut samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t = Instant::now();
        f();
        samples.push(t.elapsed().as_nanos());
    }
    Stats::from_nanos(samples)
}

/// Запускает `n` простаивающих дочерних процессов (цели для замеров).
fn spawn_children(n: usize) -> Vec<Child> {
    let exe = std::env::current_exe().expect("current_exe");
    (0..n)
        .map(|_| {
            Command::new(&exe)
                .arg("--sleep")
                .arg("60000")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("spawn child")
        })
        .collect()
}

fn main() {
    let mut iters = 1000usize;
    let mut children = 20usize;
    let mut poll_secs = 10u64;
    let mut reuse_probe = 2000usize;

    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            // Дочерний режим: простаивающий процесс-цель.
            "--sleep" => {
                let ms: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(60_000);
                std::thread::sleep(Duration::from_millis(ms));
                return;
            }
            "--iters" => iters = args.next().and_then(|s| s.parse().ok()).unwrap_or(iters),
            "--children" => children = args.next().and_then(|s| s.parse().ok()).unwrap_or(children),
            "--poll-secs" => {
                poll_secs = args
                    .next()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(poll_secs)
            }
            "--reuse-probe" => {
                reuse_probe = args
                    .next()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(reuse_probe)
            }
            other => eprintln!("игнорирую неизвестный аргумент: {other}"),
        }
    }

    println!(
        "[ENV] os=windows arch={} py_pid={}",
        std::env::consts::ARCH,
        std::process::id()
    );
    println!(
        "[CFG] iters={iters} children={children} poll_secs={poll_secs} reuse_probe={reuse_probe} \
         winapi={} sysinfo={}",
        cfg!(feature = "winapi") as u8,
        cfg!(feature = "sysinfo") as u8
    );

    let mut kids = spawn_children(children);
    let pids: Vec<u32> = kids.iter().map(|c| c.id()).collect();
    println!("[PIDS] {pids:?}");

    // Дать детям немного пожить и «набрать» RSS.
    std::thread::sleep(Duration::from_millis(300));

    // ---- 1. Сырые значения метрик на одних PID ----
    #[cfg(feature = "winapi")]
    {
        println!("[TABLE] source=winapi (working_set / private / peak_ws / cpu100ns / handles)");
        for &p in pids.iter().take(5) {
            match win::sample_all(p) {
                Ok(s) => println!(
                    "[TABLE] winapi pid={p:<7} ws={:<12} private={:<12} peak_ws={:<12} cpu100ns={:<14} handles={}",
                    s.working_set, s.private_bytes, s.peak_working_set, s.cpu_100ns, s.handles
                ),
                Err(e) => println!("[TABLE] winapi pid={p:<7} error=win32({e})"),
            }
        }
        // CPU% за интервал ~500 мс: Δcpu / wall (в процентах одного ядра).
        if let Some(&p) = pids.first() {
            if let Ok(a) = win::sample_all(p) {
                let t = Instant::now();
                std::thread::sleep(Duration::from_millis(500));
                if let Ok(b) = win::sample_all(p) {
                    let wall_100ns = t.elapsed().as_nanos() as f64 / 100.0;
                    let pct = if wall_100ns > 0.0 {
                        (b.cpu_100ns.saturating_sub(a.cpu_100ns)) as f64 / wall_100ns * 100.0
                    } else {
                        0.0
                    };
                    println!("[TABLE] winapi pid={p} cpu_percent(0.5s)={pct:.2}");
                }
            }
        }
    }

    #[cfg(feature = "sysinfo")]
    {
        println!("[TABLE] source=sysinfo (rss / virtual / cpu_percent)");
        let mut s = si::Sampler::new(&pids);
        s.refresh();
        std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
        s.refresh();
        for &p in pids.iter().take(5) {
            match s.get(p) {
                Some((rss, virt, cpu)) => println!(
                    "[TABLE] sysinfo pid={p:<7} rss={:<12} virtual={:<14} cpu_percent={cpu:.2}",
                    rss, virt
                ),
                None => println!("[TABLE] sysinfo pid={p:<7} not_found"),
            }
        }
    }

    // ---- 2. Стоимость одного сэмпла ----
    let set: Vec<u32> = pids.iter().copied().take(5).collect();
    println!("[COST] набор для сэмпла: {set:?}");

    #[cfg(feature = "winapi")]
    {
        time_iters(iters, || {
            for &p in &set {
                if let Ok(s) = win::sample_mem_cpu(p) {
                    std::hint::black_box(s.working_set);
                }
            }
        })
        .print("winapi mem+cpu (set)");
        time_iters(iters, || {
            for &p in &set {
                if let Ok(s) = win::sample_all(p) {
                    std::hint::black_box(s.handles);
                }
            }
        })
        .print("winapi mem+cpu+handles");
        if let Some(&one) = set.first() {
            time_iters(iters, || {
                if let Ok(s) = win::sample_mem_cpu(one) {
                    std::hint::black_box(s.working_set);
                }
            })
            .print("winapi mem+cpu (1 pid)");
            // Хендлы (отдельное открытие с QUERY_INFORMATION) — вне горячего пути.
            time_iters(iters, || {
                if let Ok(n) = win::sample_handles(one) {
                    std::hint::black_box(n);
                }
            })
            .print("winapi handles (1 pid)");
        }
    }

    #[cfg(feature = "sysinfo")]
    {
        let mut s = si::Sampler::new(&set);
        s.refresh();
        time_iters(iters, || {
            s.refresh();
            std::hint::black_box(s.get(set[0]));
        })
        .print("sysinfo refresh (set)");

        let mut all = si::Sampler::new(&set);
        all.refresh_all();
        time_iters(iters, || {
            all.refresh_all();
        })
        .print("sysinfo refresh (all procs)");
    }

    // ---- 3. CPU самого опроса за 2-секундный интервал ----
    for n in [1usize, 5, 20] {
        let sub: Vec<u32> = pids.iter().copied().take(n).collect();
        if sub.is_empty() {
            continue;
        }
        #[cfg(feature = "winapi")]
        poll_cpu("winapi", &sub, poll_secs, |pids| {
            for &p in pids {
                if let Ok(s) = win::sample_mem_cpu(p) {
                    std::hint::black_box(s.working_set);
                }
            }
        });
        #[cfg(feature = "sysinfo")]
        {
            let mut s = si::Sampler::new(&sub);
            s.refresh();
            poll_cpu("sysinfo", &sub, poll_secs, move |pids| {
                s.refresh();
                std::hint::black_box(s.get(pids[0]));
            });
        }
    }

    // ---- 4. Краевые случаи ----
    edge_cases(&mut kids, reuse_probe);

    // Уборка.
    for mut c in kids {
        let _ = c.kill();
        let _ = c.wait();
    }
    println!("[DONE]");
}

/// Меряет CPU самого опроса: поток крутит сэмпл набора с интервалом 2 с, а основной
/// поток считает потреблённое процессом CPU за окно `secs`.
#[cfg(any(feature = "winapi", feature = "sysinfo"))]
fn poll_cpu<F>(label: &str, set: &[u32], secs: u64, mut sample: F)
where
    F: FnMut(&[u32]) + Send + 'static,
{
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    let set2: Vec<u32> = set.to_vec();
    let handle = std::thread::spawn(move || {
        while !stop2.load(Ordering::Relaxed) {
            sample(&set2);
            // «2 c» бюджет монитора; спим чанками, чтобы быстро реагировать на стоп.
            let mut slept = 0u64;
            while slept < 2000 && !stop2.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(50));
                slept += 50;
            }
        }
    });

    #[cfg(windows)]
    let c0 = own_cpu::process_cpu_100ns();
    std::thread::sleep(Duration::from_secs(secs));
    #[cfg(windows)]
    let c1 = own_cpu::process_cpu_100ns();

    stop.store(true, Ordering::Relaxed);
    let _ = handle.join();

    #[cfg(windows)]
    {
        let cpu_secs = (c1.saturating_sub(c0)) as f64 / 1e7;
        let pct = cpu_secs / secs as f64 * 100.0;
        let polls = secs as f64 / 2.0;
        let per_poll_us = if polls > 0.0 {
            cpu_secs * 1e6 / polls
        } else {
            0.0
        };
        println!(
            "[POLL] {label:<8} n={:<3} window={secs}s cpu_total={:.4}%one_core ~{per_poll_us:.1} us CPU/poll",
            set.len(),
            pct
        );
    }
    #[cfg(not(windows))]
    {
        let _ = (label, secs);
    }
}

/// Краевые случаи: мёртвый/несуществующий/недоступный PID и best-effort PID reuse.
fn edge_cases(kids: &mut Vec<Child>, reuse_probe: usize) {
    println!("[EDGE] == краевые случаи ==");

    // (а) Мёртвый PID: убиваем одного ребёнка и сразу опрашиваем его старый PID.
    let dead_pid = if let Some(mut victim) = kids.pop() {
        let pid = victim.id();
        let _ = victim.kill();
        let _ = victim.wait();
        pid
    } else {
        0
    };
    probe_pid("dead_child", dead_pid);

    // (б) Заведомо несуществующий PID (максимум диапазона u32) и 0.
    probe_pid("u32::MAX", u32::MAX);
    probe_pid("pid_0", 0);
    // (в) Системный процесс — вероятный отказ доступа.
    probe_pid("system_pid_4", 4);

    // (г) PID reuse: best-effort — спавним короткоживущие процессы и ищем повтор dead_pid.
    if dead_pid != 0 && reuse_probe > 0 {
        let exe = std::env::current_exe().expect("current_exe");
        let mut reused = None;
        for _ in 0..reuse_probe {
            let Ok(mut c) = Command::new(&exe)
                .arg("--sleep")
                .arg("0")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            else {
                break;
            };
            let pid = c.id();
            let _ = c.wait();
            if pid == dead_pid {
                reused = Some(pid);
                break;
            }
        }
        match reused {
            Some(pid) => println!(
                "[EDGE] pid_reuse: PID {pid} переиспользован в пределах {reuse_probe} spawn'ов \
                 => открытие по PID вернёт ДРУГОЙ процесс (winapi не различает; sysinfo хранит start_time)"
            ),
            None => println!(
                "[EDGE] pid_reuse: в пределах {reuse_probe} spawn'ов повтор PID не наблюдался \
                 (на Windows PID переиспользуется из свободного списка, цикл длинный)"
            ),
        }
    }
}

/// Опрашивает один PID обоими бэкендами, печатая исход (без паники).
fn probe_pid(tag: &str, pid: u32) {
    #[cfg(feature = "winapi")]
    {
        let open = win::try_open(pid);
        let all = win::sample_all(pid);
        println!(
            "[EDGE] {tag:<14} pid={pid:<12} winapi_open={:?} winapi_sample={:?}",
            open.map(|_| "ok"),
            all.map(|s| (s.working_set, s.handles))
                .map_err(|e| format!("win32({e})"))
        );
    }
    #[cfg(feature = "sysinfo")]
    {
        use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
        let mut s = System::new();
        let one = [Pid::from_u32(pid)];
        let n = s.refresh_processes_specifics(
            ProcessesToUpdate::Some(&one),
            false,
            ProcessRefreshKind::nothing().with_memory(),
        );
        let got = s.process(Pid::from_u32(pid)).map(|p| p.memory());
        println!("[EDGE] {tag:<14} pid={pid:<12} sysinfo_refreshed={n} sysinfo_mem={got:?}");
    }
    #[cfg(not(any(feature = "winapi", feature = "sysinfo")))]
    {
        let _ = (tag, pid);
    }
}

#[cfg(test)]
mod tests {
    use super::Stats;

    /// ns → µs с округлением: 1000 нс = 1.0 µс.
    #[test]
    fn stats_single_value_is_identical() {
        let s = Stats::from_nanos(vec![5_000]);
        assert_eq!(s.n, 1);
        assert_eq!(
            (s.min, s.p50, s.p90, s.p99, s.max, s.mean),
            (5.0, 5.0, 5.0, 5.0, 5.0, 5.0)
        );
    }

    /// Пустая выборка не паникует (в стенде возможен прогон с 0 итераций).
    #[test]
    fn stats_empty_is_zero_not_panic() {
        let s = Stats::from_nanos(vec![]);
        assert_eq!(s.n, 0);
        assert_eq!((s.min, s.p50, s.max, s.mean), (0.0, 0.0, 0.0, 0.0));
    }

    /// Сортировка и перцентили: вход в обратном порядке обрабатывается как упорядоченный.
    #[test]
    fn stats_sorting_and_percentiles() {
        // 3 значения (мкс): 3, 1, 2 → отсортировано 1, 2, 3.
        let s = Stats::from_nanos(vec![3_000, 1_000, 2_000]);
        assert_eq!(s.n, 3);
        assert_eq!(s.min, 1.0);
        assert_eq!(s.max, 3.0);
        // p50 при n=3: idx = round(0.5*2) = 1 → медиана 2.
        assert_eq!(s.p50, 2.0);
        assert_eq!(s.mean, 2.0);
        // Монотонность перцентилей обязательна для читаемости отчёта.
        assert!(s.min <= s.p50 && s.p50 <= s.p90 && s.p90 <= s.p99 && s.p99 <= s.max);
    }

    /// Полный набор 1..=100: проверяем, что перцентили не «уползают» за границы и
    /// mean считается по среднему, а не по медиане.
    #[test]
    fn stats_percentiles_bounds_and_mean() {
        let nanos: Vec<u128> = (1..=100).map(|i| i * 1_000).collect();
        let s = Stats::from_nanos(nanos);
        assert_eq!(s.n, 100);
        assert_eq!(s.min, 1.0);
        assert_eq!(s.max, 100.0);
        // idx(p50) = round(0.5*99) = 50 → 51-е значение (1-based) = 51 мкс.
        assert_eq!(s.p50, 51.0);
        // mean — среднее арифметическое 1..=100 = 50.5.
        assert_eq!(s.mean, 50.5);
        assert!(s.p99 <= s.max && s.p90 <= s.p99 && s.p50 <= s.p90);
    }
}
