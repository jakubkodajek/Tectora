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

use crate::model::Project;
use crate::roof::{OpeningKind, RoofGeometry};

/// Faces whose formula and engine area differ by more than this, m², are
/// flagged ([`FaceStep::engine_gross`]): half a unit of the 3 decimals the
/// detailed steps show, so the steps shown always add up.
const AREA_TOL: f64 = 0.0005;

/// Decimal places the app shows for each kind of number in the detailed
/// steps. Every number shown is the exact value rounded; they are chosen
/// so that redoing a step by hand from the numbers shown gives the next
/// number shown to within one unit of its last decimal (half a unit more
/// per rounded term of a sum), and the last number of a face or line
/// rounds to the hundredths of the results. Coordinates and pitches are
/// the core's single-precision values (183.6199951172 for 183.62), shown
/// with enough decimals that products of coordinates up to a few
/// kilometres keep that promise. The test `displayed_steps_recompute`
/// checks this for every case and for roofs placed far from the origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Decimals {
    /// Corner coordinates, heights and coordinate differences (shown with
    /// trailing zeros trimmed, at least 3 decimals).
    pub coord: u32,
    /// Pitch in degrees (trimmed like `coord`).
    pub pitch: u32,
    /// Shoelace products, squares of coordinate differences and their sum.
    pub product: u32,
    /// Cross sum and plan area.
    pub plan: u32,
    pub cos: u32,
    pub factor: u32,
    /// Sloped area, deductions and the face's area in the detailed steps.
    pub area: u32,
    /// Line length in the detailed steps.
    pub length: u32,
    /// An area or length whose rounding to hundredths the shown digits
    /// would leave in doubt (e.g. 39.065 for 39.06480) is shown with more
    /// decimals, up to this many, until it rounds the same way.
    pub max: u32,
    /// The same for a sum, so that the terms as shown add up to the total
    /// as shown (equal terms, e.g. four hips, can need more decimals).
    pub sum_max: u32,
}

pub const DECIMALS: Decimals = Decimals {
    coord: 10,
    pitch: 10,
    product: 6,
    plan: 6,
    cos: 10,
    factor: 8,
    area: 3,
    length: 4,
    max: 8,
    sum_max: 12,
};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Breakdown {
    /// How many decimals the app shows in the detailed steps.
    pub decimals: Decimals,
    pub parts: Vec<PartBreakdown>,
    /// Faces added by hand: plan area over the cosine of the pitch.
    pub manual_faces: Vec<ManualFaceStep>,
    /// Area per roof material, with the faces it sums.
    pub materials: Vec<MaterialStep>,
    /// Line lengths by line code, with every line they sum.
    pub lines: Vec<LineGroup>,
    /// Roof area of all parts and hand-added faces, m².
    pub roof_area: f64,
    /// Decimals for the sum of the parts and hand-added faces (see
    /// [`sum_digits`]).
    pub digits: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PartBreakdown {
    pub name: String,
    pub walls: Vec<WallInput>,
    pub faces: Vec<FaceStep>,
    /// Clad gables counted as roof area, per material, m².
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub clad: Vec<(u8, f64)>,
    /// The clad gables one by one, with their area from the corners.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub clad_faces: Vec<CladStep>,
    /// Windows, chimneys, dormers and holes and the roof area each took away.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub openings: Vec<OpeningStep>,
    /// Area cut out of the faces where roofs are joined that is no opening
    /// (e.g. covered by another part), m². With `openings` it makes up the
    /// faces' `deducted`.
    #[serde(skip_serializing_if = "is_zero_f64")]
    pub other_cuts: f64,
    /// Σ of the faces' `deducted` (= Σ openings + `other_cuts`), m².
    pub deducted: f64,
    /// Sum of the faces' areas plus the clad gables, m².
    pub roof_area: f64,
    /// Decimals for that sum (see [`sum_digits`]).
    pub digits: u32,
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
    /// The plan area by the shoelace formula (Gauss): for corner `i` and
    /// the next one `j`, the products `[x_i * y_j, x_j * y_i]`.
    pub products: Vec<[f64; 2]>,
    /// `Σ (x_i * y_j - x_j * y_i)`, m² (twice the signed plan area).
    pub cross_sum: f64,
    /// Area of the face projected on the plan, `|cross_sum| / 2`, m².
    pub plan_area: f64,
    /// `cos(pitch)`.
    pub cos_pitch: f64,
    /// Decimals for `cos_pitch` (`Decimals::cos`, more for steep pitches).
    pub cos_digits: u32,
    /// `1 / cos(pitch)`.
    pub factor: f64,
    /// `plan_area * factor`, m².
    pub gross: f64,
    /// Check: the area of the 3D corners themselves (Newell's method),
    /// independent of the pitch, m². Equals `gross` for a plane face of
    /// that pitch.
    pub area_3d: f64,
    /// How far the corners lie off one plane: the largest vertical
    /// distance of a corner from the plane through their centre with the
    /// Newell normal, m. Explains a check that does not quite agree.
    pub off_plane: f64,
    /// Taken away by windows, chimneys and holes, m².
    pub deducted: f64,
    /// The face's area in the totals, m².
    pub area: f64,
    /// The engine's gross area (`area + deducted`), set where the formula's
    /// `gross` would not lead to `area` in the steps as shown: it differs by
    /// more than 0.0005 m² (joined roofs), or by enough to change the last
    /// digit or the hundredths (the core's single precision). The steps
    /// then continue from this value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine_gross: Option<f64>,
    /// Decimals for the gross area, deduction and area in the detailed
    /// steps (at least `Decimals::area`): enough for
    /// `gross - deducted = area` to hold as shown and for the area to
    /// round to the hundredths of the results.
    pub digits: u32,
    /// A (near) vertical face (|cos| ≤ 1e-6): the plan area says nothing
    /// about its area, so `gross` is the engine's and `factor` only the
    /// ratio `gross / plan_area`.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub vertical: bool,
}

/// A clad gable: vertical, so its area is taken in its own plane, with
/// `u` the horizontal distance along the wall from its first corner and
/// `z` the height: `½ |Σ (u_i z_j − u_j z_i)|` per piece.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CladStep {
    /// Wall number from 1, if known.
    pub wall: Option<usize>,
    pub material: u8,
    pub pieces: Vec<CladPiece>,
    /// The area counted (the core's), m².
    pub area: f64,
    /// Decimals for the areas in the detailed steps (see [`FaceStep::digits`]).
    pub digits: u32,
    /// Set when the core's area differs from `Σ pieces` by more than
    /// 0.0005 m² (something cut out of it, or a wall not quite vertical).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub formula_area: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CladPiece {
    pub polygon: Vec<[f64; 3]>,
    /// `[u, z]` of each corner, m.
    pub uz: Vec<[f64; 2]>,
    /// As [`FaceStep::products`], on `(u, z)`.
    pub products: Vec<[f64; 2]>,
    pub cross_sum: f64,
    pub area: f64,
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
    /// Plan corners (x, y), m.
    pub points: Vec<[f64; 2]>,
    /// As [`FaceStep::products`].
    pub products: Vec<[f64; 2]>,
    pub cross_sum: f64,
    pub plan_area: f64,
    pub cos_pitch: f64,
    /// As [`FaceStep::cos_digits`].
    pub cos_digits: u32,
    pub factor: f64,
    pub area: f64,
    /// Decimals for the area in the detailed steps (see [`FaceStep::digits`]).
    pub digits: u32,
    /// The pitch gives no area (outside 0°..90° or near vertical), so the
    /// face counts 0 m² (see `ManualFace::area`).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub pitch_out_of_range: bool,
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
    /// Decimals for the sum (see [`sum_digits`]).
    pub digits: u32,
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
    /// `Σ lengths - deducted`, not rounded, m.
    pub sum: f64,
    /// Decimals for the sum (see [`sum_digits`]).
    pub digits: u32,
    /// `sum` rounded to centimetres, as in the results.
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
    /// `[dx², dy², dz²]`, m².
    pub squares: [f64; 3],
    /// `dx² + dy² + dz²`, m².
    pub sum_sq: f64,
    /// `sqrt(dx² + dy² + dz²)`, or the length typed in for a hand-drawn line.
    pub length: f64,
    /// Decimals for the length in the detailed steps: enough for it to
    /// round to the hundredths of the results.
    pub digits: u32,
    /// Decimals for the squares and their sum: `Decimals::product`, more
    /// for a short line, so that the root of the sum as shown is within
    /// one unit of the length's last decimal.
    pub sq_digits: u32,
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

/// The shoelace products `[x_i * y_j, x_j * y_i]` of a plan polygon (`j`
/// the next corner) and their cross sum `Σ (x_i * y_j - x_j * y_i)`.
fn shoelace(pts: &[[f64; 2]]) -> (Vec<[f64; 2]>, f64) {
    let n = pts.len();
    if n < 3 {
        return (Vec::new(), 0.0);
    }
    let products: Vec<[f64; 2]> = (0..n)
        .map(|i| {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            [a[0] * b[1], b[0] * a[1]]
        })
        .collect();
    let sum = products.iter().map(|p| p[0] - p[1]).sum();
    (products, sum)
}

/// Area of a 3D polygon (Newell's method): half the length of
/// `Σ p_i × p_j`; exact for a plane polygon, whatever its tilt.
fn area_3d(poly: &[[f64; 3]]) -> f64 {
    let n = poly.len();
    if n < 3 {
        return 0.0;
    }
    let mut v = [0.0f64; 3];
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        v[0] += a[1] * b[2] - a[2] * b[1];
        v[1] += a[2] * b[0] - a[0] * b[2];
        v[2] += a[0] * b[1] - a[1] * b[0];
    }
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt() / 2.0
}

/// Decimals for `cos(pitch)`: `Decimals::cos`, more for a steep pitch, so
/// that `1 / cos` from the cosine as shown is within half a unit of the
/// factor's last decimal (rounding the cosine by `e` moves `1 / cos` by
/// about `e / cos²`).
fn cos_digits(pitch_deg: f64) -> u32 {
    let c = pitch_deg.to_radians().cos().abs();
    (DECIMALS.cos..=20)
        .find(|&d| 10f64.powi(-(d as i32)) <= c * c * 10f64.powi(-(DECIMALS.factor as i32)))
        .unwrap_or(20)
}

/// `1 / |cos(pitch)|`, or `None` for a (near) vertical face.
fn factor(pitch_deg: f64) -> Option<f64> {
    let c = pitch_deg.to_radians().cos().abs();
    (c > 1e-6).then(|| 1.0 / c)
}

/// The largest vertical distance of a corner from the plane through the
/// corners' centre with the Newell normal; 0 for a vertical polygon.
fn off_plane(poly: &[[f64; 3]]) -> f64 {
    let n = poly.len();
    if n < 3 {
        return 0.0;
    }
    let mut v = [0.0f64; 3];
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        v[0] += a[1] * b[2] - a[2] * b[1];
        v[1] += a[2] * b[0] - a[0] * b[2];
        v[2] += a[0] * b[1] - a[1] * b[0];
    }
    if v[2].abs() < 1e-12 {
        return 0.0;
    }
    let c = [0, 1, 2].map(|k| poly.iter().map(|p| p[k]).sum::<f64>() / n as f64);
    poly.iter()
        .map(|p| {
            ((p[0] - c[0]) * v[0] + (p[1] - c[1]) * v[1] + (p[2] - c[2]) * v[2]).abs() / v[2].abs()
        })
        .fold(0.0, f64::max)
}

/// A vertical piece in its own plane: `u` along the wall from the first
/// corner (towards the corner farthest away in plan), `z` the height.
fn clad_piece(poly: &[[f64; 3]]) -> CladPiece {
    let o = poly.first().copied().unwrap_or([0.0; 3]);
    let far = poly
        .iter()
        .map(|p| [p[0] - o[0], p[1] - o[1]])
        .max_by(|a, b| a[0].hypot(a[1]).total_cmp(&b[0].hypot(b[1])))
        .unwrap_or([1.0, 0.0]);
    let len = far[0].hypot(far[1]);
    let dir = if len > 1e-12 {
        [far[0] / len, far[1] / len]
    } else {
        [1.0, 0.0]
    };
    let uz: Vec<[f64; 2]> = poly
        .iter()
        .map(|p| [(p[0] - o[0]) * dir[0] + (p[1] - o[1]) * dir[1], p[2]])
        .collect();
    let (products, cross_sum) = shoelace(&uz);
    CladPiece {
        polygon: poly.to_vec(),
        uz,
        products,
        cross_sum,
        area: cross_sum.abs() / 2.0,
    }
}

fn round_to(v: f64, d: u32) -> f64 {
    let k = 10f64.powi(d as i32);
    (v * k).round() / k
}

/// Whether `base - deducted = area` holds as shown with `d` decimals:
/// within one unit of the last decimal, and rounding to the same
/// hundredths as the area.
fn adds_up_at(base: f64, deducted: f64, area: f64, d: u32) -> bool {
    let shown = round_to(base, d) - round_to(deducted, d);
    (shown - round_to(area, d)).abs() <= 10f64.powi(-(d as i32)) + 1e-12
        && round_to(shown, 2) == round_to(area, 2)
        && round_to(round_to(area, d), 2) == round_to(area, 2)
        && (deducted.abs() > 1e-12 || round_to(round_to(base, d), 2) == round_to(area, 2))
}

/// The fewest decimals (from `Decimals::area` to `Decimals::max`) at which
/// the face's last step adds up as shown.
fn digits_for(base: f64, deducted: f64, area: f64) -> Option<u32> {
    (DECIMALS.area..=DECIMALS.max).find(|&d| adds_up_at(base, deducted, area, d))
}

/// Whether the formula's gross leads to the face's area in the steps shown.
fn steps_add_up(gross: f64, deducted: f64, area: f64) -> bool {
    (gross - deducted - area).abs() <= AREA_TOL && digits_for(gross, deducted, area).is_some()
}

fn area_digits(base: f64, deducted: f64, area: f64) -> u32 {
    digits_for(base, deducted, area).unwrap_or(DECIMALS.max)
}

/// The fewest decimals (from `Decimals::area` to `Decimals::sum_max`) at
/// which a sum adds up as shown: the terms rounded add up to the total
/// rounded, and the total rounds to its hundredths. 0 when none does (the
/// app then shows `sum_max` decimals and "≈" instead of "=").
pub fn sum_digits(terms: &[f64], total: f64) -> u32 {
    (DECIMALS.area..=DECIMALS.sum_max)
        .find(|&d| {
            let shown: f64 = terms.iter().map(|&t| round_to(t, d)).sum();
            round_to(shown, d) == round_to(total, d)
                && round_to(round_to(total, d), 2) == round_to(total, 2)
        })
        .unwrap_or(0)
}

/// Clad area the producer did not list gable by gable (none today).
fn clad_rest_terms(geo: &RoofGeometry) -> Option<f64> {
    let listed: f64 = geo.clad_faces.iter().map(|c| c.area).sum();
    let rest = geo.clad_area - listed;
    (rest.abs() > 1e-12).then_some(rest)
}

fn group<'a>(lines: &'a mut BTreeMap<String, LineGroup>, code: &str) -> &'a mut LineGroup {
    lines.entry(code.to_string()).or_insert_with(|| LineGroup {
        code: code.to_string(),
        items: Vec::new(),
        deducted: 0.0,
        sum: 0.0,
        digits: DECIMALS.area,
        total: 0.0,
    })
}

fn line_step(part: Option<usize>, a: [f64; 3], b: [f64; 3], length: f64, typed: bool) -> LineStep {
    let delta = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let squares = delta.map(|d| d * d);
    let digits = (DECIMALS.length..=DECIMALS.max)
        .find(|&d| round_to(round_to(length, d), 2) == round_to(length, 2))
        .unwrap_or(DECIMALS.max);
    // Rounding the sum by half a unit of 10^-sq moves its root by about
    // 10^-sq / (4 L): keep that within half a unit of the length.
    let sq_digits = if length.abs() < 1e-9 {
        DECIMALS.product
    } else {
        (DECIMALS.product..=DECIMALS.sum_max)
            .find(|&d| 10f64.powi(-(d as i32)) <= 2.0 * length.abs() * 10f64.powi(-(digits as i32)))
            .unwrap_or(DECIMALS.sum_max)
    };
    LineStep {
        part,
        a,
        b,
        delta,
        squares,
        sum_sq: squares.iter().sum(),
        length,
        digits,
        sq_digits,
        typed,
    }
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
        for f in &geo.faces {
            // A face without corners or area (a piece the join left empty)
            // explains nothing.
            if f.polygon.len() < 3 && f.area.abs() < 1e-9 && f.deducted.abs() < 1e-9 {
                continue;
            }
            let no = faces.len() + 1;
            let pts: Vec<[f64; 2]> = f.polygon.iter().map(|p| [p[0], p[1]]).collect();
            let (products, cross_sum) = shoelace(&pts);
            let plan = cross_sum.abs() / 2.0;
            let engine = f.area + f.deducted;
            // A near-vertical face has no usable pitch factor: take the
            // engine's area as the gross and show the factor as the ratio.
            let vertical = factor(f.pitch_deg).is_none();
            let (k, gross) = match factor(f.pitch_deg) {
                Some(k) => (k, plan * k),
                None => (if plan > 1e-9 { engine / plan } else { 0.0 }, engine),
            };
            faces.push(FaceStep {
                no,
                wall: (f.edge < part.edges.len()).then(|| f.edge + 1),
                pitch_deg: f.pitch_deg,
                material: f.material,
                polygon: f.polygon.clone(),
                products,
                cross_sum,
                plan_area: plan,
                cos_pitch: f.pitch_deg.to_radians().cos(),
                cos_digits: cos_digits(f.pitch_deg),
                factor: k,
                gross,
                area_3d: area_3d(&f.polygon),
                off_plane: off_plane(&f.polygon),
                deducted: f.deducted,
                area: f.area,
                engine_gross: (!steps_add_up(gross, f.deducted, f.area)).then_some(engine),
                digits: area_digits(
                    if steps_add_up(gross, f.deducted, f.area) {
                        gross
                    } else {
                        engine
                    },
                    f.deducted,
                    f.area,
                ),
                vertical,
            });
            materials.entry(f.material).or_default().push(AreaItem {
                part: Some(pi),
                face: Some(no),
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
            group(&mut lines, l.dacad_code()).items.push(line_step(
                Some(pi),
                l.a,
                l.b,
                l.length(),
                false,
            ));
        }
        for (code, len) in &geo.minus_lines {
            group(&mut lines, code).deducted += len;
        }

        parts.push(PartBreakdown {
            name: part.name.clone(),
            walls,
            faces,
            clad,
            clad_faces: geo
                .clad_faces
                .iter()
                .map(|c| {
                    let pieces: Vec<CladPiece> = c.pieces.iter().map(|p| clad_piece(p)).collect();
                    let sum: f64 = pieces.iter().map(|p| p.area).sum();
                    CladStep {
                        wall: (c.edge < part.edges.len()).then(|| c.edge + 1),
                        material: c.material,
                        pieces,
                        area: c.area,
                        digits: area_digits(sum, 0.0, sum).max(area_digits(c.area, 0.0, c.area)),
                        formula_area: ((sum - c.area).abs() > AREA_TOL).then_some(sum),
                    }
                })
                .collect(),
            other_cuts: geo.other_cuts,
            deducted: geo.faces.iter().map(|f| f.deducted).sum(),
            openings: geo
                .openings
                .iter()
                .map(|o| OpeningStep {
                    kind: o.kind,
                    area: o.area,
                })
                .collect(),
            roof_area: part_area,
            digits: sum_digits(
                &geo.faces
                    .iter()
                    .filter(|f| {
                        !(f.polygon.len() < 3 && f.area.abs() < 1e-9 && f.deducted.abs() < 1e-9)
                    })
                    .map(|f| f.area)
                    .chain(geo.clad_faces.iter().map(|c| c.area))
                    .chain(clad_rest_terms(geo))
                    .collect::<Vec<_>>(),
                part_area,
            ),
            calculated: !geo.faces.is_empty() || geo.clad_area != 0.0,
        });
    }

    let mut manual_faces = Vec::new();
    for (i, f) in project.manual_faces.iter().enumerate() {
        let points: Vec<[f64; 2]> = f.points.iter().map(|p| [p.x, p.y]).collect();
        let (products, cross_sum) = shoelace(&points);
        let area = f.area();
        manual_faces.push(ManualFaceStep {
            no: i + 1,
            label: f.label.clone(),
            pitch_deg: f.pitch_deg,
            points,
            products,
            cross_sum,
            plan_area: cross_sum.abs() / 2.0,
            cos_pitch: f.pitch_deg.to_radians().cos(),
            cos_digits: cos_digits(f.pitch_deg),
            factor: factor(f.pitch_deg).unwrap_or(0.0),
            area,
            digits: area_digits(area, 0.0, area),
            pitch_out_of_range: f.points.len() >= 3 && !f.pitch_counts(),
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
        group(&mut lines, &l.code).items.push(line_step(
            None,
            l.a,
            l.b,
            l.length(),
            l.length.is_some(),
        ));
    }

    let mut lines: Vec<LineGroup> = lines.into_values().collect();
    for g in &mut lines {
        g.sum = g.items.iter().map(|l| l.length).sum::<f64>() - g.deducted;
        g.total = round_cm(g.sum);
        let terms: Vec<f64> = g
            .items
            .iter()
            .map(|l| l.length)
            .chain(std::iter::once(-g.deducted))
            .collect();
        g.digits = sum_digits(&terms, g.sum);
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
            digits: sum_digits(
                &items.iter().map(|i| i.area).collect::<Vec<_>>(),
                items.iter().map(|i| i.area).sum(),
            ),
            items,
        })
        .collect();
    let digits = sum_digits(
        &parts
            .iter()
            .map(|p| p.roof_area)
            .chain(manual_faces.iter().map(|f| f.area))
            .collect::<Vec<_>>(),
        roof_area,
    );

    Breakdown {
        decimals: DECIMALS,
        parts,
        manual_faces,
        materials,
        lines,
        roof_area,
        digits,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::examples;
    use crate::geom::Point2;
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
                // The independent 3D check agrees for a single roof.
                assert!(
                    (f.area_3d - f.gross).abs() < AREA_TOL,
                    "{} vs {}",
                    f.area_3d,
                    f.gross
                );
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
