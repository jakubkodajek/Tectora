//! Roof windows and chimneys, applied to generated roof faces.
//!
//! Checked against roofs calculated by DACAD itself (cases C24–C28):
//!
//! * A window deducts `width × height` from the face it sits on (both
//!   measured in the roof plane) and adds no lines.
//! * A chimney is a plan polygon. Each face loses the part of it inside the
//!   face, as plan area / cos(pitch). Its sides become wall-abutment lines:
//!   WV on the downhill side and WH uphill for sides level in the roof
//!   plane, WS for all others. Roof lines
//!   (e.g. the ridge) inside the chimney are removed.
//! * A roof opening (DACAD: Öffnung) is cut like a chimney, but per DACAD.HLP
//!   (topic "Öffnung") its sides are roof edges: the uphill side is an eave
//!   of the roof above, the downhill side a Pult of the roof below and the
//!   others verges (OL/OR when square to the eave, otherwise OS). Its full
//!   sloped area is deducted under every norm. Checked against DACAD runs
//!   OP1-OP4 (/mnt/project-files/dacad-round/openings/).

use geo::{Area, BooleanOps, MultiPolygon};

use crate::geom::{Point2, signed_area};
use crate::model::{Chimney, Project, RoofPart, RoofWindow};
use crate::roof::{
    LineKind, Opening, OpeningKind, RoofFace, RoofGeometry, RoofLine, VergeSide, generate,
};
use crate::union::to_geo;

const TOL: f64 = 1e-9;
/// A side counts as level in the roof plane (WV/WH) only when it is square
/// to the slope; every oblique side is WS (DACAD cases C32–C36).
const LEVEL: f64 = 1.0 - 1e-6;

/// Generates every part of the project with its dormers, windows and
/// chimneys applied.
pub fn generate_project(project: &Project) -> Vec<RoofGeometry> {
    // Parts with standing gables are joined as several buildings.
    let cuts: Vec<crate::gable_wall::Pieces> = project
        .parts
        .iter()
        .map(|part| {
            let mut c = crate::gable_wall::cut(part);
            if c.cuts.is_empty() {
                c.parts[0] = crate::roof::snap_in_step_planes(part);
            }
            c
        })
        .collect();
    let pieces: Vec<RoofPart> = cuts.iter().flat_map(|c| c.parts.clone()).collect();
    let mut all: Vec<RoofGeometry> = pieces.iter().map(generate).collect();
    crate::union::join(&mut all, &pieces);
    let mut geos = Vec::with_capacity(cuts.len());
    let mut rest = all.into_iter();
    for c in &cuts {
        let mut mine: Vec<RoofGeometry> = rest.by_ref().take(c.parts.len()).collect();
        for &(lo, hi, a, b, top) in &c.cuts {
            let mut higher = std::mem::take(&mut mine[hi]);
            crate::gable_wall::abut(&mut mine[lo], &mut higher, a, b, top);
            mine[hi] = higher;
        }
        let mut geo = RoofGeometry::default();
        for (g, map) in mine.into_iter().zip(&c.edges) {
            geo.faces.extend(g.faces.into_iter().map(|mut f| {
                f.edge = map[f.edge];
                f
            }));
            geo.lines.extend(g.lines);
            geo.gables.extend(g.gables);
            geo.clad_area += g.clad_area;
            geo.openings.extend(g.openings);
            geo.cut_lines.extend(g.cut_lines);
            geo.minus_lines.extend(g.minus_lines);
        }
        geos.push(geo);
    }
    for d in &project.dormers {
        crate::dormer::apply_dormer(&mut geos, d);
    }
    for w in &project.windows {
        apply_window(&mut geos, w);
    }
    for c in &project.chimneys {
        for geo in &mut geos {
            apply_cutout(geo, c, false);
        }
    }
    for o in &project.roof_openings {
        for geo in &mut geos {
            apply_cutout(geo, o, true);
        }
    }
    for geo in &mut geos {
        merge_collinear(&mut geo.lines);
        split_verges_at_knicks(&mut geo.lines);
    }
    geos
}

/// Joins lines of the same kind that continue each other, as DACAD lists
/// them: cutting roofs against each other leaves them in pieces.
pub(crate) fn merge_collinear(lines: &mut Vec<RoofLine>) {
    let dir = |l: &RoofLine| {
        let d = [l.b[0] - l.a[0], l.b[1] - l.a[1], l.b[2] - l.a[2]];
        let n = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-12);
        [d[0] / n, d[1] / n, d[2] / n]
    };
    let near = |p: [f64; 3], q: [f64; 3]| {
        (p[0] - q[0]).abs() < 1e-6 && (p[1] - q[1]).abs() < 1e-6 && (p[2] - q[2]).abs() < 1e-6
    };
    lines.retain(|l| l.length() > 1e-6);
    // Two walls' eaves stay apart where another line starts between them,
    // even on one straight line (mass case R1_0229).
    let ends: Vec<[f64; 3]> = lines
        .iter()
        .filter(|l| l.kind != LineKind::Eave)
        .flat_map(|l| [l.a, l.b])
        .collect();
    let split =
        |l: &RoofLine, p: [f64; 3]| l.kind == LineKind::Eave && ends.iter().any(|&e| near(e, p));
    loop {
        let mut joined = false;
        'outer: for i in 0..lines.len() {
            for j in 0..lines.len() {
                if i == j || lines[i].dacad_code() != lines[j].dacad_code() {
                    continue;
                }
                let (di, dj) = (dir(&lines[i]), dir(&lines[j]));
                let cos = di[0] * dj[0] + di[1] * dj[1] + di[2] * dj[2];
                if cos > 1.0 - 1e-9 && near(lines[i].b, lines[j].a) {
                    if split(&lines[i], lines[i].b) {
                        continue;
                    }
                    lines[i].b = lines[j].b;
                } else if cos < -1.0 + 1e-9 && near(lines[i].b, lines[j].b) {
                    if split(&lines[i], lines[i].b) {
                        continue;
                    }
                    lines[i].b = lines[j].a;
                } else {
                    continue;
                }
                lines.swap_remove(j);
                joined = true;
                break 'outer;
            }
        }
        if !joined {
            break;
        }
    }
}

/// DACAD ends a verge where a pitch break (Knick) meets it, even when both
/// definitions have the same pitch and the verge runs on straight (mass
/// cases R1_0166, R1_0197).
fn split_verges_at_knicks(lines: &mut Vec<RoofLine>) {
    let ends: Vec<[f64; 3]> = lines
        .iter()
        .filter(|l| l.kind == LineKind::Knick)
        .flat_map(|l| [l.a, l.b])
        .collect();
    let mut out = Vec::with_capacity(lines.len());
    for l in lines.drain(..) {
        if l.kind != LineKind::Verge {
            out.push(l);
            continue;
        }
        let len = l.length();
        let mut ts: Vec<f64> = ends
            .iter()
            .filter_map(|&p| {
                let t = (0..3)
                    .map(|k| (p[k] - l.a[k]) * (l.b[k] - l.a[k]))
                    .sum::<f64>()
                    / (len * len);
                let foot = lerp3(l.a, l.b, t);
                let d = (0..3).map(|k| (p[k] - foot[k]).powi(2)).sum::<f64>().sqrt();
                (t * len > 1e-6 && (1.0 - t) * len > 1e-6 && d < 1e-6).then_some(t)
            })
            .collect();
        ts.sort_by(f64::total_cmp);
        ts.dedup_by(|x, y| (*x - *y).abs() * len < 1e-6);
        let mut t0 = 0.0;
        for t in ts.into_iter().chain(std::iter::once(1.0)) {
            out.push(RoofLine {
                a: lerp3(l.a, l.b, t0),
                b: lerp3(l.a, l.b, t),
                ..l.clone()
            });
            t0 = t;
        }
    }
    *lines = out;
}

/// Plan outline of a face.
pub(crate) fn plan(face: &RoofFace) -> Vec<Point2> {
    face.polygon
        .iter()
        .map(|p| Point2::new(p[0], p[1]))
        .collect()
}

/// Unit uphill direction in plan and `tan(pitch)` of a face's plane.
pub(crate) fn slope(face: &RoofFace) -> (Point2, f64) {
    let (mut nx, mut ny, mut nz) = (0.0, 0.0, 0.0);
    let poly = &face.polygon;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        nx += (a[1] - b[1]) * (a[2] + b[2]);
        ny += (a[2] - b[2]) * (a[0] + b[0]);
        nz += (a[0] - b[0]) * (a[1] + b[1]);
    }
    if nz < 0.0 {
        (nx, ny, nz) = (-nx, -ny, -nz);
    }
    let h = nx.hypot(ny);
    if h < TOL || nz < TOL {
        return (Point2::new(0.0, 0.0), 0.0);
    }
    (Point2::new(-nx / h, -ny / h), h / nz)
}

fn dist3_(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

pub(crate) fn inside(poly: &[Point2], p: Point2) -> bool {
    let mut c = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a.y > p.y) != (b.y > p.y) {
            let x = a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y);
            if p.x < x {
                c = !c;
            }
        }
    }
    c
}

/// The window's corner lies on the face's boundary at the eave; test a
/// point just inside it, up the slope and along the eave.
fn apply_window(geos: &mut [RoofGeometry], w: &RoofWindow) {
    for geo in geos.iter_mut() {
        let hit = geo.faces.iter().position(|face| {
            let (up, _) = slope(face);
            let along = Point2::new(up.y, -up.x);
            let probe = |s: f64| {
                Point2::new(
                    w.x + 1e-4 * up.x + s * 1e-4 * along.x,
                    w.y + 1e-4 * up.y + s * 1e-4 * along.y,
                )
            };
            let poly = plan(face);
            inside(&poly, probe(1.0)) || inside(&poly, probe(-1.0))
        });
        if let Some(k) = hit {
            let d = w.width * w.height;
            geo.faces[k].area -= d;
            geo.faces[k].deducted += d;
            geo.openings.push(Opening {
                kind: OpeningKind::Window,
                area: d,
            });
            return;
        }
    }
}

/// Sutherland–Hodgman: `subject` clipped by the convex CCW polygon `clip`.
pub(crate) fn clip_polygon(subject: &[Point2], clip: &[Point2]) -> Vec<Point2> {
    let mut out = subject.to_vec();
    let n = clip.len();
    for i in 0..n {
        let (a, b) = (clip[i], clip[(i + 1) % n]);
        let side = |p: Point2| (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
        let input = std::mem::take(&mut out);
        let m = input.len();
        for k in 0..m {
            let (p, q) = (input[k], input[(k + 1) % m]);
            let (sp, sq) = (side(p), side(q));
            if sp >= -TOL {
                out.push(p);
            }
            if (sp >= -TOL) != (sq >= -TOL) {
                let t = sp / (sp - sq);
                out.push(Point2::new(p.x + t * (q.x - p.x), p.y + t * (q.y - p.y)));
            }
        }
        if out.is_empty() {
            break;
        }
    }
    out
}

/// Parameter intervals of segment `a`–`b` that lie inside `poly`.
fn inside_spans(poly: &[Point2], a: Point2, b: Point2) -> Vec<(f64, f64)> {
    let d = Point2::new(b.x - a.x, b.y - a.y);
    let mut ts = vec![0.0, 1.0];
    let n = poly.len();
    for i in 0..n {
        let (p, q) = (poly[i], poly[(i + 1) % n]);
        let e = Point2::new(q.x - p.x, q.y - p.y);
        let den = d.x * e.y - d.y * e.x;
        if den.abs() < TOL {
            continue;
        }
        let w = Point2::new(p.x - a.x, p.y - a.y);
        let t = (w.x * e.y - w.y * e.x) / den;
        let u = (w.x * d.y - w.y * d.x) / den;
        if (-TOL..=1.0 + TOL).contains(&u) && t > 0.0 && t < 1.0 {
            ts.push(t);
        }
    }
    ts.sort_by(f64::total_cmp);
    ts.windows(2)
        .filter(|s| s[1] - s[0] > 1e-9)
        .filter(|s| {
            let m = (s[0] + s[1]) / 2.0;
            inside(poly, Point2::new(a.x + m * d.x, a.y + m * d.y))
        })
        .map(|s| (s[0], s[1]))
        .collect()
}

pub(crate) fn lerp3(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    [
        a[0] + t * (b[0] - a[0]),
        a[1] + t * (b[1] - a[1]),
        a[2] + t * (b[2] - a[2]),
    ]
}

/// Height of a face's plane at a plan point.
fn height_at(face: &RoofFace, up: Point2, tan: f64, p: Point2) -> f64 {
    let o = face.polygon[0];
    o[2] + tan * ((p.x - o[0]) * up.x + (p.y - o[1]) * up.y)
}

/// Where two plan lines meet.
fn cross_lines(a: Point2, b: Point2, c: Point2, d: Point2) -> Option<Point2> {
    let (u, v) = (
        Point2::new(b.x - a.x, b.y - a.y),
        Point2::new(d.x - c.x, d.y - c.y),
    );
    let den = u.x * v.y - u.y * v.x;
    if den.abs() < 1e-12 {
        return None;
    }
    let t = ((c.x - a.x) * v.y - (c.y - a.y) * v.x) / den;
    Some(Point2::new(a.x + t * u.x, a.y + t * u.y))
}

/// DACAD extends each chimney side across its face from where it crosses
/// the eave. Where that line passes less than POINT_EPS (2 cm) beyond the
/// end of the face's ridge, the side is turned to run through the ridge's
/// end, pivoting on the eave crossing, and its two corners slide along the
/// neighbouring sides. Crossings on the ridge itself stay (sweep
/// /mnt/project-files/dacad-chimney-ridge-end/: 19.9 mm snapped, 20.4 mm
/// not; mass case R1_0231).
fn snap_sides_to_ridge_ends(geo: &RoofGeometry, corners: &mut [Point2]) {
    const POINT_EPS: f64 = 0.02;
    let n = corners.len();
    let mid = Point2::new(
        corners.iter().map(|p| p.x).sum::<f64>() / n as f64,
        corners.iter().map(|p| p.y).sum::<f64>() / n as f64,
    );
    let Some(face) = geo.faces.iter().find(|f| inside(&plan(f), mid)) else {
        return;
    };
    let on_face = |l: &RoofLine| {
        let has = |p: [f64; 3]| {
            face.polygon
                .iter()
                .any(|q| (0..3).all(|i| (p[i] - q[i]).abs() < 1e-6))
        };
        has(l.a) && has(l.b)
    };
    let flat = |p: [f64; 3]| Point2::new(p[0], p[1]);
    let lines_of = |k: LineKind| -> Vec<(Point2, Point2)> {
        geo.lines
            .iter()
            .filter(|l| l.kind == k && on_face(l))
            .map(|l| (flat(l.a), flat(l.b)))
            .collect()
    };
    let (eaves, ridges) = (lines_of(LineKind::Eave), lines_of(LineKind::Ridge));
    for i in 0..n {
        let (p, q) = (corners[i], corners[(i + 1) % n]);
        let Some(e) = eaves.iter().find_map(|&(a, b)| {
            let x = cross_lines(p, q, a, b)?;
            let len = a.distance(b);
            let t = ((x.x - a.x) * (b.x - a.x) + (x.y - a.y) * (b.y - a.y)) / len;
            (-TOL..=len + TOL).contains(&t).then_some(x)
        }) else {
            continue;
        };
        let end = ridges.iter().find_map(|&(a, b)| {
            let x = cross_lines(p, q, a, b)?;
            let len = a.distance(b);
            let t = ((x.x - a.x) * (b.x - a.x) + (x.y - a.y) * (b.y - a.y)) / len;
            if t < 0.0 && -t < POINT_EPS {
                Some(a)
            } else if t > len && t - len < POINT_EPS {
                Some(b)
            } else {
                None
            }
        });
        let Some(r) = end else {
            continue;
        };
        let (pp, qq) = (corners[(i + n - 1) % n], corners[(i + 2) % n]);
        if let (Some(a), Some(b)) = (cross_lines(e, r, pp, p), cross_lines(e, r, q, qq)) {
            corners[i] = a;
            corners[(i + 1) % n] = b;
        }
    }
}

/// Cuts a chimney, or with `hole` a roof opening, out of the faces.
fn apply_cutout(geo: &mut RoofGeometry, c: &Chimney, hole: bool) {
    let mut corners = c.corners.clone();
    if corners.len() < 3 {
        return;
    }
    if signed_area(&corners) < 0.0 {
        corners.reverse();
    }
    if !hole {
        snap_sides_to_ridge_ends(geo, &mut corners);
        cut_chimney(geo, &corners);
        return;
    }
    // Roof lines under the opening disappear.
    let mut kept: Vec<RoofLine> = Vec::new();
    for l in std::mem::take(&mut geo.lines) {
        let (a, b) = (Point2::new(l.a[0], l.a[1]), Point2::new(l.b[0], l.b[1]));
        let under = inside_spans(&corners, a, b);
        for &(s, e) in &under {
            geo.cut_lines
                .push((l.dacad_code().to_string(), (e - s) * l.length()));
        }
        let mut t0 = 0.0;
        for (s, e) in under.iter().copied().chain(std::iter::once((1.0, 1.0))) {
            if s - t0 > 1e-9 {
                kept.push(RoofLine {
                    a: lerp3(l.a, l.b, t0),
                    b: lerp3(l.a, l.b, s),
                    ..l.clone()
                });
            }
            t0 = e;
        }
    }
    geo.lines = kept;
    let outline = corners.clone();
    let mut total = 0.0;
    let n = outline.len();
    for face in &mut geo.faces {
        let poly = plan(face);
        let (up, tan) = slope(face);
        let cut = signed_area(&clip_polygon(&poly, &corners)).abs();
        if cut < 1e-9 {
            continue;
        }
        let d = cut * (1.0 + tan * tan).sqrt();
        face.area -= d;
        face.deducted += d;
        total += d;
        for i in 0..n {
            let (a, b) = (outline[i], outline[(i + 1) % n]);
            let dir = Point2::new(b.x - a.x, b.y - a.y);
            let len = dir.x.hypot(dir.y);
            if len < TOL {
                continue;
            }
            // Outward normal of a CCW polygon side.
            let out = (dir.y / len) * up.x - (dir.x / len) * up.y;
            let (kind, side) = if hole {
                // Outward normal across the slope, + to the right looking uphill.
                let across = (dir.y / len) * up.y + (dir.x / len) * up.x;
                if out < -LEVEL {
                    (LineKind::Pult, None)
                } else if out > LEVEL {
                    (LineKind::Eave, None)
                } else if across > LEVEL {
                    // The roof right of the hole ends here on its left.
                    (LineKind::Verge, Some(VergeSide::Left))
                } else if across < -LEVEL {
                    (LineKind::Verge, Some(VergeSide::Right))
                } else {
                    (LineKind::Verge, Some(VergeSide::Oblique))
                }
            } else if out < -LEVEL {
                (LineKind::AbutmentFront, None)
            } else if out > LEVEL {
                (LineKind::AbutmentBack, None)
            } else {
                (LineKind::AbutmentSide, None)
            };
            for (s, e) in inside_spans(&poly, a, b) {
                let p = Point2::new(a.x + s * dir.x, a.y + s * dir.y);
                let q = Point2::new(a.x + e * dir.x, a.y + e * dir.y);
                geo.lines.push(RoofLine {
                    kind,
                    a: [p.x, p.y, height_at(face, up, tan, p)],
                    b: [q.x, q.y, height_at(face, up, tan, q)],
                    side,
                });
            }
        }
    }
    if total > 0.0 {
        geo.openings.push(Opening {
            kind: if hole {
                OpeningKind::Hole
            } else {
                OpeningKind::Chimney
            },
            area: total,
        });
    }
}

/// One side of a chimney hole in one face.
struct HoleEdge {
    c: [f64; 3],
    d: [f64; 3],
    face: usize,
    shared: bool,
}

/// A chimney as DACAD's core cuts it (decompiled FUN_00da5160 and its
/// callers): every face gets its own hole, the chimney clipped by the face,
/// with corners closer than POINT_EPS in every coordinate (or VALUE_EPS in
/// plan) merged; a hole left with fewer than three corners is dropped
/// (DACAD sweep dacad-chimney-across-line, V45_02 against H45_02). Roof
/// lines stay whole. A hole edge lying on a roof line (see [`on_line`])
/// becomes a negative line of that line's kind; a hole edge of only one
/// face is also an abutment, unless the line is a verge, Pult or eave.
fn cut_chimney(geo: &mut RoofGeometry, corners: &[Point2]) {
    let mut corners = corners.to_vec();
    for _ in 0..corners.len() {
        if !pivot_corner(geo, &mut corners) {
            break;
        }
    }
    let mut edges: Vec<HoleEdge> = Vec::new();
    let mut total = 0.0;
    for fi in 0..geo.faces.len() {
        let Some((pts, _)) = face_hole(&geo.faces[fi], &corners) else {
            continue;
        };
        // The roof under a dormer is already gone (DACAD variants of mass
        // case R1_0398).
        let face = &mut geo.faces[fi];
        let (_, tan) = slope(face);
        let mut under = MultiPolygon::new(vec![to_geo(&clip_polygon(&plan(face), &corners))]);
        // Dormer faces have no footprint edge.
        for f in geo.footprints.iter().filter(|_| face.edge != usize::MAX) {
            under = under.difference(&MultiPolygon::new(vec![to_geo(f)]));
        }
        let cut = under.unsigned_area();
        let d = cut * (1.0 + tan * tan).sqrt();
        face.area -= d;
        face.deducted += d;
        total += d;
        for i in 0..pts.len() {
            edges.push(HoleEdge {
                c: pts[i],
                d: pts[(i + 1) % pts.len()],
                face: fi,
                shared: false,
            });
        }
    }
    // An edge two holes share lies on the line between their faces; the
    // hole of the lower face keeps it.
    let on_edge = |p: [f64; 3], e: &HoleEdge| {
        let (a, b) = (Point2::new(e.c[0], e.c[1]), Point2::new(e.d[0], e.d[1]));
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let l2 = dx * dx + dy * dy;
        if l2 < TOL {
            return false;
        }
        let t = ((p[0] - a.x) * dx + (p[1] - a.y) * dy) / l2;
        (-1e-9..=1.0 + 1e-9).contains(&t) && dist3_(p, lerp3(e.c, e.d, t)) < 1e-4
    };
    let mut drop = vec![false; edges.len()];
    for i in 0..edges.len() {
        let mid = lerp3(edges[i].c, edges[i].d, 0.5);
        if let Some(j) =
            (0..edges.len()).find(|&j| edges[j].face != edges[i].face && on_edge(mid, &edges[j]))
        {
            drop[i] = edges[i].face > edges[j].face;
            edges[i].shared = true;
        }
    }
    let mut keep = drop.iter().map(|d| !d);
    edges.retain(|_| keep.next().unwrap());
    // Lines whole, as DACAD keeps them; ours may still be in pieces. Wall
    // abutments count too (mass case R1_0745: −WS along a step).
    let mut roof = geo.lines.clone();
    merge_collinear(&mut roof);
    for e in &edges {
        // An edge shorter than POINT_EPS gives no line (ck2 B1; the chord
        // under a knick, dacad-chimney-knick).
        if dist3_(e.c, e.d) < 0.02 {
            continue;
        }
        let on = roof.iter().find(|r| on_line(r, e.c, e.d));
        if let Some(r) = on {
            geo.minus_lines
                .push((r.dacad_code().to_string(), dist3_(e.c, e.d)));
        }
        let edge_of_roof =
            on.is_some_and(|r| matches!(r.kind, LineKind::Verge | LineKind::Pult | LineKind::Eave));
        if e.shared || edge_of_roof {
            continue;
        }
        let face = &geo.faces[e.face];
        let (up, _) = slope(face);
        let (dx, dy) = (e.d[0] - e.c[0], e.d[1] - e.c[1]);
        let len = dx.hypot(dy);
        if len < TOL {
            continue;
        }
        // Outward normal of a CCW hole side.
        let out = (dy / len) * up.x - (dx / len) * up.y;
        let kind = if out < -LEVEL {
            LineKind::AbutmentFront
        } else if out > LEVEL {
            LineKind::AbutmentBack
        } else {
            LineKind::AbutmentSide
        };
        geo.lines.push(RoofLine {
            kind,
            a: e.c,
            b: e.d,
            side: None,
        });
    }
    if total > 0.0 {
        geo.openings.push(Opening {
            kind: OpeningKind::Chimney,
            area: total,
        });
    }
}

/// A corner dropped from a hole: the corner's index, the point it merged
/// into and whether that point lies on the side after the corner.
type Merge = (usize, Point2, bool);

/// The chimney's hole in one face, CCW, in 3D: the chimney clipped by the
/// face, without a corner next to the point where a neighbouring side
/// leaves the face that lies near it (every coordinate within POINT_EPS)
/// or within VALUE_EPS of the face's outline in plan. `None` when under three corners are left (DACAD sweeps
/// dacad-chimney-across-line V45_02, dacad-chimney-over-ridge; points where
/// sides cross the face's edge never merge, ck2 B1).
fn face_hole(face: &RoofFace, corners: &[Point2]) -> Option<(Vec<[f64; 3]>, Vec<Merge>)> {
    const POINT_EPS: f32 = 0.02;
    const VALUE_EPS: f32 = 0.01;
    let near = |p: [f64; 3], q: [f64; 3]| (0..3).all(|i| ((p[i] - q[i]) as f32).abs() < POINT_EPS);
    // Plan distance from the face's outline.
    let outline = plan(face);
    let off_edge = |p: Point2| {
        let m = outline.len();
        (0..m)
            .map(|i| {
                let (a, b) = (outline[i], outline[(i + 1) % m]);
                let (dx, dy) = (b.x - a.x, b.y - a.y);
                let l2 = dx * dx + dy * dy;
                let t = if l2 < TOL {
                    0.0
                } else {
                    (((p.x - a.x) * dx + (p.y - a.y) * dy) / l2).clamp(0.0, 1.0)
                };
                (p.x - a.x - t * dx).hypot(p.y - a.y - t * dy)
            })
            .fold(f64::INFINITY, f64::min)
    };
    // Degenerate faces the sweep leaves along a line are not DACAD faces.
    if signed_area(&plan(face)).abs() < 1e-6 {
        return None;
    }
    let mut hole = clip_polygon(&plan(face), corners);
    hole.dedup_by(|a, b| a.distance(*b) < 1e-9);
    while hole.len() > 1 && hole[0].distance(hole[hole.len() - 1]) < 1e-9 {
        hole.pop();
    }
    let cut = signed_area(&hole);
    if cut.abs() < 1e-9 {
        return None;
    }
    if cut < 0.0 {
        hole.reverse();
    }
    let (up, tan) = slope(face);
    let pts: Vec<[f64; 3]> = hole
        .iter()
        .map(|&p| [p.x, p.y, height_at(face, up, tan, p)])
        .collect();
    // A corner on the outline is where its sides leave the face.
    let corner = |p: Point2| {
        corners
            .iter()
            .position(|c| c.distance(p) < 1e-9)
            .filter(|_| off_edge(p) > 1e-9)
    };
    let n = pts.len();
    let mut merges = Vec::new();
    let mut keep = vec![true; n];
    for i in 0..n {
        let Some(k) = corner(hole[i]) else {
            continue;
        };
        for (j, next) in [((i + 1) % n, true), ((i + n - 1) % n, false)] {
            if corner(hole[j]).is_none()
                && (near(pts[i], pts[j]) || (off_edge(hole[i]) as f32) < VALUE_EPS)
            {
                keep[i] = false;
                merges.push((k, hole[j], next));
                break;
            }
        }
    }
    let pts: Vec<[f64; 3]> = pts
        .into_iter()
        .zip(keep)
        .filter_map(|(p, k)| k.then_some(p))
        .collect();
    (pts.len() >= 3).then_some((pts, merges))
}

/// Where a chimney corner merges into the point its neighbouring side
/// leaves a face at (see [`face_hole`]), DACAD turns the corner's other
/// side to run through that point, pivoting on the side's eave crossing
/// (mass case R1_0681). Returns whether a corner moved.
fn pivot_corner(geo: &RoofGeometry, corners: &mut [Point2]) -> bool {
    let n = corners.len();
    for face in &geo.faces {
        let Some((_, merges)) = face_hole(face, corners) else {
            continue;
        };
        let Some(&(k, w, next)) = merges.first() else {
            continue;
        };
        // The side that turns runs from corner r to corner k.
        let (r, rr) = if next {
            ((k + n - 1) % n, (k + n - 2) % n)
        } else {
            ((k + 1) % n, (k + 2) % n)
        };
        let has = |p: [f64; 3]| {
            face.polygon
                .iter()
                .any(|q| (0..3).all(|i| (p[i] - q[i]).abs() < 1e-6))
        };
        let flat = |p: [f64; 3]| Point2::new(p[0], p[1]);
        let eave = geo
            .lines
            .iter()
            .filter(|l| l.kind == LineKind::Eave && has(l.a) && has(l.b))
            .find_map(|l| {
                let (a, b) = (flat(l.a), flat(l.b));
                let x = cross_lines(corners[r], corners[k], a, b)?;
                let len = a.distance(b);
                let t = ((x.x - a.x) * (b.x - a.x) + (x.y - a.y) * (b.y - a.y)) / len;
                (-TOL..=len + TOL).contains(&t).then_some(x)
            });
        if let Some(e) = eave
            && let Some(x) = cross_lines(e, w, corners[rr], corners[r])
        {
            corners[r] = x;
        }
        corners[k] = w;
        return true;
    }
    false
}

/// DACAD's test whether hole edge C→D lies on roof line A→B: the ends
/// agree within VALUE_EPS, or the unit vectors B−A, C−A and D−C agree in
/// every coordinate within VALUE_EPS (either sign) with C and D inside the
/// line's box grown by VALUE_EPS; all in f32. The line is measured from
/// its lower end (DACAD stores it in face order; not modelled).
fn on_line(r: &RoofLine, c: [f64; 3], d: [f64; 3]) -> bool {
    const VALUE_EPS: f32 = 0.01;
    let (a, b) = if r.a[2] <= r.b[2] {
        (r.a, r.b)
    } else {
        (r.b, r.a)
    };
    let eq = |p: [f64; 3], q: [f64; 3]| {
        (0..3).all(|i| ((p[i] as f32) - (q[i] as f32)).abs() < VALUE_EPS)
    };
    if (eq(a, c) && eq(b, d)) || (eq(a, d) && eq(b, c)) {
        return true;
    }
    let unit = |p: [f64; 3], q: [f64; 3]| -> [f32; 3] {
        let v = [0, 1, 2].map(|i| (q[i] as f32) - (p[i] as f32));
        let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if len > 0.0 { v.map(|x| x / len) } else { v }
    };
    let parallel = |u: [f32; 3], v: [f32; 3]| {
        (0..3).all(|i| (u[i] - v[i]).abs() < VALUE_EPS)
            || (0..3).all(|i| (u[i] + v[i]).abs() < VALUE_EPS)
    };
    let inside_box = |p: [f64; 3]| {
        (0..3).all(|i| {
            let (lo, hi) = (a[i].min(b[i]) as f32, a[i].max(b[i]) as f32);
            let x = p[i] as f32;
            x >= lo - VALUE_EPS && x <= hi + VALUE_EPS
        })
    };
    if !inside_box(c) || !inside_box(d) {
        return false;
    }
    // [no source — extrapolation] an edge starting at A is measured from D.
    let uc = if eq(a, c) { unit(a, d) } else { unit(a, c) };
    let (ub, ud) = (unit(a, b), unit(c, d));
    parallel(ub, uc) && parallel(ud, uc) && parallel(ub, ud)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 10 × 8 m gable roof, 45°, eave 3 m, no overhang; ridge along x at y = 4.
    fn gable() -> Project {
        let e = |x: f64, y: f64, pitch: f64| {
            serde_json::json!({ "start": { "x": x, "y": y }, "pitch_deg": pitch,
                "eave_height": 3.0, "overhang": 0.0 })
        };
        let part = serde_json::json!({ "name": "A", "edges": [
            e(0.0, 0.0, 45.0), e(10.0, 0.0, 90.0), e(10.0, 8.0, 45.0), e(0.0, 8.0, 90.0)] });
        let mut p = Project::new("t");
        p.parts.push(serde_json::from_value(part).unwrap());
        p
    }

    fn square(x: f64, y: f64, w: f64, d: f64) -> Chimney {
        let c = |x, y| Point2::new(x, y);
        Chimney {
            corners: vec![c(x, y), c(x + w, y), c(x + w, y + d), c(x, y + d)],
            height: 0.0,
        }
    }

    fn total(geo: &RoofGeometry, code: &str) -> f64 {
        geo.lines
            .iter()
            .filter(|l| l.dacad_code() == code)
            .map(|l| l.length())
            .sum()
    }

    #[test]
    fn roof_opening_sides_are_roof_edges() {
        let mut p = gable();
        let before = generate_project(&p)[0]
            .faces
            .iter()
            .map(|f| f.area)
            .sum::<f64>();
        p.roof_openings.push(square(4.0, 1.0, 1.0, 2.0));
        let geo = &generate_project(&p)[0];
        let s = 2f64.sqrt();
        // Uphill side is an eave, downhill side a Pult, the sides verges.
        assert!((total(geo, "T") - 21.0).abs() < 1e-6);
        assert!((total(geo, "P") - 1.0).abs() < 1e-6);
        assert!((total(geo, "OL") - (8.0 * s + 2.0 * s)).abs() < 1e-6);
        assert!((total(geo, "OR") - (8.0 * s + 2.0 * s)).abs() < 1e-6);
        assert!(total(geo, "WV") + total(geo, "WH") + total(geo, "WS") < 1e-9);
        let after = geo.faces.iter().map(|f| f.area).sum::<f64>();
        assert!((before - after - 2.0 * s).abs() < 1e-6);
        assert_eq!(geo.openings[0].kind, OpeningKind::Hole);
    }

    /// DACAD run OP1: 12 × 8 m hip roof, 35°, eave 3 m, opening 5..6 × 1..2.
    #[test]
    fn roof_opening_matches_dacad_op1() {
        let e = |x: f64, y: f64| {
            serde_json::json!({ "start": { "x": x, "y": y }, "pitch_deg": 35.0,
                "eave_height": 3.0, "overhang": 0.0 })
        };
        let part = serde_json::json!({ "name": "A", "edges": [
            e(0.0, 0.0), e(12.0, 0.0), e(12.0, 8.0), e(0.0, 8.0)] });
        let mut p = Project::new("op1");
        p.parts.push(serde_json::from_value(part).unwrap());
        p.roof_openings.push(square(5.0, 1.0, 1.0, 1.0));
        let geo = &generate_project(&p)[0];
        let area = geo.faces.iter().map(|f| f.area).sum::<f64>();
        assert!((area - 115.974).abs() < 1e-3, "{area}");
        assert!((total(geo, "P") - 1.0).abs() < 1e-6);
        assert!((total(geo, "OL") - 1.221).abs() < 1e-3);
        assert!((total(geo, "OR") - 1.221).abs() < 1e-3);
        let west = geo.lines.iter().find(|l| l.dacad_code() == "OR").unwrap();
        assert!((west.a[0] - 5.0).abs() < 1e-9);
    }

    #[test]
    fn rotated_roof_opening_has_only_oblique_verges() {
        let mut p = gable();
        let c = Point2::new(5.0, 2.0);
        let (s, co) = (30f64.to_radians().sin(), 30f64.to_radians().cos());
        let mut corners = Vec::new();
        for (u, v) in [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)] {
            corners.push(Point2::new(c.x + u * co - v * s, c.y + u * s + v * co));
        }
        p.roof_openings.push(Chimney {
            corners,
            height: 0.0,
        });
        let geo = &generate_project(&p)[0];
        // Base gable: T 20, OL = OR = 8√2; the opening adds four OS sides.
        assert!((total(geo, "T") - 20.0).abs() < 1e-6);
        assert!(total(geo, "P") < 1e-9);
        assert!(total(geo, "OS") > 4.0);
    }
}
