//! Reprog Controls v4 (`0x1B04`): remapping and diversion of buttons.

use super::{be16, be16i};
use crate::{Error, Requester};

pub const ID: u16 = 0x1B04;

/// Capability flags from `getCidInfo`, bytes 4 and 8 combined.
pub mod caps {
    pub const MOUSE: u16 = 0x0001;
    pub const REPROGRAMMABLE: u16 = 0x0010;
    pub const DIVERTABLE: u16 = 0x0020;
    pub const PERSISTENTLY_DIVERTABLE: u16 = 0x0040;
    pub const VIRTUAL: u16 = 0x0080;
    pub const RAW_XY: u16 = 0x0100;
    pub const FORCE_RAW_XY: u16 = 0x0200;
    pub const ANALYTICS: u16 = 0x0400;
    pub const RAW_WHEEL: u16 = 0x0800;
}

/// Reporting flags from `getCidReporting`, bytes 2 and 5 combined. In
/// `setCidReporting` each flag is followed by a "valid" bit one position up.
pub mod reporting {
    pub const DIVERTED: u16 = 0x0001;
    pub const PERSISTENTLY_DIVERTED: u16 = 0x0004;
    pub const RAW_XY: u16 = 0x0010;
    pub const FORCE_RAW_XY: u16 = 0x0040;
    pub const ANALYTICS: u16 = 0x0100;
    pub const RAW_WHEEL: u16 = 0x0400;
}

/// Names for the control IDs found on MX mice.
pub fn control_name(cid: u16) -> Option<&'static str> {
    Some(match cid {
        0x0050 => "left",
        0x0051 => "right",
        0x0052 => "middle",
        0x0053 => "back",
        0x0056 => "forward",
        0x00C3 => "gesture",
        0x00C4 => "smartshift",
        0x00D7 => "virtual-gesture",
        0x01A0 => "haptic",
        _ => return None,
    })
}

pub fn control_by_name(name: &str) -> Option<u16> {
    [0x0050, 0x0051, 0x0052, 0x0053, 0x0056, 0x00C3, 0x00C4, 0x00D7, 0x01A0]
        .into_iter()
        .find(|c| control_name(*c) == Some(name))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Control {
    pub index: u8,
    pub cid: u16,
    pub task: u16,
    pub caps: u16,
    pub position: u8,
    pub group: u8,
    pub group_mask: u8,
}

impl Control {
    pub fn has(&self, cap: u16) -> bool {
        self.caps & cap != 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Reporting {
    pub cid: u16,
    pub flags: u16,
    /// Control this one is remapped to; equal to `cid` when not remapped.
    pub remapped_to: u16,
}

impl Reporting {
    pub fn has(&self, flag: u16) -> bool {
        self.flags & flag != 0
    }
}

pub fn count(dev: &mut impl Requester) -> Result<u8, Error> {
    Ok(dev.call(ID, 0x0, &[])?[0])
}

pub fn control(dev: &mut impl Requester, index: u8) -> Result<Control, Error> {
    let p = dev.call(ID, 0x1, &[index])?;
    Ok(Control {
        index,
        cid: be16(&p, 0),
        task: be16(&p, 2),
        caps: p[4] as u16 | (p[8] as u16) << 8,
        position: p[5],
        group: p[6],
        group_mask: p[7],
    })
}

pub fn controls(dev: &mut impl Requester) -> Result<Vec<Control>, Error> {
    (0..count(dev)?).map(|i| control(dev, i)).collect()
}

pub fn reporting(dev: &mut impl Requester, cid: u16) -> Result<Reporting, Error> {
    let [hi, lo] = cid.to_be_bytes();
    let p = dev.call(ID, 0x2, &[hi, lo])?;
    let remap = be16(&p, 3);
    Ok(Reporting {
        cid: be16(&p, 0),
        flags: p[2] as u16 | (p[5] as u16) << 8,
        remapped_to: if remap == 0 { cid } else { remap },
    })
}

/// Changes reporting flags for one control. Flags listed in `set` are turned
/// on, those in `clear` turned off, and all others left alone. Persistent
/// diversion survives power cycles, so it is refused.
pub fn set_reporting(dev: &mut impl Requester, cid: u16, set: u16, clear: u16) -> Result<(), Error> {
    if set & reporting::PERSISTENTLY_DIVERTED != 0 {
        return Err(Error::Denied { feature: ID, function: 3, reason: "persistent diversion changes the device" });
    }
    let mut bits: u16 = 0;
    for flag in [
        reporting::DIVERTED,
        reporting::PERSISTENTLY_DIVERTED,
        reporting::RAW_XY,
        reporting::FORCE_RAW_XY,
        reporting::ANALYTICS,
        reporting::RAW_WHEEL,
    ] {
        if set & flag != 0 {
            bits |= flag | flag << 1;
        } else if clear & flag != 0 {
            bits |= flag << 1;
        }
    }
    let [hi, lo] = cid.to_be_bytes();
    let low = (bits & 0xFF) as u8;
    let high = (bits >> 8) as u8;
    if high == 0 {
        dev.call(ID, 0x3, &[hi, lo, low, 0, 0])?;
    } else {
        dev.call(ID, 0x3, &[hi, lo, low, 0, 0, high])?;
    }
    Ok(())
}

/// Makes a control act as another one, in the device itself. Remapping a
/// control to itself undoes it. The target must be in one of the groups in
/// the control's `group_mask`.
pub fn remap(dev: &mut impl Requester, cid: u16, target: u16) -> Result<(), Error> {
    let [hi, lo] = cid.to_be_bytes();
    let [thi, tlo] = target.to_be_bytes();
    dev.call(ID, 0x3, &[hi, lo, 0, thi, tlo])?;
    Ok(())
}

/// Whether `control` may be remapped to `target`, going by their groups.
pub fn can_remap(control: &Control, target: &Control) -> bool {
    // The group comes from the device, so it may be larger than the mask has bits.
    let bit = target.group.checked_sub(1).and_then(|n| 1u8.checked_shl(n.into()));
    control.has(caps::REPROGRAMMABLE) && bit.is_some_and(|bit| control.group_mask & bit != 0)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// Diverted controls currently held, up to four.
    Buttons(Vec<u16>),
    RawXy {
        dx: i16,
        dy: i16,
    },
    Other(u8),
}

pub fn parse_event(function: u8, p: &[u8]) -> Event {
    match function {
        0 => Event::Buttons((0..4).map(|i| be16(p, i * 2)).filter(|c| *c != 0).collect()),
        1 => Event::RawXy { dx: be16i(p, 0), dy: be16i(p, 2) },
        n => Event::Other(n),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Recorder(Vec<Vec<u8>>);

    impl Requester for Recorder {
        fn call(&mut self, _: u16, _: u8, params: &[u8]) -> Result<Vec<u8>, Error> {
            self.0.push(params.to_vec());
            Ok(vec![0; 16])
        }
    }

    #[test]
    fn set_reporting_sets_valid_bits() {
        let mut r = Recorder(Vec::new());
        set_reporting(&mut r, 0x00C3, reporting::DIVERTED | reporting::RAW_XY, 0).unwrap();
        assert_eq!(r.0[0], vec![0x00, 0xC3, 0x33, 0, 0]);
        set_reporting(&mut r, 0x00C3, 0, reporting::DIVERTED | reporting::RAW_XY).unwrap();
        assert_eq!(r.0[1], vec![0x00, 0xC3, 0x22, 0, 0]);
    }

    #[test]
    fn remap_follows_groups() {
        let control = |group, group_mask| Control {
            index: 0,
            cid: 0x0053,
            task: 0,
            caps: caps::REPROGRAMMABLE,
            position: 0,
            group,
            group_mask,
        };
        assert!(can_remap(&control(2, 0x03), &control(2, 0)));
        assert!(!can_remap(&control(2, 0x03), &control(3, 0)));
        assert!(!can_remap(&control(2, 0x03), &control(0, 0)));
        assert!(!can_remap(&control(2, 0xFF), &control(9, 0)), "a group past the mask matches nothing");
    }

    #[test]
    fn parses_button_event() {
        let p = [0x00, 0xC3, 0x00, 0x53, 0, 0, 0, 0];
        assert_eq!(parse_event(0, &p), Event::Buttons(vec![0x00C3, 0x0053]));
        assert_eq!(parse_event(1, &[0xFF, 0xFE, 0x00, 0x03]), Event::RawXy { dx: -2, dy: 3 });
    }
}
