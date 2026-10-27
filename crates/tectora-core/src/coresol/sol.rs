//! The core SOL (DACAD's `.SO2`): the roof lines and the faces as lists
//! of signed line numbers, as the core writes it before the wall stage.

use super::ParseError;
use super::num::{put_f, scan_i32};

/// The core SOL.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CoreSol {
    /// The leading number of the file (0).
    pub lead: i32,
    /// `lines[i]` is line number `i + 1`.
    pub lines: Vec<SolLine>,
    /// The faces in file order.
    pub faces: Vec<SolFace>,
}

/// A line `p`–`q` with its line code and length.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SolLine {
    pub p: [f32; 3],
    pub q: [f32; 3],
    /// DACAD line code (1 eave, 2 ridge, ..., 17 none).
    pub code: i32,
    pub len: f32,
}

/// A face record.
#[derive(Debug, Clone, PartialEq)]
pub struct SolFace {
    /// The face's character: the letter of its wall (`W`, `G`, `g`, `t`).
    pub ch: u8,
    pub wall: i32,
    pub sub: i32,
    pub kind: i32,
    pub area: f32,
    /// The wall line `x1 y1 x2 y2` at z = 0.
    pub wall_line: [f32; 4],
    /// The plane `nx ny nz d` (n·x = d).
    pub plane: [f32; 4],
    /// Line numbers from 1; negative for a line run from `q` to `p`.
    pub edges: Vec<i32>,
}

impl SolLine {
    /// The ends of the line run in the direction of the signed `edge`.
    pub fn oriented(&self, edge: i32) -> ([f32; 3], [f32; 3]) {
        if edge > 0 {
            (self.p, self.q)
        } else {
            (self.q, self.p)
        }
    }
}

impl CoreSol {
    /// The line of a signed edge reference.
    pub fn line(&self, edge: i32) -> Option<&SolLine> {
        self.lines
            .get((edge.unsigned_abs() as usize).checked_sub(1)?)
    }

    /// Reads the core SOL text. Numbers are kept as the floats nearest to
    /// the printed decimals, signs of zero included, so that [`write`]
    /// reproduces the text; the wall stage's own reading (which loses the
    /// sign of a zero) is applied where it reads them.
    ///
    /// [`write`]: CoreSol::write
    pub fn parse(text: &str) -> Result<CoreSol, ParseError> {
        let t: Vec<&str> = text.split_ascii_whitespace().collect();
        let mut i = 0usize;
        let mut next = || -> Result<&str, ParseError> {
            let r = t.get(i).copied().ok_or(ParseError::Sol(i));
            i += 1;
            r
        };
        let int = |s: &str, at| scan_i32(s).ok_or(ParseError::Sol(at));
        let flt = |s: &str, at| -> Result<f32, ParseError> {
            s.parse::<f64>()
                .map(|v| v as f32)
                .map_err(|_| ParseError::Sol(at))
        };
        let mut at = 0;
        let lead = int(next()?, at)?;
        at += 1;
        let nl = int(next()?, at)?;
        at += 1;
        let mut lines = vec![
            SolLine {
                p: [0.0; 3],
                q: [0.0; 3],
                code: 0,
                len: 0.0
            };
            nl.max(0) as usize
        ];
        for _ in 0..nl.max(0) {
            let k = int(next()?, at)?;
            let mut v = [0.0f32; 6];
            for (j, x) in v.iter_mut().enumerate() {
                *x = flt(next()?, at + 1 + j)?;
            }
            let code = int(next()?, at + 7)?;
            let len = flt(next()?, at + 8)?;
            let slot = (k as usize)
                .checked_sub(1)
                .and_then(|s| lines.get_mut(s))
                .ok_or(ParseError::Sol(at))?;
            *slot = SolLine {
                p: [v[0], v[1], v[2]],
                q: [v[3], v[4], v[5]],
                code,
                len,
            };
            at += 9;
        }
        let nf = int(next()?, at)?;
        at += 1;
        let mut faces = Vec::with_capacity(nf.max(0) as usize);
        for _ in 0..nf.max(0) {
            let wall = int(next()?, at)?;
            let sub = int(next()?, at + 1)?;
            let kind = int(next()?, at + 2)?;
            let area = flt(next()?, at + 3)?;
            let ch = *next()?.as_bytes().first().ok_or(ParseError::Sol(at + 4))?;
            let mut v = [0.0f32; 8];
            for (j, x) in v.iter_mut().enumerate() {
                *x = flt(next()?, at + 5 + j)?;
            }
            let ne = int(next()?, at + 13)?;
            at += 14;
            let mut edges = Vec::with_capacity(ne.max(0) as usize);
            for _ in 0..ne.max(0) {
                edges.push(int(next()?, at)?);
                at += 1;
            }
            faces.push(SolFace {
                ch,
                wall,
                sub,
                kind,
                area,
                wall_line: [v[0], v[1], v[2], v[3]],
                plane: [v[4], v[5], v[6], v[7]],
                edges,
            });
        }
        Ok(CoreSol { lead, lines, faces })
    }

    /// The core SOL text, CRLF line ends, as the core writes it: the lines
    /// from the highest number down, then the faces.
    pub fn write(&self) -> String {
        let mut s = String::new();
        s.push_str(&format!("{:05}\n\n{:05}\n\n", self.lead, self.lines.len()));
        for (i, l) in self.lines.iter().enumerate().rev() {
            s.push_str(&format!("{:05} ", i + 1));
            put3(&mut s, l.p);
            s.push_str("\n      ");
            put3(&mut s, l.q);
            s.push_str(&format!("   {:02} ", l.code));
            put_f(&mut s, l.len as f64, 10, 5);
            s.push('\n');
        }
        s.push_str(&format!("\n{:05}\n\n", self.faces.len()));
        for f in &self.faces {
            s.push_str(&format!("{:05}   {}   {}   ", f.wall, f.sub, f.kind));
            put_f(&mut s, f.area as f64, 10, 5);
            if f.ch == b'L' {
                // A hole's cut-out has no wall line and no plane; the core
                // writes a fixed text for both.
                s.push_str("\nL 0.0 0.0 0.0 0.0 0.0 0.0 0.0 0.0\n\n");
            } else {
                s.push_str(&format!("\n\n        {} ", f.ch as char));
                put_list(&mut s, &f.wall_line);
                s.push_str("\n          ");
                put_list(&mut s, &f.plane);
                s.push_str("\n\n");
            }
            s.push_str(&format!("        {:05}\n\n", f.edges.len()));
            for e in &f.edges {
                s.push_str(&format!("\t{e:05}\n"));
            }
            s.push('\n');
        }
        crlf(&s)
    }
}

/// `%10.5f %10.5f %10.5f`.
pub(crate) fn put3(s: &mut String, v: [f32; 3]) {
    put_list(s, &v);
}

/// Space-separated `%10.5f` values.
pub(crate) fn put_list(s: &mut String, v: &[f32]) {
    for (k, x) in v.iter().enumerate() {
        if k > 0 {
            s.push(' ');
        }
        put_f(s, *x as f64, 10, 5);
    }
}

/// Text-mode output: every `\n` becomes CR LF.
pub(crate) fn crlf(s: &str) -> String {
    s.replace('\n', "\r\n")
}
