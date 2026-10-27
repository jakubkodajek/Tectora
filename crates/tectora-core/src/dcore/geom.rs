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
    let mut l = Line {
        p: NO_POINT,
        d: dir,
    };
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

// ---------------------------------------------------------------------------
// S2/S3: segment primitives of the sweep and the closing (each the original
// routine with its compare helpers; vectors in `geom_seg.tsv`).

/// A segment `p -> q`.
pub type Seg2 = (V3, V3);

/// Exact minimum and maximum of two floats as the x87 compare picks them
/// (`q` wins the minimum on ties, `p` the maximum).
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn min_max(p: f32, q: f32) -> (f32, f32) {
    let mn = if !(q > p) { q } else { p };
    let mx = if !(q >= p) { p } else { q };
    (mn, mx)
}

/// The intersection of two segments' lines and where it lies (seg/seg with
/// mode bits). Returns `(mask, x)`: 0 when the lines do not meet; with
/// mode 8 just 1; otherwise 0 unless `x` is inside both bounding boxes
/// within VALUE_EPS (bit 2 of the mode skips the first box, bit 4 the
/// second), then bit 1 when `x` is strictly inside the first segment in at
/// least one axis, bit 2 the same for the second (mode bit 1 also sets them
/// for any point inside the box).
pub fn seg_seg(a: Seg2, b: Seg2, mode: u32, t: &Tol) -> (u32, V3) {
    let v = t.value;
    let l1 = Line {
        p: a.0,
        d: num::sub(a.1, a.0),
    };
    let l2 = Line {
        p: b.0,
        d: num::sub(b.1, b.0),
    };
    let (found, x) = line_line(&l1, &l2, None, t);
    if !found {
        return (0, x);
    }
    if mode == 8 {
        return (1, x);
    }
    let bx = |s: Seg2| -> [(f32, f32); 3] {
        [
            min_max(s.0[0], s.1[0]),
            min_max(s.0[1], s.1[1]),
            min_max(s.0[2], s.1[2]),
        ]
    };
    let (b1, b2) = (bx(a), bx(b));
    let inside = |bb: &[(f32, f32); 3]| {
        (0..3).all(|k| !num::lt(x[k], bb[k].0, v)) && (0..3).all(|k| !num::gt(x[k], bb[k].1, v))
    };
    if !(mode & 2 != 0 || inside(&b1)) || !(mode & 4 != 0 || inside(&b2)) {
        return (0, x);
    }
    // Within [lo, hi] (le/ge) or strictly inside (lt/gt) per axis.
    let within =
        |bb: &[(f32, f32); 3], k: usize| num::le(bb[k].0, x[k], v) && num::ge(bb[k].1, x[k], v);
    let strict =
        |bb: &[(f32, f32); 3], k: usize| num::lt(bb[k].0, x[k], v) && num::gt(bb[k].1, x[k], v);
    let interior = |bb: &[(f32, f32); 3]| {
        (0..3).any(|s| (0..3).all(|k| if k == s { strict(bb, k) } else { within(bb, k) }))
    };
    let mut r = 0;
    if mode & 1 != 0 && (0..3).all(|k| within(&b1, k)) {
        r = 1;
    }
    if interior(&b1) {
        r = 1;
    }
    if mode & 1 != 0 && (0..3).all(|k| within(&b2, k)) {
        r |= 2;
    }
    if interior(&b2) {
        r |= 2;
    }
    (r, x)
}

/// Whether two segments are collinear and share an end: 1 when they lie
/// on opposite sides of the shared end (a join), -1 when they overlap from
/// it, 0 otherwise (also for equal segments' first ends).
pub fn collinear_join(a: Seg2, b: Seg2, t: &Tol) -> i32 {
    let v = t.value;
    let (p1, q1, p2, q2) = (a.0, a.1, b.0, b.1);
    if num::peq(p1, q1, v) || num::peq(p2, q2, v) {
        return 0;
    }
    let d1 = num::sub(q1, p1);
    let d2 = num::sub(q2, p2);
    let d3 = num::sub(p2, p1);
    if num::par(d1, d3, t) == 0 || num::par(d2, d3, t) == 0 || num::par(d1, d2, t) == 0 {
        return 0;
    }
    let (e1, e2) = if num::peq(p1, p2, v) {
        (num::sub(q1, p1), num::sub(q2, p2))
    } else if num::peq(p1, q2, v) {
        (num::sub(q1, p1), num::sub(p2, q2))
    } else if num::peq(q1, p2, v) {
        (num::sub(p1, q1), num::sub(q2, p2))
    } else if num::peq(q1, q2, v) {
        (num::sub(p1, q1), num::sub(p2, q2))
    } else {
        return 0;
    };
    if num::pne(num::norm(e1, t), num::norm(e2, t), v) {
        1
    } else {
        -1
    }
}

/// Whether the three difference vectors of two segments are collinear
/// (each non-zero pair parallel), the common start of the two box tests.
fn collinear3(a: Seg2, b: Seg2, t: &Tol) -> bool {
    let v = t.value;
    let d1 = num::sub(a.1, a.0);
    let d2 = num::sub(b.1, b.0);
    let d3 = num::sub(b.0, a.0);
    let bad = |x: V3, y: V3| !num::vzero(x, v) && !num::vzero(y, v) && num::par(x, y, t) == 0;
    !(bad(d1, d3) || bad(d2, d3) || bad(d1, d2))
}

/// Whether segment `b` lies on segment `a`: collinear, both ends inside
/// `a`'s box within VALUE_EPS.
pub fn seg_on_seg(a: Seg2, b: Seg2, t: &Tol) -> bool {
    let v = t.value;
    let lo = |k: usize| {
        if num::lt(a.0[k], a.1[k], v) {
            a.0[k]
        } else {
            a.1[k]
        }
    };
    let hi = |k: usize| {
        if num::gt(a.0[k], a.1[k], v) {
            a.0[k]
        } else {
            a.1[k]
        }
    };
    let (l, h) = ([lo(0), lo(1), lo(2)], [hi(0), hi(1), hi(2)]);
    if !collinear3(a, b, t) {
        return false;
    }
    (0..3).all(|k| {
        num::ge(b.0[k], l[k], v)
            && num::le(b.0[k], h[k], v)
            && num::ge(b.1[k], l[k], v)
            && num::le(b.1[k], h[k], v)
    })
}

/// Whether segment `b` overlaps segment `a`: collinear and both ends of
/// `b` inside `a`'s box, strictly inside in one axis. The original's upper
/// z bound is the y maximum (it drops the z maximum it computes).
pub fn seg_overlap(a: Seg2, b: Seg2, t: &Tol) -> bool {
    let v = t.value;
    let lo = |k: usize| {
        if num::lt(a.0[k], a.1[k], v) {
            a.0[k]
        } else {
            a.1[k]
        }
    };
    let hi = |k: usize| {
        if num::gt(a.0[k], a.1[k], v) {
            a.0[k]
        } else {
            a.1[k]
        }
    };
    let l = [lo(0), lo(1), lo(2)];
    let h = [hi(0), hi(1), hi(1)];
    if !collinear3(a, b, t) {
        return false;
    }
    let within = |x: f32, k: usize| num::ge(x, l[k], v) && num::le(x, h[k], v);
    let strict = |x: f32, k: usize| num::gt(x, l[k], v) && num::lt(x, h[k], v);
    (0..3).any(|s| {
        [b.0, b.1].iter().all(|p| {
            (0..3).all(|k| {
                if k == s {
                    strict(p[k], k)
                } else {
                    within(p[k], k)
                }
            })
        })
    })
}

/// Whether `b` points the same way as `a` (normalised within VALUE_EPS)
/// and is not clearly longer.
pub fn same_dir_within(a: V3, b: V3, t: &Tol) -> bool {
    let (na, nb) = (num::norm(a, t), num::norm(b, t));
    let (la, lb) = (num::len(a) as f32, num::len(b) as f32);
    if num::pne(na, nb, t.value) {
        return false;
    }
    num::le(lb, la, t.value)
}
