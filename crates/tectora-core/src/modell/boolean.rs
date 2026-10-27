//! The polygon booleans of the model intersection.
//!
//! All of them work on flat copies: projected to xy (or into a vertical
//! face's own frame), oriented counter-clockwise, and with the mutual
//! intersection points inserted. Results are lifted back with the first
//! polygon's back transform, so every z comes from that polygon's plane.
//! Edges keep their attributes through all of it.

use super::plane::{EPS_TINY, IDENTITY, Mat, Plane, XY, apply, transform};
use super::poly::{bbox, drop_zero, has_point_raw, intersect_vertices, merge_collinear, orient2d};
use super::types::{Edge, Poly};
use super::vec::{V3, parallel, veq};

/// The grid of the intersection vertices: DACAD's precision factor.
const GRID: f64 = 1000.0 / ((0.01f32 as f64) * 100.0);
/// The closing tolerance of the chainer's second try.
const CLOSE_TOL: f64 = 0.002;

/// The grid the vertices are rounded to before a split.
const SPLIT_GRID: f64 = 10.0 / (0.01f32 as f64);

fn minor(m: &Mat, r: usize, c: usize) -> f64 {
    let mut s = [[0.0; 3]; 3];
    for (si, i) in (0..4).filter(|&i| i != r).enumerate() {
        for (sj, j) in (0..4).filter(|&j| j != c).enumerate() {
            s[si][sj] = m[i][j];
        }
    }
    let mut acc = 0.0;
    for k in 0..3 {
        let (cc, d) = ((k + 2) % 3, (k + 1) % 3);
        acc += (s[2][cc] * s[1][d]) * s[0][k];
        acc -= (s[1][cc] * s[2][d]) * s[0][k];
    }
    acc
}

fn sgn(k: usize) -> f64 {
    if k.is_multiple_of(2) { 1.0 } else { -1.0 }
}

fn inverse(m: &Mat) -> Option<Mat> {
    let mut det = 0.0;
    for i in 0..4 {
        det += (minor(m, i, 0) * m[i][0]) * sgn(i);
    }
    if det.abs() < EPS_TINY {
        return None;
    }
    let mut out = [[0.0; 4]; 4];
    for (i, row) in out.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = (minor(m, j, i) * sgn(i + j)) / det;
        }
    }
    Some(out)
}

/// A flat copy of `p` in the work projection `m`, and the matrix that lifts
/// points back.
fn project(p: &Poly, m: &Mat) -> (Poly, Mat) {
    let n = p.edges.len();
    let (pts, plane, back): (Vec<V3>, Plane, Mat) = if *m == XY {
        let pl = &p.plane;
        let (a, b, c) = if pl.n.z.abs() < EPS_TINY {
            (0.0, 0.0, 0.0)
        } else {
            (pl.n.x / pl.n.z, pl.n.y / pl.n.z, pl.d / pl.n.z)
        };
        let back = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [-a, -b, 0.0, c],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let plane = Plane::new(V3::ZERO, V3::new(1.0, 0.0, 0.0), V3::new(0.0, 1.0, 0.0));
        (p.edges.iter().map(|e| e.p1.flat()).collect(), plane, back)
    } else {
        // A singular frame cannot happen for a polygon with a proper plane.
        let back = inverse(m).unwrap_or(IDENTITY);
        let pl = &p.plane;
        let plane = Plane::new(apply(m, pl.o), apply(m, pl.u), apply(m, pl.w));
        (
            p.edges.iter().map(|e| apply(m, e.p1)).collect(),
            plane,
            back,
        )
    };
    let edges = (0..n)
        .map(|i| Edge {
            p1: pts[i],
            p2: pts[(i + 1) % n],
            ..p.edges[i]
        })
        .collect();
    (
        Poly {
            edges,
            sign: p.sign,
            plane,
            m: p.m,
        },
        back,
    )
}

fn lift(e: &Edge, back: &Mat) -> Edge {
    Edge {
        p1: apply(back, e.p1),
        p2: apply(back, e.p2),
        ..*e
    }
}

/// A result polygon with `src`'s attributes.
fn with_attrs(src: &Poly, edges: Vec<Edge>, sign: f64) -> Poly {
    Poly {
        edges,
        sign,
        plane: src.plane,
        m: src.m,
    }
}

fn disjoint(a: &Poly, b: &Poly) -> bool {
    let (a, b) = (bbox(a), bbox(b));
    a[1] < b[0] || b[1] < a[0] || a[3] < b[2] || b[3] < a[2]
}

/// 1 same edge, -1 the same reversed, 0 otherwise.
fn edge_eq(e: &Edge, f: &Edge, tol: f64) -> i32 {
    if veq(e.p1, f.p1, tol) && veq(e.p2, f.p2, tol) {
        1
    } else if veq(e.p2, f.p1, tol) && veq(e.p1, f.p2, tol) {
        -1
    } else {
        0
    }
}

fn inside(p: &Poly, q: V3, tol: f64) -> bool {
    has_point_raw(p, q, tol, 0) != 0
}

fn line_dist(e: &Edge, v: V3) -> f64 {
    let w = v.sub(e.p1);
    let u = e.dir().normalize(super::plane::EPS_FRAME);
    let t = w.dot(u);
    w.sub(u.scale(t)).length()
}

fn collapse_short(p: &mut Poly, gap: f64) {
    let n = p.edges.len();
    for i in 0..n {
        let e = p.edges[i];
        if e.len() < gap {
            let m = e.mid();
            p.edges[(i + n - 1) % n].p2 = m;
            p.edges[(i + 1) % n].p1 = m;
            p.edges[i].p1 = m;
            p.edges[i].p2 = m;
        }
    }
}

fn split_at_vertices(p: &mut Poly, q: &Poly, gap: f64) {
    let mut i = 0;
    while i < p.edges.len() {
        for j in 0..q.edges.len() {
            let v = q.edges[j].p1;
            let e = p.edges[i];
            if line_dist(&e, v) >= gap {
                continue;
            }
            let (mut x0, mut x1, mut y0, mut y1) = (e.p1.x, e.p2.x, e.p1.y, e.p2.y);
            if x1 < x0 {
                std::mem::swap(&mut x0, &mut x1);
            }
            if y1 < y0 {
                std::mem::swap(&mut y0, &mut y1);
            }
            let ok = if x0 == x1 {
                if y0 == y1 {
                    continue;
                }
                y0 < v.y && v.y < y1
            } else if y0 == y1 {
                x0 < v.x && v.x < x1
            } else {
                x0 < v.x && v.x < x1 && y0 < v.y && v.y < y1
            };
            if ok {
                let c = Edge { p1: v, ..e };
                p.edges[i].p2 = v;
                p.edges.insert(i + 1, c);
            }
        }
        i += 1;
    }
}

/// Pulls nearly coincident geometry of two flat polygons together: short
/// edges collapse, `b`'s vertices move onto `a`'s, and vertices close to
/// an edge split it.
fn snap_together(a: &mut Poly, b: &mut Poly, gap: f64) {
    collapse_short(a, gap);
    collapse_short(b, gap);
    drop_zero(a);
    drop_zero(b);
    for j in 0..b.edges.len() {
        let v = b.edges[j].p1;
        let mut best: Option<(usize, V3)> = None;
        for (i, e) in a.edges.iter().enumerate() {
            if veq(e.p1, v, gap) {
                let d = e.p1.sub(v);
                match best {
                    None => best = Some((i, d)),
                    Some((_, bd)) if bd.length() > d.length() => best = Some((i, d)),
                    _ => {}
                }
            }
        }
        if let Some((i, _)) = best {
            b.edges[j].p1 = a.edges[i].p1;
        }
    }
    let n = b.edges.len();
    for i in 0..n {
        let q = b.edges[i].p1;
        b.edges[(i + n - 1) % n].p2 = q;
    }
    drop_zero(a);
    drop_zero(b);
    split_at_vertices(a, b, gap);
    split_at_vertices(b, a, gap);
    drop_zero(a);
    drop_zero(b);
}

/// The flat, oriented, intersected copies for the union and difference.
fn prepare(a: &Poly, b: &Poly, tol: f64, gap: f64, m: &Mat) -> Option<(Poly, Poly, Mat)> {
    let (mut pa, back) = project(a, m);
    let (mut pb, _) = project(b, m);
    orient2d(&mut pa, 1.0);
    orient2d(&mut pb, 1.0);
    if gap == 0.0 && disjoint(&pa, &pb) {
        return None;
    }
    intersect_vertices(&mut pa, &mut pb, GRID);
    if gap != 0.0 {
        merge_collinear(&mut pa, tol);
        merge_collinear(&mut pb, tol);
        snap_together(&mut pa, &mut pb, gap);
        if disjoint(&pa, &pb) {
            return None;
        }
        intersect_vertices(&mut pa, &mut pb, GRID);
    }
    Some((pa, pb, back))
}

/// `a` minus `b`. Without a shared edge the result is empty (no holes);
/// a fully covered `a` gives one empty polygon.
pub fn difference(a: &Poly, b: &Poly, tol: f64, gap: f64, m: &Mat) -> Vec<Poly> {
    if a.is_empty() || b.is_empty() {
        return Vec::new();
    }
    let Some((pa, pb, back)) = prepare(a, b, tol, gap, m) else {
        return Vec::new();
    };
    let mut contact = false;
    let mut l = Vec::new();
    for e in &pb.edges {
        if pa.edges.iter().any(|f| edge_eq(e, f, 0.0) != 0) {
            contact = true;
            continue;
        }
        if inside(&pa, e.mid(), tol) {
            l.push(*e);
        }
    }
    for e in &pa.edges {
        if pb.edges.iter().any(|f| edge_eq(e, f, 0.0) != 0) {
            contact = true;
            continue;
        }
        if !inside(&pb, e.mid(), tol) {
            l.push(*e);
        }
    }
    if !contact {
        return Vec::new();
    }
    if l.is_empty() {
        return vec![with_attrs(a, Vec::new(), a.sign)];
    }
    let mut out = Vec::new();
    let mut r: Vec<Edge> = Vec::new();
    let mut start = V3::ZERO;
    let (mut i, mut js) = (0usize, 1usize);
    while i + 1 < l.len() {
        let e = l[i];
        if r.is_empty() {
            start = e.p1;
            r.push(e);
        }
        let Some(k) = next_edge(&mut l, js, e.p2) else {
            return Vec::new();
        };
        let f = l[k];
        r.push(f);
        let mut closed = false;
        if r.len() > 2 && f.p2.same(start) {
            let lifted: Vec<Edge> = r.iter().map(|e| lift(e, &back)).collect();
            if lifted.len() >= 3 {
                out.push(with_attrs(a, lifted, a.sign));
            }
            r.clear();
            closed = true;
        }
        l.swap(js, k);
        if closed {
            i += 1;
            js += 1;
        }
        i += 1;
        js += 1;
    }
    out
}

/// The first edge from `from` on that starts at `p` (or ends there, then
/// flipped).
#[allow(clippy::needless_range_loop)]
fn next_edge(l: &mut [Edge], from: usize, p: V3) -> Option<usize> {
    for k in from..l.len() {
        if l[k].p1.same(p) {
            return Some(k);
        }
        if l[k].p2.same(p) {
            let f = &mut l[k];
            std::mem::swap(&mut f.p1, &mut f.p2);
            return Some(k);
        }
    }
    None
}

/// The union as one polygon; empty when the polygons do not make exactly
/// one simple loop.
pub fn union(a: &Poly, b: &Poly, tol: f64, gap: f64, m: &Mat) -> Poly {
    let empty = with_attrs(a, Vec::new(), a.sign);
    if a.is_empty() || b.is_empty() {
        return empty;
    }
    let Some((pa, pb, back)) = prepare(a, b, tol, gap, m) else {
        return empty;
    };
    let mut l = Vec::new();
    for e in &pb.edges {
        let mut hit = 0;
        for f in &pa.edges {
            if e.p1.same(f.p2) && e.p2.same(f.p1) {
                hit = -1;
                break;
            }
            if e.p1.same(f.p1) && e.p2.same(f.p2) {
                hit = 1;
                break;
            }
        }
        match hit {
            -1 => {}
            1 => l.push(*e),
            _ => {
                if !inside(&pa, e.mid(), tol) {
                    l.push(*e);
                }
            }
        }
    }
    for e in &pa.edges {
        if pb.edges.iter().any(|f| edge_eq(e, f, 0.0) != 0) {
            continue;
        }
        if !inside(&pb, e.mid(), tol) {
            l.push(*e);
        }
    }
    if l.len() < 2 {
        return empty;
    }
    let n = l.len();
    for i in 0..n - 1 {
        let p = l[i].p2;
        let js = i + 1;
        let Some(k) = next_edge(&mut l, js, p) else {
            return empty;
        };
        l.swap(js, k);
        if l[js + 1..].iter().any(|f| f.p1.same(p)) {
            return empty;
        }
    }
    if !l[0].p1.same(l[n - 1].p2) {
        return empty;
    }
    with_attrs(a, l.iter().map(|e| lift(e, &back)).collect(), a.sign)
}

/// Chains free edges into one loop, lifting the points; empty when the
/// chain breaks. Used edges are marked even then.
fn chain(recs: &[Edge], free: &mut [bool], back: &Mat) -> Vec<Edge> {
    let slots: Vec<usize> = (0..recs.len()).collect();
    chain_with(recs, &slots, free, back, None)
}

/// [`chain`] where record `i` keeps its free flag in `free[slots[i]]` (one
/// flag may be shared by records of two lists), optionally starting at the
/// first free record whose start lies on a split line and whose end does
/// not (`flags`: the on-line marks of both ends, 2 on the line).
fn chain_with(
    recs: &[Edge],
    slots: &[usize],
    free: &mut [bool],
    back: &Mat,
    flags: Option<&[(i32, i32)]>,
) -> Vec<Edge> {
    let is_free = |free: &[bool], i: usize| free[slots[i]];
    let preferred = flags
        .and_then(|fl| (0..recs.len()).find(|&i| is_free(free, i) && fl[i].0 == 2 && fl[i].1 != 2));
    let Some(k) = preferred.or_else(|| (0..recs.len()).find(|&i| is_free(free, i))) else {
        return Vec::new();
    };
    let start = recs[k].p1;
    let mut cur = recs[k].p2;
    free[slots[k]] = false;
    let mut r = vec![lift(&recs[k], back)];
    loop {
        let mut found = false;
        for i in 0..recs.len() {
            if !is_free(free, i) {
                continue;
            }
            if veq(recs[i].p1, cur, 0.0) {
                free[slots[i]] = false;
                cur = recs[i].p2;
                r.push(lift(&recs[i], back));
                found = true;
                break;
            }
            if veq(recs[i].p2, cur, 0.0) {
                free[slots[i]] = false;
                cur = recs[i].p1;
                let f = Edge {
                    p1: recs[i].p2,
                    p2: recs[i].p1,
                    ..recs[i]
                };
                r.push(lift(&f, back));
                found = true;
                break;
            }
        }
        if !found {
            return Vec::new();
        }
        let closing = Edge {
            p1: cur,
            p2: start,
            ..recs[0]
        };
        for t in [0.0, CLOSE_TOL] {
            for i in 0..recs.len() {
                if is_free(free, i) && edge_eq(&recs[i], &closing, t) != 0 {
                    free[slots[i]] = false;
                    let c = Edge {
                        p1: cur,
                        p2: start,
                        ..recs[i]
                    };
                    r.push(lift(&c, back));
                    return r;
                }
            }
        }
    }
}

fn add_unique(list: &mut Vec<Edge>, e: &Edge, tol: f64) {
    if list.iter().all(|f| edge_eq(e, f, tol) == 0) {
        list.push(*e);
    }
}

/// DACAD's general polygon kernel. Mode bits: 1 the part of `a` inside
/// `b`'s footprint in `a`'s plane; 4 the same in `b`'s plane; 2 and 8 the
/// outside variants.
pub fn clip(a: &Poly, b: &Poly, tol: f64, m: &Mat, mode: u32) -> Vec<Poly> {
    let (mut pa, back_a) = project(a, m);
    let (mut pb, back_b) = project(b, m);
    orient2d(&mut pa, 1.0);
    orient2d(&mut pb, 1.0);
    intersect_vertices(&mut pa, &mut pb, GRID);
    let mut lists: [Vec<Edge>; 4] = Default::default(); // bits 1, 4, 2, 8
    for (p, q, t) in [(&pa, &pb, 0.0), (&pb, &pa, tol)] {
        for e in &p.edges {
            let mut coinc = None;
            for f in &q.edges {
                if edge_eq(e, f, 0.0) != 0 {
                    coinc = Some(if parallel(e.dir(), f.dir(), tol) == 1 {
                        1
                    } else {
                        -1
                    });
                    break;
                }
            }
            let mid = e.mid();
            let im = inside(q, mid, tol);
            let b1 = veq(e.p1, mid, tol) && inside(q, e.p1, tol);
            let b2 = veq(e.p2, mid, tol) && inside(q, e.p2, tol);
            let is_in = im || (b1 != b2);
            let keep_in = match coinc {
                Some(d) => d == 1,
                None => is_in,
            };
            let keep_out = !is_in && coinc.is_none_or(|d| d == -1);
            if mode & 1 != 0 && keep_in {
                add_unique(&mut lists[0], e, t);
            }
            if mode & 4 != 0 && keep_in {
                add_unique(&mut lists[1], e, t);
            }
            if mode & 2 != 0 && keep_out {
                add_unique(&mut lists[2], e, t);
            }
            if mode & 8 != 0 && keep_out {
                add_unique(&mut lists[3], e, t);
            }
        }
    }
    for l in &mut lists {
        if l.len() < 3 {
            l.clear();
        }
    }
    let mut free: Vec<Vec<bool>> = lists.iter().map(|l| vec![true; l.len()]).collect();
    let mut out = Vec::new();
    let mut run = |k: usize, back: &Mat, src: &Poly, sign: f64, out: &mut Vec<Poly>| {
        loop {
            let r = chain(&lists[k], &mut free[k], back);
            if r.len() <= 2 {
                return;
            }
            out.push(with_attrs(src, r, src.sign * sign));
        }
    };
    if mode & 1 != 0 {
        run(0, &back_a, a, 1.0, &mut out);
    }
    if mode & 4 != 0 {
        run(1, &back_b, b, 1.0, &mut out);
    }
    if mode & 2 != 0 {
        run(2, &back_a, a, 1.0, &mut out);
        run(0, &back_a, a, -1.0, &mut out);
    }
    if mode & 8 != 0 {
        run(3, &back_b, b, 1.0, &mut out);
        run(1, &back_b, b, -1.0, &mut out);
    }
    out
}

/// Where the segment `e` meets the infinite line `l` (both flat), the
/// segment's ends included; `None` when parallel or outside the segment.
fn segment_line(e: &Edge, l: (V3, V3)) -> Option<V3> {
    let (x1, y1, x2, y2) = (e.p1.x, e.p1.y, e.p2.x, e.p2.y);
    let (x3, y3, x4, y4) = (l.0.x, l.0.y, l.1.x, l.1.y);
    if (x1 == x2 && y1 == y2)
        || (x3 == x4 && y3 == y4)
        || (x1 == x2 && x3 == x4)
        || (y1 == y2 && y3 == y4)
    {
        return None;
    }
    let (dx1, dy1, dx2, dy2) = (x2 - x1, y2 - y1, x4 - x3, y4 - y3);
    let (x, y) = if x1 == x2 && y3 == y4 {
        (x1, y3)
    } else if y1 == y2 && x3 == x4 {
        (x3, y1)
    } else if x1 == x2 {
        (x1, ((x1 - x3) * dy2) / dx2 + y3)
    } else if y1 == y2 {
        (((y1 - y3) * dx2) / dy2 + x3, y1)
    } else if x3 == x4 {
        (x3, ((x3 - x1) * dy1) / dx1 + y1)
    } else if y3 == y4 {
        (((y3 - y1) * dx1) / dy1 + x1, y3)
    } else {
        let den = dy2 * dx1 - dx2 * dy1;
        if den == 0.0 {
            return None;
        }
        let t = ((x3 - x1) * dy2 - (y3 - y1) * dx2) / den;
        (t * dx1 + x1, t * dy1 + y1)
    };
    // Closed interval tests from each end of the segment, per axis.
    let beyond = |a: f64, b: f64, v: f64| if b <= a { b < a && a < v } else { v < a };
    if beyond(x1, x2, x) || beyond(y1, y2, y) || beyond(x2, x1, x) || beyond(y2, y1, y) {
        return None;
    }
    Some(V3::new(x, y, 0.0))
}

/// The side of `p` relative to the line through `a` and `b`: 1, -1, or 0
/// on the line.
fn line_side(a: V3, b: V3, p: V3) -> i32 {
    let v = (b.y - a.y) * (b.x - p.x) + (b.x - a.x) * (p.y - b.y);
    if v == 0.0 {
        0
    } else if v < 0.0 {
        1
    } else {
        -1
    }
}

/// Lexicographic order on x, y, z.
fn lex_after(a: V3, b: V3) -> bool {
    for (p, q) in [(a.x, b.x), (a.y, b.y), (a.z, b.z)] {
        if p < q {
            return false;
        }
        if q < p {
            return true;
        }
    }
    false
}

/// A record of one half of a split: the edge, the side marks of its ends
/// and the slot of its free flag.
type HalfRec = (Edge, (i32, i32), usize);

/// DACAD's exchange sort of points into lexicographic order.
fn sort_lex(pts: &mut [V3]) {
    let mut sorted = false;
    while !sorted {
        sorted = true;
        for i in 0..pts.len().saturating_sub(1) {
            for j in i + 1..pts.len() {
                if lex_after(pts[i], pts[j]) {
                    pts.swap(i, j);
                    sorted = false;
                }
            }
        }
    }
}

/// Splits a polygon along the trace of a line in the projection `m`.
/// Mode bit 1 returns the parts on the positive side, bit 2 those on the
/// negative side. The new edges along the line are copies of `template`;
/// all others are copies of the polygon's own edges.
pub fn split_line(
    p: &Poly,
    line: (V3, V3),
    tol: f64,
    m: &Mat,
    mode: u32,
    template: &Edge,
) -> Vec<Poly> {
    let (mut q, back) = project(p, m);
    let d = line.1.sub(line.0);
    let (l0, dd) = if *m == XY {
        (line.0.flat(), d.flat())
    } else {
        (apply(m, line.0), apply(m, d))
    };
    let l = (l0, l0.add(dd));
    let n = q.edges.len();
    for i in 0..n {
        let v = q.edges[i].p1.round(SPLIT_GRID);
        q.edges[i].p1 = v;
        q.edges[(i + n - 1) % n].p2 = v;
    }
    // Records: an edge and the side marks of its ends (2 on the line).
    let mut recs: Vec<(Edge, i32, i32)> = Vec::new();
    let mut pts: Vec<V3> = Vec::new();
    for e in &q.edges {
        match segment_line(e, l) {
            Some(x) => {
                if !pts.iter().any(|&v| v.same(x)) {
                    pts.push(x);
                }
                if e.p1.same(x) {
                    recs.push((*e, 2, 0));
                } else if e.p2.same(x) {
                    recs.push((*e, 0, 2));
                } else {
                    recs.push((Edge { p2: x, ..*e }, 0, 2));
                    recs.push((Edge { p1: x, ..*e }, 2, 0));
                }
            }
            None => recs.push((*e, 0, 0)),
        }
    }
    for r in &mut recs {
        if r.1 == 0 {
            r.1 = line_side(l.0, l.1, r.0.p1);
        }
        if r.2 == 0 {
            r.2 = line_side(l.0, l.1, r.0.p2);
        }
    }
    sort_lex(&mut pts);
    // Both halves. A chord is a separate copy in each half; an original
    // record is one object, so a record on the line is used up by whichever
    // half takes it first.
    let mut halves: [Vec<HalfRec>; 2] = [Vec::new(), Vec::new()];
    let mut slots = 0;
    for w in pts.windows(2) {
        let c = Edge {
            p1: w[0],
            p2: w[1],
            ..*template
        };
        if inside(&q, c.mid(), tol) {
            halves[0].push((c, (2, 2), slots));
            halves[1].push((c, (2, 2), slots + 1));
            slots += 2;
        }
    }
    for (e, a, b) in &recs {
        if *a == 1 || *b == 1 {
            halves[0].push((*e, (*a, *b), slots));
        } else if *a == -1 || *b == -1 {
            halves[1].push((*e, (*a, *b), slots));
        } else {
            halves[0].push((*e, (*a, *b), slots));
            halves[1].push((*e, (*a, *b), slots));
        }
        slots += 1;
    }
    for h in &mut halves {
        let mut i = 0;
        while i + 1 < h.len() {
            let mut j = i + 1;
            while j < h.len() {
                if edge_eq(&h[i].0, &h[j].0, 0.0) != 0 {
                    h.remove(j);
                } else {
                    j += 1;
                }
            }
            i += 1;
        }
    }
    let mut free = vec![true; slots];
    let mut out = Vec::new();
    for (bit, h) in [(1, &halves[0]), (2, &halves[1])] {
        if mode & bit == 0 {
            continue;
        }
        let es: Vec<Edge> = h.iter().map(|r| r.0).collect();
        let sl: Vec<usize> = h.iter().map(|r| r.2).collect();
        let fl: Vec<(i32, i32)> = h.iter().map(|r| r.1).collect();
        loop {
            let r = chain_with(&es, &sl, &mut free, &back, Some(&fl));
            if r.is_empty() {
                break;
            }
            out.push(with_attrs(p, r, p.sign));
        }
    }
    out
}

/// The parts of the vertical polygon `a` below the surface `b`: over each
/// stretch of `a`'s ground line that lies within `b`'s footprint, the
/// quad from z = 0 up to `b`'s plane. The edges are copies of `a`'s first
/// edge.
pub fn below(a: &Poly, b: &Poly, tol: f64) -> Vec<Poly> {
    let Some(first) = a.edges.first() else {
        return Vec::new();
    };
    let (q, back) = project(b, &XY);
    // The longest flat segment spanned by the wall's vertices.
    let mut s = Edge {
        p1: first.p1.flat(),
        p2: first.p2.flat(),
        ..*first
    };
    for f in &a.edges {
        let e = (f.p1.flat(), f.p2.flat());
        let c = [(s.p1, e.0), (s.p1, e.1), (s.p2, e.0), (s.p2, e.1), e];
        let len = |x: (V3, V3)| x.1.sub(x.0).length();
        let all = [
            len(c[0]),
            len(c[1]),
            len(c[2]),
            len(c[3]),
            len((s.p1, s.p2)),
            len(e),
        ];
        for (k, ck) in c.iter().enumerate() {
            let own = if k < 4 { k } else { 5 };
            if (0..6).filter(|&j| j != own).all(|j| all[own] > all[j]) {
                s.p1 = ck.0;
                s.p2 = ck.1;
                break;
            }
        }
    }
    let mut pts: Vec<V3> = Vec::new();
    let add = |pts: &mut Vec<V3>, v: V3| {
        if !pts.iter().any(|&w| veq(w, v, 0.05)) {
            pts.push(v);
        }
    };
    add(&mut pts, s.p1);
    add(&mut pts, s.p2);
    for e in &q.edges {
        if let Ok(x) = super::split::crossing(&s, e, tol) {
            add(&mut pts, x);
        }
    }
    sort_lex(&mut pts);
    let mut out = Vec::new();
    for w in pts.windows(2) {
        let (p, r) = (w[0], w[1]);
        let mid = p.add(r.sub(p).scale(0.5));
        if has_point_raw(&q, mid, tol, 1) == 0 {
            continue;
        }
        let (lp, lr) = (transform(p, &back), transform(r, &back));
        let edge = |u: V3, v: V3| Edge {
            p1: u,
            p2: v,
            ..*first
        };
        let edges = vec![edge(p, r), edge(r, lr), edge(lr, lp), edge(lp, p)];
        out.push(with_attrs(a, edges, a.sign));
    }
    out
}

/// Merges collinear edges in the work projection and recomputes the
/// vertices from the plane. Down to one edge, the polygon is left as it was.
pub fn tidy(p: &mut Poly, m: &Mat, tol: f64) {
    let (mut q, back) = project(p, m);
    orient2d(&mut q, if *m == XY { 1.0 } else { -1.0 });
    let mut i: isize = 0;
    let mut j: isize = 1;
    while (i as usize) < q.edges.len() {
        let n = q.edges.len();
        if n == 1 {
            return;
        }
        let jj = j.rem_euclid(n as isize) as usize;
        let ei = q.edges[i as usize];
        if parallel(ei.dir(), q.edges[jj].dir(), tol) == 1 {
            q.edges[jj].p1 = ei.p1;
            q.edges.remove(i as usize);
            i -= 1;
            j -= 1;
        }
        i += 1;
        j += 1;
    }
    p.edges = q.edges.iter().map(|e| lift(e, &back)).collect();
}
