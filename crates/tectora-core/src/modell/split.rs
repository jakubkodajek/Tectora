//! Splitting polygon edges where other polygons' edges cross, touch or
//! overlap them, so that shared boundaries consist of the same pieces.

use super::Tol;
use super::poly::bbox;
use super::types::{Edge, Poly, State};
use super::vec::{V3, ge, gt, lt, parallel, veq};

/// No intersection point (singular systems, a miss, or a point outside
/// one of the segments).
pub(super) struct NoPoint;

/// Distance of `p` from the line through `e`.
fn line_distance(e: &Edge, p: V3) -> f64 {
    let v = p.sub(e.p1);
    let d = e.dir().normalize(super::plane::EPS_FRAME);
    let t = v.dot(d);
    v.sub(d.scale(t)).length()
}

/// Whether `p` lies on the segment. `open` excludes points equal (by
/// [`veq`]) to the ends. The final box test is exact, without tolerance.
fn has_point(e: &Edge, p: V3, tol: f64, open: bool) -> bool {
    if veq(e.p1, p, tol) || veq(e.p2, p, tol) {
        return !open;
    }
    if tol < line_distance(e, p) {
        return false;
    }
    let (a, b) = (e.p1, e.p2);
    for (lo, hi, v) in [(a.x, b.x, p.x), (a.y, b.y, p.y), (a.z, b.z, p.z)] {
        let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
        if v < lo || v > hi {
            return false;
        }
    }
    true
}

/// The crossing of `a`'s open interior with the closed segment `b`, solved
/// in the first coordinate plane where the system is regular.
pub(super) fn crossing(a: &Edge, b: &Edge, tol: f64) -> Result<V3, NoPoint> {
    let (a0, b0, da, db) = (a.p1, b.p1, a.dir(), b.dir());
    let c = |v: V3, k: usize| [v.x, v.y, v.z][k];
    for (p, q) in [(0, 1), (0, 2), (1, 2)] {
        let Ok((t, u)) = super::plane::solve2(
            c(da, p),
            -c(db, p),
            c(b0, p) - c(a0, p),
            c(da, q),
            -c(db, q),
            c(b0, q) - c(a0, q),
        ) else {
            continue;
        };
        let pa = a0.add(da.scale(t));
        let pb = b0.add(db.scale(u));
        if veq(pa, pb, tol) {
            if has_point(a, pa, tol, true) && has_point(b, pa, tol, false) {
                return Ok(pa);
            }
            return Err(NoPoint);
        }
    }
    Err(NoPoint)
}

fn lo_hi(a: f64, b: f64) -> (f64, f64) {
    if a > b { (b, a) } else { (a, b) }
}

/// Splits `e` by `s`: the pieces of `e` (two or three) as point pairs, or
/// nothing.
fn split_edge(e: &Edge, s: &Edge, tol: f64) -> Vec<(V3, V3)> {
    for (ea, eb, sa, sb) in [
        (e.p1.x, e.p2.x, s.p1.x, s.p2.x),
        (e.p1.y, e.p2.y, s.p1.y, s.p2.y),
        (e.p1.z, e.p2.z, s.p1.z, s.p2.z),
    ] {
        let (elo, ehi) = lo_hi(ea, eb);
        let (slo, shi) = lo_hi(sa, sb);
        if lt(ehi, slo, tol) || gt(elo, shi, tol) {
            return Vec::new();
        }
    }
    let (da, db) = (e.dir(), s.dir());
    if da.is_zero(tol) || db.is_zero(tol) {
        return Vec::new();
    }
    let (a0, a1, b0, b1) = (e.p1, e.p2, s.p1, s.p2);
    let crossing_lines = parallel(da, db, tol) == 0
        || parallel(da, b0.sub(a0), tol) == 0
        || parallel(da, b1.sub(a0), tol) == 0;
    if crossing_lines {
        return match crossing(e, s, tol) {
            Ok(p) => vec![(a0, p), (p, a1)],
            Err(NoPoint) => Vec::new(),
        };
    }
    let c1 = veq(a0, b0, tol);
    let c2 = veq(a1, b1, tol);
    let c3 = veq(a0, b1, tol);
    let c4 = veq(a1, b0, tol);
    if (c1 && c2) || (c3 && c4) {
        return Vec::new();
    }
    // One shared end: split at the other end of `s` when `s` runs along
    // `e` from the shared end and is shorter.
    let shared = |v1: V3, v2: V3, p: V3| {
        if parallel(v1, v2, tol) == -1 || ge(db.length(), da.length(), tol) {
            Vec::new()
        } else {
            vec![(a0, p), (p, a1)]
        }
    };
    if c1 {
        return shared(a1.sub(a0), b1.sub(b0), b1);
    }
    if c3 {
        return shared(a1.sub(a0), b0.sub(b1), b0);
    }
    if c4 {
        return shared(a0.sub(a1), b1.sub(b0), b1);
    }
    if c2 {
        return shared(a0.sub(a1), b0.sub(b1), b0);
    }
    let h0 = has_point(e, b0, tol, true);
    if h0 && has_point(e, b1, tol, true) {
        return if b1.sub(a0).length() > b0.sub(a0).length() {
            vec![(a0, b0), (b0, b1), (b1, a1)]
        } else {
            vec![(a0, b1), (b1, b0), (b0, a1)]
        };
    }
    if h0 {
        return vec![(a0, b0), (b0, a1)];
    }
    if has_point(e, b1, tol, true) {
        return vec![(a0, b1), (b1, a1)];
    }
    Vec::new()
}

fn overlap(a: &[f64; 4], b: &[f64; 4]) -> bool {
    a[1] >= b[0] && b[1] >= a[0] && a[3] >= b[2] && b[3] >= a[2]
}

fn list(st: &State, f: usize, cut: bool) -> &Vec<Poly> {
    if cut {
        &st.faces[f].cutters
    } else {
        &st.faces[f].pieces
    }
}

fn list_mut(st: &mut State, f: usize, cut: bool) -> &mut Vec<Poly> {
    if cut {
        &mut st.faces[f].cutters
    } else {
        &mut st.faces[f].pieces
    }
}

/// Splits every edge of every face polygon (pieces, then cutters) by the
/// edges of all other face polygons.
///
/// The box a polygon's neighbours are tested against is only taken for
/// pieces: a cutter is tested with the box of its face's last piece (or,
/// with no piece, whatever box came last before).
/// Most edges one polygon may have after splitting. A real roof face has a
/// few dozen; a wing a few mm off and turned by a fraction of a degree made
/// the pieces split each other without end and asked for gigabytes, which
/// aborts the process (review MATH-004, `tests/modell_budget.rs`). DACAD's
/// behaviour on that input is not known. Past this the run stops with
/// [`State::overflow`] and the engine falls back to its own sweep.
pub const MAX_POLY_EDGES: usize = 20_000;

pub fn split_all(st: &mut State, tol: &Tol) {
    let t = tol.t01;
    let mut bb_p = [0.0; 4];
    for a in 0..st.faces.len() {
        for la in [false, true] {
            for p in 0..list(st, a, la).len() {
                if list(st, a, la)[p].is_empty() {
                    continue;
                }
                if !la {
                    bb_p = bbox(&list(st, a, la)[p]);
                }
                let mut i = 0;
                while i < list(st, a, la)[p].len() {
                    let mut e = list(st, a, la)[p].edges[i];
                    for b in 0..st.faces.len() {
                        for lb in [false, true] {
                            for q in 0..list(st, b, lb).len() {
                                if lb == la && a == b && p == q {
                                    continue;
                                }
                                let other = &list(st, b, lb)[q];
                                if other.is_empty() || !overlap(&bb_p, &bbox(other)) {
                                    continue;
                                }
                                let edges = other.edges.clone();
                                for s in &edges {
                                    let pieces = split_edge(&e, s, t);
                                    if pieces.is_empty() {
                                        continue;
                                    }
                                    let template = e;
                                    e.p1 = pieces[0].0;
                                    e.p2 = pieces[0].1;
                                    let poly = &mut list_mut(st, a, la)[p];
                                    poly.edges[i] = e;
                                    for (k, &(p1, p2)) in pieces.iter().enumerate().skip(1) {
                                        poly.edges.insert(i + k, Edge { p1, p2, ..template });
                                    }
                                    if poly.edges.len() > MAX_POLY_EDGES {
                                        st.overflow = true;
                                        return;
                                    }
                                }
                            }
                        }
                    }
                    i += 1;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge(a: [f64; 3], b: [f64; 3]) -> Edge {
        Edge::new(a.into(), b.into(), b'd')
    }

    #[test]
    fn crossing_splits_in_two() {
        let e = edge([0.0, 0.0, 0.0], [4.0, 0.0, 0.0]);
        let s = edge([1.0, -1.0, 0.0], [1.0, 1.0, 0.0]);
        let r = split_edge(&e, &s, 0.01);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].1, V3::new(1.0, 0.0, 0.0));
    }

    #[test]
    fn overlap_inside_splits_in_three() {
        let e = edge([0.0, 0.0, 0.0], [4.0, 0.0, 0.0]);
        let s = edge([3.0, 0.0, 0.0], [1.0, 0.0, 0.0]);
        let r = split_edge(&e, &s, 0.01);
        assert_eq!(r.len(), 3);
        assert_eq!(r[0].1, V3::new(1.0, 0.0, 0.0));
        assert_eq!(r[1].1, V3::new(3.0, 0.0, 0.0));
    }
}
