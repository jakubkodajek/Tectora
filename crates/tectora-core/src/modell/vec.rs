//! Vectors, scalar comparisons and grid rounding of the model intersection.
//!
//! Every operation keeps DACAD's evaluation order, so the results are the
//! same doubles bit for bit. Sums are written out with explicit brackets;
//! do not "simplify" them.

use serde::{Deserialize, Serialize};

/// A point or a direction.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(from = "[f64; 3]", into = "[f64; 3]")]
pub struct V3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl From<[f64; 3]> for V3 {
    fn from(a: [f64; 3]) -> Self {
        V3::new(a[0], a[1], a[2])
    }
}

impl From<V3> for [f64; 3] {
    fn from(v: V3) -> Self {
        [v.x, v.y, v.z]
    }
}

#[allow(clippy::should_implement_trait)]
impl V3 {
    pub const ZERO: V3 = V3::new(0.0, 0.0, 0.0);

    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        V3 { x, y, z }
    }

    pub fn add(self, b: V3) -> V3 {
        V3::new(self.x + b.x, self.y + b.y, self.z + b.z)
    }

    pub fn sub(self, b: V3) -> V3 {
        V3::new(self.x - b.x, self.y - b.y, self.z - b.z)
    }

    pub fn scale(self, s: f64) -> V3 {
        V3::new(self.x * s, self.y * s, self.z * s)
    }

    pub fn neg(self) -> V3 {
        V3::new(-self.x, -self.y, -self.z)
    }

    pub fn cross(self, b: V3) -> V3 {
        V3::new(
            self.y * b.z - self.z * b.y,
            self.z * b.x - self.x * b.z,
            self.x * b.y - self.y * b.x,
        )
    }

    pub fn dot(self, b: V3) -> f64 {
        (self.z * b.z + self.y * b.y) + self.x * b.x
    }

    pub fn length(self) -> f64 {
        ((self.z * self.z + self.y * self.y) + self.x * self.x).sqrt()
    }

    pub fn is_zero(self, tol: f64) -> bool {
        self.length() < tol
    }

    /// Unit vector; every component is 0 when the length is below `eps`.
    pub fn normalize(self, eps: f64) -> V3 {
        let l = self.length();
        if l < eps {
            return V3::ZERO;
        }
        V3::new(self.x / l, self.y / l, self.z / l)
    }

    /// Exact component equality.
    pub fn same(self, b: V3) -> bool {
        self.x == b.x && self.y == b.y && self.z == b.z
    }

    /// Rounds every component to the grid `1/f`.
    pub fn round(self, f: f64) -> V3 {
        V3::new(
            round_grid(self.x, f),
            round_grid(self.y, f),
            round_grid(self.z, f),
        )
    }

    pub fn flat(self) -> V3 {
        V3::new(self.x, self.y, 0.0)
    }
}

/// Parallel test: 1 same direction, -1 opposite, 0 neither.
///
/// A vector shorter than `tol` counts as parallel to anything. Otherwise
/// the shorter vector is scaled to the longer one's length and the tips
/// must be closer than `tol`.
pub fn parallel(a: V3, b: V3, tol: f64) -> i32 {
    if a.is_zero(tol) || b.is_zero(tol) {
        return 1;
    }
    let (la, lb) = (a.length(), b.length());
    let (d1, d2) = if lb > la {
        let k = lb / la;
        (a.scale(k).sub(b), a.neg().scale(k).sub(b))
    } else {
        let k = la / lb;
        (b.scale(k).sub(a), b.neg().scale(k).sub(a))
    };
    if d1.is_zero(tol) {
        1
    } else if d2.is_zero(tol) {
        -1
    } else {
        0
    }
}

/// Point equality as DACAD does it: both points are taken as vectors from
/// the origin and must point the same way with lengths closer than `tol`.
/// So it is not a distance test, and it gets looser far from the origin.
/// A zero tolerance compares exactly.
pub fn veq(a: V3, b: V3, tol: f64) -> bool {
    if tol == 0.0 {
        return a.same(b);
    }
    parallel(a, b, tol) == 1 && (b.length() - a.length()).abs() < tol
}

pub fn eq(a: f64, b: f64, t: f64) -> bool {
    (a - b).abs() < t
}

pub fn lt(a: f64, b: f64, t: f64) -> bool {
    !eq(a, b, t) && a < b
}

pub fn gt(a: f64, b: f64, t: f64) -> bool {
    !eq(a, b, t) && a > b
}

pub fn le(a: f64, b: f64, t: f64) -> bool {
    eq(a, b, t) || lt(a, b, t)
}

pub fn ge(a: f64, b: f64, t: f64) -> bool {
    eq(a, b, t) || gt(a, b, t)
}

/// Rounds `v` to the grid `1/f`, halves away from zero.
pub fn round_grid(v: f64, f: f64) -> f64 {
    let t = v * f;
    let r = if t > 0.0 {
        let fl = t.floor();
        if (t - fl).abs() < 0.5 { fl } else { t.ceil() }
    } else {
        let c = t.ceil();
        if (t - c).abs() < 0.5 { c } else { t.floor() }
    };
    r / f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_rounds_half_away_from_zero() {
        assert_eq!(round_grid(0.125, 100.0), 0.13);
        assert_eq!(round_grid(-0.125, 100.0), -0.13);
        assert_eq!(round_grid(6.77595, 100.0), 6.78);
    }

    #[test]
    fn point_equality_depends_on_the_origin() {
        // 0.013 apart, but at |p| ~ 9.9 the lengths differ by less than 0.01.
        let a = V3::new(6.0, 4.0, 6.776);
        let b = V3::new(6.0, 3.99, 6.7676);
        assert!(veq(a, b, 0.01));
        assert!(!veq(V3::new(0.0, 0.0, 0.0), V3::new(0.0, 0.013, 0.0), 0.01));
    }
}
