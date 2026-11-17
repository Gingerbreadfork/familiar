//! The input engine: turns diverted button events into actions.
//!
//! Events arrive straight from each endpoint's reader thread, so a slow
//! settings request never delays a button. One thread serves every device.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io;
use std::path::PathBuf;
use std::process::Command as Process;
use std::str::FromStr;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use familiar_core::actions::{Action, Binding, Direction};
use familiar_core::gesture::{Recognizer, Timing, Trigger};
use familiar_core::scroll::{ThumbMode, Wheels};
use hidpp::Packet;
use hidpp::features::{hires_wheel, reprog, thumb_wheel};
use tracing::{debug, info, warn};

use crate::worker::{self, DeviceAction};

pub enum InputMsg {
    /// An event from a control or wheel feature, with its feature ID.
    Event {
        endpoint: PathBuf,
        feature: u16,
        packet: Packet,
    },
    /// How a device's diverted controls and wheels behave. Bindings are by control ID.
    Bindings {
        endpoint: PathBuf,
        index: u8,
        bindings: BTreeMap<u16, Binding>,
        wheels: Wheels,
        counts: WheelCounts,
        worker: worker::Sender,
    },
    /// The device went away; anything it held down is released.
    Offline {
        endpoint: PathBuf,
        index: u8,
    },
    /// While locked, actions are dropped.
    Locked(bool),
    Shutdown,
}

/// How many counts a diverted wheel reports per notch of its plain HID mode.
#[derive(Clone, Copy, Debug)]
pub struct WheelCounts {
    pub main: u16,
    pub thumb: u16,
}

impl Default for WheelCounts {
    fn default() -> Self {
        WheelCounts { main: 15, thumb: 6 }
    }
}

const REL_HWHEEL: u16 = 0x06;
const REL_WHEEL: u16 = 0x08;
const REL_WHEEL_HI_RES: u16 = 0x0b;
const REL_HWHEEL_HI_RES: u16 = 0x0c;
/// Hi-res scroll units per notch, fixed by the kernel interface.
const HI_RES_PER_NOTCH: i32 = 120;

/// Where injected keys, buttons and scrolling go.
pub trait Output: Send {
    /// Gets ready to send. Called when a device has something bound, so the
    /// desktop has picked the output up by the time the first event is sent.
    fn prepare(&mut self) -> io::Result<()> {
        Ok(())
    }
    fn key(&mut self, code: u16, down: bool) -> io::Result<()>;
    /// Relative axis events sent together as one frame.
    fn rel(&mut self, events: &[(u16, i32)]) -> io::Result<()>;
}

/// A virtual keyboard and mouse made with uinput, created when first needed.
#[derive(Default)]
pub struct Uinput {
    device: Option<evdev::uinput::VirtualDevice>,
}

impl Uinput {
    fn device(&mut self) -> io::Result<&mut evdev::uinput::VirtualDevice> {
        if self.device.is_none() {
            let mut keys = evdev::AttributeSet::<evdev::KeyCode>::new();
            for code in 1..=0x2ff_u16 {
                keys.insert(evdev::KeyCode::new(code));
            }
            let mut axes = evdev::AttributeSet::<evdev::RelativeAxisCode>::new();
            // X and Y are never sent, but without them libinput does not treat
            // the device as a pointer and ignores its wheel.
            for code in [0x00, 0x01, REL_HWHEEL, REL_WHEEL, REL_WHEEL_HI_RES, REL_HWHEEL_HI_RES] {
                axes.insert(evdev::RelativeAxisCode(code));
            }
            let device = evdev::uinput::VirtualDevice::builder()?
                .name("Familiar virtual input")
                .with_keys(&keys)?
                .with_relative_axes(&axes)?
                .build()?;
            info!("created uinput device");
            self.device = Some(device);
        }
        Ok(self.device.as_mut().expect("created above"))
    }
}

impl Output for Uinput {
    fn prepare(&mut self) -> io::Result<()> {
        self.device().map(|_| ())
    }

    fn key(&mut self, code: u16, down: bool) -> io::Result<()> {
        let ev = evdev::InputEvent::new(evdev::EventType::KEY.0, code, down as i32);
        self.device()?.emit(&[ev])
    }

    fn rel(&mut self, events: &[(u16, i32)]) -> io::Result<()> {
        let evs: Vec<evdev::InputEvent> =
            events.iter().map(|&(axis, v)| evdev::InputEvent::new(evdev::EventType::RELATIVE.0, axis, v)).collect();
        self.device()?.emit(&evs)
    }
}

/// Resolves an evdev name such as `KEY_LEFTCTRL` to its code.
pub fn key_code(name: &str) -> Option<u16> {
    evdev::KeyCode::from_str(name).ok().map(|k| k.code())
}

/// The codes for a shortcut, or `None` with a warning when a key is unknown,
/// so a shortcut is never sent with keys missing.
fn key_codes(names: &[String]) -> Option<Vec<u16>> {
    let codes: Option<Vec<u16>> = names.iter().map(|n| key_code(n)).collect();
    if codes.is_none() {
        warn!("unknown key in {}", names.join("+"));
    }
    codes
}

pub struct Handle {
    tx: Sender<InputMsg>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Handle {
    pub fn sender(&self) -> Sender<InputMsg> {
        self.tx.clone()
    }

    pub fn stop(mut self) {
        let _ = self.tx.send(InputMsg::Shutdown);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

pub fn spawn(output: Box<dyn Output>) -> io::Result<Handle> {
    let (tx, rx) = mpsc::channel();
    let thread = thread::Builder::new()
        .name("input".into())
        .spawn(move || Engine { output, devices: HashMap::new(), locked: false, timing: Timing::default() }.run(rx))?;
    Ok(Handle { tx, thread: Some(thread) })
}

/// What a button is doing right now that must be undone on release.
enum Held {
    Keys(Vec<u16>),
    DpiShift,
}

struct Control {
    name: &'static str,
    binding: Binding,
    recognizer: Option<Recognizer>,
    held: Option<Held>,
    cycle: usize,
}

struct DeviceInput {
    worker: worker::Sender,
    controls: HashMap<u16, Control>,
    pressed: HashSet<u16>,
    wheels: Wheels,
    counts: WheelCounts,
    /// Hi-res units not yet sent as whole low-res notches.
    main_rest: i32,
    thumb_rest: i32,
    /// Fractional thumb wheel counts carried between events.
    thumb_carry: f32,
    /// When the thumb landed on the thumb wheel, and whether it has turned since.
    touch: Option<(Instant, bool)>,
}

/// A short touch of the thumb wheel without turning it counts as a tap.
const THUMB_TAP: Duration = Duration::from_millis(250);

struct Engine {
    output: Box<dyn Output>,
    devices: HashMap<(PathBuf, u8), DeviceInput>,
    locked: bool,
    timing: Timing,
}

fn trigger_name(t: Trigger) -> &'static str {
    match t {
        Trigger::Tap => "tap",
        Trigger::Double => "double",
        Trigger::Hold => "hold",
        Trigger::Swipe(Direction::Up) => "up",
        Trigger::Swipe(Direction::Down) => "down",
        Trigger::Swipe(Direction::Left) => "left",
        Trigger::Swipe(Direction::Right) => "right",
        Trigger::Swipe(Direction::UpLeft) => "up-left",
        Trigger::Swipe(Direction::UpRight) => "up-right",
        Trigger::Swipe(Direction::DownLeft) => "down-left",
        Trigger::Swipe(Direction::DownRight) => "down-right",
    }
}

impl Engine {
    fn run(&mut self, rx: Receiver<InputMsg>) {
        loop {
            let deadline = self
                .devices
                .values()
                .flat_map(|d| d.controls.values())
                .filter_map(|c| c.recognizer.as_ref().and_then(Recognizer::deadline))
                .min();
            let wait = deadline.map_or(Duration::from_secs(3600), |d| d.saturating_duration_since(Instant::now()));
            match rx.recv_timeout(wait) {
                Ok(InputMsg::Shutdown) | Err(RecvTimeoutError::Disconnected) => {
                    let keys: Vec<(PathBuf, u8)> = self.devices.keys().cloned().collect();
                    for k in keys {
                        self.release_all(&k);
                    }
                    return;
                }
                Ok(msg) => self.handle(msg),
                Err(RecvTimeoutError::Timeout) => {}
            }
            self.tick(Instant::now());
        }
    }

    fn handle(&mut self, msg: InputMsg) {
        match msg {
            InputMsg::Event { endpoint, feature, packet } => match feature {
                reprog::ID => self.event(endpoint, &packet),
                hires_wheel::ID => self.wheel(endpoint, &packet),
                thumb_wheel::ID => self.thumb(endpoint, &packet),
                _ => {}
            },
            InputMsg::Bindings { endpoint, index, bindings, wheels, counts, worker } => {
                let key = (endpoint, index);
                self.release_all(&key);
                if (!bindings.is_empty() || wheels.scroll.smooth() || wheels.thumb.is_some())
                    && let Err(e) = self.output.prepare()
                {
                    warn!("cannot inject input: {e}");
                }
                let controls = bindings
                    .into_iter()
                    .map(|(cid, binding)| {
                        let recognizer = match &binding {
                            Binding::Gestures(g) => Some(Recognizer::new(g, self.timing)),
                            Binding::Direct(_) => None,
                        };
                        let name = reprog::control_name(cid).unwrap_or("control");
                        (cid, Control { name, binding, recognizer, held: None, cycle: 0 })
                    })
                    .collect();
                self.devices.insert(
                    key,
                    DeviceInput {
                        worker,
                        controls,
                        pressed: HashSet::new(),
                        wheels,
                        counts,
                        main_rest: 0,
                        thumb_rest: 0,
                        thumb_carry: 0.0,
                        touch: None,
                    },
                );
            }
            InputMsg::Offline { endpoint, index } => {
                let key = (endpoint, index);
                self.release_all(&key);
                if let Some(d) = self.devices.get_mut(&key) {
                    d.pressed.clear();
                    // A button that was down must not turn into a hold later.
                    for r in d.controls.values_mut().filter_map(|c| c.recognizer.as_mut()) {
                        r.cancel();
                    }
                }
            }
            InputMsg::Locked(locked) => {
                if locked != self.locked {
                    info!(locked, "screen lock changed");
                }
                self.locked = locked;
                if locked {
                    let keys: Vec<(PathBuf, u8)> = self.devices.keys().cloned().collect();
                    for k in keys {
                        self.release_all(&k);
                    }
                }
            }
            InputMsg::Shutdown => {}
        }
    }

    fn event(&mut self, endpoint: PathBuf, p: &Packet) {
        let key = (endpoint, p.device);
        let now = Instant::now();
        match reprog::parse_event(p.function(), p.payload()) {
            reprog::Event::Buttons(held) => {
                let Some(d) = self.devices.get_mut(&key) else { return };
                let now_held: HashSet<u16> = held.into_iter().collect();
                let pressed: Vec<u16> = now_held.difference(&d.pressed).copied().collect();
                let released: Vec<u16> = d.pressed.difference(&now_held).copied().collect();
                d.pressed = now_held;
                for cid in released {
                    self.release(&key, cid, now);
                }
                for cid in pressed {
                    self.press(&key, cid, now);
                }
            }
            reprog::Event::RawXy { dx, dy } => {
                if let Some(d) = self.devices.get_mut(&key) {
                    for cid in d.pressed.clone() {
                        if let Some(r) = d.controls.get_mut(&cid).and_then(|c| c.recognizer.as_mut()) {
                            r.motion(dx, dy);
                        }
                    }
                }
            }
            reprog::Event::Other(_) => {}
        }
    }

    /// Diverted main wheel: hi-res units, plus a low-res notch each time a
    /// whole one has built up, so programs that ignore hi-res still scroll.
    fn wheel(&mut self, endpoint: PathBuf, p: &Packet) {
        if self.locked {
            return;
        }
        let Some(d) = self.devices.get_mut(&(endpoint, p.device)) else { return };
        let Some(hires_wheel::Event::Movement { hires, delta, .. }) =
            hires_wheel::parse_event(p.function(), p.payload())
        else {
            return;
        };
        let per_notch = if hires { d.counts.main.max(1) as f32 } else { 1.0 };
        let units = (delta as f32 * HI_RES_PER_NOTCH as f32 / per_notch * d.wheels.scroll.speed()).round() as i32;
        if units == 0 {
            return;
        }
        d.main_rest += units;
        let notches = d.main_rest / HI_RES_PER_NOTCH;
        d.main_rest -= notches * HI_RES_PER_NOTCH;
        let mut frame = vec![(REL_WHEEL_HI_RES, units)];
        if notches != 0 {
            frame.push((REL_WHEEL, notches));
        }
        if let Err(e) = self.output.rel(&frame) {
            warn!("cannot inject scrolling: {e}");
        }
    }

    fn thumb(&mut self, endpoint: PathBuf, p: &Packet) {
        let key = (endpoint, p.device);
        let e = thumb_wheel::parse_event(p.payload());
        let now = Instant::now();
        let Some(d) = self.devices.get_mut(&key) else { return };
        let Some(cfg) = d.wheels.thumb.clone() else { return };

        let turned = e.delta != 0 || matches!(e.rotation, thumb_wheel::Rotation::Start | thumb_wheel::Rotation::Active);
        let mut tap = false;
        match (d.touch, e.touch) {
            (None, true) => d.touch = Some((now, turned)),
            (Some((since, was_turned)), true) => d.touch = Some((since, was_turned || turned)),
            (Some((since, was_turned)), false) => {
                tap = !was_turned && !turned && now.duration_since(since) < THUMB_TAP;
                d.touch = None;
            }
            (None, false) => {}
        }
        if tap && let Some(action) = cfg.tap.clone() {
            let worker = d.worker.clone();
            self.thumb_tap(&key, &action, worker);
            return;
        }
        if e.delta == 0 || self.locked {
            return;
        }
        let Some(d) = self.devices.get_mut(&key) else { return };
        let counts = e.delta as f32 * cfg.speed() + d.thumb_carry;
        let whole = counts.trunc() as i32;
        d.thumb_carry = counts - whole as f32;
        if whole == 0 {
            return;
        }
        let per_notch = d.counts.thumb.max(1) as i32;
        let r = match cfg.mode() {
            ThumbMode::Hscroll => {
                let units = whole * HI_RES_PER_NOTCH / per_notch;
                d.thumb_rest += units;
                let notches = d.thumb_rest / HI_RES_PER_NOTCH;
                d.thumb_rest -= notches * HI_RES_PER_NOTCH;
                let mut frame = vec![(REL_HWHEEL_HI_RES, units)];
                if notches != 0 {
                    frame.push((REL_HWHEEL, notches));
                }
                self.output.rel(&frame)
            }
            ThumbMode::None => Ok(()),
            mode => {
                d.thumb_rest += whole;
                let steps = d.thumb_rest / per_notch;
                d.thumb_rest -= steps * per_notch;
                self.thumb_steps(mode, steps)
            }
        };
        if let Err(e) = r {
            warn!("cannot inject thumb wheel action: {e}");
        }
    }

    /// One key press per notch for the step modes of the thumb wheel.
    fn thumb_steps(&mut self, mode: ThumbMode, steps: i32) -> io::Result<()> {
        let code = |n: &str| key_code(n).expect("known key name");
        for _ in 0..steps.unsigned_abs() {
            let up = steps > 0;
            match mode {
                ThumbMode::Volume => {
                    let k = code(if up { "KEY_VOLUMEUP" } else { "KEY_VOLUMEDOWN" });
                    self.output.key(k, true)?;
                    self.output.key(k, false)?;
                }
                ThumbMode::Zoom => {
                    let ctrl = code("KEY_LEFTCTRL");
                    let notch = if up { 1 } else { -1 };
                    self.output.key(ctrl, true)?;
                    self.output.rel(&[(REL_WHEEL_HI_RES, notch * HI_RES_PER_NOTCH), (REL_WHEEL, notch)])?;
                    self.output.key(ctrl, false)?;
                }
                ThumbMode::Tabs => {
                    let (ctrl, shift, tab) = (code("KEY_LEFTCTRL"), code("KEY_LEFTSHIFT"), code("KEY_TAB"));
                    self.output.key(ctrl, true)?;
                    if !up {
                        self.output.key(shift, true)?;
                    }
                    self.output.key(tab, true)?;
                    self.output.key(tab, false)?;
                    if !up {
                        self.output.key(shift, false)?;
                    }
                    self.output.key(ctrl, false)?;
                }
                ThumbMode::Hscroll | ThumbMode::None => {}
            }
        }
        Ok(())
    }

    /// Runs the thumb wheel's tap action, pressing and releasing at once.
    fn thumb_tap(&mut self, key: &(PathBuf, u8), action: &Action, worker: worker::Sender) {
        if self.locked {
            return;
        }
        match action {
            Action::Keys(names) | Action::Press(names) => {
                if let Some(codes) = key_codes(names) {
                    self.tap(&codes);
                }
            }
            Action::Mouse(b) => {
                if let Some(c) = key_code(b.evdev()) {
                    let _ = self.output.key(c, true);
                    let _ = self.output.key(c, false);
                }
            }
            Action::Shell(cmd) => run_shell(cmd, "thumb-wheel", "tap"),
            Action::Haptic(w) => worker.device(key.1, DeviceAction::Haptic(*w)),
            Action::Dpi(v) => worker.device(key.1, DeviceAction::Dpi(*v)),
            Action::SmartShiftToggle => worker.device(key.1, DeviceAction::SmartShiftToggle),
            Action::Host(n) => worker.device(key.1, DeviceAction::Host(*n)),
            Action::DpiCycle(_) | Action::DpiShift(_) | Action::None => {}
        }
    }

    fn press(&mut self, key: &(PathBuf, u8), cid: u16, now: Instant) {
        let Some(c) = self.devices.get_mut(key).and_then(|d| d.controls.get_mut(&cid)) else { return };
        match &c.binding {
            Binding::Direct(a) => {
                let a = a.clone();
                self.start(key, cid, &a, "press");
            }
            Binding::Gestures(g) if g.toggle.is_some() => {
                let action = g.toggle.clone().expect("checked above");
                if c.held.is_some() {
                    self.stop(key, cid);
                } else {
                    self.start(key, cid, &action, "toggle");
                }
            }
            Binding::Gestures(_) => {
                let fired = c.recognizer.as_mut().and_then(|r| r.press(now));
                if let Some(t) = fired {
                    self.fire(key, cid, t);
                }
            }
        }
    }

    fn release(&mut self, key: &(PathBuf, u8), cid: u16, now: Instant) {
        let Some(c) = self.devices.get_mut(key).and_then(|d| d.controls.get_mut(&cid)) else { return };
        if matches!(&c.binding, Binding::Gestures(g) if g.toggle.is_some()) {
            return;
        }
        let fired = c.recognizer.as_mut().and_then(|r| r.release(now));
        self.stop(key, cid);
        if let Some(t) = fired {
            self.fire(key, cid, t);
        }
    }

    fn tick(&mut self, now: Instant) {
        let mut fire = Vec::new();
        for (key, d) in &mut self.devices {
            for (cid, c) in &mut d.controls {
                if let Some(t) = c.recognizer.as_mut().and_then(|r| r.tick(now)) {
                    fire.push((key.clone(), *cid, t));
                }
            }
        }
        for (key, cid, t) in fire {
            self.fire(&key, cid, t);
        }
    }

    /// Runs the action for a recognised gesture. Held actions are pressed and
    /// released at once, except a hold, which lasts until the button is let go.
    fn fire(&mut self, key: &(PathBuf, u8), cid: u16, t: Trigger) {
        let Some(c) = self.devices.get(key).and_then(|d| d.controls.get(&cid)) else { return };
        let Binding::Gestures(g) = &c.binding else { return };
        let action = match t {
            Trigger::Tap => g.tap.clone(),
            Trigger::Double => g.double.clone(),
            Trigger::Hold => g.hold.clone(),
            Trigger::Swipe(d) => g.direction(d).cloned(),
        };
        let Some(action) = action else { return };
        let gesture = trigger_name(t);
        self.start(key, cid, &action, gesture);
        let still_down = self.devices.get(key).is_some_and(|d| d.pressed.contains(&cid));
        if !(t == Trigger::Hold && still_down) {
            self.stop(key, cid);
        }
    }

    /// Starts an action. Held actions are remembered so [`Engine::stop`] can end them.
    fn start(&mut self, key: &(PathBuf, u8), cid: u16, action: &Action, gesture: &str) {
        if self.locked {
            debug!("screen is locked; ignoring {action}");
            return;
        }
        let Some(d) = self.devices.get_mut(key) else { return };
        let Some(c) = d.controls.get_mut(&cid) else { return };
        debug!(control = c.name, gesture, %action, "action");
        match action {
            Action::None => {}
            Action::Keys(names) => {
                let Some(codes) = key_codes(names) else { return };
                // With modifiers, only they stay down, so Alt+Tab keeps the switcher
                // open without Tab repeating. A shortcut without them is held whole.
                let (mods, rest): (Vec<_>, Vec<_>) =
                    names.iter().zip(&codes).partition(|(n, _)| familiar_core::actions::is_modifier(n));
                let mods: Vec<u16> = mods.into_iter().map(|(_, &c)| c).collect();
                let rest: Vec<u16> = rest.into_iter().map(|(_, &c)| c).collect();
                let held = if mods.is_empty() || rest.is_empty() { codes } else { mods };
                for &code in &held {
                    if let Err(e) = self.output.key(code, true) {
                        warn!("cannot inject input: {e}");
                        return;
                    }
                }
                c.held = Some(Held::Keys(held.clone()));
                if held.len() < names.len() {
                    self.tap(&rest);
                }
            }
            Action::Press(names) => {
                if let Some(codes) = key_codes(names) {
                    self.tap(&codes);
                }
            }
            Action::Mouse(b) => {
                let Some(code) = key_code(b.evdev()) else { return };
                match self.output.key(code, true) {
                    Ok(()) => c.held = Some(Held::Keys(vec![code])),
                    Err(e) => warn!("cannot inject input: {e}"),
                }
            }
            Action::Shell(cmd) => run_shell(cmd, c.name, gesture),
            Action::DpiCycle(list) => {
                let dpi = list[c.cycle % list.len()];
                c.cycle += 1;
                d.worker.device(key.1, DeviceAction::Dpi(dpi));
            }
            Action::DpiShift(dpi) => {
                d.worker.device(key.1, DeviceAction::DpiShift(Some(*dpi)));
                c.held = Some(Held::DpiShift);
            }
            Action::Haptic(w) => d.worker.device(key.1, DeviceAction::Haptic(*w)),
            Action::Dpi(dpi) => d.worker.device(key.1, DeviceAction::Dpi(*dpi)),
            Action::SmartShiftToggle => d.worker.device(key.1, DeviceAction::SmartShiftToggle),
            Action::Host(n) => d.worker.device(key.1, DeviceAction::Host(*n)),
        }
    }

    fn stop(&mut self, key: &(PathBuf, u8), cid: u16) {
        let Some(d) = self.devices.get_mut(key) else { return };
        let Some(c) = d.controls.get_mut(&cid) else { return };
        match c.held.take() {
            Some(Held::Keys(codes)) => {
                for &code in codes.iter().rev() {
                    let _ = self.output.key(code, false);
                }
            }
            Some(Held::DpiShift) => d.worker.device(key.1, DeviceAction::DpiShift(None)),
            None => {}
        }
    }

    /// Presses keys in order and lets them go in reverse.
    fn tap(&mut self, codes: &[u16]) {
        for &code in codes {
            if let Err(e) = self.output.key(code, true) {
                warn!("cannot inject input: {e}");
                return;
            }
        }
        for &code in codes.iter().rev() {
            let _ = self.output.key(code, false);
        }
    }

    fn release_all(&mut self, key: &(PathBuf, u8)) {
        let cids: Vec<u16> = self.devices.get(key).map(|d| d.controls.keys().copied().collect()).unwrap_or_default();
        for cid in cids {
            self.stop(key, cid);
        }
    }
}

/// The command for a shell action, with what triggered it in its environment.
/// As a `unit` it is handed to systemd to run on its own.
fn shell_command(cmd: &str, control: &str, gesture: &str, unit: bool) -> Process {
    let env = [("FAMILIAR_CONTROL", control), ("FAMILIAR_GESTURE", gesture)];
    if unit {
        let mut p = Process::new("systemd-run");
        p.args(["--user", "--quiet", "--collect", "--no-block"]);
        p.args(env.map(|(k, v)| format!("--setenv={k}={v}")));
        p.args(["--", "sh", "-c", cmd]);
        p
    } else {
        let mut p = Process::new("sh");
        p.arg("-c").arg(cmd).envs(env);
        p
    }
}

/// Runs a shell action without waiting for it. Started by systemd, the daemon
/// may only use local sockets and its children are stopped with it, so the
/// command runs as a unit of its own. Otherwise it is a plain child.
fn run_shell(cmd: &str, control: &str, gesture: &str) {
    let unit = std::env::var_os("INVOCATION_ID").is_some();
    let (cmd, control, gesture) = (cmd.to_string(), control.to_string(), gesture.to_string());
    thread::spawn(move || {
        if unit {
            match shell_command(&cmd, &control, &gesture, true).status() {
                Ok(s) if s.success() => return,
                Ok(s) => debug!("systemd-run failed ({s}); running {cmd:?} directly"),
                Err(e) => debug!("systemd-run not usable ({e}); running {cmd:?} directly"),
            }
        }
        if let Err(e) = shell_command(&cmd, &control, &gesture, false).status() {
            warn!("cannot run {cmd:?}: {e}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_actions_say_what_triggered_them() {
        let plain = shell_command("echo hi", "back", "tap", false);
        assert_eq!(plain.get_program(), "sh");
        assert_eq!(plain.get_args().collect::<Vec<_>>(), ["-c", "echo hi"]);
        let env: Vec<_> = plain.get_envs().map(|(k, v)| (k.to_str().unwrap(), v.unwrap().to_str().unwrap())).collect();
        assert_eq!(env, [("FAMILIAR_CONTROL", "back"), ("FAMILIAR_GESTURE", "tap")]);

        let unit = shell_command("echo hi", "back", "tap", true);
        assert_eq!(unit.get_program(), "systemd-run");
        let args: Vec<_> = unit.get_args().map(|a| a.to_str().unwrap()).collect();
        assert!(args.contains(&"--setenv=FAMILIAR_CONTROL=back") && args.contains(&"--setenv=FAMILIAR_GESTURE=tap"));
        assert_eq!(&args[args.len() - 4..], ["--", "sh", "-c", "echo hi"], "the command is passed whole");
    }

    /// Injects F24, which nothing binds, and reads it back from the kernel.
    /// Skipped where `/dev/uinput` is not writable.
    #[test]
    fn uinput_keys_reach_evdev() {
        if std::fs::OpenOptions::new().write(true).open("/dev/uinput").is_err() {
            eprintln!("skipping: /dev/uinput is not writable");
            return;
        }
        let mut out = Uinput::default();
        let node = out.device().unwrap().enumerate_dev_nodes_blocking().unwrap().flatten().next().unwrap();
        // udev sets the new node's permissions a moment after it appears.
        let mut reader = (0..50)
            .find_map(|_| {
                evdev::Device::open(&node).ok().or_else(|| {
                    std::thread::sleep(Duration::from_millis(20));
                    None
                })
            })
            .expect("the new event node becomes readable");
        let f24 = key_code("KEY_F24").unwrap();
        out.key(f24, true).unwrap();
        out.key(f24, false).unwrap();
        let mut seen = Vec::new();
        while seen.len() < 2 {
            for ev in reader.fetch_events().unwrap() {
                if ev.event_type() == evdev::EventType::KEY {
                    seen.push((ev.code(), ev.value()));
                }
            }
        }
        assert_eq!(seen, vec![(f24, 1), (f24, 0)]);

        out.rel(&[(REL_WHEEL_HI_RES, 60)]).unwrap();
        let mut wheel = None;
        while wheel.is_none() {
            for ev in reader.fetch_events().unwrap() {
                if ev.event_type() == evdev::EventType::RELATIVE {
                    wheel = Some((ev.code(), ev.value()));
                }
            }
        }
        assert_eq!(wheel, Some((REL_WHEEL_HI_RES, 60)));
    }
}

#[cfg(test)]
pub mod testing {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use super::*;

    /// Records injected keys and scroll frames instead of sending them
    /// anywhere, and counts how often it was told to get ready.
    #[derive(Clone, Default)]
    pub struct Recorder(pub Arc<Mutex<Vec<(u16, bool)>>>, pub Arc<Mutex<Vec<Frame>>>, pub Arc<AtomicUsize>);

    pub type Frame = Vec<(u16, i32)>;

    impl Output for Recorder {
        fn prepare(&mut self) -> io::Result<()> {
            self.2.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn key(&mut self, code: u16, down: bool) -> io::Result<()> {
            self.0.lock().unwrap().push((code, down));
            Ok(())
        }

        fn rel(&mut self, events: &[(u16, i32)]) -> io::Result<()> {
            self.1.lock().unwrap().push(events.to_vec());
            Ok(())
        }
    }
}
