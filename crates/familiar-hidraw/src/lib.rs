//! Linux hidraw transport for HID++.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use hidpp::{ReportKind, Transport, Wake, Writer};

pub const LOGITECH: u16 = 0x046D;
pub const BUS_USB: u16 = 0x03;
pub const BUS_BLUETOOTH: u16 = 0x05;

#[derive(Clone, Debug)]
pub struct HidrawInfo {
    pub path: PathBuf,
    pub bus: u16,
    pub vendor: u16,
    pub product: u16,
    pub name: String,
    pub kinds: Vec<ReportKind>,
    /// Kernel driver bound to the HID device, such as `hid-generic`.
    pub driver: String,
    /// A device the kernel split off a receiver. The receiver's own node already
    /// reaches it, so talking to both would drive the same device twice.
    pub receiver_child: bool,
}

impl HidrawInfo {
    /// True when the kernel's Logitech driver handles HID++ for this device,
    /// including enabling hi-res scrolling on connect.
    pub fn kernel_hidpp(&self) -> bool {
        self.driver == "logitech-hidpp-device"
    }
}

fn driver_name(dev: &Path) -> String {
    fs::read_link(dev.join("driver"))
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_default()
}

/// Returns the HID++ report kinds declared in a report descriptor, meaning
/// report IDs 0x10 to 0x12 declared under a vendor-defined usage page.
pub fn hidpp_report_kinds(descriptor: &[u8]) -> Vec<ReportKind> {
    let mut kinds = Vec::new();
    let mut usage_page: u32 = 0;
    let mut report_id: u8 = 0;
    let mut i = 0;
    while i < descriptor.len() {
        let prefix = descriptor[i];
        if prefix == 0xFE {
            let len = *descriptor.get(i + 1).unwrap_or(&0) as usize;
            i += 3 + len;
            continue;
        }
        let size = match prefix & 0x03 {
            3 => 4,
            n => n as usize,
        };
        let Some(data) = descriptor.get(i + 1..i + 1 + size) else { break };
        let value = data.iter().rev().fold(0u32, |acc, b| acc << 8 | *b as u32);
        let kind = (prefix >> 2) & 0x03;
        let tag = prefix >> 4;
        match (kind, tag) {
            (1, 0x0) => usage_page = value,
            (1, 0x8) => report_id = value as u8,
            (0, 0x8 | 0x9 | 0xB) if usage_page >= 0xFF00 => {
                if let Some(k) = ReportKind::from_id(report_id)
                    && !kinds.contains(&k)
                {
                    kinds.push(k);
                }
            }
            _ => {}
        }
        i += 1 + size;
    }
    kinds
}

fn parse_hid_id(uevent: &str) -> Option<(u16, u16, u16)> {
    let line = uevent.lines().find_map(|l| l.strip_prefix("HID_ID="))?;
    let mut parts = line.split(':').map(|p| u32::from_str_radix(p, 16).ok());
    Some((parts.next()?? as u16, parts.next()?? as u16, parts.next()?? as u16))
}

fn info_for(sys: &Path) -> Option<HidrawInfo> {
    let uevent = fs::read_to_string(sys.join("device/uevent")).ok()?;
    let (bus, vendor, product) = parse_hid_id(&uevent)?;
    let name = uevent.lines().find_map(|l| l.strip_prefix("HID_NAME=")).unwrap_or("").to_string();
    let descriptor = fs::read(sys.join("device/report_descriptor")).ok()?;
    let node = sys.file_name()?;
    let hid = fs::canonicalize(sys.join("device")).ok()?;
    let parent_driver = hid.parent().map(driver_name).unwrap_or_default();
    Some(HidrawInfo {
        path: Path::new("/dev").join(node),
        bus,
        vendor,
        product,
        name,
        kinds: hidpp_report_kinds(&descriptor),
        driver: driver_name(&hid),
        receiver_child: parent_driver == "logitech-djreceiver",
    })
}

impl HidrawInfo {
    /// A Logitech HID++ endpoint that we should open.
    pub fn is_usable(&self) -> bool {
        self.vendor == LOGITECH && !self.kinds.is_empty() && !self.receiver_child
    }
}

/// Lists Logitech hidraw nodes that speak HID++, leaving out receiver children.
pub fn enumerate() -> io::Result<Vec<HidrawInfo>> {
    let mut out: Vec<HidrawInfo> = fs::read_dir("/sys/class/hidraw")?
        .filter_map(|e| e.ok())
        .filter_map(|e| info_for(&e.path()))
        .filter(HidrawInfo::is_usable)
        .collect();
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

pub struct Hidraw {
    file: File,
    kinds: Vec<ReportKind>,
    wake: Arc<EventFd>,
}

/// An eventfd that interrupts `poll` in [`Hidraw::read`].
struct EventFd(OwnedFd);

impl EventFd {
    fn new() -> io::Result<Self> {
        // SAFETY: eventfd takes no pointers; a negative return is an error.
        let fd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: fd is a freshly created descriptor that nothing else owns.
        Ok(EventFd(unsafe { OwnedFd::from_raw_fd(fd) }))
    }

    fn drain(&self) {
        let mut v = 0u64;
        // SAFETY: reads 8 bytes into a u64 owned by this frame.
        unsafe { libc::read(self.0.as_raw_fd(), (&raw mut v).cast(), 8) };
    }
}

impl Wake for EventFd {
    fn wake(&self) {
        let v = 1u64;
        // SAFETY: writes 8 bytes from a u64 owned by this frame.
        unsafe { libc::write(self.0.as_raw_fd(), (&raw const v).cast(), 8) };
    }
}

impl Hidraw {
    pub fn open(info: &HidrawInfo) -> io::Result<Self> {
        let file = OpenOptions::new().read(true).write(true).open(&info.path)?;
        Ok(Hidraw { file, kinds: info.kinds.clone(), wake: Arc::new(EventFd::new()?) })
    }
}

struct HidrawWriter(File);

impl Writer for HidrawWriter {
    fn write(&mut self, report: &[u8]) -> io::Result<()> {
        self.0.write_all(report)
    }
}

impl Transport for Hidraw {
    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> io::Result<Option<usize>> {
        let mut fds = [
            libc::pollfd { fd: self.file.as_raw_fd(), events: libc::POLLIN, revents: 0 },
            libc::pollfd { fd: self.wake.0.as_raw_fd(), events: libc::POLLIN, revents: 0 },
        ];
        let ms = timeout.as_millis().min(i32::MAX as u128) as i32;
        // SAFETY: fds is a valid array of two pollfd structs.
        let r = unsafe { libc::poll(fds.as_mut_ptr(), 2, ms) };
        if r < 0 {
            let err = io::Error::last_os_error();
            return if err.kind() == io::ErrorKind::Interrupted { Ok(None) } else { Err(err) };
        }
        if fds[0].revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "hidraw device went away"));
        }
        if fds[0].revents & libc::POLLIN != 0 {
            return self.file.read(buf).map(Some);
        }
        if fds[1].revents & libc::POLLIN != 0 {
            self.wake.drain();
        }
        Ok(None)
    }

    fn supports(&self, kind: ReportKind) -> bool {
        self.kinds.contains(&kind)
    }

    fn writer(&self) -> io::Result<Box<dyn Writer>> {
        Ok(Box::new(HidrawWriter(self.file.try_clone()?)))
    }

    fn waker(&self) -> Option<Arc<dyn Wake>> {
        Some(self.wake.clone())
    }
}

/// Reads the HID++ details of one hidraw node, such as `/dev/hidraw6`.
pub fn info(path: &Path) -> Option<HidrawInfo> {
    let node = path.file_name()?;
    info_for(&Path::new("/sys/class/hidraw").join(node))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_hidpp_reports_in_bolt_descriptor() {
        // HID++ interface descriptor of a Logi Bolt receiver.
        let d = [
            0x06, 0x00, 0xFF, 0x09, 0x01, 0xA1, 0x01, 0x85, 0x10, 0x95, 0x06, 0x75, 0x08, 0x15, 0x00, 0x26, 0xFF, 0x00,
            0x09, 0x01, 0x81, 0x00, 0x09, 0x01, 0x91, 0x00, 0xC0, 0x06, 0x00, 0xFF, 0x09, 0x02, 0xA1, 0x01, 0x85, 0x11,
            0x95, 0x13, 0x75, 0x08, 0x15, 0x00, 0x26, 0xFF, 0x00, 0x09, 0x02, 0x81, 0x00, 0x09, 0x02, 0x91, 0x00, 0xC0,
        ];
        assert_eq!(hidpp_report_kinds(&d), vec![ReportKind::Short, ReportKind::Long]);
    }

    #[test]
    fn ignores_plain_mouse_descriptor() {
        let d = [0x05, 0x01, 0x09, 0x02, 0xA1, 0x01, 0x85, 0x02, 0x09, 0x01, 0x81, 0x02, 0xC0];
        assert!(hidpp_report_kinds(&d).is_empty());
    }

    #[test]
    fn parses_hid_id() {
        assert_eq!(parse_hid_id("HID_ID=0003:0000046D:0000C548\n"), Some((3, 0x046D, 0xC548)));
    }
}
