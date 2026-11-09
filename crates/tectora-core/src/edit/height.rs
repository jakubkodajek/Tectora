//! The height of a building (a part) in one change: the editor's "Building
//! height" field and the MCP tool `set_part_height`.
//!
//! DACAD has no height of a roof model of its own: its core reads a wall's
//! start point in plan (the RD2 `W` record has x and y only) and every
//! height from the wall's roof records (`D` and `K`: eave height, pitch
//! break and half-hip heights, all above ground). Two buildings of
//! different heights are two models whose walls have different eave
//! heights, and the Modellverschnitt joins them by those heights. So the
//! building's height is the eave height of all its walls, and this
//! operation is pure input: it writes the walls' heights and calculates
//! nothing.
//!
//! [no source: an editor convenience.] Every wall gets the new eave
//! height; its pitch breaks and its half-hip move with it by the same
//! amount, so the roof keeps its shape. The dormers, chimneys and
//! hand-drawn lines on the building (as the transform operations pick
//! them: their centre inside the outline) move by the building's shift:
//! the new height less the lowest eave height the walls had.

use super::round_to;
use super::transform::inside_outline;
use crate::geom::Point2;
use crate::model::{EdgeDef, Project};

/// Lowest and highest eave height a building can be given, m. A typing
/// error (cm for m) is refused instead of written.
pub const MIN_PART_HEIGHT: f64 = 0.0;
pub const MAX_PART_HEIGHT: f64 = 1000.0;

/// The eave height all walls of the part share, or `None` when they differ
/// (the editor shows the field as mixed) or the part has no walls.
pub fn part_height(project: &Project, part: usize) -> Option<f64> {
    let edges = &project.parts.get(part)?.edges;
    let first = edges.first()?.eave_height;
    edges
        .iter()
        .all(|e| e.eave_height == first)
        .then_some(first)
}

fn shift(e: &mut EdgeDef, d: f64) {
    if d == 0.0 {
        return;
    }
    e.eave_height = round_to(e.eave_height + d, 3);
    for b in &mut e.breaks {
        b.height = round_to(b.height + d, 3);
    }
    if let Some(h) = &mut e.half_hip {
        h.height = round_to(h.height + d, 3);
    }
}

fn centre(pts: impl IntoIterator<Item = Point2>) -> Point2 {
    let (mut x, mut y, mut n) = (0.0, 0.0, 0usize);
    for p in pts {
        x += p.x;
        y += p.y;
        n += 1;
    }
    let n = n.max(1) as f64;
    Point2::new(x / n, y / n)
}

/// Sets the eave height of every wall of part `part` to `eave_height` (see
/// the module). Returns whether anything changed; an unknown part or a
/// height outside [`MIN_PART_HEIGHT`]..=[`MAX_PART_HEIGHT`] is refused and
/// changes nothing.
pub fn set_part_height(
    project: &mut Project,
    part: usize,
    eave_height: f64,
) -> Result<bool, String> {
    if !eave_height.is_finite() || !(MIN_PART_HEIGHT..=MAX_PART_HEIGHT).contains(&eave_height) {
        return Err(format!(
            "eave height {eave_height} is out of range ({MIN_PART_HEIGHT} to {MAX_PART_HEIGHT} m)"
        ));
    }
    let Some(p) = project.parts.get(part) else {
        return Err(format!("part {part} does not exist"));
    };
    if p.edges.is_empty() {
        return Err(format!("part {part} has no walls"));
    }
    let lowest = p
        .edges
        .iter()
        .map(|e| e.eave_height)
        .fold(f64::INFINITY, f64::min);
    let d = round_to(eave_height - lowest, 3);
    let outline = p.clone();
    let on = |q: Point2| inside_outline(&outline, q);
    let mut changed = false;
    for e in &mut project.parts[part].edges {
        let before = e.clone();
        shift(e, eave_height - e.eave_height);
        // Exactly the typed value, whatever the rounding of the shift.
        e.eave_height = eave_height;
        changed |= *e != before;
    }
    if d == 0.0 {
        return Ok(changed);
    }
    for dm in &mut project.dormers {
        if on(centre(dm.edges.iter().map(|e| e.start).chain([dm.end]))) {
            dm.edges.iter_mut().for_each(|e| shift(e, d));
            changed = true;
        }
    }
    for c in &mut project.chimneys {
        if on(centre(c.corners.iter().copied())) {
            c.height = round_to(c.height + d, 3);
            changed = true;
        }
    }
    for l in &mut project.manual_lines {
        if on(Point2::new(
            (l.a[0] + l.b[0]) / 2.0,
            (l.a[1] + l.b[1]) / 2.0,
        )) {
            for q in [&mut l.a, &mut l.b] {
                q[2] = round_to(q[2] + d, 3);
            }
            changed = true;
        }
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::examples;
    use crate::model::{Chimney, HalfHip, PitchBreak};

    fn two_buildings() -> Project {
        let mut p = examples::find("hip").unwrap().project();
        let mut wing = p.parts[0].clone();
        wing.name = "Wing".into();
        for e in &mut wing.edges {
            e.start.x += 6.0;
            e.start.y += 3.0;
        }
        p.parts.push(wing);
        p
    }

    #[test]
    fn sets_every_wall_and_moves_breaks_half_hips_and_objects_with_it() {
        let mut p = two_buildings();
        let base = p.parts[0].edges[0].eave_height;
        p.parts[1].edges[0].breaks = vec![PitchBreak {
            height: base + 1.5,
            pitch_deg: 20.0,
        }];
        p.parts[1].edges[1].half_hip = Some(HalfHip {
            height: base + 2.0,
            pitch_deg: 50.0,
        });
        let c = p.parts[1]
            .edges
            .iter()
            .map(|e| e.start)
            .fold(Point2::new(0.0, 0.0), |a, b| {
                Point2::new(a.x + b.x / 4.0, a.y + b.y / 4.0)
            });
        p.chimneys.push(Chimney {
            corners: vec![
                c,
                Point2::new(c.x + 0.5, c.y),
                Point2::new(c.x + 0.5, c.y + 0.5),
            ],
            height: 9.0,
        });
        let first = p.parts[0].clone();
        assert!(set_part_height(&mut p, 1, base - 1.0).unwrap());
        assert!(p.parts[1].edges.iter().all(|e| e.eave_height == base - 1.0));
        assert_eq!(
            p.parts[1].edges[0].breaks[0].height,
            round_to(base + 0.5, 3)
        );
        assert_eq!(
            p.parts[1].edges[1].half_hip.as_ref().unwrap().height,
            round_to(base + 1.0, 3)
        );
        assert_eq!(p.chimneys[0].height, 8.0);
        assert_eq!(p.parts[0], first, "the other building stays");
        assert_eq!(part_height(&p, 1), Some(base - 1.0));
        // The same height again changes nothing.
        assert!(!set_part_height(&mut p, 1, base - 1.0).unwrap());
    }

    #[test]
    fn a_lower_building_changes_how_both_roofs_are_joined() {
        let mut p = two_buildings();
        let before = crate::engine::generate(&p).unwrap();
        assert!(set_part_height(&mut p, 1, 1.5).unwrap());
        let after = crate::engine::generate(&p).unwrap();
        let area = |g: &crate::roof::RoofGeometry| g.faces.iter().map(|f| f.area).sum::<f64>();
        for k in 0..2 {
            assert!(
                (area(&before[k]) - area(&after[k])).abs() > 0.01,
                "part {k}: {} vs {}",
                area(&before[k]),
                area(&after[k])
            );
        }
    }

    #[test]
    fn mixed_walls_report_no_common_height_and_all_take_the_new_one() {
        let mut p = two_buildings();
        p.parts[0].edges[2].eave_height += 0.4;
        assert_eq!(part_height(&p, 0), None);
        assert!(set_part_height(&mut p, 0, 4.0).unwrap());
        assert_eq!(part_height(&p, 0), Some(4.0));
    }

    #[test]
    fn refuses_bad_input_without_changing_anything() {
        let mut p = two_buildings();
        let before = p.clone();
        for h in [f64::NAN, f64::INFINITY, -0.5, 1000.5, 1e300] {
            assert!(set_part_height(&mut p, 0, h).is_err(), "{h}");
        }
        assert!(set_part_height(&mut p, 7, 3.0).is_err());
        assert_eq!(p, before);
    }
}
