//! HID++ report framing.

use std::fmt;

use crate::Error;

/// The three HID++ report sizes. Receivers usually support only short and long.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReportKind {
    Short,
    Long,
    VeryLong,
}

impl ReportKind {
    pub const ALL: [ReportKind; 3] = [ReportKind::Short, ReportKind::Long, ReportKind::VeryLong];

    pub const fn id(self) -> u8 {
        match self {
            ReportKind::Short => 0x10,
            ReportKind::Long => 0x11,
            ReportKind::VeryLong => 0x12,
        }
    }

    /// Total report length including the report ID byte.
    pub const fn report_len(self) -> usize {
        match self {
            ReportKind::Short => 7,
            ReportKind::Long => 20,
            ReportKind::VeryLong => 64,
        }
    }

    pub const fn params_len(self) -> usize {
        self.report_len() - 4
    }

    pub const fn from_id(id: u8) -> Option<Self> {
        match id {
            0x10 => Some(ReportKind::Short),
            0x11 => Some(ReportKind::Long),
            0x12 => Some(ReportKind::VeryLong),
            _ => None,
        }
    }
}

const MAX_PARAMS: usize = ReportKind::VeryLong.params_len();

/// One HID++ report.
///
/// For HID++ 2.0, `sub_id` is the feature index and `address` holds the function
/// number in its high nibble and the software ID in its low nibble. For HID++ 1.0
/// they are the register sub ID and register address.
#[derive(Clone, PartialEq, Eq)]
pub struct Packet {
    pub kind: ReportKind,
    pub device: u8,
    pub sub_id: u8,
    pub address: u8,
    params: [u8; MAX_PARAMS],
}

impl Packet {
    pub fn new(kind: ReportKind, device: u8, sub_id: u8, address: u8, params: &[u8]) -> Result<Self, Error> {
        if params.len() > kind.params_len() {
            return Err(Error::ParamsTooLong { len: params.len(), max: kind.params_len() });
        }
        let mut buf = [0u8; MAX_PARAMS];
        buf[..params.len()].copy_from_slice(params);
        Ok(Packet { kind, device, sub_id, address, params: buf })
    }

    /// The parameter bytes, padded with zeros to the report's full length.
    pub fn params(&self) -> &[u8] {
        &self.params[..self.kind.params_len()]
    }

    /// The parameter bytes padded to the largest report size, for parsing events
    /// at fixed offsets whatever report size they arrived in.
    pub fn payload(&self) -> &[u8] {
        &self.params
    }

    pub fn function(&self) -> u8 {
        self.address >> 4
    }

    pub fn swid(&self) -> u8 {
        self.address & 0x0F
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.kind.report_len());
        out.extend_from_slice(&[self.kind.id(), self.device, self.sub_id, self.address]);
        out.extend_from_slice(self.params());
        out
    }

    /// Decodes a raw report. Returns `None` for reports that are not HID++.
    pub fn decode(raw: &[u8]) -> Option<Self> {
        let kind = ReportKind::from_id(*raw.first()?)?;
        if raw.len() < kind.report_len() {
            return None;
        }
        let mut params = [0u8; MAX_PARAMS];
        params[..kind.params_len()].copy_from_slice(&raw[4..kind.report_len()]);
        Some(Packet { kind, device: raw[1], sub_id: raw[2], address: raw[3], params })
    }
}

impl fmt::Debug for Packet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02x} {:02x} {:02x} {:02x} |", self.kind.id(), self.device, self.sub_id, self.address)?;
        for b in self.params() {
            write!(f, " {b:02x}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_short() {
        let p = Packet::new(ReportKind::Short, 0x02, 0x00, 0x15, &[0, 0, 0xAA]).unwrap();
        let raw = p.encode();
        assert_eq!(raw, [0x10, 0x02, 0x00, 0x15, 0x00, 0x00, 0xAA]);
        assert_eq!(Packet::decode(&raw).unwrap(), p);
    }

    #[test]
    fn long_pads_params() {
        let p = Packet::new(ReportKind::Long, 0xFF, 0x05, 0x2A, &[1, 2]).unwrap();
        assert_eq!(p.params().len(), 16);
        assert_eq!(p.encode().len(), 20);
        assert_eq!(p.function(), 2);
        assert_eq!(p.swid(), 0xA);
    }

    #[test]
    fn rejects_oversized_params() {
        assert!(Packet::new(ReportKind::Short, 1, 0, 0, &[0; 4]).is_err());
    }

    #[test]
    fn ignores_foreign_reports() {
        assert!(Packet::decode(&[0x02, 0, 0, 0]).is_none());
        assert!(Packet::decode(&[0x11, 0, 0]).is_none());
    }
}
