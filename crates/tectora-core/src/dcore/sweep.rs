//! S2: the height sweep of the per-model plane cut.
//!
//! Starting from the slab S1 leaves (`z_lo..z_hi`), every iteration picks
//! each wall's active face for the slab, builds the list of face pairs to
//! intersect (each face with the active faces of its ring neighbours),
//! intersects them, narrows the slab to the lowest event, emits the cut
//! segments into the faces ([`close::add_seg`]) and closes what can be
//! closed. The sweep runs upwards through the height levels; when faces
//! stay open, a second, downward pass follows. A model ends closed, with a
//! cut error, or (in the second pass) with "faces not closed".
//!
//! Cursor globals the original keeps (`cur_wall`, `cur_face`, `cut_cur`,
//! `face_a`, ...) are updated where it updates them: their final values
//! are visible state.

use std::cell::Cell;

use super::close;
use super::error::*;
use super::geom::{self, Line, NO_POINT};
use super::num::{self, HALF_PI_F, V3};
use super::s1;
use super::state::{Core, Cut, CutId, Face, FaceId, Notice, TFACE, WallId, ctr};

/// Sweep iterations (both passes) after which the port gives up: the
/// original loops forever on some inputs, adding segments every round.
/// Real models need at most a few dozen.
const MAX_ITER: u32 = 200;

thread_local! {
    static LAST_SITE: Cell<&'static str> = const { Cell::new("") };
}

/// Where the last cut error of the sweep was raised (for tests: several
/// sites share a message).
pub fn last_cut_site() -> &'static str {
    LAST_SITE.with(|s| s.get())
}

fn cut_err(site: &'static str, msg: Msg) -> CoreError {
    LAST_SITE.with(|s| s.set(site));
    CoreError::Cut { code: 2, msg }
}

fn fault(what: &'static str) -> CoreError {
    CoreError::Fault(what)
}

/// A progress callback: the message id of each progress call.
pub type Progress<'a> = dyn FnMut(u32, &Core) + 'a;

/// Runs the sweep and the closing of the current model (see
/// [`super::stages::sweep`]).
pub fn sweep(c: &mut Core) -> Result<i32, CoreError> {
    sweep_with(c, &mut |_, _| {})
}

fn is_t(typ: u8) -> bool {
    typ == b'T' || typ == b't'
}

struct Sweep<'p, 'a> {
    prog: &'p mut Progress<'a>,
    /// The wall of the model's single sloped face (only meaningful under
    /// flag 0x40).
    single: Option<WallId>,
    /// Number of walls in the ring.
    n: i32,
}

/// [`sweep`] with a progress callback.
pub fn sweep_with(c: &mut Core, prog: &mut Progress) -> Result<i32, CoreError> {
    LAST_SITE.with(|s| s.set(""));
    let ring = c.g.ring.ok_or(fault("no ring"))?;
    let n = c.g.counters[ctr::RING_WALLS];
    // The wall of the last face that is not vertical (set while S1 builds
    // the frames).
    let mut single = None;
    let mut w = ring;
    for _ in 0..n.max(0) {
        let mut f = c.wall(w).faces.and_then(|h| c.face(h).next);
        while let Some(fi) = f {
            if num::ne(c.face(fi).pitch, HALF_PI_F, c.g.tol.angle) {
                single = Some(w);
            }
            f = c.face(fi).next;
        }
        w = c.wall(w).next.ok_or(fault("open ring"))?;
    }
    let mut s = Sweep { prog, single, n };
    s.run(c)
}

impl Sweep<'_, '_> {
    fn progress(&mut self, c: &Core, id: u32) {
        (self.prog)(id, c);
    }

    fn ring(&self, c: &Core) -> Result<WallId, CoreError> {
        c.g.ring.ok_or(fault("no ring"))
    }

    fn wnext(c: &Core, w: WallId) -> Result<WallId, CoreError> {
        c.wall(w).next.ok_or(fault("open ring"))
    }

    fn wprev(c: &Core, w: WallId) -> Result<WallId, CoreError> {
        c.wall(w).prev.ok_or(fault("open ring"))
    }

    fn run(&mut self, c: &mut Core) -> Result<i32, CoreError> {
        let tol = c.g.tol;
        let mut iter = 0u32;
        loop {
            if c.g.pass2 != 0 {
                face_ranges_down(c)?;
                c.g.z_lo = c.g.lvl_lo;
                c.g.z_hi = c.g.z_top;
            }
            loop {
                self.progress(c, 0xa16);
                iter += 1;
                if iter > MAX_ITER {
                    return Err(CoreError::Hang("height sweep"));
                }
                let n0 = c.g.counters[ctr::SEGS];
                if num::ge(c.g.z_lo, c.g.z_hi, tol.value) || select_active(c)? == 0 {
                    break;
                }
                let ring = self.ring(c)?;
                c.g.cur_wall = Some(ring);
                for _ in 0..self.n.max(0) {
                    let w = c.g.cur_wall.unwrap();
                    if c.wall(w).left.is_none() {
                        let p = Self::wprev(c, w)?;
                        c.wall_mut(w).left = c.wall(p).act;
                    }
                    if c.wall(w).right.is_none() {
                        let nx = Self::wnext(c, w)?;
                        c.wall_mut(w).right = c.wall(nx).act;
                    }
                    c.g.cur_wall = Some(Self::wnext(c, w)?);
                }
                if c.g.pass2 != 0 {
                    slab_floor(c)?;
                }
                self.progress(c, 0xa17);
                let mut k = c.g.cuts;
                while let Some(ci) = k {
                    c.g.cut_cur = c.cuts[ci.ix()].next;
                    c.g.cut_tail = Some(ci);
                    k = c.g.cut_cur;
                }
                c.g.cuts = None;
                c.g.cut_tail = None;
                c.g.cut_cur = None;
                self.progress(c, 0xa18);
                if self.cut_list(c)? {
                    break;
                }
                mark_ends(c);
                self.progress(c, 0xa19);
                let (any_h, any_opp) = cut_lines(c)?;
                if c.g.flags & 0x40 == 0 {
                    self.progress(c, 0xa1a);
                    events(c);
                }
                self.progress(c, 0xa1b);
                if c.g.pass2 == 0 {
                    let mut k = c.g.cuts;
                    while let Some(ci) = k {
                        c.g.cut_cur = Some(ci);
                        let cut = c.cuts[ci.ix()].clone();
                        for f in [cut.a, cut.b] {
                            let f = f.ok_or(fault("cut without face"))?;
                            c.g.cur_face = Some(f);
                            let w = c.face(f).owner.ok_or(fault("face without wall"))?;
                            c.g.cur_wall = Some(w);
                            eave_seg(c, w)?;
                        }
                        k = c.cuts[ci.ix()].next;
                    }
                    c.g.cut_cur = None;
                }
                if c.g.flags & 0x40 == 0 && num::ne(c.g.z_lo, c.g.z_hi, tol.value) {
                    self.progress(c, 0xa1c);
                    sloped_cuts(c)?;
                } else {
                    let id = if c.g.flags & 0x40 != 0 { 0xa1c } else { 0xa1d };
                    self.progress(c, id);
                    self.level_cuts(c, any_h, any_opp)?;
                }
                let ring = self.ring(c)?;
                c.g.cur_wall = Some(ring);
                for _ in 0..self.n.max(0) {
                    let w = c.g.cur_wall.unwrap();
                    let mut f = c.wall(w).cur;
                    while let Some(fi) = f {
                        c.g.cur_face = Some(fi);
                        let fc = c.face(fi);
                        if fc.closed != 0 && fc.alias.is_some() {
                            c.face_mut(fi).alias = None;
                        }
                        f = c.face(fi).next;
                    }
                    c.g.cur_face = None;
                    c.g.cur_wall = Some(Self::wnext(c, w)?);
                }
                self.progress(c, 0xa1e);
                c.g.cur_wall = Some(ring);
                for _ in 0..self.n.max(0) {
                    let w = c.g.cur_wall.unwrap();
                    let mut f = c.wall(w).cur;
                    while let Some(fi) = f {
                        c.g.cur_face = Some(fi);
                        let fc = c.face(fi);
                        if fc.closed == 0 && fc.typ != b'W' {
                            close::drop_single(c, fi);
                            if close::close_face(c, fi, false)? {
                                close::advance(c, fi)?;
                            }
                        }
                        f = c.face(fi).next;
                    }
                    c.g.cur_face = None;
                    c.g.cur_wall = Some(Self::wnext(c, w)?);
                }
                if c.g.pass2 == 0 {
                    if num::lt(c.g.z_hi, 100.0, tol.value) {
                        c.g.z_top = c.g.z_hi;
                    }
                    let lv = c.g.level_cur.ok_or(fault("no level"))?;
                    let hi = c.levels[lv.ix()].hi;
                    if num::lt(c.g.z_hi, hi, tol.value) {
                        c.g.z_lo = c.g.z_hi;
                        c.g.z_hi = hi;
                    } else {
                        let Some(nl) = c.levels[lv.ix()].next else {
                            break;
                        };
                        c.g.z_lo = c.levels[nl.ix()].lo;
                        c.g.z_hi = c.levels[nl.ix()].hi;
                        c.g.level_cur = Some(nl);
                    }
                } else {
                    let lo = c.g.z_lo;
                    c.g.z_lo = -100.0;
                    c.g.z_hi = lo;
                }
                if n0 == c.g.counters[ctr::SEGS] || all_done(c)? {
                    break;
                }
            }
            self.progress(c, 0xa1f);
            if self.finish(c)? {
                return Ok(0);
            }
            c.g.pass2 = 1;
        }
    }

    /// Phase 12: closes every face (with retries under flag 0x400). True
    /// when the model is done; false when a second pass is due.
    fn finish(&mut self, c: &mut Core) -> Result<bool, CoreError> {
        let ring = self.ring(c)?;
        let mut all = true;
        c.g.cur_wall = Some(ring);
        for _ in 0..self.n.max(0) {
            let w = c.g.cur_wall.unwrap();
            let mut f = c.wall(w).faces;
            while let Some(fi) = f {
                c.g.cur_face = Some(fi);
                if c.face(fi).typ != b'W' {
                    close::close_face(c, fi, c.g.flags & 0x400 != 0)?;
                    let fc = c.face(fi);
                    let owner = fc.owner.ok_or(fault("face without wall"))?;
                    if fc.closed == 0 && is_t(c.wall(owner).typ) && fc.typ == b'K' {
                        let fm = c.face_mut(fi);
                        fm.closed = 1;
                        fm.segs = None;
                    } else {
                        all &= c.face(fi).closed != 0;
                    }
                }
                f = c.face(fi).next;
            }
            c.g.cur_face = None;
            c.g.cur_wall = Some(Self::wnext(c, w)?);
        }
        if all_done(c)? {
            all = true;
        }
        if !all {
            if c.g.pass2 == 0 {
                return Ok(false);
            }
            if c.wall(ring).typ != b'w' {
                return Err(cut_err("open faces", MSG_OPEN_FACES));
            }
        }
        c.g.cur_wall = Some(ring);
        for _ in 0..self.n.max(0) {
            let w = c.g.cur_wall.unwrap();
            c.g.cur_wall = Some(Self::wnext(c, w)?);
            c.g.cur_face = None;
        }
        c.g.ok = 1;
        Ok(true)
    }

    /// Phase 5: the cut list. Each wall's active face is paired with the
    /// active faces of its left and right neighbours (walking past walls
    /// without one); a coplanar neighbour is merged into it instead. Under
    /// flag 0x40 every active face is paired with the single sloped face.
    /// True when the sweep ends here (phase 12 follows).
    fn cut_list(&mut self, c: &mut Core) -> Result<bool, CoreError> {
        let tol = c.g.tol;
        let ring = self.ring(c)?;
        c.g.cur_wall = Some(ring);
        let n = self.n;
        let mut i = 0;
        if c.g.flags & 0x40 != 0 {
            let single = self.single.ok_or(fault("no sloped face"))?;
            while i < n {
                let w = c.g.cur_wall.unwrap();
                if w != single {
                    add_cut(c, c.wall(w).act, c.wall(single).act);
                }
                c.g.cur_wall = Some(Self::wnext(c, w)?);
                i += 1;
            }
            let w = c.g.cur_wall.unwrap();
            c.g.cur_wall = Some(Self::wnext(c, w)?);
            return Ok(false);
        }
        while i < n {
            let p = c.g.cur_wall.unwrap();
            let act = c.wall(p).act;
            c.g.cur_face = act;
            if let Some(a) = act
                && c.face(a).sub != i32::MIN
                && c.face(a).closed == 0
            {
                let flags = c.g.flags;
                let mut prev = Self::wprev(c, p)?;
                let next = Self::wnext(c, p)?;
                c.g.prev_wall = Some(prev);
                c.g.next_wall = Some(next);
                if prev == p || next == p {
                    if is_t(c.wall(p).typ) {
                        return Ok(true);
                    }
                    let cur = c.wall(prev).cur.ok_or(fault("wall without face cursor"))?;
                    if close::close_face(c, cur, flags & 0x400 != 0)? {
                        return Ok(true);
                    }
                    return Err(cut_err("single face (ring)", MSG_SINGLE_FACE));
                }
                // Left neighbours.
                loop {
                    if prev == p {
                        if is_t(c.wall(p).typ) {
                            return Ok(true);
                        }
                        let cur = c.wall(p).cur.ok_or(fault("wall without face cursor"))?;
                        if close::close_face(c, cur, flags & 0x400 != 0)? {
                            return Ok(true);
                        }
                        return Err(cut_err("single face (left)", MSG_SINGLE_FACE));
                    }
                    let Some(pa) = c.wall(prev).act else {
                        prev = Self::wprev(c, prev)?;
                        c.g.prev_wall = Some(prev);
                        continue;
                    };
                    let a = c.wall(p).act.ok_or(fault("no active face"))?;
                    if geom::coplanar(&c.face(a).into(), &c.face(pa).into(), &tol) {
                        eave_seg(c, prev)?;
                        let pa = c.wall(prev).act.ok_or(fault("no active face"))?;
                        if !close::share_end(c, a, pa) {
                            break;
                        }
                        c.face_mut(pa).alias = Some(a);
                        let (fa, fp) = (c.face(a).clone(), c.face(pa).clone());
                        let v1 = num::sub(num::add(fp.o, fp.w), fa.o);
                        let v2 = num::sub(num::add(fa.o, fa.w), fp.o);
                        let l1 = num::len(v1) as f32;
                        let l2 = num::len(v2) as f32;
                        if c.g.flags & 0x40 == 0 {
                            if num::lt(l1, l2, tol.value) {
                                c.wall_mut(p).left = c.wall(prev).left;
                            } else {
                                c.wall_mut(p).right = c.wall(prev).right;
                            }
                        }
                        close::merge_faces(c, a, pa)?;
                        note_breaks(c, p, prev, pa);
                        close_chain(c, pa);
                        let wm = c.wall_mut(prev);
                        wm.act = None;
                        wm.cur = None;
                        prev = Self::wprev(c, prev)?;
                        c.g.prev_wall = Some(prev);
                        continue;
                    }
                    let r = order_left(c, prev, p)?;
                    let pw = c.wall(prev).clone();
                    let b = a;
                    let fa = match r {
                        -1 => {
                            if is_t(pw.typ) {
                                pa
                            } else {
                                pw.eave
                            }
                        }
                        0 => {
                            if c.face(pa).sub == i32::MIN {
                                pw.faces.ok_or(fault("wall without faces"))?
                            } else {
                                pa
                            }
                        }
                        1 => {
                            if is_t(pw.typ) {
                                pa
                            } else {
                                pw.faces.ok_or(fault("wall without faces"))?
                            }
                        }
                        _ => {
                            if c.face(pa).sub == i32::MIN {
                                c.g.face_a = Some(pa);
                                c.g.face_b = Some(b);
                                let nx = Self::wnext(c, prev)?;
                                add_cut(c, Some(c.wall(nx).eave), Some(b));
                            }
                            pa
                        }
                    };
                    c.g.face_a = Some(fa);
                    c.g.face_b = Some(b);
                    if c.g.flags & 0x40 == 0 && num::par(c.face(fa).w, c.face(b).w, &tol) == 0 {
                        c.wall_mut(p).left = Some(fa);
                    }
                    add_cut(c, Some(fa), Some(b));
                    break;
                }
                // Right neighbours.
                let mut next = next;
                loop {
                    if next == p {
                        return Err(cut_err("single face (right)", MSG_SINGLE_FACE));
                    }
                    let Some(na) = c.wall(next).act else {
                        next = Self::wnext(c, next)?;
                        c.g.next_wall = Some(next);
                        continue;
                    };
                    let a = c.wall(p).act.ok_or(fault("no active face"))?;
                    if geom::coplanar(&c.face(a).into(), &c.face(na).into(), &tol) {
                        eave_seg(c, next)?;
                        let na = c.wall(next).act.ok_or(fault("no active face"))?;
                        if !close::share_end(c, a, na) {
                            break;
                        }
                        c.face_mut(na).alias = Some(a);
                        let (fa, fnx) = (c.face(a).clone(), c.face(na).clone());
                        let v1 = num::sub(num::add(fa.o, fa.w), fnx.o);
                        let v2 = num::sub(num::add(fnx.o, fnx.w), fa.o);
                        let l1 = num::len(v1) as f32;
                        let l2 = num::len(v2) as f32;
                        if c.g.flags & 0x40 == 0 {
                            if num::lt(l1, l2, tol.value) {
                                c.wall_mut(p).right = c.wall(next).right;
                            } else {
                                c.wall_mut(p).left = c.wall(next).left;
                            }
                        }
                        close::merge_faces(c, a, na)?;
                        note_breaks(c, p, next, na);
                        close_chain(c, na);
                        let wm = c.wall_mut(next);
                        wm.act = None;
                        wm.cur = None;
                        next = Self::wnext(c, next)?;
                        c.g.next_wall = Some(next);
                        continue;
                    }
                    let r = order_right(c, p, next)?;
                    let nw = c.wall(next).clone();
                    let fa = a;
                    let fb = match r {
                        0 => {
                            if c.face(na).sub == i32::MIN {
                                nw.faces.ok_or(fault("wall without faces"))?
                            } else {
                                na
                            }
                        }
                        1 => {
                            if is_t(nw.typ) {
                                na
                            } else {
                                nw.faces.ok_or(fault("wall without faces"))?
                            }
                        }
                        2 => {
                            if c.face(na).sub == i32::MIN {
                                c.g.face_a = Some(fa);
                                c.g.face_b = Some(na);
                                let pv = Self::wprev(c, next)?;
                                add_cut(c, Some(fa), Some(c.wall(pv).eave));
                            }
                            na
                        }
                        _ => {
                            if is_t(nw.typ) {
                                na
                            } else {
                                nw.eave
                            }
                        }
                    };
                    c.g.face_a = Some(fa);
                    c.g.face_b = Some(fb);
                    if c.g.flags & 0x40 == 0 && num::par(c.face(fa).w, c.face(fb).w, &tol) == 0 {
                        c.wall_mut(p).right = Some(fb);
                    }
                    add_cut(c, Some(fa), Some(fb));
                    break;
                }
            }
            c.g.cur_wall = Some(Self::wnext(c, p)?);
            i += 1;
        }
        Ok(false)
    }

    /// Phase 10: cuts within a level (horizontal cut lines, or every cut
    /// under flag 0x40): each face's segment runs between the points where
    /// the cut line meets the planes of the face's neighbours; the longer
    /// of the two is cut back to the shorter.
    fn level_cuts(&mut self, c: &mut Core, _any_h: bool, any_opp: bool) -> Result<(), CoreError> {
        let tol = c.g.tol;
        let flags = c.g.flags;
        let f40 = flags & 0x40 != 0;
        let mut k = c.g.cuts;
        while let Some(ci) = k {
            c.g.cut_cur = Some(ci);
            let cut = c.cuts[ci.ix()].clone();
            let line = Line { p: cut.p, d: cut.d };
            let sel = f40
                || (geom::horizontal(&line, &tol)
                    && (flags & 0x80 == 0 || !any_opp || cut.par == -1));
            if sel {
                let a = cut.a.ok_or(fault("cut without face"))?;
                let b = cut.b.ok_or(fault("cut without face"))?;
                c.g.face_a = Some(a);
                c.g.face_b = Some(b);
                let wa = c.face(a).owner.ok_or(fault("face without wall"))?;
                c.g.prev_wall = Some(wa);
                let (mut pa1, mut pa2) = ends_on_neighbours(c, wa, &line)?;
                if !f40 {
                    if num::peq(pa1, NO_POINT, tol.value) {
                        return Err(cut_err("level cut A1", MSG_NO_X_A1));
                    }
                    if num::peq(pa2, NO_POINT, tol.value) {
                        return Err(cut_err("level cut A2", MSG_NO_X_A2));
                    }
                }
                let wb = c.face(b).owner.ok_or(fault("face without wall"))?;
                c.g.next_wall = Some(wb);
                let (mut pb1, mut pb2) = ends_on_neighbours(c, wb, &line)?;
                if !f40 {
                    if num::peq(pb1, NO_POINT, tol.value) {
                        return Err(cut_err("level cut B1", MSG_NO_X_B1));
                    }
                    if num::peq(pb2, NO_POINT, tol.value) {
                        return Err(cut_err("level cut B2", MSG_NO_X_B2));
                    }
                }
                let mut la = num::len(num::sub(pa1, pa2)) as f32;
                let mut lb = num::len(num::sub(pb1, pb2)) as f32;
                if num::zero(la, tol.value) && num::zero(lb, tol.value) {
                    let cm = &mut c.cuts[ci.ix()];
                    cm.a = None;
                    cm.b = None;
                    k = c.cuts[ci.ix()].next;
                    continue;
                }
                if f40 {
                    if num::ne(c.face(a).pitch, HALF_PI_F, tol.angle) {
                        c.wall_mut(wb).extra = 1.0;
                    }
                    if num::ne(c.face(b).pitch, HALF_PI_F, tol.angle) {
                        c.wall_mut(wa).extra = 1.0;
                    }
                }
                let (xa, xb) = (c.wall(wa).extra, c.wall(wb).extra);
                if flags & 0x80 != 0 && xa == 0.0 && xb == 0.0 && cut.par == 1 {
                    // Parallel faces meeting at a level: both get the
                    // segment of the steeper (flag 0x100: flatter) face.
                    let (pa, pb) = (c.face(a).pitch, c.face(b).pitch);
                    let take_a = if flags & 0x100 != 0 {
                        num::lt(pa, pb, tol.value)
                    } else {
                        num::gt(pa, pb, tol.value)
                    };
                    let (p, q) = if take_a { (pa1, pa2) } else { (pb1, pb2) };
                    close::add_seg(c, cut.ea != 0, a, b'H', p, q, Some(a), Some(b))?;
                    close::add_seg(c, cut.eb != 0, b, b'H', p, q, Some(a), Some(b))?;
                    k = c.cuts[ci.ix()].next;
                    continue;
                }
                let one = |l: f32| (f64::from(l) + 1.0) as f32;
                if num::gt(xa, xb, tol.value) && num::ge(xb, 0.0, tol.value) {
                    lb = one(la);
                }
                if num::lt(xa, xb, tol.value) && num::ge(xa, 0.0, tol.value) {
                    la = one(lb);
                }
                let (ca, cb) = (c.face(a).closed != 0, c.face(b).closed != 0);
                if ca && cb {
                    let cm = &mut c.cuts[ci.ix()];
                    cm.a = None;
                    cm.b = None;
                    k = c.cuts[ci.ix()].next;
                    continue;
                }
                if !ca && cb {
                    lb = one(la);
                } else if ca && !cb {
                    la = one(lb);
                }
                let trim = flags & 0x80 == 0
                    || !(geom::seg_overlap((pa1, pa2), (pb1, pb2), &tol)
                        || geom::seg_overlap((pb1, pb2), (pa1, pa2), &tol))
                    || {
                        let mut plain = true;
                        for f in [
                            c.wall(wb).left,
                            c.wall(wb).right,
                            c.wall(wa).left,
                            c.wall(wa).right,
                        ] {
                            let f = f.ok_or(fault("wall without neighbour face"))?;
                            let fc = c.face(f);
                            if fc.sub == i32::MIN || fc.typ == b'W' {
                                plain = false;
                                break;
                            }
                        }
                        plain
                    };
                if trim {
                    if num::eq(la, lb, tol.value) {
                        c.g.z_hi = pa1[2];
                    } else if num::lt(la, lb, tol.value) {
                        if num::gt(la, 0.0, tol.value) {
                            let da = num::norm(num::sub(pa2, pa1), &tol);
                            let db = num::norm(num::sub(pb1, pb2), &tol);
                            if num::peq(pa1, pb2, tol.value) {
                                if num::peq(da, db, tol.value) && !f40 {
                                    c.wall_mut(wb).right = c.wall(wa).right;
                                }
                            } else if num::peq(pa2, pb1, tol.value)
                                && num::peq(da, db, tol.value)
                                && !f40
                            {
                                c.wall_mut(wb).left = c.wall(wa).left;
                            }
                        }
                        c.g.z_hi = pa1[2];
                        pb1 = pa1;
                        pb2 = pa2;
                    } else if num::lt(lb, la, tol.value) {
                        if num::gt(lb, 0.0, tol.value) {
                            let da = num::norm(num::sub(pa2, pa1), &tol);
                            let db = num::norm(num::sub(pb1, pb2), &tol);
                            if num::peq(pa1, pb2, tol.value) {
                                if num::peq(da, db, tol.value) && !f40 {
                                    c.wall_mut(wa).left = c.wall(wb).left;
                                }
                            } else if num::peq(pa2, pb1, tol.value)
                                && num::peq(da, db, tol.value)
                                && !f40
                            {
                                c.wall_mut(wa).right = c.wall(wb).right;
                            }
                        }
                        c.g.z_hi = pa1[2];
                        pa1 = pb1;
                        pa2 = pb2;
                    }
                }
                let mut kind = if c.face(a).typ == b'W' && c.face(b).typ == b'W' {
                    b'W'
                } else {
                    b'H'
                };
                if f40 {
                    kind = if geom::horizontal(&line, &tol) {
                        b'H'
                    } else {
                        b'S'
                    };
                }
                if c.face(a).closed == 0 {
                    close::add_seg(c, cut.ea != 0, a, kind, pa1, pa2, Some(a), Some(b))?;
                }
                if c.face(b).closed == 0 {
                    close::add_seg(c, cut.eb != 0, b, kind, pb1, pb2, Some(a), Some(b))?;
                }
                if f40 {
                    let done = if num::eq(c.face(b).pitch, HALF_PI_F, tol.angle) {
                        Some(b)
                    } else if num::eq(c.face(a).pitch, HALF_PI_F, tol.angle) {
                        Some(a)
                    } else {
                        None
                    };
                    if let Some(f) = done {
                        let fm = c.face_mut(f);
                        fm.closed = 1;
                        fm.segs = None;
                        close::advance(c, f)?;
                        c.g.counters[ctr::CLOSED] += 1;
                        if c.face(f).typ == b'D' {
                            c.g.counters[ctr::D_CLOSED] += 1;
                        } else {
                            c.g.counters[ctr::K_CLOSED] += 1;
                        }
                        self.progress(c, 0xa1c);
                    }
                }
                let cm = &mut c.cuts[ci.ix()];
                cm.a = None;
                cm.b = None;
            }
            k = c.cuts[ci.ix()].next;
        }
        c.g.cut_cur = None;
        Ok(())
    }
}

/// The points where a cut line meets the planes of a wall's left and
/// right neighbour faces ([`NO_POINT`] when it does not or a neighbour is
/// missing). Under flag 0x40 a sloped neighbour stands for its wall's eave
/// face.
fn ends_on_neighbours(c: &Core, w: WallId, line: &Line) -> Result<(V3, V3), CoreError> {
    let tol = c.g.tol;
    let mut l = c.wall(w).left;
    let mut r = c.wall(w).right;
    if c.g.flags & 0x40 != 0 {
        let eave = |f: Option<FaceId>| -> Result<Option<FaceId>, CoreError> {
            let f = f.ok_or(fault("wall without neighbour face"))?;
            if num::ne(c.face(f).pitch, HALF_PI_F, tol.angle) {
                let o = c.face(f).owner.ok_or(fault("face without wall"))?;
                Ok(Some(c.wall(o).eave))
            } else {
                Ok(Some(f))
            }
        };
        l = eave(l)?;
        r = eave(r)?;
    }
    match (l, r) {
        (Some(l), Some(r)) => Ok((lp(c, l, line), lp(c, r, line))),
        _ => Ok((NO_POINT, NO_POINT)),
    }
}

/// Line-plane intersection with a face; the sentinel on failure (what the
/// original leaves in its output).
fn lp(c: &Core, f: FaceId, line: &Line) -> V3 {
    match geom::line_plane(&c.face(f).into(), line, &c.g.tol) {
        Ok(p) | Err(p) => p,
    }
}

/// Closes a face and the faces above it after their outline was merged
/// into a coplanar neighbour.
fn close_chain(c: &mut Core, start: FaceId) {
    let mut f = Some(start);
    while let Some(fi) = f {
        let fm = c.face_mut(fi);
        fm.segs = None;
        if fm.closed == 0 {
            fm.closed = 1;
            let typ = fm.typ;
            let k = &mut c.g.counters;
            match typ {
                b'D' => {
                    k[ctr::D_OPEN] -= 1;
                    k[ctr::D_CLOSED] += 1;
                }
                b'K' => {
                    k[ctr::K_OPEN] -= 1;
                    k[ctr::K_CLOSED] += 1;
                }
                b'W' => {
                    k[ctr::W_OPEN] -= 1;
                    k[ctr::W_CLOSED] += 1;
                }
                _ => {}
            }
            k[ctr::OPEN] -= 1;
            k[ctr::CLOSED] += 1;
        }
        f = c.face(fi).next;
    }
}

/// After the faces of `absorbed` were joined into `kept`'s: DACAD's
/// information box "Knicke bei Wand %d werden ignoriert" (0xa35) when the
/// absorbed face has pitch breaks above it.
fn note_breaks(c: &mut Core, kept: WallId, absorbed: WallId, face: FaceId) {
    if c.face(face).next.is_some() {
        let model = c.g.cur_model.map_or(0, |m| c.model(m).no);
        let (kept, absorbed) = (c.wall(kept).no, c.wall(absorbed).no);
        c.notices.push(Notice::IgnoredBreaks {
            model,
            kept,
            absorbed,
        });
    }
}

/// Appends the cut (a, b) unless the pair (in either order) is listed.
fn add_cut(c: &mut Core, a: Option<FaceId>, b: Option<FaceId>) {
    let mut k = c.g.cuts;
    c.g.cut_cur = k;
    while let Some(ci) = k {
        let cut = &c.cuts[ci.ix()];
        if (cut.a == a && cut.b == b) || (cut.a == b && cut.b == a) {
            return;
        }
        k = cut.next;
        c.g.cut_cur = k;
    }
    let id = CutId(c.cuts.len() as u32);
    c.cuts.push(Cut {
        a,
        ea: 0,
        b,
        eb: 0,
        p: NO_POINT,
        d: NO_POINT,
        x: NO_POINT,
        par: 0,
        prev: c.g.cut_tail,
        next: None,
    });
    c.g.cut_cur = Some(id);
    match c.g.cuts {
        Some(_) => {
            let t = c.g.cut_tail.expect("cut list without tail");
            c.cuts[t.ix()].next = Some(id);
        }
        None => c.g.cuts = Some(id),
    }
    c.g.cut_tail = Some(id);
}

/// Which faces of a cut receive segments: sloped faces always; a pair of
/// sloped faces not when the same pair also appears with the hidden wall
/// face of the same wall in place of one of them.
fn mark_ends(c: &mut Core) {
    let ids: Vec<CutId> = {
        let mut v = Vec::new();
        let mut k = c.g.cuts;
        while let Some(ci) = k {
            v.push(ci);
            k = c.cuts[ci.ix()].next;
        }
        v
    };
    let typ = |c: &Core, f: Option<FaceId>| f.map(|f| c.face(f).typ).unwrap_or(0);
    let owner = |c: &Core, f: Option<FaceId>| f.and_then(|f| c.face(f).owner);
    for &ci in &ids {
        let cut = c.cuts[ci.ix()].clone();
        if typ(c, cut.a) != b'W' {
            c.cuts[ci.ix()].ea = 1;
        }
        if typ(c, cut.b) != b'W' {
            c.cuts[ci.ix()].eb = 1;
        }
    }
    for &i in &ids {
        let ci = c.cuts[i.ix()].clone();
        if typ(c, ci.a) != b'W' && typ(c, ci.b) != b'W' {
            for &j in &ids {
                let cj = &c.cuts[j.ix()];
                if i != j
                    && typ(c, cj.a) == b'W'
                    && owner(c, ci.a) == owner(c, cj.a)
                    && ci.b == cj.b
                {
                    c.cuts[i.ix()].eb = 0;
                    break;
                }
            }
        }
    }
    for &i in &ids {
        let ci = c.cuts[i.ix()].clone();
        if typ(c, ci.a) != b'W' && typ(c, ci.b) != b'W' {
            for &j in &ids {
                let cj = &c.cuts[j.ix()];
                if i != j
                    && typ(c, cj.b) == b'W'
                    && owner(c, ci.b) == owner(c, cj.b)
                    && ci.a == cj.a
                {
                    c.cuts[i.ix()].ea = 0;
                    break;
                }
            }
        }
    }
}

/// Phase 6: the cut line of every cut; a horizontal cut line below the
/// slab bottom closes the slab (first pass), any horizontal one does in
/// the second pass. Returns whether any line is horizontal and whether a
/// horizontal one separates opposite faces.
fn cut_lines(c: &mut Core) -> Result<(bool, bool), CoreError> {
    let tol = c.g.tol;
    let mut any_h = false;
    let mut any_opp = false;
    let mut minz = 0.0f32;
    let mut k = c.g.cuts;
    c.g.cut_cur = k;
    while let Some(ci) = k {
        let cut = c.cuts[ci.ix()].clone();
        let a = cut.a.ok_or(fault("cut without face"))?;
        let b = cut.b.ok_or(fault("cut without face"))?;
        c.g.face_a = Some(a);
        c.g.face_b = Some(b);
        let (_, l) = geom::plane_plane(&c.face(a).into(), &c.face(b).into(), &tol);
        c.g.tmp_line = [l.p[0], l.p[1], l.p[2], l.d[0], l.d[1], l.d[2]];
        c.g.prev_wall = c.face(a).owner;
        c.g.next_wall = c.face(b).owner;
        let par = if geom::horizontal(&l, &tol) {
            let z = l.p[2];
            // A plain compare: an unordered result counts as "less".
            #[allow(clippy::neg_cmp_op_on_partial_ord)]
            if !any_h || !(z >= minz) {
                minz = z;
            }
            any_h = true;
            let par = num::par(c.face(a).w, c.face(b).w, &tol);
            if par == -1 {
                any_opp = true;
            }
            par
        } else {
            0
        };
        let cm = &mut c.cuts[ci.ix()];
        cm.par = par;
        cm.p = l.p;
        cm.d = l.d;
        k = cm.next;
        c.g.cut_cur = k;
    }
    if any_h {
        if c.g.pass2 == 0 {
            if num::le(minz, c.g.z_lo, tol.value) {
                c.g.z_hi = c.g.z_lo;
            }
        } else {
            c.g.z_hi = c.g.z_lo;
        }
    }
    Ok((any_h, any_opp))
}

/// Phase 7: the intersection points of the cut lines in the slab; each
/// cut keeps its lowest one, and the slab top (second pass: bottom) moves
/// to it.
fn events(c: &mut Core) {
    let tol = c.g.tol;
    let mut i = c.g.cuts;
    while let Some(ii) = i {
        c.g.cut_cur = Some(ii);
        let mut j = c.g.cuts;
        while let Some(jj) = j {
            c.g.cut_tail = Some(jj);
            if ii != jj {
                let (ci, cj) = (c.cuts[ii.ix()].clone(), c.cuts[jj.ix()].clone());
                let li = Line { p: ci.p, d: ci.d };
                let lj = Line { p: cj.p, d: cj.d };
                let (found, x) = geom::line_line(&li, &lj, Some((c.g.z_lo, c.g.z_hi)), &tol);
                c.g.llx = x;
                if found {
                    let take = if !geom::horizontal(&li, &tol) && !geom::horizontal(&lj, &tol) {
                        num::lt(x[2], ci.x[2], tol.value)
                            && (c.g.pass2 != 0 || num::gt(x[2], c.g.z_lo, tol.value))
                    } else {
                        num::lt(x[2], ci.x[2], tol.value)
                    };
                    if take {
                        c.cuts[ii.ix()].x = x;
                    }
                    let xz = c.cuts[ii.ix()].x[2];
                    if c.g.pass2 == 0 {
                        if num::le(xz, c.g.z_hi, tol.value) {
                            c.g.z_hi = xz;
                        }
                    } else if num::ge(xz, c.g.z_lo, tol.value) {
                        c.g.z_lo = xz;
                    }
                }
            }
            j = c.cuts[jj.ix()].next;
        }
        c.g.cut_tail = None;
        i = c.cuts[ii.ix()].next;
    }
    c.g.cut_cur = None;
    c.g.cut_tail = None;
}

/// The temporary horizontal face at height `z` (template frame: u = x,
/// w = y).
fn tface_at(c: &mut Core, z: f32) {
    let mut f = Face::blank();
    f.sub = 0x53;
    f.u = [1.0, 0.0, 0.0];
    f.w = [0.0, 1.0, 0.0];
    f.n = [0.0, 0.0, 1.0];
    f.o[2] = z;
    *c.face_mut(TFACE) = f;
    s1::face_plane(c, TFACE);
}

/// Phase 9: sloped cut lines through the slab: each face of the pair gets
/// the piece of the line between the slab bottom (or the higher of the two
/// faces' bottoms when the bottom point is not on both outlines) and the
/// slab top or the cut's event point.
fn sloped_cuts(c: &mut Core) -> Result<(), CoreError> {
    let tol = c.g.tol;
    let mut k = c.g.cuts;
    while let Some(ci) = k {
        c.g.cut_cur = Some(ci);
        let cut = c.cuts[ci.ix()].clone();
        if cut.par == 0 {
            let a = cut.a.ok_or(fault("cut without face"))?;
            let b = cut.b.ok_or(fault("cut without face"))?;
            let line = Line { p: cut.p, d: cut.d };
            let z = if c.g.pass2 == 0 { c.g.z_lo } else { c.g.z_hi };
            tface_at(c, z);
            let mut p = geom::line_plane(&c.face(TFACE).into(), &line, &tol)
                .map_err(|_| cut_err("sloped cut P1", MSG_NO_XY_P1))?;
            if !close::has_end(c, a, p) || !close::has_end(c, b, p) {
                let (za, zb) = (c.face(a).zlo, c.face(b).zlo);
                let mut z = if num::gt(za, zb, tol.value) { za } else { zb };
                if c.g.pass2 == 0 && !num::gt(z, c.g.z_lo, tol.value) {
                    z = c.g.z_lo;
                }
                tface_at(c, z);
                p = geom::line_plane(&c.face(TFACE).into(), &line, &tol)
                    .map_err(|_| cut_err("sloped cut P1 (faces)", MSG_NO_XY_P1))?;
            }
            let z = if c.g.pass2 == 0 { c.g.z_hi } else { c.g.z_lo };
            tface_at(c, z);
            let x = cut.x;
            let q = if num::le(x[2], c.g.z_hi, tol.value)
                && num::ge(x[2], c.g.z_lo, tol.value)
                && num::pne(x, NO_POINT, tol.value)
            {
                x
            } else {
                geom::line_plane(&c.face(TFACE).into(), &line, &tol)
                    .map_err(|_| cut_err("sloped cut P2", MSG_NO_XY_P2))?
            };
            let kind = if c.face(a).typ == b'W' && c.face(b).typ == b'W' {
                b'W'
            } else {
                b'S'
            };
            close::add_seg(c, cut.ea != 0, a, kind, p, q, Some(a), Some(b))?;
            close::add_seg(c, cut.eb != 0, b, kind, p, q, Some(a), Some(b))?;
            let cm = &mut c.cuts[ci.ix()];
            cm.a = None;
            cm.b = None;
        }
        k = c.cuts[ci.ix()].next;
    }
    c.g.cut_cur = None;
    Ok(())
}

/// The eave segment of a wall's active face, once, when the face starts at
/// the slab bottom: between the points where the face's eave line meets
/// the planes of the wall's neighbour faces. A pitch break face also gives
/// the segment to the face below (and closes that one if it can).
fn eave_seg(c: &mut Core, w: WallId) -> Result<(), CoreError> {
    let tol = c.g.tol;
    let Some(f) = c.wall(w).act else {
        return Ok(());
    };
    let fc = c.face(f).clone();
    if fc.typ == b'W' || fc.eave_done == 1 || !num::eq(fc.zlo, c.g.z_lo, tol.value) {
        return Ok(());
    }
    c.g.tmp_line = [fc.o[0], fc.o[1], fc.o[2], fc.w[0], fc.w[1], fc.w[2]];
    let line = Line { p: fc.o, d: fc.w };
    let l = c.wall(w).left.ok_or(fault("wall without left face"))?;
    let r = c.wall(w).right.ok_or(fault("wall without right face"))?;
    let p = lp(c, l, &line);
    let q = lp(c, r, &line);
    let below = fc.prev;
    if fc.typ == b'K' {
        let pf = below.ok_or(fault("pitch break without face below"))?;
        close::add_seg(c, true, pf, b'K', p, q, Some(f), below)?;
        close::close_face(c, pf, false)?;
        close::add_seg(c, true, f, b'K', p, q, Some(f), below)?;
    } else {
        close::add_seg(c, true, f, b'T', p, q, Some(f), below)?;
    }
    c.face_mut(f).eave_done = 1;
    Ok(())
}

/// The active face of every wall for the slab (first pass: the lowest open
/// sloped face reaching above the slab bottom; second pass: the face at
/// the cursor when it reaches the slab top). Returns how many walls have
/// one.
fn select_active(c: &mut Core) -> Result<i32, CoreError> {
    let tol = c.g.tol;
    let ring = c.g.ring.ok_or(fault("no ring"))?;
    let mut count = 0;
    let mut w = ring;
    loop {
        c.wall_mut(w).act = None;
        if c.g.pass2 != 0 {
            let mut f = c.wall(w).cur;
            if let Some(f0) = f {
                c.g.cur_face = Some(f0);
            }
            while let Some(fi) = f {
                c.g.cur_face = Some(fi);
                if c.face(fi).closed != 0 {
                    break;
                }
                let fc = c.face(fi).clone();
                if fc.typ == b'D' || fc.typ == b'K' {
                    let mut found = false;
                    if num::ge(fc.zhi, c.g.z_hi, tol.value) {
                        if num::eq(fc.zlo, fc.zhi, tol.value) {
                            c.face_mut(fi).closed = 1;
                            c.g.counters[ctr::CLOSED] += 1;
                        } else {
                            found = true;
                        }
                    }
                    c.wall_mut(w).cur = Some(fi);
                    if found {
                        count += 1;
                        c.wall_mut(w).act = Some(fi);
                    }
                    break;
                }
                if fc.typ != b'W' {
                    c.wall_mut(w).cur = Some(fi);
                    break;
                }
                f = fc.prev;
                if f.is_none() {
                    c.g.cur_face = None;
                }
            }
        } else {
            let mut f = c.wall(w).cur;
            while let Some(fi) = f {
                c.g.cur_face = Some(fi);
                let fc = c.face(fi).clone();
                if fc.typ == b'D' || fc.typ == b'K' {
                    if !num::le(fc.zlo, c.g.z_lo, tol.value) {
                        let wl = c.wall(w);
                        let act = if is_t(wl.typ) { fc.prev } else { Some(wl.eave) };
                        let wm = c.wall_mut(w);
                        wm.cur = Some(fi);
                        wm.act = act;
                        break;
                    }
                    let mut found = false;
                    if num::eq(fc.zlo, fc.zhi, tol.value) {
                        c.face_mut(fi).closed = 1;
                        c.g.counters[ctr::CLOSED] += 1;
                    } else if num::eq(fc.pitch, 0.0, tol.angle) {
                        found = num::ge(fc.zhi, c.g.z_lo, tol.value);
                    } else {
                        found = num::gt(fc.zhi, c.g.z_lo, tol.value);
                    }
                    c.wall_mut(w).cur = Some(fi);
                    if found {
                        count += 1;
                        c.wall_mut(w).act = Some(fi);
                        break;
                    }
                } else if fc.typ != b'W' {
                    c.wall_mut(w).cur = Some(fi);
                }
                f = fc.next;
                if f.is_none() {
                    c.g.cur_face = None;
                }
            }
        }
        w = c.wall(w).next.ok_or(fault("open ring"))?;
        if w == ring {
            return Ok(count);
        }
    }
}

/// Second pass: the slab bottom moves up to the top of the faces at the
/// cursors (the bottom of active ones).
fn slab_floor(c: &mut Core) -> Result<(), CoreError> {
    let tol = c.g.tol;
    c.g.z_lo = c.g.lvl_lo;
    let ring = c.g.ring.ok_or(fault("no ring"))?;
    let mut w = ring;
    loop {
        let f = c.wall(w).cur;
        c.g.last_face = f;
        if let Some(fi) = f {
            let fc = c.face(fi);
            if fc.typ == b'D' || fc.typ == b'K' {
                let z = if c.wall(w).act.is_none() {
                    fc.zhi
                } else {
                    fc.zlo
                };
                if num::gt(z, c.g.z_lo, tol.value) {
                    c.g.z_lo = z;
                }
            }
        }
        w = c.wall(w).next.ok_or(fault("open ring"))?;
        if w == ring {
            return Ok(());
        }
    }
}

/// Second pass start: every roof face's range becomes the z range of its
/// outline so far.
fn face_ranges_down(c: &mut Core) -> Result<(), CoreError> {
    let tol = c.g.tol;
    let ring = c.g.ring.ok_or(fault("no ring"))?;
    let mut w = ring;
    loop {
        let hid = c.wall(w).faces.ok_or(fault("wall without faces"))?;
        let mut f = c.face(hid).next;
        while let Some(fi) = f {
            let mut lo = 100.0f32;
            let mut hi = -100.0f32;
            for s in close::segs_of(c, fi) {
                let sg = c.seg(s);
                if num::lt(sg.p[2], lo, tol.value) {
                    lo = sg.p[2];
                }
                if num::lt(sg.q[2], lo, tol.value) {
                    lo = sg.q[2];
                }
                if num::gt(sg.p[2], hi, tol.value) {
                    hi = sg.p[2];
                }
                if num::gt(sg.q[2], hi, tol.value) {
                    hi = sg.q[2];
                }
            }
            let fm = c.face_mut(fi);
            fm.zlo = lo;
            fm.zhi = hi;
            f = fm.next;
        }
        w = c.wall(w).next.ok_or(fault("open ring"))?;
        if w == ring {
            return Ok(());
        }
    }
}

/// A 'w' model whose walls all have their first roof face closed.
fn all_done(c: &Core) -> Result<bool, CoreError> {
    let ring = c.g.ring.ok_or(fault("no ring"))?;
    if c.wall(ring).typ != b'w' {
        return Ok(false);
    }
    let mut w = ring;
    for _ in 0..c.g.counters[ctr::RING_WALLS].max(0) {
        let hid = c.wall(w).faces.ok_or(fault("wall without faces"))?;
        let f = c.face(hid).next.ok_or(fault("wall without roof face"))?;
        if c.face(f).closed == 0 {
            return Ok(false);
        }
        w = c.wall(w).next.ok_or(fault("open ring"))?;
    }
    Ok(true)
}

/// How the active face of the wall `w2` relates to its left neighbour
/// `w1` (which face of `w1` it is cut with): 0 the active face, 1 the
/// hidden wall face, 2 parallel-opposite walls, -1 the eave face.
fn order_left(c: &Core, w1: WallId, w2: WallId) -> Result<i32, CoreError> {
    order(c, w1, w2, false)
}

/// The mirror of [`order_left`] for the right neighbour `w2` of `w1`.
fn order_right(c: &Core, w1: WallId, w2: WallId) -> Result<i32, CoreError> {
    order(c, w1, w2, true)
}

fn order(c: &Core, w1: WallId, w2: WallId, right: bool) -> Result<i32, CoreError> {
    let tol = c.g.tol;
    let hid = |w: WallId| c.wall(w).faces.ok_or(fault("wall without faces"));
    let first = |w: WallId| -> Result<FaceId, CoreError> {
        c.face(hid(w)?).next.ok_or(fault("wall without roof face"))
    };
    let h1 = c.face(first(w1)?).h;
    let h2 = c.face(first(w2)?).h;
    if num::eq(h1, h2, tol.value) {
        return Ok(0);
    }
    let lower = if right {
        num::lt(h2, h1, tol.value)
    } else {
        num::lt(h1, h2, tol.value)
    };
    if lower {
        return Ok(0);
    }
    let (f1, f2) = (c.face(hid(w1)?).clone(), c.face(hid(w2)?).clone());
    match num::par(f1.w, f2.w, &tol) {
        1 => return Ok(0),
        -1 => return Ok(2),
        _ => {}
    }
    let q = num::add(f1.o, f1.w);
    let r = num::add(f2.o, num::scale(f2.w, 100.0));
    let mut o = num::orient(f1.o, q, r, &tol);
    if o == -1 {
        let h = if right { h2 } else { h1 };
        if num::ge(c.g.z_lo, h, tol.value) {
            o = 0;
        }
    } else if o == 1 {
        let wh = if right { c.wall(w2).h } else { c.wall(w1).h };
        if num::ge(c.g.z_lo, wh, tol.value) {
            return Ok(0);
        }
    }
    Ok(o)
}
