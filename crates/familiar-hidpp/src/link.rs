//! A HID++ endpoint shared between threads.
//!
//! A reader thread owns the receiving half of the transport. Replies are handed
//! to the thread waiting for them, and everything else that looks like an event
//! is passed to the event sink as soon as it arrives, so a slow request never
//! holds up input events.

use std::io;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::transport::{Wake, Writer};
use crate::{Error, FeatureError, Packet, RegisterError, ReportKind, Transport};

/// What the reader thread passes on.
#[derive(Debug)]
pub enum Incoming {
    Event(Packet),
    /// The transport failed, usually because the device was unplugged.
    Closed(String),
}

/// Raw request access to an endpoint.
///
/// These methods skip the [`policy`](crate::policy) checks. Use them through
/// [`Device`](crate::Device), which applies the policy.
pub trait Exchange: Send + Sync {
    fn raw_request(&self, device: u8, feature_index: u8, function: u8, params: &[u8]) -> Result<Packet, Error>;
    /// Reads a HID++ 1.0 register. Short registers return 3 bytes, long ones 16.
    fn raw_read_register(&self, device: u8, register: u8, long: bool, params: &[u8]) -> Result<Vec<u8>, Error>;
    /// Writes a HID++ 1.0 short register.
    fn raw_write_register(&self, device: u8, register: u8, params: &[u8]) -> Result<Vec<u8>, Error>;
}

/// Software IDs stamped on requests, so replies to us can be told apart from
/// replies to other programs. Events use 0 and Solaar uses 0x8 to 0xF.
pub const DAEMON_SWIDS: (u8, u8) = (0x4, 0x7);
pub const TOOL_SWIDS: (u8, u8) = (0x2, 0x3);

struct Waiting {
    id: u64,
    device: u8,
    sub_id: u8,
    address: u8,
    result: Option<Result<Packet, Error>>,
}

struct Shared {
    writer: Mutex<Box<dyn Writer>>,
    waiting: Mutex<Vec<Waiting>>,
    answered: Condvar,
    closed: AtomicBool,
    stop: AtomicBool,
    next_swid: AtomicU8,
    next_id: Mutex<u64>,
}

pub struct Link {
    shared: Arc<Shared>,
    kinds: Vec<ReportKind>,
    swids: (u8, u8),
    timeout: Duration,
    waker: Option<Arc<dyn Wake>>,
    reader: Option<thread::JoinHandle<()>>,
}

impl Link {
    /// Starts the reader thread. `sink` receives events and the close notice.
    pub fn new(
        mut transport: Box<dyn Transport>,
        swids: (u8, u8),
        sink: impl Fn(Incoming) + Send + 'static,
    ) -> io::Result<Self> {
        let kinds: Vec<ReportKind> = ReportKind::ALL.into_iter().filter(|k| transport.supports(*k)).collect();
        let shared = Arc::new(Shared {
            writer: Mutex::new(transport.writer()?),
            waiting: Mutex::new(Vec::new()),
            answered: Condvar::new(),
            closed: AtomicBool::new(false),
            stop: AtomicBool::new(false),
            next_swid: AtomicU8::new(swids.0),
            next_id: Mutex::new(0),
        });
        let waker = transport.waker();
        // Without a waker the reader wakes up now and then to notice a stop.
        let poll = if waker.is_some() { Duration::from_secs(3600) } else { Duration::from_millis(100) };
        let s = shared.clone();
        let reader = thread::Builder::new().name("hidpp reader".into()).spawn(move || {
            let mut buf = [0u8; 64];
            while !s.stop.load(Ordering::SeqCst) {
                match transport.read(&mut buf, poll) {
                    Ok(Some(n)) => {
                        if let Some(p) = Packet::decode(&buf[..n])
                            && let Some(event) = s.deliver(p)
                        {
                            sink(Incoming::Event(event));
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        s.close();
                        if !s.stop.load(Ordering::SeqCst) {
                            sink(Incoming::Closed(e.to_string()));
                        }
                        return;
                    }
                }
            }
        })?;
        Ok(Link { shared, kinds, swids, timeout: Duration::from_secs(2), waker, reader: Some(reader) })
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    pub fn is_closed(&self) -> bool {
        self.shared.closed.load(Ordering::SeqCst)
    }

    fn kind_for(&self, len: usize) -> Result<ReportKind, Error> {
        self.kinds.iter().copied().find(|k| len <= k.params_len()).ok_or(Error::NoReportKind)
    }

    fn swid(&self) -> u8 {
        let (lo, hi) = self.swids;
        let s = self.shared.next_swid.fetch_add(1, Ordering::SeqCst);
        lo + (s.wrapping_sub(lo)) % (hi - lo + 1)
    }

    fn exchange(&self, device: u8, sub_id: u8, address: u8, params: &[u8]) -> Result<Packet, Error> {
        let packet = Packet::new(self.kind_for(params.len())?, device, sub_id, address, params)?;
        let id = {
            let mut n = self.shared.next_id.lock().unwrap();
            *n += 1;
            *n
        };
        // Registered before writing, so a fast reply cannot be missed. Closing
        // fails what is registered under this same lock, so nothing is stranded.
        {
            let mut waiting = self.shared.waiting.lock().unwrap();
            if self.is_closed() {
                return Err(closed());
            }
            waiting.push(Waiting { id, device, sub_id, address, result: None });
        }
        if let Err(e) = self.shared.writer.lock().unwrap().write(&packet.encode()) {
            self.shared.waiting.lock().unwrap().retain(|w| w.id != id);
            return Err(e.into());
        }

        let deadline = Instant::now() + self.timeout;
        let mut waiting = self.shared.waiting.lock().unwrap();
        loop {
            let pos = waiting.iter().position(|w| w.id == id).expect("request is registered");
            if waiting[pos].result.is_some() {
                return waiting.remove(pos).result.expect("checked above");
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                waiting.remove(pos);
                return Err(Error::Timeout);
            }
            waiting = self.shared.answered.wait_timeout(waiting, left).unwrap().0;
        }
    }
}

impl Shared {
    /// Hands a packet to the request waiting for it. Returns it back when it
    /// is an event nobody asked for.
    fn deliver(&self, reply: Packet) -> Option<Packet> {
        let mut waiting = self.waiting.lock().unwrap();
        let p = reply.params();
        let found = waiting.iter_mut().find(|w| {
            w.result.is_none()
                && w.device == reply.device
                && ((reply.sub_id == w.sub_id && reply.address == w.address)
                    // Error replies shift the original sub ID and address one byte right.
                    || (matches!(reply.sub_id, 0xFF | 0x8F) && reply.address == w.sub_id && p[0] == w.address))
        });
        if let Some(w) = found {
            w.result = Some(match reply.sub_id {
                0xFF if w.sub_id != 0xFF => Err(Error::Feature(FeatureError(p[1]))),
                0x8F if w.sub_id != 0x8F => Err(Error::Register(RegisterError(p[1]))),
                _ => Ok(reply),
            });
            self.answered.notify_all();
            return None;
        }
        drop(waiting);
        is_event(&reply).then_some(reply)
    }

    fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        let mut waiting = self.waiting.lock().unwrap();
        for w in waiting.iter_mut().filter(|w| w.result.is_none()) {
            w.result = Some(Err(closed()));
        }
        self.answered.notify_all();
    }
}

fn closed() -> Error {
    Error::Io(io::Error::new(io::ErrorKind::BrokenPipe, "endpoint closed"))
}

impl Exchange for Link {
    fn raw_request(&self, device: u8, feature_index: u8, function: u8, params: &[u8]) -> Result<Packet, Error> {
        self.exchange(device, feature_index, (function << 4) | self.swid(), params)
    }

    fn raw_read_register(&self, device: u8, register: u8, long: bool, params: &[u8]) -> Result<Vec<u8>, Error> {
        let sub_id = if long { 0x83 } else { 0x81 };
        Ok(self.exchange(device, sub_id, register, params)?.params().to_vec())
    }

    fn raw_write_register(&self, device: u8, register: u8, params: &[u8]) -> Result<Vec<u8>, Error> {
        Ok(self.exchange(device, 0x80, register, params)?.params().to_vec())
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        if let Some(w) = &self.waker {
            w.wake();
        }
        if let Some(t) = self.reader.take() {
            let _ = t.join();
        }
    }
}

/// HID++ 2.0 events carry software ID 0. Receivers send HID++ 1.0 notifications
/// with sub IDs 0x40 to 0x4F. Anything else is a reply to some other program.
pub fn is_event(p: &Packet) -> bool {
    (p.sub_id < 0x80 && p.swid() == 0) || (0x40..=0x4F).contains(&p.sub_id)
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;
    use crate::mock::MockTransport;

    fn link(mock: MockTransport) -> (Link, mpsc::Receiver<Incoming>) {
        let (tx, rx) = mpsc::channel();
        let link = Link::new(Box::new(mock), TOOL_SWIDS, move |i| {
            let _ = tx.send(i);
        })
        .unwrap();
        (link, rx)
    }

    #[test]
    fn matches_reply_and_passes_events_on() {
        let mock = MockTransport::new();
        mock.on_write(|req| {
            let event = vec![0x10, 0x02, 0x04, 0x00, 0x01, 0x00, 0x00];
            let foreign = vec![0x11, 0x02, 0x09, 0x18, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
            let mut reply = req.to_vec();
            reply[4] = 0x04;
            reply[5] = 0x05;
            vec![event, foreign, reply]
        });
        let (link, events) = link(mock);
        let reply = link.raw_request(0x02, 0x00, 0x1, &[0, 0, 0x42]).unwrap();
        assert_eq!(&reply.params()[..3], &[4, 5, 0x42]);
        match events.recv_timeout(Duration::from_secs(1)).unwrap() {
            Incoming::Event(p) => assert_eq!(p.sub_id, 0x04),
            other => panic!("unexpected {other:?}"),
        }
        assert!(events.recv_timeout(Duration::from_millis(50)).is_err());
    }

    #[test]
    fn maps_feature_error() {
        let mock = MockTransport::new();
        mock.on_write(|req| vec![vec![0x10, req[1], 0xFF, req[2], req[3], 0x02, 0x00]]);
        let (link, _events) = link(mock);
        match link.raw_request(0x02, 0x07, 0x3, &[]) {
            Err(Error::Feature(e)) => assert_eq!(e.0, FeatureError::INVALID_ARGUMENT),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn maps_register_error() {
        let mock = MockTransport::new();
        mock.on_write(|req| vec![vec![0x10, req[1], 0x8F, req[2], req[3], 0x09, 0x00]]);
        let (link, _events) = link(mock);
        match link.raw_request(0x03, 0x00, 0x1, &[]) {
            Err(Error::Register(e)) => assert!(e.is_unreachable()),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn times_out() {
        let (link, _events) = link(MockTransport::new());
        let link = link.with_timeout(Duration::from_millis(20));
        assert!(matches!(link.raw_request(1, 0, 0, &[]), Err(Error::Timeout)));
    }

    #[test]
    fn concurrent_requests_get_their_own_replies() {
        let mock = MockTransport::new();
        mock.on_write(|req| {
            let mut reply = req.to_vec();
            reply[4] = req[2];
            vec![reply]
        });
        let (link, _events) = link(mock);
        let link = Arc::new(link);
        let handles: Vec<_> = (1..=8u8)
            .map(|i| {
                let link = link.clone();
                thread::spawn(move || link.raw_request(0x02, i, 0x1, &[]).unwrap().params()[0])
            })
            .collect();
        let got: Vec<u8> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert_eq!(got, (1..=8).collect::<Vec<u8>>());
    }

    #[test]
    fn reports_close_and_fails_requests() {
        let mock = MockTransport::new();
        let handle = mock.handle();
        let (link, events) = link(mock);
        handle.fail();
        assert!(matches!(events.recv_timeout(Duration::from_secs(1)).unwrap(), Incoming::Closed(_)));
        assert!(link.is_closed());
        assert!(link.raw_request(1, 0, 0, &[]).is_err());
    }

    #[test]
    fn rotates_software_ids() {
        let (link, _e) = link(MockTransport::new());
        let ids: Vec<u8> = (0..4).map(|_| link.swid()).collect();
        assert_eq!(ids, vec![2, 3, 2, 3]);
    }
}
