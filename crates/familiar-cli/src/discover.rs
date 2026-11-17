use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use hidpp::{DIRECT, Device, Exchange, Incoming, Link, Policy, TOOL_SWIDS, receiver};
use hidpp_hidraw::{BUS_USB, Hidraw, HidrawInfo};

/// One HID++ endpoint and the devices that answered on it.
pub struct Endpoint {
    pub info: HidrawInfo,
    pub link: Arc<Link>,
    pub events: Receiver<Incoming>,
    pub devices: Vec<Found>,
}

impl Endpoint {
    pub fn is_receiver(&self) -> bool {
        self.info.bus == BUS_USB && receiver::is_receiver(self.info.product)
    }
}

#[derive(Clone)]
pub struct Found {
    pub index: u8,
    pub protocol: (u8, u8),
}

const PROBE_TIMEOUT: Duration = Duration::from_millis(400);

pub fn open(info: HidrawInfo) -> Result<Endpoint> {
    let transport = Hidraw::open(&info).with_context(|| format!("opening {}", info.path.display()))?;
    let (tx, events) = mpsc::channel();
    let link = Link::new(Box::new(transport), TOOL_SWIDS, move |i| {
        let _ = tx.send(i);
    })?
    .with_timeout(PROBE_TIMEOUT);
    let link = Arc::new(link);
    let indices: Vec<u8> = if info.bus == BUS_USB && receiver::is_receiver(info.product) {
        receiver::SLOTS.collect()
    } else {
        vec![DIRECT]
    };
    let devices = indices
        .into_iter()
        .filter_map(|index| {
            let dev = Device::new(link.clone() as Arc<dyn Exchange>, index);
            dev.ping().ok().map(|protocol| Found { index, protocol })
        })
        .collect();
    let mut link = Arc::try_unwrap(link).unwrap_or_else(|_| unreachable!("probe devices are dropped"));
    link.set_timeout(Duration::from_secs(2));
    Ok(Endpoint { info, link: Arc::new(link), events, devices })
}

pub fn endpoints(path: Option<&Path>) -> Result<Vec<Endpoint>> {
    let infos = match path {
        Some(p) => vec![hidpp_hidraw::info(p).with_context(|| format!("{} is not a hidraw node", p.display()))?],
        None => hidpp_hidraw::enumerate().context("listing hidraw devices")?,
    };
    if infos.is_empty() {
        bail!("no Logitech HID++ device found; is the receiver plugged in and is /dev/hidraw* readable?");
    }
    infos.into_iter().map(open).collect()
}

pub struct Selected {
    pub path: PathBuf,
    pub device: Device,
    pub events: Receiver<Incoming>,
}

/// Picks the device to act on: the requested index, or the first HID++ 2.0 device found.
pub fn select(path: Option<&Path>, index: Option<u8>, policy: Policy) -> Result<Selected> {
    for ep in endpoints(path)? {
        let Some(d) = ep.devices.iter().find(|d| d.protocol.0 >= 2 && index.is_none_or(|i| i == d.index)) else {
            continue;
        };
        let device = Device::new(ep.link.clone() as Arc<dyn Exchange>, d.index).with_policy(policy);
        return Ok(Selected { path: ep.info.path.clone(), device, events: ep.events });
    }
    match index {
        Some(i) => bail!("no HID++ 2.0 device answered at index {i}; it may be asleep, so move the mouse and retry"),
        None => bail!("no HID++ 2.0 device answered; it may be asleep, so move the mouse and retry"),
    }
}
