//! `Supervisor` — надзор за одним child-процессом плагина (Фаза 2 TZ-H2).
//!
//! Отвечает за транспорт stdio, Job Object (лимит памяти + `KILL_ON_JOB_CLOSE`), два таймера
//! watchdog (прогресс + абсолютный дедлайн invocation, §4.3), обслуживание host-call'ов с
//! проверкой permissions (§4.4). GUI-процесс при этом остаётся на `panic = "abort"` и не
//! линкует `mlua`: плагин — внешний exe.

use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, TryRecvError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use plugin_proto::envelope::{self, PluginError, ToChild, ToHost};
use plugin_proto::job::Job;
use plugin_proto::limits::{DEADLINE_MS, PROGRESS_TIMEOUT_MS};
use serde_json::Value;

use super::HostServices;

/// Флаг Win32: без консольного окна у child (TZ §2: «subsystem console, но с CREATE_NO_WINDOW»).
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Имя child-бинарника рядом с `darmark.exe`.
pub const CHILD_EXE_NAME: &str = "darmark-plugin-host.exe";

/// Результат неблокирующего чтения события child'а (см. [`Supervisor::poll_event`]).
#[derive(Debug)]
pub enum EventPoll {
    /// Пришёл кадр.
    Message(std::result::Result<ToHost, PluginError>),
    /// Кадров нет.
    Empty,
    /// Поток child'а закрыт — процесс умер.
    Closed,
}

/// Параметры запуска child-процесса.
pub struct SupervisorParams {
    /// Путь к `darmark-plugin-host.exe`.
    pub exe: PathBuf,
    pub plugin_id: String,
    /// Разрешения манифеста (Фаза 3 наполнит из `plugin.json`; здесь — уже готовый список).
    pub permissions: Vec<String>,
    pub services: Arc<dyn HostServices>,
    /// Прогресс-таймаут invocation (§4.3).
    pub progress: Duration,
    /// Абсолютный дедлайн invocation (§4.3).
    pub deadline: Duration,
    /// Лимит committed-памяти child'а (0 — без лимита).
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
            mem_bytes: 64 * 1024 * 1024,
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

/// Надзор за одним процессом плагина.
pub struct Supervisor {
    plugin_id: String,
    child: Child,
    job: Job,
    stdin: Option<ChildStdin>,
    rx: Receiver<Result<ToHost, PluginError>>,
    reader: Option<JoinHandle<()>>,
    next_id: u32,
    progress: Duration,
    deadline: Duration,
    permissions: Vec<String>,
    services: Arc<dyn HostServices>,
}

impl Supervisor {
    /// Спавнит child, назначает Job и загружает исходник (`load`), но **не** активирует.
    ///
    /// Активация — отдельный шаг ([`Supervisor::activate`]): хост успевает подписаться на
    /// события/команды до `on_activate` (Фаза 4).
    pub fn start(params: SupervisorParams, source: &str) -> Result<Self, PluginError> {
        let job = Job::new().map_err(|e| PluginError::crashed(format!("Job Object: {e}")))?;
        job.set_limits(0, params.mem_bytes)
            .map_err(|e| PluginError::crashed(format!("лимиты Job: {e}")))?;

        let mut command = Command::new(&params.exe);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
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
            stdin,
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

    /// Вызывает `on_activate` плагина.
    pub fn activate(&mut self) -> Result<Value, PluginError> {
        self.invoke("activate", Value::Null)
    }

    /// Вызывает `on_deactivate` плагина (best-effort перед остановкой).
    pub fn deactivate(&mut self) -> Result<Value, PluginError> {
        self.invoke("deactivate", Value::Null)
    }

    /// Доставляет событие шины плагину (`event`).
    pub fn dispatch_event(&mut self, name: &str, payload: Value) -> Result<Value, PluginError> {
        self.invoke(
            "event",
            serde_json::json!({ "name": name, "payload": payload }),
        )
    }

    /// Вызывает произвольный метод плагина с прогресс-таймаутом и дедлайном.
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
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| PluginError::crashed("stdin child недоступен"))?;
        envelope::write_to_child(stdin, msg)
            .map_err(|e| PluginError::crashed(format!("stdin child: {e}")))
    }

    /// Ждёт `Event{done|error, id}` для invocation `id`, попутно обслуживая host-call'ы.
    ///
    /// Два таймера: прогресс сбрасывается каждым кадром child'а, дедлайн — нет (ловит `chatty`).
    fn await_result(&mut self, id: u32) -> Result<Value, PluginError> {
        let started = Instant::now();
        let mut last_progress = started;

        loop {
            let now = Instant::now();
            if now.duration_since(started) >= self.deadline {
                self.terminate();
                return Err(PluginError::timeout(format!(
                    "дедлайн invocation ({:?}) исчерпан",
                    self.deadline
                )));
            }
            if now.duration_since(last_progress) >= self.progress {
                self.terminate();
                return Err(PluginError::timeout(format!(
                    "нет прогресса плагина ({:?})",
                    self.progress
                )));
            }

            let mut wait = self.progress.saturating_sub(last_progress.elapsed());
            wait = wait.min(self.deadline.saturating_sub(started.elapsed()));

            match self.rx.recv_timeout(wait) {
                Ok(Ok(ToHost::HostCall {
                    id: hid,
                    method,
                    args,
                })) => {
                    last_progress = Instant::now();
                    let result = self.serve_host_call(&method, &args);
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
                Ok(Ok(ToHost::Log { .. })) => {
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

    /// Публикация события плагином (используется шиной Фазы 4): читает уже пришедшие события
    /// child'а без ожидания. [`EventPoll::Closed`] сигналит о смерти child — его нельзя спутать
    /// с «кадров нет» (`Empty`), иначе шина не заметит падение (§4.4 ревью).
    pub fn poll_event(&self) -> EventPoll {
        match self.rx.try_recv() {
            Ok(message) => EventPoll::Message(message),
            Err(TryRecvError::Empty) => EventPoll::Empty,
            Err(TryRecvError::Disconnected) => EventPoll::Closed,
        }
    }

    /// Проверяет permission и исполняет host-call.
    fn serve_host_call(&self, method: &str, args: &Value) -> Result<Value, PluginError> {
        if let Some(required) = required_permission(method) {
            if !self.permissions.iter().any(|p| p == required) {
                return Err(PluginError::permission_denied(required));
            }
        }
        self.services.handle(method, args.clone())
    }

    /// Принудительно завершает child (Job Object + kill процесса).
    pub fn terminate(&mut self) {
        self.job.terminate(1);
        let _ = self.child.kill();
    }

    /// Путной путь child-бинарника рядом с текущим exe (продукт), с override для dev.
    ///
    /// Порядок: env `DARMARK_PLUGIN_HOST_EXE` → рядом с `darmark.exe` → соседний каталог
    /// (тесты: `target/debug/deps` ↔ `target/debug`).
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

/// Читает `PluginError` из `payload["error"]`; при сбое разбора — `protocol`.
fn decode_error(payload: &Value) -> PluginError {
    match payload.get("error") {
        Some(value) => serde_json::from_value::<PluginError>(value.clone())
            .unwrap_or_else(|e| PluginError::protocol(format!("битый error-payload: {e}"))),
        None => PluginError::protocol("error-событие без поля error"),
    }
}

/// Разрешение, необходимое host-call'у (§4.4). `None` — вызов без permission.
pub fn required_permission(method: &str) -> Option<&'static str> {
    match method {
        "get_document_len"
        | "get_document_range"
        | "get_document_version"
        | "get_document_text" => Some("document:read"),
        "apply_edit" => Some("document:write"),
        "show_message" => Some("ui:statusbar"),
        _ => None,
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        self.job.terminate(1);
        self.stdin.take();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
