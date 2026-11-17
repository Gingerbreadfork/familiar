//! Easy-Switch: Change Host (`0x1814`) and Hosts Info (`0x1815`).

use crate::{Error, Requester};

pub const CHANGE_HOST: u16 = 0x1814;
pub const HOSTS_INFO: u16 = 0x1815;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct HostState {
    pub count: u8,
    /// Zero-based channel the device is on now.
    pub current: u8,
}

pub fn state(dev: &mut impl Requester) -> Result<HostState, Error> {
    let p = dev.call(CHANGE_HOST, 0x0, &[])?;
    Ok(HostState { count: p[0], current: p[1] })
}

/// Moves the device to another channel. It disconnects from this host at once,
/// so a missing reply is expected and not treated as an error.
pub fn switch_to(dev: &mut impl Requester, host: u8) -> Result<(), Error> {
    match dev.call(CHANGE_HOST, 0x1, &[host]) {
        Ok(_) | Err(Error::Timeout) => Ok(()),
        Err(e) => Err(e),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Host {
    pub index: u8,
    pub paired: bool,
    pub name: String,
}

/// Renames a host slot. Names longer than the device allows are cut short.
/// Returns the name as stored.
pub fn rename(dev: &mut impl Requester, host: u8, name: &str) -> Result<String, Error> {
    let p = dev.call(HOSTS_INFO, 0x0, &[])?;
    if p[0] & 0x02 == 0 {
        return Err(Error::Invalid("this device cannot rename hosts".into()));
    }
    let info = dev.call(HOSTS_INFO, 0x1, &[host])?;
    let max = info[5] as usize;
    let mut bytes = name.as_bytes().to_vec();
    bytes.truncate(max);
    while std::str::from_utf8(&bytes).is_err() {
        bytes.pop();
    }
    for offset in (0..bytes.len().max(1)).step_by(14) {
        let end = (offset + 14).min(bytes.len());
        let mut params = vec![host, offset as u8];
        params.extend_from_slice(&bytes[offset..end]);
        dev.call(HOSTS_INFO, 0x4, &params)?;
    }
    Ok(String::from_utf8(bytes).unwrap_or_default())
}

pub fn hosts(dev: &mut impl Requester) -> Result<Vec<Host>, Error> {
    let p = dev.call(HOSTS_INFO, 0x0, &[])?;
    let can_name = p[0] & 0x01 != 0;
    let count = p[2];
    let mut out = Vec::new();
    for index in 0..count {
        let info = dev.call(HOSTS_INFO, 0x1, &[index])?;
        let paired = info[1] & 0x01 != 0;
        let len = info[4] as usize;
        let mut name = Vec::new();
        while can_name && name.len() < len {
            let chunk = dev.call(HOSTS_INFO, 0x3, &[index, name.len() as u8])?;
            let data = &chunk[2..];
            let take = (len - name.len()).min(data.len());
            if take == 0 {
                break;
            }
            name.extend_from_slice(&data[..take]);
        }
        out.push(Host { index, paired, name: String::from_utf8_lossy(&name).trim_end_matches('\0').to_string() });
    }
    Ok(out)
}
