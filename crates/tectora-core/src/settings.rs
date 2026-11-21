//! App-wide user settings shared by the desktop app, the CLI and the MCP
//! server, stored as JSON in the operating system's per-user config folder:
//!
//! - macOS: `~/Library/Application Support/Tectora/settings.json`
//! - Windows: `%APPDATA%\Tectora\settings.json`
//! - Linux: `$XDG_CONFIG_HOME/tectora/settings.json` (usually `~/.config`)
//!
//! `TECTORA_CONFIG_DIR` overrides the folder (tests, portable installs).
//! Unknown keys are kept on save, so an older build never drops settings
//! written by a newer one.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::edit::{MAX_SUMMARY_CHARS, capped};

pub const SETTINGS_FILE: &str = "settings.json";
/// Name of the drawings inbox inside the projects folder when none is set.
pub const DEFAULT_DRAWINGS_SUBDIR: &str = "drawings";

/// UI colour scheme.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

/// Values a new roof edge or standard roof starts with.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RoofDefaults {
    pub pitch_deg: f64,
    pub eave_height: f64,
    pub overhang: f64,
}

impl Default for RoofDefaults {
    fn default() -> Self {
        Self {
            pitch_deg: 40.0,
            eave_height: 3.0,
            overhang: 0.5,
        }
    }
}

impl RoofDefaults {
    /// The ranges every new wall accepts (`EdgeDef::problems`): a default
    /// the walls then refuse made every new job fail with an error about
    /// `defaults` (audit 2026-11-15 HUNTIO-007).
    pub const PITCH: (f64, f64) = (crate::model::MIN_PITCH_DEG, 89.0);
    pub const EAVE: (f64, f64) = (0.0, 1000.0);
    pub const OVERHANG: (f64, f64) = (0.0, 20.0);

    fn validate(&self) -> Result<(), SettingsError> {
        let ok = |v: f64, (lo, hi): (f64, f64)| v.is_finite() && (lo..=hi).contains(&v);
        if !ok(self.pitch_deg, Self::PITCH) {
            return Err(SettingsError::InvalidValue("pitch_deg"));
        }
        if !ok(self.eave_height, Self::EAVE) {
            return Err(SettingsError::InvalidValue("eave_height"));
        }
        if !ok(self.overhang, Self::OVERHANG) {
            return Err(SettingsError::InvalidValue("overhang"));
        }
        Ok(())
    }

    /// Each value outside its range replaced by the default, so that a
    /// settings file written before the ranges were tightened still loads
    /// and can be saved again.
    fn repaired(self) -> Self {
        let d = Self::default();
        let fix = |v: f64, (lo, hi): (f64, f64), dv: f64| {
            if v.is_finite() && (lo..=hi).contains(&v) {
                v
            } else {
                dv
            }
        };
        Self {
            pitch_deg: fix(self.pitch_deg, Self::PITCH, d.pitch_deg),
            eave_height: fix(self.eave_height, Self::EAVE, d.eave_height),
            overhang: fix(self.overhang, Self::OVERHANG, d.overhang),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Library folder; `None` = `Documents/Tectora`.
    pub projects_dir: Option<PathBuf>,
    /// Inbox for technical drawings; `None` = `<projects_dir>/drawings`.
    pub drawings_dir: Option<PathBuf>,
    /// UI language (`cs`, `sk`, `en`, `de`); `None` = system language.
    pub locale: Option<String>,
    pub theme: Theme,
    pub roof_defaults: RoofDefaults,
    /// Open the design an AI agent is working on.
    pub follow_ai: bool,
    /// A change of folders an AI client asked for, waiting for the user
    /// (review ARCH-052): an AI must not move the jobs folder or the
    /// drawings inbox on its own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub folder_request: Option<FolderRequest>,
    /// A file outside the jobs folder and the drawings inbox that an AI
    /// client asked to read, waiting for the user (review ARCH-002).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_request: Option<FileRequest>,
    /// Files outside those folders the user allowed AI clients to read.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub allowed_files: Vec<PathBuf>,
    /// Settings changes AI clients made, oldest first, at most
    /// [`MAX_AI_CHANGES`]; the app shows an open one for the user to accept
    /// or revert (audit 2026-11-21 UX-003).
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "lenient_changes"
    )]
    pub ai_changes: Vec<AiSettingsChange>,
    /// Keys this build does not know, kept as they are.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            projects_dir: None,
            drawings_dir: None,
            locale: None,
            theme: Theme::System,
            roof_defaults: RoofDefaults::default(),
            follow_ai: true,
            folder_request: None,
            file_request: None,
            allowed_files: Vec::new(),
            ai_changes: Vec::new(),
            extra: Map::new(),
        }
    }
}

pub const LOCALES: [&str; 4] = ["cs", "sk", "en", "de"];

/// Error codes; the UI translates them (`settings.err.<code>`).
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SettingsError {
    #[error("not_absolute")]
    NotAbsolute,
    #[error("not_a_folder")]
    NotAFolder,
    #[error("not_writable")]
    NotWritable,
    #[error("invalid_value:{0}")]
    InvalidValue(&'static str),
    #[error("io:{0}")]
    Io(String),
}

impl SettingsError {
    /// The refusal as an English sentence for AI clients (MCP
    /// `update_settings`), with the range a value must lie in (HUNTIO-007).
    pub fn explain(&self) -> String {
        let range = |name: &str, (lo, hi): (f64, f64), unit: &str| {
            format!("roof_defaults.{name} must be a number from {lo} to {hi}{unit}")
        };
        match self {
            Self::NotAbsolute => "the folder must be an absolute path".into(),
            Self::NotAFolder => "the path is not a folder".into(),
            Self::NotWritable => "the folder cannot be written to".into(),
            Self::InvalidValue("pitch_deg") => range("pitch_deg", RoofDefaults::PITCH, "°"),
            Self::InvalidValue("eave_height") => range("eave_height", RoofDefaults::EAVE, " m"),
            Self::InvalidValue("overhang") => range("overhang", RoofDefaults::OVERHANG, " m"),
            Self::InvalidValue("locale") => "locale must be cs, sk, en, de or null".into(),
            Self::InvalidValue("key") => "an unknown setting was given; see get_settings for the keys".into(),
            Self::InvalidValue("type") => {
                "a setting has the wrong type (folders and locale are strings, roof_defaults numbers, follow_ai true or false)".into()
            }
            Self::InvalidValue(what) => format!("invalid value ({what})"),
            Self::Io(e) => format!("the settings file could not be written: {e}"),
        }
    }

    /// Stable code without details, for UI translation.
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotAbsolute => "not_absolute",
            Self::NotAFolder => "not_a_folder",
            Self::NotWritable => "not_writable",
            Self::InvalidValue(_) => "invalid_value",
            Self::Io(_) => "io",
        }
    }
}

/// `TECTORA_CONFIG_DIR`, otherwise the OS config folder + `Tectora`.
pub fn config_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("TECTORA_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    let base = dirs::config_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    // Lowercase on Linux, as is usual under ~/.config.
    base.join(if cfg!(target_os = "linux") {
        "tectora"
    } else {
        "Tectora"
    })
}

pub fn settings_path() -> PathBuf {
    config_dir().join(SETTINGS_FILE)
}

/// `Documents/Tectora` (home folder when there is no Documents folder).
pub fn builtin_projects_dir() -> PathBuf {
    dirs::document_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Tectora")
}

/// Projects folder forced by `TECTORA_PROJECTS_DIR`, if set.
pub fn env_projects_dir() -> Option<PathBuf> {
    std::env::var_os("TECTORA_PROJECTS_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

impl Settings {
    /// Reads the settings file; a missing or unreadable file gives defaults
    /// (a broken file must never stop the app from starting).
    pub fn load() -> Self {
        Self::load_from(&settings_path())
    }

    pub fn load_from(path: &Path) -> Self {
        Self::load_checked_from(path).0
    }

    /// The settings and, when the file exists but cannot be read or parsed,
    /// why: the defaults then apply, and the app and the MCP server say so
    /// instead of silently using another jobs folder (ERR-005).
    pub fn load_checked() -> (Self, Option<String>) {
        Self::load_checked_from(&settings_path())
    }

    pub fn load_checked_from(path: &Path) -> (Self, Option<String>) {
        let text = match fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return (Self::default(), None);
            }
            Err(e) => return (Self::default(), Some(format!("{}: {e}", path.display()))),
        };
        match serde_json::from_str::<Self>(&text) {
            Ok(mut s) => {
                s.roof_defaults = s.roof_defaults.repaired();
                (s, None)
            }
            Err(e) => (Self::default(), Some(format!("{}: {e}", path.display()))),
        }
    }

    pub fn save(&self) -> Result<(), SettingsError> {
        self.save_to(&settings_path())
    }

    pub fn save_to(&self, path: &Path) -> Result<(), SettingsError> {
        self.validate()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| SettingsError::Io(e.to_string()))?;
        }
        let json =
            serde_json::to_string_pretty(self).map_err(|e| SettingsError::Io(e.to_string()))?;
        // A damaged file is kept beside the new one: it may hold the jobs
        // folder the user chose, which the defaults would otherwise replace
        // for good (ERR-005).
        if let (_, Some(_)) = Self::load_checked_from(path)
            && path.is_file()
        {
            let backup = backup_path(path);
            fs::copy(path, &backup).map_err(|e| SettingsError::Io(e.to_string()))?;
        }
        // Unique per process: the app and an MCP server saving at the same
        // time must not rename each other's half-written file.
        let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
        crate::store::write_synced(&tmp, json.as_bytes())
            .map_err(|e| SettingsError::Io(e.to_string()))?;
        fs::rename(&tmp, path).map_err(|e| SettingsError::Io(e.to_string()))?;
        crate::store::sync_parent(path);
        Ok(())
    }

    /// Checks values only; folders are checked by [`prepare_dir`] when they
    /// are chosen, so a missing network drive does not block other changes.
    pub fn validate(&self) -> Result<(), SettingsError> {
        for dir in [&self.projects_dir, &self.drawings_dir]
            .into_iter()
            .flatten()
        {
            if !dir.is_absolute() {
                return Err(SettingsError::NotAbsolute);
            }
        }
        if let Some(l) = &self.locale
            && !LOCALES.contains(&l.as_str())
        {
            return Err(SettingsError::InvalidValue("locale"));
        }
        self.roof_defaults.validate()
    }

    /// The library folder: `TECTORA_PROJECTS_DIR`, then the setting, then
    /// `Documents/Tectora`.
    pub fn projects_dir(&self) -> PathBuf {
        env_projects_dir()
            .or_else(|| self.projects_dir.clone())
            .unwrap_or_else(builtin_projects_dir)
    }

    pub fn drawings_dir(&self) -> PathBuf {
        self.drawings_dir
            .clone()
            .unwrap_or_else(|| self.projects_dir().join(DEFAULT_DRAWINGS_SUBDIR))
    }
}

/// Keys an update may change ([`Settings::patched`]).
/// Settings an AI client may only ask for ([`Settings::patched_by_ai`]).
pub const FOLDER_KEYS: [&str; 2] = ["projects_dir", "drawings_dir"];

/// Folders an AI client asked to switch to, with who asked and why.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FolderRequest {
    /// The `projects_dir` / `drawings_dir` values asked for.
    pub changes: Map<String, Value>,
    /// The AI client's name.
    pub by: String,
    /// Its one-sentence reason.
    pub summary: String,
}

/// A file outside the jobs folder and the drawings inbox an AI client
/// asked to read, with who asked and what for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileRequest {
    pub path: PathBuf,
    /// The AI client's name.
    pub by: String,
    /// What the file is wanted for: `read_drawing`, `attach` or `import`.
    pub purpose: String,
}

/// How the user answered an [`AiSettingsChange`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiChangeState {
    /// Applied, not reviewed yet.
    #[default]
    Open,
    Accepted,
    Reverted,
}

/// One settings change by an AI client (MCP `update_settings`): applied at
/// once, recorded with who, why and the values before and after so the
/// user can see it and put the old values back (UX-003). Folders are not
/// in it: those wait for the user as a [`FolderRequest`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiSettingsChange {
    /// Unique in the history: the time in ms, bumped past the last one.
    pub id: u64,
    /// The AI client's name.
    pub by: String,
    /// Its one-sentence summary.
    pub summary: String,
    /// The changed values before, by key ([`AI_CHANGE_KEYS`]).
    pub before: Map<String, Value>,
    /// The same keys after the change.
    pub after: Map<String, Value>,
    #[serde(default)]
    pub state: AiChangeState,
}

/// Most entries kept in [`Settings::ai_changes`]; the oldest go first.
pub const MAX_AI_CHANGES: usize = 20;

/// The settings an [`AiSettingsChange`] records, as `key` or
/// `roof_defaults.<key>`, sorted.
pub const AI_CHANGE_KEYS: [&str; 6] = [
    "follow_ai",
    "locale",
    "roof_defaults.eave_height",
    "roof_defaults.overhang",
    "roof_defaults.pitch_deg",
    "theme",
];

/// The value of an [`AI_CHANGE_KEYS`] key in serialized settings.
fn key_value(doc: &Value, key: &str) -> Value {
    key.split('.')
        .try_fold(doc, |v, k| v.get(k))
        .cloned()
        .unwrap_or(Value::Null)
}

/// `ai_changes` from a file: an entry this build cannot read is dropped
/// instead of making the whole settings file unreadable.
fn lenient_changes<'de, D>(d: D) -> Result<Vec<AiSettingsChange>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = Value::deserialize(d)?;
    let Value::Array(items) = raw else {
        return Ok(Vec::new());
    };
    let mut out: Vec<AiSettingsChange> = items
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect();
    let extra = out.len().saturating_sub(MAX_AI_CHANGES);
    out.drain(..extra);
    Ok(out)
}

/// Most files kept in [`Settings::allowed_files`]; the oldest go first.
pub const MAX_ALLOWED_FILES: usize = 200;

/// `path` without `.` and `..` and with the links of its existing part
/// resolved, so a path cannot leave a folder through `..` or a link. Each
/// step is resolved before the next `..` is applied, so `inbox/link/..` is
/// the folder beside the link's target, not `inbox` (ARCH-002). A part that
/// does not exist yet is appended as written: a file that is not there yet
/// still compares with its resolved folder (on Windows a resolved path has
/// another prefix, on macOS the temporary folder is itself a link).
/// A network path (`\\host\share\…`, device paths) is never touched on
/// disk, only cleaned as text: resolving it would make the computer contact
/// that host before anyone allowed it (ARCH-006).
fn resolved(path: &Path) -> PathBuf {
    use std::path::Component;
    let offline = is_network_path(path);
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            Component::Prefix(_) => out.push(c),
            other => {
                out.push(other);
                if !offline && let Ok(real) = fs::canonicalize(&out) {
                    out = real;
                }
            }
        }
    }
    out
}

/// True for a Windows network or device path (`\\host\share`,
/// `\\?\UNC\…`, `\\.\…`, `\\?\…` other than a drive): opening it
/// reaches another computer or a device, so it is only compared as text.
pub fn is_network_path(path: &Path) -> bool {
    use std::path::{Component, Prefix};
    match path.components().next() {
        Some(Component::Prefix(p)) => {
            !matches!(p.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_))
        }
        _ => false,
    }
}

pub const EDITABLE_KEYS: [&str; 6] = [
    "projects_dir",
    "drawings_dir",
    "locale",
    "theme",
    "roof_defaults",
    "follow_ai",
];

impl Settings {
    /// A copy with the keys in `patch` replaced (JSON object; `null` resets a
    /// folder or the language to its default). `roof_defaults` may be given
    /// partially. Chosen folders are created and checked.
    pub fn patched(&self, patch: &Value) -> Result<Settings, SettingsError> {
        self.patched_with(patch, true)
    }

    fn patched_with(&self, patch: &Value, prepare: bool) -> Result<Settings, SettingsError> {
        let patch = patch
            .as_object()
            .ok_or(SettingsError::InvalidValue("patch"))?;
        let mut doc = serde_json::to_value(self).map_err(|e| SettingsError::Io(e.to_string()))?;
        for (key, value) in patch {
            let Some(key) = EDITABLE_KEYS.iter().find(|k| **k == key.as_str()) else {
                return Err(SettingsError::InvalidValue("key"));
            };
            match (*key, value) {
                ("roof_defaults", Value::Object(part)) => {
                    let target = doc[*key].as_object_mut().expect("serialized struct");
                    for (k, v) in part {
                        if !target.contains_key(k) {
                            return Err(SettingsError::InvalidValue("key"));
                        }
                        // `null` counts as left out, as in every MCP tool
                        // (HUNT-108).
                        if !v.is_null() {
                            target.insert(k.clone(), v.clone());
                        }
                    }
                }
                ("projects_dir" | "drawings_dir" | "locale", Value::String(s))
                    if s.trim().is_empty() =>
                {
                    doc[*key] = Value::Null;
                }
                _ => doc[*key] = value.clone(),
            }
        }
        let next: Settings =
            serde_json::from_value(doc).map_err(|_| SettingsError::InvalidValue("type"))?;
        next.validate()?;
        for key in ["projects_dir", "drawings_dir"] {
            if prepare && patch.contains_key(key) {
                let dir = if key == "projects_dir" {
                    next.projects_dir()
                } else {
                    next.drawings_dir()
                };
                prepare_dir(&dir)?;
            }
        }
        Ok(next)
    }

    /// Settings plus the folders actually in use, for the UI and AI agents.
    /// `effective.settings_error` says why the settings file could not be
    /// read, when it could not (the defaults are in use then).
    /// Applies `patch` from an AI client: every key but the folders at once,
    /// the folders (`projects_dir`, `drawings_dir`) only as a
    /// [`FolderRequest`] the user answers in the app. Returns the settings
    /// and whether a request was made.
    pub fn patched_by_ai(
        &self,
        patch: &Value,
        by: &str,
        summary: &str,
    ) -> Result<(Settings, bool), SettingsError> {
        let obj = patch
            .as_object()
            .ok_or(SettingsError::InvalidValue("patch"))?;
        let (folders, rest): (Map<String, Value>, Map<String, Value>) = obj
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .partition(|(k, _)| FOLDER_KEYS.contains(&k.as_str()));
        let mut next = self.patched(&Value::Object(rest))?;
        // Only a real change asks the user. Asking creates no folder: that
        // waits for the user's yes.
        let moved = self.patched_with(&Value::Object(folders.clone()), false)?;
        let asks = moved.projects_dir() != self.projects_dir()
            || moved.drawings_dir() != self.drawings_dir();
        if asks {
            next.folder_request = Some(FolderRequest {
                changes: folders,
                by: by.to_string(),
                summary: summary.to_string(),
            });
        }
        next.record_ai_change(self, by, summary)?;
        Ok((next, asks))
    }

    /// Adds what changed from `old` to `self` (outside the folders) to
    /// [`Settings::ai_changes`] as an open change by `by`; nothing when
    /// nothing changed.
    fn record_ai_change(
        &mut self,
        old: &Settings,
        by: &str,
        summary: &str,
    ) -> Result<(), SettingsError> {
        let io = |e: serde_json::Error| SettingsError::Io(e.to_string());
        let (was, now) = (
            serde_json::to_value(old).map_err(io)?,
            serde_json::to_value(&*self).map_err(io)?,
        );
        let (mut before, mut after) = (Map::new(), Map::new());
        for key in AI_CHANGE_KEYS {
            let (b, a) = (key_value(&was, key), key_value(&now, key));
            if b != a {
                before.insert(key.to_string(), b);
                after.insert(key.to_string(), a);
            }
        }
        if after.is_empty() {
            return Ok(());
        }
        let last = self.ai_changes.iter().map(|c| c.id).max().unwrap_or(0);
        let id = crate::store::now_ms().max(last.saturating_add(1));
        self.ai_changes.push(AiSettingsChange {
            id,
            by: by.to_string(),
            summary: capped(summary, MAX_SUMMARY_CHARS),
            before,
            after,
            state: AiChangeState::Open,
        });
        self.bound_ai_changes();
        Ok(())
    }

    /// Keeps [`Settings::ai_changes`] at [`MAX_AI_CHANGES`] without losing
    /// a change the user has not answered (ARCH-104): answered ones go
    /// first, oldest first; when all are open, the two oldest become one
    /// change (the first one's values before, the later one's after), so
    /// Revert still puts back what the AI found.
    fn bound_ai_changes(&mut self) {
        while self.ai_changes.len() > MAX_AI_CHANGES {
            if let Some(at) = self
                .ai_changes
                .iter()
                .position(|c| c.state != AiChangeState::Open)
            {
                self.ai_changes.remove(at);
                continue;
            }
            let older = self.ai_changes.remove(0);
            let newer = &mut self.ai_changes[0];
            for (key, value) in older.before {
                newer.before.insert(key, value);
            }
            for (key, value) in older.after {
                newer.after.entry(key).or_insert(value);
            }
            if older.by != newer.by {
                newer.by = capped(&format!("{}, {}", older.by, newer.by), MAX_SUMMARY_CHARS);
            }
            newer.summary = capped(
                &format!("{} / {}", older.summary, newer.summary),
                MAX_SUMMARY_CHARS,
            );
        }
    }

    /// The newest AI settings change the user has not answered yet.
    pub fn open_ai_change(&self) -> Option<&AiSettingsChange> {
        self.ai_changes
            .iter()
            .rev()
            .find(|c| c.state == AiChangeState::Open)
    }

    /// The user's answer to the open AI settings change `id`: `revert` puts
    /// back the values it changed (a value changed again since, by the user
    /// or another client, stays as it is now), otherwise it is accepted.
    /// Either way it is closed. A change that is not open is left alone.
    pub fn answer_ai_change(&self, id: u64, revert: bool) -> Result<Settings, SettingsError> {
        let mut next = self.clone();
        let Some(at) = next
            .ai_changes
            .iter()
            .position(|c| c.id == id && c.state == AiChangeState::Open)
        else {
            return Ok(next);
        };
        if revert {
            let now = serde_json::to_value(&next).map_err(|e| SettingsError::Io(e.to_string()))?;
            let change = &next.ai_changes[at];
            let mut patch = Map::new();
            let mut roof = Map::new();
            for (key, after) in &change.after {
                if !AI_CHANGE_KEYS.contains(&key.as_str()) || &key_value(&now, key) != after {
                    continue;
                }
                let old = change.before.get(key).cloned().unwrap_or(Value::Null);
                match key.strip_prefix("roof_defaults.") {
                    Some(k) => {
                        roof.insert(k.to_string(), old);
                    }
                    None => {
                        patch.insert(key.clone(), old);
                    }
                }
            }
            if !roof.is_empty() {
                patch.insert("roof_defaults".into(), Value::Object(roof));
            }
            next = next.patched_with(&Value::Object(patch), false)?;
        }
        next.ai_changes[at].state = if revert {
            AiChangeState::Reverted
        } else {
            AiChangeState::Accepted
        };
        Ok(next)
    }

    /// The user's answer to [`Settings::folder_request`]: `allow` applies it,
    /// either way it is closed. `shown` is the `changes` of the request the
    /// user was shown; when the pending request is another one by now (an AI
    /// client replaced it), nothing is applied and the new request stays for
    /// the user to read (ARCH-005).
    pub fn answer_folder_request(
        &self,
        shown: &Map<String, Value>,
        allow: bool,
    ) -> Result<Settings, SettingsError> {
        let mut next = self.clone();
        if next
            .folder_request
            .as_ref()
            .is_none_or(|r| &r.changes != shown)
        {
            return Ok(next);
        }
        let Some(req) = next.folder_request.take() else {
            return Ok(next);
        };
        if allow {
            next = next.patched(&Value::Object(req.changes))?;
        }
        Ok(next)
    }

    /// Whether an AI client may read the file `path`: it lies in the jobs
    /// folder (`library`, the one the MCP server works on, or the one in the
    /// settings), in the drawings inbox, or the user allowed this file
    /// (ARCH-002). A path outside gets the same answer whether the file
    /// exists or not.
    pub fn ai_may_read(&self, path: &Path, library: &Path) -> bool {
        if !path.is_absolute() {
            return false;
        }
        let file = resolved(path);
        let roots = [
            library.to_path_buf(),
            self.projects_dir(),
            self.drawings_dir(),
        ];
        roots.iter().any(|r| file.starts_with(resolved(r)))
            || self.allowed_files.iter().any(|a| resolved(a) == file)
    }

    /// A copy that asks the user to allow `path` for `by`
    /// ([`Settings::file_request`]); it replaces an older open request.
    pub fn asking_for_file(&self, path: &Path, by: &str, purpose: &str) -> Settings {
        let mut next = self.clone();
        next.file_request = Some(FileRequest {
            path: resolved(path),
            by: by.to_string(),
            purpose: purpose.to_string(),
        });
        next
    }

    /// The user's answer to [`Settings::file_request`]: `allow` adds the
    /// file to [`Settings::allowed_files`], either way the request is closed.
    /// `shown` is the path of the request the user was shown; when the
    /// pending request names another path by now (an AI client replaced it),
    /// nothing is allowed and the new request stays for the user to read
    /// (ARCH-005).
    pub fn answer_file_request(&self, shown: &Path, allow: bool) -> Settings {
        let mut next = self.clone();
        if next.file_request.as_ref().is_none_or(|r| r.path != shown) {
            return next;
        }
        let Some(req) = next.file_request.take() else {
            return next;
        };
        if allow && !next.allowed_files.contains(&req.path) {
            next.allowed_files.push(req.path);
            let extra = next.allowed_files.len().saturating_sub(MAX_ALLOWED_FILES);
            next.allowed_files.drain(..extra);
        }
        next
    }

    pub fn describe(&self) -> Value {
        serde_json::json!({
            "settings": self,
            "effective": {
                "projects_dir": self.projects_dir(),
                "drawings_dir": self.drawings_dir(),
                "projects_dir_from_env": env_projects_dir().is_some(),
                "default_projects_dir": builtin_projects_dir(),
                "settings_file": settings_path(),
                "settings_error": Self::load_checked().1,
            }
        })
    }
}

/// A free name beside `path` for a damaged settings file:
/// `settings.json.bad`, then `settings.json.bad-<ms>`.
fn backup_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| SETTINGS_FILE.into());
    let first = path.with_file_name(format!("{name}.bad"));
    if !first.exists() {
        return first;
    }
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    path.with_file_name(format!("{name}.bad-{ms}"))
}

/// Creates `dir` if needed and checks that it is a folder we can write to.
pub fn prepare_dir(dir: &Path) -> Result<(), SettingsError> {
    if !dir.is_absolute() {
        return Err(SettingsError::NotAbsolute);
    }
    if dir.exists() && !dir.is_dir() {
        return Err(SettingsError::NotAFolder);
    }
    fs::create_dir_all(dir).map_err(|_| SettingsError::NotWritable)?;
    let probe = dir.join(".tectora-write-test");
    fs::write(&probe, b"").map_err(|_| SettingsError::NotWritable)?;
    let _ = fs::remove_file(&probe);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ai_reads_outside_the_job_folders_only_what_the_user_allowed() {
        // ARCH-002: MCP `path` arguments reached any file on the computer.
        let base = tmp("ai-file");
        let (library, inbox, other) = (base.join("jobs"), base.join("inbox"), base.join("other"));
        for d in [&library, &inbox, &other] {
            fs::create_dir_all(d).unwrap();
        }
        let s = Settings {
            drawings_dir: Some(inbox.clone()),
            ..Settings::default()
        };
        let secret = other.join("contract.pdf");
        fs::write(&secret, b"x").unwrap();
        assert!(s.ai_may_read(&library.join("files/a/plan.pdf"), &library));
        assert!(s.ai_may_read(&inbox.join("plan.pdf"), &library));
        assert!(!s.ai_may_read(&secret, &library));
        // The same answer for a file that is not there, `..` out of the
        // inbox and a relative path.
        assert!(!s.ai_may_read(&other.join("missing.pdf"), &library));
        assert!(!s.ai_may_read(&inbox.join("../other/contract.pdf"), &library));
        assert!(!s.ai_may_read(Path::new("contract.pdf"), &library));
        #[cfg(unix)]
        {
            // A link in the inbox does not carry a file in from outside.
            std::os::unix::fs::symlink(&secret, inbox.join("link.pdf")).unwrap();
            assert!(!s.ai_may_read(&inbox.join("link.pdf"), &library));
            // A jobs folder reached through a link (the temporary folder
            // on macOS, a resolved prefix on Windows): a file that is not
            // there yet is still inside it.
            let via = base.join("jobs-link");
            std::os::unix::fs::symlink(&library, &via).unwrap();
            assert!(s.ai_may_read(&via.join("files/a/plan.pdf"), &via));
            assert!(!s.ai_may_read(&via.join("../other/missing.pdf"), &via));
            // ARCH-002 (2026-11-15): a folder link in the inbox and then
            // `..` lands beside the link's target, not back in the inbox.
            let shared = base.join("shared");
            fs::create_dir_all(shared.join("drawings")).unwrap();
            fs::write(shared.join("private.pdf"), b"x").unwrap();
            std::os::unix::fs::symlink(shared.join("drawings"), inbox.join("shared")).unwrap();
            // Lexically this is `inbox/private.pdf`; through the link it is
            // `shared/private.pdf`, outside.
            fs::write(inbox.join("private.pdf"), b"x").unwrap();
            assert!(!s.ai_may_read(&inbox.join("shared/../private.pdf"), &library));
            assert!(!s.ai_may_read(&inbox.join("shared/./../private.pdf"), &library));
        }
        let asked = s.asking_for_file(&secret, "Claude", "attach");
        let req = asked.file_request.clone().unwrap();
        assert_eq!(
            (req.by.as_str(), req.purpose.as_str()),
            ("Claude", "attach")
        );
        assert!(
            !asked.ai_may_read(&secret, &library),
            "asking allows nothing"
        );
        let kept = asked.answer_file_request(&req.path, false);
        assert!(kept.file_request.is_none() && !kept.ai_may_read(&secret, &library));
        let allowed = asked.answer_file_request(&req.path, true);
        assert!(allowed.file_request.is_none() && allowed.ai_may_read(&secret, &library));
        assert!(
            !allowed.ai_may_read(&other.join("second.pdf"), &library),
            "only that file"
        );
        // An AI client cannot allow a file for itself through the settings.
        let patch = serde_json::json!({ "allowed_files": [other.join("second.pdf")] });
        assert!(s.patched_by_ai(&patch, "Claude", "x").is_err());
        // The list survives a save and a later change of another setting.
        let again: Settings =
            serde_json::from_str(&serde_json::to_string(&allowed).unwrap()).unwrap();
        let again = again
            .patched(&serde_json::json!({ "follow_ai": false }))
            .unwrap();
        assert!(again.ai_may_read(&secret, &library));
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn a_network_path_is_compared_as_text_and_never_opened() {
        // ARCH-006: `\\host\share\x.pdf` was resolved (the computer
        // contacted the host) before it was refused.
        assert!(!is_network_path(Path::new("/home/a/plan.pdf")));
        assert!(!is_network_path(Path::new("plan.pdf")));
        #[cfg(windows)]
        {
            for p in [
                r"\\host\share\x.pdf",
                r"\\?\UNC\host\share\x.pdf",
                r"\\.\pipe\x",
            ] {
                assert!(is_network_path(Path::new(p)), "{p}");
            }
            assert!(!is_network_path(Path::new(r"C:\Users\a\x.pdf")));
            assert!(!is_network_path(Path::new(r"\\?\C:\Users\a\x.pdf")));
            let library = tmp("ai-unc").join("jobs");
            let s = Settings::default();
            let far = Path::new(r"\\host\share\a\..\x.pdf");
            assert!(!s.ai_may_read(far, &library));
            let req = s
                .asking_for_file(far, "Claude", "attach")
                .file_request
                .unwrap();
            assert_eq!(req.path, PathBuf::from(r"\\host\share\x.pdf"));
        }
    }

    #[test]
    fn an_answer_only_applies_to_the_request_the_user_was_shown() {
        // ARCH-005: the AI replaced the request before the click.
        let base = tmp("swap");
        let (a, b) = (base.join("a.pdf"), base.join("b.pdf"));
        let asked_b = Settings::default().asking_for_file(&b, "Claude", "attach");
        let shown_a = resolved(&a);
        let after = asked_b.answer_file_request(&shown_a, true);
        assert!(after.allowed_files.is_empty(), "b was not allowed");
        assert_eq!(after.file_request, asked_b.file_request, "b still waits");
        // No request open: nothing to allow either.
        let none = Settings::default().answer_file_request(&shown_a, true);
        assert!(none.allowed_files.is_empty());
        // The same for the folders.
        let patch = |d: &str| serde_json::json!({ "projects_dir": base.join(d) });
        let (first, _) = Settings::default()
            .patched_by_ai(&patch("one"), "Claude", "x")
            .unwrap();
        let (second, _) = first.patched_by_ai(&patch("two"), "Claude", "y").unwrap();
        let shown = first.folder_request.clone().unwrap().changes;
        let kept = second.answer_folder_request(&shown, true).unwrap();
        assert_eq!(kept.projects_dir(), second.projects_dir(), "not moved");
        assert!(!base.join("two").exists() && !base.join("one").exists());
        assert_eq!(
            kept.folder_request, second.folder_request,
            "two still waits"
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn an_ai_only_asks_to_move_the_folders_and_the_user_decides() {
        // ARCH-052: MCP update_settings moved the jobs folder at once.
        let s = Settings::default();
        let target = tmp("ai-folder").join("shared");
        let patch = serde_json::json!({ "projects_dir": target, "follow_ai": false });
        let (next, asked) = s
            .patched_by_ai(&patch, "Claude", "Use the shared folder")
            .unwrap();
        assert!(asked);
        assert_eq!(next.projects_dir(), s.projects_dir(), "folder unchanged");
        assert!(!next.follow_ai, "other keys apply at once");
        assert!(!target.exists(), "asking creates no folder");
        let req = next.folder_request.clone().unwrap();
        assert_eq!(
            (req.by.as_str(), req.summary.as_str()),
            ("Claude", "Use the shared folder")
        );
        let kept = next.answer_folder_request(&req.changes, false).unwrap();
        assert_eq!(kept.projects_dir(), s.projects_dir());
        assert!(kept.folder_request.is_none());
        let moved = next.answer_folder_request(&req.changes, true).unwrap();
        assert_eq!(moved.projects_dir(), target);
        assert!(target.is_dir(), "the allowed folder is created");
        assert!(moved.folder_request.is_none());
        // Setting the folder it already is asks nothing.
        let same = serde_json::json!({ "projects_dir": s.projects_dir() });
        assert!(!s.patched_by_ai(&same, "Claude", "x").unwrap().1);
    }

    #[test]
    fn a_damaged_ai_change_drops_only_itself_from_the_history() {
        // TEST-106: one unreadable entry between two good ones keeps both
        // good ones and every other setting of the file.
        let good = |id: u64| {
            serde_json::json!({
                "id": id, "by": "Claude", "summary": "x",
                "before": { "roof_defaults.pitch_deg": 40 },
                "after": { "roof_defaults.pitch_deg": 25 },
            })
        };
        let doc = serde_json::json!({
            "locale": "de",
            "ai_changes": [good(1), { "id": "x", "by": 3 }, good(2)],
        });
        let d = tmp("ai-change-damaged");
        let path = d.join(SETTINGS_FILE);
        fs::write(&path, doc.to_string()).unwrap();
        let s = Settings::load_from(&path);
        assert_eq!(s.locale.as_deref(), Some("de"), "the file was read");
        let ids: Vec<u64> = s.ai_changes.iter().map(|c| c.id).collect();
        assert_eq!(ids, [1, 2]);
        assert_eq!(s.ai_changes[1].state, AiChangeState::Open);
    }

    #[test]
    fn an_ai_settings_change_is_recorded_and_the_user_can_revert_it() {
        // UX-003: MCP update_settings changed the roof defaults and the
        // language with no record, no before -> after and no Revert.
        let s = Settings::default();
        let patch = serde_json::json!({
            "roof_defaults": { "pitch_deg": 25, "overhang": 0.9 },
            "locale": "de",
        });
        let (next, _) = s
            .patched_by_ai(&patch, "Claude", "Company defaults")
            .unwrap();
        assert_eq!(next.roof_defaults.pitch_deg, 25.0, "applied at once");
        let change = next.open_ai_change().cloned().expect("recorded");
        assert_eq!(
            (change.by.as_str(), change.summary.as_str()),
            ("Claude", "Company defaults")
        );
        assert_eq!(change.state, AiChangeState::Open);
        let keys: Vec<&str> = change.after.keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            [
                "locale",
                "roof_defaults.overhang",
                "roof_defaults.pitch_deg"
            ],
            "only what changed"
        );
        assert_eq!(change.before["roof_defaults.pitch_deg"], 40.0);
        assert_eq!(change.after["roof_defaults.pitch_deg"], 25.0);
        assert_eq!(change.before["locale"], Value::Null);
        assert_eq!(change.after["locale"], "de");
        // The record survives a save.
        let d = tmp("ai-change");
        let path = d.join(SETTINGS_FILE);
        next.save_to(&path).unwrap();
        let next = Settings::load_from(&path);
        assert_eq!(next.open_ai_change(), Some(&change));
        // The user changes the overhang himself before reviewing: Revert
        // puts back what the AI changed and keeps the user's own value.
        let mut mine = next
            .patched(&serde_json::json!({ "roof_defaults": { "overhang": 0.7 } }))
            .unwrap();
        mine.save_to(&path).unwrap();
        mine = Settings::load_from(&path);
        let reverted = mine.answer_ai_change(change.id, true).unwrap();
        assert_eq!(reverted.roof_defaults.pitch_deg, 40.0);
        assert_eq!(
            reverted.roof_defaults.overhang, 0.7,
            "the user's value stays"
        );
        assert_eq!(reverted.locale, None);
        assert!(reverted.open_ai_change().is_none());
        assert_eq!(reverted.ai_changes[0].state, AiChangeState::Reverted);
        // Answering again, or a change that is not there, does nothing.
        assert_eq!(
            reverted.answer_ai_change(change.id, false).unwrap(),
            reverted
        );
        assert_eq!(
            reverted.answer_ai_change(change.id + 99, true).unwrap(),
            reverted
        );
        // Accept keeps the values and closes it.
        let accepted = next.answer_ai_change(change.id, false).unwrap();
        assert_eq!(accepted.roof_defaults.pitch_deg, 25.0);
        assert_eq!(accepted.ai_changes[0].state, AiChangeState::Accepted);
        // A call that changes nothing records nothing; a folder request is
        // asked as before and not recorded as a change.
        let same = serde_json::json!({ "roof_defaults": { "pitch_deg": 40 } });
        assert!(
            s.patched_by_ai(&same, "Claude", "x")
                .unwrap()
                .0
                .ai_changes
                .is_empty()
        );
        let folder = serde_json::json!({ "projects_dir": d.join("shared") });
        let (asked, yes) = s.patched_by_ai(&folder, "Claude", "x").unwrap();
        assert!(yes && asked.ai_changes.is_empty() && asked.folder_request.is_some());
        // The history is bounded and the ids stay unique.
        let mut many = s.clone();
        for i in 0..(MAX_AI_CHANGES + 5) {
            let p = serde_json::json!({ "roof_defaults": { "pitch_deg": 20 + (i % 2) } });
            many = many.patched_by_ai(&p, "Claude", "x").unwrap().0;
        }
        assert_eq!(many.ai_changes.len(), MAX_AI_CHANGES);
        let ids: std::collections::BTreeSet<u64> = many.ai_changes.iter().map(|c| c.id).collect();
        assert_eq!(ids.len(), MAX_AI_CHANGES);
        // An AI client cannot write the history itself.
        let forged = serde_json::json!({ "ai_changes": [] });
        assert!(s.patched_by_ai(&forged, "Claude", "x").is_err());
        let _ = fs::remove_dir_all(&d);
    }

    /// Audit 2026-11-21 ARCH-104, ARCH-105: 20 later changes cannot push
    /// an unanswered AI change out of the history, and a summary is stored
    /// at most [`MAX_SUMMARY_CHARS`] long.
    #[test]
    fn an_open_ai_change_is_never_dropped_and_its_summary_is_capped() {
        let s = Settings::default();
        let pitch = serde_json::json!({ "roof_defaults": { "pitch_deg": 22 } });
        let mut s = s
            .patched_by_ai(&pitch, "Claude", "Set the default pitch")
            .unwrap()
            .0;
        for i in 0..MAX_AI_CHANGES {
            let theme = if i % 2 == 0 { "dark" } else { "light" };
            let p = serde_json::json!({ "theme": theme });
            s = s.patched_by_ai(&p, "Claude", &"x".repeat(5000)).unwrap().0;
        }
        assert!(s.ai_changes.len() <= MAX_AI_CHANGES);
        let open: Vec<&AiSettingsChange> = s
            .ai_changes
            .iter()
            .filter(|c| c.state == AiChangeState::Open)
            .collect();
        let pitch_change = open
            .iter()
            .find(|c| c.after.contains_key("roof_defaults.pitch_deg"))
            .expect("the pitch change is still there to review");
        assert!(pitch_change.summary.contains("Set the default pitch"));
        assert!(
            s.ai_changes
                .iter()
                .all(|c| c.summary.chars().count() <= MAX_SUMMARY_CHARS)
        );
        // Reverting it puts the pitch back.
        let back = s.answer_ai_change(pitch_change.id, true).unwrap();
        assert_eq!(
            back.roof_defaults.pitch_deg,
            Settings::default().roof_defaults.pitch_deg
        );
    }

    /// Audit 2026-11-21 HUNT-108 item 4: a roof default sent as `null` is
    /// left as it is, not refused as the wrong type.
    #[test]
    fn a_null_roof_default_is_left_out() {
        let s = Settings::default();
        let p = serde_json::json!({ "roof_defaults": { "pitch_deg": null, "overhang": 0.8 } });
        let next = s.patched(&p).unwrap();
        assert_eq!(next.roof_defaults.pitch_deg, s.roof_defaults.pitch_deg);
        assert_eq!(next.roof_defaults.overhang, 0.8);
    }

    fn tmp(name: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("tectora-settings-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn an_old_file_with_defaults_walls_refuse_loads_repaired() {
        // HUNTIO-007: older releases saved a pitch of 1° or a negative
        // eave; such a file must load and save again.
        let d = tmp("repaired");
        let path = d.join(SETTINGS_FILE);
        let text = r#"{ "locale": "de", "roof_defaults": { "pitch_deg": 1, "eave_height": -5, "overhang": 0.8 } }"#;
        fs::write(&path, text).unwrap();
        let (s, problem) = Settings::load_checked_from(&path);
        assert!(problem.is_none());
        assert_eq!(s.locale.as_deref(), Some("de"));
        assert_eq!(s.roof_defaults.pitch_deg, 40.0);
        assert_eq!(s.roof_defaults.eave_height, 3.0);
        assert_eq!(s.roof_defaults.overhang, 0.8);
        assert!(s.save_to(&path).is_ok());
    }

    #[test]
    fn missing_or_broken_file_gives_defaults() {
        let d = tmp("broken");
        assert_eq!(
            Settings::load_from(&d.join("none.json")),
            Settings::default()
        );
        fs::write(d.join("bad.json"), "{ not json").unwrap();
        assert_eq!(
            Settings::load_from(&d.join("bad.json")),
            Settings::default()
        );
    }

    #[test]
    fn a_damaged_file_is_reported_and_kept_before_it_is_replaced() {
        // ERR-005: a damaged file silently gave defaults, and the next
        // settings write made the loss of the jobs folder permanent.
        let d = tmp("damaged");
        let path = d.join(SETTINGS_FILE);
        let text = r#"{ "locale": "de", "projects_dir": "/srv/zakazky", }"#;
        fs::write(&path, text).unwrap();
        let (s, problem) = Settings::load_checked_from(&path);
        assert_eq!(s, Settings::default());
        assert!(problem.is_some_and(|p| p.contains("trailing comma")));
        Settings::default().save_to(&path).unwrap();
        let backups: Vec<String> = fs::read_dir(&d)
            .unwrap()
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| n.starts_with("settings.json.bad"))
            .collect();
        assert_eq!(backups.len(), 1, "{backups:?}");
        assert_eq!(fs::read_to_string(d.join(&backups[0])).unwrap(), text);
        assert_eq!(Settings::load_checked_from(&path).1, None);
        // A second save of a good file makes no backup.
        Settings::default().save_to(&path).unwrap();
        assert_eq!(fs::read_dir(&d).unwrap().count(), 2);
        assert_eq!(Settings::load_checked_from(&d.join("none.json")).1, None);
    }

    #[test]
    fn round_trip_keeps_unknown_keys() {
        let d = tmp("roundtrip");
        let path = d.join(SETTINGS_FILE);
        fs::write(
            &path,
            r#"{"theme":"dark","future_option":{"a":1},"roof_defaults":{"pitch_deg":35}}"#,
        )
        .unwrap();
        let mut s = Settings::load_from(&path);
        assert_eq!(s.theme, Theme::Dark);
        assert_eq!(s.roof_defaults.pitch_deg, 35.0);
        // Missing nested keys fall back to defaults.
        assert_eq!(s.roof_defaults.overhang, 0.5);
        assert!(s.follow_ai);
        s.locale = Some("de".into());
        s.save_to(&path).unwrap();
        let again = Settings::load_from(&path);
        assert_eq!(again.locale.as_deref(), Some("de"));
        assert_eq!(again.extra["future_option"]["a"], 1);
    }

    #[test]
    fn rejects_bad_values() {
        let s = Settings {
            locale: Some("fr".into()),
            ..Settings::default()
        };
        assert_eq!(s.validate(), Err(SettingsError::InvalidValue("locale")));
        let mut s = Settings::default();
        s.roof_defaults.pitch_deg = 95.0;
        assert!(s.validate().is_err());
        let mut s = Settings::default();
        s.roof_defaults.overhang = f64::NAN;
        assert!(s.validate().is_err());
        // Defaults every new wall would refuse (HUNTIO-007).
        for (pitch, eave) in [(1.0, 3.0), (0.0, 3.0), (40.0, -5.0)] {
            let mut s = Settings::default();
            s.roof_defaults.pitch_deg = pitch;
            s.roof_defaults.eave_height = eave;
            assert!(s.validate().is_err(), "{pitch} {eave}");
        }
        let s = Settings {
            projects_dir: Some(PathBuf::from("relative/dir")),
            ..Settings::default()
        };
        assert_eq!(s.validate(), Err(SettingsError::NotAbsolute));
    }

    #[test]
    fn drawings_default_follows_projects_dir() {
        let d = tmp("drawings");
        let s = Settings {
            projects_dir: Some(d.clone()),
            ..Settings::default()
        };
        if env_projects_dir().is_none() {
            assert_eq!(s.projects_dir(), d);
            assert_eq!(s.drawings_dir(), d.join(DEFAULT_DRAWINGS_SUBDIR));
        }
        let own = d.join("inbox");
        let s = Settings {
            drawings_dir: Some(own.clone()),
            ..s
        };
        assert_eq!(s.drawings_dir(), own);
    }

    #[test]
    fn patch_changes_only_given_keys() {
        let d = tmp("patch");
        let base = Settings::default();
        let next = base
            .patched(&serde_json::json!({
                "theme": "light",
                "roof_defaults": { "overhang": 0.8 },
                "drawings_dir": d.join("in").to_string_lossy(),
            }))
            .unwrap();
        assert_eq!(next.theme, Theme::Light);
        assert_eq!(next.roof_defaults.overhang, 0.8);
        assert_eq!(next.roof_defaults.pitch_deg, 40.0);
        assert!(d.join("in").is_dir(), "chosen folder is created");
        let reset = next
            .patched(&serde_json::json!({ "drawings_dir": null }))
            .unwrap();
        assert_eq!(reset.drawings_dir, None);
        assert!(base.patched(&serde_json::json!({ "nope": 1 })).is_err());
        assert!(
            base.patched(&serde_json::json!({ "roof_defaults": { "nope": 1 } }))
                .is_err()
        );
        assert!(
            base.patched(&serde_json::json!({ "theme": "pink" }))
                .is_err()
        );
        assert!(
            base.patched(&serde_json::json!({ "follow_ai": "yes" }))
                .is_err()
        );
    }

    #[test]
    fn prepare_dir_creates_and_checks() {
        let d = tmp("prepare");
        let sub = d.join("a").join("b");
        prepare_dir(&sub).unwrap();
        assert!(sub.is_dir());
        let file = d.join("file.txt");
        fs::write(&file, "x").unwrap();
        assert_eq!(prepare_dir(&file), Err(SettingsError::NotAFolder));
        assert_eq!(
            prepare_dir(Path::new("rel")),
            Err(SettingsError::NotAbsolute)
        );
    }
}
