use std::collections::HashMap;
use std::sync::Arc;

use crate::link::Exchange;
use crate::policy::{self, Policy};
use crate::{Error, RegisterError, ReportKind};

/// Device index used for a device connected directly, not through a receiver.
pub const DIRECT: u8 = 0xFF;

/// Anything that can call a feature function by feature ID.
pub trait Requester {
    fn call(&mut self, feature: u16, function: u8, params: &[u8]) -> Result<Vec<u8>, Error>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct FeatureSlot {
    pub index: u8,
    pub flags: u8,
    pub version: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct FeatureEntry {
    pub id: u16,
    pub slot: FeatureSlot,
}

/// A HID++ 2.0 device at one index on a link.
pub struct Device {
    link: Arc<dyn Exchange>,
    index: u8,
    policy: Policy,
    features: HashMap<u16, Option<FeatureSlot>>,
}

impl Device {
    pub fn new(link: Arc<dyn Exchange>, index: u8) -> Self {
        Device { link, index, policy: Policy::Normal, features: HashMap::new() }
    }

    pub fn with_policy(mut self, policy: Policy) -> Self {
        self.policy = policy;
        self
    }

    pub fn index(&self) -> u8 {
        self.index
    }

    /// Replies are padded to at least a long report's length, so parsers can
    /// index fixed offsets even when a device answers with a short report.
    fn raw(&self, feature_index: u8, function: u8, params: &[u8]) -> Result<Vec<u8>, Error> {
        let mut p = self.link.raw_request(self.index, feature_index, function, params)?.params().to_vec();
        p.resize(p.len().max(ReportKind::Long.params_len()), 0);
        Ok(p)
    }

    /// Returns the HID++ protocol version, or `(1, 0)` for HID++ 1.0 devices.
    pub fn ping(&self) -> Result<(u8, u8), Error> {
        const MARK: u8 = 0x5A;
        match self.raw(0x00, 0x1, &[0, 0, MARK]) {
            Ok(p) if p[2] == MARK => Ok((p[0], p[1])),
            Ok(_) => Err(Error::Malformed("ping reply did not echo its marker")),
            Err(Error::Register(RegisterError(RegisterError::INVALID_SUB_ID))) => Ok((1, 0)),
            Err(e) => Err(e),
        }
    }

    /// Looks up a feature by ID, caching the answer.
    pub fn feature(&mut self, id: u16) -> Result<Option<FeatureSlot>, Error> {
        if id == 0x0000 {
            return Ok(Some(FeatureSlot { index: 0, flags: 0, version: 0 }));
        }
        if let Some(slot) = self.features.get(&id) {
            return Ok(*slot);
        }
        let p = self.raw(0x00, 0x0, &id.to_be_bytes())?;
        let slot = (p[0] != 0).then_some(FeatureSlot { index: p[0], flags: p[1], version: p[2] });
        self.features.insert(id, slot);
        Ok(slot)
    }

    pub fn has(&mut self, id: u16) -> Result<bool, Error> {
        Ok(self.feature(id)?.is_some())
    }

    /// Lists every feature the device reports, in index order.
    pub fn enumerate(&mut self) -> Result<Vec<FeatureEntry>, Error> {
        let Some(set) = self.feature(0x0001)? else { return Err(Error::Unsupported(0x0001)) };
        let count = self.raw(set.index, 0x0, &[])?[0];
        let mut out = vec![FeatureEntry { id: 0x0000, slot: FeatureSlot { index: 0, flags: 0, version: 0 } }];
        for index in 1..=count {
            let p = self.raw(set.index, 0x1, &[index])?;
            let id = u16::from_be_bytes([p[0], p[1]]);
            let slot = FeatureSlot { index, flags: p[2], version: p[3] };
            self.features.insert(id, Some(slot));
            out.push(FeatureEntry { id, slot });
        }
        Ok(out)
    }
}

impl Requester for Device {
    fn call(&mut self, feature: u16, function: u8, params: &[u8]) -> Result<Vec<u8>, Error> {
        let slot = self.feature(feature)?.ok_or(Error::Unsupported(feature))?;
        policy::check(self.policy, feature, slot.flags, function)?;
        self.raw(slot.index, function, params)
    }
}
