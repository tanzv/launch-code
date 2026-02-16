use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProcessError {
    #[error("command cannot be empty")]
    EmptyCommand,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to send signal {signal} to pid {pid}: {source}")]
    Signal {
        pid: u32,
        signal: i32,
        source: std::io::Error,
    },
    #[error("process stop timed out for pid {pid} after {grace_timeout_ms} ms")]
    StopTimeout { pid: u32, grace_timeout_ms: u64 },
    #[error("task command failed with exit code {exit_code:?}: {command}")]
    TaskCommandFailed {
        command: String,
        exit_code: Option<i32>,
    },
    #[error("unsupported operation on this platform: {0}")]
    UnsupportedOperation(&'static str),
}

pub fn spawn_process(
    command: &[String],
    cwd: &Path,
    env: &BTreeMap<String, String>,
    log_path: &Path,
) -> Result<u32, ProcessError> {
    if command.is_empty() {
        return Err(ProcessError::EmptyCommand);
    }

    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let stdout_log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    let stderr_log = stdout_log.try_clone()?;

    let mut cmd = Command::new(&command[0]);
    cmd.args(&command[1..])
        .current_dir(cwd)
        .envs(env.iter())
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout_log))
        .stderr(Stdio::from(stderr_log));

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }

    let child = cmd.spawn()?;
    Ok(child.id())
}

pub fn is_process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        let result = unsafe { libc::kill(pid as i32, 0) };
        if result == 0 {
            return true;
        }

        let code = std::io::Error::last_os_error().raw_os_error();
        code == Some(libc::EPERM)
    }

    #[cfg(windows)]
    {
        let filter = format!("PID eq {pid}");
        if let Ok(output) = Command::new("tasklist").arg("/FI").arg(filter).output() {
            if let Ok(stdout) = String::from_utf8(output.stdout) {
                return stdout.contains(&pid.to_string());
            }
        }
        false
    }

    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

pub fn suspend_process(pid: u32) -> Result<(), ProcessError> {
    #[cfg(unix)]
    {
        send_signal_group(pid, libc::SIGSTOP)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        Err(ProcessError::UnsupportedOperation("suspend"))
    }
}

pub fn resume_process(pid: u32) -> Result<(), ProcessError> {
    #[cfg(unix)]
    {
        send_signal_group(pid, libc::SIGCONT)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        Err(ProcessError::UnsupportedOperation("resume"))
    }
}

pub fn stop_process(pid: u32) -> Result<(), ProcessError> {
    stop_process_with_options(pid, true, Duration::from_millis(150))
}

pub fn stop_process_with_options(
    pid: u32,
    force_if_running: bool,
    grace_timeout: Duration,
) -> Result<(), ProcessError> {
    #[cfg(unix)]
    {
        send_signal_group(pid, libc::SIGTERM)?;
        let deadline = Instant::now() + grace_timeout;
        while Instant::now() < deadline {
            if !is_process_alive(pid) {
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }

        if is_process_alive(pid) {
            if force_if_running {
                send_signal_group(pid, libc::SIGKILL)?;
                let force_deadline = Instant::now() + Duration::from_secs(2);
                while Instant::now() < force_deadline {
                    if !is_process_alive(pid) {
                        break;
                    }
                    thread::sleep(Duration::from_millis(50));
                }
            } else {
                return Err(ProcessError::StopTimeout {
                    pid,
                    grace_timeout_ms: u64::try_from(grace_timeout.as_millis()).unwrap_or(u64::MAX),
                });
            }
        }

        let mut status = 0;
        let _ = unsafe { libc::waitpid(pid as i32, &mut status, libc::WNOHANG) };
        Ok(())
    }

    #[cfg(windows)]
    {
        if !is_process_alive(pid) {
            return Ok(());
        }
        let mut command = Command::new("taskkill");
        command.arg("/PID").arg(pid.to_string()).arg("/T");
        if force_if_running {
            command.arg("/F");
        }
        let status = command.status()?;
        if status.success() {
            return Ok(());
        }
        if !force_if_running {
            return Err(ProcessError::StopTimeout {
                pid,
                grace_timeout_ms: u64::try_from(grace_timeout.as_millis()).unwrap_or(u64::MAX),
            });
        }
        return Err(ProcessError::TaskCommandFailed {
            command: if force_if_running {
                format!("taskkill /PID {} /T /F", pid)
            } else {
                format!("taskkill /PID {} /T", pid)
            },
            exit_code: status.code(),
        });
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = (pid, force_if_running, grace_timeout);
        Err(ProcessError::UnsupportedOperation("stop"))
    }
}

#[cfg(unix)]
fn send_signal(pid: u32, signal: i32) -> Result<(), ProcessError> {
    let rc = unsafe { libc::kill(pid as i32, signal) };
    if rc == 0 {
        return Ok(());
    }

    let source = std::io::Error::last_os_error();
    if source.raw_os_error() == Some(libc::ESRCH) {
        return Ok(());
    }

    Err(ProcessError::Signal {
        pid,
        signal,
        source,
    })
}

#[cfg(unix)]
fn send_signal_group(pid: u32, signal: i32) -> Result<(), ProcessError> {
    let rc = unsafe { libc::kill(-(pid as i32), signal) };
    if rc == 0 {
        return Ok(());
    }

    let source = std::io::Error::last_os_error();
    if source.raw_os_error() == Some(libc::ESRCH) {
        return Ok(());
    }

    // Fallback to single-process signaling when process-group signaling is not available.
    if source.raw_os_error() == Some(libc::EPERM) {
        return send_signal(pid, signal);
    }

    Err(ProcessError::Signal {
        pid,
        signal,
        source,
    })
}

pub fn run_shell_task(
    task_command: &str,
    cwd: &Path,
    env: &BTreeMap<String, String>,
    log_path: &Path,
) -> Result<(), ProcessError> {
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let stdout_log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    let stderr_log = stdout_log.try_clone()?;

    let mut cmd = task_command_command(task_command);
    cmd.current_dir(cwd)
        .envs(env.iter())
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout_log))
        .stderr(Stdio::from(stderr_log));

    let status = cmd.status()?;
    if status.success() {
        return Ok(());
    }

    Err(ProcessError::TaskCommandFailed {
        command: task_command.to_string(),
        exit_code: status.code(),
    })
}

#[cfg(unix)]
fn task_command_command(task_command: &str) -> Command {
    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(task_command);
    cmd
}

#[cfg(windows)]
fn task_command_command(task_command: &str) -> Command {
    let mut cmd = Command::new("cmd");
    cmd.arg("/C").arg(task_command);
    cmd
}
