//! S5: line codes of the edge table, after every model was cut.
//!
//! Four global stages run on the edge list (`g.edges`), each looking at the
//! representative segment of every edge (`Edge::seg`, whose `code` the SOL
//! prints):
//!
//! 1. [`classify`]: codes of edges between two faces of a roof, a wall
//!    model or an eave on a gable plane (origins 'D', 'W', 'N'), from the
//!    two face pitches, the segment kind and the face geometry. An edge no
//!    roofed face touches, or that lies between two faces without the roof
//!    flag, becomes "not a roof line" (17) and keeps its code in `code2`.
//! 2. [`codes`]: edges of dormer models ('G') and chimney holes ('L'). It
//!    runs a second time after the hole edges were added.
//! 3. [`merge_d17`]: a 'D' edge lying on another 'D' edge becomes 17; when
//!    only the other one touches an abutment edge at an end, the two swap.
//! 4. [`merge_lg17`]: an 'L' or 'G' edge lying on another one becomes 17.
//!
//! Pitches are compared with ANGLE_EPS against 0 and pi/2 (as the float the
//! core stores), points and lengths with VALUE_EPS unless noted.

use super::geom::{self, Line};
use super::num::{self, HALF_PI_F, V3};
use super::state::{Codes, Core, EdgeId, Face, FaceId, Seg, SegId};

/// The representative segments of the edge list, in list order.
fn edge_segs(c: &Core) -> Vec<(EdgeId, SegId)> {
    let mut out = Vec::new();
    let mut e = c.g.edges;
    while let Some(id) = e {
        let edge = &c.edges[id.ix()];
        if let Some(s) = edge.seg {
            out.push((id, s));
        }
        e = edge.next;
    }
    out
}

/// The letter of the wall owning a face.
fn owner_typ(c: &Core, f: &Face) -> u8 {
    c.wall(f.owner.expect("face without owner wall")).typ
}

/// Whether no end of another segment of `face` lies clearly above an end of
/// `s` (ends equal to an end of `s` are not looked at). False for a face
/// without segments.
fn lowest(c: &Core, s: &Seg, face: FaceId) -> bool {
    let v = c.g.tol.value;
    let mut x = c.face(face).segs;
    if x.is_none() {
        return false;
    }
    while let Some(id) = x {
        let r = c.seg(id);
        for e in [r.p, r.q] {
            if num::pne(s.p, e, v)
                && num::pne(s.q, e, v)
                && (num::gt(e[2], s.p[2], v) || num::gt(e[2], s.q[2], v))
            {
                return false;
            }
        }
        x = r.next;
    }
    true
}

/// Whether no other segment of `face` crosses the vertical line through
/// the midpoint of `s` clearly above that midpoint.
fn open_above(c: &Core, face: FaceId, sid: SegId) -> bool {
    let t = &c.g.tol;
    let s = c.seg(sid);
    let half = num::scale(num::sub(s.q, s.p), 0.5);
    let mid = num::add(s.p, half);
    let up = num::add(mid, [0.0, 0.0, 1.0]);
    let mut x = c.face(face).segs;
    while let Some(id) = x {
        let r = c.seg(id);
        if id != sid {
            let (m, o) = geom::seg_seg((mid, up), (r.p, r.q), 7, t);
            if m & 2 != 0 && num::gt(o[2], mid[2], t.value) {
                return false;
            }
        }
        x = r.next;
    }
    true
}

/// The start and end of a face's wall (a zero-length wall: its start point
/// twice).
fn wall_ends(c: &Core, f: &Face) -> Option<(V3, V3)> {
    let w = c.wall(f.owner?);
    Some(if w.zero != 0 {
        (w.s0, w.s0)
    } else {
        (w.s, w.e)
    })
}

/// Hip or valley code of the turn of the two walls of a line, the plan
/// orientation of the four wall ends taken against each other's wall.
fn hip_valley(k: &Codes, o_p: i32, o_q: i32) -> i32 {
    match (o_p, o_q) {
        (-1, q) if q <= 0 => k.g,
        (0, -1) => k.g,
        (0, 1) => k.k,
        (1, q) if q >= 0 => k.k,
        _ => k.none,
    }
}

/// The code of a sloped line ('S' kind) from the walls of its two faces:
/// hip or valley from the plan turn of the two walls, a verge (left/right)
/// or oblique verge when one face is vertical, `none` when undecided.
fn ridge_rel(c: &Core, fa: FaceId, fb: FaceId, sid: SegId) -> i32 {
    let t = &c.g.tol;
    let k = &c.g.codes;
    let (v, ang) = (t.value, t.angle);
    let a = c.face(fa);
    let b = c.face(fb);
    let s = c.seg(sid);
    let between = num::angle_stored(a.w, b.w).to_f32();
    let d = num::sub(s.q, s.p);
    let dh = num::norm([d[0], d[1], 0.0], t);
    let (Some((mut p1, mut q1)), Some((mut p2, mut q2))) = (wall_ends(c, a), wall_ends(c, b))
    else {
        return k.none;
    };
    if num::peq(p1, q1, v) {
        q1 = num::add(p1, a.w);
    }
    if num::peq(p2, q2, v) {
        q2 = num::add(p2, b.w);
    }
    let mut saved = (p2, q2);
    for i in 0.. {
        match i {
            0 => saved = (p2, q2),
            1 => saved = (p1, q1),
            _ => {}
        }
        let (o1, o2, o3, o4);
        let joined = num::peq(q1, p2, v) || (!num::peq(p1, q2, v) && num::peq(p2, p1, v));
        if joined {
            q1 = num::add(p1, a.w);
            q2 = num::add(p2, b.w);
            o1 = 0;
            o2 = num::orient(p1, q1, q2, t);
            o3 = num::orient(p2, q2, p1, t);
            o4 = 0;
        } else if num::peq(p1, q2, v) {
            q1 = num::add(p1, a.w);
            q2 = num::add(p2, b.w);
            o1 = num::orient(p1, q1, p2, t);
            o2 = 0;
            o3 = 0;
            o4 = num::orient(p2, q2, q1, t);
        } else if num::peq(q1, q2, v) {
            q1 = num::add(p1, a.w);
            q2 = num::add(p2, b.w);
            o1 = num::orient(p1, q1, p2, t);
            o2 = 0;
            o3 = num::orient(p2, q2, p1, t);
            o4 = 0;
        } else {
            o1 = num::orient(p1, q1, p2, t);
            o2 = num::orient(p1, q1, q2, t);
            o3 = num::orient(p2, q2, p1, t);
            o4 = num::orient(p2, q2, q1, t);
        }
        for (f, id) in [(a, fa), (b, fb)] {
            if num::eq(f.pitch, HALF_PI_F, ang) {
                if !open_above(c, id, sid) {
                    return k.k;
                }
                if num::ne(between, HALF_PI_F, ang) {
                    return k.os;
                }
                let rising = num::lt(s.p[2], s.q[2], v);
                return if num::peq(dh, f.w, v) == rising {
                    k.vr
                } else {
                    k.vl
                };
            }
        }
        let c1 = hip_valley(k, o1, o2);
        let c2 = hip_valley(k, o3, o4);
        if c1 == c2 && c1 != k.none {
            return c1;
        }
        if c1 == k.none && c2 != k.none {
            return c2;
        }
        if c1 != k.none && c2 == k.none {
            return c1;
        }
        // Undecided: lengthen one wall (the first, then the second) by 100
        // in both directions and look again.
        match i {
            0 => {
                // The original also lengthens q1 here, then restores it.
                p1 = num::add(p1, num::scale(a.w, -100.0));
                q2 = saved.1;
                q1 = saved.0;
            }
            1 => {
                q2 = num::add(q2, num::scale(b.w, 100.0));
                p2 = num::add(p2, num::scale(b.w, -100.0));
                p1 = saved.0;
                q1 = saved.1;
            }
            _ => return k.none,
        }
    }
    unreachable!()
}

/// How a classified segment continues: `Final` applies the roof-flag rule,
/// `Next` leaves the segment as it is.
enum Step {
    Final,
    Next,
}

/// The code of one D/N/W-origin segment between faces `fa` and `fb`.
fn line_code(c: &Core, sid: SegId, fa: FaceId, fb: FaceId, low: bool, code: &mut i32) -> Step {
    let t = &c.g.tol;
    let k = &c.g.codes;
    let (v, ang) = (t.value, t.angle);
    let hp = HALF_PI_F;
    let s = c.seg(sid);
    let a = c.face(fa);
    let b = c.face(fb);
    let (pa, pb) = (a.pitch, b.pitch);
    let lt = |p: f32| num::lt(p, hp, ang);
    let gt = |p: f32| num::gt(p, hp, ang);
    let vert = |p: f32| num::eq(p, hp, ang);
    let wall = a.typ == b'W' || b.typ == b'W';
    let gable = |other: f32, code: &mut i32| {
        if gt(other) {
            return Step::Next;
        }
        if s.kind == b'S' {
            *code = k.k;
        }
        if s.kind == b'H' {
            *code = k.c;
        }
        Step::Final
    };
    if owner_typ(c, a) == b'T' {
        return gable(pb, code);
    }
    if owner_typ(c, b) == b'T' {
        return gable(pa, code);
    }
    if s.kind == b'T' {
        if lt(pa) {
            *code = k.t;
        } else if num::ge(pa, hp, ang) {
            *code = k.c14;
        }
        return Step::Final;
    }
    if (gt(pa) && lt(pb)) || (lt(pa) && gt(pb)) {
        let line = Line {
            p: s.p,
            d: num::sub(s.p, s.q),
        };
        *code = if geom::horizontal(&line, t) {
            k.c14
        } else {
            k.os
        };
        return Step::Final;
    }
    if gt(pa) || gt(pb) {
        *code = k.c17;
        return Step::Final;
    }
    if vert(pa) && vert(pb) {
        *code = if wall { k.ws } else { k.c13 };
        return Step::Final;
    }
    match s.kind {
        b'K' => {
            *code = if low { k.t } else { k.c };
        }
        b'H' => {
            let knick = |code: &mut i32, flag: bool| {
                *code = if low || flag { k.t } else { k.c };
            };
            if lt(pa) && lt(pb) {
                let va = num::norm([-a.w[1], a.w[0], 0.0], t);
                let vb = num::norm([-b.w[1], b.w[0], 0.0], t);
                if num::peq(va, vb, v) {
                    knick(code, s.fl & 0x40 != 0);
                } else {
                    let tip = num::add(a.o, a.w);
                    match num::orient(a.o, tip, b.o, t) {
                        -1 => *code = k.f,
                        0 | 1 => *code = k.s,
                        _ => {}
                    }
                }
            } else if vert(pa) || vert(pb) {
                // The end of the line on the vertical face's side.
                let (z, sloped, flag) = if vert(pa) {
                    (s.q[2], pb, s.fl & 0x40 != 0)
                } else {
                    (s.p[2], pa, false)
                };
                if wall {
                    if num::lt(a.zlo, z, v) {
                        *code = k.wv;
                    } else if num::gt(a.zlo, z, v) {
                        *code = k.wh;
                    }
                } else if lt(sloped) {
                    if num::peq(b.w, a.w, v) {
                        knick(code, flag);
                    } else {
                        *code = k.p;
                    }
                } else {
                    *code = k.c14;
                }
            }
        }
        b'S' => {
            if wall {
                *code = k.ws;
            } else {
                let r = ridge_rel(c, fa, fb, sid);
                if r != k.none && (vert(pa) || vert(pb) || (lt(pa) && lt(pb))) {
                    *code = r;
                }
            }
        }
        _ => {}
    }
    Step::Final
}

/// Codes of the D/N/W-origin edges (the first classification pass).
/// Wall segments (kind 'W') get the wall code; every other segment that
/// has no code yet and does not come from a hole or a dormer model gets one
/// from its two faces. A segment with a flat face is an eave.
pub fn classify(c: &mut Core) {
    c.g.z_lo = -100.0;
    c.g.z_hi = 100.0;
    let k = c.g.codes;
    let ang = c.g.tol.angle;
    for (eid, sid) in edge_segs(c) {
        let s = c.seg(sid);
        if s.kind == b'W' {
            c.seg_mut(sid).code = k.wall;
            continue;
        }
        if s.code != k.none || s.org == b'L' || s.org == b'G' {
            continue;
        }
        let fa = s.fa.expect("segment without face A");
        let fb = s.fb.expect("segment without face B");
        if num::eq(c.face(fa).pitch, 0.0, ang) || num::eq(c.face(fb).pitch, 0.0, ang) {
            c.seg_mut(sid).code = k.t;
            continue;
        }
        let mut low = false;
        if c.face(fa).b == 0 {
            low = lowest(c, c.seg(sid), fa);
        }
        if c.face(fb).b == 0 {
            low = lowest(c, c.seg(sid), fb);
        }
        let mut code = c.seg(sid).code;
        let step = line_code(c, sid, fa, fb, low, &mut code);
        c.seg_mut(sid).code = code;
        if let Step::Next = step {
            continue;
        }
        let unroofed = c.face(fa).b == 0 && c.face(fb).b == 0;
        if c.edges[eid.ix()].xc == 0 || unroofed {
            let s = c.seg_mut(sid);
            s.code2 = s.code;
            s.code = k.c17;
        }
    }
}

/// Codes of the G-origin (dormer model) and L-origin (chimney hole) edges
/// that have none yet.
pub fn codes(c: &mut Core) {
    c.g.z_lo = -100.0;
    c.g.z_hi = 100.0;
    let k = c.g.codes;
    let t = &c.g.tol.clone();
    let (v, ang) = (t.value, t.angle);
    for (_, sid) in edge_segs(c) {
        let s = c.seg(sid);
        if !(s.org == b'L' || s.org == b'G') || s.code != k.none {
            continue;
        }
        let kind = s.kind;
        let fa = s.fa.expect("segment without face A");
        let mut code = s.code;
        if s.org == b'G' {
            let fb = s.fb.expect("segment without face B");
            let (a, b) = (c.face(fa), c.face(fb));
            let ta = owner_typ(c, a) == b't';
            let tb = owner_typ(c, b) == b't';
            if ta && tb {
                if kind == b'K' {
                    code = k.c17;
                }
                if kind == b'T' {
                    code = k.wv;
                }
            } else if ta || tb {
                let other = if ta { b } else { a };
                if other.typ == b'K' {
                    code = k.c17;
                } else if kind == b'S' {
                    code = k.ws;
                }
            }
        } else if let Some(fb) = s.fb {
            let (a, b) = (c.face(fa), c.face(fb));
            let level = num::zero(a.h, v);
            if num::eq(b.pitch, 0.0, ang) {
                code = if level { k.t } else { k.wv };
            } else {
                let d = num::sub(s.q, s.p);
                let dir = num::norm([d[0], d[1], 0.0], t);
                let w = num::norm([b.w[0], b.w[1], 0.0], t);
                if num::ge(b.pitch, HALF_PI_F, ang) {
                    continue;
                }
                if kind == b'H' {
                    let along = num::peq(dir, w, v);
                    code = match (level, along) {
                        (true, true) => k.p,
                        (true, false) => k.t,
                        (false, true) => k.wv,
                        (false, false) => k.wh,
                    };
                } else if kind == b'S' {
                    let left = num::norm([-w[1], w[0], w[2]], t);
                    let right = num::norm([w[1], -w[0], w[2]], t);
                    code = if !level {
                        k.ws
                    } else if num::peq(left, dir, v) {
                        k.vl
                    } else if num::peq(right, dir, v) {
                        k.vr
                    } else {
                        k.os
                    };
                }
            }
        }
        c.seg_mut(sid).code = code;
    }
}

/// Whether an end of `s` meets an end of another edge's segment (POINT_EPS)
/// whose code or saved code is `code`.
fn touches(c: &Core, sid: SegId, code: i32) -> bool {
    let pt = c.g.tol.point;
    let s = c.seg(sid);
    edge_segs(c).into_iter().any(|(_, rid)| {
        if rid == sid {
            return false;
        }
        let r = c.seg(rid);
        let meet = num::peq(s.p, r.p, pt)
            || num::peq(s.p, r.q, pt)
            || num::peq(s.q, r.p, pt)
            || num::peq(s.q, r.q, pt);
        meet && (r.code == code || r.code2 == code)
    })
}

/// Whether segment `inner` lies on segment `outer`.
fn lies_on(c: &Core, outer: SegId, inner: SegId) -> bool {
    let (o, i) = (c.seg(outer), c.seg(inner));
    geom::seg_on_seg(o.p, o.q, i.p, i.q, &c.g.tol)
}

/// A 'D' edge lying on another 'D' edge (the first one in list order)
/// becomes 17, unless that one already is 17. When the other edge touches
/// an abutment (WV, WH, WS) at an end and this one does not, they swap:
/// this one takes the other's code (its saved code when it is 17) and the
/// other becomes 17 with its code saved.
pub fn merge_d17(c: &mut Core) {
    let k = c.g.codes;
    let list = edge_segs(c);
    for &(e1, s1) in &list {
        if c.seg(s1).org != b'D' {
            continue;
        }
        let Some(&(_, s2)) = list
            .iter()
            .find(|&&(e2, s2)| e2 != e1 && c.seg(s2).org == b'D' && lies_on(c, s2, s1))
        else {
            continue;
        };
        if c.seg(s2).code != k.c17 {
            c.seg_mut(s1).code = k.c17;
        }
        let abut = |c: &Core, s| [k.wv, k.wh, k.ws].iter().any(|&x| touches(c, s, x));
        if abut(c, s2) && !abut(c, s1) {
            let o = c.seg(s2).clone();
            c.seg_mut(s1).code = if o.code == k.c17 { o.code2 } else { o.code };
            let s = c.seg_mut(s2);
            s.code2 = s.code;
            s.code = k.c17;
        }
    }
}

/// An 'L' or 'G' edge lying on another 'L' or 'G' edge becomes 17.
pub fn merge_lg17(c: &mut Core) {
    let k = c.g.codes;
    let list = edge_segs(c);
    let lg = |c: &Core, s: SegId| matches!(c.seg(s).org, b'L' | b'G');
    for &(e1, s1) in &list {
        if !lg(c, s1) {
            continue;
        }
        if list
            .iter()
            .any(|&(e2, s2)| e2 != e1 && lg(c, s2) && lies_on(c, s2, s1))
        {
            c.seg_mut(s1).code = k.c17;
        }
    }
}
