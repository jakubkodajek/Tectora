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
