//! Reading the calculated roof at plan points: heights for placing dormers,
//! roof points to snap hand-drawn lines to (editor: `dormers.ts`,
//! `manual.ts`, `roofcalc.ts`).

use crate::geom::Point2;
use crate::model::Chimney;
use crate::roof::{LineKind, RoofGeometry};

use super::round_to;

fn inside(poly: &[[f64; 3]], p: Point2) -> bool {
    let mut c = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let ([xi, yi, _], [xj, yj, _]) = (poly[i], poly[j]);
        if (yi > p.y) != (yj > p.y) && p.x < (xj - xi) * (p.y - yi) / (yj - yi) + xi {
            c = !c;
        }
        j = i;
    }
    c
}

fn plane_z(poly: &[[f64; 3]], p: Point2) -> Option<f64> {
    let o = poly[0];
    for w in poly[1..].windows(2) {
        let (a, b) = (w[0], w[1]);
        let u = [a[0] - o[0], a[1] - o[1], a[2] - o[2]];
        let v = [b[0] - o[0], b[1] - o[1], b[2] - o[2]];
        let n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        if n[2].abs() > 1e-9 {
            return Some(o[2] - (n[0] * (p.x - o[0]) + n[1] * (p.y - o[1])) / n[2]);
        }
    }
    None
}

/// Height of the calculated roof at a plan point (the highest face over
/// it), or `None` off the roof.
pub fn roof_height_at(geos: &[RoofGeometry], p: Point2) -> Option<f64> {
    geos.iter()
        .flat_map(|g| &g.faces)
        .filter(|f| f.polygon.len() >= 3 && inside(&f.polygon, p))
        .filter_map(|f| plane_z(&f.polygon, p))
        .reduce(f64::max)
}

fn key(q: &[f64; 3]) -> [i64; 3] {
    q.map(|v| (v * 1000.0).round() as i64)
}

/// Roof points to snap to: the ends of the calculated lines and face corners.
pub fn roof_point_list(geos: &[RoofGeometry]) -> Vec<[f64; 3]> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for g in geos {
        let pts = g
            .lines
            .iter()
            .flat_map(|l| [l.a, l.b])
            .chain(g.faces.iter().flat_map(|f| f.polygon.iter().copied()));
        for q in pts {
            if seen.insert(key(&q)) {
                out.push(q);
            }
        }
    }
    out
}

/// The roof point within `tol` of `p` in plan, else `p` at height 0 (as DACAD).
pub fn snap_to_roof(geos: &[RoofGeometry], p: Point2, tol: f64) -> [f64; 3] {
    let mut best = None;
    let mut bd = tol;
    for q in roof_point_list(geos) {
        let d = (q[0] - p.x).hypot(q[1] - p.y);
        if d <= bd {
            bd = d;
            best = Some(q);
        }
    }
    best.unwrap_or([p.x, p.y, 0.0])
}

/// End points of the roof lines other than eaves (ridge, hip and valley
/// corners), highest first: the points the "point in space" helper offers.
pub fn roof_corner_points(geos: &[RoofGeometry]) -> Vec<[f64; 3]> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for l in geos.iter().flat_map(|g| &g.lines) {
        if l.kind == LineKind::Eave {
            continue;
        }
        for q in [l.a, l.b] {
            if seen.insert(key(&q)) {
                out.push(q);
            }
        }
    }
    out.sort_by(|a, b| b[2].total_cmp(&a[2]));
    out
}

/// A rectangular chimney from a corner, size and rotation in degrees.
pub fn chimney_from_rect(
    x: f64,
    y: f64,
    width: f64,
    depth: f64,
    rotation_deg: f64,
    height: f64,
) -> Chimney {
    let (sin, cos) = rotation_deg.to_radians().sin_cos();
    let p = |s: f64, t: f64| {
        Point2::new(
            round_to(x + cos * s - sin * t, 3),
            round_to(y + sin * s + cos * t, 3),
        )
    };
    Chimney {
        corners: vec![p(0.0, 0.0), p(width, 0.0), p(width, depth), p(0.0, depth)],
        height,
    }
}
