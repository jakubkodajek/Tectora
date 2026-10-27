//! S1: the RD2 reader, building the arena exactly as the core does.
//!
//! The core reads the file with `fscanf` in text mode, one `%c` at a time
//! for the record letter; every character that is not a record letter
//! (digits of skipped records, blanks, line ends) is read and ignored.
//! [`coresol::Rd2`](crate::coresol::Rd2) reads the same file token-wise
//! into plain records; this reader is the arena port of the original loop
//! (counters, list prepends, the cursor globals, the D-record's copy of the
//! previous hidden face direction, `;` comments, x87 `fcos`/`fsin`).

use super::error::*;
use super::num::{self, DEG2RAD, HALF_PI_F, ZERO3};
use super::state::*;
use super::x87;

/// `fscanf` over a text-mode stream.
pub struct Scanner {
    buf: Vec<u8>,
    pos: usize,
}

fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

impl Scanner {
    /// The stream as a text-mode read sees it: CR LF becomes LF and a ^Z ends the file.
    pub fn new(bytes: &[u8]) -> Scanner {
        let mut buf = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            let c = bytes[i];
            if c == 0x1a {
                break;
            }
            if c == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
                i += 1;
                continue;
            }
            buf.push(c);
            i += 1;
        }
        Scanner { buf, pos: 0 }
    }

    /// `fgetc`.
    pub fn getc(&mut self) -> Option<u8> {
        let c = self.buf.get(self.pos).copied();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    fn peek(&self) -> Option<u8> {
        self.buf.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while self.peek().is_some_and(is_space) {
            self.pos += 1;
        }
    }

    /// `%c`.
    pub fn char(&mut self) -> Option<u8> {
        self.getc()
    }

    /// `%d`: `Err(true)` on end of input before the field, `Err(false)` on a mismatch.
    fn int(&mut self) -> Result<i32, bool> {
        self.skip_ws();
        let start = self.pos;
        if self.peek().is_none() {
            return Err(true);
        }
        let mut neg = false;
        if let Some(c @ (b'+' | b'-')) = self.peek() {
            neg = c == b'-';
            self.pos += 1;
        }
        let mut v: i64 = 0;
        let mut any = false;
        while let Some(c) = self.peek().filter(u8::is_ascii_digit) {
            v = v.wrapping_mul(10).wrapping_add(i64::from(c - b'0'));
            any = true;
            self.pos += 1;
        }
        if !any {
            self.pos = start;
            return Err(false);
        }
        Ok(if neg { v.wrapping_neg() } else { v } as i32)
    }

    /// `%f` into a float.
    fn float(&mut self) -> Result<f32, bool> {
        self.skip_ws();
        if self.peek().is_none() {
            return Err(true);
        }
        let start = self.pos;
        let mut s = String::new();
        if let Some(c @ (b'+' | b'-')) = self.peek() {
            s.push(c as char);
            self.pos += 1;
        }
        let mut digits = 0;
        while let Some(c) = self.peek().filter(u8::is_ascii_digit) {
            s.push(c as char);
            digits += 1;
            self.pos += 1;
        }
        if self.peek() == Some(b'.') {
            s.push('.');
            self.pos += 1;
            while let Some(c) = self.peek().filter(u8::is_ascii_digit) {
                s.push(c as char);
                digits += 1;
                self.pos += 1;
            }
        }
        if digits == 0 {
            self.pos = start;
            return Err(false);
        }
        if let Some(b'e' | b'E') = self.peek() {
            let save = self.pos;
            let mut e = String::from("e");
            self.pos += 1;
            if let Some(c @ (b'+' | b'-')) = self.peek() {
                e.push(c as char);
                self.pos += 1;
            }
            let mut ed = 0;
            while let Some(c) = self.peek().filter(u8::is_ascii_digit) {
                e.push(c as char);
                ed += 1;
                self.pos += 1;
            }
            if ed > 0 {
                s.push_str(&e);
            } else {
                self.pos = save;
            }
        }
        // Correctly rounded, the sign of a zero kept.
        Ok(s.parse().unwrap_or(0.0))
    }
}

/// One `fscanf` call with a format of `%d`/`%f` fields: the number of
/// fields assigned, or -1 for end of input before the first.
macro_rules! scan {
    ($sc:expr; $($kind:ident $var:expr),+) => {{
        let mut n: i32 = 0;
        let mut stop = false;
        $(
            if !stop {
                match $sc.$kind() {
                    Ok(v) => { $var = v; n += 1; }
                    Err(eof) => { stop = true; if eof && n == 0 { n = -1; } }
                }
            }
        )+
        let _ = stop;
        n
    }};
}

/// Reads an RD2 into the state: models (prepended: the last model of the
/// file first), walls, hidden wall faces, roof faces, holes.
pub fn read(c: &mut Core, bytes: &[u8]) -> Result<(), CoreError> {
    let mut sc = Scanner::new(bytes);
    let tol = c.g.tol;
    c.g.last_model_no = 0xffff;
    let mut last_letter = b'?';
    let mut prev_type = b'?';
    let mut header: i32 = 0;
    if scan!(sc; int header) != 1 {
        return Err(CoreError::fatal(1, MSG_FORMAT));
    }
    // Locals of the original that keep their values from record to record.
    // Those read before any assignment are uninitialised there; 0 here.
    let (mut sub, mut unused, mut flag) = (0i32, 0i32, 0i32);
    let (mut h, mut ov, mut pdeg) = (0f32, 0f32, 0f32);
    let mut t_height = 0f32;
    let (mut mno, mut wno) = (0i32, 0i32);
    let (mut x, mut y, mut len, mut extra) = (0f32, 0f32, 0f32, 0f32);
    // The direction of the last hidden wall face (copied into roof faces).
    let mut hidden_w: num::V3 = ZERO3;
    while let Some(ch) = sc.char() {
        match ch {
            b';' => {
                while let Some(k) = sc.getc() {
                    if k == b'\n' {
                        break;
                    }
                }
            }
            b'D' | b'K' => {
                if last_letter == b'w' {
                    continue;
                }
                if scan!(sc; int sub, int unused, int flag, float h, float ov, float pdeg) != 6 {
                    return Err(CoreError::fatal(1, MSG_ROOF_DEF));
                }
                let _ = unused;
                let lw =
                    c.g.last_wall
                        .ok_or(CoreError::Fault("roof record before any wall"))?;
                let wt = c.wall(lw).typ;
                if wt == b'T' && ch == b'D' {
                    t_height = h;
                }
                if wt == b'G' && ch == b'D' {
                    if flag == 0
                        && num::eq(h, t_height, tol.value)
                        && num::eq(ov, 0.0, tol.value)
                        && num::le(pdeg, 90.0, tol.value)
                    {
                        flag = -1;
                    }
                    c.wall_mut(lw).h = h;
                } else if ch == b'D' {
                    c.wall_mut(lw).h = h;
                }
                let lf =
                    c.g.last_face
                        .ok_or(CoreError::Fault("roof record before any wall"))?;
                let f = c.new_face(NewFace {
                    typ: ch,
                    sub,
                    o: ZERO3,
                    u: ZERO3,
                    w: hidden_w,
                    pitch: (f64::from(pdeg) * DEG2RAD) as f32,
                    h,
                    ov,
                    b: flag,
                    owner: Some(lw),
                    prev: Some(lf),
                    next: None,
                });
                c.g.cur_face = Some(f);
                c.face_mut(lf).next = Some(f);
                c.g.last_face = Some(f);
                c.g.counters[if ch == b'D' { ctr::D_READ } else { ctr::K_READ }] += 1;
            }
            b'G' | b'T' | b'W' | b'g' | b't' | b'w' => {
                last_letter = ch;
                if ch == b'w' {
                    continue;
                }
                if ch == b'T' {
                    c.g.n_t_models += 1;
                }
                if header == 0
                    && scan!(sc; int mno, int wno, float x, float y, float len, float pdeg, float extra)
                        != 7
                {
                    return Err(CoreError::fatal(1, MSG_WALL_DEF));
                }
                if c.g.flags & 0x200 != 0 {
                    extra = 0.0;
                }
                let angle = pdeg;
                let same = c.g.last_model_no == mno
                    && ch != b't'
                    && !(prev_type == b'W' && ch == b'w')
                    && !(prev_type == b'w' && ch == b'W');
                if !same {
                    c.save_counters_to_head();
                    c.reset_model();
                }
                let start = [x, y, 0.0];
                let a = f64::from(angle) * DEG2RAD;
                let end = if !num::zero(len, tol.value) {
                    [
                        (x87::fcos(a).fmul(f64::from(len)) + f64::from(x)) as f32,
                        (x87::fsin(a).fmul(f64::from(len)) + f64::from(y)) as f32,
                        0.0,
                    ]
                } else {
                    start
                };
                hidden_w = [
                    (x87::fcos(a).fadd(f64::from(x)) - f64::from(x)) as f32,
                    (x87::fsin(a).fadd(f64::from(y)) - f64::from(y)) as f32,
                    0.0,
                ];
                let hid = c.new_face(NewFace {
                    typ: b'W',
                    sub: -1,
                    o: start,
                    u: ZERO3,
                    w: hidden_w,
                    pitch: HALF_PI_F,
                    h: 0.0,
                    ov: 0.0,
                    b: 0,
                    owner: None,
                    prev: None,
                    next: None,
                });
                c.g.last_hidden = Some(hid);
                c.g.cur_face = Some(hid);
                c.g.last_face = Some(hid);
                let w = c.new_wall(ch, wno, extra, hid, c.g.last_wall, c.g.ring);
                c.g.cur_wall = Some(w);
                {
                    let wr = c.wall_mut(w);
                    if num::zero(len, tol.value) {
                        wr.zero = 1;
                        wr.s0 = start;
                    }
                    wr.s = start;
                    wr.e = end;
                    wr.s_orig = start;
                    wr.e_orig = end;
                }
                match c.g.last_wall {
                    None => {
                        c.g.ring = Some(w);
                        c.g.last_wall = Some(w);
                    }
                    Some(lw) => {
                        c.wall_mut(lw).next = Some(w);
                        c.g.last_wall = Some(w);
                        let r = c.g.ring.ok_or(CoreError::Fault("no ring"))?;
                        c.wall_mut(r).prev = Some(w);
                    }
                }
                c.face_mut(hid).owner = c.g.last_wall;
                prev_type = ch;
                if !same {
                    c.g.last_model_no = mno;
                    let typ = match ch {
                        b'T' => b'G',
                        b'W' => b'D',
                        b't' => b'g',
                        o => o,
                    };
                    let m = c.new_model(typ, mno, w, c.g.models);
                    c.g.cur_model = Some(m);
                    c.g.models = Some(m);
                }
                c.g.counters[ctr::WALLS_READ] += 1;
            }
            b'L' => {
                let (mut no, mut hh, mut n) = (0i32, 0f32, 0i32);
                if scan!(sc; int no, float hh, int n) != 3 {
                    return Err(CoreError::fatal(1, MSG_HOLE_DEF));
                }
                let facep = if num::zero(hh, tol.value) {
                    SFACE1
                } else {
                    SFACE2
                };
                c.g.hole_chain = None;
                let (mut px, mut py) = (0f32, 0f32);
                if scan!(sc; float px, float py) != 2 {
                    return Err(CoreError::fatal(1, MSG_L_ERROR));
                }
                let first = [px, py, 0.0];
                let mut prev = first;
                let mut k = (n as u32).wrapping_sub(1);
                while k != 0 {
                    if scan!(sc; float px, float py) != 2 {
                        return Err(CoreError::fatal(1, MSG_L_ERROR));
                    }
                    let p = [px, py, 0.0];
                    let s = c.new_seg(b'L', 0, p, prev, None, c.g.hole_chain, Some(facep), None);
                    c.g.hole_chain = Some(s);
                    c.g.seg_cur = Some(s);
                    prev = p;
                    k -= 1;
                }
                let s = c.new_seg(
                    b'L',
                    0,
                    first,
                    prev,
                    None,
                    c.g.hole_chain,
                    Some(facep),
                    None,
                );
                c.g.seg_cur = Some(s);
                let node = c.new_node(no, 0.0, Some(s), c.g.holes);
                c.node_mut(node).x04 = hh.to_bits();
                c.g.hole_cur = Some(node);
                c.g.n_holes += 1;
                c.g.holes = Some(node);
            }
            _ => {}
        }
    }
    c.save_counters_to_head();
    Ok(())
}
