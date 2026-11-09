//! The wall stage: from the core SOL to the wall polygons (W3D) and the
//! SOL with walls (the core SOL plus code 18 wall lines and a wall face
//! record per wall polygon).
//!
//! The stage reads only the core SOL text, so every number is first
//! taken through `%10.5f` and `sscanf("%f")` ([`requantize`]); among other
//! things that turns "-0.00000" into +0.0.
//!
//! For each wall face (`W`) it collects where the edges of a roof face
//! cross the face's wall line, walks the sorted points inside the roof
//! polygon and takes the roof's height over the inside stretches. The
//! stretches are chained into polygons that are closed down to z = 0.

use std::fmt::Write as _;

use super::num::{f32r, hypot, put_f, requantize};
use super::sol::{CoreSol, crlf};

type P3 = [f64; 3];

// The tolerances are passed to the comparison helpers as doubles and
// compared as doubles (the compared values themselves are floats).
const T01: f64 = 0.01;
const T6: f64 = 1e-6;
const T25: f64 = 0.025;
/// One degree in radians, as the double the area routine passes (it is
/// the float value widened).
const T1DEG: f64 = 0.017_453_292_519_943_295_f64 as f32 as f64;
const T5: f64 = 1e-5;
const PI_F: f64 = std::f64::consts::PI as f32 as f64;
const HALF_PI_F: f64 = std::f64::consts::FRAC_PI_2 as f32 as f64;

/// The two files the wall stage writes.
#[derive(Debug, Clone, PartialEq)]
pub struct WallStage {
    /// The W3D text (CRLF).
    pub w3d: String,
    /// The SOL with walls (CRLF).
    pub sol: String,
    /// The area of every wall face that got a polygon, in SOL order, as
    /// the SOL with walls records it (DACAD's wall areas, "Wand").
    pub areas: Vec<f64>,
}

/// Why the wall stage failed.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum WallError {
    #[error("wall stage: more than 128 points on wall line of wall {0}")]
    TooManyPoints(i32),
    #[error("wall stage: edge {0} refers to no line")]
    NoLine(i32),
}

/// The W3D text for a core SOL.
pub fn write_w3d(sol: &CoreSol) -> Result<String, WallError> {
    Ok(wall_stage(sol)?.w3d)
}

/// The SOL with walls for a core SOL.
pub fn write_wall_sol(sol: &CoreSol) -> Result<String, WallError> {
    Ok(wall_stage(sol)?.sol)
}

// Tolerance tests of the wall stage.
fn w_eq(a: f64, b: f64, t: f64) -> bool {
    (a - b).abs() < t
}
fn w_lt(a: f64, b: f64, t: f64) -> bool {
    t <= (a - b).abs() && a < b
}
fn w_gt(a: f64, b: f64, t: f64) -> bool {
    t <= (a - b).abs() && b < a
}
fn w_le(a: f64, b: f64, t: f64) -> bool {
    !(t <= (a - b).abs() && b <= a)
}
fn w_ge(a: f64, b: f64, t: f64) -> bool {
    !(t <= (a - b).abs() && a <= b)
}
fn w_z(a: f64, t: f64) -> bool {
    a.abs() < t
}
fn w_eq3(p: P3, q: P3, t: f64) -> bool {
    w_eq(p[0], q[0], t) && w_eq(p[1], q[1], t) && w_eq(p[2], q[2], t)
}
fn w_z3(v: P3, t: f64) -> bool {
    w_z(v[0], t) && w_z(v[1], t) && w_z(v[2], t)
}
fn v_sub(a: P3, b: P3) -> P3 {
    [f32r(a[0] - b[0]), f32r(a[1] - b[1]), f32r(a[2] - b[2])]
}
fn v_len(v: P3) -> f64 {
    hypot(hypot(v[0], v[1]), v[2])
}
fn v_norm(v: P3, t: f64) -> P3 {
    let l = f32r(v_len(v));
    if w_z(l, t) {
        return [0.0; 3];
    }
    [f32r(v[0] / l), f32r(v[1] / l), f32r(v[2] / l)]
}

/// A segment with its direction.
#[derive(Debug, Clone, Copy)]
struct Seg {
    p: P3,
    q: P3,
    d: P3,
}

fn seg(p: P3, q: P3) -> Seg {
    Seg {
        p,
        q,
        d: v_norm(v_sub(q, p), T01),
    }
}

fn w_par(a: P3, b: P3, t: f64) -> i32 {
    let (u, v) = (v_norm(a, t), v_norm(b, t));
    if w_eq(u[0], v[0], t) && w_eq(u[1], v[1], t) && w_eq(u[2], v[2], t) {
        return 1;
    }
    if w_eq(-u[0], v[0], t) && w_eq(-u[1], v[1], t) && w_eq(-u[2], v[2], t) {
        return -1;
    }
    0
}

/// A 2x2 system; `None` when the determinant is near zero.
fn solve2(m: [f64; 4], r: [f64; 2], t: f64) -> Option<(f64, f64)> {
    let det = f32r(m[3] * m[0] - m[1] * m[2]);
    if w_z(det, t) {
        return None;
    }
    Some((
        f32r((m[3] * r[0] - m[1] * r[1]) / det),
        f32r((r[1] * m[0] - r[0] * m[2]) / det),
    ))
}

/// Intersection of the lines of two segments, trying the xy, xz and yz
/// systems in turn.
fn line_isect(a: &Seg, b: &Seg, t: f64) -> Option<P3> {
    for (ia, ib) in [(0, 1), (0, 2), (1, 2)] {
        let m = [a.d[ia], f32r(-b.d[ia]), a.d[ib], f32r(-b.d[ib])];
        let r = [f32r(b.p[ia] - a.p[ia]), f32r(b.p[ib] - a.p[ib])];
        let Some((s0, s1)) = solve2(m, r, t) else {
            continue;
        };
        let o = [
            f32r(s0 * a.d[0] + a.p[0]),
            f32r(s0 * a.d[1] + a.p[1]),
            f32r(s0 * a.d[2] + a.p[2]),
        ];
        if (0..3).all(|k| w_eq(o[k], f32r(s1 * b.d[k] + b.p[k]), t)) {
            return Some(o);
        }
    }
    None
}

/// Whether the line intersection lies within the bounding box of `a`
/// (bit 0 of the core's intersection bits; the others are not used here).
fn seg_isect_in_a(a: &Seg, b: &Seg, t: f64) -> Option<P3> {
    let o = line_isect(a, b, t)?;
    let inside = (0..3).all(|k| {
        let (p, q) = (a.p[k], a.q[k]);
        let lo = if q <= p { p.min(q) } else { p };
        let hi = if p <= q { q } else { p };
        w_le(lo, o[k], t) && w_ge(hi, o[k], t)
    });
    inside.then_some(o)
}

/// Point on segment (bit 0 only).
fn on_seg(pt: P3, s: &Seg, t: f64) -> bool {
    let d14 = v_sub(s.q, s.p);
    let d20 = v_sub(pt, s.p);
    let d2c = v_sub(pt, s.q);
    if !(w_z3(d14, t) || w_z3(d2c, t) || w_par(d14, d2c, t) != 0) {
        return false;
    }
    if !(w_z3(d20, t) || w_z3(d2c, t) || w_par(d20, d2c, t) != 0) {
        return false;
    }
    if !(w_z3(d14, t) || w_z3(d20, t) || w_par(d14, d20, t) != 0) {
        return false;
    }
    (0..3).all(|k| {
        let lo = if w_lt(s.p[k], s.q[k], t) {
            s.p[k]
        } else {
            s.q[k]
        };
        let hi = if w_gt(s.p[k], s.q[k], t) {
            s.p[k]
        } else {
            s.q[k]
        };
        w_ge(pt[k], lo, t) && w_le(pt[k], hi, t)
    })
}

/// Point in polygon: 1 on an edge, else the parity of a ray towards +x.
fn pip(px: f64, py: f64, edges: &[i32], lines: &[Seg]) -> bool {
    let mut cnt = 0u32;
    for &e in edges {
        let l = &lines[e.unsigned_abs() as usize - 1];
        let (p1, p2) = ([l.p[0], l.p[1], 0.0], [l.q[0], l.q[1], 0.0]);
        if on_seg([px, py, 0.0], &seg(p1, p2), T01) {
            return true;
        }
        let (x1, y1, x2, y2) = (p1[0], p1[1], p2[0], p2[1]);
        if w_eq(x1, x2, T01) {
            if w_eq(x1, px, T01) {
                if w_lt(y1, py, T01) && w_gt(y2, py, T01) {
                    return true;
                }
                if w_lt(y2, py, T01) && w_gt(y1, py, T01) {
                    return true;
                }
                continue;
            }
        } else if w_eq(y1, y2, T01) {
            if w_eq(y1, py, T01) {
                if w_lt(x1, px, T01) && w_gt(x2, px, T01) {
                    return true;
                }
                if w_lt(x2, px, T01) && w_gt(x1, px, T01) {
                    return true;
                }
            }
            continue;
        }
        let a = w_lt(x1, px, T01);
        let b = w_lt(x2, px, T01);
        if !a {
            if !b {
                let (lo, hi) = if y2 < y1 { (y2, y1) } else { (y1, y2) };
                if w_le(lo, py, T25) && w_gt(hi, py, T25) {
                    cnt += 1;
                }
            } else {
                let yv = f32r((px - x2) * ((y1 - y2) / (x1 - x2)) + y2);
                if yv <= y1 {
                    if w_le(yv, py, T25) && w_gt(y1, py, T25) {
                        cnt += 1;
                    }
                } else if w_le(y1, py, T25) && w_ge(yv, py, T25) {
                    cnt += 1;
                }
            }
        } else if !b {
            let yv = f32r((px - x1) * ((y2 - y1) / (x2 - x1)) + y1);
            if y2 <= yv {
                if w_le(y2, py, T25) && w_ge(yv, py, T25) {
                    cnt += 1;
                }
            } else if w_le(yv, py, T25) && w_gt(y2, py, T25) {
                cnt += 1;
            }
        }
    }
    cnt & 1 == 1
}

/// A face as the wall stage holds it.
struct WFace {
    ch: u8,
    wall: i32,
    sub: i32,
    kind: i32,
    area: f64,
    plane: [f64; 4],
    wl: Seg,
    edges: Vec<i32>,
    /// The segment list, head first.
    segs: Vec<(P3, P3)>,
    ok: bool,
}

impl WFace {
    fn non_vertical_wall(&self) -> bool {
        self.ch == b'W' && !w_z(self.plane[2], T6)
    }
}

/// A point of the crossing list: position and flag (1 wall start,
/// 2 wall end, 0 crossing).
type Pt = (P3, u8);

fn collect(fs: &[WFace], lines: &[Seg], i: usize, j: usize) -> Result<Vec<Pt>, WallError> {
    let mut pts: Vec<Pt> = Vec::new();
    // A new point is dropped only when it equals a crossing point; the
    // list holds 128 points, the stage fails on the 129th.
    let push = |pts: &mut Vec<Pt>, p: P3, fl: u8| {
        if pts.iter().any(|(q, f)| *f == 0 && w_eq3(p, *q, T01)) {
            return Ok(());
        }
        if pts.len() == 128 {
            return Err(WallError::TooManyPoints(fs[i].wall));
        }
        pts.push((p, fl));
        Ok(())
    };
    let wl = &fs[i].wl;
    push(&mut pts, wl.p, 1)?;
    push(&mut pts, wl.q, 2)?;
    for &e in &fs[j].edges {
        let l = lines
            .get((e.unsigned_abs() as usize).wrapping_sub(1))
            .ok_or(WallError::NoLine(e))?;
        let s = seg([l.p[0], l.p[1], 0.0], [l.q[0], l.q[1], 0.0]);
        if let Some(o) = seg_isect_in_a(&s, wl, T01) {
            push(&mut pts, o, 0)?;
        }
    }
    Ok(pts)
}

fn sort_pts(p: &mut [Pt]) {
    let n = p.len();
    if n <= 1 {
        return;
    }
    let mut sw = true;
    while sw {
        sw = false;
        for k in 0..n - 1 {
            if w_lt(p[k + 1].0[0], p[k].0[0], T01) {
                p.swap(k, k + 1);
                sw = true;
            }
        }
    }
    sw = true;
    while sw {
        sw = false;
        for k in 0..n - 1 {
            if w_eq(p[k + 1].0[0], p[k].0[0], T01) && w_lt(p[k + 1].0[1], p[k].0[1], T01) {
                p.swap(k, k + 1);
                sw = true;
            }
        }
    }
}

/// Prepends a segment unless it is a point or already in the list.
fn add_seg(segs: &mut Vec<(P3, P3)>, p: P3, q: P3) {
    if w_eq3(p, q, T01) {
        return;
    }
    if segs.iter().any(|(a, b)| {
        (w_eq3(p, *a, T01) && w_eq3(q, *b, T01)) || (w_eq3(p, *b, T01) && w_eq3(q, *a, T01))
    }) {
        return;
    }
    segs.insert(0, (p, q));
}

/// Walks the sorted points of face `i` against roof face `j`; false when
/// a stretch inside the wall did not lie inside the roof polygon.
fn walk(fs: &mut [WFace], lines: &[Seg], i: usize, j: usize, pts: &[Pt]) -> bool {
    let mut res = true;
    let mut inside = false;
    let mut prev: P3 = [0.0; 3];
    let (pl, je) = (fs[j].plane, fs[j].edges.clone());
    for &(cur, fl) in pts {
        if inside {
            if w_eq3(cur, prev, T01) {
                continue;
            }
            let d = v_sub(cur, prev);
            let h = [f32r(d[0] * 0.5), f32r(d[1] * 0.5), f32r(d[2] * 0.5)];
            let mid = [
                f32r(h[0] + prev[0]),
                f32r(h[1] + prev[1]),
                f32r(h[2] + prev[2]),
            ];
            if !pip(mid[0], mid[1], &je, lines) {
                res = false;
            } else {
                let [nx, ny, nz, dd] = pl;
                let (mut zp, mut zc) = (0.0, 0.0);
                if !w_z(nz, T6) {
                    zp = f32r(((dd - prev[0] * nx) - prev[1] * ny) / nz);
                    zc = f32r(((dd - nx * cur[0]) - cur[1] * ny) / nz);
                }
                add_seg(
                    &mut fs[i].segs,
                    [prev[0], prev[1], zp],
                    [cur[0], cur[1], zc],
                );
            }
        }
        if fl != 0 {
            inside = !inside;
        }
        prev = [cur[0], cur[1], 0.0];
    }
    res
}

/// Chains and orients the segments of a face.
fn chain_segs(s: Vec<(P3, P3)>) -> Vec<(P3, P3)> {
    let mut it = s.into_iter();
    let Some(first) = it.next() else {
        return Vec::new();
    };
    let mut h = vec![first];
    let mut rest: Vec<(P3, P3)> = it.collect();
    loop {
        let mut done = false;
        'scan: for bi in 0..h.len() {
            let b = h[bi];
            for si in 0..rest.len() {
                let x = rest[si];
                if w_eq3(x.0, b.0, T25) {
                    rest.remove(si);
                    h.insert(bi, (x.1, x.0));
                    done = true;
                } else if w_eq3(x.0, b.1, T25) {
                    rest.remove(si);
                    h.insert(bi + 1, x);
                    done = true;
                } else if w_eq3(x.1, b.0, T25) {
                    rest.remove(si);
                    h.insert(bi, x);
                    done = true;
                } else if w_eq3(x.1, b.1, T25) {
                    rest.remove(si);
                    h.insert(bi + 1, (x.1, x.0));
                    done = true;
                }
                if done {
                    break 'scan;
                }
            }
        }
        if done {
            continue;
        }
        if rest.is_empty() {
            return h;
        }
        h.insert(0, rest.remove(0));
    }
}

/// The maximal runs of a segment list where each segment starts at the
/// end of the one before.
fn chains(s: &[(P3, P3)]) -> Vec<&[(P3, P3)]> {
    let mut out = Vec::new();
    let mut start = 0;
    for k in 0..s.len() {
        if k + 1 == s.len() || !w_eq3(s[k].1, s[k + 1].0, T25) {
            out.push(&s[start..=k]);
            start = k + 1;
        }
    }
    out
}

/// The angle between the plane normal's xy part and the -y (`k` = 0) or
/// -x (`k` = 1) axis, as a float.
fn axis_angle(plane: &[f64; 4], k: usize) -> f64 {
    let v = [plane[0], plane[1], 0.0];
    let a = if k == 0 {
        [0.0, -1.0, 0.0]
    } else {
        [-1.0, 0.0, 0.0]
    };
    let dot = f32r((v[2] * a[2] + v[1] * a[1]) + v[0] * a[0]);
    let len = f32r(v_len(v));
    if w_z(len, T5) {
        return HALF_PI_F;
    }
    f32r((dot / len).acos())
}

/// The area of a wall polygon list, projected on the axis plane its
/// normal is closest to.
fn wall_area(plane: &[f64; 4], lines: &[(P3, P3)]) -> f64 {
    let mut a0 = axis_angle(plane, 0);
    let mut a1 = axis_angle(plane, 1);
    if w_eq(a0, HALF_PI_F, T1DEG) && w_eq(a1, HALF_PI_F, T1DEG) {
        return 0.0;
    }
    if w_gt(a0, HALF_PI_F, T1DEG) {
        a0 = f32r(PI_F - a0);
    }
    if w_gt(a1, HALF_PI_F, T1DEG) {
        a1 = f32r(PI_F - a1);
    }
    let yz = a0 >= a1;
    let c = if yz { a1.cos() } else { a0.cos() };
    let mut acc = 0.0;
    for (p, q) in lines {
        let d = if yz {
            q[2] * p[1] - q[1] * p[2]
        } else {
            q[2] * p[0] - q[0] * p[2]
        };
        acc += d;
    }
    (acc * 0.5 / c).abs()
}

/// Runs the wall stage on a core SOL.
pub fn wall_stage(sol: &CoreSol) -> Result<WallStage, WallError> {
    let rq = |v: f32| requantize(v) as f64;
    let rq3 = |v: [f32; 3]| [rq(v[0]), rq(v[1]), rq(v[2])];
    let lines: Vec<Seg> = sol.lines.iter().map(|l| seg(rq3(l.p), rq3(l.q))).collect();
    let mut fs: Vec<WFace> = sol
        .faces
        .iter()
        .map(|f| {
            let w = f.wall_line.map(rq);
            WFace {
                ch: f.ch,
                wall: f.wall,
                sub: f.sub,
                kind: f.kind,
                area: rq(f.area),
                plane: f.plane.map(rq),
                wl: seg([w[0], w[1], 0.0], [w[2], w[3], 0.0]),
                edges: f.edges.clone(),
                segs: Vec::new(),
                ok: false,
            }
        })
        .collect();
    let nf = fs.len();

    // Pass 1: each roofed wall face against its own polygon.
    for i in 0..nf {
        if fs[i].non_vertical_wall() {
            let mut p = collect(&fs, &lines, i, i)?;
            sort_pts(&mut p);
            if p.len() > 2 {
                fs[i].ok = walk(&mut fs, &lines, i, i, &p);
            }
        }
    }
    // Pass 2: wall faces without edges against roofs in parallel planes.
    for i in 0..nf {
        if fs[i].ok || fs[i].ch != b'W' || !fs[i].edges.is_empty() {
            continue;
        }
        for j in 0..nf {
            if j == i || !fs[j].non_vertical_wall() {
                continue;
            }
            let (a, b) = (fs[i].plane, fs[j].plane);
            let c = [
                f32r(b[2] * a[1] - a[2] * b[1]),
                f32r(a[2] * b[0] - a[0] * b[2]),
                f32r(a[0] * b[1] - b[0] * a[1]),
            ];
            if !(w_z3(c, T01) && w_eq(f32r(a[0] * b[3]), f32r(a[3] * b[0]), T01)) {
                continue;
            }
            let mut p = collect(&fs, &lines, i, j)?;
            if p.len() != 2 {
                sort_pts(&mut p);
                if walk(&mut fs, &lines, i, j, &p) {
                    break;
                }
            }
        }
    }
    // Pass 3: the remaining wall faces against any roof.
    for i in 0..nf {
        if fs[i].ok || fs[i].ch != b'W' {
            continue;
        }
        for j in 0..nf {
            if j == i || !fs[j].non_vertical_wall() {
                continue;
            }
            let mut p = collect(&fs, &lines, i, j)?;
            if p.len() != 2 {
                sort_pts(&mut p);
                if walk(&mut fs, &lines, i, j, &p) {
                    break;
                }
            }
        }
    }
    // Drop stretches at the ground, chain the rest.
    for f in fs.iter_mut().filter(|f| f.ch == b'W') {
        let s: Vec<(P3, P3)> = std::mem::take(&mut f.segs)
            .into_iter()
            .filter(|(p, q)| !(w_z(p[2], T01) && w_z(q[2], T01)))
            .collect();
        f.segs = chain_segs(s);
    }
    // Drop wall outlines that duplicate another face's.
    for i in 0..nf {
        for j in 0..nf {
            if i == j || fs[i].segs.len() != fs[j].segs.len() {
                continue;
            }
            let m = |a: &(P3, P3), b: &(P3, P3)| {
                (w_eq3(a.0, b.0, T25) && w_eq3(a.1, b.1, T25))
                    || (w_eq3(a.1, b.0, T25) && w_eq3(a.0, b.1, T25))
            };
            let all = fs[i]
                .segs
                .iter()
                .all(|a| fs[j].segs.iter().any(|b| m(a, b)));
            if all {
                fs[j].segs.clear();
            }
        }
    }

    // Counting pass: edges per chain, the chain start the writing pass
    // starts from, the line and face totals.
    let mut stale: P3 = [0.0; 3];
    let mut n_lines = sol.lines.len();
    let mut n_faces = nf;
    for f in fs.iter().filter(|f| f.ch == b'W') {
        for c in chains(&f.segs) {
            stale = c[0].0;
            n_lines += edge_count(c);
            n_faces += 1;
        }
    }

    // Writing pass: the W3D polygons and, in the same order, the wall
    // lines of the SOL, numbered down from the total.
    let mut w3d = String::new();
    let mut wall_lines = String::new();
    // The emitted lines of each face, for its area.
    let mut emitted: Vec<Vec<(P3, P3)>> = vec![Vec::new(); nf];
    let mut no = n_lines;
    let mut st = stale;
    for (fi, f) in fs.iter().enumerate() {
        if f.ch != b'W' {
            continue;
        }
        for (ci, c) in chains(&f.segs).into_iter().enumerate() {
            let _ = write!(
                w3d,
                "W {:3} {:3} {:3}\n\n\t{:5}\n\n",
                f.wall,
                -(ci as i64 + 1),
                f.kind,
                edge_count(c)
            );
            let mut edge = |a: P3, b: P3| {
                w3d.push('\t');
                put_p3(&mut w3d, a);
                w3d.push_str("\n\t");
                put_p3(&mut w3d, b);
                w3d.push_str("\n\n");
                let _ = write!(wall_lines, "{no:05} ");
                put_p3(&mut wall_lines, a);
                wall_lines.push_str("\n      ");
                put_p3(&mut wall_lines, b);
                wall_lines.push_str(" 18 ");
                put_f(&mut wall_lines, v_len(v_sub(b, a)), 10, 5);
                wall_lines.push('\n');
                no -= 1;
                emitted[fi].push((a, b));
            };
            let p0 = c[0].0;
            if !w_z(p0[2], T01) {
                edge([p0[0], p0[1], 0.0], p0);
                st = p0;
            }
            for s in c {
                edge(s.0, s.1);
            }
            let q = c[c.len() - 1].1;
            if !w_z(q[2], T01) {
                edge(q, [q[0], q[1], 0.0]);
            }
            edge([q[0], q[1], 0.0], [st[0], st[1], 0.0]);
        }
    }

    // The SOL with walls.
    let mut s = String::new();
    let _ = write!(s, "{}\n\n{}\n\n", sol.lead, n_lines);
    s.push_str(&wall_lines);
    for (i, (l, ol)) in lines.iter().zip(&sol.lines).enumerate().rev() {
        let _ = write!(s, "{:05} ", i + 1);
        put_p3(&mut s, l.p);
        s.push_str("\n      ");
        put_p3(&mut s, l.q);
        let _ = write!(s, " {:02} ", ol.code);
        put_f(&mut s, rq(ol.len), 10, 5);
        s.push('\n');
    }
    let _ = write!(s, "\n{n_faces:05}\n\n");
    let mut no = n_lines;
    let mut areas = Vec::new();
    for (fi, f) in fs.iter().enumerate() {
        if !emitted[fi].is_empty() {
            areas.push(wall_area(&f.plane, &emitted[fi]));
        }
        let _ = write!(s, "{:3} {:3} {:3} ", f.wall, f.sub, f.kind);
        put_f(&mut s, f.area, 10, 5);
        let _ = write!(s, "\n\n\t{:5}\n\n", f.edges.len());
        for e in &f.edges {
            let _ = writeln!(s, "\t{e:5}");
        }
        s.push('\n');
        for (ci, c) in chains(&f.segs).into_iter().enumerate() {
            let n = edge_count(c);
            let _ = write!(s, "{:3} {:3} {:3} ", f.wall, -(ci as i64 + 1), f.kind);
            put_f(&mut s, wall_area(&f.plane, &emitted[fi]), 10, 5);
            let _ = write!(s, "\n\n\t{n:5}\n\n");
            for _ in 0..n {
                let _ = writeln!(s, "\t{no:5}");
                no -= 1;
            }
            s.push('\n');
        }
    }
    Ok(WallStage {
        w3d: crlf(&w3d),
        sol: crlf(&s),
        areas,
    })
}

/// The edges of a closed chain: the segments, the verticals at ends off
/// the ground, and the bottom edge.
fn edge_count(c: &[(P3, P3)]) -> usize {
    usize::from(!w_z(c[0].0[2], T01)) + c.len() + usize::from(!w_z(c[c.len() - 1].1[2], T01)) + 1
}

fn put_p3(s: &mut String, v: P3) {
    for (k, x) in v.iter().enumerate() {
        if k > 0 {
            s.push(' ');
        }
        put_f(s, *x, 10, 5);
    }
}
