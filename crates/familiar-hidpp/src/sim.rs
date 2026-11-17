//! A simulated mouse behind a receiver, for testing code that drives a
//! [`Link`](crate::Link) without hardware.
//!
//! It implements enough of Root, Feature Set, FW Version, Device Name, Unified
//! Battery and Adjustable DPI to be identified and configured, and can be
//! disconnected and reconnected like a mouse going to sleep.

use std::collections::VecDeque;
use std::io;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crate::transport::{Wake, Writer};
use crate::{ReportKind, Transport};

const FEATURES: [u16; 9] = [0x0000, 0x0001, 0x0003, 0x0005, 0x1004, 0x2201, 0x1B04, 0x2121, 0x2150];
const REPROG_INDEX: u8 = 6;
const WHEEL_INDEX: u8 = 7;
const THUMB_INDEX: u8 = 8;

/// Controls as the MX Master 4 lists them: control ID, the capability flags
/// from bytes 4 and 8 of `getCidInfo`, group and group mask.
const CONTROLS: [(u16, u16, u8, u8); 7] = [
    (0x0050, 0x0001, 1, 0),
    (0x0051, 0x0001, 1, 0),
    (0x0052, 0x0131, 2, 3),
    (0x0053, 0x0131, 2, 3),
    (0x0056, 0x0131, 2, 3),
    (0x00C4, 0x0131, 2, 3),
    (0x01A0, 0x0131, 2, 3),
];
pub const UNIT_ID: [u8; 4] = [0x1A, 0x2B, 0x3C, 0x4D];
const NAME: &[u8] = b"Sim Master";
pub const DEFAULT_DPI: u16 = 1000;

pub struct SimState {
    pub index: u8,
    pub online: bool,
    pub dpi: u16,
    pub battery: u8,
    pub requests: usize,
    /// Reporting flags and remap target by control ID. Lost on reconnect.
    pub reporting: std::collections::HashMap<u16, (u16, u16)>,
    /// HiRes Wheel mode byte: bit 0 diverted, bit 1 hi-res, bit 2 inverted.
    pub wheel_mode: u8,
    /// Every wheel mode byte written, oldest first.
    pub wheel_writes: Vec<u8>,
    /// Thumb wheel diverted and inverted.
    pub thumb: (bool, bool),
    held: Vec<u16>,
    pending: VecDeque<Vec<u8>>,
    woken: bool,
}

struct Shared {
    state: Mutex<SimState>,
    ready: Condvar,
}

/// Test-side handle to the simulated device.
#[derive(Clone)]
pub struct Sim(Arc<Shared>);

/// The transport side, handed to a `Link`.
pub struct SimTransport(Arc<Shared>);

impl Sim {
    /// A mouse paired at `index` on a receiver, online, at the default DPI.
    pub fn receiver(index: u8) -> (Sim, SimTransport) {
        let shared = Arc::new(Shared {
            state: Mutex::new(SimState {
                index,
                online: true,
                dpi: DEFAULT_DPI,
                battery: 80,
                requests: 0,
                reporting: std::collections::HashMap::new(),
                wheel_mode: 0,
                wheel_writes: Vec::new(),
                thumb: (false, false),
                held: Vec::new(),
                pending: VecDeque::new(),
                woken: false,
            }),
            ready: Condvar::new(),
        });
        (Sim(shared.clone()), SimTransport(shared))
    }

    pub fn state(&self) -> MutexGuard<'_, SimState> {
        self.0.state.lock().unwrap()
    }

    fn push(&self, report: Vec<u8>) {
        self.state().pending.push_back(report);
        self.0.ready.notify_all();
    }

    /// The mouse drops its link, as when it sleeps or switches host.
    pub fn disconnect(&self) {
        let index = {
            let mut s = self.state();
            s.online = false;
            s.index
        };
        self.push(vec![0x10, index, 0x41, 0x10, 0x40, 0x42, 0xB0]);
    }

    /// The mouse comes back having forgotten its settings, as real ones do.
    pub fn reconnect(&self) {
        let index = {
            let mut s = self.state();
            s.online = true;
            s.dpi = DEFAULT_DPI;
            s.reporting.clear();
            s.wheel_mode = 0;
            s.thumb = (false, false);
            s.index
        };
        self.push(vec![0x10, index, 0x41, 0x10, 0x00, 0x42, 0xB0]);
    }

    /// Whether a control is diverted now.
    pub fn diverted(&self, cid: u16) -> bool {
        self.state().reporting.get(&cid).is_some_and(|(f, _)| f & 0x01 != 0)
    }

    /// Presses a control. Returns true when it was diverted and so reported
    /// over HID++, false when it would have gone out as a plain HID button.
    pub fn press(&self, cid: u16) -> bool {
        let (index, held, diverted) = {
            let mut s = self.state();
            s.held.push(cid);
            let diverted = s.reporting.get(&cid).is_some_and(|(f, _)| f & 0x01 != 0);
            let held: Vec<u16> =
                s.held.iter().copied().filter(|c| s.reporting.get(c).is_some_and(|(f, _)| f & 0x01 != 0)).collect();
            (s.index, held, diverted)
        };
        if diverted {
            self.push(buttons_event(index, &held));
        }
        diverted
    }

    pub fn release(&self, cid: u16) -> bool {
        let (index, held, diverted) = {
            let mut s = self.state();
            s.held.retain(|c| *c != cid);
            let diverted = s.reporting.get(&cid).is_some_and(|(f, _)| f & 0x01 != 0);
            let held: Vec<u16> =
                s.held.iter().copied().filter(|c| s.reporting.get(c).is_some_and(|(f, _)| f & 0x01 != 0)).collect();
            (s.index, held, diverted)
        };
        if diverted {
            self.push(buttons_event(index, &held));
        }
        diverted
    }

    /// Moves the pointer. Reported over HID++ only while a control diverted
    /// with raw XY is held.
    pub fn motion(&self, dx: i16, dy: i16) {
        let (index, report) = {
            let s = self.state();
            let report = s.held.iter().any(|c| s.reporting.get(c).is_some_and(|(f, _)| f & 0x11 == 0x11));
            (s.index, report)
        };
        if report {
            let [x1, x2] = dx.to_be_bytes();
            let [y1, y2] = dy.to_be_bytes();
            let mut r = vec![0x11, index, REPROG_INDEX, 0x10, x1, x2, y1, y2];
            r.resize(20, 0);
            self.push(r);
        }
    }

    /// Turns the main wheel by `delta` counts, reported over HID++ only when diverted.
    pub fn wheel(&self, delta: i16) {
        let (index, mode) = {
            let s = self.state();
            (s.index, s.wheel_mode)
        };
        if mode & 0x01 != 0 {
            let [d1, d2] = delta.to_be_bytes();
            let hires = if mode & 0x02 != 0 { 0x10 } else { 0 };
            let mut r = vec![0x11, index, WHEEL_INDEX, 0x00, hires | 1, d1, d2];
            r.resize(20, 0);
            self.push(r);
        }
    }

    /// A thumb wheel event: `delta` counts, rotation phase, and touch.
    pub fn thumb_wheel(&self, delta: i16, phase: u8, touch: bool) {
        let (index, diverted) = {
            let s = self.state();
            (s.index, s.thumb.0)
        };
        if diverted {
            let [d1, d2] = delta.to_be_bytes();
            let mut r = vec![0x11, index, THUMB_INDEX, 0x00, d1, d2, 0, 0, phase, if touch { 0x02 } else { 0 }];
            r.resize(20, 0);
            self.push(r);
        }
    }

    /// Sends a battery event with a new level.
    pub fn battery_event(&self, percent: u8) {
        let index = {
            let mut s = self.state();
            s.battery = percent;
            s.index
        };
        let mut r = vec![0x11, index, 4, 0x00, percent, 4, 0, 0];
        r.resize(20, 0);
        self.push(r);
    }
}

fn buttons_event(index: u8, held: &[u16]) -> Vec<u8> {
    let mut r = vec![0x11, index, REPROG_INDEX, 0x00];
    for c in held.iter().take(4) {
        r.extend_from_slice(&c.to_be_bytes());
    }
    r.resize(20, 0);
    r
}

fn reply(req: &[u8], params: &[u8]) -> Vec<u8> {
    let mut r = vec![0x11, req[1], req[2], req[3]];
    r.extend_from_slice(params);
    r.resize(20, 0);
    r
}

fn error(req: &[u8], kind: u8, code: u8) -> Vec<u8> {
    vec![0x10, req[1], kind, req[2], req[3], code, 0]
}

impl SimState {
    fn answer(&mut self, req: &[u8]) -> Vec<u8> {
        self.requests += 1;
        let (dev, sub, func) = (req[1], req[2], req[3] >> 4);
        let p = &req[4..];
        if dev == 0xFF {
            // Receiver registers: report notification flags, accept writes.
            return match sub {
                0x81 | 0x80 => reply(req, &[0x00, 0x09, 0x00]),
                _ => error(req, 0x8F, 0x01),
            };
        }
        if dev != self.index || !self.online {
            return error(req, 0x8F, 0x09);
        }
        let Some(&feature) = FEATURES.get(sub as usize) else { return error(req, 0xFF, 0x06) };
        match (feature, func) {
            (0x0000, 0) => {
                let id = u16::from_be_bytes([p[0], p[1]]);
                let index = FEATURES.iter().position(|f| *f == id).unwrap_or(0) as u8;
                reply(req, &[index, 0, 0])
            }
            (0x0000, 1) => reply(req, &[4, 5, p[2]]),
            (0x0001, 0) => reply(req, &[(FEATURES.len() - 1) as u8]),
            (0x0001, 1) => {
                let id = FEATURES.get(p[0] as usize).copied().unwrap_or(0);
                reply(req, &[(id >> 8) as u8, id as u8, 0, 0])
            }
            (0x0003, 0) => {
                let mut out = vec![1];
                out.extend_from_slice(&UNIT_ID);
                out.extend_from_slice(&[0x00, 0x04, 0xB0, 0x42, 0, 0, 0, 0]);
                reply(req, &out)
            }
            (0x0003, 1) => reply(req, &[0, b'R', b'B', b'M', 0x27, 0x00, 0x00, 0x15]),
            (0x0005, 0) => reply(req, &[NAME.len() as u8]),
            (0x0005, 1) => reply(req, &NAME[(p[0] as usize).min(NAME.len())..]),
            (0x0005, 2) => reply(req, &[3]),
            (0x1004, 1) => reply(req, &[self.battery, 4, 0, 0]),
            (0x2201, 1) => reply(req, &[0, 0x00, 0xC8, 0xE0, 0x32, 0x1F, 0x40, 0, 0]),
            (0x2201, 2) => {
                let [hi, lo] = self.dpi.to_be_bytes();
                let [dhi, dlo] = DEFAULT_DPI.to_be_bytes();
                reply(req, &[0, hi, lo, dhi, dlo])
            }
            (0x2201, 3) => {
                self.dpi = u16::from_be_bytes([p[1], p[2]]);
                reply(req, &p[..3])
            }
            (0x2121, 0) => reply(req, &[15, 0x0C]),
            (0x2121, 1) => reply(req, &[self.wheel_mode]),
            (0x2121, 2) => {
                self.wheel_mode = p[0] & 0x07;
                self.wheel_writes.push(self.wheel_mode);
                reply(req, &[self.wheel_mode])
            }
            (0x2121, 3) => reply(req, &[1]),
            (0x2150, 0) => reply(req, &[0, 20, 0, 120, 0, 0x03, 0x03, 0xE8]),
            (0x2150, 1) => reply(req, &[self.thumb.0 as u8, self.thumb.1 as u8]),
            (0x2150, 2) => {
                self.thumb = (p[0] & 0x01 != 0, p[1] & 0x01 != 0);
                reply(req, &p[..2])
            }
            (0x1B04, 0) => reply(req, &[CONTROLS.len() as u8]),
            (0x1B04, 1) => {
                let Some(&(cid, caps, group, mask)) = CONTROLS.get(p[0] as usize) else {
                    return error(req, 0xFF, 0x02);
                };
                let [c1, c2] = cid.to_be_bytes();
                reply(req, &[c1, c2, 0, 0, caps as u8, 0, group, mask, (caps >> 8) as u8])
            }
            (0x1B04, 2) => {
                let cid = u16::from_be_bytes([p[0], p[1]]);
                let (flags, remap) = self.reporting.get(&cid).copied().unwrap_or((0, 0));
                let [r1, r2] = remap.to_be_bytes();
                reply(req, &[p[0], p[1], flags as u8, r1, r2, (flags >> 8) as u8])
            }
            (0x1B04, 3) => {
                let cid = u16::from_be_bytes([p[0], p[1]]);
                if !CONTROLS.iter().any(|c| c.0 == cid) {
                    return error(req, 0xFF, 0x02);
                }
                let bits = p[2] as u16 | (p[5] as u16) << 8;
                let target = u16::from_be_bytes([p[3], p[4]]);
                let entry = self.reporting.entry(cid).or_insert((0, 0));
                // Each flag is followed by its "valid" bit.
                for flag in [0x0001u16, 0x0004, 0x0010, 0x0040, 0x0100, 0x0400] {
                    if bits & (flag << 1) != 0 {
                        entry.0 = if bits & flag != 0 { entry.0 | flag } else { entry.0 & !flag };
                    }
                }
                if target != 0 {
                    entry.1 = if target == cid { 0 } else { target };
                }
                reply(req, &p[..6])
            }
            _ => error(req, 0xFF, 0x07),
        }
    }
}

struct SimWaker(Arc<Shared>);

impl Wake for SimWaker {
    fn wake(&self) {
        self.0.state.lock().unwrap().woken = true;
        self.0.ready.notify_all();
    }
}

struct SimWriter(Arc<Shared>);

impl Writer for SimWriter {
    fn write(&mut self, report: &[u8]) -> io::Result<()> {
        let mut s = self.0.state.lock().unwrap();
        let r = s.answer(report);
        s.pending.push_back(r);
        self.0.ready.notify_all();
        Ok(())
    }
}

impl Transport for SimTransport {
    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> io::Result<Option<usize>> {
        let deadline = Instant::now() + timeout;
        let mut s = self.0.state.lock().unwrap();
        loop {
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
            s = self.0.ready.wait_timeout(s, left).unwrap().0;
        }
    }

    fn supports(&self, kind: ReportKind) -> bool {
        matches!(kind, ReportKind::Short | ReportKind::Long)
    }

    fn writer(&self) -> io::Result<Box<dyn Writer>> {
        Ok(Box::new(SimWriter(self.0.clone())))
    }

    fn waker(&self) -> Option<Arc<dyn Wake>> {
        Some(Arc::new(SimWaker(self.0.clone())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Device;
    use crate::features::{dpi, info};
    use crate::link::{Link, TOOL_SWIDS};

    #[test]
    fn identifies_and_sets_dpi() {
        let (sim, t) = Sim::receiver(2);
        let link = Arc::new(Link::new(Box::new(t), TOOL_SWIDS, |_| {}).unwrap());
        let mut dev = Device::new(link, 2);
        assert_eq!(dev.ping().unwrap(), (4, 5));
        assert_eq!(info::name(&mut dev).unwrap(), "Sim Master");
        assert_eq!(info::ids(&mut dev).unwrap().unit_id, "1A2B3C4D");
        assert_eq!(dpi::list(&mut dev, 0).unwrap().len(), 157);
        assert_eq!(crate::features::reprog::controls(&mut dev).unwrap().len(), 7);
        crate::features::reprog::set_reporting(&mut dev, 0x0053, 0x0011, 0).unwrap();
        assert!(sim.diverted(0x0053));
        assert!(sim.press(0x0053));
        assert!(!sim.press(0x0056));
        dpi::set(&mut dev, 0, 1600).unwrap();
        assert_eq!(sim.state().dpi, 1600);
        sim.disconnect();
        assert!(dev.ping().is_err());
    }
}
