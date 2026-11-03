//! DACAD's dormer types (Gaubenarten) as parameter sets over the engine's
//! dormer: an open wall chain of a zero-length cheek, the front wall and a
//! second zero-length cheek, as DACAD stores it (cases C29-C31, C39, C76).
//! Port of the editor's `dormers.ts`.

use std::f64::consts::{FRAC_PI_2, TAU};

use serde::{Deserialize, Serialize};

use super::round::{ArcInput, ArcMode, arc_for};
use super::{plain_edge, round_to};
use crate::geom::Point2;
use crate::model::{Dormer, EdgeDef, HalfHip, RoofPart};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DormerKind {
    Gable,
    Pointed,
    Hip,
    HalfHip,
    Shed,
    Trapezoid,
    Round,
    /// Any other chain, kept as it is.
    General,
}

/// The kinds that can be placed.
pub const DORMER_KINDS: [DormerKind; 7] = [
    DormerKind::Gable,
    DormerKind::Pointed,
    DormerKind::Hip,
    DormerKind::HalfHip,
    DormerKind::Shed,
    DormerKind::Trapezoid,
    DormerKind::Round,
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DormerParams {
    pub kind: DormerKind,
    /// Left corner of the front, seen from outside.
    pub start: Point2,
    pub width: f64,
    /// Direction of the front wall, radians (the roof rises to its left).
    pub angle: f64,
    pub eave_height: f64,
    /// Pitch of the cheek planes (gable, hip, half-hip, trapezoid).
    pub pitch_deg: f64,
    /// Pitch of the front plane (hip, shed, trapezoid).
    pub front_pitch_deg: f64,
    pub overhang: f64,
    /// Krüppelwalm: height above the eave and pitch of the clipped front.
    pub half_hip_height: f64,
    pub half_hip_pitch_deg: f64,
    /// Rundgaube: polygon segments per cheek; 0 = DACAD's count.
    pub segments: u32,
    /// Roof material of all the dormer's walls.
    pub material: u8,
}

/// The values the editor fills in when a dormer is placed.
pub fn default_params(
    kind: DormerKind,
    start: Point2,
    angle: f64,
    eave_height: f64,
) -> DormerParams {
    DormerParams {
        kind,
        start,
        width: 2.0,
        angle,
        eave_height,
        pitch_deg: 45.0,
        front_pitch_deg: if matches!(kind, DormerKind::Shed | DormerKind::Trapezoid) {
            15.0
        } else {
            45.0
        },
        overhang: 0.0,
        half_hip_height: 0.6,
        half_hip_pitch_deg: 60.0,
        segments: 0,
        material: crate::model::DEFAULT_MATERIAL,
    }
}

const GABLE_PITCH: f64 = 90.0;

pub fn build_dormer(p: &DormerParams) -> Dormer {
    let (sin, cos) = p.angle.sin_cos();
    let end = Point2::new(
        round_to(p.start.x + cos * p.width, 3),
        round_to(p.start.y + sin * p.width, 3),
    );
    let sloped = p.kind != DormerKind::Shed;
    let front_sloped = matches!(
        p.kind,
        DormerKind::Hip | DormerKind::Shed | DormerKind::Trapezoid
    );
    let base = |start: Point2, pitch: f64| {
        let mut e = plain_edge(start, pitch, p.eave_height, p.overhang);
        e.material = p.material;
        e
    };
    // A Rundgaube is a gable dormer whose cheeks follow the Runddach arc
    // (DACAD.HLP); DACAD's default arc is assumed.
    let arc = (p.kind == DormerKind::Round)
        .then(|| {
            arc_for(&ArcInput {
                span: p.width,
                overhang: p.overhang,
                eave_height: p.eave_height,
                segments: p.segments,
                mode: ArcMode::Default,
                value: 0.0,
            })
        })
        .flatten();
    let cheek = |dir: f64, start: Point2| {
        let pitch = match &arc {
            Some(a) => a.pitch_deg,
            None if sloped => p.pitch_deg,
            None => GABLE_PITCH,
        };
        let mut e = base(start, pitch);
        e.direction_rad = Some(dir.rem_euclid(TAU));
        if let Some(a) = &arc {
            e.breaks = a.breaks.clone();
        }
        e
    };
    let front_pitch = if !front_sloped {
        GABLE_PITCH
    } else if p.kind == DormerKind::Hip {
        p.pitch_deg
    } else {
        p.front_pitch_deg
    };
    let mut front = base(p.start, front_pitch);
    if p.kind == DormerKind::HalfHip {
        front.half_hip = Some(HalfHip {
            height: round_to(p.eave_height + p.half_hip_height, 3),
            pitch_deg: p.half_hip_pitch_deg,
        });
    }
    Dormer {
        edges: vec![
            cheek(p.angle - FRAC_PI_2, p.start),
            front,
            cheek(p.angle + FRAC_PI_2, end),
        ],
        end,
        host_wall: None,
    }
}

fn is_gable(e: &EdgeDef) -> bool {
    (e.pitch_deg - 90.0).abs() <= 0.001
}

/// Reads a dormer back as parameters; any other chain is `General`.
pub fn parse_dormer(d: &Dormer) -> DormerParams {
    let f = d.edges.get(1);
    let start = f.or(d.edges.first()).map_or(d.end, |e| e.start);
    let angle = (d.end.y - start.y).atan2(d.end.x - start.x);
    let mut p = DormerParams {
        width: start.distance(d.end),
        overhang: f.map_or(0.0, |f| f.overhang),
        material: f.map_or(crate::model::DEFAULT_MATERIAL, |f| f.material),
        ..default_params(
            DormerKind::General,
            start,
            angle,
            f.or(d.edges.first()).map_or(0.0, |e| e.eave_height),
        )
    };
    let [a, f, b] = d.edges.as_slice() else {
        return p;
    };
    if a.direction_rad.is_none() || b.direction_rad.is_none() || f.direction_rad.is_some() {
        return p;
    }
    let cheeks_gable = is_gable(a) && is_gable(b);
    let cheeks_sloped = !is_gable(a) && !is_gable(b) && (a.pitch_deg - b.pitch_deg).abs() < 1e-6;
    if cheeks_gable && !is_gable(f) {
        p.kind = DormerKind::Shed;
        p.front_pitch_deg = f.pitch_deg;
        return p;
    }
    if !cheeks_sloped {
        return p;
    }
    p.pitch_deg = a.pitch_deg;
    if is_gable(f) && !a.breaks.is_empty() {
        p.kind = DormerKind::Round;
        p.segments = a.breaks.len() as u32 + 1;
    } else if is_gable(f) {
        if let Some(h) = &f.half_hip {
            p.kind = DormerKind::HalfHip;
            p.half_hip_height = round_to(h.height - f.eave_height, 3);
            p.half_hip_pitch_deg = h.pitch_deg;
        } else {
            p.kind = DormerKind::Gable;
        }
    } else if (f.pitch_deg - a.pitch_deg).abs() < 1e-6 {
        p.kind = DormerKind::Hip;
    } else {
        p.kind = DormerKind::Trapezoid;
        p.front_pitch_deg = f.pitch_deg;
    }
    p
}

/// Direction of the outline wall nearest to `p` (a dormer's front runs along it).
pub fn nearest_wall_angle(parts: &[RoofPart], p: Point2) -> f64 {
    let mut best = (f64::INFINITY, 0.0);
    for part in parts {
        let es = &part.edges;
        for (i, e) in es.iter().enumerate() {
            let b = es[(i + 1) % es.len()].start;
            let (dx, dy) = (b.x - e.start.x, b.y - e.start.y);
            let l2 = dx * dx + dy * dy;
            if l2 < 1e-12 || is_gable(e) {
                continue;
            }
            let t = (((p.x - e.start.x) * dx + (p.y - e.start.y) * dy) / l2).clamp(0.0, 1.0);
            let d = (e.start.x + t * dx - p.x).hypot(e.start.y + t * dy - p.y);
            if d < best.0 {
                best = (d, dy.atan2(dx));
            }
        }
    }
    best.1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_reads_back_as_itself() {
        for kind in DORMER_KINDS {
            let mut p = default_params(kind, Point2::new(3.0, 0.5), 0.0, 4.2);
            p.width = 2.4;
            let d = build_dormer(&p);
            assert_eq!(d.edges.len(), 3);
            let back = parse_dormer(&d);
            // A pointed dormer is a gable dormer with other numbers.
            let want = if kind == DormerKind::Pointed {
                DormerKind::Gable
            } else {
                kind
            };
            assert_eq!(back.kind, want, "{kind:?}");
            assert!((back.width - 2.4).abs() < 1e-9);
        }
    }

    #[test]
    fn round_dormer_cheeks_carry_the_arc() {
        let mut p = default_params(DormerKind::Round, Point2::new(0.0, 0.0), 0.0, 4.0);
        p.width = 2.0;
        let d = build_dormer(&p);
        assert!(!d.edges[0].breaks.is_empty());
        assert_eq!(d.edges[0].breaks, d.edges[2].breaks);
        assert!(is_gable(&d.edges[1]));
    }
}
