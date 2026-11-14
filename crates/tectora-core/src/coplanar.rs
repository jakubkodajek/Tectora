//! Parallel roof planes that lie in one plane.
//!
//! Two walls facing the same way at the same pitch, one in front of the
//! other, have parallel roof planes. When the front wall's eave is set so
//! that its plane is the back wall's plane (a wing whose sides are gables
//! and whose front eave is lowered until it continues the house's front
//! plane), DACAD's core stops with an E00002 error ("Einzelne Fläche
//! vorhanden", "Nicht geschlossene Fläche(n)" or "Kein Durchstoßpunkt"; a
//! T-shaped house measured in DACAD's core in the emulator, 2026-11-14).
//! Within a few centimetres above it DACAD calculates a roof with steps of
//! those few centimetres.
//!
//! Jakub decided (2026-11-14) that Tectora calculates such a roof as one
//! plane instead, in the error band and in the few centimetres above it
//! alike: a roof step of 3 cm is nobody's intention. This module does it without a geometry of its own: the
//! front walls' eaves are raised so that their planes lie a little above
//! the back plane, where the topology no longer changes, the core
//! calculates the roof at three such offsets, and every point, which moves
//! linearly with the offset while the topology stays, is taken back to
//! offset zero (areas, quadratic in the offset, from all three). The faces
//! that then lie in the back plane are joined into one and the lines
//! in that plane between them dropped. Only a front wall between two
//! gables is joined (beside an eave the standard result is sound), and the
//! result must pass `check::sound`; where the three results differ in
//! topology or the result is not sound, nothing is done and DACAD's result
//! stands. Dormers, chimneys, openings and windows are applied to the
//! joined roof by the caller (`openings::generate_valid`).

use geo::{BooleanOps, MultiPolygon};

use crate::check::{RoofWarning, WarningKind, wing_planes};
use crate::geom::Point2;
use crate::model::{Project, RoofPart};
use crate::roof::{RoofFace, RoofGeometry, RoofLine};
use crate::union::{from_geo, near_boundary, to_geo};

/// Rise of a front plane over (or under) a parallel back plane up to which
/// the two count as one plane, m. DACAD stops on these within -4 to +2 cm
/// (the measured T) and keeps steps of a few centimetres above; under the
/// back plane the house's overhang would lie a few centimetres over the
/// wing's roof. Neither is built; 10 cm is a roofer's "the same plane".
pub const MERGE_TOL: f64 = 0.10;
/// The offsets the core calculates the front planes at, m above the back
/// plane: above DACAD's error band and its 2 cm point merge.
const OFFSETS: [f64; 3] = [0.10, 0.15, 0.20];
/// How far a point may stray from the straight line through its three
/// positions before the topology counts as changed, m.
const LINEAR_TOL: f64 = 2e-3;
/// Lines shorter than this at offset zero are gone, m.
const MIN_LINE: f64 = 1e-3;
/// A line piece this far inside a joined face lies inside it, m.
const INSIDE_TOL: f64 = 2e-3;

/// One front wall and the back wall whose plane it continues.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Pair {
    pub(crate) outer: usize,
    pub(crate) inner: usize,
    pub(crate) rise: f64,
}

/// The pairs of `part` whose planes count as one (one per front wall,
/// the back wall with the smallest rise). Only a front wall between two
/// gables, in front of a longer house wall ([`wing_planes`]): beside an
/// eave a hip or a valley ends the front plane, and the standard result
/// there is sound (a hipped house with a jog, a hipped wing).
pub(crate) fn pairs(part: &RoofPart) -> Vec<Pair> {
    let mut out: Vec<Pair> = Vec::new();
    for p in wing_planes(part) {
        if p.rise.abs() >= MERGE_TOL {
            continue;
        }
        let pair = Pair {
            outer: p.outer,
            inner: p.inner,
            rise: p.rise,
        };
        match out.iter_mut().find(|q| q.outer == p.outer) {
            Some(q) if q.rise.abs() <= p.rise.abs() => {}
            Some(q) => *q = pair,
            None => out.push(pair),
        }
    }
    // A wall both in front of one plane and behind another would move
    // twice; leave such buildings to DACAD's error.
    if out.iter().any(|p| out.iter().any(|q| q.inner == p.outer)) {
        return Vec::new();
    }
    out
}

/// The roof of the one building of `bare` (no dormers, chimneys, openings
/// or windows; the caller applies those to the result) with its parallel
/// planes within [`MERGE_TOL`] of one plane joined into one face. `Err`
/// (with the reason, for logs and tests) when the building is not such a case, when the calculation cannot be
/// taken back to the joined plane safely, or when the result does not pass
/// [`crate::check::sound`].
pub(crate) fn merged(bare: &Project) -> Result<RoofGeometry, String> {
    let [part] = &bare.parts[..] else {
        return Err("not one building".into());
    };
    let plain = bare.dormers.is_empty()
        && bare.chimneys.is_empty()
        && bare.roof_openings.is_empty()
        && bare.windows.is_empty()
        && !bare.flat_roof;
    if !plain {
        return Err("not a bare building".into());
    }
    let pairs = pairs(part);
    if pairs.is_empty() {
        return Err("no planes to join".into());
    }
    let mut runs = Vec::with_capacity(OFFSETS.len());
    for &d in &OFFSETS {
        let mut p = bare.clone();
        for pair in &pairs {
            if let Some(e) = p
                .parts
                .first_mut()
                .and_then(|q| q.edges.get_mut(pair.outer))
            {
                e.eave_height += d - pair.rise;
            }
        }
        let mut geos = crate::engine::generate(&p).map_err(|e| format!("offset {d}: {e}"))?;
        if geos.len() != 1 || geos[0].error.is_some() {
            return Err(format!("offset {d}: no roof"));
        }
        runs.push(geos.remove(0));
    }
    let mut geo = extrapolate(&runs).ok_or("the offsets differ in topology")?;
    join(&pairs, &mut geo)?;
    // The building the result is the roof of: the front walls' eaves in
    // the back planes.
    let mut joined = part.clone();
    for pair in &pairs {
        if let Some(e) = joined.edges.get_mut(pair.outer) {
            e.eave_height -= pair.rise;
        }
    }
    crate::check::sound(&joined, &geo, crate::check::Fit::STANDARD)?;
    for p in &pairs {
        geo.warnings.push(RoofWarning {
            edges: vec![p.outer, p.inner],
            ..RoofWarning::new(WarningKind::MergedPlanes, p.rise.abs())
        });
    }
    Ok(geo)
}

/// Value at offset 0 of something linear in the offset, `None` when the
/// three samples are not on a line.
fn line_at_zero(v: [f64; 3]) -> Option<f64> {
    let [d1, d2, d3] = OFFSETS;
    let slope = (v[1] - v[0]) / (d2 - d1);
    let at3 = v[0] + slope * (d3 - d1);
    ((at3 - v[2]).abs() <= LINEAR_TOL).then_some(v[0] - slope * d1)
}

/// Value at offset 0 of the parabola through the three samples.
fn parabola_at_zero(v: [f64; 3]) -> f64 {
    let [d1, d2, d3] = OFFSETS;
    v[0] * (d2 * d3) / ((d1 - d2) * (d1 - d3))
        + v[1] * (d1 * d3) / ((d2 - d1) * (d2 - d3))
        + v[2] * (d1 * d2) / ((d3 - d1) * (d3 - d2))
}

fn point_at_zero(p: [[f64; 3]; 3]) -> Option<[f64; 3]> {
    let mut out = [0.0; 3];
    for (k, o) in out.iter_mut().enumerate() {
        *o = line_at_zero([p[0][k], p[1][k], p[2][k]])?;
    }
    Some(out)
}

fn poly_at_zero(polys: [&[[f64; 3]]; 3]) -> Option<Vec<[f64; 3]>> {
    if polys[1].len() != polys[0].len() || polys[2].len() != polys[0].len() {
        return None;
    }
    (0..polys[0].len())
        .map(|i| point_at_zero([polys[0][i], polys[1][i], polys[2][i]]))
        .collect()
}

/// The roof at offset zero from the roofs at the three offsets, if they
/// share their topology.
fn extrapolate(r: &[RoofGeometry]) -> Option<RoofGeometry> {
    let [a, b, c] = [&r[0], &r[1], &r[2]];
    let same = a.faces.len() == b.faces.len()
        && a.faces.len() == c.faces.len()
        && a.lines.len() == b.lines.len()
        && a.lines.len() == c.lines.len()
        && a.gables.len() == b.gables.len()
        && a.gables.len() == c.gables.len();
    // Only what this module takes back to offset zero may be there.
    let plain = [a, b, c].iter().all(|g| {
        g.clad_faces.is_empty()
            && g.clad_area == 0.0
            && g.openings.is_empty()
            && g.cut_lines.is_empty()
            && g.minus_lines.is_empty()
            && g.window_cuts.is_empty()
    });
    if !same || !plain {
        return None;
    }
    let mut out = a.clone();
    for (i, f) in out.faces.iter_mut().enumerate() {
        let (fb, fc) = (&b.faces[i], &c.faces[i]);
        if fb.edge != f.edge || fc.edge != f.edge {
            return None;
        }
        f.polygon = poly_at_zero([&a.faces[i].polygon, &fb.polygon, &fc.polygon])?;
        f.area = parabola_at_zero([a.faces[i].area, fb.area, fc.area]);
    }
    for (i, l) in out.lines.iter_mut().enumerate() {
        let (lb, lc) = (&b.lines[i], &c.lines[i]);
        if lb.kind != l.kind || lc.kind != l.kind {
            return None;
        }
        l.a = point_at_zero([a.lines[i].a, lb.a, lc.a])?;
        l.b = point_at_zero([a.lines[i].b, lb.b, lc.b])?;
    }
    for (i, g) in out.gables.iter_mut().enumerate() {
        *g = poly_at_zero([&a.gables[i], &b.gables[i], &c.gables[i]])?;
    }
    out.lines.retain(|l| l.length() >= MIN_LINE);
    out.gables.retain(|g| crate::check::area3(g) >= 1e-4);
    out.faces
        .retain(|f| f.polygon.len() >= 3 && f.area.abs() >= 1e-6);
    // A face that turned over on the way to offset zero: the topology
    // changes below the lowest offset, which three samples cannot see.
    if out.faces.iter().any(|f| f.area.is_nan() || f.area <= 0.0) {
        return None;
    }
    Some(out)
}

fn plan(f: &RoofFace) -> Vec<Point2> {
    f.polygon.iter().map(|p| Point2::new(p[0], p[1])).collect()
}

/// Corners closer than this are one corner, and a corner this close to
/// another face's side lies on it, m.
const STITCH: f64 = 1.5e-3;

/// The plan outlines with the corners they share made identical and
/// every corner that lies on another outline's side put into that side,
/// so that outlines that meet along a line share it exactly (the core's
/// corners are float32, and on a building turned out of the axes a corner
/// on another face's side misses it by micrometres).
fn stitched(mut polys: Vec<Vec<Point2>>) -> Vec<Vec<Point2>> {
    let mut reps: Vec<Point2> = Vec::new();
    for p in polys.iter_mut().flatten() {
        match reps.iter().find(|r| r.distance(*p) < STITCH) {
            Some(r) => *p = *r,
            None => reps.push(*p),
        }
    }
    for poly in polys.iter_mut() {
        poly.dedup();
        if poly.len() > 1 && poly.first() == poly.last() {
            poly.pop();
        }
        let n = poly.len();
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            out.push(a);
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let len2 = dx * dx + dy * dy;
            if len2 < STITCH * STITCH {
                continue;
            }
            let mut on: Vec<(f64, Point2)> = reps
                .iter()
                .filter(|r| **r != a && **r != b)
                .filter_map(|r| {
                    let t = ((r.x - a.x) * dx + (r.y - a.y) * dy) / len2;
                    let foot = Point2::new(a.x + dx * t, a.y + dy * t);
                    (t > 0.0 && t < 1.0 && foot.distance(*r) < STITCH).then_some((t, *r))
                })
                .collect();
            on.sort_by(|x, y| x.0.total_cmp(&y.0));
            out.extend(on.into_iter().map(|(_, r)| r));
        }
        *poly = out;
    }
    polys
}

/// A corner this far from the back plane lies in it, m.
const IN_PLANE: f64 = 5e-3;

/// Joins the faces that lie in each back wall's plane (the front wall's
/// face, the back wall's own, and those of walls in line with it) into one
/// face and drops the line pieces between them. The faces are found by
/// their plane, not by their walls, so a building turned out of the axes
/// with corners rounded to millimetres joins like the same building
/// axis-parallel.
fn join(pairs: &[Pair], geo: &mut RoofGeometry) -> Result<(), String> {
    for pair in pairs {
        let back = geo
            .faces
            .iter()
            .position(|f| f.edge == pair.inner)
            .ok_or("the back wall has no face")?;
        if geo.faces[back].polygon.len() < 3 {
            return Err("the back wall's face is empty".into());
        }
        let plane = crate::union::Plane::of(&geo.faces[back]);
        let in_plane = |f: &RoofFace| {
            f.polygon.len() >= 3
                && f.edge != usize::MAX
                && (f.pitch_deg - geo.faces[back].pitch_deg).abs() < 0.05
                && f.polygon
                    .iter()
                    .all(|p| (plane.z(Point2::new(p[0], p[1])) - p[2]).abs() < IN_PLANE)
        };
        let members: Vec<usize> = (0..geo.faces.len())
            .filter(|&k| in_plane(&geo.faces[k]))
            .collect();
        // The front wall's face must be one of them.
        if !members.iter().any(|&k| geo.faces[k].edge == pair.outer) {
            return Err("the front wall's face is not in the back plane".into());
        }
        let material = geo.faces[back].material;
        if members.iter().any(|&k| geo.faces[k].material != material) {
            return Err("the planes have different materials".into());
        }
        let mut union = MultiPolygon::new(vec![]);
        for poly in stitched(members.iter().map(|&k| plan(&geo.faces[k])).collect()) {
            union = union.union(&MultiPolygon::new(vec![to_geo(&poly)]));
        }
        if union.0.len() != 1 || !union.0[0].interiors().is_empty() {
            return Err(format!(
                "the faces in the back plane are {} pieces",
                union.0.len()
            ));
        }
        let outline = from_geo(&union.0[0]);
        // Faces that touch in one point only are not one face.
        let pinched =
            (0..outline.len()).any(|i| (0..i).any(|j| outline[i].distance(outline[j]) < 1e-3));
        if outline.len() < 3 || pinched {
            return Err("the faces in the back plane touch in a point".into());
        }
        let area: f64 = members.iter().map(|&k| geo.faces[k].area).sum();
        let face = RoofFace {
            edge: pair.inner,
            pitch_deg: geo.faces[back].pitch_deg,
            polygon: outline.iter().map(|&p| [p.x, p.y, plane.z(p)]).collect(),
            area,
            deducted: 0.0,
            material,
        };
        let at = members.first().copied().unwrap_or(0);
        let mut faces: Vec<RoofFace> = geo
            .faces
            .iter()
            .enumerate()
            .filter(|(k, _)| !members.contains(k))
            .map(|(_, f)| f.clone())
            .collect();
        faces.insert(at.min(faces.len()), face);
        geo.faces = faces;
        // Only lines in the joined plane can lie between its faces: a line
        // of a roof under it (a wall abutment under the overhang) stays.
        let lies_in = |l: &RoofLine| {
            [l.a, l.b]
                .iter()
                .all(|p| (plane.z(Point2::new(p[0], p[1])) - p[2]).abs() < IN_PLANE)
        };
        geo.lines = geo
            .lines
            .iter()
            .flat_map(|l| {
                if lies_in(l) {
                    outside_pieces(l, &outline)
                } else {
                    vec![l.clone()]
                }
            })
            .filter(|l| l.length() >= MIN_LINE)
            .collect();
    }
    Ok(())
}

/// How far apart, m (and how far from parallel), two face sides of the
/// sweep may lie and still be one side: it leaves corners of one plane
/// a few millimetres apart where events meet.
const SIDE_TOL: f64 = 0.01;

/// Faces that lie in one plane side by side (a side of one along a side
/// of the other) joined into one, for a roof the sweep calculated:
/// it leaves a wing's face beside the house face whose plane it continues
/// as two faces with no line between them. The larger face keeps its
/// wall. `Err` when two such faces cannot be joined into one simple face.
pub(crate) fn join_neighbours_in_one_plane(geo: &mut RoofGeometry) -> Result<(), String> {
    // A side of one runs along a side of the other.
    let along = |a: [f64; 3], b: [f64; 3], c: [f64; 3], d: [f64; 3]| {
        let m = [
            (c[0] + d[0]) / 2.0,
            (c[1] + d[1]) / 2.0,
            (c[2] + d[2]) / 2.0,
        ];
        let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let l2 = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];
        let cd = [d[0] - c[0], d[1] - c[1], d[2] - c[2]];
        let lcd = (cd[0] * cd[0] + cd[1] * cd[1] + cd[2] * cd[2]).sqrt();
        if l2 < 1e-6 || lcd < 2e-3 {
            return false;
        }
        let t = (((m[0] - a[0]) * ab[0] + (m[1] - a[1]) * ab[1] + (m[2] - a[2]) * ab[2]) / l2)
            .clamp(0.0, 1.0);
        let q = [a[0] + ab[0] * t, a[1] + ab[1] * t, a[2] + ab[2] * t];
        let off = ((m[0] - q[0]).powi(2) + (m[1] - q[1]).powi(2) + (m[2] - q[2]).powi(2)).sqrt();
        let cross = [
            ab[1] * cd[2] - ab[2] * cd[1],
            ab[2] * cd[0] - ab[0] * cd[2],
            ab[0] * cd[1] - ab[1] * cd[0],
        ];
        let sin =
            (cross[0].powi(2) + cross[1].powi(2) + cross[2].powi(2)).sqrt() / (l2.sqrt() * lcd);
        off < SIDE_TOL && sin < SIDE_TOL
    };
    let sides = |f: &RoofFace| {
        let m = f.polygon.len();
        (0..m)
            .map(|i| (f.polygon[i], f.polygon[(i + 1) % m]))
            .collect::<Vec<_>>()
    };
    let shared = |a: &RoofFace, b: &RoofFace| {
        let (sa, sb) = (sides(a), sides(b));
        sa.iter().any(|&(p, q)| {
            sb.iter()
                .any(|&(c, d)| along(p, q, c, d) || along(c, d, p, q))
        })
    };
    let one_plane = |a: &RoofFace, b: &RoofFace| {
        let plane = crate::union::Plane::of(a);
        (a.pitch_deg - b.pitch_deg).abs() < 0.05
            && b.polygon
                .iter()
                .all(|p| (plane.z(Point2::new(p[0], p[1])) - p[2]).abs() < IN_PLANE)
    };
    // Each join removes a face: at most as many rounds as faces.
    for _ in 0..geo.faces.len() {
        let n = geo.faces.len();
        let found = (0..n).find_map(|i| {
            (i + 1..n).find_map(|j| {
                let (a, b) = (&geo.faces[i], &geo.faces[j]);
                let real = a.edge != usize::MAX
                    && b.edge != usize::MAX
                    && a.polygon.len() >= 3
                    && b.polygon.len() >= 3;
                (real && shared(a, b) && one_plane(a, b)).then_some((i, j))
            })
        });
        let Some((i, j)) = found else {
            return Ok(());
        };
        let (big, small) = if geo.faces[i].area >= geo.faces[j].area {
            (i, j)
        } else {
            (j, i)
        };
        let pair = Pair {
            outer: geo.faces[small].edge,
            inner: geo.faces[big].edge,
            rise: 0.0,
        };
        join(&[pair], geo)?;
    }
    Ok(())
}

/// The pieces of `l` that do not lie inside `poly` (on its boundary or
/// outside it).
fn outside_pieces(l: &RoofLine, poly: &[Point2]) -> Vec<RoofLine> {
    let (pa, pb) = (Point2::new(l.a[0], l.a[1]), Point2::new(l.b[0], l.b[1]));
    let mut ts = vec![0.0, 1.0];
    let n = poly.len();
    for i in 0..n {
        let (c, d) = (poly[i], poly[(i + 1) % n]);
        if let Some(t) = crossing(pa, pb, c, d) {
            ts.push(t);
        }
    }
    // A line through a corner of the outline (the corner where a wing's
    // side meets the house's eave) is split there too: on a building
    // turned out of the axes it misses the two sides by micrometres.
    let (dx, dy) = (pb.x - pa.x, pb.y - pa.y);
    let len2 = dx * dx + dy * dy;
    if len2 > 1e-12 {
        for c in poly {
            let t = ((c.x - pa.x) * dx + (c.y - pa.y) * dy) / len2;
            let foot = Point2::new(pa.x + dx * t, pa.y + dy * t);
            if t > 0.0 && t < 1.0 && foot.distance(*c) < INSIDE_TOL {
                ts.push(t);
            }
        }
    }
    ts.sort_by(f64::total_cmp);
    ts.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
    let at = |t: f64| {
        [
            l.a[0] + (l.b[0] - l.a[0]) * t,
            l.a[1] + (l.b[1] - l.a[1]) * t,
            l.a[2] + (l.b[2] - l.a[2]) * t,
        ]
    };
    let mut out: Vec<RoofLine> = Vec::new();
    for w in ts.windows(2) {
        let m = at((w[0] + w[1]) / 2.0);
        let mid = Point2::new(m[0], m[1]);
        let inside = crate::openings::inside(poly, mid) && !near_boundary(poly, mid, INSIDE_TOL);
        if inside {
            continue;
        }
        match out.last_mut() {
            // Continue the previous piece.
            Some(prev) if prev.b == at(w[0]) => prev.b = at(w[1]),
            _ => {
                let mut piece = l.clone();
                piece.a = at(w[0]);
                piece.b = at(w[1]);
                out.push(piece);
            }
        }
    }
    out
}

/// Where segment `ab` crosses segment `cd`, as the position along `ab`.
fn crossing(a: Point2, b: Point2, c: Point2, d: Point2) -> Option<f64> {
    let r = (b.x - a.x, b.y - a.y);
    let s = (d.x - c.x, d.y - c.y);
    let den = r.0 * s.1 - r.1 * s.0;
    if den.abs() < 1e-12 {
        return None;
    }
    let q = (c.x - a.x, c.y - a.y);
    let t = (q.0 * s.1 - q.1 * s.0) / den;
    let u = (q.0 * r.1 - q.1 * r.0) / den;
    // The core's corners are float32: an outline corner and a line on it
    // can miss each other by a few micrometres.
    let slack = 1e-5 / (s.0 * s.0 + s.1 * s.1).sqrt().max(1e-9);
    ((0.0..=1.0).contains(&t) && (-slack..=1.0 + slack).contains(&u)).then_some(t)
}
