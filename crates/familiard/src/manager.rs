//! The manager owns the device registry, the workers and the D-Bus objects.
//! Everything that changes state arrives as an [`Event`] on one queue.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use familiar_core::config::{self, Config, Identity};
use familiar_core::dbus::{DEVICE_IFACE, MANAGER_IFACE, MANAGER_PATH, device_path};
use familiar_core::settings::DeviceSettings;
use hidpp::features::{battery, haptic};
use hidpp_hidraw::HidrawInfo;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::oneshot;
use tracing::{error, info, warn};
use zbus::{Connection, OwnedObjectPath, Variant};

use crate::dbus::{DeviceIface, DeviceState, ManagerState, Published, emit_changed};
use crate::desktop;
use crate::worker::{self, Command, Update};

type Reply<T> = oneshot::Sender<Result<T, String>>;

pub enum Request {
    Set {
        key: String,
        name: String,
        value: String,
        persist: bool,
        reply: Reply<BTreeMap<String, String>>,
    },
    PlayHaptic {
        key: String,
        waveform: String,
        reply: Reply<()>,
    },
    Hosts {
        key: String,
        reply: Reply<worker::HostList>,
    },
    RenameHost {
        key: String,
        channel: u8,
        name: String,
        reply: Reply<String>,
    },
    SwitchHost {
        key: String,
        channel: u8,
        reply: Reply<()>,
    },
    Reapply {
        key: String,
        reply: Reply<()>,
    },
    /// `force` reapplies even when the file has not changed.
    Reload {
        force: bool,
        reply: Reply<()>,
    },
}

pub enum Event {
    Worker(Update),
    Added(HidrawInfo),
    Removed(PathBuf),
    ConfigChanged,
    Request(Request),
    /// A `Set` with persist succeeded on the device; save it on the manager's
    /// own loop so concurrent saves cannot overwrite each other.
    Persist {
        section: String,
        settings: DeviceSettings,
        current: BTreeMap<String, String>,
        reply: Reply<BTreeMap<String, String>>,
    },
}

struct Record {
    endpoint: PathBuf,
    index: u8,
    identity: Identity,
    path: String,
    online: bool,
    battery: Option<battery::Status>,
    state: Published<DeviceState>,
    /// Other endpoints and indices told to leave this device alone.
    ignored: Vec<(PathBuf, u8)>,
}

pub struct Manager {
    conn: Connection,
    tx: UnboundedSender<Event>,
    config_path: PathBuf,
    config: Arc<Config>,
    workers: HashMap<PathBuf, worker::Handle>,
    devices: HashMap<String, Record>,
    published: Published<ManagerState>,
    reload_pending: bool,
    input: Option<std::sync::mpsc::Sender<crate::input::InputMsg>>,
}

fn device_key(identity: &Identity, endpoint: &Path, index: u8) -> String {
    if identity.unit_id.is_empty() { format!("{}-{index}", endpoint.display()) } else { identity.unit_id.clone() }
}

fn status_name(s: &battery::Status) -> &'static str {
    use battery::Charging::*;
    match s.charging {
        Discharging => "discharging",
        Recharging => "recharging",
        AlmostFull => "almost-full",
        Full => "full",
        SlowRecharge => "slow-recharge",
        InvalidBattery | ThermalError => "error",
        Unknown(_) => "unknown",
    }
}

fn variant<T: Into<Variant<'static>>>(v: T) -> Variant<'static> {
    v.into()
}

fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn map_variant(m: BTreeMap<String, String>) -> Variant<'static> {
    variant(m.into_iter().collect::<HashMap<String, String>>())
}

impl Manager {
    pub fn new(
        conn: Connection,
        tx: UnboundedSender<Event>,
        config_path: PathBuf,
        config: Config,
        published: Published<ManagerState>,
        input: Option<std::sync::mpsc::Sender<crate::input::InputMsg>>,
    ) -> Self {
        Manager {
            conn,
            tx,
            config_path,
            config: Arc::new(config),
            workers: HashMap::new(),
            devices: HashMap::new(),
            published,
            reload_pending: false,
            input,
        }
    }

    pub async fn handle(&mut self, event: Event) {
        match event {
            Event::Added(info) => self.add_endpoint(info),
            Event::Removed(path) => self.remove_endpoint(&path).await,
            Event::Worker(update) => self.update(update).await,
            Event::ConfigChanged => self.schedule_reload(),
            Event::Request(r) => self.request(r).await,
            Event::Persist { section, settings, current, reply } => {
                let r = self.persist(&section, &settings).map(|_| current);
                let _ = reply.send(r);
            }
        }
    }

    pub fn add_endpoint(&mut self, info: HidrawInfo) {
        if self.workers.contains_key(&info.path) || !info.is_usable() {
            return;
        }
        let path = info.path.clone();
        let updates = self.worker_updates();
        match worker::spawn(info, self.config.clone(), updates, self.input.clone()) {
            Ok(h) => {
                self.workers.insert(path, h);
            }
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                warn!("cannot open {}: permission denied; install packaging/udev/70-familiar.rules", path.display())
            }
            Err(e) => warn!("cannot open {}: {e}", path.display()),
        }
    }

    /// Worker updates go through a small forwarding task into the main queue.
    fn worker_updates(&self) -> UnboundedSender<Update> {
        let (utx, mut urx) = tokio::sync::mpsc::unbounded_channel();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            while let Some(u) = urx.recv().await {
                if tx.send(Event::Worker(u)).is_err() {
                    break;
                }
            }
        });
        utx
    }

    async fn remove_endpoint(&mut self, path: &Path) {
        if let Some(h) = self.workers.remove(path) {
            tokio::task::spawn_blocking(move || h.stop());
        }
        for r in self.devices.values_mut() {
            r.ignored.retain(|(e, _)| e != path);
        }
        let keys: Vec<String> =
            self.devices.iter().filter(|(_, r)| r.endpoint == path).map(|(k, _)| k.clone()).collect();
        for k in keys {
            self.went_offline(&k).await;
        }
    }

    /// Marks a device offline and hands it to any endpoint that was told to
    /// leave it alone, in case it is still reachable there.
    async fn went_offline(&mut self, key: &str) {
        self.set_online(key, false).await;
        let ignored = self.devices.get_mut(key).map(|r| std::mem::take(&mut r.ignored)).unwrap_or_default();
        for (endpoint, index) in ignored {
            if let Some(h) = self.workers.get(&endpoint) {
                h.send(Command::Resume { index });
            }
        }
    }

    pub fn shutdown(&mut self) {
        for (_, h) in self.workers.drain() {
            h.stop();
        }
    }

    fn key_for(&self, endpoint: &Path, index: u8) -> Option<String> {
        self.devices.iter().find(|(_, r)| r.endpoint == endpoint && r.index == index).map(|(k, _)| k.clone())
    }

    async fn set_online(&mut self, key: &str, online: bool) {
        let Some(r) = self.devices.get_mut(key) else { return };
        r.online = online;
        r.state.lock().online = online;
        let path = r.path.clone();
        emit_changed(&self.conn, &path, DEVICE_IFACE, vec![("Online", variant(online))]).await;
    }

    async fn update(&mut self, update: Update) {
        match update {
            Update::Online { endpoint, index, identity, settings, errors, latency } => {
                let key = device_key(&identity, &endpoint, index);
                if !self.device_online(&key, &endpoint, index, identity).await {
                    return;
                }
                if let Some(l) = latency {
                    self.publish_reconnect(&key, l).await;
                }
                // Only the configured settings were read; a full read follows.
                self.publish_settings(&key, &settings, errors, true).await;
                self.refresh_conflicts().await;
            }
            Update::Offline { endpoint, index } => {
                if let Some(k) = self.key_for(&endpoint, index) {
                    self.went_offline(&k).await;
                }
            }
            Update::Battery { endpoint, index, status } => {
                if let Some(k) = self.key_for(&endpoint, index) {
                    self.battery(&k, status).await;
                }
            }
            Update::Settings { endpoint, index, settings, errors } => {
                if let Some(k) = self.key_for(&endpoint, index) {
                    self.publish_settings(&k, &settings, errors, false).await;
                }
            }
            Update::Gone { endpoint } => self.remove_endpoint(&endpoint).await,
        }
    }

    /// Records a device as online. Returns false when the same device is
    /// already online through another endpoint, which is then told to leave
    /// this copy alone.
    async fn device_online(&mut self, key: &str, endpoint: &Path, index: u8, identity: Identity) -> bool {
        if let Some(r) = self.devices.get_mut(key) {
            if r.online && r.endpoint != endpoint {
                warn!(%key, "already managed through {}; ignoring it on {}", r.endpoint.display(), endpoint.display());
                if let Some(h) = self.workers.get(endpoint) {
                    h.send(Command::Ignore { index });
                }
                let copy = (endpoint.to_path_buf(), index);
                if !r.ignored.contains(&copy) {
                    r.ignored.push(copy);
                }
                return false;
            }
            r.endpoint = endpoint.to_path_buf();
            r.index = index;
            {
                let mut s = r.state.lock();
                s.endpoint = endpoint.display().to_string();
                s.index = index;
            }
            let path = r.path.clone();
            let changes = vec![("Endpoint", variant(endpoint.display().to_string())), ("Index", variant(index))];
            emit_changed(&self.conn, &path, DEVICE_IFACE, changes).await;
            self.set_online(key, true).await;
            return true;
        }

        let path = device_path(key);
        let state = Published::new(DeviceState {
            name: identity.name.clone(),
            unit_id: identity.unit_id.clone(),
            model: identity.models.first().cloned().unwrap_or_default(),
            endpoint: endpoint.display().to_string(),
            index,
            online: true,
            battery_status: "unknown".into(),
            ..Default::default()
        });
        let iface = DeviceIface { key: key.to_string(), tx: self.tx.clone(), state: state.clone() };
        if let Err(e) = self.conn.object_server().at(path.as_str(), iface).await {
            error!("cannot export {path}: {e}");
        }
        info!(%key, name = %identity.name, %path, "device registered");
        self.devices.insert(
            key.to_string(),
            Record {
                endpoint: endpoint.to_path_buf(),
                index,
                identity,
                path,
                online: true,
                battery: None,
                state,
                ignored: Vec::new(),
            },
        );
        self.publish_device_list().await;
        true
    }

    async fn publish_device_list(&self) {
        let mut paths: Vec<String> = self.devices.values().map(|r| r.path.clone()).collect();
        paths.sort();
        let owned: Vec<OwnedObjectPath> =
            paths.iter().filter_map(|p| OwnedObjectPath::try_from(p.as_str()).ok()).collect();
        self.published.lock().devices = owned.clone();
        emit_changed(&self.conn, MANAGER_PATH, MANAGER_IFACE, vec![("Devices", variant(owned))]).await;
    }

    async fn refresh_conflicts(&self) {
        let found = tokio::task::spawn_blocking(desktop::conflicts).await.unwrap_or_default();
        {
            let mut s = self.published.lock();
            if s.conflicts == found {
                return;
            }
            s.conflicts = found.clone();
        }
        for c in &found {
            warn!("{c} is running and may change device settings back");
        }
        emit_changed(&self.conn, MANAGER_PATH, MANAGER_IFACE, vec![("Conflicts", variant(found))]).await;
    }

    /// Publishes the settings a worker just applied, with the time and what
    /// the config asks for. A `partial` update is merged over the last values.
    async fn publish_settings(&self, key: &str, settings: &DeviceSettings, errors: Vec<String>, partial: bool) {
        let Some(r) = self.devices.get(key) else { return };
        let mut map = if partial { r.state.lock().settings.clone() } else { BTreeMap::new() };
        map.extend(settings.to_map());
        let configured = self.config.settings_for(&r.identity).to_map();
        let now = unix_now();
        let mut changes = vec![("LastApplied", variant(now))];
        {
            let mut s = r.state.lock();
            s.last_applied = now;
            if s.settings != map {
                s.settings = map.clone();
                changes.push(("Settings", map_variant(map)));
            }
            if s.configured != configured {
                s.configured = configured.clone();
                changes.push(("Configured", map_variant(configured)));
            }
            if s.errors != errors {
                s.errors = errors.clone();
                changes.push(("ApplyErrors", variant(errors)));
            }
        }
        emit_changed(&self.conn, &r.path, DEVICE_IFACE, changes).await;
    }

    async fn publish_reconnect(&self, key: &str, latency: Duration) {
        let Some(r) = self.devices.get(key) else { return };
        let ms = latency.as_millis().min(u32::MAX as u128) as u32;
        r.state.lock().reconnect_ms = ms;
        emit_changed(&self.conn, &r.path, DEVICE_IFACE, vec![("ReconnectMs", variant(ms))]).await;
    }

    async fn battery(&mut self, key: &str, status: battery::Status) {
        let Some(r) = self.devices.get_mut(key) else { return };
        let previous = r.battery.replace(status);
        let name = r.identity.name.clone();
        let n = &self.config.notifications;
        let mut notice = None;
        if let Some(pct) = status.percent {
            let was = previous.and_then(|p| p.percent).unwrap_or(100);
            if n.battery_low > 0 && !status.is_charging() && pct <= n.battery_low && was > n.battery_low {
                notice = Some((format!("{name} battery low"), format!("{pct}% remaining")));
            }
        }
        let full = matches!(status.charging, battery::Charging::Full);
        let was_full = previous.is_some_and(|p| matches!(p.charging, battery::Charging::Full));
        if n.battery_full && full && previous.is_some() && !was_full {
            notice = Some((format!("{name} charged"), "The battery is full".into()));
        }
        if let Some((summary, body)) = notice {
            let conn = self.conn.clone();
            tokio::spawn(async move { desktop::notify(&conn, &summary, &body).await });
        }

        let percent = status.percent.unwrap_or(0);
        let label = status_name(&status).to_string();
        {
            let mut s = r.state.lock();
            s.battery_percent = percent;
            s.battery_status = label.clone();
        }
        let path = r.path.clone();
        let changes = vec![("BatteryPercent", variant(percent)), ("BatteryStatus", variant(label))];
        emit_changed(&self.conn, &path, DEVICE_IFACE, changes).await;
    }

    fn schedule_reload(&mut self) {
        if self.reload_pending {
            return;
        }
        self.reload_pending = true;
        let tx = self.tx.clone();
        tokio::spawn(async move {
            // Editors often write a file in several steps; wait for them to finish.
            tokio::time::sleep(Duration::from_millis(250)).await;
            let (reply, _) = oneshot::channel();
            let _ = tx.send(Event::Request(Request::Reload { force: false, reply }));
        });
    }

    /// Loads the config and hands it to every worker. Returns receivers that
    /// complete once each worker has applied it.
    fn reload(&mut self, force: bool) -> Result<Vec<oneshot::Receiver<Result<(), String>>>, String> {
        self.reload_pending = false;
        let config = Config::load(&self.config_path).map_err(|e| e.to_string())?;
        if *self.config == config && !force {
            return Ok(Vec::new());
        }
        info!("config reloaded");
        self.config = Arc::new(config);
        let mut waits = Vec::new();
        for h in self.workers.values() {
            let (done, rx) = oneshot::channel();
            h.send(Command::Configure { config: self.config.clone(), done: Some(done) });
            waits.push(rx);
        }
        Ok(waits)
    }

    /// Saves settings and makes them the in-memory config straight away, so a
    /// reconnect applies them even if the file watcher is not running. They go
    /// into whichever file defines the section, so the saved value takes effect.
    fn persist(&mut self, section: &str, settings: &DeviceSettings) -> Result<(), String> {
        let file = self.config.file_for(section, &self.config_path);
        config::persist(&file, section, settings).map_err(|e| format!("applied, but not saved: {e}"))?;
        self.reload(false).map(|_| ())
    }

    fn online_record(&self, key: &str) -> Result<&Record, String> {
        match self.devices.get(key) {
            Some(r) if r.online => Ok(r),
            Some(_) => Err("device is asleep or disconnected".into()),
            None => Err(format!("unknown device {key}")),
        }
    }

    fn worker_for(&self, r: &Record) -> Result<&worker::Handle, String> {
        self.workers.get(&r.endpoint).ok_or_else(|| "device endpoint is gone".to_string())
    }

    async fn request(&mut self, req: Request) {
        match req {
            Request::Reload { force, reply } => match self.reload(force) {
                Ok(waits) => {
                    tokio::spawn(async move {
                        for w in waits {
                            let _ = w.await;
                        }
                        let _ = reply.send(Ok(()));
                    });
                }
                Err(e) => {
                    error!("config not reloaded: {e}");
                    let _ = reply.send(Err(e));
                }
            },
            Request::Set { key, name, value, persist, reply } => {
                let mut settings = DeviceSettings::default();
                if let Err(e) = settings.set_str(&name, &value) {
                    let _ = reply.send(Err(e.to_string()));
                    return;
                }
                let (record, handle) = match self.online_record(&key).and_then(|r| Ok((r, self.worker_for(r)?))) {
                    Ok(t) => t,
                    Err(e) => {
                        let _ = reply.send(Err(e));
                        return;
                    }
                };
                let (wreply, wrx) = oneshot::channel();
                handle.send(Command::Set { index: record.index, settings: settings.clone(), reply: wreply });
                let section = persist.then(|| self.config.section_for(&record.identity));
                let tx = self.tx.clone();
                tokio::spawn(async move {
                    match wrx.await {
                        Ok(Ok(current)) => match section {
                            Some(section) => {
                                let current = current.to_map();
                                let _ = tx.send(Event::Persist { section, settings, current, reply });
                            }
                            None => {
                                let _ = reply.send(Ok(current.to_map()));
                            }
                        },
                        Ok(Err(e)) => {
                            let _ = reply.send(Err(e));
                        }
                        Err(_) => {
                            let _ = reply.send(Err("device thread stopped".into()));
                        }
                    }
                });
            }
            Request::PlayHaptic { key, waveform, reply } => {
                let Some(w) = haptic::Waveform::from_name(&waveform) else {
                    let _ = reply.send(Err(format!("unknown waveform {waveform:?}")));
                    return;
                };
                self.forward(&key, reply, |index, r| Command::PlayHaptic { index, waveform: w, reply: r });
            }
            Request::Hosts { key, reply } => self.forward(&key, reply, |index, r| Command::Hosts { index, reply: r }),
            Request::RenameHost { key, channel, name, reply } => {
                if channel == 0 {
                    let _ = reply.send(Err("channels are numbered from 1".into()));
                    return;
                }
                self.forward(&key, reply, |index, r| Command::RenameHost { index, host: channel - 1, name, reply: r });
            }
            Request::SwitchHost { key, channel, reply } => {
                if channel == 0 {
                    let _ = reply.send(Err("channels are numbered from 1".into()));
                    return;
                }
                self.forward(&key, reply, |index, r| Command::SwitchHost { index, host: channel - 1, reply: r });
            }
            Request::Reapply { key, reply } => {
                let (wreply, wrx) = oneshot::channel();
                match self.online_record(&key).and_then(|r| Ok((r.index, self.worker_for(r)?))) {
                    Ok((index, h)) => {
                        h.send(Command::Reapply { index, reply: wreply });
                        tokio::spawn(async move {
                            let r = wrx.await.unwrap_or_else(|_| Err("device thread stopped".into()));
                            let _ = reply.send(r.map(|_| ()));
                        });
                    }
                    Err(e) => {
                        let _ = reply.send(Err(e));
                    }
                }
            }
        }
    }

    /// Sends a command to the worker that owns a device and relays its reply.
    fn forward<T: Send + 'static>(&self, key: &str, reply: Reply<T>, make: impl FnOnce(u8, Reply<T>) -> Command) {
        match self.online_record(key).and_then(|r| Ok((r.index, self.worker_for(r)?))) {
            Ok((index, h)) => {
                let (wreply, wrx) = oneshot::channel();
                h.send(make(index, wreply));
                tokio::spawn(async move {
                    let r = wrx.await.unwrap_or_else(|_| Err("device thread stopped".into()));
                    let _ = reply.send(r);
                });
            }
            Err(e) => {
                let _ = reply.send(Err(e));
            }
        }
    }
}
