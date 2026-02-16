mod codec;
mod subprocess;

use std::collections::{HashMap, VecDeque};
use std::io::BufReader;
use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::json;

use launch_code::state::StateStore;

use crate::error::AppError;

#[derive(Debug, Default)]
pub(crate) struct DapRegistry {
    proxies: HashMap<String, Arc<DapProxy>>,
}

#[derive(Debug, Clone)]
pub(crate) struct AdoptSubprocessResult {
    pub child_session_id: String,
    pub host: String,
    pub port: u16,
    pub process_id: Option<u32>,
    pub source_event: serde_json::Value,
    pub bootstrap_responses: Vec<serde_json::Value>,
}

pub(crate) fn send_request_with_retry(
    store: &StateStore,
    registry: &Arc<Mutex<DapRegistry>>,
    session_id: &str,
    command: &str,
    arguments: serde_json::Value,
    timeout: Duration,
) -> Result<serde_json::Value, AppError> {
    let proxy = proxy_for_session(store, registry, session_id)?;
    let retry_arguments = arguments.clone();
    match proxy.send_request(command, arguments, timeout) {
        Ok(response) => Ok(response),
        Err(err) => {
            let should_retry = proxy.is_closed();
            drop_session(registry, session_id);
            if should_retry {
                let proxy = proxy_for_session(store, registry, session_id)?;
                match proxy.send_request(command, retry_arguments, timeout) {
                    Ok(response) => Ok(response),
                    Err(err) => {
                        drop_session(registry, session_id);
                        Err(err)
                    }
                }
            } else {
                Err(err)
            }
        }
    }
}

pub(crate) fn send_batch_with_retry(
    store: &StateStore,
    registry: &Arc<Mutex<DapRegistry>>,
    session_id: &str,
    requests: Vec<(String, serde_json::Value)>,
    timeout: Duration,
) -> Result<Vec<serde_json::Value>, AppError> {
    let proxy = proxy_for_session(store, registry, session_id)?;
    let retry_requests = requests.clone();
    match proxy.send_batch(requests, timeout) {
        Ok(responses) => Ok(responses),
        Err(err) => {
            let should_retry = proxy.is_closed();
            drop_session(registry, session_id);
            if should_retry {
                let proxy = proxy_for_session(store, registry, session_id)?;
                match proxy.send_batch(retry_requests, timeout) {
                    Ok(responses) => Ok(responses),
                    Err(err) => {
                        drop_session(registry, session_id);
                        Err(err)
                    }
                }
            } else {
                Err(err)
            }
        }
    }
}

pub(crate) fn adopt_debugpy_subprocess(
    store: &StateStore,
    registry: &Arc<Mutex<DapRegistry>>,
    parent_session_id: &str,
    event_timeout: Duration,
    max_events: usize,
    bootstrap_timeout: Duration,
    requested_child_session_id: Option<&str>,
) -> Result<AdoptSubprocessResult, AppError> {
    let proxy = proxy_for_session(store, registry, parent_session_id)?;
    let events = proxy.pop_events(max_events.max(1), event_timeout);

    let target = subprocess::latest_debugpy_attach_target(&events)?;

    let child_session_id = subprocess::register_subprocess_session(
        store,
        parent_session_id,
        &target,
        requested_child_session_id,
    )?;

    let bootstrap_requests = vec![
        (
            "initialize".to_string(),
            json!({
                "clientID": "launch-code",
                "adapterID": "python",
                "pathFormat": "path",
                "linesStartAt1": true,
                "columnsStartAt1": true
            }),
        ),
        ("attach".to_string(), target.attach_arguments.clone()),
        ("configurationDone".to_string(), json!({})),
    ];

    let bootstrap_responses = match send_batch_with_retry(
        store,
        registry,
        &child_session_id,
        bootstrap_requests,
        bootstrap_timeout,
    ) {
        Ok(responses) => responses,
        Err(err) => {
            let child_id = child_session_id.clone();
            let _ = store.update::<_, _, AppError>(|state| {
                state.sessions.remove(&child_id);
                Ok(())
            });
            return Err(err);
        }
    };

    Ok(AdoptSubprocessResult {
        child_session_id,
        host: target.host,
        port: target.port,
        process_id: target.process_id,
        source_event: target.source_event,
        bootstrap_responses,
    })
}

pub(crate) fn proxy_for_session(
    store: &StateStore,
    registry: &Arc<Mutex<DapRegistry>>,
    session_id: &str,
) -> Result<Arc<DapProxy>, AppError> {
    let state = store.load()?;
    let session = state
        .sessions
        .get(session_id)
        .ok_or_else(|| AppError::SessionNotFound(session_id.to_string()))?;
    let meta = session
        .debug_meta
        .as_ref()
        .ok_or_else(|| AppError::SessionMissingDebugMeta(session.id.clone()))?;
    let host = meta.host.clone();
    let port = meta.active_port;

    {
        let guard = registry
            .lock()
            .map_err(|_| AppError::Http("dap registry lock poisoned".to_string()))?;
        if let Some(proxy) = guard.proxies.get(session_id) {
            if proxy.matches(&host, port) {
                return Ok(Arc::clone(proxy));
            }
        }
    }

    let proxy = DapProxy::connect(host, port)?;
    let mut guard = registry
        .lock()
        .map_err(|_| AppError::Http("dap registry lock poisoned".to_string()))?;
    guard
        .proxies
        .insert(session_id.to_string(), Arc::clone(&proxy));
    Ok(proxy)
}

fn drop_session(registry: &Arc<Mutex<DapRegistry>>, session_id: &str) {
    if let Ok(mut guard) = registry.lock() {
        guard.proxies.remove(session_id);
    }
}

#[allow(dead_code)]
#[derive(Debug)]
pub(crate) enum DapProxy {
    Tcp(Arc<TcpDapProxy>),
    PythonAdapter(Arc<PythonAdapterDapProxy>),
}

impl DapProxy {
    pub(crate) fn connect(host: String, port: u16) -> Result<Arc<Self>, AppError> {
        let inner = TcpDapProxy::connect(host, port)?;
        Ok(Arc::new(Self::Tcp(inner)))
    }

    pub(crate) fn send_batch(
        &self,
        requests: Vec<(String, serde_json::Value)>,
        timeout: Duration,
    ) -> Result<Vec<serde_json::Value>, AppError> {
        match self {
            Self::Tcp(inner) => inner.send_batch(requests, timeout),
            Self::PythonAdapter(inner) => inner.send_batch(requests, timeout),
        }
    }

    pub(crate) fn matches(&self, host: &str, port: u16) -> bool {
        match self {
            Self::Tcp(inner) => inner.matches(host, port),
            Self::PythonAdapter(inner) => inner.matches(host, port),
        }
    }

    pub(crate) fn is_closed(&self) -> bool {
        match self {
            Self::Tcp(inner) => inner.is_closed(),
            Self::PythonAdapter(inner) => inner.is_closed(),
        }
    }

    pub(crate) fn send_request(
        &self,
        command: &str,
        arguments: serde_json::Value,
        timeout: Duration,
    ) -> Result<serde_json::Value, AppError> {
        match self {
            Self::Tcp(inner) => inner.send_request(command, arguments, timeout),
            Self::PythonAdapter(inner) => inner.send_request(command, arguments, timeout),
        }
    }

    pub(crate) fn pop_events(&self, max: usize, timeout: Duration) -> Vec<serde_json::Value> {
        match self {
            Self::Tcp(inner) => inner.pop_events(max, timeout),
            Self::PythonAdapter(inner) => inner.pop_events(max, timeout),
        }
    }
}

#[derive(Debug)]
struct DapProxyInner {
    waiters: HashMap<u64, mpsc::Sender<serde_json::Value>>,
    events: VecDeque<serde_json::Value>,
    closed: bool,
}

#[derive(Debug)]
pub(crate) struct TcpDapProxy {
    host: String,
    port: u16,
    writer: Mutex<TcpStream>,
    inner: Mutex<DapProxyInner>,
    events_cv: Condvar,
    next_seq: AtomicU64,
}

impl TcpDapProxy {
    fn connect(host: String, port: u16) -> Result<Arc<Self>, AppError> {
        let addr = format!("{host}:{port}");
        // Debug adapters (e.g. debugpy) might take a short moment to open the TCP port.
        // Retry briefly to avoid flaky "connection refused" errors right after launch.
        let deadline = Instant::now() + Duration::from_millis(1200);
        let stream = loop {
            match TcpStream::connect(&addr) {
                Ok(stream) => break stream,
                Err(err) => {
                    if err.kind() == std::io::ErrorKind::ConnectionRefused
                        && Instant::now() < deadline
                    {
                        thread::sleep(Duration::from_millis(50));
                        continue;
                    }
                    return Err(AppError::Dap(err.to_string()));
                }
            }
        };
        let _ = stream.set_nodelay(true);
        let writer = stream
            .try_clone()
            .map_err(|err| AppError::Dap(err.to_string()))?;

        let proxy = Arc::new(Self {
            host,
            port,
            writer: Mutex::new(writer),
            inner: Mutex::new(DapProxyInner {
                waiters: HashMap::new(),
                events: VecDeque::new(),
                closed: false,
            }),
            events_cv: Condvar::new(),
            next_seq: AtomicU64::new(1),
        });

        let proxy_clone = Arc::clone(&proxy);
        thread::spawn(move || dap_tcp_reader_loop(proxy_clone, stream));

        Ok(proxy)
    }

    fn matches(&self, host: &str, port: u16) -> bool {
        self.host == host && self.port == port
    }

    fn is_closed(&self) -> bool {
        self.inner.lock().map(|guard| guard.closed).unwrap_or(true)
    }

    fn send_request(
        &self,
        command: &str,
        arguments: serde_json::Value,
        timeout: Duration,
    ) -> Result<serde_json::Value, AppError> {
        let pending = self.start_request(command, arguments)?;
        pending.wait(self, timeout)
    }

    fn send_batch(
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

    fn pop_events(&self, max: usize, timeout: Duration) -> Vec<serde_json::Value> {
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

fn dap_tcp_reader_loop(proxy: Arc<TcpDapProxy>, stream: TcpStream) {
    let mut reader = BufReader::new(stream);
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
                break;
            }
        };

        match msg.get("type").and_then(|v| v.as_str()) {
            Some("event") => {
                let mut guard = proxy
                    .inner
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                guard.events.push_back(msg);
                while guard.events.len() > 1000 {
                    let _ = guard.events.pop_front();
                }
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
    fn spawn(python_bin: String, host: String, port: u16) -> Result<Arc<Self>, AppError> {
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
                waiters: HashMap::new(),
                events: VecDeque::new(),
                closed: false,
            }),
            events_cv: Condvar::new(),
            next_seq: AtomicU64::new(1),
        });

        let proxy_clone = Arc::clone(&proxy);
        thread::spawn(move || dap_stdio_reader_loop(proxy_clone, stdout));

        Ok(proxy)
    }

    fn matches(&self, host: &str, port: u16) -> bool {
        self.host == host && self.port == port
    }

    fn is_closed(&self) -> bool {
        self.inner.lock().map(|guard| guard.closed).unwrap_or(true)
    }

    fn send_request(
        &self,
        command: &str,
        arguments: serde_json::Value,
        timeout: Duration,
    ) -> Result<serde_json::Value, AppError> {
        let pending = self.start_request(command, arguments)?;
        pending.wait(self, timeout)
    }

    fn send_batch(
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

    fn pop_events(&self, max: usize, timeout: Duration) -> Vec<serde_json::Value> {
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

#[derive(Debug)]
struct DapPendingRequest {
    seq: u64,
    rx: mpsc::Receiver<serde_json::Value>,
}

impl DapPendingRequest {
    fn wait<T: DapWaiter>(
        self,
        waiter: &T,
        timeout: Duration,
    ) -> Result<serde_json::Value, AppError> {
        match self.rx.recv_timeout(timeout) {
            Ok(msg) => Ok(msg),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                waiter.remove_waiter(self.seq)?;
                Err(AppError::Dap("timeout waiting for response".to_string()))
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(AppError::Dap(
                "dap channel disconnected while waiting for response".to_string(),
            )),
        }
    }

    fn cancel<T: DapWaiter>(&self, waiter: &T) -> Result<(), AppError> {
        waiter.remove_waiter(self.seq)
    }
}

trait DapWaiter {
    fn remove_waiter(&self, seq: u64) -> Result<(), AppError>;
}

impl DapWaiter for TcpDapProxy {
    fn remove_waiter(&self, seq: u64) -> Result<(), AppError> {
        TcpDapProxy::remove_waiter(self, seq)
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
                guard.events.push_back(msg);
                while guard.events.len() > 1000 {
                    let _ = guard.events.pop_front();
                }
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
