//! A full read-only description of one device.

use hidpp::features::{battery, dpi, force_button, haptic, hires_wheel, hosts, info, reprog, smartshift, thumb_wheel};
use hidpp::policy::{self, flags};
use hidpp::{Device, Error, FeatureEntry, Requester};
use serde::Serialize;

#[derive(Serialize)]
#[serde(untagged)]
pub enum Field<T> {
    Ok(T),
    Err { error: String },
}

impl<T> Field<T> {
    fn of(r: Result<T, Error>) -> Option<Self> {
        match r {
            Ok(v) => Some(Field::Ok(v)),
            Err(Error::Unsupported(_)) => None,
            Err(e) => Some(Field::Err { error: e.to_string() }),
        }
    }
}

#[derive(Serialize)]
pub struct Feature {
    pub index: u8,
    pub id: String,
    pub name: Option<&'static str>,
    pub version: u8,
    pub flags: Vec<&'static str>,
}

impl From<&FeatureEntry> for Feature {
    fn from(e: &FeatureEntry) -> Self {
        let f = e.slot.flags;
        let mut names = Vec::new();
        for (bit, name) in [(flags::OBSOLETE, "obsolete"), (flags::HIDDEN, "hidden"), (flags::INTERNAL, "internal")] {
            if f & bit != 0 {
                names.push(name);
            }
        }
        Feature {
            index: e.slot.index,
            id: format!("{:04X}", e.id),
            name: policy::name(e.id),
            version: e.slot.version,
            flags: names,
        }
    }
}

#[derive(Serialize)]
pub struct ControlDump {
    pub cid: String,
    pub name: Option<&'static str>,
    pub task: String,
    pub caps: Vec<&'static str>,
    pub group: u8,
    pub group_mask: u8,
    pub reporting: Option<Field<ReportingDump>>,
}

#[derive(Serialize)]
pub struct ReportingDump {
    pub flags: Vec<&'static str>,
    pub remapped_to: String,
}

#[derive(Serialize)]
pub struct Dump {
    pub hidraw: String,
    pub index: u8,
    pub protocol: String,
    pub name: Option<Field<String>>,
    pub friendly_name: Option<Field<String>>,
    pub kind: Option<Field<info::Kind>>,
    pub ids: Option<Field<info::Ids>>,
    pub firmware: Option<Field<Vec<info::Firmware>>>,
    pub features: Vec<Feature>,
    pub battery: Option<Field<battery::Status>>,
    pub dpi: Option<Field<dpi::Dpi>>,
    pub dpi_list: Option<Field<Vec<u16>>>,
    pub smartshift_caps: Option<Field<smartshift::Capabilities>>,
    pub smartshift: Option<Field<smartshift::State>>,
    pub wheel_caps: Option<Field<hires_wheel::Capabilities>>,
    pub wheel_mode: Option<Field<hires_wheel::Mode>>,
    pub wheel_ratchet: Option<Field<bool>>,
    pub thumb_wheel: Option<Field<thumb_wheel::Info>>,
    pub thumb_wheel_status: Option<Field<thumb_wheel::Status>>,
    pub haptic_caps: Option<Field<haptic::Capabilities>>,
    pub haptic_level: Option<Field<haptic::Level>>,
    pub force_buttons: Option<Field<Vec<force_button::ForceButton>>>,
    pub host: Option<Field<hosts::HostState>>,
    pub hosts: Option<Field<Vec<hosts::Host>>>,
    pub controls: Option<Field<Vec<ControlDump>>>,
}

fn cap_names(c: u16) -> Vec<&'static str> {
    use reprog::caps::*;
    [
        (MOUSE, "mouse"),
        (REPROGRAMMABLE, "reprogrammable"),
        (DIVERTABLE, "divertable"),
        (PERSISTENTLY_DIVERTABLE, "persistently-divertable"),
        (VIRTUAL, "virtual"),
        (RAW_XY, "raw-xy"),
        (FORCE_RAW_XY, "force-raw-xy"),
        (ANALYTICS, "analytics"),
        (RAW_WHEEL, "raw-wheel"),
    ]
    .into_iter()
    .filter(|(bit, _)| c & bit != 0)
    .map(|(_, n)| n)
    .collect()
}

fn reporting_names(f: u16) -> Vec<&'static str> {
    use reprog::reporting::*;
    [
        (DIVERTED, "diverted"),
        (PERSISTENTLY_DIVERTED, "persistently-diverted"),
        (RAW_XY, "raw-xy"),
        (FORCE_RAW_XY, "force-raw-xy"),
        (ANALYTICS, "analytics"),
        (RAW_WHEEL, "raw-wheel"),
    ]
    .into_iter()
    .filter(|(bit, _)| f & bit != 0)
    .map(|(_, n)| n)
    .collect()
}

fn controls(dev: &mut Device) -> Result<Vec<ControlDump>, Error> {
    let list = reprog::controls(dev)?;
    Ok(list
        .into_iter()
        .map(|c| {
            let reporting = Field::of(reprog::reporting(dev, c.cid).map(|r| ReportingDump {
                flags: reporting_names(r.flags),
                remapped_to: format!("{:04X}", r.remapped_to),
            }));
            ControlDump {
                cid: format!("{:04X}", c.cid),
                name: reprog::control_name(c.cid),
                task: format!("{:04X}", c.task),
                caps: cap_names(c.caps),
                group: c.group,
                group_mask: c.group_mask,
                reporting,
            }
        })
        .collect())
}

pub fn collect(dev: &mut Device, hidraw: String) -> Result<Dump, Error> {
    let (major, minor) = dev.ping()?;
    let features = dev.enumerate()?;
    Ok(Dump {
        hidraw,
        index: dev.index(),
        protocol: format!("{major}.{minor}"),
        name: Field::of(info::name(dev)),
        friendly_name: Field::of(info::friendly_name(dev)),
        kind: Field::of(info::kind(dev)),
        ids: Field::of(info::ids(dev)),
        firmware: Field::of(info::firmware(dev)),
        features: features.iter().map(Feature::from).collect(),
        battery: Field::of(battery::status(dev)),
        dpi: Field::of(dpi::get(dev, 0)),
        dpi_list: Field::of(dpi::list(dev, 0)),
        smartshift_caps: Field::of(smartshift::capabilities(dev)),
        smartshift: Field::of(smartshift::get(dev)),
        wheel_caps: Field::of(hires_wheel::capabilities(dev)),
        wheel_mode: Field::of(hires_wheel::mode(dev)),
        wheel_ratchet: Field::of(hires_wheel::ratchet(dev)),
        thumb_wheel: Field::of(thumb_wheel::info(dev)),
        thumb_wheel_status: Field::of(thumb_wheel::status(dev)),
        haptic_caps: Field::of(haptic::capabilities(dev)),
        haptic_level: Field::of(haptic::level(dev)),
        force_buttons: Field::of(force_button::all(dev)),
        host: Field::of(hosts::state(dev)),
        hosts: Field::of(hosts::hosts(dev)),
        controls: Field::of(controls(dev)),
    })
}

/// Calls function 0 of every unknown feature that research mode permits.
pub fn research(dev: &mut Device, features: &[FeatureEntry]) -> Vec<(String, String)> {
    features
        .iter()
        .filter(|e| policy::known(e.id).is_none())
        .map(|e| {
            let id = format!("{:04X}", e.id);
            let result = match dev.call(e.id, 0, &[]) {
                Ok(p) => p.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" "),
                Err(err) => format!("({err})"),
            };
            (id, result)
        })
        .collect()
}
