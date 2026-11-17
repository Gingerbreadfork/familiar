//! Live event viewer with optional temporary diversion.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use hidpp::features::{battery, hires_wheel, reprog, thumb_wheel};
use hidpp::{Device, Incoming, Packet, policy, receiver};

pub struct Options {
    pub divert: Vec<u16>,
    pub raw_xy: bool,
    /// Report pointer movement continuously, freezing the pointer, on controls that support it.
    pub force_raw_xy: bool,
    pub wheel: bool,
    /// Put the diverted wheel in hi-res mode.
    pub hires: bool,
    pub thumb: bool,
    /// One JSON object per line, as described in docs/events.md.
    pub json: bool,
}

/// Schema version of the JSON event lines.
const JSON_VERSION: u32 = 1;

fn control(cid: u16) -> String {
    reprog::control_name(cid).map(str::to_string).unwrap_or(format!("{cid:04X}"))
}

/// The event as a JSON object; see docs/events.md.
fn to_json(feature: Option<u16>, p: &Packet) -> serde_json::Value {
    use serde_json::json;
    let data = p.payload();
    let mut v = match (feature, receiver::parse_connection(p)) {
        (_, Some(c)) => json!({ "kind": "connection", "linked": c.linked, "wpid": format!("{:04X}", c.wpid) }),
        (Some(reprog::ID), _) => match reprog::parse_event(p.function(), data) {
            reprog::Event::Buttons(cids) => {
                json!({ "kind": "buttons", "held": cids.iter().map(|c| control(*c)).collect::<Vec<_>>() })
            }
            reprog::Event::RawXy { dx, dy } => json!({ "kind": "raw-xy", "dx": dx, "dy": dy }),
            reprog::Event::Other(_) => json!({ "kind": "other" }),
        },
        (Some(hires_wheel::ID), _) => match hires_wheel::parse_event(p.function(), data) {
            Some(hires_wheel::Event::Movement { hires, periods, delta }) => {
                json!({ "kind": "wheel", "delta": delta, "periods": periods, "hires": hires })
            }
            Some(hires_wheel::Event::RatchetSwitch { ratchet }) => json!({ "kind": "ratchet", "ratchet": ratchet }),
            None => json!({ "kind": "other" }),
        },
        (Some(thumb_wheel::ID), _) => {
            let e = thumb_wheel::parse_event(data);
            let rotation = match e.rotation {
                thumb_wheel::Rotation::Inactive => "inactive",
                thumb_wheel::Rotation::Start => "start",
                thumb_wheel::Rotation::Active => "active",
                thumb_wheel::Rotation::Stop => "stop",
                thumb_wheel::Rotation::Unknown(_) => "unknown",
            };
            json!({ "kind": "thumb-wheel", "delta": e.delta, "rotation": rotation, "touch": e.touch, "tap": e.single_tap })
        }
        (Some(battery::ID), _) => {
            let b = battery::Status::parse(data);
            json!({ "kind": "battery", "percent": b.percent, "charging": b.is_charging() })
        }
        _ => json!({ "kind": "other" }),
    };
    let time_ms =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
    let raw: String = p.encode().iter().map(|b| format!("{b:02x}")).collect();
    let o = v.as_object_mut().expect("built as an object");
    o.insert("v".into(), json!(JSON_VERSION));
    o.insert("time_ms".into(), json!(time_ms));
    o.insert("device".into(), json!(p.device));
    o.insert("feature".into(), json!(feature.map(|f| format!("{f:04X}"))));
    o.insert("event".into(), json!(p.function()));
    o.insert("raw".into(), json!(raw));
    v
}

/// What we changed, so it can be put back on exit.
#[derive(Default)]
struct Restore {
    controls: Vec<(u16, u16)>,
    wheel: Option<hires_wheel::Mode>,
    thumb: Option<thumb_wheel::Status>,
}

/// Records each change in `restore` as soon as it is made, so a failure part
/// way through can still be undone.
fn apply(dev: &mut Device, opts: &Options, restore: &mut Restore) -> Result<()> {
    for &cid in &opts.divert {
        let before = reprog::reporting(dev, cid).with_context(|| format!("reading control {cid:04X}"))?;
        let mut set = reprog::reporting::DIVERTED;
        if opts.raw_xy {
            set |= reprog::reporting::RAW_XY;
        }
        if opts.force_raw_xy {
            set |= reprog::reporting::FORCE_RAW_XY;
        }
        restore.controls.push((cid, before.flags));
        reprog::set_reporting(dev, cid, set, 0).with_context(|| format!("diverting control {cid:04X}"))?;
    }
    if opts.wheel {
        let before = hires_wheel::mode(dev)?;
        restore.wheel = Some(before);
        hires_wheel::set_mode(dev, hires_wheel::Mode { diverted: true, hires: opts.hires || before.hires, ..before })?;
    }
    if opts.thumb {
        let before = thumb_wheel::status(dev)?;
        restore.thumb = Some(before);
        thumb_wheel::set(dev, thumb_wheel::Status { diverted: true, ..before })?;
    }
    Ok(())
}

fn undo(dev: &mut Device, r: &Restore) {
    use reprog::reporting::{DIVERTED, FORCE_RAW_XY, RAW_XY};
    for &(cid, flags) in &r.controls {
        let set = flags & (DIVERTED | RAW_XY | FORCE_RAW_XY);
        let clear = (DIVERTED | RAW_XY | FORCE_RAW_XY) & !flags;
        if let Err(e) = reprog::set_reporting(dev, cid, set, clear) {
            eprintln!("could not restore control {cid:04X}: {e}");
        }
    }
    if let Some(m) = r.wheel
        && let Err(e) = hires_wheel::set_mode(dev, m)
    {
        eprintln!("could not restore wheel mode: {e}");
    }
    if let Some(s) = r.thumb
        && let Err(e) = thumb_wheel::set(dev, s)
    {
        eprintln!("could not restore thumb wheel: {e}");
    }
}

fn describe(feature: u16, p: &Packet) -> String {
    let params = p.payload();
    match feature {
        reprog::ID => match reprog::parse_event(p.function(), params) {
            reprog::Event::Buttons(cids) => {
                let names: Vec<String> = cids
                    .iter()
                    .map(|c| reprog::control_name(*c).map(str::to_string).unwrap_or(format!("{c:04X}")))
                    .collect();
                format!("buttons held: [{}]", names.join(", "))
            }
            reprog::Event::RawXy { dx, dy } => format!("raw xy: dx={dx} dy={dy}"),
            reprog::Event::Other(n) => format!("reprog event {n}"),
        },
        hires_wheel::ID => match hires_wheel::parse_event(p.function(), params) {
            Some(hires_wheel::Event::Movement { hires, periods, delta }) => {
                format!("wheel: delta={delta} periods={periods} hires={hires}")
            }
            Some(hires_wheel::Event::RatchetSwitch { ratchet }) => format!("wheel ratchet={ratchet}"),
            None => "wheel event".into(),
        },
        thumb_wheel::ID => {
            let e = thumb_wheel::parse_event(params);
            format!("thumb wheel: delta={} {:?} touch={} tap={}", e.delta, e.rotation, e.touch, e.single_tap)
        }
        battery::ID => format!("battery: {:?}", battery::Status::parse(params)),
        _ => String::new(),
    }
}

pub fn run(dev: &mut Device, events: Receiver<Incoming>, opts: Options) -> Result<()> {
    let features = dev.enumerate()?;
    let by_index: HashMap<u8, u16> = features.iter().map(|e| (e.slot.index, e.id)).collect();

    let stop = Arc::new(AtomicBool::new(false));
    {
        let stop = stop.clone();
        ctrlc::set_handler(move || stop.store(true, Ordering::SeqCst)).context("installing signal handler")?;
    }

    let mut restore = Restore::default();
    let me = dev.index();
    let result = apply(dev, &opts, &mut restore).and_then(|()| {
        if !opts.json {
            eprintln!("listening for events from device {me}; press Ctrl-C to stop");
        }
        while !stop.load(Ordering::SeqCst) {
            let p = match events.recv_timeout(Duration::from_millis(100)) {
                Ok(Incoming::Event(p)) => p,
                Ok(Incoming::Closed(e)) => return Err(anyhow!("device went away: {e}")),
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => return Err(anyhow!("event reader stopped")),
            };
            if p.device != me && p.device != 0xFF {
                continue;
            }
            let feature = by_index.get(&p.sub_id).copied().filter(|_| p.sub_id < 0x80);
            if opts.json {
                println!("{}", to_json(feature, &p));
                continue;
            }
            match by_index.get(&p.sub_id) {
                Some(&id) if p.sub_id < 0x80 => {
                    let name = policy::name(id).unwrap_or("unknown");
                    println!("{:04X} {:<22} ev{} {:<40} {:?}", id, name, p.function(), describe(id, &p), p);
                }
                _ => println!("notification {:?}", p),
            }
        }
        Ok(())
    });
    undo(dev, &restore);
    result
}

#[cfg(test)]
mod tests {
    use hidpp::ReportKind;

    use super::*;

    fn long(device: u8, sub_id: u8, address: u8, params: &[u8]) -> Packet {
        Packet::new(ReportKind::Long, device, sub_id, address, params).unwrap()
    }

    #[test]
    fn thumb_wheel_event_from_the_real_mouse() {
        // Captured from an MX Master 4 while turning the thumb wheel.
        let p = long(2, 0x13, 0x00, &[0x00, 0x02, 0x00, 0x00, 0x02, 0x02]);
        let v = to_json(Some(thumb_wheel::ID), &p);
        assert_eq!(v["kind"], "thumb-wheel");
        assert_eq!(v["delta"], 2);
        assert_eq!(v["rotation"], "active");
        assert_eq!(v["touch"], true);
        assert_eq!(v["v"], 1);
        assert_eq!(v["device"], 2);
        assert_eq!(v["feature"], "2150");
        assert!(v["raw"].as_str().unwrap().starts_with("11021300"));
    }

    #[test]
    fn buttons_and_connection() {
        let p = long(2, 0x0D, 0x00, &[0x00, 0xC3, 0x01, 0xA0]);
        let v = to_json(Some(reprog::ID), &p);
        assert_eq!(v["kind"], "buttons");
        assert_eq!(v["held"], serde_json::json!(["gesture", "haptic"]));

        let c = Packet::new(ReportKind::Short, 2, 0x41, 0x10, &[0x40, 0x42, 0xB0]).unwrap();
        let v = to_json(None, &c);
        assert_eq!(v["kind"], "connection");
        assert_eq!(v["linked"], false);
        assert_eq!(v["wpid"], "B042");
        assert!(v["feature"].is_null());
    }
}
