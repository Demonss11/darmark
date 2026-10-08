//! `Supervisor` — надзор за одним child-процессом плагина (Фаза 2 TZ-H2).
//!
//! Отвечает за транспорт stdio, Job Object (лимит памяти + `KILL_ON_JOB_CLOSE`), два таймера
//! watchdog (прогресс + абсолютный дедлайн invocation, §4.3), обслуживание host-call'ов с
//! проверкой permissions (§4.4). GUI-процесс при этом остаётся на `panic = "abort"` и не
//! линкует `mlua`: плагин — внешний exe.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
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
    /// Канал в writer-поток. Отправка неблокирующая: даже если child перестал читать stdin,
    /// основной поток не виснет, а watchdog продолжает тикать (F27, §3.2 ревью).
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

        // Writer-поток владеет stdin: основной поток только кладёт кадры в канал. Если child
        // перестанет читать, заблокируется writer, а не watchdog-цикл; снятие child (Job)
        // разорвёт пайп и разблокирует writer.
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
        self.to_child
            .as_ref()
            .ok_or_else(|| PluginError::crashed("канал child недоступен"))?
            .send(msg.clone())
            .map_err(|_| PluginError::crashed("child недоступен (writer завершён)"))
    }

    /// Ждёт `Event{done|error, id}` для invocation `id`, попутно обслуживая host-call'ы.
    ///
    /// Два таймера: прогресс сбрасывается каждым кадром child'а, дедлайн — нет (ловит `chatty`).
    fn await_result(&mut self, id: u32) -> Result<Value, PluginError> {
        let mut started = Instant::now();
        let mut last_progress = started;

        loop {
            let now = Instant::now();
            if now.duration_since(started) >= self.deadline {
                // Процесс мог уже умереть (краш), а канал закрыться позже — не выдаём крах
                // за таймаут (§4.5 ревью).
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
                    // Обслуживание host-call'а — время хоста, а не исполнения плагина:
                    // `export_html` открывает нативный диалог и может блокироваться минутами.
                    // Исключаем это время из абсолютного дедлайна, иначе после закрытия
                    // диалога child был бы снят как «chatty» (F35 применим к циклу плагина,
                    // а не к диалогу пользователя). Прогресс-таймер сбрасываем явно.
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
                    // Лог плагина (host.log/print) иначе терялся бы в GUI — выводим в stderr хоста.
                    eprintln!("[plugin {level}] {message}");
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
    ///
    /// В object-args инжектится `_plugin_id` (`self.plugin_id`): хост не доверяет
    /// идентификатору из плагина (плагин не может подменить владельца view/настроек).
    fn serve_host_call(&self, method: &str, args: &Value) -> Result<Value, PluginError> {
        check_permission(&self.permissions, method)?;
        self.services
            .handle(method, with_plugin_id(args, &self.plugin_id))
    }

    /// Принудительно завершает child (Job Object + kill процесса).
    pub fn terminate(&mut self) {
        self.job.terminate(1);
        let _ = self.child.kill();
    }

    /// Завершился ли child — по объекту процесса, а не по каналу stdio (канал может закрыться
    /// позже, особенно на Windows после `abort`).
    fn child_exited(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(Some(_)))
    }

    /// Исход исчерпания бюджета: процесс уже мёртв → `crashed`, иначе `timeout`.
    fn expired(exited: bool, plugin_id: &str, budget: Duration, what: &str) -> PluginError {
        if exited {
            PluginError::crashed(format!("{plugin_id}: child завершился"))
        } else {
            PluginError::timeout(format!("{what} ({budget:?}) исчерпан"))
        }
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

/// Возвращает копию object-args с инжектированным `_plugin_id`.
///
/// Хост не доверяет идентификатору из плагина: `serve_host_call` подставляет
/// `self.plugin_id` перед передачей в [`HostServices`]. Не-object args остаются
/// как есть (host-call'ы с object-аргументами — контракт H2).
fn with_plugin_id(args: &Value, plugin_id: &str) -> Value {
    let mut args = args.clone();
    if let Some(object) = args.as_object_mut() {
        object.insert(
            "_plugin_id".to_string(),
            Value::String(plugin_id.to_string()),
        );
    }
    args
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
        "set_view_content" => Some("view:modify"),
        "show_message" => Some("ui:statusbar"),
        // Экспорт HTML идёт через нативный диалог хоста: согласие даёт сам диалог,
        // permission (в т.ч. `filesystem:write`) не требуется (§5 Фаза 5 / §6.3).
        "export_html" => None,
        _ => None,
    }
}

/// Проверяет, что у плагина есть разрешение на host-call. Отказ — значение
/// [`PluginError::permission_denied`], а не паника.
pub fn check_permission(permissions: &[String], method: &str) -> Result<(), PluginError> {
    if let Some(required) = required_permission(method) {
        if !permissions.iter().any(|p| p == required) {
            return Err(PluginError::permission_denied(required));
        }
    }
    Ok(())
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        // Закрываем канал в writer, чтобы поток завершился; снятие Job разблокирует возможную
        // зависшую запись в пайп.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_denied_for_write_without_grant() {
        let err = check_permission(&["document:read".to_string()], "apply_edit").unwrap_err();
        assert_eq!(err.code, "permission_denied");
        assert_eq!(err.permission.as_deref(), Some("document:write"));
    }

    #[test]
    fn granted_permission_allows_call() {
        assert!(check_permission(&["document:write".to_string()], "apply_edit").is_ok());
        // host-call без требуемого permission (настройки) проходит без проверки.
        assert!(check_permission(&[], "get_setting").is_ok());
    }

    #[test]
    fn read_calls_require_document_read() {
        let err = check_permission(&[], "get_document_range").unwrap_err();
        assert_eq!(err.permission.as_deref(), Some("document:read"));
    }

    #[test]
    fn set_view_content_requires_view_modify() {
        let err = check_permission(&["view:create".to_string()], "set_view_content").unwrap_err();
        assert_eq!(err.code, "permission_denied");
        assert_eq!(err.permission.as_deref(), Some("view:modify"));
        assert!(check_permission(&["view:modify".to_string()], "set_view_content").is_ok());
    }

    #[test]
    fn export_html_requires_no_permission() {
        assert_eq!(required_permission("export_html"), None);
        assert!(
            check_permission(&[], "export_html").is_ok(),
            "нативный диалог — согласие пользователя, permission не нужен"
        );
    }

    #[test]
    fn show_message_requires_statusbar() {
        assert_eq!(required_permission("show_message"), Some("ui:statusbar"));
        let err = check_permission(&[], "show_message").unwrap_err();
        assert_eq!(err.code, "permission_denied");
        assert_eq!(err.permission.as_deref(), Some("ui:statusbar"));
        assert!(check_permission(&["ui:statusbar".to_string()], "show_message").is_ok());
    }

    #[test]
    fn plugin_id_is_injected_into_host_call_args() {
        let args = serde_json::json!({ "view_id": "p:main", "html": "<b>x</b>" });
        let injected = with_plugin_id(&args, "p");
        assert_eq!(injected["_plugin_id"], "p");
        assert_eq!(injected["view_id"], "p:main");
        // Исходные аргументы не мутируются.
        assert!(args.get("_plugin_id").is_none());
    }

    #[test]
    fn plugin_id_injection_keeps_non_object_args() {
        let injected = with_plugin_id(&Value::Null, "p");
        assert_eq!(injected, Value::Null);
    }
}
