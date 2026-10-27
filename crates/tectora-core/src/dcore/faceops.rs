//! Segment insertion into a face and the face closing test, as far as the
//! dormer foot pass ([`super::post::dormer_feet`]) uses them: the original
//! routines are shared with the sweep (S2/S3), which also reaches the
//! branches for hidden wall faces that are not ported here.
//!
//! - [`add_seg`]: adds a segment to a face (dedupe, collinear extension,
//!   prepend, trim at crossings);
//! - [`ends_at`]: how many segments of a face end at a point;
//! - [`trim_crossings`]: trims crossing segments at their intersection and
//!   drops a dangling one;
//! - [`seg_cross`]: the intersection of two segments with the interior bits;
//! - [`close_face`]: the closing test with the growing POINT_EPS;
//! - [`drop_sentinel`]: unlinks segments ending at the top sentinel.

use super::error::CoreError;
use super::geom::{self, Line};
use super::num::{self, V3};
use super::state::*;

/// The number of segments of the face with an end at `p` (VALUE_EPS).
pub fn ends_at(c: &Core, f: FaceId, p: V3) -> i32 {
    let t = c.g.tol.value;
    let mut n = 0;
    let mut s = c.face(f).segs;
    while let Some(si) = s {
        let x = c.seg(si);
        if num::peq(x.p, p, t) || num::peq(x.q, p, t) {
            n += 1;
        }
        s = x.next;
    }
    n
}

/// The intersection point of the lines through two segments and where it
/// lies: 0 when the lines do not meet or the point is outside either
/// segment's box (VALUE_EPS; mode bit 2 / 4 skip the test for the first /
/// second segment), else bit 1 when the point is inside the first segment
/// along some axis (strictly between its ends in one coordinate, within
/// the box in the others) and bit 2 the same for the second. Mode bit 1
/// also counts a point anywhere within the box. Mode 8 returns 1 as soon
/// as the lines meet. The point is the line intersection's output.
pub fn seg_cross(c: &Core, a: SegId, b: SegId, mode: u32) -> (u32, V3) {
    let t = c.g.tol;
    let ve = t.value;
    let (sa, sb) = (c.seg(a), c.seg(b));
    let la = Line {
        p: sa.p,
        d: num::sub(sa.q, sa.p),
    };
    let lb = Line {
        p: sb.p,
        d: num::sub(sb.q, sb.p),
    };
    let (found, x) = geom::line_line(&la, &lb, None, &t);
    if !found {
        return (0, x);
    }
    if mode == 8 {
        return (1, x);
    }
    let lo = |p: f32, q: f32| if q <= p { q } else { p };
    let hi = |p: f32, q: f32| if p <= q { q } else { p };
    let bx = |s: &Seg| -> ([f32; 3], [f32; 3]) {
        (
            [lo(s.p[0], s.q[0]), lo(s.p[1], s.q[1]), lo(s.p[2], s.q[2])],
            [hi(s.p[0], s.q[0]), hi(s.p[1], s.q[1]), hi(s.p[2], s.q[2])],
        )
    };
    let (lo1, hi1) = bx(sa);
    let (lo2, hi2) = bx(sb);
    let inside = |l: &[f32; 3], h: &[f32; 3]| {
        (0..3).all(|k| !num::lt(x[k], l[k], ve)) && (0..3).all(|k| !num::gt(x[k], h[k], ve))
    };
    if !(mode & 2 != 0 || inside(&lo1, &hi1)) || !(mode & 4 != 0 || inside(&lo2, &hi2)) {
        return (0, x);
    }
    // Within the box in every coordinate, strictly inside in coordinate `s`.
    let strict = |l: &[f32; 3], h: &[f32; 3], s: usize| {
        (0..3).all(|k| {
            if k == s {
                num::lt(l[k], x[k], ve) && num::gt(h[k], x[k], ve)
            } else {
                num::le(l[k], x[k], ve) && num::ge(h[k], x[k], ve)
            }
        })
    };
    let within = |l: &[f32; 3], h: &[f32; 3]| {
        (0..3).all(|k| num::le(l[k], x[k], ve) && num::ge(h[k], x[k], ve))
    };
    let mut r = 0;
    if mode & 1 != 0 && within(&lo1, &hi1) {
        r = 1;
    }
    if (0..3).any(|s| strict(&lo1, &hi1, s)) {
        r = 1;
    }
    if mode & 1 != 0 && within(&lo2, &hi2) {
        r |= 2;
    }
    if (0..3).any(|s| strict(&lo2, &hi2, s)) {
        r |= 2;
    }
    (r, x)
}

/// Trims the segments of a face that cross another one at the crossing
/// point when that end is free (no other segment ends there) and the
/// piece cut off is longer than INTSCT_EPS; then unlinks the one segment
/// with a free end whose other end has more than two segments, if there is
/// exactly one such segment.
pub fn trim_crossings(c: &mut Core, f: FaceId) {
    let ie = f64::from(c.g.tol.intsct);
    let mut s = c.face(f).segs;
    while let Some(si) = s {
        let mut o = c.seg(si).next;
        while let Some(oi) = o {
            let (r, x) = seg_cross(c, si, oi, 0);
            let tgt = match r {
                1 => Some(si),
                2 => Some(oi),
                _ => None,
            };
            if let Some(g) = tgt {
                let (p, q) = (c.seg(g).p, c.seg(g).q);
                let d1 = num::len(num::sub(p, x)) as f32;
                let d2 = num::len(num::sub(q, x)) as f32;
                let mut skip = false;
                if ends_at(c, f, p) == 1 {
                    if ends_at(c, f, q) == 1 {
                        skip = true;
                    } else if f64::from(d1) > ie {
                        c.seg_mut(g).p = x;
                    }
                }
                if !skip {
                    let (p, q) = (c.seg(g).p, c.seg(g).q);
                    if ends_at(c, f, q) == 1 && ends_at(c, f, p) != 1 && f64::from(d2) > ie {
                        c.seg_mut(g).q = x;
                    }
                }
            }
            o = c.seg(oi).next;
        }
        s = c.seg(si).next;
    }
    let dangling = |c: &Core, s: SegId| {
        let (p, q) = (c.seg(s).p, c.seg(s).q);
        (ends_at(c, f, p) == 1 && ends_at(c, f, q) > 2)
            || (ends_at(c, f, q) == 1 && ends_at(c, f, p) > 2)
    };
    let mut s = c.face(f).segs;
    let mut d = None;
    while let Some(si) = s {
        if dangling(c, si) {
            d = Some(si);
            break;
        }
        s = c.seg(si).next;
    }
    let Some(d) = d else { return };
    let mut s = c.face(f).segs;
    while let Some(si) = s {
        if si != d && dangling(c, si) {
            return;
        }
        s = c.seg(si).next;
    }
    let (prev, next) = (c.seg(d).prev, c.seg(d).next);
    if c.face(f).segs == Some(d) {
        c.face_mut(f).segs = next;
        if let Some(n) = next {
            c.seg_mut(n).prev = None;
        }
    } else {
        if let Some(p) = prev {
            c.seg_mut(p).next = next;
        }
        if let Some(n) = next {
            c.seg_mut(n).prev = prev;
        }
    }
}

/// Adds the segment `p`-`q` of `kind` (between faces `fa`, `fb`) to face
/// `f` when `emit` is set, the face is open or stands for another face,
/// and the segment is not degenerate (VALUE_EPS). The first segment starts
/// the list (and the head node's list); a duplicate is dropped; a segment
/// continuing a collinear one at a free end extends it (counted as a new
/// segment); otherwise it is prepended and the face's crossings trimmed.
///
/// The extent update of hidden wall faces ('W') is not ported.
#[allow(clippy::too_many_arguments)]
pub fn add_seg(
    c: &mut Core,
    emit: bool,
    f: FaceId,
    kind: u8,
    p: V3,
    q: V3,
    fa: Option<FaceId>,
    fb: Option<FaceId>,
) -> Result<(), CoreError> {
    let t = c.g.tol;
    if c.face(f).typ == b'W' {
        return Err(CoreError::Unimplemented(
            "segment insertion into a hidden wall face",
        ));
    }
    if !emit {
        return Ok(());
    }
    if c.face(f).closed != 0 && c.face(f).alias.is_none() {
        return Ok(());
    }
    if num::peq(p, q, t.value) {
        return Ok(());
    }
    let f = c.face(f).alias.unwrap_or(f);
    let Some(head) = c.face(f).segs else {
        let s = c.new_seg(kind, 0, p, q, None, None, fa, fb);
        c.face_mut(f).segs = Some(s);
        let h = c
            .face(f)
            .head
            .ok_or(CoreError::Fault("face without head node"))?;
        c.node_mut(h).segs = Some(s);
        return Ok(());
    };
    let mut x = Some(head);
    while let Some(xi) = x {
        let o = c.seg(xi);
        if num::peq(o.p, p, t.value) && num::peq(o.q, q, t.value) {
            return Ok(());
        }
        if num::peq(o.p, q, t.value) && num::peq(o.q, p, t.value) {
            return Ok(());
        }
        x = o.next;
    }
    if kind != b'H' && ends_at(c, f, p) < 2 && ends_at(c, f, q) < 2 {
        let mut x = c.face(f).segs;
        while let Some(xi) = x {
            let o = c.seg(xi).clone();
            let touches = num::peq(o.p, p, t.value)
                || num::peq(o.q, p, t.value)
                || num::peq(o.p, q, t.value)
                || num::peq(o.q, q, t.value);
            if touches {
                match geom::collinear_join(o.p, o.q, p, q, &t) {
                    0 => {}
                    -1 => return Ok(()),
                    _ => {
                        // the end of `o` at p (or q) moves to q (or p)
                        let (at_p, at_q) = (num::peq(o.p, p, t.value), num::peq(o.q, p, t.value));
                        if at_p || at_q {
                            if ends_at(c, f, p) > 1 {
                                return Ok(());
                            }
                            if at_p {
                                c.seg_mut(xi).p = q;
                            } else {
                                c.seg_mut(xi).q = q;
                            }
                        } else if num::peq(o.p, q, t.value) {
                            if ends_at(c, f, q) > 1 {
                                return Ok(());
                            }
                            c.seg_mut(xi).p = p;
                        } else if num::peq(o.q, q, t.value) {
                            if ends_at(c, f, q) > 1 {
                                return Ok(());
                            }
                            c.seg_mut(xi).q = p;
                        } else {
                            return Ok(());
                        }
                        c.g.counters[ctr::SEGS] += 1;
                        return Ok(());
                    }
                }
            }
            x = o.next;
        }
    }
    let old = c.face(f).segs;
    let s = c.new_seg(kind, 0, p, q, None, old, fa, fb);
    if let Some(o) = old {
        c.seg_mut(o).prev = Some(s);
    }
    c.face_mut(f).segs = Some(s);
    trim_crossings(c, f);
    Ok(())
}

/// Unlinks every segment of the face with an end at the top sentinel
/// height (not clearly below 100) and marks the face open. When the first
/// segment goes, the original writes the list head of the sweep's current
/// face, not of this face.
pub fn drop_sentinel(c: &mut Core, f: FaceId) -> Result<(), CoreError> {
    let ve = c.g.tol.value;
    let mut s = c.face(f).segs;
    while let Some(si) = s {
        let x = c.seg(si);
        let (prev, next) = (x.prev, x.next);
        if num::ge(x.p[2], 100.0, ve) || num::ge(x.q[2], 100.0, ve) {
            match prev {
                None => {
                    let cf =
                        c.g.cur_face
                            .ok_or(CoreError::Fault("sentinel drop: no current face"))?;
                    c.face_mut(cf).segs = next;
                }
                Some(pv) => c.seg_mut(pv).next = next,
            }
            if let Some(n) = next {
                c.seg_mut(n).prev = prev;
            }
            c.face_mut(f).closed = 0;
        }
        s = next;
    }
    Ok(())
}

fn reverse(c: &mut Core, s: SegId) {
    let x = c.seg_mut(s);
    std::mem::swap(&mut x.p, &mut x.q);
}

/// Moves `o` directly after `s` (unless it is there already).
fn move_after(c: &mut Core, s: SegId, o: SegId) {
    if c.seg(s).next == Some(o) {
        return;
    }
    let (op, on) = (c.seg(o).prev, c.seg(o).next);
    if let Some(p) = op {
        c.seg_mut(p).next = on;
    }
    if let Some(n) = on {
        c.seg_mut(n).prev = op;
    }
    let sn = c.seg(s).next;
    let om = c.seg_mut(o);
    om.prev = Some(s);
    om.next = sn;
    c.seg_mut(s).next = Some(o);
    if let Some(n) = sn {
        c.seg_mut(n).prev = Some(o);
    }
}

/// The closing test of a face: chains the segments end to start (moving
/// and reversing segments) and marks the face closed when the chain
/// returns to the first segment's start after at least two links. Attempt
/// 0 uses POINT_EPS; with `retry`, attempts 1 to 4 grow it by 0.2 of its
/// starting value each. POINT_EPS is restored when the face stays open,
/// but keeps its grown value when the face closes, as in the original.
/// Returns whether the face is closed.
pub fn close_face(c: &mut Core, f: FaceId, retry: i32) -> Result<bool, CoreError> {
    drop_sentinel(c, f)?;
    if c.face(f).closed != 0 {
        return Ok(true);
    }
    let mut n = 0u32;
    let mut s = c.face(f).segs;
    while let Some(si) = s {
        n += 1;
        s = c.seg(si).next;
    }
    c.g.seg_cur = None;
    if n < 3 {
        return Ok(false);
    }
    let eps0 = c.g.tol.point;
    let mut k = -1i32;
    loop {
        k += 1;
        if k > 0 {
            if k == 5 || retry == 0 {
                c.g.tol.point = eps0;
                return Ok(false);
            }
            c.g.tol.point = (f64::from(eps0) * f64::from(0.2f32) + f64::from(c.g.tol.point)) as f32;
        }
        if chain(c, f)? {
            c.face_mut(f).closed = 1;
            let i = match c.face(f).typ {
                b'D' => Some(ctr::D_CLOSED),
                b'K' => Some(ctr::K_CLOSED),
                b'W' => Some(ctr::W_CLOSED),
                _ => None,
            };
            if let Some(i) = i {
                c.g.counters[i] += 1;
            }
            c.g.counters[ctr::CLOSED] += 1;
            return Ok(true);
        }
    }
}

/// One chaining attempt of [`close_face`] at the current POINT_EPS.
fn chain(c: &mut Core, f: FaceId) -> Result<bool, CoreError> {
    let pt = |c: &Core| c.g.tol.point;
    let mut rev = false;
    let mut links = 0u32;
    let Some(mut s) = c.face(f).segs else {
        return Ok(false);
    };
    loop {
        if c.seg(s).next.is_none() {
            // the last segment must end at the first one's start
            let head = c.face(f).segs.unwrap();
            if num::pne(c.seg(s).q, c.seg(head).p, pt(c)) {
                if !rev {
                    return Ok(false);
                }
                reverse(c, s);
                let head = c.face(f).segs.unwrap();
                if num::pne(c.seg(s).q, c.seg(head).p, pt(c)) {
                    return Ok(false);
                }
            }
            return Ok(links >= 2);
        }
        let find = |c: &Core, s: SegId| {
            let q = c.seg(s).q;
            let mut o = c.seg(s).next;
            while let Some(oi) = o {
                let x = c.seg(oi);
                if !num::pne(q, x.p, pt(c)) || !num::pne(q, x.q, pt(c)) {
                    return Some(oi);
                }
                o = x.next;
            }
            None
        };
        let o = match find(c, s) {
            Some(o) => o,
            None => {
                if !rev {
                    return Ok(false);
                }
                rev = false;
                reverse(c, s);
                match find(c, s) {
                    Some(o) => o,
                    None => return Ok(false),
                }
            }
        };
        let q = c.seg(s).q;
        if num::peq(c.seg(o).q, q, pt(c)) {
            reverse(c, o);
        }
        move_after(c, s, o);
        let q = c.seg(s).q;
        if num::peq(c.seg(o).p, q, pt(c)) && num::peq(c.seg(o).q, q, pt(c)) {
            rev = true;
        }
        links += 1;
        match c.seg(s).next {
            Some(n) => s = n,
            None => return Ok(false),
        }
    }
}
