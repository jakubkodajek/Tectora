//! Gables that stand as walls above a lower roof.
//!
//! A 90° gable at an inside corner whose eave reaches the lower roof all
//! along its line stands as a vertical wall, and the wall goes on along its
//! line through the building (DACAD sample KRUPA006, variants K0-K8). The
//! roof on the gable's side is a roof of its own, with the gable and its
//! overhang; the lower roof on the other side runs into the wall and abuts
//! it (WV, WS). With the gable eave below the lower roof, the roofs simply
//! meet (K1, K2) and nothing here applies.
//!
//! The part is cut along the gable's line into two pieces that are
//! generated as separate buildings and joined ([`crate::union::join`]).

use crate::geom::Point2;
use crate::model::{EdgeDef, RoofPart};
use crate::openings::plan;
use crate::roof::{LineKind, RoofGeometry};
use crate::union::{Plane, covers, unit};

/// A part cut into pieces, the gable's side first.
pub(crate) struct Pieces {
    pub parts: Vec<RoofPart>,
    /// For each piece, the original edge of each of its edges.
    pub edges: Vec<Vec<usize>>,
    /// Cuts as (lower piece, higher piece, start, end, gable eave).
    pub cuts: Vec<(usize, usize, Point2, Point2, f64)>,
}

/// Cuts a CCW part along the lines of its standing gables.
pub(crate) fn cut(part: &RoofPart) -> Pieces {
    let mut out = Pieces {
        parts: vec![part.clone()],
        edges: vec![(0..part.edges.len()).collect()],
        cuts: Vec::new(),
    };
    // Each cut removes a reflex corner, so this ends.
    let mut k = 0;
    while k < out.parts.len() {
        let Some((a, b, s, e, z)) = cut_once(&out.parts[k]) else {
            k += 1;
            continue;
        };
        let map = &out.edges[k];
        let (ma, mb) = (
            a.1.iter().map(|&i| map[i]).collect(),
            b.1.iter().map(|&i| map[i]).collect(),
        );
        let hi = k;
        let lo = out.parts.len();
        out.parts[k] = a.0;
        out.edges[k] = ma;
        out.parts.push(b.0);
        out.edges.push(mb);
        out.cuts.push((lo, hi, s, e, z));
    }
    out
}

type Piece = (RoofPart, Vec<usize>);

fn cut_once(part: &RoofPart) -> Option<(Piece, Piece, Point2, Point2, f64)> {
    let n = part.edges.len();
    let pts = part.outline();
    let dir = |i: usize| {
        let (a, b) = (pts[i % n], pts[(i + 1) % n]);
        let l = a.distance(b);
        (l > 1e-9).then(|| Point2::new((b.x - a.x) / l, (b.y - a.y) / l))
    };
    let reflex = |a: Point2, b: Point2| a.x * b.y - a.y * b.x < -1e-9;
    for i in 0..n {
        let g = &part.edges[i];
        let Some(d) = dir(i) else { continue };
        if !g.is_gable() {
            continue;
        }
        // The gable's start at an inside corner: the wall goes on backwards.
        let p = (i + n - 1) % n;
        if let Some(dp) = dir(p)
            && reflex(dp, d)
            && g.eave_height > part.edges[p].eave_height + 1e-6
            && let Some((h, hit)) = ray(&pts, pts[i], Point2::new(-d.x, -d.y), i, p)
        {
            // Gable side: hit -> (gable) -> ... -> edge h -> hit.
            let mut hi = vec![(
                EdgeDef {
                    start: hit,
                    ..g.clone()
                },
                i,
            )];
            let mut j = (i + 1) % n;
            loop {
                hi.push((part.edges[j].clone(), j));
                if j == h {
                    break;
                }
                j = (j + 1) % n;
            }
            // Other side: hit -> (rest of edge h) -> ... -> edge p -> wall.
            let mut lo = vec![(
                EdgeDef {
                    start: hit,
                    ..part.edges[h].clone()
                },
                h,
            )];
            let mut j = (h + 1) % n;
            while j != i {
                lo.push((part.edges[j].clone(), j));
                j = (j + 1) % n;
            }
            lo.push((wall(g, pts[i]), i));
            let lo = piece(part, lo);
            if stands(g, &lo.0, pts[i], hit) {
                return Some((piece(part, hi), lo, pts[i], hit, g.eave_height));
            }
            continue;
        }
        // The gable's end at an inside corner: the wall goes on forwards.
        let q = (i + 1) % n;
        if let Some(dq) = dir(q)
            && reflex(d, dq)
            && g.eave_height > part.edges[q].eave_height + 1e-6
            && let Some((h, hit)) = ray(&pts, pts[q], d, i, q)
        {
            // Gable side: (gable) -> hit -> (rest of edge h) -> ... -> start.
            let mut hi = vec![
                (g.clone(), i),
                (
                    EdgeDef {
                        start: hit,
                        ..part.edges[h].clone()
                    },
                    h,
                ),
            ];
            let mut j = (h + 1) % n;
            while j != i {
                hi.push((part.edges[j].clone(), j));
                j = (j + 1) % n;
            }
            // Other side: corner -> ... -> edge h -> hit -> (wall) -> corner.
            let mut lo = Vec::new();
            let mut j = q;
            loop {
                lo.push((part.edges[j].clone(), j));
                if j == h {
                    break;
                }
                j = (j + 1) % n;
            }
            lo.push((wall(g, hit), i));
            let lo = piece(part, lo);
            if stands(g, &lo.0, hit, pts[q]) {
                return Some((piece(part, hi), lo, hit, pts[q], g.eave_height));
            }
        }
    }
    None
}

/// Whether the gable's eave reaches the lower roof all along the wall,
/// which runs from `a` to `b` on the lower piece (DACAD: K0, K6 stand; K1,
/// K2 with the eave below the lower roof do not).
fn stands(g: &EdgeDef, lower: &RoofPart, a: Point2, b: Point2) -> bool {
    let roof = crate::roof::generate(lower);
    let d = unit(Point2::new(b.x - a.x, b.y - a.y));
    let inside = Point2::new(-d.y, d.x);
    let faces: Vec<(Vec<Point2>, Plane)> =
        roof.faces.iter().map(|f| (plan(f), Plane::of(f))).collect();
    let low = (0..=32)
        .filter_map(|k| {
            let t = k as f64 / 32.0;
            let m = Point2::new(
                a.x + (b.x - a.x) * t + inside.x * 1e-3,
                a.y + (b.y - a.y) * t + inside.y * 1e-3,
            );
            faces
                .iter()
                .find(|(f, _)| covers(f, m))
                .map(|(_, pl)| pl.z(m))
        })
        .fold(f64::INFINITY, f64::min);
    low.is_finite() && g.eave_height >= low - 1e-3
}

/// The standing wall as an edge of the lower piece: no roof, no overhang.
fn wall(g: &EdgeDef, start: Point2) -> EdgeDef {
    EdgeDef {
        start,
        overhang: 0.0,
        half_hip: None,
        breaks: Vec::new(),
        priority: 0,
        direction_rad: None,
        clad: false,
        ..g.clone()
    }
}

fn piece(part: &RoofPart, edges: Vec<(EdgeDef, usize)>) -> Piece {
    let (edges, map) = edges.into_iter().unzip();
    (
        RoofPart {
            name: part.name.clone(),
            edges,
        },
        map,
    )
}

/// First outline edge (not `skip_a`, `skip_b`) the ray from `o` along `u`
/// crosses, and where.
fn ray(
    pts: &[Point2],
    o: Point2,
    u: Point2,
    skip_a: usize,
    skip_b: usize,
) -> Option<(usize, Point2)> {
    let n = pts.len();
    let mut best: Option<(f64, usize, Point2)> = None;
    for k in 0..n {
        if k == skip_a || k == skip_b {
            continue;
        }
        let (a, b) = (pts[k], pts[(k + 1) % n]);
        let e = Point2::new(b.x - a.x, b.y - a.y);
        let den = u.x * e.y - u.y * e.x;
        if den.abs() < 1e-12 {
            continue;
        }
        let w = Point2::new(a.x - o.x, a.y - o.y);
        let s = (w.x * e.y - w.y * e.x) / den;
        let t = (w.x * u.y - w.y * u.x) / den;
        if s > 1e-6 && (1e-9..1.0 - 1e-9).contains(&t) && best.is_none_or(|b| s < b.0) {
            best = Some((s, k, Point2::new(o.x + u.x * s, o.y + u.y * s)));
        }
    }
    best.map(|(_, k, p)| (k, p))
}

/// Lines of the lower roof along a cut, under the higher roof, abut the
/// wall: the top of a plane rising to it (WV), a plane falling to it (WH)
/// and one sloping along it (WS). A verge of the higher roof on the wall
/// itself (no overhang) ends at the wall top, the gable's eave (DACAD K4).
pub(crate) fn abut(
    lower: &mut RoofGeometry,
    higher: &mut RoofGeometry,
    a: Point2,
    b: Point2,
    top: f64,
) {
    let d = unit(Point2::new(b.x - a.x, b.y - a.y));
    // The higher piece is on the right of the cut as the lower piece runs
    // it (the lower piece is CCW, so its inside is on the left).
    let into = Point2::new(d.y, -d.x);
    let len = a.distance(b);
    let on = |p: [f64; 3]| {
        let w = Point2::new(p[0] - a.x, p[1] - a.y);
        let t = w.x * d.x + w.y * d.y;
        (w.x * d.y - w.y * d.x).abs() < 1e-6 && (-1e-6..=len + 1e-6).contains(&t)
    };
    let above = |p: [f64; 3]| {
        let m = Point2::new(p[0] + into.x * 1e-3, p[1] + into.y * 1e-3);
        higher
            .faces
            .iter()
            .find(|f| covers(&plan(f), m))
            .is_some_and(|f| Plane::of(f).z(m) > p[2] + 1e-3)
    };
    for l in &mut lower.lines {
        if !on(l.a) || !on(l.b) {
            continue;
        }
        let m = [
            (l.a[0] + l.b[0]) / 2.0,
            (l.a[1] + l.b[1]) / 2.0,
            (l.a[2] + l.b[2]) / 2.0,
        ];
        if !above(m) {
            continue;
        }
        l.kind = match l.kind {
            LineKind::Pult => LineKind::AbutmentFront,
            LineKind::Eave => LineKind::AbutmentBack,
            LineKind::Verge => LineKind::AbutmentSide,
            k => k,
        };
        l.side = None;
    }
    let mut kept = Vec::with_capacity(higher.lines.len());
    for l in std::mem::take(&mut higher.lines) {
        if l.kind != LineKind::Verge || !on(l.a) || !on(l.b) || (l.a[2] - l.b[2]).abs() < 1e-9 {
            kept.push(l);
            continue;
        }
        let (lo, hi) = if l.a[2] < l.b[2] {
            (l.a, l.b)
        } else {
            (l.b, l.a)
        };
        if lo[2] >= top - 1e-6 {
            kept.push(l);
        } else if hi[2] > top + 1e-6 {
            let k = (top - lo[2]) / (hi[2] - lo[2]);
            let cut: [f64; 3] = std::array::from_fn(|i| lo[i] + (hi[i] - lo[i]) * k);
            let (a, b) = if l.a[2] < l.b[2] {
                (cut, l.b)
            } else {
                (l.a, cut)
            };
            kept.push(crate::roof::RoofLine { a, b, ..l });
        }
    }
    higher.lines = kept;
}
