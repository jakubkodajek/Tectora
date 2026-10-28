//! S3: segment insertion, outline trimming and polygon closing.
//!
//! The sweep (S2, [`super::sweep`]) hands every cut segment to [`add_seg`],
//! which merges it into the face's outline (joining collinear neighbours,
//! dropping duplicates, trimming overshooting ends). [`close_face`] then
//! orders a face's segments into a closed polygon, retrying with a growing
//! point tolerance when asked to.
//!
//! Point matching uses VALUE_EPS unless noted; the closing uses POINT_EPS.

use super::error::CoreError;
use super::geom;
use super::num::{self, V3, ZERO3};
use super::state::{Core, FaceId, SegId, ctr};

fn fault(what: &'static str) -> CoreError {
    CoreError::Fault(what)
}

/// The segments of a face in list order.
pub fn segs_of(c: &Core, f: FaceId) -> Vec<SegId> {
    let mut v = Vec::new();
    let mut s = c.face(f).segs;
    while let Some(i) = s {
        v.push(i);
        s = c.seg(i).next;
    }
    v
}

/// The number of segments of `f` with an end at `p`.
pub fn count_ends(c: &Core, f: FaceId, p: V3) -> i32 {
    let t = c.g.tol.value;
    let mut n = 0;
    let mut s = c.face(f).segs;
    while let Some(i) = s {
        let sg = c.seg(i);
        if num::peq(sg.p, p, t) || num::peq(sg.q, p, t) {
            n += 1;
        }
        s = sg.next;
    }
    n
}

/// Whether a segment of `f` ends at `p`.
pub fn has_end(c: &Core, f: FaceId, p: V3) -> bool {
    count_ends_any(c, f, p)
}

fn count_ends_any(c: &Core, f: FaceId, p: V3) -> bool {
    let t = c.g.tol.value;
    let mut s = c.face(f).segs;
    while let Some(i) = s {
        let sg = c.seg(i);
        if num::peq(sg.p, p, t) || num::peq(sg.q, p, t) {
            return true;
        }
        s = sg.next;
    }
    false
}

/// Whether an end of a segment of `a` meets an end of a segment of `b`.
pub fn share_end(c: &Core, a: FaceId, b: FaceId) -> bool {
    let t = c.g.tol.value;
    for i in segs_of(c, a) {
        let si = c.seg(i);
        for j in segs_of(c, b) {
            let sj = c.seg(j);
            if num::peq(si.p, sj.p, t)
                || num::peq(si.p, sj.q, t)
                || num::peq(si.q, sj.p, t)
                || num::peq(si.q, sj.q, t)
            {
                return true;
            }
        }
    }
    false
}

/// Unlinks `s` from the outline of `f`.
fn unlink(c: &mut Core, f: FaceId, s: SegId) {
    let (prev, next) = (c.seg(s).prev, c.seg(s).next);
    match prev {
        None => c.face_mut(f).segs = next,
        Some(p) => c.seg_mut(p).next = next,
    }
    if let Some(n) = next {
        c.seg_mut(n).prev = prev;
    }
}

/// Moves the start or end point of the wall of a hidden wall face to `p`
/// (z = 0) when `p` lies on the wall's line beyond it.
fn extend_wall(c: &mut Core, face: FaceId, p: V3) -> Result<(), CoreError> {
    let tol = c.g.tol;
    let w = c.face(face).owner.ok_or(fault("wall face without wall"))?;
    let (ws, we) = (c.wall(w).s, c.wall(w).e);
    let mut d1 = num::sub(p, ws);
    d1[2] = 0.0;
    let mut d2 = num::sub(p, we);
    d2[2] = 0.0;
    let l1 = num::len(d1) as f32;
    let l2 = num::len(d2) as f32;
    let n1 = num::norm(d1, &tol);
    let n2 = num::norm(d2, &tol);
    if !num::peq(n1, n2, tol.value) {
        return Ok(());
    }
    let to_end = if num::lt(l1, l2, tol.value) {
        false
    } else if num::lt(l2, l1, tol.value) {
        true
    } else {
        num::peq(c.face(face).w, n1, tol.value)
    };
    let wm = c.wall_mut(w);
    if to_end {
        wm.e = p;
        wm.e[2] = 0.0;
    } else {
        wm.s = p;
        wm.s[2] = 0.0;
    }
    Ok(())
}

/// Adds the segment `p -> q` of kind `kind` (between faces `fa` and `fb`)
/// to the outline of `face`.
///
/// For a hidden wall face the wall is first extended to both points. The
/// segment is added only when `emit` is set, the face is open (or merged
/// into another face, which then receives it) and `p != q`. A duplicate is
/// dropped; a segment continuing a collinear neighbour at a free end
/// extends that neighbour instead (not for 'H' segments); otherwise it is
/// prepended and the outline trimmed ([`trim`]).
#[allow(clippy::too_many_arguments)]
pub fn add_seg(
    c: &mut Core,
    emit: bool,
    face: FaceId,
    kind: u8,
    p: V3,
    q: V3,
    fa: Option<FaceId>,
    fb: Option<FaceId>,
) -> Result<(), CoreError> {
    let t = c.g.tol.value;
    if c.face(face).typ == b'W' {
        extend_wall(c, face, p)?;
        extend_wall(c, face, q)?;
    }
    let f0 = c.face(face);
    if !emit || (f0.closed != 0 && f0.alias.is_none()) || num::peq(p, q, t) {
        return Ok(());
    }
    let face = f0.alias.unwrap_or(face);
    let Some(head) = c.face(face).segs else {
        let s = c.new_seg(kind, 0, p, q, None, None, fa, fb);
        c.face_mut(face).segs = Some(s);
        let h = c.face(face).head.ok_or(fault("face without head node"))?;
        c.node_mut(h).segs = Some(s);
        return Ok(());
    };
    for s in segs_of(c, face) {
        let sg = c.seg(s);
        if (num::peq(sg.p, p, t) && num::peq(sg.q, q, t))
            || (num::peq(sg.p, q, t) && num::peq(sg.q, p, t))
        {
            return Ok(());
        }
    }
    if kind != b'H' && count_ends(c, face, p) < 2 && count_ends(c, face, q) < 2 {
        for s in segs_of(c, face) {
            let sg = c.seg(s).clone();
            let touches = num::peq(sg.p, p, t)
                || num::peq(sg.q, p, t)
                || num::peq(sg.p, q, t)
                || num::peq(sg.q, q, t);
            if !touches {
                continue;
            }
            let r = geom::collinear_join((sg.p, sg.q), (p, q), &c.g.tol);
            if r == 0 {
                continue;
            }
            if r == -1 {
                return Ok(());
            }
            // Replace the shared end of the neighbour by the far end of
            // the new segment, unless the shared point is a junction.
            let (at_p, end, other) = if num::peq(sg.p, p, t) {
                (true, p, q)
            } else if num::peq(sg.q, p, t) {
                (false, p, q)
            } else if num::peq(sg.p, q, t) {
                (true, q, p)
            } else if num::peq(sg.q, q, t) {
                (false, q, p)
            } else {
                return Ok(());
            };
            if count_ends(c, face, end) > 1 {
                return Ok(());
            }
            let sm = c.seg_mut(s);
            if at_p {
                sm.p = other;
            } else {
                sm.q = other;
            }
            c.g.counters[ctr::SEGS] += 1;
            return Ok(());
        }
    }
    let s = c.new_seg(kind, 0, p, q, None, Some(head), fa, fb);
    c.seg_mut(head).prev = Some(s);
    c.face_mut(face).segs = Some(s);
    trim(c, face);
    Ok(())
}

/// Cuts back free segment ends that overshoot an intersection with
/// another segment of the face, then drops one dangling segment hanging
/// off a junction (only when it is the only such segment).
pub fn trim(c: &mut Core, face: FaceId) {
    let tol = c.g.tol;
    let mut s1 = c.face(face).segs;
    while let Some(a) = s1 {
        let mut s2 = c.seg(a).next;
        while let Some(b) = s2 {
            let (sa, sb) = (c.seg(a), c.seg(b));
            let (r, x) = geom::seg_seg((sa.p, sa.q), (sb.p, sb.q), 0, &tol);
            let target = match r {
                1 => Some(a),
                2 => Some(b),
                _ => None,
            };
            if let Some(s) = target {
                let sg = c.seg(s).clone();
                let lp = num::len(num::sub(sg.p, x)) as f32;
                let lq = num::len(num::sub(sg.q, x)) as f32;
                let mut skip = false;
                if count_ends(c, face, sg.p) == 1 {
                    if count_ends(c, face, sg.q) == 1 {
                        skip = true;
                    } else if lp > tol.intsct {
                        c.seg_mut(s).p = x;
                    }
                }
                if !skip {
                    let sg = c.seg(s).clone();
                    if count_ends(c, face, sg.q) == 1
                        && count_ends(c, face, sg.p) != 1
                        && lq > tol.intsct
                    {
                        c.seg_mut(s).q = x;
                    }
                }
            }
            s2 = c.seg(b).next;
        }
        s1 = c.seg(a).next;
    }
    let dangling = |c: &Core, s: SegId| {
        let sg = c.seg(s);
        let (np, nq) = (count_ends(c, face, sg.p), count_ends(c, face, sg.q));
        (np == 1 && nq > 2) || (nq == 1 && np > 2)
    };
    let segs = segs_of(c, face);
    let Some(&d) = segs.iter().find(|&&s| dangling(c, s)) else {
        return;
    };
    if segs.iter().any(|&s| s != d && dangling(c, s)) {
        return;
    }
    unlink(c, face, d);
}

/// Drops the segments of `f` reaching the top (z >= 100) and reopens it.
/// An unlinked first segment is removed from the face in the current-face
/// cursor (as the original does), not necessarily from `f`.
pub fn drop_top(c: &mut Core, f: FaceId) -> Result<(), CoreError> {
    let t = c.g.tol.value;
    let mut s = c.face(f).segs;
    while let Some(si) = s {
        let sg = c.seg(si).clone();
        if num::ge(sg.p[2], 100.0, t) || num::ge(sg.q[2], 100.0, t) {
            match sg.prev {
                None => {
                    let cf = c.g.cur_face.ok_or(fault("no current face"))?;
                    c.face_mut(cf).segs = sg.next;
                }
                Some(p) => c.seg_mut(p).next = sg.next,
            }
            if let Some(n) = sg.next {
                c.seg_mut(n).prev = sg.prev;
            }
            c.face_mut(f).closed = 0;
        }
        s = sg.next;
    }
    Ok(())
}

fn swap_ends(c: &mut Core, s: SegId) {
    let sg = c.seg_mut(s);
    std::mem::swap(&mut sg.p, &mut sg.q);
}

/// Moves `t` right after `s` in the outline of `f` (no-op when it is there).
fn move_after(c: &mut Core, s: SegId, t: SegId) {
    if c.seg(s).next == Some(t) {
        return;
    }
    let (tp, tn) = (c.seg(t).prev, c.seg(t).next);
    if let Some(p) = tp {
        c.seg_mut(p).next = tn;
    }
    if let Some(n) = tn {
        c.seg_mut(n).prev = tp;
    }
    let sn = c.seg(s).next;
    c.seg_mut(t).prev = Some(s);
    c.seg_mut(t).next = sn;
    c.seg_mut(s).next = Some(t);
    if let Some(n) = sn {
        c.seg_mut(n).prev = Some(t);
    }
}

/// The first segment after `s` with an end at `s.q`.
fn find_link(c: &Core, s: SegId, e: f32) -> Option<SegId> {
    let q = c.seg(s).q;
    let mut t = c.seg(s).next;
    while let Some(i) = t {
        let tg = c.seg(i);
        if !num::pne(q, tg.p, e) || !num::pne(q, tg.q, e) {
            return Some(i);
        }
        t = tg.next;
    }
    None
}

/// One ordering attempt with point tolerance `e`: chains the segments
/// head to tail; true when the chain closes with at least three segments.
fn order(c: &mut Core, f: FaceId, e: f32) -> bool {
    let mut rev = false;
    let mut links = 0;
    let mut cur = c.face(f).segs;
    while let Some(s) = cur {
        if c.seg(s).next.is_none() {
            let Some(first) = c.face(f).segs else {
                return false;
            };
            if num::pne(c.seg(s).q, c.seg(first).p, e) {
                if !rev {
                    return false;
                }
                swap_ends(c, s);
                if num::pne(c.seg(s).q, c.seg(first).p, e) {
                    return false;
                }
            }
            return links > 1;
        }
        let t = match find_link(c, s, e) {
            Some(t) => t,
            None => {
                // A segment both of whose ends met the previous one may be
                // the wrong way round: turn it and search again.
                if !rev {
                    return false;
                }
                rev = false;
                swap_ends(c, s);
                match find_link(c, s, e) {
                    Some(t) => t,
                    None => return false,
                }
            }
        };
        let q = c.seg(s).q;
        if num::peq(q, c.seg(t).q, e) {
            swap_ends(c, t);
        }
        move_after(c, s, t);
        let tg = c.seg(t);
        if num::peq(q, tg.p, e) && num::peq(q, tg.q, e) {
            rev = true;
        }
        links += 1;
        cur = c.seg(s).next;
    }
    false
}

/// Closes the outline of `f`: drops segments at the top ([`drop_top`]);
/// true when the face is (already) closed. An open face of at least three
/// segments is ordered into a polygon with POINT_EPS; with `retry` up to
/// four more attempts each widen the tolerance by a fifth of its start
/// value. On success the face is closed and counted, and the widened
/// tolerance stays in effect (the original restores it only on failure).
pub fn close_face(c: &mut Core, f: FaceId, retry: bool) -> Result<bool, CoreError> {
    drop_top(c, f)?;
    if c.face(f).closed != 0 {
        return Ok(true);
    }
    let n = segs_of(c, f).len();
    c.g.seg_cur = None;
    if n < 3 {
        return Ok(false);
    }
    let e0 = c.g.tol.point;
    for attempt in 0..5 {
        if attempt > 0 {
            if !retry {
                break;
            }
            c.g.tol.point = (f64::from(e0) * f64::from(0.2f32) + f64::from(c.g.tol.point)) as f32;
        }
        if order(c, f, c.g.tol.point) {
            let fm = c.face_mut(f);
            fm.closed = 1;
            let typ = fm.typ;
            match typ {
                b'D' => c.g.counters[ctr::D_CLOSED] += 1,
                b'K' => c.g.counters[ctr::K_CLOSED] += 1,
                b'W' => c.g.counters[ctr::W_CLOSED] += 1,
                _ => {}
            }
            c.g.counters[ctr::CLOSED] += 1;
            return Ok(true);
        }
    }
    c.g.tol.point = e0;
    Ok(false)
}

/// Drops the outline of an open face that consists of a single sloped
/// segment.
pub fn drop_single(c: &mut Core, f: FaceId) {
    let fc = c.face(f);
    if fc.closed != 0 {
        return;
    }
    let Some(s) = fc.segs else { return };
    if c.seg(s).next.is_some() {
        return;
    }
    if !seg_horizontal(c, s) {
        c.face_mut(f).segs = None;
    }
}

/// Whether a segment is horizontal (false for a zero-length segment).
fn seg_horizontal(c: &Core, s: SegId) -> bool {
    let sg = c.seg(s);
    let d = num::sub(sg.p, sg.q);
    let t = c.g.tol.value;
    if num::vzero(d, t) {
        return false;
    }
    let l = num::len(d);
    num::zero((f64::from(d[2]) / l) as f32, t)
}

/// Moves the face cursor of `f`'s wall off `f` (up in the first pass,
/// down in the second).
pub fn advance(c: &mut Core, f: FaceId) -> Result<(), CoreError> {
    let w = c.face(f).owner.ok_or(fault("face without wall"))?;
    if c.wall(w).cur == Some(f) {
        let fc = c.face(f);
        c.wall_mut(w).cur = if c.g.pass2 == 0 { fc.next } else { fc.prev };
    }
    Ok(())
}

/// Replaces `s3` of `f1` and `s4` of `f2` by the segment between the
/// first point of each face that lies on the segment's ray within its
/// length; false (nothing changed) when either face has none.
fn fit_pair(c: &mut Core, f1: FaceId, f2: FaceId, s3: SegId, s4: SegId) -> bool {
    let tol = c.g.tol;
    let pick = |c: &Core, f: FaceId, s: SegId| -> Option<V3> {
        let sg = c.seg(s);
        let d = num::sub(sg.q, sg.p);
        for x in segs_of(c, f) {
            if x == s {
                continue;
            }
            let xg = c.seg(x);
            if geom::same_dir_within(d, num::sub(xg.p, sg.p), &tol) {
                return Some(xg.p);
            }
            if geom::same_dir_within(d, num::sub(xg.q, sg.p), &tol) {
                return Some(xg.q);
            }
        }
        None
    };
    let Some(p) = pick(c, f1, s3) else {
        return false;
    };
    let Some(q) = pick(c, f2, s4) else {
        return false;
    };
    for s in [s3, s4] {
        let sm = c.seg_mut(s);
        sm.p = p;
        sm.q = q;
    }
    true
}

/// Merges the outline of the coplanar face `f2` into `f1`: overlapping
/// segments are fitted to each other, crossing segments cut at their
/// intersection and moved over, and the rest of `f2`'s segments added.
pub fn merge_faces(c: &mut Core, f1: FaceId, f2: FaceId) -> Result<(), CoreError> {
    let tol = c.g.tol;
    let mut a = c.face(f1).segs;
    while let Some(s3) = a {
        let mut b = c.face(f2).segs;
        while let Some(s4) = b {
            let (g3, g4) = (c.seg(s3).clone(), c.seg(s4).clone());
            if !geom::seg_on_seg(g3.p, g3.q, g4.p, g4.q, &tol) {
                if geom::seg_on_seg(g4.p, g4.q, g3.p, g3.q, &tol) {
                    fit_pair(c, f1, f2, s3, s4);
                    unlink(c, f1, s3);
                }
            } else {
                fit_pair(c, f1, f2, s3, s4);
                let sm = c.seg_mut(s4);
                sm.p = ZERO3;
                sm.q = ZERO3;
            }
            b = c.seg(s4).next;
        }
        a = c.seg(s3).next;
    }
    let mut a = c.face(f1).segs;
    while let Some(s3) = a {
        let mut b = c.face(f2).segs;
        while let Some(s4) = b {
            let (g3, g4) = (c.seg(s3).clone(), c.seg(s4).clone());
            let (r, x) = geom::seg_seg((g3.p, g3.q), (g4.p, g4.q), 0, &tol);
            if r == 1 || r == 2 {
                let (f, s) = if r == 1 { (f1, s3) } else { (f2, s4) };
                if count_ends(c, f, c.seg(s).p) == 1 {
                    c.seg_mut(s).p = x;
                }
                if count_ends(c, f, c.seg(s).q) == 1 {
                    c.seg_mut(s).q = x;
                }
                let g4 = c.seg(s4).clone();
                add_seg(c, true, f1, g4.kind, g4.p, g4.q, g4.fa, g4.fb)?;
                let sm = c.seg_mut(s4);
                sm.p = ZERO3;
                sm.q = ZERO3;
            }
            b = c.seg(s4).next;
        }
        a = c.seg(s3).next;
    }
    let mut b = c.face(f2).segs;
    while let Some(s) = b {
        let g = c.seg(s).clone();
        add_seg(c, true, f1, g.kind, g.p, g.q, g.fa, g.fb)?;
        b = c.seg(s).next;
    }
    c.g.seg_cur = None;
    Ok(())
}
