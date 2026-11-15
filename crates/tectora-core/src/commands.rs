//! The bodies of the desktop app's calculation commands (`calc`,
//! `calc_breakdown`, `take_snapshot` in `app/src-tauri/src/lib.rs`), shared
//! with the browser preview's bridge (`examples/ui_bridge.rs`) so the end-to-end
//! tests run the code the shipped app runs, and tested here: the app crate
//! needs WebKitGTK and is not built in the Linux CI container (TEST-008).
//!
//! Errors carry the stable code the UI translates (`app/src/lib/errors.ts`).

use serde::Serialize;
use serde_json::Value;

use crate::{Project, snapshot};

/// Why a command refused: the code of [`Project::check_input`]
/// (`too_many`: more parts, walls or objects than the limits allow;
/// `out_of_range`: a number of the project is out of range) or `internal`
/// (the roof kernel failed), with the English detail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommandError {
    pub code: &'static str,
    pub detail: String,
}

impl CommandError {
    fn input(e: crate::model::InputError) -> Self {
        Self {
            code: e.code,
            detail: e.message,
        }
    }
    fn out_of_range(detail: String) -> Self {
        Self {
            code: "out_of_range",
            detail,
        }
    }
    fn internal(detail: String) -> Self {
        Self {
            code: "internal",
            detail,
        }
    }
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.detail)
    }
}

/// Roof geometry and quantities for every part, plus totals, what kept
/// parts from being calculated, and the stored result's status. Out-of-range
/// input and a kernel panic are errors, never a crash.
pub fn calc(project: &Project) -> Result<Value, CommandError> {
    check_input(project)?;
    // One calculation for the report and the stored result's status.
    let (mut v, status) = crate::model::guarded(|| {
        let geos = crate::openings::generate_project(project);
        (
            crate::report::calc_of(project, &geos),
            snapshot::status_of(&geos, project),
        )
    })
    .map_err(CommandError::internal)?;
    v["stored_result"] = serde_json::to_value(status).unwrap_or(Value::Null);
    Ok(v)
}

/// How every face area and line length was calculated, step by step, with
/// the same guards as [`calc`].
pub fn calc_breakdown(project: &Project) -> Result<Value, CommandError> {
    check_input(project)?;
    let b = crate::model::guarded(|| crate::breakdown::breakdown(project))
        .map_err(CommandError::internal)?;
    Ok(serde_json::to_value(b).unwrap_or(Value::Null))
}

/// The current result, to store as the job's result.
pub fn take_snapshot(project: &Project) -> Result<snapshot::ResultSnapshot, CommandError> {
    check_input(project)?;
    snapshot::take_checked(project).map_err(|e| match e {
        snapshot::Refused::OutOfRange(d) => CommandError::out_of_range(d),
        snapshot::Refused::Internal(d) => CommandError::internal(d),
        snapshot::Refused::NotCalculated(detail) => CommandError {
            code: "not_calculated",
            detail,
        },
    })
}

/// The roof windows that do not fit their roof face (`edit::check_windows`),
/// `null` when all fit, with the same guards as [`calc`].
pub fn check_windows(project: &Project) -> Result<Value, CommandError> {
    check_input(project)?;
    let bad = crate::model::guarded(|| crate::edit::check_windows(project).err())
        .map_err(CommandError::internal)?;
    Ok(serde_json::to_value(bad).unwrap_or(Value::Null))
}

/// The roof under dormer `index` (`edit::dormers::host_slope`): height and
/// slope at the middle of its front, `null` off the roof, with the same
/// guards as [`calc`]. The editor's dormer dialog derives its section from it.
pub fn dormer_host(project: &Project, index: usize) -> Result<Value, CommandError> {
    check_input(project)?;
    let host = crate::model::guarded(|| crate::edit::dormers::host_slope(project, index))
        .map_err(CommandError::internal)?;
    Ok(serde_json::to_value(host).unwrap_or(Value::Null))
}

/// A request the edit operation refused or found no solution for: the UI
/// shows its own message, `detail` is the English reason.
fn refused(detail: String) -> CommandError {
    CommandError {
        code: "refused",
        detail,
    }
}

/// What the calculation dialog shows of a helper: the new values by wall
/// (only the fields that differ from the walls now), whether the values give
/// the walls' current state, and the intermediate figures.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HelperPreview {
    pub patches: crate::edit::Patches,
    /// The helper found values and they equal what the walls have now.
    pub unchanged: bool,
    #[serde(flatten)]
    pub out: crate::edit::helper::HelperOut,
}

/// What the calculation dialog needs to know about a wall
/// (`edit::helper::context`), with the same guards as [`calc`].
pub fn roof_helper_context(
    project: &Project,
    part: usize,
    edge: usize,
    with_points: bool,
) -> Result<Value, CommandError> {
    check_input(project)?;
    let c =
        crate::model::guarded(|| crate::edit::helper::context(project, part, edge, with_points))
            .map_err(CommandError::internal)?
            .map_err(refused)?;
    Ok(serde_json::to_value(c).unwrap_or(Value::Null))
}

/// The helper's result for the dialog's live figures
/// (`edit::helper::preview`); nothing is changed. A request without a
/// solution is `refused` with the reason.
pub fn roof_helper_preview(
    project: &Project,
    part: usize,
    edge: usize,
    req: &crate::edit::helper::HelperRequest,
) -> Result<Value, CommandError> {
    check_input(project)?;
    let r = crate::model::guarded(|| {
        let mut out = crate::edit::helper::preview(project, part, edge, req)?;
        let raw = std::mem::take(&mut out.patches);
        let patches = project
            .parts
            .get(part)
            .map(|p| crate::edit::helper::changes_of(&p.edges, &raw))
            .unwrap_or_default();
        let unchanged = !raw.is_empty() && patches.is_empty();
        Ok::<_, String>(HelperPreview {
            patches,
            unchanged,
            out,
        })
    })
    .map_err(CommandError::internal)?
    .map_err(refused)?;
    Ok(serde_json::to_value(r).unwrap_or(Value::Null))
}

/// The dialog's OK: the project with the helper's values applied
/// (`edit::helper::apply`, as MCP `roof_helper` with `apply`). The result is
/// checked against the input limits before it goes back.
pub fn roof_helper_apply(
    project: &Project,
    part: usize,
    edge: usize,
    req: &crate::edit::helper::HelperRequest,
) -> Result<Project, CommandError> {
    check_input(project)?;
    let mut next = project.clone();
    crate::model::guarded(|| crate::edit::helper::apply(&mut next, part, edge, req))
        .map_err(CommandError::internal)?
        .map_err(refused)?;
    check_input(&next)?;
    Ok(next)
}

/// The round-roof dialog's figures for the values typed in
/// (`edit::round::arc_for`): the arc (`null` when the value is out of range
/// for the span), DACAD's
/// default arc for the same span, overhang and eave height (what a new mode
/// starts from), and the smallest radius.
pub fn round_arc(input: &crate::edit::round::ArcInput) -> Result<Value, CommandError> {
    use crate::edit::round::{ArcInput, ArcMode, arc_for, min_radius};
    let bounded = |v: f64| v.is_finite() && v.abs() <= crate::model::MAX_COORD;
    if !(bounded(input.span) && bounded(input.overhang) && bounded(input.eave_height)) {
        return Err(CommandError::out_of_range(
            "span, overhang or eave height out of range".into(),
        ));
    }
    let (arc, default) = crate::model::guarded(|| {
        let default = ArcInput {
            segments: 0,
            mode: ArcMode::Default,
            value: 0.0,
            ..*input
        };
        (arc_for(input), arc_for(&default))
    })
    .map_err(CommandError::internal)?;
    Ok(serde_json::json!({
        "arc": arc,
        "default_arc": default,
        "min_radius": min_radius(input.span, input.overhang),
    }))
}

/// An editor operation on shapes alone (no roof to calculate): what the
/// object bar and the plan tools ask the core for. The same functions back
/// the MCP tools `update_opening`, `set_openings` and the face area in
/// `get_project`.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum GeometryRequest {
    /// The corners of a chimney or roof opening stretched and turned so that
    /// their box becomes `to` (`edit::openings::reshape_opening`).
    ReshapeOpening {
        corners: Vec<crate::geom::Point2>,
        to: crate::edit::openings::OpeningBox,
    },
    /// A rectangular chimney from a corner, size and rotation
    /// (`edit::probe::chimney_from_rect`).
    ChimneyFromRect {
        x: f64,
        y: f64,
        width: f64,
        depth: f64,
        rotation_deg: f64,
        height: f64,
    },
    /// The sloped area of a hand-drawn face (`ManualFace::area`).
    FaceArea { face: crate::model::ManualFace },
    /// The corners of a regular polygon (`edit::transform::regular_polygon`).
    RegularPolygon {
        centre: crate::geom::Point2,
        sides: usize,
        side: f64,
        #[serde(default)]
        rotation_deg: f64,
    },
    /// Corner `vertex` of a part dropped onto its neighbour merges into it
    /// (`edit::corners::merge_dropped_corner`): `{part, merged}`.
    MergeDroppedCorner {
        part: crate::model::RoofPart,
        vertex: usize,
    },
    /// A drawn footprint without repeated corners
    /// (`edit::corners::without_repeats`): `{points}`.
    WithoutRepeats { points: Vec<crate::geom::Point2> },
    /// The centre of a part's wall outline, the default pivot
    /// (`edit::transform::part_centre`).
    PartCentre { part: crate::model::RoofPart },
}

/// Answers a [`GeometryRequest`]: `{corners}`, a chimney `{corners, height}`
/// or `{area}`. Sizes and coordinates are checked first; a request the
/// operation cannot do is `refused` with its reason.
pub fn item_geometry(req: &GeometryRequest) -> Result<Value, CommandError> {
    use crate::model::{MAX_COORD, MAX_PART_WALLS};
    let in_range = |v: f64| v.is_finite() && v.abs() <= MAX_COORD;
    let points_ok = |pts: &[crate::geom::Point2]| {
        pts.len() <= MAX_PART_WALLS && pts.iter().all(|p| in_range(p.x) && in_range(p.y))
    };
    let bad = || {
        CommandError::out_of_range("a coordinate or the number of corners is out of range".into())
    };
    match req {
        GeometryRequest::ReshapeOpening { corners, to } => {
            if !points_ok(corners) {
                return Err(bad());
            }
            let c = crate::edit::openings::reshape_opening(corners, to).map_err(refused)?;
            Ok(serde_json::json!({ "corners": c }))
        }
        GeometryRequest::ChimneyFromRect {
            x,
            y,
            width,
            depth,
            rotation_deg,
            height,
        } => {
            if ![x, y, width, depth, rotation_deg, height]
                .into_iter()
                .all(|v| in_range(*v))
            {
                return Err(bad());
            }
            let c = crate::edit::probe::chimney_from_rect(
                *x,
                *y,
                *width,
                *depth,
                *rotation_deg,
                *height,
            );
            Ok(serde_json::to_value(c).unwrap_or(Value::Null))
        }
        GeometryRequest::RegularPolygon {
            centre,
            sides,
            side,
            rotation_deg,
        } => {
            if !points_ok(&[*centre]) || !in_range(*side) || !in_range(*rotation_deg) {
                return Err(bad());
            }
            if !(3..=crate::edit::transform::MAX_POLYGON_SIDES).contains(sides) {
                return Err(refused(format!(
                    "a regular polygon has 3 to {} sides",
                    crate::edit::transform::MAX_POLYGON_SIDES
                )));
            }
            let pts =
                crate::edit::transform::regular_polygon(*centre, *sides, *side, *rotation_deg);
            Ok(serde_json::json!({ "corners": pts }))
        }
        GeometryRequest::MergeDroppedCorner { part, vertex } => {
            let pts = part.outline();
            if !points_ok(&pts) {
                return Err(bad());
            }
            let mut part = part.clone();
            let merged = crate::edit::corners::merge_dropped_corner(&mut part.edges, *vertex);
            Ok(serde_json::json!({ "part": part, "merged": merged }))
        }
        GeometryRequest::WithoutRepeats { points } => {
            if !points_ok(points) {
                return Err(bad());
            }
            Ok(serde_json::json!({ "points": crate::edit::corners::without_repeats(points) }))
        }
        GeometryRequest::PartCentre { part } => {
            let pts = part.outline();
            if !points_ok(&pts) {
                return Err(bad());
            }
            Ok(serde_json::json!({ "centre": crate::edit::transform::part_centre(part) }))
        }
        GeometryRequest::FaceArea { face } => {
            if !points_ok(&face.points) || !face.pitch_deg.is_finite() {
                return Err(bad());
            }
            Ok(serde_json::json!({ "area": face.area() }))
        }
    }
}

/// The part's move, rotation, mirror or copies, with everything lying on it
/// (`edit::transform::transform_part`, the MCP tool `transform_part`): the
/// project afterwards and the indices of the moved or new parts. The result
/// is checked against the input limits (copies add parts and objects).
pub fn transform_part(
    project: &Project,
    part: usize,
    map: crate::edit::transform::PlanMap,
    copies: u32,
) -> Result<Value, CommandError> {
    use crate::edit::transform::{MAX_COPIES, transform_part};
    check_input(project)?;
    if part >= project.parts.len() {
        return Err(refused(format!("part {part} does not exist")));
    }
    if copies > MAX_COPIES {
        return Err(refused(format!("at most {MAX_COPIES} copies")));
    }
    let mut next = project.clone();
    let parts = crate::model::guarded(|| transform_part(&mut next, part, map, copies))
        .map_err(CommandError::internal)?;
    check_input(&next)?;
    Ok(serde_json::json!({ "project": next, "parts": parts }))
}

/// Wall `edge` of part `part` moved parallel by `dist` m outward, its
/// neighbours stretched to meet it (`edit::transform::offset_wall`); `refused`
/// where a neighbour runs parallel.
pub fn offset_wall(
    project: &Project,
    part: usize,
    edge: usize,
    dist: f64,
) -> Result<Project, CommandError> {
    check_input(project)?;
    if !dist.is_finite() || dist.abs() > crate::model::MAX_COORD {
        return Err(CommandError::out_of_range(
            "the distance is out of range".into(),
        ));
    }
    let mut next = project.clone();
    let done = crate::model::guarded(|| {
        next.parts
            .get_mut(part)
            .is_some_and(|p| crate::edit::transform::offset_wall(p, edge, dist))
    })
    .map_err(CommandError::internal)?;
    if !done {
        return Err(refused(
            "no corner can be found: the wall does not exist or a neighbour runs parallel to it"
                .into(),
        ));
    }
    check_input(&next)?;
    Ok(next)
}

/// [`Project::check_input`] as a command error, with its code.
pub fn check_input(project: &Project) -> Result<(), CommandError> {
    project.check_input().map_err(CommandError::input)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hip() -> Project {
        crate::examples::find("hip").expect("example").project()
    }

    fn far_off() -> Project {
        let mut p = hip();
        p.parts[0].edges[0].start.x = 1e12;
        p
    }

    #[test]
    fn calc_gives_totals_and_the_stored_result_status() {
        let v = calc(&hip()).expect("calculates");
        assert!(v["totals"]["roof_area"].as_f64().unwrap() > 0.0);
        assert!(v["totals"]["hip_length"].as_f64().unwrap() > 0.0);
        assert!(v.get("stored_result").is_some());
        assert_eq!(v["parts"].as_array().map(Vec::len), Some(1));
    }

    /// Roof calculations one call of `f` starts (review ARCH-006).
    fn runs(f: impl FnOnce()) -> u64 {
        let before = crate::openings::core_runs();
        f();
        crate::openings::core_runs() - before
    }

    #[test]
    fn one_request_calculates_the_roof_once() {
        // `calc` calculated the roof 3 times (5 with a stored result) before.
        let mut p = hip();
        assert_eq!(runs(|| drop(calc(&p).unwrap())), 1);
        assert_eq!(runs(|| drop(take_snapshot(&p).unwrap())), 1);
        assert_eq!(
            runs(|| drop(crate::snapshot::status(&p))),
            0,
            "no stored result"
        );
        assert!(p.set_status(crate::model::JobStatus::Offer));
        assert_eq!(runs(|| drop(calc(&p).unwrap())), 1, "with a stored result");
        assert_eq!(runs(|| drop(crate::snapshot::status(&p))), 1);
        // With an object the placements measure it on the bare roof: one
        // more calculation, of another design.
        p.chimneys.push(crate::model::Chimney {
            corners: [(3.0, 3.0), (4.0, 3.0), (4.0, 4.0), (3.0, 4.0)]
                .map(|(x, y)| crate::geom::Point2::new(x, y))
                .to_vec(),
            height: 6.0,
        });
        assert_eq!(runs(|| drop(calc(&p).unwrap())), 2, "with a chimney");
    }

    #[test]
    fn calc_refuses_out_of_range_input_with_its_code() {
        let e = calc(&far_off()).unwrap_err();
        assert_eq!(e.code, "out_of_range");
        assert!(!e.detail.is_empty());
        assert_eq!(calc_breakdown(&far_off()).unwrap_err().code, "out_of_range");
        assert_eq!(take_snapshot(&far_off()).unwrap_err().code, "out_of_range");
    }

    #[test]
    fn too_many_parts_are_refused_with_their_own_code() {
        let mut p = hip();
        let part = p.parts[0].clone();
        p.parts = vec![part; crate::model::MAX_PARTS + 1];
        for e in [
            calc(&p).unwrap_err(),
            calc_breakdown(&p).unwrap_err(),
            take_snapshot(&p).unwrap_err(),
            check_windows(&p).unwrap_err(),
        ] {
            assert_eq!(e.code, "too_many", "{e}");
        }
        assert_eq!(check_windows(&far_off()).unwrap_err().code, "out_of_range");
        assert!(check_windows(&hip()).unwrap().is_null());
    }

    #[test]
    fn breakdown_and_snapshot_match_the_calculation() {
        let p = hip();
        let v = calc(&p).unwrap();
        let s = take_snapshot(&p).unwrap();
        let area = v["totals"]["roof_area"].as_f64().unwrap();
        assert!(
            (s.roof_area - area).abs() < 0.006,
            "{} vs {area}",
            s.roof_area
        );
        let b = calc_breakdown(&p).unwrap();
        assert!(!b.is_null());
    }

    fn gable_job() -> Project {
        crate::examples::find("gable").expect("example").project()
    }

    #[test]
    fn the_helper_preview_names_what_changes_and_what_is_the_same() {
        use crate::edit::calc::BuildUpEave;
        use crate::edit::helper::HelperRequest;
        let p = gable_job();
        let req = HelperRequest::BuildUp {
            thickness: 0.2,
            eave: BuildUpEave::Normal,
        };
        let v = roof_helper_preview(&p, 0, 0, &req).unwrap();
        assert_eq!(v["unchanged"], false);
        assert!(
            v["patches"]["0"]["eave_height"].as_f64().unwrap() > p.parts[0].edges[0].eave_height
        );
        // Nothing to change: the values equal the wall's.
        let zero = HelperRequest::BuildUp {
            thickness: 0.0,
            eave: BuildUpEave::Normal,
        };
        let v = roof_helper_preview(&p, 0, 0, &zero).unwrap();
        assert_eq!(v["unchanged"], true);
        assert!(v["patches"].as_object().unwrap().is_empty());
        // A wall that does not exist is a refusal, not a panic.
        assert_eq!(
            roof_helper_preview(&p, 0, 99, &req).unwrap_err().code,
            "refused"
        );
        assert_eq!(
            roof_helper_context(&p, 7, 0, false).unwrap_err().code,
            "refused"
        );
        assert_eq!(
            roof_helper_preview(&far_off(), 0, 0, &req)
                .unwrap_err()
                .code,
            "out_of_range"
        );
    }

    #[test]
    fn the_helper_apply_gives_the_project_back_and_refuses_what_changes_nothing() {
        use crate::edit::calc::BuildUpEave;
        use crate::edit::helper::HelperRequest;
        let p = gable_job();
        let req = HelperRequest::BuildUp {
            thickness: 0.2,
            eave: BuildUpEave::Normal,
        };
        let next = roof_helper_apply(&p, 0, 0, &req).unwrap();
        assert!(next.parts[0].edges[0].overhang > p.parts[0].edges[0].overhang);
        let zero = HelperRequest::BuildUp {
            thickness: 0.0,
            eave: BuildUpEave::Normal,
        };
        assert_eq!(
            roof_helper_apply(&p, 0, 0, &zero).unwrap_err().code,
            "refused"
        );
    }

    #[test]
    fn the_round_arc_answers_out_of_range_values_with_no_arc() {
        use crate::edit::round::{ArcInput, ArcMode};
        let input = ArcInput {
            span: 8.0,
            overhang: 0.5,
            eave_height: 3.0,
            segments: 0,
            mode: ArcMode::Default,
            value: 0.0,
        };
        let v = round_arc(&input).unwrap();
        assert!(v["arc"]["radius"].as_f64().unwrap() > 4.5);
        assert_eq!(v["min_radius"].as_f64(), Some(4.5));
        assert_eq!(v["default_arc"], v["arc"]);
        let small = ArcInput {
            mode: ArcMode::Radius,
            value: 1.0,
            ..input
        };
        let v = round_arc(&small).unwrap();
        assert!(v["arc"].is_null() && v["default_arc"].is_object());
        let huge = ArcInput {
            span: 1e300,
            ..input
        };
        assert_eq!(round_arc(&huge).unwrap_err().code, "out_of_range");
    }

    #[test]
    fn item_geometry_answers_shapes_and_refuses_what_it_cannot_do() {
        let ask = |v: Value| item_geometry(&serde_json::from_value(v).unwrap());
        let c = ask(
            serde_json::json!({ "op": "chimney_from_rect", "x": 4, "y": 1, "width": 0.6,
            "depth": 0.6, "rotation_deg": 20, "height": 6 }),
        )
        .unwrap();
        let corners = c["corners"].clone();
        assert_eq!(corners.as_array().map(Vec::len), Some(4));
        // The box gets stretched; the same box gives the corners back.
        let b = crate::edit::openings::opening_box(
            &serde_json::from_value::<Vec<crate::geom::Point2>>(corners.clone()).unwrap(),
        )
        .unwrap();
        let same = ask(serde_json::json!({ "op": "reshape_opening", "corners": corners, "to": b }))
            .unwrap();
        assert_eq!(same["corners"], corners);
        let wide = ask(
            serde_json::json!({ "op": "reshape_opening", "corners": corners,
            "to": { "x": b.x, "y": b.y, "width": 0.0, "depth": b.depth, "rotation_deg": b.rotation_deg } }),
        );
        assert_eq!(wide.unwrap_err().code, "refused");
        let far = ask(
            serde_json::json!({ "op": "reshape_opening", "corners": [{"x": 1e12, "y": 0}],
            "to": { "x": 0, "y": 0, "width": 1, "depth": 1, "rotation_deg": 0 } }),
        );
        assert_eq!(far.unwrap_err().code, "out_of_range");
        // Face area: plan area over the cosine, 0 where the core says so.
        let face = |pitch: f64| {
            serde_json::json!({ "op": "face_area", "face": { "points": [
            {"x": 0, "y": 0}, {"x": 4, "y": 0}, {"x": 4, "y": 3}, {"x": 0, "y": 3}], "pitch_deg": pitch } })
        };
        assert_eq!(ask(face(0.0)).unwrap()["area"], 12.0);
        assert!((ask(face(60.0)).unwrap()["area"].as_f64().unwrap() - 24.0).abs() < 1e-9);
        assert_eq!(ask(face(89.99999)).unwrap()["area"], 0.0);
    }

    #[test]
    fn transform_commands_move_copy_and_refuse() {
        use crate::edit::transform::PlanMap;
        let p = gable_job();
        let moved = transform_part(&p, 0, PlanMap::Move { dx: 2.0, dy: 0.0 }, 0).unwrap();
        assert_eq!(moved["parts"], serde_json::json!([0]));
        let x0 = p.parts[0].edges[0].start.x;
        assert_eq!(
            moved["project"]["parts"][0]["edges"][0]["start"]["x"].as_f64(),
            Some(x0 + 2.0)
        );
        let copies = transform_part(&p, 0, PlanMap::Move { dx: 20.0, dy: 0.0 }, 2).unwrap();
        assert_eq!(copies["parts"], serde_json::json!([1, 2]));
        assert_eq!(
            transform_part(&p, 5, PlanMap::Move { dx: 1.0, dy: 0.0 }, 0)
                .unwrap_err()
                .code,
            "refused"
        );
        assert_eq!(
            transform_part(&p, 0, PlanMap::Move { dx: 1.0, dy: 0.0 }, 51)
                .unwrap_err()
                .code,
            "refused"
        );
        assert_eq!(
            transform_part(&far_off(), 0, PlanMap::Move { dx: 1.0, dy: 0.0 }, 0)
                .unwrap_err()
                .code,
            "out_of_range"
        );
        let w = offset_wall(&p, 0, 0, 1.0).unwrap();
        assert_ne!(w.parts[0].edges[0].start, p.parts[0].edges[0].start);
        assert_eq!(offset_wall(&p, 0, 99, 1.0).unwrap_err().code, "refused");
        assert_eq!(
            offset_wall(&p, 0, 0, f64::INFINITY).unwrap_err().code,
            "out_of_range"
        );
        let ask = |v: Value| item_geometry(&serde_json::from_value(v).unwrap());
        let poly = ask(serde_json::json!({ "op": "regular_polygon", "centre": {"x": 0, "y": 0}, "sides": 6, "side": 2 })).unwrap();
        assert_eq!(poly["corners"].as_array().map(Vec::len), Some(6));
        assert_eq!(ask(serde_json::json!({ "op": "regular_polygon", "centre": {"x": 0, "y": 0}, "sides": 1000000000, "side": 2 })).unwrap_err().code, "refused");
        let m = ask(
            serde_json::json!({ "op": "merge_dropped_corner", "part": p.parts[0], "vertex": 1 }),
        )
        .unwrap();
        assert_eq!(m["merged"], false);
        let r = ask(serde_json::json!({ "op": "without_repeats", "points": [{"x": 0, "y": 0}, {"x": 0, "y": 0.001}, {"x": 1, "y": 0}] })).unwrap();
        assert_eq!(r["points"].as_array().map(Vec::len), Some(2));
        let c = ask(serde_json::json!({ "op": "part_centre", "part": p.parts[0] })).unwrap();
        assert!(c["centre"]["x"].is_number());
    }

    #[test]
    fn errors_serialize_as_the_ui_reads_them() {
        let e = CommandError::out_of_range("x = 1e12".into());
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            serde_json::json!({ "code": "out_of_range", "detail": "x = 1e12" })
        );
    }
}
