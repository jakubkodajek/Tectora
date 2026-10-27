//! Giving every line its code from the faces on its two sides.

use super::Tol;
use super::tables::{ANY, T1, T2, T3};
use super::types::{Line, State};
use super::vec::{V3, eq, gt, lt, veq};

/// Fixed (unscaled) tolerances of the classification.
const DEG: f64 = 1.0;
const SIDE: f64 = 0.05;
const UNIT: f64 = 0.001;
const PROBE: f64 = 10.0;

/// Exchanges the two sides and reverses the line.
fn swap(l: &mut Line) {
    std::mem::swap(&mut l.a, &mut l.b);
    std::mem::swap(&mut l.p1, &mut l.p2);
}

fn pitch_class(p: f64) -> u8 {
    if eq(p, 0.0, DEG) {
        b'0'
    } else if lt(p, 90.0, DEG) {
        b'<'
    } else if eq(p, 90.0, DEG) {
        b'='
    } else {
        b'>'
    }
}

fn horizontal(l: &Line, eps: f64) -> bool {
    let d = l.p2.sub(l.p1);
    let s = if d.is_zero(eps) {
        0.0
    } else {
        d.z / d.length()
    };
    s.abs() <= eps
}

fn vertical(l: &Line, eps: f64) -> bool {
    veq(l.p1.flat(), l.p2.flat(), eps)
}

fn op(op: i32, v: i32, r: i32) -> bool {
    match op {
        1 => v == r,
        2 => v >= r,
        3 => v > r,
        4 => v <= r,
        5 => v < r,
        6 => v != r,
        _ => true,
    }
}

fn table1(key: &[u8; 10]) -> i32 {
    for (row, code) in T1.iter() {
        if row.iter().zip(key).all(|(r, k)| *r == ANY || r == k) {
            return *code;
        }
    }
    0
}

fn table2(k: u8, kp: u8, km: u8, np: i32, nm: i32, code: i32) -> i32 {
    for &(rk, rp, rm, o4, v4, o5, v5, c, out) in T2.iter() {
        if (c == -1 || c == code)
            && (rk == ANY || rk == k)
            && (rp == ANY || rp == kp)
            && (rm == ANY || rm == km)
            && op(o4, np, v4)
            && op(o5, nm, v5)
        {
            return out;
        }
    }
    code
}

fn table3(k: u8, code: i32) -> i32 {
    T3.iter()
        .find(|r| r.0 == k && r.1 == code)
        .map_or(code, |r| r.2)
}

fn side_sign(pl: &super::plane::Plane, q: V3) -> u8 {
    if pl.above(q, SIDE) {
        b'+'
    } else if pl.below(q, SIDE) {
        b'-'
    } else {
        b'0'
    }
}

fn classify(l: &mut Line, tol: &Tol) {
    l.sign = if (l.n_plus >= 2 && l.n_minus >= 2)
        || (l.n_plus == 1
            && l.n_minus == 1
            && (l.kind_minus == b'L' as i32 || l.kind_minus == b'T' as i32))
    {
        -1
    } else {
        1
    };
    if l.code == 18 {
        return;
    }
    if super::vec::ge(l.a.angle, 90.0, DEG) && lt(l.b.angle, 90.0, DEG) {
        swap(l);
    }
    if l.a.kind == b'W' as i32 && l.b.kind == b'D' as i32 {
        swap(l);
    }
    let ta = if l.a.flag5 > 0 { b'T' } else { b'F' };
    let tb = if l.b.flag5 > 0 { b'T' } else { b'F' };
    let (pa, pb) = (pitch_class(l.a.angle), pitch_class(l.b.angle));
    let mut dir = if l.p2.z > l.p1.z { b'U' } else { b'D' };
    if horizontal(l, tol.t01) {
        dir = b'H';
    } else if vertical(l, tol.t01) {
        dir = b'V';
    }
    if l.a.plane.coplanar(&l.b.plane, tol.t01) && l.a.model != l.b.model && dir == b'H' {
        l.code = 17;
        return;
    }
    let d = l.p2.sub(l.p1);
    let u = V3::new(d.x, d.y, 0.0).normalize(UNIT).scale(PROBE);
    let ql = l.p1.add(V3::new(-u.y, u.x, 0.0));
    let qr = l.p1.add(V3::new(u.y, -u.x, 0.0));
    let mut sa = side_sign(&l.a.plane, ql);
    let mut sb = side_sign(&l.b.plane, qr);
    if pa == b'<' && pb == b'<' && (dir == b'U' || dir == b'D') {
        let (na, nb) = (l.a.plane.n, l.b.plane.n);
        let c = if dir == b'U' {
            na.cross(nb)
        } else {
            nb.cross(na)
        };
        let z = c.normalize(UNIT).z;
        let s = if gt(z, 0.0, UNIT) {
            b'+'
        } else if lt(z, 0.0, UNIT) {
            b'-'
        } else {
            b'0'
        };
        sa = s;
        sb = s;
    }
    let (np, nm, kp, km) = (l.n_plus, l.n_minus, l.kind_plus as u8, l.kind_minus as u8);
    if l.kind == b'D' as i32 && np == 0 && nm == 1 && km == b'D' {
        l.kind = b'T' as i32;
    }
    let kind = l.kind as u8;
    let key = [
        kind,
        l.a.kind as u8,
        l.b.kind as u8,
        ta,
        tb,
        pa,
        pb,
        dir,
        sa,
        sb,
    ];
    let mut code = table1(&key);
    let k2 = if kind == b'D' && np == 2 && nm == 1 && (km == b'T' || km == b'L') {
        km
    } else {
        kind
    };
    code = table2(k2, kp, km, np, nm, code);
    code = table3(kind, code);
    if l.x200 == 1 && np == 2 && nm == 0 {
        code = 17;
    }
    if kp == b'G' && km == b'G' && dir == b'H' && ta == b'T' && tb == b'F' {
        code = 9;
    }
    if kind == b'D' && np == 1 && nm == 1 {
        code = 17;
    }
    l.code = code;
}

/// Classifies every line.
pub fn classify_all(st: &mut State, tol: &Tol) {
    for l in &mut st.lines {
        classify(l, tol);
    }
}
