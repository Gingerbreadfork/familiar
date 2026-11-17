//! SmartShift Enhanced (`0x2111`): ratchet versus free spin, the speed at which
//! the wheel shifts to free spin, and the ratchet torque.

use crate::{Error, Requester};

pub const ID: u16 = 0x2111;

/// Auto-disengage value meaning "never shift to free spin".
pub const ALWAYS_RATCHET: u8 = 0xFF;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
pub enum WheelMode {
    #[cfg_attr(feature = "serde", serde(rename = "free", alias = "free-spin"))]
    FreeSpin,
    Ratchet,
}

impl WheelMode {
    fn to_byte(self) -> u8 {
        match self {
            WheelMode::FreeSpin => 1,
            WheelMode::Ratchet => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Capabilities {
    pub tunable_torque: bool,
    pub default_auto_disengage: u8,
    pub default_torque: u8,
}

pub fn capabilities(dev: &mut impl Requester) -> Result<Capabilities, Error> {
    let p = dev.call(ID, 0x0, &[])?;
    Ok(Capabilities { tunable_torque: p[0] & 0x01 != 0, default_auto_disengage: p[1], default_torque: p[2] })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct State {
    pub mode: WheelMode,
    /// Wheel speed that shifts to free spin, or [`ALWAYS_RATCHET`].
    pub auto_disengage: u8,
    pub torque: u8,
}

pub fn get(dev: &mut impl Requester) -> Result<State, Error> {
    let p = dev.call(ID, 0x1, &[])?;
    let mode = if p[0] == 1 { WheelMode::FreeSpin } else { WheelMode::Ratchet };
    Ok(State { mode, auto_disengage: p[1], torque: p[2] })
}

/// Writes all three values together. The MX Master 4 ignores torque and
/// threshold changes unless the mode byte is sent as well.
pub fn set(dev: &mut impl Requester, state: State) -> Result<(), Error> {
    dev.call(ID, 0x2, &[state.mode.to_byte(), state.auto_disengage, state.torque.max(1)])?;
    Ok(())
}
