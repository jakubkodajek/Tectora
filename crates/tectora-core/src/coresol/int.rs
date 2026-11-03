//! The INT writer: the input of the Modellverschnitt, written by the core
//! after the core SOL when several roofs are joined.
//!
//! DACAD writes each face's own polygon from memory and flags an edge
//! when it borders a wall's hidden wall face. A producer that has those
//! fills an [`IntGeometry`] itself; [`IntGeometry::from_sol`] rebuilds
//! them from the core SOL alone, which is exact for most cases but takes
//! shared coordinates from the deduplicated SOL lines (off by up to 3e-4
//! in the captures and 0.009 in synthetic multi-roof cases, and a zero's
//! sign can differ) and guesses the flag from the line codes.

use std::collections::HashMap;
use std::fmt::Write as _;

use super::num::{EPS_ANG, HALF_PI_F, R2D, feq, put_f};
use super::rd2::{Model, Rd2, Wall};
use super::sol::{CoreSol, SolFace, crlf, put3};

/// One edge of a face record.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IntEdge {
    pub p: [f32; 3],
    pub q: [f32; 3],
    /// 1 when the edge borders a hidden wall face (and is not a T edge),
    /// else 0.
    pub flag: i32,
}

/// The polygon of one face record: the face of wall `wall` (letter `ch`
/// as in the SOL: `W`, `G` or `g`) with sub number `sub`.
#[derive(Debug, Clone, PartialEq)]
pub struct IntFace {
    pub ch: u8,
    pub wall: i32,
    pub sub: i32,
    pub edges: Vec<IntEdge>,
}

/// A ground plan edge `p`–`q` at z = 0.
pub type PlanEdge = ([f32; 2], [f32; 2]);

/// Everything of the INT that is not in the RD2.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct IntGeometry {
    /// The face records in the order they are written (the core SOL's
    /// order of faces with edges).
    pub faces: Vec<IntFace>,
    /// The ground plan of each `g` model in core order, as edges `p`–`q`
    /// at z = 0; `None` or an empty list writes no model record.
    pub footprints: Vec<Option<Vec<PlanEdge>>>,
}

/// Why an INT could not be written.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum IntError {
    #[error("INT: no wall {letter} {no} in the RD2 ring", letter = *.0 as char, no = .1)]
    NoWall(u8, i32),
    #[error("INT: wall {letter} {no} has no face {sub}", letter = *.0 as char, no = .1, sub = .2)]
    NoFace(u8, i32, i32),
    #[error("INT: edge {0} refers to no line")]
    NoLine(i32),
}

/// The INT text for an RD2 and its core SOL, with the SOL-based face
/// polygons and edge flags.
pub fn write_int(rd2: &Rd2, sol: &CoreSol) -> Result<String, IntError> {
    write_int_geometry(rd2, &IntGeometry::from_sol(rd2, sol)?)
}

impl IntGeometry {
    /// Rebuilds the face polygons, edge flags and `g` ground plans from the
    /// core SOL.
    ///
    /// - Faces: every SOL face with edges and a character `W`, `G` or `g`,
    ///   in SOL order; the coordinates are those of the SOL lines.
    /// - Ground plans: the i-th `t` face belongs to the i-th `g` model, its
    ///   edges rotated by one (the last one first).
    /// - Flags: see [`sol_edge_flag`].
    pub fn from_sol(rd2: &Rd2, sol: &CoreSol) -> Result<IntGeometry, IntError> {
        let mut uses: HashMap<u32, Vec<usize>> = HashMap::new();
        for (j, f) in sol.faces.iter().enumerate() {
            for e in &f.edges {
                uses.entry(e.unsigned_abs()).or_default().push(j);
            }
        }
        let mut faces = Vec::new();
        for (j, f) in sol.faces.iter().enumerate() {
            if !matches!(f.ch, b'W' | b'G' | b'g') || f.edges.is_empty() {
                continue;
            }
            let (mi, ri) = rd2
                .find_wall(f.ch, f.wall)
                .ok_or(IntError::NoWall(f.ch, f.wall))?;
            let m = &rd2.models[mi];
            if !matches!(m.kind, b'D' | b'G' | b'g') {
                continue;
            }
            let mut edges = Vec::with_capacity(f.edges.len());
            for &e in &f.edges {
                let l = sol.line(e).ok_or(IntError::NoLine(e))?;
                let (p, q) = l.oriented(e);
                let flag = sol_edge_flag(rd2, sol, &uses, j, e, (mi, ri))?;
                edges.push(IntEdge { p, q, flag });
            }
            faces.push(IntFace {
                ch: f.ch,
                wall: f.wall,
                sub: f.sub,
                edges,
            });
        }
        let mut ts = sol.faces.iter().filter(|f| f.ch == b't');
        let mut footprints = Vec::new();
        for _ in rd2.models.iter().filter(|m| m.kind == b'g') {
            let fp = match ts.next() {
                Some(t) => {
                    let mut e = t.edges.clone();
                    if !e.is_empty() {
                        e.rotate_right(1);
                    }
                    let mut v = Vec::with_capacity(e.len());
                    for &k in &e {
                        let (p, q) = sol.line(k).ok_or(IntError::NoLine(k))?.oriented(k);
                        v.push(([p[0], p[1]], [q[0], q[1]]));
                    }
                    Some(v)
                }
                None => None,
            };
            footprints.push(fp);
        }
        Ok(IntGeometry { faces, footprints })
    }
}

/// The edge flag rebuilt from the core SOL, which does not record which
/// faces an edge lies between (an empirical rule that matches every flag
/// of the captured cases):
///
/// - line code 11 (roof against a wall): 1, unless a `t` face shares it;
/// - line code 09: 1 iff the face is a `g` face;
/// - the bottom edge of a `g` face (horizontal, at the face's lowest z):
///   1; it borders the dormer wall's hidden wall face, but its line can
///   carry the code of a roof line it coincides with;
/// - a vertical edge of a 90° face: 1 iff the wall corner at that end is
///   concave and the neighbour wall there is not 90° itself;
/// - anything else: 0.
pub fn sol_edge_flag(
    rd2: &Rd2,
    sol: &CoreSol,
    uses: &HashMap<u32, Vec<usize>>,
    face: usize,
    edge: i32,
    wall: (usize, usize),
) -> Result<i32, IntError> {
    let l = sol.line(edge).ok_or(IntError::NoLine(edge))?;
    let me: &SolFace = &sol.faces[face];
    let mut others = uses
        .get(&edge.unsigned_abs())
        .into_iter()
        .flatten()
        .filter(|&&k| k != face)
        .map(|&k| &sol.faces[k]);
    if l.code == 11 {
        return Ok(if others.any(|o| o.ch == b't') { 0 } else { 1 });
    }
    if l.code == 9 {
        return Ok(i32::from(me.ch == b'g'));
    }
    if me.ch == b'g' && (l.p[2] - l.q[2]).abs() < 1e-4 {
        let zmin = me
            .edges
            .iter()
            .filter_map(|&k| sol.line(k))
            .flat_map(|x| [x.p[2], x.q[2]])
            .fold(f32::INFINITY, f32::min);
        if l.p[2] <= zmin + 1e-4 {
            return Ok(1);
        }
    }
    let m: &Model = &rd2.models[wall.0];
    let rw = m.ring[wall.1];
    let w: &Wall = &m.walls[rw.wall];
    let f = w
        .faces
        .iter()
        .skip(1)
        .find(|f| f.sub == me.sub)
        .ok_or(IntError::NoFace(me.ch, me.wall, me.sub))?;
    if !feq(f.pitch as f64, HALF_PI_F, EPS_ANG) {
        return Ok(0);
    }
    let (a, b) = (l.p.map(f64::from), l.q.map(f64::from));
    if (a[0] - b[0]).abs() > 1e-4 || (a[1] - b[1]).abs() > 1e-4 {
        return Ok(0);
    }
    let (sx, sy) = (w.start[0] as f64, w.start[1] as f64);
    let (dx, dy) = (rw.end[0] as f64 - sx, rw.end[1] as f64 - sy);
    let ln = dx.hypot(dy);
    if ln == 0.0 {
        return Ok(0);
    }
    let t = ((a[0] - sx) * dx + (a[1] - sy) * dy) / (ln * ln);
    let n = m.ring.len();
    let nb = if t < 0.5 {
        m.ring[(wall.1 + n - 1) % n]
    } else {
        m.ring[(wall.1 + 1) % n]
    };
    let nw = &m.walls[nb.wall];
    let (ndx, ndy) = (
        nb.end[0] as f64 - nw.start[0] as f64,
        nb.end[1] as f64 - nw.start[1] as f64,
    );
    let nl = ndx.hypot(ndy);
    if nl == 0.0 || nw.faces.len() < 2 {
        return Ok(0);
    }
    let cr = if t < 0.5 {
        (ndx * dy - ndy * dx) / (nl * ln)
    } else {
        (dx * ndy - dy * ndx) / (nl * ln)
    };
    let roofed = !feq(nw.faces[1].pitch as f64, HALF_PI_F, EPS_ANG);
    Ok(i32::from(cr < -1e-3 && roofed))
}

/// The INT text, CRLF line ends.
///
/// - Model records: an `I` record per `D` model (its ring walls that are
///   neither dormer walls nor points, at the height of their first face)
///   and a `G` record per `g` model with a ground plan, in core order.
/// - Face records: [`IntGeometry::faces`], with the integers and the frame
///   of the RD2 face they belong to.
/// - Holes: the RD2 `L` records. The reader builds each outline reversed
///   from its first point on (P0, Pn-1, ..., P1), and the cut of every
///   model reverses all outlines once more, so the outline is reversed
///   for an even number of models and in file order for an odd one.
pub fn write_int_geometry(rd2: &Rd2, g: &IntGeometry) -> Result<String, IntError> {
    let mut s = String::new();
    s.push_str("000 Version\n\n");
    // Model records.
    let mut recs: Vec<(u8, i32, Vec<[f32; 6]>)> = Vec::new();
    let mut gi = 0;
    for m in &rd2.models {
        match m.kind {
            b'g' => {
                if let Some(Some(fp)) = g.footprints.get(gi)
                    && !fp.is_empty()
                {
                    let e = fp
                        .iter()
                        .map(|(p, q)| [p[0], p[1], 0.0, q[0], q[1], 0.0])
                        .collect();
                    recs.push((b'G', m.no, e));
                }
                gi += 1;
            }
            b'D' => {
                let mut e = Vec::new();
                for (w, r) in m.ring_walls() {
                    if matches!(w.letter, b'G' | b'T' | b'g' | b't' | b'w') || w.zero {
                        continue;
                    }
                    let z = w.faces.get(1).map_or(0.0, |f| f.h);
                    e.push([w.start[0], w.start[1], z, r.end[0], r.end[1], z]);
                }
                if !e.is_empty() {
                    recs.push((b'I', m.no, e));
                }
            }
            _ => {}
        }
    }
    let _ = write!(s, "{} Modelle\n\n", recs.len());
    for (letter, no, edges) in &recs {
        let _ = write!(
            s,
            "{}\t{:03} {:03} {:03} {} {}\n\n",
            *letter as char, no, 0, 0, 0, 0
        );
        s.push('\t');
        put3(&mut s, [0.0; 3]);
        s.push_str("\n\t");
        put3(&mut s, [1.0, 0.0, 0.0]);
        s.push_str("\n\t");
        put3(&mut s, [0.0, 1.0, 0.0]);
        s.push_str("\n\t");
        put_f(&mut s, 0.0, 10, 5);
        s.push_str("\n\n");
        let _ = write!(s, "\t\t{} Kanten\n\n", edges.len());
        for e in edges {
            s.push_str("\t\t");
            put3(&mut s, [e[0], e[1], e[2]]);
            s.push_str("\n\t\t");
            put3(&mut s, [e[3], e[4], e[5]]);
            s.push_str("\n\n");
        }
    }
    // Face records.
    let _ = write!(s, "{} Flaechen\n\n", g.faces.len());
    for f in &g.faces {
        let (mi, ri) = rd2
            .find_wall(f.ch, f.wall)
            .ok_or(IntError::NoWall(f.ch, f.wall))?;
        let m = &rd2.models[mi];
        let w = &m.walls[m.ring[ri].wall];
        let rf = w
            .faces
            .iter()
            .skip(1)
            .find(|x| x.sub == f.sub)
            .ok_or(IntError::NoFace(f.ch, f.wall, f.sub))?;
        let letter = match w.letter {
            b'W' => 'D',
            b'G' => 'G',
            b'g' => 'W',
            other => other as char,
        };
        let _ = write!(
            s,
            "{letter}\t{:03} {:03} {:03} {} {}\n\n",
            m.no,
            w.no,
            rf.sub,
            i32::from(w.zero),
            rf.b
        );
        for v in [rf.o, rf.u, rf.w] {
            s.push('\t');
            put3(&mut s, v);
            s.push('\n');
        }
        s.push('\t');
        put_f(&mut s, rf.pitch as f64 * R2D, 10, 5);
        s.push_str("\n\n");
        let _ = write!(s, "\t\t{} Kanten\n\n", f.edges.len());
        for e in &f.edges {
            s.push_str("\t\t");
            put3(&mut s, e.p);
            s.push_str("\n\t\t");
            put3(&mut s, e.q);
            let _ = write!(s, " {}\n\n", e.flag);
        }
        s.push('\n');
    }
    // Holes.
    let _ = write!(s, "{} Loecher\n\n", rd2.holes.len());
    for h in &rd2.holes {
        let letter = if h.h <= 0.0 { 'L' } else { 'T' };
        let _ = write!(s, "\t\t{letter} {} ", h.no);
        put_f(&mut s, h.h as f64, 10, 2);
        s.push_str("\n\n");
        let n = h.points.len();
        let _ = write!(s, "\t\t{n} Kanten\n\n");
        // The per-model reversal is skipped when an uninitialised local of
        // the cut routine happens to be 1; it holds a float there in every
        // case examined, so it is taken as never 1.
        let seq: Vec<[f32; 2]> = if rd2.models.len() % 2 == 1 {
            h.points.clone()
        } else {
            h.points
                .first()
                .into_iter()
                .chain(h.points.iter().skip(1).rev())
                .copied()
                .collect()
        };
        for k in 0..n {
            let (a, b) = (seq[k], seq[(k + 1) % n]);
            s.push_str("\t\t");
            put_f(&mut s, a[0] as f64, 10, 5);
            s.push(' ');
            put_f(&mut s, a[1] as f64, 10, 5);
            s.push_str("\n\t\t");
            put_f(&mut s, b[0] as f64, 10, 5);
            s.push(' ');
            put_f(&mut s, b[1] as f64, 10, 5);
            s.push_str("\n\n");
        }
    }
    Ok(crlf(&s))
}
