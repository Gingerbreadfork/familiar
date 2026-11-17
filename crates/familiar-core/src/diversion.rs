//! Diverting and remapping controls, and putting them back.
//!
//! Diversion lasts until the device loses power, so anything that diverts and
//! then dies leaves buttons dead. [`clear`] undoes every kind we use.

use std::collections::BTreeMap;

use hidpp::features::{hires_wheel, reprog, thumb_wheel};
use hidpp::{Error, Requester};

use crate::actions::{Action, Binding, MouseButton};

const CONTROL_FLAGS: u16 = reprog::reporting::DIVERTED | reprog::reporting::RAW_XY | reprog::reporting::FORCE_RAW_XY;

/// What [`clear`] changed, as names such as `gesture` or `thumb-wheel`.
pub type Cleared = Vec<String>;

/// Clears diversion on every control, the wheel and the thumb wheel. Returns
/// what was diverted before. Features the device lacks are skipped.
pub fn clear(dev: &mut impl Requester) -> Result<Cleared, Error> {
    let mut cleared = Vec::new();
    match reprog::controls(dev) {
        Ok(controls) => {
            for c in controls.iter().filter(|c| c.has(reprog::caps::DIVERTABLE) || c.has(reprog::caps::REPROGRAMMABLE))
            {
                let r = reprog::reporting(dev, c.cid)?;
                let name = reprog::control_name(c.cid).map(str::to_string).unwrap_or(format!("{:04X}", c.cid));
                if r.flags & CONTROL_FLAGS != 0 {
                    reprog::set_reporting(dev, c.cid, 0, r.flags & CONTROL_FLAGS)?;
                    cleared.push(name.clone());
                }
                if r.remapped_to != c.cid {
                    reprog::remap(dev, c.cid, c.cid)?;
                    cleared.push(format!("{name} remap"));
                }
            }
        }
        Err(Error::Unsupported(_)) => {}
        Err(e) => return Err(e),
    }
    match hires_wheel::mode(dev) {
        Ok(m) if m.diverted => {
            // Hi-res reports reaching a plain HID driver would scroll far too fast.
            hires_wheel::set_mode(dev, hires_wheel::Mode { diverted: false, hires: false, ..m })?;
            cleared.push("wheel".into());
        }
        Ok(_) | Err(Error::Unsupported(_)) => {}
        Err(e) => return Err(e),
    }
    match thumb_wheel::status(dev) {
        Ok(t) if t.diverted => {
            thumb_wheel::set(dev, thumb_wheel::Status { diverted: false, ..t })?;
            cleared.push("thumb-wheel".into());
        }
        Ok(_) | Err(Error::Unsupported(_)) => {}
        Err(e) => return Err(e),
    }
    Ok(cleared)
}

/// How one control should report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handling {
    /// Sent to us as HID++ events; with `motion`, also pointer movement while held.
    Divert { motion: bool },
    /// Acts as another control, inside the device.
    Remap(u16),
}

/// What to do with each control, by control ID, and the bindings that could
/// not be honoured.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub controls: BTreeMap<u16, Handling>,
    pub errors: Vec<String>,
}

fn mouse_control(b: MouseButton) -> u16 {
    match b {
        MouseButton::Left => 0x0050,
        MouseButton::Right => 0x0051,
        MouseButton::Middle => 0x0052,
        MouseButton::Back => 0x0053,
        MouseButton::Forward => 0x0056,
    }
}

/// Works out how to handle each bound button on a device with these controls.
/// A button bound to a mouse button is remapped in the device when it can be,
/// so it needs no injection and keeps working without the daemon.
pub fn plan(bindings: &BTreeMap<String, Binding>, controls: &[reprog::Control]) -> Plan {
    let mut out = Plan::default();
    for (name, binding) in bindings {
        let Some(cid) = reprog::control_by_name(name) else {
            out.errors.push(format!("{name}: not a control this program knows"));
            continue;
        };
        let Some(control) = controls.iter().find(|c| c.cid == cid) else {
            out.errors.push(format!("{name}: this device has no such button"));
            continue;
        };
        if let Binding::Direct(Action::Mouse(b)) = binding {
            let target = mouse_control(*b);
            if target == cid {
                continue;
            }
            if let Some(t) = controls.iter().find(|c| c.cid == target)
                && reprog::can_remap(control, t)
            {
                out.controls.insert(cid, Handling::Remap(target));
                continue;
            }
        }
        if !control.has(reprog::caps::DIVERTABLE) {
            out.errors.push(format!("{name}: this button cannot be reassigned"));
            continue;
        }
        let motion = binding.needs_motion();
        if motion && !control.has(reprog::caps::RAW_XY) {
            out.errors.push(format!("{name}: this button cannot report movement, so swipes are ignored"));
        }
        out.controls.insert(cid, Handling::Divert { motion: motion && control.has(reprog::caps::RAW_XY) });
    }
    out
}

/// Applies a plan, also undoing what `previous` did to controls this plan
/// handles differently or not at all. Every planned control is written, since
/// a device that lost power has forgotten them all.
pub fn apply(dev: &mut impl Requester, plan: &Plan, previous: &Plan) -> Vec<String> {
    use reprog::reporting::{DIVERTED, RAW_XY};
    let mut errors = Vec::new();
    for (&cid, old) in &previous.controls {
        let r = match (old, plan.controls.get(&cid)) {
            (Handling::Divert { .. }, None) => reprog::set_reporting(dev, cid, 0, DIVERTED | RAW_XY),
            (Handling::Remap(_), None | Some(Handling::Divert { .. })) => reprog::remap(dev, cid, cid),
            _ => continue,
        };
        if let Err(e) = r {
            errors.push(format!("{cid:04X}: {e}"));
        }
    }
    for (&cid, h) in &plan.controls {
        let r = match *h {
            Handling::Divert { motion: true } => reprog::set_reporting(dev, cid, DIVERTED | RAW_XY, 0),
            Handling::Divert { motion: false } => reprog::set_reporting(dev, cid, DIVERTED, RAW_XY),
            Handling::Remap(target) => {
                reprog::set_reporting(dev, cid, 0, DIVERTED | RAW_XY).and_then(|()| reprog::remap(dev, cid, target))
            }
        };
        if let Err(e) = r {
            let name = reprog::control_name(cid).map(str::to_string).unwrap_or(format!("{cid:04X}"));
            errors.push(format!("{name}: {e}"));
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprog::caps::*;

    fn control(cid: u16, caps: u16, group: u8, group_mask: u8) -> reprog::Control {
        reprog::Control { index: 0, cid, task: 0, caps, position: 0, group, group_mask }
    }

    /// The controls of an MX Master 4 as `familiar dump` lists them, without
    /// its gesture button, so tests have a known button that is missing.
    fn mx4() -> Vec<reprog::Control> {
        let side = REPROGRAMMABLE | DIVERTABLE | RAW_XY | MOUSE;
        vec![
            control(0x0050, MOUSE, 1, 0),
            control(0x0051, MOUSE, 1, 0),
            control(0x0052, side, 2, 0x03),
            control(0x0053, side, 2, 0x03),
            control(0x0056, side, 2, 0x03),
            control(0x00C4, side, 2, 0x03),
            control(0x01A0, side, 2, 0x03),
        ]
    }

    fn bindings(pairs: &[(&str, &str)]) -> BTreeMap<String, Binding> {
        pairs.iter().map(|(k, v)| (k.to_string(), Binding::Direct(v.parse().unwrap()))).collect()
    }

    #[test]
    fn remaps_mouse_buttons_and_diverts_the_rest() {
        let mut b = bindings(&[("back", "mouse:middle"), ("forward", "mouse:forward"), ("smartshift", "key:f24")]);
        b.insert(
            "haptic".into(),
            Binding::Gestures(Box::new(crate::actions::Gestures { up: Some(Action::None), ..Default::default() })),
        );
        let p = plan(&b, &mx4());
        assert_eq!(p.controls.get(&0x0053), Some(&Handling::Remap(0x0052)));
        assert_eq!(p.controls.get(&0x0056), None, "a button bound to itself is left alone");
        assert_eq!(p.controls.get(&0x00C4), Some(&Handling::Divert { motion: false }));
        assert_eq!(p.controls.get(&0x01A0), Some(&Handling::Divert { motion: true }));
        assert!(p.errors.is_empty(), "{:?}", p.errors);
    }

    #[test]
    fn reports_buttons_the_device_lacks() {
        let p = plan(&bindings(&[("gesture", "key:f24")]), &mx4());
        assert!(p.controls.is_empty());
        assert_eq!(p.errors.len(), 1);
    }
}
