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
/// Actor of the change-log entries Tectora writes itself (a stored result
/// recalculated by a corrected calculation): neither the user nor an AI,
/// so such an entry neither ends nor joins a pending AI review.
pub const ENGINE_ACTOR: &str = "tectora";
/// What an entry by [`ENGINE_ACTOR`] in a received bundle's change log is
/// renamed to: the sender wrote it, not this Tectora, so it is never shown
/// or trusted as the engine's own (audit 2026-11-22 ARCH-802). The app
/// knows the name (`types.ts`).
pub const RECEIVED_ENGINE_ACTOR: &str = "tectora (received)";
/// The actor names only Tectora itself writes: a name from outside (an
/// MCP client's, a received bundle's entry) that reads as one of them is
/// renamed, or its change would skip the user's review (HUNTIO-003, audit
/// 2026-11-23 ARCH-901). One list for every such check; the app's copy
/// (`types.ts`) is compared with it by `actors.test.ts`.
pub const RESERVED_ACTORS: [&str; 3] = [USER_ACTOR, ENGINE_ACTOR, RECEIVED_ENGINE_ACTOR];

/// The reserved actor ([`RESERVED_ACTORS`]) `name` reads as, compared as
/// a written entry's actor is (control and invisible characters taken
/// out, trimmed, ASCII case ignored); `None` for any other name.
pub fn reserved_actor(name: &str) -> Option<&'static str> {
    let clean = crate::edit::clean_line(name, crate::edit::MAX_ACTOR_CHARS);
    let clean = clean.trim();
    RESERVED_ACTORS
        .into_iter()
        .find(|r| clean.eq_ignore_ascii_case(r))
}

/// Whether `name` reads as one of [`RESERVED_ACTORS`].
pub fn is_reserved_actor(name: &str) -> bool {
    reserved_actor(name).is_some()
}
/// How long the recalculation on open waits for the job's lock before the
/// job opens as it is (audit 2026-11-22 ERR-803) [no source: a save holds
/// the lock for well under a second].
const REFRESH_LOCK_WAIT: std::time::Duration = std::time::Duration::from_secs(3);
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
    /// The job was last changed by an AI client ([`changed_by_ai`]): that
    /// change. The library's "changed by AI" filter, count and badge and
    /// `library::search`'s `by_ai` all read this (audit 2026-11-22 UX-806).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changed_by_ai: Option<ActivityInfo>,
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

/// What a job's stored result was taken from
/// (`.activity/<id>.result.json`), written when a save brings a newer
/// stored result: its time, the design's fingerprint then
/// ([`crate::snapshot::input_digest`]) and the last engine version that
/// checked it. A stored result is recalculated only while the design is
/// the one it was taken from, so an offer whose design was edited later
/// keeps its figures and shows the difference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ResultOrigin {
    taken_ms: u64,
    input: String,
    checked: String,
    /// The result is the engine's recalculation of an offer's result:
    /// which change-log entry wrote it. Only a result with this record is
    /// shown as recalculated and offers the way back, so an entry that
    /// came with a bundle never is (audit 2026-11-22 ARCH-802): the file
    /// is local and not part of a bundle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    refreshed: Option<RefreshedFrom>,
    /// The offer's own result, put back by the user or an AI after a
    /// recalculation: no later engine recalculates it again.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pinned: bool,
}

/// Where a recalculated stored result came from (see [`ResultOrigin`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct RefreshedFrom {
    /// The engine's change-log entry of the recalculation.
    entry_ts_ms: u64,
    /// `taken_ms` of the offer's result it replaced.
    offer_taken_ms: u64,
    /// The engine version of the offer's result.
    from: String,
    /// The offer's result the recalculation replaced, and the result it
    /// wrote: the put-back and the card's "recalculated" line read them
    /// here, so they outlive the change-log entry, which is trimmed after
    /// [`ACTIVITY_KEEP`] later changes (audit 2026-11-23 HUNT-902).
    /// `None` in records written by round-9 builds: those read the entry
    /// while it is in the log, and the next logged save copies them here
    /// ([`Store::keep_offer_figures`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    offer: Option<crate::snapshot::ResultSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    result: Option<crate::snapshot::ResultSnapshot>,
}

/// What [`Store::refresh_entry`] did: the engine's change-log entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RefreshedEntry {
    pub entry_ts_ms: u64,
    pub summary: String,
}

/// What the engine's recalculation of a stored result changed: the
/// offer's date and engine and each figure old -> new.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RefreshFigures {
    /// When the offer's result was stored.
    pub offer_ms: u64,
    pub from: String,
    pub to: String,
    pub differences: Vec<crate::snapshot::Difference>,
}

/// [`RefreshFigures`] of the change-log entry `e` from its two versions
/// ([`crate::snapshot::compare`]); `None` for every entry that is not the
/// engine's recalculation. History only: whether the job's stored result
/// still is that recalculation is [`Store::recalculation`].
pub fn refresh_figures(e: &ActivityEntry) -> Option<RefreshFigures> {
    if e.actor != ENGINE_ACTOR {
        return None;
    }
    let old = e.before.as_ref()?.result_snapshot.as_ref()?;
    let new = e.after.as_ref()?.result_snapshot.as_ref()?;
    Some(RefreshFigures {
        offer_ms: old.taken_ms,
        from: old.engine_version.clone(),
        to: new.engine_version.clone(),
        differences: crate::snapshot::compare(old, new),
    })
}

/// The offer's result and the recalculated one of the engine's change-log
/// entry at `ts_ms` in `log` (its `before` and `after`); `None` when the
/// entry is not there or is not the engine's.
fn entry_results(
    log: &[ActivityEntry],
    ts_ms: u64,
) -> Option<(
    crate::snapshot::ResultSnapshot,
    crate::snapshot::ResultSnapshot,
)> {
    let e = log.iter().rfind(|e| e.ts_ms == ts_ms)?;
    if e.actor != ENGINE_ACTOR {
        return None;
    }
    Some((
        e.before.as_ref()?.result_snapshot.clone()?,
        e.after.as_ref()?.result_snapshot.clone()?,
    ))
}

/// What [`Store::revert_to_counted`] did: the job as it is now and how
/// many recorded changes were undone.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RevertedChanges {
    pub project: Project,
    pub undone: usize,
}

/// The job's stored result is the engine's recalculation of the offer's
/// ([`Store::recalculation`]): the result card's "recalculated" line and
/// the history's "put the offer's figures back" come from this alone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recalculation {
    /// The engine's change-log entry; [`Store::restore_offer_result`]
    /// takes it.
    pub entry_ts_ms: u64,
    /// When it was recalculated.
    pub at: u64,
    /// `taken_ms` of the recalculated result, the one in the job file.
    pub taken_ms: u64,
    /// When the offer's result was stored.
    pub offer_ms: u64,
    pub from: String,
    pub to: String,
    pub differences: Vec<crate::snapshot::Difference>,
}

/// A change-log entry as the app's history shows it
/// ([`Store::activity_items`]).
#[derive(Debug, Clone, Serialize)]
pub struct ActivityItem {
    pub ts_ms: u64,
    pub actor: String,
    pub summary: String,
    pub changes: Vec<Change>,
    /// For the engine's recalculation of the stored result: the offer's
    /// figures against the new ones (audit UX-702a/b).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refreshed: Option<RefreshFigures>,
    /// For a revert: the time of the oldest entry it undid.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub undid_from: Option<u64>,
    /// For "offer result restored": the engine's entry it put back.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restores: Option<u64>,
    /// The AI's reason at the end of the summary ([`ActivityEntry::reason`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// The newest change of `log` (oldest first) when an AI client made it,
/// one rule for "changed by AI" everywhere (audit 2026-11-22 UX-806).
/// Tectora's own entries (a recalculated stored result, also one received
/// with a bundle) and "offer result restored" entries say nothing about
/// who changed the design and are stepped over: merely opening an offer
/// must not take it out of the AI list.
pub fn changed_by_ai(log: &[ActivityEntry]) -> Option<&ActivityEntry> {
    log.iter()
        .rev()
        .find(|e| {
            e.actor != ENGINE_ACTOR && e.actor != RECEIVED_ENGINE_ACTOR && e.restores.is_none()
        })
        .filter(|e| e.actor != USER_ACTOR)
}

/// How many stored results of a job keep their origin: a result put back
/// by a revert finds its own again (hunt R3 BUG-2).
const ORIGINS_KEPT: usize = 16;

/// Designs with more walls, pitch breaks and objects than this get no
/// origin: checking a result on save calculates the roof under the job's
/// lock, which a hostile file must not hold for a minute (hunt R3 BUG-1).
/// Their results are only not recalculated.
const ORIGIN_MAX_ITEMS: usize = 200;

fn design_items(p: &Project) -> usize {
    p.parts
        .iter()
        .flat_map(|part| &part.edges)
        .map(|e| 1 + e.breaks.len())
        .sum::<usize>()
        + p.dormers.len()
        + p.chimneys.len()
        + p.windows.len()
        + p.roof_openings.len()
        + p.manual_faces.len()
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
    /// For "offer result restored" ([`Store::restore_offer_result`]): the
    /// time of the engine's recalculation entry whose old result it put
    /// back. Such an entry changes the stored result only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restores: Option<u64>,
    /// The reason an AI gave for a revert or a put-back of the offer's
    /// figures, as it ends the summary (` (reason: <reason>)`): the app
    /// shows the core's sentence in the user's language and the reason
    /// as it is, without parsing the summary, whose texts are free (audit
    /// 2026-11-23 HUNT-904). The summary keeps it for older readers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl ActivityEntry {
    /// The user's own "offer result restored": it touches the stored
    /// result only, so it neither ends an AI review nor counts in one, and
    /// a revert of other entries leaves it alone.
    fn user_restore(&self) -> bool {
        self.restores.is_some() && self.actor == USER_ACTOR
    }

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

/// How the change-log entry of [`Store::restore_offer_result`] begins; the
/// app reads it (`live.svelte.ts`). The entry reads
/// `Offer result restored (Tectora <to> -> <from>): roof area <a> -> <b> m2`
/// (without the areas when they are the same).
pub const RESTORED_PREFIX: &str = "Offer result restored";

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
/// 2026-11-22 HUNT-505; HUNT-601: rectangles with a point on one side and
/// L shapes of 5 to 8 points lost a corner too).
const WHOLE_OUTLINE: usize = 8;

/// `o` without its corners that lie on a straight line between their
/// neighbours (a wall split for a door or a pitch change): the same shape
/// in fewer points. Kept as it is when fewer than 3 points would be left.
fn without_straight_points(o: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let n = o.len();
    if n <= 3 {
        return o.to_vec();
    }
    let kept: Vec<[f64; 2]> = (0..n)
        .filter(|&i| {
            let (a, b, c) = (o[(i + n - 1) % n], o[i], o[(i + 1) % n]);
            let (u, v) = ([b[0] - a[0], b[1] - a[1]], [c[0] - b[0], c[1] - b[1]]);
            let cross = u[0] * v[1] - u[1] * v[0];
            let dot = u[0] * v[0] + u[1] * v[1];
            let lens = u[0].hypot(u[1]) * v[0].hypot(v[1]);
            // Straight on: no turn and no reversal (a spike stays).
            let straight = lens > 0.0 && cross.abs() <= 1e-9 * lens && dot > 0.0;
            !straight
        })
        .map(|i| o[i])
        .collect();
    if kept.len() >= 3 { kept } else { o.to_vec() }
}

/// `outlines` with at most `max` points in all: corners on a straight
/// line go first, outlines of up to [`WHOLE_OUTLINE`] points are kept
/// whole, the larger ones keep evenly spread points, their share of what
/// the small ones leave (at least [`WHOLE_OUTLINE`], and what rounding
/// down leaves over is handed out again); outlines past the budget are
/// replaced by one box around all of them, so the thumbnail keeps the
/// job's extent.
fn bounded_outlines(outlines: Vec<Vec<[f64; 2]>>, max: usize) -> Vec<Vec<[f64; 2]>> {
    let total: usize = outlines.iter().map(Vec::len).sum();
    if total <= max {
        return outlines;
    }
    let outlines: Vec<Vec<[f64; 2]>> = outlines
        .iter()
        .map(|o| without_straight_points(o))
        .collect();
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
    // Each large outline's share, rounded down, then the points that
    // leaves over one by one to the largest remainders.
    let mut shares: Vec<usize> = outlines
        .iter()
        .map(|o| {
            if o.len() <= WHOLE_OUTLINE {
                o.len()
            } else {
                (o.len() * large_budget / large).clamp(WHOLE_OUTLINE, o.len())
            }
        })
        .collect();
    let given: usize = shares
        .iter()
        .zip(&outlines)
        .filter(|(_, o)| o.len() > WHOLE_OUTLINE)
        .map(|(s, _)| *s)
        .sum();
    let mut left = large_budget.saturating_sub(given);
    let mut by_remainder: Vec<usize> = (0..outlines.len())
        .filter(|&i| outlines[i].len() > WHOLE_OUTLINE && shares[i] < outlines[i].len())
        .collect();
    by_remainder.sort_by_key(|&i| std::cmp::Reverse((outlines[i].len() * large_budget) % large));
    for i in by_remainder {
        if left == 0 {
            break;
        }
        shares[i] += 1;
        left -= 1;
    }
    let mut used = 0;
    let mut out = Vec::new();
    let mut rest: Option<[f64; 4]> = None;
    for (o, share) in outlines.into_iter().zip(shares) {
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
    /// How long a short action of the user (Revert, Revert all, putting
    /// the offer's figures back, Accept) waits for the job's lock before it
    /// answers [`StoreError::Busy`], [`REFRESH_LOCK_WAIT`] (shorter in
    /// tests).
    action_wait: std::time::Duration,
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
    /// The change the caller meant to undo is no longer the one that would
    /// be undone (another change was saved meanwhile), or the stored result
    /// is no longer the recalculation to put back: nothing is done, so a
    /// press never acts on another entry than the one it was drawn on
    /// (audit 2026-11-22 ERR-801).
    #[error("{id}: the job changed meanwhile ({detail})")]
    RevertStale { id: String, detail: String },
    /// Another program holds the job's lock (an AI save that calculates,
    /// the recalculation on open) for longer than a short action waits
    /// ([`REFRESH_LOCK_WAIT`]): nothing is done, the caller may try again
    /// (audit 2026-11-23 ERR-1002, ARCH-1001).
    #[error(
        "{0}: the job is being saved by another program; nothing was done, try again in a moment"
    )]
    Busy(String),
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
            StoreError::RevertStale { .. } => "revert_stale",
            StoreError::Busy(_) => "busy",
        }
    }

    /// The project the error is about, if any.
    pub fn id(&self) -> Option<&str> {
        match self {
            StoreError::InvalidId(id)
            | StoreError::NotFound(id)
            | StoreError::NothingToRevert(id)
            | StoreError::RevertCreated(id)
            | StoreError::Busy(id) => Some(id),
            StoreError::Unreadable { id, .. }
            | StoreError::NewerFormat { id, .. }
            | StoreError::NoSuchChange { id, .. }
            | StoreError::RevertConflict { id, .. }
            | StoreError::RevertStale { id, .. } => Some(id),
            StoreError::Io { id, .. } => id.as_deref(),
        }
    }

    /// The underlying message (OS or parser error), if any.
    pub fn detail(&self) -> Option<&str> {
        match self {
            StoreError::Unreadable { detail, .. }
            | StoreError::Io { detail, .. }
            | StoreError::RevertConflict { detail, .. }
            | StoreError::RevertStale { detail, .. } => Some(detail),
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

/// Something in the library's own folders that blocks work
/// ([`Store::foreign_entries`]).
#[derive(Debug, Clone, Serialize)]
pub struct ForeignItem {
    /// Its path in the library, e.g. `.trash` or `.activity/dum.lock`.
    pub id: String,
    /// `foreign_entry`.
    pub code: &'static str,
    /// English, with the full path, for the details view.
    pub detail: String,
    /// What it blocks: `activity` (a link or file in place of the job
    /// histories: no job can be saved), `files` (in place of the attached
    /// files: no new job, attachment, copy or restore), `trash` (no delete
    /// or restore), `exports` (no roof sheet or list export by the AI),
    /// `job_files` (in place of one job's files folder: no file can be
    /// attached to that job and it cannot be copied), `lock` (in place of one
    /// job's lock file: that job cannot be saved), `lock_fix` (in place of
    /// the helper file of one job's lock: saves of that job may be
    /// refused), `new_lock` (in place of the lock for new jobs: no new
    /// job).
    pub kind: &'static str,
    /// The job it is about, for `job_files`, `lock` and `lock_fix`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job: Option<String>,
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

/// Something reading a job left out of it ([`Store::load_warnings`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LoadWarning {
    /// `result_dropped`: a stored result with an impossible time.
    pub code: &'static str,
    pub detail: String,
}

/// Whether `a` and `b` are the same job for a JavaScript client: equal as
/// JSON, where two numbers are equal when their f64 values are (`1.0` and
/// `1`, `-0.0` and `0`, `1e3` and `1000`, 2^53 + 1 and 2^53). Only for
/// [`Store::save_checked`], whose base came through the app (HUNT-804);
/// MCP's checked save compares exactly.
fn same_for_js(a: &Project, b: &Project) -> bool {
    fn same(a: &serde_json::Value, b: &serde_json::Value) -> bool {
        use serde_json::Value as V;
        match (a, b) {
            (V::Number(x), V::Number(y)) => x == y || x.as_f64() == y.as_f64(),
            (V::Array(x), V::Array(y)) => {
                x.len() == y.len() && x.iter().zip(y).all(|(x, y)| same(x, y))
            }
            (V::Object(x), V::Object(y)) => {
                x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w)))
            }
            _ => a == b,
        }
    }
    if a == b {
        return true;
    }
    match (serde_json::to_value(a), serde_json::to_value(b)) {
        (Ok(x), Ok(y)) => same(&x, &y),
        _ => false,
    }
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

/// How many ids based on one name a new job tries before it gives up
/// (ARCH-601: the search had no end).
const MAX_ID_TRIES: usize = 10_000;

/// The id a new job of this name starts from: its slug, with room left
/// for the "-N" suffix and clear of device names.
fn id_base(name: &str) -> String {
    let mut base = slug(name);
    base.truncate(MAX_ID_LEN - 8);
    if !valid_id(&base) {
        base.push_str("-navrh");
    }
    base
}

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
    #[cfg(test)]
    JOB_READS.with(|n| n.set(n.get() + 1));
    job_text(
        read_regular(path, MAX_JOB_FILE)?,
        &path.display().to_string(),
    )
}

#[cfg(test)]
thread_local! {
    /// Job files read by this thread ([`read_job_text`]), for tests of how
    /// often a save reads the file (ARCH-504).
    static JOB_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
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

/// What [`own_folder`] and [`not_foreign`] say about a link or a file in
/// place of one of the library's own folders. The data where a link points
/// is not lost: the way back is to move that folder into the library in
/// the link's place, never to delete it, which would hide every job's
/// history or files (audit 2026-11-22 UX-601).
const FOREIGN_FOLDER: &str = "a link or a file stands where Tectora keeps its own folder; Tectora does not write through a link, and whatever it points to is kept as it is (to use that data again, replace the link with the folder it points to, moved into the library; a stray file of this name can be moved away)";

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
                return Err(foreign(dir, FOREIGN_FOLDER));
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

/// `Ok` when `dir`, one of the library's own folders, is missing or a real
/// folder; a [`ForeignEntry`] when a link or a file stands there. For
/// reading, moving and deleting in them without creating them: a delete,
/// a copy or an export never takes files from where a link points
/// (audit 2026-11-22 ARCH-602).
fn not_foreign(dir: &Path) -> std::io::Result<()> {
    match fs::symlink_metadata(dir) {
        Ok(m) if m.is_dir() => Ok(()),
        Ok(_) => Err(foreign(dir, FOREIGN_FOLDER)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
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
    take_lock_file_within(path, None)
}

/// [`take_lock_file`] that waits for a lock another process holds for at
/// most `wait` (forever with `None`), then fails with
/// [`std::io::ErrorKind::WouldBlock`]: opening a job must not hang on a
/// writer that never lets go (audit 2026-11-22 ERR-803).
pub(crate) fn take_lock_file_within(
    path: &Path,
    wait: Option<std::time::Duration>,
) -> std::io::Result<Option<fs::File>> {
    let deadline = wait.map(|w| std::time::Instant::now() + w);
    // `Ok(false)`: this file system has no locks.
    let lock = |file: &fs::File| -> std::io::Result<bool> {
        let Some(deadline) = deadline else {
            return Ok(file.lock().is_ok());
        };
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(true),
                Err(fs::TryLockError::WouldBlock) => {
                    if std::time::Instant::now() >= deadline {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::WouldBlock,
                            "the job's lock is held by another process",
                        ));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                Err(fs::TryLockError::Error(_)) => return Ok(false),
            }
        }
    };
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
                if !lock(&file)? {
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
                    "a folder with files in it stands where the lock file belongs, so saves could not be locked; it is not Tectora's: move it out of the jobs folder (the change was not saved)",
                ));
            }
        }
        // A link to a folder is removed as a folder on Windows.
        Ok(m) if !m.file_type().is_file() && fs::remove_file(path).is_err() => {
            let _ = fs::remove_dir(path);
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
            action_wait: REFRESH_LOCK_WAIT,
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

    /// The same store whose short actions wait at most `wait` for the
    /// job's lock ([`Store::lock_briefly`]), for tests.
    pub fn with_action_wait(mut self, wait: std::time::Duration) -> Self {
        self.action_wait = wait;
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
        let log = self.activity(id);
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
            last_activity: log.last().map(ActivityEntry::info),
            changed_by_ai: changed_by_ai(&log).map(ActivityEntry::info),
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

    /// What in the library's own folders really blocks work, for the
    /// library screen to name before the user edits a job that then cannot
    /// be saved (audit 2026-11-22 UX-601, ARCH-701): a link or a file in
    /// place of `.activity`, `.files`, `.trash` or `exports`; a link in
    /// `.files`, or a file in place of an existing job's folder there; and
    /// in `.activity` a folder with files in place of a lock file (every
    /// save of that job is refused) or anything but a file in place of a
    /// lock's `.fix` file. A plain file in `.files` that is no job's folder
    /// (`Thumbs.db`, `desktop.ini`, a drawing dropped there) blocks nothing
    /// and is not named. Each with `id` its path in the library, `code`
    /// `foreign_entry`, the detail, and `kind` (see [`ForeignItem`]); at
    /// most 20 of `.files` and 20 of `.activity`, sorted. A few `lstat`
    /// calls and one listing of `.files` and of `.activity`.
    pub fn foreign_entries(&self) -> Vec<ForeignItem> {
        let mut found = Vec::new();
        let mut check = |rel: String, kind: &'static str, job: Option<String>| {
            if let Err(e) = not_foreign(&self.dir.join(&rel))
                && is_foreign(&e)
            {
                found.push(ForeignItem {
                    id: rel,
                    code: "foreign_entry",
                    detail: e.to_string(),
                    kind,
                    job,
                });
            }
        };
        for (dir, kind) in [
            (ACTIVITY_DIR, "activity"),
            (FILES_DIR, "files"),
            (TRASH_DIR, "trash"),
            (crate::bundle::EXPORTS_DIR, "exports"),
        ] {
            check(dir.to_string(), kind, None);
        }
        let listed = |dir: &Path| -> Vec<(String, fs::FileType)> {
            let mut v: Vec<(String, fs::FileType)> = fs::read_dir(dir)
                .into_iter()
                .flatten()
                .flatten()
                .filter_map(|e| Some((e.file_name().into_string().ok()?, e.file_type().ok()?)))
                .collect();
            v.sort_by(|a, b| a.0.cmp(&b.0));
            v
        };
        let files = self.dir.join(FILES_DIR);
        if is_own_folder(&files) {
            let blocking = listed(&files).into_iter().filter(|(name, t)| {
                !name.starts_with('.')
                    && (t.is_symlink()
                        || (!t.is_dir()
                            && self
                                .path(name)
                                .is_ok_and(|p| fs::symlink_metadata(p).is_ok_and(|m| m.is_file()))))
            });
            for (name, _) in blocking.take(20) {
                check(format!("{FILES_DIR}/{name}"), "job_files", Some(name));
            }
        }
        let activity = self.activity_dir();
        if is_own_folder(&activity) {
            let odd_locks = listed(&activity).into_iter().filter_map(|(name, t)| {
                let path = activity.join(&name);
                let (what, fix) = if name.ends_with(".lock") && t.is_dir() {
                    // An empty one is replaced by the next save.
                    fs::read_dir(&path).ok()?.next()?.ok()?;
                    ("a folder with files in it stands where the lock file belongs, so every save of this job is refused; it is not Tectora's: move it out of the jobs folder", false)
                } else if name.ends_with(".lock.fix") && !t.is_file() {
                    ("a link, folder or pipe stands where Tectora keeps a lock's helper file, so saves of this job can be refused; it is not Tectora's: move it out of the jobs folder", true)
                } else {
                    return None;
                };
                // Each suffix once: an id may end in `.lock` (ARCH-804).
                let base = name.strip_suffix(".fix").unwrap_or(&name);
                let base = base.strip_suffix(".lock").unwrap_or(base);
                let (kind, job) = if base == ".new" {
                    ("new_lock", None)
                } else {
                    self.path(base).ok()?;
                    // The helper file only may block a save, not every
                    // one as the lock does (ARCH-803).
                    (if fix { "lock_fix" } else { "lock" }, Some(base.to_string()))
                };
                Some(ForeignItem {
                    id: format!("{ACTIVITY_DIR}/{name}"),
                    code: "foreign_entry",
                    detail: format!("{}: {what}", path.display()),
                    kind,
                    job,
                })
            });
            found.extend(odd_locks.take(20));
        }
        found
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

    /// Recalculates the job's stored result where an older engine stored
    /// it, the design is still the one it was taken from (its
    /// [`ResultOrigin`]) and the figures are now different
    /// ([`crate::snapshot::refreshed`]), with a change-log entry by
    /// [`ENGINE_ACTOR`] that keeps the job as it was. Returns whether it
    /// did ([`Store::refresh_result`] gives the entry's summary).
    pub fn refresh_stored_result(&self, id: &str) -> Result<bool, StoreError> {
        Ok(self.refresh_result(id)?.is_some())
    }

    /// [`Store::refresh_stored_result`] with the change log's summary of
    /// the recalculation, `None` when nothing was recalculated
    /// ([`Store::refresh_entry`]).
    pub fn refresh_result(&self, id: &str) -> Result<Option<String>, StoreError> {
        Ok(self.refresh_entry(id)?.map(|r| r.summary))
    }

    /// [`Store::refresh_stored_result`] with the change-log entry of the
    /// recalculation, `None` when nothing was recalculated. A result
    /// stored before Tectora kept its origin, one whose design changed
    /// since, one put back after a recalculation (for good, whatever the
    /// engine: its origin is pinned) and one this engine already checked
    /// stay as they are, and so does the result of a job that is not an
    /// offer (draft, ordered, done). So does the result of a job whose lock cannot be
    /// taken (a folder with files where the lock file belongs): every other
    /// save of it is refused as well (audit 2026-11-22 ARCH-702), and two
    /// opens at once must not log it twice. The roof is calculated once,
    /// in the work bound ([`crate::work::calculate`]), and its result is
    /// the new origin (HUNT-703). Only the app calls it, when the user
    /// opens a job, off the window's thread; an AI reading the job over MCP
    /// never changes it (Jakub's decision 2026-11-22, audit UX-702h).
    ///
    /// Whether a recalculation is due is looked at without the job's lock
    /// first, so opening a draft, or an offer with nothing to recalculate,
    /// never waits for another writer. Only a due recalculation takes the
    /// lock, waits for it a few seconds at most (then the job opens as it
    /// is and the next open tries again) and looks again under it (audit
    /// 2026-11-22 ERR-803).
    pub fn refresh_entry(&self, id: &str) -> Result<Option<RefreshedEntry>, StoreError> {
        self.refresh_entry_within(id, REFRESH_LOCK_WAIT)
    }

    fn refresh_entry_within(
        &self,
        id: &str,
        wait: std::time::Duration,
    ) -> Result<Option<RefreshedEntry>, StoreError> {
        // No lock file is made for a job that is not there.
        if fs::symlink_metadata(self.path(id)?).is_err() {
            return Ok(None);
        }
        if self.refresh_due(id)?.is_none() {
            return Ok(None);
        }
        let Ok(_lock) = self.lock_file_within(Some(id), &format!("{id}.lock"), Some(wait)) else {
            return Ok(None);
        };
        // Again under the lock: another open may have recalculated it, or
        // a save changed the job, while this one waited.
        let Some((before, mut origin)) = self.refresh_due(id)? else {
            return Ok(None);
        };
        match crate::work::calculate(|| crate::snapshot::refreshed(&before))
            .ok()
            .flatten()
        {
            Some((after, summary)) => {
                // The fresh result is this engine's own for this design: its
                // origin, without calculating the roof again on the write.
                // It names the entry that wrote it (filled in by
                // `log_and_write`), which is what makes it "recalculated".
                let fresh = after.result_snapshot.as_ref().map(|s| ResultOrigin {
                    taken_ms: s.taken_ms,
                    input: origin.input.clone(),
                    checked: crate::snapshot::ENGINE_VERSION.to_string(),
                    refreshed: before.result_snapshot.as_ref().map(|old| RefreshedFrom {
                        entry_ts_ms: 0,
                        offer_taken_ms: old.taken_ms,
                        from: old.engine_version.clone(),
                        offer: Some(old.clone()),
                        result: Some(s.clone()),
                    }),
                    pinned: false,
                });
                let entry_ts_ms = self.log_and_write(
                    id,
                    Some(before),
                    &after,
                    ENGINE_ACTOR,
                    &summary,
                    None,
                    None,
                    None,
                    fresh,
                )?;
                // Only once saved (audit ARCH-502): the old result, put
                // back by a revert, then stays reverted; a failed save
                // leaves it to be recalculated on the next open.
                origin.checked = crate::snapshot::ENGINE_VERSION.to_string();
                let _ = self.write_result_origin(id, &origin);
                Ok(Some(RefreshedEntry {
                    entry_ts_ms,
                    summary,
                }))
            }
            None => {
                // Checked: the next open need not calculate it again (also
                // when the roof is over the work bound or cannot be
                // calculated: the stored result stays as the offer had it).
                origin.checked = crate::snapshot::ENGINE_VERSION.to_string();
                let _ = self.write_result_origin(id, &origin);
                Ok(None)
            }
        }
    }

    /// The job and the origin of its stored result when that result is to
    /// be recalculated ([`Store::refresh_entry`]); reads only.
    fn refresh_due(&self, id: &str) -> Result<Option<(Project, ResultOrigin)>, StoreError> {
        let before = self.load(id)?;
        // Only an offer follows the corrected calculation: a draft, an
        // ordered and a done job keep the figures they have (Jakub's
        // decision 2026-11-22, audit UX-702g).
        if before.job.status != crate::model::JobStatus::Offer {
            return Ok(None);
        }
        let Some(stored) = &before.result_snapshot else {
            return Ok(None);
        };
        let Some(origin) = self.result_origin(id, stored.taken_ms) else {
            return Ok(None);
        };
        // While AI changes wait for review the result stays: Revert and
        // Revert all then undo the AI's changes, not the recalculation
        // (hunt R2 BUG-3), and the review starts after the recalculation.
        if origin.taken_ms != stored.taken_ms
            || origin.checked == crate::snapshot::ENGINE_VERSION
            // The offer's own figures, put back after a recalculation,
            // stay for good.
            || origin.pinned
            || origin.input != crate::snapshot::input_digest(&before)
            || self.pending_review(id).is_some()
            // Only a design small enough to have got its origin (audit
            // ERR-101).
            || design_items(&before) > ORIGIN_MAX_ITEMS
        {
            return Ok(None);
        }
        Ok(Some((before, origin)))
    }

    /// The engine's recalculation the job's stored result is, `None` when
    /// it is any other result: one the user or an AI stored since, the
    /// offer's own put back, or one whose recalculation entry this library
    /// did not write itself (it came with a bundle, audit 2026-11-22
    /// ARCH-802). The result card's "recalculated" line and the history's
    /// "put the offer's figures back" show for this alone, at whatever
    /// position the entry has in the change log by now, in a job of any
    /// status.
    pub fn recalculation(&self, id: &str) -> Option<Recalculation> {
        let project = self.load(id).ok()?;
        // The change log only for a record a round-9 build wrote without
        // the figures: a job without a recalculated stored result answers
        // without reading it (audit 2026-11-23 ARCH-1002, a large log
        // cost every MCP get_project a second).
        let stored = project.result_snapshot.as_ref()?;
        let from = self.result_origin(id, stored.taken_ms)?.refreshed?;
        let log = if from.offer.is_some() && from.result.is_some() {
            Vec::new()
        } else {
            self.activity(id)
        };
        self.recalculation_of(id, &project, &log)
    }

    /// [`Store::recalculation`] of the job as read, with its change log.
    fn recalculation_of(
        &self,
        id: &str,
        project: &Project,
        log: &[ActivityEntry],
    ) -> Option<Recalculation> {
        self.recalculation_and_offer(id, project, log)
            .map(|(rec, _)| rec)
    }

    /// [`Store::recalculation_of`] with the offer's result it replaced.
    /// Both results come from the origin record ([`RefreshedFrom`]); from
    /// the engine's change-log entry only for a record a round-9 build
    /// wrote without them.
    ///
    /// Held to the bounds of a stored result from a file: a damaged record
    /// is no recalculation, so its text never reaches the result card or
    /// MCP (audit 2026-11-23 ARCH-1003).
    fn recalculation_and_offer(
        &self,
        id: &str,
        project: &Project,
        log: &[ActivityEntry],
    ) -> Option<(Recalculation, crate::snapshot::ResultSnapshot)> {
        let (rec, offer) = self.recorded_recalculation(id, project, log)?;
        let fresh = project.result_snapshot.as_ref()?;
        (offer.writable().is_ok() && fresh.writable().is_ok()).then_some((rec, offer))
    }

    /// [`Store::recalculation_and_offer`] as the origin record has it,
    /// within bounds or not: the put-back checks the figures it would write
    /// itself (TEST-903).
    fn recorded_recalculation(
        &self,
        id: &str,
        project: &Project,
        log: &[ActivityEntry],
    ) -> Option<(Recalculation, crate::snapshot::ResultSnapshot)> {
        let stored = project.result_snapshot.as_ref()?;
        let from = self.result_origin(id, stored.taken_ms)?.refreshed?;
        let (offer, fresh) = match (from.offer, from.result) {
            (Some(offer), Some(fresh)) => (offer, fresh),
            _ => entry_results(log, from.entry_ts_ms)?,
        };
        if offer.taken_ms != from.offer_taken_ms
            || fresh.taken_ms != stored.taken_ms
            || fresh.engine_version != stored.engine_version
            || !crate::snapshot::compare(&fresh, stored).is_empty()
        {
            return None;
        }
        let rec = Recalculation {
            entry_ts_ms: from.entry_ts_ms,
            at: from.entry_ts_ms,
            taken_ms: stored.taken_ms,
            offer_ms: offer.taken_ms,
            from: offer.engine_version.clone(),
            to: fresh.engine_version.clone(),
            differences: crate::snapshot::compare(&offer, &fresh),
        };
        Some((rec, offer))
    }

    /// Copies the offer's and the recalculated result from the engine's
    /// change-log entry into each origin record a round-9 build wrote
    /// without them, while the entry is still in `log`; the caller holds
    /// the job's lock and is about to append (and maybe trim) the log
    /// (audit 2026-11-23 HUNT-902). Best effort: a record it cannot write
    /// is tried again on the next save.
    fn keep_offer_figures(&self, id: &str, log: &[ActivityEntry]) {
        let mut all = self.result_origins(id);
        let mut changed = false;
        for o in &mut all {
            if let Some(r) = o.refreshed.as_mut()
                && (r.offer.is_none() || r.result.is_none())
                && let Some((offer, fresh)) = entry_results(log, r.entry_ts_ms)
            {
                r.offer = Some(offer);
                r.result = Some(fresh);
                changed = true;
            }
        }
        if changed {
            let _ = self.write_result_origins(id, &all);
        }
    }

    /// Puts the offer's own stored result back in place of the engine's
    /// recalculation logged at `entry_ts_ms`, and nothing else: the design
    /// and every change made before or after the recalculation stay (audit
    /// 2026-11-22 ERR-801, UX-802). Under the job's lock the file is read
    /// again; unless its stored result still is that recalculation
    /// ([`Store::recalculation`]) nothing is written and the error is
    /// [`StoreError::RevertStale`]. The entry it logs names the engine's
    /// entry (`restores`); the result put back is never recalculated again
    /// (its origin is pinned). `reason`: an AI client's summary.
    pub fn restore_offer_result(
        &self,
        id: &str,
        entry_ts_ms: u64,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<Project, StoreError> {
        self.path(id)?;
        // A short action of the user: never waits long (ERR-1002).
        let _lock = self.lock_briefly(id)?;
        let current = self.load(id)?;
        let log = self.activity(id);
        let stale = || StoreError::RevertStale {
            id: id.to_string(),
            detail: "the stored result is not that recalculation".into(),
        };
        // The offer's figures from the origin record, which outlives the
        // change-log entry (HUNT-902).
        // The record as it is: figures outside the limits are refused
        // below without writing anything (TEST-903).
        let (rec, old) = self
            .recorded_recalculation(id, &current, &log)
            .filter(|(r, _)| r.entry_ts_ms == entry_ts_ms)
            .ok_or_else(stale)?;
        // Damaged figures or an impossible date are refused before
        // anything is built or written: every later read would drop such
        // a stored result (audit 2026-11-23 HUNT-1102).
        old.writable().map_err(|detail| StoreError::Unreadable {
            id: id.to_string(),
            detail,
        })?;
        let fresh_area = current
            .result_snapshot
            .as_ref()
            .map_or(0.0, |s| s.roof_area);
        // The areas only where they differ (UX-807).
        let mut what = format!("{RESTORED_PREFIX} (Tectora {} -> {})", rec.to, rec.from);
        if rec.differences.iter().any(|d| d.key == "roof_area") {
            what += &format!(": roof area {fresh_area:.2} -> {:.2} m2", old.roof_area);
        }
        let (summary, reason) = with_reason(reason, what);
        let pinned = self.result_origin(id, old.taken_ms).map(|mut o| {
            o.pinned = true;
            o
        });
        let mut next = current.clone();
        next.result_snapshot = Some(old);
        // The log is a file like the job: a version outside the limits is
        // not written back into the job (HUNTIO-004).
        next.check_limits()
            .map_err(|detail| StoreError::Unreadable {
                id: id.to_string(),
                detail,
            })?;
        self.log_and_write(
            id,
            Some(current),
            &next,
            actor,
            &summary,
            reason,
            None,
            Some(entry_ts_ms),
            pinned,
        )?;
        Ok(next)
    }

    /// The change log of `id` as the app's history shows it, newest first
    /// ([`Store::history`] with [`refresh_figures`]).
    pub fn activity_items(&self, id: &str) -> Vec<ActivityItem> {
        let mut out: Vec<ActivityItem> = self
            .history(id)
            .into_iter()
            .map(|e| ActivityItem {
                refreshed: refresh_figures(&e),
                ts_ms: e.ts_ms,
                actor: e.actor,
                summary: e.summary,
                changes: e.changes,
                undid_from: e.undid_from,
                restores: e.restores,
                reason: e.reason,
            })
            .collect();
        out.reverse();
        out
    }

    pub fn load(&self, id: &str) -> Result<Project, StoreError> {
        self.load_noted(id).map(|(p, _)| p)
    }

    /// What reading job `id` left out of it, for the app to say when it
    /// opens the job: a stored result with an impossible time
    /// (`result_dropped`, HUNT-802). Empty when nothing was left out; an
    /// error when the job cannot be read at all, as [`Store::load`].
    pub fn load_warnings(&self, id: &str) -> Result<Vec<LoadWarning>, StoreError> {
        let (_, dropped) = self.load_noted(id)?;
        Ok(dropped
            .map(|detail| LoadWarning {
                code: "result_dropped",
                detail,
            })
            .into_iter()
            .collect())
    }

    /// [`Store::load`] that also says why a stored result was dropped.
    fn load_noted(&self, id: &str) -> Result<(Project, Option<String>), StoreError> {
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
        let mut project = Project::from_json(&text).map_err(|e| StoreError::Unreadable {
            id: id.to_string(),
            detail: e.to_string(),
        })?;
        // A stored result with an impossible time is left out, the job
        // opens (HUNT-802); the file keeps it until the next save.
        let dropped = crate::snapshot::drop_impossible(&mut project);
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
        Ok((project, dropped))
    }

    /// Picks an unused id based on the project name: no job file, attached
    /// files, change log or review mark of that id exists, so a new job never
    /// adopts what an earlier job of the same name left behind (ERR-005).
    /// Another process can take the id before it is written: create jobs
    /// through [`Store::save`] or [`Store::save_as`] with `None`, which pick
    /// and write it under the library's creation lock.
    pub fn new_id(&self, name: &str) -> String {
        // A library whose folders cannot be checked has no free id: the
        // plain one is only a suggestion then, which `save` refuses.
        self.free_id(name).unwrap_or_else(|_| id_base(name))
    }

    /// [`Store::new_id`] for creating a job: an error, never an endless
    /// search, when the library's folders cannot be checked (a file or a
    /// link to itself named `.files` made every candidate look taken, so
    /// new jobs spun for ever under the creation lock; audit 2026-11-22
    /// ARCH-601).
    fn free_id(&self, name: &str) -> Result<String, StoreError> {
        // `.files/<id>` of every candidate is looked at below: a file or a
        // link in place of `.files` is refused once, up front.
        not_foreign(&self.dir.join(FILES_DIR)).map_err(|e| StoreError::io(None, e))?;
        self.free_id_with(name, |id| self.id_used(id))
    }

    /// The first id based on `name` that `used` calls free, after at most
    /// [`MAX_ID_TRIES`] candidates.
    fn free_id_with(
        &self,
        name: &str,
        used: impl Fn(&str) -> std::io::Result<bool>,
    ) -> Result<String, StoreError> {
        let base = id_base(name);
        let mut id = base.clone();
        for n in 2..MAX_ID_TRIES + 2 {
            if !used(&id).map_err(|e| StoreError::io(None, e))? {
                return Ok(id);
            }
            id = format!("{base}-{n}");
        }
        Err(StoreError::Io {
            id: None,
            kind: IoKind::Other,
            detail: format!(
                "no free job id for `{base}` after {MAX_ID_TRIES} tries; rename the job"
            ),
        })
    }

    /// Whether anything in the library belongs to job `id`; an error for a
    /// path that cannot be checked.
    fn id_used(&self, id: &str) -> std::io::Result<bool> {
        let activity = self.activity_dir();
        for p in [
            self.dir.join(format!("{id}.{EXTENSION}")),
            self.dir.join(FILES_DIR).join(id),
            activity.join(format!("{id}.jsonl")),
            activity.join(format!("{id}.review.json")),
            activity.join(format!("{id}.result.json")),
        ] {
            // Anything there counts, also a link to nowhere: a job of that
            // id would write its log through it (ARCH-501).
            match fs::symlink_metadata(&p) {
                Ok(_) => return Ok(true),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    return Err(std::io::Error::new(
                        e.kind(),
                        format!("{}: {e}", p.display()),
                    ));
                }
            }
        }
        Ok(false)
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
        self.lock_file_within(id, name, None)
    }

    /// [`Store::lock_file`] waiting at most `wait` for a lock that is held
    /// ([`take_lock_file_within`]).
    fn lock_file_within(
        &self,
        id: Option<&str>,
        name: &str,
        wait: Option<std::time::Duration>,
    ) -> Result<Option<fs::File>, StoreError> {
        if self.library_present().is_err() {
            return Ok(None);
        }
        let dir = self.activity_dir();
        match own_folder(&dir) {
            Ok(()) => {}
            Err(e) if is_foreign(&e) => return Err(StoreError::io(id, e)),
            Err(_) => return Ok(None),
        }
        take_lock_file_within(&dir.join(name), wait).map_err(|e| match id {
            // Only the bounded wait gives up with `WouldBlock`.
            Some(id) if wait.is_some() && e.kind() == std::io::ErrorKind::WouldBlock => {
                StoreError::Busy(id.to_string())
            }
            _ => StoreError::io(id, e),
        })
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
        let id = match id {
            Some(id) => id,
            None => self.free_id(&project.name)?,
        };
        let _lock = self.lock(&id)?;
        self.write(&id, project, false)?;
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
    ///
    /// The base went through the app's `JSON.parse` / `JSON.stringify`,
    /// which keeps every number as an f64: numbers are compared that way
    /// ([`same_for_js`]), or a file with `"x": 1.0` in an unknown key could
    /// never be saved from the app (HUNT-804).
    pub fn save_checked(
        &self,
        id: &str,
        project: &Project,
        base: &Project,
    ) -> Result<CheckedSave, StoreError> {
        self.path(id)?;
        let _lock = self.lock(id)?;
        match self.load(id) {
            Ok(current) if !same_for_js(&current, base) => {
                return Ok(CheckedSave::Conflict {
                    current: Box::new(current),
                });
            }
            Ok(_) | Err(StoreError::NotFound(_) | StoreError::Unreadable { .. }) => {}
            Err(e) => return Err(e),
        }
        // `load` above refused a newer file.
        self.write(id, project, true)?;
        Ok(CheckedSave::Saved { id: id.to_string() })
    }

    /// Writes the file; the caller holds the job's lock. A file saved by a
    /// newer Tectora is refused: looked at here unless `checked`, when the
    /// caller read the file under this lock already (`load` refuses it), so
    /// a save reads the file once (audit 2026-11-22 ARCH-504 item 1).
    fn write(&self, id: &str, project: &Project, checked: bool) -> Result<(), StoreError> {
        self.write_known(id, project, checked, None)
    }

    /// [`Store::write`] where the caller knows the origin of the result it
    /// writes, `known`: recorded as such instead of calculating the roof
    /// to check it (a recalculated stored result, HUNT-703).
    fn write_known(
        &self,
        id: &str,
        project: &Project,
        checked: bool,
        known: Option<ResultOrigin>,
    ) -> Result<(), StoreError> {
        let path = self.path(id)?;
        self.library_present()?;
        if let Some(version) = (!checked)
            .then(|| read_job_text(&path).ok())
            .flatten()
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
        // The stored result the file had: a save that brings a newer one
        // records what it was taken from.
        let old_taken = known
            .is_none()
            .then(|| read_regular(&path, MAX_JOB_FILE).ok())
            .flatten()
            .and_then(|t| serde_json::from_slice::<serde_json::Value>(&t).ok())
            .and_then(|v| v.get("result_snapshot")?.get("taken_ms")?.as_u64());
        write_atomic(&path, &json).map_err(|e| StoreError::io(Some(id), e))?;
        if let Some(origin) = known {
            // Best effort, as below.
            let _ = self.write_result_origin(id, &origin);
            return Ok(());
        }
        // Only a result this engine gives for this very design is known to
        // be taken from it: an imported or restored job may carry figures
        // of a design edited since (R2-1), which must never be recalculated.
        if let Some(s) = &project.result_snapshot
            && old_taken != Some(s.taken_ms)
            && self.result_origin(id, s.taken_ms).is_none()
            && s.engine_version == crate::snapshot::ENGINE_VERSION
            && design_items(project) <= ORIGIN_MAX_ITEMS
            && crate::model::guarded(|| {
                crate::snapshot::take_checked(project)
                    .is_ok_and(|now| crate::snapshot::compare(s, &now).is_empty())
            })
            .unwrap_or(false)
        {
            // Best effort: without it the result is only not recalculated.
            let _ = self.write_result_origin(
                id,
                &ResultOrigin {
                    taken_ms: s.taken_ms,
                    input: crate::snapshot::input_digest(project),
                    checked: s.engine_version.clone(),
                    refreshed: None,
                    pinned: false,
                },
            );
        }
        Ok(())
    }

    fn result_origin_path(&self, id: &str) -> Result<PathBuf, StoreError> {
        self.path(id)?;
        Ok(self.activity_dir().join(format!("{id}.result.json")))
    }

    fn result_origins(&self, id: &str) -> Vec<ResultOrigin> {
        self.result_origin_path(id)
            .ok()
            .and_then(|p| read_regular(&p, MAX_SMALL_FILE).ok())
            .and_then(|t| serde_json::from_slice(&t).ok())
            .unwrap_or_default()
    }

    /// The origin of the stored result taken at `taken_ms`.
    fn result_origin(&self, id: &str, taken_ms: u64) -> Option<ResultOrigin> {
        self.result_origins(id)
            .into_iter()
            .find(|o| o.taken_ms == taken_ms)
    }

    /// Records `origin`, replacing the one of the same result; the oldest
    /// recorded go beyond [`ORIGINS_KEPT`].
    fn write_result_origin(&self, id: &str, origin: &ResultOrigin) -> Result<(), StoreError> {
        let mut all = self.result_origins(id);
        all.retain(|o| o.taken_ms != origin.taken_ms);
        all.push(origin.clone());
        let over = all.len().saturating_sub(ORIGINS_KEPT);
        all.drain(..over);
        self.write_result_origins(id, &all)
    }

    /// Writes the origin records of `id` as they are, in their order.
    fn write_result_origins(&self, id: &str, all: &[ResultOrigin]) -> Result<(), StoreError> {
        let io = |e| StoreError::io(Some(id), e);
        own_folder(&self.activity_dir()).map_err(io)?;
        let text = serde_json::to_string(&all).map_err(|e| io(e.into()))?;
        write_atomic(&self.result_origin_path(id)?, &text).map_err(io)
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

    /// [`Store::lock`] for a short action of the user, which must not hang
    /// the window behind another writer: waits at most the store's action
    /// wait ([`REFRESH_LOCK_WAIT`]), then fails with [`StoreError::Busy`]
    /// and nothing is done (audit 2026-11-23 ERR-1002, ARCH-1001).
    fn lock_briefly(&self, id: &str) -> Result<Option<fs::File>, StoreError> {
        self.path(id)?;
        self.lock_file_within(Some(id), &format!("{id}.lock"), Some(self.action_wait))
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
                let id = self.free_id(&project.name)?;
                let _lock = self.lock(&id)?;
                self.log_and_write(&id, None, project, actor, summary, None, None, None, None)?;
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
            self.log_and_write(id, before, project, actor, summary, None, None, None, None)?;
        }
        Ok(CheckedSave::Saved { id: id.to_string() })
    }

    /// Logs the change and writes the file; the caller holds the job's lock.
    /// `known`: the origin of the result it writes, as for
    /// [`Store::write_known`]; one of a recalculation gets the entry's time.
    /// Returns the time of the entry.
    #[allow(clippy::too_many_arguments)]
    fn log_and_write(
        &self,
        id: &str,
        before: Option<Project>,
        project: &Project,
        actor: &str,
        summary: &str,
        reason: Option<String>,
        undid_from: Option<u64>,
        restores: Option<u64>,
        mut known: Option<ResultOrigin>,
    ) -> Result<u64, StoreError> {
        // A newer file was refused before: by `load` under this lock for an
        // existing job, and a new id has no file, so `write` does not read
        // the file again (ARCH-403 item 3, ARCH-504 item 1: the job file was
        // read three, then two times per save).
        // Log first: the app reacts to the project file and then reads the
        // log. When the file then cannot be written, the entry is taken out
        // again: it stood for a change that never happened, shown as pending
        // in the history and the review (audit 2026-11-15 ERR-008).
        // The log is read once per save (ARCH-401).
        let log = self.read_log(id)?;
        // Before this entry may push the engine's out of the log.
        self.keep_offer_figures(id, &log.entries);
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
                restores,
                reason,
            },
            log,
        )?;
        if let Some(r) = known.as_mut().and_then(|o| o.refreshed.as_mut()) {
            r.entry_ts_ms = ts_ms;
        }
        self.write_known(id, project, true, known)
            .inspect_err(|_| {
                let _ = undo.apply();
            })?;
        // Trimmed only once the change is saved, so a failed write takes
        // the entry out by cutting the file back and never loses an older
        // entry (ARCH-401). A trim that fails is tried again next save.
        if let Some(entries) = trim {
            let _ = write_atomic(&self.activity_path(id)?, &self.trimmed_log(id, &entries));
        }
        Ok(ts_ms)
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
        #[cfg(test)]
        LOG_READS.with(|n| n.set(n.get() + 1));
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
        self.revert_last_expecting(id, actor, reason, None)
    }

    /// [`Store::revert_last`] of the change the caller saw as the latest,
    /// the one logged at `expect_ts_ms`: when another change is the latest
    /// by now (an AI saved meanwhile) nothing is undone and the error is
    /// [`StoreError::RevertStale`], so a Revert button never undoes another
    /// entry than the one it was drawn on (audit 2026-11-22 ERR-801).
    pub fn revert_last_expecting(
        &self,
        id: &str,
        actor: &str,
        reason: Option<&str>,
        expect_ts_ms: Option<u64>,
    ) -> Result<Project, StoreError> {
        // The log is read, the expected entry checked, the file merged and
        // written under one hold of the job's lock: an AI save waiting for
        // the lock lands before (the press is stale) or after the revert,
        // never between the check and the write (audit 2026-11-23
        // ERR-1002: the older change was undone under the AI's newer one,
        // which then skipped the review).
        if !self.has_job_or_log(id)? {
            return Err(StoreError::NothingToRevert(id.to_string()));
        }
        let _lock = self.lock_briefly(id)?;
        let log = self.activity(id);
        let pos = latest_undone(&log).ok_or_else(|| StoreError::NothingToRevert(id.to_string()))?;
        if expect_ts_ms.is_some_and(|ts| ts != log[pos].ts_ms) {
            return Err(StoreError::RevertStale {
                id: id.to_string(),
                detail: "another change is the latest one by now".into(),
            });
        }
        // Reverting from `pos` also undoes the reverts logged after it, so
        // the job ends as it was before that change.
        let summary = with_reason(reason, format!("{REVERTED_PREFIX}{}", log[pos].summary));
        self.revert_locked(id, &log, pos, actor, summary)
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
        self.revert_to_counted(id, ts_ms, actor, reason)
            .map(|r| r.project)
    }

    /// [`Store::revert_to`] that also says how many recorded changes it
    /// undid, counted under the job's lock: the app tells the user about
    /// AI changes that came in while Revert all was asked (audit
    /// 2026-11-23 ERR-1002).
    pub fn revert_to_counted(
        &self,
        id: &str,
        ts_ms: u64,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<RevertedChanges, StoreError> {
        // As in `revert_last_expecting`: one hold of the lock.
        if !self.has_job_or_log(id)? {
            return Err(StoreError::NoSuchChange {
                id: id.to_string(),
                ts_ms,
            });
        }
        let _lock = self.lock_briefly(id)?;
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
        let project = self.revert_locked(id, &log, pos, actor, with_reason(reason, summary))?;
        Ok(RevertedChanges { project, undone })
    }

    /// Whether the job file or its change log is there: a revert of a job
    /// that has neither makes no lock file for it (as `mark_reviewed`,
    /// HUNT-905).
    fn has_job_or_log(&self, id: &str) -> Result<bool, StoreError> {
        Ok(fs::symlink_metadata(self.path(id)?).is_ok()
            || fs::symlink_metadata(self.activity_path(id)?).is_ok())
    }

    /// Undoes `log[pos..]`, latest first: the inverse of each change is
    /// merged onto the current file with the version it saved as the base.
    /// The app autosaves the user's own edits without a log entry, so the
    /// file can be newer than the last entry's result, and an edit can lie
    /// between two entries; such edits are kept (ERR-001). Refuses, with
    /// nothing written, when one of them touches a value a reverted change
    /// set. The caller holds the job's lock and read `log` under it, so the
    /// file read here is the one written over (no second try with a log
    /// that may be stale, ERR-1002).
    fn revert_locked(
        &self,
        id: &str,
        log: &[ActivityEntry],
        pos: usize,
        actor: &str,
        (summary, reason): (String, Option<String>),
    ) -> Result<Project, StoreError> {
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
        let before = current.clone();
        let mut target = None;
        for (k, entry) in log[pos..].iter().enumerate().rev() {
            // The user's "offer result restored" stays when other changes
            // are undone: it is undone only when asked for by itself.
            if k > 0 && entry.user_restore() {
                continue;
            }
            let earlier = entry
                .before
                .clone()
                .ok_or_else(|| StoreError::RevertCreated(id.to_string()))?;
            let undone = match (entry.after.as_ref(), current.as_ref()) {
                (Some(after), Some(cur)) if cur != after => {
                    let merged =
                        crate::merge::merge3(&value(after)?, &value(cur)?, &value(&earlier)?)
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
                _ => earlier,
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
        if before.as_ref() != Some(&target) {
            self.log_and_write(
                id,
                before,
                &target,
                actor,
                &summary,
                reason,
                Some(log[pos].ts_ms),
                None,
                None,
            )?;
        }
        self.pin_restored(id, &log[pos..], &target);
        Ok(target)
    }

    /// After a revert of `undone` gave `target`: when that put an offer's
    /// own result back in place of the engine's recalculation, the result
    /// stays for good, as after [`Store::restore_offer_result`] (best
    /// effort, like every origin).
    fn pin_restored(&self, id: &str, undone: &[ActivityEntry], target: &Project) {
        let Some(stored) = &target.result_snapshot else {
            return;
        };
        let was_offer = undone
            .iter()
            .filter_map(refresh_figures)
            .any(|f| f.offer_ms == stored.taken_ms);
        if !was_offer {
            return;
        }
        if let Some(mut origin) = self.result_origin(id, stored.taken_ms)
            && !origin.pinned
        {
            origin.pinned = true;
            let _ = self.write_result_origin(id, &origin);
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
    ///
    /// A job that is not there (deleted meanwhile, or never) is refused
    /// with [`StoreError::NotFound`] and gets no mark: a leftover mark made
    /// the next job of that name, and the job restored from the trash,
    /// take another id (audit 2026-11-23 HUNT-905). Looked at before the
    /// job's lock is taken, so no lock file is made for it, and again under
    /// the lock, which a delete holds.
    pub fn mark_reviewed(&self, id: &str, ts_ms: u64) -> Result<u64, StoreError> {
        let path = self.review_path(id)?;
        let job = self.path(id)?;
        let there = || {
            fs::symlink_metadata(&job)
                .map(|_| ())
                .map_err(|_| StoreError::NotFound(id.to_string()))
        };
        there()?;
        // Pressed in the review banner: waits only briefly, then `busy`
        // and the banner stays (audit 2026-11-23 ARCH-1001).
        let _lock = self.lock_briefly(id)?;
        there()?;
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
            .rposition(|e| {
                // The user's "offer result restored" is no look at the AI's
                // changes: the review goes on past it.
                (e.actor == USER_ACTOR && !e.user_restore())
                    || e.actor == ENGINE_ACTOR
                    || e.ts_ms <= accepted
            })
            .map_or(0, |i| i + 1)
    }

    pub fn pending_review(&self, id: &str) -> Option<PendingReview> {
        let log = self.activity(id);
        let start = self.pending_start(id, &log);
        // Tectora's own entries are no AI change to review: a review of
        // them alone is none, and they are not counted in one.
        let pending = &log[start..];
        let ai = || {
            pending
                .iter()
                .filter(|e| e.actor != ENGINE_ACTOR && !e.user_restore())
        };
        let first = ai().next()?;
        let mut actors: Vec<String> = Vec::new();
        for e in ai() {
            if !actors.contains(&e.actor) {
                actors.push(e.actor.clone());
            }
        }
        Some(PendingReview {
            since_ts_ms: first.ts_ms,
            count: ai().count(),
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
        // Its files are moved only from the library's own folder (ARCH-602).
        self.own_files_folders(id).map_err(io)?;
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
            (self.result_origin_path(id)?, folder.join("result.json")),
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
        if self.own_files_folders(id).is_ok() {
            let _ = fs::remove_dir_all(self.files_dir(id)?);
        }
        if let Ok(p) = self.result_origin_path(id) {
            let _ = fs::remove_file(p);
        }
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
        not_foreign(&self.dir.join(FILES_DIR)).map_err(io)?;
        let id = if self.id_used(&info.id).map_err(io)? {
            self.free_id(if info.name.is_empty() {
                &info.id
            } else {
                &info.name
            })?
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
            (folder.join("result.json"), self.result_origin_path(&id)?),
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

    /// Checks that `.files` and `.files/<id>` are each missing or a real
    /// folder ([`not_foreign`]).
    fn own_files_folders(&self, id: &str) -> std::io::Result<()> {
        let files = self.dir.join(FILES_DIR);
        not_foreign(&files)?;
        not_foreign(&files.join(id))
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
        let dir = self.files_dir(id)?;
        // Never a file from where a link points (ARCH-602).
        self.own_files_folders(id)
            .map_err(|e| FileError::io(&dir, e))?;
        let path = dir.join(name);
        // Files are attached under their composed name: a decomposed
        // spelling (copied from macOS Finder) names the same file
        // (HUNT-108).
        let composed = crate::nfc::nfc(name);
        if composed != name && !path.exists() {
            return Ok(self.files_dir(id)?.join(composed.as_ref()));
        }
        Ok(path)
    }

    /// The files attached to job `id`, by name; none when `.files` or
    /// its folder of the job is a link or a file (see
    /// [`Store::try_attachments`]).
    pub fn attachments(&self, id: &str) -> Vec<Attachment> {
        self.try_attachments(id).unwrap_or_default()
    }

    /// [`Store::attachments`], refused with `foreign_entry` when `.files`
    /// or `.files/<id>` is a link or a file: files outside the library are
    /// neither listed, packed into a bundle nor copied (ARCH-602).
    pub fn try_attachments(&self, id: &str) -> Result<Vec<Attachment>, FileError> {
        let dir = self.files_dir(id)?;
        self.own_files_folders(id)
            .map_err(|e| FileError::io(&dir, e))?;
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
        Ok(out)
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
        for a in self.try_attachments(from)? {
            let path = self.attachment_path(from, &a.name)?;
            let bytes = read_regular(&path, MAX_ATTACHMENT)?;
            self.add_attachment(to, &a.name, &bytes)?;
        }
        Ok(())
    }

    /// Saves `project`, a copy of job `from` (prepared with
    /// [`crate::library::prepare_copy`]), as a new job with the files of
    /// `from` when `with_files`, logged as `actor`'s change `summary` when
    /// `logged` is given. A refused copy leaves nothing behind: the files
    /// are checked before the save, and a new job whose files could not
    /// all be copied is removed again (ARCH-602: each refused try left a
    /// new job). The app and the MCP server copy through it.
    pub fn copy_job(
        &self,
        from: &str,
        project: &Project,
        with_files: bool,
        logged: Option<(&str, &str)>,
    ) -> Result<String, FileError> {
        if with_files {
            self.try_attachments(from)?;
        }
        let id = match logged {
            Some((actor, summary)) => self.save_as(None, project, actor, summary)?,
            None => self.save(None, project)?,
        };
        if with_files && let Err(e) = self.copy_attachments(from, &id) {
            let _ = self.purge(&id);
            return Err(e);
        }
        Ok(id)
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
        // The user's "offer result restored" is no change to step back
        // over: the latest change is the one under it.
        if e.user_restore() {
            continue;
        }
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

#[cfg(test)]
thread_local! {
    /// How often [`Store::activity`] read a change log on this thread, for
    /// tests of what reads it (ARCH-1002).
    static LOG_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The summary of a revert or a put-back: the core's sentence `what`,
/// with the AI's reason at its end when there is one, and that reason
/// ([`ActivityEntry::reason`]).
///
/// The whole summary is at most [`MAX_SUMMARY_CHARS`] characters and
/// always ends with ` (reason: <reason>)`: the quoted change is cut to
/// make room, never the reason, which is cut only past what leaves
/// [`MIN_WHAT_CHARS`] for the core's sentence (audit 2026-11-23
/// HUNT-1002, ERR-1003: the log's cap cut the reason off the line).
///
/// [`MAX_SUMMARY_CHARS`]: crate::edit::MAX_SUMMARY_CHARS
fn with_reason(reason: Option<&str>, what: String) -> (String, Option<String>) {
    use crate::edit::{MAX_SUMMARY_CHARS, capped};
    match reason.map(str::trim).filter(|r| !r.is_empty()) {
        Some(r) => {
            // " (reason: " and ")".
            let frame = REVERT_REASON_OPEN.chars().count() + 1;
            let r = capped(r, MAX_SUMMARY_CHARS - frame - MIN_WHAT_CHARS);
            let what = capped(&what, MAX_SUMMARY_CHARS - frame - r.chars().count());
            (format!("{what}{REVERT_REASON_OPEN}{r})"), Some(r))
        }
        None => (what, None),
    }
}

/// Characters of a revert's or put-back's own sentence that an AI's long
/// reason leaves ([`with_reason`]).
const MIN_WHAT_CHARS: usize = 120;

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

    /// A store of its own for one test. Its cards wait for their figures
    /// up to [`TEST_BUDGET`], not the app's half second: on a busy runner a
    /// card read after the budget made tests that check the figures fail at
    /// random (audit 2026-11-22 TEST-603). Tests of the budget itself set
    /// their own ([`Store::with_summary_budget`]).
    fn temp_store(name: &str) -> Store {
        let dir = std::env::temp_dir().join(format!("tectora-test-{name}-{}", now_ms()));
        Store::new(dir).unwrap().with_summary_budget(TEST_BUDGET)
    }

    /// How long a test's card waits for its figures.
    const TEST_BUDGET: std::time::Duration = std::time::Duration::from_secs(60);

    /// Audit 2026-11-23 HUNT-905: Accept on a job that was deleted
    /// meanwhile, or never existed, is refused and leaves nothing behind,
    /// so the next job of that name keeps the name as its id.
    #[test]
    fn accepting_a_job_that_is_not_there_leaves_no_mark() {
        let store = temp_store("accept-gone");
        let p = examples::find("gable").unwrap().project();
        let id = store.save(None, &p).unwrap();
        assert!(store.mark_reviewed(&id, 5).is_ok());
        store.delete(&id, "claude", "Deleted.").unwrap();
        let review = store.activity_dir().join(format!("{id}.review.json"));
        let _ = fs::remove_file(&review);
        assert_eq!(store.mark_reviewed(&id, 6).unwrap_err().code(), "not_found");
        assert!(fs::symlink_metadata(&review).is_err());
        assert_eq!(store.save(None, &p).unwrap(), id);
        // Never there: no mark and no lock file.
        assert_eq!(
            store.mark_reviewed("never-there", 5).unwrap_err().code(),
            "not_found"
        );
        for leftover in ["never-there.review.json", "never-there.lock"] {
            assert!(
                fs::symlink_metadata(store.activity_dir().join(leftover)).is_err(),
                "{leftover}"
            );
        }
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// Audit 2026-11-23 ARCH-901: every name only Tectora writes is
    /// reserved, in any spelling, and nothing else is.
    #[test]
    fn every_reserved_actor_is_known_in_any_spelling() {
        for name in [
            "user",
            " User ",
            "tectora",
            "TECTORA",
            "tectora (received)",
            " Tectora (Received) ",
            "tectora (received)\u{200b}",
        ] {
            assert!(is_reserved_actor(name), "{name:?}");
        }
        assert_eq!(
            reserved_actor(" Tectora (Received) "),
            Some(RECEIVED_ENGINE_ACTOR)
        );
        for name in ["claude", "AI (tectora)", "tectora received", "users", ""] {
            assert!(!is_reserved_actor(name), "{name:?}");
        }
    }

    /// Audit 2026-11-22 TEST-603: the test store waits long enough for a
    /// card's figures on a loaded machine.
    #[test]
    fn the_test_store_waits_for_figures_under_load() {
        assert!(temp_store("budget").summary_budget >= std::time::Duration::from_secs(30));
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
                restores: None,
                reason: None,
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

    /// Audit 2026-11-22 ARCH-504 item 1 (ARCH-403 item 3): a checked save
    /// reads the job file once, under its lock, not once more to write it.
    #[test]
    fn a_checked_save_reads_the_job_file_once() {
        let store = temp_store("reads");
        let project = examples::find("hip").unwrap().project();
        let id = store.save_as(None, &project, USER_ACTOR, "Made.").unwrap();
        let reads = |f: &dyn Fn()| {
            JOB_READS.with(|n| n.set(0));
            f();
            JOB_READS.with(|n| n.get())
        };
        let mut p = project.clone();
        p.notes = "one".into();
        let n = reads(&|| {
            store
                .save_as_checked(&id, &p, "claude", "One.", Some(&project))
                .unwrap();
        });
        assert_eq!(n, 1, "a logged save");
        let mut q = p.clone();
        q.notes = "two".into();
        let n = reads(&|| {
            store.save_checked(&id, &q, &p).unwrap();
        });
        assert_eq!(n, 1, "the app's save");
        // A newer file is still refused, also by a save that does not load.
        let newer = q.to_json().unwrap().replacen(
            &format!("\"schema_version\": {}", crate::model::SCHEMA_VERSION),
            "\"schema_version\": 999",
            1,
        );
        assert!(newer.contains("999"));
        fs::write(store.path(&id).unwrap(), &newer).unwrap();
        assert_eq!(
            store.save(Some(id.clone()), &q).unwrap_err().code(),
            "newer_format"
        );
        assert_eq!(
            store.save_checked(&id, &q, &q).unwrap_err().code(),
            "newer_format"
        );
        assert_eq!(fs::read_to_string(store.path(&id).unwrap()).unwrap(), newer);
        let _ = fs::remove_dir_all(&store.dir);
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

    /// Runs `f` on its own thread and fails the test when it does not
    /// answer within `secs` (a spinning loop must not hang the test run).
    fn within<T: Send + 'static>(
        secs: u64,
        what: &str,
        f: impl FnOnce() -> T + Send + 'static,
    ) -> T {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
        rx.recv_timeout(std::time::Duration::from_secs(secs))
            .unwrap_or_else(|_| panic!("{what}: no answer within {secs} s"))
    }

    /// Audit 2026-11-22 ARCH-601: a plain file (or a link to itself) named
    /// `.files` made every new job spin for ever looking for a free id,
    /// holding the creation lock. A new job, a logged new job, a second
    /// store on the same library and a restore are now refused with
    /// `foreign_entry` at once.
    #[test]
    fn a_file_named_files_refuses_new_jobs_instead_of_spinning() {
        let project = examples::find("hip").unwrap().project();
        let store = temp_store("files-file");
        let deleted = store.save_as(None, &project, USER_ACTOR, "Made.").unwrap();
        let entry = store.delete(&deleted, USER_ACTOR, "Gone.").unwrap().entry;
        let _ = fs::remove_dir_all(store.dir.join(FILES_DIR));
        fs::write(store.dir.join(FILES_DIR), "x").unwrap();
        type Make = Box<dyn Fn(&Path)>;
        let mut kinds: Vec<(&str, Make)> = vec![("a file", Box::new(|_: &Path| {}))];
        #[cfg(unix)]
        kinds.push((
            "a link to itself",
            Box::new(|dir: &Path| {
                fs::remove_file(dir.join(FILES_DIR)).unwrap();
                std::os::unix::fs::symlink(FILES_DIR, dir.join(FILES_DIR)).unwrap();
            }),
        ));
        for (what, make) in kinds {
            make(&store.dir);
            let dir = store.dir.clone();
            let p = project.clone();
            let entry = entry.clone();
            let codes = within(10, what, move || {
                let one = Store::new(dir.clone()).unwrap();
                let other = Store::new(dir).unwrap();
                [
                    one.save(None, &p).unwrap_err().code(),
                    one.save_as(None, &p, "claude", "Made.").unwrap_err().code(),
                    other.save(None, &p).unwrap_err().code(),
                    one.restore(&entry).unwrap_err().code(),
                ]
            });
            assert_eq!(codes, ["foreign_entry"; 4], "{what}");
        }
        let left = fs::read_dir(&store.dir)
            .unwrap()
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == EXTENSION))
            .count();
        assert_eq!(left, 0, "no job was made");
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// Audit 2026-11-22 ARCH-601: a library whose ids are all taken gives
    /// an error, not an endless search.
    #[test]
    fn picking_a_new_id_gives_up_with_an_error() {
        let store = temp_store("ids-bound");
        let project = examples::find("hip").unwrap().project();
        // Every candidate is taken: a stand-in that says so.
        let picked = store.free_id_with(&project.name, |_| Ok(true));
        assert!(picked.is_err(), "{picked:?}");
        // A path that cannot be checked is an error, not "taken".
        let picked = store.free_id_with(&project.name, |_| {
            Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
        });
        assert_eq!(picked.unwrap_err().code(), "permission_denied");
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// Audit 2026-11-22 ARCH-602: with `.files` or `.files/<id>` a link to
    /// a folder outside the library, deleting the job, removing a file,
    /// copying the job, listing its files for MCP and exporting it are all
    /// refused with `foreign_entry`: nothing outside is moved, packed or
    /// pruned, and a refused copy leaves no new job behind.
    #[cfg(unix)]
    #[test]
    fn a_linked_files_folder_is_neither_moved_read_nor_copied() {
        let project = examples::find("hip").unwrap().project();
        for rel in [FILES_DIR.to_string(), format!("{FILES_DIR}/ref")] {
            let store = temp_store("files-link-read");
            let outside = std::env::temp_dir().join(format!("tectora-files-out-{}", now_ms()));
            let id = store
                .save_as(Some("ref".into()), &project, USER_ACTOR, "Made.")
                .unwrap();
            store
                .add_attachment(&id, "plan.pdf", b"%PDF precious")
                .unwrap();
            // The real folder moves outside and a link takes its place.
            let at = store.dir.join(&rel);
            fs::create_dir_all(&outside).unwrap();
            let target = outside.join("t");
            fs::rename(&at, &target).unwrap();
            std::os::unix::fs::symlink(&target, &at).unwrap();
            let precious = if rel == FILES_DIR {
                target.join("ref").join("plan.pdf")
            } else {
                target.join("plan.pdf")
            };
            let jobs = store.list().len();
            let foreign = |code: &str, what: &str| {
                assert_eq!(code, "foreign_entry", "{rel}: {what}");
                assert_eq!(
                    fs::read(&precious).unwrap(),
                    b"%PDF precious",
                    "{rel}: {what} changed the file outside"
                );
            };
            foreign(
                store.delete(&id, USER_ACTOR, "Gone.").unwrap_err().code(),
                "delete",
            );
            assert!(store.exists(&id), "{rel}: the job stays");
            foreign(
                store
                    .remove_attachment(&id, "plan.pdf", USER_ACTOR, "x")
                    .unwrap_err()
                    .code,
                "remove_attachment",
            );
            foreign(
                store.try_attachments(&id).unwrap_err().code,
                "try_attachments",
            );
            assert!(
                store.attachments(&id).is_empty(),
                "{rel}: listed from outside"
            );
            foreign(
                store.attachment_path(&id, "plan.pdf").unwrap_err().code,
                "attachment_path",
            );
            let mut copy = project.clone();
            copy.name = "Copy".into();
            for logged in [None, Some(("claude", "Copied."))] {
                foreign(
                    store.copy_job(&id, &copy, true, logged).unwrap_err().code,
                    "copy_job",
                );
            }
            assert_eq!(store.list().len(), jobs, "{rel}: a refused copy left a job");
            let dest = store.dir.join("x.tectorajob");
            foreign(
                crate::bundle::export(&store, &id, &dest).unwrap_err().code,
                "export",
            );
            assert!(!dest.exists());
            let _ = fs::remove_dir_all(&store.dir);
            let _ = fs::remove_dir_all(&outside);
        }
    }

    /// Audit 2026-11-22 UX-601: the library names a link or a file in
    /// place of one of its own folders before anything is edited.
    #[test]
    fn the_library_names_a_link_or_file_in_place_of_its_folders() {
        let store = temp_store("foreign-list");
        let project = examples::find("hip").unwrap().project();
        let id = store.save_as(None, &project, USER_ACTOR, "Made.").unwrap();
        store.add_attachment(&id, "plan.pdf", b"%PDF").unwrap();
        assert!(store.foreign_entries().is_empty());
        fs::create_dir_all(store.dir.join(crate::bundle::EXPORTS_DIR)).unwrap();
        fs::remove_dir_all(store.dir.join(crate::bundle::EXPORTS_DIR)).unwrap();
        fs::write(store.dir.join(crate::bundle::EXPORTS_DIR), "x").unwrap();
        fs::remove_dir_all(store.files_dir(&id).unwrap()).unwrap();
        fs::write(store.files_dir(&id).unwrap(), "x").unwrap();
        #[cfg(unix)]
        {
            let out = std::env::temp_dir().join(format!("tectora-foreign-out-{}", now_ms()));
            let activity = store.activity_dir();
            fs::rename(&activity, &out).unwrap();
            std::os::unix::fs::symlink(&out, &activity).unwrap();
        }
        let found = store.foreign_entries();
        let ids: Vec<&str> = found.iter().map(|f| f.id.as_str()).collect();
        let mut want = vec![
            crate::bundle::EXPORTS_DIR.to_string(),
            format!("{FILES_DIR}/{id}"),
        ];
        if cfg!(unix) {
            want.insert(0, ACTIVITY_DIR.to_string());
        }
        assert_eq!(ids, want);
        assert!(found.iter().all(|f| f.code == "foreign_entry"));
        // The detail says the data is kept and how to get it back.
        assert!(
            found[0].detail.contains("kept") && found[0].detail.contains("moved into the library"),
            "{}",
            found[0].detail
        );
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// Audit 2026-11-22 ARCH-701 / TEST-709: only what blocks work is
    /// named, each with what it blocks; a stray file in `.files` is not,
    /// a lock folder with files is.
    #[test]
    fn the_library_names_what_blocks_work_and_no_stray_file() {
        let store = temp_store("foreign-kinds");
        let project = examples::find("hip").unwrap().project();
        let id = store.save_as(None, &project, USER_ACTOR, "Made.").unwrap();
        let other = store.save_as(None, &project, USER_ACTOR, "Made.").unwrap();
        store.add_attachment(&id, "plan.pdf", b"%PDF").unwrap();
        let files = store.dir.join(FILES_DIR);
        for stray in ["Thumbs.db", "desktop.ini", "stray-plan.pdf", "no-such-job"] {
            fs::write(files.join(stray), "x").unwrap();
        }
        assert!(
            store.foreign_entries().is_empty(),
            "{:?}",
            store.foreign_entries()
        );
        // A file in place of an existing job's folder blocks its files.
        fs::write(files.join(&other), "x").unwrap();
        // An empty lock folder is replaced by the next save: not named.
        let activity = store.activity_dir();
        for name in [
            format!("{other}.lock"),
            format!("{id}.lock"),
            ".new.lock".into(),
        ] {
            let _ = fs::remove_file(activity.join(name));
        }
        fs::create_dir_all(activity.join(format!("{other}.lock"))).unwrap();
        fs::create_dir_all(activity.join(format!("{id}.lock"))).unwrap();
        fs::write(activity.join(format!("{id}.lock")).join("x"), "x").unwrap();
        fs::create_dir_all(activity.join(".new.lock").join("y")).unwrap();
        fs::create_dir_all(activity.join(format!("{other}.lock.fix"))).unwrap();
        fs::write(store.dir.join(TRASH_DIR), "x").unwrap();
        let found = store.foreign_entries();
        let got: Vec<(&str, &str, Option<&str>)> = found
            .iter()
            .map(|f| (f.id.as_str(), f.kind, f.job.as_deref()))
            .collect();
        let (lock, fix) = (
            format!("{ACTIVITY_DIR}/{id}.lock"),
            format!("{ACTIVITY_DIR}/{other}.lock.fix"),
        );
        let in_files = format!("{FILES_DIR}/{other}");
        let mut want = vec![
            (TRASH_DIR, "trash", None),
            (in_files.as_str(), "job_files", Some(other.as_str())),
            (".activity/.new.lock", "new_lock", None),
            (lock.as_str(), "lock", Some(id.as_str())),
            // Its own kind: it does not stop every save (ARCH-803).
            (fix.as_str(), "lock_fix", Some(other.as_str())),
        ];
        want[2..].sort();
        assert_eq!(got, want);
        assert!(found.iter().all(|f| f.code == "foreign_entry"));
        let lock_detail = &found.iter().find(|f| f.id == lock).unwrap().detail;
        assert!(
            lock_detail.contains("every save of this job is refused")
                && lock_detail.contains("move it out"),
            "{lock_detail}"
        );
        // It is what the save refuses, with the same advice.
        let e = store.save_as(Some(id.clone()), &project, USER_ACTOR, "Again.");
        let detail = e.unwrap_err().detail().unwrap_or_default().to_string();
        assert!(detail.contains("move it out"), "{detail}");
        let _ = fs::remove_dir_all(&store.dir);

        // A file in place of `.files` itself.
        let store = temp_store("foreign-files");
        store.save_as(None, &project, USER_ACTOR, "Made.").unwrap();
        let _ = fs::remove_dir_all(store.dir.join(FILES_DIR));
        fs::write(store.dir.join(FILES_DIR), "x").unwrap();
        let found = store.foreign_entries();
        let got: Vec<(&str, &str)> = found.iter().map(|f| (f.id.as_str(), f.kind)).collect();
        assert_eq!(got, vec![(FILES_DIR, "files")]);
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// TEST-709: a link at `.trash` and at most 20 links in `.files`, sorted.
    #[cfg(unix)]
    #[test]
    fn the_library_names_links_at_the_trash_and_twenty_in_files() {
        let store = temp_store("foreign-links");
        let project = examples::find("hip").unwrap().project();
        store.save_as(None, &project, USER_ACTOR, "Made.").unwrap();
        let out = std::env::temp_dir().join(format!("tectora-foreign-links-{}", now_ms()));
        fs::create_dir_all(&out).unwrap();
        let _ = fs::remove_dir_all(store.trash_dir());
        std::os::unix::fs::symlink(&out, store.trash_dir()).unwrap();
        let files = store.dir.join(FILES_DIR);
        fs::create_dir_all(&files).unwrap();
        for n in 0..25 {
            std::os::unix::fs::symlink(&out, files.join(format!("link-{n:02}"))).unwrap();
        }
        // Hidden names are left to the system.
        std::os::unix::fs::symlink(&out, files.join(".hidden")).unwrap();
        let found = store.foreign_entries();
        assert_eq!(found[0].id, TRASH_DIR);
        assert_eq!(found[0].kind, "trash");
        let linked: Vec<String> = found[1..].iter().map(|f| f.id.clone()).collect();
        let want: Vec<String> = (0..20)
            .map(|n| format!("{FILES_DIR}/link-{n:02}"))
            .collect();
        assert_eq!(linked, want);
        assert!(found[1..].iter().all(|f| f.kind == "job_files"));
        let _ = fs::remove_dir_all(&store.dir);
        let _ = fs::remove_dir_all(&out);
    }

    /// Audit 2026-11-22 ARCH-602: a copy whose files cannot all be copied
    /// leaves no new job behind.
    #[test]
    fn a_copy_that_cannot_take_its_files_leaves_no_job() {
        let store = temp_store("copy-fails");
        let project = examples::find("hip").unwrap().project();
        let id = store.save(None, &project).unwrap();
        store.add_attachment(&id, "plan.pdf", b"%PDF").unwrap();
        // Over the size bound: read_regular refuses it.
        let big = store.files_dir(&id).unwrap().join("big.pdf");
        fs::File::create(&big)
            .unwrap()
            .set_len(MAX_ATTACHMENT + 1)
            .unwrap();
        let mut copy = project.clone();
        copy.name = "Copy".into();
        let jobs = store.list().len();
        assert!(store.copy_job(&id, &copy, true, None).is_err());
        assert_eq!(store.list().len(), jobs);
        let made = store.copy_job(&id, &copy, false, None).unwrap();
        assert!(store.exists(&made));
        let _ = fs::remove_dir_all(&store.dir);
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
        // The outlines only: the figures are not waited for.
        let store = temp_store("card-bound").with_summary_budget(SUMMARY_BUDGET);
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

    /// Audit 2026-11-22 HUNT-601: outlines of 5 to 8 points over the
    /// card's budget lost a corner (a rectangle with a point on one side
    /// became a triangle, an L shape a skewed quadrilateral) while a fifth
    /// of the budget stayed unused. 400 of them now keep their shape.
    #[test]
    fn a_card_over_its_point_budget_keeps_small_outlines_whole() {
        let rect5 = |x: f64| {
            vec![
                [x, 0.0],
                [x + 5.0, 0.0],
                [x + 10.0, 0.0],
                [x + 10.0, 8.0],
                [x, 8.0],
            ]
        };
        let ell = |x: f64| {
            vec![
                [x, 0.0],
                [x + 10.0, 0.0],
                [x + 10.0, 4.0],
                [x + 4.0, 4.0],
                [x + 4.0, 8.0],
                [x, 8.0],
            ]
        };
        let ell7 = |x: f64| {
            let mut o = ell(x);
            o.insert(1, [x + 5.0, 0.0]);
            o
        };
        let bbox = |o: &[[f64; 2]]| {
            o.iter()
                .fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |b, p| {
                    [
                        b[0].min(p[0]),
                        b[1].min(p[1]),
                        b[2].max(p[0]),
                        b[3].max(p[1]),
                    ]
                })
        };
        let area = |o: &[[f64; 2]]| {
            (0..o.len())
                .map(|i| {
                    let (a, b) = (o[i], o[(i + 1) % o.len()]);
                    a[0] * b[1] - b[0] * a[1]
                })
                .sum::<f64>()
                .abs()
                / 2.0
        };
        type Make<'a> = &'a dyn Fn(f64) -> Vec<[f64; 2]>;
        let kinds: [(&str, Make); 4] = [
            ("rect5", &rect5),
            ("L", &ell),
            ("L7", &ell7),
            ("mixed", &|x: f64| {
                if (x as usize / 20).is_multiple_of(2) {
                    rect5(x)
                } else {
                    ell7(x)
                }
            }),
        ];
        for (name, make) in kinds {
            for n in [399, 400, 401, 500] {
                let input: Vec<Vec<[f64; 2]>> = (0..n).map(|i| make(i as f64 * 20.0)).collect();
                let card = bounded_outlines(input.clone(), MAX_CARD_POINTS);
                let points: usize = card.iter().map(Vec::len).sum();
                assert!(points <= MAX_CARD_POINTS, "{name} {n}: {points}");
                // Each drawn outline has the shape it was given: its box
                // and its area (a lost corner changes the area).
                // Only the box around the ones left out, last, is none of
                // the input outlines.
                for (k, o) in card.iter().enumerate() {
                    match input.iter().find(|i| bbox(i) == bbox(o)) {
                        Some(i) => assert!(
                            (area(o) - area(i)).abs() < 1e-9,
                            "{name} {n}: {o:?} of {i:?}"
                        ),
                        None => assert!(k + 1 == card.len() && o.len() == 4, "{name} {n}: {o:?}"),
                    }
                }
                // All drawn when their corners fit the budget (a
                // rectangle's side point is no corner).
                let corners = match name {
                    "rect5" => 4,
                    "L" | "L7" => 6,
                    _ => 5,
                };
                if n * corners <= MAX_CARD_POINTS - 4 {
                    assert_eq!(card.len(), n, "{name} {n}: all drawn");
                } else {
                    assert!(
                        points >= MAX_CARD_POINTS - 4 - 8,
                        "{name} {n}: {points} used"
                    );
                }
                let max_x = card.iter().flatten().map(|p| p[0]).fold(f64::MIN, f64::max);
                assert_eq!(max_x, (n - 1) as f64 * 20.0 + 10.0, "{name} {n}: extent");
            }
        }
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

    /// A gable offer whose result an older engine stored with a roof area
    /// 2 m² off (a calculation fixed since), saved to a new library.
    /// An offer whose result an older engine took from this design: saved
    /// by this engine, then file and origin set back as engine 0.0.1 with
    /// figures `extra` m2 off would have left them.
    fn old_offer_off_by(store: &Store, extra: f64) -> (String, Project) {
        old_offer_of(store, examples::find("gable").unwrap().project(), extra)
    }

    /// [`old_offer_off_by`] of the design `p`, a job in status offer (only
    /// offers are recalculated, UX-702g).
    fn old_offer_of(store: &Store, mut p: Project, extra: f64) -> (String, Project) {
        p.job.status = crate::model::JobStatus::Offer;
        p.result_snapshot = Some(crate::snapshot::take(&p));
        let id = store.save(None, &p).unwrap();
        let taken = p.result_snapshot.as_ref().unwrap().taken_ms;
        let mut origin = store.result_origin(&id, taken).expect("origin recorded");
        let s = p.result_snapshot.as_mut().unwrap();
        s.engine_version = "0.0.1".into();
        s.roof_area += extra;
        fs::write(store.path(&id).unwrap(), p.to_json().unwrap()).unwrap();
        origin.checked = "0.0.1".into();
        store.write_result_origin(&id, &origin).unwrap();
        (id, p)
    }

    fn old_offer(store: &Store) -> (String, Project) {
        old_offer_off_by(store, 2.0)
    }

    #[test]
    fn an_imported_offer_whose_design_was_edited_is_never_recalculated() {
        // R2-1: a bundle import saves the job with its stored result under
        // a new id; the design was edited after the result was taken.
        let store = temp_store("refresh-import");
        let mut p = examples::find("gable").unwrap().project();
        let mut s = crate::snapshot::take(&p);
        p.parts[0].edges[0].pitch_deg = 25.0;
        s.engine_version = "0.0.1".into();
        p.result_snapshot = Some(s.clone());
        let id = store.save(None, &p).unwrap();
        assert!(store.result_origins(&id).is_empty());
        assert!(!store.refresh_stored_result(&id).unwrap());
        assert_eq!(store.load(&id).unwrap().result_snapshot, Some(s.clone()));
        // Nor with this engine's version on figures of the old design.
        s.engine_version = crate::snapshot::ENGINE_VERSION.into();
        p.result_snapshot = Some(s);
        let id = store.save(None, &p).unwrap();
        assert!(store.result_origins(&id).is_empty());
        let _ = fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn opening_a_job_an_older_engine_stored_updates_its_result_with_a_log_entry() {
        let store = temp_store("refresh");
        let (id, p) = old_offer(&store);
        let s = p.result_snapshot.clone().unwrap();
        let before = store.activity(&id).len();
        assert!(store.refresh_stored_result(&id).unwrap());
        let q = store.load(&id).unwrap();
        let now = q.result_snapshot.as_ref().unwrap();
        assert_eq!(now.engine_version, crate::snapshot::ENGINE_VERSION);
        assert!((now.roof_area - (s.roof_area - 2.0)).abs() < 0.01);
        let log = store.activity(&id);
        assert_eq!(log.len(), before + 1);
        let entry = log.last().unwrap();
        assert_eq!(entry.actor, ENGINE_ACTOR);
        assert!(entry.summary.starts_with(crate::snapshot::REFRESHED_PREFIX));
        // The old figures stay in the log.
        assert_eq!(entry.before.as_ref().unwrap().result_snapshot, Some(s));
        // Once only.
        assert!(!store.refresh_stored_result(&id).unwrap());
        assert_eq!(store.activity(&id).len(), before + 1);
        let _ = fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn an_offer_whose_design_was_edited_keeps_its_stored_figures() {
        let store = temp_store("refresh-edited");
        let (id, mut p) = old_offer(&store);
        p.parts[0].edges[0].pitch_deg = 25.0;
        store.save(Some(id.clone()), &p).unwrap();
        assert!(!store.refresh_stored_result(&id).unwrap());
        assert_eq!(store.load(&id).unwrap().result_snapshot, p.result_snapshot);
        // A change of the name, notes or job data is no design change.
        let (id, mut p) = old_offer(&store);
        p.name = "renamed".into();
        p.notes = "called back".into();
        store.save(Some(id.clone()), &p).unwrap();
        assert!(store.refresh_stored_result(&id).unwrap());
        let _ = fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn a_result_stored_before_its_origin_was_kept_stays() {
        let store = temp_store("refresh-legacy");
        let (id, _) = old_offer(&store);
        fs::remove_file(store.result_origin_path(&id).unwrap()).unwrap();
        assert!(!store.refresh_stored_result(&id).unwrap());
        let _ = fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn a_reverted_recalculation_stays_reverted() {
        let store = temp_store("refresh-revert");
        let (id, p) = old_offer(&store);
        assert!(store.refresh_stored_result(&id).unwrap());
        store.revert_last(&id, USER_ACTOR, None).unwrap();
        assert_eq!(store.load(&id).unwrap().result_snapshot, p.result_snapshot);
        assert!(!store.refresh_stored_result(&id).unwrap());
        // So does a copy loaded before the recalculation and saved later.
        let (id, p) = old_offer(&store);
        assert!(store.refresh_stored_result(&id).unwrap());
        store.save(Some(id.clone()), &p).unwrap();
        assert!(!store.refresh_stored_result(&id).unwrap());
        let _ = fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn an_offer_put_back_by_rejecting_a_new_result_still_follows_the_engine() {
        // Hunt R3 BUG-2: the origin then described the rejected result.
        let store = temp_store("refresh-back");
        let (id, offer) = old_offer(&store);
        let mut p = offer.clone();
        let mut s = crate::snapshot::take(&p);
        s.taken_ms = offer.result_snapshot.as_ref().unwrap().taken_ms + 1000;
        p.result_snapshot = Some(s);
        store
            .save_as(Some(id.clone()), &p, "claude-ai", "Store result")
            .unwrap();
        store.revert_last(&id, USER_ACTOR, None).unwrap();
        assert_eq!(
            store.load(&id).unwrap().result_snapshot,
            offer.result_snapshot
        );
        assert!(store.refresh_stored_result(&id).unwrap());
        let _ = fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn a_result_stored_after_one_dated_ahead_gets_its_origin() {
        // Hunt R3 BUG-5: a colleague's clock an hour ahead blocked every
        // later result.
        let store = temp_store("refresh-ahead");
        let mut p = examples::find("gable").unwrap().project();
        let mut s = crate::snapshot::take(&p);
        let now = s.taken_ms;
        s.taken_ms = u64::MAX;
        p.result_snapshot = Some(s);
        let id = store.save(None, &p).unwrap();
        let mut s = crate::snapshot::take(&p);
        s.taken_ms = now;
        p.result_snapshot = Some(s);
        store.save(Some(id.clone()), &p).unwrap();
        assert!(store.result_origin(&id, now).is_some());
        let _ = fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn a_design_too_large_to_check_on_save_gets_no_origin() {
        // Hunt R3 BUG-1: checking the result held the job's lock for a
        // minute on a hostile file.
        let store = temp_store("refresh-large");
        let mut p = examples::find("gable").unwrap().project();
        let e = p.parts[0].edges[0].clone();
        for _ in 0..ORIGIN_MAX_ITEMS {
            p.parts[0].edges[0].breaks.push(crate::model::PitchBreak {
                height: e.eave_height + 1.0,
                pitch_deg: e.pitch_deg,
            });
        }
        let mut s = crate::snapshot::take(&examples::find("gable").unwrap().project());
        s.engine_version = crate::snapshot::ENGINE_VERSION.into();
        p.result_snapshot = Some(s);
        let t = std::time::Instant::now();
        let id = store.save(None, &p).unwrap();
        assert!(store.result_origins(&id).is_empty());
        assert!(t.elapsed().as_secs() < 5);
        let _ = fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn a_recalculation_that_cannot_be_saved_is_tried_again() {
        // Audit ARCH-502: the origin was marked checked before the save,
        // so a failed save left the old figures for good.
        let store = temp_store("refresh-fail");
        let (id, p) = old_offer(&store);
        let log = store.activity_path(&id).unwrap();
        let _ = fs::remove_file(&log);
        fs::create_dir_all(&log).unwrap();
        assert!(store.refresh_stored_result(&id).is_err());
        assert_eq!(store.load(&id).unwrap().result_snapshot, p.result_snapshot);
        fs::remove_dir_all(&log).unwrap();
        assert!(store.refresh_stored_result(&id).unwrap());
        let _ = fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn a_purged_job_leaves_no_result_origin_behind() {
        // Hunt R2 BUG-4: a failed bundle import kept `<id>.result.json`
        // and the next job of that name got another id.
        let store = temp_store("refresh-purge");
        let (id, _) = old_offer(&store);
        assert!(store.result_origin_path(&id).unwrap().exists());
        store.purge(&id).unwrap();
        assert!(!store.result_origin_path(&id).unwrap().exists());
        assert_eq!(store.new_id(&id), id);
        let _ = fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn a_stored_result_waits_for_a_pending_ai_review() {
        // Hunt R2 BUG-3: a recalculation after an AI change made Revert
        // undo the recalculation and keep the AI's change.
        let store = temp_store("refresh-review");
        let (id, mut p) = old_offer(&store);
        p.job.customer = "WRONG".into();
        store
            .save_as(Some(id.clone()), &p, "claude-ai", "Customer")
            .unwrap();
        assert!(!store.refresh_stored_result(&id).unwrap());
        store.revert_last(&id, USER_ACTOR, None).unwrap();
        let q = store.load(&id).unwrap();
        assert_ne!(q.job.customer, "WRONG");
        // Reviewed, the result follows the corrected calculation.
        assert!(store.refresh_stored_result(&id).unwrap());
        // An AI change after it starts its own review: rejecting it keeps
        // the recalculated result.
        let mut p = store.load(&id).unwrap();
        let fresh = p.result_snapshot.clone();
        p.notes = "by the AI".into();
        store
            .save_as(Some(id.clone()), &p, "claude-ai", "Notes")
            .unwrap();
        let pending = store.pending_review(&id).unwrap();
        assert_eq!(pending.count, 1);
        assert_eq!(pending.before.unwrap().result_snapshot, fresh);
        // Alone it is no review.
        let (id, _) = old_offer(&store);
        assert!(store.refresh_stored_result(&id).unwrap());
        assert!(store.pending_review(&id).is_none());
        let _ = fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn a_result_with_the_same_figures_is_checked_once() {
        let store = temp_store("refresh-same");
        let (id, _) = old_offer_off_by(&store, 0.0);
        assert!(!store.refresh_stored_result(&id).unwrap());
        assert_eq!(
            store.result_origins(&id)[0].checked,
            crate::snapshot::ENGINE_VERSION
        );
        let _ = fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn only_an_offer_follows_the_corrected_calculation() {
        // Jakub's decision 2026-11-22 (audit UX-702g): a draft, an ordered
        // and a done job keep their stored figures.
        use crate::model::JobStatus;
        let store = temp_store("refresh-status");
        for (status, refreshed) in [
            (JobStatus::Draft, false),
            (JobStatus::Offer, true),
            (JobStatus::Ordered, false),
            (JobStatus::Done, false),
        ] {
            let (id, mut p) = old_offer(&store);
            p.job.status = status;
            fs::write(store.path(&id).unwrap(), p.to_json().unwrap()).unwrap();
            assert_eq!(
                store.refresh_stored_result(&id).unwrap(),
                refreshed,
                "{status:?}"
            );
            if !refreshed {
                assert_eq!(store.load(&id).unwrap().result_snapshot, p.result_snapshot);
                assert!(store.activity(&id).iter().all(|e| e.actor != ENGINE_ACTOR));
            }
        }
        let _ = fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn reverting_the_recalculation_puts_the_offer_figures_back_for_good() {
        // Jakub's decision 2026-11-22 (audit UX-702d): the user may put the
        // old offer figures back, by Revert of the engine's entry.
        let store = temp_store("refresh-revert-to");
        let (id, p) = old_offer(&store);
        let summary = store.refresh_result(&id).unwrap().expect("recalculated");
        let entry = store.activity(&id).last().unwrap().clone();
        assert_eq!(
            (entry.actor.as_str(), entry.summary.as_str()),
            (ENGINE_ACTOR, summary.as_str())
        );
        let back = store.revert_to(&id, entry.ts_ms, USER_ACTOR, None).unwrap();
        assert_eq!(back.result_snapshot, p.result_snapshot);
        assert_eq!(store.load(&id).unwrap().result_snapshot, p.result_snapshot);
        // Not recalculated again on the next opens.
        assert_eq!(store.refresh_result(&id).unwrap(), None);
        assert_eq!(store.refresh_result(&id).unwrap(), None);
        assert_eq!(store.load(&id).unwrap().result_snapshot, p.result_snapshot);
        let engine = store
            .activity(&id)
            .iter()
            .filter(|e| e.actor == ENGINE_ACTOR)
            .count();
        assert_eq!(engine, 1);
        let _ = fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn a_job_whose_lock_cannot_be_taken_is_not_recalculated() {
        // Audit 2026-11-22 ARCH-702: the refresh went on without the lock
        // that every other save of the job was refused for.
        let store = temp_store("refresh-lockdir");
        let (id, p) = old_offer(&store);
        let lock = store.activity_dir().join(format!("{id}.lock"));
        let _ = fs::remove_file(&lock);
        fs::create_dir_all(&lock).unwrap();
        fs::write(lock.join("x"), "x").unwrap();
        let log = store.activity(&id).len();
        assert!(!store.refresh_stored_result(&id).unwrap());
        assert_eq!(store.refresh_result(&id).unwrap(), None);
        assert_eq!(store.load(&id).unwrap().result_snapshot, p.result_snapshot);
        assert_eq!(store.activity(&id).len(), log);
        // Once the folder is gone, the next open recalculates it.
        fs::remove_dir_all(&lock).unwrap();
        assert!(store.refresh_stored_result(&id).unwrap());
        let _ = fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn two_opens_at_once_recalculate_a_stored_result_once() {
        // ARCH-702: three servers opening the job at once logged the
        // recalculation twice.
        let store = temp_store("refresh-race");
        for _ in 0..3 {
            let (id, _) = old_offer(&store);
            let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
            let done: Vec<_> = (0..4)
                .map(|_| {
                    let (dir, id, barrier) = (store.dir.clone(), id.clone(), barrier.clone());
                    std::thread::spawn(move || {
                        let s = Store::new(dir).unwrap();
                        barrier.wait();
                        s.refresh_result(&id).unwrap()
                    })
                })
                .collect();
            let refreshed = done.into_iter().filter_map(|t| t.join().unwrap()).count();
            assert_eq!(refreshed, 1);
            let engine = store
                .activity(&id)
                .iter()
                .filter(|e| e.actor == ENGINE_ACTOR)
                .count();
            assert_eq!(engine, 1);
        }
        let _ = fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn a_recalculation_calculates_the_roof_once() {
        // Audit 2026-11-22 HUNT-703 / ARCH-703: the save of the fresh
        // result calculated the roof a second time for its origin.
        let store = temp_store("refresh-once");
        // Two buildings that overlap: their join is what the work counts.
        let mut p = examples::find("gable").unwrap().project();
        let mut second = p.parts[0].clone();
        for e in &mut second.edges {
            e.start.x += 3.0;
            e.start.y += 2.0;
        }
        p.parts.push(second);
        let (id, p) = old_offer_of(&store, p, 2.0);
        let (_, once) = crate::work::bounded_by(u64::MAX, || crate::snapshot::take_checked(&p));
        assert!(once > 0);
        let (r, used) = crate::work::bounded_by(u64::MAX, || store.refresh_result(&id));
        assert!(r.unwrap().unwrap().is_some());
        assert!(
            used < once * 3 / 2,
            "{used} steps for {once} per calculation"
        );
        // The fresh result has its origin, checked by this engine.
        let q = store.load(&id).unwrap();
        let fresh = store
            .result_origin(&id, q.result_snapshot.unwrap().taken_ms)
            .expect("origin of the fresh result");
        assert_eq!(fresh.checked, crate::snapshot::ENGINE_VERSION);
        assert_eq!(fresh.input, crate::snapshot::input_digest(&p));
        let _ = fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn a_deleted_and_restored_offer_keeps_its_result_origin() {
        // Audit 2026-11-22 TEST-703: losing the origin on delete or restore
        // passed every test.
        let store = temp_store("refresh-trash");
        let (id, p) = old_offer(&store);
        let taken = p.result_snapshot.as_ref().unwrap().taken_ms;
        let entry = store.delete(&id, USER_ACTOR, "Deleted").unwrap();
        assert!(!store.result_origin_path(&id).unwrap().exists());
        assert!(
            store
                .trash_dir()
                .join(&entry.entry)
                .join("result.json")
                .exists()
        );
        let back = store.restore(&entry.entry).unwrap();
        assert!(store.result_origin(&back, taken).is_some());
        assert!(store.refresh_stored_result(&back).unwrap());
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// Audit 2026-11-22 TEST-711: the summary of a real recalculation, in
    /// `tests/fixtures/parity/refreshed_entry.json`, which
    /// `app/src/lib/live.test.ts` translates; the engine's own version
    /// stands as `to` in it. Write it again with
    /// `TECTORA_WRITE_REFRESHED=1` after changing the summary's wording.
    #[test]
    fn the_recalculation_summary_is_the_one_the_app_translates() {
        let store = temp_store("refresh-fixture");
        let (id, _) = old_offer_off_by(&store, 2.345);
        let summary = store.refresh_result(&id).unwrap().expect("recalculated");
        assert_eq!(store.activity(&id).last().unwrap().summary, summary);
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/parity/refreshed_entry.json");
        let _ = fs::remove_dir_all(&store.dir);
        if std::env::var_os("TECTORA_WRITE_REFRESHED").is_some() {
            let file = serde_json::json!({
                "_comment": "Generated by the core test store::tests::the_recalculation_summary_is_the_one_the_app_translates (TECTORA_WRITE_REFRESHED=1): the change-log summary of a real recalculation of a stored result; app/src/lib/live.test.ts translates it. `to` is the engine version it was written with.",
                "from": "0.0.1",
                "to": crate::snapshot::ENGINE_VERSION,
                "summary": summary,
            });
            fs::write(&path, serde_json::to_string_pretty(&file).unwrap() + "\n").unwrap();
            return;
        }
        let file: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let to = file["to"].as_str().unwrap();
        let ours = summary.replace(
            &format!("-> {})", crate::snapshot::ENGINE_VERSION),
            &format!("-> {to})"),
        );
        assert_eq!(
            ours, file["summary"],
            "rewrite with TECTORA_WRITE_REFRESHED=1"
        );
    }

    /// An offer an older engine stored whose roof area is the same now
    /// and whose first line length was 2 m longer (UX-807).
    fn old_offer_lines_off(store: &Store) -> (String, Project, String) {
        let (id, mut p) = old_offer_off_by(store, 0.0);
        let s = p.result_snapshot.as_mut().unwrap();
        let code = s.lines_by_code.keys().next().expect("a line").clone();
        *s.lines_by_code.get_mut(&code).unwrap() += 2.0;
        fs::write(store.path(&id).unwrap(), p.to_json().unwrap()).unwrap();
        (id, p, code)
    }

    /// Audit 2026-11-22, Design A item 1 (ERR-802, UI-802, ARCH-802): the
    /// job's stored result is "the engine's recalculation" only while it
    /// is the very result the engine's entry wrote, as this library
    /// recorded it.
    #[test]
    fn the_recalculation_is_known_only_while_the_stored_result_is_the_engines() {
        let store = temp_store("recalc-known");
        let (id, p) = old_offer(&store);
        assert_eq!(store.recalculation(&id), None);
        let done = store.refresh_entry(&id).unwrap().expect("recalculated");
        let engines = store.load(&id).unwrap();
        let fresh = engines.result_snapshot.clone().unwrap();
        let offer = p.result_snapshot.clone().unwrap();
        let rec = store.recalculation(&id).expect("the engine's result");
        assert_eq!(rec.entry_ts_ms, done.entry_ts_ms);
        assert_eq!(rec.at, store.activity(&id).last().unwrap().ts_ms);
        assert_eq!(rec.taken_ms, fresh.taken_ms);
        assert_eq!(rec.offer_ms, offer.taken_ms);
        assert_eq!(
            (rec.from.as_str(), rec.to.as_str()),
            ("0.0.1", crate::snapshot::ENGINE_VERSION)
        );
        assert_eq!(rec.differences, crate::snapshot::compare(&offer, &fresh));
        // An edit of the design (the app's unlogged save): still the
        // engine's result, at whatever place its entry has in the log.
        let mut edited = engines.clone();
        edited.parts[0].edges[0].pitch_deg = 45.0;
        store.save(Some(id.clone()), &edited).unwrap();
        store
            .save_as(
                Some(id.clone()),
                &{
                    let mut q = edited.clone();
                    q.notes = "AI".into();
                    q
                },
                "claude",
                "Notes",
            )
            .unwrap();
        assert_eq!(
            store.recalculation(&id).map(|r| r.entry_ts_ms),
            Some(done.entry_ts_ms)
        );
        // ERR-802: the user stores a new result (no log entry): no longer.
        let mut stored = store.load(&id).unwrap();
        let mut own = crate::snapshot::take(&stored);
        own.taken_ms = fresh.taken_ms + 5;
        stored.result_snapshot = Some(own);
        store.save(Some(id.clone()), &stored).unwrap();
        assert_eq!(store.recalculation(&id), None);
        // UI-802: an Undo that writes the engine's result back is visible.
        let mut undone = stored.clone();
        undone.result_snapshot = Some(fresh.clone());
        store.save(Some(id.clone()), &undone).unwrap();
        assert_eq!(
            store.recalculation(&id).map(|r| r.entry_ts_ms),
            Some(done.entry_ts_ms)
        );
        // The result card's history rows of the entry stay either way.
        let items = store.activity_items(&id);
        let engine = items.iter().find(|a| a.actor == ENGINE_ACTOR).unwrap();
        assert_eq!(engine.ts_ms, done.entry_ts_ms);
        assert!(engine.refreshed.is_some());
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// `n` logged saves of `id` by an AI, each changing the notes.
    fn logged_saves(store: &Store, id: &str, n: usize) {
        for i in 0..n {
            let mut q = store.load(id).unwrap();
            q.notes = format!("AI note {i}");
            store
                .save_as(Some(id.to_string()), &q, "claude", "Notes")
                .unwrap();
        }
    }

    /// Audit 2026-11-23 HUNT-902, Design A: the offer's figures do not
    /// depend on the change log. After more logged changes than the log
    /// keeps, the engine's entry is trimmed, but the stored result is
    /// still known as the recalculation and the offer's figures go back.
    #[test]
    fn the_offer_figures_outlive_the_trimmed_change_log() {
        let store = temp_store("recalc-trim");
        let (id, p) = old_offer(&store);
        let offer = p.result_snapshot.clone().unwrap();
        let done = store.refresh_entry(&id).unwrap().expect("recalculated");
        let rec = store.recalculation(&id).expect("recalculated");
        logged_saves(&store, &id, ACTIVITY_KEEP + 10);
        assert!(
            store
                .activity(&id)
                .iter()
                .all(|e| e.ts_ms != done.entry_ts_ms),
            "the engine's entry is trimmed"
        );
        assert_eq!(store.recalculation(&id), Some(rec));
        let back = store
            .restore_offer_result(&id, done.entry_ts_ms, USER_ACTOR, None)
            .unwrap();
        assert_eq!(back.result_snapshot, Some(offer.clone()));
        assert_eq!(store.load(&id).unwrap().result_snapshot, Some(offer));
        assert_eq!(store.recalculation(&id), None);
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// Design A: an origin a round-9 build wrote (no offer figures in it)
    /// still reads from the change log while the entry is there, and the
    /// next logged save copies the figures into it before a trim.
    #[test]
    fn a_recalculation_recorded_without_the_offer_figures_is_upgraded() {
        let store = temp_store("recalc-r9");
        let (id, p) = old_offer(&store);
        let offer = p.result_snapshot.clone().unwrap();
        let done = store.refresh_entry(&id).unwrap().expect("recalculated");
        let rec = store.recalculation(&id).expect("recalculated");
        // As round 9 wrote it.
        let path = store.result_origin_path(&id).unwrap();
        let mut all: Vec<serde_json::Value> =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let mut stripped = 0;
        for o in &mut all {
            if let Some(r) = o.get_mut("refreshed").and_then(|r| r.as_object_mut()) {
                assert!(r.remove("offer").is_some() && r.remove("result").is_some());
                stripped += 1;
            }
        }
        assert_eq!(stripped, 1);
        fs::write(&path, serde_json::to_string(&all).unwrap()).unwrap();
        assert_eq!(store.recalculation(&id), Some(rec.clone()));
        logged_saves(&store, &id, ACTIVITY_KEEP + 10);
        assert_eq!(store.recalculation(&id), Some(rec));
        let back = store
            .restore_offer_result(&id, done.entry_ts_ms, USER_ACTOR, None)
            .unwrap();
        assert_eq!(back.result_snapshot, Some(offer));
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// Audit 2026-11-23 TEST-906: a stored result with the recalculation's
    /// time but other figures, or another engine, is no recalculation
    /// (a hand edit); the history's items say which entry a put-back is.
    #[test]
    fn a_result_with_the_recalculations_time_but_other_figures_is_none() {
        let store = temp_store("recalc-same-time");
        let (id, _) = old_offer(&store);
        let done = store.refresh_entry(&id).unwrap().expect("recalculated");
        let engines = store.load(&id).unwrap();
        assert!(store.recalculation(&id).is_some());
        let mut other = engines.clone();
        other.result_snapshot.as_mut().unwrap().roof_area += 7.0;
        store.save(Some(id.clone()), &other).unwrap();
        assert_eq!(store.recalculation(&id), None);
        let mut other = engines.clone();
        other.result_snapshot.as_mut().unwrap().engine_version = "0.0.2".into();
        store.save(Some(id.clone()), &other).unwrap();
        assert_eq!(store.recalculation(&id), None);
        store.save(Some(id.clone()), &engines).unwrap();
        assert!(store.recalculation(&id).is_some());
        store
            .restore_offer_result(&id, done.entry_ts_ms, USER_ACTOR, None)
            .unwrap();
        assert_eq!(
            store.activity_items(&id)[0].restores,
            Some(done.entry_ts_ms)
        );
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// Audit 2026-11-23 TEST-903: offer figures outside the limits (a
    /// damaged or hand-edited record) are not written back into the job.
    #[test]
    fn restoring_an_offer_result_outside_the_limits_writes_nothing() {
        let store = temp_store("restore-limits");
        let (id, _) = old_offer(&store);
        let done = store.refresh_entry(&id).unwrap().expect("recalculated");
        let path = store.result_origin_path(&id).unwrap();
        let mut all: Vec<serde_json::Value> =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        for o in &mut all {
            if let Some(offer) = o.pointer_mut("/refreshed/offer") {
                offer["roof_area"] = serde_json::json!(2e9);
            }
        }
        fs::write(&path, serde_json::to_string(&all).unwrap()).unwrap();
        let job = fs::read(store.path(&id).unwrap()).unwrap();
        let entries = store.activity(&id).len();
        let e = store
            .restore_offer_result(&id, done.entry_ts_ms, USER_ACTOR, None)
            .unwrap_err();
        assert_eq!(e.code(), "unreadable", "{e}");
        assert_eq!(fs::read(store.path(&id).unwrap()).unwrap(), job);
        assert_eq!(store.activity(&id).len(), entries);
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// ARCH-802: an entry that only says it is the engine's (written into
    /// the log by someone else) has no origin record: no recalculation,
    /// and nothing to put back.
    #[test]
    fn an_engine_entry_this_library_did_not_write_is_no_recalculation() {
        let store = temp_store("recalc-forged");
        let mut p = examples::find("gable").unwrap().project();
        p.job.status = crate::model::JobStatus::Offer;
        p.result_snapshot = Some(crate::snapshot::take(&p));
        let id = store.save(None, &p).unwrap();
        let mut before = p.clone();
        let old = before.result_snapshot.as_mut().unwrap();
        old.engine_version = "0.0.1".into();
        old.roof_area = 300.0;
        old.taken_ms -= 1000;
        let entry = ActivityEntry {
            ts_ms: now_ms(),
            actor: ENGINE_ACTOR.into(),
            summary:
                "Stored result recalculated (Tectora 0.0.1 -> 0.9.0): roof area 300.00 -> 129.24 m2"
                    .into(),
            changes: Vec::new(),
            before: Some(before),
            after: Some(p.clone()),
            undid_from: None,
            restores: None,
            reason: None,
        };
        fs::create_dir_all(store.activity_dir()).unwrap();
        fs::write(
            store.activity_path(&id).unwrap(),
            serde_json::to_string(&entry).unwrap() + "\n",
        )
        .unwrap();
        assert_eq!(store.recalculation(&id), None);
        let e = store
            .restore_offer_result(&id, entry.ts_ms, USER_ACTOR, None)
            .unwrap_err();
        assert_eq!(e.code(), "revert_stale");
        assert_eq!(store.load(&id).unwrap(), p);
        assert_eq!(store.activity(&id).len(), 1);
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// Design A item 3 (ERR-801, UX-802): the offer's result is put back
    /// by the engine's entry, and nothing else changes: an AI save and a
    /// user edit after the recalculation stay, and the AI's review goes on.
    #[test]
    fn restoring_the_offer_result_changes_the_stored_result_only() {
        let store = temp_store("restore-offer");
        let (id, p) = old_offer(&store);
        let offer = p.result_snapshot.clone().unwrap();
        let done = store.refresh_entry(&id).unwrap().expect("recalculated");
        // An AI save after it, then the user's own edit (unlogged).
        let mut q = store.load(&id).unwrap();
        q.notes = "from the AI".into();
        store
            .save_as(Some(id.clone()), &q, "claude", "Notes")
            .unwrap();
        let ai_ts = store.activity(&id).last().unwrap().ts_ms;
        q.parts[0].edges[0].overhang += 0.1;
        store.save(Some(id.clone()), &q).unwrap();
        assert_eq!(store.pending_review(&id).unwrap().count, 1);

        // Another entry's time, or an AI's: refused, nothing written.
        let stale = store
            .restore_offer_result(&id, ai_ts, USER_ACTOR, None)
            .unwrap_err();
        assert_eq!(stale.code(), "revert_stale");
        assert_eq!(store.load(&id).unwrap(), q);

        let back = store
            .restore_offer_result(&id, done.entry_ts_ms, USER_ACTOR, None)
            .unwrap();
        let mut want = q.clone();
        want.result_snapshot = Some(offer.clone());
        assert_eq!(back, want);
        assert_eq!(store.load(&id).unwrap(), want);
        let log = store.activity(&id);
        let entry = log.last().unwrap();
        assert_eq!(entry.actor, USER_ACTOR);
        assert_eq!(entry.restores, Some(done.entry_ts_ms));
        assert_eq!(entry.undid_from, None);
        assert!(
            entry.summary.starts_with(RESTORED_PREFIX),
            "{}",
            entry.summary
        );
        assert!(!entry.summary.starts_with(REVERTED_PREFIX));
        assert_eq!(
            entry
                .changes
                .iter()
                .map(|c| c.field.as_str())
                .collect::<Vec<_>>(),
            ["result_snapshot"]
        );
        assert_eq!(store.recalculation(&id), None);
        // A second press finds nothing to put back.
        let again = store
            .restore_offer_result(&id, done.entry_ts_ms, USER_ACTOR, None)
            .unwrap_err();
        assert_eq!(again.code(), "revert_stale");
        assert_eq!(store.activity(&id).len(), log.len());

        // The AI's change still waits for review, and "revert the latest
        // change" undoes it, not the restore (`pending_start`,
        // `latest_undone`).
        let pending = store.pending_review(&id).expect("still pending");
        assert_eq!((pending.count, pending.since_ts_ms), (1, ai_ts));
        let e = store
            .revert_last_expecting(&id, USER_ACTOR, None, Some(entry.ts_ms))
            .unwrap_err();
        assert_eq!(e.code(), "revert_stale");
        let reverted = store
            .revert_last_expecting(&id, USER_ACTOR, None, Some(ai_ts))
            .unwrap();
        assert_eq!(reverted.notes, p.notes);
        assert_eq!(reverted.result_snapshot, Some(offer.clone()));
        assert_eq!(reverted.parts, q.parts);
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// Pending owner decision, as recommended: the offer's figures put
    /// back stay for good, whatever engine opens the job later; an AI may
    /// put them back (with its reason); and the way back stays open after
    /// the job moved on to ordered.
    #[test]
    fn restored_offer_figures_stay_for_good_also_after_the_job_is_ordered() {
        let store = temp_store("restore-pinned");
        let (id, p) = old_offer(&store);
        let offer = p.result_snapshot.clone().unwrap();
        let done = store.refresh_entry(&id).unwrap().expect("recalculated");
        let mut q = store.load(&id).unwrap();
        q.job.status = crate::model::JobStatus::Ordered;
        store.save(Some(id.clone()), &q).unwrap();
        assert!(store.recalculation(&id).is_some(), "still offered");
        store
            .restore_offer_result(
                &id,
                done.entry_ts_ms,
                "claude",
                Some("The customer signed these"),
            )
            .unwrap();
        let entry = store.activity(&id).last().unwrap().clone();
        assert_eq!(entry.actor, "claude");
        assert!(
            entry
                .summary
                .ends_with("(reason: The customer signed these)"),
            "{}",
            entry.summary
        );
        // As if a later engine opened the offer: the origin says an older
        // engine checked it, and still nothing is recalculated.
        q = store.load(&id).unwrap();
        q.job.status = crate::model::JobStatus::Offer;
        store.save(Some(id.clone()), &q).unwrap();
        let mut origin = store.result_origin(&id, offer.taken_ms).unwrap();
        assert!(origin.pinned);
        origin.checked = "0.0.1".into();
        store.write_result_origin(&id, &origin).unwrap();
        assert_eq!(store.refresh_entry(&id).unwrap(), None);
        assert_eq!(
            store.load(&id).unwrap().result_snapshot,
            Some(offer.clone())
        );

        // The same after the generic revert of the engine's entry.
        let (id, p) = old_offer(&store);
        let offer = p.result_snapshot.clone().unwrap();
        let done = store.refresh_entry(&id).unwrap().expect("recalculated");
        store
            .revert_to(&id, done.entry_ts_ms, USER_ACTOR, None)
            .unwrap();
        let mut origin = store.result_origin(&id, offer.taken_ms).unwrap();
        assert!(origin.pinned);
        origin.checked = "0.0.1".into();
        store.write_result_origin(&id, &origin).unwrap();
        assert_eq!(store.refresh_entry(&id).unwrap(), None);
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// Design A item 4 (ERR-801): "revert the latest change" names the
    /// entry the button was drawn on; when an AI saved meanwhile nothing is
    /// undone.
    #[test]
    fn revert_last_refuses_when_another_change_is_the_latest_by_now() {
        let store = temp_store("revert-expect");
        let p = examples::find("gable").unwrap().project();
        let id = store.save(None, &p).unwrap();
        let mut a = p.clone();
        a.notes = "first".into();
        store
            .save_as(Some(id.clone()), &a, "claude", "First")
            .unwrap();
        let first = store.activity(&id).last().unwrap().ts_ms;
        let mut b = a.clone();
        b.name = "Second".into();
        store
            .save_as(Some(id.clone()), &b, "claude", "Second")
            .unwrap();
        let e = store
            .revert_last_expecting(&id, USER_ACTOR, None, Some(first))
            .unwrap_err();
        assert_eq!(e.code(), "revert_stale");
        assert_eq!(e.id(), Some(id.as_str()));
        assert_eq!(store.load(&id).unwrap(), b);
        assert_eq!(store.activity(&id).len(), 2);
        let second = store.activity(&id).last().unwrap().ts_ms;
        let back = store
            .revert_last_expecting(&id, USER_ACTOR, None, Some(second))
            .unwrap();
        assert_eq!(back, a);
        // The next one to undo is the first by now.
        assert_eq!(
            store
                .revert_last_expecting(&id, USER_ACTOR, None, Some(first))
                .unwrap(),
            p
        );
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// Design A item 6 (UX-806): one rule for "changed by AI". Opening an
    /// offer (the engine's entry) and putting its figures back do not take
    /// the job out of the AI list; the user's own logged change does.
    #[test]
    fn changed_by_ai_steps_over_the_engines_entry_and_a_restore() {
        let store = temp_store("by-ai");
        let (id, _) = old_offer(&store);
        assert!(store.summarize(&id).unwrap().changed_by_ai.is_none());
        let mut q = store.load(&id).unwrap();
        q.notes = "AI".into();
        store
            .save_as(Some(id.clone()), &q, "claude", "Notes")
            .unwrap();
        store.mark_reviewed(&id, u64::MAX / 2).unwrap();
        let by = |s: &Store| s.summarize(&id).unwrap().changed_by_ai.map(|a| a.actor);
        assert_eq!(by(&store).as_deref(), Some("claude"));
        let done = store.refresh_entry(&id).unwrap().expect("recalculated");
        let card = store.summarize(&id).unwrap();
        assert_eq!(card.last_activity.as_ref().unwrap().actor, ENGINE_ACTOR);
        assert_eq!(card.changed_by_ai.as_ref().unwrap().summary, "Notes");
        let listed = |s: &Store| {
            let (all, _) = s.scan().unwrap();
            let q = crate::library::Query {
                by_ai: true,
                ..Default::default()
            };
            crate::library::search(all, &q).unwrap().len()
        };
        assert_eq!(listed(&store), 1);
        store
            .restore_offer_result(&id, done.entry_ts_ms, USER_ACTOR, None)
            .unwrap();
        assert_eq!(by(&store).as_deref(), Some("claude"));
        // An entry received as "the engine's" is stepped over as well.
        let log = store.activity(&id);
        let mut received = log.clone();
        received.last_mut().unwrap().actor = RECEIVED_ENGINE_ACTOR.into();
        assert_eq!(
            changed_by_ai(&received).map(|e| e.actor.as_str()),
            Some("claude")
        );
        // The user's own change is the newest: not changed by AI.
        q = store.load(&id).unwrap();
        q.notes = "mine".into();
        store
            .save_as(Some(id.clone()), &q, USER_ACTOR, "Notes")
            .unwrap();
        assert_eq!(by(&store), None);
        assert_eq!(listed(&store), 0);
        assert!(changed_by_ai(&[]).is_none());
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// `refresh_figures` (moved here from the app shell and the preview
    /// bridge): the offer's date and engine and each figure old -> new for
    /// the engine's entry, nothing for any other entry.
    #[test]
    fn refresh_figures_are_given_for_the_engines_entry_only() {
        let store = temp_store("refresh-figures");
        let (id, p) = old_offer(&store);
        let offer = p.result_snapshot.clone().unwrap();
        store.refresh_entry(&id).unwrap().expect("recalculated");
        let entry = store.activity(&id).last().unwrap().clone();
        let f = refresh_figures(&entry).expect("the engine's entry");
        assert_eq!(f.offer_ms, offer.taken_ms);
        assert_eq!(
            (f.from.as_str(), f.to.as_str()),
            ("0.0.1", crate::snapshot::ENGINE_VERSION)
        );
        assert_eq!(f.differences.len(), 1);
        assert_eq!(f.differences[0].key, "roof_area");
        assert_eq!(f.differences[0].stored, offer.roof_area);
        for actor in [USER_ACTOR, "claude", RECEIVED_ENGINE_ACTOR, "Tectora"] {
            let mut other = entry.clone();
            other.actor = actor.into();
            assert_eq!(refresh_figures(&other), None, "{actor}");
        }
        let mut created = entry.clone();
        created.before = None;
        assert_eq!(refresh_figures(&created), None);
        let json = serde_json::to_value(&f).unwrap();
        assert_eq!(json["offer_ms"], offer.taken_ms);
        assert_eq!(json["differences"][0]["key"], "roof_area");
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// Audit 2026-11-22 ERR-803: opening a job that has nothing to
    /// recalculate (a draft, an offer already checked) never takes the
    /// job's lock, so it does not wait for another writer.
    #[test]
    fn a_job_with_nothing_to_recalculate_opens_without_the_lock() {
        let store = temp_store("refresh-nolock");
        let draft = store
            .save(None, &examples::find("gable").unwrap().project())
            .unwrap();
        let (offer, _) = old_offer(&store);
        assert!(store.refresh_entry(&offer).unwrap().is_some());
        for id in [draft, offer] {
            let held = store.lock(&id).unwrap().expect("a lock");
            let (tx, rx) = std::sync::mpsc::channel();
            let (dir, job) = (store.dir.clone(), id.clone());
            std::thread::spawn(move || {
                let s = Store::new(dir).unwrap();
                // Would wait an hour if it took the lock.
                let r = s.refresh_entry_within(&job, std::time::Duration::from_secs(3600));
                let _ = tx.send(r);
            });
            let r = rx
                .recv_timeout(std::time::Duration::from_secs(30))
                .unwrap_or_else(|_| panic!("{id}: waited for the lock"));
            assert_eq!(r.unwrap(), None);
            drop(held);
        }
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// ERR-803: a due recalculation waits for a held lock only so long,
    /// then the job opens as it is; the next open recalculates it.
    #[test]
    fn a_due_recalculation_gives_up_on_a_held_lock_and_the_next_open_does_it() {
        let store = temp_store("refresh-heldlock");
        let (id, p) = old_offer(&store);
        let log = store.activity(&id).len();
        let held = store.lock(&id).unwrap().expect("a lock");
        let started = std::time::Instant::now();
        let r = store
            .refresh_entry_within(&id, std::time::Duration::from_millis(150))
            .unwrap();
        assert_eq!(r, None);
        assert!(started.elapsed() >= std::time::Duration::from_millis(150));
        assert_eq!(store.load(&id).unwrap().result_snapshot, p.result_snapshot);
        assert_eq!(store.activity(&id).len(), log);
        drop(held);
        assert!(store.refresh_entry(&id).unwrap().is_some());
        // The wait the app opens with is a few seconds, not for ever.
        assert!(REFRESH_LOCK_WAIT <= std::time::Duration::from_secs(5));
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// Audit 2026-11-22 TEST-801: the recalculation on open runs in the
    /// work bound (HUNT-703): a roof over it keeps the offer's figures and
    /// is not tried again.
    #[test]
    fn a_recalculation_over_the_work_bound_keeps_the_offer_figures() {
        let store = temp_store("refresh-bound");
        let mut p = examples::find("gable").unwrap().project();
        let mut second = p.parts[0].clone();
        for e in &mut second.edges {
            e.start.x += 3.0;
            e.start.y += 2.0;
        }
        p.parts.push(second);
        let (id, p) = old_offer_of(&store, p, 2.0);
        let taken = p.result_snapshot.as_ref().unwrap().taken_ms;
        let (_, once) = crate::work::bounded_by(u64::MAX, || crate::snapshot::take_checked(&p));
        assert!(once > 1);
        // The entry point's own bound, held to half of what the roof needs.
        let r = crate::work::with_limit(once / 2, || store.refresh_result(&id));
        assert_eq!(r.unwrap(), None);
        assert_eq!(store.load(&id).unwrap().result_snapshot, p.result_snapshot);
        assert!(store.activity(&id).iter().all(|e| e.actor != ENGINE_ACTOR));
        assert_eq!(
            store.result_origin(&id, taken).unwrap().checked,
            crate::snapshot::ENGINE_VERSION
        );
        assert_eq!(store.refresh_result(&id).unwrap(), None);
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// Audit 2026-11-22 TEST-802: a refresh of a job that is not there
    /// writes nothing, no lock file either.
    #[test]
    fn refreshing_a_job_that_is_not_there_writes_nothing() {
        let store = temp_store("refresh-missing");
        assert_eq!(store.refresh_result("no-such-job").unwrap(), None);
        assert_eq!(store.refresh_entry("no-such-job").unwrap(), None);
        assert!(!store.activity_dir().join("no-such-job.lock").exists());
        assert_eq!(store.recalculation("no-such-job"), None);
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// Audit 2026-11-22 UX-807: a recalculation that leaves the roof area
    /// and changes line lengths names the line codes, not "roof area a ->
    /// a". Its summary and the one of a restore are kept in
    /// `tests/fixtures/parity/refreshed_more_entries.json` for
    /// `app/src/lib/live.test.ts` (write with `TECTORA_WRITE_REFRESHED=1`).
    #[test]
    fn a_recalculation_of_line_lengths_only_names_the_line_codes() {
        let store = temp_store("refresh-lines");
        let (id, _, code) = old_offer_lines_off(&store);
        let done = store.refresh_entry(&id).unwrap().expect("recalculated");
        assert_eq!(
            done.summary,
            format!(
                "Stored result recalculated (Tectora 0.0.1 -> {}): line lengths {code}",
                crate::snapshot::ENGINE_VERSION
            )
        );
        assert!(!done.summary.contains("roof area"));
        store
            .restore_offer_result(&id, done.entry_ts_ms, USER_ACTOR, None)
            .unwrap();
        let restored = store.activity(&id).last().unwrap().summary.clone();
        let _ = fs::remove_dir_all(&store.dir);
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/parity/refreshed_more_entries.json");
        if std::env::var_os("TECTORA_WRITE_REFRESHED").is_some() {
            let file = serde_json::json!({
                "_comment": "Generated by the core test store::tests::a_recalculation_of_line_lengths_only_names_the_line_codes (TECTORA_WRITE_REFRESHED=1): the change-log summaries of a real recalculation that changed line lengths only, and of putting the offer's result back; app/src/lib/live.test.ts translates them. `to` is the engine version they were written with.",
                "from": "0.0.1",
                "to": crate::snapshot::ENGINE_VERSION,
                "codes": code,
                "lines_summary": done.summary,
                "restored_summary": restored,
            });
            fs::write(&path, serde_json::to_string_pretty(&file).unwrap() + "\n").unwrap();
            return;
        }
        let file: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        // The engine's version moves on with every release: compared with
        // the fixture's own `to` in its place.
        let ours = |s: &str| {
            s.replace(
                crate::snapshot::ENGINE_VERSION,
                file["to"].as_str().unwrap(),
            )
        };
        assert_eq!(
            ours(&done.summary),
            file["lines_summary"],
            "rewrite with TECTORA_WRITE_REFRESHED=1"
        );
        assert_eq!(
            ours(&restored),
            file["restored_summary"],
            "rewrite with TECTORA_WRITE_REFRESHED=1"
        );
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
        assert_eq!(StoreError::Busy("x".into()).code(), "busy");
        assert_eq!(StoreError::Busy("x".into()).id(), Some("x"));
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
        store.write(&id, &ai, false).unwrap();
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

    /// Audit 2026-11-23 HUNT-904: the AI's reason of a revert and of a
    /// put-back is its own field on the entry and the history item, also
    /// when the AI's texts themselves contain " (reason: ".
    #[test]
    fn the_reason_of_a_revert_or_a_put_back_is_its_own_field() {
        let store = temp_store("reason-field");
        let a = examples::find("hip").unwrap().project();
        let id = store.save_as(None, &a, USER_ACTOR, "create").unwrap();
        let mut ai = a.clone();
        ai.notes = "x".into();
        store
            .save_as(Some(id.clone()), &ai, "claude", "Note (reason: drawing)")
            .unwrap();
        store
            .revert_last(&id, "claude", Some(" Mistake (reason: typo) fixed "))
            .unwrap();
        let entry = store.activity(&id).last().unwrap().clone();
        assert_eq!(
            entry.reason.as_deref(),
            Some("Mistake (reason: typo) fixed")
        );
        assert_eq!(
            entry.summary,
            format!(
                "{REVERTED_PREFIX}Note (reason: drawing){REVERT_REASON_OPEN}Mistake (reason: typo) fixed)"
            )
        );
        assert_eq!(
            store.activity_items(&id)[0].reason.as_deref(),
            Some("Mistake (reason: typo) fixed")
        );
        // An ordinary save has none.
        let log = store.activity(&id);
        assert!(log[..log.len() - 1].iter().all(|e| e.reason.is_none()));

        let (oid, _) = old_offer(&store);
        let done = store.refresh_entry(&oid).unwrap().expect("recalculated");
        store
            .restore_offer_result(
                &oid,
                done.entry_ts_ms,
                "claude",
                Some("Signed (reason: contract)"),
            )
            .unwrap();
        let item = &store.activity_items(&oid)[0];
        assert_eq!(item.reason.as_deref(), Some("Signed (reason: contract)"));
        assert!(
            item.summary.starts_with(RESTORED_PREFIX),
            "{}",
            item.summary
        );
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
            restores: None,
            reason: None,
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

    /// Saves `next` (made on `base`) the way MCP's `save_onto_current`
    /// does: checked against `base`, merged onto the current file on a
    /// conflict and saved again.
    fn save_like_mcp(store: &Store, id: &str, base: &Project, next: &Project, summary: &str) {
        let (mut base, mut next) = (base.clone(), next.clone());
        for _ in 0..5 {
            match store
                .save_as_checked(id, &next, "claude", summary, Some(&base))
                .unwrap()
            {
                CheckedSave::Saved { .. } => return,
                CheckedSave::Conflict { current } => {
                    let value = |p: &Project| serde_json::to_value(p).unwrap();
                    let merged =
                        crate::merge::merge3(&value(&base), &value(&next), &value(&current))
                            .expect("no overlapping change");
                    next = serde_json::from_value(merged).unwrap();
                    base = *current;
                }
            }
        }
        panic!("the AI save kept conflicting");
    }

    /// A job with the user's original and the AI's change A1 (a note),
    /// with A1's time; A2 adds a customer onto A1.
    fn ai_change_a1(store: &Store) -> (String, Project, Project, u64) {
        let p = examples::find("gable").unwrap().project();
        let id = store.save(None, &p).unwrap();
        let mut a1 = p.clone();
        a1.notes = "A1 note".into();
        store
            .save_as(Some(id.clone()), &a1, "claude", "A1: notes")
            .unwrap();
        let a1_ts = store.activity(&id).last().unwrap().ts_ms;
        let mut a2 = a1.clone();
        a2.job.customer = "A2 customer".into();
        (id, a1, a2, a1_ts)
    }

    /// What holds after the AI's A2 and the user's Revert of A1, in either
    /// order: A2's customer stays and A2 waits for the review. `revert` is
    /// the press's answer.
    fn assert_a2_survives(store: &Store, id: &str, revert: Result<Project, StoreError>) {
        let log = store.activity(id);
        let a2 = log
            .iter()
            .position(|e| e.summary == "A2: customer")
            .expect("A2 logged");
        let now = store.load(id).unwrap();
        assert_eq!(now.job.customer, "A2 customer");
        match revert {
            Err(e) => {
                // A2 came first: the press is stale.
                assert_eq!(e.code(), "revert_stale", "{e}");
                assert_eq!(now.notes, "A1 note");
                assert_eq!(log.len(), 2, "nothing logged by the stale press");
            }
            Ok(_) => {
                // The revert came first: A2 is the latest entry, merged
                // onto the reverted file.
                assert_eq!(
                    a2,
                    log.len() - 1,
                    "{:?}",
                    log.iter().map(|e| &e.summary).collect::<Vec<_>>()
                );
                assert_eq!(now.notes, "");
            }
        }
        let pending = store.pending_review(id).expect("A2 waits for review");
        assert!(pending.since_ts_ms <= log[a2].ts_ms);
    }

    /// Audit 2026-11-23 ERR-1002, fixed order (ARCH-1101): the AI's A2
    /// lands first, so the Revert drawn on A1 is stale and writes nothing.
    #[test]
    fn a_revert_after_the_ai_saved_again_is_stale() {
        let store = temp_store("revert-order-ai-first");
        let (id, a1, a2, a1_ts) = ai_change_a1(&store);
        save_like_mcp(&store, &id, &a1, &a2, "A2: customer");
        let revert = store.revert_last_expecting(&id, USER_ACTOR, None, Some(a1_ts));
        assert!(revert.is_err(), "the press is stale");
        assert_a2_survives(&store, &id, revert);
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// Audit 2026-11-23 ERR-1002, fixed order (ARCH-1101): the Revert
    /// lands first, and the AI's A2, made on A1, is merged onto the
    /// reverted file: the note stays cleared and A2 waits for the review.
    #[test]
    fn an_ai_save_after_the_revert_is_merged_onto_it() {
        let store = temp_store("revert-order-revert-first");
        let (id, a1, a2, a1_ts) = ai_change_a1(&store);
        let revert = store.revert_last_expecting(&id, USER_ACTOR, None, Some(a1_ts));
        assert!(revert.is_ok(), "{:?}", revert.as_ref().err());
        save_like_mcp(&store, &id, &a1, &a2, "A2: customer");
        assert_a2_survives(&store, &id, revert);
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// Audit 2026-11-23 ERR-1002: the user's Revert of the AI's change A1
    /// waits behind another program holding the job's lock, and so does
    /// the AI's next save A2. However the two are let in, A2 is never
    /// undone under it nor skips the review: either A2 lands first and the
    /// press is stale, or the revert lands first and A2 comes after it.
    /// The order is up to the threads: a smoke test; the two orders are
    /// pinned by the two tests above. The AI saves as MCP does (checked,
    /// merged on a conflict), not blindly (audit 2026-11-23 ARCH-1101).
    #[test]
    fn a_revert_waiting_behind_an_ai_save_never_lets_the_ai_change_skip_the_review() {
        for run in 0..3 {
            let store = temp_store(&format!("revert-race-{run}"));
            let (id, a1, a2, a1_ts) = ai_change_a1(&store);
            let held = store.lock(&id).unwrap().expect("a lock");
            let revert = std::thread::scope(|scope| {
                let ai = scope.spawn(|| save_like_mcp(&store, &id, &a1, &a2, "A2: customer"));
                std::thread::sleep(std::time::Duration::from_millis(150));
                let revert =
                    scope.spawn(|| store.revert_last_expecting(&id, USER_ACTOR, None, Some(a1_ts)));
                std::thread::sleep(std::time::Duration::from_millis(150));
                drop(held);
                ai.join().unwrap();
                revert.join().unwrap()
            });
            assert_a2_survives(&store, &id, revert);
            let _ = fs::remove_dir_all(&store.dir);
        }
    }

    /// Audit 2026-11-23 ERR-1002 / ARCH-1001 (design A1): Revert, Revert
    /// all, putting the offer's figures back and Accept wait for a held
    /// lock only briefly, then answer `busy` with nothing written; once the
    /// lock is free each works.
    #[test]
    fn short_actions_answer_busy_while_another_program_holds_the_lock() {
        let wait = std::time::Duration::from_millis(150);
        let store = temp_store("busy").with_action_wait(wait);
        let (id, _) = old_offer(&store);
        let done = store.refresh_entry(&id).unwrap().expect("recalculated");
        let mut q = store.load(&id).unwrap();
        q.notes = "AI".into();
        store
            .save_as(Some(id.clone()), &q, "claude", "Notes")
            .unwrap();
        let last = store.activity(&id).last().unwrap().ts_ms;
        let file = fs::read(store.path(&id).unwrap()).unwrap();
        let log = store.activity(&id).len();
        let reviewed = store.reviewed_at(&id);
        let held = store.lock(&id).unwrap().expect("a lock");
        let busy = |r: Result<(), StoreError>| {
            let e = r.unwrap_err();
            assert_eq!(e.code(), "busy", "{e}");
            assert_eq!(e.id(), Some(id.as_str()));
        };
        let started = std::time::Instant::now();
        busy(
            store
                .revert_last_expecting(&id, USER_ACTOR, None, Some(last))
                .map(|_| ()),
        );
        busy(store.revert_last(&id, "claude", Some("Undo")).map(|_| ()));
        busy(
            store
                .revert_to_counted(&id, last, USER_ACTOR, None)
                .map(|_| ()),
        );
        busy(
            store
                .restore_offer_result(&id, done.entry_ts_ms, USER_ACTOR, None)
                .map(|_| ()),
        );
        busy(store.mark_reviewed(&id, last).map(|_| ()));
        let took = started.elapsed();
        assert!(took >= wait * 5, "{took:?}");
        assert!(took < std::time::Duration::from_secs(5), "{took:?}");
        assert_eq!(fs::read(store.path(&id).unwrap()).unwrap(), file);
        assert_eq!(store.activity(&id).len(), log);
        assert_eq!(store.reviewed_at(&id), reviewed);
        drop(held);
        assert_eq!(store.mark_reviewed(&id, last).unwrap(), last);
        store
            .restore_offer_result(&id, done.entry_ts_ms, USER_ACTOR, None)
            .unwrap();
        store
            .revert_last_expecting(&id, USER_ACTOR, None, Some(last))
            .unwrap();
        // The wait is the one opening a job has, a few seconds.
        assert_eq!(Store::at(store.dir.clone()).action_wait, REFRESH_LOCK_WAIT);
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// Audit 2026-11-23 ERR-1002: Revert all says how many recorded changes
    /// it undid, also the ones that came in after the press was drawn.
    #[test]
    fn revert_all_counts_the_changes_it_undid() {
        let store = temp_store("revert-count");
        let p = examples::find("gable").unwrap().project();
        let id = store.save(None, &p).unwrap();
        logged_saves(&store, &id, 3);
        let first = store.activity(&id)[0].ts_ms;
        let r = store
            .revert_to_counted(&id, first, USER_ACTOR, None)
            .unwrap();
        assert_eq!(r.undone, 3);
        assert_eq!(r.project, p);
        assert_eq!(store.load(&id).unwrap(), p);
        // Nothing there: refused without a lock file.
        let e = store
            .revert_to_counted("nope", 1, USER_ACTOR, None)
            .unwrap_err();
        assert_eq!(e.code(), "no_such_change");
        let e = store.revert_last("nope", USER_ACTOR, None).unwrap_err();
        assert_eq!(e.code(), "nothing_to_revert");
        assert!(!store.activity_dir().join("nope.lock").exists());
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// Audit 2026-11-23 HUNT-1002, ERR-1003: an AI's reason stays at the
    /// end of the history line however long the reverted change's summary
    /// or the reason is; the line keeps the summary cap.
    #[test]
    fn a_long_reason_always_ends_the_revert_and_put_back_line() {
        use crate::edit::MAX_SUMMARY_CHARS;
        let ends_with_reason = |e: &ActivityEntry| {
            let r = e.reason.as_deref().expect("a reason");
            assert!(
                e.summary.ends_with(&format!("{REVERT_REASON_OPEN}{r})")),
                "{}",
                e.summary
            );
            assert!(e.summary.chars().count() <= MAX_SUMMARY_CHARS);
            r.to_string()
        };
        let store = temp_store("long-reason");
        let p = examples::find("gable").unwrap().project();
        let id = store.save(None, &p).unwrap();
        let mut q = p.clone();
        q.notes = "x".into();
        let long = format!("Note. {}", "x".repeat(480));
        store
            .save_as(Some(id.clone()), &q, "claude", &long)
            .unwrap();
        // A short reason after a long summary: the summary is cut.
        store
            .revert_last(&id, "claude", Some("Customer asked"))
            .unwrap();
        let e = store.activity(&id).last().unwrap().clone();
        assert_eq!(ends_with_reason(&e), "Customer asked");
        assert!(e.summary.starts_with(REVERTED_PREFIX));
        // A long reason after a long summary: both cut, the reason last.
        store
            .save_as(Some(id.clone()), &q, "claude", &long)
            .unwrap();
        store
            .revert_last(&id, "claude", Some(&"y".repeat(450)))
            .unwrap();
        let e = store.activity(&id).last().unwrap().clone();
        let r = ends_with_reason(&e);
        assert!(r.starts_with("yyyy") && r.chars().count() >= 300, "{r}");
        assert!(e.summary.chars().count() - r.chars().count() >= 120);
        // The put-back of the offer's figures.
        let (id, _) = old_offer(&store);
        let done = store.refresh_entry(&id).unwrap().expect("recalculated");
        let reason = format!("The customer signed these figures. {}", "y".repeat(450));
        store
            .restore_offer_result(&id, done.entry_ts_ms, "claude", Some(&reason))
            .unwrap();
        let e = store.activity(&id).last().unwrap().clone();
        let r = ends_with_reason(&e);
        assert!(r.starts_with("The customer signed"), "{r}");
        assert!(e.summary.starts_with(RESTORED_PREFIX), "{}", e.summary);
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// Audit 2026-11-23 ARCH-1002: asking for the recalculation reads the
    /// change log only for an origin record without the offer's figures,
    /// not for a draft, an offer whose result was never recalculated, or
    /// a record that has them.
    #[test]
    fn the_recalculation_reads_the_change_log_only_when_it_needs_it() {
        let store = temp_store("recalc-noread");
        let p = examples::find("gable").unwrap().project();
        let draft = store.save(None, &p).unwrap();
        logged_saves(&store, &draft, 3);
        let reads = || LOG_READS.with(|n| n.get());
        let before = reads();
        assert_eq!(store.recalculation(&draft), None);
        assert_eq!(reads(), before, "a draft without a stored result");
        let (offer, _) = old_offer(&store);
        let before = reads();
        assert_eq!(store.recalculation(&offer), None);
        assert_eq!(reads(), before, "an offer not recalculated");
        store.refresh_entry(&offer).unwrap().expect("recalculated");
        let before = reads();
        assert!(store.recalculation(&offer).is_some());
        assert_eq!(reads(), before, "a record with the offer's figures");
        // A record of a round-9 build: the figures from the log.
        let mut all = store.result_origins(&offer);
        for o in &mut all {
            if let Some(r) = o.refreshed.as_mut() {
                r.offer = None;
                r.result = None;
            }
        }
        store.write_result_origins(&offer, &all).unwrap();
        let before = reads();
        assert!(store.recalculation(&offer).is_some());
        assert_eq!(reads(), before + 1);
        let _ = fs::remove_dir_all(&store.dir);
    }

    /// Audit 2026-11-23 ARCH-1003: an origin record whose offer figures
    /// break the stored-result bounds (a hand-edited `.result.json`) is no
    /// recalculation: nothing of it reaches the card or MCP, and a
    /// put-back asked for anyway writes nothing, an impossible date of
    /// the offer's result too (audit 2026-11-23 HUNT-1102, TEST-1103).
    #[test]
    fn a_damaged_origin_record_is_no_recalculation() {
        let store = temp_store("recalc-hostile");
        let (id, _) = old_offer(&store);
        let done = store.refresh_entry(&id).unwrap().expect("recalculated");
        assert!(store.recalculation(&id).is_some());
        let good = store.result_origins(&id);
        type Snap = crate::snapshot::ResultSnapshot;
        let damaged = |f: fn(&mut Snap)| {
            let mut all = good.clone();
            for o in &mut all {
                if let Some(r) = o.refreshed.as_mut() {
                    let offer = r.offer.as_mut().unwrap();
                    f(offer);
                    // The record stays consistent, so each damage reaches
                    // the bounds check, not the lookup (TEST-1103).
                    r.offer_taken_ms = offer.taken_ms;
                }
            }
            store.write_result_origins(&id, &all).unwrap();
        };
        let checks: [fn(&mut Snap); 4] = [
            |o| o.engine_version = format!("0.0.1 \u{202e}RLO\u{7}{}", "x".repeat(600)),
            |o| {
                o.lines_by_code.insert("K".repeat(200), 1.0);
            },
            |o| o.roof_area = f64::INFINITY,
            |o| o.taken_ms = crate::snapshot::MAX_TAKEN_MS + 1,
        ];
        let job = fs::read(store.path(&id).unwrap()).unwrap();
        let entries = store.activity(&id).len();
        for f in checks {
            damaged(f);
            assert_eq!(store.recalculation(&id), None);
            let e = store
                .restore_offer_result(&id, done.entry_ts_ms, USER_ACTOR, None)
                .unwrap_err();
            assert!(["revert_stale", "unreadable"].contains(&e.code()), "{e}");
            assert_eq!(fs::read(store.path(&id).unwrap()).unwrap(), job);
            assert_eq!(store.activity(&id).len(), entries);
        }
        store.write_result_origins(&id, &good).unwrap();
        assert!(store.recalculation(&id).is_some());
        let _ = fs::remove_dir_all(&store.dir);
    }
}
