//! Logitech receivers, which relay HID++ to paired devices at indices 1 to 6.

use crate::link::Exchange;
use crate::{DIRECT, Error, Packet};

pub const BOLT: u16 = 0xC548;

const RECEIVERS: &[u16] = &[
    0xC517, 0xC51B, 0xC526, 0xC52B, 0xC52E, 0xC52F, 0xC531, 0xC532, 0xC534, 0xC539, 0xC53A, 0xC53D, 0xC53F, 0xC541,
    0xC545, 0xC547, BOLT,
];

/// True when a Logitech USB product ID belongs to a receiver.
pub fn is_receiver(product: u16) -> bool {
    RECEIVERS.contains(&product)
}

pub fn name(product: u16) -> &'static str {
    match product {
        BOLT => "Logi Bolt receiver",
        0xC52B | 0xC532 => "Unifying receiver",
        0xC539..=0xC547 => "Lightspeed receiver",
        _ if is_receiver(product) => "Nano receiver",
        _ => "device",
    }
}

/// Device slots a receiver can have.
pub const SLOTS: std::ops::RangeInclusive<u8> = 1..=6;

const REG_NOTIFICATIONS: u8 = 0x00;
const REG_CONNECTION: u8 = 0x02;

/// Receiver notification flags: device connection events, and "software
/// present", which tells devices that a host program handles their events.
const FLAG_WIRELESS: u32 = 0x00_01_00;
const FLAG_SOFTWARE_PRESENT: u32 = 0x00_08_00;

/// Turns on device connection notifications, keeping any flags already set.
pub fn enable_notifications(link: &dyn Exchange) -> Result<(), Error> {
    let cur = link.raw_read_register(DIRECT, REG_NOTIFICATIONS, false, &[])?;
    let flags = u32::from_be_bytes([0, cur[0], cur[1], cur[2]]) | FLAG_WIRELESS | FLAG_SOFTWARE_PRESENT;
    let [_, a, b, c] = flags.to_be_bytes();
    link.raw_write_register(DIRECT, REG_NOTIFICATIONS, &[a, b, c])?;
    Ok(())
}

/// Asks the receiver to send a connection notification for every paired device.
pub fn announce_devices(link: &dyn Exchange) -> Result<(), Error> {
    link.raw_write_register(DIRECT, REG_CONNECTION, &[0x02, 0, 0])?;
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Connection {
    pub index: u8,
    pub linked: bool,
    pub wpid: u16,
}

/// Parses a device connection notification, sub ID `0x41`.
pub fn parse_connection(p: &Packet) -> Option<Connection> {
    if p.sub_id != 0x41 || p.address == 0 {
        return None;
    }
    let d = p.params();
    Some(Connection { index: p.device, linked: d[0] & 0x40 == 0, wpid: u16::from_be_bytes([d[2], d[1]]) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ReportKind;

    #[test]
    fn parses_bolt_connection() {
        let up = Packet::new(ReportKind::Short, 2, 0x41, 0x10, &[0x00, 0x42, 0xB0]).unwrap();
        assert_eq!(parse_connection(&up), Some(Connection { index: 2, linked: true, wpid: 0xB042 }));
        let down = Packet::new(ReportKind::Short, 2, 0x41, 0x10, &[0x40, 0x42, 0xB0]).unwrap();
        assert!(!parse_connection(&down).unwrap().linked);
    }
}
