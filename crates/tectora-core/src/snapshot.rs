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
    // A part the core gives no roof for counts 0 m²: storing that as the
    // offer's figures would hide the problem (review ERR-003, DR-001).
    let failed = crate::model::guarded(|| {
        crate::openings::generate_project(project)
            .iter()
            .position(|g| g.error.is_some())
    })
    .map_err(Refused::Internal)?;
    if let Some(i) = failed {
        return Err(Refused::NotCalculated(format!(
            "part {} has no roof",
            i + 1
        )));
    }
    crate::model::guarded(|| take(project)).map_err(Refused::Internal)
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
    let t = crate::quantities::roof_totals(project);
    ResultSnapshot {
        engine_version: ENGINE_VERSION.to_string(),
        taken_ms: crate::store::now_ms(),
        roof_area: r2(t.roof_area),
        ridge_height: r2(t.ridge_height),
        lines_by_code: crate::quantities::dacad_line_totals(project)
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

/// The stored result against the current one, for the app and the agents.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SnapshotStatus {
    pub stored: ResultSnapshot,
    pub differences: Vec<Difference>,
    /// The stored result came from another engine version.
    pub engine_changed: bool,
}

pub fn status(project: &Project) -> Option<SnapshotStatus> {
    let stored = project.result_snapshot.clone()?;
    let current = take(project);
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
