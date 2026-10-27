//! Plan quantities that do not need the generated roof surface.
//! Roof areas, ridges, hips and valleys come with the straight-skeleton step.

use serde::Serialize;

use crate::geom::{perimeter, signed_area};
use crate::model::Project;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlanQuantities {
    /// Footprint area of all parts (wall lines), m².
    pub footprint_area: f64,
    /// Length of all wall lines, m.
    pub wall_length: f64,
    /// Length of wall lines that carry an eave (non-gable), m.
    pub eave_length: f64,
    /// Length of gable wall lines, m.
    pub gable_length: f64,
    pub parts: usize,
    pub dormers: usize,
    pub chimneys: usize,
}

pub fn plan_quantities(project: &Project) -> PlanQuantities {
    let mut q = PlanQuantities {
        footprint_area: 0.0,
        wall_length: 0.0,
        eave_length: 0.0,
        gable_length: 0.0,
        parts: project.parts.len(),
        dormers: project.dormers.len(),
        chimneys: project.chimneys.len(),
    };
    for part in &project.parts {
        let pts = part.outline();
        q.footprint_area += signed_area(&pts).abs();
        q.wall_length += perimeter(&pts);
        let n = pts.len();
        for (i, e) in part.edges.iter().enumerate() {
            let len = pts[i].distance(pts[(i + 1) % n]);
            if e.is_gable() {
                q.gable_length += len;
            } else {
                q.eave_length += len;
            }
        }
    }
    q
}

/// Roof quantities summed over all parts.
pub fn roof_totals(project: &Project) -> crate::roof::RoofQuantities {
    let mut total = crate::roof::RoofQuantities::default();
    for geo in crate::openings::generate_project(project) {
        total.add(&geo.quantities());
    }
    total
}

/// Line lengths keyed by DACAD's line codes (see `LineKind::dacad_code`),
/// rounded to centimetres.
pub fn dacad_line_totals(project: &Project) -> std::collections::BTreeMap<String, f64> {
    let mut totals: std::collections::BTreeMap<String, f64> = Default::default();
    for geo in crate::openings::generate_project(project) {
        for line in geo.lines {
            *totals.entry(line.dacad_code().to_string()).or_default() += line.length();
        }
    }
    for v in totals.values_mut() {
        *v = (*v * 100.0).round() / 100.0;
    }
    totals
}

/// Measuring standard for openings and line deductions, as in DACAD's
/// settings (DACAD keeps it per installation, not per project).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Norm {
    /// DIN 18338 (Germany).
    Din,
    /// ÖNORM B 2219 (Austria).
    Onorm,
    /// SIA (Switzerland).
    Sia,
}

impl Norm {
    /// Openings up to this area are not deducted, m².
    fn area_limit(self) -> f64 {
        match self {
            Norm::Din => 2.5,
            Norm::Onorm => 4.0,
            Norm::Sia => 1.0,
        }
    }

    /// Line deductions up to this length are ignored, m.
    fn line_limit(self) -> f64 {
        match self {
            Norm::Din => 1.0,
            Norm::Onorm => 2.0,
            Norm::Sia => 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NormTotals {
    pub norm: Norm,
    pub roof_area: f64,
    /// Line lengths by DACAD code, rounded to centimetres.
    pub lines_by_type: std::collections::BTreeMap<String, f64>,
}

/// Roof area and lines measured to a norm, as DACAD's "-Norm" groups
/// (cases C24–C38): an opening at or below the norm's area limit is not
/// deducted; under DIN that also holds for the roof under a small dormer.
/// A line piece removed by a chimney or dormer at or below the line limit
/// is not deducted (DIN 1 m, ÖNORM 2 m, SIA none).
pub fn norm_totals(project: &Project, norm: Norm) -> NormTotals {
    const EPS: f64 = 1e-6;
    let mut area = 0.0;
    let mut lines: std::collections::BTreeMap<String, f64> = Default::default();
    for geo in crate::openings::generate_project(project) {
        area += geo.faces.iter().map(|f| f.area).sum::<f64>() + geo.clad_area;
        for o in &geo.openings {
            let counts = match o.kind {
                crate::roof::OpeningKind::Dormer => norm == Norm::Din,
                _ => true,
            };
            if counts && o.area <= norm.area_limit() + EPS {
                area += o.area;
            }
        }
        for line in &geo.lines {
            *lines.entry(line.dacad_code().to_string()).or_default() += line.length();
        }
        for (code, len) in &geo.cut_lines {
            if *len <= norm.line_limit() + EPS {
                *lines.entry(code.clone()).or_default() += len;
            }
        }
    }
    for v in lines.values_mut() {
        *v = (*v * 100.0).round() / 100.0;
    }
    NormTotals {
        norm,
        roof_area: area,
        lines_by_type: lines,
    }
}
