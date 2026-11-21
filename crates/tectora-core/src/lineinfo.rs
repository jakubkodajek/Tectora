//! Details of every calculated roof line (ridge, eave, hip, valley, verge,
//! …): its code, length, heights of its ends, its own slope and the roof
//! faces it bounds with their pitches. The editor shows them when the
//! pointer is on a line in the plan or in 3D, and the MCP tool
//! `line_details` gives the same figures to AI clients.
//!
//! Display only: nothing here changes the roof. The lines are the core's
//! ([`RoofGeometry::lines`]); a line belongs to a face when its middle lies
//! on that face (the rule [`crate::quantities::lines_by_material`] uses).
//!
//! [`RoofGeometry::lines`]: crate::roof::RoofGeometry::lines

use serde::Serialize;

use crate::model::Project;
use crate::roof::{LineKind, RoofGeometry};

/// A roof face a line bounds or lies on.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LineFace {
    /// The part (building) the face belongs to, index from 0.
    pub part: usize,
    /// The face's index in that part's faces.
    pub face: usize,
    /// The wall the face rises from, index from 0; `None` for a dormer's
    /// own faces.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wall: Option<usize>,
    pub pitch_deg: f64,
    /// Roof material number of the face (1 tiles, 2 sheet metal, …).
    pub material: u8,
}

/// One calculated roof line with what the editor shows about it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LineDetail {
    /// The part whose roof the line belongs to, index from 0.
    pub part: usize,
    /// The line's index in that part's `geometry.lines`.
    pub index: usize,
    pub kind: LineKind,
    /// DACAD's line code (T, F, G, K, OL, OR, OS, P, c, S, W, WV, WH, WS).
    pub code: &'static str,
    pub a: [f64; 3],
    pub b: [f64; 3],
    /// True length, √(Δx² + Δy² + Δz²), m.
    pub length: f64,
    /// Length in plan, m.
    pub plan_length: f64,
    /// The line's own inclination against the horizontal, degrees (0 for a
    /// ridge or an eave, the hip's slope for a hip).
    pub slope_deg: f64,
    /// Height of the lower and the higher end above the ground, m.
    pub z_low: f64,
    pub z_high: f64,
    /// The faces the line bounds or lies on, in part and face order.
    pub faces: Vec<LineFace>,
}

/// Every line of roofs already calculated
/// (`openings::generate_project(project)`), part by part in line order.
/// O(lines · faces · vertices).
pub fn line_details_of(project: &Project, geos: &[RoofGeometry]) -> Vec<LineDetail> {
    let mut out = Vec::new();
    for (pi, geo) in geos.iter().enumerate() {
        for (li, l) in geo.lines.iter().enumerate() {
            let (dx, dy, dz) = (l.b[0] - l.a[0], l.b[1] - l.a[1], l.b[2] - l.a[2]);
            let plan = dx.hypot(dy);
            let mid = [
                (l.a[0] + l.b[0]) / 2.0,
                (l.a[1] + l.b[1]) / 2.0,
                (l.a[2] + l.b[2]) / 2.0,
            ];
            let faces = geos
                .iter()
                .enumerate()
                .flat_map(|(fp, g)| g.faces.iter().enumerate().map(move |(fi, f)| (fp, fi, f)))
                .filter(|(_, _, f)| crate::quantities::bounds(f, mid))
                .map(|(fp, fi, f)| LineFace {
                    part: fp,
                    face: fi,
                    wall: project
                        .parts
                        .get(fp)
                        .filter(|p| f.edge < p.edges.len())
                        .map(|_| f.edge),
                    pitch_deg: f.pitch_deg,
                    material: f.material,
                })
                .collect();
            out.push(LineDetail {
                part: pi,
                index: li,
                kind: l.kind,
                code: l.dacad_code(),
                a: l.a,
                b: l.b,
                length: l.length(),
                plan_length: plan,
                slope_deg: if plan < 1e-9 && dz.abs() < 1e-9 {
                    0.0
                } else {
                    dz.abs().atan2(plan).to_degrees()
                },
                z_low: l.a[2].min(l.b[2]),
                z_high: l.a[2].max(l.b[2]),
                faces,
            });
        }
    }
    out
}

/// [`line_details_of`] of the project's roofs.
pub fn line_details(project: &Project) -> Vec<LineDetail> {
    line_details_of(project, &crate::openings::generate_project(project))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::examples;
    use approx::assert_relative_eq;

    /// Hip roof 12 × 8 m, 35°, eave 3 m at the 0.5 m overhang: plan 13 × 9,
    /// ridge 13 − 9 = 4 m long at 3 + 4.5 · tan 35°, each hip from an eave
    /// corner to a ridge end, 4.5 m in x and y below the ridge.
    #[test]
    fn hip_roof_lines_have_closed_form_heights_and_faces() {
        let p = examples::find("hip").unwrap().project();
        let d = line_details(&p);
        let ridge_z = 3.0 + 4.5 * 35f64.to_radians().tan();

        let ridges: Vec<_> = d.iter().filter(|l| l.code == "F").collect();
        assert_eq!(ridges.len(), 1);
        let r = ridges[0];
        assert_relative_eq!(r.length, 4.0, epsilon = 1e-3);
        assert_relative_eq!(r.z_low, ridge_z, epsilon = 1e-3);
        assert_relative_eq!(r.z_high, ridge_z, epsilon = 1e-3);
        assert_relative_eq!(r.slope_deg, 0.0, epsilon = 1e-3);
        // Between the two long faces, both 35°.
        assert_eq!(r.faces.len(), 2, "{:?}", r.faces);
        assert!(r.faces.iter().all(|f| (f.pitch_deg - 35.0).abs() < 1e-6));

        let hips: Vec<_> = d.iter().filter(|l| l.code == "G").collect();
        assert_eq!(hips.len(), 4);
        let plan = 4.5f64 * 2f64.sqrt();
        let rise = ridge_z - 3.0;
        for h in hips {
            assert_relative_eq!(h.plan_length, plan, epsilon = 1e-3);
            assert_relative_eq!(h.length, plan.hypot(rise), epsilon = 1e-3);
            assert_relative_eq!(h.slope_deg, rise.atan2(plan).to_degrees(), epsilon = 1e-3);
            assert_relative_eq!(h.z_low, 3.0, epsilon = 1e-3);
            assert_relative_eq!(h.z_high, ridge_z, epsilon = 1e-3);
            // A hip lies between a long face and an end face.
            assert_eq!(h.faces.len(), 2, "{:?}", h.faces);
            assert_ne!(h.faces[0].wall, h.faces[1].wall);
        }

        // Eaves: level at 3 m, one face each, the wall it belongs to.
        let eaves: Vec<_> = d.iter().filter(|l| l.code == "T").collect();
        assert_eq!(eaves.len(), 4);
        for e in eaves {
            assert_relative_eq!(e.z_low, 3.0, epsilon = 1e-3);
            assert_relative_eq!(e.z_high, 3.0, epsilon = 1e-3);
            assert_eq!(e.faces.len(), 1, "{:?}", e.faces);
            assert!(e.faces[0].wall.is_some());
        }
    }

    /// The details list every line of the result, in its order, with the
    /// same length the result gives it.
    #[test]
    fn every_line_of_every_example_is_listed_once() {
        for ex in examples::EXAMPLES {
            let p = ex.project();
            let geos = crate::openings::generate_project(&p);
            let d = line_details_of(&p, &geos);
            let n: usize = geos.iter().map(|g| g.lines.len()).sum();
            assert_eq!(d.len(), n, "{}", ex.id);
            for l in &d {
                let line = &geos[l.part].lines[l.index];
                assert_eq!(l.code, line.dacad_code(), "{}", ex.id);
                assert_relative_eq!(l.length, line.length(), epsilon = 1e-12);
                assert!(l.z_low <= l.z_high);
                assert!(l.length.is_finite() && l.slope_deg.is_finite());
                // Every roof line bounds at least one face.
                assert!(!l.faces.is_empty(), "{} line {:?} has no face", ex.id, line);
            }
        }
    }

    /// A gable roof's verge rises with the roof: from the eave up to the
    /// ridge at the face's own pitch (the line lies in a 40° plane and runs
    /// straight up its slope).
    #[test]
    fn a_verge_rises_at_the_face_pitch() {
        let p = examples::find("gable").unwrap().project();
        let d = line_details(&p);
        let verges: Vec<_> = d.iter().filter(|l| l.code.starts_with('O')).collect();
        assert_eq!(verges.len(), 4);
        for v in verges {
            assert_relative_eq!(v.slope_deg, 40.0, epsilon = 1e-3);
            assert_eq!(v.faces.len(), 1);
            assert_relative_eq!(v.faces[0].pitch_deg, 40.0, epsilon = 1e-9);
        }
    }
}
