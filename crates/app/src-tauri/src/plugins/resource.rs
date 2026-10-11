//! Сбор метрик ресурсов (ОЗУ/CPU) главного процесса и child'ов плагинов (TZ-RESOURCE-MON,
//! ADR-0031). Модуль **не знает про Tauri**: наружу отдаёт типы снимка и канал
//! [`ResourceSink`], который в `setup_plugins` конвертируется в Tauri-события.
//!
//! Источник — WinAPI через крейт `windows` (`K32GetProcessMemoryInfo`, `GetProcessTimes`,
//! `GetProcessHandleCount`) за [`ProcessMetrics`]. Основная метрика ОЗУ зависит от класса
//! процесса (ADR-0031 §2.1 п.3): у **child'а** — `PrivateUsage` (private/commit, та же
//! величина, что ограничивает Job Object), у **главного** — `WorkingSetSize` (рабочий набор).
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
    K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
};
use windows::Win32::System::Threading::{
    GetProcessHandleCount, GetProcessTimes, OpenProcess, PROCESS_QUERY_INFORMATION,
    PROCESS_QUERY_LIMITED_INFORMATION,
};

/// Период опроса монитора (§6): тик раз в 2 с, отдельный поток.
pub const POLL_PERIOD: Duration = Duration::from_secs(2);

/// TTL записей `cpu_prev` (10× период опроса). Протухшие PID чистятся на каждом `read`:
/// иначе карта растёт монотонно при reload/crash плагинов, а переиспользованный чужой
/// PID давал бы ложную дельту CPU (ADR-0031 §2.5 п.20).
const CPU_PREV_TTL: Duration = Duration::from_secs(POLL_PERIOD.as_secs() * 10);

/// Сырые метрики одного процесса за один вызов провайдера.
///
/// `working_set_bytes`/`private_bytes` берутся из одного `K32GetProcessMemoryInfo`
/// (`PROCESS_MEMORY_COUNTERS_EX`); `cpu_percent`/`handles` опциональны — фронт рендерит их,
/// если есть (TZ §4.4). Отсутствие метрики представляется `None`, а не нулём (F51).
#[derive(Debug, Clone, Copy, Default)]
pub struct RawMetrics {
    /// Рабочий набор (`WorkingSetSize`), байты.
    pub working_set_bytes: usize,
    /// Private/commit (`PrivateUsage`), байты — то, что ограничивает Job Object.
    pub private_bytes: usize,
    /// CPU % (доля одного ядра), `None` если измерить не удалось.
    pub cpu_percent: Option<f64>,
    /// Число хендлов процесса, `None` если прав на чтение не хватает.
    pub handles: Option<u32>,
}

/// Шов тестопригодности: источник «сырых» метрик процесса по PID.
///
/// WinAPI-реализация — [`WinProcessMetrics`]; тесты подставляют фейк без реальных процессов.
/// `None` — недоступный/мёртвый/невалидный PID: метрика пропускается, не паника и не «0».
pub trait ProcessMetrics: Send + Sync {
    fn read(&self, pid: u32) -> Option<RawMetrics>;
}

/// Метрики главного процесса (`WorkingSetSize`; лимита Job Object нет).
#[derive(Debug, Clone, Serialize)]
pub struct MainMetrics {
    /// Рабочий набор, байты (TZ §4.2).
    pub mem_bytes: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handles: Option<u32>,
}

/// Метрики child'а плагина (`PrivateUsage`; знаменатель порогов — лимит Job Object).
#[derive(Debug, Clone, Serialize)]
pub struct ChildMetrics {
    pub plugin_id: String,
    pub pid: u32,
    /// Private/commit, байты (TZ §4.2).
    pub mem_bytes: usize,
    /// Лимит Job Object плагина (private/commit), байты — знаменатель прогресс-бара (TZ §6.5).
    pub mem_limit_bytes: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handles: Option<u32>,
}

/// Снимок метрик для события `resource-metrics` (TZ §4.4).
#[derive(Debug, Clone, Serialize)]
pub struct ResourceSnapshot {
    /// Момент снятия (`Date.now()` на Rust-стороне), мс.
    pub ts: u64,
    pub main: MainMetrics,
    pub children: Vec<ChildMetrics>,
}

impl ResourceSnapshot {
    /// Собирает снимок по списку детей.
    ///
    /// `None` — если метрика **главного** процесса не читается (`provider.read(main_pid)`
    /// вернул `None`): отсутствующая метрика — не ноль (F51/ADR-0031 §2.3 п.11), а главный
    /// процесс практически всегда читаем, так что лживый `mem_bytes: 0` недопустим. Для главного
    /// берётся `working_set_bytes`, для child'а — `private_bytes` (commit). Нечитаемые дети
    /// пропускаются (не «0»). `mem_limit_bytes` — бюджет Job Object, только у детей (TZ §4.4).
    ///
    // TODO(Фаза 3, ADR-0031 §2.5 п.21): `mem_limit_bytes` — скаляр, общий на всех детей.
    //   При пер-плагинных бюджетах (`super::supervisor::SupervisorParams::with_mem_bytes`)
    //   лимит нужно брать из `PluginRuntime` каждого плагина, а не передавать одним числом.
    pub fn collect(
        provider: &dyn ProcessMetrics,
        main_pid: u32,
        children: &[(String, u32)],
        mem_limit_bytes: usize,
        ts: u64,
    ) -> Option<Self> {
        // Главный процесс — обязательная часть снимка: без его метрики отдаём `None`,
        // а не подставляем 0 (нечитаемый главный → «нечего показывать», не ложный ноль).
        let main_raw = provider.read(main_pid)?;
        let children: Vec<ChildMetrics> = children
            .iter()
            .filter_map(|(plugin_id, pid)| {
                let raw = provider.read(*pid)?;
                Some(ChildMetrics {
                    plugin_id: plugin_id.clone(),
                    pid: *pid,
                    // Child ограничен Job Object по committed-памяти — берём private/commit.
                    mem_bytes: raw.private_bytes,
                    mem_limit_bytes,
                    cpu_percent: raw.cpu_percent,
                    handles: raw.handles,
                })
            })
            .collect();

        let main = MainMetrics {
            mem_bytes: main_raw.working_set_bytes,
            cpu_percent: main_raw.cpu_percent,
            handles: main_raw.handles,
        };
        Some(Self { ts, main, children })
    }
}

/// Канал фронт-шины ресурс-монитора (аналог `LogSink`/`DocumentSink`).
///
/// Реализация в `setup_plugins` эмитит Tauri-события `resource-metrics`/`plugin-process-exited`.
/// Best-effort: потеря семпла некритична.
pub trait ResourceSink: Send + Sync {
    /// Текущий снимок (событие `resource-metrics`).
    fn metrics(&self, snapshot: &ResourceSnapshot);
    /// Child утрачен по **владению процессом** (событие `plugin-process-exited`).
    fn process_exited(&self, plugin_id: &str, pid: u32, reason: Option<&str>);
}

/// Монитор: один [`Self::tick`] — цикл «снимок PID → замер → эмит»; [`Self::spawn`]
/// оборачивает его в поток с флагом остановки (ADR-0031 §2.3 п.9).
pub struct ResourceMonitor {
    provider: Arc<dyn ProcessMetrics>,
    sink: Arc<dyn ResourceSink>,
    main_pid: u32,
    mem_limit_bytes: usize,
    /// Владение child'ами на предыдущем тике: `plugin_id → pid`. Утрата записи (или смена PID
    /// при том же `plugin_id`) — это `plugin-process-exited`, а не проба «голого PID»
    /// (ADR-0031 §2.4 п.19).
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

    /// Один цикл сбора/эмита. `children` — активные `(plugin_id, pid)` из менеджера,
    /// снятые вызывающей стороной под коротким локом (вне этого вызова блокировок нет).
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

    /// Запускает поток опроса: `children()` снимает активные PID, `stop` взводится при
    /// закрытии приложения. Спит чанками, чтобы быстро реагировать на остановку.
    ///
    /// `children()` отдаёт `None`, когда снимок PID сейчас взять нельзя (занятый лок
    /// менеджера): тик **пропускается** — без ложных `plugin-process-exited` и без
    /// долгого ожидания (best-effort, ADR-0031 §2.3 п.10).
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

/// Текущее время в миллисекундах Unix-эпохи (без паники на сбое часов).
fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// WinAPI-провайдер метрик (Windows-only, ADR-0031 §2.1 п.1).
///
/// Держит `cpu_prev` для расчёта CPU % как дельты `GetProcessTimes` между тиками. Этот мьютекс
/// — внутренний для провайдера и не пересекается с `PluginManager`/`DocumentStore`.
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

    /// CPU % как доля одного ядра: `Δ(cpu_100ns) / Δ(wall_100ns) × 100`. `None`, если
    /// предыдущего замера нет или прошло нулевое время.
    fn cpu_percent(&self, pid: u32, cpu_100ns: Option<u64>) -> Option<f64> {
        let cpu = cpu_100ns?;
        let now = Instant::now();
        let mut prev = self.cpu_prev.lock().ok()?;
        // Чистим записи, не читавшиеся дольше TTL: PID reload'нутого/упавшего плагина
        // не копится в карте и не даёт ложную дельту при PID-reuse (ADR-0031 §2.5 п.20).
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
        // SAFETY и устойчивость: FFI-вызовы WinAPI ниже не разворачивают стек и не паникуют
        // (профиль `panic = "abort"`); любой отказ прав/доступа → `None`, запись пропускается.
        let access = PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_QUERY_INFORMATION;
        let handle = unsafe { OpenProcess(access, false, pid) }.ok()?;

        let memory = unsafe { memory_counter(handle) };
        let cpu = unsafe { cpu_100ns(handle) };
        // `GetProcessHandleCount` требует `PROCESS_QUERY_INFORMATION` (ADR-0031 §2.7 п.27):
        // при отказе прав хендлы просто не показываем.
        let mut handles_count = 0u32;
        let handles = unsafe { GetProcessHandleCount(handle, &mut handles_count) }
            .is_ok()
            .then_some(handles_count);
        // Хэндл закрываем сразу, вне зависимости от исхода замеров.
        unsafe {
            let _ = CloseHandle(handle);
        }

        let (working_set_bytes, private_bytes) = memory?;
        let cpu_percent = self.cpu_percent(pid, cpu);
        Some(RawMetrics {
            working_set_bytes,
            private_bytes,
            cpu_percent,
            handles,
        })
    }
}

/// (рабочий набор, private/commit) по открытому хэндлу — один вызов `K32GetProcessMemoryInfo`.
///
/// SAFETY: `handle` — валидный хэндл с правом запроса; `counters` — локальная структура
/// нужного размера; WinAPI пишет в неё и возвращает BOOL (не Result).
unsafe fn memory_counter(handle: HANDLE) -> Option<(usize, usize)> {
    let mut counters = PROCESS_MEMORY_COUNTERS_EX::default();
    let ok = K32GetProcessMemoryInfo(
        handle,
        &mut counters as *mut PROCESS_MEMORY_COUNTERS_EX as *mut PROCESS_MEMORY_COUNTERS,
        std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
    );
    ok.as_bool()
        .then_some((counters.WorkingSetSize, counters.PrivateUsage))
}

/// CPU-время (kernel + user) в единицах 100 нс по открытому хэндлу.
///
/// SAFETY: все `FILETIME` — локальные; `handle` валиден.
unsafe fn cpu_100ns(handle: HANDLE) -> Option<u64> {
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user)
        .ok()
        .map(|_| filetime_to_u64(kernel) + filetime_to_u64(user))
}

/// `FILETIME` (два 32-битных поля) → 64-битное значение (единицы 100 нс).
fn filetime_to_u64(time: FILETIME) -> u64 {
    ((time.dwHighDateTime as u64) << 32) | (time.dwLowDateTime as u64)
}
