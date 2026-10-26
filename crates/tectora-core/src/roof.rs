//! Roof generation with a weighted straight skeleton.
//!
//! The wavefront is parameterised by height `t`. Edge `i` with pitch `p` is a
//! plane rising inward; at height `t` its line sits at inward distance
//! `s * (t - h)` from the wall line, where `s = 1 / tan(p)` and `h` is the
//! eave height. Gable edges have `s = 0`, so their line never moves and the
//! neighbouring vertices slide up along them, tracing the verges.
//!
//! The simulation starts on the roof outline (walls pushed out by their
//! horizontal overhang) at the lowest eave; like DACAD, an edge's eave
//! height applies at that outer line. An edge whose eave sits higher
//! stays put like a wall until the wavefront reaches its eave height and
//! only then starts to move ("activation"); lower roofs meeting that wall
//! produce wall-abutment lines, as in DACAD.
//!
//! The event loop is a straightforward O(n³) simulation (edge and split
//! events), which is plenty for building footprints of a few dozen edges.

use serde::Serialize;

use crate::geom::Point2;
use crate::model::{EdgeDef, RoofPart};

const EPS: f64 = 1e-9;
const POS_TOL: f64 = 1e-6;
/// Guards against runaway simulations on degenerate input.
const MAX_EVENTS: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LineKind {
    Eave,
    Verge,
    Ridge,
    Hip,
    Valley,
    /// Roof meeting a higher wall (DACAD: Wandanschluss).
    Abutment,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RoofLine {
    pub kind: LineKind,
    pub a: [f64; 3],
    pub b: [f64; 3],
}

impl RoofLine {
    pub fn length(&self) -> f64 {
        dist3(self.a, self.b)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RoofFace {
    /// Index of the footprint edge this plane belongs to.
    pub edge: usize,
    pub pitch_deg: f64,
    /// Outline in 3D, counter-clockwise seen from above.
    pub polygon: Vec<[f64; 3]>,
    /// True sloped area, m².
    pub area: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct RoofGeometry {
    pub faces: Vec<RoofFace>,
    pub lines: Vec<RoofLine>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct RoofQuantities {
    pub roof_area: f64,
    pub ridge_length: f64,
    pub hip_length: f64,
    pub hip_count: usize,
    pub valley_length: f64,
    pub valley_count: usize,
    pub eave_length: f64,
    pub verge_length: f64,
    pub abutment_length: f64,
    pub ridge_height: f64,
}

impl RoofGeometry {
    pub fn quantities(&self) -> RoofQuantities {
        let mut q = RoofQuantities {
            roof_area: self.faces.iter().map(|f| f.area).sum(),
            ..Default::default()
        };
        for l in &self.lines {
            let len = l.length();
            match l.kind {
                LineKind::Eave => q.eave_length += len,
                LineKind::Verge => q.verge_length += len,
                LineKind::Abutment => q.abutment_length += len,
                LineKind::Ridge => q.ridge_length += len,
                LineKind::Hip => {
                    q.hip_length += len;
                    q.hip_count += 1;
                }
                LineKind::Valley => {
                    q.valley_length += len;
                    q.valley_count += 1;
                }
            }
            q.ridge_height = q.ridge_height.max(l.a[2]).max(l.b[2]);
        }
        q
    }
}

#[derive(Debug, Clone, Copy)]
struct Line {
    /// Unit direction of the edge (CCW footprint, interior on the left).
    dir: Point2,
    /// Unit inward normal.
    n: Point2,
    /// `n · p` of the roof outline (wall line pushed out by the overhang).
    base: f64,
    /// Horizontal inward travel per metre of height (`1 / tan(pitch)`).
    s: f64,
    /// Height at which the edge starts moving (its eave); infinite for gables.
    t_act: f64,
}

impl Line {
    fn active(&self, t: f64) -> bool {
        t >= self.t_act - EPS
    }

    /// Speed and constant of the line valid from height `t` on, as `n · p = c + s * t`.
    fn params(&self, t: f64) -> (f64, f64) {
        if self.active(t) {
            (self.base - self.s * self.t_act, self.s)
        } else {
            (self.base, 0.0)
        }
    }
}

#[derive(Debug, Clone)]
struct Vertex {
    origin: Point2,
    t0: f64,
    vel: Point2,
    node: usize,
    left: usize,
    right: usize,
    prev: usize,
    next: usize,
    reflex: bool,
    /// Traced while one neighbour was a not-yet-active (higher) eave wall.
    abutment: bool,
    alive: bool,
}

impl Vertex {
    fn at(&self, t: f64) -> Point2 {
        Point2::new(
            self.origin.x + self.vel.x * (t - self.t0),
            self.origin.y + self.vel.y * (t - self.t0),
        )
    }
}

#[derive(Debug, Clone, Copy)]
struct Arc {
    a: usize,
    b: usize,
    left: usize,
    right: usize,
    reflex: bool,
    abutment: bool,
    /// False for face-boundary-only segments of collapsed loops.
    line: bool,
}

fn dot(a: Point2, b: Point2) -> f64 {
    a.x * b.x + a.y * b.y
}

fn cross(a: Point2, b: Point2) -> f64 {
    a.x * b.y - a.y * b.x
}

fn dist3(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Solves `n1·v = s1, n2·v = s2`. Parallel lines yield `None`.
fn solve2(n1: Point2, s1: f64, n2: Point2, s2: f64) -> Option<Point2> {
    let det = cross(n1, n2);
    if det.abs() < 1e-12 {
        return None;
    }
    Some(Point2::new(
        (s1 * n2.y - s2 * n1.y) / det,
        (n1.x * s2 - n2.x * s1) / det,
    ))
}

fn speed(e: &EdgeDef) -> f64 {
    if e.is_gable() {
        0.0
    } else {
        1.0 / e.pitch_deg.to_radians().tan()
    }
}

struct Sim {
    lines: Vec<Line>,
    verts: Vec<Vertex>,
    nodes: Vec<[f64; 3]>,
    arcs: Vec<Arc>,
    /// Eave lines as (node, node, edge), recorded when an edge activates.
    eaves: Vec<(usize, usize, usize)>,
    activated: Vec<bool>,
    /// Ridges from collapsed flat loops, as node pairs.
    ridges: Vec<(usize, usize)>,
}

impl Sim {
    fn add_node(&mut self, p: Point2, t: f64) -> usize {
        self.nodes.push([p.x, p.y, t]);
        self.nodes.len() - 1
    }

    fn velocity(&self, left: usize, right: usize, t: f64) -> Point2 {
        let (l, r) = (self.lines[left], self.lines[right]);
        let (sl, sr) = (l.params(t).1, r.params(t).1);
        match solve2(l.n, sl, r.n, sr) {
            Some(v) => v,
            // Collinear neighbours move with their common normal; opposite
            // neighbours mean the region is collapsing right now.
            None if dot(l.n, r.n) > 0.0 => Point2::new(l.n.x * sl, l.n.y * sl),
            None => Point2::new(0.0, 0.0),
        }
    }

    fn new_vertex(&mut self, p: Point2, t: f64, node: usize, left: usize, right: usize) -> usize {
        let reflex = cross(self.lines[left].dir, self.lines[right].dir) < -EPS;
        let vel = self.velocity(left, right, t);
        let waiting = |l: &Line| !l.active(t) && l.t_act.is_finite();
        let abutment = waiting(&self.lines[left]) || waiting(&self.lines[right]);
        self.verts.push(Vertex {
            origin: p,
            t0: t,
            vel,
            node,
            left,
            right,
            prev: usize::MAX,
            next: usize::MAX,
            reflex,
            abutment,
            alive: true,
        });
        self.verts.len() - 1
    }

    /// Ends vertex `v` at node `node`, recording the arc it traced.
    fn retire(&mut self, v: usize, node: usize) {
        let vx = &self.verts[v];
        self.arcs.push(Arc {
            a: vx.node,
            b: node,
            left: vx.left,
            right: vx.right,
            reflex: vx.reflex,
            abutment: vx.abutment,
            line: true,
        });
        self.verts[v].alive = false;
    }

    fn loop_len(&self, start: usize) -> usize {
        let mut n = 1;
        let mut v = self.verts[start].next;
        while v != start && n <= self.verts.len() {
            n += 1;
            v = self.verts[v].next;
        }
        n
    }

    fn loop_members(&self, start: usize) -> Vec<usize> {
        let mut out = vec![start];
        let mut v = self.verts[start].next;
        while v != start && out.len() <= self.verts.len() {
            out.push(v);
            v = self.verts[v].next;
        }
        out
    }

    /// A loop is flat when its mean thickness (2·area / perimeter) is ~0.
    fn loop_is_flat(&self, start: usize, t: f64) -> bool {
        let pts: Vec<Point2> = self
            .loop_members(start)
            .iter()
            .map(|&v| self.verts[v].at(t))
            .collect();
        let per = crate::geom::perimeter(&pts);
        per < POS_TOL || 2.0 * crate::geom::signed_area(&pts).abs() / per < 1e-5
    }

    /// Retires a flat loop: its wavefront segments become face boundaries and
    /// the line they lie on becomes a ridge.
    fn collapse_loop(&mut self, start: usize, t: f64) {
        let members = self.loop_members(start);
        let nodes: Vec<usize> = members
            .iter()
            .map(|&v| {
                let p = self.verts[v].at(t);
                self.add_node(p, t)
            })
            .collect();
        for (k, &v) in members.iter().enumerate() {
            self.retire(v, nodes[k]);
        }
        let m = members.len();
        if m < 2 {
            return;
        }
        for k in 0..m {
            let e = self.verts[members[k]].right;
            let (a, b) = (nodes[k], nodes[(k + 1) % m]);
            self.arcs.push(Arc {
                a,
                b,
                left: e,
                right: e,
                reflex: false,
                abutment: false,
                line: false,
            });
        }
        // The ridge is the extent of the flat loop.
        let pts: Vec<[f64; 3]> = nodes.iter().map(|&k| self.nodes[k]).collect();
        let (mut i0, mut i1, mut best) = (0, 0, -1.0);
        for i in 0..m {
            for j in i + 1..m {
                let d = dist3(pts[i], pts[j]);
                if d > best {
                    (i0, i1, best) = (i, j, d);
                }
            }
        }
        if best > POS_TOL {
            self.ridges.push((nodes[i0], nodes[i1]));
        }
    }

    fn edge_event_time(&self, a: usize, now: f64) -> Option<f64> {
        let va = &self.verts[a];
        let vb = &self.verts[va.next];
        let d = self.lines[va.right].dir;
        // length(t) = (B(t) - A(t)) · d, linear in t.
        let len_now = dot(
            Point2::new(vb.at(now).x - va.at(now).x, vb.at(now).y - va.at(now).y),
            d,
        );
        let rate = dot(Point2::new(vb.vel.x - va.vel.x, vb.vel.y - va.vel.y), d);
        if rate >= -EPS {
            return None;
        }
        if len_now <= POS_TOL {
            return Some(now);
        }
        Some(now - len_now / rate)
    }

    /// Time when reflex vertex `v` hits the edge leaving vertex `ea`.
    fn split_event(&self, v: usize, ea: usize, now: f64) -> Option<(f64, Point2)> {
        let vx = &self.verts[v];
        let e = self.verts[ea].right;
        if e == vx.left || e == vx.right || ea == v || self.verts[ea].next == v {
            return None;
        }
        let line = self.lines[e];
        let (c, s) = line.params(now);
        // n·V(t) = c + s t  →  n·(O + vel (t - t0)) = c + s t
        let k = dot(line.n, vx.vel) - s;
        if k > -EPS {
            return None; // not approaching
        }
        let t = (c - dot(line.n, vx.origin) + dot(line.n, vx.vel) * vx.t0) / k;
        if t < now - EPS {
            return None;
        }
        let p = vx.at(t);
        let a = self.verts[ea].at(t);
        let b = self.verts[self.verts[ea].next].at(t);
        let d = line.dir;
        let pa = dot(Point2::new(p.x - a.x, p.y - a.y), d);
        let pb = dot(Point2::new(b.x - p.x, b.y - p.y), d);
        if pa < -POS_TOL || pb < -POS_TOL {
            return None;
        }
        Some((t.max(now), p))
    }

    fn run(&mut self, mut now: f64) {
        for _ in 0..MAX_EVENTS {
            // Collapse loops that have no area left (ridges).
            for v in 0..self.verts.len() {
                if self.verts[v].alive && (self.loop_len(v) <= 2 || self.loop_is_flat(v, now)) {
                    self.collapse_loop(v, now);
                }
            }
            // Find the earliest event.
            let mut best: Option<(f64, Event)> = None;
            for v in 0..self.verts.len() {
                if !self.verts[v].alive {
                    continue;
                }
                if let Some(t) = self.edge_event_time(v, now)
                    && best.as_ref().is_none_or(|b| t < b.0 - EPS)
                {
                    best = Some((t, Event::Edge(v)));
                }
                if self.verts[v].reflex {
                    let mut ea = self.verts[v].next;
                    while ea != v {
                        if let Some((t, p)) = self.split_event(v, ea, now)
                            && best.as_ref().is_none_or(|b| t < b.0 - EPS)
                        {
                            best = Some((t, Event::Split(v, ea, p)));
                        }
                        ea = self.verts[ea].next;
                    }
                }
            }
            for (i, l) in self.lines.iter().enumerate() {
                if l.t_act.is_finite() && !self.activated[i] && self.edge_alive(i) {
                    let t = l.t_act;
                    if best.as_ref().is_none_or(|b| t < b.0 - EPS) {
                        best = Some((t, Event::Activate(i)));
                    }
                }
            }
            let Some((t, event)) = best else {
                if std::env::var_os("TECTORA_DEBUG").is_some() {
                    for (i, v) in self.verts.iter().enumerate().filter(|(_, v)| v.alive) {
                        eprintln!(
                            "left {i}: at {:?} vel {:?} L{} R{} prev {} next {}",
                            v.at(now),
                            v.vel,
                            v.left,
                            v.right,
                            v.prev,
                            v.next
                        );
                    }
                }
                return;
            };
            if std::env::var_os("TECTORA_DEBUG").is_some() {
                let alive = self.verts.iter().filter(|v| v.alive).count();
                eprintln!("t={t:.4} alive={alive} {event:?}");
            }
            now = t;
            match event {
                Event::Edge(a) => self.apply_edge(a, t),
                Event::Split(v, ea, p) => self.apply_split(v, ea, p, t),
                Event::Activate(i) => self.apply_activate(i, t),
            }
        }
    }

    fn edge_alive(&self, e: usize) -> bool {
        self.verts.iter().any(|v| v.alive && v.right == e)
    }

    /// Edge `e` reaches its eave height and starts to move: every vertex on
    /// it bends here, and its eave line is recorded.
    fn apply_activate(&mut self, e: usize, t: f64) {
        self.activated[e] = true;
        let on_edge: Vec<usize> = (0..self.verts.len())
            .filter(|&v| {
                self.verts[v].alive && (self.verts[v].left == e || self.verts[v].right == e)
            })
            .collect();
        let mut replaced = vec![usize::MAX; self.verts.len()];
        for &v in &on_edge {
            let p = self.verts[v].at(t);
            let node = self.add_node(p, t);
            let (left, right, prev, next) = (
                self.verts[v].left,
                self.verts[v].right,
                self.verts[v].prev,
                self.verts[v].next,
            );
            self.retire(v, node);
            let w = self.new_vertex(p, t, node, left, right);
            replaced[v] = w;
            self.verts[w].prev = prev;
            self.verts[w].next = next;
        }
        // Re-link neighbours (they may themselves have been replaced).
        for &v in &on_edge {
            let w = replaced[v];
            let (prev, next) = (self.verts[w].prev, self.verts[w].next);
            let prev = if replaced.get(prev).is_some_and(|&r| r != usize::MAX) {
                replaced[prev]
            } else {
                prev
            };
            let next = if replaced.get(next).is_some_and(|&r| r != usize::MAX) {
                replaced[next]
            } else {
                next
            };
            self.verts[w].prev = prev;
            self.verts[w].next = next;
            self.verts[prev].next = w;
            self.verts[next].prev = w;
        }
        for &v in &on_edge {
            let w = replaced[v];
            if self.verts[w].right == e {
                let b = self.verts[w].next;
                self.eaves.push((self.verts[w].node, self.verts[b].node, e));
            }
        }
    }

    fn apply_edge(&mut self, a: usize, t: f64) {
        let b = self.verts[a].next;
        let pa = self.verts[a].at(t);
        let pb = self.verts[b].at(t);
        let p = Point2::new((pa.x + pb.x) / 2.0, (pa.y + pb.y) / 2.0);
        let node = self.add_node(p, t);
        let (prev, next) = (self.verts[a].prev, self.verts[b].next);
        let (left, right) = (self.verts[a].left, self.verts[b].right);
        self.retire(a, node);
        self.retire(b, node);
        if prev == b {
            return; // the loop was just these two vertices
        }
        let n = self.new_vertex(p, t, node, left, right);
        self.verts[n].prev = prev;
        self.verts[n].next = next;
        self.verts[prev].next = n;
        self.verts[next].prev = n;
    }

    fn apply_split(&mut self, v: usize, ea: usize, p: Point2, t: f64) {
        let node = self.add_node(p, t);
        let e = self.verts[ea].right;
        let eb = self.verts[ea].next;
        let (prev, next) = (self.verts[v].prev, self.verts[v].next);
        let (left, right) = (self.verts[v].left, self.verts[v].right);
        self.retire(v, node);
        let v1 = self.new_vertex(p, t, node, left, e);
        let v2 = self.new_vertex(p, t, node, e, right);
        // Loop 1: prev → v1 → eb …   Loop 2: ea → v2 → next …
        self.verts[prev].next = v1;
        self.verts[v1].prev = prev;
        self.verts[v1].next = eb;
        self.verts[eb].prev = v1;
        self.verts[ea].next = v2;
        self.verts[v2].prev = ea;
        self.verts[v2].next = next;
        self.verts[next].prev = v2;
    }
}

#[derive(Debug)]
enum Event {
    Edge(usize),
    Split(usize, usize, Point2),
    Activate(usize),
}

/// Generates the roof over one footprint part. The part must be CCW
/// (see [`RoofPart::normalize_ccw`]).
pub fn generate(part: &RoofPart) -> RoofGeometry {
    let edges = &part.edges;
    let n = edges.len();
    if n < 3 {
        return RoofGeometry::default();
    }
    let pts = part.outline();
    let lines: Vec<Line> = (0..n)
        .map(|i| {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            let len = a.distance(b);
            let dir = match edges[i].direction_rad {
                Some(ang) if len < POS_TOL => Point2::new(ang.cos(), ang.sin()),
                _ => Point2::new((b.x - a.x) / len.max(EPS), (b.y - a.y) / len.max(EPS)),
            };
            let nrm = Point2::new(-dir.y, dir.x);
            let e = &edges[i];
            // DACAD's eave height is measured at the outer (overhang) line.
            let t_act = if e.is_gable() { f64::INFINITY } else { e.eave_height };
            Line {
                dir,
                n: nrm,
                base: dot(nrm, a) - e.overhang,
                s: speed(e),
                t_act,
            }
        })
        .collect();
    let start = lines.iter().map(|l| l.t_act).fold(f64::INFINITY, f64::min);
    if !start.is_finite() {
        return RoofGeometry::default(); // only gables: no roof planes
    }

    let activated = lines.iter().map(|l| l.active(start)).collect();
    let mut sim = Sim {
        lines,
        verts: Vec::new(),
        nodes: Vec::new(),
        arcs: Vec::new(),
        eaves: Vec::new(),
        activated,
        ridges: Vec::new(),
    };
    // Initial vertex i sits between edge i-1 (left) and edge i (right),
    // on the roof outline.
    for i in 0..n {
        let (l, r) = ((i + n - 1) % n, i);
        let (ll, rl) = (sim.lines[l], sim.lines[r]);
        let p = solve2(ll.n, ll.base, rl.n, rl.base).unwrap_or(pts[i]);
        let node = sim.add_node(p, start);
        sim.new_vertex(p, start, node, l, r);
    }
    for i in 0..n {
        sim.verts[i].prev = (i + n - 1) % n;
        sim.verts[i].next = (i + 1) % n;
        if sim.lines[i].active(start) {
            let (a, b) = (sim.verts[i].node, sim.verts[(i + 1) % n].node);
            sim.eaves.push((a, b, i));
        }
    }
    sim.run(start);
    build_geometry(part, &sim)
}

fn build_geometry(part: &RoofPart, sim: &Sim) -> RoofGeometry {
    let edges = &part.edges;
    // Merge coincident nodes so faces chain cleanly.
    let mut canon: Vec<usize> = (0..sim.nodes.len()).collect();
    for i in 0..sim.nodes.len() {
        for j in 0..i {
            if canon[j] == j && dist3(sim.nodes[i], sim.nodes[j]) < 1e-6 {
                canon[i] = j;
                break;
            }
        }
    }
    let arcs: Vec<Arc> = sim
        .arcs
        .iter()
        .map(|a| Arc {
            a: canon[a.a],
            b: canon[a.b],
            ..*a
        })
        .filter(|a| a.a != a.b)
        .collect();
    let eaves: Vec<(usize, usize, usize)> = sim
        .eaves
        .iter()
        .map(|&(a, b, e)| (canon[a], canon[b], e))
        .filter(|&(a, b, _)| a != b)
        .collect();

    let mut geo = RoofGeometry::default();
    for &(a, b, _) in &eaves {
        geo.lines.push(RoofLine {
            kind: LineKind::Eave,
            a: sim.nodes[a],
            b: sim.nodes[b],
        });
    }
    // Each arc borders two faces; emit it once with its kind.
    let mut seen: Vec<(usize, usize)> = Vec::new();
    for &(a, b) in &sim.ridges {
        let (a, b) = (canon[a], canon[b]);
        if a != b {
            seen.push((a.min(b), a.max(b)));
            geo.lines.push(RoofLine {
                kind: LineKind::Ridge,
                a: sim.nodes[a],
                b: sim.nodes[b],
            });
        }
    }
    for arc in arcs.iter().filter(|a| a.line) {
        let key = (arc.a.min(arc.b), arc.a.max(arc.b));
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        let (pa, pb) = (sim.nodes[arc.a], sim.nodes[arc.b]);
        let kind = if edges[arc.left].is_gable() || edges[arc.right].is_gable() {
            LineKind::Verge
        } else if arc.abutment {
            LineKind::Abutment
        } else if (pa[2] - pb[2]).abs() < 1e-6 {
            LineKind::Ridge
        } else if arc.reflex {
            LineKind::Valley
        } else {
            LineKind::Hip
        };
        geo.lines.push(RoofLine { kind, a: pa, b: pb });
    }

    for &(start, end, i) in &eaves {
        let t_act = sim.lines[i].t_act;
        // Arcs traced while the edge was still a wall are not part of its plane.
        let mut segs: Vec<(usize, usize)> = arcs
            .iter()
            .filter(|a| (a.left == i || a.right == i) && sim.nodes[a.a][2] >= t_act - 1e-7)
            .map(|a| (a.a, a.b))
            .collect();
        // Walk from the eave end back to its start through skeleton arcs.
        let mut ring = vec![start, end];
        let mut cur = end;
        while cur != start {
            let Some(k) = segs.iter().position(|&(x, y)| x == cur || y == cur) else {
                break;
            };
            let (x, y) = segs.swap_remove(k);
            cur = if x == cur { y } else { x };
            if cur != start {
                ring.push(cur);
            }
        }
        let polygon: Vec<[f64; 3]> = ring.iter().map(|&k| sim.nodes[k]).collect();
        let area = area3(&polygon);
        geo.faces.push(RoofFace {
            edge: i,
            pitch_deg: edges[i].pitch_deg,
            polygon,
            area,
        });
    }
    geo
}

/// Area of a planar 3D polygon (Newell's method).
fn area3(poly: &[[f64; 3]]) -> f64 {
    let n = poly.len();
    let (mut x, mut y, mut z) = (0.0, 0.0, 0.0);
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        x += (a[1] - b[1]) * (a[2] + b[2]);
        y += (a[2] - b[2]) * (a[0] + b[0]);
        z += (a[0] - b[0]) * (a[1] + b[1]);
    }
    (x * x + y * y + z * z).sqrt() / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    fn part(points: &[(f64, f64)], pitch: &[f64]) -> RoofPart {
        let edges = points
            .iter()
            .zip(pitch)
            .map(|(&(x, y), &p)| EdgeDef {
                start: Point2::new(x, y),
                pitch_deg: p,
                eave_height: 0.0,
                overhang: 0.0,
                half_hip: None,
                direction_rad: None,
            })
            .collect();
        let mut part = RoofPart {
            name: "t".into(),
            edges,
        };
        part.normalize_ccw();
        part
    }

    const RECT: [(f64, f64); 4] = [(0.0, 0.0), (10.0, 0.0), (10.0, 6.0), (0.0, 6.0)];

    #[test]
    fn hip_roof_on_rectangle() {
        let q = generate(&part(&RECT, &[45.0; 4])).quantities();
        assert_relative_eq!(q.ridge_length, 4.0, epsilon = 1e-6);
        assert_relative_eq!(q.ridge_height, 3.0, epsilon = 1e-6);
        assert_relative_eq!(q.roof_area, 60.0 * 2f64.sqrt(), epsilon = 1e-6);
        assert_eq!(q.hip_count, 4);
        assert_relative_eq!(q.hip_length, 4.0 * (27f64).sqrt(), epsilon = 1e-6);
        assert_relative_eq!(q.eave_length, 32.0, epsilon = 1e-6);
    }

    #[test]
    fn gable_roof_on_rectangle() {
        let q = generate(&part(&RECT, &[45.0, 90.0, 45.0, 90.0])).quantities();
        assert_relative_eq!(q.ridge_length, 10.0, epsilon = 1e-6);
        assert_relative_eq!(q.roof_area, 60.0 * 2f64.sqrt(), epsilon = 1e-6);
        assert_eq!(q.hip_count, 0);
        assert_relative_eq!(q.verge_length, 4.0 * 18f64.sqrt(), epsilon = 1e-6);
        assert_relative_eq!(q.eave_length, 20.0, epsilon = 1e-6);
    }

    #[test]
    fn pyramid_on_square() {
        let sq = [(0.0, 0.0), (8.0, 0.0), (8.0, 8.0), (0.0, 8.0)];
        let q = generate(&part(&sq, &[45.0; 4])).quantities();
        assert_relative_eq!(q.ridge_length, 0.0, epsilon = 1e-6);
        assert_eq!(q.hip_count, 4);
        assert_relative_eq!(q.ridge_height, 4.0, epsilon = 1e-6);
    }

    #[test]
    fn l_shape_has_one_valley() {
        let l = [
            (0.0, 0.0),
            (10.0, 0.0),
            (10.0, 4.0),
            (4.0, 4.0),
            (4.0, 10.0),
            (0.0, 10.0),
        ];
        let geo = generate(&part(&l, &[45.0; 6]));
        let q = geo.quantities();
        assert_eq!(q.valley_count, 1);
        // Plan area 64 m² at 45° everywhere.
        assert_relative_eq!(q.roof_area, 64.0 * 2f64.sqrt(), epsilon = 1e-6);
        assert_eq!(geo.faces.len(), 6);
    }

    #[test]
    fn asymmetric_pitches_keep_area_consistent() {
        // Shed-like: front 30°, back 60°, gables on the sides.
        let g = generate(&part(&RECT, &[30.0, 90.0, 60.0, 90.0]));
        let q = g.quantities();
        let plan: f64 = g
            .faces
            .iter()
            .map(|f| f.area * f.pitch_deg.to_radians().cos())
            .sum();
        assert_relative_eq!(plan, 60.0, epsilon = 1e-6);
        assert_relative_eq!(q.ridge_length, 10.0, epsilon = 1e-6);
    }
}
