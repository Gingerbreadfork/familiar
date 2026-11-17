//! Typed access to individual HID++ 2.0 features.
//!
//! Each module wraps one feature's functions and parses their replies. Layouts
//! are described in `docs/protocol/`.

pub mod battery;
pub mod dpi;
pub mod force_button;
pub mod haptic;
pub mod hires_wheel;
pub mod hosts;
pub mod info;
pub mod reprog;
pub mod smartshift;
pub mod thumb_wheel;

pub(crate) fn be16(b: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([b[at], b[at + 1]])
}

pub(crate) fn be16i(b: &[u8], at: usize) -> i16 {
    i16::from_be_bytes([b[at], b[at + 1]])
}
