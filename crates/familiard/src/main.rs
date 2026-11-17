mod dbus;
mod desktop;
mod hotplug;
mod input;
mod manager;
mod session;
mod systemd;
mod worker;

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use familiar_core::config::Config;
use familiar_core::dbus::{BUS_NAME, MANAGER_PATH};
use tokio::signal::unix::{SignalKind, signal};
use tracing::{error, info};
use zbus::fdo::{RequestNameFlags, RequestNameReply};

use crate::dbus::{ManagerIface, ManagerState, Published};
use crate::manager::{Event, Manager};

fn parse_args() -> Result<PathBuf> {
    let mut args = std::env::args().skip(1);
    let mut config = Config::default_path();
    while let Some(a) = args.next() {
        match a.as_str() {
            "--config" => config = args.next().context("--config needs a path")?.into(),
            "--version" => {
                println!("familiard {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "--help" | "-h" => {
                println!("usage: familiard [--config PATH]\n\nSet RUST_LOG=debug for more detail.");
                std::process::exit(0);
            }
            other => bail!("unknown argument {other:?}"),
        }
    }
    // Clients are told this path, and they may run from another directory.
    if config.is_relative() {
        config = std::env::current_dir().context("finding the current directory")?.join(config);
    }
    Ok(config)
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with_target(false)
        .init();

    let config_path = parse_args()?;
    let config = match Config::load(&config_path) {
        Ok(c) => c,
        Err(e) => {
            error!("{e}; starting with an empty config");
            Config::default()
        }
    };

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Event>();

    let conn = zbus::Connection::session().await.context("connecting to the session bus")?;
    let published =
        Published::new(ManagerState { config_path: config_path.display().to_string(), ..Default::default() });
    let manager_iface = ManagerIface { tx: tx.clone(), state: published.clone() };
    conn.object_server().at(MANAGER_PATH, manager_iface).await?;
    let flags = RequestNameFlags::DO_NOT_QUEUE;
    match conn.request_name_with_flags(BUS_NAME, flags).await? {
        RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner => {}
        _ => bail!("another familiard is already running"),
    }
    info!("serving {BUS_NAME}, config {}", config_path.display());
    systemd::notify("READY=1");

    let input = input::spawn(Box::new(input::Uinput::default())).context("starting the input thread")?;
    let lock_tx = input.sender();
    tokio::spawn(async move {
        if let Err(e) = session::watch_lock(lock_tx).await {
            error!("cannot follow the screen lock; button actions will not pause when locked: {e}");
        }
    });
    let mut manager =
        Manager::new(conn.clone(), tx.clone(), config_path.clone(), config, published, Some(input.sender()));

    hotplug::watch_hidraw(tx.clone()).context("watching udev")?;
    let _watcher = hotplug::watch_config(config_path, tx.clone())
        .map_err(|e| error!("config changes will not be picked up: {e}"))
        .ok();
    for info in hidpp_hidraw::enumerate().unwrap_or_default() {
        manager.add_endpoint(info);
    }

    let mut term = signal(SignalKind::terminate())?;
    let mut int = signal(SignalKind::interrupt())?;
    // Pinged from this loop, so a stuck manager stops the pings and systemd restarts us.
    let watchdog = systemd::watchdog_interval();
    let mut ping = tokio::time::interval(watchdog.unwrap_or(Duration::from_secs(3600)));
    loop {
        tokio::select! {
            Some(event) = rx.recv() => manager.handle(event).await,
            _ = ping.tick(), if watchdog.is_some() => systemd::notify("WATCHDOG=1"),
            _ = term.recv() => break,
            _ = int.recv() => break,
        }
    }
    systemd::notify("STOPPING=1");
    info!("shutting down");
    tokio::task::block_in_place(|| {
        manager.shutdown();
        input.stop();
    });
    Ok(())
}
