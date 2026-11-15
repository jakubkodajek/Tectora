//! A building turned on the site plan and rounded to centimetres, put back
//! onto common wall lines.
//!
//! Walls that are one line (or parallel) in the building as drawn are
//! a few millimetres out of line once the corners are rounded to
//! centimetres, which the departures of [`crate::openings`] do not see: the
//! planes are "not parallel" (`coplanar`, 0.05 degrees) and the corners "not
//! on one line" (`wing_split`, 5 mm). Jakub decided (2026-11-15) that where
//! the core stops on such a building, the corners may be moved by at most a
//! centimetre onto common lines before the departures are tried again. The
//! result is checked against the straightened walls ([`crate::check::sound`]).
//! What holds: no corner moves more than a centimetre. The two ends of
//! a wall along the building's frame (fitted to all its walls) are one
//! line; two such lines become one where their means are closer than
//! rounding to centimetres usually puts the pieces of one line (about
//! 1 cm, see `ONE_LINE`) and their corners spread no further than
//! rounding can spread them. So lines 2 cm apart (a step) stay two lines
//! unless the rounding hides half of the step, and the pieces of one drawn
//! line are found again unless the rounding drives them that far apart;
//! each happens at under one turn in a hundred, where a centimetre grid
//! cannot tell the two apart.

use crate::geom::{Point2, signed_area};
use crate::model::RoofPart;
use crate::wing_split::MAX_WALLS;

/// The most a corner is moved, m.
pub(crate) const MAX_SHIFT: f64 = 0.01;
/// A wall whose ends are this far off the frame's axes (across the wall) is
/// not one of its walls, m: the two ends rounded to centimetres are up to
/// 1.42 cm off, and a little more for the frame's own error.
const ALIGNED: f64 = 0.0145;
/// Lines whose means are closer than this are one line, m, and
/// [`ONE_LINE_SHARE`] of the furthest rounding puts two corners of one
/// line apart (2h: 1 cm for a building along the axes, 1.41 cm at 45
/// degrees) where that is less. Measured on T-houses turned by every tenth
/// of a degree and rounded to centimetres, both ways: the pieces of one
/// wall line must be found again (with 1.1 cm and 0.94 they are not at 11
/// of 3600 turns; with 1.0 cm at 52), and a 2 cm step between two wall
/// lines must stay (flattened at 16 of 3600 turns; with a share of 0.95 or
/// more also at 3.2 degrees, with 1.15 cm at 48 turns). A centimetre grid
/// cannot tell the two apart at those turns.
const ONE_LINE: f64 = 0.011;
const ONE_LINE_SHARE: f64 = 0.94;
/// What the corners of one line may spread beyond the worst case of
/// rounding (2h), m: the frame is fitted to rounded corners and is a few
/// tenths of a milliradian off, which is millimetres over a long line.
const FRAME_SLACK: f64 = 0.004;
/// The most times the frame is fitted to the walls (it settles in two).
const REFITS: usize = 8;
/// Corners moved less than this are not moved at all, m.
const NOTHING: f64 = 1e-6;

/// Angle of a wall direction modulo 90 degrees, in (-45, 45].
fn mod90(angle: f64) -> f64 {
    let q = angle.to_degrees().rem_euclid(90.0);
    if q > 45.0 { q - 90.0 } else { q }
}

/// How far across a wall of length `len` its far end is from the axis
/// through its start, for a wall `dev` radians off the nearest axis.
fn across(len: f64, dev: f64) -> f64 {
    len * mod90(dev).to_radians().sin().abs()
}

/// The lines of one axis: for each corner the line it is on (`None`: on
/// none), and which axis (0: the lines' corners share `u`, 1: `v`).
#[derive(Debug)]
struct Lines {
    axis: usize,
    of: Vec<Option<usize>>,
}

impl Lines {
    fn corners(&self, line: usize) -> impl Iterator<Item = usize> + '_ {
        (0..self.of.len()).filter(move |&i| self.of[i] == Some(line))
    }

    fn across(&self, uv: (f64, f64)) -> f64 {
        if self.axis == 0 { uv.0 } else { uv.1 }
    }

    fn join(&mut self, a: usize, b: usize) {
        for l in self.of.iter_mut().filter(|l| **l == Some(b)) {
            *l = Some(a);
        }
    }

    /// The lines there are, each with its corners' count, sum, least and
    /// greatest value across it.
    fn extents(&self, uv: &[(f64, f64)]) -> Vec<(usize, f64, f64, f64, f64)> {
        let mut out: Vec<(usize, f64, f64, f64, f64)> = Vec::new();
        for (i, l) in self.of.iter().enumerate() {
            let Some(l) = *l else { continue };
            let v = self.across(uv[i]);
            match out.iter_mut().find(|e| e.0 == l) {
                Some(e) => *e = (l, e.1 + 1.0, e.2 + v, e.3.min(v), e.4.max(v)),
                None => out.push((l, 1.0, v, v, v)),
            }
        }
        out
    }

    /// Joins lines nearest first (by their means) while two are near
    /// ([`ONE_LINE`], [`ONE_LINE_SHARE`] of `reach`, the furthest rounding
    /// puts two corners of one line apart) and their corners together
    /// spread no further than `reach` and [`FRAME_SLACK`].
    fn join_near(&mut self, uv: &[(f64, f64)], reach: f64) {
        let near = ONE_LINE.min(ONE_LINE_SHARE * reach);
        loop {
            let e = self.extents(uv);
            let mut best: Option<(f64, usize, usize)> = None;
            for x in 0..e.len() {
                for y in x + 1..e.len() {
                    let gap = (e[x].2 / e[x].1 - e[y].2 / e[y].1).abs();
                    let all = e[x].4.max(e[y].4) - e[x].3.min(e[y].3);
                    let nearer = best.is_none_or(|b| gap < b.0);
                    if gap < near && all <= reach + FRAME_SLACK && nearer {
                        best = Some((gap, e[x].0, e[y].0));
                    }
                }
            }
            match best {
                Some((_, a, b)) => self.join(a.min(b), a.max(b)),
                None => return,
            }
        }
    }
}

/// The building with its corners moved (by at most [`MAX_SHIFT`]) onto
/// common lines in the frame of its walls; `None` when nothing
/// moves, when it would take more than rounding to centimetres, or when the
/// outline is not one this works on.
///
/// The frame starts as the direction of the longest wall and the walls
/// along its axes and is then fitted to all walls along it (least
/// squares); the lines are found in the fitted frame.
pub(crate) fn straightened(part: &RoofPart) -> Option<RoofPart> {
    let n = part.edges.len();
    if !(4..=MAX_WALLS).contains(&n) {
        return None;
    }
    let pts = part.outline();
    if pts.iter().any(|p| !(p.x.is_finite() && p.y.is_finite())) {
        return None;
    }
    let walls: Vec<(f64, f64)> = (0..n)
        .map(|i| {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            (a.distance(b), (b.y - a.y).atan2(b.x - a.x))
        })
        .collect();
    let (_, longest) = walls
        .iter()
        .copied()
        .filter(|w| w.0.is_finite())
        .max_by(|a, b| a.0.total_cmp(&b.0))?;
    // The first frame: the mean direction (modulo 90 degrees) of the walls
    // near the longest wall's axes, weighted by the square of the length.
    let (mut sx, mut sy) = (0.0, 0.0);
    for &(len, ang) in &walls {
        if len > 0.0 && across(len, ang - longest) <= ALIGNED {
            sx += len * len * (4.0 * ang).cos();
            sy += len * len * (4.0 * ang).sin();
        }
    }
    if !(sx.is_finite() && sy.is_finite()) || sx.hypot(sy) < 1e-9 {
        return None;
    }
    let mut frame = sy.atan2(sx) / 4.0;
    // Relative to the middle of the building, so that a turn of the frame
    // costs no distance far from the origin.
    let nf = n as f64;
    let (cx, cy) = (
        pts.iter().map(|p| p.x).sum::<f64>() / nf,
        pts.iter().map(|p| p.y).sum::<f64>() / nf,
    );
    let frame_of = |frame: f64| -> Vec<(f64, f64)> {
        let (s, c) = frame.sin_cos();
        pts.iter()
            .map(|p| {
                let (x, y) = (p.x - cx, p.y - cy);
                (x * c + y * s, -x * s + y * c)
            })
            .collect()
    };
    // Rounding to centimetres puts a corner up to half a centimetre off in
    // x and in y, which is `h` along an axis of the frame; two corners of
    // one line are up to 2h apart.
    let reach = |frame: f64| 0.005 * (frame.cos().abs() + frame.sin().abs());
    // The frame fitted to the walls alone: a line made of several walls
    // would fix it better, but a step taken for one line would turn the
    // frame until the step is gone.
    for _ in 0..REFITS {
        let uv = frame_of(frame);
        let turn = fitted_turn(&wall_lines(&walls, &uv, frame), &uv);
        if !turn.is_finite() || turn.abs() > 0.01 {
            return None;
        }
        frame += turn;
        if turn.abs() < 1e-9 {
            break;
        }
    }
    let h = reach(frame);
    let mut uv = frame_of(frame);
    let mut lines = wall_lines(&walls, &uv, frame);
    for set in lines.iter_mut() {
        set.join_near(&uv, 2.0 * h);
    }
    let (s, c) = frame.sin_cos();
    for set in &lines {
        for (line, count, sum, lo, hi) in set.extents(&uv) {
            // The mean, kept within the rounding's reach of every corner
            // of the line where they leave room for that (the middle
            // where they do not).
            let (least, most) = (hi - h, lo + h);
            let at = if least <= most {
                (sum / count).max(least).min(most)
            } else {
                (lo + hi) / 2.0
            };
            let on: Vec<usize> = set.corners(line).collect();
            for i in on {
                if set.axis == 0 {
                    uv[i].0 = at;
                } else {
                    uv[i].1 = at;
                }
            }
        }
    }
    let moved: Vec<Point2> = uv
        .iter()
        .map(|&(u, v)| Point2::new(u * c - v * s + cx, u * s + v * c + cy))
        .collect();
    let shift = moved
        .iter()
        .zip(&pts)
        .map(|(a, b)| a.distance(*b))
        .fold(0.0, f64::max);
    if !shift.is_finite() || shift <= NOTHING || shift > MAX_SHIFT {
        return None;
    }
    for i in 0..n {
        let (old, new) = (walls[i].0, moved[i].distance(moved[(i + 1) % n]));
        if !new.is_finite() || new < 0.01 || (old - new).abs() > old / 2.0 {
            return None;
        }
    }
    let (before, after) = (signed_area(&pts), signed_area(&moved));
    if !after.is_finite() || (after - before).abs() > 1e-3 * before.abs() {
        return None;
    }
    let mut out = part.clone();
    for (e, p) in out.edges.iter_mut().zip(moved) {
        e.start = p;
    }
    Some(out)
}

/// The lines of the corners `uv` (in the frame `frame`) across both axes
/// that the walls give: the two ends of each wall along the frame.
fn wall_lines(walls: &[(f64, f64)], uv: &[(f64, f64)], frame: f64) -> [Lines; 2] {
    let n = walls.len();
    let along = |w: usize| walls[w].0 > 0.0 && across(walls[w].0, walls[w].1 - frame) <= ALIGNED;
    // Corners between two walls of the frame.
    let member = |i: usize| along((i + n - 1) % n) && along(i);
    let mut sets = [0, 1].map(|axis| Lines {
        axis,
        of: (0..n).map(|i| member(i).then_some(i)).collect(),
    });
    for w in 0..n {
        let (i, j) = (w, (w + 1) % n);
        if !(member(i) && member(j)) {
            continue;
        }
        let (du, dv) = (uv[j].0 - uv[i].0, uv[j].1 - uv[i].1);
        let set = &mut sets[if du.abs() < dv.abs() { 0 } else { 1 }];
        if let (Some(a), Some(b)) = (set.of[i], set.of[j])
            && a != b
        {
            set.join(a.min(b), a.max(b));
        }
    }
    sets
}

/// The turn of the frame that brings the lines closest to its axes (least
/// squares): on a line of axis 0 `u + turn * v` is constant, of axis 1
/// `v - turn * u`.
fn fitted_turn(lines: &[Lines; 2], uv: &[(f64, f64)]) -> f64 {
    let (mut num, mut den) = (0.0, 0.0);
    for set in lines {
        for (line, count, ..) in set.extents(uv) {
            if count < 2.0 {
                continue;
            }
            let (mu, mv) = set.corners(line).fold((0.0, 0.0), |m, i| {
                (m.0 + uv[i].0 / count, m.1 + uv[i].1 / count)
            });
            for i in set.corners(line) {
                let (du, dv) = (uv[i].0 - mu, uv[i].1 - mv);
                if set.axis == 0 {
                    num -= du * dv;
                    den += dv * dv;
                } else {
                    num += du * dv;
                    den += du * du;
                }
            }
        }
    }
    if den > 1e-9 { num / den } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DEFAULT_MATERIAL, EdgeDef};
    use crate::wing_split::MAX_WALLS;

    fn edge(x: f64, y: f64, pitch_deg: f64, eave_height: f64, overhang: f64) -> EdgeDef {
        EdgeDef {
            start: Point2::new(x, y),
            pitch_deg,
            eave_height,
            overhang,
            half_hip: None,
            breaks: Vec::new(),
            priority: 0,
            direction_rad: None,
            clad: false,
            source: None,
            material: DEFAULT_MATERIAL,
        }
    }

    /// Jakub's T outline: 8 walls, gables on the odd ones.
    const T: [(f64, f64); 8] = [
        (5.0, 0.0),
        (10.0, 0.0),
        (10.0, 2.5),
        (15.0, 2.5),
        (15.0, 7.5),
        (0.0, 7.5),
        (0.0, 2.5),
        (5.0, 2.5),
    ];

    /// Measured counts (of 3600 turns), as budgets: see `ONE_LINE`.
    const FRONT_BUDGET: usize = 12;
    const STEP_BUDGET: usize = 16;

    fn part(pts: &[(f64, f64)]) -> RoofPart {
        let edges = pts
            .iter()
            .enumerate()
            .map(|(i, &(x, y))| edge(x, y, if i % 2 == 1 { 90.0 } else { 40.0 }, 5.0, 0.5))
            .collect();
        RoofPart::new("t", edges)
    }

    fn turned(pts: &[(f64, f64)], deg: f64, step: f64) -> Vec<(f64, f64)> {
        let (s, c) = deg.to_radians().sin_cos();
        let r = |v: f64| (v / step).round() * step;
        pts.iter()
            .map(|&(x, y)| (r(x * c - y * s), r(x * s + y * c)))
            .collect()
    }

    fn pts_of(p: &RoofPart) -> Vec<(f64, f64)> {
        p.edges.iter().map(|e| (e.start.x, e.start.y)).collect()
    }

    #[test]
    fn an_exactly_drawn_outline_is_not_straightened() {
        assert!(straightened(&part(&T)).is_none());
    }

    #[test]
    fn an_exactly_turned_outline_is_not_straightened() {
        // Unrounded turn: already on common lines in the wall frame.
        for deg in [17.3, 45.0, 200.0] {
            assert!(
                straightened(&part(&turned(&T, deg, 1e-12))).is_none(),
                "{deg}"
            );
        }
    }

    #[test]
    fn a_cm_rounded_turned_t_is_straightened_by_at_most_a_centimetre() {
        for deg in [1.0, 5.0, 17.3, 33.0, 45.0, 61.7, 200.0] {
            let drawn = part(&turned(&T, deg, 0.01));
            let s = straightened(&drawn).unwrap_or_else(|| panic!("{deg}: None"));
            let (a, b) = (pts_of(&drawn), pts_of(&s));
            for (p, q) in a.iter().zip(&b) {
                let d = ((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)).sqrt();
                assert!(d <= 0.01 + 1e-12, "{deg}: corner moved {d}");
            }
            // Walls of one direction are parallel within 1e-9: the cross
            // product of the unit directions of wall 0 and wall 4 (both along
            // the longest wall's frame) and of 1 and 3 (the other axis).
            let dir = |i: usize| {
                let (p, q) = (b[i], b[(i + 1) % b.len()]);
                let l = ((q.0 - p.0).powi(2) + (q.1 - p.1).powi(2)).sqrt();
                ((q.0 - p.0) / l, (q.1 - p.1) / l)
            };
            for (i, j) in [(0, 2), (0, 4), (0, 6), (1, 3), (1, 5), (1, 7)] {
                let (u, v) = (dir(i), dir(j));
                assert!((u.0 * v.1 - u.1 * v.0).abs() < 1e-9, "{deg}: walls {i} {j}");
            }
            // Every wall keeps its definition and index.
            for (x, y) in drawn.edges.iter().zip(&s.edges) {
                assert_eq!(
                    (x.pitch_deg, x.eave_height, x.overhang),
                    (y.pitch_deg, y.eave_height, y.overhang)
                );
            }
        }
    }

    #[test]
    fn a_deliberate_jog_of_one_and_a_half_centimetres_is_not_flattened() {
        let mut pts = T.to_vec();
        // Move the notch corner 1.5 cm: a jog, not rounding noise.
        pts[2].0 += 0.015;
        pts[3].0 += 0.015;
        assert!(straightened(&part(&pts)).is_none());
    }

    /// How far corner 3 (the right house-front piece) lies off the line
    /// of the left one (corners 6 and 7), m.
    fn front_step(p: &[(f64, f64)]) -> f64 {
        let (a, b, c) = (p[6], p[7], p[3]);
        let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        ((b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)).abs() / len
    }

    /// Review MATH-001 of round 2: at every tenth of a degree, the two
    /// house-front pieces of the T (one line as drawn) are one line again;
    /// the count left is where rounding drives them 1.1 cm apart. And no
    /// corner moves more than a centimetre at any turn.
    #[test]
    fn the_pieces_of_one_wall_line_are_found_again_at_nearly_every_turn() {
        let mut apart = Vec::new();
        for k in 0..3600 {
            let deg = k as f64 * 0.1;
            let drawn = part(&turned(&T, deg, 0.01));
            let Some(s) = straightened(&drawn) else {
                // Nothing to move: the rounded corners are on their lines.
                assert!(front_step(&pts_of(&drawn)) < 1e-6, "{deg}: None");
                continue;
            };
            for (p, q) in pts_of(&drawn).iter().zip(&pts_of(&s)) {
                let d = ((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)).sqrt();
                assert!(d <= MAX_SHIFT + 1e-12, "{deg}: corner moved {d}");
            }
            if front_step(&pts_of(&s)) > 1e-6 {
                apart.push(deg);
            }
        }
        assert!(apart.len() <= FRONT_BUDGET, "{}: {apart:?}", apart.len());
    }

    /// Review MATH-004 of round 2: a step of 2 cm between the two
    /// house-front pieces stays a step, but for the turns where rounding
    /// hides half of it; a step of 2.5 cm stays at every turn.
    #[test]
    fn a_two_centimetre_step_stays_at_nearly_every_turn() {
        for (step, budget) in [(0.02, STEP_BUDGET), (0.025, 0)] {
            let mut pts = T.to_vec();
            pts[2].1 += step;
            pts[3].1 += step;
            let mut gone = Vec::new();
            for k in 0..3600 {
                let deg = k as f64 * 0.1;
                let drawn = part(&turned(&pts, deg, 0.01));
                if let Some(s) = straightened(&drawn)
                    && front_step(&pts_of(&s)) < 0.002
                {
                    gone.push(deg);
                }
            }
            assert!(
                gone.len() <= budget,
                "step {step}: {}: {gone:?}",
                gone.len()
            );
        }
    }

    #[test]
    fn bad_input_gives_none_without_a_panic() {
        let mut nan = T.to_vec();
        nan[3].0 = f64::NAN;
        assert!(straightened(&part(&nan)).is_none());
        let mut inf = T.to_vec();
        inf[3].1 = f64::INFINITY;
        assert!(straightened(&part(&inf)).is_none());
        assert!(straightened(&part(&[(0.0, 0.0), (4.0, 0.0), (0.0, 3.0)])).is_none());
        let many: Vec<(f64, f64)> = (0..MAX_WALLS + 1)
            .map(|i| {
                let a = i as f64 * std::f64::consts::TAU / (MAX_WALLS + 1) as f64;
                (10.0 * a.cos(), 10.0 * a.sin())
            })
            .collect();
        assert!(straightened(&part(&many)).is_none());
    }
}
