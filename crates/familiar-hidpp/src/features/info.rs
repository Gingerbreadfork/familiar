//! Device identity: firmware (`0x0003`), name and kind (`0x0005`), friendly name (`0x0007`).

use super::be16;
use crate::{Error, Requester};

pub const FW_VERSION: u16 = 0x0003;
pub const NAME: u16 = 0x0005;
pub const FRIENDLY_NAME: u16 = 0x0007;

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Ids {
    pub unit_id: String,
    pub model_id: String,
    /// Transport product IDs, e.g. `("wpid", "B042")`.
    pub transports: Vec<(&'static str, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Firmware {
    pub kind: FirmwareKind,
    pub name: String,
    pub version: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum FirmwareKind {
    Main,
    Bootloader,
    Hardware,
    Other(u8),
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02X}")).collect()
}

pub fn ids(dev: &mut impl Requester) -> Result<Ids, Error> {
    let p = dev.call(FW_VERSION, 0x0, &[])?;
    let model = &p[7..13];
    let bits = p[6];
    let mut transports = Vec::new();
    let mut offset = 0;
    for (name, flag) in [("btid", 0x1), ("btleid", 0x2), ("wpid", 0x4), ("usbid", 0x8)] {
        if bits & flag != 0 && offset + 2 <= model.len() {
            transports.push((name, hex(&model[offset..offset + 2])));
            offset += 2;
        }
    }
    Ok(Ids { unit_id: hex(&p[1..5]), model_id: hex(model), transports })
}

pub fn firmware(dev: &mut impl Requester) -> Result<Vec<Firmware>, Error> {
    let count = dev.call(FW_VERSION, 0x0, &[])?[0];
    let mut out = Vec::new();
    for entity in 0..count {
        let p = dev.call(FW_VERSION, 0x1, &[entity])?;
        let fw = match p[0] & 0x0F {
            level @ (0 | 1) => {
                let mut version = format!("{:02X}.{:02X}", p[4], p[5]);
                let build = be16(&p, 6);
                if build != 0 {
                    version.push_str(&format!(".B{build:04X}"));
                }
                Firmware {
                    kind: if level == 0 { FirmwareKind::Main } else { FirmwareKind::Bootloader },
                    name: String::from_utf8_lossy(&p[1..4]).trim_end_matches('\0').to_string(),
                    version,
                }
            }
            2 => Firmware { kind: FirmwareKind::Hardware, name: String::new(), version: p[1].to_string() },
            other => Firmware { kind: FirmwareKind::Other(other), name: String::new(), version: String::new() },
        };
        out.push(fw);
    }
    Ok(out)
}

pub fn name(dev: &mut impl Requester) -> Result<String, Error> {
    let len = dev.call(NAME, 0x0, &[])?[0] as usize;
    let mut name = Vec::with_capacity(len);
    while name.len() < len {
        let chunk = dev.call(NAME, 0x1, &[name.len() as u8])?;
        let take = (len - name.len()).min(chunk.len());
        name.extend_from_slice(&chunk[..take]);
    }
    Ok(String::from_utf8_lossy(&name).trim_end_matches('\0').to_string())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum Kind {
    Keyboard,
    Remote,
    Numpad,
    Mouse,
    Trackpad,
    Trackball,
    Presenter,
    Receiver,
    Other(u8),
}

pub fn kind(dev: &mut impl Requester) -> Result<Kind, Error> {
    Ok(match dev.call(NAME, 0x2, &[])?[0] {
        0 => Kind::Keyboard,
        1 => Kind::Remote,
        2 => Kind::Numpad,
        3 => Kind::Mouse,
        4 => Kind::Trackpad,
        5 => Kind::Trackball,
        6 => Kind::Presenter,
        7 => Kind::Receiver,
        n => Kind::Other(n),
    })
}

pub fn friendly_name(dev: &mut impl Requester) -> Result<String, Error> {
    let len = dev.call(FRIENDLY_NAME, 0x0, &[])?[0] as usize;
    let mut name = Vec::with_capacity(len);
    while name.len() < len {
        let chunk = dev.call(FRIENDLY_NAME, 0x1, &[name.len() as u8])?;
        let data = &chunk[1..];
        let take = (len - name.len()).min(data.len());
        if take == 0 {
            break;
        }
        name.extend_from_slice(&data[..take]);
    }
    Ok(String::from_utf8_lossy(&name).trim_end_matches('\0').to_string())
}
