//! A single text file with what is needed to understand a bug report.
//! Nothing is sent anywhere; the user reads the file and shares it.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{fs, time};

use anyhow::{Context, Result};
use familiar_core::config::Config;
use hidpp::Policy;

use crate::{daemon, discover, dump, runtime};

fn section(out: &mut String, title: &str) {
    let _ = write!(out, "\n## {title}\n\n");
}

fn command(out: &mut String, program: &str, args: &[&str]) {
    match Command::new(program).args(args).output() {
        Ok(o) => {
            out.push_str(&String::from_utf8_lossy(&o.stdout));
            out.push_str(&String::from_utf8_lossy(&o.stderr));
        }
        Err(e) => {
            let _ = writeln!(out, "({program} not available: {e})");
        }
    }
}

fn file(out: &mut String, path: &Path) {
    match fs::read_to_string(path) {
        Ok(t) => {
            let _ = writeln!(out, "# {}\n{t}", path.display());
        }
        Err(e) => {
            let _ = writeln!(out, "# {}: {e}", path.display());
        }
    }
}

fn os_release() -> String {
    fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|t| t.lines().find_map(|l| l.strip_prefix("PRETTY_NAME=").map(|v| v.trim_matches('"').to_string())))
        .unwrap_or_else(|| "unknown".into())
}

async fn daemon_view(out: &mut String) -> Result<bool> {
    let Some(d) = daemon::connect().await else { return Ok(false) };
    let m = d.manager().await?;
    let _ = writeln!(out, "familiard {}, config {}", m.version().await?, m.config_path().await?);
    let _ = writeln!(out, "conflicts: {:?}", m.conflicts().await?);
    for dev in d.devices().await? {
        let _ = writeln!(out, "\n{} ({})", dev.name().await?, dev.unit_id().await?);
        let _ = writeln!(out, "  endpoint {} index {}", dev.endpoint().await?, dev.index().await?);
        let _ = writeln!(out, "  online {}", dev.online().await?);
        let _ = writeln!(out, "  battery {}% {}", dev.battery_percent().await?, dev.battery_status().await?);
        let _ = writeln!(out, "  settings {:?}", daemon::settings(&dev).await?);
        let _ = writeln!(out, "  configured {:?}", daemon::configured(&dev).await?);
        let _ = writeln!(out, "  apply errors {:?}", dev.apply_errors().await?);
        let _ = writeln!(out, "  last applied {}", daemon::ago(dev.last_applied().await?));
        let _ = writeln!(out, "  last reconnect {} ms", dev.reconnect_ms().await?);
    }
    Ok(true)
}

/// Writes the report and returns where it went.
pub fn write(path: Option<&Path>, hidraw: Option<&Path>, index: Option<u8>) -> Result<PathBuf> {
    let mut out = String::new();
    let now = time::SystemTime::now().duration_since(time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let _ = writeln!(out, "# Familiar diagnostics\n");
    let _ = writeln!(out, "familiar {}", env!("CARGO_PKG_VERSION"));
    let _ = writeln!(out, "created at unix time {now}");
    let _ = writeln!(out, "system {}", os_release());
    command(&mut out, "uname", &["-srm"]);
    let _ = writeln!(out, "desktop {}", std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default());
    let _ = writeln!(out, "session {}", std::env::var("XDG_SESSION_TYPE").unwrap_or_default());

    section(&mut out, "HID++ endpoints");
    match hidpp_hidraw::enumerate() {
        Ok(list) if list.is_empty() => out.push_str("none found\n"),
        Ok(list) => {
            for i in list {
                let _ = writeln!(
                    out,
                    "{}  bus {:02x}  {:04x}:{:04x}  driver {}  reports {:?}  {}",
                    i.path.display(),
                    i.bus,
                    i.vendor,
                    i.product,
                    i.driver,
                    i.kinds,
                    i.name
                );
            }
        }
        Err(e) => {
            let _ = writeln!(out, "cannot list hidraw devices: {e}");
        }
    }

    section(&mut out, "Device");
    match discover::select(hidraw, index, Policy::Normal) {
        Ok(mut sel) => match dump::collect(&mut sel.device, sel.path.display().to_string()) {
            Ok(d) => {
                // One compact line per field keeps long lists such as DPI readable.
                for (key, value) in serde_json::to_value(&d)?.as_object().into_iter().flatten() {
                    let _ = writeln!(out, "{key}: {value}");
                }
            }
            Err(e) => {
                let _ = writeln!(out, "dump failed: {e}");
            }
        },
        Err(e) => {
            let _ = writeln!(out, "{e}");
        }
    }
    out.push('\n');

    section(&mut out, "Daemon");
    let rt = runtime()?;
    let mut view = String::new();
    match rt.block_on(daemon_view(&mut view)) {
        Ok(true) => out.push_str(&view),
        Ok(false) => out.push_str("familiard is not running\n"),
        Err(e) => {
            let _ = writeln!(out, "{view}\nerror talking to familiard: {e}");
        }
    }
    command(&mut out, "systemctl", &["--user", "--no-pager", "status", "familiard.service"]);

    section(&mut out, "Config");
    let config_path = Config::default_path();
    file(&mut out, &config_path);
    if let Ok(c) = Config::load(&config_path) {
        for inc in &c.included {
            file(&mut out, inc);
        }
    }

    section(&mut out, "Recent daemon log");
    command(
        &mut out,
        "journalctl",
        &["--user", "-u", "familiard.service", "-n", "200", "--no-pager", "-o", "short-iso"],
    );

    let path = path.map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from(format!("familiar-diagnostics-{now}.txt")));
    fs::write(&path, out).with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}
