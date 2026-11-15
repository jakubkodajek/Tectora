//! The calculation helpers of the roof definition as one operation: a
//! request (`tool` and its values) in, new values for some walls out. The
//! editor's calculation dialog (Tauri commands `roof_helper*`, through
//! `commands.rs`) and the MCP tool `roof_helper` both call [`run`], so a
//! colleague and an AI client get the same roof from the same values
//! (review ARCH-004: the editor used to carry a TypeScript copy of
//! `calc.rs`, `round.rs` and the glue below).

use serde::{Deserialize, Serialize};

use super::calc::{self as hc, BuildUpEave, ParallelField, ParallelState, Target, Triangle};
use super::probe::roof_corner_points_kinded;
use super::round::{Arc, ArcInput, ArcMode, arc_for, opposite_wall, span_between};
use super::{EdgePatch, Patches, apply_patches, combine};
use crate::geom::Point2;
use crate::model::{EdgeDef, Project};
use crate::roof::LineKind;

/// The neighbour a hip or valley is measured against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Neighbour {
    #[default]
    Next,
    Prev,
}

/// What the two lengths of a hip or valley are measured along.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HipBy {
    #[default]
    Rafters,
    Eaves,
}

/// One value typed into the 2 Parallele dialog.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ParallelStep {
    pub field: ParallelField,
    pub value: f64,
}

fn yes() -> bool {
    true
}

/// A helper and its values; the wall it works on is given beside it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "tool", rename_all = "snake_case")]
pub enum HelperRequest {
    /// Pitch from two of width, rafter and ridge height.
    Triangle {
        #[serde(default)]
        width: Option<f64>,
        #[serde(default)]
        rafter: Option<f64>,
        #[serde(default)]
        ridge: Option<f64>,
        #[serde(default)]
        width_includes_overhang: bool,
    },
    /// The typed values of the 2 Parallele dialog, applied in order.
    Parallel {
        target: Target,
        #[serde(default)]
        steps: Vec<ParallelStep>,
        #[serde(default = "yes")]
        width_includes_overhang: bool,
    },
    /// The plane must reach a point in space.
    Point { target: Target, point: [f64; 3] },
    /// Parallel walls take this wall's plane; `walls` default to all of them.
    Merge {
        target: Target,
        #[serde(default)]
        walls: Option<Vec<usize>>,
    },
    /// Pitches from a hip or valley length and two rafters or eave pieces.
    Hip {
        #[serde(default)]
        neighbour: Neighbour,
        #[serde(default)]
        by: HipBy,
        hip: f64,
        first: f64,
        second: f64,
    },
    BuildUp {
        thickness: f64,
        #[serde(default)]
        eave: BuildUpEave,
    },
    /// An arc over this wall and the one facing it, from the wall's own eave
    /// height and overhang unless the dialog gives others.
    RoundRoof {
        #[serde(default)]
        eave_height: Option<f64>,
        #[serde(default)]
        overhang: Option<f64>,
        #[serde(default)]
        segments: u32,
        #[serde(default)]
        mode: ArcMode,
        #[serde(default)]
        value: f64,
    },
}

/// What a helper worked out.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct HelperOut {
    /// New values by wall, as the helper gives them (also those that equal
    /// the wall's value now). Empty when `refused` is set.
    #[serde(skip_serializing_if = "Patches::is_empty")]
    pub patches: Patches,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub triangle: Option<Triangle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opposite_edge: Option<usize>,
    /// The 2 Parallele dialog after the steps.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<ParallelState>,
    /// Rafter lengths of side A and B of `state`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rafters: Option<[f64; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arc: Option<Arc>,
    /// Why the values were not taken although they were worked out (an eave
    /// below the eave of a neighbouring gable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refused: Option<String>,
}

/// The message for an eave below a neighbouring gable's.
pub const EAVE_BELOW_GABLE: &str = "the eave would end up below the eave of a neighbouring gable";

/// What the dialog needs to know about the wall before anything is typed.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HelperContext {
    /// The wall facing this one across the part.
    pub opposite: Option<usize>,
    /// Distance to it, m (0 without one).
    pub span: f64,
    /// Where the two planes meet: plan width from this wall, and height.
    pub current: Option<Meeting>,
    /// One of the two walls has pitch breaks (2 Parallele is not available).
    pub breaks: bool,
    /// Other walls parallel to this one and facing the same way.
    pub mates: Vec<usize>,
    /// End points of the roof lines other than eaves, highest first; only
    /// when asked for (it calculates the roof).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roof_points: Option<Vec<RoofPoint>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Meeting {
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct RoofPoint {
    pub p: [f64; 3],
    pub kind: LineKind,
}

/// [`HelperContext`] of wall `edge`; `roof_points` are those of the
/// calculated roof of `project` (asked for by the point-in-space tool).
pub fn context(
    project: &Project,
    part: usize,
    edge: usize,
    with_points: bool,
) -> Result<HelperContext, String> {
    let es = wall_list(project, part, edge)?;
    let opposite = opposite_wall(es, edge);
    let span = opposite.map_or(0.0, |o| span_between(es, edge, o));
    let current =
        opposite.and_then(|o| hc::meeting(&es[edge], &es[o], span).map(|(w, h)| Meeting { w, h }));
    let breaks = opposite.is_some_and(|o| !es[edge].breaks.is_empty() || !es[o].breaks.is_empty());
    let roof_points = with_points.then(|| {
        let geos = crate::openings::generate_project(project);
        roof_corner_points_kinded(&geos)
            .into_iter()
            .map(|(p, kind)| RoofPoint { p, kind })
            .collect()
    });
    Ok(HelperContext {
        opposite,
        span,
        current,
        breaks,
        mates: hc::same_way(es, edge)
            .into_iter()
            .filter(|&k| k != edge)
            .collect(),
        roof_points,
    })
}

fn wall_list(project: &Project, part: usize, edge: usize) -> Result<&[EdgeDef], String> {
    let es = &project
        .parts
        .get(part)
        .ok_or_else(|| {
            format!(
                "part {part} does not exist (the design has {} parts)",
                project.parts.len()
            )
        })?
        .edges;
    if edge >= es.len() {
        return Err(format!(
            "edge {edge} does not exist (the part has {} edges)",
            es.len()
        ));
    }
    Ok(es)
}

/// Works the request out for wall `edge` of part `part`, without changing
/// anything. Values that give no solution are an `Err` with the reason; a
/// parallel result the roof definition refuses (an eave below a neighbouring
/// gable) comes back with `refused` set instead, so the dialog can still show
/// the state it reached.
pub fn preview(
    project: &Project,
    part: usize,
    edge: usize,
    req: &HelperRequest,
) -> Result<HelperOut, String> {
    let es = wall_list(project, part, edge)?;
    let e = &es[edge];
    let single = |patch: EdgePatch| Patches::from([(edge, patch)]);
    let mut out = HelperOut::default();
    let patches: Patches = match req {
        HelperRequest::Triangle {
            width,
            rafter,
            ridge,
            width_includes_overhang,
        } => {
            let t = hc::triangle(e, *width, *rafter, *ridge, *width_includes_overhang)
                .ok_or("give two of width, rafter and ridge that make a triangle")?;
            out.triangle = Some(t);
            single(hc::triangle_patch(e, t.pitch_deg))
        }
        HelperRequest::Parallel {
            target,
            steps,
            width_includes_overhang,
        } => {
            let opp = opposite_wall(es, edge).ok_or("this wall has no facing wall")?;
            if !e.breaks.is_empty() || !es[opp].breaks.is_empty() {
                return Err("not available on walls with pitch breaks".into());
            }
            let span = span_between(es, edge, opp);
            let (w, h) = hc::meeting(e, &es[opp], span).ok_or("the two planes do not meet")?;
            let mut st = hc::parallel_start(e, &es[opp], span, w, h);
            for (k, s) in steps.iter().enumerate() {
                st = hc::parallel_edit(&st, s.field, s.value, *target, *width_includes_overhang)
                    .ok_or_else(|| format!("step {k}: no solution for this value"))?;
            }
            out.opposite_edge = Some(opp);
            out.rafters = Some([
                hc::parallel_rafter(&st, true),
                hc::parallel_rafter(&st, false),
            ]);
            let patches = if *target == Target::Position {
                combine([
                    &hc::move_wall(es, edge, st.shift_a),
                    &hc::move_wall(es, opp, st.shift_b),
                ])
            } else {
                let (pa, pb) = hc::parallel_patches(&st, *target);
                if *target == Target::Eave
                    && (hc::eave_below_gable(es, edge, pa.eave_height.unwrap_or(0.0))
                        || hc::eave_below_gable(es, opp, pb.eave_height.unwrap_or(0.0)))
                {
                    out.refused = Some(EAVE_BELOW_GABLE.into());
                    Patches::new()
                } else {
                    Patches::from([(edge, pa), (opp, pb)])
                }
            };
            out.state = Some(st);
            patches
        }
        HelperRequest::Point { target, point: q } => {
            if !q.iter().all(|v| v.is_finite()) {
                return Err("the point must be finite".into());
            }
            let d = hc::inward(es, edge, Point2::new(q[0], q[1]));
            if *target == Target::Position {
                let sh = hc::shift_for(e, d, q[2]).ok_or("no wall position reaches that point")?;
                hc::move_wall(es, edge, sh)
            } else {
                single(
                    hc::solve(e, d, q[2], *target)
                        .ok_or("no plane of this wall reaches that point")?,
                )
            }
        }
        HelperRequest::Merge { target, walls } => {
            let walls: Vec<usize> = walls.clone().unwrap_or_else(|| hc::same_way(es, edge));
            if let Some(k) = walls.iter().find(|&&k| k >= es.len()) {
                return Err(format!("wall {k} does not exist"));
            }
            match target {
                Target::Pitch => return Err("merge solves for eave, overhang or position".into()),
                Target::Position => {
                    hc::merge_by_moving(es, edge, &walls).ok_or("this wall has no slope")?
                }
                t => hc::merge_onto(es, edge, &walls, *t),
            }
        }
        HelperRequest::Hip {
            neighbour,
            by,
            hip,
            first,
            second,
        } => {
            let n = es.len();
            let other = match neighbour {
                Neighbour::Prev => (edge + n - 1) % n,
                Neighbour::Next => (edge + 1) % n,
            };
            let f = if *by == HipBy::Eaves {
                hc::hip_pitches_from_eaves
            } else {
                hc::hip_pitches
            };
            f(es, edge, other, *hip, *first, *second).ok_or(
                "no pitches fit these lengths (both walls need a slope and the same eave height)",
            )?
        }
        HelperRequest::BuildUp { thickness, eave } => {
            single(hc::build_up(e, *thickness, *eave).ok_or("not possible on this wall")?)
        }
        HelperRequest::RoundRoof {
            eave_height,
            overhang,
            segments,
            mode,
            value,
        } => {
            let opp = opposite_wall(es, edge).ok_or("this wall has no facing wall")?;
            let eave = eave_height.unwrap_or(e.eave_height);
            let over = overhang.unwrap_or(e.overhang);
            let arc = arc_for(&ArcInput {
                span: span_between(es, edge, opp),
                overhang: over,
                eave_height: eave,
                segments: *segments,
                mode: *mode,
                value: *value,
            })
            .ok_or("the value is out of range for this span")?;
            let patch = round_patch(&arc, eave, over);
            out.opposite_edge = Some(opp);
            let patches = Patches::from([(edge, patch.clone()), (opp, patch)]);
            out.arc = Some(arc);
            patches
        }
    };
    out.patches = patches;
    Ok(out)
}

/// [`preview`] for callers that take nothing short of a result (MCP): a
/// refused parallel result is an error.
pub fn run(
    project: &Project,
    part: usize,
    edge: usize,
    req: &HelperRequest,
) -> Result<HelperOut, String> {
    let out = preview(project, part, edge, req)?;
    match out.refused {
        Some(why) => Err(why),
        None => Ok(out),
    }
}

/// The values the round-roof helper gives a wall: the arc's pitch and
/// breaks, the given eave height and overhang, and no half-hip.
pub fn round_patch(arc: &Arc, eave_height: f64, overhang: f64) -> EdgePatch {
    EdgePatch {
        pitch_deg: Some(arc.pitch_deg),
        eave_height: Some(eave_height),
        overhang: Some(overhang),
        breaks: Some(arc.breaks.clone()),
        half_hip: Some(None),
        ..EdgePatch::default()
    }
}

/// Applies `patches` to part `part`. A building without a height of its own
/// keeps the one it shows when an eave height changes (`freeze_height`).
/// Returns the patches that changed something.
pub fn apply_to_part(project: &mut Project, part: usize, patches: &Patches) -> Patches {
    let Some(p) = project.parts.get_mut(part) else {
        return Patches::new();
    };
    if patches.values().any(|q| q.eave_height.is_some()) {
        p.freeze_height();
    }
    apply_patches(&mut p.edges, patches)
}

/// The patches without the fields that would not change the walls.
pub fn changes_of(edges: &[EdgeDef], patches: &Patches) -> Patches {
    patches
        .iter()
        .filter_map(|(&k, patch)| {
            let c = patch.changes(edges.get(k)?);
            (!c.is_empty()).then_some((k, c))
        })
        .collect()
}

/// Works the request out and applies it (the MCP tool with `apply`, the
/// dialog's OK). `Err` when it has no solution or gives the same values.
pub fn apply(
    project: &mut Project,
    part: usize,
    edge: usize,
    req: &HelperRequest,
) -> Result<HelperOut, String> {
    let out = run(project, part, edge, req)?;
    if apply_to_part(project, part, &out.patches).is_empty() {
        return Err("the walls already have these values".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::plain_edge;
    use crate::model::RoofPart;

    /// 10 × 8 gable roof, eave 3.0, overhang 0.5, pitch 40° (DACAD's cases).
    fn gable() -> Project {
        let wall = |x, y, pitch| plain_edge(Point2::new(x, y), pitch, 3.0, 0.5);
        let mut p = Project::new("helper");
        p.parts.push(RoofPart {
            name: "A".into(),
            height: None,
            edges: vec![
                wall(0.0, 0.0, 40.0),
                wall(10.0, 0.0, 90.0),
                wall(10.0, 8.0, 40.0),
                wall(0.0, 8.0, 90.0),
            ],
        });
        p
    }

    fn parallel(target: Target, field: ParallelField, value: f64) -> HelperRequest {
        HelperRequest::Parallel {
            target,
            steps: vec![ParallelStep { field, value }],
            width_includes_overhang: true,
        }
    }

    #[test]
    fn the_request_reads_as_the_dialog_and_mcp_send_it() {
        let r: HelperRequest = serde_json::from_value(serde_json::json!({
            "tool": "parallel", "target": "eave",
            "steps": [{ "field": "width_a", "value": 4.0 }],
        }))
        .unwrap();
        assert_eq!(r, parallel(Target::Eave, ParallelField::WidthA, 4.0));
        let r: HelperRequest =
            serde_json::from_value(serde_json::json!({ "tool": "build_up", "thickness": 0.2 }))
                .unwrap();
        assert_eq!(
            r,
            HelperRequest::BuildUp {
                thickness: 0.2,
                eave: BuildUpEave::Normal
            }
        );
        assert!(
            serde_json::from_value::<HelperRequest>(serde_json::json!({ "tool": "nope" })).is_err()
        );
    }

    #[test]
    fn a_triangle_gives_the_pitch_and_its_patch() {
        let p = gable();
        let req = HelperRequest::Triangle {
            width: Some(4.0),
            rafter: None,
            ridge: Some(6.0),
            width_includes_overhang: false,
        };
        let out = preview(&p, 0, 0, &req).unwrap();
        let pitch = out.patches[&0].pitch_deg.unwrap();
        assert!((pitch - 33.6901).abs() < 1e-4, "{pitch}");
        assert_eq!(out.triangle.unwrap().pitch_deg, pitch);
        // Not enough values: a reason, not a panic.
        let none = HelperRequest::Triangle {
            width: Some(4.0),
            rafter: None,
            ridge: None,
            width_includes_overhang: false,
        };
        assert!(preview(&p, 0, 0, &none).is_err());
    }

    #[test]
    fn an_eave_below_the_gables_is_worked_out_but_refused() {
        // DACAD case Eb: the dialog still shows the state, MCP gets an error.
        let p = gable();
        let req = parallel(Target::Eave, ParallelField::WidthA, 4.0);
        let out = preview(&p, 0, 0, &req).unwrap();
        assert_eq!(out.refused.as_deref(), Some(EAVE_BELOW_GABLE));
        assert!(out.patches.is_empty() && out.state.is_some() && out.rafters.is_some());
        assert_eq!(run(&p, 0, 0, &req).unwrap_err(), EAVE_BELOW_GABLE);
    }

    #[test]
    fn a_parallel_step_without_solution_names_the_step() {
        let p = gable();
        let req = parallel(Target::Pitch, ParallelField::WidthA, 99.0);
        assert!(preview(&p, 0, 0, &req).unwrap_err().contains("step 0"));
    }

    #[test]
    fn the_context_names_the_facing_wall_and_the_walls_that_join() {
        let p = gable();
        let c = context(&p, 0, 0, false).unwrap();
        assert_eq!(c.opposite, Some(2));
        assert!((c.span - 8.0).abs() < 1e-9);
        assert!(c.current.is_some() && !c.breaks && c.mates.is_empty() && c.roof_points.is_none());
        let c = context(&p, 0, 0, true).unwrap();
        let pts = c.roof_points.unwrap();
        assert!(!pts.is_empty());
        assert!(
            pts.windows(2).all(|w| w[0].p[2] >= w[1].p[2]),
            "highest first"
        );
        assert!(pts.iter().all(|q| q.kind != LineKind::Eave));
    }

    #[test]
    fn bad_indices_are_refused_not_indexed() {
        let p = gable();
        assert!(context(&p, 3, 0, false).is_err());
        assert!(context(&p, 0, 4, false).is_err());
        let merge = |walls| HelperRequest::Merge {
            target: Target::Eave,
            walls: Some(walls),
        };
        assert!(
            preview(&p, 0, 0, &merge(vec![2, 99]))
                .unwrap_err()
                .contains("99")
        );
        assert!(preview(&p, 0, 0, &merge(vec![2])).is_ok());
    }

    #[test]
    fn applying_sets_the_walls_and_keeps_the_building_height() {
        let mut p = gable();
        let req = HelperRequest::BuildUp {
            thickness: 0.2,
            eave: BuildUpEave::Normal,
        };
        let before = p.parts[0].building_height();
        apply(&mut p, 0, 0, &req).unwrap();
        let e = &p.parts[0].edges[0];
        assert!((e.eave_height - 3.1532).abs() < 1e-4 && (e.overhang - 0.6286).abs() < 1e-4);
        assert_eq!(p.parts[0].height, before, "an eave edit freezes the height");
        // The same values again change nothing: no result.
        let mut q = p.clone();
        assert!(
            apply(&mut q, 0, 0, &req).is_ok(),
            "a second build-up adds again"
        );
        let same = HelperRequest::Triangle {
            width: Some(4.0),
            rafter: None,
            ridge: Some(6.0),
            width_includes_overhang: false,
        };
        let mut r = gable();
        r.parts[0].edges[0].pitch_deg = 33.6901;
        assert!(apply(&mut r, 0, 0, &same).is_err());
    }

    #[test]
    fn the_round_roof_takes_the_dialogs_eave_and_overhang() {
        let p = gable();
        let req = HelperRequest::RoundRoof {
            eave_height: Some(3.5),
            overhang: Some(0.25),
            segments: 0,
            mode: ArcMode::Default,
            value: 0.0,
        };
        let out = preview(&p, 0, 0, &req).unwrap();
        assert_eq!(out.opposite_edge, Some(2));
        let (a, b) = (&out.patches[&0], &out.patches[&2]);
        assert_eq!(a, b);
        assert_eq!((a.eave_height, a.overhang), (Some(3.5), Some(0.25)));
        assert_eq!(a.half_hip, Some(None));
        assert_eq!(
            a.breaks.as_ref().map(Vec::len),
            Some(out.arc.unwrap().breaks.len())
        );
    }
}
