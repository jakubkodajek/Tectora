//! Planes, 4x4 frames and the plane-plane intersection.

use serde::{Deserialize, Serialize};

use super::vec::{V3, eq, parallel};

/// The tolerances that are not scaled by the unit: DACAD's fixed epsilons.
pub const EPS_TINY: f64 = 1e-10;
pub const EPS_FRAME: f64 = 0.05;

/// A plane given by an origin and two spanning vectors.
///
/// `n = w x u` is not normalised and `d = n . o`.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Plane {
    pub o: V3,
    pub u: V3,
    pub w: V3,
    pub n: V3,
    pub d: f64,
}

/// No intersection line (parallel planes or a singular system).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoLine;

impl Plane {
    pub fn new(o: V3, u: V3, w: V3) -> Plane {
        let n = w.cross(u);
        Plane {
            o,
            u,
            w,
            n,
            d: o.dot(n),
        }
    }

    /// The signed offset of `p` from the plane, as a vector along the normal.
    pub fn offset(&self, p: V3, eps: f64) -> V3 {
        let v = p.sub(self.o);
        let n = self.n.normalize(eps);
        let s = v.dot(n);
        n.scale(s)
    }

    pub fn on(&self, p: V3, eps: f64) -> bool {
        self.offset(p, eps).is_zero(eps)
    }

    /// Strictly on the side the normal points to.
    pub fn above(&self, p: V3, eps: f64) -> bool {
        let off = self.offset(p, eps);
        !off.is_zero(eps) && parallel(off, self.n, eps) == 1
    }

    pub fn below(&self, p: V3, eps: f64) -> bool {
        let off = self.offset(p, eps);
        !off.is_zero(eps) && parallel(off, self.n, eps) == -1
    }

    /// Same plane, whatever the scale or orientation of the normals.
    pub fn coplanar(&self, b: &Plane, eps: f64) -> bool {
        let (na, nb) = (self.n, b.n);
        if !na.cross(nb).is_zero(eps) {
            return false;
        }
        eq(nb.x * self.d, na.x * b.d, eps)
            && eq(nb.y * self.d, na.y * b.d, eps)
            && eq(nb.z * self.d, na.z * b.d, eps)
    }

    /// The angle between the normals, with only this normal's length divided out.
    pub fn angle_to(&self, unit_n: V3) -> f64 {
        (self.n.dot(unit_n) / self.n.length()).acos()
    }

    /// The intersection line as (point, point + direction).
    pub fn intersect(&self, b: &Plane, eps: f64) -> Result<(V3, V3), NoLine> {
        let (na, nb) = (self.n, b.n);
        let dir = na.cross(nb);
        if dir.is_zero(eps) {
            return Err(NoLine);
        }
        let p = if let Ok((x, y)) = solve2(na.x, na.y, self.d, nb.x, nb.y, b.d) {
            V3::new(x, y, 0.0)
        } else if let Ok((x, z)) = solve2(na.x, na.z, self.d, nb.x, nb.z, b.d) {
            V3::new(x, 0.0, z)
        } else if let Ok((y, z)) = solve2(na.y, na.z, self.d, nb.y, nb.z, b.d) {
            V3::new(0.0, y, z)
        } else {
            return Err(NoLine);
        };
        Ok((p, p.add(dir)))
    }
}

/// Solves `m11 x + m12 y = b1, m21 x + m22 y = b2` (Cramer's rule).
pub fn solve2(
    m11: f64,
    m12: f64,
    b1: f64,
    m21: f64,
    m22: f64,
    b2: f64,
) -> Result<(f64, f64), NoLine> {
    let det = m22 * m11 - m12 * m21;
    // Written this way round so that a NaN determinant is singular too.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    if !(EPS_TINY < det.abs()) {
        return Err(NoLine);
    }
    Ok(((m22 * b1 - b2 * m12) / det, (b2 * m11 - b1 * m21) / det))
}

/// A 4x4 matrix, row-major; points are column vectors.
pub type Mat = [[f64; 4]; 4];

pub const IDENTITY: Mat = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// The projection "drop z": DACAD's XY projection matrix.
pub const XY: Mat = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 0.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];
pub const XZ: Mat = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 0.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];
pub const YZ: Mat = [
    [0.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// `M p` with DACAD's summation order.
pub fn apply(m: &Mat, p: V3) -> V3 {
    let row = |r: usize| ((p.z * m[r][2] + p.y * m[r][1]) + p.x * m[r][0]) + m[r][3];
    V3::new(row(0), row(1), row(2))
}

/// Transforms a point, taking the shortcuts for the three projections.
/// The shortcuts are taken on content equality, like DACAD's.
pub fn transform(p: V3, m: &Mat) -> V3 {
    if *m == XY {
        V3::new(p.x, p.y, 0.0)
    } else if *m == XZ {
        V3::new(p.x, 0.0, p.z)
    } else if *m == YZ {
        V3::new(0.0, p.y, p.z)
    } else {
        apply(m, p)
    }
}

/// The local frame of a plane for the XY projection: rows e1, e2, e3 and
/// the translation that moves the plane origin to 0.
pub fn frame(pl: &Plane) -> Mat {
    let n = pl.w.cross(pl.u);
    let e1 = pl.u.normalize(EPS_FRAME);
    let e3 = n.normalize(EPS_FRAME);
    let e2 = e1.cross(e3);
    let o = pl.o.neg();
    [
        [e1.x, e1.y, e1.z, (e1.z * o.z + e1.x * o.x) + e1.y * o.y],
        [e2.x, e2.y, e2.z, (e2.z * o.z + e2.y * o.y) + e2.x * o.x],
        [e3.x, e3.y, e3.z, (e3.z * o.z + e3.y * o.y) + e3.x * o.x],
        [0.0, 0.0, 0.0, 1.0],
    ]
}
