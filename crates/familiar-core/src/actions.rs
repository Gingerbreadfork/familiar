//! What a button can do, written as short strings in the config file.
//!
//! ```toml
//! [profile.default.buttons]
//! back = "key:alt+left"                    # acts like a key while held
//! haptic = { tap = "key:super", hold = "haptic:completed" }
//! middle = { tap = "mouse:middle", up = "key:super+page_up", down = "key:super+page_down" }
//! ```

use std::fmt;

use hidpp::features::haptic::Waveform;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

impl MouseButton {
    pub fn name(self) -> &'static str {
        match self {
            MouseButton::Left => "left",
            MouseButton::Right => "right",
            MouseButton::Middle => "middle",
            MouseButton::Back => "back",
            MouseButton::Forward => "forward",
        }
    }

    /// The evdev button this maps to.
    pub fn evdev(self) -> &'static str {
        match self {
            MouseButton::Left => "BTN_LEFT",
            MouseButton::Right => "BTN_RIGHT",
            MouseButton::Middle => "BTN_MIDDLE",
            MouseButton::Back => "BTN_SIDE",
            MouseButton::Forward => "BTN_EXTRA",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "left" => MouseButton::Left,
            "right" => MouseButton::Right,
            "middle" => MouseButton::Middle,
            "back" => MouseButton::Back,
            "forward" => MouseButton::Forward,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    None,
    /// A shortcut that follows the button: modifiers stay down while it is
    /// held and the other keys are pressed once. Keys as evdev names such as
    /// `KEY_LEFTCTRL`.
    Keys(Vec<String>),
    /// A shortcut pressed and let go at once, whatever the button does next.
    Press(Vec<String>),
    Mouse(MouseButton),
    /// Run through `sh -c`, with the button and gesture in the environment.
    Shell(String),
    Haptic(Waveform),
    Dpi(u16),
    /// Step through these DPI values, one per trigger.
    DpiCycle(Vec<u16>),
    /// Use this DPI while the button is held.
    DpiShift(u16),
    SmartShiftToggle,
    /// Easy-Switch channel, numbered from 1.
    Host(u8),
}

impl Action {
    /// Actions that have a pressed and a released state follow the button.
    pub fn is_held(&self) -> bool {
        matches!(self, Action::Keys(_) | Action::Mouse(_) | Action::DpiShift(_))
    }
}

/// Whether an evdev key name is Ctrl, Shift, Alt, AltGr or Super.
pub fn is_modifier(name: &str) -> bool {
    matches!(
        name,
        "KEY_LEFTCTRL"
            | "KEY_RIGHTCTRL"
            | "KEY_LEFTSHIFT"
            | "KEY_RIGHTSHIFT"
            | "KEY_LEFTALT"
            | "KEY_RIGHTALT"
            | "KEY_LEFTMETA"
            | "KEY_RIGHTMETA"
    )
}

/// Friendly key names and the evdev names they stand for.
const KEY_ALIASES: &[(&str, &str)] = &[
    ("ctrl", "KEY_LEFTCTRL"),
    ("control", "KEY_LEFTCTRL"),
    ("shift", "KEY_LEFTSHIFT"),
    ("alt", "KEY_LEFTALT"),
    ("altgr", "KEY_RIGHTALT"),
    ("super", "KEY_LEFTMETA"),
    ("meta", "KEY_LEFTMETA"),
    ("win", "KEY_LEFTMETA"),
    ("enter", "KEY_ENTER"),
    ("return", "KEY_ENTER"),
    ("esc", "KEY_ESC"),
    ("escape", "KEY_ESC"),
    ("tab", "KEY_TAB"),
    ("space", "KEY_SPACE"),
    ("backspace", "KEY_BACKSPACE"),
    ("delete", "KEY_DELETE"),
    ("insert", "KEY_INSERT"),
    ("home", "KEY_HOME"),
    ("end", "KEY_END"),
    ("pageup", "KEY_PAGEUP"),
    ("page_up", "KEY_PAGEUP"),
    ("pagedown", "KEY_PAGEDOWN"),
    ("page_down", "KEY_PAGEDOWN"),
    ("up", "KEY_UP"),
    ("down", "KEY_DOWN"),
    ("left", "KEY_LEFT"),
    ("right", "KEY_RIGHT"),
    ("print", "KEY_SYSRQ"),
    ("printscreen", "KEY_SYSRQ"),
    ("volumeup", "KEY_VOLUMEUP"),
    ("volume_up", "KEY_VOLUMEUP"),
    ("volumedown", "KEY_VOLUMEDOWN"),
    ("volume_down", "KEY_VOLUMEDOWN"),
    ("mute", "KEY_MUTE"),
    ("micmute", "KEY_MICMUTE"),
    ("playpause", "KEY_PLAYPAUSE"),
    ("play_pause", "KEY_PLAYPAUSE"),
    ("next", "KEY_NEXTSONG"),
    ("previous", "KEY_PREVIOUSSONG"),
    ("stop", "KEY_STOPCD"),
    ("brightnessup", "KEY_BRIGHTNESSUP"),
    ("brightnessdown", "KEY_BRIGHTNESSDOWN"),
    ("back", "KEY_BACK"),
    ("forward", "KEY_FORWARD"),
    ("calculator", "KEY_CALC"),
    ("minus", "KEY_MINUS"),
    ("equal", "KEY_EQUAL"),
    ("comma", "KEY_COMMA"),
    ("dot", "KEY_DOT"),
    ("slash", "KEY_SLASH"),
    ("semicolon", "KEY_SEMICOLON"),
    ("apostrophe", "KEY_APOSTROPHE"),
    ("grave", "KEY_GRAVE"),
    ("leftbrace", "KEY_LEFTBRACE"),
    ("rightbrace", "KEY_RIGHTBRACE"),
    ("backslash", "KEY_BACKSLASH"),
];

/// Resolves one key name: an alias, a single letter or digit, `f1` to `f24`,
/// or an evdev name such as `KEY_ZOOM` passed through as written.
fn key_name(s: &str) -> Option<String> {
    if s.starts_with("KEY_") || s.starts_with("BTN_") {
        return Some(s.to_string());
    }
    let lower = s.to_ascii_lowercase();
    if let Some((_, k)) = KEY_ALIASES.iter().find(|(a, _)| *a == lower) {
        return Some(k.to_string());
    }
    if lower.len() == 1 && lower.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Some(format!("KEY_{}", lower.to_ascii_uppercase()));
    }
    if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<u8>().ok())
        && (1..=24).contains(&n)
    {
        return Some(format!("KEY_F{n}"));
    }
    None
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct ActionError(String);

fn dpi(s: &str) -> Result<u16, ActionError> {
    s.trim().parse().map_err(|_| ActionError(format!("expected a DPI value, got {s:?}")))
}

impl std::str::FromStr for Action {
    type Err = ActionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "none" {
            return Ok(Action::None);
        }
        if s == "smartshift:toggle" {
            return Ok(Action::SmartShiftToggle);
        }
        let (kind, arg) = s.split_once(':').ok_or_else(|| ActionError(format!("{s:?} is not an action")))?;
        match kind {
            "key" | "press" => {
                let keys = arg
                    .split('+')
                    .map(|k| key_name(k.trim()).ok_or_else(|| ActionError(format!("unknown key {k:?}"))))
                    .collect::<Result<Vec<_>, _>>()?;
                if keys.is_empty() {
                    return Err(ActionError(format!("{kind}: needs at least one key")));
                }
                Ok(if kind == "key" { Action::Keys(keys) } else { Action::Press(keys) })
            }
            "mouse" => MouseButton::parse(arg)
                .map(Action::Mouse)
                .ok_or_else(|| ActionError(format!("unknown mouse button {arg:?}"))),
            "shell" if !arg.trim().is_empty() => Ok(Action::Shell(arg.to_string())),
            "haptic" => Waveform::from_name(arg)
                .map(Action::Haptic)
                .ok_or_else(|| ActionError(format!("unknown waveform {arg:?}"))),
            "dpi" => Ok(Action::Dpi(dpi(arg)?)),
            "dpi-shift" => Ok(Action::DpiShift(dpi(arg)?)),
            "dpi-cycle" => {
                let list = arg.split(',').map(dpi).collect::<Result<Vec<_>, _>>()?;
                if list.len() < 2 {
                    return Err(ActionError("dpi-cycle: needs at least two values".into()));
                }
                Ok(Action::DpiCycle(list))
            }
            "host" => match arg.parse::<u8>() {
                Ok(n @ 1..=3) => Ok(Action::Host(n)),
                _ => Err(ActionError(format!("host: expects 1, 2 or 3, got {arg:?}"))),
            },
            _ => Err(ActionError(format!("{s:?} is not an action"))),
        }
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Action::None => f.write_str("none"),
            Action::Keys(k) => write!(f, "key:{}", k.join("+")),
            Action::Press(k) => write!(f, "press:{}", k.join("+")),
            Action::Mouse(b) => write!(f, "mouse:{}", b.name()),
            Action::Shell(c) => write!(f, "shell:{c}"),
            Action::Haptic(w) => write!(f, "haptic:{}", w.name()),
            Action::Dpi(d) => write!(f, "dpi:{d}"),
            Action::DpiCycle(l) => {
                write!(f, "dpi-cycle:{}", l.iter().map(u16::to_string).collect::<Vec<_>>().join(","))
            }
            Action::DpiShift(d) => write!(f, "dpi-shift:{d}"),
            Action::SmartShiftToggle => f.write_str("smartshift:toggle"),
            Action::Host(n) => write!(f, "host:{n}"),
        }
    }
}

impl Serialize for Action {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Action {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// The directions a held button can be moved in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
    UpLeft,
    UpRight,
    DownLeft,
    DownRight,
}

/// Actions for one button, by gesture.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Gestures {
    /// Press once to hold the action down, press again to let it go. For
    /// anyone who finds holding a button hard. Cannot be combined with others.
    pub toggle: Option<Action>,
    pub tap: Option<Action>,
    pub double: Option<Action>,
    pub hold: Option<Action>,
    pub up: Option<Action>,
    pub down: Option<Action>,
    pub left: Option<Action>,
    pub right: Option<Action>,
    pub up_left: Option<Action>,
    pub up_right: Option<Action>,
    pub down_left: Option<Action>,
    pub down_right: Option<Action>,
}

impl Gestures {
    pub fn direction(&self, d: Direction) -> Option<&Action> {
        match d {
            Direction::Up => self.up.as_ref(),
            Direction::Down => self.down.as_ref(),
            Direction::Left => self.left.as_ref(),
            Direction::Right => self.right.as_ref(),
            Direction::UpLeft => self.up_left.as_ref(),
            Direction::UpRight => self.up_right.as_ref(),
            Direction::DownLeft => self.down_left.as_ref(),
            Direction::DownRight => self.down_right.as_ref(),
        }
    }

    pub fn has_directions(&self) -> bool {
        self.up.is_some() || self.down.is_some() || self.left.is_some() || self.right.is_some() || self.has_diagonals()
    }

    /// Explains why these gestures cannot work together, if they cannot.
    pub fn conflict(&self) -> Option<&'static str> {
        let others = self.tap.is_some() || self.double.is_some() || self.hold.is_some() || self.has_directions();
        (self.toggle.is_some() && others).then_some("toggle cannot be combined with other gestures")
    }

    pub fn has_diagonals(&self) -> bool {
        self.up_left.is_some() || self.up_right.is_some() || self.down_left.is_some() || self.down_right.is_some()
    }
}

/// What one button does: either acts as a single action that follows the
/// button, or recognises gestures.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Binding {
    Direct(Action),
    Gestures(Box<Gestures>),
}

impl Binding {
    /// Whether the device has to report pointer movement while this button is held.
    pub fn needs_motion(&self) -> bool {
        matches!(self, Binding::Gestures(g) if g.has_directions())
    }
}

impl Serialize for Binding {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Binding::Direct(a) => a.serialize(s),
            Binding::Gestures(g) => g.serialize(s),
        }
    }
}

impl<'de> Deserialize<'de> for Binding {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Direct(String),
            Gestures(Box<Gestures>),
        }
        match Raw::deserialize(d)? {
            Raw::Direct(s) => s.parse().map(Binding::Direct).map_err(serde::de::Error::custom),
            Raw::Gestures(g) => Ok(Binding::Gestures(g)),
        }
    }
}

/// Button names usable in `[profile.<name>.buttons]`.
pub const BUTTONS: &[&str] = &["middle", "back", "forward", "gesture", "smartshift", "haptic"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_actions() {
        let a: Action = "key:ctrl+shift+t".parse().unwrap();
        assert_eq!(a, Action::Keys(vec!["KEY_LEFTCTRL".into(), "KEY_LEFTSHIFT".into(), "KEY_T".into()]));
        assert_eq!("key:super+page_up".parse::<Action>().unwrap().to_string(), "key:KEY_LEFTMETA+KEY_PAGEUP");
        assert_eq!("key:f24".parse::<Action>().unwrap(), Action::Keys(vec!["KEY_F24".into()]));
        assert_eq!("key:KEY_ZOOM".parse::<Action>().unwrap(), Action::Keys(vec!["KEY_ZOOM".into()]));
        let once: Action = "press:alt+tab".parse().unwrap();
        assert_eq!(once, Action::Press(vec!["KEY_LEFTALT".into(), "KEY_TAB".into()]));
        assert_eq!(once.to_string(), "press:KEY_LEFTALT+KEY_TAB");
        assert!(!once.is_held());
        assert!("press:".parse::<Action>().is_err());
        assert_eq!("mouse:back".parse::<Action>().unwrap(), Action::Mouse(MouseButton::Back));
        assert_eq!("dpi-cycle:800,1600".parse::<Action>().unwrap(), Action::DpiCycle(vec![800, 1600]));
        assert_eq!("host:2".parse::<Action>().unwrap(), Action::Host(2));
        assert_eq!("shell:echo hi:there".parse::<Action>().unwrap(), Action::Shell("echo hi:there".into()));
        for bad in ["key:", "key:notakey", "mouse:side", "host:4", "dpi:lots", "dpi-cycle:800", "frob:1", "shell:"] {
            assert!(bad.parse::<Action>().is_err(), "{bad} should not parse");
        }
    }

    #[test]
    fn parses_bindings() {
        #[derive(Deserialize)]
        struct T {
            buttons: std::collections::BTreeMap<String, Binding>,
        }
        let t: T = toml::from_str(
            r#"
            [buttons]
            back = "key:alt+left"
            haptic = { tap = "key:super", hold = "haptic:completed", up = "key:volumeup" }
            "#,
        )
        .unwrap();
        assert!(matches!(&t.buttons["back"], Binding::Direct(Action::Keys(k)) if k.len() == 2));
        let Binding::Gestures(g) = &t.buttons["haptic"] else { panic!("expected gestures") };
        assert!(g.has_directions() && !g.has_diagonals());
        assert!(t.buttons["haptic"].needs_motion());
        assert!(toml::from_str::<T>("[buttons]\nback = { tapp = \"none\" }").is_err());
        assert!(toml::from_str::<T>("[buttons]\nback = \"key:nope\"").is_err());
    }
}
