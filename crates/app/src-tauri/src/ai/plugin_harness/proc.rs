use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::error::{CommandError, ErrorCode};


pub(super) const VALIDATION_TIMEOUT: Duration = Duration::from_secs(15);


pub(super) const CLI_ENV: &str = "DARMARK_PLUGIN_CLI";
pub(super) const CLI_EXE: &str = "darmark-plugin.exe";

pub(super) const HOST_ENV: &str = "DARMARK_PLUGIN_HOST_EXE";
pub(super) const HOST_EXE: &str = "darmark-plugin-host.exe";



pub(super) fn resolve_sidecar(env: &str, name: &str) -> Option<PathBuf> {
    if let Ok(path) = std::env::var(env) {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    let current = std::env::current_exe().ok()?;
    let dir = current.parent()?;
    let candidates = [
        dir.join(name),
        dir.parent().map(|p| p.join(name)).unwrap_or_default(),
    ];
    candidates.into_iter().find(|p| p.is_file())
}






pub(super) fn run_with_timeout(
    mut command: Command,
    timeout: Duration,
) -> Result<ProcessOutput, CommandError> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command.spawn().map_err(process_io_error)?;
    let stdout_thread = std::thread::spawn({
        let pipe = child.stdout.take();
        move || read_pipe(pipe)
    });
    let stderr_thread = std::thread::spawn({
        let pipe = child.stderr.take();
        move || read_pipe(pipe)
    });

    let started = Instant::now();
    let outcome = loop {
        match child.try_wait() {
            Ok(Some(status)) => break WaitOutcome::Exited(status),
            Ok(None) => {
                if started.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    break WaitOutcome::TimedOut;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                break WaitOutcome::Failed(process_io_error(e));
            }
        }
    };
    let stdout = stdout_thread.join().unwrap_or_default();
    let stderr = stderr_thread.join().unwrap_or_default();
    match outcome {
        WaitOutcome::Exited(status) => Ok(ProcessOutput {
            success: status.success(),
            stdout,
            stderr,
        }),
        WaitOutcome::TimedOut => Err(CommandError::new(
            ErrorCode::AiPluginInvalid,
            "таймаут проверки плагина",
        )),
        WaitOutcome::Failed(error) => Err(error),
    }
}


enum WaitOutcome {
    Exited(std::process::ExitStatus),
    TimedOut,
    Failed(CommandError),
}


pub(super) struct ProcessOutput {
    pub(super) success: bool,
    pub(super) stdout: String,
    pub(super) stderr: String,
}

fn read_pipe(pipe: Option<impl std::io::Read>) -> String {
    let mut buffer = String::new();
    if let Some(mut pipe) = pipe {
        let _ = pipe.read_to_string(&mut buffer);
    }
    buffer
}

fn process_io_error(e: std::io::Error) -> CommandError {
    CommandError::new(ErrorCode::AiIo, format!("процесс проверки плагина: {e}"))
}


pub(super) fn tail(text: &str) -> String {
    const MAX: usize = 400;
    let trimmed = text.trim();
    if trimmed.chars().count() <= MAX {
        return trimmed.to_string();
    }
    let mut tail: String = trimmed.chars().rev().take(MAX).collect();
    tail = tail.chars().rev().collect();
    format!("…{tail}")
}
