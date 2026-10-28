//! S6: chimneys and roof openings inside the core.
//!
//! Two stages:
//!
//! - [`chimney_holes`] (per model, flag 0x8): every hole outline of the RD2
//!   is checked for convexity and turned counter-clockwise, then clipped
//!   against every closed, non-vertical roof face of the current model's
//!   walls. Each non-empty intersection becomes a hole piece (a node with
//!   an `'L'` segment ring and its area) on the face's hole list.
//! - [`hole_edges`] (once, after the code-17 merges): an `'L'`/`'G'` edge
//!   that coincides with or lies on a `'D'` edge takes that edge's code and
//!   a negative length; when it is the edge's only segment and the roof
//!   edge is not an eave, verge or shed top, a copy enters the edge table
//!   as a new line (it becomes the abutment line).
//!
//! The clipper works in the xy projection: the face outline is clipped by
//! the line of every hole edge in turn (keeping the left side). Each step
//! labels the clipped vertices, collects the points where the outline
//! meets the line, sorts them along the line with the C runtime's `qsort`
//! (reimplemented here so that equal keys end in the same order), pairs
//! them into new edges and traces the edge set into one or more polygons.
//! Only the float rounding the original performs is reproduced: every
//! value it keeps in a `float` is an `f32`, intermediate x87 values are
//! `f64` (53-bit precision), angles come from [`x87::fpatan`].

use super::error::{CoreError, Msg};
use super::geom;
use super::num::{self, Tol, V3};
use super::state::*;
use super::x87::{self, X80};

/// The warning box of a non-convex hole outline. DACAD shows "Nicht
/// konvexer Schornstein/Öffnung %d wird ignoriert!" (string 0xa25) and the
/// next progress call cancels the run; the core then returns 1 and leaves
/// the "0 0" placeholder SOL.
pub const MSG_NONCONVEX: Msg = Msg {
    id: Some(0xa25),
    text: "Nicht konvexer Schornstein/Öffnung wird ignoriert!",
};

/// The cancel that follows the warning box (a fatal error with code 0 and
/// no message, which the pipeline returns as 1).
pub const MSG_CANCEL_AFTER_NONCONVEX: Msg = Msg {
    id: None,
    text: "cancelled after: Nicht konvexer Schornstein/Öffnung wird ignoriert!",
};

/// f32(pi), the constant the trace step compares turning angles with.
const PI_F: f32 = f32::from_bits(0x4049_0fdb);
/// The determinant below which the clipper treats two lines as parallel (a double).
const PAR_EPS: f64 = 0.0001;
/// The capacity of the clipper's intersection array.
const MAX_HITS: usize = 100;

// ---------------------------------------------------------------------------------------------
// Small helpers of the clipper

/// `|b-a| <= t`; unordered counts as true.
#[inline]
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn eqle(a: f32, b: f32, t: f32) -> bool {
    let d = (f64::from(b) - f64::from(a)).abs();
    !(d > f64::from(t))
}

/// 2D point equality of the clipper (x and y within VALUE_EPS, `<=`).
#[inline]
fn pt2eq(p: V3, q: V3, t: &Tol) -> bool {
    eqle(p[0], q[0], t.value) && eqle(p[1], q[1], t.value)
}

#[inline]
fn w(x: f32) -> f64 {
    f64::from(x)
}

/// Side of `r` against the line `p -> q` with an exact zero threshold:
/// 1 left, -1 right (or unordered), 0 on it.
fn side(p: V3, q: V3, r: V3) -> i32 {
    let v = (w(r[1]) - w(p[1])) * (w(q[0]) - w(p[0])) - (w(q[1]) - w(p[1])) * (w(r[0]) - w(p[0]));
    if v < 0.0 || v.is_nan() {
        -1
    } else if v > 0.0 {
        1
    } else {
        0
    }
}

/// The turn `a -> b -> c` (positive when counter-clockwise).
fn turn(a: V3, b: V3, c: V3) -> f64 {
    (w(b[0]) - w(a[0])) * (w(c[1]) - w(b[1])) - (w(c[0]) - w(b[0])) * (w(b[1]) - w(a[1]))
}

/// Convexity and orientation of a segment chain: counts the turns between
/// consecutive segments (the closing turn from the last segment back to the
/// first is not counted). Returns (convex, sign) with sign 1 for more left
/// turns, -1 for more right turns, 0 when they balance.
fn convex_sign(c: &Core, first: Option<SegId>) -> (bool, i32) {
    let (mut neg, mut pos) = (0, 0);
    let mut cur = first;
    let mut sign = 0;
    if let Some(s0) = first
        && c.seg(s0).next.is_some()
    {
        while let Some(s) = cur {
            let Some(n) = c.seg(s).next else { break };
            let v = turn(c.seg(s).p, c.seg(s).q, c.seg(n).q);
            if v < 0.0 || v.is_nan() {
                neg += 1;
            } else if v > 0.0 {
                pos += 1;
            }
            cur = Some(n);
        }
        if pos != neg {
            sign = if neg <= pos { 1 } else { -1 };
        }
    }
    (!(pos > 0 && neg > 0), sign)
}

/// Reverses a doubly linked segment chain in place and swaps the end points
/// of every segment. Returns the new head.
fn reverse_chain(c: &mut Core, head: SegId) -> SegId {
    let mut ids = Vec::new();
    let mut s = Some(head);
    while let Some(i) = s {
        ids.push(i);
        s = c.seg(i).next;
    }
    ids.reverse();
    for (k, &i) in ids.iter().enumerate() {
        let sg = c.seg_mut(i);
        std::mem::swap(&mut sg.p, &mut sg.q);
        sg.prev = if k == 0 { None } else { Some(ids[k - 1]) };
        sg.next = ids.get(k + 1).copied();
    }
    ids[0]
}

/// The label map of the edge records: an on-line vertex ('E') becomes a
/// plain outline vertex ('P'), an intersection ('I') a cut vertex ('S').
#[inline]
fn relabel(l: u8) -> u8 {
    match l {
        b'E' => b'P',
        b'I' => b'S',
        x => x,
    }
}

/// Direction angle of `a -> b` in the xy plane, 0 when `dx + dy` is within
/// VALUE_EPS of zero (the original tests the sum, not both components).
fn dir_angle(a: V3, b: V3, t: &Tol) -> X80 {
    let dx = (w(b[0]) - w(a[0])) as f32;
    let dy = (w(b[1]) - w(a[1])) as f32;
    let s = (w(dy) + w(dx)) as f32;
    if eqle(s, 0.0, t.value) {
        return X80::from_f64(0.0);
    }
    x87::fpatan(w(dy), w(dx))
}

/// A vertex of a clipper polygon (z is always 0 inside the clipper).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Vtx {
    p: V3,
    lab: u8,
    fa: Option<FaceId>,
    fb: Option<FaceId>,
}

/// An edge record of the trace step.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Run {
    p: V3,
    lp: u8,
    q: V3,
    lq: u8,
    fa: Option<FaceId>,
    fb: Option<FaceId>,
}

/// The C runtime's `qsort` (the version without median of three): a
/// quicksort on the middle element with an explicit stack, and a
/// selection sort (largest to the end) for partitions of up to 8 elements.
/// Equal keys end in the order this algorithm produces.
pub fn crt_qsort<T, F: FnMut(&T, &T) -> i32>(v: &mut [T], mut cmp: F) {
    let n = v.len();
    if n < 2 {
        return;
    }
    let mut stack: Vec<(usize, usize)> = Vec::new();
    let (mut lo, mut hi) = (0usize, n - 1);
    loop {
        let size = hi - lo + 1;
        let mut pop = false;
        if size <= 8 {
            // selection sort, largest element to the end
            let mut h = hi;
            while h > lo {
                let mut max = lo;
                for p in lo + 1..=h {
                    if cmp(&v[p], &v[max]) > 0 {
                        max = p;
                    }
                }
                v.swap(max, h);
                h -= 1;
            }
            pop = true;
        } else {
            v.swap(lo + size / 2, lo);
            let mut loguy = lo;
            let mut higuy = hi + 1;
            loop {
                loop {
                    loguy += 1;
                    if !(loguy <= hi && cmp(&v[loguy], &v[lo]) <= 0) {
                        break;
                    }
                }
                loop {
                    higuy -= 1;
                    if !(higuy > lo && cmp(&v[higuy], &v[lo]) >= 0) {
                        break;
                    }
                }
                if higuy < loguy {
                    break;
                }
                v.swap(loguy, higuy);
            }
            v.swap(lo, higuy);
            // the element counts compared as the original's byte distances
            let left = higuy as isize - 1 - lo as isize;
            let right = hi as isize - loguy as isize;
            if left >= right {
                if lo + 1 < higuy {
                    stack.push((lo, higuy - 1));
                }
                if loguy < hi {
                    lo = loguy;
                } else {
                    pop = true;
                }
            } else {
                if loguy < hi {
                    stack.push((loguy, hi));
                }
                if lo + 1 < higuy {
                    hi = higuy - 1;
                } else {
                    pop = true;
                }
            }
        }
        if pop {
            match stack.pop() {
                Some((l, h)) => {
                    lo = l;
                    hi = h;
                }
                None => return,
            }
        }
    }
}

/// The comparison of two floats the sort comparators use: 1 when `a > b`,
/// -1 when `a < b` (or unordered), 0 when equal.
fn cmp_up(a: f32, b: f32) -> i32 {
    if a > b {
        1
    } else if a < b || a.is_nan() || b.is_nan() {
        -1
    } else {
        0
    }
}

/// Descending: 1 when `a < b` (or unordered), -1 when `a > b`, 0 when equal.
fn cmp_down(a: f32, b: f32) -> i32 {
    if a < b || a.is_nan() || b.is_nan() {
        1
    } else if a > b {
        -1
    } else {
        0
    }
}

/// Sorts the intersection points along the direction `p -> q`.
fn sort_hits(hits: &mut [Vtx], p: V3, q: V3) {
    let unordered_x = p[0].is_nan() || q[0].is_nan();
    if p[0] < q[0] || unordered_x {
        crt_qsort(hits, |a, b| cmp_up(a.p[0], b.p[0]));
    } else if p[0] > q[0] {
        crt_qsort(hits, |a, b| cmp_down(a.p[0], b.p[0]));
    } else if p[1] < q[1] || p[1].is_nan() || q[1].is_nan() {
        crt_qsort(hits, |a, b| cmp_up(a.p[1], b.p[1]));
    } else {
        crt_qsort(hits, |a, b| cmp_down(a.p[1], b.p[1]));
    }
}

/// Intersection of the line through `a` with direction `d` and the line
/// through `b` with direction `e`, in xy; None when nearly parallel.
fn cross_point(a: V3, d: [f32; 2], b: V3, e: [f32; 2]) -> Option<V3> {
    let det = w(e[0]) * w(d[1]) - w(e[1]) * w(d[0]);
    let mag = if det <= 0.0 || det.is_nan() {
        -det
    } else {
        det
    };
    if mag < PAR_EPS || mag.is_nan() {
        return None;
    }
    let det_f = det as f32;
    let num = (w(a[0]) - w(b[0])) * w(e[1]) - (w(a[1]) - w(b[1])) * w(e[0]);
    let t = num / w(det_f);
    Some([
        (t * w(d[0]) + w(a[0])) as f32,
        (t * w(d[1]) + w(a[1])) as f32,
        0.0,
    ])
}

/// `x` lies between `a` and `b` (L1 distances in x and in y, VALUE_EPS).
fn between(a: V3, b: V3, x: V3, t: &Tol) -> bool {
    let dx = (w(b[0]) - w(a[0])).abs() as f32;
    let dy = (w(b[1]) - w(a[1])).abs() as f32;
    let sx = ((w(x[0]) - w(a[0])).abs() + (w(b[0]) - w(x[0])).abs()) as f32;
    let sy = ((w(x[1]) - w(a[1])).abs() + (w(b[1]) - w(x[1])).abs()) as f32;
    eqle(dx, sx, t.value) && eqle(dy, sy, t.value)
}

fn find_hit(hits: &[Vtx], p: V3, t: &Tol) -> Option<usize> {
    hits.iter().position(|h| pt2eq(h.p, p, t))
}

fn vtx(p: V3, lab: u8, fa: Option<FaceId>, fb: Option<FaceId>) -> Vtx {
    Vtx {
        p: [p[0], p[1], 0.0],
        lab,
        fa,
        fb,
    }
}

// ---------------------------------------------------------------------------------------------
// The clipper

/// What the clipper needs besides the polygons.
struct Ctx<'a> {
    t: &'a Tol,
    hole_face: Option<FaceId>,
    face: FaceId,
    sign: i32,
}

/// Clips the polygon `inp` by the line `p -> q` (keeping the left side)
/// into the polygon list `out` at `cur` (and possibly more polygons after
/// it). Returns 0 for an empty input, 1 otherwise.
fn clip(
    out: &mut Vec<Vec<Vtx>>,
    cur: usize,
    inp: &[Vtx],
    p: V3,
    q: V3,
    x: &Ctx,
) -> Result<i32, CoreError> {
    let d = [(w(p[0]) - w(q[0])) as f32, (w(p[1]) - w(q[1])) as f32];
    if inp.is_empty() {
        return Ok(0);
    }
    let mut hits: Vec<Vtx> = Vec::new();
    let push_hit = |hits: &mut Vec<Vtx>, h: Vtx| -> Result<(), CoreError> {
        if hits.len() >= MAX_HITS {
            return Err(CoreError::Fault(
                "hole clipper: more than 100 intersection points",
            ));
        }
        hits.push(h);
        Ok(())
    };
    // The crossing point is a local of the original; a nearly parallel
    // crossing keeps the one of the previous crossing.
    let mut stale: Option<V3> = None;
    let mut sp = side(p, q, inp[0].p);
    for k in 1..inp.len() {
        let (pv, cv) = (inp[k - 1], inp[k]);
        let sc = side(p, q, cv.p);
        if (sp == 1 || sp == 0) && sc == 1 {
            if sp == 0 {
                push_hit(&mut hits, vtx(pv.p, pv.lab, cv.fa, cv.fb))?;
            }
            out[cur].push(vtx(cv.p, cv.lab, cv.fa, cv.fb));
        } else if sp == -1 && sc == -1 {
        } else {
            if sp != 0 && sc != 0 {
                // the edge crosses the line
                let e = [
                    (w(pv.p[0]) - w(cv.p[0])) as f32,
                    (w(pv.p[1]) - w(cv.p[1])) as f32,
                ];
                if let Some(xp) = cross_point(p, d, pv.p, e) {
                    stale = Some(xp);
                }
                let Some(xp) = stale else {
                    return Err(CoreError::Fault(
                        "hole clipper: parallel crossing reads an uninitialised point",
                    ));
                };
                if between(pv.p, cv.p, xp, x.t) {
                    out[cur].push(vtx(xp, b'I', cv.fa, cv.fb));
                    let lab = if pv.lab == b'P' || cv.lab == b'P' {
                        b'I'
                    } else {
                        b'T'
                    };
                    push_hit(&mut hits, vtx(xp, lab, cv.fa, cv.fb))?;
                }
            }
            if sc == 0 {
                out[cur].push(vtx(cv.p, b'E', cv.fa, cv.fb));
                if sp == 1 {
                    push_hit(&mut hits, vtx(cv.p, cv.lab, cv.fa, cv.fb))?;
                }
            } else if sc == 1 {
                out[cur].push(vtx(cv.p, cv.lab, cv.fa, cv.fb));
            }
        }
        sp = sc;
    }
    let Some(&first) = out[cur].first() else {
        return Ok(1);
    };
    out[cur].push(first);
    if hits.is_empty() {
        return Ok(1);
    }
    sort_hits(&mut hits, p, q);

    // Edge records: the clipped outline without the pieces along the line...
    let is_ei = |l: u8| l == b'I' || l == b'E';
    let lab_of = |v: &Vtx| -> u8 {
        if is_ei(v.lab) {
            match find_hit(&hits, v.p, x.t) {
                Some(i) => relabel(hits[i].lab),
                None => b' ',
            }
        } else {
            relabel(v.lab)
        }
    };
    let poly = &out[cur];
    let mut runs: Vec<Run> = Vec::new();
    let mut lp = lab_of(&poly[0]);
    for k in 1..poly.len() {
        let (a, b) = (&poly[k - 1], &poly[k]);
        let lq = lab_of(b);
        if !(is_ei(a.lab) && is_ei(b.lab)) {
            runs.push(Run {
                p: a.p,
                lp,
                q: b.p,
                lq,
                fa: b.fa,
                fb: b.fb,
            });
        }
        lp = lq;
    }
    // ... and the pieces of the line between paired intersection points.
    let mut k = 1;
    while k < hits.len() {
        let (a, b) = (&hits[k - 1], &hits[k]);
        if !pt2eq(a.p, b.p, x.t) {
            let (s, e) = if x.sign == -1 { (b, a) } else { (a, b) };
            runs.push(Run {
                p: s.p,
                lp: relabel(s.lab),
                q: e.p,
                lq: relabel(e.lab),
                fa: x.hole_face,
                fb: Some(x.face),
            });
        }
        k += 2;
    }
    out.truncate(cur + 1);
    out[cur].clear();
    trace(out, cur, runs, x.t);
    Ok(1)
}

/// The next edge record starting at `pt`: the first one that does not
/// turn back by pi (with a plain outline start, only one between two cut
/// vertices qualifies), else the last one starting there.
fn next_run(runs: &[Run], pt: V3, ang: f32, lab: u8, t: &Tol) -> Option<usize> {
    let mut cand = None;
    for (j, r) in runs.iter().enumerate() {
        if pt2eq(pt, r.p, t) {
            let a2 = dir_angle(r.p, r.q, t);
            let diff = a2.fsub(w(ang)).abs() as f32;
            if !eqle(diff, PI_F, t.angle) && (lab != b'P' || (r.lp == b'S' && r.lq == b'S')) {
                return Some(j);
            }
            cand = Some(j);
        }
    }
    cand
}

/// Traces the edge records into closed polygons, starting at `cur` and
/// inserting further polygons after it.
fn trace(out: &mut Vec<Vec<Vtx>>, cur: usize, mut runs: Vec<Run>, t: &Tol) {
    let mut lh = cur;
    let mut flag = false;
    if let Some(r) = runs.first() {
        out[lh].push(vtx(r.p, r.lp, r.fa, r.fb));
    }
    let mut ei = 0;
    while !runs.is_empty() {
        if flag {
            out.insert(lh + 1, Vec::new());
            lh += 1;
            flag = false;
            let r = runs[ei];
            out[lh].push(vtx(r.p, r.lp, r.fa, r.fb));
        }
        let e = runs[ei];
        out[lh].push(vtx(e.q, e.lq, e.fa, e.fb));
        let first = out[lh][0];
        if pt2eq(first.p, e.q, t) {
            out[lh].push(vtx(e.q, first.lab, e.fa, e.fb));
            flag = true;
            out[lh][0].fa = e.fa;
            out[lh][0].fb = e.fb;
            runs.remove(ei);
            if runs.is_empty() {
                return;
            }
            ei = 0;
        } else {
            let ang = dir_angle(e.p, e.q, t).to_f32();
            let lab = if e.lp == b'S' && e.lq == b'S' {
                b'S'
            } else {
                b'P'
            };
            runs.remove(ei);
            match next_run(&runs, e.q, ang, lab, t) {
                Some(j) => ei = j,
                None => {
                    out[lh].clear();
                    ei = 0;
                }
            }
        }
    }
    // The loop ran out of records while a polygon was open: it is dropped.
    out[lh].clear();
    out.truncate(lh + 1);
}

/// Height of the face plane `n . x = d` at (x, y); 0 for a vertical plane.
fn plane_z(v: V3, n: V3, d: f32) -> f32 {
    if n[2] == 0.0 || n[2].is_nan() {
        return 0.0;
    }
    (((w(d) - w(v[0]) * w(n[0])) - w(v[1]) * w(n[1])) / w(n[2])) as f32
}

/// Area of a clipper polygon on a face of pitch `pitch` (shoelace / cos).
fn piece_area(poly: &[Vtx], pitch: f32) -> f32 {
    let mut acc = 0.0f64;
    for k in 1..poly.len() {
        let (a, b) = (poly[k - 1].p, poly[k].p);
        acc += w(b[1]) * w(a[0]) - w(a[1]) * w(b[0]);
    }
    let a = (acc * 0.5).abs();
    x87::fcos(w(pitch)).fdivr(a) as f32
}

/// Stores one clipped polygon as a hole piece of `face` after `last`.
/// Returns whether a piece was created.
fn store_piece(c: &mut Core, face: FaceId, last: &mut Option<NodeId>, poly: &mut [Vtx]) -> bool {
    if poly.is_empty() {
        return false;
    }
    let t = c.g.tol;
    let (n, d) = (c.face(face).n, c.face(face).d);
    let mut segs: Vec<(V3, V3, Option<FaceId>)> = Vec::new();
    for k in 1..poly.len() {
        if !pt2eq(poly[k - 1].p, poly[k].p, &t) {
            poly[k - 1].p[2] = plane_z(poly[k - 1].p, n, d);
            poly[k].p[2] = plane_z(poly[k].p, n, d);
            segs.push((poly[k - 1].p, poly[k].p, poly[k].fb));
        }
    }
    if segs.len() < 3 || !pt2eq(segs[0].0, segs[segs.len() - 1].1, &t) {
        return false;
    }
    let base = c.segs.len() as u32;
    let cnt = segs.len() as u32;
    for (k, (p, q, fb)) in segs.into_iter().enumerate() {
        let k = k as u32;
        c.segs.push(Seg {
            org: 0,
            kind: b'L',
            line: 0,
            code: 0,
            code2: 0,
            fl: 0,
            p,
            q,
            len: 0.0,
            fa: c.g.scratch100,
            fb,
            prev: if k == 0 {
                None
            } else {
                Some(SegId(base + k - 1))
            },
            next: if k + 1 == cnt {
                None
            } else {
                Some(SegId(base + k + 1))
            },
        });
    }
    let node = NodeId(c.nodes.len() as u32);
    c.nodes.push(Node {
        no: c.face(face).sub,
        b10: 1,
        area: piece_area(poly, c.face(face).pitch),
        segs: Some(SegId(base)),
        ..Node::blank()
    });
    if let Some(l) = *last {
        c.node_mut(l).next = Some(node);
    }
    if c.face(face).holes.is_none() {
        c.face_mut(face).holes = Some(node);
    }
    let (_, sign) = convex_sign(c, Some(SegId(base)));
    if sign == -1 {
        let h = reverse_chain(c, SegId(base));
        c.node_mut(node).segs = Some(h);
    }
    *last = Some(node);
    true
}

/// Cuts one hole outline (`hole`, oriented counter-clockwise) out of one
/// closed roof face.
fn cut_face(c: &mut Core, face: FaceId, hole: Option<SegId>) -> Result<(), CoreError> {
    let t = c.g.tol;
    let fsegs = c.face(face).segs;
    let (_, fsign) = convex_sign(c, fsegs);
    let mut poly = Vec::new();
    let mut s = fsegs;
    while let Some(i) = s {
        let sg = c.seg(i);
        poly.push(vtx(sg.q, b'P', sg.fa, sg.fb));
        s = sg.next;
    }
    let Some(&first) = poly.first() else {
        return Err(CoreError::Fault("hole clipper: face without segments"));
    };
    poly.push(first);
    c.g.scratch0fc = 1;
    let x = Ctx {
        t: &t,
        hole_face: c.g.scratch100,
        face,
        sign: fsign,
    };
    let mut a: Vec<Vec<Vtx>> = vec![poly];
    let mut h = hole;
    while let Some(hs) = h {
        let (p, q) = (c.seg(hs).p, c.seg(hs).q);
        let mut b: Vec<Vec<Vtx>> = vec![Vec::new()];
        let mut cur = 0;
        let n_in = a.len();
        for (k, inp) in a.iter().enumerate() {
            clip(&mut b, cur, inp, p, q, &x)?;
            if !b[cur].is_empty() && k + 1 < n_in {
                b.push(Vec::new());
                cur = b.len() - 1;
            }
        }
        a = b;
        h = c.seg(hs).next;
    }
    let mut last = c.face(face).holes;
    while let Some(l) = last.and_then(|l| c.node(l).next) {
        last = Some(l);
    }
    for poly in a.iter_mut() {
        if !poly.is_empty() && store_piece(c, face, &mut last, poly) {
            c.g.n_pieces2 += 1;
            c.g.n_pieces += 1;
        }
    }
    Ok(())
}

/// S6, per model with flag 0x8: clips every hole outline against every
/// closed, non-vertical roof face of the current model's walls.
///
/// A non-convex outline shows the warning box; DACAD cancels the run at the
/// next progress call, which this returns as a fatal error (code 0).
pub fn chimney_holes(c: &mut Core) -> Result<(), CoreError> {
    let Some(holes) = c.g.holes else {
        return Ok(());
    };
    let Some(start) = c.g.ring else {
        return Ok(());
    };
    let t = c.g.tol;
    let mut wall = start;
    loop {
        let mut h = Some(holes);
        while let Some(hn) = h {
            let Some(hsegs) = c.node(hn).segs else {
                return Err(CoreError::Fault("hole without outline"));
            };
            c.g.scratch100 = c.seg(hsegs).fa;
            let (convex, sign) = convex_sign(c, Some(hsegs));
            if !convex {
                return Err(CoreError::fatal(0, MSG_CANCEL_AFTER_NONCONVEX));
            }
            if sign == -1 {
                let head = reverse_chain(c, hsegs);
                c.node_mut(hn).segs = Some(head);
            }
            let hsegs = c.node(hn).segs;
            let mut f = c.wall(wall).faces;
            while let Some(fi) = f {
                let fc = c.face(fi);
                if !eqle(fc.pitch, num::HALF_PI_F, t.angle)
                    && fc.closed != 0
                    && fc.segs.is_some()
                    && fc.typ != b'W'
                {
                    cut_face(c, fi, hsegs)?;
                }
                f = c.face(fi).next;
            }
            h = c.node(hn).next;
        }
        match c.wall(wall).next {
            Some(n) if n != start => wall = n,
            _ => break,
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Hole edges against roof edges

/// The two segments are equal in either direction (points within VALUE_EPS).
pub fn seg_same(a: (V3, V3), b: (V3, V3), t: &Tol) -> bool {
    (num::peq(a.0, b.0, t.value) && num::peq(a.1, b.1, t.value))
        || (num::peq(a.0, b.1, t.value) && num::peq(a.1, b.0, t.value))
}

/// S6, once after the code-17 merges: an `'L'`/`'G'` edge equal to or lying
/// on a `'D'` edge takes its code and a negative length; an `'L'`/`'G'`
/// edge with a single segment on a roof edge that is not an eave, verge or
/// shed top is also copied into the edge table as a new line.
pub fn hole_edges(c: &mut Core) -> Result<(), CoreError> {
    let t = c.g.tol;
    let k = c.g.codes;
    let mut e = c.g.edges;
    while let Some(ei) = e {
        let Some(s) = c.edges[ei.ix()].seg else {
            return Err(CoreError::Fault("edge without segment"));
        };
        let sg = c.seg(s);
        if (sg.org == b'L' || sg.org == b'G') && sg.code != k.c17 {
            let hs = (sg.p, sg.q);
            let mut r = c.g.edges;
            while let Some(ri) = r {
                if ri != ei {
                    let Some(rs) = c.edges[ri.ix()].seg else {
                        return Err(CoreError::Fault("edge without segment"));
                    };
                    let rg = c.seg(rs);
                    if rg.org == b'D' && rg.code != k.c17 {
                        let rsp = (rg.p, rg.q);
                        if seg_same(rsp, hs, &t) || geom::seg_on_seg(rsp.0, rsp.1, hs.0, hs.1, &t) {
                            let code = rg.code;
                            if c.edges[ei.ix()].cnt == 1
                                && code != k.vl
                                && code != k.vr
                                && code != k.os
                                && code != k.p
                                && code != k.t
                            {
                                c.g.n_lines += 1;
                                let mut ns = c.seg(s).clone();
                                ns.line = c.g.n_lines;
                                ns.code = k.none;
                                let nsid = SegId(c.segs.len() as u32);
                                c.segs.push(ns);
                                let mut ne = c.edges[ei.ix()].clone();
                                ne.cnt = 0;
                                ne.seg = Some(nsid);
                                ne.no = c.g.n_lines;
                                ne.next = c.g.edges;
                                c.g.edges = Some(EdgeId(c.edges.len() as u32));
                                c.edges.push(ne);
                            }
                            let sm = c.seg_mut(s);
                            sm.code = code;
                            if sm.len > 0.0 {
                                sm.len = -sm.len;
                            }
                            break;
                        }
                    }
                }
                r = c.edges[ri.ix()].next;
            }
        }
        e = c.edges[ei.ix()].next;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reference: the same algorithm on indices with the original's pointer
    /// arithmetic spelled out, checked against a stable sort for distinct keys.
    #[test]
    fn crt_qsort_sorts() {
        let mut seed = 12345u32;
        for n in 0..40 {
            let mut v: Vec<(i32, usize)> = (0..n)
                .map(|i| {
                    seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                    (((seed >> 16) % 7) as i32, i)
                })
                .collect();
            let mut want = v.clone();
            crt_qsort(&mut v, |a, b| (a.0 > b.0) as i32 - (a.0 < b.0) as i32);
            want.sort_by_key(|x| x.0);
            let keys: Vec<i32> = v.iter().map(|x| x.0).collect();
            let wk: Vec<i32> = want.iter().map(|x| x.0).collect();
            assert_eq!(keys, wk);
        }
    }

    #[test]
    fn crt_qsort_tie_order_small() {
        // selection sort moves the first maximum to the end: equal keys are
        // swapped, not kept in order.
        let mut v = vec![(1, 'a'), (1, 'b'), (0, 'c')];
        crt_qsort(&mut v, |a, b| (a.0 > b.0) as i32 - (a.0 < b.0) as i32);
        assert_eq!(v, vec![(0, 'c'), (1, 'b'), (1, 'a')]);
    }
}
