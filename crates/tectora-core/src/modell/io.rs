//! Reading the INT (and W3D) text and writing the SOL text.

use std::fmt::Write as _;

use super::Tol;
use super::plane::Plane;
use super::poly::{area, snap};
use super::types::{Edge, Face, Poly, Srf, State};
use super::vec::{V3, veq};

/// Why an input could not be read.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ReadError {
    #[error("INT: unexpected end or token at token {0}")]
    Int(usize),
    #[error("W3D: unexpected end or token at token {0}")]
    W3d(usize),
}

/// A whitespace token reader with fscanf-like conversions.
struct Scan<'a> {
    toks: Vec<&'a str>,
    pos: usize,
}

impl<'a> Scan<'a> {
    fn new(text: &'a str) -> Self {
        Scan {
            toks: text.split_ascii_whitespace().collect(),
            pos: 0,
        }
    }

    fn word(&mut self) -> Option<&'a str> {
        let t = self.toks.get(self.pos).copied();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn int(&mut self) -> Option<i32> {
        let t = self.toks.get(self.pos)?;
        let v = t.parse::<i64>().ok()? as i32;
        self.pos += 1;
        Some(v)
    }

    /// A "%f" read: the value is stored as a float32 and used widened.
    fn float(&mut self) -> Option<f64> {
        let t = self.toks.get(self.pos)?;
        let v = t.parse::<f64>().ok()? as f32 as f64;
        self.pos += 1;
        Some(v)
    }

    fn vec(&mut self) -> Option<V3> {
        Some(V3::new(self.float()?, self.float()?, self.float()?))
    }
}

struct Header {
    letter: u8,
    rec: Srf,
}

fn header(s: &mut Scan) -> Option<Header> {
    let letter = *s.word()?.as_bytes().first()?;
    let (i1, i2, i3, i4, i5) = (s.int()?, s.int()?, s.int()?, s.int()?, s.int()?);
    let (o, u, w) = (s.vec()?, s.vec()?, s.vec()?);
    let angle = s.float()?;
    Some(Header {
        letter,
        rec: Srf {
            kind: letter as i32,
            model: i1,
            face: i2,
            sub: i3,
            flag5: i5,
            flag4: i4,
            angle,
            zmin: 0.0,
            plane: Plane::new(o, u, w),
        },
    })
}

fn min_z(p: &Poly) -> f64 {
    let mut z = p.edges[0].p1.z;
    for e in &p.edges {
        if e.p1.z < z {
            z = e.p1.z;
        }
        if e.p2.z < z {
            z = e.p2.z;
        }
    }
    z
}

/// Reads the INT file and, when given, the W3D wall file.
pub fn read(int: &str, w3d: Option<&str>, tol: &Tol) -> Result<State, ReadError> {
    let mut st = State::default();
    let mut s = Scan::new(int);
    let err = |s: &Scan| ReadError::Int(s.pos);
    // The edge type is kept from record to record unless a record sets it.
    let mut kind = b'?';
    // The model number of the last model or face record; walls inherit it.
    let mut last_model = 0;

    s.int().ok_or_else(|| err(&s))?;
    s.word().ok_or_else(|| err(&s))?;
    let n_models = s.int().ok_or_else(|| err(&s))?;
    s.word().ok_or_else(|| err(&s))?;
    st.n_models = n_models;
    for _ in 0..n_models.max(0) {
        let h = header(&mut s).ok_or_else(|| err(&s))?;
        last_model = h.rec.model;
        let n = s.int().ok_or_else(|| err(&s))?;
        s.word().ok_or_else(|| err(&s))?;
        match h.letter {
            b'G' | b'I' => kind = b'w',
            b'O' => kind = b'd',
            _ => {}
        }
        let mut poly = Poly {
            plane: h.rec.plane,
            ..Poly::default()
        };
        for _ in 0..n.max(0) {
            let a = s.vec().ok_or_else(|| err(&s))?;
            let b = s.vec().ok_or_else(|| err(&s))?;
            poly.edges.push(Edge::new(a.flat(), b.flat(), kind));
        }
        if poly.is_empty() {
            continue;
        }
        snap(&mut poly, tol.grid_int);
        let face = Face::new(h.rec, poly);
        match h.letter {
            b'G' | b'I' => st.models.push(face),
            b'O' => st.omodels.push(face),
            _ => {}
        }
    }

    let n_faces = s.int().ok_or_else(|| err(&s))?;
    s.word().ok_or_else(|| err(&s))?;
    st.n_faces = n_faces;
    for _ in 0..n_faces.max(0) {
        let mut h = header(&mut s).ok_or_else(|| err(&s))?;
        last_model = h.rec.model;
        let n = s.int().ok_or_else(|| err(&s))?;
        s.word().ok_or_else(|| err(&s))?;
        let mut poly = Poly {
            plane: h.rec.plane,
            ..Poly::default()
        };
        for _ in 0..n.max(0) {
            let a = s.vec().ok_or_else(|| err(&s))?;
            let b = s.vec().ok_or_else(|| err(&s))?;
            let flag = s.int().ok_or_else(|| err(&s))?;
            kind = if flag != 0 { b'W' } else { b'd' };
            poly.edges.push(Edge::new(a, b, kind));
        }
        if poly.is_empty() {
            continue;
        }
        snap(&mut poly, tol.grid_int);
        h.rec.zmin = min_z(&poly);
        st.faces.push(Face::new(h.rec, poly));
    }

    let n_holes = s.int().unwrap_or(0);
    st.n_holes = n_holes;
    if n_holes > 0 {
        s.word().ok_or_else(|| err(&s))?;
    }
    for _ in 0..n_holes.max(0) {
        let letter = *s
            .word()
            .and_then(|w| w.as_bytes().first())
            .ok_or_else(|| err(&s))?;
        let k = s.int().ok_or_else(|| err(&s))?;
        // The height is read and then replaced: every hole sits at z = 100.
        s.float().ok_or_else(|| err(&s))?;
        let h = 100.0;
        let n = s.int().ok_or_else(|| err(&s))?;
        s.word().ok_or_else(|| err(&s))?;
        let rec = Srf {
            kind: letter as i32,
            model: -1,
            face: 0,
            sub: k,
            plane: Plane::new(
                V3::new(0.0, 0.0, h),
                V3::new(1.0, 0.0, 0.0),
                V3::new(0.0, -1.0, 0.0),
            ),
            ..Srf::default()
        };
        match letter {
            b'L' => kind = b'L',
            b'T' => kind = b'T',
            _ => {}
        }
        let mut poly = Poly {
            plane: rec.plane,
            ..Poly::default()
        };
        for _ in 0..n.max(0) {
            let (ax, ay) = (
                s.float().ok_or_else(|| err(&s))?,
                s.float().ok_or_else(|| err(&s))?,
            );
            let (bx, by) = (
                s.float().ok_or_else(|| err(&s))?,
                s.float().ok_or_else(|| err(&s))?,
            );
            poly.edges
                .push(Edge::new(V3::new(ax, ay, h), V3::new(bx, by, h), kind));
        }
        if poly.is_empty() {
            continue;
        }
        snap(&mut poly, tol.grid_int);
        st.holes.push(Face::new(rec, poly));
    }

    if let Some(w3d) = w3d {
        let mut s = Scan::new(w3d);
        loop {
            let start = s.pos;
            let Some(letter) = s.word().and_then(|w| w.as_bytes().first().copied()) else {
                break;
            };
            let (Some(j1), Some(j2), Some(_)) = (s.int(), s.int(), s.int()) else {
                s.pos = start;
                break;
            };
            let n = s.int().ok_or(ReadError::W3d(s.pos))?;
            let mut poly = Poly::default();
            for _ in 0..n.max(0) {
                let a = s.vec().ok_or(ReadError::W3d(s.pos))?;
                let b = s.vec().ok_or(ReadError::W3d(s.pos))?;
                poly.edges.push(Edge::new(a, b, b'w'));
            }
            if poly.is_empty() {
                continue;
            }
            snap(&mut poly, tol.grid_w3d);
            let (o, u, w) = fit_frame(&poly);
            let mut rec = Srf {
                kind: letter as i32,
                model: last_model,
                face: j1,
                sub: j2,
                plane: Plane::new(o, u, w),
                ..Srf::default()
            };
            poly.plane = rec.plane;
            rec.zmin = min_z(&poly);
            st.walls.push(Face::new(rec, poly));
        }
    }
    Ok(st)
}

/// A frame for a wall polygon: the start of its longest edge, that edge,
/// and the vertex farthest from the edge's line.
fn fit_frame(p: &Poly) -> (V3, V3, V3) {
    let (mut o, mut u, mut w) = (V3::ZERO, V3::ZERO, V3::ZERO);
    if p.len() < 3 {
        return (o, u, w);
    }
    let mut best = 0.0;
    for e in &p.edges {
        let l = e.len();
        if best < l {
            o = e.p1;
            u = e.dir();
            best = l;
        }
    }
    let mut best = 0.0;
    for e in &p.edges {
        let l = e.p1.sub(o).cross(u).length();
        if best < l {
            best = l;
            w = e.p1.sub(o);
        }
    }
    (o, u, w)
}

pub(super) fn cut_kind(kind: i32) -> i32 {
    match kind as u8 {
        b'L' => 1,
        b'T' => 2,
        b'G' => 3,
        b'D' => 4,
        _ => 0,
    }
}

/// Writes the SOL text (with CRLF line ends, as DACAD's text-mode file).
pub fn write(st: &State, tol: &Tol) -> String {
    let mut o = String::new();
    o.push_str("0\n\n");
    let _ = write!(o, "{}\n\n", st.lines.len());
    for (i, l) in st.lines.iter().enumerate() {
        let len = l.p2.sub(l.p1).length() * l.sign as f64;
        let _ = write!(
            o,
            "\t{:05} {:10.5} {:10.5} {:10.5}\n\t      {:10.5} {:10.5} {:10.5}\t      {:02} {:10.5}\n\n",
            i + 1,
            l.p1.x,
            l.p1.y,
            l.p1.z,
            l.p2.x,
            l.p2.y,
            l.p2.z,
            l.code,
            len
        );
    }
    let roof: usize = st
        .faces
        .iter()
        .map(|f| f.pieces.len() + f.cutters.len())
        .sum();
    // Walls count once each, whatever their polygon lists hold.
    let _ = write!(o, "{}\n\n", roof + st.walls.len());
    let face = |o: &mut String, f: &Face, k: i32, a: f64, q: &Poly| {
        let _ = write!(
            o,
            "\t{:03} {:03} {:03} {:10.5}\n\n",
            f.rec.face, f.rec.sub, k, a
        );
        let _ = write!(o, "\t\t{:05}\n\n", q.len());
        for e in &q.edges {
            let s = match st.lines.get(e.line as usize) {
                Some(l) if veq(e.p1, l.p1, tol.t01) => 1,
                _ => -1,
            };
            let _ = writeln!(o, "\t\t{:05}", s * (e.line + 1));
        }
        o.push('\n');
    };
    for f in &st.faces {
        for q in &f.pieces {
            face(&mut o, f, 0, area(q), q);
        }
        for (q, src) in f.cutters.iter().zip(&f.cut_src) {
            face(&mut o, f, cut_kind(src.kind), -area(q), q);
        }
    }
    for f in &st.walls {
        for q in &f.pieces {
            face(&mut o, f, 0, area(q), q);
        }
        for q in &f.cutters {
            face(&mut o, f, 3, -area(q), q);
        }
    }
    o.replace('\n', "\r\n")
}
