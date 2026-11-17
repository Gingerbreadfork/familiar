//! Haptic (`0x19B0`), the MX Master 4 thumb panel motor.

use crate::{Error, Requester};

pub const ID: u16 = 0x19B0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum Waveform {
    SharpStateChange = 0x00,
    DampStateChange = 0x01,
    SharpCollision = 0x02,
    DampCollision = 0x03,
    SubtleCollision = 0x04,
    HappyAlert = 0x05,
    AngryAlert = 0x06,
    Completed = 0x07,
    Square = 0x08,
    Wave = 0x09,
    Firework = 0x0A,
    Mad = 0x0B,
    Knock = 0x0C,
    Jingle = 0x0D,
    Ringing = 0x0E,
    WhisperCollision = 0x1B,
}

impl Waveform {
    pub const ALL: [Waveform; 16] = [
        Waveform::SharpStateChange,
        Waveform::DampStateChange,
        Waveform::SharpCollision,
        Waveform::DampCollision,
        Waveform::SubtleCollision,
        Waveform::HappyAlert,
        Waveform::AngryAlert,
        Waveform::Completed,
        Waveform::Square,
        Waveform::Wave,
        Waveform::Firework,
        Waveform::Mad,
        Waveform::Knock,
        Waveform::Jingle,
        Waveform::Ringing,
        Waveform::WhisperCollision,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Waveform::SharpStateChange => "sharp-state-change",
            Waveform::DampStateChange => "damp-state-change",
            Waveform::SharpCollision => "sharp-collision",
            Waveform::DampCollision => "damp-collision",
            Waveform::SubtleCollision => "subtle-collision",
            Waveform::HappyAlert => "happy-alert",
            Waveform::AngryAlert => "angry-alert",
            Waveform::Completed => "completed",
            Waveform::Square => "square",
            Waveform::Wave => "wave",
            Waveform::Firework => "firework",
            Waveform::Mad => "mad",
            Waveform::Knock => "knock",
            Waveform::Jingle => "jingle",
            Waveform::Ringing => "ringing",
            Waveform::WhisperCollision => "whisper-collision",
        }
    }

    pub fn from_name(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|w| w.name() == s)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Capabilities {
    /// Raw bitmask of supported waveform numbers.
    pub waveform_mask: u32,
    pub waveforms: Vec<Waveform>,
    pub raw: Vec<u8>,
}

impl Capabilities {
    pub fn supports(&self, w: Waveform) -> bool {
        self.waveform_mask & (1 << w as u32) != 0
    }
}

pub fn capabilities(dev: &mut impl Requester) -> Result<Capabilities, Error> {
    let p = dev.call(ID, 0x0, &[])?;
    let waveform_mask = u32::from_be_bytes([p[4], p[5], p[6], p[7]]);
    let waveforms = Waveform::ALL.into_iter().filter(|w| waveform_mask & (1 << *w as u32) != 0).collect();
    Ok(Capabilities { waveform_mask, waveforms, raw: p[..8].to_vec() })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Level {
    pub enabled: bool,
    /// Strength in percent.
    pub percent: u8,
    /// The device only accepts a few fixed steps instead of any percentage.
    pub stepped: bool,
}

pub fn level(dev: &mut impl Requester) -> Result<Level, Error> {
    let p = dev.call(ID, 0x1, &[])?;
    Ok(Level { enabled: p[0] & 0x01 != 0, percent: p[1], stepped: p[2] & 0x01 != 0 })
}

/// Sets the strength. Zero turns haptics off and keeps the previous strength stored.
pub fn set_level(dev: &mut impl Requester, percent: u8) -> Result<(), Error> {
    let percent = percent.min(100);
    if percent == 0 {
        let current = level(dev)?.percent.max(1);
        dev.call(ID, 0x2, &[0x00, current])?;
    } else {
        dev.call(ID, 0x2, &[0x01, percent])?;
    }
    Ok(())
}

pub fn play(dev: &mut impl Requester, w: Waveform) -> Result<(), Error> {
    dev.call(ID, 0x4, &[w as u8])?;
    Ok(())
}
