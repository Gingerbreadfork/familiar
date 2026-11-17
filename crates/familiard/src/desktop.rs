//! Desktop notifications and detection of other device configuration tools.

use std::collections::HashMap;
use std::fs;

use tracing::debug;
use zbus::Connection;
use zbus::Variant;

/// Shows a desktop notification. Failures are logged and otherwise ignored.
pub async fn notify(conn: &Connection, summary: &str, body: &str) {
    let hints: HashMap<&str, Variant<'_>> = HashMap::new();
    let r = conn
        .call_method(
            Some("org.freedesktop.Notifications"),
            "/org/freedesktop/Notifications",
            Some("org.freedesktop.Notifications"),
            "Notify",
            &("Familiar", 0u32, "input-mouse", summary, body, Vec::<&str>::new(), hints, -1i32),
        )
        .await;
    if let Err(e) = r {
        debug!("notification not shown: {e}");
    }
}

const OTHERS: &[(&str, &str)] = &[("solaar", "Solaar"), ("logid", "logiops"), ("mouser", "Mouser")];

/// Names of running programs that also configure Logitech devices.
pub fn conflicts() -> Vec<String> {
    let mut found = Vec::new();
    let Ok(dir) = fs::read_dir("/proc") else { return found };
    for entry in dir.flatten() {
        let name = entry.file_name();
        if !name.to_string_lossy().bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let Ok(cmdline) = fs::read(entry.path().join("cmdline")) else { continue };
        let args: Vec<String> = cmdline
            .split(|b| *b == 0)
            .take(3)
            .map(|a| String::from_utf8_lossy(a).rsplit('/').next().unwrap_or("").to_lowercase())
            .collect();
        for (needle, label) in OTHERS {
            // Python programs show up as the interpreter followed by the script.
            if args.iter().any(|a| a == needle) && !found.iter().any(|f| f == label) {
                found.push(label.to_string());
            }
        }
    }
    found.sort();
    found
}
