//! The user's config file, `$XDG_CONFIG_HOME/familiar/config.toml`.
//!
//! ```toml
//! [device."*"]             # every device
//! haptic = 60
//!
//! [device."B042"]          # by model ID
//! dpi = 1600
//!
//! [device."1A2B3C4D"]      # by unit ID, the most specific
//! smartshift = "off"
//! ```
//!
//! `include` lists further files, relative to this one, that are merged on top
//! in order. `{hostname}` in a name is replaced by the short host name, and
//! missing files are skipped, so one repository can serve several machines.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{env, fs, io};

use serde::{Deserialize, Serialize};

use crate::actions::{BUTTONS, Binding};
use crate::scroll::{Scroll, ThumbWheel, Wheels};
use crate::settings::{DeviceSettings, wheel_mode_name};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Config {
    /// Schema version. Files without one are version 1.
    #[serde(default = "current_version")]
    pub version: u32,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub device: BTreeMap<String, DeviceSettings>,
    #[serde(default)]
    pub notifications: Notifications,
    /// Button behaviour, by profile. `default` applies everywhere; others
    /// apply when their `match` fits the focused app.
    #[serde(default)]
    pub profile: BTreeMap<String, Profile>,
    /// The file that last defined each device section, where saves go.
    #[serde(skip)]
    pub sources: BTreeMap<String, PathBuf>,
    /// Included files that were found and merged.
    #[serde(skip)]
    pub included: Vec<PathBuf>,
    /// Every file that sets each profile entry, in load order. Keys look like
    /// `default.buttons.back`, `default.scroll` and `default.thumb-wheel`.
    #[serde(skip)]
    pub profile_sources: BTreeMap<String, Vec<PathBuf>>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Profile {
    #[serde(rename = "match")]
    pub matches: Option<Match>,
    /// Profile whose buttons this one starts from.
    pub inherit: Option<String>,
    #[serde(default)]
    pub buttons: BTreeMap<String, Binding>,
    pub scroll: Option<Scroll>,
    pub thumb_wheel: Option<ThumbWheel>,
}

/// Which focused app a profile applies to. Any field that is set must match.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Match {
    pub app_id: Option<String>,
    pub exe: Option<String>,
    pub title: Option<String>,
}

/// What an included file may contain.
#[derive(Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct Fragment {
    #[serde(default)]
    device: BTreeMap<String, DeviceSettings>,
    #[serde(default)]
    notifications: PartialNotifications,
    #[serde(default)]
    profile: BTreeMap<String, Profile>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct PartialNotifications {
    battery_low: Option<u8>,
    battery_full: Option<bool>,
}

/// The short host name used for `{hostname}` in includes.
pub fn hostname() -> String {
    let raw = fs::read_to_string("/proc/sys/kernel/hostname")
        .ok()
        .or_else(|| env::var("HOSTNAME").ok())
        .or_else(|| env::var("COMPUTERNAME").ok())
        .unwrap_or_default();
    raw.trim().split('.').next().unwrap_or("").to_string()
}

pub const VERSION: u32 = 1;

fn current_version() -> u32 {
    VERSION
}

impl Default for Config {
    fn default() -> Self {
        Config {
            version: VERSION,
            include: Vec::new(),
            device: BTreeMap::new(),
            notifications: Notifications::default(),
            profile: BTreeMap::new(),
            sources: BTreeMap::new(),
            included: Vec::new(),
            profile_sources: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields, default)]
pub struct Notifications {
    /// Notify when the battery drops to this percentage; 0 disables.
    pub battery_low: u8,
    /// Notify when charging completes.
    pub battery_full: bool,
}

impl Default for Notifications {
    fn default() -> Self {
        Notifications { battery_low: 15, battery_full: true }
    }
}

/// What a config section can match a device by.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Identity {
    pub unit_id: String,
    /// Model product IDs such as `B042`.
    pub models: Vec<String>,
    pub name: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("reading {path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("{path}: {source}")]
    Parse { path: PathBuf, source: Box<toml::de::Error> },
    #[error("{path}: {source}")]
    Edit { path: PathBuf, source: toml_edit::TomlError },
    #[error("{path}: {msg}")]
    Shape { path: PathBuf, msg: String },
}

/// Section precedence, lowest first. `None` means the key does not match.
fn rank(key: &str, id: &Identity) -> Option<u8> {
    if key == "*" {
        Some(0)
    } else if !id.name.is_empty() && key.eq_ignore_ascii_case(&id.name) {
        Some(1)
    } else if id.models.iter().any(|m| key.eq_ignore_ascii_case(m)) {
        Some(2)
    } else if !id.unit_id.is_empty() && key.eq_ignore_ascii_case(&id.unit_id) {
        Some(3)
    } else {
        None
    }
}

impl Config {
    pub fn default_path() -> PathBuf {
        let base = env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|h| Path::new(&h).join(".config")))
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("familiar").join("config.toml")
    }

    /// Loads the config and its includes. A missing file is an empty config.
    pub fn load(path: &Path) -> Result<Config, ConfigError> {
        Self::load_with_host(path, &hostname())
    }

    pub fn load_with_host(path: &Path, host: &str) -> Result<Config, ConfigError> {
        Self::load_from(path, host, &read_optional)
    }

    /// Loads with `read` supplying each file's text, so edits can be checked
    /// before they are written.
    fn load_from(path: &Path, host: &str, read: &dyn Fn(&Path) -> ReadResult) -> Result<Config, ConfigError> {
        let Some(text) = read(path)? else { return Ok(Config::default()) };
        let mut config: Config =
            toml::from_str(&text).map_err(|e| ConfigError::Parse { path: path.to_path_buf(), source: Box::new(e) })?;
        if config.version > VERSION {
            return Err(ConfigError::Shape {
                path: path.to_path_buf(),
                msg: format!("version {} is newer than this program understands ({VERSION})", config.version),
            });
        }
        config.sources = config.device.keys().map(|k| (k.clone(), path.to_path_buf())).collect();
        let mut profile_sources = BTreeMap::new();
        note_profiles(&mut profile_sources, &config.profile, path);

        let dir = path.parent().unwrap_or(Path::new("."));
        for pattern in config.include.clone() {
            let file = dir.join(pattern.replace("{hostname}", host));
            let Some(text) = read(&file)? else { continue };
            let frag: Fragment =
                toml::from_str(&text).map_err(|e| ConfigError::Parse { path: file.clone(), source: Box::new(e) })?;
            for (key, settings) in frag.device {
                config.device.entry(key.clone()).or_default().merge_from(&settings);
                config.sources.insert(key, file.clone());
            }
            if let Some(v) = frag.notifications.battery_low {
                config.notifications.battery_low = v;
            }
            if let Some(v) = frag.notifications.battery_full {
                config.notifications.battery_full = v;
            }
            note_profiles(&mut profile_sources, &frag.profile, &file);
            for (name, p) in frag.profile {
                let into = config.profile.entry(name).or_default();
                if p.matches.is_some() {
                    into.matches = p.matches;
                }
                if p.inherit.is_some() {
                    into.inherit = p.inherit;
                }
                into.buttons.extend(p.buttons);
                if let Some(s) = p.scroll {
                    into.scroll.get_or_insert_with(Scroll::default).merge_from(&s);
                }
                if let Some(t) = p.thumb_wheel {
                    into.thumb_wheel.get_or_insert_with(ThumbWheel::default).merge_from(&t);
                }
            }
            config.included.push(file);
        }
        config.profile_sources = profile_sources;
        config.check_profiles().map_err(|msg| ConfigError::Shape { path: path.to_path_buf(), msg })?;
        Ok(config)
    }

    /// The nearest profile `profile` inherits from that satisfies `has`.
    fn inherited_from(&self, profile: &str, has: impl Fn(&Profile) -> bool) -> Option<String> {
        let mut at = self.profile.get(profile).and_then(|p| p.inherit.clone());
        for _ in 0..self.profile.len() {
            let name = at?;
            let p = self.profile.get(&name)?;
            if has(p) {
                return Some(name);
            }
            at = p.inherit.clone();
        }
        None
    }

    fn check_profiles(&self) -> Result<(), String> {
        for (name, p) in &self.profile {
            if let Some(b) = p.buttons.keys().find(|b| !BUTTONS.contains(&b.as_str())) {
                return Err(format!("profile {name:?}: unknown button {b:?}; buttons are {}", BUTTONS.join(", ")));
            }
            if let Some(parent) = &p.inherit
                && !self.profile.contains_key(parent)
            {
                return Err(format!("profile {name:?} inherits from {parent:?}, which does not exist"));
            }
            for (button, binding) in &p.buttons {
                if let Binding::Gestures(g) = binding
                    && let Some(why) = g.conflict()
                {
                    return Err(format!("profile {name:?}, button {button:?}: {why}"));
                }
            }
            self.buttons(name).map_err(|e| format!("profile {name:?}: {e}"))?;
        }
        Ok(())
    }

    /// The profile and its ancestors, the most distant first.
    fn chain(&self, profile: &str) -> Result<Vec<&Profile>, String> {
        let mut names = Vec::new();
        let mut at = Some(profile.to_string());
        while let Some(name) = at {
            if names.contains(&name) {
                return Err(format!("inherit loop through {name:?}"));
            }
            at = self.profile.get(&name).and_then(|p| p.inherit.clone());
            names.push(name);
        }
        Ok(names.iter().rev().filter_map(|n| self.profile.get(n)).collect())
    }

    /// The buttons of a profile with everything it inherits merged in, the
    /// profile's own bindings winning.
    pub fn buttons(&self, profile: &str) -> Result<BTreeMap<String, Binding>, String> {
        let mut out = BTreeMap::new();
        for p in self.chain(profile)? {
            out.extend(p.buttons.clone());
        }
        Ok(out)
    }

    /// Wheel behaviour of a profile, merged the same way as buttons.
    pub fn wheels(&self, profile: &str) -> Result<Wheels, String> {
        let mut out = Wheels::default();
        for p in self.chain(profile)? {
            if let Some(s) = &p.scroll {
                out.scroll.merge_from(s);
            }
            if let Some(t) = &p.thumb_wheel {
                out.thumb.get_or_insert_with(ThumbWheel::default).merge_from(t);
            }
        }
        Ok(out)
    }

    /// The file a setting for `section` should be saved in: the last file that
    /// defines that section, so the saved value is the one that takes effect.
    pub fn file_for(&self, section: &str, main: &Path) -> PathBuf {
        self.sources.get(section).cloned().unwrap_or_else(|| main.to_path_buf())
    }

    /// Merged settings for a device, from the least to the most specific section.
    pub fn settings_for(&self, id: &Identity) -> DeviceSettings {
        let mut matched: Vec<(u8, &DeviceSettings)> =
            self.device.iter().filter_map(|(k, s)| rank(k, id).map(|r| (r, s))).collect();
        matched.sort_by_key(|(r, _)| *r);
        let mut out = DeviceSettings::default();
        for (_, s) in matched {
            out.merge_from(s);
        }
        out.keep_in_range();
        out
    }

    /// The section a new setting for this device should go in: the most specific
    /// existing one that is not `*`, or a new one named after the unit ID, the
    /// model or the name, whichever the device has.
    pub fn section_for(&self, id: &Identity) -> String {
        let fallback = [id.unit_id.as_str(), id.models.first().map_or("", String::as_str), id.name.as_str()]
            .into_iter()
            .find(|k| !k.is_empty())
            .unwrap_or("*");
        self.device
            .keys()
            .filter_map(|k| rank(k, id).filter(|r| *r > 0).map(|r| (r, k)))
            .max_by_key(|(r, _)| *r)
            .map(|(_, k)| k.clone())
            .unwrap_or_else(|| fallback.to_string())
    }
}

type ReadResult = Result<Option<String>, ConfigError>;

fn note_profiles(into: &mut BTreeMap<String, Vec<PathBuf>>, profiles: &BTreeMap<String, Profile>, file: &Path) {
    for (name, p) in profiles {
        let mut keys: Vec<String> = p.buttons.keys().map(|b| format!("{name}.buttons.{b}")).collect();
        if p.scroll.is_some() {
            keys.push(format!("{name}.scroll"));
        }
        if p.thumb_wheel.is_some() {
            keys.push(format!("{name}.thumb-wheel"));
        }
        for key in keys {
            let files = into.entry(key).or_default();
            if files.last().map(PathBuf::as_path) != Some(file) {
                files.push(file.to_path_buf());
            }
        }
    }
}

fn read_optional(path: &Path) -> ReadResult {
    match fs::read_to_string(path) {
        Ok(t) => Ok(Some(t)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(ConfigError::Io { path: path.to_path_buf(), source: e }),
    }
}

/// Where the previous version of a file is kept when we rewrite it.
pub fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".bak");
    path.with_file_name(name)
}

/// One change to one file, made to it as a document so comments and layout
/// survive.
type Edit<'a> = Box<dyn FnOnce(&mut toml_edit::DocumentMut) -> Result<(), String> + 'a>;

/// Takes an exclusive lock beside `path` for as long as the returned file is
/// held, so the app and the daemon take turns rewriting it.
fn lock(path: &Path) -> Result<fs::File, ConfigError> {
    let mut name = std::ffi::OsString::from(".");
    name.push(path.file_name().unwrap_or_default());
    name.push(".lock");
    let lock = path.with_file_name(name);
    let io_err = |e: io::Error| ConfigError::Io { path: lock.clone(), source: e };
    if let Some(dir) = lock.parent() {
        fs::create_dir_all(dir).map_err(|e| ConfigError::Io { path: dir.to_path_buf(), source: e })?;
    }
    let file = fs::OpenOptions::new().create(true).truncate(false).write(true).open(&lock).map_err(io_err)?;
    file.lock().map_err(io_err)?;
    Ok(file)
}

/// Applies edits to the main config file and its includes together. Nothing
/// is written unless the merged result still loads. Each rewritten file keeps
/// its previous version beside it with a `.bak` suffix.
fn edit_files(main: &Path, host: &str, edits: Vec<(PathBuf, Edit<'_>)>) -> Result<(), ConfigError> {
    let mut targets: Vec<PathBuf> = edits.iter().map(|(p, _)| p.clone()).collect();
    targets.sort();
    targets.dedup();
    let _locks = targets.iter().map(|p| lock(p)).collect::<Result<Vec<_>, _>>()?;

    let mut changed: Vec<(PathBuf, String, String)> = Vec::new();
    for (path, f) in edits {
        let (old, current) = match changed.iter().position(|(p, ..)| *p == path) {
            Some(i) => {
                let (_, old, new) = changed.remove(i);
                (old, new)
            }
            None => {
                let text = read_optional(&path)?.unwrap_or_default();
                (text.clone(), text)
            }
        };
        let mut doc: toml_edit::DocumentMut =
            current.parse().map_err(|e| ConfigError::Edit { path: path.clone(), source: e })?;
        f(&mut doc).map_err(|msg| ConfigError::Shape { path: path.clone(), msg })?;
        changed.push((path, old, doc.to_string()));
    }

    Config::load_from(main, host, &|p: &Path| match changed.iter().find(|(q, ..)| q == p) {
        Some((_, _, new)) => Ok(Some(new.clone())),
        None => read_optional(p),
    })?;
    for (path, old, new) in &changed {
        if old != new {
            write_with_backup(path, old, new)?;
        }
    }
    Ok(())
}

fn write_with_backup(path: &Path, old: &str, new: &str) -> Result<(), ConfigError> {
    // A symlinked config, as from a dotfiles repository, is written where it
    // points, so the link survives.
    let linked = fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink());
    let target = if linked { fs::canonicalize(path).ok() } else { None };
    let path = target.as_deref().unwrap_or(path);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| ConfigError::Io { path: dir.to_path_buf(), source: e })?;
    }
    // A file the user made private stays private, and so does its backup.
    let mode = fs::metadata(path).ok().map(|m| m.permissions());
    if !old.is_empty() {
        let bak = backup_path(path);
        write_with_mode(&bak, old, mode.as_ref()).map_err(|e| ConfigError::Io { path: bak.clone(), source: e })?;
    }
    let tmp = path.with_extension("toml.tmp");
    write_with_mode(&tmp, new, mode.as_ref()).map_err(|e| ConfigError::Io { path: tmp.clone(), source: e })?;
    fs::rename(&tmp, path).map_err(|e| ConfigError::Io { path: path.to_path_buf(), source: e })
}

/// Writes a new file, setting its permissions before any content goes in and
/// flushing it to disk. A file already there is removed first, since it may
/// have been left read-only.
fn write_with_mode(path: &Path, text: &str, mode: Option<&fs::Permissions>) -> io::Result<()> {
    use io::Write;
    match fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    let mut file = fs::File::create_new(path)?;
    if let Some(mode) = mode {
        file.set_permissions(mode.clone())?;
    }
    file.write_all(text.as_bytes())?;
    file.sync_all()
}

/// The table at `profile.<name>.<section>`, created when missing.
fn profile_table<'d>(
    doc: &'d mut toml_edit::DocumentMut,
    name: &str,
    section: &str,
) -> Result<&'d mut dyn toml_edit::TableLike, String> {
    let implicit = || {
        let mut t = toml_edit::Table::new();
        t.set_implicit(true);
        toml_edit::Item::Table(t)
    };
    doc.entry("profile")
        .or_insert_with(implicit)
        .as_table_like_mut()
        .ok_or("`profile` must be a table")?
        .entry(name)
        .or_insert_with(implicit)
        .as_table_like_mut()
        .ok_or_else(|| format!("`profile.{name}` must be a table"))?
        .entry(section)
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_like_mut()
        .ok_or_else(|| format!("`profile.{name}.{section}` must be a table"))
}

fn binding_item(b: &Binding) -> toml_edit::Item {
    match b {
        Binding::Direct(a) => toml_edit::value(a.to_string()),
        Binding::Gestures(g) => {
            let mut t = toml_edit::InlineTable::new();
            let fields = [
                ("toggle", &g.toggle),
                ("tap", &g.tap),
                ("double", &g.double),
                ("hold", &g.hold),
                ("up", &g.up),
                ("down", &g.down),
                ("left", &g.left),
                ("right", &g.right),
                ("up-left", &g.up_left),
                ("up-right", &g.up_right),
                ("down-left", &g.down_left),
                ("down-right", &g.down_right),
            ];
            for (k, v) in fields {
                if let Some(a) = v {
                    t.insert(k, a.to_string().into());
                }
            }
            toml_edit::value(t)
        }
    }
}

/// Sets `key` in `table`. A value that replaces another keeps the comments
/// and spacing around it.
fn put(table: &mut dyn toml_edit::TableLike, key: &str, mut item: toml_edit::Item) {
    if let Some(slot) = table.get_mut(key)
        && let (Some(old), Some(new)) = (slot.as_value(), item.as_value_mut())
    {
        *new.decor_mut() = old.decor().clone();
        *slot = item;
    } else {
        table.insert(key, item);
    }
}

/// Removes `keys`, a path below `[profile.<profile>]`, when it exists.
fn remove_from_profile(doc: &mut toml_edit::DocumentMut, profile: &str, keys: &[&str]) {
    let Some((last, parents)) = keys.split_last() else { return };
    let mut t = doc.get_mut("profile").and_then(|p| p.get_mut(profile)).and_then(|p| p.as_table_like_mut());
    for k in parents {
        t = t.and_then(|t| t.get_mut(k)).and_then(|i| i.as_table_like_mut());
    }
    if let Some(t) = t {
        t.remove(last);
    }
}

/// Sets the fields that are `Some` in `[profile.<profile>.<section>]` when
/// `here`, and removes the ones that are `None` wherever they are.
fn set_fields(
    doc: &mut toml_edit::DocumentMut,
    profile: &str,
    section: &str,
    here: bool,
    fields: Vec<(&str, Option<toml_edit::Item>)>,
) -> Result<(), String> {
    if here {
        let t = profile_table(doc, profile, section)?;
        for (k, v) in fields {
            match v {
                Some(item) => put(t, k, item),
                None => {
                    t.remove(k);
                }
            }
        }
    } else {
        for (k, v) in fields {
            if v.is_none() {
                remove_from_profile(doc, profile, &[section, k]);
            }
        }
    }
    Ok(())
}

fn rounded(v: f32) -> toml_edit::Item {
    toml_edit::value((v as f64 * 100.0).round() / 100.0)
}

fn shape(path: &Path, msg: String) -> ConfigError {
    ConfigError::Shape { path: path.to_path_buf(), msg }
}

/// The file a profile entry should be written to, where its value takes
/// effect, and every file that sets it now.
fn profile_files(config: &Config, main: &Path, key: &str) -> (PathBuf, Vec<PathBuf>) {
    let mut files = config.profile_sources.get(key).cloned().unwrap_or_default();
    let target = files.last().cloned().unwrap_or_else(|| main.to_path_buf());
    if !files.contains(&target) {
        files.push(target.clone());
    }
    (target, files)
}

/// Sets one button in `[profile.<profile>.buttons]`, in the file where it
/// takes effect. With `None` it is removed from every file that sets it.
pub fn persist_button(path: &Path, profile: &str, button: &str, binding: Option<&Binding>) -> Result<(), ConfigError> {
    let host = hostname();
    let config = Config::load_with_host(path, &host)?;
    let (target, files) = profile_files(&config, path, &format!("{profile}.buttons.{button}"));
    let edits: Vec<(PathBuf, Edit)> = match binding {
        Some(b) => vec![(
            target,
            Box::new(move |doc: &mut toml_edit::DocumentMut| {
                put(profile_table(doc, profile, "buttons")?, button, binding_item(b));
                Ok(())
            }),
        )],
        None => {
            if let Some(parent) = config.inherited_from(profile, |p| p.buttons.contains_key(button)) {
                let msg = format!("profile {profile:?} inherits {button:?} from {parent:?}; change it there");
                return Err(shape(path, msg));
            }
            let edits = files.into_iter().map(|file| -> (PathBuf, Edit) {
                (
                    file,
                    Box::new(move |doc: &mut toml_edit::DocumentMut| {
                        remove_from_profile(doc, profile, &["buttons", button]);
                        Ok(())
                    }),
                )
            });
            edits.collect()
        }
    };
    edit_files(path, &host, edits)
}

/// Writes `[profile.<profile>.scroll]` in the file where it takes effect.
/// Fields that are `None` are removed from every file that sets them.
pub fn persist_scroll(path: &Path, profile: &str, scroll: &Scroll) -> Result<(), ConfigError> {
    let host = hostname();
    let config = Config::load_with_host(path, &host)?;
    let (target, files) = profile_files(&config, path, &format!("{profile}.scroll"));
    let edits = files.into_iter().map(|file| -> (PathBuf, Edit) {
        let here = file == target;
        let scroll = scroll.clone();
        (
            file,
            Box::new(move |doc: &mut toml_edit::DocumentMut| {
                let fields =
                    vec![("smooth", scroll.smooth.map(toml_edit::value)), ("speed", scroll.speed.map(rounded))];
                set_fields(doc, profile, "scroll", here, fields)
            }),
        )
    });
    edit_files(path, &host, edits.collect())
}

/// Writes `[profile.<profile>.thumb-wheel]` in the file where it takes
/// effect. `None` removes it from every file, which gives the thumb wheel back
/// to the system.
pub fn persist_thumb_wheel(path: &Path, profile: &str, thumb: Option<&ThumbWheel>) -> Result<(), ConfigError> {
    let host = hostname();
    let config = Config::load_with_host(path, &host)?;
    let (target, files) = profile_files(&config, path, &format!("{profile}.thumb-wheel"));
    let Some(thumb) = thumb else {
        if let Some(parent) = config.inherited_from(profile, |p| p.thumb_wheel.is_some()) {
            let msg = format!("profile {profile:?} inherits its thumb wheel from {parent:?}; change it there");
            return Err(shape(path, msg));
        }
        let edits = files.into_iter().map(|file| -> (PathBuf, Edit) {
            (
                file,
                Box::new(move |doc: &mut toml_edit::DocumentMut| {
                    remove_from_profile(doc, profile, &["thumb-wheel"]);
                    Ok(())
                }),
            )
        });
        return edit_files(path, &host, edits.collect());
    };
    let mode = thumb.mode.map(|m| match m {
        crate::scroll::ThumbMode::Hscroll => "hscroll",
        crate::scroll::ThumbMode::Volume => "volume",
        crate::scroll::ThumbMode::Zoom => "zoom",
        crate::scroll::ThumbMode::Tabs => "tabs",
        crate::scroll::ThumbMode::None => "none",
    });
    let edits = files.into_iter().map(|file| -> (PathBuf, Edit) {
        let here = file == target;
        (
            file,
            Box::new(move |doc: &mut toml_edit::DocumentMut| {
                let fields = vec![
                    ("mode", mode.map(toml_edit::value)),
                    ("speed", thumb.speed.map(rounded)),
                    ("tap", thumb.tap.as_ref().map(|a| toml_edit::value(a.to_string()))),
                ];
                set_fields(doc, profile, "thumb-wheel", here, fields)
            }),
        )
    });
    edit_files(path, &host, edits.collect())
}

/// Writes settings into one `[device."<section>"]` table, keeping the rest of
/// the file, including comments, as it was. The previous version is kept
/// next to it with a `.bak` suffix.
pub fn persist(path: &Path, section: &str, settings: &DeviceSettings) -> Result<(), ConfigError> {
    let _lock = lock(path)?;
    let text = read_optional(path)?.unwrap_or_default();
    let mut doc: toml_edit::DocumentMut =
        text.parse().map_err(|e| ConfigError::Edit { path: path.to_path_buf(), source: e })?;

    let shape = |msg: String| ConfigError::Shape { path: path.to_path_buf(), msg };
    let devices = doc.entry("device").or_insert_with(|| {
        let mut t = toml_edit::Table::new();
        t.set_implicit(true);
        toml_edit::Item::Table(t)
    });
    let table = devices
        .as_table_like_mut()
        .ok_or_else(|| shape("`device` must be a table".into()))?
        .entry(section)
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_like_mut()
        .ok_or_else(|| shape(format!("`device.\"{section}\"` must be a table")))?;

    use toml_edit::value;
    let mut put = |key: &str, item: toml_edit::Item| put(table, key, item);
    if let Some(v) = settings.dpi {
        put("dpi", value(v as i64));
    }
    if let Some(v) = settings.haptic {
        put("haptic", value(v as i64));
    }
    if let Some(v) = settings.force {
        put("force", value(v as i64));
    }
    if let Some(v) = settings.wheel {
        put("wheel", value(wheel_mode_name(v)));
    }
    if let Some(v) = settings.smartshift {
        let item = match v {
            crate::settings::Shift::Off => value("off"),
            crate::settings::Shift::Speed(s) => value(s as i64),
        };
        put("smartshift", item);
    }
    if let Some(v) = settings.torque {
        put("torque", value(v as i64));
    }
    if let Some(v) = settings.wheel_invert {
        put("wheel-invert", value(v));
    }
    if let Some(v) = settings.thumb_invert {
        put("thumb-invert", value(v));
    }

    write_with_backup(path, &text, &doc.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Shift;

    fn mx4() -> Identity {
        Identity { unit_id: "1A2B3C4D".into(), models: vec!["B042".into()], name: "MX Master 4".into() }
    }

    #[test]
    fn merges_by_specificity() {
        let c: Config = toml::from_str(
            r#"
            [device."*"]
            haptic = 60
            dpi = 800
            [device."B042"]
            dpi = 1600
            [device."1a2b3c4d"]
            smartshift = "off"
            [device."other"]
            dpi = 100
            "#,
        )
        .unwrap();
        let s = c.settings_for(&mx4());
        assert_eq!((s.haptic, s.dpi, s.smartshift), (Some(60), Some(1600), Some(Shift::Off)));
        assert_eq!(c.section_for(&mx4()), "1a2b3c4d");
    }

    #[test]
    fn settings_from_the_file_are_kept_in_range() {
        let c: Config = toml::from_str("[device.\"*\"]\nhaptic = 250\ntorque = 0\n").unwrap();
        let s = c.settings_for(&mx4());
        assert_eq!((s.haptic, s.torque), (Some(100), Some(1)));
        let c: Config = toml::from_str("[device.\"*\"]\ntorque = 200\n").unwrap();
        assert_eq!(c.settings_for(&mx4()).torque, Some(100));
    }

    #[test]
    fn new_section_uses_unit_id() {
        assert_eq!(Config::default().section_for(&mx4()), "1A2B3C4D");
        let no_unit = Identity { unit_id: String::new(), ..mx4() };
        assert_eq!(Config::default().section_for(&no_unit), "B042");
    }

    #[test]
    fn merges_includes_and_saves_where_sections_live() {
        let dir = std::env::temp_dir().join(format!("familiar-include-{}", std::process::id()));
        fs::create_dir_all(dir.join("hosts")).unwrap();
        let main = dir.join("config.toml");
        fs::write(
            &main,
            "include = [\"hosts/{hostname}.toml\", \"missing.toml\"]\n[device.\"B042\"]\ndpi = 800\nhaptic = 50\n",
        )
        .unwrap();
        fs::write(dir.join("hosts/laptop.toml"), "[device.\"B042\"]\ndpi = 1600\n[notifications]\nbattery-low = 30\n")
            .unwrap();

        let c = Config::load_with_host(&main, "laptop").unwrap();
        let s = c.settings_for(&mx4());
        assert_eq!((s.dpi, s.haptic), (Some(1600), Some(50)));
        assert_eq!(c.notifications.battery_low, 30);
        assert_eq!(c.included, vec![dir.join("hosts/laptop.toml")]);
        assert_eq!(c.file_for("B042", &main), dir.join("hosts/laptop.toml"));

        let other = Config::load_with_host(&main, "desktop").unwrap();
        assert_eq!(other.settings_for(&mx4()).dpi, Some(800));
        assert_eq!(other.file_for("B042", &main), main);
        assert_eq!(other.file_for("1A2B3C4D", &main), main);

        fs::write(dir.join("hosts/laptop.toml"), "include = [\"x\"]\n").unwrap();
        assert!(Config::load_with_host(&main, "laptop").is_err());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn profiles_inherit_and_check_names() {
        let dir = std::env::temp_dir().join(format!("familiar-profiles-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        fs::write(
            &path,
            r#"
            [profile.default.buttons]
            back = "key:alt+left"
            haptic = { tap = "key:super" }
            [profile.gimp]
            match = { app-id = "org.gimp.GIMP" }
            inherit = "default"
            [profile.gimp.buttons]
            back = "key:ctrl+z"
            "#,
        )
        .unwrap();
        let c = Config::load(&path).unwrap();
        let b = c.buttons("gimp").unwrap();
        assert_eq!(b["back"].clone(), "key:ctrl+z".parse().map(Binding::Direct).unwrap());
        assert!(b.contains_key("haptic"));
        assert_eq!(c.profile["gimp"].matches.as_ref().unwrap().app_id.as_deref(), Some("org.gimp.GIMP"));
        assert!(c.buttons("nothing").unwrap().is_empty());
        assert_eq!(c.wheels("gimp").unwrap(), Wheels::default());

        fs::write(
            &path,
            "[profile.default.buttons]
wheel = \"none\"\n",
        )
        .unwrap();
        assert!(matches!(Config::load(&path), Err(ConfigError::Shape { .. })));
        fs::write(&path, "[profile.a]\ninherit = \"b\"\n[profile.b]\ninherit = \"a\"\n").unwrap();
        assert!(matches!(Config::load(&path), Err(ConfigError::Shape { .. })));
        fs::write(&path, "[profile.a.buttons]\nback = { toggle = \"mouse:left\", tap = \"none\" }\n").unwrap();
        assert!(matches!(Config::load(&path), Err(ConfigError::Shape { .. })));
        fs::write(&path, "[profile.a]\ninherit = \"missing\"\n").unwrap();
        assert!(matches!(Config::load(&path), Err(ConfigError::Shape { .. })));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn wheels_merge_through_inheritance() {
        let c: Config = toml::from_str(
            r#"
            [profile.default.scroll]
            smooth = true
            speed = 1.5
            [profile.default.thumb-wheel]
            mode = "volume"
            [profile.video]
            inherit = "default"
            [profile.video.scroll]
            speed = 3
            [profile.video.thumb-wheel]
            tap = "key:playpause"
            "#,
        )
        .unwrap();
        let w = c.wheels("video").unwrap();
        assert!(w.scroll.smooth());
        assert_eq!(w.scroll.speed(), 3.0);
        let t = w.thumb.unwrap();
        assert_eq!(t.mode(), crate::scroll::ThumbMode::Volume);
        assert!(t.tap.is_some());
        assert!(toml::from_str::<Config>("[profile.a.thumb-wheel]\nmode = \"spin\"\n").is_err());
    }

    #[test]
    fn writes_buttons_scroll_and_thumb_wheel() {
        let dir = std::env::temp_dir().join(format!("familiar-gui-writes-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        fs::write(&path, "# keep me\n[device.\"B042\"]\ndpi = 1600\n").unwrap();

        let back: Binding = Binding::Direct("key:alt+left".parse().unwrap());
        persist_button(&path, "default", "back", Some(&back)).unwrap();
        let gestures = Binding::Gestures(Box::new(crate::actions::Gestures {
            tap: Some("key:super".parse().unwrap()),
            up: Some("key:volumeup".parse().unwrap()),
            ..Default::default()
        }));
        persist_button(&path, "default", "haptic", Some(&gestures)).unwrap();
        persist_scroll(&path, "default", &Scroll { smooth: Some(true), speed: Some(1.5) }).unwrap();
        let thumb = ThumbWheel { mode: Some(crate::scroll::ThumbMode::Volume), speed: None, tap: None };
        persist_thumb_wheel(&path, "default", Some(&thumb)).unwrap();

        let text = fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("# keep me"));
        let c = Config::load(&path).unwrap();
        let b = c.buttons("default").unwrap();
        assert_eq!(b["back"], back);
        assert_eq!(b["haptic"], gestures);
        let w = c.wheels("default").unwrap();
        assert!(w.scroll.smooth());
        assert_eq!(w.thumb.unwrap().mode(), crate::scroll::ThumbMode::Volume);
        assert_eq!(c.device["B042"].dpi, Some(1600));

        persist_button(&path, "default", "back", None).unwrap();
        persist_thumb_wheel(&path, "default", None).unwrap();
        let c = Config::load(&path).unwrap();
        assert!(!c.buttons("default").unwrap().contains_key("back"));
        assert!(c.wheels("default").unwrap().thumb.is_none());

        let bad = Binding::Gestures(Box::new(crate::actions::Gestures {
            toggle: Some("mouse:left".parse().unwrap()),
            tap: Some("none".parse().unwrap()),
            ..Default::default()
        }));
        assert!(persist_button(&path, "default", "back", Some(&bad)).is_err(), "invalid configs are not written");
        assert!(!fs::read_to_string(&path).unwrap().contains("toggle"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn profile_writes_follow_includes_and_inheritance() {
        let dir = std::env::temp_dir().join(format!("familiar-profile-includes-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let main = dir.join("config.toml");
        let extra = dir.join("extra.toml");
        fs::write(&main, "include = [\"extra.toml\"]\n[profile.default.buttons]\nback = \"mouse:back\"\n").unwrap();
        fs::write(
            &extra,
            "[profile.default.buttons]\nback = \"key:alt+left\"\n[profile.work.buttons]\nforward = \"none\"\n",
        )
        .unwrap();

        let next: Binding = Binding::Direct("key:ctrl+w".parse().unwrap());
        persist_button(&main, "default", "back", Some(&next)).unwrap();
        assert_eq!(Config::load(&main).unwrap().buttons("default").unwrap()["back"], next);
        assert!(fs::read_to_string(&extra).unwrap().contains("KEY_W"), "written where it takes effect");

        persist_button(&main, "default", "back", None).unwrap();
        assert!(!Config::load(&main).unwrap().buttons("default").unwrap().contains_key("back"));

        fs::write(&main, "include = [\"extra.toml\"]\n[profile.default]\ninherit = \"work\"\n").unwrap();
        persist_scroll(&main, "default", &Scroll { smooth: Some(true), speed: Some(2.0) }).unwrap();
        assert!(Config::load(&main).unwrap().wheels("default").unwrap().scroll.smooth());
        let err = persist_button(&main, "default", "forward", None).unwrap_err();
        assert!(err.to_string().contains("inherits"), "{err}");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn persist_keeps_a_backup() {
        let dir = std::env::temp_dir().join(format!("familiar-backup-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        fs::write(&path, "# before\n").unwrap();
        persist(&path, "B042", &DeviceSettings { dpi: Some(1200), ..Default::default() }).unwrap();
        assert_eq!(fs::read_to_string(backup_path(&path)).unwrap(), "# before\n");
        assert!(fs::read_to_string(&path).unwrap().contains("dpi = 1200"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn persist_keeps_the_file_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("familiar-mode-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        fs::write(&path, "# before\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        persist(&path, "B042", &DeviceSettings { dpi: Some(1200), ..Default::default() }).unwrap();
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(&backup_path(&path)), 0o600);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn persist_writes_through_a_symlink() {
        let dir = std::env::temp_dir().join(format!("familiar-symlink-{}", std::process::id()));
        fs::create_dir_all(dir.join("dotfiles")).unwrap();
        fs::create_dir_all(dir.join("config")).unwrap();
        let real = dir.join("dotfiles/familiar.toml");
        let link = dir.join("config/config.toml");
        fs::write(&real, "# before\n").unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();

        persist(&link, "B042", &DeviceSettings { dpi: Some(1200), ..Default::default() }).unwrap();
        let back: Binding = Binding::Direct("key:alt+left".parse().unwrap());
        persist_button(&link, "default", "back", Some(&back)).unwrap();

        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink(), "still a link");
        let text = fs::read_to_string(&real).unwrap();
        assert!(text.contains("dpi = 1200") && text.contains("KEY_LEFT"), "{text}");
        assert!(backup_path(&real).exists(), "the backup sits beside the real file");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rejects_newer_versions() {
        let dir = std::env::temp_dir().join(format!("familiar-version-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        fs::write(&path, "version = 2\n").unwrap();
        assert!(matches!(Config::load(&path), Err(ConfigError::Shape { .. })));
        fs::write(&path, "version = 1\n").unwrap();
        assert_eq!(Config::load(&path).unwrap().version, 1);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn persist_handles_inline_tables_and_bad_shapes() {
        let dir = std::env::temp_dir().join(format!("familiar-inline-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        fs::write(&path, "device = { \"B042\" = { dpi = 800 } }\n").unwrap();
        persist(&path, "B042", &DeviceSettings { haptic: Some(40), ..Default::default() }).unwrap();
        let s = Config::load(&path).unwrap().settings_for(&mx4());
        assert_eq!((s.dpi, s.haptic), (Some(800), Some(40)));
        fs::write(&path, "device = 3\n").unwrap();
        let r = persist(&path, "B042", &DeviceSettings { dpi: Some(1), ..Default::default() });
        assert!(matches!(r, Err(ConfigError::Shape { .. })));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn persist_keeps_comments() {
        let dir = std::env::temp_dir().join(format!("familiar-test-{}", std::process::id()));
        let path = dir.join("config.toml");
        fs::create_dir_all(&dir).unwrap();
        fs::write(&path, "# my mouse\n[notifications]\nbattery-low = 20\n").unwrap();
        persist(&path, "1A2B3C4D", &DeviceSettings { dpi: Some(1200), ..Default::default() }).unwrap();
        persist(&path, "1A2B3C4D", &DeviceSettings { smartshift: Some(Shift::Off), ..Default::default() }).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("# my mouse"));
        let c = Config::load(&path).unwrap();
        assert_eq!(c.notifications.battery_low, 20);
        let s = c.settings_for(&mx4());
        assert_eq!((s.dpi, s.smartshift), (Some(1200), Some(Shift::Off)));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn changing_a_value_keeps_its_comments() {
        let dir = std::env::temp_dir().join(format!("familiar-comments-{}", std::process::id()));
        let path = dir.join("config.toml");
        fs::create_dir_all(&dir).unwrap();
        let before = "\
[device.\"B042\"]
# pointer speed
dpi = 1600             # 200-8000 in steps of 50

[profile.default.buttons]
# the thumb button
back = \"key:alt+left\"  # goes back

[profile.default.scroll]
smooth = false   # hi-res
speed = 1.0      # times the mouse's own
";
        fs::write(&path, before).unwrap();

        persist(&path, "B042", &DeviceSettings { dpi: Some(1200), ..Default::default() }).unwrap();
        let next: Binding = Binding::Direct("mouse:middle".parse().unwrap());
        persist_button(&path, "default", "back", Some(&next)).unwrap();
        persist_scroll(&path, "default", &Scroll { smooth: Some(true), speed: Some(2.0) }).unwrap();

        let after = before
            .replace("dpi = 1600", "dpi = 1200")
            .replace("\"key:alt+left\"", "\"mouse:middle\"")
            .replace("smooth = false", "smooth = true")
            .replace("speed = 1.0", "speed = 2.0");
        assert_eq!(fs::read_to_string(&path).unwrap(), after);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_binding_written_as_a_table_can_be_replaced() {
        let dir = std::env::temp_dir().join(format!("familiar-subtable-{}", std::process::id()));
        let path = dir.join("config.toml");
        fs::create_dir_all(&dir).unwrap();
        fs::write(&path, "[profile.default.buttons.haptic]\ntap = \"key:super\"\n").unwrap();
        let next: Binding = Binding::Direct("mouse:middle".parse().unwrap());
        persist_button(&path, "default", "haptic", Some(&next)).unwrap();
        assert_eq!(Config::load(&path).unwrap().buttons("default").unwrap()["haptic"], next);
        fs::remove_dir_all(&dir).unwrap();
    }
}

#[cfg(test)]
mod example {
    use super::*;

    #[test]
    fn shipped_example_parses() {
        let c: Config = toml::from_str(include_str!("../../../packaging/config.example.toml")).unwrap();
        assert_eq!(c.device["B042"].dpi, Some(1600));
        assert_eq!(c.notifications.battery_low, 15);
    }
}

#[cfg(test)]
mod docs {
    use super::*;

    #[test]
    fn scrolling_doc_example_parses() {
        let doc = include_str!("../../../docs/scrolling.md");
        let start = doc.find("```toml\n").expect("a toml block") + 8;
        let end = start + doc[start..].find("```").expect("closed");
        let c: Config = toml::from_str(&doc[start..end]).unwrap();
        let w = c.wheels("default").unwrap();
        assert!(w.scroll.smooth());
        assert!(w.thumb.unwrap().tap.is_some());
    }

    /// The first TOML block in docs/buttons.md must be a valid config.
    #[test]
    fn buttons_doc_example_parses() {
        let doc = include_str!("../../../docs/buttons.md");
        let start = doc.find("```toml\n").expect("a toml block") + 8;
        let end = start + doc[start..].find("```").expect("closed");
        let c: Config = toml::from_str(&doc[start..end]).unwrap();
        c.check_profiles().unwrap();
        assert_eq!(c.buttons("default").unwrap().len(), 5);
    }
}
