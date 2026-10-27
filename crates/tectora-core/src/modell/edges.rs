//! Building the output lines from the polygon edges, and finding a second
//! face for lines that only one face references.

use super::Tol;
use super::plane::{Plane, frame};
use super::poly::has_point;
use super::types::{Edge, Face, Line, Poly, Srf, State};
use super::vec::{V3, veq};

/// 1 if the edges are equal, -1 if equal reversed, 0 otherwise.
fn same_edge(a: (V3, V3), b: (V3, V3), tol: f64) -> i32 {
    if veq(a.0, b.0, tol) && veq(a.1, b.1, tol) {
        1
    } else if veq(a.1, b.0, tol) && veq(a.0, b.1, tol) {
        -1
    } else {
        0
    }
}

/// The side kind a face kind gives a line.
fn side_kind(kind: i32) -> i32 {
    match kind as u8 {
        b'D' | b'G' | b'L' => b'D' as i32,
        b'T' | b'W' => b'W' as i32,
        _ => kind,
    }
}

/// A face reference to a line: the face's header with its side kind.
fn side_of(f: &Face) -> Srf {
    Srf {
        kind: side_kind(f.rec.kind),
        ..f.rec
    }
}

/// The edge, cutter header and polygon position of one reference.
struct Ref<'a> {
    e: Option<&'a Edge>,
    cut: Option<&'a Srf>,
    idx: i32,
}

/// Adds a reference of face `f` to line `k`, or makes a new line when `k`
/// is `None`. `dir` is +1 for face pieces and -1 for cutters, which run
/// the other way round. Returns the line index or -1.
fn reference(
    lines: &mut Vec<Line>,
    f: &Face,
    r: Ref,
    k: Option<usize>,
    dir: i32,
    tol: &Tol,
) -> i32 {
    let s = side_of(f);
    let raw = f.rec.kind;
    if let Some(k) = k {
        let g = &mut lines[k];
        // A wall never attaches to a roof line here.
        if g.a.kind == b'D' as i32 && raw == b'W' as i32 {
            return k as i32;
        }
        if dir == -1 {
            g.n_minus += 1;
            if let Some(x) = r.cut {
                g.kind_minus = x.kind;
            }
        }
        let is =
            |o: &Srf| o.kind == s.kind && o.model == s.model && o.face == s.face && o.sub == s.sub;
        if is(&g.a) || is(&g.b) {
            return k as i32;
        }
        if dir == 1 {
            g.n_plus += 1;
            if let Some(x) = r.cut {
                g.kind_plus = x.kind;
            }
            if g.n_plus > 2 {
                match r.e {
                    None => return k as i32,
                    Some(e) if veq(e.p1, g.p1, tol.t01) => return k as i32,
                    _ => {}
                }
            }
        }
        match raw as u8 {
            b'D' | b'G' => {
                if g.a_set == 0 {
                    g.a_set = 1;
                    g.a = s;
                } else {
                    g.b_set = 1;
                    g.b = s;
                }
            }
            b'L' | b'T' | b'W' if g.b_set == 0 => g.b = s,
            _ => {}
        }
        return k as i32;
    }
    let Some(e) = r.e else { return -1 };
    let (p1, p2) = if dir == -1 {
        (e.p2, e.p1)
    } else {
        (e.p1, e.p2)
    };
    let mut l = Line {
        p1,
        p2,
        kind: raw,
        n_plus: 0,
        kind_plus: b' ' as i32,
        n_minus: 0,
        kind_minus: b' ' as i32,
        code: 0,
        sign: 1,
        x64: 0,
        a_set: 1,
        a: s,
        b_set: 0,
        b: Srf::default(),
        idx: r.idx,
        x200: 0,
    };
    if dir == 1 {
        l.n_plus = 1;
        l.kind_plus = raw;
    } else {
        l.n_minus = 1;
        l.kind_minus = raw;
    }
    let d = p2.sub(p1);
    let h = V3::new(d.x, d.y, 0.0);
    let virtual_side = |kind: u8, o: V3, h: V3, up: f64| Srf {
        kind: kind as i32,
        model: -1,
        face: -1,
        sub: -1,
        flag5: 0,
        angle: 90.0,
        plane: Plane::new(o, V3::new(0.0, 0.0, up), h),
        ..Srf::default()
    };
    match e.kind() {
        b'd' => l.b = virtual_side(b'D', p1, h, 1.0),
        b'W' => l.b = virtual_side(b'W', p1, h, if dir == 1 { 1.0 } else { -1.0 }),
        b'w' | b'T' => {
            l.kind = b'T' as i32;
            l.b = virtual_side(b'D', p1, h, -1.0);
        }
        b'L' => {
            l.kind = b'L' as i32;
            l.b = virtual_side(b'D', p2, V3::new(-d.x, -d.y, 0.0), -1.0);
        }
        b's' => {
            l.b = Srf {
                kind: b'D' as i32,
                ..e.info
            };
            l.b_set = 1;
            if dir == 1 {
                l.n_plus += 1;
            } else {
                l.n_minus += 1;
            }
        }
        _ => {}
    }
    if raw == b'W' as i32 {
        l.code = 18;
    }
    lines.push(l);
    (lines.len() - 1) as i32
}

fn find(lines: &[Line], e: &Edge, tol: f64) -> Option<usize> {
    lines
        .iter()
        .position(|g| same_edge((e.p1, e.p2), (g.p1, g.p2), tol) != 0)
}

/// Drops consecutive edges that became the same line; true when fewer than
/// three edges are left.
fn drop_repeats(p: &mut Poly) -> bool {
    let mut i: isize = 0;
    let mut j: isize = 1;
    while (i as usize) < p.edges.len() {
        let n = p.edges.len() as isize;
        let a = p.edges[i as usize].line.abs();
        let b = p.edges[j.rem_euclid(n) as usize].line.abs();
        if a == b {
            p.edges.remove(i as usize);
            i -= 1;
            j -= 1;
        }
        i += 1;
        j += 1;
    }
    p.edges.len() < 3
}

fn link(
    lines: &mut Vec<Line>,
    f: &Face,
    p: &mut Poly,
    cut: Option<&Srf>,
    dir: i32,
    tol: f64,
    t: &Tol,
) {
    for i in 0..p.edges.len() {
        let e = p.edges[i];
        let k = find(lines, &e, tol);
        let r = Ref {
            e: Some(&e),
            cut,
            idx: i as i32,
        };
        p.edges[i].line = reference(lines, f, r, k, dir, t);
    }
}

/// Matches every polygon edge to a line by its end points, creating lines
/// as needed: face pieces, cutters, wall-kind faces, walls.
pub fn build_lines(st: &mut State, tol: &Tol) {
    let mut lines = std::mem::take(&mut st.lines);
    let t07 = tol.t07;
    for wall_kind in [false, true] {
        for f in 0..st.faces.len() {
            if (st.faces[f].kind() == b'W') != wall_kind {
                continue;
            }
            let mut i = 0;
            while i < st.faces[f].pieces.len() {
                let mut p = std::mem::take(&mut st.faces[f].pieces[i]);
                link(&mut lines, &st.faces[f], &mut p, None, 1, t07, tol);
                let gone = drop_repeats(&mut p);
                st.faces[f].pieces[i] = p;
                if gone {
                    st.faces[f].pieces.remove(i);
                } else {
                    i += 1;
                }
            }
        }
        if wall_kind {
            break;
        }
        for f in 0..st.faces.len() {
            let mut j = 0;
            while j < st.faces[f].cutters.len() {
                let mut p = std::mem::take(&mut st.faces[f].cutters[j]);
                let src = st.faces[f].cut_src.get(j).copied();
                link(&mut lines, &st.faces[f], &mut p, src.as_ref(), -1, t07, tol);
                let gone = drop_repeats(&mut p);
                st.faces[f].cutters[j] = p;
                if gone {
                    st.faces[f].cutters.remove(j);
                    if j < st.faces[f].cut_src.len() {
                        st.faces[f].cut_src.remove(j);
                    }
                } else {
                    j += 1;
                }
            }
        }
    }
    for f in 0..st.walls.len() {
        for i in 0..st.walls[f].pieces.len() {
            let mut p = std::mem::take(&mut st.walls[f].pieces[i]);
            link(&mut lines, &st.walls[f], &mut p, None, 1, tol.t01, tol);
            st.walls[f].pieces[i] = p;
        }
    }
    st.lines = lines;
}

/// For a line only one eave-side face references, looks for faces whose
/// polygon contains the line, and adds them as a second side.
pub fn find_second_faces(st: &mut State, tol: &Tol) {
    let t = tol.t01;
    for k in 0..st.lines.len() {
        let l = st.lines[k];
        if !(l.code == 0 && l.n_plus < 2 && l.a.flag5 != 0) {
            continue;
        }
        for f in 0..st.faces.len() {
            for p in &st.faces[f].pieces {
                let m = frame(&p.plane);
                let pl = st.faces[f].rec.plane;
                let l = st.lines[k];
                if !(pl.on(l.p1, t) && pl.on(l.p2, t)) {
                    continue;
                }
                let mid = l.p1.add(l.p2.sub(l.p1).scale(0.5));
                if has_point(p, mid, t, 0, &m) == 0 {
                    continue;
                }
                if st.faces[f]
                    .cutters
                    .iter()
                    .any(|c| has_point(c, mid, t, 0, &frame(&c.plane)) != 0)
                {
                    continue;
                }
                let r = Ref {
                    e: None,
                    cut: None,
                    idx: 0,
                };
                reference(&mut st.lines, &st.faces[f], r, Some(k), 1, tol);
                let l = &mut st.lines[k];
                let w = l.p2.sub(l.p1).cross(l.a.plane.n);
                let q = l.p1.add(w);
                l.x200 = pl.above(q, t) as i32;
            }
        }
    }
}
