//! One thread per HID++ endpoint. It applies settings whenever a device
//! connects and reports what it sees to the manager.
//!
//! The endpoint's [`Link`] reads in its own thread, so events and commands
//! arrive here on one queue and are never dropped while a request is waiting.

use std::collections::{BTreeMap, HashMap};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use familiar_core::config::{Config, Identity};
use familiar_core::diversion::{self, Handling, Plan};
use familiar_core::settings::{self, DeviceSettings};
use hidpp::features::{battery, dpi, haptic, hires_wheel, hosts, info, reprog, smartshift, thumb_wheel};
use hidpp::{DAEMON_SWIDS, DIRECT, Device, Error, Exchange, Incoming, Link, Packet, Transport, receiver};
use hidpp_hidraw::{BUS_USB, Hidraw, HidrawInfo};
use tokio::sync::{mpsc::UnboundedSender, oneshot};
use tracing::{debug, error, info, warn};

use crate::input::{InputMsg, WheelCounts};

const WIRELESS_STATUS: u16 = 0x1D4B;
const RETRIES: u8 = 4;
const RETRY_DELAY: Duration = Duration::from_millis(300);

pub type Reply<T> = oneshot::Sender<Result<T, String>>;

/// The current channel, and each channel as (channel, paired, name), all from 0.
pub type HostList = (u8, Vec<(u8, bool, String)>);

pub enum Command {
    /// New config. `done` is answered once every online device has it.
    Configure {
        config: Arc<Config>,
        done: Option<Reply<()>>,
    },
    Set {
        index: u8,
        settings: DeviceSettings,
        reply: Reply<DeviceSettings>,
    },
    PlayHaptic {
        index: u8,
        waveform: haptic::Waveform,
        reply: Reply<()>,
    },
    SwitchHost {
        index: u8,
        host: u8,
        reply: Reply<()>,
    },
    Reapply {
        index: u8,
        reply: Reply<DeviceSettings>,
    },
    /// The current channel and every channel's pairing and name.
    Hosts {
        index: u8,
        reply: Reply<HostList>,
    },
    RenameHost {
        index: u8,
        host: u8,
        name: String,
        reply: Reply<String>,
    },
    /// Another endpoint already manages this device; leave it alone.
    Ignore {
        index: u8,
    },
    /// The endpoint that managed an ignored device has lost it; take it up here.
    Resume {
        index: u8,
    },
    /// Something a button asked the device to do.
    Device {
        index: u8,
        action: DeviceAction,
    },
    Shutdown,
}

/// Actions the input engine hands back to the device's worker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceAction {
    Haptic(haptic::Waveform),
    Dpi(u16),
    /// Use this DPI until called again with `None`, which restores the previous one.
    DpiShift(Option<u16>),
    SmartShiftToggle,
    /// Easy-Switch channel, numbered from 1.
    Host(u8),
}

/// A clonable way to send commands to a worker.
#[derive(Clone)]
pub struct Sender(mpsc::Sender<Msg>);

impl Sender {
    pub fn device(&self, index: u8, action: DeviceAction) {
        let _ = self.0.send(Msg::Command(Command::Device { index, action }));
    }
}

#[derive(Debug)]
pub enum Update {
    Online {
        endpoint: PathBuf,
        index: u8,
        identity: Identity,
        settings: DeviceSettings,
        errors: Vec<String>,
        /// Time from the connection notice to settings being in place, when
        /// this was a reconnect rather than the daemon starting.
        latency: Option<Duration>,
    },
    Offline {
        endpoint: PathBuf,
        index: u8,
    },
    Battery {
        endpoint: PathBuf,
        index: u8,
        status: battery::Status,
    },
    Settings {
        endpoint: PathBuf,
        index: u8,
        settings: DeviceSettings,
        errors: Vec<String>,
    },
    Gone {
        endpoint: PathBuf,
    },
}

pub(crate) enum Msg {
    Command(Command),
    Link(Incoming),
    Retry { index: u8, attempt: u8, since: Instant },
}

/// The manager's handle on a running worker.
pub struct Handle {
    tx: mpsc::Sender<Msg>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Handle {
    pub fn send(&self, cmd: Command) {
        let _ = self.tx.send(Msg::Command(cmd));
    }

    pub fn stop(mut self) {
        self.send(Command::Shutdown);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

struct Slot {
    device: Device,
    cache: settings::Cache,
    /// Leftover diversion from a crashed program has been cleared.
    undiverted: bool,
    identity: Identity,
    features: HashMap<u8, u16>,
    controls: Vec<reprog::Control>,
    /// How each bound control is handled now.
    plan: Plan,
    /// Bindings and wheel modes the device could not take.
    button_errors: Vec<String>,
    counts: WheelCounts,
    /// The wheel mode from before we took the wheel over, to put back after.
    wheel_before: Option<hires_wheel::Mode>,
    thumb_diverted: bool,
    /// The DPI to go back to when a DPI shift ends.
    dpi_before_shift: Option<u16>,
    online: bool,
    ignored: bool,
}

/// Feature IDs by (device index, feature index) for events that go to the input engine.
type Routes = Arc<RwLock<HashMap<(u8, u8), u16>>>;

struct Worker {
    endpoint: PathBuf,
    link: Arc<Link>,
    receiver: bool,
    config: Arc<Config>,
    slots: HashMap<u8, Slot>,
    /// Last time we tried to identify an unknown index, to avoid retry storms.
    tried: HashMap<u8, Instant>,
    updates: UnboundedSender<Update>,
    tx: mpsc::Sender<Msg>,
    input: Option<mpsc::Sender<InputMsg>>,
    routes: Routes,
}

pub fn spawn(
    info: HidrawInfo,
    config: Arc<Config>,
    updates: UnboundedSender<Update>,
    input: Option<mpsc::Sender<InputMsg>>,
) -> std::io::Result<Handle> {
    let transport = Hidraw::open(&info)?;
    let receiver = info.bus == BUS_USB && receiver::is_receiver(info.product);
    spawn_with(Box::new(transport), info.path, receiver, config, updates, input)
}

/// Starts a worker on any transport. `receiver` says whether devices sit at
/// indices 1 to 6 behind it, or directly at index 0xFF.
pub fn spawn_with(
    transport: Box<dyn Transport>,
    endpoint: PathBuf,
    receiver: bool,
    config: Arc<Config>,
    updates: UnboundedSender<Update>,
    input: Option<mpsc::Sender<InputMsg>>,
) -> std::io::Result<Handle> {
    let (tx, rx) = mpsc::channel();
    let sink = tx.clone();
    let routes: Routes = Arc::default();
    let (sink_routes, sink_input, sink_endpoint) = (routes.clone(), input.clone(), endpoint.clone());
    let link = Link::new(transport, DAEMON_SWIDS, move |i| {
        // Button events skip the worker's queue so settings work never delays them.
        if let (Incoming::Event(p), Some(input)) = (&i, &sink_input)
            && let Some(feature) = sink_routes.read().ok().and_then(|r| r.get(&(p.device, p.sub_id)).copied())
        {
            let _ = input.send(InputMsg::Event { endpoint: sink_endpoint.clone(), feature, packet: p.clone() });
            return;
        }
        let _ = sink.send(Msg::Link(i));
    })?
    .with_timeout(Duration::from_millis(1500));
    let worker_tx = tx.clone();
    let thread = thread::Builder::new().name(format!("hidpp {}", endpoint.display())).spawn(move || {
        let gone = (endpoint.clone(), updates.clone());
        let mut w = Worker {
            endpoint,
            link: Arc::new(link),
            receiver,
            config,
            slots: HashMap::new(),
            tried: HashMap::new(),
            updates,
            tx: worker_tx,
            input,
            routes,
        };
        if catch_unwind(AssertUnwindSafe(|| w.run(rx))).is_err() {
            error!(endpoint = %gone.0.display(), "device thread crashed");
            let _ = gone.1.send(Update::Gone { endpoint: gone.0 });
        }
    })?;
    Ok(Handle { tx, thread: Some(thread) })
}

fn identify(dev: &mut Device) -> Result<(Identity, HashMap<u8, u16>), Error> {
    let features: HashMap<u8, u16> = dev.enumerate()?.into_iter().map(|e| (e.slot.index, e.id)).collect();
    let name = info::name(dev).unwrap_or_default();
    let mut models: Vec<String> = Vec::new();
    let mut unit_id = String::new();
    if let Ok(ids) = info::ids(dev) {
        unit_id = ids.unit_id;
        for (_, m) in ids.transports {
            if !models.contains(&m) {
                models.push(m);
            }
        }
    }
    Ok((Identity { unit_id, models, name }, features))
}

/// Puts every control back to plain reporting, logging what was left diverted.
fn undivert(index: u8, dev: &mut Device) {
    match diversion::clear(dev) {
        Ok(cleared) if !cleared.is_empty() => info!(index, "cleared diversion: {}", cleared.join(", ")),
        Ok(_) => {}
        Err(e) => warn!(index, "could not clear diversion: {e}"),
    }
}

/// Diverts or remaps the bound buttons and tells the input engine what each
/// diverted one does. Returns what could not be done.
fn apply_buttons(
    slot: &mut Slot,
    config: &Config,
    endpoint: &std::path::Path,
    index: u8,
    input: Option<&mpsc::Sender<InputMsg>>,
    tx: &mpsc::Sender<Msg>,
) -> Vec<String> {
    let bindings = config.buttons("default").unwrap_or_default();
    let wheels = config.wheels("default").unwrap_or_default();
    let (smooth, thumb) = (wheels.scroll.smooth(), wheels.thumb.is_some());
    let idle = slot.plan.controls.is_empty() && slot.wheel_before.is_none() && !slot.thumb_diverted;
    if bindings.is_empty() && !smooth && !thumb && idle {
        return Vec::new();
    }
    let plan = diversion::plan(&bindings, &slot.controls);
    let mut errors = plan.errors.clone();
    errors.extend(diversion::apply(&mut slot.device, &plan, &slot.plan));
    errors.extend(apply_wheels(slot, smooth, thumb));
    let diverted: BTreeMap<u16, _> = plan
        .controls
        .iter()
        .filter(|(_, h)| matches!(h, Handling::Divert { .. }))
        .filter_map(|(cid, _)| {
            let name = reprog::control_name(*cid)?;
            bindings.get(name).map(|b| (*cid, b.clone()))
        })
        .collect();
    slot.plan = plan;
    if let Some(input) = input {
        let msg = InputMsg::Bindings {
            endpoint: endpoint.to_path_buf(),
            index,
            bindings: diverted,
            wheels,
            counts: slot.counts,
            worker: Sender(tx.clone()),
        };
        let _ = input.send(msg);
    }
    errors
}

/// Takes over or gives back the main wheel and the thumb wheel. Written every
/// time, since a device that lost power has forgotten.
fn apply_wheels(slot: &mut Slot, smooth: bool, thumb: bool) -> Vec<String> {
    let mut errors = Vec::new();
    if smooth || slot.wheel_before.is_some() {
        let r = hires_wheel::mode(&mut slot.device).and_then(|m| {
            if smooth {
                slot.wheel_before.get_or_insert(hires_wheel::Mode { diverted: false, ..m });
                // Direction is a device setting and stays as it is now.
                hires_wheel::set_mode(&mut slot.device, hires_wheel::Mode { diverted: true, hires: true, ..m })
            } else {
                let before = slot.wheel_before.take().unwrap_or_default();
                hires_wheel::set_mode(&mut slot.device, hires_wheel::Mode { inverted: m.inverted, ..before })
            }
        });
        if let Err(e) = r {
            errors.push(format!("smooth scrolling: {e}"));
        }
    }
    if thumb || slot.thumb_diverted {
        let r = thumb_wheel::status(&mut slot.device)
            .and_then(|t| thumb_wheel::set(&mut slot.device, thumb_wheel::Status { diverted: thumb, ..t }));
        match r {
            Ok(()) => slot.thumb_diverted = thumb,
            Err(e) => errors.push(format!("thumb wheel: {e}")),
        }
    }
    errors
}

fn errors_to_strings(errors: Vec<(&'static str, Error)>) -> Vec<String> {
    errors.into_iter().map(|(n, e)| format!("{n}: {e}")).collect()
}

impl Worker {
    fn run(&mut self, rx: Receiver<Msg>) {
        info!(endpoint = %self.endpoint.display(), receiver = self.receiver, "endpoint opened");
        if self.receiver
            && let Err(e) = receiver::enable_notifications(self.link.as_ref())
        {
            warn!("could not enable receiver notifications: {e}");
        }
        let indices: Vec<u8> = if self.receiver { receiver::SLOTS.collect() } else { vec![DIRECT] };
        for index in indices {
            self.connect(index, None, None);
        }

        while let Ok(msg) = rx.recv() {
            match msg {
                Msg::Command(Command::Shutdown) => {
                    for (index, slot) in self.slots.iter_mut().filter(|(_, s)| s.online && !s.ignored) {
                        undivert(*index, &mut slot.device);
                    }
                    return;
                }
                Msg::Command(cmd) => self.command(cmd),
                Msg::Link(Incoming::Event(p)) => self.event(p),
                Msg::Link(Incoming::Closed(e)) => {
                    info!(endpoint = %self.endpoint.display(), "endpoint closed: {e}");
                    let _ = self.updates.send(Update::Gone { endpoint: self.endpoint.clone() });
                    return;
                }
                Msg::Retry { index, attempt, since } => self.connect(index, Some(attempt), Some(since)),
            }
        }
    }

    fn schedule_retry(&self, index: u8, attempt: u8, since: Instant) {
        let tx = self.tx.clone();
        thread::spawn(move || {
            thread::sleep(RETRY_DELAY);
            let _ = tx.send(Msg::Retry { index, attempt, since });
        });
    }

    /// Identifies a device if needed, applies its settings and reports it online.
    /// `attempt` is set when the device is expected to be there, so a failed ping
    /// is retried a few times without blocking this thread. `since` is when the
    /// device announced itself, for measuring how quickly it was ready.
    fn connect(&mut self, index: u8, attempt: Option<u8>, since: Option<Instant>) {
        if self.slots.get(&index).is_some_and(|s| s.ignored) {
            return;
        }
        let mut dev = Device::new(self.link.clone() as Arc<dyn Exchange>, index);
        let major = match dev.ping() {
            Ok((major, _)) => major,
            Err(e) => {
                debug!(index, ?attempt, "ping failed: {e}");
                if let Some(n) = attempt
                    && n + 1 < RETRIES
                {
                    self.schedule_retry(index, n + 1, since.unwrap_or_else(Instant::now));
                }
                return;
            }
        };
        if major < 2 {
            debug!(index, "ignoring HID++ 1.0 device");
            return;
        }

        let config = self.config.clone();
        let slot = match self.slots.entry(index) {
            std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
            std::collections::hash_map::Entry::Vacant(e) => match identify(&mut dev) {
                Ok((identity, features)) => {
                    info!(index, name = %identity.name, unit = %identity.unit_id, "device found");
                    let controls = reprog::controls(&mut dev).unwrap_or_default();
                    if let Ok(mut r) = self.routes.write() {
                        r.retain(|(i, _), _| *i != index);
                        for (&fi, &id) in &features {
                            if matches!(id, reprog::ID | hires_wheel::ID | thumb_wheel::ID) {
                                r.insert((index, fi), id);
                            }
                        }
                    }
                    let mut counts = WheelCounts::default();
                    if let Ok(c) = hires_wheel::capabilities(&mut dev) {
                        counts.main = c.multiplier.max(1) as u16;
                    }
                    if let Ok(t) = thumb_wheel::info(&mut dev)
                        && t.native_resolution > 0
                    {
                        counts.thumb = (t.diverted_resolution / t.native_resolution).max(1);
                    }
                    e.insert(Slot {
                        device: dev,
                        cache: settings::Cache::default(),
                        undiverted: false,
                        identity,
                        features,
                        controls,
                        plan: Plan::default(),
                        button_errors: Vec::new(),
                        counts,
                        wheel_before: None,
                        thumb_diverted: false,
                        dpi_before_shift: None,
                        online: false,
                        ignored: false,
                    })
                }
                Err(err) => {
                    warn!(index, ?attempt, "could not identify device: {err}");
                    if let Some(n) = attempt
                        && n + 1 < RETRIES
                    {
                        self.schedule_retry(index, n + 1, since.unwrap_or_else(Instant::now));
                    }
                    return;
                }
            },
        };
        // Leftovers from a crashed program go first, so they cannot undo our own diversion.
        if !slot.undiverted {
            slot.undiverted = true;
            undivert(index, &mut slot.device);
        }
        // Buttons first: they are what the user notices after a wake.
        let button_errors = apply_buttons(slot, &config, &self.endpoint, index, self.input.as_ref(), &self.tx);
        slot.button_errors = button_errors;
        let mut errors = slot.button_errors.clone();
        let want = config.settings_for(&slot.identity);
        let applied = settings::apply_cached(&mut slot.device, &want, &mut slot.cache);
        let latency = since.map(|t| t.elapsed());
        errors.extend(errors_to_strings(applied.errors));
        for e in &errors {
            warn!(index, "setting not applied: {e}");
        }
        if let Some(l) = latency {
            info!(index, ms = l.as_millis() as u64, "settings in place after reconnect");
        }
        slot.online = true;
        let _ = self.updates.send(Update::Online {
            endpoint: self.endpoint.clone(),
            index,
            identity: slot.identity.clone(),
            settings: applied.current,
            errors: errors.clone(),
            latency,
        });
        // Everything not needed to get the device right is read afterwards.
        if let Ok(status) = battery::status(&mut slot.device) {
            let _ = self.updates.send(Update::Battery { endpoint: self.endpoint.clone(), index, status });
        }
        let settings = settings::read(&mut slot.device);
        let _ = self.updates.send(Update::Settings { endpoint: self.endpoint.clone(), index, settings, errors });
    }

    fn offline(&mut self, index: u8) {
        if let Some(slot) = self.slots.get_mut(&index)
            && slot.online
        {
            slot.online = false;
            info!(index, "device went offline");
            if let Some(input) = &self.input {
                let _ = input.send(InputMsg::Offline { endpoint: self.endpoint.clone(), index });
            }
            let _ = self.updates.send(Update::Offline { endpoint: self.endpoint.clone(), index });
        }
    }

    fn event(&mut self, p: Packet) {
        if let Some(c) = receiver::parse_connection(&p) {
            debug!(?c, "connection notification");
            // A different device paired into the same slot must be identified afresh.
            let wpid = format!("{:04X}", c.wpid);
            if let Some(slot) = self.slots.get(&c.index)
                && !slot.identity.models.is_empty()
                && !slot.identity.models.contains(&wpid)
            {
                info!(index = c.index, %wpid, "a different device is now in this slot");
                self.offline(c.index);
                self.slots.remove(&c.index);
                self.forget_routes(c.index);
            }
            if c.linked {
                self.connect(c.index, Some(0), Some(Instant::now()));
            } else {
                self.offline(c.index);
            }
            return;
        }
        let index = p.device;
        let Some(slot) = self.slots.get(&index) else {
            // A device we have not met yet is talking; it probably just woke up.
            let recent = self.tried.get(&index).is_some_and(|t| t.elapsed() < Duration::from_secs(10));
            if p.sub_id < 0x80 && !recent {
                self.tried.insert(index, Instant::now());
                self.connect(index, Some(0), Some(Instant::now()));
            }
            return;
        };
        if slot.ignored {
            return;
        }
        let Some(&feature) = slot.features.get(&p.sub_id) else { return };
        let data = p.payload();
        match feature {
            battery::ID => {
                let status = battery::Status::parse(data);
                let _ = self.updates.send(Update::Battery { endpoint: self.endpoint.clone(), index, status });
            }
            WIRELESS_STATUS if p.function() == 0 && data[0] == 1 => {
                debug!(index, "device reports reconnection");
                self.connect(index, Some(0), Some(Instant::now()));
            }
            _ => {
                if !slot.online {
                    self.connect(index, Some(0), Some(Instant::now()));
                }
            }
        }
    }

    /// Stops sending this index's events straight to the input engine.
    fn forget_routes(&self, index: u8) {
        if let Ok(mut r) = self.routes.write() {
            r.retain(|(i, _), _| *i != index);
        }
    }

    fn device_action(&mut self, index: u8, action: DeviceAction) -> Result<(), String> {
        self.with_slot(index, |s| match action {
            DeviceAction::Haptic(w) => haptic::play(&mut s.device, w),
            DeviceAction::Dpi(v) => dpi::set(&mut s.device, 0, v),
            DeviceAction::DpiShift(Some(v)) => {
                if s.dpi_before_shift.is_none() {
                    s.dpi_before_shift = Some(dpi::get(&mut s.device, 0)?.current);
                }
                dpi::set(&mut s.device, 0, v)
            }
            DeviceAction::DpiShift(None) => match s.dpi_before_shift.take() {
                Some(v) => dpi::set(&mut s.device, 0, v),
                None => Ok(()),
            },
            DeviceAction::SmartShiftToggle => {
                let mut st = smartshift::get(&mut s.device)?;
                st.mode = match st.mode {
                    smartshift::WheelMode::Ratchet => smartshift::WheelMode::FreeSpin,
                    smartshift::WheelMode::FreeSpin => smartshift::WheelMode::Ratchet,
                };
                smartshift::set(&mut s.device, st)
            }
            DeviceAction::Host(n) => hosts::switch_to(&mut s.device, n.saturating_sub(1)),
        })
    }

    fn with_slot<T>(&mut self, index: u8, f: impl FnOnce(&mut Slot) -> Result<T, Error>) -> Result<T, String> {
        let r = match self.slots.get_mut(&index) {
            Some(slot) if slot.online && !slot.ignored => f(slot),
            _ => return Err("device is asleep or disconnected".into()),
        };
        match r {
            Ok(v) => Ok(v),
            Err(e) => {
                if matches!(&e, Error::Timeout) || matches!(&e, Error::Register(r) if r.is_unreachable()) {
                    self.offline(index);
                }
                Err(e.to_string())
            }
        }
    }

    /// Applies settings and reports the device's state. The report also lists
    /// bindings that are still not in place; the result only what failed here.
    fn apply_and_report(&mut self, index: u8, want: &DeviceSettings) -> Result<DeviceSettings, String> {
        let endpoint = self.endpoint.clone();
        let updates = self.updates.clone();
        let (current, errors, standing) = self.with_slot(index, |slot| {
            let started = Instant::now();
            let applied = settings::apply_cached(&mut slot.device, want, &mut slot.cache);
            debug!(index, ms = started.elapsed().as_millis() as u64, "settings applied");
            Ok((settings::read(&mut slot.device), errors_to_strings(applied.errors), slot.button_errors.clone()))
        })?;
        let reported = standing.into_iter().chain(errors.iter().cloned()).collect();
        let _ = updates.send(Update::Settings { endpoint, index, settings: current.clone(), errors: reported });
        if errors.is_empty() { Ok(current) } else { Err(errors.join("; ")) }
    }

    fn command(&mut self, cmd: Command) {
        match cmd {
            Command::Configure { config, done } => {
                self.config = config;
                let online: Vec<u8> =
                    self.slots.iter().filter(|(_, s)| s.online && !s.ignored).map(|(i, _)| *i).collect();
                for index in online {
                    let config = self.config.clone();
                    let (endpoint, input, tx) = (self.endpoint.clone(), self.input.clone(), self.tx.clone());
                    if let Some(slot) = self.slots.get_mut(&index) {
                        let errors = apply_buttons(slot, &config, &endpoint, index, input.as_ref(), &tx);
                        for e in &errors {
                            warn!(index, "setting not applied: {e}");
                        }
                        slot.button_errors = errors;
                    }
                    let want = config.settings_for(&self.slots[&index].identity);
                    let _ = self.apply_and_report(index, &want);
                }
                if let Some(done) = done {
                    let _ = done.send(Ok(()));
                }
            }
            Command::Set { index, settings, reply } => {
                let _ = reply.send(self.apply_and_report(index, &settings));
            }
            Command::Reapply { index, reply } => {
                let want = match self.slots.get(&index) {
                    Some(s) => self.config.settings_for(&s.identity),
                    None => DeviceSettings::default(),
                };
                let _ = reply.send(self.apply_and_report(index, &want));
            }
            Command::PlayHaptic { index, waveform, reply } => {
                let _ = reply.send(self.with_slot(index, |s| haptic::play(&mut s.device, waveform)));
            }
            Command::SwitchHost { index, host, reply } => {
                let r = self.with_slot(index, |s| {
                    let state = hosts::state(&mut s.device)?;
                    if host >= state.count {
                        return Err(Error::Invalid(format!("channel must be between 1 and {}", state.count)));
                    }
                    hosts::switch_to(&mut s.device, host)
                });
                let _ = reply.send(r);
            }
            Command::Hosts { index, reply } => {
                let r = self.with_slot(index, |s| {
                    let state = hosts::state(&mut s.device)?;
                    let list = hosts::hosts(&mut s.device)?;
                    Ok((state.current, list.into_iter().map(|h| (h.index, h.paired, h.name)).collect()))
                });
                let _ = reply.send(r);
            }
            Command::RenameHost { index, host, name, reply } => {
                let _ = reply.send(self.with_slot(index, |s| hosts::rename(&mut s.device, host, &name)));
            }
            Command::Ignore { index } => {
                if let Some(s) = self.slots.get_mut(&index) {
                    s.ignored = true;
                    s.online = false;
                }
                self.forget_routes(index);
            }
            Command::Resume { index } => {
                // Identified afresh, so its routes and diversion are set up again.
                if self.slots.get(&index).is_some_and(|s| s.ignored) {
                    self.slots.remove(&index);
                    info!(index, "taking up a device another endpoint managed");
                    self.connect(index, Some(0), Some(Instant::now()));
                }
            }
            Command::Device { index, action } => {
                if let Err(e) = self.device_action(index, action) {
                    warn!(index, ?action, "button action failed: {e}");
                }
            }
            Command::Shutdown => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hidpp::sim::{DEFAULT_DPI, Sim};
    use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

    fn config(dpi: u16) -> Arc<Config> {
        Arc::new(toml::from_str(&format!("[device.\"1A2B3C4D\"]\ndpi = {dpi}\n")).unwrap())
    }

    async fn next(rx: &mut UnboundedReceiver<Update>) -> Update {
        tokio::time::timeout(Duration::from_secs(5), rx.recv()).await.expect("worker update").expect("open")
    }

    #[tokio::test]
    async fn reapplies_settings_after_reconnect() {
        let (sim, transport) = Sim::receiver(2);
        let (utx, mut urx) = unbounded_channel();
        let h = spawn_with(Box::new(transport), "/dev/sim".into(), true, config(1600), utx, None).unwrap();

        match next(&mut urx).await {
            Update::Online { index, identity, settings, errors, latency, .. } => {
                assert_eq!(index, 2);
                assert_eq!(identity.unit_id, "1A2B3C4D");
                assert_eq!(identity.models, vec!["B042".to_string()]);
                assert_eq!(settings.dpi, Some(1600));
                assert!(errors.is_empty(), "{errors:?}");
                assert!(latency.is_none(), "startup is not a reconnect");
            }
            other => panic!("expected online, got {other:?}"),
        }
        match next(&mut urx).await {
            Update::Battery { status, .. } => assert_eq!(status.percent, Some(80)),
            other => panic!("expected battery, got {other:?}"),
        }
        assert!(matches!(next(&mut urx).await, Update::Settings { .. }));
        assert_eq!(sim.state().dpi, 1600);

        sim.disconnect();
        assert!(matches!(next(&mut urx).await, Update::Offline { index: 2, .. }));

        sim.reconnect();
        match next(&mut urx).await {
            Update::Online { settings, latency, .. } => {
                assert_eq!(settings.dpi, Some(1600));
                assert!(latency.is_some_and(|l| l < Duration::from_millis(100)), "{latency:?}");
            }
            other => panic!("expected online, got {other:?}"),
        }
        assert!(matches!(next(&mut urx).await, Update::Battery { .. }));
        assert!(matches!(next(&mut urx).await, Update::Settings { .. }));
        assert_eq!(sim.state().dpi, 1600);

        sim.battery_event(42);
        match next(&mut urx).await {
            Update::Battery { status, .. } => assert_eq!(status.percent, Some(42)),
            other => panic!("expected battery, got {other:?}"),
        }

        let (done, done_rx) = oneshot::channel();
        h.send(Command::Configure { config: config(800), done: Some(done) });
        done_rx.await.unwrap().unwrap();
        assert_eq!(sim.state().dpi, 800);
        assert!(matches!(next(&mut urx).await, Update::Settings { .. }));

        tokio::task::spawn_blocking(move || h.stop()).await.unwrap();
    }

    #[tokio::test]
    async fn commands_fail_cleanly_when_offline() {
        let (sim, transport) = Sim::receiver(3);
        let (utx, mut urx) = unbounded_channel();
        let h =
            spawn_with(Box::new(transport), "/dev/sim".into(), true, Arc::new(Config::default()), utx, None).unwrap();
        assert!(matches!(next(&mut urx).await, Update::Online { index: 3, .. }));
        assert!(matches!(next(&mut urx).await, Update::Battery { .. }));
        assert!(matches!(next(&mut urx).await, Update::Settings { .. }));
        sim.disconnect();
        assert!(matches!(next(&mut urx).await, Update::Offline { .. }));

        let (reply, rx) = oneshot::channel();
        h.send(Command::Set { index: 3, settings: DeviceSettings { dpi: Some(1200), ..Default::default() }, reply });
        assert!(rx.await.unwrap().unwrap_err().contains("asleep"));
        tokio::task::spawn_blocking(move || h.stop()).await.unwrap();
    }

    #[tokio::test]
    async fn ignored_devices_are_left_alone() {
        let (sim, transport) = Sim::receiver(2);
        let (utx, mut urx) = unbounded_channel();
        let h = spawn_with(Box::new(transport), "/dev/sim".into(), true, config(1600), utx, None).unwrap();
        assert!(matches!(next(&mut urx).await, Update::Online { .. }));
        assert!(matches!(next(&mut urx).await, Update::Battery { .. }));
        assert!(matches!(next(&mut urx).await, Update::Settings { .. }));
        h.send(Command::Ignore { index: 2 });
        sim.disconnect();
        sim.reconnect();
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(sim.state().dpi, DEFAULT_DPI);
        assert!(urx.try_recv().is_err());
        tokio::task::spawn_blocking(move || h.stop()).await.unwrap();
    }

    #[tokio::test]
    async fn ignored_devices_are_taken_up_when_resumed() {
        let (sim, transport) = Sim::receiver(2);
        let (utx, mut urx) = unbounded_channel();
        let h = spawn_with(Box::new(transport), "/dev/sim".into(), true, config(1600), utx, None).unwrap();
        assert!(matches!(next(&mut urx).await, Update::Online { .. }));
        assert!(matches!(next(&mut urx).await, Update::Battery { .. }));
        assert!(matches!(next(&mut urx).await, Update::Settings { .. }));
        h.send(Command::Ignore { index: 2 });
        sim.disconnect();
        sim.reconnect();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(sim.state().dpi, DEFAULT_DPI);

        h.send(Command::Resume { index: 2 });
        assert!(matches!(next(&mut urx).await, Update::Online { index: 2, .. }));
        assert_eq!(sim.state().dpi, 1600);
        tokio::task::spawn_blocking(move || h.stop()).await.unwrap();
    }

    #[tokio::test]
    async fn binding_errors_stay_reported_after_later_changes() {
        // The simulated mouse has no gesture button.
        let config: Arc<Config> =
            Arc::new(toml::from_str("[profile.default.buttons]\ngesture = \"key:f24\"\n").unwrap());
        let (_sim, transport) = Sim::receiver(2);
        let (utx, mut urx) = unbounded_channel();
        let h = spawn_with(Box::new(transport), "/dev/sim".into(), true, config.clone(), utx, None).unwrap();
        let about_gesture = |errors: &[String]| errors.iter().any(|e| e.starts_with("gesture:"));
        match next(&mut urx).await {
            Update::Online { errors, .. } => assert!(about_gesture(&errors), "{errors:?}"),
            other => panic!("expected online, got {other:?}"),
        }
        assert!(matches!(next(&mut urx).await, Update::Battery { .. }));
        assert!(matches!(next(&mut urx).await, Update::Settings { .. }));

        let (reply, rx) = oneshot::channel();
        h.send(Command::Set { index: 2, settings: DeviceSettings { dpi: Some(1200), ..Default::default() }, reply });
        rx.await.unwrap().expect("the setting itself is fine");
        match next(&mut urx).await {
            Update::Settings { errors, .. } => assert!(about_gesture(&errors), "{errors:?}"),
            other => panic!("expected settings, got {other:?}"),
        }

        let (done, done_rx) = oneshot::channel();
        h.send(Command::Configure { config, done: Some(done) });
        done_rx.await.unwrap().unwrap();
        match next(&mut urx).await {
            Update::Settings { errors, .. } => assert!(about_gesture(&errors), "{errors:?}"),
            other => panic!("expected settings, got {other:?}"),
        }
        tokio::task::spawn_blocking(move || h.stop()).await.unwrap();
    }

    mod buttons {
        use super::*;
        use crate::input::{self, InputMsg, key_code, testing::Recorder};

        struct Rig {
            sim: Sim,
            worker: Handle,
            input: input::Handle,
            keys: Recorder,
            updates: UnboundedReceiver<Update>,
        }

        async fn rig(config_text: &str) -> Rig {
            let config: Config = toml::from_str(config_text).unwrap();
            let (sim, transport) = Sim::receiver(2);
            let keys = Recorder::default();
            let input = input::spawn(Box::new(keys.clone())).unwrap();
            let (utx, mut updates) = unbounded_channel();
            let worker =
                spawn_with(Box::new(transport), "/dev/sim".into(), true, Arc::new(config), utx, Some(input.sender()))
                    .unwrap();
            loop {
                if let Update::Settings { .. } = next(&mut updates).await {
                    break;
                }
            }
            Rig { sim, worker, input, keys, updates }
        }

        impl Rig {
            /// Waits until the recorder has seen `n` key events.
            async fn keys(&self, n: usize) -> Vec<(u16, bool)> {
                for _ in 0..200 {
                    let got = self.keys.0.lock().unwrap().clone();
                    if got.len() >= n {
                        return got;
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                self.keys.0.lock().unwrap().clone()
            }

            async fn stop(self) {
                let Rig { worker, input, .. } = self;
                tokio::task::spawn_blocking(move || {
                    worker.stop();
                    input.stop();
                })
                .await
                .unwrap();
            }
        }

        fn k(name: &str) -> u16 {
            key_code(name).unwrap()
        }

        #[tokio::test]
        async fn held_shortcut_keeps_modifiers_down_and_presses_the_key_once() {
            let r = rig("[profile.default.buttons]\nback = \"key:alt+tab\"\n").await;
            assert!(r.sim.diverted(0x0053));
            assert!(r.sim.press(0x0053));
            let down = vec![(k("KEY_LEFTALT"), true), (k("KEY_TAB"), true), (k("KEY_TAB"), false)];
            assert_eq!(r.keys(3).await, down);
            r.sim.release(0x0053);
            assert_eq!(&r.keys(4).await[3..], &[(k("KEY_LEFTALT"), false)]);
            r.stop().await;
        }

        #[tokio::test]
        async fn output_is_made_ready_when_something_is_bound() {
            use std::sync::atomic::Ordering;
            let r = rig("[profile.default.buttons]\nback = \"key:f24\"\n").await;
            for _ in 0..200 {
                if r.keys.2.load(Ordering::SeqCst) > 0 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            assert!(r.keys.2.load(Ordering::SeqCst) > 0, "ready before any button is pressed");
            assert!(r.keys.0.lock().unwrap().is_empty());
            r.stop().await;

            let r = rig("[profile.default.buttons]\nback = \"mouse:middle\"\n").await;
            tokio::time::sleep(Duration::from_millis(50)).await;
            assert_eq!(r.keys.2.load(Ordering::SeqCst), 0, "a remap inside the mouse needs no output");
            r.stop().await;
        }

        #[tokio::test]
        async fn pressed_shortcut_lets_go_at_once() {
            let r = rig("[profile.default.buttons]\nback = \"press:alt+tab\"\n").await;
            assert!(r.sim.press(0x0053));
            let all =
                vec![(k("KEY_LEFTALT"), true), (k("KEY_TAB"), true), (k("KEY_TAB"), false), (k("KEY_LEFTALT"), false)];
            assert_eq!(r.keys(4).await, all);
            r.sim.release(0x0053);
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            assert_eq!(r.keys(4).await.len(), 4, "releasing the button sends nothing more");
            r.stop().await;
        }

        #[tokio::test]
        async fn mouse_button_bindings_are_remapped_in_the_device() {
            let r = rig("[profile.default.buttons]\nback = \"mouse:middle\"\n").await;
            assert_eq!(r.sim.state().reporting.get(&0x0053), Some(&(0, 0x0052)));
            assert!(!r.sim.press(0x0053), "a remapped button is not diverted");
            r.stop().await;
        }

        #[tokio::test]
        async fn remap_is_undone_when_the_button_gets_an_action() {
            let r = rig("[profile.default.buttons]\nback = \"mouse:middle\"\n").await;
            let config: Config = toml::from_str("[profile.default.buttons]\nback = \"key:f24\"\n").unwrap();
            let (done, wait) = oneshot::channel();
            r.worker.send(Command::Configure { config: Arc::new(config), done: Some(done) });
            wait.await.unwrap().unwrap();
            assert_eq!(r.sim.state().reporting.get(&0x0053), Some(&(reprog::reporting::DIVERTED, 0)));
            r.stop().await;
        }

        #[tokio::test]
        async fn gestures_on_the_thumb_panel() {
            let r = rig("[profile.default.buttons]\nhaptic = { tap = \"key:f24\", up = \"key:volumeup\" }\n").await;
            assert_eq!(r.sim.state().reporting.get(&0x01A0).map(|x| x.0), Some(0x0011));
            r.sim.press(0x01A0);
            r.sim.motion(-6125, -7389);
            for _ in 0..8 {
                r.sim.motion(0, -20);
            }
            r.sim.release(0x01A0);
            assert_eq!(r.keys(2).await, vec![(k("KEY_VOLUMEUP"), true), (k("KEY_VOLUMEUP"), false)]);
            r.sim.press(0x01A0);
            r.sim.motion(0, 0);
            r.sim.release(0x01A0);
            assert_eq!(&r.keys(4).await[2..], &[(k("KEY_F24"), true), (k("KEY_F24"), false)]);
            r.stop().await;
        }

        #[tokio::test]
        async fn diversion_comes_back_after_reconnect() {
            let mut r = rig("[profile.default.buttons]\nforward = \"key:f24\"\n").await;
            r.sim.disconnect();
            r.sim.reconnect();
            loop {
                if let Update::Online { .. } = next(&mut r.updates).await {
                    break;
                }
            }
            assert!(r.sim.diverted(0x0056));
            r.sim.press(0x0056);
            assert_eq!(r.keys(1).await, vec![(k("KEY_F24"), true)]);
            r.stop().await;
        }

        #[tokio::test]
        async fn held_keys_are_released_when_the_mouse_goes_away() {
            let r = rig("[profile.default.buttons]\nback = \"key:shift\"\n").await;
            r.sim.press(0x0053);
            assert_eq!(r.keys(1).await, vec![(k("KEY_LEFTSHIFT"), true)]);
            r.sim.disconnect();
            assert_eq!(&r.keys(2).await[1..], &[(k("KEY_LEFTSHIFT"), false)]);
            r.stop().await;
        }

        #[tokio::test]
        async fn nothing_is_injected_while_locked() {
            let r = rig("[profile.default.buttons]\nback = \"key:f24\"\n").await;
            r.input.sender().send(InputMsg::Locked(true)).unwrap();
            r.sim.press(0x0053);
            r.sim.release(0x0053);
            tokio::time::sleep(Duration::from_millis(100)).await;
            assert!(r.keys.0.lock().unwrap().is_empty());
            r.input.sender().send(InputMsg::Locked(false)).unwrap();
            r.sim.press(0x0053);
            assert_eq!(r.keys(1).await, vec![(k("KEY_F24"), true)]);
            r.stop().await;
        }

        impl Rig {
            /// Waits until the recorder has seen `n` scroll frames.
            async fn frames(&self, n: usize) -> Vec<Vec<(u16, i32)>> {
                for _ in 0..200 {
                    let got = self.keys.1.lock().unwrap().clone();
                    if got.len() >= n {
                        return got;
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                self.keys.1.lock().unwrap().clone()
            }
        }

        const WHEEL: u16 = 0x08;
        const WHEEL_HI: u16 = 0x0b;
        const HWHEEL: u16 = 0x06;
        const HWHEEL_HI: u16 = 0x0c;

        #[tokio::test]
        async fn smooth_scrolling_sends_hi_res_and_whole_notches() {
            let r = rig("[profile.default.scroll]\nsmooth = true\n").await;
            assert_eq!(r.sim.state().wheel_mode & 0x03, 0x03, "diverted in hi-res mode");
            r.sim.wheel(15);
            assert_eq!(r.frames(1).await, vec![vec![(WHEEL_HI, 120), (WHEEL, 1)]]);
            r.sim.wheel(7);
            r.sim.wheel(8);
            let f = r.frames(3).await;
            assert_eq!(f[1], vec![(WHEEL_HI, 56)], "half a notch is hi-res only");
            assert_eq!(f[2], vec![(WHEEL_HI, 64), (WHEEL, 1)]);
            r.sim.wheel(-15);
            assert_eq!(r.frames(4).await[3], vec![(WHEEL_HI, -120), (WHEEL, -1)]);
            r.stop().await;
        }

        #[tokio::test]
        async fn scroll_speed_multiplies() {
            let r = rig("[profile.default.scroll]\nsmooth = true\nspeed = 2.0\n").await;
            r.sim.wheel(15);
            assert_eq!(r.frames(1).await, vec![vec![(WHEEL_HI, 240), (WHEEL, 2)]]);
            r.stop().await;
        }

        #[tokio::test]
        async fn thumb_wheel_volume_and_tap() {
            let r = rig("[profile.default.thumb-wheel]\nmode = \"volume\"\ntap = \"key:f24\"\n").await;
            assert!(r.sim.state().thumb.0, "thumb wheel diverted");
            r.sim.thumb_wheel(0, 0, true);
            r.sim.thumb_wheel(3, 1, true);
            r.sim.thumb_wheel(3, 2, true);
            r.sim.thumb_wheel(0, 3, false);
            assert_eq!(r.keys(2).await, vec![(k("KEY_VOLUMEUP"), true), (k("KEY_VOLUMEUP"), false)]);
            r.sim.thumb_wheel(0, 0, true);
            r.sim.thumb_wheel(0, 0, false);
            assert_eq!(&r.keys(4).await[2..], &[(k("KEY_F24"), true), (k("KEY_F24"), false)]);
            tokio::time::sleep(Duration::from_millis(50)).await;
            assert_eq!(r.keys.0.lock().unwrap().len(), 4, "turning the wheel was not a tap");
            r.stop().await;
        }

        #[tokio::test]
        async fn thumb_wheel_horizontal_scroll() {
            let r = rig("[profile.default.thumb-wheel]\nmode = \"hscroll\"\n").await;
            r.sim.thumb_wheel(3, 1, true);
            r.sim.thumb_wheel(3, 2, true);
            let f = r.frames(2).await;
            assert_eq!(f, vec![vec![(HWHEEL_HI, 60)], vec![(HWHEEL_HI, 60), (HWHEEL, 1)]]);
            r.stop().await;
        }

        #[tokio::test]
        async fn wheels_are_given_back_and_taken_again_after_reconnect() {
            let mut r = rig("[profile.default.scroll]\nsmooth = true\n").await;
            r.sim.disconnect();
            r.sim.reconnect();
            loop {
                if let Update::Online { .. } = next(&mut r.updates).await {
                    break;
                }
            }
            assert_eq!(r.sim.state().wheel_mode & 0x03, 0x03);

            let (done, rx) = oneshot::channel();
            r.worker.send(Command::Configure { config: Arc::new(Config::default()), done: Some(done) });
            rx.await.unwrap().unwrap();
            assert_eq!(r.sim.state().wheel_mode & 0x03, 0, "back to plain low-res reporting");
            r.stop().await;
        }

        #[tokio::test]
        async fn taking_the_wheel_keeps_its_direction() {
            let text = "[device.\"*\"]\nwheel-invert = true\n[profile.default.scroll]\nsmooth = true\n";
            let r = rig(text).await;
            assert_eq!(r.sim.state().wheel_mode, 0x07, "diverted, hi-res and inverted");
            let before = r.sim.state().wheel_writes.len();

            let (done, rx) = oneshot::channel();
            r.worker.send(Command::Configure { config: Arc::new(toml::from_str(text).unwrap()), done: Some(done) });
            rx.await.unwrap().unwrap();
            let writes = r.sim.state().wheel_writes[before..].to_vec();
            assert!(writes.iter().all(|m| m & 0x04 != 0), "direction was dropped on the way: {writes:02x?}");
            r.stop().await;
        }

        #[tokio::test]
        async fn toggle_holds_until_pressed_again() {
            let r = rig("[profile.default.buttons]\nback = { toggle = \"mouse:left\" }\n").await;
            r.sim.press(0x0053);
            r.sim.release(0x0053);
            assert_eq!(r.keys(1).await, vec![(k("BTN_LEFT"), true)]);
            tokio::time::sleep(Duration::from_millis(50)).await;
            assert_eq!(r.keys.0.lock().unwrap().len(), 1, "still held after release");
            r.sim.press(0x0053);
            r.sim.release(0x0053);
            assert_eq!(&r.keys(2).await[1..], &[(k("BTN_LEFT"), false)]);
            r.stop().await;
        }

        #[tokio::test]
        async fn dpi_shift_lasts_while_held() {
            let r = rig("[profile.default.buttons]\nback = \"dpi-shift:400\"\n").await;
            r.sim.press(0x0053);
            for _ in 0..200 {
                if r.sim.state().dpi == 400 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            assert_eq!(r.sim.state().dpi, 400);
            r.sim.release(0x0053);
            for _ in 0..200 {
                if r.sim.state().dpi == DEFAULT_DPI {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            assert_eq!(r.sim.state().dpi, DEFAULT_DPI);
            r.stop().await;
        }
    }
}
