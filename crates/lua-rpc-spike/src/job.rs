//! Job Object (Windows) — лимиты RSS/CPU/время для child-процесса плагина (P9/M11, N5).
//!
//! Без внешних крейтов: нужен минимум API, объявляем FFI на kernel32 вручную. Задача — не
//! ручной kill по таймауту (как в C-спайке), а **детерминированное** срабатывание лимита ОС:
//! CPU/время — процесс завершает сам Job; память — аллокации в процессе начинают падать
//! (`malloc` → NULL), а Job-учёт `PeakProcessMemoryUsed` ограничен сверху.

#![cfg(windows)]

use std::ffi::c_void;
use std::io;

type Handle = *mut c_void;
type Bool = i32;

#[link(name = "kernel32")]
extern "system" {
    fn CreateJobObjectW(attrs: *const c_void, name: *const u16) -> Handle;
    fn SetInformationJobObject(job: Handle, class: u32, info: *const c_void, len: u32) -> Bool;
    fn QueryInformationJobObject(
        job: Handle,
        class: u32,
        info: *mut c_void,
        len: u32,
        ret: *mut u32,
    ) -> Bool;
    fn AssignProcessToJobObject(job: Handle, process: Handle) -> Bool;
    fn TerminateJobObject(job: Handle, code: u32) -> Bool;
    fn OpenProcess(access: u32, inherit: Bool, pid: u32) -> Handle;
    fn CloseHandle(h: Handle) -> Bool;
    fn GetLastError() -> u32;
    fn GetCurrentProcess() -> Handle;
    fn IsProcessInJob(process: Handle, job: Handle, result: *mut Bool) -> Bool;
}

/// Находится ли **наш** процесс внутри чужого Job (тогда создаваемый Job — вложенный).
/// Вложенность важна: для неё действует правило «эффективный лимит — не менее строгий из цепочки».
pub fn parent_is_in_job() -> bool {
    let mut r: Bool = 0;
    unsafe { IsProcessInJob(GetCurrentProcess(), std::ptr::null_mut(), &mut r) };
    r != 0
}

const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: u32 = 9;
const JOB_OBJECT_BASIC_ACCOUNTING_INFORMATION: u32 = 1;

const JOB_OBJECT_LIMIT_PROCESS_TIME: u32 = 0x0000_0002;
const JOB_OBJECT_LIMIT_JOB_TIME: u32 = 0x0000_0004;
const JOB_OBJECT_LIMIT_PROCESS_MEMORY: u32 = 0x0000_0100;
const JOB_OBJECT_LIMIT_JOB_MEMORY: u32 = 0x0000_0200;
const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x0000_2000;

const PROCESS_SET_QUOTA: u32 = 0x0100;
const PROCESS_TERMINATE: u32 = 0x0001;

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct IoCounters {
    read_ops: u64,
    write_ops: u64,
    other_ops: u64,
    read_bytes: u64,
    write_bytes: u64,
    other_bytes: u64,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct BasicLimitInformation {
    per_process_user_time_limit: i64, // 100 нс
    per_job_user_time_limit: i64,     // 100 нс
    limit_flags: u32,
    minimum_working_set_size: usize,
    maximum_working_set_size: usize,
    active_process_limit: u32,
    affinity: usize,
    priority_class: u32,
    scheduling_class: u32,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct ExtendedLimitInformation {
    basic: BasicLimitInformation,
    io_info: IoCounters,
    process_memory_limit: usize,
    job_memory_limit: usize,
    peak_process_memory_used: usize,
    peak_job_memory_used: usize,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct BasicAccountingInformation {
    total_user_time: i64,
    total_kernel_time: i64,
    this_period_total_user_time: i64,
    this_period_total_kernel_time: i64,
    total_page_fault_count: u32,
    total_processes: u32,
    active_processes: u32,
    total_terminated_processes: u32,
}

fn last_error() -> io::Error {
    io::Error::from_raw_os_error(unsafe { GetLastError() } as i32)
}

/// Job Object как RAII-хэндл. С `KILL_ON_JOB_CLOSE` смерть parent уносит и child.
pub struct Job {
    handle: Handle,
}

// Хэндл ядра потокобезопасен для передачи между потоками.
unsafe impl Send for Job {}

impl Job {
    pub fn new() -> io::Result<Self> {
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_null() {
            return Err(last_error());
        }
        Ok(Self { handle })
    }

    /// Лимиты: `cpu_ms` — суммарное пользовательское время процесса (0 = выключить),
    /// `mem_bytes` — лимит committed-памяти процесса и job'а (0 = выключить).
    pub fn set_limits(&self, cpu_ms: u64, mem_bytes: usize) -> io::Result<()> {
        let mut info = ExtendedLimitInformation::default();
        let mut flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

        if cpu_ms > 0 {
            // Ставим и per-process, и per-job лимит: эффективный — более строгий (равны).
            flags |= JOB_OBJECT_LIMIT_PROCESS_TIME | JOB_OBJECT_LIMIT_JOB_TIME;
            let ticks = (cpu_ms as i64) * 10_000; // мс → 100 нс
            info.basic.per_process_user_time_limit = ticks;
            info.basic.per_job_user_time_limit = ticks;
        }
        if mem_bytes > 0 {
            flags |= JOB_OBJECT_LIMIT_PROCESS_MEMORY | JOB_OBJECT_LIMIT_JOB_MEMORY;
            info.process_memory_limit = mem_bytes;
            info.job_memory_limit = mem_bytes;
        }
        info.basic.limit_flags = flags;

        let ok = unsafe {
            SetInformationJobObject(
                self.handle,
                JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
                &info as *const _ as *const c_void,
                std::mem::size_of::<ExtendedLimitInformation>() as u32,
            )
        };
        if ok == 0 {
            return Err(last_error());
        }
        Ok(())
    }

    /// Назначает процесс по PID (нужны права PROCESS_SET_QUOTA|PROCESS_TERMINATE).
    pub fn assign_pid(&self, pid: u32) -> io::Result<()> {
        let proc = unsafe { OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid) };
        if proc.is_null() {
            return Err(last_error());
        }
        let ok = unsafe { AssignProcessToJobObject(self.handle, proc) };
        let err = if ok == 0 { Some(last_error()) } else { None };
        unsafe { CloseHandle(proc) };
        match err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Принудительно завершает все процессы job'а (страховка/уборка).
    pub fn terminate(&self, code: u32) {
        unsafe { TerminateJobObject(self.handle, code) };
    }

    /// То, что реально хранит ОС: (limit_flags, PerProcessUserTimeLimit 100нс, PerJobUserTimeLimit 100нс).
    pub fn stored_limits(&self) -> (u32, i64, i64) {
        let mut info = ExtendedLimitInformation::default();
        let ok = unsafe {
            QueryInformationJobObject(
                self.handle,
                JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
                &mut info as *mut _ as *mut c_void,
                std::mem::size_of::<ExtendedLimitInformation>() as u32,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            (0, 0, 0)
        } else {
            (
                info.basic.limit_flags,
                info.basic.per_process_user_time_limit,
                info.basic.per_job_user_time_limit,
            )
        }
    }

    /// Пиковая committed-память процессов job'а, байт.
    pub fn peak_process_memory(&self) -> usize {
        let mut info = ExtendedLimitInformation::default();
        let ok = unsafe {
            QueryInformationJobObject(
                self.handle,
                JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
                &mut info as *mut _ as *mut c_void,
                std::mem::size_of::<ExtendedLimitInformation>() as u32,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            0
        } else {
            info.peak_process_memory_used
        }
    }

    /// Пиковая job-wide committed-память (сумма по всем процессам job'а), байт.
    pub fn peak_job_memory(&self) -> usize {
        let mut info = ExtendedLimitInformation::default();
        let ok = unsafe {
            QueryInformationJobObject(
                self.handle,
                JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
                &mut info as *mut _ as *mut c_void,
                std::mem::size_of::<ExtendedLimitInformation>() as u32,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            0
        } else {
            info.peak_job_memory_used
        }
    }

    /// Суммарное пользовательское время процессов job'а, мс.
    pub fn total_user_time_ms(&self) -> f64 {
        let mut info = BasicAccountingInformation::default();
        let ok = unsafe {
            QueryInformationJobObject(
                self.handle,
                JOB_OBJECT_BASIC_ACCOUNTING_INFORMATION,
                &mut info as *mut _ as *mut c_void,
                std::mem::size_of::<BasicAccountingInformation>() as u32,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            0.0
        } else {
            info.total_user_time as f64 / 10_000.0
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.handle) };
    }
}
