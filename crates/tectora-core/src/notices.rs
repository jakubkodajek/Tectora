//! Input notices: values a roofer typed that are allowed but unlikely (a
//! lost decimal comma, "5" for 0,5 m of overhang), and half-hips or pitch
//! breaks at or above their roof's ridge, which the calculation leaves
//! without effect. The editor marks the field and lists them in the Results
//! panel (reviews UX-009, UX-003); the MCP server gives the same list
//! (`calc`, `get_project`: `input_notices`), so an AI client is told what a
//! user is told. Also the defaults the wall bar gives a new half-hip and a
//! new pitch break.
//!
//! The editor's `app/src/lib/plausible.ts` computes the same for live
//! typing; `tests/edit_parity.rs` checks both give the same answers.
//! [no source: thresholds are a few times what TERRAN's roofs have.]

use serde::Serialize;

use crate::model::{EdgeDef, Project};
use crate::roof::RoofGeometry;

/// Above this an overhang is unusual, m (overhangs stay well under 1,5 m).
pub const UNUSUAL_OVERHANG: f64 = 3.0;
/// Above this an eave or building height is unusual, m.
pub const UNUSUAL_HEIGHT: f64 = 30.0;
/// Above this a wall length is unusual, m.
pub const UNUSUAL_LENGTH: f64 = 200.0;
/// Tolerance for "at the ridge", m (1 cm).
pub const AT_RIDGE: f64 = 0.01;

/// One notice (see the module).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InputNotice {
    /// `unusual_overhang`, `unusual_height` (an eave height),
    /// `unusual_length`, `unusual_building_height`, `half_hip_above_ridge`
    /// or `break_above_ridge`.
    pub kind: &'static str,
    pub part: usize,
    /// The wall; absent for a building height.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge: Option<usize>,
    /// The pitch break's index (`break_above_ridge`).
    #[serde(rename = "break", skip_serializing_if = "Option::is_none")]
    pub brk: Option<usize>,
    /// The value typed (`unusual_*`) or the ridge height (`*_above_ridge`).
    pub value: f64,
}

/// As the editor's `isGable`: a wall entered as a gable (90°).
fn gable(e: &EdgeDef) -> bool {
    (e.pitch_deg - 90.0).abs() <= 0.001
}

fn marker(e: &EdgeDef) -> bool {
    e.direction_rad.is_some()
}

fn unusual(v: f64, limit: f64) -> bool {
    v.is_finite() && v > limit
}

/// Every wall value that is unusually large, in wall order: overhang, eave
/// height, wall length (as the editor's `unusualValues`).
pub fn unusual_values(project: &Project) -> Vec<InputNotice> {
    let mut out = Vec::new();
    for (pi, p) in project.parts.iter().enumerate() {
        let n = p.edges.len();
        for (ei, e) in p.edges.iter().enumerate() {
            if marker(e) {
                continue;
            }
            let at = |kind, value| InputNotice {
                kind,
                part: pi,
                edge: Some(ei),
                brk: None,
                value,
            };
            if unusual(e.overhang, UNUSUAL_OVERHANG) {
                out.push(at("unusual_overhang", e.overhang));
            }
            if unusual(e.eave_height, UNUSUAL_HEIGHT) {
                out.push(at("unusual_height", e.eave_height));
            }
            let b = p.edges[(ei + 1) % n].start;
            let len = e.start.distance(b);
            if unusual(len, UNUSUAL_LENGTH) {
                out.push(at("unusual_length", len));
            }
        }
    }
    out
}

/// The highest point of each part's calculated roof, `None` without one
/// (as the editor's `ridgeOf`).
pub fn ridges(geos: &[RoofGeometry]) -> Vec<Option<f64>> {
    geos.iter()
        .map(|g| {
            if g.error.is_some() || g.faces.is_empty() {
                return None;
            }
            let h = crate::quantities::part_quantities(g).ridge_height;
            (h.is_finite() && h > 0.0).then_some(h)
        })
        .collect()
}

fn above_ridge(height: f64, ridge: f64) -> bool {
    height.is_finite() && height >= ridge - AT_RIDGE
}

/// Half-hips and pitch breaks at or above their roof's ridge, in wall
/// order, given each part's ridge ([`ridges`]).
pub fn above_ridge_issues(project: &Project, ridges: &[Option<f64>]) -> Vec<InputNotice> {
    let mut out = Vec::new();
    for (pi, p) in project.parts.iter().enumerate() {
        let Some(ridge) = ridges.get(pi).copied().flatten() else {
            continue;
        };
        for (ei, e) in p.edges.iter().enumerate() {
            if marker(e) {
                continue;
            }
            let g = gable(e);
            if g && e
                .half_hip
                .as_ref()
                .is_some_and(|h| above_ridge(h.height, ridge))
            {
                out.push(InputNotice {
                    kind: "half_hip_above_ridge",
                    part: pi,
                    edge: Some(ei),
                    brk: None,
                    value: ridge,
                });
            }
            if !g || e.half_hip.is_some() {
                for (bi, b) in e.breaks.iter().enumerate() {
                    if above_ridge(b.height, ridge) {
                        out.push(InputNotice {
                            kind: "break_above_ridge",
                            part: pi,
                            edge: Some(ei),
                            brk: Some(bi),
                            value: ridge,
                        });
                    }
                }
            }
        }
    }
    out
}

/// All notices of a calculated project: the unusual wall values, an
/// unusual building height (the wall bar's mark) and the half-hips and
/// breaks above the ridge.
pub fn input_notices(project: &Project, geos: &[RoofGeometry]) -> Vec<InputNotice> {
    let mut out = unusual_values(project);
    for (pi, p) in project.parts.iter().enumerate() {
        if let Some(h) = p.height.filter(|&h| unusual(h, UNUSUAL_HEIGHT)) {
            out.push(InputNotice {
                kind: "unusual_building_height",
                part: pi,
                edge: None,
                brk: None,
                value: h,
            });
        }
    }
    out.extend(above_ridge_issues(project, &ridges(geos)));
    out
}

fn round2(v: f64) -> f64 {
    crate::edit::round_to(v, 2)
}

/// The height a new half-hip starts at: halfway between the wall's eave
/// and the ridge, so ticking it changes the roof; without a ridge 2,5 m
/// above the eave (the wall bar's default).
pub fn half_hip_default(eave_height: f64, ridge: Option<f64>) -> f64 {
    match ridge {
        Some(r) if r > eave_height + 0.1 => round2((eave_height + r) / 2.0),
        _ => round2(eave_height + 2.5),
    }
}

/// Pitch of a new half-hip, degrees (the wall bar's default).
pub const HALF_HIP_DEFAULT_PITCH: f64 = 50.0;

/// The height a new pitch break starts at: 1,5 m above `top` (the highest
/// break so far, else the eave), but below the ridge when that would lie at
/// or above it.
pub fn break_default(top: f64, ridge: Option<f64>) -> f64 {
    let h = top + 1.5;
    match ridge {
        Some(r) if r > top + 0.1 && h >= r - AT_RIDGE => round2((top + r) / 2.0),
        _ => round2(h),
    }
}

/// Pitch of a new pitch break: the pitch below it less 20°, kept within
/// 5..85° (the wall bar's default).
pub fn break_default_pitch(below: f64) -> f64 {
    (below - 20.0).clamp(5.0, 85.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gable_job() -> Project {
        crate::examples::find("gable").unwrap().project()
    }

    #[test]
    fn a_lost_decimal_comma_in_an_overhang_is_a_notice() {
        let mut p = gable_job();
        p.parts[0].edges[0].overhang = 5.0;
        let n = unusual_values(&p);
        assert_eq!(n.len(), 1);
        assert_eq!(
            (n[0].kind, n[0].edge, n[0].value),
            ("unusual_overhang", Some(0), 5.0)
        );
        p.parts[0].edges[0].overhang = 3.0;
        assert!(
            unusual_values(&p).is_empty(),
            "3 m is the limit, not above it"
        );
    }

    #[test]
    fn a_break_above_the_ridge_is_a_notice_and_one_below_is_not() {
        let mut p = gable_job();
        let geos = crate::openings::generate_project(&p);
        let ridge = ridges(&geos)[0].expect("ridge");
        p.parts[0].edges[0].breaks = vec![crate::model::PitchBreak {
            height: ridge + 0.5,
            pitch_deg: 20.0,
        }];
        let n = above_ridge_issues(&p, &[Some(ridge)]);
        assert_eq!(n.len(), 1);
        assert_eq!((n[0].kind, n[0].brk), ("break_above_ridge", Some(0)));
        p.parts[0].edges[0].breaks[0].height = ridge - 1.0;
        assert!(above_ridge_issues(&p, &[Some(ridge)]).is_empty());
    }

    #[test]
    fn an_unusual_building_height_is_a_notice_of_the_part() {
        let mut p = gable_job();
        p.parts[0].height = Some(45.0);
        let geos = crate::openings::generate_project(&p);
        let n = input_notices(&p, &geos);
        assert!(
            n.iter()
                .any(|x| x.kind == "unusual_building_height" && x.edge.is_none()),
            "{n:?}"
        );
    }

    #[test]
    fn the_defaults_stay_below_the_ridge() {
        assert_eq!(half_hip_default(3.0, Some(7.0)), 5.0);
        assert_eq!(half_hip_default(3.0, None), 5.5);
        assert_eq!(break_default(3.0, Some(10.0)), 4.5);
        assert_eq!(break_default(3.0, Some(4.0)), 3.5);
        assert_eq!(break_default_pitch(40.0), 20.0);
        assert_eq!(break_default_pitch(20.0), 5.0);
    }
}
