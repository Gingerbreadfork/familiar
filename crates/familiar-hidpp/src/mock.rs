//! An in-memory transport for tests.

use std::collections::VecDeque;
use std::io;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::transport::{Wake, Writer};
use crate::{ReportKind, Transport};

type Responder = Box<dyn FnMut(&[u8]) -> Vec<Vec<u8>> + Send>;

#[derive(Default)]
struct State {
    responder: Option<Responder>,
    pending: VecDeque<Vec<u8>>,
    written: Vec<Vec<u8>>,
    failed: bool,
    woken: bool,
}

#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    ready: Condvar,
}

/// Answers each written report with whatever the responder returns.
pub struct MockTransport {
    shared: Arc<Shared>,
    kinds: Vec<ReportKind>,
}

/// Test-side handle that stays usable after the transport is handed off.
#[derive(Clone)]
pub struct MockHandle(Arc<Shared>);

impl MockTransport {
    pub fn new() -> Self {
        MockTransport { shared: Arc::default(), kinds: vec![ReportKind::Short, ReportKind::Long] }
    }

    pub fn on_write(&self, f: impl FnMut(&[u8]) -> Vec<Vec<u8>> + Send + 'static) {
        self.shared.state.lock().unwrap().responder = Some(Box::new(f));
    }

    pub fn with_kinds(mut self, kinds: &[ReportKind]) -> Self {
        self.kinds = kinds.to_vec();
        self
    }

    pub fn handle(&self) -> MockHandle {
        MockHandle(self.shared.clone())
    }
}

impl Default for MockTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl MockHandle {
    /// Queues a report as if the device had sent it unprompted.
    pub fn push(&self, report: Vec<u8>) {
        self.0.state.lock().unwrap().pending.push_back(report);
        self.0.ready.notify_all();
    }

    /// Makes the next read fail, as when the device is unplugged.
    pub fn fail(&self) {
        self.0.state.lock().unwrap().failed = true;
        self.0.ready.notify_all();
    }

    pub fn written(&self) -> Vec<Vec<u8>> {
        self.0.state.lock().unwrap().written.clone()
    }
}

struct MockWriter(Arc<Shared>);

impl Writer for MockWriter {
    fn write(&mut self, report: &[u8]) -> io::Result<()> {
        let mut s = self.0.state.lock().unwrap();
        s.written.push(report.to_vec());
        if let Some(mut r) = s.responder.take() {
            let replies = r(report);
            s.pending.extend(replies);
            s.responder = Some(r);
        }
        self.0.ready.notify_all();
        Ok(())
    }
}

struct MockWaker(Arc<Shared>);

impl Wake for MockWaker {
    fn wake(&self) {
        self.0.state.lock().unwrap().woken = true;
        self.0.ready.notify_all();
    }
}

impl Transport for MockTransport {
    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> io::Result<Option<usize>> {
        let deadline = Instant::now() + timeout;
        let mut s = self.shared.state.lock().unwrap();
        loop {
            if s.failed {
                return Err(io::Error::new(io::ErrorKind::BrokenPipe, "mock device removed"));
            }
            if let Some(r) = s.pending.pop_front() {
                let n = r.len().min(buf.len());
                buf[..n].copy_from_slice(&r[..n]);
                return Ok(Some(n));
            }
            if std::mem::take(&mut s.woken) {
                return Ok(None);
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(None);
            }
            s = self.shared.ready.wait_timeout(s, left).unwrap().0;
        }
    }

    fn supports(&self, kind: ReportKind) -> bool {
        self.kinds.contains(&kind)
    }

    fn writer(&self) -> io::Result<Box<dyn Writer>> {
        Ok(Box::new(MockWriter(self.shared.clone())))
    }

    fn waker(&self) -> Option<Arc<dyn Wake>> {
        Some(Arc::new(MockWaker(self.shared.clone())))
    }
}
