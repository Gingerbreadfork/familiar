//! HiRes Wheel (`0x2121`): resolution, inversion and diversion of the main wheel.

use super::be16i;
use crate::{Error, Requester};

pub const ID: u16 = 0x2121;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Capabilities {
    /// Hi-res counts per low-res detent.
    pub multiplier: u8,
    pub has_invert: bool,
    pub has_ratchet_switch: bool,
}

pub fn capabilities(dev: &mut impl Requester) -> Result<Capabilities, Error> {
    let p = dev.call(ID, 0x0, &[])?;
    Ok(Capabilities { multiplier: p[0], has_invert: p[1] & 0x08 != 0, has_ratchet_switch: p[1] & 0x04 != 0 })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Mode {
    /// Send wheel movement as HID++ events instead of HID reports.
    pub diverted: bool,
    pub hires: bool,
    pub inverted: bool,
}

impl Mode {
    fn from_byte(b: u8) -> Self {
        Mode { diverted: b & 0x01 != 0, hires: b & 0x02 != 0, inverted: b & 0x04 != 0 }
    }

    fn to_byte(self) -> u8 {
        self.diverted as u8 | (self.hires as u8) << 1 | (self.inverted as u8) << 2
    }
}

pub fn mode(dev: &mut impl Requester) -> Result<Mode, Error> {
    Ok(Mode::from_byte(dev.call(ID, 0x1, &[])?[0]))
}

pub fn set_mode(dev: &mut impl Requester, mode: Mode) -> Result<(), Error> {
    dev.call(ID, 0x2, &[mode.to_byte()])?;
    Ok(())
}

/// True when the wheel is in ratchet mode, false when it spins freely.
pub fn ratchet(dev: &mut impl Requester) -> Result<bool, Error> {
    Ok(dev.call(ID, 0x3, &[])?[0] & 0x01 != 0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Movement { hires: bool, periods: u8, delta: i16 },
    RatchetSwitch { ratchet: bool },
}

/// Parses an event from this feature, given the event number and its parameters.
pub fn parse_event(function: u8, p: &[u8]) -> Option<Event> {
    match function {
        0 => Some(Event::Movement { hires: p[0] & 0x10 != 0, periods: p[0] & 0x0F, delta: be16i(p, 1) }),
        1 => Some(Event::RatchetSwitch { ratchet: p[0] & 0x01 != 0 }),
        _ => None,
    }
}
