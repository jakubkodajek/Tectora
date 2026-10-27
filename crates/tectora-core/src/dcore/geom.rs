//! S0: the plane and line primitives the sweep (S2) and the closing (S3)
//! share. Each is the original routine with its exact store points; the
//! unit vectors in `tests/fixtures/dcore/geom.tsv` were produced by calling
//! the original routines in the emulator on random inputs (see PORTING.md).
//!
//! A line is `p + t d` (the `p`/`d` pair of a cut record). Planes are taken
//! from faces: `n . x = d`, frame `o`, `u`, `w`.

use super::num::{self, Tol, V3};

/// The "no point" sentinel the routines leave in their output on failure.
pub const NO_POINT: V3 = [100.0, 100.0, 100.0];

/// A line `p + t d`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Line {
    pub p: V3,
    pub d: V3,
}

/// The plane data of a face the primitives read.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plane {
    pub o: V3,
    pub u: V3,
    pub w: V3,
    pub n: V3,
    pub d: f32,
}

impl From<&super::state::Face> for Plane {
    fn from(f: &super::state::Face) -> Plane {
        Plane {
            o: f.o,
            u: f.u,
            w: f.w,
            n: f.n,
            d: f.d,
        }
    }
}

/// `a x b`, each component stored.
pub fn cross(a: V3, b: V3) -> V3 {
    let f = f64::from;
    [
        (f(b[2]) * f(a[1]) - f(a[2]) * f(b[1])) as f32,
        (f(a[2]) * f(b[0]) - f(a[0]) * f(b[2])) as f32,
        (f(a[0]) * f(b[1]) - f(b[0]) * f(a[1])) as f32,
    ]
}

/// Whether a line is horizontal: false for a zero direction, else
/// `zero(d.z / |d|)` (VALUE_EPS).
pub fn horizontal(l: &Line, t: &Tol) -> bool {
    if num::vzero(l.d, t.value) {
        return false;
    }
    num::zero((f64::from(l.d[2]) / num::len(l.d)) as f32, t.value)
}

/// Whether two planes coincide: parallel normals and equal offsets, the
/// offsets compared through the x components only (`a.d * b.n.x` against
/// `b.d * a.n.x`), so planes with `n.x == 0` count as coplanar whatever
/// their offsets.
pub fn coplanar(a: &Plane, b: &Plane, t: &Tol) -> bool {
    num::vzero(cross(a.n, b.n), t.value)
        && num::eq(
            (f64::from(b.d) * f64::from(a.n[0])) as f32,
            (f64::from(a.d) * f64::from(b.n[0])) as f32,
            t.value,
        )
}

/// The intersection line of two planes. The direction is `norm(a.n x b.n)`;
/// the point solves the two plane equations with one coordinate 0, trying
/// z = 0, then y = 0, then x = 0. `None` for parallel planes or when no
/// pair is solvable; the line then holds what the original leaves (point
/// [`NO_POINT`], direction the normalised cross product).
pub fn plane_plane(a: &Plane, b: &Plane, t: &Tol) -> (Option<Line>, Line) {
    let dir = num::norm(cross(a.n, b.n), t);
    let mut l = Line { p: NO_POINT, d: dir };
    if num::vzero(dir, t.value) {
        return (None, l);
    }
    let bb = [a.d, b.d];
    if let Some(x) = num::solve2(&[a.n[0], a.n[1], b.n[0], b.n[1]], bb, t) {
        l.p = [x[0], x[1], 0.0];
        return (Some(l), l);
    }
    if let Some(x) = num::solve2(&[a.n[0], a.n[2], b.n[0], b.n[2]], bb, t) {
        l.p = [x[0], 0.0, x[1]];
        return (Some(l), l);
    }
    if let Some(x) = num::solve2(&[a.n[1], a.n[2], b.n[1], b.n[2]], bb, t) {
        l.p = [0.0, x[0], x[1]];
        return (Some(l), l);
    }
    (None, l)
}

/// The intersection of a line with a face's plane, solved in the face's
/// frame (`o + s u + r w = p + t d`). `Err(NO_POINT)` when the system is
/// singular (the original stores the sentinel).
pub fn line_plane(f: &Plane, l: &Line, t: &Tol) -> Result<V3, V3> {
    let d = l.d;
    let a = [
        f.u[0], f.w[0], -d[0], f.u[1], f.w[1], -d[1], f.u[2], f.w[2], -d[2],
    ];
    let b = [
        (f64::from(l.p[0]) - f64::from(f.o[0])) as f32,
        (f64::from(l.p[1]) - f64::from(f.o[1])) as f32,
        (f64::from(l.p[2]) - f64::from(f.o[2])) as f32,
    ];
    let Some(r) = num::solve3(&a, b, t) else {
        return Err(NO_POINT);
    };
    let s = f64::from(r[2]);
    Ok([
        (s * f64::from(d[0]) + f64::from(l.p[0])) as f32,
        (s * f64::from(d[1]) + f64::from(l.p[1])) as f32,
        (s * f64::from(d[2]) + f64::from(l.p[2])) as f32,
    ])
}

/// The intersection point of two lines. Solves `p1 + t1 d1 = p2 + t2 d2`
/// in the xy, xz and yz components in turn; a solution counts when the
/// point on the second line equals the one on the first within VALUE_EPS.
/// With `in_slab`, a point clearly below `z_lo` or clearly above `z_hi` is
/// rejected. Returns whether a point was found and the output point
/// (the last point computed, or [`NO_POINT`]).
pub fn line_line(l1: &Line, l2: &Line, in_slab: Option<(f32, f32)>, t: &Tol) -> (bool, V3) {
    let f = f64::from;
    let diff = |k: usize| (f(l2.p[k]) - f(l1.p[k])) as f32;
    let mut out = NO_POINT;
    for (i, j) in [(0, 1), (0, 2), (1, 2)] {
        let a = [l1.d[i], -l2.d[i], l1.d[j], -l2.d[j]];
        let Some([t1, t2]) = num::solve2(&a, [diff(i), diff(j)], t) else {
            continue;
        };
        let (t1, t2) = (f(t1), f(t2));
        for (k, o) in out.iter_mut().enumerate() {
            *o = (t1 * f(l1.d[k]) + f(l1.p[k])) as f32;
        }
        let on2 = |k: usize| (t2 * f(l2.d[k]) + f(l2.p[k])) as f32;
        if !(0..3).all(|k| num::eq(out[k], on2(k), t.value)) {
            continue;
        }
        if let Some((lo, hi)) = in_slab
            && (num::lt(out[2], lo, t.value) || num::gt(out[2], hi, t.value))
        {
            return (false, out);
        }
        return (true, out);
    }
    (false, out)
}
