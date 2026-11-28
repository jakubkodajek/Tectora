//! Curved outlines: a rounded corner and a wall bent into an arc, both as
//! short straight walls along the arc. DACAD has no curved wall; its help
//! takes a round building as a polygon of walls (DACAD.HLP via the
//! 2026-11-08 gap analysis), and its round roof (Runddach) splits an arc
//! into segments of 11.25°, or of 15° where such a segment would be
//! shorter than 0.4 m ([`super::round`], DACAD runs RD1-RD4, RGa-RGs). The
//! arcs here use the same steps, so the roof core sees what DACAD would be
//! given for such a building: each new wall carries the roof definition of
//! the wall it replaces.

// `!(x > 0.0)` and the like are deliberate: they also refuse NaN.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use std::f64::consts::PI;

use super::round_to;
use crate::geom::Point2;
use crate::model::{EdgeDef, RoofPart};

/// Most walls one arc may become.
pub const MAX_ARC_SEGMENTS: usize = 64;

/// DACAD's segment angle and the coarser one for short segments (as
/// [`super::round`]).
const SEGMENT_ARC: f64 = 11.25;
const SHORT_SEGMENT_ARC: f64 = 15.0;
const SHORT_CHORD: f64 = 0.4;

/// Walls for an arc of `sweep` radians on radius `r`: DACAD's 11.25° steps,
/// 15° when a step's chord would be under 0.4 m, at least 2.
pub fn default_segments(r: f64, sweep: f64) -> usize {
    let chord = |deg: f64| 2.0 * r * (deg.to_radians() / 2.0).sin();
    let step = if chord(SEGMENT_ARC) < SHORT_CHORD {
        SHORT_SEGMENT_ARC
    } else {
        SEGMENT_ARC
    };
    ((sweep.abs().to_degrees() / step).ceil() as usize).clamp(2, MAX_ARC_SEGMENTS)
}

/// The shortest wall an arc may have, m. The roof core merges points
/// within 2 cm and takes walls under about 10 cm with visible error (audit
/// MATH-003: at r = 0.1 m the roof was 0.49 m² too large, no warning).
pub const MIN_ARC_WALL: f64 = 0.1;

/// The walls of an arc of `sweep` radians on radius `r`: the given count,
/// or DACAD's steps with fewer walls where they would be shorter than
/// [`MIN_ARC_WALL`]; refused when even that leaves a wall too short.
fn segments_or_default(segments: Option<usize>, r: f64, sweep: f64) -> Result<usize, String> {
    let chord = |n: usize| 2.0 * r * (sweep.abs() / (2.0 * n as f64)).sin();
    let n = match segments {
        None => {
            let mut n = default_segments(r, sweep);
            while n > 2 && chord(n) < MIN_ARC_WALL {
                n -= 1;
            }
            n
        }
        Some(n) if (2..=MAX_ARC_SEGMENTS).contains(&n) => n,
        Some(n) => return Err(format!("an arc has 2 to {MAX_ARC_SEGMENTS} walls, not {n}")),
    };
    if chord(n) < MIN_ARC_WALL {
        return Err(format!(
            "the arc's walls would be shorter than {MIN_ARC_WALL} m: give a larger radius or fewer walls"
        ));
    }
    Ok(n)
}

fn p3(p: Point2) -> Point2 {
    Point2::new(round_to(p.x, 4), round_to(p.y, 4))
}

/// Replaces wall `i` of `part` (counter-clockwise outline) by an arc whose
/// middle stands `bulge` m off the straight wall, outward for a positive
/// bulge, inward for a negative one; at most half the wall's length (a
/// semicircle). The new walls carry wall `i`'s roof definition. Returns how
/// many walls the arc has.
pub fn bend_wall(
    part: &mut RoofPart,
    i: usize,
    bulge: f64,
    segments: Option<usize>,
) -> Result<usize, String> {
    let n = part.edges.len();
    if i >= n {
        return Err(format!("wall {i} does not exist"));
    }
    let a = part.edges[i].start;
    let b = part.edges[(i + 1) % n].start;
    let l = a.distance(b);
    if !(l > 0.01) {
        return Err("the wall has no length".into());
    }
    if !(bulge.is_finite() && bulge.abs() >= 0.005 && bulge.abs() <= l / 2.0 + 1e-9) {
        return Err(format!(
            "the bend must be at least 0.005 m and at most half the wall ({:.3} m)",
            l / 2.0
        ));
    }
    let ccw = crate::geom::signed_area(&part.outline()) >= 0.0;
    // Outward is to the right of a wall of a counter-clockwise outline.
    let (ux, uy) = ((b.x - a.x) / l, (b.y - a.y) / l);
    let (rx, ry) = if ccw { (uy, -ux) } else { (-uy, ux) };
    let s = bulge.abs().min(l / 2.0);
    let r = (l * l / 4.0 + s * s) / (2.0 * s);
    let sweep = 4.0 * (2.0 * s / l).atan();
    let segs = segments_or_default(segments, r, sweep)?;
    let mid = Point2::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
    let sign = bulge.signum();
    let apex = Point2::new(mid.x + rx * s * sign, mid.y + ry * s * sign);
    let c = Point2::new(apex.x - rx * r * sign, apex.y - ry * r * sign);
    let a0 = (a.y - c.y).atan2(a.x - c.x);
    let am = (apex.y - c.y).atan2(apex.x - c.x);
    let dir = if ((a0 + sweep / 2.0) - am)
        .rem_euclid(2.0 * PI)
        .min(2.0 * PI - ((a0 + sweep / 2.0) - am).rem_euclid(2.0 * PI))
        < 1e-6
    {
        1.0
    } else {
        -1.0
    };
    let template = part.edges[i].clone();
    let new: Vec<EdgeDef> = (1..segs)
        .map(|k| {
            let t = a0 + dir * sweep * k as f64 / segs as f64;
            EdgeDef {
                start: p3(Point2::new(c.x + r * t.cos(), c.y + r * t.sin())),
                source: None,
                direction_rad: None,
                ..template.clone()
            }
        })
        .collect();
    part.edges[i].source = None;
    part.edges.splice(i + 1..i + 1, new);
    Ok(segs)
}

/// Rounds the corner at the start of wall `v` of `part` with radius
/// `radius`: both walls are shortened by the tangent length and an arc of
/// walls joins them. The arc walls carry the roof definition of the wall
/// before the corner, or of wall `v` when that one is a gable and wall `v`
/// is not. Returns how many walls the arc has.
pub fn round_corner(
    part: &mut RoofPart,
    v: usize,
    radius: f64,
    segments: Option<usize>,
) -> Result<usize, String> {
    let n = part.edges.len();
    if v >= n || n < 3 {
        return Err(format!("corner {v} does not exist"));
    }
    if !(radius.is_finite() && radius >= 0.01) {
        return Err("the radius must be at least 0.01 m".into());
    }
    let prev = part.edges[(v + n - 1) % n].start;
    let p = part.edges[v].start;
    let next = part.edges[(v + 1) % n].start;
    let (l1, l2) = (p.distance(prev), p.distance(next));
    if !(l1 > 0.01 && l2 > 0.01) {
        return Err("a wall at the corner has no length".into());
    }
    let e1 = Point2::new((prev.x - p.x) / l1, (prev.y - p.y) / l1);
    let e2 = Point2::new((next.x - p.x) / l2, (next.y - p.y) / l2);
    let theta = (e1.x * e2.x + e1.y * e2.y).clamp(-1.0, 1.0).acos();
    if !(theta > 1e-3 && theta < PI - 1e-3) {
        return Err("the walls at the corner run straight on".into());
    }
    let half = theta / 2.0;
    let t = radius / half.tan();
    // Each wall keeps at least MIN_ARC_WALL, or ends exactly at the arc
    // is not allowed either: a zero-length wall is a problem.
    if t > l1 - MIN_ARC_WALL || t > l2 - MIN_ARC_WALL {
        return Err(format!(
            "the radius is too large for the walls at the corner (at most {:.3} m)",
            ((l1.min(l2) - MIN_ARC_WALL).max(0.0)) * half.tan()
        ));
    }
    let t1 = Point2::new(p.x + e1.x * t, p.y + e1.y * t);
    let t2 = Point2::new(p.x + e2.x * t, p.y + e2.y * t);
    let bis = Point2::new(e1.x + e2.x, e1.y + e2.y);
    let bl = bis.x.hypot(bis.y);
    let c = Point2::new(
        p.x + bis.x / bl * radius / half.sin(),
        p.y + bis.y / bl * radius / half.sin(),
    );
    let sweep = PI - theta;
    let segs = segments_or_default(segments, radius, sweep)?;
    let a0 = (t1.y - c.y).atan2(t1.x - c.x);
    let a1 = (t2.y - c.y).atan2(t2.x - c.x);
    // The short way round from t1 to t2.
    let mut d = a1 - a0;
    while d > PI {
        d -= 2.0 * PI;
    }
    while d < -PI {
        d += 2.0 * PI;
    }
    let before = part.edges[(v + n - 1) % n].clone();
    let after = part.edges[v].clone();
    let template = if before.is_gable() && !after.is_gable() {
        after.clone()
    } else {
        before
    };
    let mut arc: Vec<EdgeDef> = Vec::with_capacity(segs + 1);
    // The arc starts at t1 (where the wall before now ends) and its last
    // wall ends at t2, where wall v now starts.
    for k in 0..segs {
        let a = a0 + d * k as f64 / segs as f64;
        let q = if k == 0 {
            t1
        } else {
            Point2::new(c.x + radius * a.cos(), c.y + radius * a.sin())
        };
        arc.push(EdgeDef {
            start: p3(q),
            source: None,
            direction_rad: None,
            ..template.clone()
        });
    }
    let mut moved = after;
    moved.start = p3(t2);
    moved.source = None;
    part.edges[v] = moved;
    part.edges.splice(v..v, arc);
    if let Some(e) = part.edges.get_mut((v + n - 1 + segs) % (n + segs)) {
        e.source = None;
    }
    Ok(segs)
}

/// Why an arc made an outline invalid: the first problem of a building in
/// `after` that `before` did not have (an outline crossing itself, a wall
/// too short); `None` when there is none. The editor's command and MCP
/// `edit_walls` both refuse such an arc (audit ARCH-005).
pub fn outline_spoiled(before: &crate::Project, after: &crate::Project) -> Option<String> {
    let had: std::collections::HashSet<_> = before
        .problems()
        .into_iter()
        .map(|p| (p.item, p.index, p.code))
        .collect();
    after
        .problems()
        .into_iter()
        .find(|p| {
            p.item == crate::model::ProblemItem::Part && !had.contains(&(p.item, p.index, p.code))
        })
        .map(|p| {
            format!(
                "the arc would make the outline invalid: {}",
                after.describe(&p)
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::plain_edge;

    fn square(s: f64) -> RoofPart {
        let pts = [(0.0, 0.0), (s, 0.0), (s, s), (0.0, s)];
        RoofPart::new(
            "A",
            pts.iter()
                .map(|&(x, y)| plain_edge(Point2::new(x, y), 30.0, 3.0, 0.5))
                .collect(),
        )
    }

    #[test]
    fn a_rounded_corner_is_tangent_to_both_walls_and_keeps_the_radius() {
        let mut p = square(10.0);
        let segs = round_corner(&mut p, 2, 3.0, None).unwrap();
        // 90° at 11.25° steps (chords of 0.59 m): 8 walls.
        assert_eq!(segs, 8);
        assert_eq!(p.edges.len(), 4 + 8);
        // The corner (10, 10) is gone; the arc runs from (10, 7) to (7, 10)
        // around (7, 7).
        assert!(
            p.edges
                .iter()
                .all(|e| e.start.distance(Point2::new(10.0, 10.0)) > 0.5)
        );
        let arc: Vec<Point2> = p.edges[2..2 + 8].iter().map(|e| e.start).collect();
        assert!(arc[0].approx_eq(Point2::new(10.0, 7.0), 1e-4));
        for q in &arc {
            assert!((q.distance(Point2::new(7.0, 7.0)) - 3.0).abs() < 1e-3);
        }
        assert!(p.edges[10].start.approx_eq(Point2::new(7.0, 10.0), 1e-4));
        // Plan area: the square less the 3 × 3 corner plus the quarter
        // circle as 8 triangles of 11.25°.
        let area = crate::geom::signed_area(&p.outline());
        let sector = 8.0 * 0.5 * 9.0 * (PI / 16.0).sin();
        assert!((area - (100.0 - 9.0 + sector)).abs() < 1e-3, "{area}");
        let mut prj = crate::Project::new("t");
        prj.parts.push(p);
        assert!(prj.problems().is_empty(), "{:?}", prj.problems());
    }

    #[test]
    fn a_small_radius_takes_dacads_coarser_steps() {
        // Radius 2: an 11.25° chord would be 0.39 m, under 0.4 m, so 15°.
        let mut p = square(10.0);
        assert_eq!(round_corner(&mut p, 2, 2.0, None), Ok(6));
    }

    #[test]
    fn a_bent_wall_bulges_outward_by_the_given_amount() {
        let mut p = square(10.0);
        let segs = bend_wall(&mut p, 0, 1.0, Some(6)).unwrap();
        assert_eq!(segs, 6);
        assert_eq!(p.edges.len(), 4 + 5);
        // Wall 0 runs from (0, 0) to (10, 0); outward is -y.
        let mid = p.edges[3].start;
        assert!(mid.approx_eq(Point2::new(5.0, -1.0), 1e-4), "{mid:?}");
        assert!(p.edges[1..6].iter().all(|e| e.start.y < 0.0));
        let mut inward = square(10.0);
        bend_wall(&mut inward, 0, -1.0, Some(6)).unwrap();
        assert!(inward.edges[3].start.approx_eq(Point2::new(5.0, 1.0), 1e-4));
    }

    #[test]
    fn arcs_refuse_what_does_not_fit() {
        let mut p = square(10.0);
        assert!(round_corner(&mut p, 1, 10.5, None).is_err());
        assert!(round_corner(&mut p, 7, 1.0, None).is_err());
        assert!(bend_wall(&mut p, 0, 5.5, None).is_err());
        assert!(bend_wall(&mut p, 0, 1.0, Some(1)).is_err());
        assert!(bend_wall(&mut p, 0, f64::NAN, None).is_err());
        assert_eq!(p, square(10.0));
    }

    #[test]
    fn a_rounded_building_still_calculates() {
        let mut p = square(10.0);
        for v in [3, 2, 1, 0] {
            round_corner(&mut p, v, 2.0, None).unwrap();
        }
        let mut prj = crate::Project::new("t");
        prj.parts.push(p);
        let geos = crate::openings::generate_project(&prj);
        assert!(geos[0].error.is_none(), "{:?}", geos[0].error);
        let a: f64 = geos[0].faces.iter().map(|f| f.area).sum();
        // Plan with the overhang, over cos 30°: a little under the square's
        // (11 × 11) / cos 30° = 139.7 m².
        assert!(a > 130.0 && a < 139.7, "{a}");
    }

    #[test]
    fn an_arc_never_makes_walls_the_core_cannot_take() {
        // Audit MATH-003 / ARCH-005: r = 0.012 saved five zero-length walls
        // over MCP; r = 0.05 and 0.1 gave wrong areas.
        for r in [0.012, 0.05, 0.1, 0.2] {
            let mut part = square(10.0);
            match round_corner(&mut part, 1, r, None) {
                Ok(_) => {
                    let pts = part.outline();
                    let n = pts.len();
                    for k in 0..n {
                        let l = pts[k].distance(pts[(k + 1) % n]);
                        assert!(l >= MIN_ARC_WALL - 1e-3, "r {r}: wall {k} is {l} m");
                    }
                }
                Err(e) => assert!(e.contains("shorter than"), "r {r}: {e}"),
            }
        }
        let mut part = square(10.0);
        assert!(round_corner(&mut part, 1, 0.012, None).is_err());
        assert!(round_corner(&mut part, 1, 1.0, Some(64)).is_err());
        assert!(bend_wall(&mut square(0.12), 0, 0.06, None).is_err());
    }
}
