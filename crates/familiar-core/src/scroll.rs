//! Main wheel and thumb wheel behaviour.
//!
//! ```toml
//! [profile.default.scroll]
//! smooth = true            # hi-res scrolling through the daemon
//! speed = 1.5
//!
//! [profile.default.thumb-wheel]
//! mode = "volume"          # hscroll, volume, zoom, tabs or none
//! tap = "key:playpause"
//! ```
//!
//! Direction is set once, with the `wheel-invert` and `thumb-invert` device
//! settings, so it is never inverted twice.

use serde::{Deserialize, Serialize};

use crate::actions::Action;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Scroll {
    /// Take over the wheel and send hi-res scroll events. Off leaves the wheel to the system.
    pub smooth: Option<bool>,
    /// Multiplies scroll distance; 1 is the mouse's own speed.
    pub speed: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThumbMode {
    /// Horizontal scrolling, as without the daemon but with speed.
    Hscroll,
    /// Volume up and down, one step per notch.
    Volume,
    /// Control plus the main wheel, which zooms in most apps.
    Zoom,
    /// Next and previous tab with Control+Tab and Control+Shift+Tab.
    Tabs,
    None,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ThumbWheel {
    pub mode: Option<ThumbMode>,
    pub speed: Option<f32>,
    /// Action for a short touch of the thumb wheel without turning it.
    pub tap: Option<Action>,
}

impl Scroll {
    pub fn merge_from(&mut self, o: &Scroll) {
        if o.smooth.is_some() {
            self.smooth = o.smooth;
        }
        if o.speed.is_some() {
            self.speed = o.speed;
        }
    }

    pub fn smooth(&self) -> bool {
        self.smooth.unwrap_or(false)
    }

    pub fn speed(&self) -> f32 {
        clamp_speed(self.speed)
    }
}

impl ThumbWheel {
    pub fn merge_from(&mut self, o: &ThumbWheel) {
        if o.mode.is_some() {
            self.mode = o.mode;
        }
        if o.speed.is_some() {
            self.speed = o.speed;
        }
        if o.tap.is_some() {
            self.tap = o.tap.clone();
        }
    }

    pub fn mode(&self) -> ThumbMode {
        self.mode.unwrap_or(ThumbMode::Hscroll)
    }

    pub fn speed(&self) -> f32 {
        clamp_speed(self.speed)
    }
}

/// The speed to use: 1 when unset or not a number, kept between 0.1 and 10.
fn clamp_speed(speed: Option<f32>) -> f32 {
    speed.filter(|s| !s.is_nan()).unwrap_or(1.0).clamp(0.1, 10.0)
}

/// Resolved wheel behaviour for one profile.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Wheels {
    pub scroll: Scroll,
    /// `None` leaves the thumb wheel to the system.
    pub thumb: Option<ThumbWheel>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_stays_a_usable_number() {
        let speed = |text: &str| toml::from_str::<Scroll>(text).unwrap().speed();
        assert_eq!(speed(""), 1.0);
        assert_eq!(speed("speed = 2.5"), 2.5);
        assert_eq!(speed("speed = 0"), 0.1);
        assert_eq!(speed("speed = inf"), 10.0);
        assert_eq!(speed("speed = nan"), 1.0);
        assert_eq!(toml::from_str::<ThumbWheel>("speed = nan").unwrap().speed(), 1.0);
    }
}
