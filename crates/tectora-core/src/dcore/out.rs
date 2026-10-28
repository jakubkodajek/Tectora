//! S7: the core SOL (`.SO2`) and INT writers, reading the arena.
//!
//! The text formats are those of [`crate::coresol`]: the arena fills a
//! [`CoreSol`] and an [`IntGeometry`] (with the [`coresol::Rd2`] view the
//! INT writer takes its model, frame and hole data from) and the coresol
//! writers produce the bytes. What only the arena knows is applied here:
//! which faces and hole pieces the core selects, the area sign change it
//! makes while writing, the closing call on the ground faces, and the two
//! header counts that the original computes separately from what it then
//! writes.

use crate::coresol::{self, CoreSol, IntEdge, IntFace, IntGeometry, SolFace, SolLine};

use super::error::*;
use super::num;
use super::state::*;

fn fault(what: &'static str) -> CoreError {
    CoreError::Fault(what)
}

/// The CRT prints a negative zero without its sign (a tiny negative value
/// keeps it); coresol's writers print the sign of a zero, as the files they
/// read back carry it. Every float handed to them goes through this.
fn pz(v: f32) -> f32 {
    if v == 0.0 { 0.0 } else { v }
}

fn pz3(v: [f32; 3]) -> [f32; 3] {
    v.map(pz)
}

/// Iterates a segment chain.
fn chain(c: &Core, s: Option<SegId>) -> Vec<SegId> {
    let mut v = Vec::new();
    let mut s = s;
    while let Some(i) = s {
        v.push(i);
        s = c.seg(i).next;
    }
    v
}

/// The walls of a model's ring as the writers walk them (ring counter times).
fn ring_walls(c: &Core, m: ModelId) -> Vec<WallId> {
    let n = c.model(m).counters[ctr::RING_WALLS] as u32;
    let mut v = Vec::new();
    let mut w = c.model(m).ring;
    for _ in 0..n {
        let Some(wi) = w else { break };
        v.push(wi);
        w = c.wall(wi).next;
    }
    v
}

fn models(c: &Core) -> Vec<ModelId> {
    let mut v = Vec::new();
    let mut m = c.g.models;
    while let Some(i) = m {
        v.push(i);
        m = c.model(i).next;
    }
    v
}

/// The faces of a wall the SOL and INT write, by wall type.
fn selected_faces(c: &Core, w: WallId) -> Vec<FaceId> {
    let wt = c.wall(w).typ;
    let mut v = Vec::new();
    let mut f = c.wall(w).faces;
    while let Some(fi) = f {
        let ft = c.face(fi).typ;
        let take = match wt {
            b'G' | b't' => ft != b'W',
            b'g' => ft == b'D',
            _ => true,
        };
        if take {
            v.push(fi);
        }
        f = c.face(fi).next;
    }
    v
}

/// Whether a wall's faces are written at all.
fn wall_written(c: &Core, w: WallId) -> bool {
    match c.wall(w).typ {
        b'T' | b'w' => false,
        b'g' => c.g.flags & 0x20 != 0,
        _ => true,
    }
}

/// The core SOL. Changes the state as the original does: the area of a
/// `t` face's head node changes sign.
pub fn write_sol(c: &mut Core) -> Result<Vec<u8>, CoreError> {
    if c.g.flags & 0x20 == 0 {
        return Err(CoreError::Unimplemented(
            "core SOL without wall faces (flag 0x20)",
        ));
    }
    let tol = c.g.tol;
    // Lines: the edge table, written in list order.
    let mut edges = Vec::new();
    let mut e = c.g.edges;
    while let Some(i) = e {
        edges.push(i);
        e = c.edges[i.ix()].next;
    }
    let n = c.g.n_lines.max(0) as usize;
    let mut lines = vec![
        SolLine {
            p: [0.0; 3],
            q: [0.0; 3],
            code: 0,
            len: 0.0
        };
        n
    ];
    // coresol writes line n first and line 1 last: the table must be in that order.
    if edges.len() != n {
        return Err(CoreError::Unimplemented(
            "edge table length differs from the line count",
        ));
    }
    for (k, &ei) in edges.iter().enumerate() {
        let s = c.seg(c.edges[ei.ix()].seg.ok_or(fault("edge without segment"))?);
        if s.line as usize != n - k {
            return Err(CoreError::Unimplemented(
                "edge table not in descending line order",
            ));
        }
        lines[n - k - 1] = SolLine {
            p: pz3(s.p),
            q: pz3(s.q),
            code: s.code,
            len: pz(s.len),
        };
    }
    // Count pass (the header count).
    let mut count = 0usize;
    for &m in &models(c) {
        for w in ring_walls(c, m) {
            if !wall_written(c, w) {
                continue;
            }
            for f in selected_faces(c, w) {
                let fc = c.face(f);
                if fc.closed == 0 {
                    continue;
                }
                if fc.segs.is_none() {
                    count += 1; // flag 0x20 is set
                    continue;
                }
                count += 1;
                let mut h = fc.holes;
                while let Some(hi) = h {
                    if chain(c, c.node(hi).segs).len() > 2 {
                        count += 1;
                    }
                    h = c.node(hi).next;
                }
            }
        }
    }
    // Write pass.
    let mut faces = Vec::new();
    for &m in &models(c) {
        c.g.counters = c.model(m).counters;
        for w in ring_walls(c, m) {
            if !wall_written(c, w) {
                continue;
            }
            let wr = c.wall(w).clone();
            let kind = if wr.typ == b't' { 3 } else { 0 };
            for f in selected_faces(c, w) {
                let fc = c.face(f).clone();
                if fc.closed == 0 {
                    continue;
                }
                let head = fc.head.ok_or(fault("face without head node"))?;
                if kind != 0 {
                    let a = c.node(head).area;
                    c.node_mut(head).area = -a;
                }
                let segs = chain(c, fc.segs);
                faces.push(SolFace {
                    ch: wr.typ,
                    wall: wr.no,
                    sub: fc.sub,
                    kind,
                    area: pz(c.node(head).area),
                    wall_line: [wr.s[0], wr.s[1], wr.e[0], wr.e[1]].map(pz),
                    plane: [fc.n[0], fc.n[1], fc.n[2], fc.d].map(pz),
                    edges: segs.iter().map(|&s| c.seg(s).line).collect(),
                });
                if segs.is_empty() {
                    continue;
                }
                let mut h = fc.holes;
                while let Some(hi) = h {
                    let hs = chain(c, c.node(hi).segs);
                    let keep: Vec<SegId> = hs
                        .iter()
                        .copied()
                        .filter(|&s| !num::peq(c.seg(s).p, c.seg(s).q, tol.value))
                        .collect();
                    if keep.len() > 2 {
                        let fa = c.seg(hs[0]).fa.ok_or(fault("hole piece without face"))?;
                        faces.push(SolFace {
                            ch: b'L',
                            wall: wr.no,
                            sub: fc.sub,
                            kind: c.face(fa).sub.max(1),
                            area: pz(-c.node(hi).area),
                            wall_line: [0.0; 4],
                            plane: [0.0; 4],
                            edges: keep.iter().map(|&s| c.seg(s).line).collect(),
                        });
                    }
                    h = c.node(hi).next;
                }
            }
        }
    }
    let written = faces.len();
    let sol = CoreSol {
        lead: 0,
        lines,
        faces,
    };
    let mut text = sol.write();
    if count != written {
        // The header count comes from the count pass.
        let head = CoreSol {
            lead: 0,
            lines: sol.lines.clone(),
            faces: Vec::new(),
        }
        .write();
        let at = head.len() - "00000\r\n\r\n".len();
        let old = format!("{written:05}\r\n\r\n");
        text = format!(
            "{}{count:05}\r\n\r\n{}",
            &text[..at],
            &text[at + old.len()..]
        );
    }
    Ok(text.into_bytes())
}

/// Removes a face's segments that reach the top (z >= 100) and, unless the
/// face is closed, closes its outline (see [`super::close::close_face`]).
pub fn close_face(c: &mut Core, f: FaceId, retry: bool) -> Result<bool, CoreError> {
    super::close::close_face(c, f, retry)
}

/// The edge flag of the INT: 1 when the segment borders a hidden wall face
/// (and is not a T segment).
fn int_flag(c: &Core, s: SegId) -> i32 {
    let sg = c.seg(s);
    if sg.kind == b'T' {
        return 0;
    }
    let hidden =
        |f: Option<FaceId>| f.is_some_and(|f| c.face(f).typ == b'W' && c.face(f).sub == -1);
    i32::from(hidden(sg.fa) || hidden(sg.fb))
}

/// The arena as the [`coresol::Rd2`] the INT writer reads model, frame and
/// hole data from: models in list order, walls in ring order.
fn rd2_view(c: &Core) -> coresol::Rd2 {
    let ms = models(c);
    let mut out = Vec::new();
    for &m in &ms {
        let md = c.model(m);
        let mut walls = Vec::new();
        let mut ring = Vec::new();
        for w in ring_walls(c, m) {
            let wr = c.wall(w);
            let mut faces = Vec::new();
            let mut f = wr.faces;
            while let Some(fi) = f {
                let fc = c.face(fi);
                faces.push(coresol::Face {
                    kind: fc.typ,
                    sub: fc.sub,
                    unused: 0,
                    flag: fc.b,
                    b: fc.b,
                    h: pz(fc.h),
                    ov: fc.ov,
                    pitch_deg: 0.0,
                    pitch: pz(fc.pitch),
                    o: pz3(fc.o),
                    u: pz3(fc.u),
                    w: pz3(fc.w),
                });
                f = fc.next;
            }
            ring.push(coresol::RingWall {
                wall: walls.len(),
                end: [pz(wr.e_orig[0]), pz(wr.e_orig[1])],
            });
            walls.push(coresol::Wall {
                letter: wr.typ,
                model_no: md.no,
                no: wr.no,
                start: [pz(wr.s_orig[0]), pz(wr.s_orig[1])],
                len: 0.0,
                angle: 0.0,
                priority: wr.extra,
                end: [pz(wr.e_orig[0]), pz(wr.e_orig[1])],
                zero: wr.zero != 0,
                height: wr.h,
                faces,
            });
        }
        out.push(coresol::Model {
            kind: md.typ,
            no: md.no,
            walls,
            ring,
        });
    }
    // Holes: the writer derives the printed order from the file order and
    // the number of models; invert that from the arena's outline.
    let odd = ms.len() % 2 == 1;
    let mut holes = Vec::new();
    let mut h = c.g.holes;
    while let Some(hi) = h {
        let seq: Vec<[f32; 2]> = chain(c, c.node(hi).segs)
            .iter()
            .map(|&s| [pz(c.seg(s).p[0]), pz(c.seg(s).p[1])])
            .collect();
        let points = if odd || seq.is_empty() {
            seq
        } else {
            std::iter::once(seq[0])
                .chain(seq[1..].iter().rev().copied())
                .collect()
        };
        holes.push(coresol::Hole {
            no: c.node(hi).no,
            h: pz(c.node(hi).hole_h()),
            points,
        });
        h = c.node(hi).next;
    }
    coresol::Rd2 {
        lead: 0,
        models: out,
        holes,
    }
}

/// The INT. Calls the closing on every `D`/`g` model's ground face, as the
/// original does.
pub fn write_int(c: &mut Core) -> Result<Vec<u8>, CoreError> {
    let ms = models(c);
    let mut n_models = 0;
    for &m in &ms {
        if matches!(c.model(m).typ, b'D' | b'g') {
            let gf = c.model(m).gface;
            close_face(c, gf, true)?;
            n_models += 1;
        }
    }
    let mut footprints = Vec::new();
    for &m in &ms {
        let typ = c.model(m).typ;
        if !matches!(typ, b'D' | b'g') {
            continue;
        }
        c.g.counters = c.model(m).counters;
        let gf = c.model(m).gface;
        let closed = close_face(c, gf, true)?;
        let segs = chain(c, c.face(gf).segs);
        let fp: Option<Vec<coresol::PlanEdge>> = (closed && !segs.is_empty()).then(|| {
            segs.iter()
                .map(|&s| {
                    let sg = c.seg(s);
                    ([pz(sg.p[0]), pz(sg.p[1])], [pz(sg.q[0]), pz(sg.q[1])])
                })
                .collect()
        });
        if typ == b'g' {
            footprints.push(fp);
        } else if fp.is_some() {
            return Err(CoreError::Unimplemented("INT ground record of a D model"));
        }
    }
    let mut faces = Vec::new();
    for &m in &ms {
        if !matches!(c.model(m).typ, b'D' | b'G' | b'g') {
            continue;
        }
        c.g.counters = c.model(m).counters;
        for w in ring_walls(c, m) {
            if matches!(c.wall(w).typ, b'T' | b't' | b'w') || !wall_written(c, w) {
                continue;
            }
            for f in selected_faces(c, w) {
                let fc = c.face(f);
                if fc.closed == 0 || fc.segs.is_none() {
                    continue;
                }
                let edges = chain(c, fc.segs)
                    .iter()
                    .map(|&s| IntEdge {
                        p: pz3(c.seg(s).p),
                        q: pz3(c.seg(s).q),
                        flag: int_flag(c, s),
                    })
                    .collect();
                faces.push(IntFace {
                    ch: c.wall(w).typ,
                    wall: c.wall(w).no,
                    sub: fc.sub,
                    edges,
                });
            }
        }
    }
    let rd2 = rd2_view(c);
    let recs = rd2
        .models
        .iter()
        .enumerate()
        .filter(|(_, m)| m.kind == b'D')
        .filter(|(_, m)| {
            m.ring_walls()
                .any(|(w, _)| !matches!(w.letter, b'G' | b'T' | b'g' | b't' | b'w') && !w.zero)
        })
        .count()
        + footprints
            .iter()
            .filter(|f| f.as_ref().is_some_and(|v| !v.is_empty()))
            .count();
    let g = IntGeometry { faces, footprints };
    let mut text =
        coresol::write_int_geometry(&rd2, &g).map_err(|_| CoreError::Fault("INT face lookup"))?;
    if recs != n_models {
        // The model count of the header is the number of D and g models.
        let old = format!("000 Version\r\n\r\n{recs} Modelle\r\n");
        let new = format!("000 Version\r\n\r\n{n_models} Modelle\r\n");
        text = text.replacen(&old, &new, 1);
    }
    Ok(text.into_bytes())
}

/// The SOL the driver writes before reading the RD2; a run that ends with
/// a fatal or cut error leaves it behind.
pub const PLACEHOLDER_SOL: &[u8] = b"0 0";

/// The fallback SOL of a broken wall chain: one line along the wall the
/// chain sort stopped at.
pub fn fallback_sol(c: &Core) -> Result<Vec<u8>, CoreError> {
    let w = c.wall(c.g.chain_wall.ok_or(fault("no chain wall"))?);
    let mut s = String::from("00000\n\n1\n\n00001 ");
    let f5 = |s: &mut String, v: f32| coresol::num::put_f(s, f64::from(v), 10, 5);
    for (k, v) in w.s.iter().enumerate() {
        if k > 0 {
            s.push(' ');
        }
        f5(&mut s, *v);
    }
    s.push_str("\n      ");
    for (k, v) in w.e.iter().enumerate() {
        if k > 0 {
            s.push(' ');
        }
        f5(&mut s, *v);
    }
    s.push_str(&format!("   {:02} ", c.g.codes.none as u32));
    f5(&mut s, 0.0);
    s.push_str("\n\n0\n");
    Ok(s.replace('\n', "\r\n").into_bytes())
}
