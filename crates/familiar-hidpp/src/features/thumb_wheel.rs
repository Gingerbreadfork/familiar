//! Thumb Wheel (`0x2150`).

use super::{be16, be16i};
use crate::{Error, Requester};

pub const ID: u16 = 0x2150;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Info {
    pub native_resolution: u16,
    pub diverted_resolution: u16,
    pub default_inverted: bool,
    pub has_timestamp: bool,
    pub has_touch: bool,
    pub has_proximity: bool,
    pub has_single_tap: bool,
    pub time_unit: u16,
}

pub fn info(dev: &mut impl Requester) -> Result<Info, Error> {
    let p = dev.call(ID, 0x0, &[])?;
    Ok(Info {
        native_resolution: be16(&p, 0),
        diverted_resolution: be16(&p, 2),
        default_inverted: p[4] & 0x01 != 0,
        has_timestamp: p[5] & 0x01 != 0,
        has_touch: p[5] & 0x02 != 0,
        has_proximity: p[5] & 0x04 != 0,
        has_single_tap: p[5] & 0x08 != 0,
        time_unit: be16(&p, 6),
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Status {
    pub diverted: bool,
    pub inverted: bool,
}

pub fn status(dev: &mut impl Requester) -> Result<Status, Error> {
    let p = dev.call(ID, 0x1, &[])?;
    Ok(Status { diverted: p[0] & 0x01 != 0, inverted: p[1] & 0x01 != 0 })
}

pub fn set(dev: &mut impl Requester, status: Status) -> Result<(), Error> {
    dev.call(ID, 0x2, &[status.diverted as u8, status.inverted as u8])?;
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rotation {
    Inactive,
    Start,
    Active,
    Stop,
    Unknown(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Event {
    pub delta: i16,
    pub timestamp: u16,
    pub rotation: Rotation,
    pub touch: bool,
    pub proximity: bool,
    pub single_tap: bool,
}

/// Event flag bits match the capability bits in [`Info`].
pub fn parse_event(p: &[u8]) -> Event {
    Event {
        delta: be16i(p, 0),
        timestamp: be16(p, 2),
        rotation: match p[4] {
            0 => Rotation::Inactive,
            1 => Rotation::Start,
            2 => Rotation::Active,
            3 => Rotation::Stop,
            n => Rotation::Unknown(n),
        },
        touch: p[5] & 0x02 != 0,
        proximity: p[5] & 0x04 != 0,
        single_tap: p[5] & 0x08 != 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rotation_with_touch() {
        let e = parse_event(&[0x00, 0x02, 0x00, 0x00, 0x02, 0x02]);
        assert_eq!((e.delta, e.rotation, e.touch), (2, Rotation::Active, true));
        let e = parse_event(&[0x00, 0x00, 0x00, 0x00, 0x03, 0x00]);
        assert_eq!((e.rotation, e.touch), (Rotation::Stop, false));
    }
}
