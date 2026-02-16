use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::json;

use launch_code::model::{
    DebugConfig, DebugSessionMeta, SessionRecord, SessionStatus, unix_timestamp_secs,
};
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

#[derive(Debug, Clone)]
struct DebugpyAttachTarget {
    host: String,
    port: u16,
    process_id: Option<u32>,
    attach_arguments: serde_json::Value,
    source_event: serde_json::Value,
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

    let mut malformed_attach_error: Option<String> = None;
    let mut target: Option<DebugpyAttachTarget> = None;
    for event in events.iter().rev() {
        if event.get("event").and_then(|value| value.as_str()) != Some("debugpyAttach") {
            continue;
        }
        match parse_debugpy_attach_target(event) {
            Ok(value) => {
                target = Some(value);
                break;
            }
            Err(err) => {
                malformed_attach_error = Some(err.to_string());
            }
        }
    }

    let target = match target {
        Some(value) => value,
        None => {
            if let Some(message) = malformed_attach_error {
                return Err(AppError::Dap(format!(
                    "invalid debugpyAttach event: {message}"
                )));
            }
            return Err(AppError::Dap(
                "no debugpyAttach event available; poll events and retry".to_string(),
            ));
        }
    };

    let child_session_id = register_subprocess_session(
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

fn register_subprocess_session(
    store: &StateStore,
    parent_session_id: &str,
    target: &DebugpyAttachTarget,
    requested_child_session_id: Option<&str>,
) -> Result<String, AppError> {
    let parent_session_id = parent_session_id.to_string();
    let requested_child_session_id = requested_child_session_id.map(str::to_string);
    let target = target.clone();

    store.update::<_, _, AppError>(|state| {
        let parent = state
            .sessions
            .get(&parent_session_id)
            .cloned()
            .ok_or_else(|| AppError::SessionNotFound(parent_session_id.clone()))?;

        let mut child_spec = parent.spec.clone();
        if let Some(debug) = child_spec.debug.as_mut() {
            debug.host = target.host.clone();
            debug.port = target.port;
            debug.wait_for_client = false;
        } else {
            child_spec.debug = Some(DebugConfig {
                host: target.host.clone(),
                port: target.port,
                wait_for_client: false,
                subprocess: true,
            });
        }

        let base = match &requested_child_session_id {
            Some(value) => value.clone(),
            None => match target.process_id {
                Some(pid) => format!("{parent_session_id}-subprocess-{pid}"),
                None => format!("{parent_session_id}-subprocess-{}", target.port),
            },
        };
        let child_session_id =
            unique_session_id(state, &base, requested_child_session_id.is_none())
                .ok_or_else(|| AppError::Dap(format!("session id already exists: {base}")))?;
        let now = unix_timestamp_secs();

        state.sessions.insert(
            child_session_id.clone(),
            SessionRecord {
                id: child_session_id.clone(),
                spec: child_spec,
                status: SessionStatus::Running,
                pid: target.process_id,
                supervisor_pid: parent.pid.or(parent.supervisor_pid),
                log_path: parent.log_path.clone(),
                debug_meta: Some(DebugSessionMeta {
                    host: target.host.clone(),
                    requested_port: target.port,
                    active_port: target.port,
                    fallback_applied: false,
                    reconnect_policy: "auto-retry".to_string(),
                }),
                created_at: now,
                updated_at: now,
                last_exit_code: None,
                restart_count: 0,
            },
        );

        Ok(child_session_id)
    })
}

fn unique_session_id(
    state: &launch_code::model::AppState,
    base: &str,
    allow_suffix: bool,
) -> Option<String> {
    if !state.sessions.contains_key(base) {
        return Some(base.to_string());
    }
    if !allow_suffix {
        return None;
    }

    let mut suffix = 1usize;
    loop {
        let candidate = format!("{base}-{suffix}");
        if !state.sessions.contains_key(&candidate) {
            return Some(candidate);
        }
        suffix += 1;
    }
}

fn parse_debugpy_attach_target(event: &serde_json::Value) -> Result<DebugpyAttachTarget, AppError> {
    if event.get("type").and_then(|value| value.as_str()) != Some("event") {
        return Err(AppError::Dap(
            "event payload type must be `event`".to_string(),
        ));
    }
    if event.get("event").and_then(|value| value.as_str()) != Some("debugpyAttach") {
        return Err(AppError::Dap(
            "event name must be `debugpyAttach`".to_string(),
        ));
    }

    let body = event
        .get("body")
        .and_then(|value| value.as_object())
        .ok_or_else(|| AppError::Dap("debugpyAttach event requires object body".to_string()))?;
    let (host, port) = parse_attach_host_port(body)?;
    let process_id = body
        .get("subProcessId")
        .or_else(|| body.get("processId"))
        .and_then(parse_optional_u32);

    let mut attach_arguments = body.clone();
    for key in [
        "name", "type", "request", "connect", "host", "port", "listen",
    ] {
        attach_arguments.remove(key);
    }
    if attach_arguments.is_empty() {
        attach_arguments.insert("justMyCode".to_string(), json!(false));
    }

    Ok(DebugpyAttachTarget {
        host,
        port,
        process_id,
        attach_arguments: serde_json::Value::Object(attach_arguments),
        source_event: event.clone(),
    })
}

fn parse_attach_host_port(
    body: &serde_json::Map<String, serde_json::Value>,
) -> Result<(String, u16), AppError> {
    if let Some(connect) = body.get("connect").and_then(|value| value.as_object()) {
        let host = connect
            .get("host")
            .and_then(|value| value.as_str())
            .ok_or_else(|| AppError::Dap("connect.host is required".to_string()))?;
        let port = connect
            .get("port")
            .and_then(parse_optional_u16)
            .ok_or_else(|| AppError::Dap("connect.port is required".to_string()))?;
        return Ok((host.to_string(), port));
    }

    let host = body
        .get("host")
        .and_then(|value| value.as_str())
        .ok_or_else(|| AppError::Dap("host is required when connect is missing".to_string()))?;
    let port = body
        .get("port")
        .and_then(parse_optional_u16)
        .ok_or_else(|| AppError::Dap("port is required when connect is missing".to_string()))?;
    Ok((host.to_string(), port))
}

fn parse_optional_u16(value: &serde_json::Value) -> Option<u16> {
    if let Some(num) = value.as_u64() {
        return u16::try_from(num).ok();
    }
    value.as_str().and_then(|text| text.parse::<u16>().ok())
}

fn parse_optional_u32(value: &serde_json::Value) -> Option<u32> {
    if let Some(num) = value.as_u64() {
        return u32::try_from(num).ok();
    }
    value.as_str().and_then(|text| text.parse::<u32>().ok())
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
        dap_write_message(&mut *writer, msg)
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
        let msg = match dap_read_message(&mut reader) {
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
        dap_write_message(&mut *writer, msg)
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
        let msg = match dap_read_message(&mut reader) {
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

fn dap_write_message<W: Write>(writer: &mut W, msg: &serde_json::Value) -> Result<(), AppError> {
    let payload = serde_json::to_vec(msg).map_err(|err| AppError::Dap(err.to_string()))?;
    let header = format!("Content-Length: {}\r\n\r\n", payload.len());
    writer
        .write_all(header.as_bytes())
        .map_err(|err| AppError::Dap(err.to_string()))?;
    writer
        .write_all(&payload)
        .map_err(|err| AppError::Dap(err.to_string()))?;
    writer
        .flush()
        .map_err(|err| AppError::Dap(err.to_string()))?;
    Ok(())
}

fn dap_read_message<R: BufRead>(reader: &mut R) -> Result<serde_json::Value, AppError> {
    let mut content_len: Option<usize> = None;
    loop {
        let mut line = String::new();
        let bytes = reader
            .read_line(&mut line)
            .map_err(|err| AppError::Dap(err.to_string()))?;
        if bytes == 0 {
            return Err(AppError::Dap(
                "unexpected eof while reading headers".to_string(),
            ));
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        let lower = trimmed.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("content-length:") {
            let parsed = rest
                .trim()
                .parse::<usize>()
                .map_err(|err| AppError::Dap(err.to_string()))?;
            content_len = Some(parsed);
        }
    }

    let len =
        content_len.ok_or_else(|| AppError::Dap("missing content-length header".to_string()))?;
    let mut buf = vec![0u8; len];
    reader
        .read_exact(&mut buf)
        .map_err(|err| AppError::Dap(err.to_string()))?;
    serde_json::from_slice(&buf).map_err(|err| AppError::Dap(err.to_string()))
}
