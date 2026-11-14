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
//! inside them dropped. Where the three results differ in topology, or
//! the project has more than one building, dormers or openings, nothing is
//! done and DACAD's error stands.

use geo::{BooleanOps, MultiPolygon};

use crate::check::{RoofWarning, WarningKind, parallel_planes};
use crate::geom::Point2;
use crate::model::Project;
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
struct Pair {
    outer: usize,
    inner: usize,
    rise: f64,
}

/// The pairs of `project`'s only building whose planes count as one (one
/// per front wall, the back wall with the smallest rise).
fn pairs(project: &Project) -> Vec<Pair> {
    let Some(part) = project.parts.first() else {
        return Vec::new();
    };
    let mut out: Vec<Pair> = Vec::new();
    for p in parallel_planes(part) {
        if p.rise.abs() >= MERGE_TOL {
            continue;
        }
        match out.iter_mut().find(|q| q.outer == p.outer) {
            Some(q) if q.rise.abs() <= p.rise.abs() => {}
            Some(q) => {
                *q = Pair {
                    outer: p.outer,
                    inner: p.inner,
                    rise: p.rise,
                }
            }
            None => out.push(Pair {
                outer: p.outer,
                inner: p.inner,
                rise: p.rise,
            }),
        }
    }
    // A wall both in front of one plane and behind another would move
    // twice; leave such buildings to DACAD's error.
    if out.iter().any(|p| out.iter().any(|q| q.inner == p.outer)) {
        return Vec::new();
    }
    out
}

/// The roof of a one-building project DACAD stops on because two parallel
/// planes lie in one plane, calculated with those planes joined. `None`
/// when the project is not such a case or the calculation cannot be taken
/// back to the joined plane safely.
pub(crate) fn merged(project: &Project) -> Option<Vec<RoofGeometry>> {
    let simple = project.parts.len() == 1
        && project.dormers.is_empty()
        && project.chimneys.is_empty()
        && project.roof_openings.is_empty()
        && project.windows.is_empty()
        && !project.flat_roof;
    if !simple {
        return None;
    }
    let pairs = pairs(project);
    if pairs.is_empty() {
        return None;
    }
    let mut runs = Vec::with_capacity(OFFSETS.len());
    for &d in &OFFSETS {
        let mut p = project.clone();
        for pair in &pairs {
            p.parts[0].edges[pair.outer].eave_height += d - pair.rise;
        }
        let mut geos = crate::engine::generate(&p).ok()?;
        if geos.len() != 1 || geos[0].error.is_some() {
            return None;
        }
        runs.push(geos.remove(0));
    }
    let mut geo = extrapolate(&runs)?;
    join(project, &pairs, &mut geo)?;
    for p in &pairs {
        geo.warnings.push(RoofWarning {
            edges: vec![p.outer, p.inner],
            ..RoofWarning::new(WarningKind::MergedPlanes, p.rise.abs())
        });
    }
    Some(vec![geo])
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
    Some(out)
}

/// Plan coordinates rounded to 0.1 mm, so that the corners faces share
/// match exactly when they are joined.
fn plan_snapped(f: &RoofFace) -> Vec<Point2> {
    let r = |v: f64| (v * 1e4).round() / 1e4;
    f.polygon
        .iter()
        .map(|p| Point2::new(r(p[0]), r(p[1])))
        .collect()
}

/// Joins the faces that lie in each back wall's plane (the front walls'
/// faces and the back wall's own, and those of walls in line with it)
/// into one face and drops the line pieces inside it.
fn join(project: &Project, pairs: &[Pair], geo: &mut RoofGeometry) -> Option<()> {
    let part = &project.parts[0];
    let planes = parallel_planes(part);
    for pair in pairs {
        // Walls whose plane is the back wall's: the back wall, the front
        // wall, and walls in line with the back wall at its eave.
        let mut walls = vec![pair.inner, pair.outer];
        for p in &planes {
            for (w, other) in [(p.outer, p.inner), (p.inner, p.outer)] {
                if walls.contains(&other) && !walls.contains(&w) && p.rise.abs() < 1e-6 {
                    walls.push(w);
                }
            }
        }
        for (i, e) in part.edges.iter().enumerate() {
            if walls.contains(&i) {
                continue;
            }
            // In line with the back wall, same plane.
            let inner = &part.edges[pair.inner];
            let same = (e.pitch_deg - inner.pitch_deg).abs() < 1e-9
                && (e.eave_height - inner.eave_height).abs() < 1e-9
                && (e.overhang - inner.overhang).abs() < 1e-9
                && in_line(part, i, pair.inner);
            if same {
                walls.push(i);
            }
        }
        let members: Vec<usize> = (0..geo.faces.len())
            .filter(|&k| walls.contains(&geo.faces[k].edge))
            .collect();
        if members.len() < 2 {
            continue;
        }
        let material = geo.faces[members[0]].material;
        if members.iter().any(|&k| geo.faces[k].material != material) {
            return None;
        }
        let mut union = MultiPolygon::new(vec![]);
        for &k in &members {
            union = union.union(&MultiPolygon::new(vec![to_geo(&plan_snapped(
                &geo.faces[k],
            ))]));
        }
        if union.0.len() != 1 || !union.0[0].interiors().is_empty() {
            return None;
        }
        let outline = from_geo(&union.0[0]);
        // The joined plane: the back wall's face's plane.
        let back = members
            .iter()
            .find(|&&k| geo.faces[k].edge == pair.inner)
            .copied()
            .unwrap_or(members[0]);
        let plane = crate::union::Plane::of(&geo.faces[back]);
        let area: f64 = members.iter().map(|&k| geo.faces[k].area).sum();
        let face = RoofFace {
            edge: pair.inner,
            pitch_deg: geo.faces[back].pitch_deg,
            polygon: outline.iter().map(|&p| [p.x, p.y, plane.z(p)]).collect(),
            area,
            deducted: 0.0,
            material,
        };
        let mut faces: Vec<RoofFace> = geo
            .faces
            .iter()
            .enumerate()
            .filter(|(k, _)| !members.contains(k))
            .map(|(_, f)| f.clone())
            .collect();
        faces.insert(members[0].min(faces.len()), face);
        geo.faces = faces;
        geo.lines = geo
            .lines
            .iter()
            .flat_map(|l| outside_pieces(l, &outline))
            .filter(|l| l.length() >= MIN_LINE)
            .collect();
    }
    Some(())
}

/// Whether walls `i` and `j` of `part` lie on one line.
fn in_line(part: &crate::model::RoofPart, i: usize, j: usize) -> bool {
    let n = part.edges.len();
    let seg = |k: usize| (part.edges[k].start, part.edges[(k + 1) % n].start);
    let (a, b) = seg(j);
    let (c, d) = seg(i);
    let len = a.distance(b);
    if len < 1e-9 {
        return false;
    }
    let off = |p: Point2| ((b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)) / len;
    off(c).abs() < 1e-6 && off(d).abs() < 1e-6
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
