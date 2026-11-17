use familiar_client as daemon;
mod diagnostics;
mod discover;
mod dump;
mod events;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Result, anyhow, bail};
use clap::builder::PossibleValuesParser;
use clap::{Parser, Subcommand};
use familiar_core::config::{Config, Identity};
use familiar_core::settings::{self, DeviceSettings, NAMES};
use hidpp::features::{haptic, hosts, info, reprog};
use hidpp::{Device, Exchange, Policy, receiver};

#[derive(Parser)]
#[command(name = "familiar", version, about = "Configure Logitech MX mice over HID++")]
struct Cli {
    /// hidraw node to use, e.g. /dev/hidraw6. Implies --direct.
    #[arg(long, global = true)]
    path: Option<PathBuf>,
    /// Device index behind a receiver, 1 to 6, or 255 for a direct connection.
    #[arg(long, short, global = true)]
    device: Option<u8>,
    /// Talk to the device even when familiard is running.
    #[arg(long, global = true)]
    direct: bool,
    /// Print JSON instead of text.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Show what familiard knows: devices, battery, settings and conflicts.
    Status,
    /// Make familiard reread its config file.
    Reload,
    /// List HID++ endpoints and the devices that answer on them.
    List,
    /// Describe the device: identity, features and every readable setting.
    Dump {
        /// Also call function 0 of unknown, non-internal features.
        #[arg(long)]
        research: bool,
    },
    /// Show current settings.
    Get,
    /// Compare the config file with what the device has.
    Diff,
    /// Change a setting. Through familiard it is also saved to the config.
    Set {
        #[arg(value_parser = PossibleValuesParser::new(NAMES))]
        setting: String,
        value: String,
        /// Through familiard, apply without saving to the config.
        #[arg(long)]
        temporary: bool,
    },
    /// Play a haptic waveform, or list them.
    Haptic {
        waveform: Option<String>,
        /// Play it this many times, talking to the device directly, and print each reply time.
        #[arg(long, default_value_t = 1)]
        repeat: u32,
        /// Milliseconds to wait between repeats.
        #[arg(long, default_value_t = 0)]
        gap: u64,
    },
    /// Show Easy-Switch hosts, or switch to channel 1-3.
    Host {
        channel: Option<u8>,
        /// Rename this channel instead of switching to it; without a channel, the current one.
        #[arg(long, value_name = "NAME")]
        rename: Option<String>,
    },
    /// Write a report for bug reports: system, device dump, daemon state, config
    /// and recent log. Nothing is sent anywhere.
    Diagnostics {
        /// Where to write it; defaults to familiar-diagnostics-<time>.txt here.
        output: Option<PathBuf>,
    },
    /// Put every control, the wheel and the thumb wheel back to plain reporting,
    /// on every device found. Safe to run at any time.
    Undivert,
    /// Print device events. Diverted controls are restored on exit.
    Events {
        /// Control to divert, by name such as gesture, haptic or back, or by hex CID. Repeatable.
        #[arg(long)]
        divert: Vec<String>,
        /// Also report raw pointer movement while a diverted control is held.
        #[arg(long)]
        raw_xy: bool,
        /// Report pointer movement all the time, which freezes the pointer. Only some controls allow it.
        #[arg(long)]
        force_raw_xy: bool,
        /// Divert the main wheel.
        #[arg(long)]
        wheel: bool,
        /// With --wheel, switch it to hi-res reporting while diverted.
        #[arg(long)]
        hires: bool,
        /// Divert the thumb wheel.
        #[arg(long)]
        thumb: bool,
    },
}

fn parse_control(s: &str) -> Result<u16> {
    if let Some(c) = reprog::control_by_name(s) {
        return Ok(c);
    }
    u16::from_str_radix(s.trim_start_matches("0x"), 16).map_err(|_| anyhow!("unknown control {s:?}"))
}

fn print_json<T: serde::Serialize>(v: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(v)?);
    Ok(())
}

fn cmd_list(cli: &Cli) -> Result<()> {
    for ep in discover::endpoints(cli.path.as_deref())? {
        let kind = if ep.is_receiver() { receiver::name(ep.info.product) } else { "direct" };
        println!(
            "{}  {:04x}:{:04x}  {}  ({kind})",
            ep.info.path.display(),
            ep.info.vendor,
            ep.info.product,
            ep.info.name
        );
        if ep.devices.is_empty() {
            println!("    no device answered");
        }
        for d in &ep.devices {
            let mut dev = Device::new(ep.link.clone() as Arc<dyn Exchange>, d.index);
            let name = if d.protocol.0 >= 2 { info::name(&mut dev).unwrap_or_default() } else { String::new() };
            println!("    index {:>3}  HID++ {}.{}  {name}", d.index, d.protocol.0, d.protocol.1);
        }
    }
    Ok(())
}

fn cmd_dump(cli: &Cli, research: bool) -> Result<()> {
    let policy = if research { Policy::Research } else { Policy::Normal };
    let mut sel = discover::select(cli.path.as_deref(), cli.device, policy)?;
    let d = dump::collect(&mut sel.device, sel.path.display().to_string())?;
    let extra = if research {
        let features = sel.device.enumerate()?;
        dump::research(&mut sel.device, &features)
    } else {
        Vec::new()
    };
    if cli.json {
        #[derive(serde::Serialize)]
        struct Out<'a> {
            #[serde(flatten)]
            dump: &'a dump::Dump,
            #[serde(skip_serializing_if = "Vec::is_empty")]
            research: Vec<(String, String)>,
        }
        return print_json(&Out { dump: &d, research: extra });
    }
    // Text output mirrors the JSON structure closely enough for reading by eye.
    let v = serde_json::to_value(&d)?;
    for (key, value) in v.as_object().into_iter().flatten() {
        if key == "features" || key == "controls" {
            continue;
        }
        println!("{key:<20} {}", serde_json::to_string(value)?);
    }
    println!("\nfeatures:");
    for f in &d.features {
        println!("  {:>2}  {}  v{}  {:<24} {}", f.index, f.id, f.version, f.name.unwrap_or("?"), f.flags.join(","));
    }
    if let Some(dump::Field::Ok(controls)) = &d.controls {
        println!("\ncontrols:");
        for c in controls {
            let rep = match &c.reporting {
                Some(dump::Field::Ok(r)) => format!("-> {} [{}]", r.remapped_to, r.flags.join(",")),
                Some(dump::Field::Err { error }) => format!("({error})"),
                None => String::new(),
            };
            println!("  {}  {:<16} task {}  [{}]  {rep}", c.cid, c.name.unwrap_or("?"), c.task, c.caps.join(","));
        }
    }
    if !extra.is_empty() {
        println!("\nresearch (function 0 of unknown features):");
        for (id, r) in extra {
            println!("  {id}  {r}");
        }
    }
    Ok(())
}

fn print_settings(cli: &Cli, m: &BTreeMap<String, String>) -> Result<()> {
    if cli.json {
        return print_json(m);
    }
    for name in NAMES {
        if let Some(v) = m.get(*name) {
            println!("{name:<14} {v}");
        }
    }
    Ok(())
}

fn runtime() -> Result<tokio::runtime::Runtime> {
    Ok(tokio::runtime::Builder::new_current_thread().enable_all().build()?)
}

/// Runs `f` against familiard when it is running and the user did not ask for
/// direct access. Returns `None` when the command should go direct instead.
fn via_daemon<T>(cli: &Cli, f: impl AsyncFnOnce(&daemon::Daemon) -> Result<T>) -> Result<Option<T>> {
    if cli.direct || cli.path.is_some() {
        return Ok(None);
    }
    let rt = runtime()?;
    rt.block_on(async {
        match daemon::connect().await {
            Some(d) => f(&d).await.map(Some),
            None => Ok(None),
        }
    })
}

fn daemon_running() -> bool {
    runtime().map(|rt| rt.block_on(daemon::connect()).is_some()).unwrap_or(false)
}

fn cmd_status(cli: &Cli) -> Result<()> {
    let out = via_daemon(cli, async |d| {
        let m = d.manager().await?;
        println!("familiard {}  config {}", m.version().await?, m.config_path().await?);
        let conflicts = m.conflicts().await?;
        if !conflicts.is_empty() {
            let verb = if conflicts.len() == 1 { "is" } else { "are" };
            println!("warning: {} {verb} running and may undo settings", conflicts.join(", "));
        }
        for dev in d.devices().await? {
            let state = if dev.online().await? { "online" } else { "offline" };
            let pct = dev.battery_percent().await?;
            println!(
                "\n{} ({})  {state}  battery {}% {}",
                dev.name().await?,
                dev.unit_id().await?,
                pct,
                dev.battery_status().await?
            );
            println!("  {} index {}", dev.endpoint().await?, dev.index().await?);
            let s = daemon::settings(&dev).await?;
            let line: Vec<String> = NAMES.iter().filter_map(|n| s.get(*n).map(|v| format!("{n}={v}"))).collect();
            println!("  {}", line.join("  "));
            let mut timing = format!("  settings applied {}", daemon::ago(dev.last_applied().await?));
            let ms = dev.reconnect_ms().await?;
            if ms > 0 {
                timing.push_str(&format!(", last reconnect ready in {ms} ms"));
            }
            println!("{timing}");
            for e in dev.apply_errors().await? {
                println!("  not applied: {e}");
            }
        }
        Ok(())
    })?;
    out.ok_or_else(|| anyhow!("familiard is not running"))
}

fn cmd_reload(cli: &Cli) -> Result<()> {
    via_daemon(cli, async |d| d.manager().await?.reload().await.map_err(daemon::clean))?
        .ok_or_else(|| anyhow!("familiard is not running"))
}

fn cmd_get(cli: &Cli) -> Result<()> {
    let m = match via_daemon(cli, async |d| daemon::settings(&d.device(cli.device).await?).await)? {
        Some(m) => m,
        None => {
            let mut sel = discover::select(cli.path.as_deref(), cli.device, Policy::Normal)?;
            settings::read(&mut sel.device).to_map()
        }
    };
    print_settings(cli, &m)
}

fn cmd_diff(cli: &Cli) -> Result<()> {
    let through = via_daemon(cli, async |d| {
        let dev = d.device(cli.device).await?;
        let applied = daemon::ago(dev.last_applied().await?);
        Ok((daemon::configured(&dev).await?, daemon::settings(&dev).await?, Some(applied)))
    })?;
    let (want, have, applied) = match through {
        Some(t) => t,
        None => {
            let config = Config::load(&Config::default_path())?;
            let mut sel = discover::select(cli.path.as_deref(), cli.device, Policy::Normal)?;
            let identity = identity(&mut sel.device);
            (config.settings_for(&identity).to_map(), settings::read(&mut sel.device).to_map(), None)
        }
    };
    let mut differ = 0;
    let rows: Vec<(&str, String, String, &str)> = NAMES
        .iter()
        .filter_map(|n| {
            let w = want.get(*n);
            let h = have.get(*n)?;
            let state = match w {
                None => "not configured",
                Some(w) if w == h => "",
                Some(_) => {
                    differ += 1;
                    "differs"
                }
            };
            Some((*n, w.cloned().unwrap_or_else(|| "-".into()), h.clone(), state))
        })
        .collect();
    if cli.json {
        let v: Vec<_> = rows
            .iter()
            .map(|(n, w, h, s)| serde_json::json!({"setting": n, "config": w, "device": h, "state": s}))
            .collect();
        return print_json(&v);
    }
    println!("{:<14} {:<10} {:<10}", "setting", "config", "device");
    for (n, w, h, s) in rows {
        println!("{n:<14} {w:<10} {h:<10} {s}");
    }
    if let Some(a) = applied {
        println!("\nsettings applied {a}");
    }
    if differ > 0 {
        bail!("{differ} setting(s) differ from the config");
    }
    Ok(())
}

/// What a config section can match this device by.
fn identity(dev: &mut Device) -> Identity {
    let ids = info::ids(dev).ok();
    Identity {
        unit_id: ids.as_ref().map(|i| i.unit_id.clone()).unwrap_or_default(),
        models: ids.map(|i| i.transports.into_iter().map(|(_, m)| m).collect()).unwrap_or_default(),
        name: info::name(dev).unwrap_or_default(),
    }
}

fn cmd_set(cli: &Cli, name: &str, value: &str, temporary: bool) -> Result<()> {
    let mut want = DeviceSettings::default();
    want.set_str(name, value)?;
    let through = via_daemon(cli, async |d| {
        let dev = d.device(cli.device).await?;
        let m = dev.set(name, value, !temporary).await.map_err(daemon::clean)?;
        Ok(m.into_iter().collect::<BTreeMap<_, _>>())
    })?;
    let m = match through {
        Some(m) => m,
        None => {
            let mut sel = discover::select(cli.path.as_deref(), cli.device, Policy::Normal)?;
            let applied = settings::apply(&mut sel.device, &want);
            if let Some((_, e)) = applied.errors.into_iter().next() {
                bail!("{name}: {e}");
            }
            if daemon_running() {
                eprintln!("note: familiard will apply its config again when the device reconnects");
            }
            settings::read(&mut sel.device).to_map()
        }
    };
    print_settings(cli, &m)
}

fn cmd_haptic(cli: &Cli, waveform: Option<&str>, repeat: u32, gap: u64) -> Result<()> {
    if let Some(name) = waveform {
        let w = haptic::Waveform::from_name(name)
            .ok_or_else(|| anyhow!("unknown waveform {name:?}; run without arguments to list them"))?;
        if repeat > 1 {
            let mut sel = discover::select(cli.path.as_deref(), cli.device, Policy::Normal)?;
            for i in 1..=repeat {
                let started = std::time::Instant::now();
                let r = haptic::play(&mut sel.device, w);
                let ms = started.elapsed().as_secs_f64() * 1000.0;
                match r {
                    Ok(()) => println!("{i:>3}  accepted in {ms:.1} ms"),
                    Err(e) => println!("{i:>3}  {e} after {ms:.1} ms"),
                }
                std::thread::sleep(std::time::Duration::from_millis(gap));
            }
            return Ok(());
        }
        let done =
            via_daemon(cli, async |d| d.device(cli.device).await?.play_haptic(name).await.map_err(daemon::clean))?;
        if done.is_some() {
            return Ok(());
        }
        let mut sel = discover::select(cli.path.as_deref(), cli.device, Policy::Normal)?;
        if !haptic::capabilities(&mut sel.device)?.supports(w) {
            bail!("this device does not support {name}");
        }
        return Ok(haptic::play(&mut sel.device, w)?);
    }
    let mut sel = discover::select(cli.path.as_deref(), cli.device, Policy::Normal)?;
    for w in haptic::capabilities(&mut sel.device)?.waveforms {
        println!("{}", w.name());
    }
    Ok(())
}

fn cmd_host(cli: &Cli, channel: Option<u8>, rename: Option<&str>) -> Result<()> {
    if channel == Some(0) {
        bail!("channels are numbered from 1");
    }
    if let Some(name) = rename {
        let renamed = via_daemon(cli, async |d| {
            let dev = d.device(cli.device).await?;
            let (current, list) = dev.hosts().await.map_err(daemon::clean)?;
            let n = channel.unwrap_or(current);
            if usize::from(n) > list.len() {
                bail!("channel must be between 1 and {}", list.len());
            }
            Ok((n, dev.rename_host(n, name).await.map_err(daemon::clean)?))
        })?;
        let (n, stored) = match renamed {
            Some(r) => r,
            None => {
                let mut sel = discover::select(cli.path.as_deref(), cli.device, Policy::Normal)?;
                let state = hosts::state(&mut sel.device)?;
                let host = match channel {
                    Some(n) if n > state.count => bail!("channel must be between 1 and {}", state.count),
                    Some(n) => n - 1,
                    None => state.current,
                };
                (host + 1, hosts::rename(&mut sel.device, host, name)?)
            }
        };
        println!("channel {n} is now {stored:?}");
        return Ok(());
    }
    if let Some(n) = channel {
        let done = via_daemon(cli, async |d| d.device(cli.device).await?.switch_host(n).await.map_err(daemon::clean))?;
        if done.is_none() {
            let mut sel = discover::select(cli.path.as_deref(), cli.device, Policy::Normal)?;
            let state = hosts::state(&mut sel.device)?;
            if n > state.count {
                bail!("channel must be between 1 and {}", state.count);
            }
            hosts::switch_to(&mut sel.device, n - 1)?;
        }
        println!("switched to channel {n}");
        return Ok(());
    }
    // The daemon numbers channels from 1; the device and the JSON output from 0.
    let listed = via_daemon(cli, async |d| d.device(cli.device).await?.hosts().await.map_err(daemon::clean))?;
    let (current, list) = match listed {
        Some((current, list)) => {
            let list = list
                .into_iter()
                .map(|(channel, paired, name)| hosts::Host { index: channel.saturating_sub(1), paired, name })
                .collect();
            (current.saturating_sub(1), list)
        }
        None => {
            let mut sel = discover::select(cli.path.as_deref(), cli.device, Policy::Normal)?;
            let state = hosts::state(&mut sel.device)?;
            (state.current, hosts::hosts(&mut sel.device).unwrap_or_default())
        }
    };
    if cli.json {
        return print_json(&list);
    }
    for h in list {
        let mark = if h.index == current { "*" } else { " " };
        let paired = if h.paired { h.name.as_str() } else { "(not paired)" };
        println!("{mark} {}  {paired}", h.index + 1);
    }
    Ok(())
}

fn cmd_undivert(cli: &Cli) -> Result<()> {
    let mut failed = false;
    for ep in discover::endpoints(cli.path.as_deref())? {
        for d in ep.devices.iter().filter(|d| d.protocol.0 >= 2 && cli.device.is_none_or(|i| i == d.index)) {
            let mut dev = Device::new(ep.link.clone() as Arc<dyn Exchange>, d.index);
            match familiar_core::diversion::clear(&mut dev) {
                Ok(c) if c.is_empty() => println!("{} index {}: nothing diverted", ep.info.path.display(), d.index),
                Ok(c) => println!("{} index {}: cleared {}", ep.info.path.display(), d.index, c.join(", ")),
                Err(e) => {
                    eprintln!("{} index {}: {e}", ep.info.path.display(), d.index);
                    failed = true;
                }
            }
        }
    }
    if failed { bail!("some devices could not be reset") } else { Ok(()) }
}

fn main() -> Result<()> {
    // Exit quietly when piped into a program that stops reading, like `head`.
    // SAFETY: resetting a signal disposition before any threads exist.
    unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) };
    let cli = Cli::parse();
    match &cli.command {
        Command::Status => cmd_status(&cli),
        Command::Reload => cmd_reload(&cli),
        Command::List => cmd_list(&cli),
        Command::Dump { research } => cmd_dump(&cli, *research),
        Command::Get => cmd_get(&cli),
        Command::Diff => cmd_diff(&cli),
        Command::Set { setting, value, temporary } => cmd_set(&cli, setting, value, *temporary),
        Command::Haptic { waveform, repeat, gap } => cmd_haptic(&cli, waveform.as_deref(), *repeat, *gap),
        Command::Host { channel, rename } => cmd_host(&cli, *channel, rename.as_deref()),
        Command::Undivert => cmd_undivert(&cli),
        Command::Diagnostics { output } => {
            let path = diagnostics::write(output.as_deref(), cli.path.as_deref(), cli.device)?;
            println!("wrote {}", path.display());
            println!("It contains your device's unit ID and your config; read it before sharing.");
            Ok(())
        }
        Command::Events { divert, raw_xy, force_raw_xy, wheel, hires, thumb } => {
            let divert = divert.iter().map(|s| parse_control(s)).collect::<Result<Vec<_>>>()?;
            let mut sel = discover::select(cli.path.as_deref(), cli.device, Policy::Normal)?;
            let opts = events::Options {
                divert,
                raw_xy: *raw_xy,
                force_raw_xy: *force_raw_xy,
                wheel: *wheel,
                hires: *hires,
                thumb: *thumb,
                json: cli.json,
            };
            events::run(&mut sel.device, sel.events, opts)
        }
    }
}
