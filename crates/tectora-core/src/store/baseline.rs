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
//! | R7 | the take-back of that revert | the two swap again: the old baseline comes back and the one it replaced is kept, so a take-back of the take-back is R7 again |
//! | R8 | no baseline, the user's revert puts back an AI change that did not stand at the last Accept | the job before that revert |
//! | R9 | Accept up to `ts` (also an older Tectora's: the review mark is newer than the file's `accepted_ms`) | settled; with an AI entry newer than `ts`: the job without the AI's entries from that one on |
//! | R10 | another program saved the job without a log entry (the file's digest is not Tectora's last write) | the edit is applied like R3 from the job Tectora last wrote or the log explains; when that job is not known, `outside`: Revert all refuses until the baseline goes |
//!
//! The file is untrusted input like every library file: read only as a
//! regular file (never through a link) of bounded size, and a damaged or
//! unknown one counts as missing. A missing file is a job of an older
//! Tectora: its review is derived once from the change log (the round-19
//! rule, [`review_of`]) and written at the next write. Entries another
//! program logged since the file was written are caught up on read (R1,
//! R4, R9 from the review mark), and so are its unlogged saves (R10), on
//! every read and before every write, so the banner, `get_review`, Revert
//! all and the next write agree.

use super::*;
use std::collections::BTreeMap;

/// The format of the baseline file.
const BASELINE_FORMAT: u32 = 1;

/// Largest baseline file read: the job, the one it replaced (R6) and the
/// job Tectora last saved without a log entry (R10).
const MAX_BASELINE_FILE: u64 = 3 * MAX_JOB_FILE + MAX_SMALL_FILE;

/// Largest value a conflict remembers (`conflict_was`): a bigger one is not
/// kept, and its conflict stays until the baseline goes.
const MAX_CONFLICT_VALUE: usize = 4096;

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
    /// For each conflict, the value the place had before the user's first
    /// edit there (the AI's value): a later edit that puts it back, or that
    /// makes the place the baseline's again, ends the conflict (ERR-2002).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub conflict_was: BTreeMap<String, serde_json::Value>,
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
    /// The job as Tectora last saved it without a log entry (the editor's
    /// save, a copy), the base for another program's later unlogged save
    /// (R10); `None` after a logged write, whose job the log holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_saved: Option<Box<Project>>,
    /// Another program saved the job without a log entry and Tectora could
    /// not tell its edit from the AI's (R10, case 4): Revert all refuses
    /// (`revert_outside`) until the baseline goes (Accept, R4) or R6 sets
    /// it anew.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub outside: bool,
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
    /// The review mark when the file was written: a newer mark is an Accept
    /// by a Tectora that writes no baseline (R9 on read, ARCH-2001). `None`
    /// in a file of round 20: not checked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    accepted_ms: Option<u64>,
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
        conflict_was: BTreeMap::new(),
        replaced: None,
        job_digest: None,
        last_saved: None,
        outside: false,
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

/// Whether the steps [`steps_of`] lists stand in the job: false when the
/// list is its fallback (no AI step stands, the job differs from the
/// baseline by something else), so a banner shows them as history only.
pub(super) fn steps_stand(log: &[ActivityEntry], b: &Baseline, steps: &[usize]) -> bool {
    let pos = since_index(log, b.since_ts_ms);
    let ranges = revert_ranges(log);
    let by = undone_by(log, &ranges);
    steps.iter().any(|&i| {
        i >= pos
            && by[i].is_none()
            && match ranges[i] {
                None => true,
                Some((u, _)) => u < pos,
            }
    })
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
    /// and to the one a take-back would bring back. A remembered conflict
    /// ends when the edit puts the AI's value back or makes the place the
    /// baseline's (ERR-2002); a place whose value cannot be found stays a
    /// conflict.
    fn rebase(&mut self, old: &Project, new: &Project) {
        if old == new {
            return;
        }
        let v = |p: &Project| serde_json::to_value(p).ok();
        if let Some((p, conflicts)) = rebased(old, &self.project, new) {
            self.project = p;
            let (vo, vn, vb) = (v(old), v(new), v(&self.project));
            for c in conflicts {
                if self.conflicts.len() < MAX_CONFLICTS && !self.conflicts.contains(&c) {
                    if let Some(was) = vo.as_ref().and_then(|o| crate::merge::value_at(o, &c))
                        && was.to_string().len() <= MAX_CONFLICT_VALUE
                    {
                        self.conflict_was.insert(c.clone(), was.clone());
                    }
                    self.conflicts.push(c);
                }
            }
            if let (Some(vn), Some(vb)) = (vn.as_ref(), vb.as_ref()) {
                let was = &self.conflict_was;
                let ended = |c: &String| {
                    let Some(now) = crate::merge::value_at(vn, c) else {
                        return false;
                    };
                    was.get(c).is_some_and(|w| crate::merge::same_value(now, w))
                        || crate::merge::value_at(vb, c)
                            .is_some_and(|b| crate::merge::same_value(now, b))
                };
                let ended: Vec<String> = self
                    .conflicts
                    .iter()
                    .filter(|c| ended(c))
                    .cloned()
                    .collect();
                self.conflicts.retain(|c| !ended.contains(c));
                for c in ended {
                    self.conflict_was.remove(&c);
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
            && self.conflict_was.len() <= self.conflicts.len()
            && self.conflict_was.keys().all(|c| self.conflicts.contains(c))
    }

    /// Ends every remembered conflict (R6, R7: the baseline is another
    /// job).
    fn clear_conflicts(&mut self) {
        self.conflicts.clear();
        self.conflict_was.clear();
    }
}

/// R10 for the baseline `b` of a file written after the log entry
/// `log_ts_ms`, given the log, the job `now` and the digest of its file
/// `file` (audit 2026-11-24 round 21, ERR-2001, ARCH-2002): when the file is
/// not what Tectora last wrote, what changed it is found out:
/// 1. the file is Tectora's last write, or no digest is known: nothing;
/// 2. entries another program logged since (an older Tectora's MCP) are
///    R2 for their own change, and a gap between the job known so far and
///    the `before` of such an entry is an unlogged save, applied like R3;
/// 3. the job that is left differs from the file: another program's
///    unlogged save, applied like R3 from the job Tectora last wrote (the
///    one saved without a log entry, `last_saved`, or the newest logged
///    one, whose digest is the file's then), so the colleague's edit lands
///    in the baseline, an edit of a value the AI set is a conflict, and
///    Revert all keeps it;
/// 4. that job is not known: `outside`, Revert all refuses; unless the
///    log's newest entry is the file, which the log then explains.
///
/// A file that cannot be read (`now` `None`) is left to Revert all, which
/// repairs it.
fn fold_outside(
    b: &mut Baseline,
    log_ts_ms: u64,
    log: &[ActivityEntry],
    now: Option<&Project>,
    file: Option<&str>,
) {
    let (Some(want), Some(file), Some(now)) = (b.job_digest.clone(), file, now) else {
        return;
    };
    if want == file {
        return;
    }
    let ours = |p: &Project| digest_of(p).as_deref() == Some(want.as_str());
    let saved = b.last_saved.as_deref().filter(|p| ours(p)).cloned();
    let mut known: Option<Project> = saved.or_else(|| {
        log.iter()
            .rev()
            .find(|e| e.ts_ms <= log_ts_ms)
            .and_then(|e| e.after.as_ref())
            .filter(|p| ours(p))
            .cloned()
    });
    let mut told_apart = true;
    for e in log.iter().filter(|e| e.ts_ms > log_ts_ms) {
        work(1);
        match (&known, &e.before) {
            (Some(k), Some(before)) if k != before => b.rebase(k, before),
            (Some(_), Some(_)) => {}
            (None, Some(before)) if ours(before) => {}
            _ => told_apart = false,
        }
        known = e.after.clone();
    }
    match known {
        Some(k) if told_apart => b.rebase(&k, now),
        // The log's newest entry is the file (a log rewritten with its
        // file): the log explains it.
        _ if log
            .last()
            .and_then(|e| e.after.as_ref())
            .is_some_and(|a| digest_of(a).as_deref() == Some(file)) => {}
        _ => b.outside = true,
    }
    b.job_digest = Some(file.to_string());
    b.last_saved = Some(Box::new(now.clone()));
}

/// A baseline left at an id by another job (ARCH-2006: an older Tectora's
/// delete leaves the file, and a new job takes the id): the log is empty,
/// or it starts with the creation of a job after the baseline's start.
fn stray(b: &Baseline, log: &[ActivityEntry]) -> bool {
    log.first()
        .is_none_or(|e| e.before.is_none() && e.ts_ms > b.since_ts_ms)
}

/// What a baseline file says about a job before the entries `log` holds
/// are caught up ([`Store::review_baseline`]).
enum Stored {
    /// No file (a job of an older Tectora) or a damaged one.
    Missing,
    Settled {
        log_ts_ms: u64,
        accepted_ms: Option<u64>,
    },
    Open {
        log_ts_ms: u64,
        baseline: Box<Baseline>,
        /// [`BaselineFile::accepted_ms`].
        accepted_ms: Option<u64>,
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
                accepted_ms: f.accepted_ms,
            },
            Some(b) if b.plausible() => Stored::Open {
                log_ts_ms: f.log_ts_ms,
                baseline: Box::new(b),
                accepted_ms: f.accepted_ms,
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
            accepted_ms: Some(self.reviewed_at(id)),
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
    /// job, then R1 for the AI's next entry), an Accept of a Tectora that
    /// writes no baseline (R9 from the review mark) and another program's
    /// unlogged save (R10); for a job without a file (an older Tectora's)
    /// or a file another job left (ARCH-2006), derived once from the log.
    /// `None`: settled. The job file is read for its digest when a stored
    /// baseline needs it ([`Store::review_baseline_at`] when the caller
    /// has it).
    pub(super) fn review_baseline(
        &self,
        id: &str,
        log: &[ActivityEntry],
        now: Option<&Project>,
    ) -> Option<Baseline> {
        self.review_baseline_at(id, log, now, None)
    }

    /// [`Store::review_baseline`] with the digest of the job file `now`
    /// was read from, `file` (`None`: read here when needed).
    pub(super) fn review_baseline_at(
        &self,
        id: &str,
        log: &[ActivityEntry],
        now: Option<&Project>,
        file: Option<&str>,
    ) -> Option<Baseline> {
        let (b, newer_than, accepted_ms) = match self.stored_baseline(id) {
            Stored::Missing => return self.derived_baseline(id, log, now),
            Stored::Settled {
                log_ts_ms,
                accepted_ms,
            } => (None, log_ts_ms, accepted_ms),
            Stored::Open { baseline, .. } if stray(&baseline, log) => {
                return self.derived_baseline(id, log, now);
            }
            Stored::Open {
                log_ts_ms,
                baseline,
                accepted_ms,
            } => (Some(*baseline), log_ts_ms, accepted_ms),
        };
        let mut b = self.caught_up_logged(log, b, newer_than, now)?;
        if b.job_digest.is_some() {
            let read;
            let file = match file {
                Some(f) => Some(f),
                None => {
                    read = self
                        .path(id)
                        .ok()
                        .and_then(|p| read_regular(&p, MAX_JOB_FILE).ok())
                        .map(|bytes| {
                            job_digest(bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&bytes))
                        });
                    read.as_deref()
                }
            };
            fold_outside(&mut b, newer_than, log, now, file);
        }
        // An Accept no baseline was written for (an older Tectora's, or
        // ours when the write after the mark failed): R9 with its time.
        let mark = accepted_ms.map(|_| self.reviewed_at(id));
        if let (Some(at), Some(mark)) = (accepted_ms, mark)
            && mark > at
        {
            return self.accepted(id, log, now, b, mark);
        }
        Some(b)
    }

    /// Catches up the entries another program logged after the file's
    /// `newer_than` on the stored state `b` (R4, then R1).
    fn caught_up_logged(
        &self,
        log: &[ActivityEntry],
        b: Option<Baseline>,
        newer_than: u64,
        now: Option<&Project>,
    ) -> Option<Baseline> {
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
        let (now, file) = match self.load_with_digest(id) {
            Ok((p, d)) => (Some(p), Some(d)),
            Err(_) => (None, None),
        };
        let b = self.review_baseline_at(id, &log, now.as_ref(), file.as_deref())?;
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
                        // A swap, so the take-back of this take-back is R7
                        // again, any number of times (ARCH-2003).
                        Some(from)
                            if b.replaced.as_ref().is_some_and(|r| r.revert_ts_ms == from) =>
                        {
                            let r = b.replaced.take().expect("checked");
                            let old = std::mem::replace(&mut b.project, *r.project);
                            b.replaced = Some(Replaced {
                                revert_ts_ms: ts_ms,
                                project: Box::new(old),
                            });
                            b.clear_conflicts();
                        }
                        // R6: the user reverted work settled before the
                        // review: the job they wrote is the baseline.
                        Some(from) if from < b.since_ts_ms => {
                            let old = std::mem::replace(&mut b.project, after.clone());
                            b.replaced = Some(Replaced {
                                revert_ts_ms: ts_ms,
                                project: Box::new(old),
                            });
                            b.clear_conflicts();
                            b.outside = false;
                        }
                        // R5: a revert inside the review moves the job only.
                        Some(_) => {}
                    }
                }
                Some(b)
            }
        };
        // The job is the log's now (R10 reads it from the newest entry).
        let next = next.map(|mut b| {
            b.job_digest = digest();
            b.last_saved = None;
            b
        });
        self.write_baseline(id, ts_ms, next.as_ref());
    }

    /// R3 for the editor's own save of `new` over `old` (the file under
    /// the lock, read here when not given): applied to an open baseline.
    /// Only the baseline file is read when no review is open. Returns the
    /// raw file to put back if the job write fails.
    ///
    /// The file's digest `file` (read here when not given) first goes
    /// through R10, so another program's save before this one is folded in
    /// before the digest moves on (ERR-2001); the entries another program
    /// logged and an Accept without a baseline are caught up the same way,
    /// and the file is written as of the newest entry.
    pub(super) fn baseline_on_unlogged(
        &self,
        id: &str,
        old: Option<(&Project, &str)>,
        new: &Project,
    ) -> Option<Option<Vec<u8>>> {
        let Stored::Open {
            log_ts_ms: stored_ts,
            baseline: stored,
            ..
        } = self.stored_baseline(id)
        else {
            return None;
        };
        let raw = self.baseline_file_bytes(id);
        let read;
        let (old, file) = match old {
            Some((o, d)) => (Some(o), Some(d)),
            None => {
                read = self.load_with_digest(id).ok();
                (
                    read.as_ref().map(|(p, _)| p),
                    read.as_ref().map(|(_, d)| d.as_str()),
                )
            }
        };
        let log = self.activity(id);
        let newest = log.last().map_or(0, |e| e.ts_ms);
        let Some(mut b) = self.review_baseline_at(id, &log, old, file) else {
            self.write_baseline(id, newest, None);
            return Some(raw);
        };
        if let Some(old) = old {
            b.rebase(old, new);
        }
        b.job_digest = digest_of(new);
        b.last_saved = Some(Box::new(new.clone()));
        // Nothing to write when the file says this already (an autosave
        // of the same job).
        if *stored != b || stored_ts != newest {
            self.write_baseline(id, newest, Some(&b));
        }
        Some(raw)
    }

    /// R9: the user accepted every entry up to `accepted_ms`. The caller
    /// holds the job's lock and wrote the review mark.
    pub(super) fn baseline_on_accept(&self, id: &str, accepted_ms: u64) {
        let log = self.activity(id);
        let read = self.load_with_digest(id).ok();
        let now = read.as_ref().map(|(p, _)| p);
        let file = read.as_ref().map(|(_, d)| d.as_str());
        let newest = log.last().map_or(0, |e| e.ts_ms);
        let Some(prior) = self.review_baseline_at(id, &log, now, file) else {
            // Settled already (written, so an older Tectora's log is
            // never read for it again).
            if !matches!(self.stored_baseline(id), Stored::Settled { .. }) {
                self.write_baseline(id, newest, None);
            }
            return;
        };
        let next = self.accepted(id, &log, now, prior, accepted_ms);
        self.write_baseline(id, newest, next.as_ref());
    }

    /// R9 on the baseline `prior`, in memory: every entry up to
    /// `accepted_ms` is accepted. `None`: settled; with an AI entry newer
    /// than `accepted_ms` from the review's start on, the job `now` without
    /// the AI's entries from that one on (the user's unlogged edits kept).
    /// The same for our Accept and for one read from the review mark
    /// (ARCH-2001), so applying it twice gives the same baseline.
    fn accepted(
        &self,
        id: &str,
        log: &[ActivityEntry],
        now: Option<&Project>,
        prior: Baseline,
        accepted_ms: u64,
    ) -> Option<Baseline> {
        let k = log
            .iter()
            .position(|e| e.ts_ms > accepted_ms && is_ai(e) && e.ts_ms >= prior.since_ts_ms)?;
        let mut b = before_entry(&log[k])?;
        if prior.created && b.created {
            return Some(prior);
        }
        let mut kept = false;
        if let Some(now) = now
            && let Ok(p) = undo_onto(id, log, k, Some(now.clone()))
        {
            b.project = p;
            kept = true;
        }
        b.job_digest = prior.job_digest;
        b.last_saved = prior.last_saved;
        // Taken from the job as it is, the baseline holds every edit saved
        // without a log entry, another program's too.
        b.outside = prior.outside && !kept;
        Some(b)
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
        let (now, file) = match self.load_with_digest(id) {
            Ok((p, d)) => (Some(p), Some(d)),
            Err(e @ StoreError::NewerFormat { .. }) => return Err(e),
            Err(_) => (None, None),
        };
        let b = self
            .review_baseline_at(id, &log, now.as_ref(), file.as_deref())
            .filter(|b| b.since_ts_ms == since_ts_ms)
            .ok_or_else(stale)?;
        self.revert_to_baseline(id, &log, &b, actor, reason)
    }

    /// Revert all: writes the baseline `b` of `id` (the caller holds the
    /// lock and read `log` under it), logged as `actor`'s revert from
    /// `since_ts_ms` (from the AI's first step after its creation for an
    /// AI's new job, which stays). Refused, with nothing written, while
    /// the user edited a value the AI set (`revert_conflict`), when
    /// another program's unlogged save could not be told apart from the
    /// AI's changes (`revert_outside`, R10),
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
        // R10, case 4: another program's unlogged save that could not be
        // told apart from the AI's changes (the digest itself is checked
        // where the baseline is read, `fold_outside`).
        if b.outside {
            return Err(StoreError::RevertOutside {
                id: id.to_string(),
                detail: "the job file was saved by another program while the AI's changes waited, and its edit cannot be told apart from the AI's"
                    .into(),
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
        b.project.check_limits().map_err(|detail| {
            let id = id.to_string();
            if damaged {
                StoreError::RevertOutOfLimitsDamaged { id, detail }
            } else {
                StoreError::RevertOutOfLimits { id, detail }
            }
        })?;
        let pos = since_index(log, b.since_ts_ms);
        // The AI's creation stays: counted and named from the step after
        // it, when the log still holds it (ARCH-2005).
        let from = if b.created
            && log
                .get(pos)
                .is_some_and(|e| e.ts_ms == b.since_ts_ms && e.before.is_none())
        {
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
                conflict_was: BTreeMap::new(),
                replaced: None,
                job_digest: digest_of(before),
                last_saved: None,
                outside: false,
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
        // The copy is written without a log entry: its own job is the base
        // for another program's save (R10), and nothing outside is in it.
        b.job_digest = digest_of(copy);
        b.last_saved = Some(Box::new(copy.clone()));
        b.outside = false;
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
