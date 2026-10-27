//! S4: the per-model post-processing after the plane cut (map.md §1.2,
//! stages M, E, A) and the dormer foot pass (stage G).
//!
//! - [`collinear_merge`]: joins consecutive collinear segments of every
//!   closed face;
//! - [`origins`] and [`edge_table`]: the origin byte of every segment and
//!   the global edge (line) table, where equal segments of different faces
//!   share one line number;
//! - [`areas`]: the area of every face, from the face's own segment list;
//! - [`dormer_feet`]: the foot outline of every `g` model.

use super::error::{CoreError, Msg};
use super::faceops;
use super::geom;
use super::num::{self, V3};
use super::state::*;

/// Walls of the current ring in the order the ring-count loops visit them
/// (`counters[RING_WALLS]` steps along `next`, starting at the ring head).
fn ring_walls(c: &Core) -> Vec<WallId> {
    let mut out = Vec::new();
    let mut w = c.g.ring;
    let n = c.g.counters[ctr::RING_WALLS];
    let mut k = 0;
    while k < n {
        let Some(wi) = w else { break };
        out.push(wi);
        w = c.wall(wi).next;
        k += 1;
    }
    out
}

/// Faces of a wall along `next`, starting at its first (hidden) face.
fn wall_faces(c: &Core, w: WallId) -> Vec<FaceId> {
    let mut out = Vec::new();
    let mut f = c.wall(w).faces;
    while let Some(fi) = f {
        out.push(fi);
        f = c.face(fi).next;
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Collinear merge

/// Joins consecutive collinear segments of every closed face of the
/// current model; segments of kind 'H' (and their successors' 'H') are
/// left alone. A segment absorbs its successor (taking its end point and
/// its `next`); the last segment, when it joins the first, gives its start
/// point to the first one and is cut off the list. Returns the number of
/// joins.
///
/// The successor's `prev` is not updated when its predecessor absorbs it,
/// as in the original.
pub fn collinear_merge(c: &mut Core) -> Result<i32, CoreError> {
    let t = c.g.tol;
    let mut count = 0;
    for w in ring_walls(c) {
        for f in wall_faces(c, w) {
            let face = c.face(f);
            if face.closed == 0 {
                continue;
            }
            let head = face.segs;
            let mut cur = head;
            while let Some(s) = cur {
                let nx = c.seg(s).next;
                let o = nx.or(head).expect("face with segments");
                if c.seg(s).kind == b'H' || c.seg(o).kind == b'H' {
                    cur = nx;
                    continue;
                }
                let (a, b) = (c.seg(s), c.seg(o));
                match geom::collinear_join(a.p, a.q, b.p, b.q, &t) {
                    1 => {
                        if nx.is_none() {
                            // the last segment joins the first one
                            let p = c.seg(s).p;
                            c.seg_mut(o).p = p;
                            let prev = c.seg(s).prev.ok_or(CoreError::Fault("merge: no prev"))?;
                            c.seg_mut(prev).next = None;
                            count += 1;
                            cur = None;
                        } else {
                            let (q, onx) = (c.seg(o).q, c.seg(o).next);
                            let sm = c.seg_mut(s);
                            sm.q = q;
                            sm.next = onx;
                            count += 1;
                        }
                    }
                    _ => cur = nx,
                }
            }
        }
    }
    Ok(count)
}

// ---------------------------------------------------------------------------------------------
// Edge table

/// The origin byte of every segment of the current model, and the lists
/// of faces that are not exported cleared (`T` walls, the hidden faces of
/// `G` walls, the non-roof faces of `g`/`w` walls and, without flag 0x20,
/// their roof faces too). Origins: 'D' a roof face of a `W`/`G` wall, 'W'
/// a roof face of a `w`/`g` wall (its segments become kind 'W'), 'G' any
/// face of a `t` wall, '?' anything else; a 'D' eave ('T') on a face with
/// b == -1 is 'N'.
pub fn origins(c: &mut Core) -> Result<(), CoreError> {
    let flag20 = c.g.flags & 0x20 != 0;
    for w in ring_walls(c) {
        let wt = c.wall(w).typ;
        for f in wall_faces(c, w) {
            let ft = c.face(f).typ;
            let clear = match wt {
                b'w' | b'g' => ft != b'D' || !flag20,
                b'T' => true,
                b'G' => ft == b'W',
                _ => false,
            };
            if clear {
                let fm = c.face_mut(f);
                fm.segs = None;
                fm.holes = None;
            }
            let mut s = c.face(f).segs;
            while let Some(si) = s {
                let (fa, fb) = (c.seg(si).fa, c.seg(si).fb);
                let (Some(fa), Some(fb)) = (fa, fb) else {
                    return Err(CoreError::Fault("NULL Fehler s"));
                };
                if c.face(fa).owner.is_none() || c.face(fb).owner.is_none() {
                    return Err(CoreError::Fault("NULL Fehler d"));
                }
                s = c.seg(si).next;
            }
            let org = match wt {
                b'W' if ft != b'W' => b'D',
                b'w' if ft == b'D' => b'W',
                b'G' if ft != b'W' => b'D',
                b'g' if ft == b'D' => b'W',
                b't' => b'G',
                _ => b'?',
            };
            let mut s = c.face(f).segs;
            while let Some(si) = s {
                if (wt == b'w' || wt == b'g') && ft == b'D' {
                    c.seg_mut(si).kind = b'W';
                }
                c.seg_mut(si).org = org;
                if org == b'D' && c.seg(si).kind == b'T' {
                    let fa = c.seg(si).fa.unwrap();
                    if c.face(fa).b == -1 {
                        c.seg_mut(si).org = b'N';
                    }
                }
                s = c.seg(si).next;
            }
        }
    }
    Ok(())
}

/// Whether a face (or, when its list is empty, the face it stands for)
/// has a segment equal to `s` within POINT_EPS, in either direction.
pub fn face_has_seg(c: &Core, f: FaceId, s: SegId) -> bool {
    let t = c.g.tol.point;
    let mut f = f;
    if c.face(f).segs.is_none()
        && let Some(a) = c.face(f).alias
    {
        f = a;
    }
    let (p, q) = (c.seg(s).p, c.seg(s).q);
    let mut x = c.face(f).segs;
    while let Some(xi) = x {
        let o = c.seg(xi);
        if num::peq(p, o.p, t) && num::peq(q, o.q, t) {
            return true;
        }
        if num::peq(p, o.q, t) && num::peq(q, o.p, t) {
            return true;
        }
        x = o.next;
    }
    false
}

/// Whether the edge's representative equals `s` (POINT_EPS, either
/// direction), in the original's test order.
fn same_line(c: &Core, rep: SegId, s: SegId) -> bool {
    let t = c.g.tol.point;
    let (r, x) = (c.seg(rep), c.seg(s));
    (num::peq(r.p, x.p, t) && num::peq(r.q, x.q, t))
        || (num::peq(r.p, x.q, t) && num::peq(r.q, x.p, t))
}

/// A new edge for `s` with the next line number, prepended to the list.
fn new_edge(c: &mut Core, s: SegId, xc: i32) -> EdgeId {
    c.g.n_lines += 1;
    let no = c.g.n_lines;
    let (p, q) = (c.seg(s).p, c.seg(s).q);
    let sm = c.seg_mut(s);
    sm.line = no;
    sm.len = num::len(num::sub(p, q)) as f32;
    let id = EdgeId(c.edges.len() as u32);
    c.edges.push(Edge {
        no,
        x04: 0,
        cnt: 1,
        xc,
        x10: 0,
        x14: 0,
        x18: 0,
        seg: Some(s),
        next: c.g.edges,
    });
    c.g.edges = Some(id);
    id
}

/// Mask of four face tests: bit 0x1000 face A of `s`, 0x100 face B of `s`,
/// 0x10 and 1 faces A and B of the representative.
fn mask4(a: bool, b: bool, c: bool, d: bool) -> u32 {
    (if a { 0x1000 } else { 0 })
        | (if b { 0x100 } else { 0 })
        | (if c { 0x10 } else { 0 })
        | (if d { 1 } else { 0 })
}

/// A roof segment that equals the representative `rep` of edge `e`.
/// `cached` holds what the original read from `s` and `rep` before the
/// comparison: faces of `s` (A, B), faces of `rep` (A, B), kinds and
/// origins.
#[allow(clippy::too_many_arguments)]
fn join_edge(c: &mut Core, e: EdgeId, s: SegId, cs: &SegCache, cr: &SegCache) {
    let t = c.g.tol.point;
    let rep = c.edges[e.ix()].seg.unwrap();
    let dir = if num::peq(c.seg(s).p, c.seg(rep).p, t) {
        1
    } else {
        -1
    };
    let no = c.edges[e.ix()].no;
    if cs.org == b'W' || cs.org == b'N' {
        c.seg_mut(s).line = no * dir;
        c.edges[e.ix()].cnt += 1;
        return;
    }
    // A 'D' segment replaces an 'N', 'G' or 'W' representative, a 'G' one a
    // 'W' representative: the representative takes the segment's faces,
    // kind, origin and (oriented) end points.
    let take = cr.org == b'N'
        || (cs.org == b'D' && (cr.org == b'G' || cr.org == b'W'))
        || (cs.org == b'G' && cr.org == b'W');
    if take {
        let x = c.seg(s).clone();
        let r = c.seg_mut(rep);
        r.fa = cs.fa;
        r.fb = cs.fb;
        r.kind = cs.kind;
        r.org = cs.org;
        if dir == 1 {
            r.p = x.p;
            r.q = x.q;
        } else {
            r.p = x.q;
            r.q = x.p;
        }
    }
    // Two eaves on hidden wall faces: exchange the non-wall faces and turn
    // both into 'H'.
    let typ = |f: Option<FaceId>| c.face(f.unwrap()).typ;
    let m = mask4(
        typ(cs.fa) != b'W',
        typ(cs.fb) != b'W',
        typ(cr.fa) != b'W',
        typ(cr.fb) != b'W',
    );
    if cs.kind == b'T' && cr.kind == b'T' {
        let (sa, sb) = (c.seg(s).fa, c.seg(s).fb);
        let (ra, rb) = (c.seg(rep).fa, c.seg(rep).fb);
        let done = match m {
            0x1010 if sa != ra => {
                c.seg_mut(s).fb = ra;
                c.seg_mut(rep).fb = sa;
                true
            }
            0x1001 if sa != rb => {
                c.seg_mut(s).fb = rb;
                c.seg_mut(rep).fa = sa;
                true
            }
            0x101 if sb != rb => {
                c.seg_mut(s).fa = rb;
                c.seg_mut(rep).fa = sb;
                true
            }
            0x110 if sb != ra => {
                c.seg_mut(s).fa = ra;
                c.seg_mut(rep).fb = sb;
                true
            }
            _ => false,
        };
        if done {
            c.seg_mut(s).kind = b'H';
            c.seg_mut(rep).kind = b'H';
        }
    }
    // The same exchange by the b flags of the cached faces; it clears bit
    // 0x40 of the representative's flags word.
    let bf = |f: Option<FaceId>| c.face(f.unwrap()).b != 0;
    let m = mask4(bf(cs.fa), bf(cs.fb), bf(cr.fa), bf(cr.fb));
    let (sa, sb) = (c.seg(s).fa, c.seg(s).fb);
    let (ra, rb) = (c.seg(rep).fa, c.seg(rep).fb);
    let done = match m {
        0x1001 if sa != rb => {
            c.seg_mut(s).fb = rb;
            c.seg_mut(rep).fa = sa;
            true
        }
        0x101 if sb != rb => {
            c.seg_mut(s).fa = rb;
            c.seg_mut(rep).fa = sb;
            true
        }
        0x110 if sb != ra => {
            c.seg_mut(s).fa = ra;
            c.seg_mut(rep).fb = sb;
            true
        }
        0x1010 if sa != ra => {
            c.seg_mut(s).fb = ra;
            c.seg_mut(rep).fb = sa;
            true
        }
        _ => false,
    };
    if done {
        c.seg_mut(rep).fl &= !0x40;
    }
    c.seg_mut(s).line = no * dir;
    c.edges[e.ix()].cnt += 1;
    let (fa, fb) = (c.seg(s).fa.unwrap(), c.seg(s).fb.unwrap());
    if c.face(fa).b != 0 || c.face(fb).b != 0 {
        if c.face(fa).b != 0 && face_has_seg(c, fa, s) {
            c.edges[e.ix()].xc += 1;
        }
        if c.face(fb).b != 0 && face_has_seg(c, fb, s) {
            c.edges[e.ix()].xc += 1;
        }
    }
}

/// What the edge table reads from a segment before comparing it.
struct SegCache {
    fa: Option<FaceId>,
    fb: Option<FaceId>,
    kind: u8,
    org: u8,
}

impl SegCache {
    fn of(c: &Core, s: SegId) -> SegCache {
        let x = c.seg(s);
        SegCache {
            fa: x.fa,
            fb: x.fb,
            kind: x.kind,
            org: x.org,
        }
    }
}

/// Whether a face with b != 0 has the segment: the face itself when it
/// stands for no other face, else the chain of faces it stands for.
fn flagged_face_has(c: &Core, f: FaceId, s: SegId) -> bool {
    match c.face(f).alias {
        None => face_has_seg(c, f, s),
        Some(mut a) => loop {
            if face_has_seg(c, a, s) {
                return true;
            }
            match c.face(a).alias {
                Some(n) => a = n,
                None => return false,
            }
        },
    }
}

/// The edge table of the current model: [`origins`], then every segment of
/// every face either joins the first edge whose representative equals it
/// (getting `±line`, the sign telling the direction) or opens a new edge.
/// Hole pieces follow each face: their degenerate segments are unlinked;
/// a piece with three or more segments gets kinds 'H'/'S' and joins 'L'
/// and 'G' edges only. At the end every representative's codes are reset.
pub fn edge_table(c: &mut Core) -> Result<(), CoreError> {
    origins(c)?;
    let pt = c.g.tol.point;
    for w in ring_walls(c) {
        for f in wall_faces(c, w) {
            let mut s = c.face(f).segs;
            while let Some(si) = s {
                let cs = SegCache::of(c, si);
                let mut e = c.g.edges;
                let mut joined = false;
                while let Some(ei) = e {
                    let rep = c.edges[ei.ix()].seg.unwrap();
                    let cr = SegCache::of(c, rep);
                    if same_line(c, rep, si) {
                        join_edge(c, ei, si, &cs, &cr);
                        joined = true;
                        break;
                    }
                    e = c.edges[ei.ix()].next;
                }
                if !joined {
                    let fa = c.seg(si).fa.unwrap();
                    let fb = c.seg(si).fb.unwrap();
                    let mut xc = 0;
                    if c.face(fa).b != 0 || c.face(fb).b != 0 {
                        if c.face(fa).b != 0 && flagged_face_has(c, fa, si) {
                            xc = 1;
                        }
                        if c.face(fb).b != 0 && flagged_face_has(c, fb, si) {
                            xc = 1;
                        }
                    }
                    new_edge(c, si, xc);
                }
                s = c.seg(si).next;
            }
            // hole pieces
            let mut h = c.face(f).holes;
            while let Some(hi) = h {
                let mut n = 0u32;
                let mut s = c.node(hi).segs;
                while let Some(si) = s {
                    c.seg_mut(si).org = b'L';
                    let x = c.seg(si);
                    let (prev, next) = (x.prev, x.next);
                    if num::peq(x.p, x.q, pt) {
                        if c.node(hi).segs == Some(si) {
                            c.node_mut(hi).segs = next;
                        }
                        if let Some(pv) = prev {
                            c.seg_mut(pv).next = next;
                        }
                        if let Some(nx) = next {
                            c.seg_mut(nx).prev = prev;
                        }
                    } else {
                        n += 1;
                    }
                    s = next;
                }
                if n >= 3 {
                    let mut s = c.node(hi).segs;
                    while let Some(si) = s {
                        hole_seg(c, si);
                        s = c.seg(si).next;
                    }
                }
                h = c.node(hi).next;
            }
        }
    }
    let none = c.g.codes.none;
    let mut e = c.g.edges;
    while let Some(ei) = e {
        let rep = c.edges[ei.ix()].seg.unwrap();
        let r = c.seg_mut(rep);
        r.code = none;
        r.code2 = none;
        e = c.edges[ei.ix()].next;
    }
    Ok(())
}

/// One segment of a hole piece in the edge table.
fn hole_seg(c: &mut Core, s: SegId) {
    let t = c.g.tol;
    let (p, q) = (c.seg(s).p, c.seg(s).q);
    let horiz = geom::horizontal(
        &geom::Line {
            p,
            d: num::sub(p, q),
        },
        &t,
    );
    c.seg_mut(s).kind = if horiz { b'H' } else { b'S' };
    let mut e = c.g.edges;
    while let Some(ei) = e {
        let rep = c.edges[ei.ix()].seg.unwrap();
        if same_line(c, rep, s) {
            let dir = if num::peq(p, c.seg(rep).p, t.point) {
                1
            } else {
                -1
            };
            let org = c.seg(rep).org;
            if org == b'L' || org == b'G' {
                let no = c.edges[ei.ix()].no;
                c.seg_mut(s).line = no * dir;
                c.edges[ei.ix()].cnt += 1;
                let (fa, fb) = (c.seg(s).fa.unwrap(), c.seg(s).fb.unwrap());
                if c.face(fa).b != 0 || c.face(fb).b != 0 {
                    c.edges[ei.ix()].xc += 1;
                }
                return;
            }
        }
        e = c.edges[ei.ix()].next;
    }
    let (fa, fb) = (c.seg(s).fa.unwrap(), c.seg(s).fb.unwrap());
    let xc = i32::from(c.face(fa).b != 0 || c.face(fb).b != 0);
    new_edge(c, s, xc);
}

// ---------------------------------------------------------------------------------------------
// Areas

/// The message of the area routine for a face whose normal is
/// perpendicular to all three axes.
pub const MSG_AREA_AXES: Msg = Msg {
    id: Some(0xa12),
    text: "Flächenberechnung",
};

/// `acos(dot(n, axis) / |n|)` with the dot product stored as a float (the
/// angle between a face normal and a unit axis), as a float.
fn axis_angle(n: V3, axis: V3) -> f32 {
    let d = num::dot(n, axis) as f32;
    let q = f64::from(d) / num::len(n);
    super::x87::crt_acos(q).to_f32()
}

/// The area of a face from its own segment list: the shoelace sum in the
/// projection onto the coordinate plane closest to the face (the smallest
/// of the angles to the y axis, the x axis and the pitch, each folded to
/// at most 90 degrees), divided by the cosine of that angle.
pub fn face_area(c: &Core, f: FaceId, segs: Option<SegId>) -> Result<f32, CoreError> {
    let t = c.g.tol;
    let face = c.face(f);
    let mut ay = axis_angle(face.n, [0.0, 1.0, 0.0]);
    let mut ax = axis_angle(face.n, [1.0, 0.0, 0.0]);
    let mut ap = face.pitch;
    let hp = num::HALF_PI_F;
    if num::eq(ay, hp, t.angle) && num::eq(ax, hp, t.angle) && num::eq(ap, hp, t.angle) {
        return Err(CoreError::fatal(2, MSG_AREA_AXES));
    }
    let pi = f64::from(std::f32::consts::PI);
    if num::gt(ay, hp, t.value) {
        ay = (pi - f64::from(ay)) as f32;
    }
    if num::gt(ax, hp, t.value) {
        ax = (pi - f64::from(ax)) as f32;
    }
    if num::gt(ap, hp, t.value) {
        ap = (pi - f64::from(ap)) as f32;
    }
    // `a <= b` as the original's fcomp/test 0x41 (true when unordered).
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    let le = |a: f32, b: f32| !(a > b);
    let (mode, ang) = if le(ap, ay) && le(ap, ax) {
        (2, ap)
    } else if le(ax, ay) && le(ax, ap) {
        (1, ax)
    } else if le(ay, ax) && le(ay, ap) {
        (0, ay)
    } else {
        return Ok(0.0);
    };
    let cos = super::x87::fcos(f64::from(ang));
    let mut acc = 0.0f64;
    let mut s = segs;
    while let Some(si) = s {
        let x = c.seg(si);
        let (p, q) = (x.p.map(f64::from), x.q.map(f64::from));
        let (a, b) = match mode {
            0 => (q[2] * p[0], q[0] * p[2]),
            1 => (q[2] * p[1], q[1] * p[2]),
            _ => (q[1] * p[0], q[0] * p[1]),
        };
        acc += a - b;
        s = x.next;
    }
    Ok(cos.fdivr(acc * 0.5).abs() as f32)
}

/// The area of every face of the current model except the first (hidden)
/// face of each wall, into the face's head node. Walks the ring once
/// around (not by the ring count).
pub fn areas(c: &mut Core) -> Result<(), CoreError> {
    let Some(head) = c.g.ring else {
        return Err(CoreError::Fault("areas: no ring"));
    };
    let mut w = head;
    loop {
        let first = c.wall(w).faces.ok_or(CoreError::Fault("areas: no face"))?;
        let mut f = c.face(first).next;
        while let Some(fi) = f {
            let a = face_area(c, fi, c.face(fi).segs)?;
            let h = c.face(fi).head.ok_or(CoreError::Fault("areas: no head"))?;
            c.node_mut(h).area = a;
            f = c.face(fi).next;
        }
        w = c.wall(w).next.ok_or(CoreError::Fault("areas: open ring"))?;
        if w == head {
            break;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Dormer feet

/// The foot outline of every `g` model: every segment of a roof face of a
/// `t` wall that no other roof face of the same wall shares (VALUE_EPS,
/// either direction) is added at z = 0 as a 'G' segment to the model's
/// ground face, which is then closed. The model's counters are loaded and
/// stay loaded.
pub fn dormer_feet(c: &mut Core) -> Result<(), CoreError> {
    let vt = c.g.tol.value;
    let mut m = c.g.models;
    while let Some(mi) = m {
        if c.model(mi).typ == b'g' {
            c.g.counters = c.model(mi).counters;
            let mut w = c.model(mi).ring;
            let mut k = 0;
            while k < c.g.counters[ctr::RING_WALLS] {
                let Some(wi) = w else {
                    return Err(CoreError::Fault("dormer: open ring"));
                };
                if !matches!(c.wall(wi).typ, b'G' | b'T' | b'W' | b'g' | b'w') {
                    let faces = wall_faces(c, wi);
                    for &f in &faces {
                        if c.face(f).typ == b'W' {
                            continue;
                        }
                        let mut s = c.face(f).segs;
                        while let Some(si) = s {
                            let (p, q) = (c.seg(si).p, c.seg(si).q);
                            let shared = faces.iter().any(|&f2| {
                                c.face(f2).typ != b'W' && f2 != f && {
                                    let mut x = c.face(f2).segs;
                                    let mut hit = false;
                                    while let Some(xi) = x {
                                        let o = c.seg(xi);
                                        if (num::peq(p, o.p, vt) && num::peq(q, o.q, vt))
                                            || (num::peq(p, o.q, vt) && num::peq(q, o.p, vt))
                                        {
                                            hit = true;
                                            break;
                                        }
                                        x = o.next;
                                    }
                                    hit
                                }
                            });
                            if !shared {
                                let gf = c.model(mi).gface;
                                faceops::add_seg(
                                    c,
                                    true,
                                    gf,
                                    b'G',
                                    [p[0], p[1], 0.0],
                                    [q[0], q[1], 0.0],
                                    Some(f),
                                    Some(f),
                                )?;
                            }
                            s = c.seg(si).next;
                        }
                    }
                }
                w = c.wall(wi).next;
                k += 1;
            }
            let gf = c.model(mi).gface;
            faceops::close_face(c, gf, 1)?;
        }
        m = c.model(mi).next;
    }
    Ok(())
}
