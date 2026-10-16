//! `Supervisor` — надзор за одним child-процессом плагина (Фаза 2 плагинной системы).
//!
//! Отвечает за транспорт stdio, Job Object (лимит памяти + `KILL_ON_JOB_CLOSE`), два таймера
//! watchdog (прогресс + абсолютный дедлайн invocation, §4.3), обслуживание host-call'ов с
//! проверкой permissions (§4.4). GUI-процесс при этом остаётся на `panic = "abort"` и не
//! линкует `mlua`: плагин — внешний exe.

mod permissions;
#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use plugin_core::envelope::{self, PluginError, ToChild, ToHost};
use plugin_core::job::Job;
use plugin_core::limits::{DEADLINE_MS, DEFAULT_MEM_BYTES, PROGRESS_TIMEOUT_MS};
use serde_json::Value;

use super::HostServices;

use permissions::{decode_error, with_plugin_id};

pub use permissions::{check_permission, required_permission};


const CREATE_NO_WINDOW: u32 = 0x0800_0000;


pub const CHILD_EXE_NAME: &str = "darmark-plugin-host.exe";


pub struct SupervisorParams {
    
    pub exe: PathBuf,
    pub plugin_id: String,
    
    pub permissions: Vec<String>,
    pub services: Arc<dyn HostServices>,
    
    pub progress: Duration,
    
    pub deadline: Duration,
    
    pub mem_bytes: usize,
}

impl SupervisorParams {
    pub fn new(
        exe: impl Into<PathBuf>,
        plugin_id: impl Into<String>,
        services: Arc<dyn HostServices>,
    ) -> Self {
        Self {
            exe: exe.into(),
            plugin_id: plugin_id.into(),
            permissions: Vec::new(),
            services,
            progress: Duration::from_millis(PROGRESS_TIMEOUT_MS),
            deadline: Duration::from_millis(DEADLINE_MS),
            mem_bytes: DEFAULT_MEM_BYTES,
        }
    }

    pub fn with_permissions(mut self, permissions: Vec<String>) -> Self {
        self.permissions = permissions;
        self
    }

    pub fn with_mem_bytes(mut self, bytes: usize) -> Self {
        self.mem_bytes = bytes;
        self
    }
}


pub struct Supervisor {
    plugin_id: String,
    child: Child,
    job: Job,
    
    
    to_child: Option<mpsc::Sender<ToChild>>,
    writer: Option<JoinHandle<()>>,
    rx: Receiver<Result<ToHost, PluginError>>,
    reader: Option<JoinHandle<()>>,
    next_id: u32,
    progress: Duration,
    deadline: Duration,
    permissions: Vec<String>,
    services: Arc<dyn HostServices>,
}

impl Supervisor {
    
    
    
    
    pub fn start(params: SupervisorParams, source: &str) -> Result<Self, PluginError> {
        let job = Job::new().map_err(|e| PluginError::crashed(format!("Job Object: {e}")))?;
        job.set_limits(0, params.mem_bytes)
            .map_err(|e| PluginError::crashed(format!("лимиты Job: {e}")))?;

        let mut command = Command::new(&params.exe);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = command
            .spawn()
            .map_err(|e| PluginError::crashed(format!("spawn {}: {e}", params.exe.display())))?;
        job.assign_pid(child.id())
            .map_err(|e| PluginError::crashed(format!("assign Job: {e}")))?;

        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let (to_child_tx, to_child_rx) = mpsc::channel::<ToChild>();
        let writer = stdin.map(|mut stdin| {
            thread::spawn(move || {
                while let Ok(msg) = to_child_rx.recv() {
                    if envelope::write_to_child(&mut stdin, &msg).is_err() {
                        break;
                    }
                }
            })
        });

        let (tx, rx) = mpsc::channel::<Result<ToHost, PluginError>>();
        let reader = stdout.map(|out| {
            thread::spawn(move || {
                let mut out = out;
                loop {
                    match envelope::read_to_host(&mut out) {
                        Ok(Ok(msg)) => {
                            if tx.send(Ok(msg)).is_err() {
                                break;
                            }
                        }
                        Ok(Err(protocol)) => {
                            let _ = tx.send(Err(protocol));
                            break;
                        }
                        Err(_) => break,
                    }
                }
            })
        });

        let mut supervisor = Self {
            plugin_id: params.plugin_id,
            child,
            job,
            to_child: Some(to_child_tx),
            writer,
            rx,
            reader,
            next_id: 1,
            progress: params.progress,
            deadline: params.deadline,
            permissions: params.permissions,
            services: params.services,
        };

        supervisor.invoke(
            "load",
            serde_json::json!({ "source": source, "plugin_id": supervisor.plugin_id.clone() }),
        )?;
        Ok(supervisor)
    }

    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    
    
    
    
    
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    
    pub fn activate(&mut self) -> Result<Value, PluginError> {
        self.invoke("activate", Value::Null)
    }

    
    pub fn deactivate(&mut self) -> Result<Value, PluginError> {
        self.invoke("deactivate", Value::Null)
    }

    
    pub fn dispatch_event(&mut self, name: &str, payload: Value) -> Result<Value, PluginError> {
        self.invoke(
            "event",
            serde_json::json!({ "name": name, "payload": payload }),
        )
    }

    
    pub fn invoke(&mut self, method: &str, args: Value) -> Result<Value, PluginError> {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.write_to_child(&ToChild::Invoke {
            id,
            method: method.to_string(),
            args,
        })?;
        self.await_result(id)
    }

    fn write_to_child(&mut self, msg: &ToChild) -> Result<(), PluginError> {
        self.to_child
            .as_ref()
            .ok_or_else(|| PluginError::crashed("канал child недоступен"))?
            .send(msg.clone())
            .map_err(|_| PluginError::crashed("child недоступен (writer завершён)"))
    }

    
    
    
    fn await_result(&mut self, id: u32) -> Result<Value, PluginError> {
        let mut started = Instant::now();
        let mut last_progress = started;

        loop {
            let now = Instant::now();
            if now.duration_since(started) >= self.deadline {
                let exited = self.child_exited();
                self.terminate();
                return Err(Self::expired(
                    exited,
                    &self.plugin_id,
                    self.deadline,
                    "дедлайн invocation",
                ));
            }
            if now.duration_since(last_progress) >= self.progress {
                let exited = self.child_exited();
                self.terminate();
                return Err(Self::expired(
                    exited,
                    &self.plugin_id,
                    self.progress,
                    "нет прогресса плагина",
                ));
            }

            let mut wait = self.progress.saturating_sub(last_progress.elapsed());
            wait = wait.min(self.deadline.saturating_sub(started.elapsed()));

            match self.rx.recv_timeout(wait) {
                Ok(Ok(ToHost::HostCall {
                    id: hid,
                    method,
                    args,
                })) => {
                    let call_started = Instant::now();
                    let result = self.serve_host_call(&method, &args);
                    started += call_started.elapsed();
                    last_progress = Instant::now();
                    self.write_to_child(&ToChild::Reply { id: hid, result })?;
                }
                Ok(Ok(ToHost::Event { kind, payload })) => {
                    last_progress = Instant::now();
                    if payload.get("id").and_then(Value::as_u64) == Some(u64::from(id)) {
                        match kind.as_str() {
                            "done" => {
                                return Ok(payload.get("result").cloned().unwrap_or(Value::Null))
                            }
                            "error" => return Err(decode_error(&payload)),
                            _ => {}
                        }
                    }
                }
                Ok(Ok(ToHost::Log { level, message })) => {
                    let args = serde_json::json!({ "level": level, "message": message });
                    let _ = self
                        .services
                        .handle("log", with_plugin_id(&args, &self.plugin_id));
                    last_progress = Instant::now();
                }
                Ok(Err(protocol)) => {
                    self.terminate();
                    return Err(protocol);
                }
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(PluginError::crashed(format!(
                        "{}: child завершился",
                        self.plugin_id
                    )));
                }
            }
        }
    }

    
    
    
    
    fn serve_host_call(&self, method: &str, args: &Value) -> Result<Value, PluginError> {
        check_permission(&self.permissions, method)?;
        self.services
            .handle(method, with_plugin_id(args, &self.plugin_id))
    }

    
    pub fn terminate(&mut self) {
        self.job.terminate(1);
        let _ = self.child.kill();
    }

    
    
    fn child_exited(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(Some(_)))
    }

    
    fn expired(exited: bool, plugin_id: &str, budget: Duration, what: &str) -> PluginError {
        if exited {
            PluginError::crashed(format!("{plugin_id}: child завершился"))
        } else {
            PluginError::timeout(format!("{what} ({budget:?}) исчерпан"))
        }
    }

    
    
    
    
    pub fn resolve_child_exe() -> Option<PathBuf> {
        if let Ok(path) = std::env::var("DARMARK_PLUGIN_HOST_EXE") {
            let path = PathBuf::from(path);
            if path.is_file() {
                return Some(path);
            }
        }
        let current = std::env::current_exe().ok()?;
        let dir = current.parent()?;
        let candidates = [
            dir.join(CHILD_EXE_NAME),
            dir.parent()
                .map(|p| p.join(CHILD_EXE_NAME))
                .unwrap_or_default(),
        ];
        candidates.into_iter().find(|p| p.is_file())
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        self.to_child.take();
        self.job.terminate(1);
        let _ = self.child.wait();
        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
