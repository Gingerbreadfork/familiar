//! The Familiar app: a window onto `familiard`.
//!
//! Device settings go through the daemon's D-Bus API. Buttons and wheel
//! behaviour are written to the config file, which the daemon then reloads.
//! A copy installed from a package can also replace itself with a newer one.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::BTreeMap;
use std::path::PathBuf;

use familiar_client::{Daemon, Snapshot};
use familiar_core::actions::{Action, Binding, Gestures};
use familiar_core::config::{self, Config};
use familiar_core::scroll::{Scroll, ThumbMode, ThumbWheel};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tauri::utils::config::BundleType;
use tauri::{Emitter, Manager, State};
use tokio::sync::OnceCell;
use zbus::Connection;

type Result<T> = std::result::Result<T, String>;

/// The profile the app edits until per-app profiles arrive.
const PROFILE: &str = "default";

#[derive(Default)]
struct App {
    conn: OnceCell<Connection>,
}

impl App {
    async fn conn(&self) -> Result<&Connection> {
        self.conn.get_or_try_init(Connection::session).await.map_err(|e| format!("cannot reach the session bus: {e}"))
    }

    /// The daemon, or an error that says how to start it.
    async fn daemon(&self) -> Result<Daemon> {
        let conn = self.conn().await?.clone();
        let running = familiar_client::connect_with(&conn).await;
        running.ok_or_else(|| "familiard is not running. Start it with: systemctl --user start familiard".into())
    }
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn zerr(e: zbus::Error) -> String {
    familiar_client::clean(e).to_string()
}

#[tauri::command]
async fn snapshot(app: State<'_, App>) -> Result<Option<Snapshot>> {
    let conn = app.conn().await?.clone();
    match familiar_client::connect_with(&conn).await {
        Some(d) => d.snapshot().await.map(Some).map_err(err),
        None => Ok(None),
    }
}

#[tauri::command]
async fn set_setting(
    app: State<'_, App>,
    path: String,
    name: String,
    value: String,
) -> Result<BTreeMap<String, String>> {
    let d = app.daemon().await?;
    let dev = d.device_at(&path).await.map_err(err)?;
    Ok(dev.set(&name, &value, true).await.map_err(zerr)?.into_iter().collect())
}

#[tauri::command]
async fn play_haptic(app: State<'_, App>, path: String, waveform: String) -> Result<()> {
    let d = app.daemon().await?;
    d.device_at(&path).await.map_err(err)?.play_haptic(&waveform).await.map_err(zerr)
}

#[derive(Serialize)]
struct Host {
    channel: u8,
    paired: bool,
    name: String,
}

#[derive(Serialize)]
struct Hosts {
    current: u8,
    hosts: Vec<Host>,
}

#[tauri::command]
async fn hosts(app: State<'_, App>, path: String) -> Result<Hosts> {
    let d = app.daemon().await?;
    let (current, list) = d.device_at(&path).await.map_err(err)?.hosts().await.map_err(zerr)?;
    let hosts = list.into_iter().map(|(channel, paired, name)| Host { channel, paired, name }).collect();
    Ok(Hosts { current, hosts })
}

#[tauri::command]
async fn switch_host(app: State<'_, App>, path: String, channel: u8) -> Result<()> {
    let d = app.daemon().await?;
    d.device_at(&path).await.map_err(err)?.switch_host(channel).await.map_err(zerr)
}

#[tauri::command]
async fn rename_host(app: State<'_, App>, path: String, channel: u8, name: String) -> Result<String> {
    let d = app.daemon().await?;
    d.device_at(&path).await.map_err(err)?.rename_host(channel, &name).await.map_err(zerr)
}

#[tauri::command]
async fn reload(app: State<'_, App>) -> Result<()> {
    let d = app.daemon().await?;
    d.manager().await.map_err(err)?.reload().await.map_err(zerr)
}

/// A button binding as the page edits it.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum BindingView {
    Direct { action: String },
    Gestures { gestures: BTreeMap<String, String> },
}

const GESTURE_KEYS: &[&str] = &[
    "toggle",
    "tap",
    "double",
    "hold",
    "up",
    "down",
    "left",
    "right",
    "up-left",
    "up-right",
    "down-left",
    "down-right",
];

fn gesture_slot<'g>(g: &'g mut Gestures, key: &str) -> Option<&'g mut Option<Action>> {
    Some(match key {
        "toggle" => &mut g.toggle,
        "tap" => &mut g.tap,
        "double" => &mut g.double,
        "hold" => &mut g.hold,
        "up" => &mut g.up,
        "down" => &mut g.down,
        "left" => &mut g.left,
        "right" => &mut g.right,
        "up-left" => &mut g.up_left,
        "up-right" => &mut g.up_right,
        "down-left" => &mut g.down_left,
        "down-right" => &mut g.down_right,
        _ => return None,
    })
}

impl From<&Binding> for BindingView {
    fn from(b: &Binding) -> Self {
        match b {
            Binding::Direct(a) => BindingView::Direct { action: a.to_string() },
            Binding::Gestures(g) => {
                let mut copy = (**g).clone();
                let gestures = GESTURE_KEYS
                    .iter()
                    .filter_map(|k| {
                        let a = gesture_slot(&mut copy, k)?.as_ref()?;
                        Some((k.to_string(), a.to_string()))
                    })
                    .collect();
                BindingView::Gestures { gestures }
            }
        }
    }
}

impl TryFrom<BindingView> for Binding {
    type Error = String;

    fn try_from(v: BindingView) -> Result<Self> {
        match v {
            BindingView::Direct { action } => Ok(Binding::Direct(action.parse().map_err(err)?)),
            BindingView::Gestures { gestures } => {
                let mut g = Gestures::default();
                for (k, a) in gestures {
                    let slot = gesture_slot(&mut g, &k).ok_or_else(|| format!("unknown gesture {k:?}"))?;
                    *slot = Some(a.parse().map_err(|e| format!("{k}: {e}"))?);
                }
                if let Some(why) = g.conflict() {
                    return Err(why.to_string());
                }
                Ok(Binding::Gestures(Box::new(g)))
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThumbView {
    mode: String,
    speed: f32,
    tap: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfigView {
    path: String,
    buttons: BTreeMap<String, BindingView>,
    smooth: bool,
    speed: f32,
    thumb: Option<ThumbView>,
}

fn mode_name(m: ThumbMode) -> &'static str {
    match m {
        ThumbMode::Hscroll => "hscroll",
        ThumbMode::Volume => "volume",
        ThumbMode::Zoom => "zoom",
        ThumbMode::Tabs => "tabs",
        ThumbMode::None => "none",
    }
}

fn mode_from(s: &str) -> Result<ThumbMode> {
    Ok(match s {
        "hscroll" => ThumbMode::Hscroll,
        "volume" => ThumbMode::Volume,
        "zoom" => ThumbMode::Zoom,
        "tabs" => ThumbMode::Tabs,
        "none" => ThumbMode::None,
        other => return Err(format!("unknown thumb wheel mode {other:?}")),
    })
}

/// The config file the daemon reads, or the default one when it is not running.
async fn config_path(app: &App) -> PathBuf {
    if let Ok(d) = app.daemon().await
        && let Ok(m) = d.manager().await
        && let Ok(p) = m.config_path().await
    {
        return PathBuf::from(p);
    }
    Config::default_path()
}

/// Runs file work off the async threads.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(err)?
}

#[tauri::command]
async fn config(app: State<'_, App>) -> Result<ConfigView> {
    let path = config_path(&app).await;
    let load = path.clone();
    let c = blocking(move || Config::load(&load).map_err(err)).await?;
    let buttons = c.buttons(PROFILE)?.iter().map(|(k, b)| (k.clone(), BindingView::from(b))).collect();
    let w = c.wheels(PROFILE)?;
    let thumb = w.thumb.map(|t| ThumbView {
        mode: mode_name(t.mode()).into(),
        speed: t.speed(),
        tap: t.tap.as_ref().map(Action::to_string),
    });
    Ok(ConfigView {
        path: path.display().to_string(),
        buttons,
        smooth: w.scroll.smooth(),
        speed: w.scroll.speed(),
        thumb,
    })
}

/// Asks the daemon to apply the file now rather than when it notices the
/// change. When the daemon is not running it applies the file as it starts.
async fn apply(app: &App) -> Result<()> {
    let Ok(d) = app.daemon().await else { return Ok(()) };
    let m = d.manager().await.map_err(err)?;
    m.reload().await.map_err(|e| format!("Saved, but the mouse was not updated: {}", familiar_client::clean(e)))
}

#[tauri::command]
async fn set_button(app: State<'_, App>, button: String, binding: Option<BindingView>) -> Result<()> {
    let binding = binding.map(Binding::try_from).transpose()?;
    let path = config_path(&app).await;
    blocking(move || config::persist_button(&path, PROFILE, &button, binding.as_ref()).map_err(err)).await?;
    apply(&app).await
}

#[tauri::command]
async fn set_scroll(app: State<'_, App>, smooth: bool, speed: f32) -> Result<()> {
    let path = config_path(&app).await;
    let scroll = Scroll { smooth: Some(smooth), speed: Some(speed) };
    blocking(move || config::persist_scroll(&path, PROFILE, &scroll).map_err(err)).await?;
    apply(&app).await
}

#[tauri::command]
async fn set_thumb(app: State<'_, App>, thumb: Option<ThumbView>) -> Result<()> {
    let thumb = thumb
        .map(|t| -> Result<ThumbWheel> {
            let tap = t.tap.filter(|s| !s.is_empty()).map(|s| s.parse::<Action>().map_err(err)).transpose()?;
            Ok(ThumbWheel { mode: Some(mode_from(&t.mode)?), speed: Some(t.speed), tap })
        })
        .transpose()?;
    let path = config_path(&app).await;
    blocking(move || config::persist_thumb_wheel(&path, PROFILE, thumb.as_ref()).map_err(err)).await?;
    apply(&app).await
}

/// Checks an action string and returns it in canonical form.
#[tauri::command]
fn check_action(action: String) -> Result<String> {
    action.parse::<Action>().map(|a| a.to_string()).map_err(err)
}

#[tauri::command]
fn waveforms() -> Vec<String> {
    hidpp::features::haptic::Waveform::ALL.iter().map(|w| w.name().to_string()).collect()
}

/// Whether this copy came from a package the updater can replace. One built
/// from source is updated by building it again.
fn self_updating() -> bool {
    matches!(tauri::utils::platform::bundle_type(), Some(BundleType::Deb | BundleType::Rpm))
}

#[tauri::command]
fn self_updates() -> bool {
    self_updating()
}

/// Restarts the daemon if it is running, so the one just installed takes over.
#[tauri::command]
async fn restart_daemon() -> Result<()> {
    blocking(|| {
        for args in [&["--user", "daemon-reload"][..], &["--user", "try-restart", "familiard.service"]] {
            let status = std::process::Command::new("systemctl").args(args).status().map_err(err)?;
            if !status.success() {
                return Err(format!("systemctl {} failed: {status}", args.join(" ")));
            }
        }
        Ok(())
    })
    .await
}

/// Tells the page whenever the daemon publishes a change, or starts or stops.
/// If the bus is not reachable yet, or the stream ends, it tries again.
async fn forward_changes(app: tauri::AppHandle) {
    let state = app.state::<App>();
    loop {
        if let Ok(conn) = state.conn().await
            && let Ok(mut changes) = Daemon::changes(conn).await
        {
            let _ = app.emit("familiar-changed", ());
            while changes.next().await.is_some() {
                let _ = app.emit("familiar-changed", ());
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}

fn main() {
    let mut builder = tauri::Builder::default().manage(App::default()).plugin(tauri_plugin_process::init());
    if self_updating() {
        builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
    }
    builder
        .setup(|app| {
            tauri::async_runtime::spawn(forward_changes(app.handle().clone()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            snapshot,
            set_setting,
            play_haptic,
            hosts,
            switch_host,
            rename_host,
            reload,
            config,
            set_button,
            set_scroll,
            set_thumb,
            check_action,
            waveforms,
            self_updates,
            restart_daemon
        ])
        .run(tauri::generate_context!())
        .expect("the app window could not start");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bindings_round_trip_through_the_page_shape() {
        let direct = Binding::Direct("key:alt+left".parse().unwrap());
        let view = BindingView::from(&direct);
        assert!(matches!(&view, BindingView::Direct { action } if action == "key:KEY_LEFTALT+KEY_LEFT"));
        assert_eq!(Binding::try_from(view).unwrap(), direct);

        let mut gestures = BTreeMap::new();
        gestures.insert("tap".to_string(), "key:super".to_string());
        gestures.insert("up-left".to_string(), "key:volumeup".to_string());
        let binding = Binding::try_from(BindingView::Gestures { gestures }).unwrap();
        let back = BindingView::from(&binding);
        let BindingView::Gestures { gestures } = back else { panic!("expected gestures") };
        assert_eq!(gestures["up-left"], "key:KEY_VOLUMEUP");
        assert_eq!(gestures.len(), 2);
    }

    #[test]
    fn rejects_what_the_daemon_would_reject() {
        let mut bad = BTreeMap::new();
        bad.insert("sideways".to_string(), "none".to_string());
        assert!(Binding::try_from(BindingView::Gestures { gestures: bad }).is_err());

        let mut both = BTreeMap::new();
        both.insert("toggle".to_string(), "mouse:left".to_string());
        both.insert("tap".to_string(), "none".to_string());
        assert!(Binding::try_from(BindingView::Gestures { gestures: both }).is_err());

        assert!(Binding::try_from(BindingView::Direct { action: "key:nonsense".into() }).is_err());
        assert!(mode_from("spin").is_err());
        assert_eq!(check_action("key:ctrl+c".into()).unwrap(), "key:KEY_LEFTCTRL+KEY_C");
    }
}
