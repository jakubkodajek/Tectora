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

fn r2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// The project's current result.
/// [`take`] for input from outside: refuses a project out of range
/// (`Err(Refused::OutOfRange)`) and turns a kernel panic into
/// `Err(Refused::Internal)` instead of aborting the caller.
pub fn take_checked(project: &Project) -> Result<ResultSnapshot, Refused> {
    project.check_limits().map_err(Refused::OutOfRange)?;
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
            None => Ok(take_of(&geos, project)),
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
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refused::OutOfRange(d) => write!(f, "out of range: {d}"),
            Refused::Internal(d) => f.write_str(d),
            Refused::NotCalculated(d) => write!(f, "the roof cannot be calculated: {d}"),
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
pub const REFRESHED_PREFIX: &str = "Stored result recalculated";

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
    if compare(stored, &current).is_empty() {
        return None;
    }
    let summary = format!(
        "{REFRESHED_PREFIX} (Tectora {} -> {}): roof area {:.2} -> {:.2} m2",
        stored.engine_version, ENGINE_VERSION, stored.roof_area, current.roof_area
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
