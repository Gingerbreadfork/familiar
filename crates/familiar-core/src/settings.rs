//! Device settings: parsing, reading from a device and applying to it.

use std::collections::BTreeMap;
use std::fmt;

use hidpp::features::{dpi, force_button, haptic, hires_wheel, smartshift, thumb_wheel};
use hidpp::{Error, Requester};
use serde::{Deserialize, Serialize};

pub use hidpp::features::smartshift::WheelMode;

/// Wheel speed at which SmartShift releases the ratchet, or never.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shift {
    Off,
    Speed(u8),
}

impl Shift {
    fn to_byte(self) -> u8 {
        match self {
            Shift::Off => smartshift::ALWAYS_RATCHET,
            Shift::Speed(s) => s.clamp(1, 254),
        }
    }

    fn from_byte(b: u8) -> Self {
        if b == smartshift::ALWAYS_RATCHET { Shift::Off } else { Shift::Speed(b) }
    }
}

impl fmt::Display for Shift {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Shift::Off => f.write_str("off"),
            Shift::Speed(s) => write!(f, "{s}"),
        }
    }
}

impl Serialize for Shift {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Shift::Off => s.serialize_str("off"),
            Shift::Speed(v) => s.serialize_u8(*v),
        }
    }
}

impl<'de> Deserialize<'de> for Shift {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Num(u8),
            Str(String),
        }
        match Raw::deserialize(d)? {
            Raw::Num(n) => Ok(Shift::Speed(n.clamp(1, 254))),
            Raw::Str(s) if s == "off" => Ok(Shift::Off),
            Raw::Str(s) => Err(serde::de::Error::custom(format!("expected a speed or \"off\", got {s:?}"))),
        }
    }
}

/// Every setting is optional: `None` means "leave the device as it is".
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct DeviceSettings {
    pub dpi: Option<u16>,
    /// Haptic strength in percent; 0 turns haptics off.
    pub haptic: Option<u8>,
    /// Thumb panel press threshold.
    pub force: Option<u16>,
    pub wheel: Option<WheelMode>,
    pub smartshift: Option<Shift>,
    pub torque: Option<u8>,
    pub wheel_invert: Option<bool>,
    pub thumb_invert: Option<bool>,
}

pub const NAMES: &[&str] = &["dpi", "haptic", "force", "wheel", "smartshift", "torque", "wheel-invert", "thumb-invert"];

#[derive(Debug, thiserror::Error)]
pub enum SettingError {
    #[error("unknown setting {0:?}; known settings are {list}", list = NAMES.join(", "))]
    Unknown(String),
    #[error("{name}: {msg}")]
    Value { name: &'static str, msg: String },
}

fn bad(name: &'static str, msg: impl Into<String>) -> SettingError {
    SettingError::Value { name, msg: msg.into() }
}

fn parse_bool(name: &'static str, s: &str) -> Result<bool, SettingError> {
    match s {
        "on" | "true" | "yes" | "1" => Ok(true),
        "off" | "false" | "no" | "0" => Ok(false),
        _ => Err(bad(name, format!("expected on or off, got {s:?}"))),
    }
}

fn parse_num<T: TryFrom<u64>>(name: &'static str, s: &str) -> Result<T, SettingError> {
    let n: u64 = s.parse().map_err(|_| bad(name, format!("expected a number, got {s:?}")))?;
    T::try_from(n).map_err(|_| bad(name, format!("{n} is out of range")))
}

fn on_off(b: bool) -> String {
    if b { "on".into() } else { "off".into() }
}

pub fn wheel_mode_name(m: WheelMode) -> &'static str {
    match m {
        WheelMode::Ratchet => "ratchet",
        WheelMode::FreeSpin => "free",
    }
}

impl DeviceSettings {
    /// Sets one setting from its command line form, e.g. `("smartshift", "off")`.
    pub fn set_str(&mut self, name: &str, value: &str) -> Result<&'static str, SettingError> {
        let name = *NAMES.iter().find(|n| **n == name).ok_or_else(|| SettingError::Unknown(name.into()))?;
        match name {
            "dpi" => self.dpi = Some(parse_num(name, value)?),
            "haptic" => self.haptic = Some(parse_num(name, value)?),
            "force" => self.force = Some(parse_num(name, value)?),
            "wheel" => {
                self.wheel = Some(match value {
                    "ratchet" => WheelMode::Ratchet,
                    "free" | "freespin" => WheelMode::FreeSpin,
                    _ => return Err(bad(name, "expected ratchet or free")),
                })
            }
            "smartshift" => {
                self.smartshift = Some(if value == "off" {
                    Shift::Off
                } else {
                    Shift::Speed(parse_num::<u8>(name, value)?.clamp(1, 254))
                })
            }
            "torque" => self.torque = Some(parse_num(name, value)?),
            "wheel-invert" => self.wheel_invert = Some(parse_bool(name, value)?),
            "thumb-invert" => self.thumb_invert = Some(parse_bool(name, value)?),
            _ => unreachable!(),
        }
        self.keep_in_range();
        Ok(name)
    }

    /// Brings haptic strength and torque within what the device takes.
    pub fn keep_in_range(&mut self) {
        self.haptic = self.haptic.map(|v| v.min(100));
        self.torque = self.torque.map(|v| v.clamp(1, 100));
    }

    /// The settings that are set, in their command line form.
    pub fn to_map(&self) -> BTreeMap<String, String> {
        let mut m = BTreeMap::new();
        let mut put = |k: &str, v: Option<String>| {
            if let Some(v) = v {
                m.insert(k.to_string(), v);
            }
        };
        put("dpi", self.dpi.map(|v| v.to_string()));
        put("haptic", self.haptic.map(|v| v.to_string()));
        put("force", self.force.map(|v| v.to_string()));
        put("wheel", self.wheel.map(|v| wheel_mode_name(v).to_string()));
        put("smartshift", self.smartshift.map(|v| v.to_string()));
        put("torque", self.torque.map(|v| v.to_string()));
        put("wheel-invert", self.wheel_invert.map(on_off));
        put("thumb-invert", self.thumb_invert.map(on_off));
        m
    }

    /// Overrides fields with those set in `other`.
    pub fn merge_from(&mut self, other: &DeviceSettings) {
        macro_rules! take {
            ($($f:ident),*) => { $( if other.$f.is_some() { self.$f = other.$f; } )* };
        }
        take!(dpi, haptic, force, wheel, smartshift, torque, wheel_invert, thumb_invert);
    }

    /// The fields of `self` that differ from `current`.
    pub fn diff(&self, current: &DeviceSettings) -> DeviceSettings {
        let mut out = DeviceSettings::default();
        macro_rules! cmp {
            ($($f:ident),*) => { $( if self.$f.is_some() && self.$f != current.$f { out.$f = self.$f; } )* };
        }
        cmp!(dpi, haptic, force, wheel, smartshift, torque, wheel_invert, thumb_invert);
        out
    }

    pub fn is_empty(&self) -> bool {
        *self == DeviceSettings::default()
    }
}

/// Reads every setting the device supports. Unsupported ones stay `None`.
pub fn read(dev: &mut impl Requester) -> DeviceSettings {
    let all = DeviceSettings {
        dpi: Some(0),
        haptic: Some(0),
        force: Some(0),
        wheel: Some(WheelMode::Ratchet),
        smartshift: Some(Shift::Off),
        torque: Some(0),
        wheel_invert: Some(false),
        thumb_invert: Some(false),
    };
    read_some(dev, &all)
}

/// Reads only the settings that are set in `which`, so applying a short config
/// costs a few requests rather than one per setting.
pub fn read_some(dev: &mut impl Requester, which: &DeviceSettings) -> DeviceSettings {
    let mut s = DeviceSettings::default();
    if which.dpi.is_some() {
        s.dpi = dpi::get(dev, 0).ok().map(|d| d.current);
    }
    if which.haptic.is_some()
        && let Ok(l) = haptic::level(dev)
    {
        s.haptic = Some(if l.enabled { l.percent } else { 0 });
    }
    if which.force.is_some() {
        s.force = force_button::get(dev, 0).ok().map(|b| b.current);
    }
    if (which.wheel.is_some() || which.smartshift.is_some() || which.torque.is_some())
        && let Ok(st) = smartshift::get(dev)
    {
        s.wheel = Some(st.mode);
        s.smartshift = Some(Shift::from_byte(st.auto_disengage));
        s.torque = Some(st.torque);
    }
    if which.wheel_invert.is_some() {
        s.wheel_invert = hires_wheel::mode(dev).ok().map(|m| m.inverted);
    }
    if which.thumb_invert.is_some() {
        s.thumb_invert = thumb_wheel::status(dev).ok().map(|t| t.inverted);
    }
    s
}

/// The outcome of [`apply`].
pub struct Applied {
    /// The wanted settings as the device has them now: what was read before,
    /// with successful writes on top. Settings not in `want` are not read.
    pub current: DeviceSettings,
    /// Settings that could not be written, with their errors.
    pub errors: Vec<(&'static str, Error)>,
}

/// Facts about a device that do not change, kept between applies so a
/// reconnect needs fewer requests.
#[derive(Default)]
pub struct Cache {
    dpi_list: Option<Vec<u16>>,
}

/// Writes the settings that differ from what the device has now. A failed
/// setting does not stop the others.
pub fn apply(dev: &mut impl Requester, want: &DeviceSettings) -> Applied {
    apply_cached(dev, want, &mut Cache::default())
}

pub fn apply_cached(dev: &mut impl Requester, want: &DeviceSettings, cache: &mut Cache) -> Applied {
    let mut current = read_some(dev, want);
    let todo = want.diff(&current);
    let mut errors = Vec::new();

    if let Some(v) = todo.dpi {
        let list = match cache.dpi_list.take() {
            Some(l) => Ok(l),
            None => dpi::list(dev, 0),
        };
        let r = list.and_then(|list| {
            let ok = list.is_empty() || list.contains(&v);
            cache.dpi_list = Some(list);
            if ok { dpi::set(dev, 0, v) } else { Err(Error::Invalid(format!("{v} DPI is not supported"))) }
        });
        match r {
            Ok(()) => current.dpi = Some(v),
            Err(e) => errors.push(("dpi", e)),
        }
    }
    if let Some(v) = todo.haptic {
        match haptic::set_level(dev, v) {
            Ok(()) => current.haptic = Some(v),
            Err(e) => errors.push(("haptic", e)),
        }
    }
    if let Some(v) = todo.force {
        match force_button::set(dev, 0, v) {
            Ok(()) => current.force = Some(v),
            Err(e) => errors.push(("force", e)),
        }
    }
    if todo.wheel.is_some() || todo.smartshift.is_some() || todo.torque.is_some() {
        // read_some read all three together, so the full state is known already.
        let known = match (current.wheel, current.smartshift, current.torque) {
            (Some(mode), Some(shift), Some(torque)) => {
                Ok(smartshift::State { mode, auto_disengage: shift.to_byte(), torque })
            }
            _ => smartshift::get(dev),
        };
        let r = known.and_then(|mut st| {
            if let Some(m) = todo.wheel {
                st.mode = m;
            }
            if let Some(s) = todo.smartshift {
                st.auto_disengage = s.to_byte();
            }
            if let Some(t) = todo.torque {
                st.torque = t.max(1);
            }
            smartshift::set(dev, st).map(|()| st)
        });
        match r {
            Ok(st) => {
                current.wheel = Some(st.mode);
                current.smartshift = Some(Shift::from_byte(st.auto_disengage));
                current.torque = Some(st.torque);
            }
            Err(e) => errors.push(("smartshift", e)),
        }
    }
    if let Some(v) = todo.wheel_invert {
        let r = hires_wheel::mode(dev).and_then(|m| hires_wheel::set_mode(dev, hires_wheel::Mode { inverted: v, ..m }));
        match r {
            Ok(()) => current.wheel_invert = Some(v),
            Err(e) => errors.push(("wheel-invert", e)),
        }
    }
    if let Some(v) = todo.thumb_invert {
        let r = thumb_wheel::status(dev).and_then(|t| thumb_wheel::set(dev, thumb_wheel::Status { inverted: v, ..t }));
        match r {
            Ok(()) => current.thumb_invert = Some(v),
            Err(e) => errors.push(("thumb-invert", e)),
        }
    }
    Applied { current, errors }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_formats() {
        let mut s = DeviceSettings::default();
        s.set_str("smartshift", "off").unwrap();
        s.set_str("wheel", "free").unwrap();
        s.set_str("thumb-invert", "on").unwrap();
        s.set_str("haptic", "250").unwrap();
        let m = s.to_map();
        assert_eq!(m["smartshift"], "off");
        assert_eq!(m["wheel"], "free");
        assert_eq!(m["thumb-invert"], "on");
        assert_eq!(m["haptic"], "100");
        s.set_str("torque", "0").unwrap();
        assert_eq!(s.torque, Some(1));
        assert!(s.set_str("colour", "red").is_err());
        assert!(s.set_str("dpi", "lots").is_err());
    }

    #[test]
    fn diff_only_keeps_changes() {
        let want = DeviceSettings { dpi: Some(1600), haptic: Some(50), ..Default::default() };
        let have = DeviceSettings { dpi: Some(1600), haptic: Some(100), torque: Some(10), ..Default::default() };
        assert_eq!(want.diff(&have), DeviceSettings { haptic: Some(50), ..Default::default() });
    }

    #[test]
    fn toml_round_trip() {
        let s: DeviceSettings = toml::from_str("dpi = 1200\nsmartshift = \"off\"\nwheel = \"ratchet\"\n").unwrap();
        assert_eq!(s.smartshift, Some(Shift::Off));
        assert_eq!(s.wheel, Some(WheelMode::Ratchet));
        let s: DeviceSettings = toml::from_str("smartshift = 12").unwrap();
        assert_eq!(s.smartshift, Some(Shift::Speed(12)));
        assert!(toml::from_str::<DeviceSettings>("nope = 1").is_err());
    }
}
