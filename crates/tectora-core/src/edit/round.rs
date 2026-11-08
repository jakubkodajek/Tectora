//! DACAD's Runddach helper (Parameter › Neigungen berechnen › Runddach): on
//! two parallel walls with the same eave height it sets pitches and Knicke
//! so that an inscribed polygon approximates a circular arc ending at the
//! overhang tips (DACAD.HLP topic "Runddach"). The numbers follow DACAD runs
//! RD1-RD4: the default arc spans 45° on each side, a side gets
//! ceil(arc / 11.25°) segments (15° steps when such a segment's chord would
//! be under 0.4 m, Rundgaube runs RGa-RGs), and segment k has the pitch
//! (k + ½)·arc / n. Port of the editor's `roundroof.ts`.

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
