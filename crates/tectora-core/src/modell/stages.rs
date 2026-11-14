//! The stage order and the face-level stages: which part of a face another
//! face or a hole hides, merging and subtracting those parts, tidying.

use super::Tol;
use super::boolean;
use super::classify::classify_all;
use super::edges::{build_lines, find_second_faces};
use super::plane::{Mat, XY, frame};
use super::poly::{bbox, orient_to_plane, sides};
use super::split::split_all;
use super::types::{Edge, Face, Poly, Srf, State};
use super::vec::{eq, gt, lt};

/// The booleans' own tolerance, not scaled by the unit.
const BOOL_TOL: f64 = 0.01;

pub fn run(st: &mut State, tol: &Tol, to: usize) {
    run_range(st, tol, 0, to);
}

pub fn run_range(st: &mut State, tol: &Tol, from: usize, to: usize) {
    let stages: [fn(&mut State, &Tol); 8] = [
        hidden_parts,
        merge_cutters,
        subtract_cutters,
        tidy,
        split_all,
        build_lines,
        find_second_faces,
        classify_all,
    ];
    for (i, f) in stages.iter().enumerate() {
        if st.overflow {
            return;
        }
        if i >= from && i < to {
            f(st, tol);
        }
    }
}

fn vertical(f: &Face) -> bool {
    eq(f.rec.angle, 90.0, 1.0)
}

fn steep(f: &Face) -> bool {
    eq(f.rec.angle, 90.0, 1.0) || gt(f.rec.angle, 90.0, 1.0)
}

/// The projection a face's polygon is worked in: xy, or for a vertical face
/// its own frame.
fn work_frame(f: &Face, p: &Poly) -> Mat {
    if vertical(f) { frame(&p.plane) } else { XY }
}

/// The part of `f`'s piece hidden by `g`: within `g`'s footprint, where
/// `f` lies below `g`'s plane (a coplanar tie goes against the lower face
/// number).
fn hidden_by(
    f: &Face,
    lp: &[Poly],
    g: &Srf,
    gsrc: &[Poly],
    tol: &Tol,
    template: &Edge,
) -> Vec<Poly> {
    let mut out = Vec::new();
    let (Some(a), Some(b)) = (lp.first(), gsrc.first()) else {
        return out;
    };
    let (ba, bb) = (bbox(a), bbox(b));
    if ba[1] < bb[0] || bb[1] < ba[0] || ba[3] < bb[2] || bb[3] < ba[2] {
        return out;
    }
    let c = boolean::clip(a, b, tol.t01, &XY, 1);
    if c.is_empty() {
        return out;
    }
    let r = match a.plane.intersect(&b.plane, tol.t01) {
        Ok(line) => c
            .iter()
            .flat_map(|p| boolean::split_line(p, line, tol.t01, &XY, 0xf, template))
            .collect(),
        Err(_) => c,
    };
    for mut p in r {
        p.sign = -1.0;
        let bits = sides(b, &p, tol.t05);
        let keep = if bits & 6 == 2 {
            true
        } else if bits & 6 == 4 {
            false
        } else {
            f.rec.face < g.face
        };
        if keep {
            out.push(p);
        }
    }
    out
}

/// For a vertical face: the part below the other surface.
fn hidden_vertical(lp: &[Poly], gsrc: &[Poly], tol: &Tol) -> Vec<Poly> {
    let mut out = Vec::new();
    let (Some(a), Some(b)) = (lp.first(), gsrc.first()) else {
        return out;
    };
    let (ba, bb) = (bbox(a), bbox(b));
    if !(bb[0] <= ba[1] && ba[0] <= bb[1] && bb[2] <= ba[3] && ba[2] <= bb[3]) {
        return out;
    }
    for t in boolean::below(a, b, tol.t01) {
        let m = frame(&a.plane);
        for mut r in boolean::clip(a, &t, tol.t01, &m, 1) {
            r.sign = -1.0;
            out.push(r);
        }
    }
    out
}

/// Stage 0: the own part of every face, and the cutters.
fn hidden_parts(st: &mut State, tol: &Tol) {
    // The edge template for cutter edges; a hole only changes its kind, so
    // the rest stays from the last face that cut.
    let mut template = Edge::new(Default::default(), Default::default(), b'?');
    if st.n_models != 1 {
        for i in 0..st.faces.len() {
            if steep(&st.faces[i]) {
                continue;
            }
            let p = st.faces[i].pieces[0].clone();
            st.faces[i].own.clear();
            let model = st.faces[i].rec.model;
            let Some(m) = st.models.iter().find(|m| m.rec.model == model) else {
                continue;
            };
            let res = boolean::clip(&m.pieces[0], &p, tol.t01, &XY, 4);
            st.faces[i].own = if res.is_empty() { vec![p] } else { res };
        }
        for i in 0..st.faces.len() {
            for j in 0..st.faces.len() {
                let (f, g) = (&st.faces[i], &st.faces[j]);
                if g.rec.model == f.rec.model || steep(g) {
                    continue;
                }
                template.info = Srf {
                    kind: b's' as i32,
                    ..g.rec
                };
                let lp = vec![f.pieces[0].clone()];
                let out = if vertical(f) {
                    hidden_vertical(&lp, &g.own, tol)
                } else if lt(f.rec.angle, 90.0, 1.0) {
                    let gsrc = if f.rec.plane.coplanar(&g.rec.plane, tol.t01) {
                        &g.pieces
                    } else {
                        &g.own
                    };
                    hidden_by(f, &lp, &g.rec, gsrc, tol, &template)
                } else {
                    Vec::new()
                };
                let src = g.rec;
                for p in out {
                    st.faces[i].cutters.push(p);
                    st.faces[i].cut_src.push(src);
                }
            }
        }
    }
    if st.n_holes != 0 {
        for h in 0..st.holes.len() {
            if steep(&st.holes[h]) {
                continue;
            }
            let lp = vec![st.holes[h].pieces[0].clone()];
            template.info.kind = st.holes[h].rec.kind;
            let src = st.holes[h].rec;
            for i in 0..st.faces.len() {
                let f = &st.faces[i];
                let fp = vec![f.pieces[0].clone()];
                let out = if vertical(f) {
                    hidden_vertical(&fp, &lp, tol)
                } else if lt(f.rec.angle, 90.0, 1.0) {
                    hidden_by(f, &fp, &src, &lp, tol, &template)
                } else {
                    Vec::new()
                };
                for p in out {
                    st.faces[i].cutters.push(p);
                    st.faces[i].cut_src.push(src);
                }
            }
        }
    }
}

/// Stage 1: unites overlapping or touching cutters of each face.
fn merge_cutters(st: &mut State, tol: &Tol) {
    for f in &mut st.faces {
        'restart: loop {
            for i in 0..f.cutters.len() {
                for j in 0..f.cutters.len() {
                    if i == j {
                        continue;
                    }
                    let (ci, cj) = (&f.cutters[i], &f.cutters[j]);
                    let m = if vertical_face(&f.rec) {
                        frame(&ci.plane)
                    } else {
                        XY
                    };
                    let mut u = boolean::union(ci, cj, BOOL_TOL, 0.0, &m);
                    if u.is_empty() {
                        u = boolean::union(ci, cj, BOOL_TOL, tol.t05, &m);
                    }
                    if u.is_empty() {
                        continue;
                    }
                    let src = f.cut_src[i];
                    let (hi, lo) = if i < j { (j, i) } else { (i, j) };
                    f.cutters.remove(lo);
                    f.cutters.remove(hi - 1);
                    f.cut_src.remove(lo);
                    f.cut_src.remove(hi - 1);
                    orient_to_plane(&mut u, 1, tol.t01);
                    f.cutters.push(u);
                    f.cut_src.push(src);
                    continue 'restart;
                }
            }
            break;
        }
    }
}

fn vertical_face(rec: &Srf) -> bool {
    eq(rec.angle, 90.0, 1.0)
}

/// Stage 2: subtracts the roof cutters from the pieces where that leaves
/// exactly one result.
fn subtract_cutters(st: &mut State, tol: &Tol) {
    for f in &mut st.faces {
        'restart: loop {
            for i in 0..f.pieces.len() {
                for j in 0..f.cutters.len() {
                    let k = f.cut_src[j].kind as u8;
                    if k == b'G' || k == b'T' || k == b'L' {
                        continue;
                    }
                    let p = &f.pieces[i];
                    let m = if vertical_face(&f.rec) {
                        frame(&p.plane)
                    } else {
                        XY
                    };
                    let res = boolean::difference(p, &f.cutters[j], BOOL_TOL, tol.t05, &m);
                    if res.len() != 1 {
                        continue;
                    }
                    f.pieces.remove(i);
                    f.cutters.remove(j);
                    f.cut_src.remove(j);
                    for mut r in res {
                        if r.len() > 2 {
                            orient_to_plane(&mut r, 1, tol.t01);
                            f.pieces.insert(0, r);
                        }
                    }
                    continue 'restart;
                }
            }
            break;
        }
    }
}

/// Stage 3: merges collinear edges of pieces and cutters and drops
/// polygons with fewer than three edges. (A dropped cutter keeps its
/// header entry, so later headers shift against their cutters.)
fn tidy(st: &mut State, tol: &Tol) {
    for f in &mut st.faces {
        let vert = vertical_face(&f.rec);
        for list in [&mut f.pieces, &mut f.cutters] {
            let mut i = 0;
            while i < list.len() {
                let m = if vert { frame(&list[i].plane) } else { XY };
                boolean::tidy(&mut list[i], &m, tol.t025);
                if list[i].len() < 3 {
                    list.remove(i);
                } else {
                    i += 1;
                }
            }
        }
    }
    let _ = work_frame;
}
