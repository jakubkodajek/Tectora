//! The stored result of a job: the figures an offer was made with, kept in
//! the project so a later change of the design or of the roof engine shows
//! up as a difference instead of silently changing the numbers.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::model::Project;

/// Version of the roof engine (this crate) that calculates the results.
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Values closer than this count as equal (results are kept to centimetres).
const TOLERANCE: f64 = 0.005;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MaterialArea {
    pub material: u8,
    /// m²
    pub area: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ResultSnapshot {
    /// Roof engine version that calculated the result.
    pub engine_version: String,
    /// When it was stored, ms since the Unix epoch.
    pub taken_ms: u64,
    /// Roof area, m².
    pub roof_area: f64,
    /// Ridge height, m.
    pub ridge_height: f64,
    /// Line lengths by line code, m.
    pub lines_by_code: BTreeMap<String, f64>,
    #[serde(default)]
    pub area_by_material: Vec<MaterialArea>,
}

/// Most line codes a stored result may hold: the core gives about 15.
pub const MAX_STORED_CODES: usize = 64;
/// Longest line code of a stored result, in characters (the core's are
/// one or two letters).
pub const MAX_CODE_CHARS: usize = 4;
/// Longest engine version of a stored result, in characters.
pub const MAX_ENGINE_VERSION_CHARS: usize = 40;
/// Most material figures of a stored result (materials are 0 to 255).
pub const MAX_STORED_MATERIALS: usize = 256;
/// Largest absolute figure of a stored result (m², m).
const MAX_FIGURE: f64 = 1.0e9;
/// Latest time a stored result may carry, ms since the Unix epoch: the
/// last date JavaScript (the app) can show, 13 September 275760. It is
/// below 2^53, so the app reads and sends back every allowed time exactly
/// (audit 2026-11-22 HUNT-802).
pub const MAX_TAKEN_MS: u64 = 8_640_000_000_000_000;
/// Most differences [`compare`] gives: a second bound after the stored
/// result's own (ARCH-303).
pub const MAX_DIFFERENCES: usize = 100;

/// Whether `code` reads as a line code: 1 to [`MAX_CODE_CHARS`] ASCII
/// letters and digits (`T`, `OL`, `c`, `WV`, …).
pub fn is_line_code(code: &str) -> bool {
    (1..=MAX_CODE_CHARS).contains(&code.len()) && code.bytes().all(|b| b.is_ascii_alphanumeric())
}

impl ResultSnapshot {
    /// Why this stored result is not one Tectora wrote, or `None`: too many
    /// line codes or materials, a code that is no line code, an engine
    /// version that is not one short line, a figure that is not a finite
    /// number of a real roof. A job from outside (a received bundle, a file
    /// copied into the library) is held to it: 200 000 codes froze the
    /// editor and made every MCP read of the job 23 MB (ARCH-303).
    pub fn problem(&self) -> Option<String> {
        self.out_of_bounds()
            .map(|why| format!("the stored result is damaged: {why}"))
    }

    /// What of this result is outside the bounds [`ResultSnapshot::problem`]
    /// holds a stored result to, or `None`.
    fn out_of_bounds(&self) -> Option<String> {
        let why = if self.lines_by_code.len() > MAX_STORED_CODES {
            format!(
                "too many line codes: {} (at most {MAX_STORED_CODES})",
                self.lines_by_code.len()
            )
        } else if let Some(k) = self.lines_by_code.keys().find(|k| !is_line_code(k)) {
            let shown: String = k.chars().take(8).collect();
            format!("{shown:?} is not a line code")
        } else if self.area_by_material.len() > MAX_STORED_MATERIALS {
            format!(
                "too many materials: {} (at most {MAX_STORED_MATERIALS})",
                self.area_by_material.len()
            )
        } else if self.engine_version.chars().count() > MAX_ENGINE_VERSION_CHARS
            || crate::edit::check_label("engine version", &self.engine_version).is_err()
        {
            format!(
                "the engine version is not one line of at most {MAX_ENGINE_VERSION_CHARS} characters"
            )
        } else if [self.roof_area, self.ridge_height]
            .into_iter()
            .chain(self.lines_by_code.values().copied())
            .chain(self.area_by_material.iter().map(|m| m.area))
            .any(|v| !(v.is_finite() && v.abs() <= MAX_FIGURE))
        {
            format!(
                "a figure is not a number of a real roof (an area of at most {MAX_FIGURE} m², a length or height of at most {MAX_FIGURE} m)"
            )
        } else {
            return None;
        };
        Some(why)
    }

    /// Why the time of this stored result cannot be a real one (after
    /// [`MAX_TAKEN_MS`]), or `None`. Not a reason to refuse the job: the
    /// time is metadata (lessons-learned 2026-11-03), so such a result is
    /// dropped instead ([`drop_impossible`]).
    pub fn impossible_time(&self) -> Option<String> {
        (self.taken_ms > MAX_TAKEN_MS).then(|| {
            format!(
                "the stored result is dated {} ms after 1970, later than any calendar date (at most {MAX_TAKEN_MS}); it was left out",
                self.taken_ms
            )
        })
    }

    /// Why this result, read from a record or the change log, may not be
    /// written into a job as its stored result: damaged
    /// ([`ResultSnapshot::problem`]) or dated after any calendar date
    /// ([`ResultSnapshot::impossible_time`]), which every later read would
    /// drop (audit 2026-11-23 HUNT-1102). `Ok` when it may.
    pub fn writable(&self) -> Result<(), String> {
        match self.problem().or_else(|| self.impossible_time()) {
            Some(why) => Err(why),
            None => Ok(()),
        }
    }

    /// The engine version of a stored result from outside as one line of
    /// at most [`MAX_ENGINE_VERSION_CHARS`] characters (ARCH-303).
    pub fn clean(&mut self) {
        self.engine_version =
            crate::edit::clean_line(&self.engine_version, MAX_ENGINE_VERSION_CHARS);
    }
}

/// Drops the stored result of `project` when its time cannot be a real
/// one ([`ResultSnapshot::impossible_time`]) and says why; the job opens
/// without it. The app could neither show nor send back such a time, so
/// the job could never be saved from the app (audit 2026-11-22 HUNT-802;
/// owner decision: open the job, drop the result, warn).
pub fn drop_impossible(project: &mut Project) -> Option<String> {
    let why = project.result_snapshot.as_ref()?.impossible_time()?;
    project.result_snapshot = None;
    Some(why)
}

fn r2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// The project's current result.
/// [`take`] for input from outside: refuses a project out of range
/// (`Err(Refused::OutOfRange)`) and turns a kernel panic into
/// `Err(Refused::Internal)` instead of aborting the caller.
pub fn take_checked(project: &Project) -> Result<ResultSnapshot, Refused> {
    project.check_limits().map_err(Refused::OutOfRange)?;
    // No building and no hand-drawn face: no figures to offer (UX-102).
    if project.parts.is_empty() && project.manual_faces.is_empty() {
        return Err(Refused::NoBuilding);
    }
    // Input the calculation leaves out (a building with a wall shorter than
    // 1 cm, …) is missing from the figures (review UX-002).
    if let Some(p) = project.problems().first() {
        // Indices from 0, as `calc.problems` and every MCP tool (ERR-007).
        let edge = p
            .edge
            .map(|e| format!(", edge index {e}"))
            .unwrap_or_default();
        return Err(Refused::NotCalculated(format!(
            "{:?} index {}{edge} is left out of the calculation ({})",
            p.item, p.index, p.code
        )));
    }
    // A part the core gives no roof for counts 0 m²: storing that as the
    // offer's figures would hide the problem (review ERR-003, DR-001).
    // One calculation for the check and the figures.
    crate::model::guarded(|| {
        let geos = crate::openings::generate_project(project);
        match geos.iter().position(|g| g.error.is_some()) {
            Some(i) => Err(Refused::NotCalculated(format!(
                "part index {i} has no roof"
            ))),
            None => {
                let taken = take_of(&geos, project);
                // A result the store would refuse to keep is refused here,
                // for what it is: the roof is too large, not a damaged
                // file (audit 2026-11-22 HUNT-405, ARCH-403 item 1).
                match taken.out_of_bounds() {
                    Some(why) => Err(Refused::TooLarge(why)),
                    None => Ok(taken),
                }
            }
        }
    })
    .map_err(Refused::Internal)?
}

/// Why [`take_checked`] stored no result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// A number of the project is out of range (the detail says which).
    OutOfRange(String),
    /// The roof calculation failed.
    Internal(String),
    /// A part of the roof cannot be calculated (the detail says which).
    NotCalculated(String),
    /// The job has no building and no hand-drawn face: nothing to store.
    NoBuilding,
    /// The figures are larger than a stored result may hold (a roof of
    /// over 10⁹ m² or m): the detail says which bound.
    TooLarge(String),
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refused::OutOfRange(d) => write!(f, "out of range: {d}"),
            Refused::Internal(d) => f.write_str(d),
            Refused::NotCalculated(d) => write!(f, "the roof cannot be calculated: {d}"),
            Refused::NoBuilding => {
                f.write_str("the job has no building, so there are no figures to store")
            }
            Refused::TooLarge(d) => write!(f, "the figures are too large to keep: {d}"),
        }
    }
}

pub fn take(project: &Project) -> ResultSnapshot {
    take_of(&crate::openings::generate_project(project), project)
}

/// [`take`] of roofs already calculated (`openings::generate_project`).
pub fn take_of(geos: &[crate::roof::RoofGeometry], project: &Project) -> ResultSnapshot {
    let t = crate::quantities::roof_totals_of(geos, project);
    ResultSnapshot {
        engine_version: ENGINE_VERSION.to_string(),
        taken_ms: crate::store::now_ms(),
        roof_area: r2(t.roof_area),
        ridge_height: r2(t.ridge_height),
        lines_by_code: crate::quantities::dacad_line_totals_of(geos, project)
            .into_iter()
            .map(|(k, v)| (k, r2(v)))
            .filter(|(_, v)| v.abs() >= TOLERANCE)
            .collect(),
        area_by_material: t
            .area_by_material
            .iter()
            .map(|&(material, a)| MaterialArea {
                material,
                area: r2(a),
            })
            .collect(),
    }
}

/// One value that differs between the stored and the current result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Difference {
    /// `roof_area`, `ridge_height`, `line:<code>` or `material:<number>`.
    pub key: String,
    pub stored: f64,
    pub current: f64,
}

/// What changed from `stored` to `current`; empty when the result is the same.
pub fn compare(stored: &ResultSnapshot, current: &ResultSnapshot) -> Vec<Difference> {
    let mut out = Vec::new();
    let mut check = |key: String, a: f64, b: f64| {
        if (a - b).abs() >= TOLERANCE {
            out.push(Difference {
                key,
                stored: a,
                current: b,
            });
        }
    };
    check("roof_area".into(), stored.roof_area, current.roof_area);
    check(
        "ridge_height".into(),
        stored.ridge_height,
        current.ridge_height,
    );
    let codes: std::collections::BTreeSet<&String> = stored
        .lines_by_code
        .keys()
        .chain(current.lines_by_code.keys())
        .collect();
    for c in codes {
        let get = |s: &ResultSnapshot| s.lines_by_code.get(c).copied().unwrap_or(0.0);
        check(format!("line:{c}"), get(stored), get(current));
    }
    let mats: std::collections::BTreeSet<u8> = stored
        .area_by_material
        .iter()
        .chain(&current.area_by_material)
        .map(|m| m.material)
        .collect();
    // One default material is the roof area again; only a split says more.
    if mats.iter().all(|&m| m == crate::model::DEFAULT_MATERIAL) {
        out.truncate(MAX_DIFFERENCES);
        return out;
    }
    for m in mats {
        let get = |s: &ResultSnapshot| {
            s.area_by_material
                .iter()
                .find(|x| x.material == m)
                .map_or(0.0, |x| x.area)
        };
        check(format!("material:{m}"), get(stored), get(current));
    }
    out.truncate(MAX_DIFFERENCES);
    out
}

/// How the figures (area, line lengths, ridge height, area by material)
/// change from `before` to `after`; `None` stands for a project without a
/// roof. A roof that is out of range or makes the calculation fail gives no
/// figures, so its change is unknown (`None`).
pub fn change_in_figures(before: Option<&Project>, after: &Project) -> Option<Vec<Difference>> {
    let figures = |p: &Project| {
        p.check_limits().ok()?;
        crate::model::guarded(|| take(p)).ok()
    };
    let empty = ResultSnapshot {
        engine_version: ENGINE_VERSION.to_string(),
        taken_ms: 0,
        roof_area: 0.0,
        ridge_height: 0.0,
        lines_by_code: BTreeMap::new(),
        area_by_material: Vec::new(),
    };
    let a = match before {
        Some(p) => figures(p)?,
        None => empty,
    };
    Some(compare(&a, &figures(after)?))
}

/// How the change-log entry of [`refreshed`] begins; the app reads it to
/// show the sentence in the user's language. The entry reads
/// `Stored result recalculated (Tectora <from> -> <to>): roof area <a> -> <b> m2`.
/// When the roof area is the same it names what else changed instead
/// ([`refreshed_what`]): `line lengths G, K` or `other figures`.
pub const REFRESHED_PREFIX: &str = "Stored result recalculated";

/// How many line codes the summary of a recalculation names.
const REFRESHED_CODES: usize = 8;

/// What a recalculation changed, for its change-log summary, from the
/// `differences` themselves (audit 2026-11-22 UX-807: "roof area 142.83 ->
/// 142.83 m2" read as "nothing changed" while the hips changed by 2 m):
/// the roof area when it differs, else the codes of the lines whose
/// lengths differ, else "other figures" (the ridge height, the areas by
/// material). The app reads all three (`live.svelte.ts`).
fn refreshed_what(differences: &[Difference]) -> String {
    if let Some(area) = differences.iter().find(|d| d.key == "roof_area") {
        return format!("roof area {:.2} -> {:.2} m2", area.stored, area.current);
    }
    let codes: Vec<&str> = differences
        .iter()
        .filter_map(|d| d.key.strip_prefix("line:"))
        // A code is a word of a DACAD line table; one from a file that is
        // none would break the sentence.
        .filter(|c| !c.is_empty() && c.len() <= 8 && c.chars().all(|c| c.is_ascii_alphanumeric()))
        .take(REFRESHED_CODES)
        .collect();
    if codes.is_empty() {
        "other figures".into()
    } else {
        format!("line lengths {}", codes.join(", "))
    }
}

/// `a.b.c` as numbers, for comparing engine versions.
fn version(v: &str) -> Option<(u64, u64, u64)> {
    // A clean `a.b.c[-pre][+build]` only: the string comes from the file
    // and goes into the change log.
    if v.len() > 32
        || !v
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
    {
        return None;
    }
    let mut it = v.split(['.', '-', '+']).map(|n| n.parse::<u64>().ok());
    Some((it.next()??, it.next()??, it.next()??))
}

/// A fingerprint of what the roof is calculated from: the project without
/// its name, notes, job data and stored result (FNV-1a over its JSON).
/// Equal fingerprints mean the design is unchanged.
pub fn input_digest(project: &Project) -> String {
    let mut p = project.clone();
    p.result_snapshot = None;
    p.name.clear();
    p.notes.clear();
    p.job = Default::default();
    p.extra.clear();
    let text = serde_json::to_string(&p).unwrap_or_default();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// The job with its stored result recalculated, where an older engine
/// stored it and this one, with its corrections, gives other figures
/// (Jakub's decision 2026-11-21: stored results follow a fixed
/// calculation, the old figures stay in the change log). `None` when the
/// result was stored by this engine or a newer one, the figures are the
/// same, or the roof cannot be calculated now (then the stored result
/// stays as the offer had it), with the change log's summary otherwise.
/// Whether the design changed since the result was stored is not known
/// here: [`crate::store::Store::refresh_stored_result`] checks that.
pub fn refreshed(project: &Project) -> Option<(Project, String)> {
    let stored = project.result_snapshot.as_ref()?;
    if version(&stored.engine_version)? >= version(ENGINE_VERSION)? {
        return None;
    }
    let current = take_checked(project).ok()?;
    let differences = compare(stored, &current);
    if differences.is_empty() {
        return None;
    }
    let summary = format!(
        "{REFRESHED_PREFIX} (Tectora {} -> {}): {}",
        stored.engine_version,
        ENGINE_VERSION,
        refreshed_what(&differences)
    );
    let mut p = project.clone();
    p.result_snapshot = Some(current);
    Some((p, summary))
}

/// The stored result against the current one, for the app and the agents.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SnapshotStatus {
    pub stored: ResultSnapshot,
    pub differences: Vec<Difference>,
    /// The stored result came from another engine version.
    pub engine_changed: bool,
}

pub fn status(project: &Project) -> Option<SnapshotStatus> {
    project.result_snapshot.as_ref()?;
    status_of(&crate::openings::generate_project(project), project)
}

/// [`status`] of roofs already calculated.
pub fn status_of(geos: &[crate::roof::RoofGeometry], project: &Project) -> Option<SnapshotStatus> {
    let stored = project.result_snapshot.clone()?;
    let current = take_of(geos, project);
    Some(SnapshotStatus {
        differences: compare(&stored, &current),
        engine_changed: stored.engine_version != ENGINE_VERSION,
        stored,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::examples;
    use crate::model::JobStatus;

    /// Audit 2026-11-22 UX-807: the summary of a recalculation says what
    /// changed, from the differences: the area when it differs, else the
    /// line codes, else "other figures".
    #[test]
    fn the_recalculation_summary_names_what_changed() {
        let d = |key: &str, stored: f64, current: f64| Difference {
            key: key.into(),
            stored,
            current,
        };
        assert_eq!(
            refreshed_what(&[d("roof_area", 150.0, 142.834), d("line:G", 30.41, 28.41)]),
            "roof area 150.00 -> 142.83 m2"
        );
        assert_eq!(
            refreshed_what(&[d("line:G", 30.41, 28.41), d("line:K", 1.0, 2.0)]),
            "line lengths G, K"
        );
        assert_eq!(
            refreshed_what(&[d("ridge_height", 4.0, 4.2)]),
            "other figures"
        );
        assert_eq!(
            refreshed_what(&[d("material:2", 4.0, 4.2)]),
            "other figures"
        );
        // A code that is no word of a line table never breaks the sentence.
        assert_eq!(
            refreshed_what(&[d("line:a, b): x", 1.0, 2.0), d("line:WV", 1.0, 2.0)]),
            "line lengths WV"
        );
        let many: Vec<Difference> = (0..20)
            .map(|i| d(&format!("line:L{i}"), 1.0, 2.0))
            .collect();
        assert_eq!(refreshed_what(&many).split(", ").count(), REFRESHED_CODES);

        // Through `refreshed`: the same area, one line 2 m longer before.
        let mut p = examples::find("gable").unwrap().project();
        let mut s = take(&p);
        s.engine_version = "0.0.1".into();
        let code = s.lines_by_code.keys().next().unwrap().clone();
        *s.lines_by_code.get_mut(&code).unwrap() += 2.0;
        p.result_snapshot = Some(s);
        let (_, summary) = refreshed(&p).expect("recalculated");
        assert_eq!(
            summary,
            format!("{REFRESHED_PREFIX} (Tectora 0.0.1 -> {ENGINE_VERSION}): line lengths {code}")
        );
    }

    /// Audit 2026-11-22 HUNT-405, ARCH-403 item 1: the result of a roof
    /// the core calculates but a stored result may not hold (a face of
    /// 40 km x 40 km, 1.85e9 m²) is refused as too large, not as a damaged
    /// file, and is never handed out to be stored.
    #[test]
    fn a_result_too_large_to_keep_is_refused_as_too_large_not_damaged() {
        use crate::geom::Point2;
        let mut p = examples::find("gable").unwrap().project();
        let s = 40_000.0;
        p.manual_faces.push(crate::model::ManualFace {
            points: vec![
                Point2::new(20.0, 0.0),
                Point2::new(20.0 + s, 0.0),
                Point2::new(20.0 + s, s),
                Point2::new(20.0, s),
            ],
            pitch_deg: 30.0,
            label: String::new(),
        });
        assert!(p.check_limits().is_ok());
        assert!(take(&p).roof_area > MAX_FIGURE, "the core calculates it");
        let e = take_checked(&p).unwrap_err();
        assert!(matches!(e, Refused::TooLarge(_)), "{e:?}");
        let text = e.to_string();
        assert!(text.contains("too large"), "{text}");
        // With its units, for the AI as for the app (UX-504).
        assert!(text.contains(" m²") && text.contains(" m)"), "{text}");
        assert!(!text.contains("damaged"), "{text}");
        let e = crate::commands::take_snapshot(&p).unwrap_err();
        assert_eq!(e.code, "figures_too_large", "{e:?}");
        assert!(!e.detail.contains("damaged"), "{e:?}");
    }

    /// A gable job whose stored result an older engine took with a roof
    /// area 2 m² off (a calculation fixed since).
    fn stored_by(version: &str, off: f64) -> Project {
        let mut p = examples::find("gable").unwrap().project();
        let mut s = take(&p);
        s.engine_version = version.into();
        s.roof_area += off;
        p.result_snapshot = Some(s);
        p
    }

    #[test]
    fn a_result_an_older_engine_stored_follows_the_corrected_calculation() {
        let p = stored_by("0.0.1", 2.0);
        let (q, summary) = refreshed(&p).expect("refreshed");
        let s = q.result_snapshot.as_ref().unwrap();
        assert_eq!(s.engine_version, ENGINE_VERSION);
        assert!(compare(s, &take(&p)).is_empty());
        let old = p.result_snapshot.as_ref().unwrap().roof_area;
        assert_eq!(
            summary,
            format!(
                "{REFRESHED_PREFIX} (Tectora 0.0.1 -> {ENGINE_VERSION}): roof area {old:.2} -> {:.2} m2",
                s.roof_area
            )
        );
        // Nothing else of the job changes.
        let mut back = q.clone();
        back.result_snapshot = p.result_snapshot.clone();
        assert_eq!(back, p);
    }

    #[test]
    fn a_result_stored_by_this_or_a_newer_engine_or_with_the_same_figures_stays() {
        assert!(refreshed(&stored_by(ENGINE_VERSION, 2.0)).is_none());
        assert!(refreshed(&stored_by("999.0.0", 2.0)).is_none());
        assert!(refreshed(&stored_by("0.0.1", 0.0)).is_none());
        assert!(refreshed(&stored_by("not a version", 2.0)).is_none());
        assert!(refreshed(&stored_by("0.0.1-a b", 2.0)).is_none());
        assert!(refreshed(&stored_by("0.0.1+x\ny", 2.0)).is_none());
        assert!(refreshed(&stored_by(&format!("0.0.1-{}", "x".repeat(40)), 2.0)).is_none());
        assert!(refreshed(&stored_by("0.0.1-rc.1+build.5", 2.0)).is_some());
        let mut none = stored_by("0.0.1", 2.0);
        none.result_snapshot = None;
        assert!(refreshed(&none).is_none());
    }

    #[test]
    fn a_stored_result_stays_when_the_roof_cannot_be_calculated_now() {
        let mut p = stored_by("0.0.1", 2.0);
        p.parts[0].edges[0].start.x = 1e300;
        assert!(refreshed(&p).is_none());
    }

    #[test]
    fn storing_the_result_of_a_roof_out_of_range_is_refused_not_run() {
        let ok = examples::find("gable").unwrap().project();
        assert_eq!(take_checked(&ok).unwrap().roof_area, take(&ok).roof_area);
        let mut far = ok.clone();
        far.parts[0].edges[0].start.x = 1e300;
        assert!(matches!(take_checked(&far), Err(Refused::OutOfRange(_))));
    }

    #[test]
    fn storing_the_result_of_a_roof_that_cannot_be_calculated_is_refused() {
        // ERR-003 / DR-001: a non-convex chimney leaves the part without a
        // roof; the offer stored 0 m² as its figures.
        let mut p = examples::find("hip").unwrap().project();
        let pt = |x, y| crate::geom::Point2::new(x, y);
        p.chimneys = vec![crate::model::Chimney {
            corners: vec![
                pt(3.0, 3.0),
                pt(4.0, 3.0),
                pt(3.5, 3.4),
                pt(4.0, 4.0),
                pt(3.0, 4.0),
            ],
            height: 0.0,
        }];
        assert!(matches!(take_checked(&p), Err(Refused::NotCalculated(_))));
    }

    #[test]
    fn change_in_figures_reports_area_and_lines() {
        let a = examples::find("gable").unwrap().project();
        let mut b = a.clone();
        for e in &mut b.parts[0].edges {
            e.overhang += 0.3;
        }
        let d = change_in_figures(Some(&a), &b).unwrap();
        let area = d.iter().find(|x| x.key == "roof_area").unwrap();
        assert!(area.current > area.stored);
        assert!(d.iter().any(|x| x.key == "line:T"));
        assert!(change_in_figures(Some(&a), &a).unwrap().is_empty());
        let created = change_in_figures(None, &a).unwrap();
        assert_eq!(created[0].stored, 0.0);
    }

    #[test]
    fn leaving_draft_stores_the_result_and_a_change_shows_up() {
        let mut p = examples::find("gable").unwrap().project();
        assert!(p.result_snapshot.is_none());
        assert!(p.set_status(JobStatus::Offer));
        assert!(!p.set_status(JobStatus::Ordered), "kept once stored");
        let s = status(&p).unwrap();
        assert!(s.differences.is_empty());
        assert!(!s.engine_changed);
        p.parts[0].edges[0].pitch_deg += 5.0;
        let s = status(&p).unwrap();
        assert!(s.differences.iter().any(|d| d.key == "roof_area"));
    }

    /// The cases the UI's copy of this rule (`app/src/lib/offer.ts`
    /// `snapshotDifferences`) is tested with too, so the two cannot drift
    /// (TEST-302): the keys `compare` reports, in its order.
    #[test]
    fn compare_matches_the_shared_cases() {
        let text = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/parity/snapshot_compare_cases.json"),
        )
        .unwrap();
        let file: serde_json::Value = serde_json::from_str(&text).unwrap();
        let snap = |over: &serde_json::Value| -> ResultSnapshot {
            let mut v = file["base"].clone();
            v["engine_version"] = "test".into();
            v["taken_ms"] = 0.into();
            for (k, x) in over.as_object().unwrap() {
                v[k] = x.clone();
            }
            serde_json::from_value(v).unwrap()
        };
        let cases = file["cases"].as_array().unwrap();
        assert!(cases.len() >= 10);
        for c in cases {
            let keys: Vec<String> = compare(&snap(&c["stored"]), &snap(&c["current"]))
                .into_iter()
                .map(|d| d.key)
                .collect();
            let want: Vec<String> = serde_json::from_value(c["differs"].clone()).unwrap();
            assert_eq!(keys, want, "{}", c["name"]);
        }
    }

    #[test]
    fn older_files_migrate_and_newer_ones_are_refused() {
        let p = examples::find("gable").unwrap().project();
        let mut v = serde_json::to_value(&p).unwrap();
        v["schema_version"] = 1.into();
        let back = Project::from_json(&v.to_string()).unwrap();
        assert_eq!(back.schema_version, crate::model::SCHEMA_VERSION);
        v["schema_version"] = 99.into();
        let err = Project::from_json(&v.to_string()).unwrap_err().to_string();
        assert!(err.contains("newer"), "{err}");
    }
}
