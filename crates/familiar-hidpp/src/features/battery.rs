//! Unified Battery (`0x1004`).

use crate::{Error, Requester};

pub const ID: u16 = 0x1004;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum Charging {
    Discharging,
    Recharging,
    AlmostFull,
    Full,
    SlowRecharge,
    InvalidBattery,
    ThermalError,
    Unknown(u8),
}

impl From<u8> for Charging {
    fn from(b: u8) -> Self {
        match b {
            0 => Charging::Discharging,
            1 => Charging::Recharging,
            2 => Charging::AlmostFull,
            3 => Charging::Full,
            4 => Charging::SlowRecharge,
            5 => Charging::InvalidBattery,
            6 => Charging::ThermalError,
            n => Charging::Unknown(n),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum Level {
    Critical,
    Low,
    Good,
    Full,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Status {
    /// State of charge in percent, when the device reports one.
    pub percent: Option<u8>,
    pub level: Level,
    pub charging: Charging,
    pub external_power: bool,
}

impl Status {
    /// Parses a `getStatus` reply or a battery event.
    pub fn parse(p: &[u8]) -> Self {
        let level = match p[1] {
            8 => Level::Full,
            4 => Level::Good,
            2 => Level::Low,
            1 => Level::Critical,
            _ => Level::Unknown,
        };
        Status {
            percent: (p[0] != 0).then_some(p[0]),
            level,
            charging: Charging::from(p[2]),
            external_power: p[3] != 0,
        }
    }

    pub fn is_charging(&self) -> bool {
        matches!(self.charging, Charging::Recharging | Charging::AlmostFull | Charging::SlowRecharge)
    }
}

pub fn status(dev: &mut impl Requester) -> Result<Status, Error> {
    Ok(Status::parse(&dev.call(ID, 0x1, &[])?))
}
