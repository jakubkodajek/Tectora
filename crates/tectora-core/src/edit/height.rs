//! The height of a building (a part): the editor's "Building height"
//! field and the MCP tool `set_part_height`.
//!
//! DACAD has no height of a roof model of its own: its core reads a wall's
//! start point in plan (the RD2 `W` record has x and y only) and every
//! height from the wall's roof records (`D` and `K`: eave height, pitch
//! break and half-hip heights, all above ground). Two buildings of
//! different heights are two models whose walls have different eave
//! heights, and the Modellverschnitt joins them by those heights.
//!
//! The building's height is therefore Tectora's own value
//! ([`RoofPart::height`], the top of the walls): it changes no roof figure,
//! and a wall's eave may be lower than it (DACAD measures the eave height
//! at the overhang line, `roof::generate_mode`, so an eave with an overhang
//! sits below the wall top) or higher. Jakub (2026-11-14): "the building
//! height does not mean the eave height; the user can set an eave lower
//! than the building".
//!
//! [no source: an editor convenience.] Raising or lowering the building
//! carries its roof along: every wall's eave height, pitch breaks and
//! half-hip move by the building's shift, so each wall keeps its own eave
//! below or above the wall top. The dormers, chimneys and hand-drawn lines
//! on the building (as the transform operations pick them: their centre
//! inside the outline) move by the same shift.
//! [`set_part_eave_height`] is the other way round: every wall at one eave
//! height, the building height unchanged.

use super::round_to;
use super::transform::inside_outline;
use crate::geom::Point2;
use crate::model::{EdgeDef, Project, RoofPart};

/// Lowest and highest building or eave height a building can be given, m.
/// A typing error (cm for m) is refused instead of written.
pub const MIN_PART_HEIGHT: f64 = 0.0;
pub const MAX_PART_HEIGHT: f64 = 1000.0;

/// The height of building `part` ([`RoofPart::building_height`]: its own
/// height, else its lowest eave); `None` for an unknown part or one
/// without walls.
pub fn part_height(project: &Project, part: usize) -> Option<f64> {
    project.parts.get(part)?.building_height()
}

/// The eave height all walls of the part share, or `None` when they differ
/// or the part has no walls.
pub fn common_eave_height(project: &Project, part: usize) -> Option<f64> {
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

fn check(v: f64, what: &str) -> Result<(), String> {
    if !v.is_finite() || !(MIN_PART_HEIGHT..=MAX_PART_HEIGHT).contains(&v) {
        return Err(format!(
            "{what} {v} is out of range ({MIN_PART_HEIGHT} to {MAX_PART_HEIGHT} m)"
        ));
    }
    Ok(())
}

fn walls(project: &Project, part: usize) -> Result<&RoofPart, String> {
    let Some(p) = project.parts.get(part) else {
        return Err(format!("part {part} does not exist"));
    };
    if p.edges.is_empty() {
        return Err(format!("part {part} has no walls"));
    }
    Ok(p)
}

/// Moves the dormers, chimneys and hand-drawn lines on part `part` (centre
/// inside its outline) up by `d`. Returns whether any moved.
fn move_objects(project: &mut Project, part: usize, d: f64) -> bool {
    if d == 0.0 {
        return false;
    }
    let outline = project.parts[part].clone();
    let on = |q: Point2| inside_outline(&outline, q);
    let mut changed = false;
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
    changed
}

/// Sets the height of building `part` to `height` and carries its roof
/// along (see the module): every wall's eave, breaks and half-hip and the
/// objects on the building move by the change of the building height.
/// Returns whether anything changed; an unknown part or a height outside
/// [`MIN_PART_HEIGHT`]..=[`MAX_PART_HEIGHT`] is refused and changes nothing.
pub fn set_part_height(project: &mut Project, part: usize, height: f64) -> Result<bool, String> {
    check(height, "building height")?;
    let p = walls(project, part)?;
    let old = p.building_height().unwrap_or(height);
    // A height from the file out of range (a damaged or hand-edited job)
    // would move every eave by an absurd amount (review ARCH-003).
    check(old, "the building's current height")?;
    let d = round_to(height - old, 3);
    let changed = p.height != Some(height) || d != 0.0;
    let mut next = project.clone();
    next.parts[part].height = Some(height);
    if d != 0.0 {
        for e in &mut next.parts[part].edges {
            shift(e, d);
        }
        // Every eave, break and half-hip stays above ground: an eave lower
        // than the building would else go below it (review MATH-003).
        let lowest = next.parts[part]
            .edges
            .iter()
            .flat_map(|e| {
                std::iter::once(e.eave_height)
                    .chain(e.breaks.iter().map(|b| b.height))
                    .chain(e.half_hip.iter().map(|h| h.height))
            })
            .fold(f64::INFINITY, f64::min);
        if lowest < 0.0 {
            return Err(format!(
                "at a building height of {height:.3} m an eave would be {lowest:.3} m, below the ground"
            ));
        }
        move_objects(&mut next, part, d);
    }
    *project = next;
    Ok(changed)
}

/// Sets the eave height of every wall of part `part` to `eave_height`; the
/// walls' pitch breaks and half-hips move with them, and so do the objects
/// on the building, by the new eave less the lowest eave the walls had.
/// The building height stays as it is (a part without its own height keeps
/// the one it showed, its lowest eave). Returns whether anything changed;
/// bad input is refused and changes nothing.
pub fn set_part_eave_height(
    project: &mut Project,
    part: usize,
    eave_height: f64,
) -> Result<bool, String> {
    check(eave_height, "eave height")?;
    let p = walls(project, part)?;
    let lowest = p
        .edges
        .iter()
        .map(|e| e.eave_height)
        .fold(f64::INFINITY, f64::min);
    let keep = p.building_height();
    let d = round_to(eave_height - lowest, 3);
    let mut changed = false;
    let pm = &mut project.parts[part];
    if pm.height.is_none() && keep.is_some() && d != 0.0 {
        pm.height = keep;
        changed = true;
    }
    for e in &mut pm.edges {
        let before = e.clone();
        shift(e, eave_height - e.eave_height);
        // Exactly the typed value, whatever the rounding of the shift.
        e.eave_height = eave_height;
        changed |= *e != before;
    }
    changed |= move_objects(project, part, d);
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
        assert!(set_part_eave_height(&mut p, 1, base - 1.0).unwrap());
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
        assert_eq!(common_eave_height(&p, 1), Some(base - 1.0));
        // The building keeps the height it showed (its lowest eave then).
        assert_eq!(part_height(&p, 1), Some(base));
        // The same height again changes nothing.
        assert!(!set_part_eave_height(&mut p, 1, base - 1.0).unwrap());
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
        assert_eq!(common_eave_height(&p, 0), None);
        assert!(set_part_eave_height(&mut p, 0, 4.0).unwrap());
        assert_eq!(common_eave_height(&p, 0), Some(4.0));
    }

    #[test]
    fn refuses_bad_input_without_changing_anything() {
        let mut p = two_buildings();
        let before = p.clone();
        for h in [f64::NAN, f64::INFINITY, -0.5, 1000.5, 1e300] {
            assert!(set_part_height(&mut p, 0, h).is_err(), "{h}");
            assert!(set_part_eave_height(&mut p, 0, h).is_err(), "{h}");
        }
        assert!(set_part_height(&mut p, 7, 3.0).is_err());
        assert!(set_part_eave_height(&mut p, 7, 3.0).is_err());
        assert_eq!(p, before);
    }

    /// Jakub (2026-11-14): the building height is not the eave height; an
    /// eave can be set lower than the building.
    #[test]
    fn an_eave_lower_than_the_building_stays_lower_when_the_building_moves() {
        let mut p = two_buildings();
        let base = p.parts[1].edges[0].eave_height;
        // A building of its own height: only the height is stored, the
        // roof is untouched.
        let roof = p.parts[1].edges.clone();
        assert!(set_part_height(&mut p, 1, base).unwrap());
        assert_eq!(p.parts[1].height, Some(base));
        assert_eq!(p.parts[1].edges, roof);
        // One eave 0.4 m below the building (an eave wall with overhang).
        p.parts[1].edges[0].eave_height = base - 0.4;
        assert_eq!(
            part_height(&p, 1),
            Some(base),
            "the building keeps its height"
        );
        let before = crate::engine::generate(&p).unwrap();
        // The building 1 m higher: every wall rises by 1 m, the low eave
        // stays 0.4 m below the building.
        assert!(set_part_height(&mut p, 1, base + 1.0).unwrap());
        assert_eq!(part_height(&p, 1), Some(base + 1.0));
        assert_eq!(p.parts[1].edges[0].eave_height, round_to(base + 0.6, 3));
        assert!(
            p.parts[1].edges[1..]
                .iter()
                .all(|e| e.eave_height == round_to(base + 1.0, 3))
        );
        let after = crate::engine::generate(&p).unwrap();
        assert_ne!(
            before[1].faces.len() + before[0].faces.len(),
            0,
            "both roofs calculate"
        );
        assert!(
            after
                .iter()
                .all(|g| g.error.is_none() && !g.faces.is_empty())
        );
        // The height alone changes no roof figure.
        let mut q = p.clone();
        q.parts[1].height = Some(base + 7.0);
        let area = |g: &[crate::roof::RoofGeometry]| {
            g.iter().flat_map(|g| &g.faces).map(|f| f.area).sum::<f64>()
        };
        assert_eq!(area(&crate::engine::generate(&q).unwrap()), area(&after));
        // The same height again changes nothing.
        assert!(!set_part_height(&mut p, 1, base + 1.0).unwrap());
    }

    #[test]
    fn a_file_without_building_height_shows_its_lowest_eave() {
        let mut p = two_buildings();
        p.parts[0].edges[2].eave_height += 0.4;
        let low = p.parts[0].edges[0].eave_height;
        assert_eq!(p.parts[0].height, None);
        assert_eq!(part_height(&p, 0), Some(low));
        let json = serde_json::to_string(&p.parts[0]).unwrap();
        assert!(!json.contains("\"height\":"), "{json}");
        p.parts[0].height = Some(5.5);
        let back: RoofPart =
            serde_json::from_str(&serde_json::to_string(&p.parts[0]).unwrap()).unwrap();
        assert_eq!(back.height, Some(5.5));
    }

    #[test]
    fn a_building_height_that_puts_an_eave_below_ground_is_refused() {
        let mut p = two_buildings();
        let base = p.parts[1].edges[0].eave_height;
        assert!(set_part_height(&mut p, 1, base).unwrap());
        p.parts[1].edges[0].eave_height = base - 0.4;
        let before = p.clone();
        // 0.2 m: the low eave would be at -0.2 m (review MATH-003).
        assert!(set_part_height(&mut p, 1, 0.2).is_err());
        assert_eq!(p, before);
        assert!(set_part_height(&mut p, 1, 0.4).unwrap());
        assert_eq!(p.parts[1].edges[0].eave_height, 0.0);
    }

    #[test]
    fn a_stored_building_height_out_of_range_is_refused() {
        // 99 999 m passes the file limits; moving from it would put every
        // eave about 100 km below ground (review ARCH-003).
        let mut p = two_buildings();
        p.parts[1].height = Some(99_999.0);
        let before = p.clone();
        assert!(set_part_height(&mut p, 1, 3.0).is_err());
        assert_eq!(p, before);
    }
}
