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
//! What holds: no corner moves more than a centimetre, and lines closer
//! than rounding to centimetres can tell apart (about 1 cm, which
//! depends on the turn) become one. A parallel offset of 1.5 cm or more
//! between two lines is kept unless the rounding noise hides it, so a step
//! is flattened only where a centimetre grid cannot show it.

use crate::geom::{Point2, signed_area};
use crate::model::RoofPart;
use crate::wing_split::MAX_WALLS;

/// The most a corner is moved, m.
pub(crate) const MAX_SHIFT: f64 = 0.01;
/// A wall whose ends are this far off the frame's axes (across the wall) is
/// not one of its walls, m: the two ends rounded to centimetres are up to
/// 1.42 cm off, and a little more for the frame's own error.
const ALIGNED: f64 = 0.0145;
/// The share of the worst case of rounding (2h) up to which two coordinates
/// are one line. Measured on turned cm-rounded T-houses: just under the worst
/// case keeps a 2 cm step apart where the rounding hides most of it, and
/// still joins the corners of a line at nearly all turns.
const SPAN_SHARE: f64 = 0.97;
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

/// The corners `members` sorted by `value`, in groups that start at a value
/// and reach at most `span` above it.
fn groups(members: &[usize], value: &dyn Fn(usize) -> f64, span: f64) -> Vec<Vec<usize>> {
    let mut order = members.to_vec();
    order.sort_by(|&a, &b| value(a).total_cmp(&value(b)));
    let mut out = Vec::new();
    let mut k = 0;
    while k < order.len() {
        let first = value(order[k]);
        let mut m = k;
        while m + 1 < order.len() && value(order[m + 1]) - first <= span {
            m += 1;
        }
        out.push(order[k..=m].to_vec());
        k = m + 1;
    }
    out
}

/// The building with its corners moved (by at most [`MAX_SHIFT`]) onto
/// common lines in the frame of its longest wall; `None` when nothing
/// moves, when it would take more than rounding to centimetres, or when the
/// outline is not one this works on.
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
    // The frame: the mean direction (modulo 90 degrees) of the walls near
    // the longest wall's axes, weighted by the square of the length.
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
    let frame = sy.atan2(sx) / 4.0;
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
    // Corners between two walls of the frame.
    let aligned = |i: usize, frame: f64| {
        let (prev, next) = (walls[(i + n - 1) % n], walls[i]);
        prev.0 > 0.0
            && next.0 > 0.0
            && across(prev.0, prev.1 - frame) <= ALIGNED
            && across(next.0, next.1 - frame) <= ALIGNED
    };
    let members: Vec<usize> = (0..n).filter(|&i| aligned(i, frame)).collect();
    let (s, c) = frame.sin_cos();
    let mut uv = frame_of(frame);
    // Rounding to centimetres puts a corner up to half a centimetre off in
    // x and in y, which is `h` along an axis of the frame; two corners of one
    // line are up to 2h apart. Lines further apart are two lines.
    let h = 0.005 * (frame.cos().abs() + frame.sin().abs());
    let span = SPAN_SHARE * 2.0 * h;
    for axis in 0..2 {
        let value = |i: usize| if axis == 0 { uv[i].0 } else { uv[i].1 };
        let set: Vec<(usize, f64)> = groups(&members, &value, span)
            .into_iter()
            .flat_map(|g| {
                let mean = g.iter().map(|&i| value(i)).sum::<f64>() / g.len() as f64;
                g.into_iter().map(move |i| (i, mean))
            })
            .collect();
        for (i, v) in set {
            if axis == 0 {
                uv[i].0 = v;
            } else {
                uv[i].1 = v;
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
