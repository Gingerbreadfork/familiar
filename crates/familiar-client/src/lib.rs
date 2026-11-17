//! Talking to a running `familiard` over D-Bus.

use std::collections::{BTreeMap, HashMap};

use anyhow::{Context, Result, anyhow};
use familiar_core::dbus::BUS_NAME;
use zbus::{Connection, OwnedObjectPath};

#[zbus::proxy(
    interface = "io.github.gingerbreadfork.Familiar.Manager1",
    default_service = "io.github.gingerbreadfork.Familiar",
    default_path = "/io/github/gingerbreadfork/Familiar"
)]
pub trait Manager {
    #[zbus(property)]
    fn devices(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
    #[zbus(property)]
    fn conflicts(&self) -> zbus::Result<Vec<String>>;
    #[zbus(property)]
    fn config_path(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn version(&self) -> zbus::Result<String>;
    fn reload(&self) -> zbus::Result<()>;
}

#[zbus::proxy(
    interface = "io.github.gingerbreadfork.Familiar.Device1",
    default_service = "io.github.gingerbreadfork.Familiar"
)]
pub trait Device {
    #[zbus(property)]
    fn name(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn unit_id(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn model(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn endpoint(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn index(&self) -> zbus::Result<u8>;
    #[zbus(property)]
    fn online(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn battery_percent(&self) -> zbus::Result<u8>;
    #[zbus(property)]
    fn battery_status(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn settings(&self) -> zbus::Result<HashMap<String, String>>;
    #[zbus(property)]
    fn apply_errors(&self) -> zbus::Result<Vec<String>>;
    #[zbus(property)]
    fn configured(&self) -> zbus::Result<HashMap<String, String>>;
    #[zbus(property)]
    fn last_applied(&self) -> zbus::Result<u64>;
    #[zbus(property)]
    fn reconnect_ms(&self) -> zbus::Result<u32>;
    fn set(&self, name: &str, value: &str, persist: bool) -> zbus::Result<HashMap<String, String>>;
    fn play_haptic(&self, waveform: &str) -> zbus::Result<()>;
    fn switch_host(&self, channel: u8) -> zbus::Result<()>;
    fn hosts(&self) -> zbus::Result<(u8, Vec<(u8, bool, String)>)>;
    fn rename_host(&self, channel: u8, name: &str) -> zbus::Result<String>;
    fn reapply(&self) -> zbus::Result<()>;
}

pub struct Daemon {
    pub conn: Connection,
}

/// Connects to the daemon if it is running on the session bus.
pub async fn connect() -> Option<Daemon> {
    let conn = Connection::session().await.ok()?;
    connect_with(&conn).await
}

/// The daemon on an existing connection, if it is running.
pub async fn connect_with(conn: &Connection) -> Option<Daemon> {
    let dbus = zbus::fdo::DBusProxy::new(conn).await.ok()?;
    let name = zbus::names::BusName::try_from(BUS_NAME).ok()?;
    dbus.name_has_owner(name).await.ok()?.then_some(Daemon { conn: conn.clone() })
}

/// D-Bus errors carry a "org.freedesktop.DBus.Error.Failed: " prefix; show only the message.
pub fn clean(e: zbus::Error) -> anyhow::Error {
    match e {
        zbus::Error::FDO(e) => match *e {
            zbus::fdo::Error::Failed(msg) => anyhow!(msg),
            other => anyhow!(other.to_string()),
        },
        zbus::Error::MethodError(_, Some(msg), _) => anyhow!(msg),
        other => anyhow!(other.to_string()),
    }
}

impl Daemon {
    pub async fn manager(&self) -> Result<ManagerProxy<'_>> {
        Ok(ManagerProxy::new(&self.conn).await?)
    }

    pub async fn devices(&self) -> Result<Vec<DeviceProxy<'_>>> {
        let mut out = Vec::new();
        for path in self.manager().await?.devices().await? {
            out.push(DeviceProxy::builder(&self.conn).path(path).build().await?);
        }
        Ok(out)
    }

    /// The device to act on: the one at `index`, or the first online one.
    pub async fn device(&self, index: Option<u8>) -> Result<DeviceProxy<'_>> {
        let mut first_offline = None;
        for d in self.devices().await? {
            if index.is_some() && Some(d.index().await?) != index {
                continue;
            }
            if d.online().await? {
                return Ok(d);
            }
            first_offline.get_or_insert(d);
        }
        match first_offline {
            Some(d) => Err(anyhow!("{} is asleep or disconnected; move the mouse and retry", d.name().await?)),
            None => Err(anyhow!("the daemon has not seen a matching device")),
        }
        .context("familiard")
    }
}

pub async fn settings(d: &DeviceProxy<'_>) -> Result<BTreeMap<String, String>> {
    Ok(d.settings().await?.into_iter().collect())
}

pub async fn configured(d: &DeviceProxy<'_>) -> Result<BTreeMap<String, String>> {
    Ok(d.configured().await?.into_iter().collect())
}

/// "12 s ago", "3 min ago" and so on, from a Unix time.
pub fn ago(unix: u64) -> String {
    if unix == 0 {
        return "never".into();
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let s = now.saturating_sub(unix);
    match s {
        0..60 => format!("{s} s ago"),
        60..3600 => format!("{} min ago", s / 60),
        3600..86400 => format!("{} h ago", s / 3600),
        _ => format!("{} days ago", s / 86400),
    }
}

/// Everything the app shows about one device, read in one go.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSnapshot {
    pub path: String,
    pub name: String,
    pub unit_id: String,
    pub model: String,
    pub online: bool,
    pub battery_percent: u8,
    pub battery_status: String,
    pub settings: BTreeMap<String, String>,
    pub configured: BTreeMap<String, String>,
    pub apply_errors: Vec<String>,
    pub last_applied: u64,
    pub reconnect_ms: u32,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub version: String,
    pub config_path: String,
    pub conflicts: Vec<String>,
    pub devices: Vec<DeviceSnapshot>,
}

impl Daemon {
    pub async fn snapshot(&self) -> Result<Snapshot> {
        let m = self.manager().await?;
        let mut devices = Vec::new();
        for d in self.devices().await? {
            devices.push(DeviceSnapshot {
                path: d.inner().path().to_string(),
                name: d.name().await?,
                unit_id: d.unit_id().await?,
                model: d.model().await?,
                online: d.online().await?,
                battery_percent: d.battery_percent().await?,
                battery_status: d.battery_status().await?,
                settings: settings(&d).await?,
                configured: configured(&d).await?,
                apply_errors: d.apply_errors().await?,
                last_applied: d.last_applied().await?,
                reconnect_ms: d.reconnect_ms().await?,
            });
        }
        Ok(Snapshot {
            version: m.version().await?,
            config_path: m.config_path().await?,
            conflicts: m.conflicts().await?,
            devices,
        })
    }

    /// The device object at `path`.
    pub async fn device_at(&self, path: &str) -> Result<DeviceProxy<'_>> {
        Ok(DeviceProxy::builder(&self.conn).path(path.to_string()).build().await?)
    }

    /// Resolves whenever anything the daemon publishes changes, or the daemon
    /// starts or stops.
    pub async fn changes(conn: &Connection) -> Result<impl futures_util::Stream<Item = ()> + use<>> {
        use futures_util::StreamExt;
        let props = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender(BUS_NAME)
            .interface("org.freedesktop.DBus.Properties")
            .member("PropertiesChanged")
            .build()?;
        let owner = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender("org.freedesktop.DBus")
            .member("NameOwnerChanged")
            .arg(0, BUS_NAME)
            .build()?;
        let a = zbus::MessageStream::for_match_rule(props, conn, std::num::NonZeroUsize::new(64)).await?;
        let b = zbus::MessageStream::for_match_rule(owner, conn, std::num::NonZeroUsize::new(8)).await?;
        Ok(futures_util::stream::select(a, b).map(|_| ()))
    }
}
