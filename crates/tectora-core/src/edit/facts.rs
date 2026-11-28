//! Read-only facts about each dormer, chimney, roof window and roof opening
//! that the object editor shows next to its fields: the roof face it stands
//! on and whether its "from eave" distance starts at the eave or at a pitch
//! break, the area it covers in plan and on the slope, a chimney's height
//! above the roof on its upper and lower side, a roof window's outline in
//! plan and a chimney's or opening's box (position, size, rotation).
//!
//! The MCP server gives these with every placement (`get_project`), so an
//! AI client reads the same facts a user reads in the editor (UI/MCP
//! parity). [no source: Tectora's own read-outs of the core's results; the
//! areas restate what the calculation deducts, `plan / cos(pitch)`.]

use serde::Serialize;

use super::openings::{OpeningBox, opening_box};
use super::position::{bare_roof, locate};
use super::probe::host_height_at;
use super::transform::{ItemKind, ItemRef};
use crate::geom::Point2;
use crate::model::Project;
use crate::openings::window_outline;
use crate::roof::RoofGeometry;

/// A chimney's top above the roof: on its upper side (the top minus the
/// highest roof point under it) and its lower side (minus the lowest).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct AboveRoof {
    pub upper: f64,
    pub lower: f64,
}

/// The facts of one object (see the module). Lengths in m, areas in m².
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ObjectFacts {
    /// Index of the host face among its part's faces of the roof without
    /// objects (as `calc_breakdown` lists a part's faces when the part has
    /// no objects).
    pub face: usize,
    /// Where "from eave" is measured from: `eave` (the lowest edge of the
    /// wall's roof) or `pitch_break` (a face above a pitch break).
    pub measured_from: &'static str,
    /// Plan area the object covers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_area: Option<f64>,
    /// The same area on the slope of its face: what the roof loses.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slope_area: Option<f64>,
    /// Chimneys only: how far the top stands above the roof.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub above_roof: Option<AboveRoof>,
    /// Chimneys only: the top is below the roof on its upper side.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub top_below_roof: bool,
    /// Roof windows only: the outline in plan (width level along the face,
    /// height up the slope), as the editor draws it.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub outline: Vec<Point2>,
    /// Chimneys and roof openings: the box the object bar edits.
    #[serde(rename = "box", skip_serializing_if = "Option::is_none")]
    pub frame: Option<OpeningBox>,
}

/// [`ObjectFacts`] of every object, by kind and index (`None` where an
/// object stands on no face).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Facts {
    pub dormers: Vec<Option<ObjectFacts>>,
    pub chimneys: Vec<Option<ObjectFacts>>,
    pub roof_openings: Vec<Option<ObjectFacts>>,
    pub windows: Vec<Option<ObjectFacts>>,
}

/// Shoelace plan area of a polygon.
pub fn plan_area(k: &[Point2]) -> f64 {
    let n = k.len();
    let mut a = 0.0;
    for i in 0..n {
        let (p, q) = (k[i], k[(i + 1) % n]);
        a += p.x * q.y - q.x * p.y;
    }
    (a / 2.0).abs()
}

fn finite(v: f64) -> Option<f64> {
    v.is_finite().then_some(v)
}

/// A chimney's height above the roof (see [`AboveRoof`]): the roof under
/// each corner, taken 1 mm inside towards the centre. `None` when a corner
/// is off the roof.
fn above_roof(bare: &[RoofGeometry], corners: &[Point2], top: f64) -> Option<AboveRoof> {
    let n = corners.len();
    if n < 3 || !top.is_finite() {
        return None;
    }
    let c = Point2::new(
        corners.iter().map(|p| p.x).sum::<f64>() / n as f64,
        corners.iter().map(|p| p.y).sum::<f64>() / n as f64,
    );
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for &p in corners {
        let d = (c.x - p.x).hypot(c.y - p.y);
        let k = if d > 1e-6 { 1e-3 / d } else { 0.0 };
        let z = host_height_at(
            bare,
            Point2::new(p.x + (c.x - p.x) * k, p.y + (c.y - p.y) * k),
        )?;
        if !z.is_finite() {
            return None;
        }
        lo = lo.min(z);
        hi = hi.max(z);
    }
    Some(AboveRoof {
        upper: finite(top - hi)?,
        lower: finite(top - lo)?,
    })
}

fn facts_of(project: &Project, bare: &[RoofGeometry], it: ItemRef) -> Option<ObjectFacts> {
    let (pl, face) = locate(project, bare, it)?;
    let faces = &bare.get(pl.part)?.faces;
    let low =
        |f: &crate::roof::RoofFace| f.polygon.iter().map(|q| q[2]).fold(f64::INFINITY, f64::min);
    let wall_low = faces
        .iter()
        .filter(|f| f.edge == pl.edge && f.polygon.len() >= 3)
        .map(low)
        .fold(f64::INFINITY, f64::min);
    let host_low = low(faces.get(face)?);
    let measured_from = if host_low - wall_low < 1e-3 {
        "eave"
    } else {
        "pitch_break"
    };
    let cos = pl.pitch_deg.to_radians().cos();
    let on_slope = |plan: f64| finite(if cos > 1e-9 { plan / cos } else { plan });
    let mut f = ObjectFacts {
        face,
        measured_from,
        plan_area: None,
        slope_area: None,
        above_roof: None,
        top_below_roof: false,
        outline: Vec::new(),
        frame: None,
    };
    match it.kind {
        ItemKind::Chimney => {
            let c = project.chimneys.get(it.index)?;
            let a = plan_area(&c.corners);
            f.plan_area = finite(a);
            f.slope_area = on_slope(a);
            f.above_roof = above_roof(bare, &c.corners, c.height);
            f.top_below_roof = f.above_roof.is_some_and(|h| h.upper < 0.0);
            f.frame = opening_box(&c.corners);
        }
        ItemKind::RoofOpening => {
            let o = project.roof_openings.get(it.index)?;
            let a = plan_area(&o.corners);
            f.plan_area = finite(a);
            f.slope_area = on_slope(a);
            f.frame = opening_box(&o.corners);
        }
        ItemKind::Window => {
            let w = project.windows.get(it.index)?;
            f.slope_area = finite(w.width * w.height);
            f.plan_area = finite(w.width * w.height * cos);
            f.outline = window_outline(bare, w);
        }
        ItemKind::Dormer | ItemKind::ManualLine | ItemKind::ManualFace => {}
    }
    Some(f)
}

/// The facts of every object of `project`, on one roof without objects.
pub fn facts(project: &Project) -> Facts {
    let any = !project.dormers.is_empty()
        || !project.chimneys.is_empty()
        || !project.roof_openings.is_empty()
        || !project.windows.is_empty();
    if !any {
        return Facts::default();
    }
    facts_on(project, &bare_roof(project))
}

/// [`facts`] on a given bare roof ([`bare_roof`] of the project).
pub fn facts_on(project: &Project, bare: &[RoofGeometry]) -> Facts {
    let all = |kind: ItemKind, n: usize| -> Vec<Option<ObjectFacts>> {
        (0..n)
            .map(|index| facts_of(project, bare, ItemRef { kind, index }))
            .collect()
    };
    Facts {
        dormers: all(ItemKind::Dormer, project.dormers.len()),
        chimneys: all(ItemKind::Chimney, project.chimneys.len()),
        roof_openings: all(ItemKind::RoofOpening, project.roof_openings.len()),
        windows: all(ItemKind::Window, project.windows.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Chimney, RoofWindow};

    fn gable() -> Project {
        // 12 x 8 gable, 40°, eave 3 m, overhang 0.5 (the example roof).
        let mut p = crate::examples::find("gable")
            .expect("gable example")
            .project();
        p.chimneys.push(Chimney {
            corners: vec![
                Point2::new(5.0, 1.0),
                Point2::new(5.6, 1.0),
                Point2::new(5.6, 1.6),
                Point2::new(5.0, 1.6),
            ],
            height: 9.0,
        });
        p.roof_openings.push(Chimney {
            corners: vec![
                Point2::new(2.0, 0.0),
                Point2::new(3.0, 0.0),
                Point2::new(3.0, 1.0),
                Point2::new(2.0, 1.0),
            ],
            height: 0.0,
        });
        p.windows.push(RoofWindow {
            x: 8.0,
            y: 1.0,
            width: 0.78,
            height: 1.18,
        });
        p
    }

    #[test]
    fn a_chimney_on_a_40_degree_face_stands_above_the_roof_by_its_top_minus_the_slope() {
        let p = gable();
        let f = facts(&p);
        let c = f.chimneys[0].as_ref().expect("chimney on the roof");
        assert_eq!(c.measured_from, "eave");
        let tan = 40f64.to_radians().tan();
        // The eave height is measured at the overhang line (0.5 m outside
        // the wall), so the lower face stands at z(y) = 3 + (y + 0.5)·tan40.
        let z = |y: f64| 3.0 + (y + 0.5) * tan;
        let a = c.above_roof.expect("above roof");
        assert!((a.upper - (9.0 - z(1.6))).abs() < 0.01, "{a:?}");
        assert!((a.lower - (9.0 - z(1.0))).abs() < 0.01, "{a:?}");
        assert!(!c.top_below_roof);
        let plan = 0.36;
        assert!((c.plan_area.unwrap() - plan).abs() < 1e-9);
        assert!((c.slope_area.unwrap() - plan / 40f64.to_radians().cos()).abs() < 1e-6);
        let b = c.frame.expect("box");
        assert!((b.width - 0.6).abs() < 1e-9 && (b.depth - 0.6).abs() < 1e-9);
    }

    #[test]
    fn a_chimney_top_below_the_roof_is_flagged() {
        let mut p = gable();
        p.chimneys[0].height = 3.5;
        let c = facts(&p).chimneys[0].clone().unwrap();
        assert!(c.top_below_roof, "{c:?}");
        assert!(c.above_roof.unwrap().upper < 0.0);
    }

    #[test]
    fn a_window_has_its_slope_area_and_plan_outline_and_an_opening_its_box() {
        let p = gable();
        let f = facts(&p);
        let w = f.windows[0].as_ref().expect("window on the roof");
        assert!((w.slope_area.unwrap() - 0.78 * 1.18).abs() < 1e-9);
        assert_eq!(w.outline.len(), 4);
        // The window's height up the slope is shortened by cos(40°) in plan.
        let h = (w.outline[3].y - w.outline[0].y).abs();
        assert!((h - 1.18 * 40f64.to_radians().cos()).abs() < 1e-6, "{h}");
        let o = f.roof_openings[0].as_ref().expect("opening on the roof");
        assert!((o.plan_area.unwrap() - 1.0).abs() < 1e-9);
        assert!(o.frame.is_some());
        assert!(o.above_roof.is_none() && o.outline.is_empty());
    }

    #[test]
    fn an_object_above_a_pitch_break_is_measured_from_the_break() {
        let mut p = gable();
        p.parts[0].edges[0].breaks = vec![crate::model::PitchBreak {
            height: 4.0,
            pitch_deg: 25.0,
        }];
        // Move the window high up the lower face, above the break at 4 m.
        p.windows[0].y = 3.0;
        let f = facts(&p);
        let w = f.windows[0].as_ref().expect("window on the roof");
        assert_eq!(w.measured_from, "pitch_break", "{w:?}");
        let o = f.roof_openings[0]
            .as_ref()
            .expect("opening below the break");
        assert_eq!(o.measured_from, "eave");
    }

    #[test]
    fn no_objects_no_facts_and_an_object_off_the_roof_has_none() {
        let mut p = crate::examples::find("gable").unwrap().project();
        assert_eq!(facts(&p), Facts::default());
        p.chimneys.push(Chimney {
            corners: vec![
                Point2::new(50.0, 50.0),
                Point2::new(51.0, 50.0),
                Point2::new(51.0, 51.0),
            ],
            height: 1.0,
        });
        assert_eq!(facts(&p).chimneys, vec![None]);
    }
}
