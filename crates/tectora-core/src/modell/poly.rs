//! Small polygon kernels: snapping, orientation, point in polygon,
//! intersection vertices, area.

use std::f64::consts::{FRAC_PI_2, PI};

use super::plane::{IDENTITY, Mat, Plane, transform};
use super::types::Poly;
use super::vec::{V3, eq, gt, le, lt, parallel, round_grid};

/// Snaps every vertex to the grid `1/f` and closes the polygon: each p1 is
/// rounded and the previous edge's p2 is set to it.
pub fn snap(p: &mut Poly, f: f64) {
    let n = p.edges.len();
    for i in 0..n {
        let q = p.edges[i].p1.round(f);
        p.edges[i].p1 = q;
        p.edges[(i + n - 1) % n].p2 = q;
    }
}

/// The xy bounding box `[xmin, xmax, ymin, ymax]` of the vertices; all
/// zero for a polygon without edges.
pub fn bbox(p: &Poly) -> [f64; 4] {
    let Some(first) = p.edges.first() else {
        return [0.0; 4];
    };
    let s = first.p1;
    let (mut x0, mut x1, mut y0, mut y1) = (s.x, s.x, s.y, s.y);
    for e in &p.edges {
        let q = e.p2;
        if q.x < x0 {
            x0 = q.x;
        } else if q.x > x1 {
            x1 = q.x;
        }
        if q.y < y0 {
            y0 = q.y;
        } else if q.y > y1 {
            y1 = q.y;
        }
    }
    [x0, x1, y0, y1]
}

/// Where the vertices of `q` lie against the plane of `p`:
/// bit 2 above, bit 1 below, bit 0 on.
pub fn sides(p: &Poly, q: &Poly, tol: f64) -> u32 {
    let mut bits = 0;
    for e in &q.edges {
        if p.plane.above(e.p1, tol) {
            bits |= 4;
        }
        if p.plane.below(e.p1, tol) {
            bits |= 2;
        }
        if p.plane.on(e.p1, tol) {
            bits |= 1;
        }
    }
    bits
}

/// Point in polygon in the frame `m`: 1 inside, 0 outside, `on_edge` on
/// an exact hit of a vertex or an edge. The vertices are the edges' p1;
/// comparisons are exact.
pub fn has_point(p: &Poly, pt: V3, eps: f64, on_edge: i32, m: &Mat) -> i32 {
    let q = transform(pt, m);
    if q.z.abs() >= eps || q.z.is_nan() {
        return 0;
    }
    let (px, py) = (q.x, q.y);
    let n = p.edges.len();
    let mut cnt = 0;
    for i in 0..n {
        let a = transform(p.edges[i].p1, m);
        let b = transform(p.edges[(i + 1) % n].p1, m);
        let (x1, y1, x2, y2) = (a.x, a.y, b.x, b.y);
        let dy = y2 - y1;
        if dy == 0.0 {
            if py == y1 && ((x1 <= px && px <= x2) || (x2 <= px && px <= x1)) {
                return on_edge;
            }
            continue;
        }
        if x1 < px && x2 < px {
            continue;
        }
        if y1 < py && y2 < py {
            continue;
        }
        if y1 > py && y2 > py {
            continue;
        }
        if (px == x1 && py == y1) || (px == x2 && py == y2) {
            return on_edge;
        }
        let xi = ((x2 - x1) * (py - y1)) / dy + x1;
        if px == xi {
            return on_edge;
        }
        if x1 >= px && x2 >= px {
            if (y1 > py && y2 < py)
                || (y1 < py && y2 > py)
                || (y1 == py && y2 > py)
                || (y2 == py && y1 > py)
            {
                cnt += 1;
            }
        } else if xi > px && (y1 > py || y2 > py) {
            cnt += 1;
        }
    }
    cnt & 1
}

/// [`has_point`] on the raw coordinates (an identity frame).
pub fn has_point_raw(p: &Poly, pt: V3, eps: f64, on_edge: i32) -> i32 {
    has_point(p, pt, eps, on_edge, &IDENTITY)
}

/// Reverses the edge order and every edge; the first vertex stays first.
pub fn reverse(p: &mut Poly) {
    p.edges.reverse();
    for e in &mut p.edges {
        std::mem::swap(&mut e.p1, &mut e.p2);
    }
}

/// Orients a flat polygon counter-clockwise (`up > 0`) or clockwise
/// (`up < 0`) in xy; `up == 0` always reverses.
pub fn orient2d(p: &mut Poly, up: f64) {
    let n = p.edges.len();
    if n == 0 {
        return;
    }
    let mut bi = 0;
    let mut best = p.edges[0].p2;
    for (i, e) in p.edges.iter().enumerate().skip(1) {
        let v = e.p2;
        if best.y >= v.y && (best.y > v.y || best.x > v.x) {
            best = v;
            bi = i;
        }
    }
    let d1 = p.edges[bi].dir().neg();
    let d2 = p.edges[(bi + 1) % n].dir();
    let cz = d2.cross(d1).z;
    if (up > 0.0 && cz > 0.0) || (up < 0.0 && cz < 0.0) {
        return;
    }
    reverse(p);
}

/// Orients a polygon in space so that its turning direction is parallel
/// (`flag` 1) to `n`.
pub fn orient3d(p: &mut Poly, flag: i32, tol: f64, n: V3) {
    if p.edges.is_empty() {
        return;
    }
    let mut bi = 0;
    let mut best = p.edges[0].p2;
    for (i, e) in p.edges.iter().enumerate().skip(1) {
        let v = e.p2;
        if !le(v.z, best.z, tol) {
            continue;
        }
        if lt(v.z, best.z, tol) {
            best = v;
            bi = i;
            continue;
        }
        if !le(v.y, best.y, tol) {
            continue;
        }
        if lt(v.y, best.y, tol) {
            best = v;
            bi = i;
            continue;
        }
        if lt(v.x, best.x, tol) {
            best = v;
            bi = i;
        }
    }
    let len = p.edges.len();
    let d1 = p.edges[bi].dir().neg();
    let d2 = p.edges[(bi + 1) % len].dir();
    let c = d2.cross(d1);
    if parallel(c, n, tol) != flag {
        reverse(p);
    }
}

/// Orients a polygon along its own plane normal.
pub fn orient_to_plane(p: &mut Poly, flag: i32, tol: f64) {
    let n = p.plane.n;
    orient3d(p, flag, tol, n);
}

/// Drops edges whose ends are exactly equal.
pub fn drop_zero(p: &mut Poly) {
    p.edges.retain(|e| !e.p1.same(e.p2));
}

/// Merges consecutive edges that run the same way (or where either is
/// shorter than `tol`) into the later one.
pub fn merge_collinear(p: &mut Poly, tol: f64) {
    let mut i: isize = 0;
    let mut j: isize = 1;
    while (i as usize) < p.edges.len() {
        let n = p.edges.len();
        if n == 1 {
            return;
        }
        let ei = p.edges[i as usize];
        let jj = j.rem_euclid(n as isize) as usize;
        if parallel(ei.dir(), p.edges[jj].dir(), tol) == 1 {
            p.edges[jj].p1 = ei.p1;
            p.edges.remove(i as usize);
            i -= 1;
            j -= 1;
        }
        i += 1;
        j += 1;
    }
}

fn split_at(p: &mut Poly, i: usize, q: V3) {
    let mut c = p.edges[i];
    c.p1 = q;
    p.edges[i].p2 = q;
    p.edges.insert(i + 1, c);
}

/// Splits the edge at the other edge's coordinate along a shared axis line
/// (at most once per pair).
#[allow(clippy::if_same_then_else)]
fn collinear(lo: f64, hi: f64, c1: f64, c2: f64, own1: f64) -> Option<f64> {
    if c1 <= lo || hi <= c1 {
        if lo < c2 && c2 < hi { Some(c2) } else { None }
    } else if hi <= c2 || c2 <= lo {
        Some(c1)
    } else if (c1 - own1).abs() < (c2 - own1).abs() {
        Some(c1)
    } else {
        Some(c2)
    }
}

fn minmax(a: f64, b: f64) -> (f64, f64) {
    if b < a { (b, a) } else { (a, b) }
}

/// Inserts the mutual intersection points of two flat polygons into both,
/// after snapping both to the grid `1/grid`.
pub fn intersect_vertices(a: &mut Poly, b: &mut Poly, grid: f64) {
    snap(a, grid);
    snap(b, grid);
    let g = 1.0 / grid;
    let r = |v: f64| round_grid(v, grid);
    let mut i = 0;
    while i < a.edges.len() {
        let mut j = 0;
        while j < b.edges.len() {
            let (e, f) = (a.edges[i], b.edges[j]);
            let (x1, x2, y1, y2) = (r(e.p1.x), r(e.p2.x), r(e.p1.y), r(e.p2.y));
            let (axmin, axmax) = minmax(x1, x2);
            let (aymin, aymax) = minmax(y1, y2);
            let (bx1, bx2, by1, by2) = (r(f.p1.x), r(f.p2.x), r(f.p1.y), r(f.p2.y));
            let (bxmin, bxmax) = minmax(bx1, bx2);
            let (bymin, bymax) = minmax(by1, by2);
            let skip = axmax < bxmin
                || aymax < bymin
                || bxmax < axmin
                || bymax < aymin
                || (x1 == bx1 && y1 == by1)
                || (x1 == bx2 && y1 == by2)
                || (x2 == bx1 && y2 == by1)
                || (x2 == bx2 && y2 == by2);
            if skip {
                j += 1;
                continue;
            }
            if x1 == x2 && bx1 == bx2 {
                if x1 == bx1 {
                    if let Some(c) = collinear(aymin, aymax, by1, by2, y1) {
                        split_at(a, i, V3::new(x1, c, 0.0));
                    }
                    if let Some(c) = collinear(bymin, bymax, y1, y2, by1) {
                        split_at(b, j, V3::new(x1, c, 0.0));
                    }
                }
            } else if y1 == y2 && by1 == by2 {
                if y1 == by1 {
                    if let Some(c) = collinear(axmin, axmax, bx1, bx2, x1) {
                        split_at(a, i, V3::new(c, y1, 0.0));
                    }
                    if let Some(c) = collinear(bxmin, bxmax, x1, x2, bx1) {
                        split_at(b, j, V3::new(c, y1, 0.0));
                    }
                }
            } else {
                let (dxa, dya, dxb, dyb) = (x2 - x1, y2 - y1, bx2 - bx1, by2 - by1);
                let (px, py) = if x1 == x2 && by1 == by2 {
                    (x1, by1)
                } else if y1 == y2 && bx1 == bx2 {
                    (bx1, y1)
                } else if x1 == x2 {
                    (x1, ((x1 - bx1) * dyb) / dxb + by1)
                } else if y1 == y2 {
                    (((y1 - by1) * dxb) / dyb + bx1, y1)
                } else if bx1 == bx2 {
                    (bx1, ((bx1 - x1) * dya) / dxa + y1)
                } else if by1 == by2 {
                    (((by1 - y1) * dxa) / dya + x1, by1)
                } else {
                    let den = dyb * dxa - dxb * dya;
                    if den == 0.0 {
                        j += 1;
                        continue;
                    }
                    let t = ((bx1 - x1) * dyb - (by1 - y1) * dxb) / den;
                    (dxa * t + x1, dya * t + y1)
                };
                let (px, py) = (r(px), r(py));
                let inside = |x1: f64, y1: f64, x2: f64, y2: f64, b: [f64; 4], o: [f64; 4]| {
                    let [xmin, xmax, ymin, ymax] = b;
                    let [oxmin, oxmax, oymin, oymax] = o;
                    !(px == x1 && py == y1)
                        && !(px == x2 && py == y2)
                        && ((xmin < px && px < xmax && ymin <= py && py <= ymax)
                            || (xmin <= px && px <= xmax && ymin < py && py < ymax))
                        && (oxmin - g <= px
                            && px <= g + oxmax
                            && oymin - g <= py
                            && py <= g + oymax)
                };
                let ab = [axmin, axmax, aymin, aymax];
                let bb = [bxmin, bxmax, bymin, bymax];
                if inside(x1, y1, x2, y2, ab, bb) {
                    split_at(a, i, V3::new(px, py, 0.0));
                }
                if inside(bx1, by1, bx2, by2, bb, ab) {
                    split_at(b, j, V3::new(px, py, 0.0));
                }
            }
            j += 1;
        }
        i += 1;
    }
}

/// The polygon's area, measured in the coordinate plane it is most
/// parallel to and scaled back with the cosine of the angle.
pub fn area(p: &Poly) -> f64 {
    let o = V3::ZERO;
    let (ex, ey, ez) = (
        V3::new(1.0, 0.0, 0.0),
        V3::new(0.0, 1.0, 0.0),
        V3::new(0.0, 0.0, 1.0),
    );
    let n_xz = Plane::new(o, ex, ez).n;
    let n_yz = Plane::new(o, ey, ez).n;
    let n_xy = Plane::new(o, ex, ey).n;
    let pl = &p.plane;
    let (mut a_xz, mut a_yz, mut a_xy) = (pl.angle_to(n_xz), pl.angle_to(n_yz), pl.angle_to(n_xy));
    let deg = 0.017453292519943295;
    if eq(a_xz, FRAC_PI_2, deg) && eq(a_yz, FRAC_PI_2, deg) && eq(a_xy, FRAC_PI_2, deg) {
        return 0.0;
    }
    for a in [&mut a_xz, &mut a_yz, &mut a_xy] {
        if gt(*a, FRAC_PI_2, deg) {
            *a = PI - *a;
        }
    }
    let (plane, c) = if !(a_xz < a_xy || a_yz < a_xy) {
        (2, a_xy.cos())
    } else if !(a_xz < a_yz || a_xy < a_yz) {
        (1, a_yz.cos())
    } else if !(a_yz < a_xz || a_xy < a_xz) {
        (0, a_xz.cos())
    } else {
        return 0.0;
    };
    let mut sum = 0.0;
    for e in &p.edges {
        let (a, b) = (e.p1, e.p2);
        let (u1, v1, u2, v2) = match plane {
            0 => (a.x, a.z, b.x, b.z),
            1 => (a.y, a.z, b.y, b.z),
            _ => (a.x, a.y, b.x, b.y),
        };
        sum += u1 * v2 - u2 * v1;
    }
    ((sum * 0.5) / c).abs()
}
