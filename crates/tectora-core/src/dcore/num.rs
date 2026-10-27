//! S0: the numeric kernel of the core.
//!
//! Arithmetic discipline (see PORTING.md): every record field is an `f32`;
//! the arithmetic between two stores runs on the x87 at 53-bit precision,
//! which is exactly Rust's `f64` arithmetic on the widened values. Write
//! each expression in the order the original evaluates it, widen operands
//! with `f64::from`, and round with `as f32` only where the original stores
//! to a float field or local. Transcendentals go through [`super::x87`].

use super::x87;

pub type V3 = [f32; 3];

pub const ZERO3: V3 = [0.0; 3];

/// The tolerance set (DACAD.INI may override; the defaults are used).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tol {
    /// POLYG_EPS: polygon closing (0.02).
    pub polyg: f32,
    /// VALUE_EPS: general value comparisons (0.01).
    pub value: f32,
    /// ANGLE_EPS: angle comparisons in radians (0.5 degrees).
    pub angle: f32,
    /// POINT_EPS: point matching while closing polygons (0.02); the closing
    /// retries change it.
    pub point: f32,
    /// INTSCT_EPS: intersection tests (0.05).
    pub intsct: f32,
}

impl Default for Tol {
    fn default() -> Self {
        Tol {
            polyg: f32::from_bits(0x3ca3_d70a),
            value: f32::from_bits(0x3c23_d70a),
            angle: f32::from_bits(0x3c0e_fa35),
            point: f32::from_bits(0x3ca3_d70a),
            intsct: f32::from_bits(0x3d4c_cccd),
        }
    }
}

/// pi/2 as the float the core stores for vertical faces.
pub const HALF_PI_F: f32 = f32::from_bits(0x3fc9_0fdb);
/// The double the core multiplies degrees with.
pub const DEG2RAD: f64 = 0.017453292519943295;
/// The double the INT writer multiplies radians with.
pub const RAD2DEG: f64 = 57.29577951308232;

#[inline]
fn d(a: f32, b: f32) -> f64 {
    (f64::from(a) - f64::from(b)).abs()
}

/// `|a-b| < t`.
#[inline]
pub fn eq(a: f32, b: f32, t: f32) -> bool {
    f64::from(t) > d(a, b)
}

/// `|a-b| >= t` (also when unordered).
#[inline]
pub fn ne(a: f32, b: f32, t: f32) -> bool {
    !eq(a, b, t)
}

// The negated comparisons keep the original's NaN behaviour (an
// unordered compare sets the flags that make the branch go the other way).

/// Clearly less: `|a-b| >= t && a < b` (unordered counts as true in both tests).
#[inline]
#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub fn lt(a: f32, b: f32, t: f32) -> bool {
    !(f64::from(t) > d(a, b)) && !(a >= b)
}

/// Clearly greater: `|a-b| >= t && a > b`.
#[inline]
#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub fn gt(a: f32, b: f32, t: f32) -> bool {
    !(f64::from(t) > d(a, b)) && a > b
}

/// Not clearly greater: false only when `|a-b| >= t && a >= b`.
#[inline]
#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub fn le(a: f32, b: f32, t: f32) -> bool {
    f64::from(t) > d(a, b) || !(a >= b)
}

/// Not clearly less: false only when `|a-b| >= t && a <= b` (or unordered).
#[inline]
pub fn ge(a: f32, b: f32, t: f32) -> bool {
    f64::from(t) > d(a, b) || a > b
}

/// `|a| < t`.
#[inline]
pub fn zero(a: f32, t: f32) -> bool {
    f64::from(t) > f64::from(a).abs()
}

/// All three coordinates `eq`.
#[inline]
pub fn peq(p: V3, q: V3, t: f32) -> bool {
    eq(p[0], q[0], t) && eq(p[1], q[1], t) && eq(p[2], q[2], t)
}

/// Any coordinate `ne`.
#[inline]
pub fn pne(p: V3, q: V3, t: f32) -> bool {
    !peq(p, q, t)
}

/// All three components `zero`.
#[inline]
pub fn vzero(v: V3, t: f32) -> bool {
    zero(v[0], t) && zero(v[1], t) && zero(v[2], t)
}

#[inline]
pub fn add(a: V3, b: V3) -> V3 {
    [
        (f64::from(a[0]) + f64::from(b[0])) as f32,
        (f64::from(a[1]) + f64::from(b[1])) as f32,
        (f64::from(a[2]) + f64::from(b[2])) as f32,
    ]
}

#[inline]
pub fn sub(a: V3, b: V3) -> V3 {
    [
        (f64::from(a[0]) - f64::from(b[0])) as f32,
        (f64::from(a[1]) - f64::from(b[1])) as f32,
        (f64::from(a[2]) - f64::from(b[2])) as f32,
    ]
}

/// `s * v` per component (the scale factor is a float argument).
#[inline]
pub fn scale(v: V3, s: f32) -> V3 {
    let s = f64::from(s);
    [
        (s * f64::from(v[0])) as f32,
        (s * f64::from(v[1])) as f32,
        (s * f64::from(v[2])) as f32,
    ]
}

/// `sqrt((x*x + y*y) + z*z)`, left unrounded on the x87 stack.
#[inline]
pub fn len(v: V3) -> f64 {
    let (x, y, z) = (f64::from(v[0]), f64::from(v[1]), f64::from(v[2]));
    ((x * x + y * y) + z * z).sqrt()
}

/// `(z*z' + y*y') + x*x'`, unrounded.
#[inline]
pub fn dot(a: V3, b: V3) -> f64 {
    (f64::from(a[2]) * f64::from(b[2]) + f64::from(a[1]) * f64::from(b[1]))
        + f64::from(a[0]) * f64::from(b[0])
}

/// The normalised vector: `L = f32(len)`, zero vector when `zero(L, value)`.
#[inline]
pub fn norm(v: V3, t: &Tol) -> V3 {
    let l = len(v) as f32;
    if zero(l, t.value) {
        return ZERO3;
    }
    let l = f64::from(l);
    [
        (f64::from(v[0]) / l) as f32,
        (f64::from(v[1]) / l) as f32,
        (f64::from(v[2]) / l) as f32,
    ]
}

/// `acos(dot / (len*len))` with the CRT acos; the 64-bit result.
pub fn angle(a: V3, b: V3) -> x87::X80 {
    let q = dot(a, b) / (len(a) * len(b));
    x87::crt_acos(q)
}

/// Parallel test of two directions: 1 same, -1 opposite, 0 neither
/// (normalised components within VALUE_EPS).
pub fn par(a: V3, b: V3, t: &Tol) -> i32 {
    let na = norm(a, t);
    let nb = norm(b, t);
    if eq(na[0], nb[0], t.value) && eq(na[1], nb[1], t.value) && eq(na[2], nb[2], t.value) {
        return 1;
    }
    if eq(-na[0], nb[0], t.value) && eq(-na[1], nb[1], t.value) && eq(-na[2], nb[2], t.value) {
        return -1;
    }
    0
}

/// The 2D turn of `r` against the line `p -> q`: 0 within VALUE_EPS, +1 when
/// the stored cross value is clearly negative, -1 otherwise.
pub fn orient(p: V3, q: V3, r: V3, t: &Tol) -> i32 {
    let f = f64::from;
    let v = ((f(q[1]) - f(p[1])) * (f(q[0]) - f(r[0])) + (f(r[1]) - f(q[1])) * (f(q[0]) - f(p[0])))
        as f32;
    if zero(v, t.value) {
        0
    } else if lt(v, 0.0, t.value) {
        1
    } else {
        -1
    }
}

/// Cramer's rule for a 3x3 system `A x = b`, `A` row major. Returns
/// `None` when the determinant is zero within VALUE_EPS; the solution is
/// `(x0, x1, x2)` as the core stores it back into `b`.
pub fn solve3(a: &[f32; 9], b: [f32; 3], t: &Tol) -> Option<[f32; 3]> {
    let a: [f64; 9] = a.map(f64::from);
    let [b0, b1, b2] = b.map(f64::from);
    let det = ((((a[0] * a[4]) * a[8] + (a[1] * a[5]) * a[6]) + (a[3] * a[7]) * a[2])
        - (a[0] * a[7]) * a[5]
        - (a[1] * a[3]) * a[8]
        - (a[4] * a[6]) * a[2]) as f32;
    if zero(det, t.value) {
        return None;
    }
    let det = f64::from(det);
    let n0 = (a[5] * b0) * a[6] + (a[3] * b2) * a[2] + (b1 * a[0]) * a[8]
        - (b2 * a[0]) * a[5]
        - (a[3] * b0) * a[8]
        - (b1 * a[6]) * a[2];
    let n1 = ((a[1] * b1) * a[6] + (b2 * a[0]) * a[4] + (a[3] * a[7]) * b0
        - (b1 * a[0]) * a[7]
        - (a[1] * a[3]) * b2
        - (a[4] * b0) * a[6]) as f32;
    let n2 = (a[1] * b2) * a[5] + (a[4] * b0) * a[8] + (b1 * a[7]) * a[2]
        - (a[7] * a[5]) * b0
        - (a[1] * b1) * a[8]
        - (b2 * a[4]) * a[2];
    Some([
        (n2 / det) as f32,
        (n0 / det) as f32,
        (f64::from(n1) / det) as f32,
    ])
}

/// Cramer's rule for a 2x2 system (`a` = [a0 a1; a2 a3]).
pub fn solve2(a: &[f32; 4], b: [f32; 2], t: &Tol) -> Option<[f32; 2]> {
    let a: [f64; 4] = a.map(f64::from);
    let [b0, b1] = b.map(f64::from);
    let det = (a[3] * a[0] - a[1] * a[2]) as f32;
    if zero(det, t.value) {
        return None;
    }
    let det = f64::from(det);
    Some([
        ((a[3] * b0 - a[1] * b1) / det) as f32,
        ((b1 * a[0] - b0 * a[2]) / det) as f32,
    ])
}

/// `F.u.z`-style slope term: the rise of a face of pitch `p` over a run of
/// `s` along its fall line: 0 for flat or vertical, else
/// `sign * sqrt((s/cos p)^2 - s^2)` (negative for pitches above 90 degrees).
pub fn rise(pitch: f32, s: f32, t: &Tol) -> f64 {
    if eq(pitch, 0.0, t.angle) || eq(pitch, HALF_PI_F, t.angle) {
        return 0.0;
    }
    let sign = if gt(pitch, HALF_PI_F, t.angle) { -1.0 } else { 1.0 };
    let q = x87::fcos(f64::from(pitch)).fdivr(f64::from(s));
    let s = f64::from(s);
    sign * (q * q - s * s).sqrt()
}

pub use crate::coresol::num::hypot;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares() {
        let t = Tol::default();
        assert!(eq(1.0, 1.005, t.value));
        assert!(!eq(1.0, 1.02, t.value));
        assert!(lt(1.0, 1.02, t.value) && !lt(1.0, 1.005, t.value));
        assert!(gt(1.02, 1.0, t.value) && !gt(1.0, 1.02, t.value));
        assert!(le(1.005, 1.0, t.value) && !le(1.02, 1.0, t.value));
        assert!(ge(1.0, 1.005, t.value) && !ge(1.0, 1.02, t.value));
        assert!(lt(f32::NAN, 1.0, t.value) && !gt(f32::NAN, 1.0, t.value));
    }

    #[test]
    fn orientation_sign() {
        let t = Tol::default();
        let p = [0.0, 0.0, 0.0];
        let q = [1.0, 0.0, 0.0];
        // Left of p -> q the stored value is positive.
        assert_eq!(orient(p, q, [0.5, 1.0, 0.0], &t), -1);
        assert_eq!(orient(p, q, [0.5, -1.0, 0.0], &t), 1);
        assert_eq!(orient(p, q, [2.0, 0.0, 0.0], &t), 0);
    }

    #[test]
    fn solver() {
        let t = Tol::default();
        let a = [2.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 4.0];
        assert_eq!(solve3(&a, [2.0, 6.0, 12.0], &t), Some([1.0, 2.0, 3.0]));
        assert_eq!(solve2(&[2.0, 0.0, 0.0, 4.0], [2.0, 8.0], &t), Some([1.0, 2.0]));
        assert_eq!(solve3(&[0.0; 9], [1.0; 3], &t), None);
    }
}
