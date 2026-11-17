//! Force Sensing Button (`0x19C0`), the press threshold of the thumb panel.

use super::be16;
use crate::{Error, Requester};

pub const ID: u16 = 0x19C0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ForceButton {
    pub number: u8,
    pub changeable: bool,
    pub default: u16,
    pub min: u16,
    pub max: u16,
    pub current: u16,
}

pub fn count(dev: &mut impl Requester) -> Result<u8, Error> {
    Ok(dev.call(ID, 0x0, &[])?[0])
}

pub fn get(dev: &mut impl Requester, number: u8) -> Result<ForceButton, Error> {
    let info = dev.call(ID, 0x1, &[number])?;
    let current = dev.call(ID, 0x2, &[number])?;
    Ok(ForceButton {
        number,
        changeable: be16(&info, 0) & 0x01 != 0,
        default: be16(&info, 2),
        max: be16(&info, 4),
        min: be16(&info, 6),
        current: be16(&current, 0),
    })
}

pub fn all(dev: &mut impl Requester) -> Result<Vec<ForceButton>, Error> {
    (0..count(dev)?).map(|n| get(dev, n)).collect()
}

pub fn set(dev: &mut impl Requester, number: u8, value: u16) -> Result<(), Error> {
    let b = get(dev, number)?;
    if !b.changeable {
        return Err(Error::Invalid(format!("force button {number} is not adjustable")));
    }
    if !(b.min..=b.max).contains(&value) {
        return Err(Error::Invalid(format!("force must be between {} and {}", b.min, b.max)));
    }
    let [hi, lo] = value.to_be_bytes();
    dev.call(ID, 0x3, &[number, hi, lo])?;
    Ok(())
}
