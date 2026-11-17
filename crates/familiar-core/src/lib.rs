//! Settings and configuration shared by `familiard` and `familiar`.

pub mod actions;
pub mod config;
pub mod diversion;
pub mod gesture;
pub mod scroll;
pub mod settings;

/// D-Bus names. The daemon owns `BUS_NAME` on the session bus.
pub mod dbus {
    pub const BUS_NAME: &str = "io.github.gingerbreadfork.Familiar";
    pub const MANAGER_PATH: &str = "/io/github/gingerbreadfork/Familiar";
    pub const MANAGER_IFACE: &str = "io.github.gingerbreadfork.Familiar.Manager1";
    pub const DEVICE_IFACE: &str = "io.github.gingerbreadfork.Familiar.Device1";
    pub const DEVICES_PREFIX: &str = "/io/github/gingerbreadfork/Familiar/devices";

    /// Object path for a device, from its unit ID or another stable key.
    pub fn device_path(key: &str) -> String {
        let clean: String = key.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
        format!("{DEVICES_PREFIX}/d{clean}")
    }
}
