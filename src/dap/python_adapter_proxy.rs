use std::io::BufReader;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::json;

use crate::error::AppError;

use super::codec;
use super::shared::{DapPendingRequest, DapProxyInner, DapWaiter, push_event};

#[allow(dead_code)]
#[derive(Debug)]
pub(crate) struct PythonAdapterDapProxy {
    host: String,
    port: u16,
    python_bin: String,
    child: Mutex<std::process::Child>,
    writer: Mutex<std::process::ChildStdin>,
    inner: Mutex<DapProxyInner>,
    events_cv: Condvar,
    next_seq: AtomicU64,
}

#[allow(dead_code)]
impl PythonAdapterDapProxy {
    pub(super) fn spawn(
        python_bin: String,
        host: String,
        port: u16,
    ) -> Result<Arc<Self>, AppError> {
        let mut cmd = std::process::Command::new(&python_bin);
        cmd.args(["-m", "debugpy.adapter"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit());

        let mut child = cmd.spawn().map_err(|err| AppError::Dap(err.to_string()))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| AppError::Dap("failed to capture debug adapter stdin".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AppError::Dap("failed to capture debug adapter stdout".to_string()))?;

        let proxy = Arc::new(Self {
            host,
            port,
            python_bin,
            child: Mutex::new(child),
            writer: Mutex::new(stdin),
            inner: Mutex::new(DapProxyInner {
                waiters: std::collections::HashMap::new(),
                events: std::collections::VecDeque::new(),
                closed: false,
            }),
            events_cv: Condvar::new(),
            next_seq: AtomicU64::new(1),
        });

        let proxy_clone = Arc::clone(&proxy);
        thread::spawn(move || dap_stdio_reader_loop(proxy_clone, stdout));

        Ok(proxy)
    }

    pub(super) fn matches(&self, host: &str, port: u16) -> bool {
        self.host == host && self.port == port
    }

    pub(super) fn is_closed(&self) -> bool {
        self.inner.lock().map(|guard| guard.closed).unwrap_or(true)
    }

    pub(super) fn send_request(
        &self,
        command: &str,
        arguments: serde_json::Value,
        timeout: Duration,
    ) -> Result<serde_json::Value, AppError> {
        let pending = self.start_request(command, arguments)?;
        pending.wait(self, timeout)
    }

    pub(super) fn send_batch(
        &self,
        requests: Vec<(String, serde_json::Value)>,
        timeout: Duration,
    ) -> Result<Vec<serde_json::Value>, AppError> {
        if requests.is_empty() {
            return Ok(Vec::new());
        }

        let deadline = Instant::now() + timeout;
        let mut pending = Vec::with_capacity(requests.len());
        for (command, arguments) in requests {
            pending.push(self.start_request(&command, arguments)?);
        }

        let mut out = Vec::with_capacity(pending.len());
        for item in pending {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining == Duration::ZERO {
                item.cancel(self)?;
                return Err(AppError::Dap("timeout waiting for response".to_string()));
            }
            out.push(item.wait(self, remaining)?);
        }

        Ok(out)
    }

    pub(super) fn pop_events(&self, max: usize, timeout: Duration) -> Vec<serde_json::Value> {
        let mut guard = match self.inner.lock() {
            Ok(value) => value,
            Err(poisoned) => poisoned.into_inner(),
        };

        if guard.events.is_empty() && timeout > Duration::from_millis(0) && !guard.closed {
            let (new_guard, _) = match self.events_cv.wait_timeout(guard, timeout) {
                Ok(value) => value,
                Err(poisoned) => poisoned.into_inner(),
            };
            guard = new_guard;
        }

        let mut out = Vec::new();
        while out.len() < max {
            let Some(event) = guard.events.pop_front() else {
                break;
            };
            out.push(event);
        }
        out
    }

    fn write_message(&self, msg: &serde_json::Value) -> Result<(), AppError> {
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| AppError::Dap("dap writer lock poisoned".to_string()))?;
        codec::write_message(&mut *writer, msg)
    }

    fn start_request(
        &self,
        command: &str,
        arguments: serde_json::Value,
    ) -> Result<DapPendingRequest, AppError> {
        let seq = self.next_seq.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel::<serde_json::Value>();

        {
            let mut guard = self
                .inner
                .lock()
                .map_err(|_| AppError::Dap("dap state lock poisoned".to_string()))?;
            guard.waiters.insert(seq, tx);
        }

        let request = json!({
            "seq": seq,
            "type": "request",
            "command": command,
            "arguments": arguments,
        });
        if let Err(err) = self.write_message(&request) {
            let mut guard = self
                .inner
                .lock()
                .map_err(|_| AppError::Dap("dap state lock poisoned".to_string()))?;
            guard.waiters.remove(&seq);
            return Err(err);
        }

        Ok(DapPendingRequest { seq, rx })
    }

    fn remove_waiter(&self, seq: u64) -> Result<(), AppError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| AppError::Dap("dap state lock poisoned".to_string()))?;
        guard.waiters.remove(&seq);
        Ok(())
    }
}

impl DapWaiter for PythonAdapterDapProxy {
    fn remove_waiter(&self, seq: u64) -> Result<(), AppError> {
        PythonAdapterDapProxy::remove_waiter(self, seq)
    }
}

#[allow(dead_code)]
fn dap_stdio_reader_loop(proxy: Arc<PythonAdapterDapProxy>, stdout: std::process::ChildStdout) {
    let mut reader = BufReader::new(stdout);
    loop {
        let msg = match codec::read_message(&mut reader) {
            Ok(value) => value,
            Err(_) => {
                let mut guard = proxy
                    .inner
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                guard.closed = true;
                guard.waiters.clear();
                proxy.events_cv.notify_all();
                let _ = proxy
                    .child
                    .lock()
                    .ok()
                    .and_then(|mut child| child.kill().ok());
                break;
            }
        };

        match msg.get("type").and_then(|v| v.as_str()) {
            Some("event") => {
                let mut guard = proxy
                    .inner
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                push_event(&mut guard, msg);
                proxy.events_cv.notify_all();
            }
            Some("response") => {
                let request_seq = msg.get("request_seq").and_then(|v| v.as_u64()).unwrap_or(0);
                let tx = {
                    let mut guard = proxy
                        .inner
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    guard.waiters.remove(&request_seq)
                };
                if let Some(tx) = tx {
                    let _ = tx.send(msg);
                }
            }
            Some("request") => {
                let request_seq = msg.get("seq").and_then(|v| v.as_u64()).unwrap_or(0);
                let command = msg
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let seq = proxy.next_seq.fetch_add(1, Ordering::SeqCst);
                let response = json!({
                    "seq": seq,
                    "type": "response",
                    "request_seq": request_seq,
                    "success": false,
                    "command": command,
                    "message": "unsupported",
                });
                let _ = proxy.write_message(&response);
            }
            _ => {}
        }
    }
}
