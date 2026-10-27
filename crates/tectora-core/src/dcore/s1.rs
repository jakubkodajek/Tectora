//! S1 after reading: the wall ring sort and collinear merge, the hole
//! relink, the face frames and planes, the embedded eave faces, the
//! validity ranges, the eave heights and the level list (the first part of
//! the per-model core routine, up to its first height slab).

use super::error::*;
use super::num::{self, HALF_PI_F, V3};
use super::state::*;
use super::x87;

fn fault(what: &'static str) -> CoreError {
    CoreError::Fault(what)
}

impl Core {
    fn wnext(&self, w: WallId) -> Result<WallId, CoreError> {
        self.wall(w).next.ok_or(fault("wall ring broken"))
    }
    fn wprev(&self, w: WallId) -> Result<WallId, CoreError> {
        self.wall(w).prev.ok_or(fault("wall ring broken"))
    }
    fn set_next(&mut self, w: WallId, n: WallId) {
        self.wall_mut(w).next = Some(n);
    }
    fn set_prev(&mut self, w: WallId, p: WallId) {
        self.wall_mut(w).prev = Some(p);
    }
}

/// The wall chain sort and the collinear merge of the current model's ring.
///
/// Walks the ring backwards from the second wall, moving the wall that
/// ends where the current one starts in front of it (a zero-length wall at
/// that point first). A leading dormer wall ('T'/'t') is taken out first
/// and put back after a forward pass; without one, collinear neighbours
/// with equal roof data are merged. A broken chain is a "no plan" error.
pub fn ring_sort(c: &mut Core) -> Result<Option<WallId>, CoreError> {
    let tol = c.g.tol;
    let pe = tol.polyg;
    let ring = c.g.ring.ok_or(fault("no ring"))?;
    let mut saved_t: Option<WallId> = None;
    if matches!(c.wall(ring).typ, b'T' | b't') {
        c.g.counters[ctr::RING_WALLS] -= 1;
        saved_t = Some(ring);
        let (p, n) = (c.wprev(ring)?, c.wnext(ring)?);
        c.set_next(p, n);
        c.set_prev(n, p);
    }
    let n9 = |c: &Core| c.g.counters[ctr::RING_WALLS];
    let mut cur = c.wnext(c.g.ring.unwrap())?;
    let mut forward = false;
    let mut i5 = 0;
    if 0 < n9(c) {
        'back: loop {
            let mut cand = c.wprev(cur)?;
            c.g.chain_wall = Some(cur);
            c.wall_mut(cur).visited = 1;
            let cs = c.wall(cur).s;
            let mut found = None;
            let mut j = 0;
            while j < n9(c) - 1 {
                let w = c.wall(cand);
                if (w.visited == 0 || j == 0 || i5 == n9(c) - 1)
                    && num::eq(cs[0], w.e[0], pe)
                    && num::eq(cs[1], w.e[1], pe)
                    && num::eq(w.s[0], w.e[0], pe)
                    && num::eq(w.s[1], w.e[1], pe)
                {
                    found = Some(cand);
                    break;
                }
                cand = c.wprev(cand)?;
                j += 1;
            }
            if found.is_none() {
                let mut cand = c.wprev(cur)?;
                let mut j = 0;
                loop {
                    if n9(c) - 1 <= j {
                        if saved_t.is_some() {
                            forward = true;
                            break 'back;
                        }
                        return Err(CoreError::NoPlan { msg: MSG_NO_PLAN });
                    }
                    let w = c.wall(cand);
                    if (w.visited == 0 || j == 0 || i5 == n9(c) - 1)
                        && num::eq(cs[0], w.e[0], pe)
                        && num::eq(cs[1], w.e[1], pe)
                        && (num::ne(w.s[0], w.e[0], pe) || num::ne(w.s[1], w.e[1], pe))
                    {
                        found = Some(cand);
                        break;
                    }
                    cand = c.wprev(cand)?;
                    j += 1;
                }
            }
            let cand = found.unwrap();
            let cp = c.wprev(cur)?;
            if cp != cand {
                let (ap, an) = (c.wprev(cand)?, c.wnext(cand)?);
                c.set_prev(an, ap);
                c.set_next(ap, an);
                let cp = c.wprev(cur)?;
                c.set_next(cp, cand);
                c.set_prev(cand, cp);
                c.set_prev(cur, cand);
                c.set_next(cand, cur);
            }
            cur = c.wprev(cur)?;
            i5 += 1;
            if n9(c) <= i5 {
                break;
            }
        }
    }
    if saved_t.is_none() && !forward {
        merge(c)?;
        return Ok(c.g.ring);
    }
    // Forward pass (only with a dormer wall taken out).
    let t = saved_t.unwrap();
    let mut i3 = 0;
    while i3 < n9(c) - 1 {
        c.g.chain_wall = Some(cur);
        let ce = c.wall(cur).e;
        let mut cand = c.wnext(cur)?;
        let mut found = None;
        let mut j = 0;
        while j < n9(c) - 1 {
            let w = c.wall(cand);
            if num::eq(ce[0], w.s[0], pe)
                && num::eq(ce[1], w.s[1], pe)
                && num::eq(w.s[0], w.e[0], pe)
                && num::eq(w.s[1], w.e[1], pe)
            {
                found = Some(cand);
                break;
            }
            cand = c.wnext(cand)?;
            j += 1;
        }
        if found.is_none() {
            let mut cand = c.wnext(cur)?;
            let mut j = 0;
            while j < n9(c) - 1 {
                let w = c.wall(cand);
                if num::eq(ce[0], w.s[0], pe)
                    && num::eq(ce[1], w.s[1], pe)
                    && (num::ne(w.s[0], w.e[0], pe) || num::ne(w.s[1], w.e[1], pe))
                {
                    found = Some(cand);
                    break;
                }
                cand = c.wnext(cand)?;
                j += 1;
            }
        }
        let Some(cand) = found else {
            return Err(CoreError::NoPlan { msg: MSG_NO_PLAN });
        };
        let cn = c.wnext(cur)?;
        if cn != cand {
            let (ap, an) = (c.wprev(cand)?, c.wnext(cand)?);
            c.set_next(ap, an);
            c.set_prev(an, ap);
            let cn = c.wnext(cur)?;
            c.set_prev(cn, cand);
            c.set_next(cand, cn);
            c.set_next(cur, cand);
            c.set_prev(cand, cur);
        }
        cur = c.wnext(cur)?;
        i3 += 1;
    }
    let n = c.wnext(cur)?;
    c.set_next(t, n);
    c.set_prev(t, cur);
    c.set_prev(n, t);
    c.set_next(cur, t);
    c.g.counters[ctr::RING_WALLS] += 1;
    Ok(c.g.ring)
}

/// The collinear merge: a wall absorbs its successor when both are
/// non-zero, collinear and their first roof faces agree in height,
/// overhang and flag.
fn merge(c: &mut Core) -> Result<(), CoreError> {
    let tol = c.g.tol;
    let mut i: i32 = 0;
    let mut p = c.g.ring.ok_or(fault("no ring"))?;
    if c.g.counters[ctr::RING_WALLS] <= 0 {
        return Ok(());
    }
    loop {
        let q = c.wnext(p)?;
        let mut base = p;
        if c.wall(p).zero == 0 && c.wall(q).zero == 0 {
            let first = |w: WallId| -> Result<FaceId, CoreError> {
                let h = c.wall(w).faces.ok_or(fault("wall without faces"))?;
                c.face(h).next.ok_or(fault("wall without roof face"))
            };
            let (fp, fq) = (first(p)?, first(q)?);
            let (wp, wq) = (c.wall(p), c.wall(q));
            let d1 = num::sub(wp.e, wp.s);
            let d2 = num::sub(wq.e, wp.s);
            let d3 = num::sub(wq.e, wq.s);
            let (a, b) = (c.face(fp), c.face(fq));
            if num::par(d1, d2, &tol) != 0
                && num::par(d1, d3, &tol) != 0
                && num::eq(a.h, b.h, tol.value)
                && num::eq(a.ov, b.ov, tol.value)
                && a.b == b.b
            {
                let qn = c.wnext(q)?;
                c.set_next(p, qn);
                c.set_prev(qn, p);
                c.g.counters[ctr::RING_WALLS] -= 1;
                i -= 1;
                if let Some(m) = c.g.cur_model {
                    c.models[m.ix()].counters = c.g.counters;
                }
                if Some(q) == c.g.ring {
                    if let Some(m) = c.g.cur_model {
                        c.models[m.ix()].ring = Some(p);
                    }
                    c.g.ring = Some(p);
                }
                let (qe, qeo) = (c.wall(q).e, c.wall(q).e_orig);
                c.wall_mut(p).e = qe;
                base = c.wprev(p)?;
                c.wall_mut(p).e_orig = qeo;
                if Some(q) == c.g.ring {
                    c.g.ring = Some(base);
                }
            }
        }
        p = c.wnext(base)?;
        i += 1;
        if i >= c.g.counters[ctr::RING_WALLS] {
            return Ok(());
        }
    }
}

/// Relinks every hole outline in reverse order with swapped ends (the
/// core does this at the start of every model; the guard on an
/// uninitialised local that would skip it is taken as never set).
pub fn relink_holes(c: &mut Core) {
    let mut h = c.g.holes;
    while let Some(hid) = h {
        c.g.hole_cur = Some(hid);
        let head = c.node(hid).segs;
        // Step 1: every segment's prev becomes its old next, next becomes null.
        let mut s2 = head;
        let mut s = head;
        while let Some(si) = s {
            s = c.seg(si).next;
            let cur = s2.unwrap();
            c.seg_mut(cur).prev = s;
            c.seg_mut(cur).next = None;
            s2 = s;
        }
        // Step 2: walk the new prev links, swapping ends and linking next.
        let mut s = c.node(hid).segs;
        while let Some(si) = s {
            let seg = c.seg_mut(si);
            std::mem::swap(&mut seg.p, &mut seg.q);
            if let Some(pv) = seg.prev {
                c.seg_mut(pv).next = Some(si);
                c.node_mut(hid).segs = Some(pv);
            }
            s = c.seg(si).prev;
        }
        c.g.seg_cur = None;
        c.g.seg_cur2 = None;
        h = c.node(hid).next;
    }
    c.g.hole_cur = None;
}

/// The frame of a roof face from its predecessor (the face below it):
/// origin on the eave line moved out by the overhang (pitch break faces:
/// moved up the predecessor's slope to the break height), `w` along the
/// wall, `u` up the slope with the rise of the pitch.
pub fn face_frame(c: &mut Core, f: FaceId) -> Result<(), CoreError> {
    let tol = c.g.tol;
    let pid = c.face(f).prev.ok_or(fault("roof face without predecessor"))?;
    let pf = c.face(pid).clone();
    let fd = f64::from;
    let mut x0 = pf.o[0];
    let mut y0 = pf.o[1];
    let mut x1 = (fd(x0) + fd(pf.w[0])) as f32;
    let mut y1 = (fd(y0) + fd(pf.w[1])) as f32;
    let me = c.face(f).clone();
    let mut o = me.o;
    if me.typ == b'K' {
        if num::ne(pf.pitch, HALF_PI_F, tol.angle) {
            let q = x87::fptan(fd(pf.pitch)).fdivr(fd(me.h) - fd(pf.h));
            let t = q / num::hypot(fd(pf.w[1]), fd(pf.w[0]));
            o[0] = (fd(x0) - t * fd(pf.w[1])) as f32;
            x0 = o[0];
            o[1] = (t * fd(pf.w[0]) + fd(y0)) as f32;
            y0 = o[1];
            x1 = (fd(x0) + fd(pf.w[0])) as f32;
            y1 = (fd(y0) + fd(pf.w[1])) as f32;
        } else {
            o = pf.o;
        }
    } else {
        let d = fd(y1) - fd(y0);
        let k = fd(me.ov) / num::hypot(fd(x1) - fd(x0), d);
        o[0] = (fd(d as f32) * k + fd(x0)) as f32;
        o[1] = ((fd(x0) - fd(x1)) * k + fd(y0)) as f32;
    }
    o[2] = me.h;
    let fm = c.face_mut(f);
    fm.o = o;
    fm.w = pf.w;
    if num::eq(fm.pitch, HALF_PI_F, tol.angle) {
        fm.u = [0.0, 0.0, 1.0];
        return Ok(());
    }
    let fm = c.face(f).clone();
    let mut u: V3;
    if num::zero(fm.ov, tol.value) {
        let dx = (fd(x1) - fd(x0)) as f32;
        let k = 1.0 / num::hypot(fd(dx), fd(y1) - fd(y0));
        u = [((fd(y0) - fd(y1)) * k) as f32, (fd(dx) * k) as f32, 0.0];
        u[2] = num::rise(fm.pitch, 1.0, &tol) as f32;
    } else {
        u = [
            (fd(x0) - fd(fm.o[0])) as f32,
            (fd(y0) - fd(fm.o[1])) as f32,
            0.0,
        ];
        u[2] = num::rise(fm.pitch, fm.ov, &tol) as f32;
    }
    c.face_mut(f).u = u;
    Ok(())
}

/// Normalises `u` and `w` and sets the plane `n = u x w`, `d = n . o`.
pub fn face_plane(c: &mut Core, f: FaceId) {
    let tol = c.g.tol;
    let fc = c.face_mut(f);
    fc.u = num::norm(fc.u, &tol);
    fc.w = num::norm(fc.w, &tol);
    let fd = f64::from;
    let (u, w, o) = (fc.u, fc.w, fc.o);
    let nx = fd(u[2]) * fd(w[1]) - fd(u[1]) * fd(w[2]);
    let ny = fd(w[2]) * fd(u[0]) - fd(u[2]) * fd(w[0]);
    let nz = fd(u[1]) * fd(w[0]) - fd(w[1]) * fd(u[0]);
    fc.n = [nx as f32, ny as f32, nz as f32];
    fc.d = ((nz * fd(o[2]) + ny * fd(o[1])) + nx * fd(o[0])) as f32;
}

/// The hidden wall face: vertical, its height the top of the first roof
/// face's overhang.
fn hidden_height(c: &mut Core, h: FaceId) -> Result<(), CoreError> {
    let tol = c.g.tol;
    let n = c.face(h).next.ok_or(fault("wall without roof face"))?;
    let nf = c.face(n).clone();
    let r = num::rise(nf.pitch, nf.ov, &tol);
    let hf = c.face_mut(h);
    hf.u[2] = 1.0;
    hf.h = (r + f64::from(nf.h)) as f32;
    Ok(())
}

/// Frames, planes and eave faces of every wall; sets flag 0x40 when the
/// model has exactly one sloped (or flagged) face.
pub fn frames(c: &mut Core) -> Result<(), CoreError> {
    let tol = c.g.tol;
    c.g.flags &= !0x40;
    let mut count = 0;
    let ring = c.g.ring.ok_or(fault("no ring"))?;
    c.g.cur_wall = Some(ring);
    for _ in 0..c.g.counters[ctr::RING_WALLS].max(0) {
        let w = c.g.cur_wall.unwrap();
        let hid = c.wall(w).faces.ok_or(fault("wall without faces"))?;
        c.g.cur_face = c.face(hid).next;
        while let Some(f) = c.g.cur_face {
            let fc = c.face(f);
            if num::ne(fc.pitch, HALF_PI_F, tol.angle) || fc.b != 0 {
                count += 1;
            }
            face_frame(c, f)?;
            face_plane(c, f);
            c.g.cur_face = c.face(f).next;
        }
        hidden_height(c, hid)?;
        face_plane(c, hid);
        // The eave face: a copy of the first roof face, made vertical and
        // ranging from 0 to the face's height.
        let first = c.face(hid).next.ok_or(fault("wall without roof face"))?;
        let mut e = c.face(first).clone();
        let hf = c.face(hid).clone();
        e.sub = i32::MIN;
        e.typ = b'D';
        e.pitch = HALF_PI_F;
        e.u = [0.0, 0.0, 1.0];
        e.b = 0;
        e.zlo = 0.0;
        e.zhi = e.h;
        e.h = 0.0;
        e.closed = 1;
        e.owner = Some(w);
        e.prev = hf.prev;
        e.next = hf.next;
        let eid = c.wall(w).eave;
        *c.face_mut(eid) = e;
        face_plane(c, eid);
        c.g.cur_wall = c.wall(w).next;
    }
    if count == 1 {
        c.g.flags |= 0x40;
    }
    Ok(())
}

/// Validity range of every face: from its own height to the next face's
/// (a hidden wall face up to the top of the first roof face's overhang).
pub fn ranges(c: &mut Core) -> Result<(), CoreError> {
    let tol = c.g.tol;
    let mut w = c.g.ring.ok_or(fault("no ring"))?;
    for _ in 0..c.g.counters[ctr::RING_WALLS].max(0) {
        c.g.cur_face = c.wall(w).faces;
        while let Some(f) = c.g.cur_face {
            let fc = c.face(f).clone();
            let mut zhi;
            match fc.next {
                None => zhi = 100.0,
                Some(n) => {
                    let nf = c.face(n);
                    zhi = nf.o[2];
                    if fc.typ == b'W' {
                        zhi = if !num::ne(nf.pitch, HALF_PI_F, tol.angle) {
                            100.0
                        } else {
                            (f64::from(nf.h) + f64::from(zhi)) as f32
                        };
                    }
                }
            }
            let fm = c.face_mut(f);
            fm.zlo = fc.o[2];
            fm.zhi = zhi;
            c.g.cur_face = fc.next;
        }
        w = c.wnext(w)?;
    }
    Ok(())
}

/// The line-plane intersection: the plane of `f` (origin, `u`, `w`) with
/// the line `p + t d`. `None` (and the sentinel point) when parallel.
pub fn line_plane(c: &Core, f: FaceId, p: V3, d: V3) -> Option<V3> {
    let fc = c.face(f);
    let a = [
        fc.u[0], fc.w[0], -d[0], fc.u[1], fc.w[1], -d[1], fc.u[2], fc.w[2], -d[2],
    ];
    let b = [
        (f64::from(p[0]) - f64::from(fc.o[0])) as f32,
        (f64::from(p[1]) - f64::from(fc.o[1])) as f32,
        (f64::from(p[2]) - f64::from(fc.o[2])) as f32,
    ];
    let r = num::solve3(&a, b, &c.g.tol)?;
    let t = f64::from(r[2]);
    Some([
        (t * f64::from(d[0]) + f64::from(p[0])) as f32,
        (t * f64::from(d[1]) + f64::from(p[1])) as f32,
        (t * f64::from(d[2]) + f64::from(p[2])) as f32,
    ])
}

/// Eave heights: a wall's height is where its first roof face (in front
/// of the wall line) meets the wall's vertical line, 100 when none.
pub fn eave_heights(c: &mut Core) -> Result<(), CoreError> {
    let tol = c.g.tol;
    let ring = c.g.ring.ok_or(fault("no ring"))?;
    let mut w = ring;
    loop {
        let hid = c.wall(w).faces.ok_or(fault("wall without faces"))?;
        let hf = c.face(hid).clone();
        let (lp, ld) = (hf.o, hf.u);
        c.wall_mut(w).h = 100.0;
        c.face_mut(hid).h = 100.0;
        c.face_mut(hid).zhi = 100.0;
        let mut f = Some(hid);
        while let Some(fi) = f {
            if fi != hid {
                let o = c.face(hid).o;
                let q = num::add(o, c.face(hid).w);
                let fc = c.face(fi).clone();
                let r = num::orient(o, q, fc.o, &tol);
                if r == -1 {
                    break;
                }
                if r == 0 {
                    if num::ne(fc.pitch, HALF_PI_F, tol.angle) {
                        c.face_mut(hid).h = fc.h;
                        c.face_mut(hid).zhi = fc.h;
                        c.wall_mut(w).h = fc.h;
                        break;
                    }
                } else if num::lt(fc.pitch, HALF_PI_F, tol.angle) {
                    let Some(x) = line_plane(c, fi, lp, ld) else {
                        return Err(CoreError::fatal(2, MSG_EAVE_CUT));
                    };
                    c.face_mut(hid).h = x[2];
                    c.face_mut(hid).zhi = x[2];
                    c.wall_mut(w).h = x[2];
                }
            }
            f = c.face(fi).next;
        }
        w = c.wnext(w)?;
        if w == ring {
            return Ok(());
        }
    }
}

/// Inserts a level boundary `h` (kept sorted, no duplicates within VALUE_EPS).
fn level_insert(c: &mut Core, h: f32) {
    let tol = c.g.tol;
    let mut n = c.g.levels;
    while let Some(i) = n {
        if num::eq(h, c.levels[i.ix()].hi, tol.value) {
            return;
        }
        n = c.levels[i.ix()].next;
    }
    let id = LevelId(c.levels.len() as u32);
    c.levels.push(Level {
        lo: 0.0,
        hi: h,
        next: None,
    });
    let mut prev = None;
    let mut n = c.g.levels;
    if n.is_some() {
        while let Some(i) = n {
            if num::lt(h, c.levels[i.ix()].hi, tol.value) {
                break;
            }
            prev = Some(i);
            n = c.levels[i.ix()].next;
        }
        c.levels[id.ix()].next = n;
        if let Some(p) = prev {
            c.levels[p.ix()].next = Some(id);
            return;
        }
    }
    c.g.levels = Some(id);
}

/// The level list: every face height and 100, sorted; each interval's
/// lower end is the previous upper end.
pub fn levels(c: &mut Core) -> Result<(), CoreError> {
    c.g.levels = None;
    let ring = c.g.ring.ok_or(fault("no ring"))?;
    let mut w = ring;
    loop {
        let mut f = c.wall(w).faces;
        while let Some(fi) = f {
            let h = c.face(fi).h;
            level_insert(c, h);
            f = c.face(fi).next;
        }
        w = c.wnext(w)?;
        if w == ring {
            break;
        }
    }
    level_insert(c, 100.0);
    let mut prev: Option<LevelId> = None;
    let mut n = c.g.levels;
    while let Some(i) = n {
        if let Some(p) = prev {
            c.levels[i.ix()].lo = c.levels[p.ix()].hi;
        }
        prev = Some(i);
        n = c.levels[i.ix()].next;
    }
    Ok(())
}

/// Hooks the pipeline calls at the stage boundaries inside S1.
pub type Hook<'a> = Option<Box<dyn FnMut(&str, &Core) + 'a>>;

/// The S1 part of the per-model routine, up to the first height slab.
pub fn model_start(c: &mut Core, hook: &mut Hook) -> Result<(), CoreError> {
    c.g.ok = 0;
    ring_sort(c)?;
    if c.g.ring.map(|r| c.wall(r).typ) == Some(b'w') {
        return Err(CoreError::Unimplemented("'w' model heights"));
    }
    relink_holes(c);
    if let Some(h) = hook {
        h("s1_ring", c);
    }
    frames(c)?;
    if let Some(h) = hook {
        h("s1_frames", c);
    }
    ranges(c)?;
    eave_heights(c)?;
    levels(c)?;
    c.g.pass2 = 0;
    let head = c.g.levels.ok_or(fault("no levels"))?;
    c.g.level_cur = c.levels[head.ix()].next;
    let Some(l) = c.g.level_cur else {
        return Err(CoreError::fatal(2, MSG_INTERVAL));
    };
    c.g.lvl_lo = c.levels[l.ix()].lo;
    c.g.z_top = c.levels[l.ix()].hi;
    c.g.z_lo = c.g.lvl_lo;
    c.g.z_hi = c.g.z_top;
    if let Some(h) = hook {
        h("s1_end", c);
    }
    Ok(())
}
