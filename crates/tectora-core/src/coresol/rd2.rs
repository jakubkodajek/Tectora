//! The RD2 text (the core's input) and the state the core derives from it
//! before cutting: the wall ring of each model and the frame of each face.

use super::ParseError;
use super::num::{
    D2R, EPS, EPS_ANG, EPS_CHAIN, HALF_PI_F, f32r, feq, fne, hypot, is0, scan_f32, scan_i32,
};

/// The parsed RD2 and the derived ring and frames.
#[derive(Debug, Clone, PartialEq)]
pub struct Rd2 {
    /// The leading number of the file (0).
    pub lead: i32,
    /// The models in the core's list order. The core prepends each new
    /// model to its list, so this is the reverse of the file order.
    pub models: Vec<Model>,
    /// The holes (`L` records), likewise in reverse file order.
    pub holes: Vec<Hole>,
}

/// A model: a run of wall records.
#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    /// `D` (roof, from `W` walls), `G` (dormer, from `T`), `g` (from `t`),
    /// `w` (wall model) or the letter of the first wall otherwise.
    pub kind: u8,
    /// The model number of the wall records.
    pub no: i32,
    /// The walls in file order.
    pub walls: Vec<Wall>,
    /// The wall ring after the chain sort and the collinear merge, from
    /// the ring's first wall on. Empty for `w` models, which the core
    /// does not sort.
    pub ring: Vec<RingWall>,
}

/// One wall of the ring.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RingWall {
    /// Index into [`Model::walls`].
    pub wall: usize,
    /// The end point; a wall that absorbed its collinear neighbours ends
    /// where the last of them ended.
    pub end: [f32; 2],
}

/// A wall record `c model wall x y len angle priority`.
#[derive(Debug, Clone, PartialEq)]
pub struct Wall {
    /// `W`, `T`, `G`, `w`, `t` or `g`.
    pub letter: u8,
    pub model_no: i32,
    pub no: i32,
    pub start: [f32; 2],
    pub len: f32,
    /// Direction in degrees.
    pub angle: f32,
    /// The last number of the record.
    pub priority: f32,
    /// The end point computed from start, length and angle.
    pub end: [f32; 2],
    /// The length is below 0.01: the wall is a point.
    pub zero: bool,
    /// The height of the last `D` record.
    pub height: f32,
    /// `faces[0]` is the hidden wall face; the `D`/`K` records follow.
    pub faces: Vec<Face>,
}

/// A face of a wall: the hidden wall face or a `D`/`K` record
/// `sub unused flag h ov pitch`.
#[derive(Debug, Clone, PartialEq)]
pub struct Face {
    /// `W` for the hidden wall face, else `D` or `K`.
    pub kind: u8,
    /// -1 for the hidden wall face.
    pub sub: i32,
    /// The second number of the record, unused by the core.
    pub unused: i32,
    /// The flag as read.
    pub flag: i32,
    /// The flag the core keeps: as read, or -1 for a dormer cheek.
    pub b: i32,
    pub h: f32,
    pub ov: f32,
    pub pitch_deg: f32,
    /// The pitch in radians, as the stored float.
    pub pitch: f32,
    /// Frame: origin, unit up-slope vector, unit eave direction.
    pub o: [f32; 3],
    pub u: [f32; 3],
    pub w: [f32; 3],
}

/// A hole record `L no h n` with its points.
#[derive(Debug, Clone, PartialEq)]
pub struct Hole {
    pub no: i32,
    pub h: f32,
    pub points: Vec<[f32; 2]>,
}

impl Model {
    /// The ring walls with their ring ends.
    pub fn ring_walls(&self) -> impl Iterator<Item = (&Wall, &RingWall)> {
        self.ring.iter().map(|r| (&self.walls[r.wall], r))
    }
}

impl Rd2 {
    /// Reads the RD2 text the way the core does.
    pub fn parse(text: &str) -> Result<Rd2, ParseError> {
        let mut rd2 = read(text)?;
        for m in &mut rd2.models {
            if m.kind == b'w' {
                continue;
            }
            m.ring = chain_and_merge(&m.walls)?;
            for w in &mut m.walls {
                for k in 1..w.faces.len() {
                    let (head, tail) = w.faces.split_at_mut(k);
                    frame(&mut tail[0], &head[k - 1]);
                }
            }
        }
        Ok(rd2)
    }

    /// The ring wall with this letter and number, first match in core
    /// order, with its model and ring position.
    pub fn find_wall(&self, letter: u8, no: i32) -> Option<(usize, usize)> {
        for (mi, m) in self.models.iter().enumerate() {
            if m.kind == b'w' {
                continue;
            }
            for (ri, r) in m.ring.iter().enumerate() {
                let w = &m.walls[r.wall];
                if w.letter == letter && w.no == no {
                    return Some((mi, ri));
                }
            }
        }
        None
    }
}

struct Toks<'a> {
    t: Vec<&'a str>,
    i: usize,
}

impl<'a> Toks<'a> {
    fn next(&mut self) -> Result<&'a str, ParseError> {
        let t = self.t.get(self.i).copied().ok_or(ParseError::Rd2(self.i))?;
        self.i += 1;
        Ok(t)
    }
    fn int(&mut self) -> Result<i32, ParseError> {
        let i = self.i;
        scan_i32(self.next()?).ok_or(ParseError::Rd2(i))
    }
    fn float(&mut self) -> Result<f32, ParseError> {
        let i = self.i;
        scan_f32(self.next()?).ok_or(ParseError::Rd2(i))
    }
}

fn read(text: &str) -> Result<Rd2, ParseError> {
    let mut s = Toks {
        t: text.split_ascii_whitespace().collect(),
        i: 0,
    };
    let lead = s.int()?;
    let mut models: Vec<Model> = Vec::new();
    let mut holes = Vec::new();
    let mut cur_no = 0xffff;
    let mut last_letter = b'?';
    // The height of the last D record of a 'T' wall.
    let mut t_height = 0.0f32;
    let mut have_wall = false;
    while s.i < s.t.len() {
        let at = s.i;
        let c = s.next()?;
        let c = match c.as_bytes() {
            [b] => *b,
            _ => return Err(ParseError::Rd2(at)),
        };
        match c {
            b'D' | b'K' => {
                let cur = models.last_mut().and_then(|m| m.walls.last_mut());
                let cur = match cur {
                    Some(w) if have_wall && w.letter != b'w' => w,
                    _ => {
                        for _ in 0..6 {
                            s.next()?;
                        }
                        continue;
                    }
                };
                let sub = s.int()?;
                let unused = s.int()?;
                let flag = s.int()?;
                let (h, ov, pdeg) = (s.float()?, s.float()?, s.float()?);
                if cur.letter == b'T' && c == b'D' {
                    t_height = h;
                }
                let mut b = flag;
                if cur.letter == b'G' && c == b'D' && flag == 0 {
                    let pd = pdeg as f64;
                    if feq(h as f64, t_height as f64, EPS)
                        && feq(ov as f64, 0.0, EPS)
                        && !(EPS <= f32r(pd - 90.0).abs() && 90.0 <= pd)
                    {
                        b = -1;
                    }
                }
                if c == b'D' {
                    cur.height = h;
                }
                cur.faces.push(Face {
                    kind: c,
                    sub,
                    unused,
                    flag,
                    b,
                    h,
                    ov,
                    pitch_deg: pdeg,
                    pitch: (pdeg as f64 * D2R) as f32,
                    o: [0.0; 3],
                    u: [0.0; 3],
                    w: [0.0; 3],
                });
            }
            b'G' | b'T' | b'W' | b'g' | b't' | b'w' => {
                let mno = s.int()?;
                let wno = s.int()?;
                let (x, y, len, a, prio) =
                    (s.float()?, s.float()?, s.float()?, s.float()?, s.float()?);
                let new_model = !(cur_no == mno
                    && c != b't'
                    && !(last_letter == b'W' && c == b'w')
                    && !(last_letter == b'w' && c == b'W'));
                let ar = a as f64 * D2R;
                let (xd, yd) = (x as f64, y as f64);
                let zero = is0(len as f64, EPS);
                let end = if zero {
                    [x, y]
                } else {
                    [
                        (ar.cos() * len as f64 + xd) as f32,
                        (ar.sin() * len as f64 + yd) as f32,
                    ]
                };
                let hidden = Face {
                    kind: b'W',
                    sub: -1,
                    unused: 0,
                    flag: 0,
                    b: 0,
                    h: 0.0,
                    ov: 0.0,
                    pitch_deg: 90.0,
                    pitch: HALF_PI_F as f32,
                    o: [x, y, 0.0],
                    u: [0.0; 3],
                    w: [
                        ((ar.cos() + xd) - xd) as f32,
                        ((ar.sin() + yd) - yd) as f32,
                        0.0,
                    ],
                };
                last_letter = c;
                if new_model {
                    cur_no = mno;
                    let kind = match c {
                        b'T' => b'G',
                        b'W' => b'D',
                        b't' => b'g',
                        other => other,
                    };
                    models.push(Model {
                        kind,
                        no: mno,
                        walls: Vec::new(),
                        ring: Vec::new(),
                    });
                }
                let m = models.last_mut().ok_or(ParseError::Rd2(at))?;
                m.walls.push(Wall {
                    letter: c,
                    model_no: mno,
                    no: wno,
                    start: [x, y],
                    len,
                    angle: a,
                    priority: prio,
                    end,
                    zero,
                    height: 0.0,
                    faces: vec![hidden],
                });
                have_wall = true;
            }
            b'L' => {
                let no = s.int()?;
                let h = s.float()?;
                let n = s.int()?;
                let mut points = Vec::with_capacity(n.max(0) as usize);
                for _ in 0..n.max(0) {
                    points.push([s.float()?, s.float()?]);
                }
                holes.push(Hole { no, h, points });
            }
            _ => return Err(ParseError::Rd2(at)),
        }
    }
    models.reverse();
    holes.reverse();
    Ok(Rd2 {
        lead,
        models,
        holes,
    })
}

/// The wall chain sort and the collinear merge of one model's ring.
fn chain_and_merge(ws: &[Wall]) -> Result<Vec<RingWall>, ParseError> {
    let total = ws.len();
    if total == 0 {
        return Ok(Vec::new());
    }
    let mut next: Vec<usize> = (0..total).map(|k| (k + 1) % total).collect();
    let mut prev: Vec<usize> = (0..total).map(|k| (k + total - 1) % total).collect();
    let mut visited = vec![false; total];
    let mut end: Vec<[f64; 2]> = ws
        .iter()
        .map(|w| [w.end[0] as f64, w.end[1] as f64])
        .collect();
    let st = |k: usize| [ws[k].start[0] as f64, ws[k].start[1] as f64];
    let mut n = total as i64;
    let mut first = 0usize;
    let mut saved_t = None;
    if matches!(ws[first].letter, b'T' | b't') {
        n -= 1;
        saved_t = Some(first);
        let (p, q) = (prev[first], next[first]);
        next[p] = q;
        prev[q] = p;
    }
    let move_before = |next: &mut Vec<usize>, prev: &mut Vec<usize>, c: usize, cur: usize| {
        let (cp, cn) = (prev[c], next[c]);
        prev[cn] = cp;
        next[cp] = cn;
        let p = prev[cur];
        next[p] = c;
        prev[c] = p;
        prev[cur] = c;
        next[c] = cur;
    };
    let move_after = |next: &mut Vec<usize>, prev: &mut Vec<usize>, c: usize, cur: usize| {
        let (cp, cn) = (prev[c], next[c]);
        next[cp] = cn;
        prev[cn] = cp;
        let q = next[cur];
        prev[q] = c;
        next[c] = q;
        next[cur] = c;
        prev[c] = cur;
    };
    // Point tests of the sort: a zero-length candidate, or a real one,
    // whose end meets `p`.
    let meets = |e: [f64; 2], p: [f64; 2]| feq(p[0], e[0], EPS_CHAIN) && feq(p[1], e[1], EPS_CHAIN);
    let is_point =
        |s: [f64; 2], e: [f64; 2]| feq(s[0], e[0], EPS_CHAIN) && feq(s[1], e[1], EPS_CHAIN);
    let is_real =
        |s: [f64; 2], e: [f64; 2]| fne(s[0], e[0], EPS_CHAIN) || fne(s[1], e[1], EPS_CHAIN);

    let mut cur = next[first];
    let mut i5: i64 = 0;
    let mut forward = false;
    if n > 0 {
        loop {
            visited[cur] = true;
            let mut found = None;
            let mut cand = prev[cur];
            for i8 in 0..(n - 1).max(0) {
                if (!visited[cand] || i8 == 0 || i5 == n - 1)
                    && meets(end[cand], st(cur))
                    && is_point(st(cand), end[cand])
                {
                    found = Some(cand);
                    break;
                }
                cand = prev[cand];
            }
            if found.is_none() {
                let mut cand = prev[cur];
                let mut i3 = 0;
                loop {
                    if n - 1 <= i3 {
                        if saved_t.is_some() {
                            forward = true;
                            break;
                        }
                        return Err(ParseError::WallChain(ws[cur].model_no));
                    }
                    if (!visited[cand] || i3 == 0 || i5 == n - 1)
                        && meets(end[cand], st(cur))
                        && is_real(st(cand), end[cand])
                    {
                        found = Some(cand);
                        break;
                    }
                    cand = prev[cand];
                    i3 += 1;
                }
            }
            if forward {
                break;
            }
            let f = found.ok_or(ParseError::WallChain(ws[cur].model_no))?;
            if prev[cur] != f {
                move_before(&mut next, &mut prev, f, cur);
            }
            cur = prev[cur];
            i5 += 1;
            if n <= i5 {
                break;
            }
        }
    }
    if let Some(t) = saved_t {
        // Forward chain from the wall where the backward chain broke,
        // then the T wall goes back in after the last wall.
        if forward {
            let mut i3 = 0;
            while i3 < n - 1 {
                let mut found = None;
                let mut cand = next[cur];
                for _ in 0..n - 1 {
                    if meets(st(cand), end[cur]) && is_point(st(cand), end[cand]) {
                        found = Some(cand);
                        break;
                    }
                    cand = next[cand];
                }
                if found.is_none() {
                    let mut cand = next[cur];
                    for _ in 0..n - 1 {
                        if meets(st(cand), end[cur]) && is_real(st(cand), end[cand]) {
                            found = Some(cand);
                            break;
                        }
                        cand = next[cand];
                    }
                }
                let f = found.ok_or(ParseError::WallChain(ws[cur].model_no))?;
                if next[cur] != f {
                    move_after(&mut next, &mut prev, f, cur);
                }
                cur = next[cur];
                i3 += 1;
            }
        }
        let nx = next[cur];
        next[t] = nx;
        prev[t] = cur;
        prev[nx] = t;
        next[cur] = t;
        n += 1;
    } else {
        // The merge pass.
        let mut p = first;
        let mut i = 0;
        while i < n {
            let q = next[p];
            let mut base = p;
            if !ws[p].zero && !ws[q].zero {
                let (fp, fq) = (ws[p].faces.get(1), ws[q].faces.get(1));
                let ps = st(p);
                let d1 = [f32r(end[p][0] - ps[0]), f32r(end[p][1] - ps[1]), 0.0];
                let d2 = [f32r(end[q][0] - ps[0]), f32r(end[q][1] - ps[1]), 0.0];
                let qs = st(q);
                let d3 = [f32r(end[q][0] - qs[0]), f32r(end[q][1] - qs[1]), 0.0];
                if let (Some(fp), Some(fq)) = (fp, fq)
                    && par(d1, d2) != 0
                    && par(d1, d3) != 0
                    && feq(fp.h as f64, fq.h as f64, EPS)
                    && feq(fp.ov as f64, fq.ov as f64, EPS)
                    && fp.b == fq.b
                {
                    let qn = next[q];
                    next[p] = qn;
                    prev[qn] = p;
                    n -= 1;
                    i -= 1;
                    if q == first {
                        first = p;
                    }
                    end[p] = end[q];
                    base = prev[p];
                }
            }
            p = next[base];
            i += 1;
        }
    }
    let mut ring = Vec::with_capacity(n.max(0) as usize);
    let mut w = first;
    for _ in 0..n.max(0) {
        ring.push(RingWall {
            wall: w,
            end: [end[w][0] as f32, end[w][1] as f32],
        });
        w = next[w];
    }
    Ok(ring)
}

/// The core's normalisation: zero below a length of 0.01.
pub(crate) fn norm3(v: [f64; 3]) -> [f64; 3] {
    let l = f32r((v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt());
    if is0(l, EPS) {
        return [0.0; 3];
    }
    [f32r(v[0] / l), f32r(v[1] / l), f32r(v[2] / l)]
}

/// Parallel test of the core: +1 same direction, -1 opposite, 0 neither.
fn par(a: [f64; 3], b: [f64; 3]) -> i32 {
    let (a, b) = (norm3(a), norm3(b));
    if (0..3).all(|k| feq(a[k], b[k], EPS)) {
        return 1;
    }
    if (0..3).all(|k| feq(-a[k], b[k], EPS)) {
        return -1;
    }
    0
}

/// The rise for the run `s` at the face's pitch; negative beyond 90°.
fn rise(p: f64, s: f64) -> f64 {
    if feq(p, 0.0, EPS_ANG) || feq(p, HALF_PI_F, EPS_ANG) {
        return 0.0;
    }
    let sign = if EPS_ANG <= f32r(p - HALF_PI_F).abs() && HALF_PI_F < p {
        -1.0
    } else {
        1.0
    };
    let q = s / p.cos();
    sign * (q * q - s * s).sqrt()
}

/// The frame of face `f` from its parent `pa` (the wall's previous face).
fn frame(f: &mut Face, pa: &Face) {
    let pw = [pa.w[0] as f64, pa.w[1] as f64];
    let pp = pa.pitch as f64;
    let (mut x0, mut y0) = (pa.o[0] as f64, pa.o[1] as f64);
    let mut x1 = f32r(x0 + pw[0]);
    let mut y1 = f32r(y0 + pw[1]);
    let mut o = [0.0f64; 3];
    if f.kind == b'K' {
        if fne(pp, HALF_PI_F, EPS_ANG) {
            let t = (f.h as f64 - pa.h as f64) / pp.tan();
            let t = t / hypot(pw[1], pw[0]);
            let ox = f32r(x0 - t * pw[1]);
            let oy = f32r(t * pw[0] + y0);
            x0 = ox;
            y0 = oy;
            x1 = f32r(x0 + pw[0]);
            y1 = f32r(y0 + pw[1]);
            o = [ox, oy, 0.0];
        } else {
            o = [pa.o[0] as f64, pa.o[1] as f64, pa.o[2] as f64];
        }
    } else {
        let dyf = f32r(y1 - y0);
        let k = f.ov as f64 / hypot(x1 - x0, y1 - y0);
        o[0] = f32r(dyf * k + x0);
        o[1] = f32r((x0 - x1) * k + y0);
    }
    o[2] = f.h as f64;
    let p = f.pitch as f64;
    let u = if feq(p, HALF_PI_F, EPS_ANG) {
        [0.0, 0.0, 1.0]
    } else if is0(f.ov as f64, EPS) {
        let dxf = f32r(x1 - x0);
        let k = 1.0 / hypot(dxf, y1 - y0);
        [f32r((y0 - y1) * k), f32r(dxf * k), f32r(rise(p, 1.0))]
    } else {
        [f32r(x0 - o[0]), f32r(y0 - o[1]), f32r(rise(p, f.ov as f64))]
    };
    let u = norm3(u);
    let w = norm3([pw[0], pw[1], pa.w[2] as f64]);
    f.o = [o[0] as f32, o[1] as f32, o[2] as f32];
    f.u = [u[0] as f32, u[1] as f32, u[2] as f32];
    f.w = [w[0] as f32, w[1] as f32, w[2] as f32];
}
