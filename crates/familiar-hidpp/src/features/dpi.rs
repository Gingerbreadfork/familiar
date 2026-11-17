//! Adjustable DPI (`0x2201`).

use super::be16;
use crate::{Error, Requester};

pub const ID: u16 = 0x2201;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Dpi {
    pub current: u16,
    pub default: u16,
}

/// Expands a DPI list. A value with the top three bits set is a step size that
/// fills the range between its neighbours. Zero ends the list.
pub fn parse_list(bytes: &[u8]) -> Vec<u16> {
    let mut out: Vec<u16> = Vec::new();
    let mut i = 0;
    while i + 1 < bytes.len() {
        let v = be16(bytes, i);
        if v == 0 {
            break;
        }
        if v >> 13 == 0b111 && i + 3 < bytes.len() {
            // Wider than u16, so a range ending at 0xFFFF still ends.
            let step = u32::from(v & 0x1FFF);
            let last = u32::from(be16(bytes, i + 2));
            if let Some(&start) = out.last()
                && step > 0
            {
                let mut d = u32::from(start) + step;
                while d <= last {
                    out.push(d as u16);
                    d += step;
                }
            }
            i += 4;
        } else {
            out.push(v);
            i += 2;
        }
    }
    out
}

pub fn list(dev: &mut impl Requester, sensor: u8) -> Result<Vec<u16>, Error> {
    let mut bytes = Vec::new();
    for page in 0..8u8 {
        let p = dev.call(ID, 0x1, &[sensor, 0, page])?;
        // Each page holds seven values after the sensor byte.
        bytes.extend_from_slice(&p[1..15]);
        if bytes.as_chunks::<2>().0.contains(&[0, 0]) {
            break;
        }
    }
    Ok(parse_list(&bytes))
}

pub fn get(dev: &mut impl Requester, sensor: u8) -> Result<Dpi, Error> {
    let p = dev.call(ID, 0x2, &[sensor])?;
    let current = be16(&p, 1);
    let default = be16(&p, 3);
    Ok(Dpi { current: if current == 0 { default } else { current }, default })
}

pub fn set(dev: &mut impl Requester, sensor: u8, dpi: u16) -> Result<(), Error> {
    let [hi, lo] = dpi.to_be_bytes();
    dev.call(ID, 0x3, &[sensor, hi, lo])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_ranges() {
        // 200, step 50, up to 400, then 1000.
        let bytes = [0x00, 0xC8, 0xE0, 0x32, 0x01, 0x90, 0x03, 0xE8, 0x00, 0x00, 0xFF];
        assert_eq!(parse_list(&bytes), vec![200, 250, 300, 350, 400, 1000]);
    }

    #[test]
    fn range_up_to_the_largest_value_ends() {
        // 57000, step 0x1FFF, up to 0xFFFF.
        let bytes = [0xDE, 0xA8, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00];
        assert_eq!(parse_list(&bytes), vec![57000, 65191]);
    }

    #[test]
    fn plain_list() {
        assert_eq!(parse_list(&[0x01, 0x90, 0x03, 0x20, 0, 0]), vec![400, 800]);
    }
}
