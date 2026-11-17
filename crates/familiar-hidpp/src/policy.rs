//! Which feature functions this library is willing to call.
//!
//! Devices expose internal features for manufacturing, firmware update and
//! debugging. Calling the wrong function on one of those can change persistent
//! state, so every request is checked against an allowlist of functions whose
//! effect is understood.

use crate::Error;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Policy {
    /// Only allowlisted functions.
    #[default]
    Normal,
    /// Also function 0 of unknown features that are not flagged hidden or
    /// internal, for protocol exploration.
    Research,
}

/// A feature this library knows, with a bitmask of the functions it may call.
pub struct KnownFeature {
    pub id: u16,
    pub name: &'static str,
    allowed: u16,
}

const fn f(id: u16, name: &'static str, functions: &[u8]) -> KnownFeature {
    let mut allowed = 0u16;
    let mut i = 0;
    while i < functions.len() {
        allowed |= 1 << functions[i];
        i += 1;
    }
    KnownFeature { id, name, allowed }
}

pub const KNOWN: &[KnownFeature] = &[
    f(0x0000, "Root", &[0, 1]),
    f(0x0001, "Feature Set", &[0, 1]),
    f(0x0003, "Device FW Version", &[0, 1, 2]),
    f(0x0005, "Device Name", &[0, 1, 2]),
    f(0x0007, "Device Friendly Name", &[0, 1, 2]),
    f(0x0020, "Config Change", &[]),
    f(0x0021, "Crypto ID", &[]),
    f(0x1004, "Unified Battery", &[0, 1]),
    f(0x1814, "Change Host", &[0, 1]),
    f(0x1815, "Hosts Info", &[0, 1, 3, 4]),
    f(0x19B0, "Haptic", &[0, 1, 2, 4]),
    f(0x19C0, "Force Sensing Button", &[0, 1, 2, 3]),
    f(0x1B04, "Reprog Controls v4", &[0, 1, 2, 3]),
    f(0x1D4B, "Wireless Device Status", &[]),
    f(0x2111, "SmartShift Enhanced", &[0, 1, 2]),
    f(0x2121, "HiRes Wheel", &[0, 1, 2, 3]),
    f(0x2150, "Thumb Wheel", &[0, 1, 2]),
    f(0x2201, "Adjustable DPI", &[0, 1, 2, 3]),
    f(0x2250, "XY Stats", &[]),
    f(0x2251, "Wheel Stats", &[]),
    f(0x1802, "Device Reset", &[]),
    f(0x1816, "BLE Pro Pre-pairing", &[]),
    f(0x1805, "OOB State", &[]),
    f(0x1E00, "Enable Hidden Features", &[]),
    f(0x0011, "Property Access", &[]),
];

/// Firmware update features. Never touched, not even in research mode.
fn is_firmware_update(id: u16) -> bool {
    (0x00C0..=0x00DF).contains(&id)
}

pub fn known(id: u16) -> Option<&'static KnownFeature> {
    KNOWN.iter().find(|k| k.id == id)
}

pub fn name(id: u16) -> Option<&'static str> {
    known(id).map(|k| k.name)
}

/// Feature flags reported by Feature Set `getFeatureId`.
pub mod flags {
    pub const OBSOLETE: u8 = 0x80;
    pub const HIDDEN: u8 = 0x40;
    pub const INTERNAL: u8 = 0x20;
}

pub fn check(policy: Policy, id: u16, feature_flags: u8, function: u8) -> Result<(), Error> {
    let deny = |reason| Err(Error::Denied { feature: id, function, reason });
    if function > 15 {
        return deny("function number out of range");
    }
    if let Some(k) = known(id) {
        return if k.allowed & (1 << function) != 0 { Ok(()) } else { deny("function not allowlisted") };
    }
    if policy == Policy::Normal {
        return deny("unknown feature");
    }
    if is_firmware_update(id) {
        return deny("firmware update feature");
    }
    if feature_flags & (flags::HIDDEN | flags::INTERNAL) != 0 {
        return deny("hidden or internal feature");
    }
    if function != 0 {
        return deny("research mode only calls function 0");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_known_functions_only() {
        assert!(check(Policy::Normal, 0x19B0, 0, 4).is_ok());
        assert!(check(Policy::Normal, 0x19B0, 0, 3).is_err());
        assert!(check(Policy::Normal, 0x1E00, 0, 0).is_err());
        assert!(check(Policy::Research, 0x1E00, 0, 0).is_err());
    }

    #[test]
    fn research_mode_is_narrow() {
        assert!(check(Policy::Normal, 0x1701, 0, 0).is_err());
        assert!(check(Policy::Research, 0x1701, 0, 0).is_ok());
        assert!(check(Policy::Research, 0x1701, 0, 1).is_err());
        assert!(check(Policy::Research, 0x1830, flags::INTERNAL, 0).is_err());
        assert!(check(Policy::Research, 0x00D1, 0, 0).is_err());
    }
}
