//! Сбор метрик ресурсов (ОЗУ/CPU) главного процесса и child'ов плагинов (ADR-0031).
//! Модуль **не знает про Tauri**: наружу отдаёт типы снимка и канал
//! [`ResourceSink`], который в `setup_plugins` конвертируется в Tauri-события.
//!
//! Источник — WinAPI через крейт `windows` (`K32GetProcessMemoryInfo`, `GetProcessTimes`,
//! `GetProcessHandleCount`) за [`ProcessMetrics`]. Основная метрика ОЗУ зависит от класса
//! процесса (ADR-0031 §2.1 п.3): у **child'а** — `PrivateUsage` (private/commit, та же
//! величина, что ограничивает Job Object), у **главного** — `PrivateWorkingSetSize`
//! (`PROCESS_MEMORY_COUNTERS_EX2`, приватный рабочий набор — то, что показывает Диспетчер
//! задач), с фолбэком на `WorkingSetSize` там, где EX2-хвост не заполнен.
//!
//! **Дедлок-безопасность (ADR-0031 §2.3 п.10):** снимок списка `(plugin_id, pid)` берётся
//! неблокирующим `try_lock` `PluginManager` в потоке монитора; при занятом локе тик
//! пропускается (best-effort). WinAPI-замеры и эмит идут уже **вне** блокировок
//! менеджера/стора. Сам [`ResourceMonitor::tick`] ничего не блокирует.
//!
//! **Устойчивость FFI (ADR-0031 §2.3 п.11, F51):** в FFI-пути нет `unwrap`/`expect`/`panic!`
//! (профиль `panic = "abort"`). Недоступный/мёртвый/невалидный PID → `None`, запись
//! пропускается. Отсутствующая метрика главного процесса → снимка нет целиком (`None`),
//! а не лживый ноль: главный процесс практически всегда читаем.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use windows::Win32::Foundation::{CloseHandle, FILETIME, HANDLE};
use windows::Win32::System::ProcessStatus::{
    K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX2,
};
use windows::Win32::System::Threading::{
    GetProcessHandleCount, GetProcessTimes, OpenProcess, PROCESS_QUERY_INFORMATION,
    PROCESS_QUERY_LIMITED_INFORMATION,
};


pub const POLL_PERIOD: Duration = Duration::from_secs(2);




const CPU_PREV_TTL: Duration = Duration::from_secs(POLL_PERIOD.as_secs() * 10);







#[derive(Debug, Clone, Copy, Default)]
pub struct RawMetrics {
    
    pub working_set_bytes: usize,
    
    pub private_bytes: usize,
    
    
    
    pub private_working_set_bytes: usize,
    
    pub cpu_percent: Option<f64>,
    
    pub handles: Option<u32>,
}

impl RawMetrics {
    
    
    
    pub fn private_working_set(&self) -> usize {
        if self.private_working_set_bytes == 0 {
            self.working_set_bytes
        } else {
            self.private_working_set_bytes
        }
    }
}





pub trait ProcessMetrics: Send + Sync {
    fn read(&self, pid: u32) -> Option<RawMetrics>;
}


#[derive(Debug, Clone, Serialize)]
pub struct MainMetrics {
    
    pub mem_bytes: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handles: Option<u32>,
}


#[derive(Debug, Clone, Serialize)]
pub struct ChildMetrics {
    pub plugin_id: String,
    pub pid: u32,
    
    pub mem_bytes: usize,
    
    pub mem_limit_bytes: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handles: Option<u32>,
}


#[derive(Debug, Clone, Serialize)]
pub struct ResourceSnapshot {
    
    pub ts: u64,
    pub main: MainMetrics,
    pub children: Vec<ChildMetrics>,
}

impl ResourceSnapshot {
    
    
    
    
    
    
    
    
    
    
    pub fn collect(
        provider: &dyn ProcessMetrics,
        main_pid: u32,
        children: &[(String, u32)],
        mem_limit_bytes: usize,
        ts: u64,
    ) -> Option<Self> {
        let main_raw = provider.read(main_pid)?;
        let children: Vec<ChildMetrics> = children
            .iter()
            .filter_map(|(plugin_id, pid)| {
                let raw = provider.read(*pid)?;
                Some(ChildMetrics {
                    plugin_id: plugin_id.clone(),
                    pid: *pid,
                    mem_bytes: raw.private_bytes,
                    mem_limit_bytes,
                    cpu_percent: raw.cpu_percent,
                    handles: raw.handles,
                })
            })
            .collect();

        let main = MainMetrics {
            mem_bytes: main_raw.private_working_set(),
            cpu_percent: main_raw.cpu_percent,
            handles: main_raw.handles,
        };
        Some(Self { ts, main, children })
    }
}





pub trait ResourceSink: Send + Sync {
    
    fn metrics(&self, snapshot: &ResourceSnapshot);
    
    fn process_exited(&self, plugin_id: &str, pid: u32, reason: Option<&str>);
}



pub struct ResourceMonitor {
    provider: Arc<dyn ProcessMetrics>,
    sink: Arc<dyn ResourceSink>,
    main_pid: u32,
    mem_limit_bytes: usize,
    
    
    
    active: HashMap<String, u32>,
}

impl ResourceMonitor {
    pub fn new<S: ResourceSink + 'static>(
        provider: Arc<dyn ProcessMetrics>,
        sink: Arc<S>,
        main_pid: u32,
        mem_limit_bytes: usize,
    ) -> Self {
        Self {
            provider,
            sink,
            main_pid,
            mem_limit_bytes,
            active: HashMap::new(),
        }
    }

    
    
    pub fn tick(&mut self, children: &[(String, u32)]) {
        let snapshot = ResourceSnapshot::collect(
            self.provider.as_ref(),
            self.main_pid,
            children,
            self.mem_limit_bytes,
            now_millis(),
        );

        let new_active: HashMap<String, u32> = children.iter().cloned().collect();
        for (plugin_id, pid) in &self.active {
            if new_active.get(plugin_id) != Some(pid) {
                self.sink.process_exited(plugin_id, *pid, None);
            }
        }
        self.active = new_active;

        if let Some(snapshot) = snapshot {
            self.sink.metrics(&snapshot);
        }
    }

    
    
    
    
    
    
    pub fn spawn<F>(self, children: F, stop: Arc<AtomicBool>) -> JoinHandle<()>
    where
        F: Fn() -> Option<Vec<(String, u32)>> + Send + 'static,
    {
        thread::spawn(move || {
            let mut monitor = self;
            while !stop.load(Ordering::Relaxed) {
                if let Some(kids) = children() {
                    monitor.tick(&kids);
                }
                let deadline = Instant::now() + POLL_PERIOD;
                while Instant::now() < deadline && !stop.load(Ordering::Relaxed) {
                    thread::sleep(Duration::from_millis(50));
                }
            }
        })
    }
}


fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}





pub struct WinProcessMetrics {
    cpu_prev: std::sync::Mutex<HashMap<u32, (u64, Instant)>>,
}

impl Default for WinProcessMetrics {
    fn default() -> Self {
        Self {
            cpu_prev: std::sync::Mutex::new(HashMap::new()),
        }
    }
}

impl WinProcessMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    
    
    fn cpu_percent(&self, pid: u32, cpu_100ns: Option<u64>) -> Option<f64> {
        let cpu = cpu_100ns?;
        let now = Instant::now();
        let mut prev = self.cpu_prev.lock().ok()?;
        prev.retain(|_, (_, at)| now.duration_since(*at) < CPU_PREV_TTL);
        let previous = prev.insert(pid, (cpu, now));
        let (prev_cpu, prev_at) = previous?;
        let wall_100ns = now.duration_since(prev_at).as_nanos() as u64 / 100;
        if wall_100ns == 0 {
            return None;
        }
        Some(cpu.saturating_sub(prev_cpu) as f64 / wall_100ns as f64 * 100.0)
    }
}

impl ProcessMetrics for WinProcessMetrics {
    fn read(&self, pid: u32) -> Option<RawMetrics> {
        let access = PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_QUERY_INFORMATION;
        let handle = unsafe { OpenProcess(access, false, pid) }.ok()?;

        let memory = unsafe { memory_counter(handle) };
        let cpu = unsafe { cpu_100ns(handle) };
        let mut handles_count = 0u32;
        let handles = unsafe { GetProcessHandleCount(handle, &mut handles_count) }
            .is_ok()
            .then_some(handles_count);
        unsafe {
            let _ = CloseHandle(handle);
        }

        let (working_set_bytes, private_bytes, private_working_set_bytes) = memory?;
        let cpu_percent = self.cpu_percent(pid, cpu);
        Some(RawMetrics {
            working_set_bytes,
            private_bytes,
            private_working_set_bytes,
            cpu_percent,
            handles,
        })
    }
}










unsafe fn memory_counter(handle: HANDLE) -> Option<(usize, usize, usize)> {
    let mut counters = PROCESS_MEMORY_COUNTERS_EX2::default();
    let ok = K32GetProcessMemoryInfo(
        handle,
        &mut counters as *mut PROCESS_MEMORY_COUNTERS_EX2 as *mut PROCESS_MEMORY_COUNTERS,
        std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX2>() as u32,
    );
    ok.as_bool().then_some((
        counters.WorkingSetSize,
        counters.PrivateUsage,
        counters.PrivateWorkingSetSize,
    ))
}




unsafe fn cpu_100ns(handle: HANDLE) -> Option<u64> {
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user)
        .ok()
        .map(|_| filetime_to_u64(kernel) + filetime_to_u64(user))
}


fn filetime_to_u64(time: FILETIME) -> u64 {
    ((time.dwHighDateTime as u64) << 32) | (time.dwLowDateTime as u64)
}
