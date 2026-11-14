//! Where an object sits on its roof face: its distance from the eave edge
//! and from the sides of the face, as a roofer measures it on the roof
//! (Jakub, 2026-11-10: "every element on the roof needs a dimension from
//! the eave edge and from the side"), and moving it to given distances.
//! The editor shows these as dimension lines and fields for the selected
//! dormer, chimney, roof window or opening; the MCP tool `place_item`
//! takes the same values.
//!
//! [no source: Tectora's own definition, DACAD's dialogs only offer plan
//! positions.] Measured on the roof face the object stands on, in the calculated roof
//! without dormers, chimneys, openings and windows (so a hole an object cuts
//! does not change its own reference):
//!
//! - The face's frame: `up` is the plan direction the face rises in,
//!   `along` the level direction along its eave (to the right when looking
//!   up the slope from the eave, as a roof window's width runs:
//!   [`crate::openings::window_outline`]).
//! - From the eave: from the lowest edge of the face (the eave line at the
//!   overhang, or a pitch break below a mansard face) up to the lowest point
//!   of the object, measured along the slope; the plan distance is given too.
//! - From the left and right side: level, along the eave, from the edge of
//!   the face (a verge, a hip, a valley, the end of a joined roof) to the
//!   object's outline, at the height of the object's lowest point.
//!
//! The object's outline: a dormer's walls in plan (front and cheeks), a
//! chimney's or opening's corners, a roof window's outline in the face.

use serde::Serialize;

use super::transform::{ItemKind, ItemRef, follow_roof};
use crate::geom::Point2;
use crate::model::Project;
use crate::openings::{slope, window_outline};
use crate::roof::RoofGeometry;

/// An object's place on its roof face (see the module). Lengths in m.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Placement {
    /// The part and face (index into the part's calculated faces of the
    /// bare roof) the object stands on, and the wall the face belongs to.
    pub part: usize,
    pub edge: usize,
    /// Pitch of the face, degrees.
    pub pitch_deg: f64,
    /// From the lowest edge of the face to the object's lowest point, along
    /// the slope, and the same in plan.
    pub from_eave: f64,
    pub from_eave_plan: f64,
    /// From the left and right edge of the face, level along the eave, at
    /// the height of the object's lowest point.
    pub from_left: f64,
    pub from_right: f64,
    /// The dimension lines in plan, for drawing: eave edge to the object
    /// (at the middle of the object's width), left edge to the object,
    /// object to the right edge.
    pub eave_line: [Point2; 2],
    pub left_line: [Point2; 2],
    pub right_line: [Point2; 2],
}

/// The roof every object is measured on: the project without dormers,
/// chimneys, openings and windows. A roof the core cannot calculate gives
/// none (no placement), not a crash.
pub fn bare_roof(project: &Project) -> Vec<RoofGeometry> {
    let mut rest = project.clone();
    rest.dormers.clear();
    rest.chimneys.clear();
    rest.roof_openings.clear();
    rest.windows.clear();
    rest.manual_lines.clear();
    rest.manual_faces.clear();
    if rest.parts.is_empty() {
        return Vec::new();
    }
    crate::model::guarded(|| crate::openings::generate_project(&rest)).unwrap_or_default()
}

fn dot(a: Point2, b: Point2) -> f64 {
    a.x * b.x + a.y * b.y
}

fn inside(poly: &[Point2], p: Point2) -> bool {
    let mut c = false;
    let n = poly.len();
    let mut j = n.wrapping_sub(1);
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            c = !c;
        }
        j = i;
    }
    c
}

/// The plan outline of an object, `None` for a kind without a place on a
/// face or an index that does not exist.
fn outline(project: &Project, bare: &[RoofGeometry], it: ItemRef) -> Option<Vec<Point2>> {
    let pts = match it.kind {
        ItemKind::Dormer => {
            let d = project.dormers.get(it.index)?;
            let mut v: Vec<Point2> = d.edges.iter().map(|e| e.start).collect();
            v.push(d.end);
            v
        }
        ItemKind::Chimney => project.chimneys.get(it.index)?.corners.clone(),
        ItemKind::RoofOpening => project.roof_openings.get(it.index)?.corners.clone(),
        ItemKind::Window => window_outline(bare, project.windows.get(it.index)?),
        ItemKind::ManualLine | ItemKind::ManualFace => return None,
    };
    let finite = pts.iter().all(|p| p.x.is_finite() && p.y.is_finite());
    (pts.len() >= 2 && finite).then_some(pts)
}

/// The face the object stands on: the face of the bare roof holding a
/// point just inside the object's lowest corner, else the one holding its
/// centre. Returns (geometry, face).
fn host_face(bare: &[RoofGeometry], pts: &[Point2]) -> Option<(usize, usize)> {
    let n = pts.len() as f64;
    let c = Point2::new(
        pts.iter().map(|p| p.x).sum::<f64>() / n,
        pts.iter().map(|p| p.y).sum::<f64>() / n,
    );
    let plan = |f: &crate::roof::RoofFace| -> Vec<Point2> {
        f.polygon.iter().map(|q| Point2::new(q[0], q[1])).collect()
    };
    // The centre first: an object across a ridge or a hip belongs to the
    // face most of it stands on.
    let find = |p: Point2| {
        bare.iter().enumerate().find_map(|(g, geo)| {
            geo.faces
                .iter()
                .position(|f| f.polygon.len() >= 3 && slope(f).1 > 0.0 && inside(&plan(f), p))
                .map(|k| (g, k))
        })
    };
    find(c).or_else(|| {
        pts.iter().find_map(|&p| {
            // Slightly towards the centre, off a line.
            find(Point2::new(
                p.x + (c.x - p.x) * 1e-3,
                p.y + (c.y - p.y) * 1e-3,
            ))
        })
    })
}

/// The tangent of the face's pitch as entered, else as measured from its
/// points.
fn exact_tan(face: &crate::roof::RoofFace, measured: f64) -> f64 {
    let t = face.pitch_deg.to_radians().tan();
    if face.pitch_deg > 0.0 && face.pitch_deg < 90.0 && (t - measured).abs() < 1e-3 * (1.0 + t) {
        t
    } else {
        measured
    }
}

/// Where the line `v = level` (in the frame) crosses the polygon: the level
/// stretches inside it, as `(lo, hi)` along the eave.
fn chord(poly: &[Point2], up: Point2, along: Point2, level: f64) -> Vec<(f64, f64)> {
    let n = poly.len();
    let mut xs: Vec<f64> = Vec::new();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let (va, vb) = (dot(a, up), dot(b, up));
        if (va > level) != (vb > level) {
            let t = (level - va) / (vb - va);
            xs.push(dot(a, along) + t * (dot(b, along) - dot(a, along)));
        }
    }
    xs.sort_by(f64::total_cmp);
    xs.chunks_exact(2).map(|w| (w[0], w[1])).collect()
}

/// Where object `it` sits on its roof face (see the module), measured on
/// `bare` ([`bare_roof`] of the project). `None` when the object does not
/// exist or stands on no sloped face.
pub fn placement_in(project: &Project, bare: &[RoofGeometry], it: ItemRef) -> Option<Placement> {
    locate(project, bare, it).map(|(p, _)| p)
}

/// [`placement_in`] with the index of the host face in its part's faces.
fn locate(project: &Project, bare: &[RoofGeometry], it: ItemRef) -> Option<(Placement, usize)> {
    let pts = outline(project, bare, it)?;
    let (g, k) = host_face(bare, &pts)?;
    let face = bare.get(g)?.faces.get(k)?;
    let (up, measured) = slope(face);
    if measured <= 0.0 || !measured.is_finite() {
        return None;
    }
    // The pitch as entered (the core's points are single precision).
    let tan = exact_tan(face, measured);
    let along = Point2::new(up.y, -up.x);
    let poly: Vec<Point2> = face
        .polygon
        .iter()
        .map(|q| Point2::new(q[0], q[1]))
        .collect();
    let v = |p: Point2| dot(p, up);
    let u = |p: Point2| dot(p, along);
    let eave_v = poly.iter().map(|&p| v(p)).fold(f64::INFINITY, f64::min);
    let top_v = poly.iter().map(|&p| v(p)).fold(f64::NEG_INFINITY, f64::max);
    let low = pts.iter().map(|&p| v(p)).fold(f64::INFINITY, f64::min);
    let (u0, u1) = pts
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &p| {
            (a.min(u(p)), b.max(u(p)))
        });
    let from_eave_plan = low - eave_v;
    let cos = 1.0 / (1.0 + tan * tan).sqrt();
    // The face's width at the object's lowest point; just inside the face
    // where that point lies on (or beyond) its lowest or highest edge.
    let eps = 1e-6;
    let level = low.clamp(eave_v + eps, top_v - eps);
    let mid = (u0 + u1) / 2.0;
    let spans = chord(&poly, up, along, level);
    let (lo, hi) = spans
        .iter()
        .copied()
        .min_by(|a, b| {
            let d = |s: (f64, f64)| {
                if (s.0..=s.1).contains(&mid) {
                    0.0
                } else {
                    (s.0 - mid).abs().min((s.1 - mid).abs())
                }
            };
            d(*a).total_cmp(&d(*b))
        })
        .unwrap_or((u0, u1));
    let at = |uu: f64, vv: f64| Point2::new(along.x * uu + up.x * vv, along.y * uu + up.y * vv);
    let p = Placement {
        part: g,
        edge: face.edge,
        pitch_deg: tan.atan().to_degrees(),
        from_eave: from_eave_plan / cos,
        from_eave_plan,
        from_left: u0 - lo,
        from_right: hi - u1,
        eave_line: [at(mid, eave_v), at(mid, low)],
        left_line: [at(lo, low), at(u0, low)],
        right_line: [at(u1, low), at(hi, low)],
    };
    let ok = [p.from_eave, p.from_left, p.from_right, p.pitch_deg]
        .iter()
        .all(|x| x.is_finite());
    ok.then_some((p, k))
}

/// [`placement_in`] on the project's own bare roof.
pub fn placement(project: &Project, it: ItemRef) -> Option<Placement> {
    placement_in(project, &bare_roof(project), it)
}

/// The placement of every dormer, chimney, roof opening and window, in
/// that order of kinds and by index (`None` where an object stands on no
/// face), on one bare roof: for the calculation result.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Placements {
    pub dormers: Vec<Option<Placement>>,
    pub chimneys: Vec<Option<Placement>>,
    pub roof_openings: Vec<Option<Placement>>,
    pub windows: Vec<Option<Placement>>,
}

pub fn placements(project: &Project) -> Placements {
    let any = !project.dormers.is_empty()
        || !project.chimneys.is_empty()
        || !project.roof_openings.is_empty()
        || !project.windows.is_empty();
    if !any {
        return Placements::default();
    }
    let bare = bare_roof(project);
    let all = |kind: ItemKind, n: usize| -> Vec<Option<Placement>> {
        (0..n)
            .map(|index| placement_in(project, &bare, ItemRef { kind, index }))
            .collect()
    };
    Placements {
        dormers: all(ItemKind::Dormer, project.dormers.len()),
        chimneys: all(ItemKind::Chimney, project.chimneys.len()),
        roof_openings: all(ItemKind::RoofOpening, project.roof_openings.len()),
        windows: all(ItemKind::Window, project.windows.len()),
    }
}

/// Largest distance [`place`] accepts, m (beyond any roof face). A typing
/// error (cm for m) below it is refused because the object would leave its
/// face, which [`place`] checks after every step.
pub const MAX_DISTANCE: f64 = 500.0;

/// Moves object `it` so that it sits `from_eave` (along the slope) above
/// the lowest edge of its face and `from_left` or `from_right` from the
/// side, on the face it stands on now. A value left out keeps the
/// object's position along that direction: the distance from the eave, or
/// the place along the eave (on a hip or valley face, whose sides slant,
/// the side distances then change). It must stay on the same face with
/// each value given reached, else nothing changes and an error says why.
/// The object keeps its height above the roof (a dormer's eave
/// and a chimney's top follow the roof, [`follow_roof`]). Returns whether
/// it moved.
pub fn place(
    project: &mut Project,
    it: ItemRef,
    from_eave: Option<f64>,
    from_left: Option<f64>,
    from_right: Option<f64>,
) -> Result<bool, String> {
    if from_left.is_some() && from_right.is_some() {
        return Err("give from_left or from_right, not both".into());
    }
    for (name, v) in [
        ("from_eave", from_eave),
        ("from_left", from_left),
        ("from_right", from_right),
    ] {
        if let Some(v) = v
            && (!v.is_finite() || !(0.0..=MAX_DISTANCE).contains(&v))
        {
            return Err(format!(
                "{name} {v:.3} is out of range (0 to {MAX_DISTANCE} m from the edge of its roof face)"
            ));
        }
    }
    if matches!(it.kind, ItemKind::ManualLine | ItemKind::ManualFace) {
        return Err(
            "only dormers, chimneys, roof windows and openings are placed on a face".into(),
        );
    }
    let bare = bare_roof(project);
    let before = project.clone();
    let Some((now, k)) = locate(project, &bare, it) else {
        return Err("the object stands on no roof face, so it has no distances to set".into());
    };
    let face = &bare[now.part].faces[k];
    let (up, measured) = slope(face);
    let tan = exact_tan(face, measured);
    let along = Point2::new(up.y, -up.x);
    let cos = 1.0 / (1.0 + tan * tan).sqrt();
    // Up the slope first: on a hip face the width depends on the height.
    let dv = from_eave.map_or(0.0, |e| (e - now.from_eave) * cos);
    let mut moved = false;
    if dv != 0.0 {
        shift(project, &bare, it, Point2::new(up.x * dv, up.y * dv));
        moved = true;
    }
    // Every step must leave the object on the face it started on: past a
    // ridge, hip or pitch break the distances belong to another face, so the
    // field would show another number than the one typed, and a side step
    // measured there but applied along this face put objects off the roof
    // (review MATH-002). Then nothing changes and the call says why.
    let leave = |project: &mut Project, what: &str| -> Result<bool, String> {
        *project = before.clone();
        Err(format!(
            "the object would leave its roof face ({what}); give a smaller distance"
        ))
    };
    let on_face = |project: &Project| -> Option<Placement> {
        locate(project, &bare, it).and_then(|(p, k2)| (k2 == k && p.part == now.part).then_some(p))
    };
    if from_left.is_some() || from_right.is_some() {
        let Some(again) = on_face(project) else {
            return leave(project, "from the eave");
        };
        let du = match (from_left, from_right) {
            (Some(l), _) => l - again.from_left,
            (_, Some(r)) => again.from_right - r,
            _ => 0.0,
        };
        if du != 0.0 {
            shift(project, &bare, it, Point2::new(along.x * du, along.y * du));
            moved = true;
        }
    }
    let Some(done) = on_face(project) else {
        let what = if from_left.is_some() || from_right.is_some() {
            "from the side"
        } else {
            "from the eave"
        };
        return leave(project, what);
    };
    // Each value given is where the object now is (1 mm): a hip face, whose
    // width changes with the height, cannot reach every pair.
    let off = |want: Option<f64>, got: f64| want.is_some_and(|w| (w - got).abs() > 1e-3);
    if off(from_eave, done.from_eave)
        || off(from_left, done.from_left)
        || off(from_right, done.from_right)
    {
        return leave(project, "the distances do not fit on it");
    }
    // A side step must keep the object inside both sides of the face.
    if (from_left.is_some() || from_right.is_some())
        && (done.from_left < -1e-3 || done.from_right < -1e-3)
    {
        return leave(project, "past the other side");
    }
    Ok(moved)
}

/// Moves an object by `d` in plan without rounding (the distances are
/// exact), its heights following the roof.
fn shift(project: &mut Project, bare: &[RoofGeometry], it: ItemRef, d: Point2) {
    let mv = |p: &mut Point2| {
        p.x += d.x;
        p.y += d.y;
    };
    let mut from_to = None;
    match it.kind {
        ItemKind::Dormer => {
            if let Some(dm) = project.dormers.get_mut(it.index) {
                let a = dm.edges.first().map_or(dm.end, |e| e.start);
                let from = Point2::new((a.x + dm.end.x) / 2.0, (a.y + dm.end.y) / 2.0);
                dm.edges.iter_mut().for_each(|e| mv(&mut e.start));
                mv(&mut dm.end);
                if let Some(w) = dm.host_wall.as_mut() {
                    w.iter_mut().for_each(mv);
                }
                from_to = Some((from, Point2::new(from.x + d.x, from.y + d.y)));
            }
        }
        ItemKind::Chimney | ItemKind::RoofOpening => {
            let list = if it.kind == ItemKind::Chimney {
                &mut project.chimneys
            } else {
                &mut project.roof_openings
            };
            if let Some(c) = list.get_mut(it.index) {
                let n = c.corners.len().max(1) as f64;
                let from = Point2::new(
                    c.corners.iter().map(|p| p.x).sum::<f64>() / n,
                    c.corners.iter().map(|p| p.y).sum::<f64>() / n,
                );
                c.corners.iter_mut().for_each(mv);
                from_to = Some((from, Point2::new(from.x + d.x, from.y + d.y)));
            }
        }
        ItemKind::Window => {
            if let Some(w) = project.windows.get_mut(it.index) {
                w.x += d.x;
                w.y += d.y;
            }
        }
        ItemKind::ManualLine | ItemKind::ManualFace => {}
    }
    if let Some((from, to)) = from_to {
        follow_roof(project, &[(it, from, to)], |p| {
            super::probe::host_height_at(bare, p)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::examples;
    use crate::model::{Chimney, RoofWindow};

    /// A gable roof 10 m along x, 8 m deep, gables at x = 0 and x = 10,
    /// eaves along y = 0 and y = 8 with `ov` overhang.
    fn gable(ov: f64, pitch: f64) -> Project {
        let mut p = examples::find("gable").unwrap().project();
        p.dormers.clear();
        p.chimneys.clear();
        p.windows.clear();
        p.roof_openings.clear();
        let pts = [(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)];
        let mut e = p.parts[0].edges[0].clone();
        e.breaks.clear();
        e.half_hip = None;
        p.parts[0].edges = pts
            .iter()
            .enumerate()
            .map(|(i, &(x, y))| {
                let mut w = e.clone();
                w.start = Point2::new(x, y);
                w.overhang = ov;
                w.pitch_deg = if i % 2 == 1 { 90.0 } else { pitch };
                w
            })
            .collect();
        p
    }

    /// 0.01 mm: the core writes its points in single precision.
    fn near(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-5, "{a} vs {b}");
    }

    #[test]
    fn a_window_is_measured_from_the_eave_along_the_slope_and_from_the_verges() {
        // Closed form: the front face rises in +y from the eave at
        // y = -0.5; the verges stand 0.5 m out at x = -0.5 and 10.5.
        let mut p = gable(0.5, 40.0);
        p.windows.push(RoofWindow {
            x: 2.0,
            y: 1.5,
            width: 1.0,
            height: 1.4,
        });
        let it = ItemRef {
            kind: ItemKind::Window,
            index: 0,
        };
        let pl = placement(&p, it).unwrap();
        near(pl.pitch_deg, 40.0);
        near(pl.from_eave_plan, 2.0);
        near(pl.from_eave, 2.0 / 40f64.to_radians().cos());
        // Verge overhangs: DACAD gives a gable's verge the gable wall's
        // overhang (0.5 m here).
        near(pl.from_left, 2.5);
        near(pl.from_right, 10.5 - 3.0);
        // Moved to 1.2 m from the eave (along the slope) and 0.8 m from the
        // right verge.
        assert!(place(&mut p, it, Some(1.2), None, Some(0.8)).unwrap());
        let pl = placement(&p, it).unwrap();
        near(pl.from_eave, 1.2);
        near(pl.from_right, 0.8);
        near(p.windows[0].y, -0.5 + 1.2 * 40f64.to_radians().cos());
        near(p.windows[0].x, 10.5 - 0.8 - 1.0);
        // The same values again keep it where it is.
        let at = p.windows[0].clone();
        place(&mut p, it, Some(1.2), None, Some(0.8)).unwrap();
        near(p.windows[0].x, at.x);
        near(p.windows[0].y, at.y);
    }

    #[test]
    fn a_chimney_on_the_back_face_is_measured_on_that_face_and_keeps_its_height_above_it() {
        let mut p = gable(0.0, 45.0);
        // Back face: rises towards -y from the eave at y = 8.
        p.chimneys.push(Chimney {
            corners: vec![
                Point2::new(4.0, 6.0),
                Point2::new(4.6, 6.0),
                Point2::new(4.6, 6.6),
                Point2::new(4.0, 6.6),
            ],
            height: 9.0,
        });
        let it = ItemRef {
            kind: ItemKind::Chimney,
            index: 0,
        };
        let pl = placement(&p, it).unwrap();
        // Lowest point: y = 6.6, 1.4 m in plan from the eave at y = 8.
        near(pl.from_eave_plan, 1.4);
        near(pl.from_eave, 1.4 * 2f64.sqrt());
        // Looking up the back face, left is +x.
        near(pl.from_left, 10.0 - 4.6);
        near(pl.from_right, 4.0);
        assert!(
            place(
                &mut p,
                it,
                Some(1.4 * 2f64.sqrt() + 0.5 * 2f64.sqrt()),
                None,
                None
            )
            .unwrap()
        );
        // 0.5 m further up the slope in plan, 0.5 m higher on a 45° roof.
        near(p.chimneys[0].corners[0].y, 5.5);
        near(p.chimneys[0].height, 9.5);
    }

    #[test]
    fn a_hip_face_is_measured_to_the_hips_at_the_objects_height() {
        let mut p = examples::find("hip").unwrap().project();
        p.dormers.clear();
        p.chimneys.clear();
        p.windows.clear();
        p.roof_openings.clear();
        let bare = bare_roof(&p);
        // A window on the first wall's face, 1 m up from its eave in plan.
        let e = &p.parts[0].edges;
        let (a, b) = (e[0].start, e[1].start);
        let len = a.distance(b);
        let dir = Point2::new((b.x - a.x) / len, (b.y - a.y) / len);
        let inward = Point2::new(-dir.y, dir.x);
        let mid = Point2::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
        p.windows.push(RoofWindow {
            x: mid.x + inward.x * 1.0,
            y: mid.y + inward.y * 1.0,
            width: 1.0,
            height: 1.0,
        });
        let it = ItemRef {
            kind: ItemKind::Window,
            index: 0,
        };
        let pl = placement_in(&p, &bare, it).unwrap();
        // At its height the face is narrower than at the eave: the window
        // is closer to a hip than the plain distance along the eave.
        let ov = e[0].overhang;
        near(pl.from_eave_plan, 1.0 + ov);
        let pitch = e[0].pitch_deg.to_radians();
        let hip_pitch = e[1].pitch_deg.to_radians();
        // Each hip moves in by (rise / tan of the neighbouring pitch).
        let rise = (1.0 + ov) * pitch.tan();
        let half = len / 2.0 + e[1].overhang - rise / hip_pitch.tan();
        near(pl.from_left, half);
        near(pl.from_right, half - 1.0);
    }

    #[test]
    fn objects_off_the_roof_or_unknown_have_no_placement_and_are_refused() {
        let mut p = gable(0.5, 40.0);
        p.windows.push(RoofWindow {
            x: 30.0,
            y: 30.0,
            width: 1.0,
            height: 1.0,
        });
        let it = ItemRef {
            kind: ItemKind::Window,
            index: 0,
        };
        assert!(placement(&p, it).is_none());
        assert!(place(&mut p, it, Some(1.0), None, None).is_err());
        let gone = ItemRef {
            kind: ItemKind::Window,
            index: 7,
        };
        assert!(place(&mut p, gone, Some(1.0), None, None).is_err());
        p.windows[0].x = 2.0;
        p.windows[0].y = 2.0;
        let before = p.clone();
        for bad in [f64::NAN, f64::INFINITY, 1e9] {
            assert!(place(&mut p, it, Some(bad), None, None).is_err());
        }
        assert!(place(&mut p, it, None, Some(1.0), Some(1.0)).is_err());
        assert_eq!(p, before);
    }

    #[test]
    fn a_distance_past_the_ridge_or_the_verge_is_refused_and_changes_nothing() {
        // Gable 10 x 8 m, 0.5 m overhang, 40°: 5.874 m from eave to ridge,
        // the face 11 m wide (review MATH-002).
        let mut p = gable(0.5, 40.0);
        p.windows.push(RoofWindow {
            x: 2.0,
            y: 2.0,
            width: 1.0,
            height: 1.0,
        });
        let it = ItemRef {
            kind: ItemKind::Window,
            index: 0,
        };
        let before = p.clone();
        for (eave, left, right) in [
            (Some(5.5), None, None),      // over the ridge onto the back face
            (Some(150.0), None, None),    // cm typed for m
            (None, Some(-1.0), None),     // beyond the verge
            (None, Some(10.5), None),     // off the other verge
            (Some(5.5), Some(3.0), None), // the side step after leaving
            (None, None, Some(-0.2)),     // negative from the right
        ] {
            assert!(
                place(&mut p, it, eave, left, right).is_err(),
                "{eave:?} {left:?} {right:?}"
            );
            assert_eq!(p, before, "{eave:?} {left:?} {right:?}");
        }
        assert_eq!(place(&mut p, it, Some(2.0), None, Some(0.5)), Ok(true));
        let pl = placement(&p, it).unwrap();
        near(pl.from_eave, 2.0);
        near(pl.from_right, 0.5);
        near(pl.from_left, 9.5);
    }
}
