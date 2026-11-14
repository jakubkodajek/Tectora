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
//! Every check here is independent of how the roof was calculated: it uses
//! the walls, their overhangs and the faces' own corners. One more kind,
//! [`WarningKind::MergedWalls`], comes from the calculation itself
//! (`engine`): walls whose pitch the calculation does not use.

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
/// Net area below which a face has lost more to its deductions than it
/// has, m² (a little below zero, for rounding of the joined roofs' cuts).
pub const OVERCUT_TOL: f64 = 0.01;

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
    /// Two walls in line were calculated as one wall, so the roof
    /// definition of the second one is not used (the calculation joins
    /// walls in line with the same eave height and overhang, whatever
    /// their pitches; and where their roof faces lie in one plane it drops
    /// the second wall's pitch breaks). `edges`: the wall whose definition
    /// is used, then the wall whose pitch or pitch breaks are ignored.
    /// `value`: 0.
    MergedWalls,
    /// Several roofs are joined and no face of this part's roof is left:
    /// another part covers it (a building drawn inside or on top of
    /// another). The plan shows the walls with no roof. `value`: 0.
    Covered,
    /// More is deducted from a roof face than the face has: its area is
    /// negative, so the roof total is too small. A window larger than the
    /// face it sits on does it (its `width × height` is deducted as DACAD
    /// deducts it), and so does the cut of joined roofs where it exceeds
    /// a face. `edges`: the walls of those faces (dormer faces name
    /// none). `value`: the area missing below zero, summed, m².
    CutExceedsFace,
    /// A dormer sits on no roof face (placed beside the roof or moved off
    /// it): it is left out of the calculation, so its area, lines and
    /// material are not in the result. `dormer`: its index. `value`: 0.
    DormerOffRoof,
    /// A roof window sits on no roof face: nothing is deducted for it, so
    /// it is not in the result. `item`: its index in `project.windows`.
    /// `value`: 0.
    WindowOffRoof,
    /// A chimney lies on no roof face: it cuts nothing, so it is not in
    /// the result. `item`: its index in `project.chimneys`. `value`: 0.
    ChimneyOffRoof,
    /// A roof opening lies on no roof face: it cuts nothing, so it is not
    /// in the result. `item`: its index in `project.roof_openings`.
    /// `value`: 0.
    OpeningOffRoof,
    /// The roof was not calculated by the standard calculation (DACAD's
    /// core) but by Tectora's own fallback, so its figures may differ from
    /// the standard result. `value`: 0.
    NotCalculatedByCore,
    /// The roof plane of a wall is parallel to the plane of another wall
    /// of the building further in (same direction and pitch, not in line)
    /// and lies above it, so it runs on past that wall's eave as one
    /// plane: the part in front of the building gets no ridge of its own
    /// (a wing whose sides are gables, its front an eave). DACAD
    /// calculates such a roof; it is rarely what was meant. `edges`: the
    /// outer wall, then the inner wall. `value`: how much higher the outer
    /// plane lies, m.
    PlaneOverWall,
    /// Two parallel roof planes lie in one plane (within
    /// [`crate::coplanar::MERGE_TOL`]): DACAD stops with an error there,
    /// Tectora calculates them as one face (Jakub's decision 2026-11-14,
    /// `coplanar.rs`). Not DACAD's result. `edges`: the front wall, then
    /// the wall whose plane it continues. `value`: how far apart the two
    /// planes were, m.
    MergedPlanes,
    /// The building's roof did not close in one calculation (a wing whose
    /// eaves are so low that its roof stays under the house's), so the
    /// wing was calculated as a building of its own against the house and
    /// the two roofs joined (Jakub's decision 2026-11-14,
    /// `wing_split.rs`). DACAD stops with an error here. `edges`: the
    /// walls of the wing. `value`: 0.
    WingJoined,
}

impl WarningKind {
    /// A hint about the input or a note on what Tectora did beyond the
    /// standard calculation, not a sign that the roof's geometry is wrong:
    /// the roof is sound as calculated.
    pub fn is_notice(self) -> bool {
        matches!(
            self,
            WarningKind::PlaneOverWall | WarningKind::MergedPlanes | WarningKind::WingJoined
        )
    }
}

/// One finding about a calculated roof.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RoofWarning {
    pub kind: WarningKind,
    /// How large the deviation is, in the unit `kind` gives.
    pub value: f64,
    /// The walls (edge indices of the part) the finding is about, where
    /// `kind` names them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<usize>,
    /// The dormer (index into `project.dormers`) the finding is about.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dormer: Option<usize>,
    /// The window, chimney or opening (index into the project's list of
    /// that kind) the finding is about.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<usize>,
}

impl RoofWarning {
    /// A finding about the whole roof.
    pub fn new(kind: WarningKind, value: f64) -> RoofWarning {
        RoofWarning {
            kind,
            value,
            edges: Vec::new(),
            dormer: None,
            item: None,
        }
    }
}

/// The windows, chimneys and openings of `project` that lie on no face of
/// the calculated roofs `geos` (same order as `project.parts`): the
/// calculation leaves them out, so each is reported on the part nearest
/// to it, as `(part, warning)`. Nothing is reported for a project without
/// a calculated face (the calculation's error says enough).
pub fn off_roof(project: &crate::Project, geos: &[RoofGeometry]) -> Vec<(usize, RoofWarning)> {
    if geos.iter().any(|g| g.error.is_some()) || geos.iter().all(|g| g.faces.is_empty()) {
        return Vec::new();
    }
    let plans: Vec<Vec<Point2>> = geos
        .iter()
        .flat_map(|g| &g.faces)
        .filter(|f| f.polygon.len() >= 3)
        .map(plan)
        .collect();
    let on_roof = |corners: &[Point2]| {
        let n = corners.len().max(1) as f64;
        let c = Point2::new(
            corners.iter().map(|p| p.x).sum::<f64>() / n,
            corners.iter().map(|p| p.y).sum::<f64>() / n,
        );
        plans.iter().any(|poly| {
            corners.iter().chain([&c]).any(|&q| inside(poly, q))
                || poly.iter().any(|&q| inside(corners, q))
        })
    };
    let nearest = |q: Point2| {
        (0..project.parts.len().min(geos.len())).min_by(|&a, &b| {
            crate::engine::outline_distance(&project.parts[a], q)
                .total_cmp(&crate::engine::outline_distance(&project.parts[b], q))
        })
    };
    let mut out = Vec::new();
    let mut report = |kind, item, at: Point2| {
        if let Some(part) = nearest(at) {
            out.push((
                part,
                RoofWarning {
                    item: Some(item),
                    ..RoofWarning::new(kind, 0.0)
                },
            ));
        }
    };
    for (i, w) in project.windows.iter().enumerate() {
        if crate::openings::window_face(geos, w).is_none() {
            report(WarningKind::WindowOffRoof, i, Point2::new(w.x, w.y));
        }
    }
    for (kind, list) in [
        (WarningKind::ChimneyOffRoof, &project.chimneys),
        (WarningKind::OpeningOffRoof, &project.roof_openings),
    ] {
        for (i, c) in list.iter().enumerate() {
            if !c.corners.is_empty() && !on_roof(&c.corners) {
                report(kind, i, c.corners[0]);
            }
        }
    }
    out
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
pub(crate) fn area3(poly: &[[f64; 3]]) -> f64 {
    let n = poly.len();
    let Some(&o) = poly.first() else {
        return 0.0;
    };
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
        .enumerate()
        .map(|(i, geo)| {
            let roofed = parts
                .get(i)
                .is_some_and(|p| p.edges.iter().any(|e| !e.is_gable()));
            if joined && roofed && geo.error.is_none() && geo.faces.is_empty() {
                return vec![RoofWarning::new(WarningKind::Covered, 0.0)];
            }
            let mut w = warnings(&outline, geo, joined);
            if let Some(part) = parts.get(i) {
                w.extend(plane_over_wall(part, geo));
            }
            w
        })
        .collect()
}

fn warnings(outline: &[Vec<Point2>], geo: &RoofGeometry, joined: bool) -> Vec<RoofWarning> {
    if geo.error.is_some() || geo.faces.is_empty() {
        return Vec::new();
    }
    let m = measure(outline, geo);
    let mut out = Vec::new();
    if m.outside > OUTSIDE_TOL {
        out.push(RoofWarning::new(WarningKind::OutsideOutline, m.outside));
    }
    if m.overlap > OVERLAP_TOL {
        out.push(RoofWarning::new(WarningKind::OverlappingFaces, m.overlap));
    }
    if !joined && m.area > AREA_TOL {
        out.push(RoofWarning::new(WarningKind::AreaMismatch, m.area));
    }
    if let Some(w) = over_cut(geo) {
        out.push(w);
    }
    out
}

/// Two walls of one building whose first roof planes are parallel: same
/// direction, same pitch, not in line. Their planes never meet; between
/// them only other planes (hips, a ridge across) can end one of them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParallelPlanes {
    /// The wall further out (along the walls' outward normal).
    pub outer: usize,
    /// The wall further in.
    pub inner: usize,
    /// How much higher the outer wall's plane lies than the inner one's,
    /// at every point, m (negative: lower).
    pub rise: f64,
    /// Distance between the two walls' lines, m.
    pub depth: f64,
}

/// Directions closer than this (1 − cos of the angle) count as parallel.
/// 0.05°: a wall turned out of the axes with its corners rounded to
/// millimetres is up to 0.011° off its direction per 5 m.
const PARALLEL_COS: f64 = 4e-7;
/// Pitches closer than this count as equal, degrees.
const SAME_PITCH: f64 = 1e-3;
/// Wall lines closer than this are in line (the calculation joins those
/// walls instead, see [`WarningKind::MergedWalls`]), m.
const IN_LINE: f64 = 0.01;

/// Every pair of walls of `part` (counter-clockwise, as the calculation
/// takes it) whose first roof planes are parallel. A wall's plane is
/// `z = eave + tan(pitch) · (overhang + d − m·p)` with `m` its outward
/// normal and `d = m·start`, so two such planes differ by a constant.
pub fn parallel_planes(part: &RoofPart) -> Vec<ParallelPlanes> {
    let n = part.edges.len();
    let wall = |i: usize| {
        let e = &part.edges[i];
        let b = part.edges[(i + 1) % n].start;
        let (dx, dy) = (b.x - e.start.x, b.y - e.start.y);
        let len = dx.hypot(dy);
        let sloped = e.pitch_deg > 0.0 && e.pitch_deg < crate::model::GABLE_PITCH_DEG;
        (sloped && len > IN_LINE && e.direction_rad.is_none() && len.is_finite()).then(|| {
            let m = (dy / len, -dx / len);
            let d = m.0 * e.start.x + m.1 * e.start.y;
            let c = e.eave_height + e.pitch_deg.to_radians().tan() * (e.overhang + d);
            (m, d, c, e.pitch_deg)
        })
    };
    let walls: Vec<_> = (0..n).map(wall).collect();
    let mut out = Vec::new();
    for i in 0..n {
        let Some((mi, di, ci, pi)) = walls[i] else {
            continue;
        };
        for (j, wj) in walls.iter().enumerate().skip(i + 1) {
            let Some((mj, dj, cj, pj)) = *wj else {
                continue;
            };
            let same_dir = mi.0 * mj.0 + mi.1 * mj.1 > 1.0 - PARALLEL_COS;
            if !same_dir || (pi - pj).abs() > SAME_PITCH || (di - dj).abs() < IN_LINE {
                continue;
            }
            let (outer, inner, rise) = if di > dj {
                (i, j, ci - cj)
            } else {
                (j, i, cj - ci)
            };
            out.push(ParallelPlanes {
                outer,
                inner,
                rise,
                depth: (di - dj).abs(),
            });
        }
    }
    out
}

/// How far a face of the outer wall must reach past the inner wall's eave
/// line before [`WarningKind::PlaneOverWall`] is reported, m. Faces that
/// end at a hip or a ridge between the two walls stay in front of it.
pub const OVER_WALL_TOL: f64 = 0.10;
/// Rise of an outer plane below which it is the same plane as the inner
/// one (DACAD merges heights within 1 cm and stops with an error on
/// planes this close, see [`coplanar_error_walls`]), m.
pub const SAME_PLANE: f64 = 0.02;

/// [`WarningKind::PlaneOverWall`] for one calculated part: an outer
/// plane with gables on both sides, higher than a parallel inner one, whose
/// face reaches past the inner wall's eave line. One warning per outer wall, naming the inner
/// wall it passes furthest.
pub fn plane_over_wall(part: &RoofPart, geo: &RoofGeometry) -> Vec<RoofWarning> {
    if geo.error.is_some() || part.edges.len() < 4 {
        return Vec::new();
    }
    let n = part.edges.len();
    let mut best: Vec<(usize, usize, f64, f64)> = Vec::new();
    for p in wing_planes(part) {
        if p.rise < SAME_PLANE {
            continue;
        }
        let e = &part.edges[p.inner];
        let b = part.edges[(p.inner + 1) % n].start;
        let len = (b.x - e.start.x).hypot(b.y - e.start.y);
        let m = ((b.y - e.start.y) / len, -(b.x - e.start.x) / len);
        // The inner wall's eave line, as `m·p`.
        let eave = m.0 * e.start.x + m.1 * e.start.y + e.overhang;
        let past = geo
            .faces
            .iter()
            .filter(|f| f.edge == p.outer)
            .flat_map(|f| f.polygon.iter())
            .map(|q| eave - (m.0 * q[0] + m.1 * q[1]))
            .fold(f64::NEG_INFINITY, f64::max);
        if past > OVER_WALL_TOL {
            match best.iter_mut().find(|b| b.0 == p.outer) {
                Some(b) if b.3 >= past => {}
                Some(b) => *b = (p.outer, p.inner, p.rise, past),
                None => best.push((p.outer, p.inner, p.rise, past)),
            }
        }
    }
    best.sort_by_key(|b| b.0);
    best.into_iter()
        .map(|(outer, inner, rise, _)| RoofWarning {
            edges: vec![outer, inner],
            ..RoofWarning::new(WarningKind::PlaneOverWall, rise)
        })
        .collect()
}

/// The parallel planes of `part` whose outer wall is the front of a wing
/// without a ridge of its own: gables on both sides of it, in front of a
/// longer house wall.
///
/// Only gables on both sides leave the outer plane nothing to end it
/// before the inner plane: with an eave beside it, a hip or a valley ends
/// it (a hipped wing whose hip face reaches a little past the main eave,
/// or the two halves of a stepped house, are sound). And a wing stands in
/// front of a longer house wall; a short wall behind a long one is a notch
/// cut into the house, whose own plane the long wall's plane rightly runs
/// over (a gable roof with a corner cut out).
pub fn wing_planes(part: &RoofPart) -> Vec<ParallelPlanes> {
    let n = part.edges.len();
    if n < 4 {
        return Vec::new();
    }
    let wall_len = |k: usize| {
        let a = part.edges[k].start;
        let b = part.edges[(k + 1) % n].start;
        (b.x - a.x).hypot(b.y - a.y)
    };
    parallel_planes(part)
        .into_iter()
        .filter(|p| {
            let gable_beside = [p.outer + n - 1, p.outer + 1]
                .iter()
                .all(|&k| part.edges[k % n].is_gable());
            let same_line = |k: usize| {
                let (ek, ei) = (&part.edges[k], &part.edges[p.inner]);
                k == p.inner
                    || ((ek.pitch_deg - ei.pitch_deg).abs() <= SAME_PITCH
                        && (ek.eave_height - ei.eave_height).abs() < 1e-6
                        && (ek.overhang - ei.overhang).abs() < 1e-6
                        && collinear(part, k, p.inner))
            };
            let behind: f64 = (0..n).filter(|&k| same_line(k)).map(wall_len).sum();
            gable_beside && wall_len(p.outer) < behind
        })
        .collect()
}

/// Whether walls `i` and `j` of `part` lie on one line.
fn collinear(part: &RoofPart, i: usize, j: usize) -> bool {
    let n = part.edges.len();
    let (a, b) = (part.edges[j].start, part.edges[(j + 1) % n].start);
    let (c, d) = (part.edges[i].start, part.edges[(i + 1) % n].start);
    let len = (b.x - a.x).hypot(b.y - a.y);
    if len < IN_LINE {
        return false;
    }
    let off = |p: Point2| ((b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)) / len;
    off(c).abs() < IN_LINE && off(d).abs() < IN_LINE
}

/// How far an outer plane may lie below a parallel inner one for the two
/// to be named as the likely cause of an error, m: further apart, raising
/// an eave "a little" is no advice.
pub const ERROR_WALLS_RISE: f64 = 0.5;

/// For a part the calculation stops on with `kind`: the outer and inner
/// wall of the parallel planes most likely at fault, where an outer plane
/// between two gables lies in, or up to [`ERROR_WALLS_RISE`] below, a
/// parallel inner one. Such planes never meet, and the walls between them
/// are gables, so nothing closes the roof there (a wing whose front eave
/// was lowered until its plane reached the main roof's). Empty when the
/// part has no such planes, and for the kinds parallel planes cannot
/// cause (a non-convex chimney, an invalid wall).
pub fn coplanar_error_walls(part: &RoofPart, kind: crate::roof::CalcErrorKind) -> Vec<usize> {
    use crate::roof::CalcErrorKind as K;
    if !matches!(kind, K::OpenFaces | K::Cut | K::NoEnd | K::NoSolution) {
        return Vec::new();
    }
    let n = part.edges.len();
    parallel_planes(part)
        .into_iter()
        .filter(|p| {
            p.rise < crate::coplanar::MERGE_TOL
                && p.rise > -ERROR_WALLS_RISE
                && [p.outer + n - 1, p.outer + 1]
                    .iter()
                    .all(|&k| part.edges[k % n].is_gable())
        })
        .min_by(|a, b| a.rise.abs().total_cmp(&b.rise.abs()))
        .map(|p| vec![p.outer, p.inner])
        .unwrap_or_default()
}

/// The faces (dormer faces too) whose deductions exceed their gross area.
pub fn over_cut(geo: &RoofGeometry) -> Option<RoofWarning> {
    let mut missing = 0.0;
    let mut edges = Vec::new();
    for f in geo.faces.iter().filter(|f| f.area < -OVERCUT_TOL) {
        missing -= f.area;
        if f.edge != usize::MAX && !edges.contains(&f.edge) {
            edges.push(f.edge);
        }
    }
    edges.sort_unstable();
    (missing > 0.0).then_some(RoofWarning {
        edges,
        ..RoofWarning::new(WarningKind::CutExceedsFace, missing)
    })
}

// ------------------------------------------------------------ self-check

/// How closely a roof that is not the standard result must follow its own
/// geometry before it is shown ([`sound`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fit {
    /// A face's slope against the pitch it reports, degrees.
    pub pitch: f64,
    /// A face corner out of the face's plane, m.
    pub planar: f64,
}

impl Fit {
    /// A roof from one calculation.
    pub const STANDARD: Fit = Fit {
        pitch: 0.05,
        planar: 1e-3,
    };
    /// A roof joined from two buildings: the join snaps corners to whole
    /// centimetres of the joined model, which leaves faces up to a
    /// centimetre out of their plane on a building turned out of the axes
    /// (the same join of two buildings drawn by hand gives the same
    /// faces).
    pub const JOINED: Fit = Fit {
        pitch: 0.1,
        planar: 0.012,
    };
}

/// A line end this close to a face side lies on it, m (the core merges
/// points within 2 cm).
const NEAR: f64 = 0.025;
/// Area a face may differ from its polygon per metre of its outline, m
/// (merged corners leave slivers).
const SLIVER: f64 = 0.02;
/// A face corner this close to a side it does not belong to makes the
/// outline touch itself, m.
const SPIKE: f64 = 2e-3;
/// A face's plane at its wall's eave line against the wall's eave height, m.
const EAVE_TOL: f64 = 0.01;
/// Share of the eave outline the faces may cover more than once where
/// eaves differ in height (a higher roof's overhang over a lower roof; the
/// largest of 150 measured wing roofs is 2.6 %).
const OVER_COVER: f64 = 0.03;

fn sub3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm3(a: [f64; 3]) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

/// Distance of `p` from the segment `a`-`b` in space.
fn seg_dist3(p: [f64; 3], a: [f64; 3], b: [f64; 3]) -> f64 {
    let ab = sub3(b, a);
    let l2 = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];
    if l2 == 0.0 {
        return norm3(sub3(p, a));
    }
    let ap = sub3(p, a);
    let t = ((ap[0] * ab[0] + ap[1] * ab[1] + ap[2] * ab[2]) / l2).clamp(0.0, 1.0);
    norm3(sub3(
        p,
        [a[0] + ab[0] * t, a[1] + ab[1] * t, a[2] + ab[2] * t],
    ))
}

/// Whether segment `p`-`q` runs along segment `a`-`b`: its middle is on
/// `a`-`b` and the two are parallel.
fn along(a: [f64; 3], b: [f64; 3], p: [f64; 3], q: [f64; 3]) -> bool {
    let len = norm3(sub3(q, p));
    let m = [
        (p[0] + q[0]) / 2.0,
        (p[1] + q[1]) / 2.0,
        (p[2] + q[2]) / 2.0,
    ];
    len > 0.0
        && seg_dist3(m, a, b) < NEAR
        && norm3(cross3(sub3(b, a), sub3(q, p))) <= NEAR * norm3(sub3(b, a))
}

/// The first roof plane of wall `i` of `part` as (outward normal, height
/// of the plane over the wall line), `None` for a gable, a direction
/// marker or a wall without length.
fn wall_plane(part: &RoofPart, i: usize) -> Option<((f64, f64), f64)> {
    let n = part.edges.len();
    let e = part.edges.get(i)?;
    let b = part.edges.get((i + 1) % n)?.start;
    let (dx, dy) = (b.x - e.start.x, b.y - e.start.y);
    let len = dx.hypot(dy);
    let sloped = e.pitch_deg > 0.0 && e.pitch_deg < crate::model::GABLE_PITCH_DEG;
    (sloped && len > IN_LINE && len.is_finite() && e.direction_rad.is_none()).then(|| {
        (
            (dy / len, -dx / len),
            e.eave_height + e.pitch_deg.to_radians().tan() * e.overhang,
        )
    })
}

/// Whether the roof `geo` of the building `part` (counter-clockwise,
/// without dormers, chimneys, openings and windows) is a sound roof of
/// these walls. Every result that is not the standard calculation's must
/// pass before it is shown: a wrong roof is worse than an error.
///
/// * there are faces, every one names a wall, is planar, lies at the pitch
///   it reports, and that pitch is one of its wall's;
/// * every area is finite, positive and the area of the face's polygon;
/// * a face at its wall's first pitch passes the wall line at the wall's
///   eave height (a roof that ignores an eave height fails here);
/// * every sloped wall has a face, or its plane is the plane of another
///   wall that has one (walls in line, planes joined);
/// * the faces cover the eave outline, and no more than it except where
///   eaves differ in height;
/// * every face side lies on a roof line and every roof line on a face
///   side; no face outline touches itself; no face ends along another
///   wall's eave line at another height; no two faces of one plane lie
///   side by side;
/// * [`check`] gives nothing but notices.
///
/// `Err` says what failed, for logs and tests.
pub fn sound(part: &RoofPart, geo: &RoofGeometry, fit: Fit) -> Result<(), String> {
    let n = part.edges.len();
    if geo.error.is_some() {
        return Err("calculation error".into());
    }
    if geo.faces.is_empty() {
        return Err("no faces".into());
    }
    let planes: Vec<_> = (0..n).map(|i| wall_plane(part, i)).collect();
    let mut has_face = vec![false; n];
    // Per face: its unit normal and a point, for the test of neighbours.
    let mut face_planes: Vec<([f64; 3], [f64; 3], usize)> = Vec::new();
    let mut plan_sum = 0.0;
    let mut sides = 0.0;
    let mut segs: Vec<([f64; 3], [f64; 3])> = Vec::new();
    for f in &geo.faces {
        let Some(e) = part.edges.get(f.edge) else {
            return Err("a face names no wall".into());
        };
        let m = f.polygon.len();
        if m < 3 || !f.polygon.iter().flatten().all(|v| v.is_finite()) {
            return Err(format!("face {}: no polygon", f.edge));
        }
        if !(f.area.is_finite() && f.area > 0.0 && f.deducted == 0.0) {
            return Err(format!("face {}: area {}", f.edge, f.area));
        }
        // Newell area vector.
        let mut av = [0.0; 3];
        let mut perim = 0.0;
        for i in 0..m {
            let (p, q) = (f.polygon[i], f.polygon[(i + 1) % m]);
            let c = cross3(p, q);
            av = [av[0] + c[0] / 2.0, av[1] + c[1] / 2.0, av[2] + c[2] / 2.0];
            perim += norm3(sub3(q, p));
        }
        let a3 = norm3(av);
        if a3 < 1e-6 {
            return Err(format!("face {}: empty polygon", f.edge));
        }
        if (f.area - a3).abs() > 1e-4 * a3 + 5e-4 + SLIVER * perim {
            return Err(format!(
                "face {}: area {} against polygon {a3}",
                f.edge, f.area
            ));
        }
        plan_sum += av[2].abs();
        let pitch = (av[2].abs() / a3).clamp(-1.0, 1.0).acos().to_degrees();
        if (pitch - f.pitch_deg).abs() > fit.pitch {
            return Err(format!(
                "face {}: lies at {pitch}, reports {}",
                f.edge, f.pitch_deg
            ));
        }
        let nrm = [av[0] / a3, av[1] / a3, av[2] / a3];
        let o = f.polygon[0];
        let off = f
            .polygon
            .iter()
            .map(|v| {
                let d = sub3(*v, o);
                (d[0] * nrm[0] + d[1] * nrm[1] + d[2] * nrm[2]).abs()
            })
            .fold(0.0, f64::max);
        if off > fit.planar {
            return Err(format!("face {}: not planar ({off} m)", f.edge));
        }
        let of_wall = (f.pitch_deg - e.pitch_deg).abs() <= fit.pitch
            || e.half_hip
                .as_ref()
                .is_some_and(|h| (f.pitch_deg - h.pitch_deg).abs() <= fit.pitch)
            || e.breaks
                .iter()
                .any(|b| (f.pitch_deg - b.pitch_deg).abs() <= fit.pitch);
        if !of_wall {
            return Err(format!(
                "face {}: pitch {} is not the wall's",
                f.edge, f.pitch_deg
            ));
        }
        // The plane at the wall line: the wall's eave height (plus the
        // rise over the overhang).
        if let Some((_, want)) = wall_plane(part, f.edge)
            .filter(|_| (f.pitch_deg - e.pitch_deg).abs() <= fit.pitch && nrm[2].abs() > 1e-6)
        {
            let b = part.edges[(f.edge + 1) % n].start;
            for p in [e.start, b] {
                let z = o[2] - (nrm[0] * (p.x - o[0]) + nrm[1] * (p.y - o[1])) / nrm[2];
                if (z - want).abs() > EAVE_TOL {
                    return Err(format!(
                        "face {}: {z} m over the wall line, the eave height gives {want}",
                        f.edge
                    ));
                }
            }
        }
        has_face[f.edge] = true;
        let up = if nrm[2] < 0.0 {
            [-nrm[0], -nrm[1], -nrm[2]]
        } else {
            nrm
        };
        face_planes.push((up, o, segs.len()));
        // A simple outline: no corner on a side it does not belong to (a
        // spike, a pinch point).
        // (Corners the core left a few millimetres apart are one corner.)
        let mut ring: Vec<[f64; 3]> = Vec::with_capacity(m);
        for v in &f.polygon {
            if ring
                .last()
                .is_none_or(|l| norm3(sub3(*v, *l)) >= 2.0 * SPIKE)
            {
                ring.push(*v);
            }
        }
        while ring.len() > 1 && norm3(sub3(ring[0], ring[ring.len() - 1])) < 2.0 * SPIKE {
            ring.pop();
        }
        let r = ring.len();
        for (i, v) in ring.iter().enumerate() {
            for j in 0..r {
                let apart = j != i && (j + 1) % r != i;
                if apart && seg_dist3(*v, ring[j], ring[(j + 1) % r]) < SPIKE {
                    return Err(format!("face {}: outline touches itself at {v:?}", f.edge));
                }
            }
        }
        for i in 0..m {
            let (a, b) = (f.polygon[i], f.polygon[(i + 1) % m]);
            let len = norm3(sub3(b, a));
            if len < 1e-4 {
                continue;
            }
            // A side along another wall's eave line is that eave: a face
            // ending there at another height ends in the air under (or
            // over) that eave instead of at a wall.
            for (w, pl) in planes.iter().enumerate() {
                let (Some(_), true) = (pl, len >= NEAR && part.edges[w].overhang >= NEAR) else {
                    continue;
                };
                let (s, t) = (part.edges[w].start, part.edges[(w + 1) % n].start);
                let wl = s.distance(t);
                let (ux, uy) = ((t.x - s.x) / wl, (t.y - s.y) / wl);
                let ov = part.edges[w].overhang;
                // Across (outward) and along the wall, from its start.
                let at = |q: [f64; 3]| {
                    let (dx, dy) = (q[0] - s.x, q[1] - s.y);
                    (dx * uy - dy * ux - ov, dx * ux + dy * uy)
                };
                let ((ca, la), (cb, lb)) = (at(a), at(b));
                let mid = (la + lb) / 2.0;
                let on_eave_line =
                    ca.abs() < EAVE_TOL && cb.abs() < EAVE_TOL && mid > -ov && mid < wl + ov;
                let h = part.edges[w].eave_height;
                if on_eave_line && ((a[2] - h).abs() > NEAR || (b[2] - h).abs() > NEAR) {
                    return Err(format!(
                        "face {}: side {a:?}-{b:?} on wall {w}'s eave line is not at its eave height",
                        f.edge
                    ));
                }
            }
            sides += len;
            segs.push((a, b));
            if len >= NEAR && !geo.lines.iter().any(|l| along(l.a, l.b, a, b)) {
                return Err(format!("face {}: side {a:?}-{b:?} has no line", f.edge));
            }
        }
    }
    for l in &geo.lines {
        let on_side = l.length() < NEAR
            || segs
                .iter()
                .any(|&(a, b)| along(a, b, l.a, l.b) || along(l.a, l.b, a, b));
        if !on_side {
            return Err(format!(
                "{} line {:?}-{:?} bounds no face",
                l.dacad_code(),
                l.a,
                l.b
            ));
        }
    }
    // Two faces of one plane side by side are one face: a line between
    // them (a valley, a verge) is no roof line.
    for (i, &(ni, oi, si)) in face_planes.iter().enumerate() {
        for (j, &(nj, oj, sj)) in face_planes.iter().enumerate().skip(i + 1) {
            let same = ni[0] * nj[0] + ni[1] * nj[1] + ni[2] * nj[2] > 1.0 - 1e-6 && {
                let d = sub3(oj, oi);
                (d[0] * ni[0] + d[1] * ni[1] + d[2] * ni[2]).abs() < EAVE_TOL
            };
            if !same {
                continue;
            }
            let end = |k: usize| face_planes.get(k + 1).map_or(segs.len(), |f| f.2);
            let (mine, theirs) = (&segs[si..end(i)], &segs[sj..end(j)]);
            let shared = mine.iter().any(|&(a, b)| {
                norm3(sub3(b, a)) >= 2.0 * NEAR
                    && theirs
                        .iter()
                        .any(|&(c, d)| along(c, d, a, b) || along(a, b, c, d))
            });
            if shared {
                return Err(format!(
                    "faces {} and {} lie in one plane side by side",
                    geo.faces[i].edge, geo.faces[j].edge
                ));
            }
        }
    }
    // Every sloped wall has its roof.
    for i in 0..n {
        let Some((mi, ci)) = planes[i] else {
            continue;
        };
        if has_face[i] {
            continue;
        }
        let d = |k: usize, m: (f64, f64)| m.0 * part.edges[k].start.x + m.1 * part.edges[k].start.y;
        let shared = (0..n).any(|j| {
            let Some((mj, cj)) = planes[j] else {
                return false;
            };
            let tan = part.edges[i].pitch_deg.to_radians().tan();
            has_face[j]
                && mi.0 * mj.0 + mi.1 * mj.1 > 1.0 - PARALLEL_COS
                && (part.edges[i].pitch_deg - part.edges[j].pitch_deg).abs() <= SAME_PITCH
                && ((ci + tan * d(i, mi)) - (cj + tan * d(j, mi))).abs() <= EAVE_TOL
        });
        if !shared {
            return Err(format!("wall {i} has no roof face"));
        }
    }
    // Plan coverage of the eave outline.
    let mut outline = MultiPolygon::new(vec![]);
    for piece in eave_outline(part) {
        if piece.len() >= 3 && signed_area(&piece).abs() > 1e-12 {
            outline = outline.union(&MultiPolygon::new(vec![to_geo(&piece)]));
        }
    }
    let plan = outline.unsigned_area();
    let tiling = 1e-4 * plan.max(1.0) + 1e-3 * sides;
    let mut eaves = part
        .edges
        .iter()
        .filter(|e| !e.is_gable())
        .map(|e| e.eave_height);
    let first = eaves.next().unwrap_or(0.0);
    let level = eaves.all(|h| (h - first).abs() < 1e-9);
    let most = if level {
        plan + tiling
    } else {
        plan * (1.0 + OVER_COVER) + tiling
    };
    if !(plan_sum >= plan - tiling && plan_sum <= most) {
        return Err(format!("faces cover {plan_sum} m2 of {plan} m2 in plan"));
    }
    let warnings: Vec<RoofWarning> = check(std::slice::from_ref(part), std::slice::from_ref(geo))
        .into_iter()
        .flatten()
        .chain(geo.warnings.iter().cloned())
        .filter(|w| !w.kind.is_notice() && w.kind != WarningKind::MergedWalls)
        .collect();
    if let Some(w) = warnings.first() {
        return Err(format!("warning {:?} {}", w.kind, w.value));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DEFAULT_MATERIAL, EdgeDef, Project};

    fn edge(x: f64, y: f64, pitch_deg: f64, eave_height: f64) -> EdgeDef {
        EdgeDef {
            start: Point2::new(x, y),
            pitch_deg,
            eave_height,
            overhang: 0.5,
            half_hip: None,
            breaks: Vec::new(),
            priority: 0,
            direction_rad: None,
            clad: false,
            source: None,
            material: DEFAULT_MATERIAL,
        }
    }

    /// A 10 × 8 m roof, the short walls hips or gables, as the standard
    /// calculation gives it.
    fn house(end_pitch: f64) -> (RoofPart, RoofGeometry) {
        let part = RoofPart::new(
            "A",
            vec![
                edge(0.0, 0.0, 40.0, 3.0),
                edge(10.0, 0.0, end_pitch, 3.0),
                edge(10.0, 8.0, 40.0, 3.0),
                edge(0.0, 8.0, end_pitch, 3.0),
            ],
        );
        let mut p = Project::new("house");
        p.parts.push(part.clone());
        let geo = crate::engine::generate(&p).expect("calculates").remove(0);
        (part, geo)
    }

    #[test]
    fn standard_roofs_are_sound() {
        for end in [40.0, 90.0] {
            let (part, geo) = house(end);
            assert_eq!(sound(&part, &geo, Fit::STANDARD), Ok(()), "ends {end}");
        }
    }

    fn refused(part: &RoofPart, geo: &RoofGeometry, why: &str) {
        let got = sound(part, geo, Fit::STANDARD);
        assert!(
            got.as_ref().is_err_and(|e| e.contains(why)),
            "want an error with {why:?}, got {got:?}"
        );
    }

    #[test]
    fn a_roof_without_faces_is_refused() {
        let (part, mut geo) = house(40.0);
        geo.faces.clear();
        refused(&part, &geo, "no faces");
    }

    #[test]
    fn a_face_with_an_area_that_is_not_its_polygons_is_refused() {
        let (part, mut geo) = house(40.0);
        geo.faces[0].area *= 1.5;
        refused(&part, &geo, "against polygon");
        geo.faces[0].area = -1.0;
        refused(&part, &geo, "area");
        geo.faces[0].area = f64::NAN;
        refused(&part, &geo, "area");
    }

    #[test]
    fn a_face_emitted_twice_is_refused() {
        let (part, mut geo) = house(40.0);
        geo.faces.push(geo.faces[0].clone());
        assert!(sound(&part, &geo, Fit::STANDARD).is_err());
    }

    /// The roof of the same house with one eave 1 m higher: its faces are
    /// planar, tile the plan and carry their lines, but one wall's roof
    /// is not at that wall's eave height.
    #[test]
    fn a_roof_that_ignores_an_eave_height_is_refused() {
        let (mut part, geo) = house(90.0);
        part.edges[0].eave_height = 2.0;
        refused(&part, &geo, "the eave height gives");
    }

    #[test]
    fn a_wall_without_a_roof_face_is_refused() {
        let (part, mut geo) = house(40.0);
        let gone = geo.faces.remove(1);
        // Its neighbours' lines stay, so the first thing missing is the
        // plan it covered or the face itself.
        let got = sound(&part, &geo, Fit::STANDARD);
        assert!(got.is_err(), "face of wall {} removed", gone.edge);
    }

    #[test]
    fn a_face_side_without_a_line_and_a_line_without_a_face_are_refused() {
        let (part, geo) = house(40.0);
        let mut no_ridge = geo.clone();
        no_ridge
            .lines
            .retain(|l| l.kind != crate::roof::LineKind::Ridge);
        refused(&part, &no_ridge, "has no line");
        let mut stray = geo.clone();
        let mut l = stray.lines[0].clone();
        l.a = [2.0, 2.0, 9.0];
        l.b = [5.0, 5.0, 9.0];
        stray.lines.push(l);
        refused(&part, &stray, "bounds no face");
    }

    #[test]
    fn a_face_off_its_pitch_or_out_of_its_plane_is_refused() {
        let (part, geo) = house(40.0);
        let mut bent = geo.clone();
        // One corner of the largest face lifted by 5 cm.
        let k = (0..bent.faces.len())
            .max_by(|&a, &b| bent.faces[a].area.total_cmp(&bent.faces[b].area))
            .unwrap();
        bent.faces[k].polygon[0][2] += 0.05;
        assert!(sound(&part, &bent, Fit::STANDARD).is_err());
        let mut mislabelled = geo.clone();
        mislabelled.faces[k].pitch_deg = 35.0;
        refused(&part, &mislabelled, "reports 35");
    }

    #[test]
    fn a_face_whose_outline_touches_itself_is_refused() {
        let (part, mut geo) = house(90.0);
        // A spike: out along one side and back.
        let f = &mut geo.faces[0];
        let (a, b) = (f.polygon[0], f.polygon[1]);
        let mid = [
            (a[0] + b[0]) / 2.0,
            (a[1] + b[1]) / 2.0,
            (a[2] + b[2]) / 2.0,
        ];
        f.polygon.insert(1, a);
        f.polygon.insert(1, mid);
        refused(&part, &geo, "touches itself");
    }

    #[test]
    fn error_walls_only_for_planes_that_can_be_the_cause() {
        use crate::roof::CalcErrorKind as K;
        // Jakub's T: wing front (wall 0) between gables, its plane in the
        // house's front plane (wall 2).
        let t = |front: f64| {
            RoofPart::new(
                "T",
                vec![
                    edge(5.0, 0.0, 40.0, front),
                    edge(10.0, 0.0, 90.0, 5.0),
                    edge(10.0, 2.5, 40.0, 5.0),
                    edge(15.0, 2.5, 90.0, 5.0),
                    edge(15.0, 7.5, 40.0, 5.0),
                    edge(0.0, 7.5, 90.0, 5.0),
                    edge(0.0, 2.5, 40.0, 5.0),
                    edge(5.0, 2.5, 90.0, 5.0),
                ],
            )
        };
        assert_eq!(coplanar_error_walls(&t(2.9), K::Cut), [0, 2]);
        assert_eq!(coplanar_error_walls(&t(2.9), K::OpenFaces), [0, 2]);
        assert_eq!(coplanar_error_walls(&t(2.5), K::Cut), [0, 2]);
        // Not for an object's error or an invalid wall.
        assert!(coplanar_error_walls(&t(2.9), K::NonConvexOpening).is_empty());
        assert!(coplanar_error_walls(&t(2.9), K::Geometry).is_empty());
        // Not 0.9 m below, and not above (that roof calculates).
        assert!(coplanar_error_walls(&t(2.0), K::Cut).is_empty());
        assert!(coplanar_error_walls(&t(3.2), K::Cut).is_empty());
    }

    #[test]
    fn walls_turned_and_rounded_to_millimetres_are_still_parallel() {
        // The T turned by 17.3°, corners rounded to mm.
        let (s, c) = 17.3f64.to_radians().sin_cos();
        let mm = |v: f64| (v * 1000.0).round() / 1000.0;
        let pts = [
            (5.0, 0.0, 40.0, 2.9),
            (10.0, 0.0, 90.0, 5.0),
            (10.0, 2.5, 40.0, 5.0),
            (15.0, 2.5, 90.0, 5.0),
            (15.0, 7.5, 40.0, 5.0),
            (0.0, 7.5, 90.0, 5.0),
            (0.0, 2.5, 40.0, 5.0),
            (5.0, 2.5, 90.0, 5.0),
        ];
        let part = RoofPart::new(
            "T",
            pts.iter()
                .map(|&(x, y, p, h)| edge(mm(x * c - y * s), mm(x * s + y * c), p, h))
                .collect(),
        );
        let wings = wing_planes(&part);
        // The wing's front against both halves of the house's front.
        let mut inner: Vec<usize> = wings
            .iter()
            .filter(|p| p.outer == 0)
            .map(|p| p.inner)
            .collect();
        inner.sort_unstable();
        assert_eq!(inner, [2, 6], "{wings:?}");
        assert!(wings.iter().all(|p| p.rise.abs() < 0.01), "{wings:?}");
    }
}
