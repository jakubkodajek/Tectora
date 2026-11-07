//! How the roof's quantities were calculated, step by step, so someone
//! checking an offer can redo every number by hand: the walls each part was
//! built from, every roof face as plan area over the cosine of its pitch
//! less the openings, every line as the 3D distance of its ends, and how
//! the faces and lines add up to the totals.
//!
//! Nothing here calculates the roof: the faces, lines and areas are the
//! engine's (the same ones [`crate::report::calc`] reports). The plan area
//! and the pitch factor are derived from each face's polygon to show the
//! formula; where they do not give the engine's area, `check` says so.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::geom::{Point2, signed_area};
use crate::model::Project;
use crate::roof::{OpeningKind, RoofGeometry};

/// Faces whose formula and engine area differ by more than this, m², are
/// flagged ([`FaceStep::engine_gross`]).
const AREA_TOL: f64 = 0.01;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Breakdown {
    pub parts: Vec<PartBreakdown>,
    /// Faces added by hand: plan area over the cosine of the pitch.
    pub manual_faces: Vec<ManualFaceStep>,
    /// Area per roof material, with the faces it sums.
    pub materials: Vec<MaterialStep>,
    /// Line lengths by line code, with every line they sum.
    pub lines: Vec<LineGroup>,
    /// Roof area of all parts and hand-added faces, m².
    pub roof_area: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PartBreakdown {
    pub name: String,
    pub walls: Vec<WallInput>,
    pub faces: Vec<FaceStep>,
    /// Clad gables counted as roof area, per material, m².
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub clad: Vec<(u8, f64)>,
    /// Windows, chimneys, dormers and holes and the roof area each took away.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub openings: Vec<OpeningStep>,
    /// Sum of the faces' areas plus the clad gables, m².
    pub roof_area: f64,
    /// The part has no roof (input problem or calculation error).
    pub calculated: bool,
}

/// One wall of a part as the user entered it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WallInput {
    /// Wall number from 1, as in the editor.
    pub no: usize,
    /// Plan length of the wall line, m.
    pub length: f64,
    pub gable: bool,
    pub pitch_deg: f64,
    pub eave_height: f64,
    pub overhang: f64,
    pub material: u8,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub half_hip: bool,
    /// Number of pitch breaks above the eave.
    #[serde(skip_serializing_if = "is_zero")]
    pub breaks: usize,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub clad: bool,
}

/// One roof face: `plan_area / cos(pitch) = gross`, `gross - deducted = area`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FaceStep {
    /// Face number within its part, from 1.
    pub no: usize,
    /// Wall number from 1 the face rises from; `None` for a dormer's face.
    pub wall: Option<usize>,
    pub pitch_deg: f64,
    pub material: u8,
    /// Corners in 3D (x, y, height), m.
    pub polygon: Vec<[f64; 3]>,
    /// Area of the face projected on the plan, m².
    pub plan_area: f64,
    /// `1 / cos(pitch)`.
    pub factor: f64,
    /// `plan_area * factor`, m².
    pub gross: f64,
    /// Taken away by windows, chimneys and holes, m².
    pub deducted: f64,
    /// The face's area in the totals, m².
    pub area: f64,
    /// Set when the engine's gross area (`area + deducted`) differs from
    /// the formula by more than 1 dm²; the totals use the engine's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine_gross: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OpeningStep {
    pub kind: OpeningKind,
    pub area: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ManualFaceStep {
    pub no: usize,
    pub label: String,
    pub pitch_deg: f64,
    pub plan_area: f64,
    pub factor: f64,
    pub area: f64,
}

/// One material's area: the faces (`"part.face"`, `"clad"`, `"manual.n"`)
/// and their sum.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MaterialStep {
    pub material: u8,
    /// The project's own name for the material, if it has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub items: Vec<AreaItem>,
    pub area: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AreaItem {
    /// Part index from 0; `None` for a hand-added face.
    pub part: Option<usize>,
    /// Face number in the part (or of the hand-added faces); `None` for the
    /// part's clad gables.
    pub face: Option<usize>,
    pub area: f64,
}

/// One line code's total: `Σ lines - Σ deductions = total`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LineGroup {
    /// Line code: T eave, F ridge, G hip, K valley, OL/OR/OS verge, P shed
    /// top edge, c pitch break, S gorge, W… abutments.
    pub code: String,
    pub items: Vec<LineStep>,
    /// Pieces taken off where a chimney hole edge lies on a line, m.
    #[serde(skip_serializing_if = "is_zero_f64")]
    pub deducted: f64,
    /// Rounded to centimetres, as in the results.
    pub total: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LineStep {
    /// Part index from 0; `None` for a line drawn by hand.
    pub part: Option<usize>,
    pub a: [f64; 3],
    pub b: [f64; 3],
    /// `b - a`, m.
    pub delta: [f64; 3],
    /// `sqrt(dx² + dy² + dz²)`, or the length typed in for a hand-drawn line.
    pub length: f64,
    /// A hand-drawn line whose length was typed in.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub typed: bool,
}

fn is_zero(v: &usize) -> bool {
    *v == 0
}

fn is_zero_f64(v: &f64) -> bool {
    v.abs() < 1e-12
}

fn plan_area(poly: &[[f64; 3]]) -> f64 {
    let pts: Vec<Point2> = poly.iter().map(|p| Point2::new(p[0], p[1])).collect();
    signed_area(&pts).abs()
}

/// `1 / |cos(pitch)|`, or `None` for a (near) vertical face.
fn factor(pitch_deg: f64) -> Option<f64> {
    let c = pitch_deg.to_radians().cos().abs();
    (c > 1e-6).then(|| 1.0 / c)
}

fn group<'a>(lines: &'a mut BTreeMap<String, LineGroup>, code: &str) -> &'a mut LineGroup {
    lines.entry(code.to_string()).or_insert_with(|| LineGroup {
        code: code.to_string(),
        items: Vec::new(),
        deducted: 0.0,
        total: 0.0,
    })
}

fn round_cm(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// The step-by-step calculation of `project`'s roof.
pub fn breakdown(project: &Project) -> Breakdown {
    let geos = crate::openings::generate_project(project);
    from_geometry(project, &geos)
}

/// [`breakdown`] for geometry already generated for `project`.
pub fn from_geometry(project: &Project, geos: &[RoofGeometry]) -> Breakdown {
    let mut materials: BTreeMap<u8, Vec<AreaItem>> = BTreeMap::new();
    let mut lines: BTreeMap<String, LineGroup> = BTreeMap::new();
    let mut roof_area = 0.0;
    let mut parts = Vec::with_capacity(project.parts.len());

    for (pi, (part, geo)) in project.parts.iter().zip(geos).enumerate() {
        let outline = part.outline();
        let n = outline.len();
        let walls = part
            .edges
            .iter()
            .enumerate()
            .map(|(i, e)| WallInput {
                no: i + 1,
                length: outline[i].distance(outline[(i + 1) % n]),
                gable: e.is_gable(),
                pitch_deg: e.pitch_deg,
                eave_height: e.eave_height,
                overhang: e.overhang,
                material: e.material,
                half_hip: e.half_hip.is_some(),
                breaks: e.breaks.len(),
                clad: e.clad,
            })
            .collect();

        let mut faces = Vec::with_capacity(geo.faces.len());
        for (fi, f) in geo.faces.iter().enumerate() {
            let plan = plan_area(&f.polygon);
            let engine = f.area + f.deducted;
            // A near-vertical face has no usable pitch factor: take the
            // engine's area as the gross and show the factor as the ratio.
            let (k, gross) = match factor(f.pitch_deg) {
                Some(k) => (k, plan * k),
                None => (if plan > 1e-9 { engine / plan } else { 0.0 }, engine),
            };
            faces.push(FaceStep {
                no: fi + 1,
                wall: (f.edge < part.edges.len()).then(|| f.edge + 1),
                pitch_deg: f.pitch_deg,
                material: f.material,
                polygon: f.polygon.clone(),
                plan_area: plan,
                factor: k,
                gross,
                deducted: f.deducted,
                area: f.area,
                engine_gross: ((gross - engine).abs() > AREA_TOL).then_some(engine),
            });
            materials.entry(f.material).or_default().push(AreaItem {
                part: Some(pi),
                face: Some(fi + 1),
                area: f.area,
            });
        }

        // Clad gables, split by material where the engine knows the walls
        // (as in `RoofGeometry::quantities`).
        let mut clad: Vec<(u8, f64)> = Vec::new();
        let mut rest = geo.clad_area;
        for &(m, a) in &geo.clad_materials {
            crate::roof::add_material_area(&mut clad, m, a);
            rest -= a;
        }
        if rest.abs() > 1e-9 {
            crate::roof::add_material_area(&mut clad, crate::model::DEFAULT_MATERIAL, rest);
        }
        for &(m, a) in &clad {
            materials.entry(m).or_default().push(AreaItem {
                part: Some(pi),
                face: None,
                area: a,
            });
        }

        let part_area = geo.faces.iter().map(|f| f.area).sum::<f64>() + geo.clad_area;
        roof_area += part_area;

        for l in &geo.lines {
            let delta = [l.b[0] - l.a[0], l.b[1] - l.a[1], l.b[2] - l.a[2]];
            group(&mut lines, l.dacad_code()).items.push(LineStep {
                part: Some(pi),
                a: l.a,
                b: l.b,
                delta,
                length: l.length(),
                typed: false,
            });
        }
        for (code, len) in &geo.minus_lines {
            group(&mut lines, code).deducted += len;
        }

        parts.push(PartBreakdown {
            name: part.name.clone(),
            walls,
            faces,
            clad,
            openings: geo
                .openings
                .iter()
                .map(|o| OpeningStep {
                    kind: o.kind,
                    area: o.area,
                })
                .collect(),
            roof_area: part_area,
            calculated: !geo.faces.is_empty() || geo.clad_area != 0.0,
        });
    }

    let mut manual_faces = Vec::new();
    for (i, f) in project.manual_faces.iter().enumerate() {
        let pts = &f.points;
        let plan = if pts.len() >= 3 {
            signed_area(pts).abs()
        } else {
            0.0
        };
        let area = f.area();
        manual_faces.push(ManualFaceStep {
            no: i + 1,
            label: f.label.clone(),
            pitch_deg: f.pitch_deg,
            plan_area: plan,
            factor: factor(f.pitch_deg).unwrap_or(0.0),
            area,
        });
        if area != 0.0 {
            // A hand-added face takes the first material (see `manual_quantities`).
            materials
                .entry(crate::model::DEFAULT_MATERIAL)
                .or_default()
                .push(AreaItem {
                    part: None,
                    face: Some(i + 1),
                    area,
                });
        }
        roof_area += area;
    }

    for l in project.manual_lines.iter().filter(|l| l.counted()) {
        let delta = [l.b[0] - l.a[0], l.b[1] - l.a[1], l.b[2] - l.a[2]];
        group(&mut lines, &l.code).items.push(LineStep {
            part: None,
            a: l.a,
            b: l.b,
            delta,
            length: l.length(),
            typed: l.length.is_some(),
        });
    }

    let mut lines: Vec<LineGroup> = lines.into_values().collect();
    for g in &mut lines {
        g.total = round_cm(g.items.iter().map(|l| l.length).sum::<f64>() - g.deducted);
    }

    let materials = materials
        .into_iter()
        .map(|(m, items)| MaterialStep {
            material: m,
            name: project
                .material_names
                .get(usize::from(m.max(1) - 1))
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(str::to_string),
            area: items.iter().map(|i| i.area).sum(),
            items,
        })
        .collect();

    Breakdown {
        parts,
        manual_faces,
        materials,
        lines,
        roof_area,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::examples;
    use crate::model::{ManualFace, ManualLine};
    use crate::quantities::{dacad_line_totals, roof_totals};

    fn assert_matches_totals(p: &Project) {
        let b = breakdown(p);
        let t = roof_totals(p);
        assert!(
            (b.roof_area - t.roof_area).abs() < 1e-9,
            "{} vs {}",
            b.roof_area,
            t.roof_area
        );
        let by: Vec<(u8, f64)> = b.materials.iter().map(|m| (m.material, m.area)).collect();
        assert_eq!(by.len(), t.area_by_material.len());
        for ((m1, a1), (m2, a2)) in by.iter().zip(&t.area_by_material) {
            assert_eq!(m1, m2);
            assert!((a1 - a2).abs() < 1e-9);
        }
        let codes = dacad_line_totals(p);
        let mine: BTreeMap<String, f64> =
            b.lines.iter().map(|g| (g.code.clone(), g.total)).collect();
        assert_eq!(mine, codes);
        for part in &b.parts {
            let sum: f64 = part.faces.iter().map(|f| f.area).sum::<f64>()
                + part.clad.iter().map(|c| c.1).sum::<f64>();
            assert!((sum - part.roof_area).abs() < 1e-9);
            for f in &part.faces {
                assert!(
                    f.engine_gross.is_none(),
                    "{}: face {} formula {} vs engine {:?}",
                    part.name,
                    f.no,
                    f.gross,
                    f.engine_gross
                );
                assert!((f.gross - f.deducted - f.area).abs() < AREA_TOL);
            }
        }
    }

    #[test]
    fn every_example_adds_up_to_the_results() {
        for ex in examples::EXAMPLES {
            assert_matches_totals(&ex.project());
        }
    }

    #[test]
    fn gable_face_is_plan_area_over_cos_pitch() {
        let p = examples::find("gable").unwrap().project();
        let b = breakdown(&p);
        let part = &b.parts[0];
        assert!(part.calculated);
        assert_eq!(part.walls.len(), p.parts[0].edges.len());
        let f = &part.faces[0];
        let k = 1.0 / f.pitch_deg.to_radians().cos();
        assert!((f.factor - k).abs() < 1e-12);
        assert!((f.plan_area * k - f.area).abs() < AREA_TOL);
        assert!(f.wall.is_some());
        for g in &b.lines {
            for l in &g.items {
                let d = l.delta;
                let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                assert!((len - l.length).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn windows_and_chimneys_are_deducted_per_face() {
        let mut p = examples::find("gable").unwrap().project();
        p.windows.push(crate::model::RoofWindow {
            x: 3.0,
            y: 1.0,
            width: 0.78,
            height: 1.4,
        });
        let b = breakdown(&p);
        let deducted: f64 = b.parts[0].faces.iter().map(|f| f.deducted).sum();
        assert!((deducted - 0.78 * 1.4).abs() < 1e-9, "{deducted}");
        assert_eq!(b.parts[0].openings.len(), 1);
        assert_matches_totals(&p);
    }

    #[test]
    fn hand_added_lines_and_faces_are_listed() {
        let mut p = examples::find("gable").unwrap().project();
        p.manual_lines.push(ManualLine {
            a: [0.0, 0.0, 0.0],
            b: [3.0, 4.0, 0.0],
            code: "G".into(),
            length: None,
            label: String::new(),
        });
        p.manual_lines.push(ManualLine {
            a: [0.0, 0.0, 0.0],
            b: [1.0, 0.0, 0.0],
            code: "T".into(),
            length: Some(2.5),
            label: String::new(),
        });
        p.manual_faces.push(ManualFace {
            points: vec![
                Point2::new(0.0, 0.0),
                Point2::new(2.0, 0.0),
                Point2::new(2.0, 3.0),
            ],
            pitch_deg: 60.0,
            label: "Stříška".into(),
        });
        let b = breakdown(&p);
        let hip = b.lines.iter().find(|g| g.code == "G").unwrap();
        assert!(
            hip.items
                .iter()
                .any(|l| l.part.is_none() && l.length == 5.0)
        );
        let eave = b.lines.iter().find(|g| g.code == "T").unwrap();
        assert!(eave.items.iter().any(|l| l.typed && l.length == 2.5));
        let mf = &b.manual_faces[0];
        assert!((mf.plan_area - 3.0).abs() < 1e-9 && (mf.area - 6.0).abs() < 1e-9);
        assert_matches_totals(&p);
    }

    #[test]
    fn a_part_without_roof_is_marked() {
        let mut p = examples::find("gable").unwrap().project();
        p.parts[0].edges[1].pitch_deg = 0.0;
        let b = breakdown(&p);
        assert!(!b.parts[0].calculated);
        assert!(b.parts[0].faces.is_empty());
    }
}
