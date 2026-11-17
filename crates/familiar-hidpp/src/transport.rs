use std::io;
use std::sync::Arc;
use std::time::Duration;

use crate::ReportKind;

/// Interrupts a [`Transport::read`] that is blocked in another thread.
pub trait Wake: Send + Sync {
    fn wake(&self);
}

/// The sending half of a transport.
pub trait Writer: Send {
    fn write(&mut self, report: &[u8]) -> io::Result<()>;
}

/// A raw HID++ endpoint, such as a hidraw node or a Win32 HID handle.
///
/// The transport itself is the receiving half and is read from one thread.
/// [`Transport::writer`] hands out a sending half that can be used from another.
/// Reports include the report ID as their first byte.
pub trait Transport: Send {
    /// Reads one report into `buf`, waiting at most `timeout`.
    /// Returns `Ok(None)` when nothing arrived in time or the read was woken.
    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> io::Result<Option<usize>>;

    fn supports(&self, kind: ReportKind) -> bool;

    fn writer(&self) -> io::Result<Box<dyn Writer>>;

    /// A handle that makes a blocked `read` return early, if the transport has one.
    fn waker(&self) -> Option<Arc<dyn Wake>> {
        None
    }
}
