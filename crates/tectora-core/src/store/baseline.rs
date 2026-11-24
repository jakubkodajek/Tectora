//! The review's baseline: the job as the user last settled it, kept beside
//! the job (`.activity/<id>.baseline.json`) from an AI's first write until
//! the user's Accept (ADR 0007, audit 2026-11-24 round 20, design Y).
//!
//! A review is open exactly while a baseline exists and the job differs
//! from it (or the AI's creation of the job still waits); Revert all writes
//! the baseline. What the AI's steps were is read from the change log for
//! display only, so a trimmed log, a revert naming a revert or a take-back
//! no longer decide what was accepted (ARCH-1901, UX-1901, HUNT-1901).
//!
//! The rules, applied in the two write funnels (`log_and_write` for every
//! logged write, `save` / `save_checked` for the editor's own saves):
//!
//! | | event | baseline |
//! | --- | --- | --- |
//! | R1 | an AI's logged write, no baseline | the job before it (`created` for an AI's new job), `since_ts_ms` its entry |
//! | R2 | any later AI write | unchanged |
//! | R3 | the editor's unlogged save, the user's put-back of the offer's figures | the user's edit is applied to the baseline too (3-way merge); a value the AI had changed keeps the baseline's value and is remembered (`conflicts`: Revert all refuses) |
//! | R4 | the user's own other logged change, the engine's recalculation | settled: no baseline |
//! | R5 | the user's revert from an entry at or after `since_ts_ms` | unchanged |
//! | R6 | the user's revert from before `since_ts_ms` | the job it writes is the baseline; the old one is kept for the take-back (`replaced`) |
//! | R7 | the take-back of that revert | the old baseline comes back |
//! | R8 | no baseline, the user's revert puts back an AI change that did not stand at the last Accept | the job before that revert |
//! | R9 | Accept up to `ts` | settled; with an AI entry newer than `ts`: the job without the AI's entries from that one on |
//!
//! The file is untrusted input like every library file: read only as a
//! regular file (never through a link) of bounded size, and a damaged or
//! unknown one counts as missing. A missing file is a job of an older
//! Tectora: its review is derived once from the change log (the round-19
//! rule, [`review_of`]) and written at the next write. Entries another
//! program logged since the file was written are caught up on read (R1,
//! R4); an older Tectora's unlogged saves are not, and the job's digest in
//! the file makes Revert all refuse then (`revert_outside`).

use super::*;

/// The format of the baseline file.
const BASELINE_FORMAT: u32 = 1;

/// Largest baseline file read: the job and the one it replaced (R6).
const MAX_BASELINE_FILE: u64 = 2 * MAX_JOB_FILE + MAX_SMALL_FILE;

/// Most places a baseline remembers where the user edited what the AI set
/// (R3), and the longest one: a hostile file cannot make Revert all's
/// refusal huge.
pub(super) const MAX_CONFLICTS: usize = 200;
const MAX_CONFLICT_CHARS: usize = 300;

/// The job as the user last settled it ([`Store::review_baseline`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Baseline {
    /// The AI's entry the baseline was taken before: where Revert all is
    /// logged from. The log may no longer hold it.
    pub since_ts_ms: u64,
    /// The settled job, with the user's own later edits applied (R3).
    pub project: Project,
    /// An AI created the job: `project` is the job as created, and the
    /// creation itself waits until Accept.
    #[serde(default)]
    pub created: bool,
    /// Places the user edited that the AI had changed (R3): Revert all
    /// refuses and names them.
    #[serde(default)]
    pub conflicts: Vec<String>,
    /// The baseline a user's revert from before `since_ts_ms` replaced
    /// (R6), for its take-back (R7).
    #[serde(default)]
    pub replaced: Option<Replaced>,
    /// Digest of the job file as Tectora last wrote it while this baseline
    /// stood ([`job_digest`]): another program's unlogged save (an older
    /// Tectora on the same folder) changes the file without applying R3,
    /// and Revert all refuses then. `None` for a baseline derived from an
    /// older Tectora's log.
    #[serde(default)]
    pub job_digest: Option<String>,
}

/// The baseline a user's revert from before the review replaced (R6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Replaced {
    /// The time of that revert's entry.
    pub revert_ts_ms: u64,
    pub project: Box<Project>,
}

/// The file `.activity/<id>.baseline.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct BaselineFile {
    format: u32,
    /// The newest change-log entry when the file was written: later
    /// entries another program logged are caught up on read.
    #[serde(default)]
    log_ts_ms: u64,
    /// `None`: settled, no review.
    #[serde(default)]
    baseline: Option<Baseline>,
}

/// FNV-1a of the job file's bytes as Tectora writes them.
pub(super) fn job_digest(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}-{}", bytes.len())
}

/// [`job_digest`] of `project` as [`Store::write`] writes it.
fn digest_of(project: &Project) -> Option<String> {
    project.to_json().ok().map(|j| job_digest(j.as_bytes()))
}

fn is_ai(e: &ActivityEntry) -> bool {
    !RESERVED_ACTORS.contains(&e.actor.as_str())
}

/// A logged entry that settles the job (R4): the user's own change (also
/// the revert of a Tectora that did not record `undid_from`) or the
/// engine's recalculation. Received entries never settle.
fn settles(e: &ActivityEntry) -> bool {
    (e.actor == USER_ACTOR && e.undid_from.is_none() && e.restores.is_none())
        || e.actor == ENGINE_ACTOR
}

/// The baseline taken before the AI's entry `e` (R1); `None` when `e`
/// holds no job.
fn before_entry(e: &ActivityEntry) -> Option<Baseline> {
    let (project, created) = match (&e.before, &e.after) {
        (Some(b), _) => (b.clone(), false),
        (None, Some(a)) => (a.clone(), true),
        (None, None) => return None,
    };
    Some(Baseline {
        since_ts_ms: e.ts_ms,
        project,
        created,
        conflicts: Vec::new(),
        replaced: None,
        job_digest: None,
    })
}

/// The index of the entry `since` names in `log`, or of the first one
/// logged after it when the log no longer has it.
pub(super) fn since_index(log: &[ActivityEntry], since: u64) -> usize {
    log.iter()
        .rposition(|e| e.ts_ms == since)
        .or_else(|| log.iter().position(|e| e.ts_ms >= since))
        .unwrap_or(log.len())
}

/// The AI steps of `log` the review of `b` lists, oldest first (display
/// only): the AI's changes from `since_ts_ms` on that stand, and the AI's
/// reverts that stand and reach back before it (they took settled work
/// out or put it back, HUNT-1901). When none stands but the job differs,
/// the newest AI entry from `since_ts_ms` on, else the newest AI entry.
/// Linear in the log.
pub(super) fn steps_of(log: &[ActivityEntry], b: &Baseline) -> Vec<usize> {
    let n = log.len();
    let pos = since_index(log, b.since_ts_ms);
    let ranges = revert_ranges(log);
    let by = undone_by(log, &ranges);
    let mut steps: Vec<usize> = (pos..n)
        .filter(|&i| {
            work(1);
            is_ai(&log[i])
                && by[i].is_none()
                && match ranges[i] {
                    None => true,
                    Some((u, _)) => u < pos,
                }
        })
        .collect();
    if steps.is_empty() {
        steps.extend((pos..n).rev().find(|&i| is_ai(&log[i])));
    }
    if steps.is_empty() {
        steps.extend((0..n).rev().find(|&i| is_ai(&log[i])));
    }
    steps
}

/// `ours` with the edit from `old` to `new` applied ([`crate::merge`]):
/// where the edit touches a value `ours` changed too, `ours` keeps its
/// value and the place is named. `None` when a side cannot be merged.
fn rebased(old: &Project, ours: &Project, new: &Project) -> Option<(Project, Vec<String>)> {
    let v = |p: &Project| serde_json::to_value(p).ok();
    let (vo, vb, vn) = (v(old)?, v(ours)?, v(new)?);
    let (merged, conflicts) = crate::merge::merge3_keep_ours(&vo, &vb, &vn);
    let p: Project = serde_json::from_value(merged).ok()?;
    Some((p, conflicts))
}

impl Baseline {
    /// R3: the user's edit from `old` to `new` applied to the baseline,
    /// and to the one a take-back would bring back.
    fn rebase(&mut self, old: &Project, new: &Project) {
        if old == new {
            return;
        }
        if let Some((p, conflicts)) = rebased(old, &self.project, new) {
            self.project = p;
            for c in conflicts {
                if self.conflicts.len() < MAX_CONFLICTS && !self.conflicts.contains(&c) {
                    self.conflicts.push(c);
                }
            }
        }
        if let Some(r) = self.replaced.as_mut()
            && let Some((p, _)) = rebased(old, &r.project, new)
        {
            *r.project = p;
        }
    }

    /// Whether the file's shape is one Tectora writes.
    fn plausible(&self) -> bool {
        self.conflicts.len() <= MAX_CONFLICTS
            && self
                .conflicts
                .iter()
                .all(|c| c.chars().count() <= MAX_CONFLICT_CHARS)
    }
}

/// What a baseline file says about a job before the entries `log` holds
/// are caught up ([`Store::review_baseline`]).
enum Stored {
    /// No file (a job of an older Tectora) or a damaged one.
    Missing,
    Settled {
        log_ts_ms: u64,
    },
    Open {
        log_ts_ms: u64,
        baseline: Box<Baseline>,
    },
}

impl Store {
    pub(super) fn baseline_path(&self, id: &str) -> Result<PathBuf, StoreError> {
        self.path(id)?;
        Ok(self.activity_dir().join(format!("{id}.baseline.json")))
    }

    /// The baseline file of `id` as it is: a link, a folder, a pipe, a file
    /// over [`MAX_BASELINE_FILE`], one that does not parse, one of a newer
    /// format or with an implausible shape is [`Stored::Missing`].
    fn stored_baseline(&self, id: &str) -> Stored {
        let Ok(path) = self.baseline_path(id) else {
            return Stored::Missing;
        };
        if !is_own_folder(&self.activity_dir())
            || !fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_file())
        {
            return Stored::Missing;
        }
        let Some(f) = read_regular(&path, MAX_BASELINE_FILE)
            .ok()
            .and_then(|t| serde_json::from_slice::<BaselineFile>(&t).ok())
        else {
            return Stored::Missing;
        };
        if f.format != BASELINE_FORMAT {
            return Stored::Missing;
        }
        match f.baseline {
            None => Stored::Settled {
                log_ts_ms: f.log_ts_ms,
            },
            Some(b) if b.plausible() => Stored::Open {
                log_ts_ms: f.log_ts_ms,
                baseline: Box::new(b),
            },
            Some(_) => Stored::Missing,
        }
    }

    /// Writes the baseline state of `id` (`None`: settled) after the
    /// change-log entry `log_ts_ms`. Best effort: a state that could not
    /// be written is caught up from the log on the next read where the
    /// log tells (R1, R4).
    fn write_baseline(&self, id: &str, log_ts_ms: u64, baseline: Option<&Baseline>) {
        let Ok(path) = self.baseline_path(id) else {
            return;
        };
        let file = BaselineFile {
            format: BASELINE_FORMAT,
            log_ts_ms,
            baseline: baseline.cloned(),
        };
        if self.dir.is_dir()
            && own_folder(&self.activity_dir()).is_ok()
            && missing_or_regular(&path).is_ok()
            && let Ok(text) = serde_json::to_string(&file)
        {
            let _ = write_atomic(&path, &text);
        }
    }

    /// The raw baseline file, to put it back when the job write it went
    /// with failed ([`Store::restore_baseline_file`]).
    fn baseline_file_bytes(&self, id: &str) -> Option<Vec<u8>> {
        let path = self.baseline_path(id).ok()?;
        fs::symlink_metadata(&path)
            .ok()
            .filter(|m| m.file_type().is_file())?;
        read_regular(&path, MAX_BASELINE_FILE).ok()
    }

    pub(super) fn restore_baseline_file(&self, id: &str, bytes: Option<Vec<u8>>) {
        let Ok(path) = self.baseline_path(id) else {
            return;
        };
        match bytes {
            Some(b) if missing_or_regular(&path).is_ok() => {
                let _ = write_atomic_bytes(&path, &b);
            }
            Some(_) => {}
            None => {
                if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_file()) {
                    let _ = fs::remove_file(&path);
                }
            }
        }
    }

    /// The baseline of `id` given its change log `log` and the job `now`
    /// (`None` when the file cannot be read): the file, with entries
    /// another program logged since caught up (R4 when one settles the
    /// job, then R1 for the AI's next entry); for a job without a file
    /// (an older Tectora's), derived once from the log. `None`: settled.
    pub(super) fn review_baseline(
        &self,
        id: &str,
        log: &[ActivityEntry],
        now: Option<&Project>,
    ) -> Option<Baseline> {
        let (b, newer_than) = match self.stored_baseline(id) {
            Stored::Missing => return self.derived_baseline(id, log, now),
            Stored::Settled { log_ts_ms } => (None, log_ts_ms),
            Stored::Open {
                log_ts_ms,
                baseline,
            } => (Some(*baseline), log_ts_ms),
        };
        // An entry another program logged after the file was written (or
        // the user's revert of a Tectora that did not record `undid_from`,
        // which no Tectora with baselines writes) that settles the job (R4):
        // no Tectora with baselines logs one while a baseline stands.
        let foreign = |e: &ActivityEntry| e.ts_ms > newer_than || e.legacy_user_revert() > 0;
        let from = b.as_ref().map_or(0, |b| b.since_ts_ms);
        let settled_at = log
            .iter()
            .rposition(|e| e.ts_ms >= from && foreign(e) && settles(e));
        match (b, settled_at) {
            (Some(b), None) => Some(b),
            // Then R1 for the AI's next entry after it, or after the file.
            (_, at) => {
                let start = at.map_or(0, |k| k + 1);
                let k = (start..log.len()).find(|&k| foreign(&log[k]) && is_ai(&log[k]))?;
                self.caught_up(log, k, now)
            }
        }
    }

    /// R1 for the AI's entry `log[k]` that no baseline saw: the job `now`
    /// without the entries from it on (keeping edits saved without a log
    /// entry), else the job before it.
    fn caught_up(
        &self,
        log: &[ActivityEntry],
        k: usize,
        now: Option<&Project>,
    ) -> Option<Baseline> {
        let mut b = before_entry(&log[k])?;
        if !b.created
            && let Some(now) = now
            && let Ok(p) = undo_onto("", log, k, Some(now.clone()))
        {
            b.project = p;
        }
        Some(b)
    }

    /// The baseline of a job an older Tectora reviewed by its log: the
    /// round-19 rule ([`review_of`]) says whether a review is open and
    /// where it starts; the baseline is what its Revert all wrote (the job
    /// without the entries from the start on, keeping the unlogged edits).
    fn derived_baseline(
        &self,
        id: &str,
        log: &[ActivityEntry],
        now: Option<&Project>,
    ) -> Option<Baseline> {
        let review = review_of(log, self.reviewed_at(id));
        if !review.open() {
            return None;
        }
        let first = &log[review.start];
        let mut b = before_entry(first)?;
        let from = if b.created {
            review.start + 1
        } else {
            review.start
        };
        if from < log.len()
            && let Some(now) = now
            && let Ok(p) = undo_onto(id, log, from, Some(now.clone()))
        {
            b.project = p;
        }
        Some(b)
    }

    /// The open review of `id`: its baseline, the job (`None` when the
    /// file cannot be read) and the log. `None` when nothing waits.
    pub(super) fn open_review(
        &self,
        id: &str,
    ) -> Option<(Baseline, Option<Project>, Vec<ActivityEntry>)> {
        let log = self.activity(id);
        let now = self.load(id).ok();
        let b = self.review_baseline(id, &log, now.as_ref())?;
        is_open(&b, now.as_ref()).then_some((b, now, log))
    }

    /// The baseline state before a logged write of `id` whose log was read
    /// as `log` and whose job was `before`, and the raw file to put back
    /// if the job write fails.
    pub(super) fn baseline_before_write(
        &self,
        id: &str,
        log: &[ActivityEntry],
        before: Option<&Project>,
    ) -> (Option<Baseline>, Option<Vec<u8>>) {
        let raw = self.baseline_file_bytes(id);
        (self.review_baseline(id, log, before), raw)
    }

    /// Applies the rules to the logged write of `after` over `before` by
    /// `actor` at `ts_ms` (R1 to R7) given the state `prior`, and writes
    /// the new state. Called before the job is written.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn baseline_on_logged(
        &self,
        id: &str,
        prior: Option<Baseline>,
        before: Option<&Project>,
        after: &Project,
        actor: &str,
        undid_from: Option<u64>,
        restores: Option<u64>,
        ts_ms: u64,
    ) {
        let digest = || digest_of(after);
        let ai = !RESERVED_ACTORS.contains(&actor);
        let next = match prior {
            // R1: the AI's first write since the job was settled.
            None if ai => before_entry(&ActivityEntry {
                ts_ms,
                actor: actor.to_string(),
                summary: String::new(),
                before: before.cloned(),
                changes: Vec::new(),
                after: Some(after.clone()),
                undid_from,
                restores,
                reason: None,
            }),
            None => None,
            Some(mut b) => {
                if ai || (actor != USER_ACTOR && actor != ENGINE_ACTOR) {
                    // R2 (also a received entry, which never settles).
                } else if actor == ENGINE_ACTOR {
                    // R4: the engine's recalculation settles.
                    self.write_baseline(id, ts_ms, None);
                    return;
                } else {
                    match undid_from {
                        // R3: the user's put-back of the offer's figures.
                        None if restores.is_some() => {
                            if let Some(old) = before {
                                b.rebase(old, after);
                            }
                        }
                        // R4: the user's own change settles.
                        None => {
                            self.write_baseline(id, ts_ms, None);
                            return;
                        }
                        // R7: the take-back of the revert that replaced it.
                        Some(from)
                            if b.replaced.as_ref().is_some_and(|r| r.revert_ts_ms == from) =>
                        {
                            let r = b.replaced.take().expect("checked");
                            b.project = *r.project;
                            b.conflicts.clear();
                        }
                        // R6: the user reverted work settled before the
                        // review: the job they wrote is the baseline.
                        Some(from) if from < b.since_ts_ms => {
                            let old = std::mem::replace(&mut b.project, after.clone());
                            b.replaced = Some(Replaced {
                                revert_ts_ms: ts_ms,
                                project: Box::new(old),
                            });
                            b.conflicts.clear();
                        }
                        // R5: a revert inside the review moves the job only.
                        Some(_) => {}
                    }
                }
                Some(b)
            }
        };
        let next = next.map(|mut b| {
            b.job_digest = digest();
            b
        });
        self.write_baseline(id, ts_ms, next.as_ref());
    }

    /// R3 for the editor's own save of `new` over `old` (the file under
    /// the lock, read here when not given): applied to an open baseline.
    /// Only the baseline file is read when no review is open. Returns the
    /// raw file to put back if the job write fails.
    pub(super) fn baseline_on_unlogged(
        &self,
        id: &str,
        old: Option<&Project>,
        new: &Project,
    ) -> Option<Option<Vec<u8>>> {
        let Stored::Open {
            log_ts_ms,
            baseline: mut b,
        } = self.stored_baseline(id)
        else {
            return None;
        };
        let raw = self.baseline_file_bytes(id);
        let read;
        let old = match old {
            Some(o) => Some(o),
            None => {
                read = self.load(id).ok();
                read.as_ref()
            }
        };
        if let Some(old) = old {
            b.rebase(old, new);
        }
        b.job_digest = digest_of(new);
        self.write_baseline(id, log_ts_ms, Some(&b));
        Some(raw)
    }

    /// R9: the user accepted every entry up to `accepted_ms`. The caller
    /// holds the job's lock.
    pub(super) fn baseline_on_accept(&self, id: &str, accepted_ms: u64) {
        let log = self.activity(id);
        let now = self.load(id).ok();
        let newest = log.last().map_or(0, |e| e.ts_ms);
        let Some(prior) = self.review_baseline(id, &log, now.as_ref()) else {
            // Settled already (written, so an older Tectora's log is
            // never read for it again).
            if !matches!(self.stored_baseline(id), Stored::Settled { .. }) {
                self.write_baseline(id, newest, None);
            }
            return;
        };
        let later = log
            .iter()
            .position(|e| e.ts_ms > accepted_ms && is_ai(e) && e.ts_ms >= prior.since_ts_ms);
        let next = later.and_then(|k| {
            let mut b = before_entry(&log[k])?;
            if prior.created && b.created {
                return Some(prior.clone());
            }
            if let Some(now) = now.as_ref()
                && let Ok(p) = undo_onto(id, &log, k, Some(now.clone()))
            {
                b.project = p;
            }
            b.job_digest = prior.job_digest.clone();
            Some(b)
        });
        self.write_baseline(id, newest, next.as_ref());
    }

    /// The baseline of an open review of `id` (ADR 0007), `None` when no
    /// review is open.
    pub fn review_baseline_of(&self, id: &str) -> Option<Baseline> {
        self.open_review(id).map(|(b, _, _)| b)
    }

    /// The user's Revert all of the review the banner showed, the one that
    /// starts at `since_ts_ms` ([`PendingReview::since_ts_ms`]): writes the
    /// baseline, logged as `actor`'s revert; nothing is written (and nothing
    /// counted) when the job is its baseline already (the AI reverted its
    /// steps meanwhile). When the job has no baseline with that start any
    /// more (the user accepted the review meanwhile, or the AI's changes
    /// are another review) nothing is written and the error is
    /// [`StoreError::RevertStale`], so a stale banner never takes accepted
    /// work out (audit 2026-11-24 ERR-1905).
    pub fn revert_review(
        &self,
        id: &str,
        since_ts_ms: u64,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<RevertedChanges, StoreError> {
        let stale = || StoreError::RevertStale {
            id: id.to_string(),
            detail: "the review is another one by now (accepted, reverted or changed meanwhile)"
                .into(),
        };
        if !self.has_job_or_log(id)? {
            return Err(stale());
        }
        let _lock = self.lock_briefly(id)?;
        let log = self.activity(id);
        let now = match self.load(id) {
            Ok(p) => Some(p),
            Err(e @ StoreError::NewerFormat { .. }) => return Err(e),
            Err(_) => None,
        };
        let b = self
            .review_baseline(id, &log, now.as_ref())
            .filter(|b| b.since_ts_ms == since_ts_ms)
            .ok_or_else(stale)?;
        self.revert_to_baseline(id, &log, &b, actor, reason)
    }

    /// Revert all: writes the baseline `b` of `id` (the caller holds the
    /// lock and read `log` under it), logged as `actor`'s revert from
    /// `since_ts_ms` (from the AI's first step after its creation for an
    /// AI's new job, which stays). Refused, with nothing written, while
    /// the user edited a value the AI set (`revert_conflict`), when
    /// another program saved the job unlogged since (`revert_outside`),
    /// and for a baseline outside the limits (`revert_out_of_limits`,
    /// `_damaged` when the job file cannot be read either). Counts as
    /// [`Store::revert_to_counted`]: `undone` the changes undone,
    /// `undone_ai` the AI steps the review listed (the AI's creation not).
    pub(super) fn revert_to_baseline(
        &self,
        id: &str,
        log: &[ActivityEntry],
        b: &Baseline,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<RevertedChanges, StoreError> {
        if !b.conflicts.is_empty() {
            return Err(StoreError::RevertConflict {
                id: id.to_string(),
                detail: b.conflicts.join(", "),
            });
        }
        let mut damaged = false;
        let now = match self.load(id) {
            Ok(p) => Some(p),
            Err(e @ StoreError::NewerFormat { .. }) => return Err(e),
            Err(e) => {
                damaged = matches!(
                    e,
                    StoreError::Unreadable { .. } | StoreError::TooMany { .. }
                );
                None
            }
        };
        if now.is_some()
            && let Some(want) = &b.job_digest
            && let Ok(bytes) = read_regular(&self.path(id)?, MAX_JOB_FILE)
            && job_digest(&bytes) != *want
        {
            return Err(StoreError::RevertOutside {
                id: id.to_string(),
                detail: "the job file was saved by another program while the AI's changes waited"
                    .into(),
            });
        }
        b.project.check_limits().map_err(|detail| {
            let id = id.to_string();
            if damaged {
                StoreError::RevertOutOfLimitsDamaged { id, detail }
            } else {
                StoreError::RevertOutOfLimits { id, detail }
            }
        })?;
        let pos = since_index(log, b.since_ts_ms);
        // The AI's creation stays: counted and named from the step after it.
        let from = if b.created && pos < log.len() {
            pos + 1
        } else {
            pos
        };
        let undone = reverted_entries(log.get(from..).unwrap_or_default())
            .filter(|e| e.undid_from.is_none())
            .count()
            .max(1);
        let undone_ai = steps_of(log, b)
            .into_iter()
            .filter(|&i| log[i].before.is_some())
            .count();
        let first = log.get(from).map_or("", |e| e.summary.as_str());
        let summary = if undone == 1 {
            format!("{REVERTED_PREFIX}{first}")
        } else {
            format!("Reverted {undone} changes since: {first}")
        };
        let (summary, reason) = with_reason(reason, summary);
        let undid_from = log.get(from).map_or(b.since_ts_ms, |e| e.ts_ms);
        let entry = if now.as_ref() != Some(&b.project) {
            Some(self.log_and_write(
                id,
                now,
                &b.project,
                actor,
                &summary,
                reason,
                Some(undid_from.max(b.since_ts_ms)),
                None,
                None,
            )?)
        } else {
            None
        };
        self.pin_restored(id, log.get(pos..).unwrap_or_default(), &b.project);
        Ok(RevertedChanges {
            project: b.project.clone(),
            undone: if entry.is_some() { undone } else { 0 },
            undone_ai: if entry.is_some() { undone_ai } else { 0 },
            repaired: entry.is_some() && damaged,
            entry_ts_ms: entry,
        })
    }

    /// R8: before the user's revert of `log[pos..]` that writes over
    /// `before` with no baseline: when it puts back an AI change that did
    /// not stand at the last Accept (the sender of a received job had
    /// reverted it, the user had accepted its removal), the job before
    /// this revert is the baseline.
    pub(super) fn baseline_on_put_back(
        &self,
        id: &str,
        log: &[ActivityEntry],
        pos: usize,
        before: &Project,
    ) {
        if self.review_baseline(id, log, Some(before)).is_some() {
            return;
        }
        let (_, back) = revert_effect(log, pos, log.len(), true);
        if back.is_empty() {
            return;
        }
        let accepted = self.reviewed_at(id);
        let k = log.partition_point(|e| e.ts_ms <= accepted);
        let by = undone_by(&log[..k], &revert_ranges(&log[..k]));
        let unaccepted = back
            .iter()
            .copied()
            .find(|&i| is_ai(&log[i]) && log[i].before.is_some() && (i >= k || by[i].is_some()));
        if let Some(i) = unaccepted {
            let b = Baseline {
                since_ts_ms: log[i].ts_ms,
                project: before.clone(),
                created: false,
                conflicts: Vec::new(),
                replaced: None,
                job_digest: digest_of(before),
            };
            self.write_baseline(id, log.last().map_or(0, |e| e.ts_ms), Some(&b));
        }
    }

    /// Copies the open review of job `from` to its copy `to`, whose job is
    /// `copy` (owner decision 2 of round 20, UX-1905): the copy's log gets
    /// the source's entries from the review's start and its baseline is the
    /// source's with the copy's own changes (name, job, status) applied, so
    /// it shows the same banner and Revert all. Best effort: a copy whose
    /// review could not be written opens without it.
    pub(super) fn copy_review(&self, from: &str, to: &str, copy: &Project) {
        let Some((mut b, Some(src), log)) = self.open_review(from) else {
            return;
        };
        // The copy's own changes (name, job data, draft status, no stored
        // result) win over the baseline's values: the copy is a new job.
        if let Some((p, _)) = rebased(&src, copy, &b.project) {
            b.project = p;
        }
        b.replaced = None;
        b.job_digest = digest_of(copy);
        let pos = since_index(&log, b.since_ts_ms);
        let lines: String = log[pos..]
            .iter()
            .filter_map(|e| serde_json::to_string(e).ok())
            .map(|l| l + "\n")
            .collect();
        let Ok(path) = self.activity_path(to) else {
            return;
        };
        if lines.is_empty() || own_folder(&self.activity_dir()).is_err() {
            return;
        }
        let Ok(mut f) =
            open_regular_for_write(&path, fs::OpenOptions::new().write(true).create_new(true))
        else {
            return;
        };
        if f.write_all(lines.as_bytes())
            .and_then(|()| sync_file(&f))
            .is_err()
        {
            drop(f);
            let _ = fs::remove_file(&path);
            return;
        }
        self.write_baseline(to, log.last().map_or(0, |e| e.ts_ms), Some(&b));
    }
}

/// Whether the review of baseline `b` is open with the job `now` (`None`:
/// the file cannot be read, which differs from any job).
pub(super) fn is_open(b: &Baseline, now: Option<&Project>) -> bool {
    b.created || now != Some(&b.project)
}

/// The job `current` without the entries `log[pos..]` (the user's
/// put-back of the offer's figures after `pos` kept): the inverse of each
/// is merged onto it, latest first, with the version it saved as the base,
/// so edits saved after it without a log entry stay (ERR-001). `None` for
/// `current` (a file that cannot be read): the version before `log[pos]`.
/// What [`Store::revert_to`] writes.
pub(super) fn undo_onto(
    id: &str,
    log: &[ActivityEntry],
    pos: usize,
    mut current: Option<Project>,
) -> Result<Project, StoreError> {
    let value = |p: &Project| {
        serde_json::to_value(p).map_err(|e| StoreError::Unreadable {
            id: id.to_string(),
            detail: e.to_string(),
        })
    };
    let mut target = None;
    let undo: Vec<&ActivityEntry> = reverted_entries(log.get(pos..).unwrap_or_default()).collect();
    for entry in undo.into_iter().rev() {
        let earlier = entry
            .before
            .clone()
            .ok_or_else(|| StoreError::RevertCreated(id.to_string()))?;
        let undone = match (entry.after.as_ref(), current.as_ref()) {
            (Some(after), Some(cur)) if cur != after => {
                let merged = crate::merge::merge3(&value(after)?, &value(cur)?, &value(&earlier)?)
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
    target.ok_or_else(|| StoreError::NothingToRevert(id.to_string()))
}
