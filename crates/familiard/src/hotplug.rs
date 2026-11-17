//! udev hidraw hotplug and config file watching, both feeding the manager queue.

use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::thread;

use notify::{EventKind, RecursiveMode, Watcher};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{debug, warn};

use crate::manager::Event;

/// Watches udev for hidraw nodes appearing and disappearing.
pub fn watch_hidraw(tx: UnboundedSender<Event>) -> std::io::Result<()> {
    // The udev handles are not Send, so the monitor is created on its own thread.
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    thread::Builder::new().name("udev".into()).spawn(move || {
        let socket =
            match udev::MonitorBuilder::new().and_then(|b| b.match_subsystem("hidraw")).and_then(|b| b.listen()) {
                Ok(s) => {
                    let _ = ready_tx.send(Ok(()));
                    s
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                    return;
                }
            };
        loop {
            let mut pfd = libc::pollfd { fd: socket.as_raw_fd(), events: libc::POLLIN, revents: 0 };
            // SAFETY: pfd is one valid pollfd.
            let r = unsafe { libc::poll(&mut pfd, 1, -1) };
            if r < 0 {
                let e = std::io::Error::last_os_error();
                if e.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                // Any other failure would repeat at once, for ever.
                warn!("udev monitor stopped, so plugging and unplugging will not be noticed: {e}");
                return;
            }
            for event in socket.iter() {
                let Some(node) = event.devnode().map(Path::to_path_buf) else { continue };
                let msg = match event.event_type() {
                    udev::EventType::Add => match hidpp_hidraw::info(&node) {
                        Some(info) if info.is_usable() => {
                            debug!("hidraw added: {}", node.display());
                            Event::Added(info)
                        }
                        _ => continue,
                    },
                    udev::EventType::Remove => Event::Removed(node),
                    _ => continue,
                };
                if tx.send(msg).is_err() {
                    return;
                }
            }
        }
    })?;
    ready_rx.recv().unwrap_or_else(|_| Err(std::io::Error::other("udev thread exited")))
}

/// Watches the config directory, including subdirectories for included files,
/// and reports any change to a `.toml` file. Reloading is cheap and does
/// nothing when the merged config is unchanged. The watcher must be kept alive.
pub fn watch_config(path: PathBuf, tx: UnboundedSender<Event>) -> notify::Result<notify::RecommendedWatcher> {
    let dir = path.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    if let Err(e) = std::fs::create_dir_all(&dir) {
        warn!("cannot create {}: {e}", dir.display());
    }
    // A config that is a symlink changes where it points, outside this directory.
    let outside = std::fs::canonicalize(&path)
        .ok()
        .filter(|real| std::fs::canonicalize(&dir).is_ok_and(|d| !real.starts_with(d)));
    let linked = outside.clone();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let Ok(ev) = res else { return };
        if matches!(ev.kind, EventKind::Access(_)) {
            return;
        }
        if ev.paths.iter().any(|p| p.extension().is_some_and(|e| e == "toml") || Some(p) == linked.as_ref()) {
            let _ = tx.send(Event::ConfigChanged);
        }
    })?;
    watcher.watch(&dir, RecursiveMode::Recursive)?;
    if let Some(real_dir) = outside.as_deref().and_then(Path::parent)
        && let Err(e) = watcher.watch(real_dir, RecursiveMode::NonRecursive)
    {
        warn!("cannot watch {}: {e}", real_dir.display());
    }
    Ok(watcher)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn symlinked_config_is_watched_where_it_points() {
        let dir = std::env::temp_dir().join(format!("familiar-watch-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("dotfiles")).unwrap();
        std::fs::create_dir_all(dir.join("config")).unwrap();
        let real = dir.join("dotfiles/familiar.conf");
        std::fs::write(&real, "").unwrap();
        let link = dir.join("config/config.toml");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let _watcher = watch_config(link, tx).unwrap();
        std::fs::write(&real, "version = 1\n").unwrap();
        let seen = (0..100).any(|_| {
            thread::sleep(Duration::from_millis(20));
            matches!(rx.try_recv(), Ok(Event::ConfigChanged))
        });
        assert!(seen, "a change to the file behind the link is reported");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
