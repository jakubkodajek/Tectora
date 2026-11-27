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

/// The quantities of one part's roof as the totals count them: line
/// lengths net of the negative lines DACAD books where a chimney or
/// opening edge lies on a roof line ([`RoofGeometry::minus_lines`]).
///
/// [`RoofGeometry::minus_lines`]: crate::roof::RoofGeometry::minus_lines
pub fn part_quantities(geo: &crate::roof::RoofGeometry) -> crate::roof::RoofQuantities {
    let mut q = geo.quantities();
    subtract_minus_lines(&mut q, &geo.minus_lines);
    q
}

/// Roof quantities summed over all parts.
pub fn roof_totals(project: &Project) -> crate::roof::RoofQuantities {
    roof_totals_of(&crate::openings::generate_project(project), project)
}

/// [`roof_totals`] of roofs already calculated
/// (`openings::generate_project(project)`), so a caller that needs the
/// roofs too calculates them once.
pub fn roof_totals_of(
    geos: &[crate::roof::RoofGeometry],
    project: &Project,
) -> crate::roof::RoofQuantities {
    let mut total = crate::roof::RoofQuantities::default();
    for geo in geos {
        total.add(&part_quantities(geo));
    }
    total.add(&manual_quantities(project));
    total
}

/// Roof area per pitch, m², pitches rounded to 0.01° and sorted. Offers
/// list the roof area this way (one figure per pitch), so it is what an
/// offer or a drawing's area table is checked against. Hand-drawn faces
/// count under their own pitch and clad gables (walls carrying the roof
/// material) under 90°, so the figures add up to the roof area (audit
/// 2026-11-21 HUNT-203).
pub fn area_by_pitch(project: &Project) -> Vec<(f64, f64)> {
    area_by_pitch_in(&crate::openings::generate_project(project), project)
}

/// The calculated roof faces alone per pitch, m² (no clad gables, no
/// hand-drawn faces): the core's own figures, as the calculation checks
/// compare them.
pub fn area_by_pitch_of(geos: &[crate::roof::RoofGeometry]) -> Vec<(f64, f64)> {
    let mut by: std::collections::BTreeMap<i64, f64> = Default::default();
    for geo in geos {
        for f in &geo.faces {
            *by.entry((f.pitch_deg * 100.0).round() as i64).or_default() += f.area;
        }
    }
    by.into_iter()
        .map(|(p, a)| (p as f64 / 100.0, (a * 100.0).round() / 100.0))
        .collect()
}

/// [`area_by_pitch`] of roofs already calculated.
pub fn area_by_pitch_in(geos: &[crate::roof::RoofGeometry], project: &Project) -> Vec<(f64, f64)> {
    let mut by: std::collections::BTreeMap<i64, f64> = Default::default();
    let key = |pitch: f64| (pitch * 100.0).round() as i64;
    for geo in geos {
        for f in &geo.faces {
            *by.entry(key(f.pitch_deg)).or_default() += f.area;
        }
        if geo.clad_area != 0.0 {
            *by.entry(key(90.0)).or_default() += geo.clad_area;
        }
    }
    for f in &project.manual_faces {
        let a = f.area();
        if a != 0.0 {
            *by.entry(key(f.pitch_deg)).or_default() += a;
        }
    }
    by.into_iter()
        .map(|(p, a)| (p as f64 / 100.0, (a * 100.0).round() / 100.0))
        .collect()
}

/// How much of a job's roof the calculation gave, the one rule the CLI,
/// MCP `calc` and the library card follow (audit 2026-11-21 ERR-202,
/// HUNT-204). Hand-drawn faces are roof as much as a building is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RoofCoverage {
    /// No building and no hand-drawn face: nothing to calculate.
    NoBuilding,
    /// Every building is without a roof and there is no hand-drawn face.
    None,
    /// Some buildings are without a roof (their figures are missing), the
    /// rest or the hand-drawn faces are calculated.
    Partial,
    /// Every building has its roof and nothing is left out.
    Complete,
}

impl RoofCoverage {
    /// The coverage of a job whose `parts_without_roof` of its buildings
    /// have no roof. A chimney, window, opening or dormer the calculation
    /// leaves out (`Project::problems`) makes a roof `Partial` too: its
    /// figures miss it, as `partial` says (audit 2026-11-22 ERR-305).
    pub fn of(project: &Project, parts_without_roof: usize) -> RoofCoverage {
        match RoofCoverage::from_counts(
            project.parts.len(),
            parts_without_roof,
            project.manual_faces.len(),
        ) {
            RoofCoverage::Complete if !project.problems().is_empty() => RoofCoverage::Partial,
            c => c,
        }
    }

    /// The coverage of `parts` buildings, `failed` of them without a roof,
    /// and `faces` hand-drawn faces.
    pub fn from_counts(parts: usize, failed: usize, faces: usize) -> RoofCoverage {
        if parts == 0 && faces == 0 {
            RoofCoverage::NoBuilding
        } else if failed == 0 {
            RoofCoverage::Complete
        } else if failed >= parts && faces == 0 {
            RoofCoverage::None
        } else {
            RoofCoverage::Partial
        }
    }
}

/// The buildings of `project` without a roof in `geos`: those the core gave
/// none for and those whose input left them out of the calculation
/// (`Project::problems` of the part).
pub fn parts_without_roof(geos: &[crate::roof::RoofGeometry], project: &Project) -> usize {
    let left_out: std::collections::BTreeSet<usize> = project
        .problems()
        .iter()
        .filter(|p| p.item == crate::model::ProblemItem::Part)
        .map(|p| p.index)
        .collect();
    (0..project.parts.len())
        .filter(|i| left_out.contains(i) || geos.get(*i).is_some_and(|g| g.error.is_some()))
        .count()
}

/// Takes the line pieces DACAD subtracts (`minus_lines`, e.g. a ridge
/// through a chimney) off the line lengths, as `dacad_line_totals` does, so
/// the totals by line type agree with the totals by line code.
pub fn subtract_minus_lines(q: &mut crate::roof::RoofQuantities, minus: &[(String, f64)]) {
    for (code, len) in minus {
        let field = match code.as_str() {
            "T" => &mut q.eave_length,
            "F" => &mut q.ridge_length,
            "G" => &mut q.hip_length,
            "K" => &mut q.valley_length,
            "O" | "OL" | "OR" | "OS" => &mut q.verge_length,
            "P" => &mut q.pult_length,
            "c" => &mut q.knick_length,
            "S" => &mut q.gorge_length,
            _ => &mut q.abutment_length,
        };
        *field -= len;
    }
}

/// Line lengths keyed by DACAD's line codes (see `LineKind::dacad_code`),
/// rounded to centimetres.
pub fn dacad_line_totals(project: &Project) -> std::collections::BTreeMap<String, f64> {
    dacad_line_totals_of(&crate::openings::generate_project(project), project)
}

/// [`dacad_line_totals`] of roofs already calculated.
pub fn dacad_line_totals_of(
    geos: &[crate::roof::RoofGeometry],
    project: &Project,
) -> std::collections::BTreeMap<String, f64> {
    let mut totals: std::collections::BTreeMap<String, f64> = Default::default();
    for geo in geos {
        for line in &geo.lines {
            *totals.entry(line.dacad_code().to_string()).or_default() += line.length();
        }
        for (code, len) in &geo.minus_lines {
            *totals.entry(code.clone()).or_default() -= len;
        }
    }
    for line in project.manual_lines.iter().filter(|l| l.counted()) {
        *totals.entry(line.code.clone()).or_default() += line.length();
    }
    for v in totals.values_mut() {
        *v = (*v * 100.0).round() / 100.0;
    }
    totals
}

/// Line lengths per roof material, keyed by DACAD's line codes and rounded
/// to centimetres: a line counts for the material of every face it bounds,
/// so a line between tiles and sheet metal counts for both. An offer for one
/// material (tiles only, say) takes its lines from here instead of from the
/// totals, which also count the other materials' parts. These are the lines
/// as drawn: DACAD's corrections around openings (`minus_lines`) and lines
/// added by hand are only in [`dacad_line_totals`].
pub fn lines_by_material(project: &Project) -> Vec<(u8, std::collections::BTreeMap<String, f64>)> {
    lines_by_material_of(&crate::openings::generate_project(project))
}

/// [`lines_by_material`] of roofs already calculated.
pub fn lines_by_material_of(
    geos: &[crate::roof::RoofGeometry],
) -> Vec<(u8, std::collections::BTreeMap<String, f64>)> {
    let faces: Vec<&crate::roof::RoofFace> = geos.iter().flat_map(|g| &g.faces).collect();
    let mut by: std::collections::BTreeMap<u8, std::collections::BTreeMap<String, f64>> =
        Default::default();
    for line in geos.iter().flat_map(|g| &g.lines) {
        let m = [
            (line.a[0] + line.b[0]) / 2.0,
            (line.a[1] + line.b[1]) / 2.0,
            (line.a[2] + line.b[2]) / 2.0,
        ];
        let mut mats: Vec<u8> = faces
            .iter()
            .filter(|f| bounds(f, m))
            .map(|f| f.material)
            .collect();
        mats.sort_unstable();
        mats.dedup();
        for mat in mats {
            *by.entry(mat)
                .or_default()
                .entry(line.dacad_code().to_string())
                .or_default() += line.length();
        }
    }
    by.into_iter()
        .map(|(m, mut lines)| {
            for v in lines.values_mut() {
                *v = (*v * 100.0).round() / 100.0;
            }
            (m, lines)
        })
        .collect()
}

/// Whether the point `m` (the middle of a roof line) lies on `face`: on its
/// outline, or inside it in plan on the face's plane. Face outlines are not
/// cut where another roof or an opening hides them, so the lines there
/// (valleys between two roofs, the edges of an opening) lie inside the
/// faces they bound. A face that passes above or below the point does not
/// count.
pub(crate) fn bounds(face: &crate::roof::RoofFace, m: [f64; 3]) -> bool {
    const TOL: f64 = 0.02;
    let poly = &face.polygon;
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let on_outline = (0..n).any(|i| {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let l2 = dx * dx + dy * dy;
        let t = if l2 < 1e-12 {
            0.0
        } else {
            (((m[0] - a[0]) * dx + (m[1] - a[1]) * dy) / l2).clamp(0.0, 1.0)
        };
        let q = [a[0] + t * dx, a[1] + t * dy, a[2] + t * (b[2] - a[2])];
        (m[0] - q[0]).hypot(m[1] - q[1]) < TOL && (m[2] - q[2]).abs() < TOL * 2.0
    });
    if on_outline {
        return true;
    }
    // Newell's normal of the face; a vertical face has no height over a
    // plan point.
    let mut nrm = [0.0f64; 3];
    let mut c = [0.0f64; 3];
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        nrm[0] += (a[1] - b[1]) * (a[2] + b[2]);
        nrm[1] += (a[2] - b[2]) * (a[0] + b[0]);
        nrm[2] += (a[0] - b[0]) * (a[1] + b[1]);
        for k in 0..3 {
            c[k] += a[k] / n as f64;
        }
    }
    let len = (nrm[0] * nrm[0] + nrm[1] * nrm[1] + nrm[2] * nrm[2]).sqrt();
    if !len.is_finite() || len <= 1e-9 || (nrm[2] / len).abs() < 1e-3 {
        return false;
    }
    let z = c[2] - (nrm[0] * (m[0] - c[0]) + nrm[1] * (m[1] - c[1])) / nrm[2];
    if (m[2] - z).abs() >= TOL * 2.0 {
        return false;
    }
    // Even-odd test in plan.
    let mut inside = false;
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a[1] > m[1]) != (b[1] > m[1])
            && m[0] < a[0] + (m[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0])
        {
            inside = !inside;
        }
    }
    inside
}

/// Quantities of the lines and faces added by hand, to add to the roof's.
pub fn manual_quantities(project: &Project) -> crate::roof::RoofQuantities {
    let mut q = crate::roof::RoofQuantities {
        roof_area: project.manual_faces.iter().map(|f| f.area()).sum(),
        ..Default::default()
    };
    if q.roof_area != 0.0 {
        // DACAD gives a new face no wall, so it takes the first material.
        q.area_by_material
            .push((crate::model::DEFAULT_MATERIAL, q.roof_area));
    }
    for line in project.manual_lines.iter().filter(|l| l.counted()) {
        let len = line.length();
        match line.code.as_str() {
            "T" => q.eave_length += len,
            "F" => q.ridge_length += len,
            "G" => {
                q.hip_length += len;
                q.hip_count += 1;
            }
            "K" => {
                q.valley_length += len;
                q.valley_count += 1;
            }
            "OL" | "OR" | "OS" => q.verge_length += len,
            "P" => q.pult_length += len,
            "c" => q.knick_length += len,
            "S" => q.gorge_length += len,
            _ => q.abutment_length += len,
        }
    }
    q
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
    norm_totals_of(&crate::openings::generate_project(project), norm)
}

/// [`norm_totals`] of roofs already calculated.
pub fn norm_totals_of(geos: &[crate::roof::RoofGeometry], norm: Norm) -> NormTotals {
    const EPS: f64 = 1e-6;
    let mut area = 0.0;
    let mut lines: std::collections::BTreeMap<String, f64> = Default::default();
    for geo in geos {
        area += geo.faces.iter().map(|f| f.area).sum::<f64>() + geo.clad_area;
        for o in &geo.openings {
            // DACAD deducts roof openings in full under every norm (OP3/OP4).
            let counts = match o.kind {
                crate::roof::OpeningKind::Dormer => norm == Norm::Din,
                crate::roof::OpeningKind::Hole => false,
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
        // Treated like a cut piece under a norm (DACAD C28_chimney_ridge).
        for (code, len) in &geo.minus_lines {
            if *len > norm.line_limit() + EPS {
                *lines.entry(code.clone()).or_default() -= len;
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

#[cfg(test)]
mod manual_tests {
    use super::*;
    use crate::model::{ManualFace, ManualLine};

    // DACAD case DL2 (2026-10-29): a hand-drawn line typed as hip (G) on the
    // 10 x 8 gable roof counts 4.0 m in the hip totals; a dimension line
    // counts nowhere.
    #[test]
    fn manual_lines_count_in_their_type() {
        let mut p = Project::new("DL2");
        p.parts.push(crate::examples::standard_roof(
            crate::examples::StandardRoof::Gable,
            10.0,
            8.0,
            40.0,
            3.0,
            0.5,
        ));
        let before = dacad_line_totals(&p);
        p.manual_lines.push(ManualLine {
            a: [5.0, 0.0, 0.0],
            b: [5.0, 4.0, 0.0],
            code: "G".into(),
            length: None,
            label: String::new(),
        });
        p.manual_lines.push(ManualLine {
            a: [0.0, 0.0, 0.0],
            b: [1.0, 0.0, 0.0],
            code: "M".into(),
            length: None,
            label: String::new(),
        });
        let after = dacad_line_totals(&p);
        assert_eq!(
            after.get("G").copied().unwrap_or(0.0) - before.get("G").copied().unwrap_or(0.0),
            4.0
        );
        assert!(!after.contains_key("M"));
        let q = manual_quantities(&p);
        assert_eq!((q.hip_length, q.hip_count), (4.0, 1));
    }

    #[test]
    fn manual_face_area_is_plan_over_cos_pitch() {
        let f = ManualFace {
            points: vec![
                crate::geom::Point2 { x: 0.0, y: 0.0 },
                crate::geom::Point2 { x: 2.0, y: 0.0 },
                crate::geom::Point2 { x: 2.0, y: 3.0 },
            ],
            pitch_deg: 60.0,
            label: String::new(),
        };
        assert!((f.area() - 6.0).abs() < 1e-9);
        // A near-vertical pitch gives no area instead of millions of m².
        for pitch_deg in [89.99999, 90.0, 95.0, -1.0] {
            assert_eq!(
                ManualFace {
                    pitch_deg,
                    ..f.clone()
                }
                .area(),
                0.0
            );
        }
    }

    #[test]
    fn line_types_take_off_the_ridge_through_a_chimney() {
        let mut p = crate::examples::find("gable").unwrap().project();
        let before = roof_totals(&p).ridge_length;
        p.chimneys.push(crate::model::Chimney {
            corners: [(5.0, 4.0), (6.2, 4.0), (6.2, 5.0), (5.0, 5.0)]
                .map(|(x, y)| crate::geom::Point2::new(x, y))
                .to_vec(),
            height: 10.0,
        });
        let ridge = roof_totals(&p).ridge_length;
        let f = dacad_line_totals(&p)["F"];
        assert!(ridge < before - 1.0, "ridge {ridge} before {before}");
        assert!((ridge - f).abs() < 0.006, "ridge {ridge} vs F {f}");
        let report = crate::report::calc(&p);
        assert!((report["totals"]["ridge_length"].as_f64().unwrap() - ridge).abs() < 1e-9);
    }
}

#[cfg(test)]
mod material_tests {
    use super::*;

    fn split(project: &Project) -> (f64, Vec<(u8, f64)>) {
        let t = roof_totals(project);
        (t.roof_area, t.area_by_material)
    }

    #[test]
    fn one_material_takes_the_whole_area() {
        let p = crate::examples::find("l-shape").unwrap().project();
        let (area, by) = split(&p);
        assert_eq!(by.len(), 1);
        assert_eq!(by[0].0, crate::model::DEFAULT_MATERIAL);
        assert!((by[0].1 - area).abs() < 1e-9);
    }

    #[test]
    fn faces_take_their_walls_material() {
        let mut p = crate::examples::find("hip").unwrap().project();
        let before = roof_totals(&p).roof_area;
        p.parts[0].edges[0].material = 2;
        let (area, by) = split(&p);
        // The material does not change the roof, only splits its area.
        assert!((area - before).abs() < 1e-9);
        assert_eq!(by.iter().map(|e| e.0).collect::<Vec<_>>(), vec![1, 2]);
        assert!((by.iter().map(|e| e.1).sum::<f64>() - area).abs() < 1e-9);
        let face: f64 = crate::openings::generate_project(&p)[0]
            .faces
            .iter()
            .filter(|f| f.edge == 0)
            .map(|f| f.area)
            .sum();
        assert!((by[1].1 - face).abs() < 1e-9);
    }

    #[test]
    fn clad_gable_counts_for_its_material() {
        let mut p = crate::examples::find("gable").unwrap().project();
        let gable = p.parts[0].edges.iter().position(|e| e.is_gable()).unwrap();
        p.parts[0].edges[gable].clad = true;
        p.parts[0].edges[gable].material = 4;
        let geo = &crate::openings::generate_project(&p)[0];
        assert!(geo.clad_area > 0.0);
        let (area, by) = split(&p);
        let slate = by.iter().find(|e| e.0 == 4).map(|e| e.1).unwrap();
        assert!((slate - geo.clad_area).abs() < 1e-9);
        assert!((by.iter().map(|e| e.1).sum::<f64>() - area).abs() < 1e-9);
    }

    #[test]
    fn lines_count_for_the_materials_they_bound() {
        let mut p = crate::examples::find("hip").unwrap().project();
        p.parts[0].edges[0].material = 2;
        let total = dacad_line_totals(&p);
        let by = lines_by_material(&p);
        assert_eq!(by.iter().map(|e| e.0).collect::<Vec<_>>(), vec![1, 2]);
        let get = |m: usize, code: &str| by[m].1.get(code).copied().unwrap_or(0.0);
        // An eave bounds one face, so the eaves split between the materials.
        assert!((get(0, "T") + get(1, "T") - total["T"]).abs() < 0.02);
        assert!(get(1, "T") > 0.0);
        // Every hip bounds a tiled face; the two beside the sheet-metal face
        // count for both materials.
        assert!((get(0, "G") - total["G"]).abs() < 0.02);
        assert!(get(1, "G") > 0.0 && get(1, "G") < total["G"]);
    }

    #[test]
    fn every_line_counts_for_a_material() {
        // Lines inside a face (the joints of two roofs, the edges of an
        // opening) count too, so no line is lost from the split.
        for ex in crate::examples::EXAMPLES {
            let mut p = ex.project();
            for part in &mut p.parts {
                for (i, e) in part.edges.iter_mut().enumerate() {
                    e.material = 1 + (i % 2) as u8;
                }
            }
            let by = lines_by_material(&p);
            for (code, total) in dacad_line_totals(&p) {
                let sum: f64 = by.iter().filter_map(|(_, l)| l.get(&code)).sum();
                assert!(
                    sum >= total - 0.05,
                    "{}: {code} {total} split as {sum}",
                    ex.id
                );
            }
        }
    }

    #[test]
    fn material_names_default_to_dacad() {
        let mut p = Project::new("x");
        assert_eq!(p.material_name(2), "Blech");
        p.material_names = vec![String::new(), "Plech Lindab".into()];
        assert_eq!(p.material_name(1), "Ziegel");
        assert_eq!(p.material_name(2), "Plech Lindab");
        assert_eq!(p.material_name(9), "Material 9");
    }
}

#[cfg(test)]
mod audit_tests {
    use super::*;
    use crate::model::ManualLine;
    use crate::roof::{RoofFace, RoofGeometry};

    fn line(code: &str, b: [f64; 3]) -> ManualLine {
        ManualLine {
            a: [0.0; 3],
            b,
            code: code.into(),
            length: None,
            label: String::new(),
        }
    }

    /// TEST-010: one hand-drawn line per counted code lands in its own
    /// total, and hips and valleys are counted as well as measured.
    #[test]
    fn hand_drawn_lines_count_in_every_total() {
        let mut p = Project::new("hand");
        // Lengths are 3-4-5 style triples so every one is exact.
        p.manual_lines = vec![
            line("T", [3.0, 4.0, 0.0]),  // 5
            line("F", [6.0, 8.0, 0.0]),  // 10
            line("G", [0.0, 3.0, 4.0]),  // 5, hip
            line("G", [0.0, 6.0, 8.0]),  // 10, hip
            line("K", [0.0, 12.0, 5.0]), // 13, valley
            line("K", [0.0, 0.0, 2.0]),  // 2, valley
            line("K", [0.0, 0.0, 1.0]),  // 1, valley
            line("OL", [0.0, 0.0, 7.0]), // 7 verge
            line("OR", [0.0, 0.0, 1.0]), // 1 verge
            line("P", [0.0, 0.0, 4.0]),  // 4
            line("c", [0.0, 0.0, 6.0]),  // 6
            line("S", [0.0, 0.0, 8.0]),  // 8
            line("WV", [0.0, 0.0, 9.0]), // 9 abutment
            line("M", [0.0, 0.0, 50.0]), // dimension: counts nowhere
        ];
        let q = manual_quantities(&p);
        let near = |a: f64, b: f64| assert!((a - b).abs() < 1e-9, "{a} vs {b}");
        near(q.eave_length, 5.0);
        near(q.ridge_length, 10.0);
        near(q.hip_length, 15.0);
        assert_eq!(q.hip_count, 2);
        near(q.valley_length, 16.0);
        assert_eq!(q.valley_count, 3);
        near(q.verge_length, 8.0);
        near(q.pult_length, 4.0);
        near(q.knick_length, 6.0);
        near(q.gorge_length, 8.0);
        near(q.abutment_length, 9.0);
        near(q.roof_area, 0.0);
    }

    /// TEST-010: pitches that differ by 0.01 degrees are listed apart
    /// (the list is keyed to 0.01 degrees, not 0.1).
    #[test]
    fn area_by_pitch_keeps_pitches_a_hundredth_apart() {
        let face = |pitch_deg: f64, area: f64| RoofFace {
            edge: 0,
            pitch_deg,
            polygon: Vec::new(),
            area,
            deducted: 0.0,
            material: 1,
        };
        let geos = [RoofGeometry {
            faces: vec![face(33.69, 10.0), face(33.7, 20.0), face(33.69, 5.0)],
            ..Default::default()
        }];
        assert_eq!(
            area_by_pitch_in(&geos, &Project::new("x")),
            [(33.69, 15.0), (33.7, 20.0)]
        );
    }
}
