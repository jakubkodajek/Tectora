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
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
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
/// Deleted jobs and removed attachments, one folder per deletion
/// (`.trash/<entry>/`), so a delete by mistake or by an AI client can be
/// undone ([`Store::restore`]).
pub const TRASH_DIR: &str = ".trash";
/// What was deleted, by whom and why, in each trash folder.
const TRASH_INFO: &str = "deleted.json";
/// Trash folders older than this are removed for good on the next delete.
pub const TRASH_KEEP_DAYS: u64 = 30;
/// Most files of one name a job keeps (`plan.pdf`, `plan (2).pdf`, …).
const MAX_SAME_NAME: usize = 10_000;
/// Largest file attached from disk, bytes (as in a job bundle).
pub const MAX_ATTACHMENT: u64 = 256 * 1024 * 1024;

/// A deleted job or a removed attachment, kept in the library's trash.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrashEntry {
    /// Folder name in `.trash`; pass it to [`Store::restore`].
    pub entry: String,
    /// The job (deleted, or the one the file was attached to).
    pub id: String,
    /// The job's name when it was deleted.
    pub name: String,
    /// The attachment's name when only a file was removed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    pub ts_ms: u64,
    /// `user` for the desktop app, otherwise the AI client's name.
    pub actor: String,
    /// One sentence on why it was deleted.
    pub summary: String,
}

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
    /// The roof took longer than the listing waits for ([`SUMMARY_BUDGET`]):
    /// `roof_area` and `lines` are empty while it is still calculated in the
    /// background; a later listing has them (ARCH-001).
    pub figures_pending: bool,
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
    /// Short English description, e.g. "Reading “Rodinný dům”" (logs and
    /// older apps); for a change, the agent's own summary.
    pub message: String,
    pub project_id: Option<String>,
    /// The agent saved `project_id` or asked to show it: the app may open it
    /// (follow mode). Reading or calculating a job does not set it.
    #[serde(default)]
    pub follow: bool,
    /// What the agent is doing, as a code the app translates
    /// ([`LIVE_KINDS`]); `changed` shows `message`, the agent's summary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// The job or file name the sentence names.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// The codes of [`LiveStatus::kind`] (the app has a sentence for each, in
/// every UI language: `ai.live.<kind>`).
pub const LIVE_KINDS: [&str; 18] = [
    "changed",
    "settings",
    "browsing",
    "reading",
    "calculating",
    "explaining",
    "comparing",
    "showing",
    "reverted",
    "reverted_changes",
    "copied",
    "deleted",
    "restored",
    "attached",
    "removed",
    "exported",
    "exported_sheet",
    "imported",
];

/// Summary prefixes of the change-log entries a revert writes; the app
/// reads them to show the sentence in the user's language.
pub const REVERTED_PREFIX: &str = "Reverted: ";
/// What follows a revert's sentence when the AI gave a reason: the entry
/// reads `Reverted: <summary> (reason: <reason>)` (the app's
/// `live.svelte.ts` splits it there).
pub const REVERT_REASON_OPEN: &str = " (reason: ";

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
    lines_of(&crate::openings::generate_project(project))
}

/// [`roof_lines`] of roofs already calculated.
fn lines_of(geos: &[roof::RoofGeometry]) -> Vec<[[f64; 2]; 2]> {
    geos.iter()
        .flat_map(|g| &g.lines)
        .filter(|l| l.kind != roof::LineKind::Eave)
        .map(|l| [[l.a[0], l.a[1]], [l.b[0], l.b[1]]])
        .collect()
}

/// A library card's roof area and plan lines, from one calculation
/// (ARCH-006).
fn card_figures(project: &Project) -> (f64, Vec<[[f64; 2]; 2]>) {
    let geos = crate::openings::generate_project(project);
    (
        crate::quantities::roof_totals_of(&geos, project).roof_area,
        lines_of(&geos),
    )
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

/// How long listing the library waits for one job's roof figures before it
/// lists the job without them (ARCH-001).
pub const SUMMARY_BUDGET: std::time::Duration = std::time::Duration::from_millis(500);

pub struct Store {
    pub dir: PathBuf,
    /// Summaries by id with the file stamps they were made from, so listing
    /// a large library does not recalculate every roof each time. Shared
    /// with the threads that finish slow roofs in the background.
    cache: Arc<Mutex<SummaryCache>>,
    /// How long listing waits for one job's roof figures.
    summary_budget: std::time::Duration,
}

#[derive(Default)]
struct SummaryCache {
    entries: HashMap<String, (Stamp, ProjectSummary)>,
    /// Jobs whose figures a background thread is calculating: at most one
    /// thread per job, however often the library is listed.
    running: std::collections::HashSet<String>,
}

fn lock_cache(cache: &Mutex<SummaryCache>) -> std::sync::MutexGuard<'_, SummaryCache> {
    cache.lock().unwrap_or_else(|e| e.into_inner())
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

/// Why work with a file other than the project file failed: an attachment,
/// a job bundle, an imported file. Like [`StoreError`] it has a stable
/// `code` the app translates; `detail` is English for the CLI, the MCP
/// server and the "Details" view.
///
/// Codes: those of [`StoreError`], `file_not_found`, `not_a_file` (a folder,
/// device or pipe), `too_large`, `unsupported_type`, `invalid_name`,
/// `out_of_range`, `too_many`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{detail}")]
pub struct FileError {
    pub code: &'static str,
    /// The job the error is about, if any.
    pub id: Option<String>,
    pub detail: String,
}

impl FileError {
    pub fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            id: None,
            detail: detail.into(),
        }
    }

    /// An operating system error on `path`, with the code the app explains.
    pub fn io(path: &Path, e: std::io::Error) -> Self {
        let code = match e.kind() {
            std::io::ErrorKind::NotFound => "file_not_found",
            _ => StoreError::io(None, std::io::Error::new(e.kind(), "")).code(),
        };
        Self::new(code, format!("{}: {e}", path.display()))
    }
}

impl From<StoreError> for FileError {
    fn from(e: StoreError) -> Self {
        FileError {
            code: e.code(),
            id: e.id().map(str::to_string),
            detail: e.to_string(),
        }
    }
}

impl From<FileError> for String {
    fn from(e: FileError) -> String {
        e.detail
    }
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
pub(crate) fn newer_format(text: &str) -> Option<u64> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let version = v.get("schema_version")?.as_u64()?;
    (version > u64::from(crate::model::SCHEMA_VERSION)).then_some(version)
}

#[cfg(test)]
thread_local! {
    /// Paths whose move fails in this test thread, as a file open in another
    /// program makes it fail on Windows.
    static FAIL_MOVE: std::cell::RefCell<Vec<PathBuf>> = const { std::cell::RefCell::new(Vec::new()) };
    /// Files synced to disk by this test thread.
    static SYNCED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// `fs::rename`, with a way for tests to make it fail.
fn move_path(from: &Path, to: &Path) -> std::io::Result<()> {
    #[cfg(test)]
    if FAIL_MOVE.with(|f| f.borrow().iter().any(|p| p == from)) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "the file is open in another program",
        ));
    }
    fs::rename(from, to)
}

/// Flushes `file`'s data to the disk (`fsync`, `F_FULLFSYNC` on macOS,
/// `FlushFileBuffers` on Windows), so a power cut cannot leave it empty
/// after a rename that already reached the disk (ERR-006).
pub(crate) fn sync_file(file: &fs::File) -> std::io::Result<()> {
    file.sync_all()?;
    #[cfg(test)]
    SYNCED.with(|s| s.set(s.get() + 1));
    Ok(())
}

/// Flushes the folder holding `path`, so a rename into it is on the disk
/// too. Only where a folder can be opened as a file (macOS, Linux); Windows
/// keeps the rename in its file system journal. Best effort: some file
/// systems refuse to sync a folder, and the file itself is already synced.
pub(crate) fn sync_parent(path: &Path) {
    #[cfg(unix)]
    if let Some(dir) = path.parent() {
        let dir = if dir.as_os_str().is_empty() {
            Path::new(".")
        } else {
            dir
        };
        if let Ok(d) = fs::File::open(dir) {
            let _ = d.sync_all();
        }
    }
    #[cfg(not(unix))]
    let _ = path;
}

/// Writes `contents` to `tmp` and syncs it, removing `tmp` again on failure.
pub(crate) fn write_synced(tmp: &Path, contents: &[u8]) -> std::io::Result<()> {
    let written = fs::File::create(tmp).and_then(|mut f| {
        f.write_all(contents)?;
        sync_file(&f)
    });
    if written.is_err() {
        let _ = fs::remove_file(tmp);
    }
    written
}

fn write_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = path.with_extension(format!("{}-{n}.tmp", std::process::id()));
    write_synced(&tmp, contents.as_bytes())?;
    fs::rename(&tmp, path).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })?;
    sync_parent(path);
    Ok(())
}

/// What [`Store::save_checked`] and [`Store::save_as_checked`] did: saved,
/// or found the file changed since the caller's base and left it alone.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CheckedSave {
    Saved {
        id: String,
    },
    /// Someone else saved the job after the base was read: `current` is the
    /// file now, to merge with and save again.
    Conflict {
        current: Box<Project>,
    },
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
            cache: Arc::default(),
            summary_budget: SUMMARY_BUDGET,
        }
    }

    /// The same store waiting at most `budget` for one job's roof figures
    /// when it lists the library.
    pub fn with_summary_budget(mut self, budget: std::time::Duration) -> Self {
        self.summary_budget = budget;
        self
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

    /// The library card of job `id`. Its roof figures are waited for at
    /// most the store's summary budget ([`SUMMARY_BUDGET`]); a roof that
    /// takes longer is finished in a background thread and the card comes
    /// back without them (`figures_pending`) until a later call (ARCH-001).
    pub fn summarize(&self, id: &str) -> Result<ProjectSummary, StoreError> {
        let stamp = self.stamp(id)?;
        if stamp[0].is_some()
            && let Some((s, summary)) = lock_cache(&self.cache).entries.get(id)
            && *s == stamp
        {
            return Ok(summary.clone());
        }
        let (mut summary, project) = self.summarize_uncached(id)?;
        // A broken design still shows up in the library (so it can be opened
        // and fixed or deleted), just without its roof preview.
        if project.check_limits().is_err() {
            lock_cache(&self.cache)
                .entries
                .insert(id.to_string(), (stamp, summary.clone()));
            return Ok(summary);
        }
        if !lock_cache(&self.cache).running.insert(id.to_string()) {
            // An older version of this job is still being calculated: the
            // next listing after it finishes calculates this one.
            summary.figures_pending = true;
            return Ok(summary);
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let (cache, job, at) = (Arc::clone(&self.cache), id.to_string(), stamp);
        let started = std::thread::Builder::new()
            .name(format!("summary {id}"))
            .spawn(move || {
                let figures = crate::model::guarded(|| card_figures(&project)).unwrap_or_default();
                // Under the lock, so `summarize` either sees the answer or
                // has left its pending card for this thread to fill in.
                let mut c = lock_cache(&cache);
                c.running.remove(&job);
                if let Some((s, card)) = c.entries.get_mut(&job)
                    && *s == at
                    && card.figures_pending
                {
                    (card.roof_area, card.lines) = figures.clone();
                    card.figures_pending = false;
                }
                let _ = tx.send(figures);
            });
        if started.is_err() {
            // No thread to be had: listed without its figures this time.
            lock_cache(&self.cache).running.remove(id);
            summary.figures_pending = true;
            return Ok(summary);
        }
        let figures = rx.recv_timeout(self.summary_budget).ok();
        let mut c = lock_cache(&self.cache);
        match figures.or_else(|| rx.try_recv().ok()) {
            Some((area, lines)) => (summary.roof_area, summary.lines) = (area, lines),
            None => summary.figures_pending = true,
        }
        c.entries.insert(id.to_string(), (stamp, summary.clone()));
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

    /// The card of job `id` without its roof figures, and the job.
    fn summarize_uncached(&self, id: &str) -> Result<(ProjectSummary, Project), StoreError> {
        let project = self.load(id)?;
        let summary = ProjectSummary {
            id: id.to_string(),
            name: project.name.clone(),
            notes: project.notes.clone(),
            job: project.job.clone(),
            modified_ms: modified_ms(&self.path(id)?),
            roof_area: 0.0,
            parts: project.parts.len(),
            outlines: outlines(&project),
            lines: Vec::new(),
            last_activity: self.activity(id).last().map(ActivityEntry::info),
            pitch_range: crate::library::pitch_range(&project),
            dormers: project.dormers.len(),
            files: self.attachments(id).len(),
            has_result: project.result_snapshot.is_some(),
            figures_pending: false,
        };
        Ok((summary, project))
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
        lock_cache(&self.cache)
            .entries
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

    /// Picks an unused id based on the project name: no job file, attached
    /// files, change log or review mark of that id exists, so a new job never
    /// adopts what an earlier job of the same name left behind (ERR-005).
    /// Another process can take the id before it is written: create jobs
    /// through [`Store::save`] or [`Store::save_as`] with `None`, which pick
    /// and write it under the library's creation lock.
    pub fn new_id(&self, name: &str) -> String {
        let mut base = slug(name);
        // Leave room for the "-N" suffix and keep clear of device names.
        base.truncate(MAX_ID_LEN - 8);
        if !valid_id(&base) {
            base.push_str("-navrh");
        }
        let mut id = base.clone();
        let mut n = 2;
        while self.id_used(&id) {
            id = format!("{base}-{n}");
            n += 1;
        }
        id
    }

    /// Whether anything in the library belongs to job `id`.
    fn id_used(&self, id: &str) -> bool {
        let activity = self.activity_dir();
        [
            self.dir.join(format!("{id}.{EXTENSION}")),
            self.dir.join(FILES_DIR).join(id),
            activity.join(format!("{id}.jsonl")),
            activity.join(format!("{id}.review.json")),
        ]
        .iter()
        // A path that cannot be checked counts as taken.
        .any(|p| p.try_exists().unwrap_or(true))
    }

    /// Takes the library's creation lock (`.activity/.new.lock`, a name no job id can have), held until
    /// the returned file is dropped: picking a new id and writing its file is
    /// one step for every process, so two jobs created with one name at the
    /// same moment get two ids (TEST-001). Best effort, like [`Store::lock`].
    fn creation_lock(&self) -> Option<fs::File> {
        fs::create_dir_all(self.activity_dir()).ok()?;
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.activity_dir().join(".new.lock"))
            .ok()?;
        file.lock().ok()?;
        Some(file)
    }

    /// Saves without recording activity (app autosave of its own edits).
    ///
    /// Refuses to replace a file saved by a newer Tectora: on a shared
    /// library folder an older app would otherwise write its stale copy over
    /// a colleague's newer one.
    pub fn save(&self, id: Option<String>, project: &Project) -> Result<String, StoreError> {
        let _creating = id.is_none().then(|| self.creation_lock());
        let id = id.unwrap_or_else(|| self.new_id(&project.name));
        let _lock = self.lock(&id);
        self.write(&id, project)?;
        Ok(id)
    }

    /// Saves the app's own edits (like [`Store::save`]) only if the file
    /// still holds `base`, the version the editor last read or wrote.
    /// Otherwise the file is left alone and its current content comes back
    /// as [`CheckedSave::Conflict`], for the editor to merge its edits onto
    /// and save again: an AI (MCP) save that landed in between is never
    /// overwritten. The check and the write happen under the job's lock
    /// file, which every save of every process (app, MCP server, CLI) takes.
    /// A missing or damaged file is written (repaired with the editor's
    /// copy, ERR-018); one saved by a newer Tectora is refused.
    pub fn save_checked(
        &self,
        id: &str,
        project: &Project,
        base: &Project,
    ) -> Result<CheckedSave, StoreError> {
        self.path(id)?;
        let _lock = self.lock(id);
        match self.load(id) {
            Ok(current) if current != *base => {
                return Ok(CheckedSave::Conflict {
                    current: Box::new(current),
                });
            }
            Ok(_) | Err(StoreError::NotFound(_) | StoreError::Unreadable { .. }) => {}
            Err(e) => return Err(e),
        }
        self.write(id, project)?;
        Ok(CheckedSave::Saved { id: id.to_string() })
    }

    /// Writes the file; the caller holds the job's lock.
    fn write(&self, id: &str, project: &Project) -> Result<(), StoreError> {
        let path = self.path(id)?;
        if let Some(version) = fs::read_to_string(&path)
            .ok()
            .and_then(|t| newer_format(&t))
        {
            return Err(StoreError::NewerFormat {
                id: id.to_string(),
                version,
                max: crate::model::SCHEMA_VERSION,
            });
        }
        let json = project.to_json().map_err(|e| StoreError::Unreadable {
            id: id.to_string(),
            detail: e.to_string(),
        })?;
        write_atomic(&path, &json).map_err(|e| StoreError::io(Some(id), e))
    }

    /// Takes the job's lock file (`.activity/<id>.lock`), held until the
    /// returned file is dropped, so that reading the current file and
    /// writing the next one is one step for every process saving the job.
    /// Best effort: where the folder cannot hold a lock file (read-only,
    /// a file system without locks), saving goes on without it and the
    /// write reports its own error.
    fn lock(&self, id: &str) -> Option<fs::File> {
        self.path(id).ok()?;
        fs::create_dir_all(self.activity_dir()).ok()?;
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.activity_dir().join(format!("{id}.lock")))
            .ok()?;
        file.lock().ok()?;
        Some(file)
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
        let id = match id {
            Some(id) => id,
            None => {
                let _creating = self.creation_lock();
                let id = self.new_id(&project.name);
                let _lock = self.lock(&id);
                self.log_and_write(&id, None, project, actor, summary)?;
                return Ok(id);
            }
        };
        match self.save_as_checked(&id, project, actor, summary, None)? {
            CheckedSave::Saved { id } => Ok(id),
            CheckedSave::Conflict { .. } => unreachable!("no base, no conflict"),
        }
    }

    /// [`Store::save_as`] of an existing job, only if the file still holds
    /// `base` (the version the change was made on); otherwise nothing is
    /// saved or logged and the current file comes back as
    /// [`CheckedSave::Conflict`]. MCP tools use it, so a user edit the app
    /// saved while the tool ran is not overwritten.
    pub fn save_as_checked(
        &self,
        id: &str,
        project: &Project,
        actor: &str,
        summary: &str,
        base: Option<&Project>,
    ) -> Result<CheckedSave, StoreError> {
        self.path(id)?;
        let _lock = self.lock(id);
        let before = match self.load(id) {
            // Refused before anything is logged (see `save`).
            Err(e @ StoreError::NewerFormat { .. }) => return Err(e),
            r => r.ok(),
        };
        if let (Some(base), Some(current)) = (base, before.as_ref())
            && current != base
        {
            return Ok(CheckedSave::Conflict {
                current: Box::new(current.clone()),
            });
        }
        if before.as_ref() != Some(project) {
            self.log_and_write(id, before, project, actor, summary)?;
        }
        Ok(CheckedSave::Saved { id: id.to_string() })
    }

    /// Logs the change and writes the file; the caller holds the job's lock.
    fn log_and_write(
        &self,
        id: &str,
        before: Option<Project>,
        project: &Project,
        actor: &str,
        summary: &str,
    ) -> Result<(), StoreError> {
        // A newer file is refused before anything is logged.
        if let Some(version) = fs::read_to_string(self.path(id)?)
            .ok()
            .and_then(|t| newer_format(&t))
        {
            return Err(StoreError::NewerFormat {
                id: id.to_string(),
                version,
                max: crate::model::SCHEMA_VERSION,
            });
        }
        // Log first: the app reacts to the project file and then reads the log.
        self.append_activity(
            id,
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
        self.write(id, project)
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
            let cut = entries.len() - ACTIVITY_KEEP;
            let merged;
            if start < cut {
                // That entry then stands for the changes dropped after it
                // too: it saved what the last dropped one saved, so "Revert
                // all" undoes them with it and keeps the user's unlogged
                // edits in between (ERR-001).
                let mut first = entries[start].clone();
                if let Some(after) = entries[cut].after.clone() {
                    first.changes = first
                        .before
                        .as_ref()
                        .map(|b| diff(b, &after))
                        .unwrap_or_default();
                    first.after = Some(after);
                }
                merged = first;
                keep[0] = &merged;
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
        // A torn last line (no newline) would swallow this entry too.
        let torn = fs::read(&path)
            .ok()
            .is_some_and(|b| b.last().is_some_and(|&c| c != b'\n'));
        if torn {
            writeln!(f).map_err(io)?;
        }
        writeln!(f, "{line}").map_err(io)?;
        sync_file(&f).map_err(io)
    }

    /// Recorded changes of a project, oldest first.
    pub fn activity(&self, id: &str) -> Vec<ActivityEntry> {
        let Ok(path) = self.activity_path(id) else {
            return Vec::new();
        };
        // Read bytes, not a string: one torn or invalid line (an interrupted
        // write) must cost that line only, not the whole history (ERR-005).
        let Ok(bytes) = fs::read(path) else {
            return Vec::new();
        };
        bytes
            .split(|&b| b == b'\n')
            .filter_map(|l| serde_json::from_slice(l).ok())
            .collect()
    }

    /// Undoes the latest recorded change, keeping later unlogged edits.
    /// `reason` (an AI client's summary) is logged after what was reverted
    /// ([`REVERT_REASON_OPEN`]).
    pub fn revert_last(
        &self,
        id: &str,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<Project, StoreError> {
        let log = self.activity(id);
        let last = log
            .last()
            .ok_or_else(|| StoreError::NothingToRevert(id.to_string()))?;
        self.revert_from(
            id,
            &log,
            log.len() - 1,
            actor,
            with_reason(reason, format!("{REVERTED_PREFIX}{}", last.summary)),
        )
    }

    /// Restores the version from before the change recorded at `ts_ms`,
    /// undoing it and every later change in one step.
    pub fn revert_to(
        &self,
        id: &str,
        ts_ms: u64,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<Project, StoreError> {
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
            format!("{REVERTED_PREFIX}{}", log[pos].summary)
        } else {
            format!("Reverted {undone} changes since: {}", log[pos].summary)
        };
        self.revert_from(id, &log, pos, actor, with_reason(reason, summary))
    }

    /// Undoes `log[pos..]`, latest first: the inverse of each change is
    /// merged onto the current file with the version it saved as the base.
    /// The app autosaves the user's own edits without a log entry, so the
    /// file can be newer than the last entry's result, and an edit can lie
    /// between two entries; such edits are kept (ERR-001). Refuses, with
    /// nothing written, when one of them touches a value a reverted change
    /// set.
    fn revert_from(
        &self,
        id: &str,
        log: &[ActivityEntry],
        pos: usize,
        actor: &str,
        summary: String,
    ) -> Result<Project, StoreError> {
        // The file is read, merged and written back only if nobody saved in
        // between (the app's autosave); otherwise the merge runs again on
        // the newer file.
        for _ in 0..4 {
            if let Some(p) = self.revert_once(id, log, pos, actor, &summary)? {
                return Ok(p);
            }
        }
        Err(StoreError::RevertConflict {
            id: id.to_string(),
            detail: "the job kept changing while it was reverted".into(),
        })
    }

    /// One attempt of [`Store::revert_from`]: `None` when the file changed
    /// between reading and writing it.
    fn revert_once(
        &self,
        id: &str,
        log: &[ActivityEntry],
        pos: usize,
        actor: &str,
        summary: &str,
    ) -> Result<Option<Project>, StoreError> {
        let value = |p: &Project| {
            serde_json::to_value(p).map_err(|e| StoreError::Unreadable {
                id: id.to_string(),
                detail: e.to_string(),
            })
        };
        let mut current = match self.load(id) {
            Ok(p) => Some(p),
            // A file newer than this Tectora is neither read nor written.
            Err(e @ StoreError::NewerFormat { .. }) => return Err(e),
            // A damaged file: the version before the change is restored.
            Err(_) => None,
        };
        let base = current.clone();
        let mut target = None;
        for entry in log[pos..].iter().rev() {
            let before = entry
                .before
                .clone()
                .ok_or_else(|| StoreError::RevertCreated(id.to_string()))?;
            let undone = match (entry.after.as_ref(), current.as_ref()) {
                (Some(after), Some(cur)) if cur != after => {
                    let merged =
                        crate::merge::merge3(&value(after)?, &value(cur)?, &value(&before)?)
                            .map_err(|paths| StoreError::RevertConflict {
                                id: id.to_string(),
                                detail: paths.join(", "),
                            })?;
                    serde_json::from_value(merged).map_err(|e| StoreError::Unreadable {
                        id: id.to_string(),
                        detail: e.to_string(),
                    })?
                }
                // Unchanged since, written by Tectora 0.5 or older (no
                // `after`), or no readable file: the version before it.
                _ => before,
            };
            current = Some(undone.clone());
            target = Some(undone);
        }
        let target = target.ok_or_else(|| StoreError::NothingToRevert(id.to_string()))?;
        match self.save_as_checked(id, &target, actor, summary, base.as_ref())? {
            CheckedSave::Saved { .. } => Ok(Some(target)),
            CheckedSave::Conflict { .. } => Ok(None),
        }
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

    /// Moves job `id` with its history and attached files to the library's
    /// trash (`.trash/<entry>/`), noting who deleted it and why, so it can
    /// be restored ([`Store::restore`]). Trash older than
    /// [`TRASH_KEEP_DAYS`] is removed for good at the same time.
    pub fn delete(&self, id: &str, actor: &str, summary: &str) -> Result<TrashEntry, StoreError> {
        let path = self.path(id)?;
        if !path.is_file() {
            return Err(StoreError::NotFound(id.to_string()));
        }
        // A damaged job can be deleted too, it just has no name to show.
        let name = self.load(id).map(|p| p.name).unwrap_or_default();
        let io = |e| StoreError::io(Some(id), e);
        // No save of this job lands while it is moved.
        let _lock = self.lock(id);
        let (entry, folder) = self.new_trash_folder(id).map_err(io)?;
        // Everything or nothing: the attached files first (on Windows a
        // drawing open in a viewer keeps its folder from moving), the
        // project file last. A part left behind would show up on the next
        // job of the same name, or be missing when the job is restored
        // (ERR-005).
        let moves = [
            (self.files_dir(id)?, folder.join("files")),
            (self.activity_path(id)?, folder.join("activity.jsonl")),
            (self.review_path(id)?, folder.join("review.json")),
            (path, folder.join(format!("{id}.{EXTENSION}"))),
        ];
        let info = TrashEntry {
            entry,
            id: id.to_string(),
            name,
            file: None,
            ts_ms: now_ms(),
            actor: actor.to_string(),
            summary: summary.to_string(),
        };
        // The note first: a job moved to the trash can always be restored.
        let noted = self.write_trash_info(&folder, &info);
        if let Err(e) = noted.and_then(|()| move_all(&moves)) {
            let _ = fs::remove_file(folder.join(TRASH_INFO));
            // Only an empty folder goes: whatever could not be moved back
            // stays there rather than being lost.
            let _ = fs::remove_dir(&folder);
            return Err(io(e));
        }
        self.prune_trash();
        Ok(info)
    }

    /// Removes job `id`, its history and files for good (a half-imported
    /// bundle that never was the user's job).
    pub(crate) fn purge(&self, id: &str) -> Result<(), StoreError> {
        fs::remove_file(self.path(id)?).map_err(|e| StoreError::io(Some(id), e))?;
        let _ = fs::remove_file(self.activity_path(id)?);
        let _ = fs::remove_file(self.review_path(id)?);
        let _ = fs::remove_dir_all(self.files_dir(id)?);
        // Its lock file too (ARCH-008). Where another process still holds
        // it (Windows), it stays and the next job of that id reuses it.
        let _ = fs::remove_file(self.activity_dir().join(format!("{id}.lock")));
        Ok(())
    }

    pub fn trash_dir(&self) -> PathBuf {
        self.dir.join(TRASH_DIR)
    }

    /// A new, empty folder in the trash for a deletion from job `id`.
    fn new_trash_folder(&self, id: &str) -> std::io::Result<(String, PathBuf)> {
        let base = format!("{}-{id}", now_ms());
        for n in 1..1000 {
            let entry = if n == 1 {
                base.clone()
            } else {
                format!("{base}-{n}")
            };
            let folder = self.trash_dir().join(&entry);
            fs::create_dir_all(self.trash_dir())?;
            match fs::create_dir(&folder) {
                Ok(()) => return Ok((entry, folder)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
        Err(std::io::Error::other("too many deletions at once"))
    }

    fn write_trash_info(&self, folder: &Path, info: &TrashEntry) -> std::io::Result<()> {
        let text = serde_json::to_string_pretty(info).map_err(std::io::Error::other)?;
        write_atomic(&folder.join(TRASH_INFO), &text)
    }

    /// The trash folder `entry`, if it is one (a plain name inside `.trash`).
    fn trash_folder(&self, entry: &str) -> Option<(PathBuf, TrashEntry)> {
        if !valid_file_name(entry) {
            return None;
        }
        let folder = self.trash_dir().join(entry);
        let text = fs::read_to_string(folder.join(TRASH_INFO)).ok()?;
        let mut info: TrashEntry = serde_json::from_str(&text).ok()?;
        // The folder name is what restores it, whatever the file says.
        info.entry = entry.to_string();
        valid_id(&info.id).then_some((folder, info))
    }

    /// Deleted jobs and removed files in the trash, newest first.
    pub fn trash(&self) -> Vec<TrashEntry> {
        let mut out: Vec<TrashEntry> = fs::read_dir(self.trash_dir())
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .filter_map(|name| self.trash_folder(&name).map(|(_, info)| info))
            .collect();
        out.sort_by(|a, b| b.ts_ms.cmp(&a.ts_ms).then(b.entry.cmp(&a.entry)));
        out
    }

    /// Removes trash folders older than [`TRASH_KEEP_DAYS`] for good.
    fn prune_trash(&self) {
        let keep_ms = TRASH_KEEP_DAYS * 24 * 3600 * 1000;
        let now = now_ms();
        for e in self.trash() {
            if now.saturating_sub(e.ts_ms) > keep_ms {
                let _ = fs::remove_dir_all(self.trash_dir().join(&e.entry));
            }
        }
    }

    /// Brings a deleted job (with its history and files) or a removed file
    /// back from the trash; returns the job's id. A job whose id was taken
    /// in the meantime comes back under a new id, a file under a free name.
    pub fn restore(&self, entry: &str) -> Result<String, StoreError> {
        let (folder, info) = self
            .trash_folder(entry)
            .ok_or_else(|| StoreError::NotFound(entry.to_string()))?;
        let io = |e| StoreError::io(Some(&info.id), e);
        if let Some(name) = &info.file {
            if !self.exists(&info.id) {
                return Err(StoreError::NotFound(info.id.clone()));
            }
            let src = folder.join("files").join(name);
            // The free name is taken by an empty file the move replaces.
            let (reserved, used) =
                self.reserve_attachment_name(&info.id, name)
                    .map_err(|e| StoreError::Io {
                        id: Some(info.id.clone()),
                        kind: IoKind::Other,
                        detail: e.detail,
                    })?;
            drop(reserved);
            let dest = self.files_dir(&info.id)?.join(&used);
            if let Err(e) = move_path(&src, &dest) {
                let _ = fs::remove_file(&dest);
                return Err(io(e));
            }
            let _ = fs::remove_dir_all(&folder);
            return Ok(info.id);
        }
        if !folder.join(format!("{}.{EXTENSION}", info.id)).is_file() {
            return Err(StoreError::NotFound(entry.to_string()));
        }
        // A new id is picked and taken in one step (see `save`).
        let _creating = self.creation_lock();
        let id = if self.id_used(&info.id) {
            self.new_id(if info.name.is_empty() {
                &info.id
            } else {
                &info.name
            })
        } else {
            info.id.clone()
        };
        let _lock = self.lock(&id);
        fs::create_dir_all(self.activity_dir()).map_err(io)?;
        fs::create_dir_all(self.dir.join(FILES_DIR)).map_err(io)?;
        // Everything or nothing, the project file last: the app's watcher
        // sees the job when it is whole.
        let moves = [
            (folder.join("files"), self.files_dir(&id)?),
            (folder.join("activity.jsonl"), self.activity_path(&id)?),
            (folder.join("review.json"), self.review_path(&id)?),
            (
                folder.join(format!("{}.{EXTENSION}", info.id)),
                self.path(&id)?,
            ),
        ];
        move_all(&moves).map_err(io)?;
        let _ = fs::remove_dir_all(&folder);
        Ok(id)
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

    pub fn attachment_path(&self, id: &str, name: &str) -> Result<PathBuf, FileError> {
        if !valid_file_name(name) {
            return Err(FileError::new(
                "invalid_name",
                format!("invalid file name: {name}"),
            ));
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
    pub fn add_attachment(&self, id: &str, name: &str, bytes: &[u8]) -> Result<String, FileError> {
        if !self.exists(id) {
            return Err(StoreError::NotFound(id.to_string()).into());
        }
        let (mut file, used) = self.reserve_attachment_name(id, name)?;
        let path = self.attachment_path(id, &used)?;
        if let Err(e) = file.write_all(bytes).and_then(|()| sync_file(&file)) {
            drop(file);
            let _ = fs::remove_file(&path);
            return Err(FileError::io(&path, e));
        }
        Ok(used)
    }

    /// Creates the file `name`, or `name (2)` etc. when job `id` has a file
    /// of that name, and returns it open for writing with the name used.
    /// The file is created only if it does not exist (`create_new`), so two
    /// files of one name added at the same moment, by this process or
    /// another, both keep their bytes (TEST-010).
    fn reserve_attachment_name(
        &self,
        id: &str,
        name: &str,
    ) -> Result<(fs::File, String), FileError> {
        let name = name.trim();
        self.attachment_path(id, name)?;
        let dir = self.files_dir(id)?;
        fs::create_dir_all(&dir).map_err(|e| FileError::io(&dir, e))?;
        let (stem, ext) = match name.rfind('.') {
            Some(i) if i > 0 => (&name[..i], &name[i..]),
            _ => (name, ""),
        };
        for n in 1..=MAX_SAME_NAME {
            let used = if n == 1 {
                name.to_string()
            } else {
                format!("{stem} ({n}){ext}")
            };
            let path = self.attachment_path(id, &used)?;
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(file) => return Ok((file, used)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(FileError::io(&path, e)),
            }
        }
        Err(FileError::new(
            "too_many",
            format!("{name}: more than {MAX_SAME_NAME} files of this name"),
        ))
    }

    /// Copies a file from disk into job `id`'s attachments. Only a regular
    /// file up to [`MAX_ATTACHMENT`] bytes: a folder, a device (`/dev/zero`,
    /// which never ends) or a pipe (`/dev/stdin`, which blocks until the
    /// other end closes) is refused before it is opened (ARCH-001).
    pub fn attach_file(&self, id: &str, src: &Path) -> Result<String, FileError> {
        let name = src.file_name().and_then(|n| n.to_str()).ok_or_else(|| {
            FileError::new("not_a_file", format!("{}: not a file", src.display()))
        })?;
        let not_a_file = || {
            FileError::new(
                "not_a_file",
                format!(
                    "{}: not a regular file (a folder, device or pipe)",
                    src.display()
                ),
            )
        };
        let too_large = || {
            FileError::new(
                "too_large",
                format!(
                    "{}: larger than {} MB, the most a job can carry",
                    src.display(),
                    MAX_ATTACHMENT >> 20
                ),
            )
        };
        let meta = fs::metadata(src).map_err(|e| FileError::io(src, e))?;
        if !meta.is_file() {
            return Err(not_a_file());
        }
        if meta.len() > MAX_ATTACHMENT {
            return Err(too_large());
        }
        let file = fs::File::open(src).map_err(|e| FileError::io(src, e))?;
        // The path may have been replaced between the check and the open.
        if !file.metadata().is_ok_and(|m| m.is_file()) {
            return Err(not_a_file());
        }
        let mut bytes = Vec::new();
        std::io::Read::take(file, MAX_ATTACHMENT + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| FileError::io(src, e))?;
        if bytes.len() as u64 > MAX_ATTACHMENT {
            return Err(too_large());
        }
        self.add_attachment(id, name, &bytes)
    }

    /// Moves attachment `name` of job `id` to the library's trash (see
    /// [`Store::delete`]).
    pub fn remove_attachment(
        &self,
        id: &str,
        name: &str,
        actor: &str,
        summary: &str,
    ) -> Result<TrashEntry, FileError> {
        let path = self.attachment_path(id, name)?;
        if !path.is_file() {
            return Err(FileError::io(
                &path,
                std::io::Error::from(std::io::ErrorKind::NotFound),
            ));
        }
        let job = self.load(id).map(|p| p.name).unwrap_or_default();
        let (entry, folder) = self
            .new_trash_folder(id)
            .map_err(|e| FileError::io(&self.trash_dir(), e))?;
        let dest = folder.join("files");
        let moved = fs::create_dir_all(&dest).and_then(|()| fs::rename(&path, dest.join(name)));
        if let Err(e) = moved {
            let _ = fs::remove_dir_all(&folder);
            return Err(FileError::io(&path, e));
        }
        let info = TrashEntry {
            entry,
            id: id.to_string(),
            name: job,
            file: Some(name.to_string()),
            ts_ms: now_ms(),
            actor: actor.to_string(),
            summary: summary.to_string(),
        };
        self.write_trash_info(&folder, &info)
            .map_err(|e| FileError::io(&folder, e))?;
        self.prune_trash();
        Ok(info)
    }

    /// Copies all attachments of job `from` to job `to`.
    pub fn copy_attachments(&self, from: &str, to: &str) -> Result<(), FileError> {
        for a in self.attachments(from) {
            let path = self.attachment_path(from, &a.name)?;
            let bytes = fs::read(&path).map_err(|e| FileError::io(&path, e))?;
            self.add_attachment(to, &a.name, &bytes)?;
        }
        Ok(())
    }

    pub fn set_live(&self, status: &LiveStatus) {
        if fs::create_dir_all(self.activity_dir()).is_ok()
            && let Ok(text) = serde_json::to_string(status)
        {
            let _ = write_atomic(&self.activity_dir().join(LIVE_FILE), &text);
        }
    }

    pub fn live(&self) -> Option<LiveStatus> {
        let text = fs::read_to_string(self.activity_dir().join(LIVE_FILE)).ok()?;
        serde_json::from_str(&text).ok()
    }
}

/// Moves each existing `from` to its `to`, in order. When one fails, the
/// ones already moved go back and the error is returned, so nothing is left
/// half moved.
fn move_all(moves: &[(PathBuf, PathBuf)]) -> std::io::Result<()> {
    let mut done: Vec<&(PathBuf, PathBuf)> = Vec::new();
    for m in moves {
        let (from, to) = m;
        if !from.try_exists()? {
            continue;
        }
        if let Err(e) = move_path(from, to) {
            for (from, to) in done.into_iter().rev() {
                let _ = move_path(to, from);
            }
            return Err(e);
        }
        done.push(m);
    }
    Ok(())
}

/// `what (reason: …)` when a reason is given, else `what`: the sentence
/// the core wrote stays first, so the app recognises and translates it.
fn with_reason(reason: Option<&str>, what: String) -> String {
    match reason.map(str::trim).filter(|r| !r.is_empty()) {
        Some(r) => format!("{what}{REVERT_REASON_OPEN}{r})"),
        None => what,
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
        assert_eq!(
            store.delete("nope", USER_ACTOR, "test").unwrap_err().code(),
            "not_found"
        );
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
        assert_eq!(
            store.revert_to(&id, oldest, USER_ACTOR, None).unwrap(),
            mine
        );
        assert_eq!(store.load(&id).unwrap(), mine);
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// The project as the editor sends it back as the base of a checked
    /// save: through JSON, as the UI holds it.
    fn through_json(p: &Project) -> Project {
        serde_json::from_str(&serde_json::to_string(p).unwrap()).unwrap()
    }

    #[test]
    fn a_checked_save_never_overwrites_an_outside_save_made_after_its_base() {
        // An MCP save landing between the editor's read and its write was
        // overwritten by the editor's copy (recheck-ce84f6d, new problem 1).
        let store = temp_store("checked");
        let base = examples::find("hip").unwrap().project();
        let id = store.save(None, &base).unwrap();
        let base = through_json(&store.load(&id).unwrap());

        // The AI saves wall 3 at 41 degrees.
        let mut ai = base.clone();
        ai.parts[0].edges[2].pitch_deg = 41.0;
        store
            .save_as(Some(id.clone()), &ai, "claude", "AI B")
            .unwrap();

        // The editor, still on the old base, saves its overhang edit.
        let mut mine = base.clone();
        mine.parts[0].edges[0].overhang = 0.77;
        match store.save_checked(&id, &mine, &base).unwrap() {
            CheckedSave::Conflict { current } => assert_eq!(*current, ai),
            other => panic!("expected a conflict, got {other:?}"),
        }
        assert_eq!(store.load(&id).unwrap(), ai, "the AI's save is kept");

        // Against the current file as base, the save goes through.
        let current = through_json(&ai);
        let mut merged = mine.clone();
        merged.parts[0].edges[2].pitch_deg = 41.0;
        assert_eq!(
            store.save_checked(&id, &merged, &current).unwrap(),
            CheckedSave::Saved { id: id.clone() }
        );
        assert_eq!(store.load(&id).unwrap(), merged);
        // The editor's own save is still not logged as a change.
        assert_eq!(store.activity(&id).len(), 1);
        let _ = fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn a_checked_save_repairs_a_missing_or_damaged_file_and_refuses_a_newer_one() {
        let store = temp_store("checked-repair");
        let p = examples::find("gable").unwrap().project();
        let id = store.save(None, &p).unwrap();
        let mut mine = p.clone();
        mine.parts[0].edges[0].overhang = 0.9;
        // Deleted outside the app: written again (as an unchecked save is).
        fs::remove_file(store.path(&id).unwrap()).unwrap();
        assert!(matches!(
            store.save_checked(&id, &mine, &p).unwrap(),
            CheckedSave::Saved { .. }
        ));
        assert_eq!(store.load(&id).unwrap(), mine);
        // Damaged: repaired with the editor's good copy (ERR-018).
        fs::write(store.path(&id).unwrap(), "{ not json").unwrap();
        assert!(matches!(
            store.save_checked(&id, &mine, &p).unwrap(),
            CheckedSave::Saved { .. }
        ));
        assert_eq!(store.load(&id).unwrap(), mine);
        // Saved by a newer Tectora: refused, file kept.
        let newer = format!(
            "{{\"schema_version\": {}, \"name\": \"x\"}}",
            crate::model::SCHEMA_VERSION + 1
        );
        fs::write(store.path(&id).unwrap(), &newer).unwrap();
        assert_eq!(
            store.save_checked(&id, &mine, &p).unwrap_err().code(),
            "newer_format"
        );
        assert_eq!(fs::read_to_string(store.path(&id).unwrap()).unwrap(), newer);
        assert_eq!(
            store.save_checked("../x", &mine, &p).unwrap_err().code(),
            "invalid_id"
        );
        let _ = fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn a_logged_save_checked_against_its_base_refuses_an_edit_saved_meanwhile() {
        // MCP side: the app saved the user's edit after the tool loaded the job.
        let store = temp_store("checked-as");
        let base = examples::find("hip").unwrap().project();
        let id = store.save(None, &base).unwrap();
        let mut user = base.clone();
        user.parts[0].edges[0].overhang = 0.9;
        store.save(Some(id.clone()), &user).unwrap();
        let mut ai = base.clone();
        ai.parts[0].edges[2].pitch_deg = 41.0;
        match store
            .save_as_checked(&id, &ai, "claude", "AI", Some(&base))
            .unwrap()
        {
            CheckedSave::Conflict { current } => assert_eq!(*current, user),
            other => panic!("expected a conflict, got {other:?}"),
        }
        assert_eq!(store.load(&id).unwrap(), user);
        assert!(store.activity(&id).is_empty(), "nothing logged");
        // Against the current file it saves and logs.
        let mut merged = user.clone();
        merged.parts[0].edges[2].pitch_deg = 41.0;
        assert_eq!(
            store
                .save_as_checked(&id, &merged, "claude", "AI", Some(&user))
                .unwrap(),
            CheckedSave::Saved { id: id.clone() }
        );
        assert_eq!(store.load(&id).unwrap(), merged);
        assert_eq!(store.activity(&id)[0].before.as_ref(), Some(&user));
        let _ = fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn saves_of_one_job_wait_for_each_other() {
        // The check and the write happen under the job's lock file, which
        // the app and the MCP server (separate processes) both take.
        let store = temp_store("checked-lock");
        let base = examples::find("gable").unwrap().project();
        let id = store.save(None, &base).unwrap();
        let held = store.lock(&id).expect("lock file");
        let (tx, rx) = std::sync::mpsc::channel();
        let (dir, id2, b2) = (store.dir.clone(), id.clone(), base.clone());
        let t = std::thread::spawn(move || {
            let other = Store::new(dir).unwrap();
            let mut mine = b2.clone();
            mine.parts[0].edges[0].overhang = 0.9;
            let r = other.save_checked(&id2, &mine, &b2).unwrap();
            tx.send(()).unwrap();
            r
        });
        assert!(
            rx.recv_timeout(std::time::Duration::from_millis(300))
                .is_err(),
            "the save waits while the lock is held"
        );
        // Meanwhile the holder writes: the waiting save must see it.
        let mut ai = base.clone();
        ai.parts[0].edges[2].pitch_deg = 41.0;
        store.write(&id, &ai).unwrap();
        drop(held);
        assert!(matches!(t.join().unwrap(), CheckedSave::Conflict { .. }));
        assert_eq!(store.load(&id).unwrap(), ai);
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
        store.delete(&id, USER_ACTOR, "test").unwrap();
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

        store.delete(&id, USER_ACTOR, "test").unwrap();
        assert_eq!(store.reviewed_at(&id), 0);
        let _ = fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn a_torn_line_in_the_change_log_costs_only_that_line() {
        // ERR-005: an interrupted write left invalid UTF-8 in the log; the
        // whole history, the AI review and Revert disappeared, and later
        // entries were glued to the torn line.
        let store = temp_store("torn-log");
        let a = examples::find("hip").unwrap().project();
        let id = store.save_as(None, &a, "claude", "Created").unwrap();
        let mut f = fs::OpenOptions::new()
            .append(true)
            .open(store.activity_path(&id).unwrap())
            .unwrap();
        f.write_all(b"{\"P\xc5").unwrap();
        drop(f);
        assert_eq!(store.activity(&id).len(), 1, "the good entry stays");
        let mut b = a.clone();
        b.parts[0].edges[0].pitch_deg = 45.0;
        store
            .save_as(Some(id.clone()), &b, "claude", "Steeper")
            .unwrap();
        let log = store.activity(&id);
        assert_eq!(log.len(), 2, "the entry after the torn line is kept");
        assert_eq!(log[1].summary, "Steeper");
        assert!(store.pending_review(&id).is_some());
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

        assert_eq!(store.revert_last(&id, USER_ACTOR, None).unwrap(), a);
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
        assert_eq!(store.revert_to(&id, first, USER_ACTOR, None).unwrap(), a);
        assert_eq!(store.load(&id).unwrap(), a);
        // The user's revert closes the review.
        assert!(store.pending_review(&id).is_none());
        store.delete(&id, USER_ACTOR, "test").unwrap();
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
        assert_eq!(store.revert_last(&id, USER_ACTOR, None).unwrap(), want);
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
        assert_eq!(store.revert_to(&id, ts2, USER_ACTOR, None).unwrap(), want2);
        assert_eq!(store.load(&id).unwrap(), want2);
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// ERR-001: "Revert all" over several AI steps keeps an edit the user
    /// made between them (the app autosaves it without a log entry), and
    /// refuses when that edit is in a value one of the steps set.
    #[test]
    fn revert_all_keeps_the_users_edit_between_two_ai_steps() {
        let store = temp_store("revert-all-between");
        let a = examples::find("hip").unwrap().project();
        let id = store.save_as(None, &a, USER_ACTOR, "create").unwrap();
        let mut ai = a.clone();
        ai.parts[0].edges[3].pitch_deg = 44.0;
        store
            .save_as(Some(id.clone()), &ai, "claude", "Steeper wall 4")
            .unwrap();
        let first = store.activity(&id).last().unwrap().ts_ms;
        let mut mine = ai.clone();
        mine.parts[0].edges[0].overhang = 0.9;
        store.save(Some(id.clone()), &mine).unwrap();
        let mut ai2 = mine.clone();
        ai2.parts[0].edges[2].pitch_deg = 41.0;
        std::thread::sleep(std::time::Duration::from_millis(2));
        store
            .save_as(Some(id.clone()), &ai2, "claude", "Steeper wall 3")
            .unwrap();

        let mut want = a.clone();
        want.parts[0].edges[0].overhang = 0.9;
        assert_eq!(store.revert_to(&id, first, USER_ACTOR, None).unwrap(), want);
        assert_eq!(store.load(&id).unwrap(), want);

        // The user's edit between the steps is in the value the first step
        // set: refused, nothing changed or logged.
        let mut ai = want.clone();
        ai.parts[0].edges[3].pitch_deg = 44.0;
        std::thread::sleep(std::time::Duration::from_millis(2));
        store
            .save_as(Some(id.clone()), &ai, "claude", "Steeper wall 4")
            .unwrap();
        let first = store.activity(&id).last().unwrap().ts_ms;
        let mut mine = ai.clone();
        mine.parts[0].edges[3].pitch_deg = 40.0;
        store.save(Some(id.clone()), &mine).unwrap();
        let mut ai2 = mine.clone();
        ai2.parts[0].edges[2].pitch_deg = 41.0;
        std::thread::sleep(std::time::Duration::from_millis(2));
        store
            .save_as(Some(id.clone()), &ai2, "claude", "Steeper wall 3")
            .unwrap();
        let n = store.activity(&id).len();
        let err = store.revert_to(&id, first, USER_ACTOR, None).unwrap_err();
        assert_eq!(err.code(), "revert_conflict");
        assert_eq!(store.load(&id).unwrap(), ai2);
        assert_eq!(store.activity(&id).len(), n, "nothing logged");
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// UX-002: a revert's entry starts with the fixed sentence the app
    /// translates, and the AI's reason follows in a fixed form.
    #[test]
    fn a_revert_with_a_reason_starts_with_the_fixed_sentence() {
        let store = temp_store("revert-reason");
        let a = examples::find("hip").unwrap().project();
        let id = store.save_as(None, &a, USER_ACTOR, "create").unwrap();
        let mut ai = a.clone();
        ai.parts[0].edges[3].pitch_deg = 44.0;
        // Distinct times: `revert_to` finds the entry by its time.
        std::thread::sleep(std::time::Duration::from_millis(2));
        store
            .save_as(Some(id.clone()), &ai, "claude", "Steeper wall 4")
            .unwrap();
        // Distinct times: `revert_to` finds the entry by its time.
        std::thread::sleep(std::time::Duration::from_millis(2));
        store.revert_last(&id, "claude", Some(" undo ")).unwrap();
        assert_eq!(
            store.activity(&id).last().unwrap().summary,
            format!("{REVERTED_PREFIX}Steeper wall 4{REVERT_REASON_OPEN}undo)")
        );
        // Distinct times: `revert_to` finds the entry by its time.
        std::thread::sleep(std::time::Duration::from_millis(2));
        store
            .save_as(Some(id.clone()), &ai, "claude", "Steeper wall 4")
            .unwrap();
        let ts = store.activity(&id)[1].ts_ms;
        store.revert_to(&id, ts, "claude", Some("back")).unwrap();
        let s = store.activity(&id).last().unwrap().summary.clone();
        assert!(
            s.starts_with("Reverted 3 changes since: Steeper wall 4")
                && s.ends_with(" (reason: back)"),
            "{s}"
        );
        // Without a reason nothing is added.
        // Distinct times: `revert_to` finds the entry by its time.
        std::thread::sleep(std::time::Duration::from_millis(2));
        store
            .save_as(Some(id.clone()), &ai, "claude", "Again")
            .unwrap();
        store.revert_last(&id, "claude", Some("  ")).unwrap();
        assert_eq!(
            store.activity(&id).last().unwrap().summary,
            "Reverted: Again"
        );
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
        // Distinct times: `revert_to` finds the entry by its time.
        std::thread::sleep(std::time::Duration::from_millis(2));
        store
            .save_as(Some(id.clone()), &ai, "claude", "Steeper wall 4")
            .unwrap();
        let mut mine = ai.clone();
        mine.parts[0].edges[3].pitch_deg = 40.0;
        store.save(Some(id.clone()), &mine).unwrap();
        let n = store.activity(&id).len();
        let err = store.revert_last(&id, USER_ACTOR, None).unwrap_err();
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

    #[test]
    fn only_regular_files_of_a_sane_size_are_attached() {
        // ARCH-001: `/dev/stdin`, devices, FIFOs and folders were read as
        // files (the MCP server then swallowed its own input, `/dev/zero`
        // filled the memory).
        let store = temp_store("attach-checks");
        let id = store
            .save(None, &examples::find("gable").unwrap().project())
            .unwrap();
        let dir = store.dir.join("in");
        fs::create_dir_all(&dir).unwrap();
        let e = store.attach_file(&id, &dir).unwrap_err();
        assert_eq!(e.code, "not_a_file", "{e}");
        #[cfg(unix)]
        for device in ["/dev/null", "/dev/zero", "/dev/stdin"] {
            let e = store.attach_file(&id, Path::new(device)).unwrap_err();
            assert_eq!(e.code, "not_a_file", "{device}: {e}");
        }
        // Larger than any drawing or photo: refused before it is read
        // (a sparse file, so the test writes nothing).
        let big = dir.join("huge.pdf");
        fs::File::create(&big)
            .unwrap()
            .set_len(MAX_ATTACHMENT + 1)
            .unwrap();
        assert_eq!(store.attach_file(&id, &big).unwrap_err().code, "too_large");
        let missing = store.attach_file(&id, &dir.join("gone.pdf")).unwrap_err();
        assert_eq!(missing.code, "file_not_found");
        let ok = dir.join("plan.pdf");
        fs::write(&ok, b"%PDF").unwrap();
        assert_eq!(store.attach_file(&id, &ok).unwrap(), "plan.pdf");
        assert_eq!(store.attachments(&id).len(), 1);
        let _ = fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn deletes_go_to_the_trash_which_forgets_after_a_month() {
        // ARCH-004: a delete is reversible, and the trash does not grow forever.
        let store = temp_store("trash");
        let id = store
            .save_as(
                None,
                &examples::find("gable").unwrap().project(),
                "claude",
                "create",
            )
            .unwrap();
        store.add_attachment(&id, "plan.pdf", b"%PDF").unwrap();
        let old = store.trash_dir().join("1-old");
        fs::create_dir_all(&old).unwrap();
        let stale = TrashEntry {
            entry: "1-old".into(),
            id: "old".into(),
            name: "Old".into(),
            file: None,
            ts_ms: 1,
            actor: USER_ACTOR.into(),
            summary: "long ago".into(),
        };
        fs::write(old.join(TRASH_INFO), serde_json::to_string(&stale).unwrap()).unwrap();
        let e = store.delete(&id, "claude", "Old draft").unwrap();
        assert!(!old.exists(), "older than {TRASH_KEEP_DAYS} days");
        assert!(!store.exists(&id) && store.attachments(&id).is_empty());
        assert_eq!(store.trash(), std::slice::from_ref(&e));
        assert_eq!(
            (e.actor.as_str(), e.summary.as_str()),
            ("claude", "Old draft")
        );
        // The id was taken again meanwhile: the job comes back beside it.
        store
            .save(Some(id.clone()), &Project::new("Other"))
            .unwrap();
        let back = store.restore(&e.entry).unwrap();
        assert_ne!(back, id);
        assert_eq!(store.attachments(&back).len(), 1);
        assert_eq!(store.activity(&back).len(), 1);
        assert!(store.trash().is_empty());
        assert_eq!(store.restore("nope").unwrap_err().code(), "not_found");
        let _ = fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn two_new_jobs_with_one_name_saved_at_once_are_both_kept() {
        // TEST-001: the id was picked before the lock, so both saves got
        // `novak` and the second replaced the first.
        use std::sync::{Arc, Barrier};
        let store = Arc::new(temp_store("same-name"));
        for round in 0..40 {
            let barrier = Arc::new(Barrier::new(2));
            let ids: Vec<String> = (0..2)
                .map(|k| {
                    let (s, b) = (store.clone(), barrier.clone());
                    std::thread::spawn(move || {
                        let mut p = Project::new(format!("Novák {round}"));
                        p.job.customer = format!("customer {k}");
                        b.wait();
                        if k == 0 {
                            s.save(None, &p).unwrap()
                        } else {
                            s.save_as(None, &p, "claude", "create").unwrap()
                        }
                    })
                })
                .collect::<Vec<_>>()
                .into_iter()
                .map(|h| h.join().unwrap())
                .collect();
            assert_ne!(ids[0], ids[1], "round {round}");
            let customers: Vec<String> = ids
                .iter()
                .map(|id| store.load(id).unwrap().job.customer)
                .collect();
            assert_ne!(customers[0], customers[1], "round {round}: one job lost");
        }
        assert_eq!(store.list().len(), 80);
        let _ = fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn a_second_file_of_one_name_is_kept_beside_the_first() {
        // TEST-010: one name twice, one after the other and at once.
        use std::sync::{Arc, Barrier};
        let store = Arc::new(temp_store("same-file"));
        let id = store.save(None, &Project::new("Plan")).unwrap();
        assert_eq!(
            store.add_attachment(&id, "plan.pdf", b"a").unwrap(),
            "plan.pdf"
        );
        assert_eq!(
            store.add_attachment(&id, "plan.pdf", b"b").unwrap(),
            "plan (2).pdf"
        );
        let read = |name: &str| fs::read(store.attachment_path(&id, name).unwrap()).unwrap();
        assert_eq!(
            (read("plan.pdf"), read("plan (2).pdf")),
            (b"a".to_vec(), b"b".to_vec())
        );
        for round in 0..40 {
            let name = format!("r{round}.pdf");
            let barrier = Arc::new(Barrier::new(2));
            let used: Vec<String> = (0..2u8)
                .map(|k| {
                    let (s, b, id, name) =
                        (store.clone(), barrier.clone(), id.clone(), name.clone());
                    std::thread::spawn(move || {
                        b.wait();
                        s.add_attachment(&id, &name, &[k]).unwrap()
                    })
                })
                .collect::<Vec<_>>()
                .into_iter()
                .map(|h| h.join().unwrap())
                .collect();
            assert_ne!(used[0], used[1], "round {round}");
            let mut bytes: Vec<Vec<u8>> = used.iter().map(|n| read(n)).collect();
            bytes.sort();
            assert_eq!(bytes, [vec![0], vec![1]], "round {round}: a file lost");
        }
        let _ = fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn a_new_job_never_adopts_files_or_history_left_by_an_old_one() {
        // ERR-005: leftovers of a job named "Novák" must not appear on the
        // next job of that name.
        let store = temp_store("leftovers");
        let files = store.dir.join(FILES_DIR).join("novak");
        fs::create_dir_all(&files).unwrap();
        fs::write(files.join("plan.pdf"), b"%PDF old customer").unwrap();
        fs::create_dir_all(store.activity_dir()).unwrap();
        let old = Project::new("Novák");
        let entry = ActivityEntry {
            ts_ms: 1,
            actor: "claude".into(),
            summary: "old".into(),
            before: None,
            changes: Vec::new(),
            after: Some(old.clone()),
        };
        fs::write(
            store.activity_dir().join("novak-2.jsonl"),
            serde_json::to_string(&entry).unwrap() + "\n",
        )
        .unwrap();
        fs::write(
            store.activity_dir().join("novak-3.review.json"),
            r#"{"accepted_ms":5}"#,
        )
        .unwrap();
        for saved in [
            store.save(None, &old).unwrap(),
            store.save_as(None, &old, USER_ACTOR, "create").unwrap(),
        ] {
            assert!(
                !["novak", "novak-2", "novak-3"].contains(&saved.as_str()),
                "{saved}"
            );
            assert!(store.attachments(&saved).is_empty());
            assert!(store.activity(&saved).len() <= 1);
            assert_eq!(store.reviewed_at(&saved), 0);
        }
        let _ = fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn a_delete_that_cannot_move_every_file_leaves_the_job_whole() {
        // ERR-005: a drawing open in a viewer on Windows makes the files
        // folder unmovable; the delete must then fail with nothing moved.
        let store = temp_store("delete-fails");
        let id = store
            .save_as(None, &Project::new("Novák"), "claude", "create")
            .unwrap();
        store.add_attachment(&id, "plan.pdf", b"%PDF").unwrap();
        store.mark_reviewed(&id, 7).unwrap();
        let before = fs::read(store.path(&id).unwrap()).unwrap();
        for stuck in [
            store.files_dir(&id).unwrap(),
            store.activity_path(&id).unwrap(),
            store.review_path(&id).unwrap(),
            store.path(&id).unwrap(),
        ] {
            FAIL_MOVE.with(|f| *f.borrow_mut() = vec![stuck.clone()]);
            let e = store.delete(&id, USER_ACTOR, "test").unwrap_err();
            assert_eq!(e.code(), "permission_denied", "{}", stuck.display());
            assert_eq!(fs::read(store.path(&id).unwrap()).unwrap(), before);
            assert_eq!(store.attachments(&id).len(), 1, "{}", stuck.display());
            assert_eq!(store.activity(&id).len(), 1, "{}", stuck.display());
            assert_eq!(store.reviewed_at(&id), 7, "{}", stuck.display());
            assert!(store.trash().is_empty(), "{}", stuck.display());
        }
        FAIL_MOVE.with(|f| f.borrow_mut().clear());
        store.delete(&id, USER_ACTOR, "test").unwrap();
        assert!(!store.exists(&id) && store.attachments(&id).is_empty());
        let _ = fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn a_restore_that_cannot_move_every_file_keeps_the_trash_entry() {
        let store = temp_store("restore-fails");
        let id = store
            .save_as(None, &Project::new("Novák"), "claude", "create")
            .unwrap();
        store.add_attachment(&id, "plan.pdf", b"%PDF").unwrap();
        let e = store.delete(&id, USER_ACTOR, "test").unwrap();
        let folder = store.trash_dir().join(&e.entry);
        FAIL_MOVE.with(|f| *f.borrow_mut() = vec![folder.join("files")]);
        assert_eq!(
            store.restore(&e.entry).unwrap_err().code(),
            "permission_denied"
        );
        assert!(!store.exists(&id));
        assert!(store.activity(&id).is_empty());
        assert_eq!(store.trash().len(), 1);
        FAIL_MOVE.with(|f| f.borrow_mut().clear());
        assert_eq!(store.restore(&e.entry).unwrap(), id);
        assert_eq!(store.attachments(&id).len(), 1);
        assert_eq!(store.activity(&id).len(), 1);
        let _ = fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn saves_are_synced_to_disk_before_they_replace_the_file() {
        // ERR-006: without a sync a power cut after the rename can leave an
        // empty job file.
        let store = temp_store("sync");
        let synced = || SYNCED.with(|s| s.get());
        let start = synced();
        let id = store.save(None, &Project::new("Sync")).unwrap();
        assert!(synced() > start, "the job file is synced");
        let start = synced();
        store
            .save_as(
                Some(id.clone()),
                &Project::new("Sync 2"),
                "claude",
                "rename",
            )
            .unwrap();
        assert!(
            synced() >= start + 2,
            "the change log and the job file are synced"
        );
        let start = synced();
        store.add_attachment(&id, "plan.pdf", b"%PDF").unwrap();
        assert!(synced() > start, "an attached file is synced");
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// `n` overlapping rectangles with 8 pitch breaks on every wall: within
    /// every limit, but slow to join (ARCH-001's reproducer).
    fn slow_project(n: usize) -> Project {
        let breaks: Vec<serde_json::Value> = (0..8)
            .map(|k| serde_json::json!({"height": 3.2 + k as f64 * 0.15, "pitch_deg": 40.0 - k as f64 * 3.0}))
            .collect();
        let edge = |x: f64, y: f64| serde_json::json!({"start": {"x": x, "y": y}, "pitch_deg": 40.0, "eave_height": 3.0, "overhang": 0.5, "breaks": breaks});
        let parts: Vec<serde_json::Value> = (0..n)
            .map(|i| {
                let (x, y) = (i as f64 * 0.5, i as f64 * 0.3);
                serde_json::json!({"name": format!("B{i}"), "edges": [edge(x, y), edge(x + 10.0, y), edge(x + 10.0, y + 8.0), edge(x, y + 8.0)]})
            })
            .collect();
        let p = serde_json::json!({"schema_version": 2, "name": "slow", "parts": parts});
        Project::from_json(&p.to_string()).unwrap()
    }

    #[test]
    fn a_slow_job_is_listed_at_once_without_its_figures() {
        // ARCH-001: one job within the limits blocked listing for minutes.
        let store =
            temp_store("slow-scan").with_summary_budget(std::time::Duration::from_millis(100));
        let slow = slow_project(4);
        assert!(slow.check_limits().is_ok());
        let slow_id = store.save(None, &slow).unwrap();
        let fast_id = store
            .save(None, &examples::find("gable").unwrap().project())
            .unwrap();
        let start = std::time::Instant::now();
        let (ok, broken) = store.scan().unwrap();
        let took = start.elapsed();
        assert!(
            took < std::time::Duration::from_secs(2),
            "listing took {took:?}"
        );
        assert!(broken.is_empty());
        let by = |id: &str| ok.iter().find(|s| s.id == id).cloned().expect("listed");
        let s = by(&slow_id);
        assert!(s.figures_pending && s.roof_area == 0.0, "{s:?}");
        assert_eq!((s.parts, s.outlines.len()), (4, 4));
        let f = by(&fast_id);
        assert!(!f.figures_pending && f.roof_area > 0.0, "{f:?}");
        // A second listing does not wait for it again.
        let start = std::time::Instant::now();
        assert!(
            store
                .list()
                .iter()
                .any(|s| s.id == slow_id && s.figures_pending)
        );
        assert!(start.elapsed() < std::time::Duration::from_secs(1));
        let _ = fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn figures_left_to_the_background_show_up_in_a_later_listing() {
        let store = temp_store("late-figures").with_summary_budget(std::time::Duration::ZERO);
        let id = store
            .save(None, &examples::find("hip").unwrap().project())
            .unwrap();
        let expected = roof_area(&store.load(&id).unwrap());
        let start = std::time::Instant::now();
        loop {
            let s = store.summarize(&id).unwrap();
            if !s.figures_pending {
                assert!((s.roof_area - expected).abs() < 1e-9);
                assert!(!s.lines.is_empty());
                break;
            }
            assert!(
                start.elapsed() < std::time::Duration::from_secs(60),
                "never filled in"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let _ = fs::remove_dir_all(&store.dir);
    }
}
