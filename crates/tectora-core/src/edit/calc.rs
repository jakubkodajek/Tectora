//! DACAD's calculation helpers (Parameter › Neigungen / Traufenhöhen /
//! Vorsprünge / Lage berechnen, Um Aufbaudicke korrigieren; DACAD.HLP):
//! they solve a wall's plane for its pitch, eave height, overhang or
//! position from given measures. A wall's plane rises from its eave line,
//! which lies at the overhang line: z = eave + tan(pitch) · (inward
//! distance from the wall + overhang). Port of the editor's `roofcalc.ts`;
//! the rules were measured in DACAD (cases named in the tests).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{EdgePatch, Patches, combine, is_marker, round_to};
use crate::geom::Point2;
use crate::model::EdgeDef;

/// What a helper solves for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    Pitch,
    Eave,
    Overhang,
    /// Moves the wall(s) instead (DACAD: Lage berechnen).
    Position,
}

fn tan(deg: f64) -> f64 {
    deg.to_radians().tan()
}
fn atan_deg(v: f64) -> f64 {
    v.atan().to_degrees()
}
fn r4(v: f64) -> f64 {
    round_to(v, 4)
}

/// Wall `i`'s line: start, unit direction and inward normal (the roof rises
/// to the left).
pub fn wall_frame(edges: &[EdgeDef], i: usize) -> (Point2, Point2, Point2) {
    let a = edges[i].start;
    let b = edges[(i + 1) % edges.len()].start;
    let mut l = a.distance(b);
    if l == 0.0 {
        l = 1.0;
    }
    let u = Point2::new((b.x - a.x) / l, (b.y - a.y) / l);
    (a, u, Point2::new(-u.y, u.x))
}

/// Inward distance of `p` from wall `i`'s line.
pub fn inward(edges: &[EdgeDef], i: usize, p: Point2) -> f64 {
    let (a, _, n) = wall_frame(edges, i);
    (p.x - a.x) * n.x + (p.y - a.y) * n.y
}

/// Result of [`triangle`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Triangle {
    pub pitch_deg: f64,
    pub width: f64,
    pub rafter: f64,
    pub ridge: f64,
}

/// Einfaches Dreieck (measured in DACAD 2026-10-29): two of plan width
/// (wall to top; the overhang added unless `with_overhang`), rafter length
/// (from the eave, overhang included) and ridge height (absolute) give the
/// pitch. With pitch breaks DACAD takes the last break as a plane of its own
/// starting at the break height at the wall's overhang line (case Te).
pub fn triangle(
    e: &EdgeDef,
    width: Option<f64>,
    rafter: Option<f64>,
    ridge: Option<f64>,
    with_overhang: bool,
) -> Option<Triangle> {
    let base = e
        .breaks
        .iter()
        .map(|b| b.height)
        .fold(None, |m: Option<f64>, h| Some(m.map_or(h, |m| m.max(h))))
        .unwrap_or(e.eave_height);
    let ov = if with_overhang { 0.0 } else { e.overhang };
    let mut run = width.map(|w| w + ov);
    let mut rafter = rafter;
    let mut rise = ridge.map(|r| r - base);
    match (run, rafter, rise) {
        (Some(w), _, Some(h)) => rafter = Some(w.hypot(h)),
        (Some(w), Some(r), None) => {
            if r <= w {
                return None;
            }
            rise = Some((r * r - w * w).sqrt());
        }
        (None, Some(r), Some(h)) => {
            if r <= h {
                return None;
            }
            run = Some((r * r - h * h).sqrt());
        }
        _ => return None,
    }
    let (run, rise, rafter) = (run?, rise?, rafter?);
    if !(run > 0.0) || !(rise > 0.0) {
        return None;
    }
    Some(Triangle {
        pitch_deg: r4(atan_deg(rise / run)),
        width: run - ov,
        rafter,
        ridge: rise + base,
    })
}

/// The patch of a triangle result: with breaks, the top break's pitch.
pub fn triangle_patch(e: &EdgeDef, pitch_deg: f64) -> EdgePatch {
    if e.breaks.is_empty() {
        return EdgePatch {
            pitch_deg: Some(pitch_deg),
            ..EdgePatch::default()
        };
    }
    let top = (0..e.breaks.len()).fold(0, |k, i| {
        if e.breaks[i].height > e.breaks[k].height {
            i
        } else {
            k
        }
    });
    let mut breaks = e.breaks.clone();
    breaks[top].pitch_deg = pitch_deg;
    EdgePatch {
        breaks: Some(breaks),
        ..EdgePatch::default()
    }
}

/// One wall's values in the 2 Parallele dialog.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Side {
    pub pitch_deg: f64,
    pub eave_height: f64,
    pub overhang: f64,
}

impl Side {
    fn of(e: &EdgeDef) -> Self {
        Self {
            pitch_deg: e.pitch_deg,
            eave_height: e.eave_height,
            overhang: e.overhang,
        }
    }
}

/// 2 Parallele: DACAD's one dialog for pitch, eave height, overhang and
/// wall position (measured 2026-10-29). Two facing walls `span` apart; wall
/// A's roof meets wall B's at a ridge, or at two ridges `top` apart (a flat
/// strip). `w_a`, `w_b` are the plan runs from each wall line to its ridge
/// (overhang not included), `h` the ridge height.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParallelState {
    pub span: f64,
    pub top: f64,
    pub w_a: f64,
    pub w_b: f64,
    pub h: f64,
    pub a: Side,
    pub b: Side,
    /// Inward shifts of the walls (target position).
    pub shift_a: f64,
    pub shift_b: f64,
    /// Rafter lengths typed in, for the circle intersection.
    pub r_a: Option<f64>,
    pub r_b: Option<f64>,
}

/// A value typed into the 2 Parallele dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParallelField {
    WidthA,
    WidthB,
    RafterA,
    RafterB,
    Ridge,
    Top,
}

pub fn parallel_start(a: &EdgeDef, b: &EdgeDef, span: f64, w: f64, h: f64) -> ParallelState {
    ParallelState {
        span,
        top: 0.0,
        w_a: w,
        w_b: span - w,
        h,
        a: Side::of(a),
        b: Side::of(b),
        shift_a: 0.0,
        shift_b: 0.0,
        r_a: None,
        r_b: None,
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Ab {
    A,
    B,
}

impl ParallelState {
    fn side(&self, s: Ab) -> &Side {
        if s == Ab::A { &self.a } else { &self.b }
    }
    fn side_mut(&mut self, s: Ab) -> &mut Side {
        if s == Ab::A { &mut self.a } else { &mut self.b }
    }
    fn w(&self, s: Ab) -> f64 {
        if s == Ab::A { self.w_a } else { self.w_b }
    }
    fn run(&self, s: Ab) -> f64 {
        self.w(s) + self.side(s).overhang
    }
    fn set_run(&mut self, s: Ab, w: f64, free: f64) {
        if s == Ab::A {
            self.w_a = w;
            self.w_b = free - w;
        } else {
            self.w_b = w;
            self.w_a = free - w;
        }
    }
    fn pitches(&mut self) -> bool {
        for s in [Ab::A, Ab::B] {
            let rise = self.h - self.side(s).eave_height;
            let run = self.run(s);
            if !(rise > 0.0) || !(run > 0.0) {
                return false;
            }
            self.side_mut(s).pitch_deg = atan_deg(rise / run);
        }
        true
    }
    fn eaves(&mut self, ta: f64, tb: f64) {
        self.a.eave_height = self.h - ta * self.run(Ab::A);
        self.b.eave_height = self.h - tb * self.run(Ab::B);
    }
    fn overhangs(&mut self, ta: f64, tb: f64) -> bool {
        if !(ta > 0.0) || !(tb > 0.0) {
            return false;
        }
        self.a.overhang = (self.h - self.a.eave_height) / ta - self.w_a;
        self.b.overhang = (self.h - self.b.eave_height) / tb - self.w_b;
        self.a.overhang >= -1e-9 && self.b.overhang >= -1e-9
    }
    /// Both walls move; the ridge stays in place in plan.
    fn move_walls(&mut self, ta: f64, tb: f64) -> bool {
        if !(ta > 0.0) || !(tb > 0.0) {
            return false;
        }
        let wa = (self.h - self.a.eave_height) / ta - self.a.overhang;
        let wb = (self.h - self.b.eave_height) / tb - self.b.overhang;
        self.shift_a += self.w_a - wa;
        self.shift_b += self.w_b - wb;
        self.span += wa - self.w_a + wb - self.w_b;
        self.w_a = wa;
        self.w_b = wb;
        wa > 0.0 && wb > 0.0
    }
}

/// Rafter length of side A or B (eave to ridge, overhang included).
pub fn parallel_rafter(st: &ParallelState, side_a: bool) -> f64 {
    let s = if side_a { Ab::A } else { Ab::B };
    st.run(s).hypot(st.h - st.side(s).eave_height)
}

/// Applies one typed value to the dialog state the way DACAD does for the
/// target. Widths are typed with or without the overhang (`with_ov`).
pub fn parallel_edit(
    st0: &ParallelState,
    field: ParallelField,
    v: f64,
    target: Target,
    with_ov: bool,
) -> Option<ParallelState> {
    let mut st = st0.clone();
    let ta = tan(st.a.pitch_deg);
    let tb = tan(st.b.pitch_deg);
    let ok = |st: ParallelState, c: bool| c.then_some(st);
    let free = st.span - st.top;
    match field {
        ParallelField::Top => {
            // The strip takes its width from both sides; slopes stay.
            if !(v >= 0.0) || v >= st.span {
                return None;
            }
            let d = (v - st.top) / 2.0;
            st.w_a -= d;
            st.w_b -= d;
            st.top = v;
            st.h = st.a.eave_height + ta * st.run(Ab::A);
            let c = st.w_a > 0.0 && st.w_b > 0.0;
            ok(st, c)
        }
        ParallelField::WidthA | ParallelField::WidthB => {
            let side = if field == ParallelField::WidthA {
                Ab::A
            } else {
                Ab::B
            };
            let w = v - if with_ov { st.side(side).overhang } else { 0.0 };
            match target {
                Target::Overhang => {
                    // DACAD keeps the wall runs and the sum of both widths
                    // with overhang (case Oa).
                    if !with_ov {
                        return None;
                    }
                    let total = st.w_a + st.a.overhang + st.w_b + st.b.overhang;
                    let other = if side == Ab::A { Ab::B } else { Ab::A };
                    st.side_mut(side).overhang = v - st.w(side);
                    let ow = st.w(other);
                    st.side_mut(other).overhang = total - v - ow;
                    let c = st.a.overhang >= -1e-9 && st.b.overhang >= -1e-9;
                    ok(st, c)
                }
                Target::Position => {
                    // A new width sets the ridge height; both walls follow (case Lb).
                    let s = *st.side(side);
                    st.h = s.eave_height + tan(s.pitch_deg) * (w + s.overhang);
                    let c = st.move_walls(ta, tb);
                    ok(st, c)
                }
                Target::Pitch | Target::Eave => {
                    if !(w > 0.0) || !(w < free) {
                        return None;
                    }
                    st.set_run(side, w, free);
                    if target == Target::Pitch {
                        let c = st.pitches();
                        return ok(st, c);
                    }
                    st.eaves(ta, tb);
                    Some(st)
                }
            }
        }
        ParallelField::Ridge => match target {
            Target::Eave => {
                st.a.eave_height += v - st.h;
                st.b.eave_height += v - st.h;
                st.h = v;
                Some(st)
            }
            Target::Pitch => {
                st.h = v;
                let c = st.pitches();
                ok(st, c)
            }
            Target::Overhang => {
                st.h = v;
                let c = st.overhangs(ta, tb);
                ok(st, c)
            }
            Target::Position => {
                st.h = v;
                let c = st.move_walls(ta, tb);
                ok(st, c)
            }
        },
        ParallelField::RafterA | ParallelField::RafterB => {
            let side = if field == ParallelField::RafterA {
                Ab::A
            } else {
                Ab::B
            };
            if side == Ab::A {
                st.r_a = Some(v);
            } else {
                st.r_b = Some(v);
            }
            if target == Target::Pitch {
                let rise = |r: f64, x: f64| (r * r - x * x).max(0.0).sqrt();
                let sum = free + st.a.overhang + st.b.overhang;
                if let (Some(ra), Some(rb)) = (st.r_a, st.r_b) {
                    // Both rafters: the ridge where the two circles meet (case P2c).
                    let (ea, eb) = (st.a.eave_height, st.b.eave_height);
                    let f = |xa: f64| ea + rise(ra, xa) - (eb + rise(rb, sum - xa));
                    let mut lo = (sum - rb).max(0.0);
                    let mut hi = ra.min(sum);
                    if !(lo < hi) || f(lo) * f(hi) > 0.0 {
                        return None;
                    }
                    for _ in 0..200 {
                        let mid = (lo + hi) / 2.0;
                        if f(mid) * f(lo) > 0.0 {
                            lo = mid;
                        } else {
                            hi = mid;
                        }
                    }
                    let xa = (lo + hi) / 2.0;
                    st.h = ea + rise(ra, xa);
                    let ov = st.a.overhang;
                    st.set_run(Ab::A, xa - ov, free);
                } else {
                    // One rafter: the ridge height stays, the ridge moves along.
                    let s = *st.side(side);
                    let x = (v * v - (st.h - s.eave_height).powi(2)).sqrt();
                    if !(x > 0.0) {
                        return None;
                    }
                    st.set_run(side, x - s.overhang, free);
                }
                let c = st.w_a > 0.0 && st.w_b > 0.0 && st.pitches();
                return ok(st, c);
            }
            // Eave, overhang and position keep the pitches: the rafter fixes
            // the ridge height.
            let d = *st.side(side);
            let (sp, cp) = d.pitch_deg.to_radians().sin_cos();
            st.h = d.eave_height + v * sp;
            match target {
                Target::Eave => {
                    // The rafter's own eave stays (case Ec).
                    let w = v * cp - d.overhang;
                    if !(w > 0.0) || !(w < free) {
                        return None;
                    }
                    st.set_run(side, w, free);
                    st.eaves(ta, tb);
                    Some(st)
                }
                Target::Overhang => {
                    let c = st.overhangs(ta, tb);
                    ok(st, c)
                }
                _ => {
                    let c = st.move_walls(ta, tb);
                    ok(st, c)
                }
            }
        }
    }
}

/// The wall patches of a parallel state for `target` (not position).
pub fn parallel_patches(st: &ParallelState, target: Target) -> (EdgePatch, EdgePatch) {
    let f = |d: &Side| match target {
        Target::Pitch => EdgePatch {
            pitch_deg: Some(r4(d.pitch_deg)),
            ..EdgePatch::default()
        },
        Target::Eave => EdgePatch {
            eave_height: Some(r4(d.eave_height)),
            ..EdgePatch::default()
        },
        _ => EdgePatch {
            overhang: Some(r4(d.overhang.max(0.0))),
            ..EdgePatch::default()
        },
    };
    (f(&st.a), f(&st.b))
}

/// DACAD refuses an eave height below the eave of a neighbouring gable
/// (measured: cases Eb, Ef, Eh; accepted on a hip roof).
pub fn eave_below_gable(edges: &[EdgeDef], i: usize, eave: f64) -> bool {
    let n = edges.len();
    [&edges[(i + n - 1) % n], &edges[(i + 1) % n]]
        .iter()
        .any(|g| g.is_gable() && eave < g.eave_height - 1e-9)
}

/// How the eave is finished after a build-up correction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildUpEave {
    /// Square to the plane.
    #[default]
    Normal,
    Horizontal,
    Vertical,
}

/// Um Aufbaudicke korrigieren: a build-up of thickness `d` (negative takes
/// it off) lifts the plane by `d` square to it. Measured in DACAD (40°,
/// d 0.2): normal eave +d·cos, overhang +d·sin; horizontal overhang +d/sin;
/// vertical eave +d/cos.
pub fn build_up(e: &EdgeDef, d: f64, eave: BuildUpEave) -> Option<EdgePatch> {
    if e.is_gable() || !(e.pitch_deg > 0.0) {
        return None;
    }
    let p = e.pitch_deg.to_radians();
    let dz = match eave {
        BuildUpEave::Normal => d * p.cos(),
        BuildUpEave::Vertical => d / p.cos(),
        BuildUpEave::Horizontal => 0.0,
    };
    let dov = match eave {
        BuildUpEave::Normal => d * p.sin(),
        BuildUpEave::Horizontal => d / p.sin(),
        BuildUpEave::Vertical => 0.0,
    };
    let mut patch = EdgePatch::default();
    if dz != 0.0 {
        patch.eave_height = Some(r4(e.eave_height + dz));
        if !e.breaks.is_empty() {
            patch.breaks = Some(lift(e, dz));
        }
    }
    if dov != 0.0 {
        let ov = e.overhang + dov;
        if ov < -1e-9 {
            return None;
        }
        patch.overhang = Some(r4(ov.max(0.0)));
    }
    Some(patch)
}

fn lift(e: &EdgeDef, dz: f64) -> Vec<crate::model::PitchBreak> {
    e.breaks
        .iter()
        .map(|b| crate::model::PitchBreak {
            height: r4(b.height + dz),
            pitch_deg: b.pitch_deg,
        })
        .collect()
}

#[derive(Clone, Copy)]
struct Seg {
    x: f64,
    z: f64,
    t: f64,
}

/// The wall's roof profile as segments from the overhang line.
fn profile(e: &EdgeDef) -> Vec<Seg> {
    let mut segs = vec![Seg {
        x: 0.0,
        z: e.eave_height,
        t: tan(e.pitch_deg),
    }];
    for b in &e.breaks {
        let last = *segs.last().expect("non-empty");
        if b.height <= last.z || !(last.t > 0.0) {
            break;
        }
        segs.push(Seg {
            x: last.x + (b.height - last.z) / last.t,
            z: b.height,
            t: tan(b.pitch_deg),
        });
    }
    segs
}

fn seg_at_run(segs: &[Seg], x: f64) -> Seg {
    *segs.iter().rev().find(|g| x >= g.x).unwrap_or(&segs[0])
}

fn seg_at_height(segs: &[Seg], h: f64) -> Seg {
    *segs.iter().rev().find(|g| h >= g.z).unwrap_or(&segs[0])
}

/// Solves wall `e` so that its roof reaches height `z` at inward distance
/// `d` (DACAD: Punkt im Raum). With pitch breaks, the pitch is that of the
/// last break; a new eave height moves the breaks with it.
pub fn solve(e: &EdgeDef, d: f64, z: f64, target: Target) -> Option<EdgePatch> {
    if e.is_gable() {
        return None;
    }
    let segs = profile(e);
    let run = d + e.overhang;
    match target {
        Target::Pitch => {
            let last = *segs.last().expect("non-empty");
            if !(run > last.x) || !(z > last.z) {
                return None;
            }
            let pitch = r4(atan_deg((z - last.z) / (run - last.x)));
            if segs.len() == 1 {
                return Some(EdgePatch {
                    pitch_deg: Some(pitch),
                    ..EdgePatch::default()
                });
            }
            let mut breaks = e.breaks.clone();
            breaks[segs.len() - 2].pitch_deg = pitch;
            Some(EdgePatch {
                breaks: Some(breaks),
                ..EdgePatch::default()
            })
        }
        Target::Eave => {
            let s = seg_at_run(&segs, run);
            let dz = z - (s.z + s.t * (run - s.x));
            Some(EdgePatch {
                eave_height: Some(r4(e.eave_height + dz)),
                breaks: (!e.breaks.is_empty()).then(|| lift(e, dz)),
                ..EdgePatch::default()
            })
        }
        Target::Overhang => {
            let s = seg_at_height(&segs, z);
            let run_at = if s.t > 0.0 {
                s.x + (z - s.z) / s.t
            } else {
                f64::NAN
            };
            let o = run_at - d;
            (o >= -1e-9).then(|| EdgePatch {
                overhang: Some(r4(o.max(0.0))),
                ..EdgePatch::default()
            })
        }
        Target::Position => None,
    }
}

/// Height of wall `e`'s roof profile at inward distance `d`.
pub fn roof_height(e: &EdgeDef, d: f64) -> f64 {
    let segs = profile(e);
    let x = d + e.overhang;
    let g = seg_at_run(&segs, x);
    g.z + g.t * (x - g.x)
}

/// Where two facing walls `span` apart meet: plan width from wall A and height.
pub fn meeting(a: &EdgeDef, b: &EdgeDef, span: f64) -> Option<(f64, f64)> {
    let f = |w: f64| roof_height(a, w) - roof_height(b, span - w);
    let (mut lo, mut hi) = (0.0, span);
    if f(lo) > 0.0 || f(hi) < 0.0 {
        return None;
    }
    for _ in 0..100 {
        let mid = (lo + hi) / 2.0;
        if f(mid) < 0.0 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Some((lo, roof_height(a, lo)))
}

/// Flächen zusammenfassen: walls parallel to the reference wall `rf` take
/// its plane, solved for the target (eave or overhang).
pub fn merge_onto(edges: &[EdgeDef], rf: usize, walls: &[usize], target: Target) -> Patches {
    let r = &edges[rf];
    let mut out = Patches::new();
    for &i in walls {
        if i == rf {
            continue;
        }
        let e = &edges[i];
        let s = inward(edges, rf, e.start);
        let z = r.eave_height + tan(r.pitch_deg) * (s + r.overhang);
        let mut patch = EdgePatch {
            pitch_deg: Some(r.pitch_deg),
            ..EdgePatch::default()
        };
        match target {
            Target::Eave => patch.eave_height = Some(r4(z - tan(r.pitch_deg) * e.overhang)),
            Target::Overhang => {
                let o = (z - e.eave_height) / tan(r.pitch_deg);
                if o < -1e-9 {
                    continue;
                }
                patch.overhang = Some(r4(o.max(0.0)));
            }
            _ => continue,
        }
        out.insert(i, patch);
    }
    out
}

/// Flächen zusammenfassen with target position: each wall moves until its
/// eave (at its overhang) lies on the reference wall's plane.
pub fn merge_by_moving(edges: &[EdgeDef], rf: usize, walls: &[usize]) -> Option<Patches> {
    let e = &edges[rf];
    let tr = tan(e.pitch_deg);
    if !(tr > 0.0) {
        return None;
    }
    let moves: Vec<Patches> = walls
        .iter()
        .filter(|&&k| k != rf)
        .map(|&k| {
            let s = inward(edges, rf, edges[k].start);
            let want = (edges[k].eave_height - e.eave_height) / tr + edges[k].overhang - e.overhang;
            let pitch = BTreeMap::from([(
                k,
                EdgePatch {
                    pitch_deg: Some(e.pitch_deg),
                    ..EdgePatch::default()
                },
            )]);
            combine([&move_wall(edges, k, want - s), &pitch])
        })
        .collect();
    Some(combine(moves.iter()))
}

/// Walls of the outline parallel to wall `i` and facing the same way.
pub fn same_way(edges: &[EdgeDef], i: usize) -> Vec<usize> {
    let (_, fu, _) = wall_frame(edges, i);
    let n = edges.len();
    (0..n)
        .filter(|&k| {
            let e = &edges[k];
            if is_marker(e) || e.is_gable() {
                return false;
            }
            let (_, gu, _) = wall_frame(edges, k);
            e.start.distance(edges[(k + 1) % n].start) > 1e-6
                && gu.x * fu.x + gu.y * fu.y > 1.0 - 1e-6
        })
        .collect()
}

/// Inward shift of wall `e` that makes its roof reach height `z` at inward
/// distance `d` (DACAD: Lage berechnen).
pub fn shift_for(e: &EdgeDef, d: f64, z: f64) -> Option<f64> {
    if e.is_gable() {
        return None;
    }
    let segs = profile(e);
    let s = seg_at_height(&segs, z);
    if !(s.t > 0.0) || z < segs[0].z {
        return None;
    }
    Some(d - (s.x + (z - s.z) / s.t - e.overhang))
}

/// Moves wall `i` parallel by `delta` inward; its ends slide along the
/// neighbouring walls (or move with it where a neighbour is parallel).
/// Zero-length marker walls at a moved corner move with it.
pub fn move_wall(edges: &[EdgeDef], i: usize, delta: f64) -> Patches {
    let n = edges.len() as isize;
    let at = |k: isize| edges[k.rem_euclid(n) as usize].start;
    let short = |k: isize| at(k).distance(at(k + 1)) < 1e-6;
    let (fa, fu, fnrm) = wall_frame(edges, i);
    let a = Point2::new(fa.x + fnrm.x * delta, fa.y + fnrm.y * delta);
    let meet = |p: Point2, q: Point2, fallback: Point2| {
        let v = Point2::new(q.x - p.x, q.y - p.y);
        let den = fu.x * v.y - fu.y * v.x;
        if den.abs() < 1e-9 * 1e-9f64.max(v.x.hypot(v.y)) {
            return fallback;
        }
        let t = ((p.x - a.x) * v.y - (p.y - a.y) * v.x) / den;
        Point2::new(round_to(a.x + fu.x * t, 3), round_to(a.y + fu.y * t, 3))
    };
    let shifted = |p: Point2| {
        Point2::new(
            round_to(p.x + fnrm.x * delta, 3),
            round_to(p.y + fnrm.y * delta, 3),
        )
    };
    let set = |out: &mut Patches, j: isize, p: Point2| {
        out.insert(
            j.rem_euclid(n) as usize,
            EdgePatch {
                start: Some(p),
                ..EdgePatch::default()
            },
        );
    };
    let mut out = Patches::new();
    let i = i as isize;
    let mut start_idx = vec![i];
    let mut k = i - 1;
    while short(k) && (start_idx.len() as isize) < n {
        start_idx.push(k);
        k -= 1;
    }
    let s = meet(at(k), at(k + 1), shifted(at(i)));
    for j in start_idx {
        set(&mut out, j, s);
    }
    let mut end_idx = vec![i + 1];
    k = i + 1;
    while short(k) && (end_idx.len() as isize) < n {
        k += 1;
        end_idx.push(k);
    }
    let e = meet(at(k), at(k + 1), shifted(at(i + 1)));
    for j in end_idx {
        set(&mut out, j, e);
    }
    out
}

/// Grat/Kehle: from the natural measures of an existing roof (hip or valley
/// length and the two rafter lengths, both from the eave lines) the pitches
/// of two neighbouring walls `i` and `j` with the same eave height.
pub fn hip_pitches(
    edges: &[EdgeDef],
    i: usize,
    j: usize,
    hip: f64,
    r1: f64,
    r2: f64,
) -> Option<Patches> {
    let n = edges.len();
    let (first, second, ra, rb) = if (i + 1) % n == j {
        (i, j, r1, r2)
    } else {
        (j, i, r2, r1)
    };
    if (first + 1) % n != second {
        return None;
    }
    let (e1, e2) = (&edges[first], &edges[second]);
    if e1.is_gable() || e2.is_gable() || (e1.eave_height - e2.eave_height).abs() > 1e-3 {
        return None;
    }
    let (_, u1, _) = wall_frame(edges, first);
    let (_, u2, _) = wall_frame(edges, second);
    // Interior angle at the corner; above 180° the line is a valley.
    let alpha = std::f64::consts::PI - (u1.x * u2.y - u1.y * u2.x).atan2(u1.x * u2.x + u1.y * u2.y);
    let s2 = alpha.sin().powi(2);
    if s2 < 1e-9 {
        return None;
    }
    let f = |h: f64| {
        let d1 = (ra * ra - h * h).max(0.0).sqrt();
        let d2 = (rb * rb - h * h).max(0.0).sqrt();
        (d1 * d1 + d2 * d2 + 2.0 * d1 * d2 * alpha.cos()) / s2 + h * h - hip * hip
    };
    let (mut lo, mut hi) = (0.0, ra.min(rb));
    if !(hi > 0.0) || f(lo) < 0.0 || f(hi) > 0.0 {
        return None;
    }
    for _ in 0..200 {
        let mid = (lo + hi) / 2.0;
        if f(mid) > 0.0 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let h = (lo + hi) / 2.0;
    let pitch = |r: f64| r4(atan_deg(h / (r * r - h * h).sqrt()));
    let patch = |p| EdgePatch {
        pitch_deg: Some(p),
        ..EdgePatch::default()
    };
    Some(Patches::from([
        (first, patch(pitch(ra))),
        (second, patch(pitch(rb))),
    ]))
}

/// Grat/Kehle with the eave pieces instead of the rafters: `e1`, `e2` are
/// the plan lengths along each eave line from the corner to the foot of the
/// rafter through the hip's top end (measured: hip 7.5, pieces 5 and 4.5 on
/// a square corner give 36.39° and 33.56°).
pub fn hip_pitches_from_eaves(
    edges: &[EdgeDef],
    i: usize,
    j: usize,
    hip: f64,
    e1: f64,
    e2: f64,
) -> Option<Patches> {
    let n = edges.len();
    let (first, second, ea, eb) = if (i + 1) % n == j {
        (i, j, e1, e2)
    } else {
        (j, i, e2, e1)
    };
    if (first + 1) % n != second {
        return None;
    }
    let (_, u1, n1) = wall_frame(edges, first);
    let (_, u2, n2) = wall_frame(edges, second);
    let det = u1.x * u2.y - u1.y * u2.x;
    if det.abs() < 1e-9 {
        return None;
    }
    let px = (-ea * u2.y - eb * u1.y) / det;
    let py = (u1.x * eb + u2.x * ea) / det;
    let d1 = px * n1.x + py * n1.y;
    let d2 = px * n2.x + py * n2.y;
    let h2 = hip * hip - (px * px + py * py);
    if !(d1 > 0.0) || !(d2 > 0.0) || !(h2 > 0.0) {
        return None;
    }
    let h = h2.sqrt();
    hip_pitches(edges, first, second, hip, d1.hypot(h), d2.hypot(h))
}

#[cfg(test)]
mod tests {
    //! Against values DACAD itself stored (2026-10-29). Projects: 10 × 8,
    //! walls counter-clockwise from (0, 0), eave 3.0, overhang 0.5, pitch
    //! 40° (gable ends 90° where named).
    use super::*;
    use crate::edit::plain_edge;
    use crate::model::PitchBreak;

    fn wall(x: f64, y: f64, pitch: f64) -> EdgeDef {
        plain_edge(Point2::new(x, y), pitch, 3.0, 0.5)
    }
    fn gable() -> Vec<EdgeDef> {
        vec![
            wall(0.0, 0.0, 40.0),
            wall(10.0, 0.0, 90.0),
            wall(10.0, 8.0, 40.0),
            wall(0.0, 8.0, 90.0),
        ]
    }
    fn hip() -> Vec<EdgeDef> {
        vec![
            wall(0.0, 0.0, 40.0),
            wall(10.0, 0.0, 40.0),
            wall(10.0, 8.0, 40.0),
            wall(0.0, 8.0, 40.0),
        ]
    }
    fn close(got: Option<f64>, want: f64, d: i32) {
        let got = got.expect("a value");
        assert!((got - want).abs() < 0.5 * 10f64.powi(-d), "{got} vs {want}");
    }
    fn broken() -> EdgeDef {
        let mut k = wall(0.0, 0.0, 40.0);
        k.breaks = vec![PitchBreak {
            height: 4.5,
            pitch_deg: 60.0,
        }];
        k
    }

    #[test]
    fn simple_triangle() {
        let e = &gable()[0];
        let p = |w, r, h, ov| triangle(e, w, r, h, ov).map(|t| t.pitch_deg);
        close(p(Some(4.0), None, Some(6.0), false), 33.69007, 4); // Ta
        close(p(None, Some(5.5), Some(6.0), false), 33.05573, 4); // Tc
        close(p(Some(4.0), Some(5.5), None, false), 35.0968, 4); // Td
        close(p(Some(4.5), None, Some(6.0), true), 33.69007, 4);
        close(
            triangle(&broken(), Some(4.0), None, Some(7.0), false).map(|t| t.pitch_deg),
            29.0546,
            4,
        ); // Te
    }

    fn run(
        edges: &[EdgeDef],
        target: Target,
        steps: &[(ParallelField, f64)],
        with_ov: bool,
    ) -> ParallelState {
        let (a, b) = (&edges[2], &edges[0]);
        let (w, h) = meeting(a, b, 8.0).unwrap();
        let mut st = parallel_start(a, b, 8.0, w, h);
        for &(f, v) in steps {
            st = parallel_edit(&st, f, v, target, with_ov).unwrap();
        }
        st
    }
    fn pp(
        edges: &[EdgeDef],
        target: Target,
        steps: &[(ParallelField, f64)],
        with_ov: bool,
    ) -> (EdgePatch, EdgePatch) {
        parallel_patches(&run(edges, target, steps, with_ov), target)
    }
    use ParallelField::*;

    #[test]
    fn two_parallels() {
        let (n, s) = pp(&gable(), Target::Pitch, &[(WidthA, 4.0)], true);
        close(n.pitch_deg, 43.3496, 4);
        close(s.pitch_deg, 37.0597, 4);
        close(
            pp(&gable(), Target::Pitch, &[(Ridge, 7.5)], true)
                .0
                .pitch_deg,
            45.0,
            4,
        );
        let (n, s) = pp(
            &gable(),
            Target::Pitch,
            &[(RafterA, 6.0), (RafterB, 5.5)],
            true,
        );
        close(n.pitch_deg, 36.5593, 4);
        close(s.pitch_deg, 40.5269, 4);
        close(
            pp(&gable(), Target::Pitch, &[(Top, 1.0)], true).0.pitch_deg,
            40.0,
            4,
        );
        let (n, s) = pp(&gable(), Target::Pitch, &[(WidthA, 3.0)], false);
        close(n.pitch_deg, 47.172, 3);
        close(s.pitch_deg, 34.471, 3);
        close(
            pp(&gable(), Target::Eave, &[(Ridge, 7.5)], true)
                .1
                .eave_height,
            3.72405,
            4,
        );
        let (n, s) = pp(&gable(), Target::Eave, &[(RafterA, 6.0)], true);
        close(n.eave_height, 3.0, 4);
        close(s.eave_height, 3.16156, 4);
        let (n, s) = pp(&hip(), Target::Eave, &[(WidthA, 4.0)], true);
        close(n.eave_height, 3.41955, 4);
        close(s.eave_height, 2.58045, 4);
        let (_, s) = pp(&gable(), Target::Eave, &[(WidthA, 4.0)], true);
        assert!(eave_below_gable(&gable(), 0, s.eave_height.unwrap()));
        assert!(!eave_below_gable(&hip(), 0, s.eave_height.unwrap()));
        let (n, s) = pp(&gable(), Target::Overhang, &[(WidthA, 4.0)], true);
        close(n.overhang, 0.0, 4);
        close(s.overhang, 1.0, 4);
        close(
            pp(&gable(), Target::Overhang, &[(Ridge, 7.5)], true)
                .0
                .overhang,
            1.36289,
            4,
        );
        close(
            pp(&gable(), Target::Overhang, &[(Ridge, 6.5)], true)
                .1
                .overhang,
            0.17114,
            4,
        );
        let (n, s) = pp(&gable(), Target::Overhang, &[(RafterA, 6.5)], true);
        close(n.overhang, 0.97929, 4);
        close(s.overhang, 0.97929, 4);
        let st = run(&gable(), Target::Position, &[(Ridge, 7.5)], true);
        close(Some(st.shift_a), -0.8629, 4);
        close(Some(st.shift_b), -0.8629, 4);
        close(
            Some(run(&gable(), Target::Position, &[(WidthA, 4.0)], true).shift_b),
            0.5,
            4,
        );
        close(
            Some(run(&gable(), Target::Position, &[(RafterA, 6.0)], true).shift_a),
            -0.0963,
            4,
        );
    }

    #[test]
    fn point_in_space() {
        let s = &gable()[0];
        close(
            solve(s, 4.0, 7.0, Target::Pitch).and_then(|p| p.pitch_deg),
            41.63354,
            4,
        );
        close(
            solve(s, 4.0, 7.0, Target::Eave).and_then(|p| p.eave_height),
            3.22405,
            4,
        );
        close(
            solve(s, 4.0, 7.0, Target::Overhang).and_then(|p| p.overhang),
            0.76701,
            4,
        );
        let k = solve(&broken(), 4.0, 7.0, Target::Pitch).unwrap();
        close(Some(k.breaks.unwrap()[0].pitch_deg), 42.66687, 4);
        close(shift_for(s, 4.0, 7.0), -0.267, 3);
    }

    #[test]
    fn merge_faces() {
        let step = || {
            vec![
                wall(0.0, 0.0, 40.0),
                wall(5.0, 0.0, 90.0),
                wall(5.0, 1.0, 40.0),
                wall(10.0, 1.0, 90.0),
                wall(10.0, 8.0, 40.0),
                wall(0.0, 8.0, 90.0),
            ]
        };
        close(
            merge_onto(&step(), 0, &[2], Target::Eave)[&2].eave_height,
            3.8391,
            4,
        );
        close(
            merge_onto(&step(), 0, &[2], Target::Overhang)[&2].overhang,
            1.5,
            4,
        );
        assert_eq!(same_way(&step(), 0), vec![0, 2]);
    }

    #[test]
    fn hip_or_valley() {
        let m = hip_pitches(&hip(), 0, 1, 7.5, 5.5, 6.0).unwrap();
        close(m[&0].pitch_deg, 35.09679, 4);
        close(m[&1].pitch_deg, 31.8061, 4);
        let m = hip_pitches_from_eaves(&hip(), 0, 1, 7.5, 5.0, 4.5).unwrap();
        close(m[&0].pitch_deg, 36.39125, 3);
        close(m[&1].pitch_deg, 33.55732, 4);
    }

    #[test]
    fn build_up_thickness() {
        let s = &gable()[0];
        let p = build_up(s, 0.2, BuildUpEave::Normal).unwrap();
        close(p.eave_height, 3.15321, 4);
        close(p.overhang, 0.62856, 4);
        close(
            build_up(s, 0.2, BuildUpEave::Horizontal).unwrap().overhang,
            0.81114,
            4,
        );
        close(
            build_up(s, 0.2, BuildUpEave::Vertical).unwrap().eave_height,
            3.26108,
            4,
        );
    }

    #[test]
    fn moving_a_wall_slides_its_ends_along_the_neighbours() {
        let m = move_wall(&gable(), 0, 1.0);
        assert_eq!(m[&0].start, Some(Point2::new(0.0, 1.0)));
        assert_eq!(m[&1].start, Some(Point2::new(10.0, 1.0)));
    }
}
