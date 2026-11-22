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
pub(crate) const ACTIVITY_KEEP: usize = 50;
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
    /// For a removed file: the trash entry of its job, while that job is
    /// deleted too. Its id may then be taken by a new job, which must not
    /// receive the file (audit 2026-11-15 HUNTIO-011).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_in_trash: Option<String>,
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
    /// Buildings the core gave no roof for (ERRU-005): equal to `parts`, the
    /// roof is not calculated and `roof_area` is no figure; fewer, the roof
    /// is partly calculated and `roof_area` leaves them out. 0 while
    /// `figures_pending`.
    pub parts_failed: usize,
    /// The roof area of the stored result (the offer's figure), if any
    /// (UX-008): the library shows it instead of the current one.
    pub stored_area: Option<f64>,
    /// The current figures differ from the stored result's
    /// (`snapshot::status`); false without a stored result or while
    /// `figures_pending`.
    pub result_differs: bool,
    /// The job is too complex to calculate (`work`, ARCH-102): no figures,
    /// `parts_failed` equals `parts`.
    #[serde(default)]
    pub too_complex: bool,
    /// Hand-drawn faces: with any, the roof is calculated (partly) even when
    /// every building is without a roof (audit 2026-11-21 ERR-202).
    #[serde(default)]
    pub faces: usize,
    /// The design is outside the limits (`Project::check_limits`): nothing
    /// is calculated, hand-drawn faces neither.
    #[serde(default)]
    pub refused: bool,
}

impl ProjectSummary {
    /// How much of the roof the card's figures cover; `None` while
    /// `figures_pending` (see [`crate::quantities::RoofCoverage`]). A job
    /// too complex to calculate or outside the limits has no roof, its
    /// hand-drawn faces notwithstanding: nothing was calculated (TEST-301).
    pub fn coverage(&self) -> Option<crate::quantities::RoofCoverage> {
        (!self.figures_pending).then(|| {
            if self.too_complex || self.refused {
                crate::quantities::RoofCoverage::None
            } else {
                crate::quantities::RoofCoverage::from_counts(
                    self.parts,
                    self.parts_failed,
                    self.faces,
                )
            }
        })
    }
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
    /// For a revert: the time of the oldest entry it undid (it undid that
    /// one and every later one). Lets "revert the latest change" step past
    /// what a revert already undid.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub undid_from: Option<u64>,
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

/// The name of a test job whose card calculation fails (TEST-504).
#[cfg(test)]
const CARD_FAILS: &str = "test: the card calculation fails";

/// A library card's roof figures, from one calculation (ARCH-006).
type CardFigures = (f64, Vec<[[f64; 2]; 2]>, usize, bool);

/// A library card's roof area, plan lines and the number of buildings
/// without a roof (ERRU-005), from one calculation (ARCH-006). A building
/// is without a roof when the core gave none or when its input was left
/// out of the calculation (a wall under 1 cm, crossing walls, a refused
/// pitch: `Project::problems` of the part), so the area of the others is
/// never shown as the job's whole roof (audit 2026-11-21 ERR-001).
fn card_figures(project: &Project) -> CardFigures {
    #[cfg(test)]
    if project.name == CARD_FAILS {
        panic!("the card calculation of this test job fails");
    }
    let geos = crate::openings::generate_project(project);
    let failed = crate::quantities::parts_without_roof(&geos, project);
    (
        crate::quantities::roof_totals_of(&geos, project).roof_area,
        lines_of(&geos),
        failed,
        crate::snapshot::status_of(&geos, project).is_some_and(|s| !s.differences.is_empty()),
    )
}

pub fn outlines(project: &Project) -> Vec<Vec<[f64; 2]>> {
    project
        .parts
        .iter()
        .map(|p| p.outline().iter().map(|pt| [pt.x, pt.y]).collect())
        .collect()
}

/// Most outline points a library card carries in all. A job within the
/// limits can hold 500 hand-drawn faces of 500 corners each: carried
/// whole, such a card was megabytes of the library listing and its
/// thumbnail emptied the library screen (audit 2026-11-22 ARCH-402).
pub const MAX_CARD_POINTS: usize = 2000;

/// The outlines of a library card's thumbnail: the walls of each part and
/// the hand-drawn faces, so a job of faces only has a thumbnail too
/// (audit 2026-11-22 HUNT-305 item 5), at most [`MAX_CARD_POINTS`] points.
pub fn card_outlines(project: &Project) -> Vec<Vec<[f64; 2]>> {
    let mut out = outlines(project);
    out.extend(
        project
            .manual_faces
            .iter()
            .filter(|f| f.points.len() >= 3)
            .map(|f| f.points.iter().map(|p| [p.x, p.y]).collect()),
    );
    bounded_outlines(out, MAX_CARD_POINTS)
}

/// Outlines of at most this many points are kept whole on a card: they
/// cost little, and a rectangle thinned to a triangle looks wrong (audit
/// 2026-11-22 HUNT-505).
const WHOLE_OUTLINE: usize = 4;

/// `outlines` with at most `max` points in all: outlines of up to
/// [`WHOLE_OUTLINE`] points are kept whole, the larger ones keep evenly
/// spread points, their share of what the small ones leave (at least
/// [`WHOLE_OUTLINE`]); outlines past the budget are replaced by one box
/// around all of them, so the thumbnail keeps the job's extent.
fn bounded_outlines(outlines: Vec<Vec<[f64; 2]>>, max: usize) -> Vec<Vec<[f64; 2]>> {
    let total: usize = outlines.iter().map(Vec::len).sum();
    if total <= max {
        return outlines;
    }
    let budget = max.saturating_sub(4);
    let small: usize = outlines
        .iter()
        .map(Vec::len)
        .filter(|&n| n <= WHOLE_OUTLINE)
        .sum();
    let large = (total - small).max(1);
    let large_budget = budget.saturating_sub(small);
    let mut used = 0;
    let mut out = Vec::new();
    let mut rest: Option<[f64; 4]> = None;
    for o in outlines {
        let share = if o.len() <= WHOLE_OUTLINE {
            o.len()
        } else {
            (o.len() * large_budget / large).clamp(WHOLE_OUTLINE, o.len())
        };
        let thin: Vec<[f64; 2]> = (0..share).map(|i| o[i * o.len() / share]).collect();
        if used + thin.len() <= budget {
            used += thin.len();
            out.push(thin);
        } else {
            for p in &o {
                let b = rest.get_or_insert([p[0], p[1], p[0], p[1]]);
                *b = [
                    b[0].min(p[0]),
                    b[1].min(p[1]),
                    b[2].max(p[0]),
                    b[3].max(p[1]),
                ];
            }
        }
    }
    if let Some([x0, y0, x1, y1]) = rest {
        out.push(vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]]);
    }
    out
}

/// A card's figures when its calculation was refused: no roof for any
/// building, and `too_complex` only when it was over the work bound, not
/// when it failed (TEST-404 N13).
fn refused_card(project: &Project, r: &crate::work::Refusal) -> (CardFigures, bool) {
    (
        (0.0, Vec::new(), project.parts.len(), false),
        *r == crate::work::Refusal::TooComplex,
    )
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
    /// How many bytes of a change log are read, [`MAX_ACTIVITY_FILE`]
    /// (smaller in tests).
    activity_window: u64,
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
    /// A link, or a folder with files, in the library's own folders
    /// ([`ForeignEntry`]).
    Foreign,
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
            StoreError::Io {
                kind: IoKind::Foreign,
                ..
            } => "foreign_entry",
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
            _ if is_foreign(&e) => IoKind::Foreign,
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
/// device or pipe), `too_large`, `unsupported_type`, `not_attachable` (a
/// file type a job does not carry), `invalid_name`, `out_of_range`,
/// `too_many`.
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
        // Its text names the path already.
        if is_foreign(&e) {
            return Self::new("foreign_entry", e.to_string());
        }
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
    let version = whole_version(v.get("schema_version")?)?;
    (version > u64::from(crate::model::SCHEMA_VERSION)).then_some(version)
}

/// A format version in a file as a whole number: `4.0` and `1e30` are
/// whole numbers too (ARCH-205 item 1), a huge one is held at `u64::MAX`;
/// `None` for a fraction, a negative number or not a number.
pub(crate) fn whole_version(n: &serde_json::Value) -> Option<u64> {
    n.as_u64().or_else(|| {
        n.as_f64()
            .filter(|f| f.is_finite() && f.fract() == 0.0 && *f >= 0.0)
            .map(|f| f.min(u64::MAX as f64) as u64)
    })
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
    write_atomic_bytes(path, contents.as_bytes())
}

fn write_atomic_bytes(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = path.with_extension(format!("{}-{n}.tmp", std::process::id()));
    write_synced(&tmp, contents)?;
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

/// Whether Windows takes `name` for a device in every folder, after
/// Microsoft's "Naming Files, Paths, and Namespaces": `CON`, `PRN`, `AUX`,
/// `NUL`, `COM0`-`COM9`, `LPT0`-`LPT9` (also with the superscript digits
/// ¹ ² ³), `CONIN$` and `CONOUT$`, in any case, also when an extension
/// follows (`NUL.pdf`, `con.x.tectora`) or spaces end the stem
/// (`CON .txt`). `CON.tectora` opens the console, not a file (audit
/// 2026-11-21 ARCH-004).
fn is_device_name(name: &str) -> bool {
    let stem = name
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end_matches(' ')
        .to_ascii_lowercase();
    if matches!(
        stem.as_str(),
        "con" | "prn" | "aux" | "nul" | "conin$" | "conout$"
    ) {
        return true;
    }
    let mut rest = stem.chars();
    let prefix: String = rest.by_ref().take(3).collect();
    let digit: Vec<char> = rest.collect();
    (prefix == "com" || prefix == "lpt")
        && matches!(digit.as_slice(), [d] if d.is_ascii_digit() || matches!(d, '¹' | '²' | '³'))
}

/// Characters that change how a name reads without being seen: the
/// Unicode format characters (category Cf) such as the bidirectional
/// overrides (`file\u{202e}gnp.pdf` shows as "filefdp.png"), zero-width
/// spaces and joiners, the byte order mark and the soft hyphen (audit
/// 2026-11-21 HUNTIO-007).
pub(crate) fn is_invisible(c: char) -> bool {
    matches!(
        c,
        '\u{ad}'
            | '\u{600}'..='\u{605}'
            | '\u{61c}'
            | '\u{6dd}'
            | '\u{70f}'
            | '\u{890}'..='\u{891}'
            | '\u{8e2}'
            | '\u{180e}'
            | '\u{200b}'..='\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{206f}'
            | '\u{feff}'
            | '\u{fff9}'..='\u{fffb}'
            | '\u{110bd}'
            | '\u{110cd}'
            | '\u{13430}'..='\u{1343f}'
            | '\u{1bca0}'..='\u{1bca3}'
            | '\u{1d173}'..='\u{1d17a}'
            | '\u{e0001}'
            | '\u{e0020}'..='\u{e007f}'
    )
}

/// A project id is a plain file name inside the library folder on both
/// macOS and Windows: no separators, drive or stream prefix (`C:x`,
/// `x:stream`), characters Windows forbids, hidden names or device names.
fn valid_id(id: &str) -> bool {
    id.len() <= MAX_ID_LEN && plain_name(id) && !is_device_name(id)
}

/// No separators, drive or stream prefix, characters Windows forbids,
/// control or invisible characters, hidden or trailing-dot names.
fn plain_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && !name.ends_with(['.', ' '])
        && !name.chars().any(|c| {
            c.is_control()
                || is_invisible(c)
                || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        })
}

/// An attachment or export name is a plain file name on both macOS and
/// Windows: the [`valid_id`] rules, and no device name before an extension
/// either (`NUL.pdf` is the null device on Windows). Without this a received
/// bundle's `files/C:x.dll` joins to `C:x.dll` on Windows, outside the
/// library (ARCH-039).
pub fn valid_file_name(name: &str) -> bool {
    name.len() <= 255 && plain_name(name) && !is_device_name(name)
}

fn too_large_file(path: &Path, max: u64) -> FileError {
    FileError::new(
        "too_large",
        format!("{}: larger than {} MB", path.display(), max >> 20),
    )
}

/// Largest job file read, bytes. A job within the limits
/// (`Project::check_limits`, notes of at most a million characters) is far
/// smaller; the bound keeps a file of several GB copied in by mistake, or
/// a link to `/dev/zero`, from filling the memory (ARCH-204).
pub const MAX_JOB_FILE: u64 = 64 * 1024 * 1024;

/// The text of the job file at `path`, through [`open_regular`] with
/// [`MAX_JOB_FILE`]: a named pipe, a device or a folder is `not_a_file`
/// instead of hanging or reading for ever, a larger file `too_large`
/// (ARCH-204). For the library, the CLI and files opened from elsewhere.
pub fn read_job_text(path: &Path) -> Result<String, FileError> {
    job_text(
        read_regular(path, MAX_JOB_FILE)?,
        &path.display().to_string(),
    )
}

/// The bytes of a job file (`what` names it) as its text: a leading UTF-8
/// byte order mark, which Windows tools write (PowerShell 5, Notepad), is
/// dropped, and a file in another encoding (a Windows code page) is
/// `unreadable` as "not UTF-8 text" instead of an operating system error
/// (audit 2026-11-22 ERR-306).
pub fn job_text(mut bytes: Vec<u8>, what: &str) -> Result<String, FileError> {
    if bytes.starts_with(b"\xef\xbb\xbf") {
        bytes.drain(..3);
    }
    String::from_utf8(bytes).map_err(|e| {
        FileError::new(
            "unreadable",
            format!(
                "{what}: not UTF-8 text (saved in another encoding?), at byte {}",
                e.utf8_error().valid_up_to()
            ),
        )
    })
}

/// Largest change log (`.activity/<id>.jsonl`) read, bytes: of a longer
/// one only the newest entries in this many bytes are read (ARCH-301).
pub const MAX_ACTIVITY_FILE: u64 = 4 * MAX_JOB_FILE;

/// Largest small file of the library read (review mark, live file, trash
/// note), bytes (ARCH-301).
pub const MAX_SMALL_FILE: u64 = 1024 * 1024;

/// The bytes of the regular file at `path`, at most `max` of them
/// ([`open_regular`]): a pipe or a device in its place is refused instead
/// of hanging the reader (ARCH-301).
pub fn read_regular(path: &Path, max: u64) -> Result<Vec<u8>, FileError> {
    let mut out = Vec::new();
    std::io::Read::read_to_end(
        &mut std::io::Read::take(open_regular(path, max)?, max),
        &mut out,
    )
    .map_err(|e| FileError::io(path, e))?;
    Ok(out)
}

/// The newest whole lines of the regular file at `path` that fit in `max`
/// bytes (the whole file when it fits); a pipe or a device is refused
/// (ARCH-301).
pub fn read_newest_lines(path: &Path, max: u64) -> Result<Vec<u8>, FileError> {
    use std::io::{Read, Seek};
    let mut f = open_regular(path, u64::MAX)?;
    let len = f.metadata().map_err(|e| FileError::io(path, e))?.len();
    let from = len.saturating_sub(max);
    if from > 0 {
        // One byte before, to tell whether `from` starts a line.
        f.seek(std::io::SeekFrom::Start(from - 1))
            .map_err(|e| FileError::io(path, e))?;
    }
    let mut out = Vec::new();
    f.take(max + u64::from(from > 0))
        .read_to_end(&mut out)
        .map_err(|e| FileError::io(path, e))?;
    if from > 0 {
        let at = out
            .iter()
            .position(|&b| b == b'\n')
            .map_or(out.len(), |i| i + 1);
        out.drain(..at);
    }
    Ok(out)
}

/// Whether `path` is missing or a regular file itself: a pipe or a device
/// in its place must not be opened for writing, which waits for a reader
/// (ARCH-301), and a symbolic link must not be written through, which
/// would append to or create a file outside the library (audit 2026-11-22
/// HUNT-402, HUNT-403). The link itself is looked at, not its target.
pub(crate) fn missing_or_regular(path: &Path) -> std::io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(m) if !m.file_type().is_file() => Err(foreign(
            path,
            "not a regular file (a link, folder, device or pipe)",
        )),
        _ => Ok(()),
    }
}

/// Something in the library's own folders that Tectora did not put there
/// and does not write through: a link, a pipe, a folder where a file
/// belongs (audit 2026-11-22 HUNT-402, HUNT-403, HUNT-501, HUNT-503).
/// Its errors have the code `foreign_entry`.
#[derive(Debug)]
struct ForeignEntry(String);

impl std::fmt::Display for ForeignEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ForeignEntry {}

/// The error for `path`, a [`ForeignEntry`]: `what` it is.
pub(crate) fn foreign(path: &Path, what: &str) -> std::io::Error {
    std::io::Error::other(ForeignEntry(format!("{}: {what}", path.display())))
}

/// Whether `e` is about a [`ForeignEntry`].
fn is_foreign(e: &std::io::Error) -> bool {
    e.get_ref().is_some_and(|r| r.is::<ForeignEntry>())
}

/// Makes sure `dir`, one of the library's own folders (`.activity`,
/// `.files`, `.files/<id>`, `.trash`), is a real folder, creating it when
/// missing. Tectora manages these folders itself: a link in place of one
/// (a shared or synced library) is refused, never written through, so no
/// job, history, drawing or deleted job lands outside the library (audit
/// 2026-11-22 HUNT-503). Its parent must exist.
pub(crate) fn own_folder(dir: &Path) -> std::io::Result<()> {
    for _ in 0..3 {
        match fs::symlink_metadata(dir) {
            Ok(m) if m.is_dir() => return Ok(()),
            Ok(_) => {
                return Err(foreign(
                    dir,
                    "a link or a file stands where Tectora keeps its own folder; Tectora does not write through it (remove it, Tectora then makes the folder again)",
                ));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => match fs::create_dir(dir) {
                Ok(()) => return Ok(()),
                // Made by another process meanwhile: looked at again.
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e),
            },
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::other(format!(
        "{}: the folder keeps changing",
        dir.display()
    )))
}

/// Whether `dir` is a real folder itself, not a link (for reading the
/// library's own folders without creating them).
fn is_own_folder(dir: &Path) -> bool {
    fs::symlink_metadata(dir).is_ok_and(|m| m.is_dir())
}

/// Opens `path` for writing with `options` (a lock file, the change log)
/// only when it is missing or a regular file itself ([`missing_or_regular`]):
/// never through a link, and never waiting on a pipe. The open does not
/// wait for a pipe put in its place between the check and the open
/// (`O_NONBLOCK`), and on macOS and Linux the opened file must be the one
/// at `path` itself, not one a link swapped in meanwhile points to.
pub(crate) fn open_regular_for_write(
    path: &Path,
    options: &mut fs::OpenOptions,
) -> std::io::Result<fs::File> {
    missing_or_regular(path)?;
    #[cfg(any(target_os = "linux", target_os = "android"))]
    std::os::unix::fs::OpenOptionsExt::custom_flags(options, 0o4000);
    #[cfg(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd"
    ))]
    std::os::unix::fs::OpenOptionsExt::custom_flags(options, 0x0004);
    let file = options.open(path)?;
    let opened = file.metadata()?;
    let here = fs::symlink_metadata(path)?;
    #[cfg(unix)]
    let same = {
        use std::os::unix::fs::MetadataExt;
        here.dev() == opened.dev() && here.ino() == opened.ino()
    };
    #[cfg(not(unix))]
    let same = true;
    if !(opened.is_file() && here.file_type().is_file() && same) {
        return Err(foreign(
            path,
            "not a regular file (a link, folder, device or pipe)",
        ));
    }
    Ok(file)
}

/// Entries of a change log's bytes: one torn or invalid line (an
/// interrupted write) costs that line only, not the whole history (ERR-005).
fn parse_log(bytes: &[u8]) -> Vec<ActivityEntry> {
    bytes
        .split(|&b| b == b'\n')
        .filter_map(|l| serde_json::from_slice(l).ok())
        .collect()
}

/// A change log as read before a save adds an entry.
struct LogRead {
    path: PathBuf,
    /// Whether anything is at the path (also a pipe or a link).
    there: bool,
    /// Its entries in the read window (all of a log within the bounds).
    entries: Vec<ActivityEntry>,
}

/// How to take an appended entry out of the change log again when the job
/// file then could not be written (ERR-008).
#[must_use]
enum LogUndo {
    /// Cut the file back to its old length, so no entry before it is lost
    /// (ARCH-401: the log was written back from what was read, which
    /// dropped every entry outside the read window).
    Truncate(PathBuf, u64),
    /// There was no log.
    Remove(PathBuf),
}

impl LogUndo {
    fn apply(self) -> std::io::Result<()> {
        match self {
            LogUndo::Truncate(path, len) => {
                open_regular_for_write(&path, fs::OpenOptions::new().write(true))?.set_len(len)
            }
            LogUndo::Remove(path) => fs::remove_file(path),
        }
    }
}

/// How often [`take_lock_file`] starts again when its lock path changed.
const LOCK_TRIES: usize = 8;

/// Takes the lock file at `path` (created when missing), held until the
/// returned file is dropped. `Ok(None)` when no lock can be had where the
/// folder or file system holds none (read-only, no locks; best effort, see
/// [`Store::lock`]). A link, pipe, device or empty folder found in its
/// place is never opened (it would hang the open or create a file outside
/// the library, HUNT-402, HUNT-403) but replaced by a plain lock file
/// ([`replace_odd_lock`]); a folder with files in it is refused, a
/// [`ForeignEntry`]: saving on unlocked lost changes of two writers
/// without a word (audit 2026-11-22 HUNT-501). Once locked, the path must
/// still name the locked file, else it starts again: a lock on a file
/// another process has just replaced locks nobody out (HUNT-502).
pub(crate) fn take_lock_file(path: &Path) -> std::io::Result<Option<fs::File>> {
    let open = || {
        open_regular_for_write(
            path,
            fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true),
        )
    };
    let mut moved = false;
    for _ in 0..LOCK_TRIES {
        match open() {
            Ok(file) => {
                if file.lock().is_err() {
                    return Ok(None);
                }
                if names_file(path, &file) {
                    return Ok(Some(file));
                }
                moved = true;
            }
            // Gone again or a regular file by now (replaced by another
            // process): opened again. A regular file that never opens
            // (read-only) leaves the save unlocked, as before.
            Err(_) => match fs::symlink_metadata(path) {
                Ok(m) if !m.file_type().is_file() => replace_odd_lock(path)?,
                _ => {}
            },
        }
    }
    if moved {
        return Err(foreign(
            path,
            "the lock file keeps being replaced while it is taken; the change was not saved",
        ));
    }
    Ok(None)
}

/// Whether `path` itself still names `file` (on Windows, which has no
/// stable file ids in std, it is taken as such).
fn names_file(path: &Path, file: &fs::File) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        match (fs::symlink_metadata(path), file.metadata()) {
            (Ok(here), Ok(locked)) => {
                here.file_type().is_file()
                    && here.dev() == locked.dev()
                    && here.ino() == locked.ino()
            }
            _ => false,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = file;
        fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_file())
    }
}

/// Removes the link, pipe, device or empty folder at the lock path `path`,
/// so a plain lock file can be made. One process at a time, under the
/// lock of `<path>.fix`, and only while the entry is still odd: a process
/// that saw it odd before another one replaced it must never remove that
/// one's live lock file (HUNT-502: both then held "the lock" on two files
/// and a change was lost). A folder with files in it is refused.
fn replace_odd_lock(path: &Path) -> std::io::Result<()> {
    let mut fix = path.as_os_str().to_owned();
    fix.push(".fix");
    let fix = PathBuf::from(fix);
    let guard = open_regular_for_write(
        &fix,
        fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true),
    )?;
    let _ = guard.lock();
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() => {
            if fs::remove_dir(path).is_err() && is_own_folder(path) {
                return Err(foreign(
                    path,
                    "a folder with files in it stands where the lock file belongs, so saves could not be locked; remove it (the change was not saved)",
                ));
            }
        }
        Ok(m) if !m.file_type().is_file() => {
            // A link to a folder is removed as a folder on Windows.
            if fs::remove_file(path).is_err() {
                let _ = fs::remove_dir(path);
            }
        }
        // Missing, or a regular file another process made meanwhile.
        _ => {}
    }
    Ok(())
}

/// Opens `path` for reading when it is a regular file of at most `max`
/// bytes: a folder, a device (`/dev/zero`, which never ends) or a named
/// pipe (which blocks the open until a writer comes, and with it the MCP
/// server) is refused before it is opened, `not_a_file`; a larger file is
/// `too_large`. A path replaced by a pipe between the check and the open
/// is opened without waiting and refused as well (ARCH-001, ARCH-002).
pub fn open_regular(path: &Path, max: u64) -> Result<fs::File, FileError> {
    let not_a_file = || {
        FileError::new(
            "not_a_file",
            format!(
                "{}: not a regular file (a folder, device or pipe)",
                path.display()
            ),
        )
    };
    let meta = fs::metadata(path).map_err(|e| FileError::io(path, e))?;
    if !meta.is_file() {
        return Err(not_a_file());
    }
    if meta.len() > max {
        return Err(too_large_file(path, max));
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    // O_NONBLOCK: an open of a pipe returns at once; reads of a regular
    // file are not changed by it.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    std::os::unix::fs::OpenOptionsExt::custom_flags(&mut options, 0o4000);
    #[cfg(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd"
    ))]
    std::os::unix::fs::OpenOptionsExt::custom_flags(&mut options, 0x0004);
    let file = options.open(path).map_err(|e| FileError::io(path, e))?;
    // The path may have been replaced between the check and the open.
    match file.metadata() {
        Ok(m) if m.is_file() && m.len() <= max => Ok(file),
        Ok(m) if m.is_file() => Err(too_large_file(path, max)),
        _ => Err(not_a_file()),
    }
}

/// [`slug`] for the tests of other modules.
#[cfg(test)]
pub(crate) fn slug_of(name: &str) -> String {
    slug(name)
}

/// File-name friendly id derived from a project name.
fn slug(name: &str) -> String {
    // Composed first: a name from macOS may come decomposed (HUNTIO-007).
    let mut s: String = crate::library::fold(name)
        .chars()
        .map(|c| {
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

/// The text of the change log `entries` (oldest first) trimmed to
/// [`ACTIVITY_KEEP`] entries and `window` bytes, the entry at `start`
/// (the first change waiting for review) kept in place of the oldest kept
/// one ([`Store::trimmed_log`]).
pub(crate) fn trim_log(entries: &[ActivityEntry], start: usize, window: u64) -> String {
    let n = entries.len();
    if n == 0 {
        return String::new();
    }
    // Only the entries that can be kept are written out.
    let from = n.saturating_sub(ACTIVITY_KEEP);
    let lines: Vec<String> = entries[from..]
        .iter()
        .map(|e| serde_json::to_string(e).unwrap_or_default())
        .collect();
    // The first change still waiting for review holds the version the
    // user had before it ("Revert all" goes back to it). When more
    // changes than the log keeps are pending, keep that one in place of
    // the oldest kept one, so the pre-AI version is never dropped while
    // the review is open.
    let mut cut = from;
    loop {
        let first = if start < cut {
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
            serde_json::to_string(&first).unwrap_or_default()
        } else {
            lines[cut - from].clone()
        };
        let rest = &lines[cut + 1 - from..];
        let bytes = first.len() + 1 + rest.iter().map(|l| l.len() + 1).sum::<usize>();
        // The newest entry is always kept, even alone over the window.
        if bytes as u64 <= window || cut + 1 >= n {
            let mut text = String::with_capacity(bytes);
            for l in std::iter::once(&first).chain(rest) {
                if !l.is_empty() {
                    text.push_str(l);
                    text.push('\n');
                }
            }
            return text;
        }
        cut += 1;
    }
}

impl Store {
    /// Refuses to write when the jobs folder itself is gone (deleted, a
    /// network drive or USB disk disconnected): it used to be created
    /// again, empty, and the next saves went into it while the jobs were
    /// elsewhere (audit 2026-11-15 HUNTIO-010).
    fn library_present(&self) -> Result<(), StoreError> {
        if self.dir.is_dir() {
            return Ok(());
        }
        Err(StoreError::io(
            None,
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("the jobs folder {} is missing", self.dir.display()),
            ),
        ))
    }

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
            activity_window: MAX_ACTIVITY_FILE,
        }
    }

    /// The same store waiting at most `budget` for one job's roof figures
    /// when it lists the library.
    pub fn with_summary_budget(mut self, budget: std::time::Duration) -> Self {
        self.summary_budget = budget;
        self
    }

    /// The same store reading at most `bytes` of a change log, for tests
    /// of logs longer than [`MAX_ACTIVITY_FILE`].
    #[cfg(test)]
    fn with_activity_window(mut self, bytes: u64) -> Self {
        self.activity_window = bytes;
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
            summary.parts_failed = summary.parts;
            summary.refused = true;
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
                // A roof that panicked has no figures at all (ERRU-005); one
                // over the work bound stops there instead of keeping a core
                // busy for as long as the app runs (ARCH-102).
                let figures = match crate::work::calculate(|| card_figures(&project)) {
                    Ok(f) => (f, false),
                    Err(r) => refused_card(&project, &r),
                };
                // Under the lock, so `summarize` either sees the answer or
                // has left its pending card for this thread to fill in.
                let mut c = lock_cache(&cache);
                c.running.remove(&job);
                if let Some((s, card)) = c.entries.get_mut(&job)
                    && *s == at
                    && card.figures_pending
                {
                    (
                        card.roof_area,
                        card.lines,
                        card.parts_failed,
                        card.result_differs,
                    ) = figures.0.clone();
                    card.too_complex = figures.1;
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
            Some(((area, lines, failed, differs), too_complex)) => {
                (summary.roof_area, summary.lines, summary.parts_failed) = (area, lines, failed);
                summary.result_differs = differs;
                summary.too_complex = too_complex;
            }
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
            outlines: card_outlines(&project),
            lines: Vec::new(),
            last_activity: self.activity(id).last().map(ActivityEntry::info),
            pitch_range: crate::library::pitch_range(&project),
            dormers: project.dormers.len(),
            files: self.attachments(id).len(),
            has_result: project.result_snapshot.is_some(),
            figures_pending: false,
            parts_failed: 0,
            stored_area: project.result_snapshot.as_ref().map(|s| s.roof_area),
            result_differs: false,
            too_complex: false,
            refused: false,
            faces: project.manual_faces.len(),
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
        let path = self.path(id)?;
        // Bounded, and never blocked by a pipe or a device named like a
        // job (ARCH-204).
        let text = match read_job_text(&path) {
            Ok(t) => t,
            Err(e) if matches!(e.code, "not_a_file" | "too_large" | "unreadable") => {
                return Err(StoreError::Unreadable {
                    id: id.to_string(),
                    detail: e.detail,
                });
            }
            Err(e) if e.code == "file_not_found" => {
                return Err(StoreError::NotFound(id.to_string()));
            }
            Err(e) => {
                return Err(StoreError::Io {
                    id: Some(id.to_string()),
                    kind: match e.code {
                        "permission_denied" => IoKind::PermissionDenied,
                        "disk_full" => IoKind::DiskFull,
                        _ => IoKind::Other,
                    },
                    detail: e.detail,
                });
            }
        };
        if let Some(version) = newer_format(&text) {
            return Err(StoreError::NewerFormat {
                id: id.to_string(),
                version,
                max: crate::model::SCHEMA_VERSION,
            });
        }
        let project = Project::from_json(&text).map_err(|e| StoreError::Unreadable {
            id: id.to_string(),
            detail: e.to_string(),
        })?;
        // A stored result over its bound is not handed on: 200 000 line
        // codes froze the editor and made every MCP read 23 MB (ARCH-303).
        if let Some(why) = project
            .result_snapshot
            .as_ref()
            .and_then(crate::snapshot::ResultSnapshot::problem)
        {
            return Err(StoreError::Unreadable {
                id: id.to_string(),
                detail: why,
            });
        }
        Ok(project)
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
        // Anything there counts, also a link to nowhere: a job of that id
        // would write its log through it (ARCH-501). A path that cannot be
        // checked counts as taken.
        .any(|p| match fs::symlink_metadata(p) {
            Ok(_) => true,
            Err(e) => e.kind() != std::io::ErrorKind::NotFound,
        })
    }

    /// Takes the library's creation lock (`.activity/.new.lock`, a name no job id can have), held until
    /// the returned file is dropped: picking a new id and writing its file is
    /// one step for every process, so two jobs created with one name at the
    /// same moment get two ids (TEST-001). Best effort, like [`Store::lock`].
    fn creation_lock(&self) -> Result<Option<fs::File>, StoreError> {
        self.lock_file(None, ".new.lock")
    }

    /// Takes the lock file `name` in `.activity` for [`Store::lock`] and
    /// [`Store::creation_lock`]: `Ok(None)` where no lock can be had (no
    /// library, a read-only folder), an error for a link in place of
    /// `.activity` or a lock path that cannot be made a lock file
    /// ([`take_lock_file`]).
    fn lock_file(&self, id: Option<&str>, name: &str) -> Result<Option<fs::File>, StoreError> {
        if self.library_present().is_err() {
            return Ok(None);
        }
        let dir = self.activity_dir();
        match own_folder(&dir) {
            Ok(()) => {}
            Err(e) if is_foreign(&e) => return Err(StoreError::io(id, e)),
            Err(_) => return Ok(None),
        }
        take_lock_file(&dir.join(name)).map_err(|e| StoreError::io(id, e))
    }

    /// Saves without recording activity (app autosave of its own edits).
    ///
    /// Refuses to replace a file saved by a newer Tectora: on a shared
    /// library folder an older app would otherwise write its stale copy over
    /// a colleague's newer one.
    pub fn save(&self, id: Option<String>, project: &Project) -> Result<String, StoreError> {
        let _creating = match id {
            None => self.creation_lock()?,
            Some(_) => None,
        };
        let id = id.unwrap_or_else(|| self.new_id(&project.name));
        let _lock = self.lock(&id)?;
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
        let _lock = self.lock(id)?;
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
        self.library_present()?;
        if let Some(version) = read_job_text(&path).ok().and_then(|t| newer_format(&t)) {
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
    fn lock(&self, id: &str) -> Result<Option<fs::File>, StoreError> {
        self.path(id)?;
        self.lock_file(Some(id), &format!("{id}.lock"))
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
                let _creating = self.creation_lock()?;
                let id = self.new_id(&project.name);
                let _lock = self.lock(&id)?;
                self.log_and_write(&id, None, project, actor, summary, None)?;
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
        self.save_checked_undoing(id, project, actor, summary, base, None)
    }

    /// [`Store::save_as_checked`] that records, for a revert, the time of
    /// the oldest entry it undid.
    fn save_checked_undoing(
        &self,
        id: &str,
        project: &Project,
        actor: &str,
        summary: &str,
        base: Option<&Project>,
        undid_from: Option<u64>,
    ) -> Result<CheckedSave, StoreError> {
        self.path(id)?;
        let _lock = self.lock(id)?;
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
            self.log_and_write(id, before, project, actor, summary, undid_from)?;
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
        undid_from: Option<u64>,
    ) -> Result<(), StoreError> {
        // A newer file was refused before: by `load` under this lock for an
        // existing job, and a new id has no file. `write` checks again and
        // the entry is then taken out (ARCH-403 item 3: the job file was
        // read three times per save).
        // Log first: the app reacts to the project file and then reads the
        // log. When the file then cannot be written, the entry is taken out
        // again: it stood for a change that never happened, shown as pending
        // in the history and the review (audit 2026-11-15 ERR-008).
        // The log is read once per save (ARCH-401).
        let log = self.read_log(id)?;
        // Entries are found and accepted by their time, so each one is later
        // than the one before, even when two saves fall in one millisecond
        // (the review's list then had two equal keys, ERRU-009) or the log
        // came with a clock that is ahead (a received bundle): a change
        // after an accepted future-dated entry used to count as accepted
        // too (HUNTIO-005).
        let last_ts = log.entries.last().map_or(0, |e| e.ts_ms);
        let ts_ms = now_ms().max(last_ts.saturating_add(1));
        let (undo, trim) = self.append_activity(
            id,
            ActivityEntry {
                ts_ms,
                actor: actor.to_string(),
                summary: summary.to_string(),
                changes: before
                    .as_ref()
                    .map(|b| diff(b, project))
                    .unwrap_or_default(),
                before,
                after: Some(project.clone()),
                undid_from,
            },
            log,
        )?;
        self.write(id, project).inspect_err(|_| {
            let _ = undo.apply();
        })?;
        // Trimmed only once the change is saved, so a failed write takes
        // the entry out by cutting the file back and never loses an older
        // entry (ARCH-401). A trim that fails is tried again next save.
        if let Some(entries) = trim {
            let _ = write_atomic(&self.activity_path(id)?, &self.trimmed_log(id, &entries));
        }
        Ok(())
    }

    /// The change log of `id` as [`Store::activity`] reads it, with what
    /// is needed to add an entry and to take it out again.
    fn read_log(&self, id: &str) -> Result<LogRead, StoreError> {
        let path = self.activity_path(id)?;
        let entries = read_newest_lines(&path, self.activity_window)
            .map(|b| parse_log(&b))
            .unwrap_or_default();
        Ok(LogRead {
            there: fs::symlink_metadata(&path).is_ok(),
            path,
            entries,
        })
    }

    /// Appends `entry` to the change log read as `log`. Returns how to take
    /// it out again, and the entries to trim the log to once the change is
    /// saved: the log is kept to [`ACTIVITY_KEEP`] entries and to the read
    /// window in bytes, whatever the size of one entry, so every entry kept
    /// is read again and the log never grows without bound (audit
    /// 2026-11-22 ARCH-401: past the window it was never trimmed).
    fn append_activity(
        &self,
        id: &str,
        mut entry: ActivityEntry,
        log: LogRead,
    ) -> Result<(LogUndo, Option<Vec<ActivityEntry>>), StoreError> {
        self.library_present()?;
        // A summary is one sentence for the user (ARCH-105).
        entry.summary = crate::edit::capped(&entry.summary, crate::edit::MAX_SUMMARY_CHARS);
        let io = |e| StoreError::io(Some(id), e);
        own_folder(&self.activity_dir()).map_err(io)?;
        let path = log.path.clone();
        let line = serde_json::to_string(&entry).map_err(|e| io(e.into()))?;
        let mut f = open_regular_for_write(
            &path,
            fs::OpenOptions::new().create(true).append(true).read(true),
        )
        .map_err(io)?;
        let old_len = f.metadata().map_err(io)?.len();
        // A torn last line (no newline) would swallow this entry too.
        let torn = {
            use std::io::{Read, Seek};
            let mut last = [0u8; 1];
            old_len > 0
                && f.seek(std::io::SeekFrom::End(-1)).is_ok()
                && f.read_exact(&mut last).is_ok()
                && last[0] != b'\n'
        };
        let undo = if old_len == 0 && !log.there {
            LogUndo::Remove(path)
        } else {
            LogUndo::Truncate(path, old_len)
        };
        let written = (|| {
            if torn {
                writeln!(f)?;
            }
            writeln!(f, "{line}")?;
            sync_file(&f)
        })();
        if let Err(e) = written {
            let _ = undo.apply();
            return Err(io(e));
        }
        let len = f.metadata().map_or(u64::MAX, |m| m.len());
        // Trimmed when there are too many entries, or when the file is
        // longer than what is read of it.
        let trim = (log.entries.len() >= ACTIVITY_KEEP || len > self.activity_window).then(|| {
            let mut entries = log.entries;
            entries.push(entry);
            entries
        });
        Ok((undo, trim))
    }

    /// The text of the change log `entries` (oldest first, the new one
    /// last) trimmed to [`ACTIVITY_KEEP`] entries and the read window.
    fn trimmed_log(&self, id: &str, entries: &[ActivityEntry]) -> String {
        trim_log(
            entries,
            self.pending_start(id, entries),
            self.activity_window,
        )
    }

    /// Recorded changes of a project, oldest first.
    pub fn activity(&self, id: &str) -> Vec<ActivityEntry> {
        let Ok(path) = self.activity_path(id) else {
            return Vec::new();
        };
        // Read bytes, not a string: one torn or invalid line (an interrupted
        // write) must cost that line only, not the whole history (ERR-005).
        // Bounded, and never blocked by a pipe in its place (ARCH-301).
        let Ok(bytes) = read_newest_lines(&path, self.activity_window) else {
            return Vec::new();
        };
        parse_log(&bytes)
    }

    /// The change log of `id` as the app's history and MCP `get_history`
    /// show it: [`Store::activity`] as the next save keeps it, at most
    /// [`ACTIVITY_KEEP`] entries. A longer log (another program wrote it)
    /// froze the job's AI panel and flooded an AI client (ARCH-503).
    pub fn history(&self, id: &str) -> Vec<ActivityEntry> {
        let log = self.activity(id);
        if log.len() <= ACTIVITY_KEEP {
            return log;
        }
        parse_log(self.trimmed_log(id, &log).as_bytes())
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
        let pos = latest_undone(&log).ok_or_else(|| StoreError::NothingToRevert(id.to_string()))?;
        // Reverting from `pos` also undoes the reverts logged after it, so
        // the job ends as it was before that change.
        self.revert_from(
            id,
            &log,
            pos,
            actor,
            with_reason(reason, format!("{REVERTED_PREFIX}{}", log[pos].summary)),
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
        // The log is a file like the job: a version outside the limits is
        // not written back into the job (HUNTIO-004).
        target
            .check_limits()
            .map_err(|detail| StoreError::Unreadable {
                id: id.to_string(),
                detail,
            })?;
        let undid_from = Some(log[pos].ts_ms);
        match self.save_checked_undoing(id, &target, actor, summary, base.as_ref(), undid_from)? {
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
            .and_then(|p| read_regular(&p, MAX_SMALL_FILE).ok())
            .and_then(|t| serde_json::from_slice::<ReviewMark>(&t).ok())
            .map_or(0, |m| m.accepted_ms)
    }

    /// Accepts every change recorded at or before `ts_ms`. Never moves the
    /// mark back, so a late call from a second window cannot reopen a review.
    pub fn mark_reviewed(&self, id: &str, ts_ms: u64) -> Result<u64, StoreError> {
        let path = self.review_path(id)?;
        let accepted_ms = self.reviewed_at(id).max(ts_ms);
        let io = |e| StoreError::io(Some(id), e);
        own_folder(&self.activity_dir()).map_err(io)?;
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
        let _lock = self.lock(id)?;
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
            job_in_trash: None,
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
        // Files removed from this job earlier now belong to the deleted job.
        self.relink_removed_files(
            |e| e.id == id && e.job_in_trash.is_none(),
            |e| {
                e.job_in_trash = Some(info.entry.clone());
            },
        );
        self.prune_trash();
        Ok(info)
    }

    /// Rewrites the notes of the removed files `pick` selects.
    fn relink_removed_files(
        &self,
        pick: impl Fn(&TrashEntry) -> bool,
        change: impl Fn(&mut TrashEntry),
    ) {
        for mut e in self.trash() {
            if e.file.is_some() && pick(&e) {
                change(&mut e);
                let _ = self.write_trash_info(&self.trash_dir().join(&e.entry), &e);
            }
        }
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
            own_folder(&self.trash_dir())?;
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
        // Only real folders in a real trash: a link there would bring files
        // from outside into a job, or let pruning delete them (HUNT-503).
        if !(is_own_folder(&self.trash_dir()) && is_own_folder(&folder)) {
            return None;
        }
        let text = read_regular(&folder.join(TRASH_INFO), MAX_SMALL_FILE).ok()?;
        let mut info: TrashEntry = serde_json::from_slice(&text).ok()?;
        // The folder name is what restores it, whatever the file says.
        info.entry = entry.to_string();
        valid_id(&info.id).then_some((folder, info))
    }

    /// Deleted jobs and removed files in the trash, newest first.
    pub fn trash(&self) -> Vec<TrashEntry> {
        if !is_own_folder(&self.trash_dir()) {
            return Vec::new();
        }
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
            // Its job is deleted (another job may have its id now): the job
            // has to come back first.
            if info.job_in_trash.is_some() || !self.exists(&info.id) {
                return Err(StoreError::NotFound(info.id.clone()));
            }
            let src = folder.join("files").join(name);
            // The free name is taken by an empty file the move replaces.
            let (reserved, used) =
                self.reserve_attachment_name(&info.id, name)
                    .map_err(|e| StoreError::Io {
                        id: Some(info.id.clone()),
                        kind: if e.code == "foreign_entry" {
                            IoKind::Foreign
                        } else {
                            IoKind::Other
                        },
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
        let _creating = self.creation_lock()?;
        let id = if self.id_used(&info.id) {
            self.new_id(if info.name.is_empty() {
                &info.id
            } else {
                &info.name
            })
        } else {
            info.id.clone()
        };
        let _lock = self.lock(&id)?;
        // Into the library's own folders, never through a link (HUNT-503).
        own_folder(&self.activity_dir()).map_err(io)?;
        own_folder(&self.dir.join(FILES_DIR)).map_err(io)?;
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
        // Files removed from the job before it was deleted can come back
        // into it again, under the id it has now.
        self.relink_removed_files(
            |e| e.job_in_trash.as_deref() == Some(entry),
            |e| {
                e.id = id.clone();
                e.job_in_trash = None;
            },
        );
        Ok(id)
    }

    /// [`Store::files_dir`], created when missing; refused when it or
    /// `.files` is a link, so attached drawings never land outside the
    /// library (HUNT-503).
    fn own_files_dir(&self, id: &str) -> Result<PathBuf, FileError> {
        let dir = self.files_dir(id)?;
        let files = self.dir.join(FILES_DIR);
        own_folder(&files).map_err(|e| FileError::io(&files, e))?;
        own_folder(&dir).map_err(|e| FileError::io(&dir, e))?;
        Ok(dir)
    }

    /// Folder of the files attached to job `id`.
    pub fn files_dir(&self, id: &str) -> Result<PathBuf, StoreError> {
        self.path(id)?;
        Ok(self.dir.join(FILES_DIR).join(id))
    }

    /// Whether a file of this name may be attached through the MCP server:
    /// drawings, photos, documents and spreadsheets, also the types that do
    /// not open by one click ([`Store::opens_directly`]).
    pub fn can_attach(name: &str) -> bool {
        const ATTACHABLE: &[&str] = &[
            "pdf", "png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff", "heic", "svg", "txt",
            "csv", "dxf", "dwg", "xlsx", "xls", "ods", "docx", "doc", "odt",
        ];
        // Only the last extension counts: `plan.pdf.exe` is an `.exe`.
        name.rsplit_once('.')
            .map(|(stem, ext)| {
                !stem.is_empty() && ATTACHABLE.contains(&ext.to_ascii_lowercase().as_str())
            })
            .unwrap_or(false)
    }

    /// Whether an attached file may be opened with the system's default app
    /// by one click: drawings, photos and documents that carry no script.
    /// Anything else (`.exe`, `.command`, `.lnk`, `.js`, `.app`, ...) may
    /// have come in a received bundle; extracted by Tectora it carries no
    /// "downloaded from the internet" mark, so the system would run it
    /// without a warning. The same holds for `.svg` (the browser runs its
    /// script) and the legacy `.xls` and `.doc` (macros, and Office opens
    /// them without Protected View), so those, like `.xlsm`, are shown in
    /// their folder instead (ARCH-005). The same on macOS and Windows.
    pub fn opens_directly(name: &str) -> bool {
        const WITH_CODE: &[&str] = &["svg", "xls", "doc"];
        let ext = name
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_ascii_lowercase())
            .unwrap_or_default();
        Self::can_attach(name) && !WITH_CODE.contains(&ext.as_str())
    }

    pub fn attachment_path(&self, id: &str, name: &str) -> Result<PathBuf, FileError> {
        if !valid_file_name(name) {
            return Err(FileError::new(
                "invalid_name",
                format!("invalid file name: {name}"),
            ));
        }
        let path = self.files_dir(id)?.join(name);
        // Files are attached under their composed name: a decomposed
        // spelling (copied from macOS Finder) names the same file
        // (HUNT-108).
        let composed = crate::nfc::nfc(name);
        if composed != name && !path.exists() {
            return Ok(self.files_dir(id)?.join(composed.as_ref()));
        }
        Ok(path)
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
        // Composed, so one name decomposed (from macOS) and precomposed is
        // one file, never two that look the same (HUNTIO-007).
        let name = crate::nfc::nfc(name.trim()).into_owned();
        let name = name.as_str();
        self.attachment_path(id, name)?;
        self.own_files_dir(id)?;
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
    /// other end closes) is refused before it is opened (ARCH-001). Only a
    /// type [`Store::can_attach`] takes (`not_attachable`): a bundle import
    /// refuses any other, so the job could no longer be handed over
    /// (HUNTIO-001). The same rule for the app and the MCP server.
    pub fn attach_file(&self, id: &str, src: &Path) -> Result<String, FileError> {
        let name = src.file_name().and_then(|n| n.to_str()).ok_or_else(|| {
            FileError::new("not_a_file", format!("{}: not a file", src.display()))
        })?;
        let file = open_regular(src, MAX_ATTACHMENT)?;
        if !Self::can_attach(name) {
            return Err(FileError::new(
                "not_attachable",
                format!(
                    "{name}: only drawings, photos and documents can be attached (pdf, png, jpg, …, dwg, xlsx, docx)"
                ),
            ));
        }
        let mut bytes = Vec::new();
        std::io::Read::take(file, MAX_ATTACHMENT + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| FileError::io(src, e))?;
        if bytes.len() as u64 > MAX_ATTACHMENT {
            return Err(too_large_file(src, MAX_ATTACHMENT));
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
            job_in_trash: None,
        };
        // Without its note the file could not be found in the trash again:
        // it goes back to the job.
        if let Err(e) = self.write_trash_info(&folder, &info) {
            let _ = fs::rename(dest.join(name), &path);
            let _ = fs::remove_dir_all(&folder);
            return Err(FileError::io(&folder, e));
        }
        self.prune_trash();
        Ok(info)
    }

    /// Copies all attachments of job `from` to job `to`.
    pub fn copy_attachments(&self, from: &str, to: &str) -> Result<(), FileError> {
        for a in self.attachments(from) {
            let path = self.attachment_path(from, &a.name)?;
            let bytes = read_regular(&path, MAX_ATTACHMENT)?;
            self.add_attachment(to, &a.name, &bytes)?;
        }
        Ok(())
    }

    pub fn set_live(&self, status: &LiveStatus) {
        if self.dir.is_dir()
            && own_folder(&self.activity_dir()).is_ok()
            && let Ok(text) = serde_json::to_string(status)
        {
            let _ = write_atomic(&self.activity_dir().join(LIVE_FILE), &text);
        }
    }

    pub fn live(&self) -> Option<LiveStatus> {
        let text = read_regular(&self.activity_dir().join(LIVE_FILE), MAX_SMALL_FILE).ok()?;
        serde_json::from_slice(&text).ok()
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
/// How many log entries before it a revert entry undid: 1 for
/// `Reverted: …`, n for `Reverted n changes since: …`, 0 for a change.
fn reverted_count(summary: &str) -> usize {
    if summary.starts_with(REVERTED_PREFIX) {
        return 1;
    }
    summary
        .strip_prefix("Reverted ")
        .and_then(|r| r.split_once(" changes since: "))
        .and_then(|(n, _)| n.parse().ok())
        .unwrap_or(0)
}

/// The latest logged change that no later revert undid. A second "revert
/// the latest change" used to revert the first revert and so put the change
/// back (audit 2026-11-15 HUNTIO-002); it now undoes one more step.
fn latest_undone(log: &[ActivityEntry]) -> Option<usize> {
    let mut i = log.len();
    while i > 0 {
        i -= 1;
        let e = &log[i];
        if let Some(from) = e.undid_from {
            // Everything from that entry on is undone; go on before it.
            i = log[..i].iter().rposition(|x| x.ts_ms == from)?;
            continue;
        }
        // A revert logged before reverts recorded what they undid.
        match reverted_count(&e.summary) {
            0 => return Some(i),
            n => i = i.saturating_sub(n),
        }
    }
    None
}

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

    /// Audit 2026-11-21 ARCH-204: a named pipe or a link to a device named
    /// like a job is listed as broken without waiting or reading for ever.
    #[cfg(unix)]
    #[test]
    fn a_pipe_or_device_named_like_a_job_is_broken_not_a_hang() {
        let store = temp_store("fifo");
        let id = store
            .save(None, &examples::find("hip").unwrap().project())
            .unwrap();
        let fifo = store.dir.join("f.tectora");
        let made = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .is_ok_and(|s| s.success());
        std::os::unix::fs::symlink("/dev/zero", store.dir.join("z.tectora")).unwrap();
        let started = std::time::Instant::now();
        let (ok, broken) = store.scan().unwrap();
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
        assert!(ok.iter().any(|s| s.id == id));
        let codes: Vec<(&str, &str)> = broken.iter().map(|b| (b.id.as_str(), b.code)).collect();
        assert!(codes.contains(&("z", "unreadable")), "{codes:?}");
        if made {
            assert!(codes.contains(&("f", "unreadable")), "{codes:?}");
        }
        assert_eq!(store.load("z").unwrap_err().code(), "unreadable");
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// Audit 2026-11-22 ARCH-301: a named pipe in place of a job's change
    /// log, review mark, the live file, a trash note or the job file itself
    /// never hangs a save, the library list, the trash or a bundle export.
    #[cfg(unix)]
    #[test]
    fn a_pipe_in_place_of_the_log_or_a_mark_never_hangs_a_save() {
        let store = temp_store("fifo-log");
        let project = examples::find("hip").unwrap().project();
        let id = store.save_as(None, &project, "user", "Made.").unwrap();
        let mkfifo = |p: &Path| {
            let _ = fs::remove_file(p);
            std::process::Command::new("mkfifo")
                .arg(p)
                .status()
                .is_ok_and(|s| s.success())
        };
        let act = store.activity_dir();
        let trash = store.trash_dir().join("x-1");
        fs::create_dir_all(&trash).unwrap();
        if ![
            act.join(format!("{id}.jsonl")),
            act.join(format!("{id}.review.json")),
            act.join(LIVE_FILE),
            trash.join(TRASH_INFO),
            store.dir.join("f.tectora"),
        ]
        .iter()
        .all(|p| mkfifo(p))
        {
            eprintln!("mkfifo not available, skipped");
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let dir = store.dir.clone();
        std::thread::spawn(move || {
            let store = Store::new(dir).unwrap();
            let mut p = examples::find("hip").unwrap().project();
            let _ = store.activity(&id);
            let _ = store.reviewed_at(&id);
            let _ = store.live();
            let _ = store.trash();
            let _ = store.scan();
            p.notes = "changed".into();
            let saved = store.save_as(Some(id.clone()), &p, "user", "Changed.");
            let _ = store.save_checked("f", &p, &p);
            let out = store.dir.join("out.tectorajob");
            let exported = crate::bundle::export(&store, &id, &out);
            let _ = tx.send((saved.is_ok(), exported.is_ok()));
        });
        let done = rx.recv_timeout(std::time::Duration::from_secs(20));
        assert!(done.is_ok(), "a read of a pipe hung");
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// Audit 2026-11-22 HUNT-305 item 5: a job of hand-drawn faces only has
    /// a thumbnail on its card.
    #[test]
    fn the_card_of_a_job_of_faces_only_has_a_thumbnail() {
        let store = temp_store("card-faces");
        let mut p = Project::new("Faces");
        p.manual_faces.push(crate::model::ManualFace {
            points: vec![
                crate::geom::Point2::new(0.0, 0.0),
                crate::geom::Point2::new(4.0, 0.0),
                crate::geom::Point2::new(4.0, 4.0),
            ],
            pitch_deg: 30.0,
            label: String::new(),
        });
        let id = store.save(None, &p).unwrap();
        let s = store.summarize(&id).unwrap();
        assert_eq!(s.outlines, vec![vec![[0.0, 0.0], [4.0, 0.0], [4.0, 4.0]]]);
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// Audit 2026-11-22 TEST-404 (N13): a card whose calculation failed is
    /// without a roof but not "too complex"; only the work bound says that.
    #[test]
    fn a_card_refused_for_a_failure_is_not_too_complex() {
        let p = examples::find("hip").unwrap().project();
        let ((area, lines, failed, differs), too_complex) =
            refused_card(&p, &crate::work::Refusal::Failed("boom".into()));
        assert!(!too_complex);
        assert_eq!(
            (area, lines.len(), failed, differs),
            (0.0, 0, p.parts.len(), false)
        );
        assert!(refused_card(&p, &crate::work::Refusal::TooComplex).1);
    }

    /// Audit 2026-11-22 TEST-504: the card `summarize` hands out for a job
    /// whose calculation failed is without a roof but not "too complex"
    /// (testing `refused_card` alone let `summarize` mark it so again).
    #[test]
    fn a_listed_job_whose_calculation_fails_is_not_too_complex() {
        let store = temp_store("card-failed");
        let mut p = examples::find("hip").unwrap().project();
        p.name = CARD_FAILS.into();
        let id = store.save(None, &p).unwrap();
        let card = store.summarize(&id).unwrap();
        assert!(!card.figures_pending);
        assert!(!card.too_complex, "a failure is not the work bound");
        assert_eq!(card.parts_failed, p.parts.len());
        assert_eq!(card.roof_area, 0.0);
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// Audit 2026-11-22 ARCH-503: a change log longer than a log keeps
    /// (written by another program, or by an older Tectora's import) is
    /// handed to the app and MCP as the next save keeps it: the newest
    /// entries, and the first change waiting for review in place of the
    /// oldest kept one.
    #[test]
    fn the_history_handed_out_is_what_the_next_save_keeps() {
        let store = temp_store("history-cap");
        let p = examples::find("hip").unwrap().project();
        let id = store.save(None, &p).unwrap();
        let entry = |i: u64, actor: &str| {
            let mut after = p.clone();
            after.notes = format!("{i}");
            let mut before = p.clone();
            before.notes = format!("{}", i - 1);
            ActivityEntry {
                ts_ms: i,
                actor: actor.into(),
                summary: format!("change {i}"),
                changes: Vec::new(),
                before: Some(before),
                after: Some(after),
                undid_from: None,
            }
        };
        let write = |entries: &[ActivityEntry]| {
            fs::create_dir_all(store.activity_dir()).unwrap();
            let text: String = entries
                .iter()
                .map(|e| serde_json::to_string(e).unwrap() + "\n")
                .collect();
            fs::write(store.activity_path(&id).unwrap(), text).unwrap();
        };
        let n = ACTIVITY_KEEP as u64 + 30;
        write(&(1..=n).map(|i| entry(i, USER_ACTOR)).collect::<Vec<_>>());
        let h = store.history(&id);
        assert_eq!(h.len(), ACTIVITY_KEEP);
        assert_eq!(h[0].summary, "change 31");
        assert_eq!(h.last().unwrap().summary, format!("change {n}"));
        // Every change by an AI waits for review: the first one is kept.
        write(&(1..=n).map(|i| entry(i, "claude")).collect::<Vec<_>>());
        let h = store.history(&id);
        assert_eq!(h.len(), ACTIVITY_KEEP);
        assert_eq!(h[0].ts_ms, 1);
        assert_eq!(h[0].before.as_ref().unwrap().notes, "0");
        // A log within the bound is handed out as it is.
        write(&(1..=3).map(|i| entry(i, "claude")).collect::<Vec<_>>());
        assert_eq!(
            serde_json::to_value(store.history(&id)).unwrap(),
            serde_json::to_value(store.activity(&id)).unwrap()
        );
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// Audit 2026-11-22 ARCH-401: entries so large that the log's entries
    /// do not fit in the read window still keep the log bounded (in
    /// entries and in bytes), and the entry with the user's version from
    /// before the AI started is kept, so "Revert all" returns to it.
    #[test]
    fn a_log_of_large_entries_stays_bounded_and_keeps_the_pre_ai_version() {
        const WINDOW: u64 = 600_000;
        let store = temp_store("big-log").with_activity_window(WINDOW);
        let mut p = examples::find("gable").unwrap().project();
        let id = store.save_as(None, &p, USER_ACTOR, "create").unwrap();
        let mine = p.clone();
        let log = store.activity_path(&id).unwrap();
        for i in 1..=ACTIVITY_KEEP + 10 {
            // About 4 x 30 kB per entry: some ten entries fill the window.
            p.notes = format!("{i} {}", "x".repeat(30_000));
            store
                .save_as(Some(id.clone()), &p, "claude", &format!("notes {i}"))
                .unwrap();
            let len = fs::metadata(&log).unwrap().len();
            assert!(len <= WINDOW, "after save {i} the log is {len} bytes");
        }
        let text = fs::read_to_string(&log).unwrap();
        assert!(text.lines().count() <= ACTIVITY_KEEP);
        let pending = store.pending_review(&id).unwrap();
        assert_eq!(pending.before.as_ref(), Some(&mine), "the pre-AI version");
        assert_eq!(store.activity(&id).len(), text.lines().count());
        assert_eq!(
            store
                .revert_to(&id, pending.since_ts_ms, USER_ACTOR, None)
                .unwrap(),
            mine
        );
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// Audit 2026-11-22 ARCH-401: a log already longer than the read window
    /// (written by an earlier build) is trimmed by the next save, and a
    /// save whose job file cannot be written leaves the log as it was,
    /// entries outside the window included.
    #[test]
    fn a_log_longer_than_the_window_is_trimmed_and_a_failed_save_keeps_it_whole() {
        const WINDOW: u64 = 200_000;
        let store = temp_store("long-log").with_activity_window(WINDOW);
        let mut p = examples::find("hip").unwrap().project();
        let id = store.save_as(None, &p, USER_ACTOR, "create").unwrap();
        let log = store.activity_path(&id).unwrap();
        let line = fs::read_to_string(&log).unwrap();
        // 60 entries over the window, as an earlier build left them.
        let mut old = String::new();
        while (old.len() as u64) < 3 * WINDOW {
            old.push_str(&line);
        }
        fs::write(&log, &old).unwrap();
        // The job file cannot be replaced: a folder stands in its place.
        let path = store.path(&id).unwrap();
        let saved = fs::read(&path).unwrap();
        fs::remove_file(&path).unwrap();
        fs::create_dir_all(path.join("blocker")).unwrap();
        p.notes = "AI".into();
        assert!(
            store
                .save_as(Some(id.clone()), &p, "claude", "notes")
                .is_err()
        );
        assert_eq!(fs::read_to_string(&log).unwrap(), old, "the log as it was");
        fs::remove_dir_all(&path).unwrap();
        fs::write(&path, saved).unwrap();
        store
            .save_as(Some(id.clone()), &p, "claude", "notes")
            .unwrap();
        let text = fs::read_to_string(&log).unwrap();
        assert!(text.len() as u64 <= WINDOW, "{} bytes", text.len());
        assert!(text.lines().count() <= ACTIVITY_KEEP);
        assert_eq!(store.activity(&id).last().unwrap().summary, "notes");
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// Audit 2026-11-22 HUNT-402, HUNT-403: a named pipe or a link in place
    /// of a job's lock, the creation lock or the change log never hangs a
    /// save and never writes outside the library.
    #[cfg(unix)]
    #[test]
    fn a_pipe_or_link_in_place_of_a_lock_or_the_log_never_hangs_or_writes_outside() {
        let store = temp_store("lock-odd");
        let project = examples::find("hip").unwrap().project();
        let id = store.save_as(None, &project, USER_ACTOR, "Made.").unwrap();
        let act = store.activity_dir();
        let outside = std::env::temp_dir().join(format!("tectora-outside-{}", std::process::id()));
        let _ = fs::remove_dir_all(&outside);
        fs::create_dir_all(&outside).unwrap();
        let victim = outside.join("victim.txt");
        fs::write(&victim, "# victim\n").unwrap();
        let mkfifo = |p: &Path| {
            let _ = fs::remove_file(p);
            std::process::Command::new("mkfifo")
                .arg(p)
                .status()
                .is_ok_and(|s| s.success())
        };
        let fifos = mkfifo(&act.join(format!("{id}.lock"))) && mkfifo(&act.join(".new.lock"));
        let (tx, rx) = std::sync::mpsc::channel();
        let dir = store.dir.clone();
        let id2 = id.clone();
        std::thread::spawn(move || {
            let store = Store::new(dir).unwrap();
            let mut p = examples::find("hip").unwrap().project();
            p.notes = "through a pipe".into();
            let saved = store.save_as(Some(id2.clone()), &p, "claude", "Changed.");
            let created = store.save_as(None, &p, "claude", "New.");
            let _ = tx.send((saved.is_ok(), created.is_ok()));
        });
        let done = rx.recv_timeout(std::time::Duration::from_secs(10));
        assert!(done.is_ok(), "a pipe as a lock file hung the save");
        if fifos {
            assert_eq!(done.unwrap(), (true, true));
            assert!(
                fs::symlink_metadata(act.join(format!("{id}.lock")))
                    .unwrap()
                    .is_file()
            );
        }
        // A link as the lock: its target is never created.
        let lock = act.join(format!("{id}.lock"));
        fs::remove_file(&lock).unwrap();
        std::os::unix::fs::symlink(outside.join("created-by-lock"), &lock).unwrap();
        let mut p = project.clone();
        p.notes = "lock link".into();
        store
            .save_as(Some(id.clone()), &p, "claude", "Lock.")
            .unwrap();
        assert!(!outside.join("created-by-lock").exists());
        // A link as the change log: nothing is appended to its target.
        let log = store.activity_path(&id).unwrap();
        fs::remove_file(&log).unwrap();
        std::os::unix::fs::symlink(&victim, &log).unwrap();
        p.notes = "log link".into();
        let saved = store.save_as(Some(id.clone()), &p, "claude", "Log.");
        assert!(saved.is_err(), "{saved:?}");
        assert_eq!(fs::read_to_string(&victim).unwrap(), "# victim\n");
        let _ = fs::remove_dir_all(&store.dir);
        let _ = fs::remove_dir_all(&outside);
    }

    /// Audit 2026-11-22 HUNT-503: a link in place of one of the library's
    /// own folders (`.activity`, `.files`, `.files/<id>`, `.trash`) is
    /// refused with `foreign_entry`; nothing is written outside the
    /// library through it.
    #[cfg(unix)]
    #[test]
    fn a_link_in_place_of_a_library_folder_is_refused_and_nothing_lands_outside() {
        let project = examples::find("hip").unwrap().project();
        let outside = std::env::temp_dir().join(format!("tectora-dirlink-{}", now_ms()));
        let empty = |what: &str| {
            let left: Vec<_> = fs::read_dir(&outside).unwrap().flatten().collect();
            assert!(left.is_empty(), "{what}: written outside: {left:?}");
        };
        let link = |store: &Store, rel: &str| {
            let at = store.dir.join(rel);
            let _ = fs::remove_dir_all(&at);
            std::os::unix::fs::symlink(&outside, &at).unwrap();
        };
        fs::create_dir_all(&outside).unwrap();
        // `.activity`: logged saves, plain saves and new jobs.
        let store = temp_store("dirlink-activity");
        let id = store.save(None, &project).unwrap();
        link(&store, ACTIVITY_DIR);
        let mut p = project.clone();
        p.notes = "changed".into();
        let e = store
            .save_as(Some(id.clone()), &p, "claude", "Changed.")
            .unwrap_err();
        assert_eq!(e.code(), "foreign_entry", "{e}");
        assert!(e.to_string().contains(ACTIVITY_DIR), "{e}");
        assert_eq!(
            store.save(Some(id.clone()), &p).unwrap_err().code(),
            "foreign_entry"
        );
        assert_eq!(store.save(None, &p).unwrap_err().code(), "foreign_entry");
        assert_eq!(
            store.mark_reviewed(&id, 1).unwrap_err().code(),
            "foreign_entry"
        );
        let live: LiveStatus = serde_json::from_value(serde_json::json!({
            "ts_ms": 1, "actor": "claude", "message": "m", "project_id": null
        }))
        .unwrap();
        store.set_live(&live);
        assert_eq!(store.load(&id).unwrap(), project, "the job is unchanged");
        empty(".activity");
        let _ = fs::remove_dir_all(&store.dir);
        // `.files` and `.files/<id>`: attachments.
        for rel in [FILES_DIR.to_string(), format!("{FILES_DIR}/a")] {
            let store = temp_store("dirlink-files");
            let id = store.save(Some("a".into()), &project).unwrap();
            fs::create_dir_all(store.dir.join(FILES_DIR)).unwrap();
            link(&store, &rel);
            let e = store.add_attachment(&id, "plan.pdf", b"%PDF").unwrap_err();
            assert_eq!(e.code, "foreign_entry", "{rel}: {e}");
            empty(&rel);
            let _ = fs::remove_dir_all(&store.dir);
        }
        // `.trash`: a delete leaves the job where it is.
        let store = temp_store("dirlink-trash");
        let id = store.save_as(None, &project, USER_ACTOR, "Made.").unwrap();
        link(&store, TRASH_DIR);
        let e = store.delete(&id, USER_ACTOR, "Gone.").unwrap_err();
        assert_eq!(e.code(), "foreign_entry", "{e}");
        assert!(store.exists(&id));
        empty(".trash");
        // A trash folder outside is neither listed, restored nor pruned.
        let old = outside.join("1-old");
        fs::create_dir_all(&old).unwrap();
        fs::write(
            old.join(TRASH_INFO),
            r#"{"entry":"1-old","id":"old","name":"","ts_ms":1,"actor":"user","summary":""}"#,
        )
        .unwrap();
        fs::write(old.join("old.tectora"), project.to_json().unwrap()).unwrap();
        assert!(store.trash().is_empty());
        assert!(store.restore("1-old").is_err());
        store.add_attachment(&id, "x.pdf", b"%PDF").unwrap();
        assert!(
            store
                .remove_attachment(&id, "x.pdf", USER_ACTOR, "x")
                .is_err()
        );
        assert!(
            old.join("old.tectora").exists(),
            "pruned outside the library"
        );
        let _ = fs::remove_dir_all(&store.dir);
        let _ = fs::remove_dir_all(&outside);
    }

    /// Audit 2026-11-22 HUNT-501: a folder in place of a job's lock file
    /// turned locking off for good (two writers then overwrote each other).
    /// An empty folder is replaced by a lock file; a folder with files in
    /// it refuses the save, `foreign_entry`, and the job stays as it was.
    #[test]
    fn a_folder_in_place_of_a_lock_file_is_replaced_or_refuses_the_save() {
        let store = temp_store("lock-dir");
        let project = examples::find("hip").unwrap().project();
        let id = store.save_as(None, &project, USER_ACTOR, "Made.").unwrap();
        let lock = store.activity_dir().join(format!("{id}.lock"));
        let _ = fs::remove_file(&lock);
        fs::create_dir(&lock).unwrap();
        let mut p = project.clone();
        p.notes = "one".into();
        store
            .save_as(Some(id.clone()), &p, "claude", "One.")
            .unwrap();
        assert!(
            fs::symlink_metadata(&lock).unwrap().is_file(),
            "a lock file again"
        );
        // A folder with something in it is not Tectora's to remove.
        fs::remove_file(&lock).unwrap();
        fs::create_dir(&lock).unwrap();
        fs::write(lock.join("keep.txt"), "x").unwrap();
        p.notes = "two".into();
        let e = store
            .save_as(Some(id.clone()), &p, "claude", "Two.")
            .unwrap_err();
        assert_eq!(e.code(), "foreign_entry", "{e}");
        assert!(e.to_string().contains("lock"), "{e}");
        assert_eq!(
            store.save(Some(id.clone()), &p).unwrap_err().code(),
            "foreign_entry"
        );
        assert_eq!(store.load(&id).unwrap().notes, "one");
        assert_eq!(fs::read_to_string(lock.join("keep.txt")).unwrap(), "x");
        // The same for the creation lock of new jobs.
        let new = store.activity_dir().join(".new.lock");
        let _ = fs::remove_file(&new);
        fs::create_dir(&new).unwrap();
        fs::write(new.join("keep.txt"), "x").unwrap();
        assert_eq!(store.save(None, &p).unwrap_err().code(), "foreign_entry");
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// Audit 2026-11-22 HUNT-502: a writer that saw a pipe at the lock path
    /// before another one replaced it never removes the other's live lock
    /// file; a second writer then waits for that lock.
    #[cfg(unix)]
    #[test]
    fn a_stale_look_at_an_odd_lock_never_removes_a_live_lock_file() {
        use std::os::unix::fs::MetadataExt;
        let store = temp_store("lock-stale");
        fs::create_dir_all(store.activity_dir()).unwrap();
        let path = store.activity_dir().join("x.lock");
        let held = take_lock_file(&path).unwrap().unwrap();
        let ino = fs::metadata(&path).unwrap().ino();
        // What the other writer does after it saw the pipe that was there.
        replace_odd_lock(&path).unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().ino(),
            ino,
            "the live lock was removed"
        );
        let (tx, rx) = std::sync::mpsc::channel();
        let p = path.clone();
        std::thread::spawn(move || {
            let second = take_lock_file(&p).unwrap().unwrap();
            let _ = tx.send(());
            drop(second);
        });
        assert!(
            rx.recv_timeout(std::time::Duration::from_millis(300))
                .is_err(),
            "a second writer took the lock while it was held"
        );
        drop(held);
        assert!(rx.recv_timeout(std::time::Duration::from_secs(10)).is_ok());
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// Audit 2026-11-22 HUNT-501, HUNT-502: two writers that find an odd
    /// entry (a folder, a pipe, a link) at a job's lock path at the same
    /// moment both end up holding one lock: no change is lost.
    #[cfg(unix)]
    #[test]
    fn two_writers_finding_an_odd_lock_at_once_lose_no_change() {
        let store = temp_store("lock-race");
        let project = examples::find("hip").unwrap().project();
        let id = store.save_as(None, &project, USER_ACTOR, "Made.").unwrap();
        let lock = store.activity_dir().join(format!("{id}.lock"));
        let outside = std::env::temp_dir().join(format!("tectora-race-link-{}", now_ms()));
        let mut lost = Vec::new();
        for trial in 0..60 {
            let _ = fs::remove_file(&lock);
            let _ = fs::remove_dir(&lock);
            match trial % 3 {
                0 => fs::create_dir(&lock).unwrap(),
                1 => {
                    let made = std::process::Command::new("mkfifo")
                        .arg(&lock)
                        .status()
                        .is_ok_and(|s| s.success());
                    if !made {
                        fs::create_dir(&lock).unwrap();
                    }
                }
                _ => std::os::unix::fs::symlink(&outside, &lock).unwrap(),
            }
            let barrier = Arc::new(std::sync::Barrier::new(2));
            let writers: Vec<_> = ["a", "b"]
                .into_iter()
                .map(|who| {
                    let (dir, id, barrier) = (store.dir.clone(), id.clone(), Arc::clone(&barrier));
                    std::thread::spawn(move || {
                        let store = Store::at(dir);
                        barrier.wait();
                        loop {
                            let base = store.load(&id).unwrap();
                            let mut p = base.clone();
                            p.notes.push_str(&format!("{who}{trial};"));
                            match store.save_as_checked(&id, &p, "claude", "Note.", Some(&base)) {
                                Ok(CheckedSave::Saved { .. }) => return,
                                Ok(CheckedSave::Conflict { .. }) => continue,
                                Err(e) => panic!("{who}{trial}: {e}"),
                            }
                        }
                    })
                })
                .collect();
            for w in writers {
                w.join().unwrap();
            }
            let notes = store.load(&id).unwrap().notes;
            for who in ["a", "b"] {
                if !notes.contains(&format!("{who}{trial};")) {
                    lost.push(format!("{who}{trial}"));
                }
            }
            assert!(!outside.exists(), "a lock was made outside");
        }
        assert!(lost.is_empty(), "changes lost: {lost:?}");
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// Audit 2026-11-22 ARCH-402: a job at the face and corner limits (500
    /// hand-drawn faces of 500 corners, 200 buildings) gives a card of at
    /// most `MAX_CARD_POINTS` points that still spans the whole job.
    #[test]
    fn the_card_of_a_job_at_the_face_and_corner_limits_is_bounded() {
        use crate::geom::Point2;
        use crate::model::{MAX_OBJECTS, MAX_PART_WALLS, MAX_PARTS, ManualFace};
        let store = temp_store("card-bound");
        let ring = |cx: f64, cy: f64, r: f64, n: usize| -> Vec<Point2> {
            (0..n)
                .map(|i| {
                    let a = i as f64 / n as f64 * std::f64::consts::TAU;
                    Point2::new(cx + r * a.cos(), cy + r * a.sin())
                })
                .collect()
        };
        for (name, faces, corners) in [
            ("round", MAX_OBJECTS, MAX_PART_WALLS),
            ("small", MAX_OBJECTS, 3),
        ] {
            let mut p = examples::find("hip").unwrap().project();
            let part = p.parts[0].clone();
            while p.parts.len() < MAX_PARTS {
                p.parts.push(part.clone());
            }
            for i in 0..faces {
                p.manual_faces.push(ManualFace {
                    points: ring(i as f64 * 30.0, 0.0, 10.0, corners),
                    pitch_deg: 30.0,
                    label: String::new(),
                });
            }
            let id = store.save(Some(name.into()), &p).unwrap();
            let card = store.summarize(&id).unwrap();
            let points: usize = card.outlines.iter().map(Vec::len).sum();
            assert!(points <= MAX_CARD_POINTS, "{name}: {points} points");
            assert!(card.outlines.iter().all(|o| o.len() >= 3), "{name}");
            // HUNT-505: the rectangular buildings keep their 4 corners.
            assert_eq!(card.outlines[0].len(), 4, "{name}: {:?}", card.outlines[0]);
            let max_x = card
                .outlines
                .iter()
                .flatten()
                .map(|p| p[0])
                .fold(f64::MIN, f64::max);
            let far = (faces - 1) as f64 * 30.0 + 10.0;
            assert!(
                (max_x - far).abs() < 1.0,
                "{name}: the card spans to {max_x}, not {far}"
            );
            let text = serde_json::to_string(&card.outlines).unwrap();
            assert!(text.len() < 200_000, "{name}: {} bytes", text.len());
        }
        // A small job keeps every point.
        let p = examples::find("hip").unwrap().project();
        assert_eq!(card_outlines(&p), outlines(&p));
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// Audit 2026-11-22 HUNT-505: one face over the point budget turned
    /// every rectangle of a card (and the building) into a triangle while
    /// most of the budget stayed unused; small outlines are kept whole and
    /// only the large ones are thinned.
    #[test]
    fn a_card_over_its_point_budget_keeps_rectangles_whole() {
        let square = |x: f64| vec![[x, 0.0], [x + 1.0, 0.0], [x + 1.0, 1.0], [x, 1.0]];
        let building = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 8.0], [0.0, 8.0]];
        let mut squares = vec![building.clone()];
        squares.extend((0..500).map(|i| square(i as f64 * 2.0)));
        let card = bounded_outlines(squares, MAX_CARD_POINTS);
        assert_eq!(card[0], building);
        assert!(
            card.iter().all(|o| o.len() == 4),
            "a rectangle lost a corner"
        );
        let points: usize = card.iter().map(Vec::len).sum();
        assert!(points <= MAX_CARD_POINTS, "{points}");
        // The box of the ones left out still reaches the last square.
        let max_x = card.iter().flatten().map(|p| p[0]).fold(f64::MIN, f64::max);
        assert_eq!(max_x, 999.0);
        // Large outlines share what the small ones leave.
        let ring = |n: usize| -> Vec<[f64; 2]> { (0..n).map(|i| [i as f64, 1.0]).collect() };
        let mut mixed = vec![building.clone()];
        mixed.extend((0..5).map(|_| ring(400)));
        let card = bounded_outlines(mixed, MAX_CARD_POINTS);
        assert_eq!(card[0], building);
        let points: usize = card.iter().map(Vec::len).sum();
        assert!((1900..=MAX_CARD_POINTS).contains(&points), "{points}");
        assert_eq!(card.len(), 6);
    }

    /// Audit 2026-11-22 ERR-306: a job saved with a UTF-8 byte order mark
    /// (PowerShell 5, Notepad) opens; one in another encoding (cp1250) is
    /// unreadable as "not UTF-8 text", not an operating system error.
    #[test]
    fn a_job_with_a_byte_order_mark_opens_and_one_not_in_utf8_says_so() {
        let store = temp_store("bom");
        let p = examples::find("hip").unwrap().project();
        let mut bytes = b"\xef\xbb\xbf".to_vec();
        bytes.extend(p.to_json().unwrap().as_bytes());
        fs::write(store.dir.join("bom.tectora"), &bytes).unwrap();
        assert_eq!(store.load("bom").unwrap(), p);
        let mut q = p.clone();
        q.notes = "Pozn\u{e1}mka".into();
        // Each letter outside ASCII as one byte, as a Windows code page.
        let cp1250: Vec<u8> = q
            .to_json()
            .unwrap()
            .chars()
            .map(|c| if c.is_ascii() { c as u8 } else { 0xe1 })
            .collect();
        fs::write(store.dir.join("cp.tectora"), cp1250).unwrap();
        let e = store.load("cp").unwrap_err();
        assert_eq!(e.code(), "unreadable", "{e}");
        assert!(e.to_string().contains("not UTF-8 text"), "{e}");
        let (_, broken) = store.scan().unwrap();
        assert!(
            broken
                .iter()
                .any(|b| b.id == "cp" && b.code == "unreadable"),
            "{broken:?}"
        );
        assert!(!broken.iter().any(|b| b.id == "bom"), "{broken:?}");
        // The same for a file opened from elsewhere.
        let outside = std::env::temp_dir().join(format!("tectora-bom-{}.tectora", now_ms()));
        fs::write(&outside, &bytes).unwrap();
        let id = crate::bundle::open_path(&store, &outside).unwrap();
        assert_eq!(store.load(&id).unwrap().parts, p.parts);
        let _ = fs::remove_file(&outside);
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// ARCH-301: of a log over the bound only its newest whole lines are read.
    #[test]
    fn of_a_long_log_only_the_newest_whole_lines_are_read() {
        let store = temp_store("tail");
        let p = store.dir.join("log.jsonl");
        fs::write(&p, b"aaaa\nbbbb\ncccc\n").unwrap();
        assert_eq!(read_newest_lines(&p, 100).unwrap(), b"aaaa\nbbbb\ncccc\n");
        assert_eq!(read_newest_lines(&p, 10).unwrap(), b"bbbb\ncccc\n");
        assert_eq!(read_newest_lines(&p, 9).unwrap(), b"cccc\n");
        assert_eq!(read_newest_lines(&p, 3).unwrap(), b"");
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// ARCH-204: a job file over [`MAX_JOB_FILE`] is not read.
    #[test]
    fn a_job_file_over_the_size_bound_is_not_read() {
        let store = temp_store("huge");
        let f = fs::File::create(store.dir.join("big.tectora")).unwrap();
        f.set_len(MAX_JOB_FILE + 1).unwrap();
        let e = store.load("big").unwrap_err();
        assert_eq!(e.code(), "unreadable");
        assert!(e.to_string().contains("larger than"), "{e}");
        assert_eq!(store.load("none").unwrap_err().code(), "not_found");
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// Audit 2026-11-21 TEST-207: the store keeps at most
    /// `MAX_SUMMARY_CHARS` of a summary, ending in "…".
    #[test]
    fn a_long_summary_is_capped_in_the_log() {
        let store = temp_store("cap");
        let p = examples::find("hip").unwrap().project();
        let id = store.save_as(None, &p, "AI", &"s".repeat(5000)).unwrap();
        let log = store.activity(&id);
        let summary = &log.last().unwrap().summary;
        assert_eq!(
            summary.chars().count(),
            crate::edit::MAX_SUMMARY_CHARS,
            "{summary}"
        );
        assert!(summary.ends_with('…'));
        let _ = fs::remove_dir_all(&store.dir);
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
        // ARCH-005: these can carry scripts or macros and no download mark,
        // so their folder is shown; they can still be attached.
        for risky in ["plan.svg", "PLAN.SVG", "offer.xls", "letter.doc", "a.b.xls"] {
            assert!(!Store::opens_directly(risky), "{risky}");
            assert!(Store::can_attach(risky), "{risky}");
        }
        assert!(!Store::can_attach("x.exe") && !Store::can_attach("x.xlsm"));
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
        // HUNT-204 item 3: ligatures, `ß` and the three spellings of Å.
        assert_eq!(slug("ﬁle ﬂat"), "file-flat");
        assert_eq!(slug("Straße"), "strasse");
        for a in ["\u{c5}", "A\u{30a}", "\u{212b}"] {
            assert_eq!(slug(a), "a", "{a:?}");
        }
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

    #[test]
    fn a_vanished_jobs_folder_is_reported_not_created_again() {
        let store = temp_store("vanished");
        let p = examples::find("hip").unwrap().project();
        store.save_as(None, &p, USER_ACTOR, "create").unwrap();
        fs::remove_dir_all(&store.dir).unwrap();
        assert!(store.scan().is_err());
        store.set_live(&LiveStatus {
            ts_ms: 1,
            actor: "claude".into(),
            message: "x".into(),
            project_id: None,
            follow: false,
            kind: None,
            name: None,
        });
        assert!(store.save_as(None, &p, "claude", "again").is_err());
        assert!(store.save(None, &p).is_err());
        assert!(!store.dir.exists());
    }

    /// HUNTIO-011: a file removed from a job that was then deleted does not
    /// go into a new job that took the old id; it comes back with its job.
    #[test]
    fn a_removed_file_returns_to_its_own_job() {
        let store = temp_store("trash-file");
        let p = examples::find("hip").unwrap().project();
        let mut named = p.clone();
        named.name = "D".into();
        let old = store.save_as(None, &named, USER_ACTOR, "create").unwrap();
        store.add_attachment(&old, "plan.pdf", b"%PDF old").unwrap();
        let file = store
            .remove_attachment(&old, "plan.pdf", USER_ACTOR, "not needed")
            .unwrap();
        let job = store.delete(&old, USER_ACTOR, "done").unwrap();
        let new = store.save_as(None, &named, USER_ACTOR, "create").unwrap();
        assert_eq!(new, old, "the new job takes the free id");
        assert!(store.restore(&file.entry).is_err());
        assert!(store.attachments(&new).is_empty());
        let back = store.restore(&job.entry).unwrap();
        assert_ne!(back, new);
        assert_eq!(store.restore(&file.entry).unwrap(), back);
        assert_eq!(
            fs::read(store.attachment_path(&back, "plan.pdf").unwrap()).unwrap(),
            b"%PDF old"
        );
        assert!(store.attachments(&new).is_empty());
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// HUNTIO-005, ERRU-009: every entry is later than the one before, so
    /// a log from a clock that is ahead never makes a new change look
    /// accepted, and two saves in one millisecond get distinct times.
    #[test]
    fn log_times_always_increase() {
        let store = temp_store("monotonic");
        let a = examples::find("hip").unwrap().project();
        let id = store.save_as(None, &a, USER_ACTOR, "create").unwrap();
        // A log entry a day ahead, accepted by the user.
        let path = store.activity_path(&id).unwrap();
        let mut log = store.activity(&id);
        let ahead = now_ms() + 86_400_000;
        log[0].ts_ms = ahead;
        let text: String = log
            .iter()
            .map(|e| serde_json::to_string(e).unwrap() + "\n")
            .collect();
        fs::write(&path, text).unwrap();
        store.mark_reviewed(&id, ahead).unwrap();
        let mut p = a.clone();
        for deg in [30.0, 31.0, 32.0] {
            p.parts[0].edges[0].pitch_deg = deg;
            store
                .save_as(Some(id.clone()), &p, "claude", "steeper")
                .unwrap();
        }
        let ts: Vec<u64> = store.activity(&id).iter().map(|e| e.ts_ms).collect();
        assert!(ts.windows(2).all(|w| w[0] < w[1]), "{ts:?}");
        assert!(
            store.pending_review(&id).is_some(),
            "the AI change waits for review"
        );
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// ERR-008 (audit 2026-11-15): a save that fails after its change was
    /// logged takes the entry out again, so no phantom change is pending.
    #[test]
    fn a_failed_write_leaves_no_log_entry() {
        let store = temp_store("phantom");
        let a = examples::find("hip").unwrap().project();
        let id = store.save_as(None, &a, USER_ACTOR, "create").unwrap();
        let n = store.activity(&id).len();
        // The job file cannot be replaced: a folder stands in its place.
        let path = store.path(&id).unwrap();
        fs::remove_file(&path).unwrap();
        fs::create_dir_all(path.join("blocker")).unwrap();
        let mut ai = a.clone();
        ai.parts[0].edges[0].pitch_deg = 44.0;
        assert!(
            store
                .save_as(Some(id.clone()), &ai, "claude", "steeper")
                .is_err()
        );
        assert_eq!(store.activity(&id).len(), n);
        assert!(store.pending_review(&id).is_none());
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// HUNTIO-002: a second "revert the latest change" undoes one more
    /// step instead of putting back the change the first one undid.
    #[test]
    fn reverting_twice_goes_two_steps_back() {
        let store = temp_store("revert-twice");
        let a = examples::find("hip").unwrap().project();
        let id = store.save_as(None, &a, USER_ACTOR, "create").unwrap();
        let pitch = |p: &Project| p.parts[0].edges[0].pitch_deg;
        let mut prev = a.clone();
        for (deg, what) in [(30.0, "to 30"), (20.0, "to 20")] {
            let mut next = prev.clone();
            next.parts[0].edges[0].pitch_deg = deg;
            std::thread::sleep(std::time::Duration::from_millis(2));
            store
                .save_as(Some(id.clone()), &next, "claude", what)
                .unwrap();
            prev = next;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
        assert_eq!(
            pitch(&store.revert_last(&id, "claude", None).unwrap()),
            30.0
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
        let back = store.revert_last(&id, "claude", None).unwrap();
        assert_eq!(pitch(&back), pitch(&a));
        assert_eq!(
            store.activity(&id).last().unwrap().summary,
            format!("{REVERTED_PREFIX}to 30")
        );
        // A third one never brings a reverted pitch back.
        std::thread::sleep(std::time::Duration::from_millis(2));
        if let Ok(p) = store.revert_last(&id, "claude", None) {
            assert_eq!(pitch(&p), pitch(&a));
        }
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
            job_in_trash: None,
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
            undid_from: None,
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

    /// A wall for the listing tests below.
    fn card_edge(x: f64, y: f64, pitch: f64, eave: f64, overhang: f64) -> serde_json::Value {
        serde_json::json!({
            "start": { "x": x, "y": y }, "pitch_deg": pitch,
            "eave_height": eave, "overhang": overhang,
        })
    }

    /// Audit 2026-11-22 TEST-301: a job out of the limits is calculated
    /// not at all, hand-drawn faces or not: its card has no roof.
    #[test]
    fn the_card_of_a_job_out_of_the_limits_with_a_face_has_no_roof() {
        let store = temp_store("card-limits-face");
        let mut p = examples::find("hip").unwrap().project();
        p.parts[0].edges[0].start.x = 1.0e7;
        p.manual_faces.push(crate::model::ManualFace {
            points: vec![
                crate::geom::Point2::new(20.0, 0.0),
                crate::geom::Point2::new(26.0, 0.0),
                crate::geom::Point2::new(26.0, 4.0),
            ],
            pitch_deg: 30.0,
            label: String::new(),
        });
        let id = store.save(None, &p).unwrap();
        let s = store.summarize(&id).unwrap();
        assert_eq!(
            s.coverage(),
            Some(crate::quantities::RoofCoverage::None),
            "{s:?}"
        );
        let mut faces_only = p.clone();
        faces_only.parts.clear();
        faces_only.manual_faces[0].points[0].x = 1.0e7;
        let id = store.save(None, &faces_only).unwrap();
        let s = store.summarize(&id).unwrap();
        assert_eq!(
            s.coverage(),
            Some(crate::quantities::RoofCoverage::None),
            "{s:?}"
        );
        let _ = fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn the_listing_marks_a_roof_the_core_refused_as_not_calculated() {
        // ERRU-005: a refused roof was listed as 0.00 m², like an empty one.
        let store = temp_store("card-refused");
        let walls: Vec<_> = [(0.0, 0.0), (12.0, 0.0), (12.0, 8.0), (0.0, 8.0)]
            .iter()
            .map(|&(x, y)| card_edge(x, y, 35.0, 3.0, 0.5))
            .collect();
        let corners: Vec<_> = [
            (4.0, 3.0),
            (6.0, 3.0),
            (6.0, 4.0),
            (5.0, 4.0),
            (5.0, 5.0),
            (4.0, 5.0),
        ]
        .iter()
        .map(|&(x, y)| serde_json::json!({ "x": x, "y": y }))
        .collect();
        let p = serde_json::json!({
            "schema_version": 2, "name": "refused",
            "parts": [{ "name": "A", "edges": walls }],
            "chimneys": [{ "corners": corners, "height": 9.0 }],
        });
        let id = store
            .save(None, &Project::from_json(&p.to_string()).unwrap())
            .unwrap();
        let s = store.summarize(&id).unwrap();
        assert!(!s.figures_pending, "{s:?}");
        assert_eq!((s.parts, s.parts_failed), (1, 1), "{s:?}");
        let ok = store
            .save(None, &examples::find("hip").unwrap().project())
            .unwrap();
        assert_eq!(store.summarize(&ok).unwrap().parts_failed, 0);
        let _ = fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn the_listing_marks_a_job_with_one_building_without_roof_as_partly_calculated() {
        // ERRU-005: the other buildings' area was listed as if complete. The
        // two buildings of `a_building_the_core_fails_on_alone_leaves_the_others_their_roofs`.
        let store = temp_store("card-partial");
        let mut hh = card_edge(6.0, 12.8, 90.0, 3.0, 0.5);
        hh["half_hip"] = serde_json::json!({ "height": 5.0, "pitch_deg": 50.0 });
        let p = serde_json::json!({
            "schema_version": 2, "name": "partial",
            "parts": [
                { "name": "A", "edges": [
                    card_edge(0.0, 0.0, 30.0, 3.0, 0.6), card_edge(6.2, 0.0, 90.0, 3.0, 0.6),
                    card_edge(6.2, 5.9, 30.0, 3.0, 0.6), card_edge(0.0, 5.9, 90.0, 3.0, 0.6),
                ] },
                { "name": "B", "edges": [
                    card_edge(2.2, 5.9, 45.0, 3.0, 0.0), card_edge(6.0, 5.9, 30.0, 2.5, 0.0),
                    hh, card_edge(2.2, 12.8, 90.0, 3.5, 0.5),
                ] },
            ],
        });
        let id = store
            .save(None, &Project::from_json(&p.to_string()).unwrap())
            .unwrap();
        let s = store.summarize(&id).unwrap();
        assert!(!s.figures_pending, "{s:?}");
        assert_eq!((s.parts, s.parts_failed), (2, 1), "{s:?}");
        assert!(s.roof_area > 0.0, "{s:?}");
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// Audit 2026-11-21 ERR-001: a building left out of the calculation
    /// for its input (here a corner doubled 1 mm away) has no roof either;
    /// the house's area alone was listed as the job's complete roof.
    #[test]
    fn the_listing_counts_a_building_left_out_for_its_input_as_without_roof() {
        let store = temp_store("card-left-out");
        let house: Vec<_> = [(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)]
            .iter()
            .map(|&(x, y)| card_edge(x, y, 35.0, 3.0, 0.5))
            .collect();
        let garage: Vec<_> = [
            (20.0, 0.0),
            (26.0, 0.0),
            (26.0, 0.001),
            (26.0, 6.0),
            (20.0, 6.0),
        ]
        .iter()
        .map(|&(x, y)| card_edge(x, y, 35.0, 3.0, 0.5))
        .collect();
        let p = serde_json::json!({
            "schema_version": 2, "name": "left out",
            "parts": [{ "name": "A", "edges": house }, { "name": "B", "edges": garage }],
        });
        let p = Project::from_json(&p.to_string()).unwrap();
        assert!(
            p.problems()
                .iter()
                .any(|q| q.index == 1 && q.code == "zero_length_wall")
        );
        let id = store.save(None, &p).unwrap();
        let s = store.summarize(&id).unwrap();
        assert!(!s.figures_pending, "{s:?}");
        assert_eq!((s.parts, s.parts_failed), (2, 1), "{s:?}");
        assert!(s.roof_area > 0.0, "{s:?}");
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// Audit 2026-11-21 ARCH-004: Microsoft's device names, also with the
    /// superscript digits, 0, the console names and before a dot in ids.
    #[test]
    fn every_windows_device_name_is_refused_in_ids_and_file_names() {
        let devices = [
            "COM¹", "com²", "LPT³", "lpt¹", "COM0", "lpt0", "CONIN$", "conout$", "Con", "aux",
            "COM9", "LPT5",
        ];
        for d in devices {
            for name in [d.to_string(), format!("{d}.pdf"), format!("{d}.x.pdf")] {
                assert!(!valid_file_name(&name), "{name}");
            }
            assert!(!valid_id(d), "{d}");
        }
        let store = Store::at(PathBuf::from("/tmp/x"));
        for id in ["con.x", "com1.x", "nul.tectora", "CONIN$", "lpt²"] {
            assert!(store.path(id).is_err(), "{id}");
        }
        assert!(!valid_file_name("CON .txt"));
        assert_eq!(store.new_id("Com0"), "com0-navrh");
        for ok in [
            "com10.pdf",
            "comp.pdf",
            "lpt.pdf",
            "console.txt",
            "com-1.pdf",
            "conin.pdf",
        ] {
            assert!(valid_file_name(ok), "{ok}");
        }
    }

    /// Audit 2026-11-21 HUNTIO-007: a name decomposed on macOS gives the
    /// same id and the same attachment as its precomposed spelling, and an
    /// attachment name cannot hide what it is behind invisible characters.
    #[test]
    fn decomposed_names_and_invisible_characters_in_names() {
        assert_eq!(slug("Nov\u{79}\u{301} D\u{75}\u{30a}m"), "novy-dum");
        let store = temp_store("names-nfc");
        let id = store
            .save(None, &examples::find("gable").unwrap().project())
            .unwrap();
        let nfc = "Půdorys přízemí.pdf";
        let nfd = "P\u{75}\u{30a}dorys pr\u{30c}i\u{301}zemi\u{301}.pdf";
        assert_eq!(store.add_attachment(&id, nfc, b"a").unwrap(), nfc);
        assert_eq!(
            store.add_attachment(&id, nfd, b"b").unwrap(),
            "Půdorys přízemí (2).pdf"
        );
        for bad in [
            "file\u{202e}gnp.pdf",
            "plan\u{200b}.pdf",
            "\u{feff}plan.pdf",
        ] {
            assert!(!valid_file_name(bad), "{bad:?}");
            assert_eq!(
                store.add_attachment(&id, bad, b"x").unwrap_err().code,
                "invalid_name"
            );
        }
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// Audit 2026-11-21 HUNT-108 item 1: a file attached under its composed
    /// name is found (removed, read) by its decomposed spelling too, as a
    /// client on macOS copies it.
    #[test]
    fn an_attached_file_is_removed_by_its_decomposed_name() {
        let store = temp_store("names-nfd-remove");
        let id = store
            .save(None, &examples::find("gable").unwrap().project())
            .unwrap();
        let nfc = "Půdorys přízemí.pdf";
        let nfd = "P\u{75}\u{30a}dorys pr\u{30c}i\u{301}zemi\u{301}.pdf";
        assert_eq!(store.add_attachment(&id, nfd, b"a").unwrap(), nfc);
        assert!(store.attachment_path(&id, nfd).unwrap().is_file());
        store
            .remove_attachment(&id, nfd, "Claude", "Old plan")
            .unwrap();
        assert!(store.attachments(&id).is_empty());
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// Audit 2026-11-21 HUNTIO-001: the app attached any file type, and the
    /// bundle of the job then could not be imported anywhere.
    #[test]
    fn only_types_a_bundle_carries_are_attached() {
        let store = temp_store("attach-types");
        let id = store
            .save(None, &examples::find("gable").unwrap().project())
            .unwrap();
        let dir = store.dir.join("in");
        fs::create_dir_all(&dir).unwrap();
        for name in [
            "Zpráva od zákazníka.msg",
            "fotky.zip",
            "mail.eml",
            "run.command",
        ] {
            let src = dir.join(name);
            fs::write(&src, b"x").unwrap();
            let e = store.attach_file(&id, &src).unwrap_err();
            assert_eq!(e.code, "not_attachable", "{name}: {e}");
        }
        assert!(store.attachments(&id).is_empty());
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// Audit 2026-11-21 ARCH-002: a named pipe is refused before it is
    /// opened, so nothing waits for a writer that never comes.
    #[cfg(unix)]
    #[test]
    fn a_named_pipe_is_refused_without_waiting() {
        let store = temp_store("fifo");
        let id = store
            .save(None, &examples::find("gable").unwrap().project())
            .unwrap();
        let pipe = store.dir.join("pipe.pdf");
        let made = std::process::Command::new("mkfifo")
            .arg(&pipe)
            .status()
            .is_ok_and(|s| s.success());
        if !made {
            eprintln!("mkfifo not available, skipped");
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let (dir, p) = (store.dir.clone(), pipe.clone());
        std::thread::spawn(move || {
            let store = Store::at(dir);
            let attach = store.attach_file(&id, &p).map_err(|e| e.code);
            let open = open_regular(&p, 1 << 20).map(|_| ()).map_err(|e| e.code);
            let _ = tx.send((attach, open));
        });
        let (attach, open) = rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("refused without waiting for a writer");
        assert_eq!(attach, Err("not_a_file"));
        assert_eq!(open, Err("not_a_file"));
        let _ = fs::remove_dir_all(&store.dir);
    }
}
