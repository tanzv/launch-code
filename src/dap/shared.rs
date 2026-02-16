use std::collections::{HashMap, VecDeque};
use std::sync::mpsc;
use std::time::Duration;

use crate::error::AppError;

const MAX_EVENT_QUEUE_LEN: usize = 1000;

#[derive(Debug)]
pub(super) struct DapProxyInner {
    pub waiters: HashMap<u64, mpsc::Sender<serde_json::Value>>,
    pub events: VecDeque<serde_json::Value>,
    pub closed: bool,
}

#[derive(Debug)]
pub(super) struct DapPendingRequest {
    pub seq: u64,
    pub rx: mpsc::Receiver<serde_json::Value>,
}

impl DapPendingRequest {
    pub(super) fn wait<T: DapWaiter>(
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

    pub(super) fn cancel<T: DapWaiter>(&self, waiter: &T) -> Result<(), AppError> {
        waiter.remove_waiter(self.seq)
    }
}

pub(super) trait DapWaiter {
    fn remove_waiter(&self, seq: u64) -> Result<(), AppError>;
}

pub(super) fn push_event(inner: &mut DapProxyInner, event: serde_json::Value) {
    inner.events.push_back(event);
    while inner.events.len() > MAX_EVENT_QUEUE_LEN {
        let _ = inner.events.pop_front();
    }
}
