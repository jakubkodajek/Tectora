//! Plausibility checks of a calculated roof.
//!
//! The engine reproduces DACAD's calculation exactly, including the rare
//! roofs where DACAD's own result is geometrically wrong (see
//! `docs/core-verification.md`, findings 2 and 3): hips running outside the
//! building, faces lying over each other, or a face area that does not
//! match the face. The result is kept as DACAD gives it; these checks only
//! measure the geometry against the walls so the app can tell the user to
//! look at the roof before using its quantities.
//!
//! Every check is independent of how the roof was calculated: it uses the
//! walls, their overhangs and the faces' own corners.

use geo::{Area, BooleanOps, Coord, LineString, MultiPolygon, Polygon};
use serde::Serialize;

use crate::geom::{Point2, signed_area};
use crate::model::RoofPart;
use crate::roof::{RoofFace, RoofGeometry};

/// How far a roof corner may lie outside the eave outline, m. DACAD merges
/// points within 2 cm and snaps outline points to whole centimetres, so its
/// correct roofs reach a few centimetres past the exact outline.
pub const OUTSIDE_TOL: f64 = 0.05;
/// Plan area where faces pass through each other that is still rounding,
/// m². Sound roofs stay below 0.06 m² (2 700 DACAD-calculated roofs), the
/// faulty ones found are 0.5 m² and more.
pub const OVERLAP_TOL: f64 = 0.25;
/// Relative difference between the area reported for the roof faces and
/// the area of the faces themselves above which the quantities are suspect
/// (1 %; DACAD's sweep tolerances alone give up to about 0.5 %).
pub const AREA_TOL: f64 = 0.01;

/// What is wrong with a calculated roof.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WarningKind {
    /// Roof corners lie outside the walls pushed out by their overhangs
    /// (for example hips running out of the building). `value`: largest
    /// distance outside, m.
    OutsideOutline,
    /// Roof faces lie over each other in plan. `value`: plan area covered
    /// more than once, m².
    OverlappingFaces,
    /// The roof area reported for the faces differs from the area of the
    /// faces' own outlines. `value`: relative difference.
    AreaMismatch,
}

/// One finding about a calculated roof.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RoofWarning {
    pub kind: WarningKind,
    /// How large the deviation is, in the unit `kind` gives.
    pub value: f64,
}

/// Plan pieces where two faces overlap smaller than this are rounding
/// slivers along their common line, m².
const MIN_PIECE: f64 = 0.01;
/// Height difference below which two faces count as touching, m.
const Z_TOL: f64 = 0.02;

/// `z = a x + b y + c` of a face, from its Newell normal.
fn plane_of(poly: &[[f64; 3]]) -> Option<[f64; 3]> {
    let n = poly.len();
    let mut nv = [0.0; 3];
    let mut c = [0.0; 3];
    for i in 0..n {
        let (p, q) = (poly[i], poly[(i + 1) % n]);
        nv[0] += (p[1] - q[1]) * (p[2] + q[2]);
        nv[1] += (p[2] - q[2]) * (p[0] + q[0]);
        nv[2] += (p[0] - q[0]) * (p[1] + q[1]);
        for k in 0..3 {
            c[k] += p[k] / n as f64;
        }
    }
    let len = (nv[0] * nv[0] + nv[1] * nv[1] + nv[2] * nv[2]).sqrt();
    if len == 0.0 || nv[2].abs() < 1e-3 * len {
        return None;
    }
    let (a, b) = (-nv[0] / nv[2], -nv[1] / nv[2]);
    Some([a, b, c[2] - a * c[0] - b * c[1]])
}

/// Whether two faces lying over each other on `piece` pass through each
/// other or coincide there. A face above another one all over the piece
/// (an overhang reaching over a lower roof) is a sound roof.
fn crossing(piece: &Polygon<f64>, a: [f64; 3], b: [f64; 3]) -> bool {
    let ring = &piece.exterior().0;
    let pts = &ring[..ring.len().saturating_sub(1)];
    if pts.is_empty() {
        return false;
    }
    let n = pts.len() as f64;
    let (cx, cy) = (
        pts.iter().map(|p| p.x).sum::<f64>() / n,
        pts.iter().map(|p| p.y).sum::<f64>() / n,
    );
    // Corners pulled a little towards the middle, so faces meeting on the
    // piece's border do not count.
    let samples = pts
        .iter()
        .map(|p| (cx + 0.9 * (p.x - cx), cy + 0.9 * (p.y - cy)))
        .chain(std::iter::once((cx, cy)));
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for (x, y) in samples {
        let d = (a[0] - b[0]) * x + (a[1] - b[1]) * y + (a[2] - b[2]);
        lo = lo.min(d);
        hi = hi.max(d);
    }
    (lo < -Z_TOL && hi > Z_TOL) || (lo.abs() < Z_TOL && hi.abs() < Z_TOL)
}

/// The measurements the checks compare with their tolerances.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Measures {
    /// Largest distance of a roof corner outside the eave outline, m.
    pub outside: f64,
    /// Plan area where two faces pass through each other or coincide, m².
    pub overlap: f64,
    /// Relative difference of reported to polygon area of the roof faces.
    pub area: f64,
}

/// Roof faces of the part itself: not dormer faces (no footprint edge),
/// not vertical faces (clad gables have no plan area).
fn roof_faces(geo: &RoofGeometry) -> impl Iterator<Item = &RoofFace> {
    geo.faces
        .iter()
        .filter(|f| f.edge != usize::MAX && f.pitch_deg < 89.9 && f.polygon.len() >= 3)
}

fn plan(face: &RoofFace) -> Vec<Point2> {
    face.polygon
        .iter()
        .map(|p| Point2::new(p[0], p[1]))
        .collect()
}

fn to_geo(pts: &[Point2]) -> Polygon<f64> {
    let mut ring: Vec<Coord<f64>> = pts.iter().map(|p| Coord { x: p.x, y: p.y }).collect();
    if signed_area(pts) < 0.0 {
        ring.reverse();
    }
    Polygon::new(LineString::new(ring), vec![])
}

/// True area of a planar 3D polygon.
fn area3(poly: &[[f64; 3]]) -> f64 {
    let n = poly.len();
    let o = poly[0];
    let mut s = [0.0; 3];
    for i in 1..n.saturating_sub(1) {
        let a = [poly[i][0] - o[0], poly[i][1] - o[1], poly[i][2] - o[2]];
        let b = [
            poly[i + 1][0] - o[0],
            poly[i + 1][1] - o[1],
            poly[i + 1][2] - o[2],
        ];
        s[0] += a[1] * b[2] - a[2] * b[1];
        s[1] += a[2] * b[0] - a[0] * b[2];
        s[2] += a[0] * b[1] - a[1] * b[0];
    }
    0.5 * (s[0] * s[0] + s[1] * s[1] + s[2] * s[2]).sqrt()
}

/// Distance from `q` to the segment `a`–`b`.
fn seg_dist(q: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 {
        (((q.x - a.x) * dx + (q.y - a.y) * dy) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    q.distance(Point2::new(a.x + dx * t, a.y + dy * t))
}

fn inside(poly: &[Point2], q: Point2) -> bool {
    let mut c = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a.y > q.y) != (b.y > q.y) && q.x < a.x + (b.x - a.x) * (q.y - a.y) / (b.y - a.y) {
            c = !c;
        }
    }
    c
}

/// Distance of `q` outside `poly` (0 inside).
fn outside_by(poly: &[Point2], q: Point2) -> f64 {
    if inside(poly, q) {
        return 0.0;
    }
    let n = poly.len();
    (0..n)
        .map(|i| seg_dist(q, poly[i], poly[(i + 1) % n]))
        .fold(f64::INFINITY, f64::min)
}

/// The eave outline as pieces: the walls' own polygon and, for every wall,
/// the strip between the wall and its line moved out by the overhang, cut
/// where it meets the neighbouring walls' lines (mitred). On a convex
/// building their union is the walls pushed out by their overhangs; where
/// walls fold around a notch the strips overlap instead of the mitred
/// outline crossing itself. Zero-length walls (direction markers) are left
/// out; where two walls in line have different overhangs the outline steps
/// from one to the other at their common corner.
pub fn eave_outline(part: &RoofPart) -> Vec<Vec<Point2>> {
    let mut p = part.clone();
    p.normalize_ccw();
    let n = p.edges.len();
    // Each wall: its corners and its line moved out (to the right of a
    // counter-clockwise wall).
    let walls: Vec<(Point2, Point2, Point2, Point2)> = (0..n)
        .filter_map(|i| {
            let a = p.edges[i].start;
            let b = p.edges[(i + 1) % n].start;
            let len = a.distance(b);
            (len > 1e-6).then(|| {
                let u = Point2::new((b.x - a.x) / len, (b.y - a.y) / len);
                let ov = p.edges[i].overhang;
                (a, b, Point2::new(a.x + u.y * ov, a.y - u.x * ov), u)
            })
        })
        .collect();
    let m = walls.len();
    let mut pieces = vec![p.outline()];
    if m < 3 {
        return pieces;
    }
    // Where line `i` meets line `j` (j follows i): on line i.
    let meet = |i: usize, j: usize| -> Point2 {
        let (_, corner, p0, u0) = walls[i];
        let (_, _, p1, u1) = walls[j];
        let den = u0.x * u1.y - u0.y * u1.x;
        if den.abs() < 1e-9 {
            // In line (or folding back): leave line i at the corner.
            let t = (corner.x - p0.x) * u0.x + (corner.y - p0.y) * u0.y;
            return Point2::new(p0.x + u0.x * t, p0.y + u0.y * t);
        }
        let t = ((p1.x - p0.x) * u1.y - (p1.y - p0.y) * u1.x) / den;
        Point2::new(p0.x + u0.x * t, p0.y + u0.y * t)
    };
    for (i, &(a, b, p1, u1)) in walls.iter().enumerate() {
        let start = meet((i + m - 1) % m, i);
        // The start on line i itself: where line i-1 meets it, or for walls
        // in line the foot of the corner on line i.
        let start = {
            let t = (start.x - p1.x) * u1.x + (start.y - p1.y) * u1.y;
            Point2::new(p1.x + u1.x * t, p1.y + u1.y * t)
        };
        let end = meet(i, (i + 1) % m);
        pieces.push(vec![a, b, end, start]);
    }
    pieces
}

/// Measures a calculated roof against `outline`, the pieces of the eave
/// outlines ([`eave_outline`]) of every building it may cover.
pub fn measure(outline: &[Vec<Point2>], geo: &RoofGeometry) -> Measures {
    let mut m = Measures::default();
    if outline.iter().all(|piece| piece.len() < 3) {
        return m;
    }
    let faces: Vec<&RoofFace> = roof_faces(geo).collect();
    for p in faces.iter().flat_map(|f| f.polygon.iter()) {
        let q = Point2::new(p[0], p[1]);
        let d = outline
            .iter()
            .map(|piece| outside_by(piece, q))
            .fold(f64::INFINITY, f64::min);
        m.outside = m.outside.max(d);
    }
    let plans: Vec<MultiPolygon<f64>> = faces
        .iter()
        .map(|f| MultiPolygon::new(vec![to_geo(&plan(f))]))
        .collect();
    let planes: Vec<Option<[f64; 3]>> = faces.iter().map(|f| plane_of(&f.polygon)).collect();
    for i in 0..faces.len() {
        for j in i + 1..faces.len() {
            let (Some(pi), Some(pj)) = (planes[i], planes[j]) else {
                continue;
            };
            for piece in plans[i].intersection(&plans[j]) {
                let a = piece.unsigned_area();
                if a < MIN_PIECE {
                    continue;
                }
                if crossing(&piece, pi, pj) {
                    m.overlap += a;
                }
            }
        }
    }
    let (mut reported, mut polygons) = (0.0, 0.0);
    for f in &faces {
        reported += f.area + f.deducted;
        polygons += area3(&f.polygon);
    }
    if polygons > 0.0 {
        m.area = (reported - polygons).abs() / polygons;
    }
    m
}

/// The warnings for each part's calculated roof (same order as `parts`);
/// empty for a sound roof and for a roof DACAD gave no result for.
///
/// Where a project has several roofs they are joined (DACAD's
/// Modellverschnitt): a lower roof runs on to the higher building, so a
/// roof may cover every building's eave outline, and its faces are the
/// pieces the join leaves, whose areas DACAD books by its own rules; the
/// area check is then left out.
pub fn check(parts: &[RoofPart], geos: &[RoofGeometry]) -> Vec<Vec<RoofWarning>> {
    let outline: Vec<Vec<Point2>> = parts.iter().flat_map(eave_outline).collect();
    let joined = parts.len() > 1;
    geos.iter()
        .map(|geo| warnings(&outline, geo, joined))
        .collect()
}

fn warnings(outline: &[Vec<Point2>], geo: &RoofGeometry, joined: bool) -> Vec<RoofWarning> {
    if geo.error.is_some() || geo.faces.is_empty() {
        return Vec::new();
    }
    let m = measure(outline, geo);
    let mut out = Vec::new();
    if m.outside > OUTSIDE_TOL {
        out.push(RoofWarning {
            kind: WarningKind::OutsideOutline,
            value: m.outside,
        });
    }
    if m.overlap > OVERLAP_TOL {
        out.push(RoofWarning {
            kind: WarningKind::OverlappingFaces,
            value: m.overlap,
        });
    }
    if !joined && m.area > AREA_TOL {
        out.push(RoofWarning {
            kind: WarningKind::AreaMismatch,
            value: m.area,
        });
    }
    out
}
