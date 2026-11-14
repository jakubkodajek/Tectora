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
    fn validate(&self) -> Result<(), SettingsError> {
        let ok = |v: f64, lo: f64, hi: f64| v.is_finite() && (lo..=hi).contains(&v);
        if !ok(self.pitch_deg, 0.0, 89.0) {
            return Err(SettingsError::InvalidValue("pitch_deg"));
        }
        if !ok(self.eave_height, -100.0, 1000.0) {
            return Err(SettingsError::InvalidValue("eave_height"));
        }
        if !ok(self.overhang, 0.0, 20.0) {
            return Err(SettingsError::InvalidValue("overhang"));
        }
        Ok(())
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
        match serde_json::from_str(&text) {
            Ok(s) => (s, None),
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
                        target.insert(k.clone(), v.clone());
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
        Ok((next, asks))
    }

    /// The user's answer to [`Settings::folder_request`]: `allow` applies it,
    /// either way it is closed.
    pub fn answer_folder_request(&self, allow: bool) -> Result<Settings, SettingsError> {
        let mut next = self.clone();
        let Some(req) = next.folder_request.take() else {
            return Ok(next);
        };
        if allow {
            next = next.patched(&Value::Object(req.changes))?;
        }
        Ok(next)
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
        let kept = next.answer_folder_request(false).unwrap();
        assert_eq!(kept.projects_dir(), s.projects_dir());
        assert!(kept.folder_request.is_none());
        let moved = next.answer_folder_request(true).unwrap();
        assert_eq!(moved.projects_dir(), target);
        assert!(target.is_dir(), "the allowed folder is created");
        assert!(moved.folder_request.is_none());
        // Setting the folder it already is asks nothing.
        let same = serde_json::json!({ "projects_dir": s.projects_dir() });
        assert!(!s.patched_by_ai(&same, "Claude", "x").unwrap().1);
    }

    fn tmp(name: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("tectora-settings-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
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
