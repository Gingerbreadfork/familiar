//! Readiness and watchdog notifications to systemd, when it started us.

use std::env;
use std::os::unix::net::{SocketAddr, UnixDatagram};
use std::time::Duration;

use tracing::debug;

fn socket_addr() -> Option<SocketAddr> {
    let path = env::var("NOTIFY_SOCKET").ok()?;
    if let Some(name) = path.strip_prefix('@') {
        use std::os::linux::net::SocketAddrExt;
        SocketAddr::from_abstract_name(name.as_bytes()).ok()
    } else {
        SocketAddr::from_pathname(path).ok()
    }
}

/// Sends a state such as `READY=1`. Does nothing outside systemd.
pub fn notify(state: &str) {
    let Some(addr) = socket_addr() else { return };
    let r = UnixDatagram::unbound().and_then(|s| s.send_to_addr(state.as_bytes(), &addr));
    if let Err(e) = r {
        debug!("sd_notify {state:?} failed: {e}");
    }
}

/// How often to ping the watchdog: half the timeout systemd gave us, or
/// `None` when no watchdog is set for this process.
pub fn watchdog_interval() -> Option<Duration> {
    let usec: u64 = env::var("WATCHDOG_USEC").ok()?.parse().ok()?;
    if let Ok(pid) = env::var("WATCHDOG_PID")
        && pid.parse::<u32>().ok() != Some(std::process::id())
    {
        return None;
    }
    Some(Duration::from_micros(usec / 2).max(Duration::from_millis(100)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notify_reaches_an_abstract_socket() {
        use std::os::linux::net::SocketAddrExt;
        let name = format!("familiar-notify-test-{}", std::process::id());
        let server = UnixDatagram::bind_addr(&SocketAddr::from_abstract_name(name.as_bytes()).unwrap()).unwrap();
        // SAFETY: tests in this module do not read the environment concurrently.
        unsafe { env::set_var("NOTIFY_SOCKET", format!("@{name}")) };
        notify("READY=1");
        let mut buf = [0u8; 32];
        let n = server.recv(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"READY=1");
        unsafe { env::remove_var("NOTIFY_SOCKET") };
    }
}
