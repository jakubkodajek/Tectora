//! Project library: one `.tectora` JSON file per design in a plain folder,
//! so the CLI, MCP server and the app can all read and write the same files.
//!
//! Every save made through [`Store::save_as`] is also recorded in an
//! activity log (`.activity/<id>.jsonl`) together with the previous version,
//! so the app can show who changed what and revert it. AI agents publish
//! what they are doing right now in `.activity/live.json`. How far the user
//! has reviewed (accepted) the agents' changes is kept in
//! `.activity/<id>.review.json`, so the app and the agents see the same state.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::diff::{Change, diff};
use crate::model::{JobInfo, Project};
use crate::roof;

pub const EXTENSION: &str = "tectora";
pub const ACTIVITY_DIR: &str = ".activity";
pub const LIVE_FILE: &str = "live.json";
/// Files attached to a job (drawings, photos), in `.files/<id>/`.
pub const FILES_DIR: &str = ".files";
/// The actor name used for edits made in the desktop app.
pub const USER_ACTOR: &str = "user";
/// Activity entries kept per project (each holds a full previous version).
const ACTIVITY_KEEP: usize = 50;

/// A file attached to a job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attachment {
    pub name: String,
    /// Bytes.
    pub size: u64,
    pub modified_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub notes: String,
    pub job: JobInfo,
    pub modified_ms: u64,
    pub roof_area: f64,
    pub parts: usize,
    /// Footprint outlines for the card thumbnail.
    pub outlines: Vec<Vec<[f64; 2]>>,
    /// Ridge, hip and valley lines in plan for the thumbnail.
    pub lines: Vec<[[f64; 2]; 2]>,
    /// Latest recorded change, if any.
    pub last_activity: Option<ActivityInfo>,
    /// Lowest and highest roof pitch, degrees (gables left out).
    pub pitch_range: Option<[f64; 2]>,
    pub dormers: usize,
    /// Attached drawings and photos.
    pub files: usize,
    /// The figures an offer was made with are stored.
    pub has_result: bool,
}

/// An activity entry without the stored previous version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityInfo {
    pub ts_ms: u64,
    pub actor: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityEntry {
    pub ts_ms: u64,
    /// `user` for the desktop app, otherwise the AI client's name.
    pub actor: String,
    /// One sentence on what was done and why.
    pub summary: String,
    /// The project before this change (`None` when it was created).
    pub before: Option<Project>,
    /// What this change did, computed when it was saved.
    #[serde(default)]
    pub changes: Vec<Change>,
    /// The project this change saved. A revert merges the inverse of the
    /// change onto the current file with it as the base, so edits saved
    /// later without a log entry (the app's autosave) are kept. `None` in
    /// entries written by Tectora 0.5 and older.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<Project>,
}

impl ActivityEntry {
    pub fn info(&self) -> ActivityInfo {
        ActivityInfo {
            ts_ms: self.ts_ms,
            actor: self.actor.clone(),
            summary: self.summary.clone(),
        }
    }
}

/// How far the user has reviewed a project's AI changes.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReviewMark {
    /// Every change recorded at or before this time is accepted.
    pub accepted_ms: u64,
}

/// AI changes the user has not reviewed yet: every agent change after the
/// last accept and after the user's own last recorded action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingReview {
    /// The oldest pending change; reverting to it undoes them all.
    pub since_ts_ms: u64,
    /// Number of pending changes.
    pub count: usize,
    /// Who made them (the MCP client names), oldest first, without repeats.
    pub actors: Vec<String>,
    /// The project before the oldest pending change (`None` when an agent
    /// created it).
    pub before: Option<Project>,
}

/// A pending review with what it does to the figures, for the app and the
/// agents (`get_history`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewStatus {
    #[serde(flatten)]
    pub pending: PendingReview,
    /// Time up to which the user accepted earlier changes (0: never).
    pub accepted_ms: u64,
    /// Figures before the pending changes against now; `None` when one of
    /// the two versions cannot be calculated.
    pub differences: Option<Vec<crate::snapshot::Difference>>,
}

/// What an AI agent is doing right now, shown live in the app.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveStatus {
    pub ts_ms: u64,
    pub actor: String,
    /// Short description, e.g. "Reading project rodinny-dum".
    pub message: String,
    pub project_id: Option<String>,
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// The library folder: `TECTORA_PROJECTS_DIR`, then the folder chosen in
/// the app settings, then `Documents/Tectora`.
pub fn default_dir() -> PathBuf {
    crate::settings::Settings::load().projects_dir()
}

/// Plan view of the inner roof lines (everything except eaves).
pub fn roof_lines(project: &Project) -> Vec<[[f64; 2]; 2]> {
    crate::openings::generate_project(project)
        .into_iter()
        .flat_map(|g| g.lines)
        .filter(|l| l.kind != roof::LineKind::Eave)
        .map(|l| [[l.a[0], l.a[1]], [l.b[0], l.b[1]]])
        .collect()
}

pub fn outlines(project: &Project) -> Vec<Vec<[f64; 2]>> {
    project
        .parts
        .iter()
        .map(|p| p.outline().iter().map(|pt| [pt.x, pt.y]).collect())
        .collect()
}

pub fn roof_area(project: &Project) -> f64 {
    crate::quantities::roof_totals(project).roof_area
}

pub struct Store {
    pub dir: PathBuf,
    /// Summaries by id with the file stamps they were made from, so listing
    /// a large library does not recalculate every roof each time.
    cache: Mutex<HashMap<String, (Stamp, ProjectSummary)>>,
}

/// What a cached summary depends on: the project file, its activity log and
/// its files folder (modification time and size of each).
type Stamp = [Option<(SystemTime, u64)>; 3];

/// Why a library operation failed. Each variant has a stable [`code`]
/// (`StoreError::code`) so the app can show a translated message; the
/// `Display` text is English for the CLI, the MCP server and logs.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    /// The id is empty or would leave the library folder.
    #[error("invalid project id: {0}")]
    InvalidId(String),
    /// No project file with this id.
    #[error("{0}: project not found")]
    NotFound(String),
    /// The file exists but is not a valid Tectora project.
    #[error("{id}: the project file is damaged ({detail})")]
    Unreadable { id: String, detail: String },
    /// The file was saved by a newer Tectora: it is neither read nor
    /// overwritten, so the newer version's data is not lost.
    #[error(
        "{id}: the file was saved by a newer Tectora (format {version}, this version reads up to {max})"
    )]
    NewerFormat { id: String, version: u64, max: u32 },
    /// The operating system refused to read or write a file.
    #[error("{}{detail}", id.as_deref().map(|i| format!("{i}: ")).unwrap_or_default())]
    Io {
        id: Option<String>,
        kind: IoKind,
        detail: String,
    },
    /// Revert asked for, but no change has been recorded.
    #[error("{0}: no recorded change to revert")]
    NothingToRevert(String),
    /// The change to revert created the project; there is no earlier version.
    #[error("{0}: that change created the project")]
    RevertCreated(String),
    /// No recorded change has this timestamp (the log moved on).
    #[error("{id}: no recorded change at {ts_ms}")]
    NoSuchChange { id: String, ts_ms: u64 },
    /// The job was edited after the change to revert, in values that change
    /// also set: reverting would overwrite those edits, so nothing is done.
    #[error("{id}: edited since that change in the same values ({detail})")]
    RevertConflict { id: String, detail: String },
}

/// The kinds of file system failure the app explains differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IoKind {
    PermissionDenied,
    DiskFull,
    Other,
}

impl StoreError {
    /// A stable, machine-readable code (`snake_case`).
    pub fn code(&self) -> &'static str {
        match self {
            StoreError::InvalidId(_) => "invalid_id",
            StoreError::NotFound(_) => "not_found",
            StoreError::Unreadable { .. } => "unreadable",
            StoreError::NewerFormat { .. } => "newer_format",
            StoreError::Io {
                kind: IoKind::PermissionDenied,
                ..
            } => "permission_denied",
            StoreError::Io {
                kind: IoKind::DiskFull,
                ..
            } => "disk_full",
            StoreError::Io { .. } => "io",
            StoreError::NothingToRevert(_) => "nothing_to_revert",
            StoreError::RevertCreated(_) => "revert_created",
            StoreError::NoSuchChange { .. } => "no_such_change",
            StoreError::RevertConflict { .. } => "revert_conflict",
        }
    }

    /// The project the error is about, if any.
    pub fn id(&self) -> Option<&str> {
        match self {
            StoreError::InvalidId(id)
            | StoreError::NotFound(id)
            | StoreError::NothingToRevert(id)
            | StoreError::RevertCreated(id) => Some(id),
            StoreError::Unreadable { id, .. }
            | StoreError::NewerFormat { id, .. }
            | StoreError::NoSuchChange { id, .. }
            | StoreError::RevertConflict { id, .. } => Some(id),
            StoreError::Io { id, .. } => id.as_deref(),
        }
    }

    /// The underlying message (OS or parser error), if any.
    pub fn detail(&self) -> Option<&str> {
        match self {
            StoreError::Unreadable { detail, .. }
            | StoreError::Io { detail, .. }
            | StoreError::RevertConflict { detail, .. } => Some(detail),
            _ => None,
        }
    }

    fn io(id: Option<&str>, e: std::io::Error) -> StoreError {
        let kind = match e.kind() {
            std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::ReadOnlyFilesystem => {
                IoKind::PermissionDenied
            }
            std::io::ErrorKind::StorageFull | std::io::ErrorKind::QuotaExceeded => IoKind::DiskFull,
            _ => IoKind::Other,
        };
        StoreError::Io {
            id: id.map(str::to_string),
            kind,
            detail: e.to_string(),
        }
    }
}

/// The MCP server and the CLI pass errors on as text.
impl From<StoreError> for String {
    fn from(e: StoreError) -> String {
        e.to_string()
    }
}

/// A project file in the library folder that could not be read.
#[derive(Debug, Clone, Serialize)]
pub struct BrokenFile {
    pub id: String,
    pub code: &'static str,
    pub detail: String,
}

fn modified_ms(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Writes via a temporary file so a watcher never sees a half-written file.
/// The temporary name is unique per process and call, so the app and an MCP
/// server saving the same design at once cannot write into each other's file.
/// The `schema_version` of a project file's JSON when it is newer than this
/// app reads, else `None` (also for text that is not JSON).
fn newer_format(text: &str) -> Option<u64> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let version = v.get("schema_version")?.as_u64()?;
    (version > u64::from(crate::model::SCHEMA_VERSION)).then_some(version)
}

fn write_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = path.with_extension(format!("{}-{n}.tmp", std::process::id()));
    fs::write(&tmp, contents)?;
    fs::rename(&tmp, path).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

/// Longest accepted project id (file names stay well inside the 255-byte
/// limit of common file systems).
const MAX_ID_LEN: usize = 120;

/// Names Windows reserves for devices in every folder (`CON.tectora` opens
/// the console, not a file).
const WINDOWS_DEVICE_NAMES: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// A project id is a plain file name inside the library folder on both
/// macOS and Windows: no separators, drive or stream prefix (`C:x`,
/// `x:stream`), characters Windows forbids, hidden names or device names.
fn valid_id(id: &str) -> bool {
    id.len() <= MAX_ID_LEN
        && plain_name(id)
        && !WINDOWS_DEVICE_NAMES.contains(&id.to_ascii_lowercase().as_str())
}

/// No separators, drive or stream prefix, characters Windows forbids,
/// hidden or trailing-dot names.
fn plain_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && !name.ends_with(['.', ' '])
        && !name.chars().any(|c| {
            c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        })
}

/// An attachment or export name is a plain file name on both macOS and
/// Windows: the [`valid_id`] rules, and no device name before an extension
/// either (`NUL.pdf` is the null device on Windows). Without this a received
/// bundle's `files/C:x.dll` joins to `C:x.dll` on Windows, outside the
/// library (ARCH-039).
pub fn valid_file_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or("").trim_end();
    name.len() <= 255
        && plain_name(name)
        && !WINDOWS_DEVICE_NAMES.contains(&stem.to_ascii_lowercase().as_str())
}

/// File-name friendly id derived from a project name.
fn slug(name: &str) -> String {
    let mut s: String = name
        .to_lowercase()
        .chars()
        .map(|c| {
            let c = crate::library::fold_char(c);
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    while s.contains("--") {
        s = s.replace("--", "-");
    }
    let s = s.trim_matches('-').to_string();
    if s.is_empty() { "navrh".into() } else { s }
}

impl Store {
    pub fn new(dir: PathBuf) -> std::io::Result<Self> {
        fs::create_dir_all(&dir)?;
        Ok(Self::at(dir))
    }

    /// A store for `dir` without creating the folder.
    pub fn at(dir: PathBuf) -> Self {
        Self {
            dir,
            cache: Mutex::default(),
        }
    }

    pub fn open_default() -> std::io::Result<Self> {
        Self::new(default_dir())
    }

    pub fn path(&self, id: &str) -> Result<PathBuf, StoreError> {
        if !valid_id(id) {
            return Err(StoreError::InvalidId(id.to_string()));
        }
        Ok(self.dir.join(format!("{id}.{EXTENSION}")))
    }

    pub fn activity_dir(&self) -> PathBuf {
        self.dir.join(ACTIVITY_DIR)
    }

    pub(crate) fn activity_path(&self, id: &str) -> Result<PathBuf, StoreError> {
        self.path(id)?;
        Ok(self.activity_dir().join(format!("{id}.jsonl")))
    }

    pub fn exists(&self, id: &str) -> bool {
        self.path(id).is_ok_and(|p| p.exists())
    }

    pub fn summarize(&self, id: &str) -> Result<ProjectSummary, StoreError> {
        let stamp = self.stamp(id)?;
        let cache = || self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if stamp[0].is_some()
            && let Some((s, summary)) = cache().get(id)
            && *s == stamp
        {
            return Ok(summary.clone());
        }
        let summary = self.summarize_uncached(id)?;
        cache().insert(id.to_string(), (stamp, summary.clone()));
        Ok(summary)
    }

    fn stamp(&self, id: &str) -> Result<Stamp, StoreError> {
        let of = |p: PathBuf| {
            fs::metadata(p)
                .ok()
                .and_then(|m| Some((m.modified().ok()?, m.len())))
        };
        Ok([
            of(self.path(id)?),
            of(self.activity_path(id)?),
            of(self.files_dir(id)?),
        ])
    }

    fn summarize_uncached(&self, id: &str) -> Result<ProjectSummary, StoreError> {
        let project = self.load(id)?;
        // A broken design still shows up in the library (so it can be opened
        // and fixed or deleted), just without its roof preview.
        let (area, lines) = project
            .check_limits()
            .and_then(|()| crate::model::guarded(|| (roof_area(&project), roof_lines(&project))))
            .unwrap_or_default();
        Ok(ProjectSummary {
            id: id.to_string(),
            name: project.name.clone(),
            notes: project.notes.clone(),
            job: project.job.clone(),
            modified_ms: modified_ms(&self.path(id)?),
            roof_area: area,
            parts: project.parts.len(),
            outlines: outlines(&project),
            lines,
            last_activity: self.activity(id).last().map(ActivityEntry::info),
            pitch_range: crate::library::pitch_range(&project),
            dormers: project.dormers.len(),
            files: self.attachments(id).len(),
            has_result: project.result_snapshot.is_some(),
        })
    }

    pub fn list(&self) -> Vec<ProjectSummary> {
        self.scan().map(|(ok, _)| ok).unwrap_or_default()
    }

    /// Every project in the library, newest first, and the project files
    /// that could not be read (so the app can say so instead of hiding
    /// them). Fails only when the folder itself cannot be read.
    pub fn scan(&self) -> Result<(Vec<ProjectSummary>, Vec<BrokenFile>), StoreError> {
        let entries = fs::read_dir(&self.dir).map_err(|e| StoreError::io(None, e))?;
        let mut ok = Vec::new();
        let mut broken = Vec::new();
        let ids = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some(EXTENSION))
            .filter_map(|p| p.file_stem().and_then(|s| s.to_str()).map(str::to_string));
        for id in ids {
            // One bad design must not take the whole library down.
            let summary =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.summarize(&id)));
            match summary {
                Ok(Ok(s)) => ok.push(s),
                Ok(Err(e)) => broken.push(BrokenFile {
                    id,
                    code: e.code(),
                    detail: e.detail().unwrap_or_default().to_string(),
                }),
                Err(panic) => broken.push(BrokenFile {
                    id,
                    code: "internal",
                    detail: panic_message(&*panic),
                }),
            }
        }
        ok.sort_by_key(|s| std::cmp::Reverse(s.modified_ms));
        broken.sort_by(|a, b| a.id.cmp(&b.id));
        // Forget deleted projects.
        self.cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|id, _| ok.iter().any(|s| &s.id == id));
        Ok((ok, broken))
    }

    pub fn load(&self, id: &str) -> Result<Project, StoreError> {
        let text = fs::read_to_string(self.path(id)?).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StoreError::NotFound(id.to_string())
            } else {
                StoreError::io(Some(id), e)
            }
        })?;
        if let Some(version) = newer_format(&text) {
            return Err(StoreError::NewerFormat {
                id: id.to_string(),
                version,
                max: crate::model::SCHEMA_VERSION,
            });
        }
        Project::from_json(&text).map_err(|e| StoreError::Unreadable {
            id: id.to_string(),
            detail: e.to_string(),
        })
    }

    /// Picks an unused id based on the project name.
    pub fn new_id(&self, name: &str) -> String {
        let mut base = slug(name);
        // Leave room for the "-N" suffix and keep clear of device names.
        base.truncate(MAX_ID_LEN - 8);
        if !valid_id(&base) {
            base.push_str("-navrh");
        }
        let mut id = base.clone();
        let mut n = 2;
        while self.dir.join(format!("{id}.{EXTENSION}")).exists() {
            id = format!("{base}-{n}");
            n += 1;
        }
        id
    }

    /// Saves without recording activity (app autosave of its own edits).
    ///
    /// Refuses to replace a file saved by a newer Tectora: on a shared
    /// library folder an older app would otherwise write its stale copy over
    /// a colleague's newer one.
    pub fn save(&self, id: Option<String>, project: &Project) -> Result<String, StoreError> {
        let id = id.unwrap_or_else(|| self.new_id(&project.name));
        let path = self.path(&id)?;
        if let Some(version) = fs::read_to_string(&path)
            .ok()
            .and_then(|t| newer_format(&t))
        {
            return Err(StoreError::NewerFormat {
                id,
                version,
                max: crate::model::SCHEMA_VERSION,
            });
        }
        let json = project.to_json().map_err(|e| StoreError::Unreadable {
            id: id.clone(),
            detail: e.to_string(),
        })?;
        write_atomic(&path, &json).map_err(|e| StoreError::io(Some(&id), e))?;
        Ok(id)
    }

    /// Saves and records who made the change and why, keeping the previous
    /// version so it can be reviewed and reverted.
    pub fn save_as(
        &self,
        id: Option<String>,
        project: &Project,
        actor: &str,
        summary: &str,
    ) -> Result<String, StoreError> {
        let before = match id.as_deref().map(|id| self.load(id)) {
            // Refused before anything is logged (see `save`).
            Some(Err(e @ StoreError::NewerFormat { .. })) => return Err(e),
            Some(r) => r.ok(),
            None => None,
        };
        if before.as_ref() == Some(project) {
            return Ok(id.unwrap_or_default());
        }
        let id = id.unwrap_or_else(|| self.new_id(&project.name));
        // Log first: the app reacts to the project file and then reads the log.
        self.append_activity(
            &id,
            ActivityEntry {
                ts_ms: now_ms(),
                actor: actor.to_string(),
                summary: summary.to_string(),
                changes: before
                    .as_ref()
                    .map(|b| diff(b, project))
                    .unwrap_or_default(),
                before,
                after: Some(project.clone()),
            },
        )?;
        self.save(Some(id.clone()), project)
    }

    fn append_activity(&self, id: &str, entry: ActivityEntry) -> Result<(), StoreError> {
        let io = |e| StoreError::io(Some(id), e);
        fs::create_dir_all(self.activity_dir()).map_err(io)?;
        let path = self.activity_path(id)?;
        let mut entries = self.activity(id);
        let line = serde_json::to_string(&entry).map_err(|e| io(e.into()))?;
        if entries.len() >= ACTIVITY_KEEP {
            entries.push(entry);
            let mut keep: Vec<&ActivityEntry> =
                entries[entries.len() - ACTIVITY_KEEP..].iter().collect();
            // The first change still waiting for review holds the version
            // the user had before it ("Revert all" goes back to it). When
            // more changes than the log keeps are pending, keep that one in
            // place of the oldest kept one, so the pre-AI version is never
            // dropped while the review is open.
            let start = self.pending_start(id, &entries);
            if start < entries.len() - ACTIVITY_KEEP {
                keep[0] = &entries[start];
            }
            let text: Vec<String> = keep
                .iter()
                .filter_map(|e| serde_json::to_string(e).ok())
                .collect();
            return write_atomic(&path, &(text.join("\n") + "\n")).map_err(io);
        }
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(io)?;
        writeln!(f, "{line}").map_err(io)
    }

    /// Recorded changes of a project, oldest first.
    pub fn activity(&self, id: &str) -> Vec<ActivityEntry> {
        let Ok(path) = self.activity_path(id) else {
            return Vec::new();
        };
        fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect()
    }

    /// Undoes the latest recorded change, keeping later unlogged edits.
    pub fn revert_last(&self, id: &str, actor: &str) -> Result<Project, StoreError> {
        let log = self.activity(id);
        let last = log
            .last()
            .ok_or_else(|| StoreError::NothingToRevert(id.to_string()))?;
        self.revert_from(
            id,
            &log,
            log.len() - 1,
            actor,
            format!("Reverted: {}", last.summary),
        )
    }

    /// Restores the version from before the change recorded at `ts_ms`,
    /// undoing it and every later change in one step.
    pub fn revert_to(&self, id: &str, ts_ms: u64, actor: &str) -> Result<Project, StoreError> {
        let log = self.activity(id);
        let pos =
            log.iter()
                .rposition(|e| e.ts_ms == ts_ms)
                .ok_or_else(|| StoreError::NoSuchChange {
                    id: id.to_string(),
                    ts_ms,
                })?;
        let undone = log.len() - pos;
        let summary = if undone == 1 {
            format!("Reverted: {}", log[pos].summary)
        } else {
            format!("Reverted {undone} changes since: {}", log[pos].summary)
        };
        self.revert_from(id, &log, pos, actor, summary)
    }

    /// Undoes `log[pos..]`: the version before `log[pos]`, plus whatever the
    /// current file changed after the latest entry. The app autosaves the
    /// user's own edits without a log entry, so the file can be newer than
    /// the last entry's result; those edits are merged onto the reverted
    /// version (ERR-001). Refuses when they touch a value the reverted
    /// changes set. Edits saved between the reverted entries are reverted
    /// with them.
    fn revert_from(
        &self,
        id: &str,
        log: &[ActivityEntry],
        pos: usize,
        actor: &str,
        summary: String,
    ) -> Result<Project, StoreError> {
        let before = log[pos]
            .before
            .clone()
            .ok_or_else(|| StoreError::RevertCreated(id.to_string()))?;
        let target = match (log.last().and_then(|e| e.after.as_ref()), self.load(id)) {
            (Some(after), Ok(current)) if &current != after => {
                let value = |p: &Project| {
                    serde_json::to_value(p).map_err(|e| StoreError::Unreadable {
                        id: id.to_string(),
                        detail: e.to_string(),
                    })
                };
                let merged =
                    crate::merge::merge3(&value(after)?, &value(&current)?, &value(&before)?)
                        .map_err(|paths| StoreError::RevertConflict {
                            id: id.to_string(),
                            detail: paths.join(", "),
                        })?;
                serde_json::from_value(merged).map_err(|e| StoreError::Unreadable {
                    id: id.to_string(),
                    detail: e.to_string(),
                })?
            }
            // A file newer than this Tectora is neither read nor written.
            (_, Err(e @ StoreError::NewerFormat { .. })) => return Err(e),
            _ => before,
        };
        self.save_as(Some(id.to_string()), &target, actor, &summary)?;
        Ok(target)
    }

    fn review_path(&self, id: &str) -> Result<PathBuf, StoreError> {
        self.path(id)?;
        Ok(self.activity_dir().join(format!("{id}.review.json")))
    }

    /// Time up to which the user accepted the changes of project `id`
    /// (0 when nothing was accepted yet).
    pub fn reviewed_at(&self, id: &str) -> u64 {
        self.review_path(id)
            .ok()
            .and_then(|p| fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str::<ReviewMark>(&t).ok())
            .map_or(0, |m| m.accepted_ms)
    }

    /// Accepts every change recorded at or before `ts_ms`. Never moves the
    /// mark back, so a late call from a second window cannot reopen a review.
    pub fn mark_reviewed(&self, id: &str, ts_ms: u64) -> Result<u64, StoreError> {
        let path = self.review_path(id)?;
        let accepted_ms = self.reviewed_at(id).max(ts_ms);
        let io = |e| StoreError::io(Some(id), e);
        fs::create_dir_all(self.activity_dir()).map_err(io)?;
        let text = serde_json::to_string(&ReviewMark { accepted_ms }).map_err(|e| io(e.into()))?;
        write_atomic(&path, &text).map_err(io)?;
        Ok(accepted_ms)
    }

    /// The agents' changes to `id` that wait for the user's review.
    /// Index of the first entry of `log` still waiting for review: the one
    /// after the last entry by the user or accepted in the app.
    fn pending_start(&self, id: &str, log: &[ActivityEntry]) -> usize {
        let accepted = self.reviewed_at(id);
        log.iter()
            .rposition(|e| e.actor == USER_ACTOR || e.ts_ms <= accepted)
            .map_or(0, |i| i + 1)
    }

    pub fn pending_review(&self, id: &str) -> Option<PendingReview> {
        let log = self.activity(id);
        let start = self.pending_start(id, &log);
        let pending = &log[start..];
        let first = pending.first()?;
        let mut actors: Vec<String> = Vec::new();
        for e in pending {
            if !actors.contains(&e.actor) {
                actors.push(e.actor.clone());
            }
        }
        Some(PendingReview {
            since_ts_ms: first.ts_ms,
            count: pending.len(),
            actors,
            before: first.before.clone(),
        })
    }

    /// The pending review of `id` with its effect on the figures, or `None`
    /// when every change is reviewed.
    pub fn review_status(&self, id: &str) -> Result<Option<ReviewStatus>, StoreError> {
        let Some(pending) = self.pending_review(id) else {
            return Ok(None);
        };
        let current = self.load(id)?;
        Ok(Some(ReviewStatus {
            differences: crate::snapshot::change_in_figures(pending.before.as_ref(), &current),
            accepted_ms: self.reviewed_at(id),
            pending,
        }))
    }

    pub fn delete(&self, id: &str) -> Result<(), StoreError> {
        fs::remove_file(self.path(id)?).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StoreError::NotFound(id.to_string())
            } else {
                StoreError::io(Some(id), e)
            }
        })?;
        let _ = fs::remove_file(self.activity_path(id)?);
        let _ = fs::remove_file(self.review_path(id)?);
        let _ = fs::remove_dir_all(self.files_dir(id)?);
        Ok(())
    }

    /// Folder of the files attached to job `id`.
    pub fn files_dir(&self, id: &str) -> Result<PathBuf, StoreError> {
        self.path(id)?;
        Ok(self.dir.join(FILES_DIR).join(id))
    }

    /// Path of attachment `name` of job `id`.
    /// Whether an attached file may be opened with the system's default app
    /// by one click: drawings, photos, documents and spreadsheets. Anything
    /// else (`.exe`, `.command`, `.lnk`, `.js`, `.app`, …) may have come in a
    /// received bundle; extracted by Tectora it carries no "downloaded from
    /// the internet" mark, so the system would run it without a warning. The
    /// app shows such a file in its folder instead.
    pub fn opens_directly(name: &str) -> bool {
        const SAFE: &[&str] = &[
            "pdf", "png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff", "heic", "svg", "txt",
            "csv", "dxf", "dwg", "xlsx", "xls", "ods", "docx", "doc", "odt",
        ];
        // Only the last extension counts: `plan.pdf.exe` is an `.exe`.
        name.rsplit_once('.')
            .map(|(stem, ext)| {
                !stem.is_empty() && SAFE.contains(&ext.to_ascii_lowercase().as_str())
            })
            .unwrap_or(false)
    }

    pub fn attachment_path(&self, id: &str, name: &str) -> Result<PathBuf, String> {
        if !valid_file_name(name) {
            return Err(format!("invalid file name: {name}"));
        }
        Ok(self.files_dir(id)?.join(name))
    }

    /// The files attached to job `id`, by name.
    pub fn attachments(&self, id: &str) -> Vec<Attachment> {
        let Ok(dir) = self.files_dir(id) else {
            return Vec::new();
        };
        let mut out: Vec<Attachment> = fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .filter_map(|e| {
                let name = e.file_name().into_string().ok()?;
                (!name.starts_with('.')).then(|| Attachment {
                    size: e.metadata().map_or(0, |m| m.len()),
                    modified_ms: modified_ms(&e.path()),
                    name,
                })
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    /// Stores `bytes` as an attachment of job `id` under `name`, or under
    /// `name (2)` etc. when that name is taken. Returns the name used.
    pub fn add_attachment(&self, id: &str, name: &str, bytes: &[u8]) -> Result<String, String> {
        if !self.exists(id) {
            return Err(format!("no project with id {id}"));
        }
        let name = name.trim();
        self.attachment_path(id, name)?;
        let (stem, ext) = match name.rfind('.') {
            Some(i) if i > 0 => (&name[..i], &name[i..]),
            _ => (name, ""),
        };
        let mut used = name.to_string();
        let mut n = 2;
        while self.attachment_path(id, &used)?.exists() {
            used = format!("{stem} ({n}){ext}");
            n += 1;
        }
        let path = self.attachment_path(id, &used)?;
        fs::create_dir_all(path.parent().expect("has a parent")).map_err(|e| e.to_string())?;
        fs::write(&path, bytes).map_err(|e| e.to_string())?;
        Ok(used)
    }

    /// Copies a file from disk into job `id`'s attachments.
    pub fn attach_file(&self, id: &str, src: &Path) -> Result<String, String> {
        let bytes = fs::read(src).map_err(|e| format!("{}: {e}", src.display()))?;
        let name = src
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| format!("{}: not a file", src.display()))?;
        self.add_attachment(id, name, &bytes)
    }

    pub fn remove_attachment(&self, id: &str, name: &str) -> Result<(), String> {
        fs::remove_file(self.attachment_path(id, name)?).map_err(|e| e.to_string())
    }

    /// Copies all attachments of job `from` to job `to`.
    pub fn copy_attachments(&self, from: &str, to: &str) -> Result<(), String> {
        for a in self.attachments(from) {
            let bytes =
                fs::read(self.attachment_path(from, &a.name)?).map_err(|e| e.to_string())?;
            self.add_attachment(to, &a.name, &bytes)?;
        }
        Ok(())
    }

    pub fn set_live(&self, status: &LiveStatus) {
        if fs::create_dir_all(self.activity_dir()).is_ok() {
            if let Ok(text) = serde_json::to_string(status) {
                let _ = write_atomic(&self.activity_dir().join(LIVE_FILE), &text);
            }
        }
    }

    pub fn live(&self) -> Option<LiveStatus> {
        let text = fs::read_to_string(self.activity_dir().join(LIVE_FILE)).ok()?;
        serde_json::from_str(&text).ok()
    }
}

/// The text of a caught panic payload.
pub fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::examples;

    fn temp_store(name: &str) -> Store {
        let dir = std::env::temp_dir().join(format!("tectora-test-{name}-{}", now_ms()));
        Store::new(dir).unwrap()
    }

    #[test]
    fn errors_have_stable_codes() {
        let store = temp_store("errors");
        assert_eq!(store.load("nope").unwrap_err().code(), "not_found");
        assert_eq!(store.load("../x").unwrap_err().code(), "invalid_id");
        assert_eq!(store.delete("nope").unwrap_err().code(), "not_found");
        fs::write(store.dir.join("bad.tectora"), "{ not json").unwrap();
        let e = store.load("bad").unwrap_err();
        assert_eq!(e.code(), "unreadable");
        assert_eq!(e.id(), Some("bad"));
        let _ = fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn attachment_names_stay_plain_file_names_on_every_platform() {
        for bad in [
            "C:x.dll", "x:stream", "NUL.pdf", "nul", "Com1.txt", "lpt9", "a<b", "", ".hidden",
            "x.", "a/b", "a\\b",
        ] {
            assert!(!valid_file_name(bad), "{bad}");
        }
        for ok in [
            "plan.pdf",
            "Řez A-A (2).pdf",
            "nultá verze.pdf",
            "console.txt",
            "výkres.dwg",
        ] {
            assert!(valid_file_name(ok), "{ok}");
        }
    }

    #[test]
    fn only_documents_and_images_open_with_one_click() {
        for ok in ["plan.pdf", "Foto.JPG", "nabidka.xlsx", "rez A-A.dwg"] {
            assert!(Store::opens_directly(ok), "{ok}");
        }
        for bad in [
            "plan.pdf.exe",
            "Plan strechy.pdf.command",
            "vykres.pdf.lnk",
            "x.js",
            "x.app",
            "x.bat",
            "x.xlsm",
            "pdf",
            ".pdf",
            "x.scr",
            "x.ps1",
            "x.vbs",
            "x.html",
        ] {
            assert!(!Store::opens_directly(bad), "{bad}");
        }
    }

    #[test]
    fn revert_all_after_more_ai_steps_than_the_log_keeps_restores_the_users_version() {
        // ERR-015: the trimmed log must keep the entry that holds the
        // version before the pending AI changes.
        let store = temp_store("long-review");
        let mut p = examples::find("gable").unwrap().project();
        let id = store.save_as(None, &p, USER_ACTOR, "create").unwrap();
        let mine = p.clone();
        for i in 1..=ACTIVITY_KEEP + 5 {
            p.parts[0].edges[0].overhang = 0.5 + i as f64 / 100.0;
            store
                .save_as(Some(id.clone()), &p, "claude", &format!("step {i}"))
                .unwrap();
        }
        assert_eq!(store.activity(&id).len(), ACTIVITY_KEEP);
        let pending = store.pending_review(&id).unwrap();
        assert_eq!(pending.before.as_ref(), Some(&mine));
        let oldest = store.activity(&id)[0].ts_ms;
        assert_eq!(pending.since_ts_ms, oldest);
        assert_eq!(store.revert_to(&id, oldest, USER_ACTOR).unwrap(), mine);
        assert_eq!(store.load(&id).unwrap(), mine);
        let _ = fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn a_file_saved_by_a_newer_tectora_is_neither_read_nor_overwritten() {
        // ERR-010: on a shared folder an older app must not write its stale
        // copy over a colleague's newer file.
        let store = temp_store("newer");
        let p = examples::find("gable").unwrap().project();
        let id = store.save(None, &p).unwrap();
        let newer = r#"{"schema_version": 99, "name": "x", "job": {"customer": "Newer"}, "future_field": 1}"#;
        fs::write(store.dir.join(format!("{id}.tectora")), newer).unwrap();
        let e = store.load(&id).unwrap_err();
        assert_eq!(e.code(), "newer_format");
        assert_eq!(e.id(), Some(id.as_str()));
        assert_eq!(
            store.save(Some(id.clone()), &p).unwrap_err().code(),
            "newer_format"
        );
        assert_eq!(
            store
                .save_as(Some(id.clone()), &p, "claude", "edit")
                .unwrap_err()
                .code(),
            "newer_format"
        );
        assert!(
            store.activity(&id).is_empty(),
            "nothing logged for a refused save"
        );
        assert_eq!(
            fs::read_to_string(store.dir.join(format!("{id}.tectora"))).unwrap(),
            newer
        );
        // A damaged file can still be replaced (that is how it is repaired).
        fs::write(store.dir.join(format!("{id}.tectora")), "{").unwrap();
        store.save(Some(id.clone()), &p).unwrap();
        assert_eq!(store.load(&id).unwrap(), p);
        let _ = fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn scan_reports_unreadable_files_instead_of_hiding_them() {
        let store = temp_store("scan");
        let id = store
            .save(None, &crate::examples::find("gable").unwrap().project())
            .unwrap();
        fs::write(store.dir.join("broken.tectora"), "{").unwrap();
        let (ok, broken) = store.scan().unwrap();
        assert_eq!(
            ok.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            [id.as_str()]
        );
        assert_eq!(broken.len(), 1);
        assert_eq!(broken[0].id, "broken");
        assert_eq!(broken[0].code, "unreadable");
        assert_eq!(store.list().len(), 1);
        let _ = fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn cached_summary_follows_saves_files_and_deletes() {
        let store = temp_store("cache");
        let mut p = examples::find("gable").unwrap().project();
        let id = store.save_as(None, &p, USER_ACTOR, "create").unwrap();
        let first = store.summarize(&id).unwrap();
        assert_eq!((first.files, first.dormers), (0, 0));
        assert!(first.pitch_range.is_some());
        p.job.customer = "Novák".into();
        store
            .save_as(Some(id.clone()), &p, "claude", "set customer")
            .unwrap();
        let s = store.summarize(&id).unwrap();
        assert_eq!(s.job.customer, "Novák");
        assert_eq!(s.last_activity.unwrap().actor, "claude");
        store.add_attachment(&id, "plan.pdf", b"%PDF").unwrap();
        assert_eq!(store.summarize(&id).unwrap().files, 1);
        store.delete(&id).unwrap();
        assert!(store.summarize(&id).is_err());
        assert!(store.list().is_empty());
        let _ = fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn slug_handles_czech_names() {
        assert_eq!(slug("Rodinný dům Říčany"), "rodinny-dum-ricany");
        assert_eq!(slug("  "), "navrh");
        let store = Store::at(PathBuf::from("/nonexistent-tectora-dir"));
        assert_eq!(store.new_id("Con"), "con-navrh");
        assert!(
            store
                .path(&store.new_id(&"dlouhy nazev ".repeat(40)))
                .is_ok()
        );
    }

    #[test]
    fn rejects_path_traversal() {
        let store = Store::at(PathBuf::from("/tmp/x"));
        assert!(store.path("../etc/passwd").is_err());
        assert!(store.path("ok-id").is_ok());
        assert!(store.path("rodinny-dum-2").is_ok());
        assert!(store.path("Dům Říčany").is_ok());
        for bad in [
            "",
            ".hidden",
            "a/b",
            "a\\b",
            "C:evil",
            "x:stream",
            "CON",
            "nul",
            "com1",
            "trailing.",
            "trailing ",
            "a\nb",
            "q?",
            "a*b",
        ] {
            assert!(store.path(bad).is_err(), "{bad:?} must be rejected");
        }
        assert!(store.path(&"x".repeat(MAX_ID_LEN + 1)).is_err());
    }

    #[test]
    fn accepting_closes_the_review_and_never_moves_back() {
        let store = temp_store("review");
        let a = examples::find("gable").unwrap().project();
        let id = store.save_as(None, &a, "claude", "Created").unwrap();
        let review = store.pending_review(&id).unwrap();
        assert_eq!(review.count, 1);
        assert!(review.before.is_none(), "created by the agent");

        let ts = store.activity(&id)[0].ts_ms;
        assert_eq!(store.mark_reviewed(&id, ts).unwrap(), ts);
        assert!(store.pending_review(&id).is_none());
        assert_eq!(store.mark_reviewed(&id, ts - 1).unwrap(), ts);
        assert_eq!(store.reviewed_at(&id), ts);

        let mut b = a.clone();
        b.parts[0].edges[0].overhang = 0.7;
        std::thread::sleep(std::time::Duration::from_millis(2));
        store
            .save_as(Some(id.clone()), &b, "codex", "Overhang")
            .unwrap();
        let review = store.pending_review(&id).unwrap();
        assert_eq!(
            (review.count, review.actors.clone()),
            (1, vec!["codex".to_string()])
        );
        assert_eq!(review.before.as_ref(), Some(&a));
        let status = store.review_status(&id).unwrap().unwrap();
        assert_eq!(status.accepted_ms, ts);
        assert!(
            status
                .differences
                .unwrap()
                .iter()
                .any(|d| d.key == "roof_area")
        );

        store.delete(&id).unwrap();
        assert_eq!(store.reviewed_at(&id), 0);
        let _ = fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn records_activity_and_reverts() {
        let store = temp_store("activity");
        let a = examples::find("hip").unwrap().project();
        let id = store
            .save_as(None, &a, "claude", "Created hip roof")
            .unwrap();
        let mut b = a.clone();
        b.parts[0].edges[0].pitch_deg = 45.0;
        store
            .save_as(Some(id.clone()), &b, "claude", "Steeper front")
            .unwrap();

        let log = store.activity(&id);
        assert_eq!(log.len(), 2);
        assert!(log[0].before.is_none());
        assert_eq!(log[1].before.as_ref(), Some(&a));
        assert_eq!(
            store.summarize(&id).unwrap().last_activity.unwrap().summary,
            "Steeper front"
        );

        assert_eq!(store.revert_last(&id, USER_ACTOR).unwrap(), a);
        assert_eq!(store.load(&id).unwrap(), a);

        // Two agent steps undone together.
        store
            .save_as(Some(id.clone()), &b, "claude", "Step 1")
            .unwrap();
        let first = store.activity(&id).last().unwrap().ts_ms;
        let mut c = b.clone();
        c.parts[0].edges[1].overhang = 0.9;
        std::thread::sleep(std::time::Duration::from_millis(2));
        store
            .save_as(Some(id.clone()), &c, "claude", "Step 2")
            .unwrap();
        let review = store.pending_review(&id).unwrap();
        assert_eq!((review.count, review.since_ts_ms), (2, first));
        assert_eq!(review.before.as_ref(), Some(&a), "after the user's revert");
        assert_eq!(review.actors, ["claude"]);
        assert_eq!(store.revert_to(&id, first, USER_ACTOR).unwrap(), a);
        assert_eq!(store.load(&id).unwrap(), a);
        // The user's revert closes the review.
        assert!(store.pending_review(&id).is_none());
        store.delete(&id).unwrap();
        assert!(store.activity(&id).is_empty());
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// ERR-001: the app saves its own edits without a log entry; reverting
    /// the AI step before them must not throw them away.
    #[test]
    fn reverting_an_ai_change_keeps_the_users_later_edits() {
        let store = temp_store("revert-keeps-user");
        let a = examples::find("hip").unwrap().project();
        let id = store.save_as(None, &a, USER_ACTOR, "create").unwrap();
        let mut ai = a.clone();
        ai.parts[0].edges[3].pitch_deg = 44.0;
        store
            .save_as(Some(id.clone()), &ai, "claude", "Steeper wall 4")
            .unwrap();
        let ts = store.activity(&id).last().unwrap().ts_ms;
        // The user's own correction, autosaved by the app (not logged).
        let mut mine = ai.clone();
        mine.parts[0].edges[0].overhang = 0.9;
        store.save(Some(id.clone()), &mine).unwrap();

        let mut want = a.clone();
        want.parts[0].edges[0].overhang = 0.9;
        assert_eq!(store.revert_last(&id, USER_ACTOR).unwrap(), want);
        assert_eq!(store.load(&id).unwrap(), want);

        // The same through "revert all".
        store
            .save_as(Some(id.clone()), &mine, USER_ACTOR, "again")
            .unwrap();
        let mut ai2 = mine.clone();
        ai2.parts[0].edges[3].pitch_deg = 44.0;
        std::thread::sleep(std::time::Duration::from_millis(2));
        store
            .save_as(Some(id.clone()), &ai2, "claude", "Steeper wall 4")
            .unwrap();
        let ts2 = store.activity(&id).last().unwrap().ts_ms;
        assert_ne!(ts, ts2);
        let mut mine2 = ai2.clone();
        mine2.parts[0].edges[1].overhang = 0.7;
        store.save(Some(id.clone()), &mine2).unwrap();
        let mut want2 = want.clone();
        want2.parts[0].edges[1].overhang = 0.7;
        assert_eq!(store.revert_to(&id, ts2, USER_ACTOR).unwrap(), want2);
        assert_eq!(store.load(&id).unwrap(), want2);
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// ERR-001: when the user changed the very value the AI step changed,
    /// revert refuses and the file stays as it is.
    #[test]
    fn revert_refuses_when_the_user_changed_the_same_value() {
        let store = temp_store("revert-conflict");
        let a = examples::find("hip").unwrap().project();
        let id = store.save_as(None, &a, USER_ACTOR, "create").unwrap();
        let mut ai = a.clone();
        ai.parts[0].edges[3].pitch_deg = 44.0;
        store
            .save_as(Some(id.clone()), &ai, "claude", "Steeper wall 4")
            .unwrap();
        let mut mine = ai.clone();
        mine.parts[0].edges[3].pitch_deg = 40.0;
        store.save(Some(id.clone()), &mine).unwrap();
        let n = store.activity(&id).len();
        let err = store.revert_last(&id, USER_ACTOR).unwrap_err();
        assert_eq!(err.code(), "revert_conflict");
        assert!(
            err.detail()
                .unwrap()
                .contains("parts[0].edges[3].pitch_deg")
        );
        assert_eq!(store.load(&id).unwrap(), mine);
        assert_eq!(store.activity(&id).len(), n, "nothing logged");
        let _ = fs::remove_dir_all(&store.dir);
    }
}
