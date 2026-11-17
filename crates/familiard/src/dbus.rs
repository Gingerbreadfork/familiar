//! D-Bus objects.
//!
//! zbus holds an object's lock for as long as one of its methods runs, and our
//! methods wait for the manager. So the manager never takes that lock: the
//! published state lives in a plain mutex shared with the manager, which also
//! emits `PropertiesChanged` itself.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, MutexGuard};

use tokio::sync::{mpsc::UnboundedSender, oneshot};
use tracing::debug;
use zbus::names::InterfaceName;
use zbus::object_server::SignalEmitter;
use zbus::{Connection, OwnedObjectPath, Variant, fdo, interface};

use crate::manager::{Event, Request};

async fn ask<T>(
    tx: &UnboundedSender<Event>,
    make: impl FnOnce(oneshot::Sender<Result<T, String>>) -> Request,
) -> fdo::Result<T> {
    let (reply, rx) = oneshot::channel();
    tx.send(Event::Request(make(reply))).map_err(|_| fdo::Error::Failed("daemon is shutting down".into()))?;
    rx.await.map_err(|_| fdo::Error::Failed("no answer from device thread".into()))?.map_err(fdo::Error::Failed)
}

/// State behind a D-Bus object, shared with the manager.
pub struct Published<T>(Arc<Mutex<T>>);

impl<T> Clone for Published<T> {
    fn clone(&self) -> Self {
        Published(self.0.clone())
    }
}

impl<T> Published<T> {
    pub fn new(v: T) -> Self {
        Published(Arc::new(Mutex::new(v)))
    }

    pub fn lock(&self) -> MutexGuard<'_, T> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Emits `PropertiesChanged` for some properties of one object.
pub async fn emit_changed(
    conn: &Connection,
    path: &str,
    iface: &'static str,
    changed: Vec<(&'static str, Variant<'static>)>,
) {
    if changed.is_empty() {
        return;
    }
    let Ok(emitter) = SignalEmitter::new(conn, path.to_string()) else { return };
    let map: HashMap<&str, Variant<'_>> = changed.into_iter().collect();
    let name = InterfaceName::from_static_str_unchecked(iface);
    if let Err(e) = fdo::Properties::properties_changed(&emitter, name, map, Cow::Borrowed(&[])).await {
        debug!("PropertiesChanged not sent: {e}");
    }
}

#[derive(Default)]
pub struct ManagerState {
    pub devices: Vec<OwnedObjectPath>,
    pub conflicts: Vec<String>,
    pub config_path: String,
}

pub struct ManagerIface {
    pub tx: UnboundedSender<Event>,
    pub state: Published<ManagerState>,
}

#[interface(name = "io.github.gingerbreadfork.Familiar.Manager1")]
impl ManagerIface {
    /// Object paths of every device seen since the daemon started.
    #[zbus(property)]
    fn devices(&self) -> Vec<OwnedObjectPath> {
        self.state.lock().devices.clone()
    }

    /// Other programs that also configure Logitech devices and may undo our settings.
    #[zbus(property)]
    fn conflicts(&self) -> Vec<String> {
        self.state.lock().conflicts.clone()
    }

    #[zbus(property)]
    fn config_path(&self) -> String {
        self.state.lock().config_path.clone()
    }

    #[zbus(property)]
    fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    /// Rereads the config file and applies it to every connected device.
    async fn reload(&self) -> fdo::Result<()> {
        ask(&self.tx, |reply| Request::Reload { force: true, reply }).await
    }
}

#[derive(Clone, Default)]
pub struct DeviceState {
    pub name: String,
    pub unit_id: String,
    pub model: String,
    pub endpoint: String,
    pub index: u8,
    pub online: bool,
    pub battery_percent: u8,
    pub battery_status: String,
    pub settings: BTreeMap<String, String>,
    /// What the config asks for, in the same form as `settings`.
    pub configured: BTreeMap<String, String>,
    pub errors: Vec<String>,
    /// Unix time of the last time settings were applied.
    pub last_applied: u64,
    /// How long the last reconnect took until settings were in place.
    pub reconnect_ms: u32,
}

pub struct DeviceIface {
    pub key: String,
    pub tx: UnboundedSender<Event>,
    pub state: Published<DeviceState>,
}

#[interface(name = "io.github.gingerbreadfork.Familiar.Device1")]
impl DeviceIface {
    #[zbus(property)]
    fn name(&self) -> String {
        self.state.lock().name.clone()
    }

    #[zbus(property)]
    fn unit_id(&self) -> String {
        self.state.lock().unit_id.clone()
    }

    #[zbus(property)]
    fn model(&self) -> String {
        self.state.lock().model.clone()
    }

    /// The hidraw node the device is reached through.
    #[zbus(property)]
    fn endpoint(&self) -> String {
        self.state.lock().endpoint.clone()
    }

    /// Index behind the receiver, or 255 for a direct connection.
    #[zbus(property)]
    fn index(&self) -> u8 {
        self.state.lock().index
    }

    #[zbus(property)]
    fn online(&self) -> bool {
        self.state.lock().online
    }

    /// Battery charge in percent; 0 when unknown.
    #[zbus(property)]
    fn battery_percent(&self) -> u8 {
        self.state.lock().battery_percent
    }

    /// One of discharging, recharging, almost-full, full, slow-recharge, error or unknown.
    #[zbus(property)]
    fn battery_status(&self) -> String {
        self.state.lock().battery_status.clone()
    }

    /// Current settings in their command line form, e.g. dpi = "1000".
    #[zbus(property)]
    fn settings(&self) -> HashMap<String, String> {
        self.state.lock().settings.clone().into_iter().collect()
    }

    /// Settings and button bindings that could not be put on the device.
    #[zbus(property)]
    fn apply_errors(&self) -> Vec<String> {
        self.state.lock().errors.clone()
    }

    /// What the config file asks for this device, in the same form as `Settings`.
    #[zbus(property)]
    fn configured(&self) -> HashMap<String, String> {
        self.state.lock().configured.clone().into_iter().collect()
    }

    /// Unix time when settings were last applied; 0 before the first time.
    #[zbus(property)]
    fn last_applied(&self) -> u64 {
        self.state.lock().last_applied
    }

    /// Milliseconds from the last reconnect notice to settings being in place; 0 if none yet.
    #[zbus(property)]
    fn reconnect_ms(&self) -> u32 {
        self.state.lock().reconnect_ms
    }

    /// Changes one setting. With `persist` it is also saved to the config file.
    async fn set(&self, name: String, value: String, persist: bool) -> fdo::Result<HashMap<String, String>> {
        let key = self.key.clone();
        let m = ask(&self.tx, |reply| Request::Set { key, name, value, persist, reply }).await?;
        Ok(m.into_iter().collect())
    }

    /// Plays a haptic waveform, e.g. "completed".
    async fn play_haptic(&self, waveform: String) -> fdo::Result<()> {
        let key = self.key.clone();
        ask(&self.tx, |reply| Request::PlayHaptic { key, waveform, reply }).await
    }

    /// The current Easy-Switch channel, numbered from 1, and each channel as
    /// (channel, paired, name).
    async fn hosts(&self) -> fdo::Result<(u8, Vec<(u8, bool, String)>)> {
        let key = self.key.clone();
        let (current, list) = ask(&self.tx, |reply| Request::Hosts { key, reply }).await?;
        Ok((current + 1, list.into_iter().map(|(i, p, n)| (i + 1, p, n)).collect()))
    }

    /// Renames an Easy-Switch channel. Returns the name as the mouse stored it.
    async fn rename_host(&self, channel: u8, name: String) -> fdo::Result<String> {
        let key = self.key.clone();
        ask(&self.tx, |reply| Request::RenameHost { key, channel, name, reply }).await
    }

    /// Moves the device to Easy-Switch channel 1, 2 or 3.
    async fn switch_host(&self, channel: u8) -> fdo::Result<()> {
        let key = self.key.clone();
        ask(&self.tx, |reply| Request::SwitchHost { key, channel, reply }).await
    }

    /// Applies the configured settings again.
    async fn reapply(&self) -> fdo::Result<()> {
        let key = self.key.clone();
        ask(&self.tx, |reply| Request::Reapply { key, reply }).await
    }
}

#[cfg(test)]
mod tests {
    use familiar_core::dbus::{DEVICE_IFACE, MANAGER_IFACE};

    // Signals are emitted by name, so the names in the attributes must match core.
    #[test]
    fn interface_names_match_core() {
        let src = include_str!("dbus.rs");
        assert!(src.contains(&format!("#[interface(name = \"{MANAGER_IFACE}\")]")));
        assert!(src.contains(&format!("#[interface(name = \"{DEVICE_IFACE}\")]")));
    }
}
