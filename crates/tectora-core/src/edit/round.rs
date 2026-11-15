//! DACAD's Runddach helper (Parameter › Neigungen berechnen › Runddach): on
//! two parallel walls with the same eave height it sets pitches and Knicke
//! so that an inscribed polygon approximates a circular arc ending at the
//! overhang tips (DACAD.HLP topic "Runddach"). The numbers follow DACAD runs
//! RD1-RD4: the default arc spans 45° on each side, a side gets
//! ceil(arc / 11.25°) segments (15° steps when such a segment's chord would
//! be under 0.4 m, Rundgaube runs RGa-RGs), and segment k has the pitch
//! (k + ½)·arc / n. The round-roof dialog calls it through `commands::round_arc`;
//! `roundroof.ts` keeps a copy only for the round dormer.

// `!(x > 0.0)` and the like are deliberate: they also refuse NaN.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use std::f64::consts::PI;

use serde::{Deserialize, Serialize};

use super::{is_marker, round_to};
use crate::geom::Point2;
use crate::model::{EdgeDef, PitchBreak};

/// How the arc is given.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArcMode {
    /// DACAD's default arc, 45° on each side.
    #[default]
    Default,
    Radius,
    /// Absolute ridge height.
    Ridge,
    /// Developed length of the whole arc, eave tip to eave tip.
    Length,
}

/// DACAD's default arc on each side of the ridge, degrees.
const DEFAULT_ARC: f64 = 45.0;
/// DACAD's segment angle, degrees, and the coarser one for short segments.
const SEGMENT_ARC: f64 = 11.25;
const SHORT_SEGMENT_ARC: f64 = 15.0;
/// Below this chord, m, DACAD takes the coarser steps.
const MIN_CHORD: f64 = 0.4;

/// Most segments per side a round roof or Rundgaube may have (the editor's
/// field allows 30). A larger count from a file or an MCP client would
/// build that many pitch breaks.
pub const MAX_ROUND_SEGMENTS: u32 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ArcInput {
    /// Distance between the two wall lines, m.
    pub span: f64,
    pub overhang: f64,
    pub eave_height: f64,
    /// Segments on each side of the ridge; 0 = DACAD's count.
    pub segments: u32,
    pub mode: ArcMode,
    /// The radius, ridge height or developed length, by mode.
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Arc {
    pub radius: f64,
    pub ridge: f64,
    pub length: f64,
    pub segments: u32,
    pub pitch_deg: f64,
    pub breaks: Vec<PitchBreak>,
}

/// Half chord of the arc (half the span plus the overhang); the smallest radius.
pub fn min_radius(span: f64, overhang: f64) -> f64 {
    span / 2.0 + overhang
}

pub fn default_radius(span: f64, overhang: f64) -> f64 {
    min_radius(span, overhang) / DEFAULT_ARC.to_radians().sin()
}

/// DACAD's segment count for an arc of `deg` degrees on one side.
pub fn segments_for(deg: f64, radius: f64) -> u32 {
    let steps = |arc: f64| ((deg / arc - 1e-9).ceil() as i64).max(1) as u32;
    let n = steps(SEGMENT_ARC);
    let chord = 2.0 * radius * ((deg / n as f64).to_radians() / 2.0).sin();
    if chord < MIN_CHORD {
        steps(SHORT_SEGMENT_ARC)
    } else {
        n
    }
}

/// The arc for the input, or `None` when the value is out of range.
pub fn arc_for(a: &ArcInput) -> Option<Arc> {
    if a.segments > MAX_ROUND_SEGMENTS {
        return None;
    }
    let c = min_radius(a.span, a.overhang);
    if !(c > 0.0) {
        return None;
    }
    // A value that is not a number, or a radius beyond what a job may hold,
    // gives a ridge of -inf: out of range like any other (BUG-1).
    let bounded = |v: f64| v.is_finite() && v.abs() <= crate::model::MAX_COORD;
    if !bounded(a.eave_height) || (a.mode != ArcMode::Default && !bounded(a.value)) {
        return None;
    }
    let mut r = default_radius(a.span, a.overhang);
    match a.mode {
        ArcMode::Default => {}
        ArcMode::Radius => {
            if a.value < c - 1e-9 {
                return None;
            }
            r = a.value;
        }
        ArcMode::Ridge => {
            // Rise above the eave tips h: r = (c² + h²) / 2h.
            let rise = a.value - a.eave_height;
            if !(rise > 0.0) || rise > c + 1e-9 {
                return None;
            }
            r = (rise * rise + c * c) / (2.0 * rise);
        }
        ArcMode::Length => {
            // Developed length 2·r·asin(c/r) falls from π·c towards 2·c as r grows.
            if a.value > PI * c + 1e-9 || a.value <= 2.0 * c {
                return None;
            }
            let (mut lo, mut hi) = (c, c * 1e4);
            for _ in 0..200 {
                let mid = (lo + hi) / 2.0;
                if 2.0 * mid * (c / mid).asin() > a.value {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            r = (lo + hi) / 2.0;
        }
    }
    let half = (c / r).min(1.0).asin();
    let n = if a.segments > 0 {
        a.segments
    } else {
        segments_for(half.to_degrees(), r)
    };
    let centre = a.eave_height - (r * r - c * c).max(0.0).sqrt();
    let at = |k: u32| half * (1.0 - k as f64 / n as f64);
    let pitch = |k: u32| round_to(((at(k) + at(k + 1)) / 2.0).to_degrees(), 4);
    let breaks = (1..n)
        .map(|k| PitchBreak {
            height: round_to(centre + r * at(k).cos(), 4),
            pitch_deg: pitch(k),
        })
        .collect();
    Some(Arc {
        radius: r,
        ridge: centre + r,
        length: 2.0 * r * half,
        segments: n,
        pitch_deg: pitch(0),
        breaks,
    })
}

fn sub(p: Point2, q: Point2) -> Point2 {
    Point2::new(p.x - q.x, p.y - q.y)
}
fn dot(p: Point2, q: Point2) -> f64 {
    p.x * q.x + p.y * q.y
}

/// Index of the wall facing edge `i` across the part (anti-parallel,
/// overlapping), or `None`.
pub fn opposite_wall(edges: &[EdgeDef], i: usize) -> Option<usize> {
    let n = edges.len();
    let seg = |k: usize| (edges[k].start, edges[(k + 1) % n].start);
    let (a, b) = seg(i);
    let len = a.distance(b);
    if len < 1e-6 {
        return None;
    }
    let u = Point2::new((b.x - a.x) / len, (b.y - a.y) / len);
    let inward = Point2::new(-u.y, u.x);
    let mut best: Option<(usize, f64)> = None;
    for (k, e) in edges.iter().enumerate() {
        if k == i || is_marker(e) {
            continue;
        }
        let (sa, sb) = seg(k);
        let l = sa.distance(sb);
        if l < 1e-6 {
            continue;
        }
        let v = Point2::new((sb.x - sa.x) / l, (sb.y - sa.y) / l);
        if dot(u, v) > -1.0 + 1e-6 {
            continue;
        }
        let d = dot(sub(sa, a), inward);
        if d <= 1e-6 {
            continue;
        }
        let (t0, t1) = (dot(sub(sa, a), u), dot(sub(sb, a), u));
        if t0.max(t1) <= 1e-6 || t0.min(t1) >= len - 1e-6 {
            continue;
        }
        if best.is_none_or(|(_, bd)| d < bd) {
            best = Some((k, d));
        }
    }
    best.map(|b| b.0)
}

/// Distance between edge `i`'s line and edge `j`'s start, across the part.
pub fn span_between(edges: &[EdgeDef], i: usize, j: usize) -> f64 {
    let a = edges[i].start;
    let b = edges[(i + 1) % edges.len()].start;
    let l = a.distance(b);
    dot(
        sub(edges[j].start, a),
        Point2::new(-(b.y - a.y) / l, (b.x - a.x) / l),
    )
    .abs()
}

/// Closed-form checks of the round-roof helper (TEST-006): the pitches and
/// breaks it writes describe the polygon inscribed in a circle through both
/// eave tips. DACAD's own values (runs RD1-RD4, RGa-RGs) are checked by
/// `tests/round_cases.rs` when the private captures are at hand.
#[cfg(test)]
mod tests {
    use super::*;

    fn input(span: f64, overhang: f64, mode: ArcMode, value: f64) -> ArcInput {
        ArcInput {
            span,
            overhang,
            eave_height: 3.0,
            segments: 0,
            mode,
            value,
        }
    }

    /// The polygon the arc gives on one side, from the eave tip up to the
    /// ridge, as (horizontal distance from the arc's axis, height): walk
    /// each segment at its pitch up to the next break height (the last one
    /// up to the ridge).
    fn walk(a: &ArcInput, arc: &Arc) -> Vec<(f64, f64)> {
        let c = min_radius(a.span, a.overhang);
        let mut pts = vec![(c, a.eave_height)];
        let pitches: Vec<f64> = std::iter::once(arc.pitch_deg)
            .chain(arc.breaks.iter().map(|b| b.pitch_deg))
            .collect();
        let tops: Vec<f64> = arc
            .breaks
            .iter()
            .map(|b| b.height)
            .chain(std::iter::once(arc.ridge))
            .collect();
        for (p, top) in pitches.iter().zip(tops) {
            let (x, z) = *pts.last().unwrap();
            let rise = top - z;
            pts.push((x - rise / p.to_radians().tan(), top));
        }
        pts
    }

    /// Every vertex of the walked polygon lies on the circle (break heights
    /// and pitches are rounded to 4 decimals, so within a millimetre), and
    /// the last one is the ridge over the axis.
    fn assert_inscribed(a: &ArcInput, arc: &Arc) {
        let centre = arc.ridge - arc.radius;
        let pts = walk(a, arc);
        assert_eq!(pts.len() as u32, arc.segments + 1);
        for (i, &(x, z)) in pts.iter().enumerate() {
            let d = x.hypot(z - centre);
            assert!(
                (d - arc.radius).abs() < 1e-3,
                "vertex {i} ({x:.4}, {z:.4}) is {d:.5} from the centre, radius {:.5}",
                arc.radius
            );
        }
        let (x, _) = *pts.last().unwrap();
        assert!(x.abs() < 1e-3, "the last segment ends {x} m off the ridge");
        // Equal segments: the same chord everywhere.
        let chords: Vec<f64> = pts
            .windows(2)
            .map(|w| (w[0].0 - w[1].0).hypot(w[0].1 - w[1].1))
            .collect();
        for c in &chords {
            assert!((c - chords[0]).abs() < 2e-3, "{chords:?}");
        }
        // Pitches fall from the eave to the ridge.
        let mut last = 90.0;
        for p in std::iter::once(arc.pitch_deg).chain(arc.breaks.iter().map(|b| b.pitch_deg)) {
            assert!(p < last && p > 0.0, "pitch {p} after {last}");
            last = p;
        }
    }

    #[test]
    fn the_default_arc_spans_45_degrees_on_each_side() {
        for (span, ov) in [(4.0, 0.5), (10.0, 0.5), (2.0, 0.0), (8.0, 0.0)] {
            let a = input(span, ov, ArcMode::Default, 0.0);
            let arc = arc_for(&a).unwrap();
            let c = span / 2.0 + ov;
            assert!((arc.radius - c * 2f64.sqrt()).abs() < 1e-9, "{span}");
            // Rise of a 45° arc over its half chord: r (1 - cos 45°).
            let rise = arc.radius * (1.0 - std::f64::consts::FRAC_1_SQRT_2);
            assert!((arc.ridge - 3.0 - rise).abs() < 1e-9, "{span}");
            assert!((arc.length - arc.radius * PI / 2.0).abs() < 1e-9);
            assert_inscribed(&a, &arc);
        }
    }

    #[test]
    fn segment_k_has_the_pitch_of_its_middle() {
        // 45° in 4 segments of 11.25°: 39.375, 28.125, 16.875, 5.625.
        let arc = arc_for(&input(4.0, 0.5, ArcMode::Default, 0.0)).unwrap();
        assert_eq!(arc.segments, 4);
        assert_eq!(arc.pitch_deg, 39.375);
        let rest: Vec<f64> = arc.breaks.iter().map(|b| b.pitch_deg).collect();
        assert_eq!(rest, [28.125, 16.875, 5.625]);
        // Break k at centre + r cos(45° (1 - k/4)).
        let centre = arc.ridge - arc.radius;
        for (k, b) in arc.breaks.iter().enumerate() {
            let at = (45.0 * (1.0 - (k + 1) as f64 / 4.0)).to_radians();
            assert!(
                (b.height - (centre + arc.radius * at.cos())).abs() < 1e-4,
                "break {k}"
            );
        }
    }

    #[test]
    fn short_chords_take_15_degree_steps() {
        // r = 1 / sin 45° = 1.414: an 11.25° chord is 0.277 m < 0.4 m.
        assert_eq!(segments_for(45.0, 2f64.sqrt()), 3);
        // r = 2.5 / sin 45°: 0.693 m.
        assert_eq!(segments_for(45.0, 2.5 * 2f64.sqrt()), 4);
        // The rule's edge: the 11.25° chord just at 0.4 m keeps 11.25°.
        let r_edge = MIN_CHORD / (2.0 * (SEGMENT_ARC / 2.0).to_radians().sin());
        assert_eq!(segments_for(45.0, r_edge * 1.0001), 4);
        assert_eq!(segments_for(45.0, r_edge * 0.9999), 3);
        // An arc of a little more than a step needs one more segment.
        assert_eq!(segments_for(11.25, 10.0), 1);
        assert_eq!(segments_for(11.26, 10.0), 2);
        let small = arc_for(&input(2.0, 0.0, ArcMode::Default, 0.0)).unwrap();
        assert_eq!(small.segments, 3);
        assert_eq!(small.pitch_deg, 37.5);
    }

    #[test]
    fn radius_ridge_and_length_give_the_arc_they_name() {
        let c = 2.5;
        let a = input(4.0, 0.5, ArcMode::Radius, 4.0);
        let arc = arc_for(&a).unwrap();
        assert_eq!(arc.radius, 4.0);
        let half = (c / 4.0f64).asin();
        assert!((arc.ridge - (3.0 + 4.0 - (16.0 - c * c).sqrt())).abs() < 1e-9);
        assert!((arc.length - 8.0 * half).abs() < 1e-9);
        assert_inscribed(&a, &arc);

        let a = input(4.0, 0.5, ArcMode::Ridge, 4.5);
        let arc = arc_for(&a).unwrap();
        assert!((arc.ridge - 4.5).abs() < 1e-9);
        // r = (c² + h²) / 2h with h = 1.5.
        assert!((arc.radius - (c * c + 2.25) / 3.0).abs() < 1e-9);
        assert_inscribed(&a, &arc);

        let want = 6.0;
        let a = input(4.0, 0.5, ArcMode::Length, want);
        let arc = arc_for(&a).unwrap();
        assert!((arc.length - want).abs() < 1e-6, "{}", arc.length);
        assert_inscribed(&a, &arc);

        // The half circle: radius = half chord, ridge = eave + c.
        let a = input(4.0, 0.5, ArcMode::Radius, c);
        let arc = arc_for(&a).unwrap();
        assert!((arc.ridge - (3.0 + c)).abs() < 1e-9);
        assert_eq!(arc.segments, 8);
        assert_inscribed(&a, &arc);
    }

    #[test]
    fn values_out_of_range_give_no_arc() {
        let c = 2.5;
        for (mode, value) in [
            (ArcMode::Radius, c - 0.01),
            (ArcMode::Ridge, 3.0),
            (ArcMode::Ridge, 3.0 + c + 0.01),
            (ArcMode::Length, 2.0 * c),
            (ArcMode::Length, PI * c + 0.01),
        ] {
            assert!(
                arc_for(&input(4.0, 0.5, mode, value)).is_none(),
                "{mode:?} {value}"
            );
        }
        assert!(arc_for(&input(0.0, 0.0, ArcMode::Default, 0.0)).is_none());
        assert!(arc_for(&input(f64::NAN, 0.5, ArcMode::Default, 0.0)).is_none());
        let mut a = input(4.0, 0.5, ArcMode::Default, 0.0);
        a.segments = MAX_ROUND_SEGMENTS + 1;
        assert!(arc_for(&a).is_none());
        a.segments = MAX_ROUND_SEGMENTS;
        let arc = arc_for(&a).unwrap();
        assert_eq!(arc.segments, MAX_ROUND_SEGMENTS);
        assert_eq!(arc.breaks.len() as u32, MAX_ROUND_SEGMENTS - 1);
    }

    #[test]
    fn every_number_of_an_arc_is_finite() {
        // MATH-001 lesson: check closed forms with is_finite.
        for span in [0.01, 0.4, 4.0, 40.0, 400.0] {
            for ov in [0.0, 0.5, 2.0] {
                for segments in [0, 1, 7, MAX_ROUND_SEGMENTS] {
                    let mut a = input(span, ov, ArcMode::Default, 0.0);
                    a.segments = segments;
                    let arc = arc_for(&a).unwrap();
                    let all = [arc.radius, arc.ridge, arc.length, arc.pitch_deg]
                        .into_iter()
                        .chain(arc.breaks.iter().flat_map(|b| [b.height, b.pitch_deg]));
                    for v in all {
                        assert!(v.is_finite(), "{span} {ov} {segments}: {arc:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn the_facing_wall_and_the_span_of_a_rectangle() {
        let pt = Point2::new;
        let e = |x: f64, y: f64| super::super::plain_edge(pt(x, y), 40.0, 3.0, 0.5);
        let es = [e(0.0, 0.0), e(6.0, 0.0), e(6.0, 4.0), e(0.0, 4.0)];
        assert_eq!(opposite_wall(&es, 0), Some(2));
        assert_eq!(opposite_wall(&es, 2), Some(0));
        assert_eq!(opposite_wall(&es, 1), Some(3));
        assert!((span_between(&es, 0, 2) - 4.0).abs() < 1e-12);
        assert!((span_between(&es, 1, 3) - 6.0).abs() < 1e-12);
    }

    /// MATH-001 lesson: a closed form returns finite numbers or nothing.
    /// A radius of 1e200 m (an MCP `roof_helper` round_roof call with mode
    /// radius) gives `ridge: -inf`, and a NaN value an arc of NaNs.
    #[test]
    fn an_arc_is_none_or_finite_for_any_value() {
        for (mode, value) in [
            (ArcMode::Radius, 1e200),
            (ArcMode::Radius, 1e308),
            (ArcMode::Radius, f64::NAN),
            (ArcMode::Ridge, f64::NAN),
            (ArcMode::Length, f64::NAN),
        ] {
            if let Some(arc) = arc_for(&input(4.0, 0.5, mode, value)) {
                let all: Vec<f64> = [arc.radius, arc.ridge, arc.length, arc.pitch_deg]
                    .into_iter()
                    .chain(arc.breaks.iter().flat_map(|b| [b.height, b.pitch_deg]))
                    .collect();
                assert!(
                    all.iter().all(|v| v.is_finite()),
                    "{mode:?} {value}: {arc:?}"
                );
            }
        }
    }
}
