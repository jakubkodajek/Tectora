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
//! height applies at that outer line. Each edge moves through its planes
//! (see [`crate::model::EdgeDef::planes`]): until the first one starts it stays put like a
//! wall, and at every plane start all vertices on it bend. Checked against
//! roofs calculated by DACAD itself (`tests/fixtures/dacad_cases`):
//!
//! * A plane meeting a still-waiting wall traces a verge (OL/OR, or OS when
//!   not square to the eave), also where
//!   eaves sit at different heights (case C10), and a horizontal top edge
//!   against a wall is a Pult line P (shed, C07).
//! * A clipped gable (Krüppelwalm) waits up to its half-hip height; the
//!   lower edge of its plane counts as eave T (C08).
//! * A pitch break leaves a Knick line c and the hips run through it (C14).
//! * Parallel planes of different pitch (a recessed or projecting wall)
//!   meet in a horizontal line c; the plane with priority, else the steeper
//!   one, ends there and its edge continues on the other plane (C18–C23).
//!   A plane rising from an inside corner along a wall makes a valley K.
//!
//! The event loop is a straightforward O(n³) simulation (edge and split
//! events), which is plenty for building footprints of a few dozen edges.

use serde::Serialize;

use crate::geom::Point2;
use crate::model::{EdgeDef, RoofPart};

const EPS: f64 = 1e-9;
/// Walls narrower than this between planes moving in step are dropped.
const MERGE_TOL: f64 = 2e-3;
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
    /// Horizontal top edge of a plane against a wall (DACAD: Pult).
    Pult,
    /// Pitch break between two planes of one wall (DACAD: Knick).
    Knick,
    /// Horizontal valley where two roofs meet (DACAD: Schlucht).
    Gorge,
    /// Roof meeting the wall of another building (DACAD: Wandanschluss).
    /// Not generated yet: within one part DACAD reports verges instead.
    Abutment,
    /// Wall abutment on the downhill side, e.g. below a chimney (WV).
    AbutmentFront,
    /// Wall abutment on the uphill side (WH).
    AbutmentBack,
    /// Wall abutment along the slope (WS).
    AbutmentSide,
}

/// Which end of its roof plane a verge lies on, seen from outside the eave.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VergeSide {
    Left,
    Right,
    /// Not square to the eave in plan (DACAD: OS).
    Oblique,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RoofLine {
    pub kind: LineKind,
    pub a: [f64; 3],
    pub b: [f64; 3],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub side: Option<VergeSide>,
}

impl RoofLine {
    fn new(kind: LineKind, a: [f64; 3], b: [f64; 3]) -> Self {
        Self {
            kind,
            a,
            b,
            side: None,
        }
    }

    pub fn length(&self) -> f64 {
        dist3(self.a, self.b)
    }

    /// DACAD's line code, with verges split into OL (left), OR (right) and
    /// OS (oblique), seen from outside the plane's eave (cases C15–C17).
    pub fn dacad_code(&self) -> &'static str {
        match (self.kind, self.side) {
            (LineKind::Verge, Some(VergeSide::Left)) => "OL",
            (LineKind::Verge, Some(VergeSide::Right)) => "OR",
            (LineKind::Verge, Some(VergeSide::Oblique)) => "OS",
            (kind, _) => kind.dacad_code(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RoofFace {
    /// Index of the footprint edge this plane belongs to.
    pub edge: usize,
    pub pitch_deg: f64,
    /// Outline in 3D, counter-clockwise seen from above.
    pub polygon: Vec<[f64; 3]>,
    /// True sloped area, m², less windows and chimneys.
    pub area: f64,
    /// Area deducted for windows and chimneys, m².
    #[serde(default)]
    pub deducted: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct RoofGeometry {
    pub faces: Vec<RoofFace>,
    pub lines: Vec<RoofLine>,
    /// Vertical gable walls above the eave line (for rendering).
    pub gables: Vec<Vec<[f64; 3]>>,
    /// Area of the clad gables above their eave (see [`EdgeDef::clad`]),
    /// part of the roof area.
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub clad_area: f64,
    /// Area deducted per window, chimney and dormer (for norm rules).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub openings: Vec<Opening>,
    /// Line lengths removed by chimneys and dormers, by DACAD code.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cut_lines: Vec<(String, f64)>,
    /// Negative lines DACAD adds where a chimney side runs right beside a
    /// ridge or hip, by DACAD code; the line itself stays whole.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub minus_lines: Vec<(String, f64)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OpeningKind {
    Window,
    Chimney,
    Dormer,
    /// A hole in the roof (DACAD: Öffnung).
    Hole,
}

/// Roof area one opening took away, m².
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Opening {
    pub kind: OpeningKind,
    pub area: f64,
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
    pub pult_length: f64,
    pub knick_length: f64,
    #[serde(default)]
    pub gorge_length: f64,
    pub abutment_length: f64,
    pub ridge_height: f64,
}

impl RoofQuantities {
    /// Adds another part's quantities (the ridge height is the maximum).
    pub fn add(&mut self, q: &RoofQuantities) {
        self.roof_area += q.roof_area;
        self.ridge_length += q.ridge_length;
        self.hip_length += q.hip_length;
        self.hip_count += q.hip_count;
        self.valley_length += q.valley_length;
        self.valley_count += q.valley_count;
        self.eave_length += q.eave_length;
        self.verge_length += q.verge_length;
        self.pult_length += q.pult_length;
        self.knick_length += q.knick_length;
        self.gorge_length += q.gorge_length;
        self.abutment_length += q.abutment_length;
        self.ridge_height = self.ridge_height.max(q.ridge_height);
    }
}

impl LineKind {
    /// DACAD's line code: T Traufe, F First, G Grat, K Kehle, O Ortgang,
    /// P Pult, c Knick, S Schlucht, W Wandanschluss. See [`RoofLine::dacad_code`] for
    /// the verge sides.
    pub fn dacad_code(self) -> &'static str {
        match self {
            LineKind::Eave => "T",
            LineKind::Ridge => "F",
            LineKind::Hip => "G",
            LineKind::Valley => "K",
            LineKind::Verge => "O",
            LineKind::Pult => "P",
            LineKind::Knick => "c",
            LineKind::Gorge => "S",
            LineKind::Abutment => "W",
            LineKind::AbutmentFront => "WV",
            LineKind::AbutmentBack => "WH",
            LineKind::AbutmentSide => "WS",
        }
    }
}

impl RoofGeometry {
    pub fn quantities(&self) -> RoofQuantities {
        let mut q = RoofQuantities {
            roof_area: self.faces.iter().map(|f| f.area).sum::<f64>() + self.clad_area,
            ..Default::default()
        };
        for l in &self.lines {
            let len = l.length();
            match l.kind {
                LineKind::Eave => q.eave_length += len,
                LineKind::Verge => q.verge_length += len,
                LineKind::Pult => q.pult_length += len,
                LineKind::Knick => q.knick_length += len,
                LineKind::Gorge => q.gorge_length += len,
                LineKind::Abutment
                | LineKind::AbutmentFront
                | LineKind::AbutmentBack
                | LineKind::AbutmentSide => q.abutment_length += len,
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

/// One plane of an edge: from height `t` on, the edge line is `n · p = c + s * t`.
#[derive(Debug, Clone, Copy)]
struct Stage {
    t: f64,
    /// Horizontal inward travel per metre of height (`1 / tan(pitch)`).
    s: f64,
    c: f64,
    pitch_deg: f64,
}

#[derive(Debug, Clone)]
struct Line {
    /// Unit direction of the edge (CCW footprint, interior on the left).
    dir: Point2,
    /// Unit inward normal.
    n: Point2,
    /// `n · p` of the roof outline (wall line pushed out by the overhang).
    base: f64,
    /// The edge's planes by start height; empty for a plain gable.
    stages: Vec<Stage>,
    priority: u8,
    /// Zero-length marker edge (see `EdgeDef::direction_rad`).
    marker: bool,
    /// A wall with a higher eave than the lowest, waiting at its wall line
    /// (see [`generate`]).
    raised: bool,
}

impl Line {
    fn new(dir: Point2, n: Point2, base: f64, planes: &[(f64, f64)], priority: u8) -> Self {
        let mut stages: Vec<Stage> = Vec::with_capacity(planes.len());
        let (mut c, mut s) = (base, 0.0);
        for &(t, pitch_deg) in planes {
            let s_new = if (pitch_deg - 90.0).abs() <= 1e-9 {
                0.0
            } else {
                1.0 / pitch_deg.to_radians().tan()
            };
            // Continuous at t: c + s t = c' + s' t.
            c += (s - s_new) * t;
            s = s_new;
            stages.push(Stage { t, s, c, pitch_deg });
        }
        Self {
            dir,
            n,
            base,
            stages,
            priority,
            marker: false,
            raised: false,
        }
    }

    /// Height at which the edge starts moving (its eave); infinite for gables.
    fn t_act(&self) -> f64 {
        self.stages.first().map_or(f64::INFINITY, |s| s.t)
    }

    fn active(&self, t: f64) -> bool {
        t >= self.t_act() - EPS
    }

    /// A plane at height `t`: active and not turned vertical (a gable above
    /// a hip, DACAD sample DACHBRUC).
    fn sloped(&self, t: f64) -> bool {
        self.active(t)
            && self
                .stage_at(t)
                .is_some_and(|k| self.stages[k].s.abs() > 1e-12)
    }

    /// Index of the plane valid at height `t`.
    fn stage_at(&self, t: f64) -> Option<usize> {
        self.stages.iter().rposition(|s| t >= s.t - EPS)
    }

    /// Speed and constant of the line valid from height `t` on, as `n · p = c + s * t`.
    fn params(&self, t: f64) -> (f64, f64) {
        match self.stage_at(t) {
            Some(k) => (self.stages[k].c, self.stages[k].s),
            None => (self.base, 0.0),
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
    /// Whether the left / right edge was still a wall (not yet a plane)
    /// when this vertex started.
    waiting: [bool; 2],
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
    waiting: [bool; 2],
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

/// The lower edge of plane `stage` of edge `edge`, from node `a` to `b`.
#[derive(Debug, Clone, Copy)]
struct Bottom {
    a: usize,
    b: usize,
    edge: usize,
    stage: usize,
}

struct Sim {
    lines: Vec<Line>,
    verts: Vec<Vertex>,
    nodes: Vec<[f64; 3]>,
    arcs: Vec<Arc>,
    /// Plane bottoms (eaves and pitch breaks), recorded when an edge bends.
    bottoms: Vec<Bottom>,
    /// Number of stages each edge has entered.
    applied: Vec<usize>,
    /// Lines from collapsed flat loops (ridges or Pult lines), as node pairs.
    ridges: Vec<(usize, usize, LineKind)>,
}

impl Sim {
    fn add_node(&mut self, p: Point2, t: f64) -> usize {
        self.nodes.push([p.x, p.y, t]);
        self.nodes.len() - 1
    }

    fn velocity(&self, left: usize, right: usize, t: f64) -> Point2 {
        let (l, r) = (&self.lines[left], &self.lines[right]);
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
        let waiting = [!self.lines[left].sloped(t), !self.lines[right].sloped(t)];
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
            waiting,
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
            waiting: vx.waiting,
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

    /// A loop is flat when its mean thickness (2·area / perimeter) is ~0,
    /// allowing for DACAD's inputs rounded to 0.1 mm: planes meant to be one
    /// can then stay a few hundredths of a millimetre apart (case C72).
    fn loop_is_flat(&self, start: usize, t: f64) -> bool {
        let pts: Vec<Point2> = self
            .loop_members(start)
            .iter()
            .map(|&v| self.verts[v].at(t))
            .collect();
        let per = crate::geom::perimeter(&pts);
        per < POS_TOL || 2.0 * crate::geom::signed_area(&pts).abs() / per < 1e-4
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
                waiting: [false; 2],
                line: false,
            });
        }
        let pts: Vec<[f64; 3]> = nodes.iter().map(|&k| self.nodes[k]).collect();
        // A flat loop that branches (ridges crossing, sample HINT04) runs
        // along each ridge twice, once on either side: each such pair of
        // wavefront segments is a ridge.
        let segs: Vec<(usize, usize, usize)> = (0..m)
            .filter(|&k| dist3(pts[k], pts[(k + 1) % m]) > POS_TOL)
            .map(|k| (k, (k + 1) % m, self.verts[members[k]].right))
            .collect();
        let twin = |&(a, b, _): &(usize, usize, usize)| {
            segs.iter().position(|&(c, d, _)| {
                dist3(pts[a], pts[d]) < POS_TOL && dist3(pts[b], pts[c]) < POS_TOL
            })
        };
        if segs.len() > 2 && segs.iter().all(|s| twin(s).is_some()) {
            for (i, s) in segs.iter().enumerate() {
                let j = twin(s).expect("checked");
                if j < i {
                    continue;
                }
                let (l1, l2) = (&self.lines[s.2], &self.lines[segs[j].2]);
                let kind = if l1.sloped(t) && l2.sloped(t) && dot(l1.n, l2.n) < 1.0 - 1e-9 {
                    LineKind::Ridge
                } else {
                    LineKind::Pult
                };
                self.ridges.push((nodes[s.0], nodes[s.1], kind));
            }
            return;
        }
        // The ridge is the extent of the flat loop.
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
            // Two planes meeting make a ridge; a single plane ending against
            // walls makes a Pult line (DACAD case C07).
            let mut normals: Vec<Point2> = Vec::new();
            for &v in &members {
                let l = &self.lines[self.verts[v].right];
                if l.sloped(t) && !normals.iter().any(|n| dot(*n, l.n) > 1.0 - 1e-9) {
                    normals.push(l.n);
                }
            }
            let kind = if normals.len() <= 1 {
                LineKind::Pult
            } else {
                LineKind::Ridge
            };
            self.ridges.push((nodes[i0], nodes[i1], kind));
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
        // A wall squeezed to nothing between two planes that now move in
        // step (after an overtake, or planes that coincide) is gone, even
        // though it no longer shrinks. DACAD stores heights to the
        // millimetre, so planes that coincide may leave a sliver that thin
        // (sample TRAING08: eaves 3.25 and 2.529 1.2 m apart at 31°).
        if len_now <= MERGE_TOL && rate.abs() < EPS && !self.lines[va.right].marker {
            return Some(now);
        }
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
        let line = &self.lines[e];
        let (c, s) = self.lines[e].params(now);
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
                    && better(&best, t, 2)
                {
                    best = Some((t, Event::Edge(v)));
                }
                if self.verts[v].reflex {
                    let mut ea = self.verts[v].next;
                    while ea != v {
                        if let Some((t, p)) = self.split_event(v, ea, now)
                            && better(&best, t, 1)
                        {
                            best = Some((t, Event::Split(v, ea, p)));
                        }
                        ea = self.verts[ea].next;
                    }
                }
            }
            if let Some((t, stop, cont)) = self.overtake_event(now)
                && better(&best, t, 0)
            {
                best = Some((t, Event::Overtake(stop, cont)));
            }
            for (i, l) in self.lines.iter().enumerate() {
                if let Some(st) = l.stages.get(self.applied[i])
                    && self.edge_alive(i)
                    && better(&best, st.t, 0)
                {
                    best = Some((st.t, Event::Bend(i)));
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
                Event::Bend(i) => self.apply_bend(i, t),
                Event::Overtake(stop, cont) => {
                    // From here on the stopped edge runs on the other plane.
                    let k = self.applied[stop];
                    let (c, s) = self.lines[cont].params(t);
                    let cont_line = &self.lines[cont];
                    let pitch_deg = cont_line.stages[cont_line.stage_at(t).unwrap_or(0)].pitch_deg;
                    let line = &mut self.lines[stop];
                    line.stages.truncate(k);
                    line.stages.push(Stage { t, s, c, pitch_deg });
                    self.apply_bend(stop, t);
                }
            }
        }
    }

    /// Extent of edge `e`'s wavefront at height `t` along its direction.
    fn extent(&self, e: usize, t: f64) -> Option<(f64, f64)> {
        let d = self.lines[e].dir;
        let mut out: Option<(f64, f64)> = None;
        for v in self.verts.iter().filter(|v| v.alive && v.right == e) {
            let (a, b) = (v.at(t), self.verts[v.next].at(t));
            let (pa, pb) = (dot(a, d), dot(b, d));
            let (lo, hi) = (pa.min(pb), pa.max(pb));
            out = Some(out.map_or((lo, hi), |(l, h)| (l.min(lo), h.max(hi))));
        }
        out
    }

    /// The next moment two parallel planes of different pitch meet in a
    /// horizontal line where their wavefronts touch. DACAD ends one of them
    /// there (a Knick line c): the one with priority, otherwise the steeper
    /// one (cases C18–C23). Returns `(t, stopping edge, continuing edge)`.
    fn overtake_event(&self, now: f64) -> Option<(f64, usize, usize)> {
        let alive: Vec<usize> = (0..self.lines.len())
            .filter(|&e| self.lines[e].sloped(now) && self.edge_alive(e))
            .collect();
        let mut best: Option<(f64, usize, usize)> = None;
        for (k, &i) in alive.iter().enumerate() {
            for &j in &alive[k + 1..] {
                let (li, lj) = (&self.lines[i], &self.lines[j]);
                if dot(li.n, lj.n) < 1.0 - 1e-9 {
                    continue;
                }
                let ((ci, si), (cj, sj)) = (li.params(now), lj.params(now));
                if (si - sj).abs() < 1e-9 {
                    continue;
                }
                let t = (cj - ci) / (si - sj);
                if t < now - EPS || best.is_some_and(|b| b.0 <= t) {
                    continue;
                }
                let (Some(ei), Some(ej)) = (self.extent(i, t), self.extent(j, t)) else {
                    continue;
                };
                if ei.0 > ej.1 + POS_TOL || ej.0 > ei.1 + POS_TOL {
                    continue;
                }
                // Planes that only touch end to end across a real wall do not
                // meet: that wall's plane goes on between them, e.g. a short
                // wall whose neighbours' overhang lines coincide (mass case
                // R1_0229). Across a marker or a gable they do (DACAD case C18).
                let touching = ei.0.max(ej.0) >= ei.1.min(ej.1) - POS_TOL;
                if touching && self.wall_between(i, j, t) {
                    continue;
                }
                let stop_i = match (li.priority > 0, lj.priority > 0) {
                    (true, false) => true,
                    (false, true) => false,
                    _ => si < sj,
                };
                best = Some(if stop_i { (t, i, j) } else { (t, j, i) });
            }
        }
        best
    }

    /// Whether edges `i` and `j` are joined through one live sloped edge
    /// that is not a marker.
    fn wall_between(&self, i: usize, j: usize, now: f64) -> bool {
        self.verts.iter().any(|v| {
            if !v.alive {
                return false;
            }
            let w = &self.verts[v.next];
            let m = v.right;
            ((v.left == i && w.right == j) || (v.left == j && w.right == i))
                && m != i
                && m != j
                && !self.lines[m].marker
                && self.lines[m].sloped(now)
        })
    }

    fn edge_alive(&self, e: usize) -> bool {
        self.verts.iter().any(|v| v.alive && v.right == e)
    }

    /// Edge `e` reaches the start of its next plane: every vertex on it
    /// bends here, and the plane's lower edge is recorded.
    fn apply_bend(&mut self, e: usize, t: f64) {
        let stage = self.applied[e];
        self.applied[e] += 1;
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
                self.bottoms.push(Bottom {
                    a: self.verts[w].node,
                    b: self.verts[b].node,
                    edge: e,
                    stage,
                });
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
        self.absorb(n, t);
    }

    /// A plane that has just caught up with a parallel wall beside it (the
    /// wall in line with its wavefront, DACAD case C69) covers the wall at
    /// once: the wall's edge collapses to where the plane meets the edge
    /// beyond it.
    fn absorb(&mut self, n: usize, t: f64) {
        let (left, right) = (self.verts[n].left, self.verts[n].right);
        let (l, r) = (&self.lines[left], &self.lines[right]);
        if dot(l.n, r.n) < 1.0 - 1e-9 {
            return;
        }
        let (sl, sr) = (l.params(t).1, r.params(t).1);
        if (sl - sr).abs() < EPS {
            return;
        }
        // The slower edge is swallowed; `w` is its vertex on the far side.
        let (w, keep, beyond) = if sr < sl {
            let w = self.verts[n].next;
            (w, left, self.verts[w].right)
        } else {
            let w = self.verts[n].prev;
            (w, right, self.verts[w].left)
        };
        if w == n || beyond == keep {
            return;
        }
        // A plane with priority ends at the wall instead and runs on as the
        // wall (a Knick line c, DACAD KRUPA006 variants K1, K2).
        let slow = if sr < sl { right } else { left };
        if self.lines[keep].priority > 0 && self.lines[slow].priority == 0 {
            let k = self.applied[keep];
            let (c, s) = self.lines[slow].params(t);
            let line = &mut self.lines[keep];
            line.stages.truncate(k);
            line.stages.push(Stage {
                t,
                s,
                c,
                pitch_deg: 90.0,
            });
            self.apply_bend(keep, t);
            return;
        }
        let (lk, lb) = (&self.lines[keep], &self.lines[beyond]);
        let Some(p) = solve2(
            lk.n,
            lk.params(t).0 + lk.params(t).1 * t,
            lb.n,
            lb.params(t).0 + lb.params(t).1 * t,
        ) else {
            return;
        };
        let node = self.add_node(p, t);
        let (prev, next) = if sr < sl {
            (self.verts[n].prev, self.verts[w].next)
        } else {
            (self.verts[w].prev, self.verts[n].next)
        };
        // The kept plane covers the swallowed edge at once: the level
        // stretch it sweeps ends in the air above it, an eave (DACAD C69).
        self.retire(n, node);
        if let Some(arc) = self.arcs.last_mut() {
            arc.line = false;
            self.ridges.push((arc.a, arc.b, LineKind::Eave));
        }
        self.retire(w, node);
        if prev == w || next == n {
            return;
        }
        let (a, b) = if sr < sl {
            (keep, beyond)
        } else {
            (beyond, keep)
        };
        let m = self.new_vertex(p, t, node, a, b);
        self.verts[m].prev = prev;
        self.verts[m].next = next;
        self.verts[prev].next = m;
        self.verts[next].prev = m;
        self.absorb(m, t);
    }

    fn apply_split(&mut self, v: usize, ea: usize, p: Point2, t: f64) {
        // Two reflex vertices arriving at the same point: a vertex event.
        let other = (0..self.verts.len()).find(|&w| {
            w != v
                && self.verts[w].alive
                && (self.verts[w].reflex || w == ea || w == self.verts[ea].next)
                && self.verts[w].at(t).approx_eq(p, 1e-6)
        });
        if let Some(w) = other {
            let same_loop = self.loop_members(v).contains(&w);
            if same_loop && self.verts[w].next != v && self.verts[v].next != w {
                return self.apply_vertex_event(v, w, p, t);
            }
        }
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
        self.absorb(v1, t);
        if self.verts[v2].alive {
            self.absorb(v2, t);
        }
    }

    /// Reflex vertices `v` and `w` of one loop meet at `p`; the loop splits
    /// into `prev(v) → x → next(w)` and `prev(w) → y → next(v)`.
    fn apply_vertex_event(&mut self, v: usize, w: usize, p: Point2, t: f64) {
        let node = self.add_node(p, t);
        let (pv, nv, lv, rv) = (
            self.verts[v].prev,
            self.verts[v].next,
            self.verts[v].left,
            self.verts[v].right,
        );
        let (pw, nw, lw, rw) = (
            self.verts[w].prev,
            self.verts[w].next,
            self.verts[w].left,
            self.verts[w].right,
        );
        self.retire(v, node);
        self.retire(w, node);
        let x = self.new_vertex(p, t, node, lv, rw);
        let y = self.new_vertex(p, t, node, lw, rv);
        self.verts[pv].next = x;
        self.verts[x].prev = pv;
        self.verts[x].next = nw;
        self.verts[nw].prev = x;
        self.verts[pw].next = y;
        self.verts[y].prev = pw;
        self.verts[y].next = nv;
        self.verts[nv].prev = y;
    }
}

/// Whether an event at `t` with priority `rank` beats the current best.
/// Simultaneous events run bends first, then splits, then edges.
fn better(best: &Option<(f64, Event)>, t: f64, rank: u8) -> bool {
    match best {
        None => true,
        Some((bt, ev)) => t < bt - EPS || ((t - bt).abs() <= EPS && rank < ev.rank()),
    }
}

#[derive(Debug)]
enum Event {
    Edge(usize),
    Split(usize, usize, Point2),
    Bend(usize),
    Overtake(usize, usize),
}

impl Event {
    fn rank(&self) -> u8 {
        match self {
            Event::Bend(_) | Event::Overtake(..) => 0,
            Event::Split(..) => 1,
            Event::Edge(_) => 2,
        }
    }
}

/// Walls with an overhang standing at an inside corner next to a sloped wall
/// whose eave is lower. A gable stands as high as the higher eave beside it
/// (sample TRAING04; not between coinciding planes as in TRAING08).
/// Zero-length marker walls count by their direction.
fn raised_edges(part: &RoofPart) -> Vec<bool> {
    let n = part.edges.len();
    let pts = part.outline();
    let first = |e: &EdgeDef| e.planes().first().map_or(f64::INFINITY, |p| p.0);
    let height = |i: usize| {
        let e = &part.edges[i];
        if !e.is_gable() {
            return first(e);
        }
        let beside = [(i + n - 1) % n, (i + 1) % n]
            .iter()
            .map(|&j| &part.edges[j])
            .filter(|ej| !ej.is_gable())
            .map(first)
            .fold(f64::NEG_INFINITY, f64::max);
        if beside.is_finite() {
            beside
        } else {
            f64::INFINITY
        }
    };
    let dir = |i: usize| {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let l = a.distance(b);
        if l > POS_TOL {
            Point2::new((b.x - a.x) / l, (b.y - a.y) / l)
        } else {
            let ang = part.edges[i].direction_rad.unwrap_or(0.0);
            Point2::new(ang.cos(), ang.sin())
        }
    };
    // Inside corner: the outline turns right (it is CCW).
    let reflex = |a: Point2, b: Point2| a.x * b.y - a.y * b.x < -1e-9;
    (0..n)
        .map(|i| {
            let e = &part.edges[i];
            if e.overhang <= EPS {
                return false;
            }
            let t = height(i);
            // Millimetres are DACAD rounding, not a raised wall (sample HPS01).
            let lower = |j: usize| !part.edges[j].is_gable() && height(j) < t - MERGE_TOL;
            let (prev, next) = ((i + n - 1) % n, (i + 1) % n);
            (lower(prev) && reflex(dir(prev), dir(i))) || (lower(next) && reflex(dir(i), dir(next)))
        })
        .collect()
}

/// Makes marker walls (with a direction) that DACAD's centimetre snapping
/// left a few millimetres long zero-length again: the next wall starts
/// where the marker does (sample HPS01).
fn snap_markers(part: &RoofPart) -> RoofPart {
    let mut out = part.clone();
    let n = out.edges.len();
    for i in 0..n {
        let j = (i + 1) % n;
        let (a, b) = (out.edges[i].start, out.edges[j].start);
        let d = a.distance(b);
        if out.edges[i].direction_rad.is_some() && d > 0.0 && d < 0.005 {
            out.edges[j].start = a;
        }
    }
    out
}

/// Planes in step closer than this are one plane to DACAD.
const SAME_PLANE: f64 = 1e-2;

/// Parallel walls of one pitch whose roof planes are in step less than a
/// centimetre apart roof one plane to DACAD: the later wall's eave moves
/// onto the earlier wall's plane (sample 91021421, a raised wall 5 mm
/// under the plane of a wall further along; TRAING09). Only for a part
/// that stays one building: across a standing gable DACAD keeps the
/// planes apart (KRUPA006).
pub(crate) fn snap_in_step_planes(part: &RoofPart) -> RoofPart {
    let mut out = part.clone();
    let n = out.edges.len();
    let pts = part.outline();
    let line = |i: usize| {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let l = a.distance(b);
        (l > POS_TOL).then(|| {
            let d = Point2::new((b.x - a.x) / l, (b.y - a.y) / l);
            // The wall line's offset along its outward normal.
            (d, d.y * a.x - d.x * a.y)
        })
    };
    let sloped = |e: &EdgeDef| !e.is_gable() && e.pitch_deg > 0.0 && e.breaks.is_empty();
    for j in 0..n {
        let Some((dj, cj)) = line(j) else { continue };
        if !sloped(&out.edges[j]) {
            continue;
        }
        for i in 0..j {
            let (e, f) = (&out.edges[i], &out.edges[j]);
            let Some((di, ci)) = line(i) else { continue };
            if !sloped(e)
                || dot(di, dj) < 1.0 - 1e-9
                || (e.pitch_deg - f.pitch_deg).abs() > 1e-9
                || (ci - cj).abs() < POS_TOL
            {
                continue;
            }
            let tan = e.pitch_deg.to_radians().tan();
            let want = e.eave_height + tan * (e.overhang + ci - cj);
            let have = f.eave_height + tan * f.overhang;
            if (want - have).abs() > EPS && (want - have).abs() < SAME_PLANE {
                out.edges[j].eave_height += want - have;
                break;
            }
        }
    }
    out
}

/// Gable walls on one line, even with other walls between them, are one
/// gable to DACAD: a clipped gable (Krüppel) on one clips them all (sample
/// TRAING01). Only when the building runs on between them: the gable ends
/// of two wings across a notch keep their own (mass case R1_0050).
fn share_half_hips(part: &RoofPart) -> RoofPart {
    let mut out = part.clone();
    let n = out.edges.len();
    let pts = part.outline();
    let line = |i: usize| {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let l = a.distance(b);
        (l > POS_TOL).then(|| {
            let d = Point2::new((b.x - a.x) / l, (b.y - a.y) / l);
            (d, d.y * a.x - d.x * a.y)
        })
    };
    // Whether the footprint continues just inside the line between the two
    // collinear walls.
    let joined = |i: usize, j: usize| {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let l = a.distance(b);
        let d = Point2::new((b.x - a.x) / l, (b.y - a.y) / l);
        let t = |p: Point2| (p.x - a.x) * d.x + (p.y - a.y) * d.y;
        let (tj0, tj1) = (t(pts[j]), t(pts[(j + 1) % n]));
        let (lo, hi) = if tj0.min(tj1) >= l {
            (l, tj0.min(tj1))
        } else {
            (tj0.max(tj1), 0.0)
        };
        if hi - lo < POS_TOL {
            return true;
        }
        // The roof rises to the left of a wall (counter-clockwise outline).
        (1..10).all(|k| {
            let s = lo + (hi - lo) * k as f64 / 10.0;
            let p = Point2::new(a.x + d.x * s - d.y * 1e-3, a.y + d.y * s + d.x * 1e-3);
            crate::openings::inside(&pts, p)
        })
    };
    for i in 0..n {
        let e = &part.edges[i];
        let (Some(h), Some((d, c))) = (e.half_hip.clone(), line(i)) else {
            continue;
        };
        if !e.is_gable() {
            continue;
        }
        for j in (0..n).filter(|&j| j != i) {
            let f = &part.edges[j];
            if !f.is_gable()
                || f.half_hip.is_some()
                || (f.eave_height - e.eave_height).abs() > 1e-6
                || (f.overhang - e.overhang).abs() > 1e-6
            {
                continue;
            }
            if let Some((dj, cj)) = line(j)
                && dot(d, dj) > 1.0 - 1e-9
                && (c - cj).abs() < 1e-6
                && joined(i, j)
            {
                out.edges[j].half_hip = Some(h.clone());
            }
        }
    }
    out
}

/// Generates the roof over one footprint part. The part must be CCW
/// (see [`RoofPart::normalize_ccw`]).
///
/// A wall whose eave is higher than the lowest stands at its wall line until
/// its plane leaves the wall top; its overhang floats above the neighbouring
/// roof, which runs on to the wall (DACAD sample 90062001). Such parts are
/// generated twice, with every wall at its overhang line and with the raised
/// walls at their wall lines, and the two are combined
/// ([`crate::raised::combine`]).
pub fn generate(part: &RoofPart) -> RoofGeometry {
    let (part, map) = merge_collinear_walls(&share_half_hips(&snap_markers(part)));
    let raised = raised_edges(&part);
    let (off, _) = generate_mode(&part, &vec![false; raised.len()]);
    let mut geo = if raised.contains(&true) {
        let (body, _) = generate_mode(&part, &raised);
        let eaves: Vec<f64> = part.edges.iter().map(|e| e.eave_height).collect();
        crate::raised::combine(off, body, &eaves, &raised)
    } else {
        off
    };
    for f in &mut geo.faces {
        if let Some(&i) = map.get(f.edge) {
            f.edge = i;
        }
    }
    drop_eaves_on_ridges(&mut geo);
    one_sided_ridges(&mut geo);
    drop_short_ridges(&mut geo);
    geo
}

/// DACAD has no ridge shorter than about 3.5 cm (kept from 40 mm, dropped
/// up to 30 mm; sweep in /mnt/project-files/dacad-short-ridge/, mass cases
/// R1_0135, R1_0179). The face of the lower-numbered wall below it becomes
/// a triangle: its two hips run on to where they meet. The other face's
/// hips keep their ends. [no source — extrapolation] for which face that is.
fn drop_short_ridges(geo: &mut RoofGeometry) {
    const MIN_RIDGE: f64 = 0.035;
    let near = |p: [f64; 3], q: [f64; 3]| dist3(p, q) < 1e-6;
    let mut k = 0;
    while k < geo.lines.len() {
        let r = geo.lines[k].clone();
        if r.kind != LineKind::Ridge || r.length() >= MIN_RIDGE {
            k += 1;
            continue;
        }
        // Faces with the ridge as an edge, lowest wall first.
        let mut faces: Vec<usize> = (0..geo.faces.len())
            .filter(|&f| {
                let poly = &geo.faces[f].polygon;
                let n = poly.len();
                (0..n).any(|i| {
                    let (p, q) = (poly[i], poly[(i + 1) % n]);
                    (near(p, r.a) && near(q, r.b)) || (near(p, r.b) && near(q, r.a))
                })
            })
            .collect();
        faces.sort_by_key(|&f| geo.faces[f].edge);
        let Some(&f) = faces.first() else {
            k += 1;
            continue;
        };
        // The face's hips at the two ridge ends.
        let poly = geo.faces[f].polygon.clone();
        let hip_at = |end: [f64; 3]| {
            geo.lines.iter().position(|l| {
                l.kind == LineKind::Hip
                    && (near(l.a, end) || near(l.b, end))
                    && poly
                        .iter()
                        .any(|&p| near(p, if near(l.a, end) { l.b } else { l.a }))
            })
        };
        let (Some(ha), Some(hb)) = (hip_at(r.a), hip_at(r.b)) else {
            k += 1;
            continue;
        };
        let far = |l: &RoofLine, end: [f64; 3]| if near(l.a, end) { l.b } else { l.a };
        let (fa, fb) = (far(&geo.lines[ha], r.a), far(&geo.lines[hb], r.b));
        let Some(top) = meet3(fa, r.a, fb, r.b) else {
            k += 1;
            continue;
        };
        for (h, end) in [(ha, r.a), (hb, r.b)] {
            let l = &mut geo.lines[h];
            if near(l.a, end) {
                l.a = top;
            } else {
                l.b = top;
            }
        }
        let face = &mut geo.faces[f];
        let mut out: Vec<[f64; 3]> = Vec::new();
        for &p in &face.polygon {
            let q = if near(p, r.a) || near(p, r.b) { top } else { p };
            if out.last().is_none_or(|&l| !near(l, q)) {
                out.push(q);
            }
        }
        if out.len() > 1 && near(out[0], out[out.len() - 1]) {
            out.pop();
        }
        face.polygon = out;
        geo.lines.remove(k);
    }
}

/// Where the lines p→p2 and q→q2 (in one plane) meet, from plan.
fn meet3(p: [f64; 3], p2: [f64; 3], q: [f64; 3], q2: [f64; 3]) -> Option<[f64; 3]> {
    let (d, e) = ([p2[0] - p[0], p2[1] - p[1]], [q2[0] - q[0], q2[1] - q[1]]);
    let den = d[0] * e[1] - d[1] * e[0];
    if den.abs() < 1e-12 {
        return None;
    }
    let t = ((q[0] - p[0]) * e[1] - (q[1] - p[1]) * e[0]) / den;
    Some(std::array::from_fn(|i| p[i] + (p2[i] - p[i]) * t))
}

/// A plane that covers a gable at the moment it meets the opposite plane
/// makes a ridge there, not an eave (DACAD case C43).
fn drop_eaves_on_ridges(geo: &mut RoofGeometry) {
    let on = |p: [f64; 3], l: &RoofLine| {
        let d: [f64; 3] = std::array::from_fn(|i| l.b[i] - l.a[i]);
        let len2: f64 = d.iter().map(|x| x * x).sum();
        let k = (0..3).map(|i| (p[i] - l.a[i]) * d[i]).sum::<f64>() / len2;
        let foot: [f64; 3] = std::array::from_fn(|i| l.a[i] + d[i] * k);
        len2 > 0.0 && (-1e-6..=1.0 + 1e-6).contains(&k) && dist3(p, foot) < POS_TOL
    };
    let ridges: Vec<RoofLine> = geo
        .lines
        .iter()
        .filter(|l| l.kind == LineKind::Ridge)
        .cloned()
        .collect();
    geo.lines
        .retain(|l| l.kind != LineKind::Eave || !ridges.iter().any(|r| on(l.a, r) && on(l.b, r)));
}

/// A stretch of ridge with a roof face on one side only, e.g. over an
/// overhang strip that only one plane has, is a Pult line (DACAD case C43).
fn one_sided_ridges(geo: &mut RoofGeometry) {
    use crate::union::{covers, keep_parts, splits, unit};
    let polys: Vec<Vec<Point2>> = geo.faces.iter().map(crate::openings::plan).collect();
    let refs: Vec<&[Point2]> = polys.iter().map(Vec::as_slice).collect();
    let mut out = Vec::with_capacity(geo.lines.len());
    for l in std::mem::take(&mut geo.lines) {
        if l.kind != LineKind::Ridge {
            out.push(l);
            continue;
        }
        let d = unit(Point2::new(l.b[0] - l.a[0], l.b[1] - l.a[1]));
        let nrm = Point2::new(-d.y, d.x);
        let both = |m: Point2| {
            let side = |k: f64| {
                let p = Point2::new(m.x + nrm.x * k, m.y + nrm.y * k);
                polys.iter().any(|f| covers(f, p))
            };
            side(1e-4) && side(-1e-4)
        };
        let ts = splits(l.a, l.b, &refs, &[]);
        let two = keep_parts(l.a, l.b, &ts, |m, _| both(m));
        let one = keep_parts(l.a, l.b, &ts, |m, _| !both(m));
        if one.is_empty() {
            out.push(l);
            continue;
        }
        for (a, b) in two {
            out.push(RoofLine { a, b, ..l.clone() });
        }
        for (a, b) in one {
            out.push(RoofLine {
                kind: LineKind::Pult,
                a,
                b,
                side: None,
            });
        }
    }
    geo.lines = out;
}

/// Inward distance of a wall's roof line from its outline at height `t`.
fn reach(planes: &[(f64, f64)], t: f64) -> f64 {
    let mut d = 0.0;
    for (k, &(t0, p)) in planes.iter().enumerate() {
        if t <= t0 {
            break;
        }
        let t1 = planes.get(k + 1).map_or(f64::INFINITY, |q| q.0);
        d += (t.min(t1) - t0) / p.to_radians().tan();
    }
    d
}

/// Consecutive walls on one straight line with the same overhang act as one
/// wall whose roof is the lowest of their planes at every height, e.g. a
/// Krüppel plane over only part of a gable spans the whole gable (DACAD case
/// C44). Returns the merged part and, per merged wall, its first wall's
/// index.
fn merge_collinear_walls(part: &RoofPart) -> (RoofPart, Vec<usize>) {
    let n = part.edges.len();
    let pts = part.outline();
    let dir = |i: usize| {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let l = a.distance(b);
        (l > POS_TOL).then(|| Point2::new((b.x - a.x) / l, (b.y - a.y) / l))
    };
    let joins = |i: usize| {
        // Edge i continues edge i-1.
        let j = (i + n - 1) % n;
        match (dir(j), dir(i)) {
            (Some(a), Some(b)) => {
                dot(a, b) > 1.0 - 1e-9
                    && (part.edges[i].overhang - part.edges[j].overhang).abs() < 1e-9
            }
            _ => false,
        }
    };
    if n < 4 || !(0..n).any(joins) {
        return (part.clone(), (0..n).collect());
    }
    // Start at an edge that does not continue its predecessor.
    let Some(first) = (0..n).find(|&i| !joins(i)) else {
        return (part.clone(), (0..n).collect());
    };
    let mut edges: Vec<EdgeDef> = Vec::new();
    let mut map = Vec::new();
    let mut k = 0;
    while k < n {
        let i = (first + k) % n;
        let mut run = vec![i];
        while k + run.len() < n && joins((first + k + run.len()) % n) {
            run.push((first + k + run.len()) % n);
        }
        k += run.len();
        map.push(i);
        if run.len() == 1 {
            edges.push(part.edges[i].clone());
            continue;
        }
        edges.push(envelope(run.iter().map(|&r| &part.edges[r]).collect()));
    }
    (
        RoofPart {
            name: part.name.clone(),
            edges,
        },
        map,
    )
}

/// One wall standing for collinear walls `defs`: at every height its roof
/// line is the furthest in of theirs.
fn envelope(defs: Vec<&EdgeDef>) -> EdgeDef {
    let planes: Vec<Vec<(f64, f64)>> = defs.iter().map(|e| e.planes()).collect();
    let mut ts: Vec<f64> = planes.iter().flatten().map(|p| p.0).collect();
    ts.sort_by(f64::total_cmp);
    ts.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
    let far = |t: f64| planes.iter().map(|p| reach(p, t)).fold(0.0f64, f64::max);
    // Refine by bisecting crossings between breakpoints.
    let mut cuts: Vec<f64> = ts.clone();
    for w in ts.windows(2) {
        let (lo, hi) = (w[0], w[1]);
        // Within (lo, hi) each line is straight; the max changes at most at
        // pairwise crossings.
        for a in &planes {
            for b in &planes {
                let (da0, da1) = (reach(a, lo), reach(a, hi));
                let (db0, db1) = (reach(b, lo), reach(b, hi));
                let (f0, f1) = (da0 - db0, da1 - db1);
                if f0 * f1 < 0.0 {
                    cuts.push(lo + (hi - lo) * f0 / (f0 - f1));
                }
            }
        }
    }
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
    // Slope of the envelope on each interval, as planes.
    let mut out: Vec<(f64, f64)> = Vec::new();
    for (k, &t0) in cuts.iter().enumerate() {
        let t1 = cuts.get(k + 1).copied().unwrap_or(t0 + 1.0);
        let rate = (far(t1) - far(t0)) / (t1 - t0);
        if rate <= 1e-12 {
            continue;
        }
        let pitch = (1.0 / rate).atan().to_degrees();
        if out.last().is_some_and(|l| (l.1 - pitch).abs() < 1e-9) {
            continue;
        }
        out.push((t0, pitch));
    }
    let mut e = defs[0].clone();
    e.direction_rad = None;
    e.priority = defs.iter().map(|d| d.priority).max().unwrap_or(0);
    e.clad = defs.iter().any(|d| d.clad);
    e.breaks.clear();
    let all_gables = defs.iter().all(|d| d.is_gable());
    let mut rest = out.iter();
    if all_gables {
        e.half_hip = rest
            .next()
            .map(|&(height, pitch_deg)| crate::model::HalfHip { height, pitch_deg });
    } else if let Some(&(t, p)) = rest.next() {
        e.eave_height = t;
        e.pitch_deg = p;
        e.half_hip = None;
    }
    e.breaks = rest
        .map(|&(height, pitch_deg)| crate::model::PitchBreak { height, pitch_deg })
        .collect();
    e
}

fn generate_mode(part: &RoofPart, raised: &[bool]) -> (RoofGeometry, Vec<Point2>) {
    let edges = &part.edges;
    let n = edges.len();
    if n < 3 {
        return (RoofGeometry::default(), Vec::new());
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
            let mut line = Line::new(dir, nrm, dot(nrm, a) - e.overhang, &e.planes(), e.priority);
            line.marker = len < POS_TOL;
            if raised[i] {
                // Same plane, but a wall at the wall line until the plane
                // reaches the wall top.
                line.base += e.overhang;
                if let Some(st) = line.stages.first_mut() {
                    st.t += e.overhang / st.s;
                }
                line.raised = true;
            }
            line
        })
        .collect();
    let start = lines.iter().map(Line::t_act).fold(f64::INFINITY, f64::min);
    if !start.is_finite() {
        return (RoofGeometry::default(), Vec::new()); // only gables: no roof planes
    }

    let applied = lines
        .iter()
        .map(|l| l.stages.iter().filter(|s| s.t <= start + EPS).count())
        .collect();
    let mut sim = Sim {
        lines,
        verts: Vec::new(),
        nodes: Vec::new(),
        arcs: Vec::new(),
        bottoms: Vec::new(),
        applied,
        ridges: Vec::new(),
    };
    // Initial vertex i sits between edge i-1 (left) and edge i (right),
    // on the roof outline.
    for i in 0..n {
        let (l, r) = ((i + n - 1) % n, i);
        let (ll, rl) = (&sim.lines[l], &sim.lines[r]);
        let p = solve2(ll.n, ll.base, rl.n, rl.base).unwrap_or(pts[i]);
        let node = sim.add_node(p, start);
        sim.new_vertex(p, start, node, l, r);
    }
    for i in 0..n {
        sim.verts[i].prev = (i + n - 1) % n;
        sim.verts[i].next = (i + 1) % n;
        if sim.lines[i].active(start) {
            let (a, b) = (sim.verts[i].node, sim.verts[(i + 1) % n].node);
            sim.bottoms.push(Bottom {
                a,
                b,
                edge: i,
                stage: 0,
            });
        }
    }
    let initial: Vec<usize> = (0..n).map(|i| sim.verts[i].node).collect();
    let outline = initial
        .iter()
        .map(|&k| Point2::new(sim.nodes[k][0], sim.nodes[k][1]))
        .collect();
    sim.run(start);
    (build_geometry(part, &sim, &initial), outline)
}

/// Walks from `end` back to `start` through `segs`, returning the ring.
fn trace_ring(start: usize, end: usize, mut segs: Vec<(usize, usize)>) -> Vec<usize> {
    if start == end {
        // A plane whose eave shrank to a point (sample HINT04): walk round
        // from that point.
        let Some(k) = segs.iter().position(|&(x, y)| x == start || y == start) else {
            return vec![start];
        };
        let (x, y) = segs.swap_remove(k);
        let next = if x == start { y } else { x };
        return trace_ring(start, next, segs);
    }
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
    ring
}

fn build_geometry(part: &RoofPart, sim: &Sim, initial: &[usize]) -> RoofGeometry {
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
    let bottoms: Vec<Bottom> = sim
        .bottoms
        .iter()
        .map(|b| Bottom {
            a: canon[b.a],
            b: canon[b.b],
            ..*b
        })
        .collect();

    let mut geo = RoofGeometry::default();
    // A plane leaning outward (pitch over 90°) carries no roofing: it has no
    // face and no eave, and roof planes end against it in verges (C71).
    let bare = |e: usize, z: f64| {
        let l = &sim.lines[e];
        l.stage_at(z).is_some_and(|k| l.stages[k].s < -1e-12)
    };
    // A sloped plane that has not started yet at height `z`.
    let later = |e: usize, z: f64| {
        let l = &sim.lines[e];
        !l.active(z) && l.stages.first().is_some_and(|s| s.s.abs() > 1e-12)
    };
    for b in bottoms
        .iter()
        .filter(|b| sim.lines[b.edge].stages[b.stage].s >= -1e-12)
    {
        // A plane starting where the edge was a wall begins at an eave
        // (also the Krüppel plane of a clipped gable, DACAD case C08);
        // a later plane of the same wall begins at a pitch break.
        let kind = if b.stage == 0 {
            LineKind::Eave
        } else {
            LineKind::Knick
        };
        geo.lines
            .push(RoofLine::new(kind, sim.nodes[b.a], sim.nodes[b.b]));
    }
    // Each arc borders two faces; emit it once with its kind.
    let mut seen: Vec<(usize, usize)> = Vec::new();
    for &(a, b, kind) in &sim.ridges {
        let (a, b) = (canon[a], canon[b]);
        if a != b {
            seen.push((a.min(b), a.max(b)));
            geo.lines
                .push(RoofLine::new(kind, sim.nodes[a], sim.nodes[b]));
        }
    }
    // Edges on one line with the same plane at the arc's height.
    let coplanar = |a: usize, b: usize, z: f64| {
        let (la, lb) = (&sim.lines[a], &sim.lines[b]);
        let ((ca, sa), (cb, sb)) = (la.params(z), lb.params(z));
        dot(la.n, lb.n) > 1.0 - 1e-9 && (ca - cb).abs() < MERGE_TOL && (sa - sb).abs() < 1e-9
    };
    for arc in arcs
        .iter()
        .filter(|a| a.line && !coplanar(a.left, a.right, sim.nodes[a.a][2]))
    {
        let key = (arc.a.min(arc.b), arc.a.max(arc.b));
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        let (pa, pb) = (sim.nodes[arc.a], sim.nodes[arc.b]);
        // Level, allowing for the millimetres DACAD's rounded inputs leave
        // where several planes nearly meet in one point (sample HOFSTTN).
        let run = (pa[0] - pb[0]).hypot(pa[1] - pb[1]);
        let flat = (pa[2] - pb[2]).abs() < 1e-6f64.max(0.005 * run);
        let z = (pa[2] + pb[2]) / 2.0;
        let waiting = [
            arc.waiting[0] || bare(arc.left, z),
            arc.waiting[1] || bare(arc.right, z),
        ];
        let (kind, side) = match waiting {
            // A corner between two walls carries no roof.
            [true, true] => continue,
            // A plane sliding along a wall: verge, or a Pult line on top.
            // The plane's start (left edge waiting) is on the left when
            // facing the roof from outside its eave.
            [true, false] if flat => (LineKind::Pult, None),
            [false, true] if flat => (LineKind::Pult, None),
            // A plane against a raised wall from an inside corner abuts it
            // (DACAD sample 90062001).
            [true, false] if arc.reflex && sim.lines[arc.left].raised => {
                (LineKind::AbutmentSide, None)
            }
            [false, true] if arc.reflex && sim.lines[arc.right].raised => {
                (LineKind::AbutmentSide, None)
            }
            // From an inside corner against a plane that starts higher up
            // (a marker wall with a higher eave), the lower plane abuts the
            // step below it (DACAD case C73).
            [true, false] if arc.reflex && later(arc.left, z) => (LineKind::AbutmentSide, None),
            [false, true] if arc.reflex && later(arc.right, z) => (LineKind::AbutmentSide, None),
            // From an inside corner between a wall and a plane: DACAD
            // reports a valley (C18–C23).
            [true, false] | [false, true] if arc.reflex => (LineKind::Valley, None),
            [true, false] => (
                LineKind::Verge,
                Some(verge_side(sim, arc.right, pa, pb, VergeSide::Left)),
            ),
            [false, true] => (
                LineKind::Verge,
                Some(verge_side(sim, arc.left, pa, pb, VergeSide::Right)),
            ),
            [false, false] if flat => (LineKind::Ridge, None),
            [false, false] if arc.reflex => (LineKind::Valley, None),
            [false, false] => (LineKind::Hip, None),
        };
        geo.lines.push(RoofLine {
            kind,
            a: pa,
            b: pb,
            side,
        });
    }

    // One face per plane: the arcs of its edge between its start height and
    // the next plane's, closed by the next plane's bottom (a pitch break).
    for b in bottoms
        .iter()
        .filter(|b| sim.lines[b.edge].stages[b.stage].s > 1e-12)
    {
        let line = &sim.lines[b.edge];
        let lo = line.stages[b.stage].t;
        let hi = line.stages.get(b.stage + 1).map_or(f64::INFINITY, |s| s.t);
        let mut segs: Vec<(usize, usize)> = arcs
            .iter()
            .filter(|a| {
                let z = sim.nodes[a.a][2];
                (a.left == b.edge || a.right == b.edge) && z >= lo - 1e-7 && z < hi - 1e-7
            })
            .map(|a| (a.a, a.b))
            .collect();
        segs.extend(
            bottoms
                .iter()
                .filter(|u| u.edge == b.edge && u.stage == b.stage + 1)
                .map(|u| (u.a, u.b)),
        );
        let ring = trace_ring(b.a, b.b, segs);
        let polygon: Vec<[f64; 3]> = ring.iter().map(|&k| sim.nodes[k]).collect();
        let area = area3(&polygon);
        geo.faces.push(RoofFace {
            edge: b.edge,
            pitch_deg: line.stages[b.stage].pitch_deg,
            polygon,
            area,
            deducted: 0.0,
        });
    }
    let n = edges.len();
    for (i, e) in edges.iter().enumerate() {
        if !e.is_gable() {
            continue;
        }
        // The vertical wall below the gable's Krüppel plane, if any.
        let top = sim.lines[i].t_act();
        let (start, end) = (canon[initial[i]], canon[initial[(i + 1) % n]]);
        let mut segs: Vec<(usize, usize)> = arcs
            .iter()
            .filter(|a| (a.left == i || a.right == i) && sim.nodes[a.a][2] < top - 1e-7)
            .map(|a| (a.a, a.b))
            .collect();
        segs.extend(
            bottoms
                .iter()
                .filter(|u| u.edge == i && u.stage == 0)
                .map(|u| (u.a, u.b)),
        );
        let ring = trace_ring(start, end, segs);
        if ring.len() >= 3 {
            let poly: Vec<[f64; 3]> = ring.iter().map(|&k| sim.nodes[k]).collect();
            if e.clad {
                geo.clad_area += area3(&poly);
            }
            geo.gables.push(poly);
        }
    }
    geo
}

/// Side of a verge of the plane of edge `plane`: `end` (left at the plane's
/// start, right at its end) when the verge is square to the eave in plan,
/// otherwise oblique (DACAD case C17).
fn verge_side(sim: &Sim, plane: usize, a: [f64; 3], b: [f64; 3], end: VergeSide) -> VergeSide {
    let d = Point2::new(b[0] - a[0], b[1] - a[1]);
    let len = d.x.hypot(d.y);
    if len > POS_TOL && dot(d, sim.lines[plane].dir).abs() / len > 1e-6 {
        VergeSide::Oblique
    } else {
        end
    }
}

fn is_zero_f64(v: &f64) -> bool {
    *v == 0.0
}

/// Area of a planar 3D polygon (Newell's method).
pub(crate) fn area3(poly: &[[f64; 3]]) -> f64 {
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
    use crate::model::EdgeDef;
    use approx::assert_relative_eq;

    /// A 0.8 m bump wall between two parallel walls whose overhang lines
    /// coincide keeps its plane, which runs on to the main ridge (mass case
    /// R1_0229, DACAD G 12.08 m and a Pult on the west gable).
    #[test]
    fn short_wall_between_coinciding_eaves_keeps_its_plane() {
        let mut p = part(
            &[
                (0.0, 0.0),
                (2.55, 0.0),
                (2.55, -0.8),
                (4.35, -0.8),
                (4.35, 0.0),
                (9.25, 0.0),
                (9.25, 10.05),
                (0.0, 10.05),
            ],
            &[51.0, 51.0, 57.0, 51.0, 26.0, 90.0, 51.0, 90.0],
        );
        p.edges[4].overhang = 0.8;
        let geo = generate(&p);
        let hips: f64 = geo
            .lines
            .iter()
            .filter(|l| l.kind == LineKind::Hip)
            .map(|l| l.length())
            .sum();
        assert!((hips - 12.08).abs() < 0.01, "{hips}");
        assert!(geo.lines.iter().any(|l| l.kind == LineKind::Pult));
        assert!(!geo.lines.iter().any(|l| l.kind == LineKind::Knick));
    }

    /// Two wings of a U, both ending in a gable on y = 13.3 (mass case
    /// R1_0050): DACAD clips only the gable that has the Krüppel.
    #[test]
    fn krueppel_stays_on_its_own_wing() {
        let mut p = part(
            &[
                (0.0, 0.0),
                (12.95, 0.0),
                (12.95, 13.3),
                (7.65, 13.3),
                (7.65, 6.65),
                (5.3, 6.65),
                (5.3, 13.3),
                (0.0, 13.3),
            ],
            &[55.0, 55.0, 90.0, 35.0, 57.0, 55.0, 90.0, 28.0],
        );
        p.edges[6].half_hip = Some(crate::model::HalfHip {
            height: 1.7,
            pitch_deg: 62.0,
        });
        let shared = share_half_hips(&p);
        assert!(shared.edges[2].half_hip.is_none());
        assert!(shared.edges[6].half_hip.is_some());
    }

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
                breaks: Vec::new(),
                priority: 0,
                clad: false,
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
    fn t_shape_gables_have_valleys_but_no_hips() {
        let t = [
            (0.0, 0.0),
            (16.0, 0.0),
            (16.0, 7.0),
            (11.0, 7.0),
            (11.0, 14.0),
            (5.0, 14.0),
            (5.0, 7.0),
            (0.0, 7.0),
        ];
        let p = [40.0, 90.0, 40.0, 40.0, 90.0, 40.0, 40.0, 90.0];
        let g = generate(&part(&t, &p));
        let q = g.quantities();
        assert_eq!(q.valley_count, 2);
        assert_eq!(q.hip_count, 0);
        assert_eq!(g.gables.len(), 3);
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
