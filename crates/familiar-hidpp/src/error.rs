use std::fmt;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("transport error: {0}")]
    Io(#[from] std::io::Error),
    #[error("no reply from device")]
    Timeout,
    #[error("device returned {0}")]
    Feature(FeatureError),
    #[error("receiver returned {0}")]
    Register(RegisterError),
    #[error("device does not have feature {0:#06x}")]
    Unsupported(u16),
    #[error("refusing to call function {function} of feature {feature:#06x}: {reason}")]
    Denied { feature: u16, function: u8, reason: &'static str },
    #[error("{len} parameter bytes do not fit in a report of {max}")]
    ParamsTooLong { len: usize, max: usize },
    #[error("transport supports no report kind large enough")]
    NoReportKind,
    #[error("malformed reply: {0}")]
    Malformed(&'static str),
    #[error("{0}")]
    Invalid(String),
}

/// HID++ 2.0 error code, sent as a reply with feature index `0xFF`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeatureError(pub u8);

impl FeatureError {
    pub const INVALID_ARGUMENT: u8 = 0x02;
    pub const OUT_OF_RANGE: u8 = 0x03;
    pub const INVALID_FUNCTION: u8 = 0x07;
    pub const BUSY: u8 = 0x08;

    pub fn name(self) -> &'static str {
        match self.0 {
            0x00 => "no error",
            0x01 => "unknown error",
            0x02 => "invalid argument",
            0x03 => "out of range",
            0x04 => "hardware error",
            0x05 => "internal error",
            0x06 => "invalid feature index",
            0x07 => "invalid function",
            0x08 => "busy",
            0x09 => "unsupported",
            _ => "unrecognised error",
        }
    }
}

impl fmt::Display for FeatureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({:#04x})", self.name(), self.0)
    }
}

/// HID++ 1.0 error code, sent as a reply with sub ID `0x8F`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegisterError(pub u8);

impl RegisterError {
    pub const INVALID_SUB_ID: u8 = 0x01;
    pub const UNKNOWN_DEVICE: u8 = 0x08;
    pub const RESOURCE_ERROR: u8 = 0x09;

    pub fn name(self) -> &'static str {
        match self.0 {
            0x01 => "invalid sub ID",
            0x02 => "invalid address",
            0x03 => "invalid value",
            0x04 => "connection failed",
            0x05 => "too many devices",
            0x06 => "already exists",
            0x07 => "busy",
            0x08 => "unknown device",
            0x09 => "device unreachable",
            0x0A => "request unavailable",
            0x0B => "invalid parameter",
            0x0C => "wrong PIN",
            _ => "unrecognised error",
        }
    }

    /// True when the receiver says the paired device is asleep or not paired.
    pub fn is_unreachable(self) -> bool {
        matches!(self.0, Self::UNKNOWN_DEVICE | Self::RESOURCE_ERROR)
    }
}

impl fmt::Display for RegisterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({:#04x})", self.name(), self.0)
    }
}
