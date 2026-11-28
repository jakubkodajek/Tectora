//! Reading the calculated roof at plan points: heights for placing dormers,
//! roof points to snap hand-drawn lines to (editor: `dormers.ts`,
//! `manual.ts`, `roofcalc.ts`).

use crate::geom::Point2;
use crate::model::Chimney;
use crate::roof::{LineKind, RoofGeometry};

use super::round_to;

fn inside(poly: &[[f64; 3]], p: Point2) -> bool {
    let mut c = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let ([xi, yi, _], [xj, yj, _]) = (poly[i], poly[j]);
        if (yi > p.y) != (yj > p.y) && p.x < (xj - xi) * (p.y - yi) / (yj - yi) + xi {
            c = !c;
        }
        j = i;
    }
    c
}

fn plane_z(poly: &[[f64; 3]], p: Point2) -> Option<f64> {
    let o = poly[0];
    for w in poly[1..].windows(2) {
        let (a, b) = (w[0], w[1]);
        let u = [a[0] - o[0], a[1] - o[1], a[2] - o[2]];
        let v = [b[0] - o[0], b[1] - o[1], b[2] - o[2]];
        let n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        if n[2].abs() > 1e-9 {
            return Some(o[2] - (n[0] * (p.x - o[0]) + n[1] * (p.y - o[1])) / n[2]);
        }
    }
    None
}

/// Height of the calculated roof at a plan point (the highest face over
/// it), or `None` off the roof.
pub fn roof_height_at(geos: &[RoofGeometry], p: Point2) -> Option<f64> {
    geos.iter()
        .flat_map(|g| &g.faces)
        .filter(|f| f.polygon.len() >= 3 && inside(&f.polygon, p))
        .filter_map(|f| plane_z(&f.polygon, p))
        .reduce(f64::max)
}

/// Height of the roof a dormer or chimney stands on at a plan point: as
/// [`roof_height_at`] without the dormers' own faces.
pub fn host_height_at(geos: &[RoofGeometry], p: Point2) -> Option<f64> {
    geos.iter()
        .flat_map(|g| &g.faces)
        .filter(|f| f.edge != usize::MAX && f.polygon.len() >= 3 && inside(&f.polygon, p))
        .filter_map(|f| plane_z(&f.polygon, p))
        .reduce(f64::max)
}

/// The highest point of the building roof (no dormer faces) whose faces
/// hold plan point `p`: what a dormer standing there is compared with
/// ([`crate::check`]'s `dormer_above_roof`). `None` off the roof.
pub fn building_top_at(geos: &[RoofGeometry], p: Point2) -> Option<f64> {
    geos.iter()
        .filter(|g| {
            g.faces
                .iter()
                .any(|f| f.edge != usize::MAX && f.polygon.len() >= 3 && inside(&f.polygon, p))
        })
        .flat_map(|g| g.faces.iter().filter(|f| f.edge != usize::MAX))
        .flat_map(|f| f.polygon.iter().map(|q| q[2]))
        .filter(|z| z.is_finite())
        .reduce(f64::max)
}

/// The face a dormer or chimney stands on at a plan point (as
/// [`host_height_at`]): its height there and its gradient (dz/dx, dz/dy).
pub fn host_plane_at(geos: &[RoofGeometry], p: Point2) -> Option<(f64, [f64; 2])> {
    geos.iter()
        .flat_map(|g| &g.faces)
        .filter(|f| f.edge != usize::MAX && f.polygon.len() >= 3 && inside(&f.polygon, p))
        .filter_map(|f| {
            let z = plane_z(&f.polygon, p)?;
            let zx = plane_z(&f.polygon, Point2::new(p.x + 1.0, p.y))? - z;
            let zy = plane_z(&f.polygon, Point2::new(p.x, p.y + 1.0))? - z;
            Some((z, [zx, zy]))
        })
        .reduce(|a, b| if b.0 > a.0 { b } else { a })
}

fn key(q: &[f64; 3]) -> [i64; 3] {
    q.map(|v| (v * 1000.0).round() as i64)
}

/// Roof points to snap to: the ends of the calculated lines and face corners.
pub fn roof_point_list(geos: &[RoofGeometry]) -> Vec<[f64; 3]> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for g in geos {
        let pts = g
            .lines
            .iter()
            .flat_map(|l| [l.a, l.b])
            .chain(g.faces.iter().flat_map(|f| f.polygon.iter().copied()));
        for q in pts {
            if seen.insert(key(&q)) {
                out.push(q);
            }
        }
    }
    out
}

/// The roof point within `tol` of `p` in plan, else `p` at height 0 (as DACAD).
pub fn snap_to_roof(geos: &[RoofGeometry], p: Point2, tol: f64) -> [f64; 3] {
    let mut best = None;
    let mut bd = tol;
    for q in roof_point_list(geos) {
        let d = (q[0] - p.x).hypot(q[1] - p.y);
        if d <= bd {
            bd = d;
            best = Some(q);
        }
    }
    best.unwrap_or([p.x, p.y, 0.0])
}

/// End points of the roof lines other than eaves (ridge, hip and valley
/// corners), highest first: the points the "point in space" helper offers.
pub fn roof_corner_points(geos: &[RoofGeometry]) -> Vec<[f64; 3]> {
    roof_corner_points_kinded(geos)
        .into_iter()
        .map(|(q, _)| q)
        .collect()
}

/// [`roof_corner_points`] with the kind of the line that first reached each.
pub fn roof_corner_points_kinded(geos: &[RoofGeometry]) -> Vec<([f64; 3], LineKind)> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for l in geos.iter().flat_map(|g| &g.lines) {
        if l.kind == LineKind::Eave {
            continue;
        }
        for q in [l.a, l.b] {
            if seen.insert(key(&q)) {
                out.push((q, l.kind));
            }
        }
    }
    out.sort_by(|a, b| b.0[2].total_cmp(&a.0[2]));
    out
}

/// The roof height a new chimney's top is set 1 m above: the roof at the
/// middle of its corners (the editor's and MCP `add_object`'s default).
pub fn chimney_default_base(geos: &[RoofGeometry], corners: &[Point2]) -> Option<f64> {
    if corners.is_empty() {
        return None;
    }
    let n = corners.len() as f64;
    let c = Point2::new(
        corners.iter().map(|p| p.x).sum::<f64>() / n,
        corners.iter().map(|p| p.y).sum::<f64>() / n,
    );
    roof_height_at(geos, c)
}

/// A rectangular chimney from a corner, size and rotation in degrees.
pub fn chimney_from_rect(
    x: f64,
    y: f64,
    width: f64,
    depth: f64,
    rotation_deg: f64,
    height: f64,
) -> Chimney {
    let (sin, cos) = rotation_deg.to_radians().sin_cos();
    let p = |s: f64, t: f64| {
        Point2::new(
            round_to(x + cos * s - sin * t, 3),
            round_to(y + sin * s + cos * t, 3),
        )
    };
    Chimney {
        corners: vec![p(0.0, 0.0), p(width, 0.0), p(width, depth), p(0.0, depth)],
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::dormers::{DormerKind, build_dormer, default_params};
    use crate::edit::plain_edge;
    use crate::model::{Project, RoofPart};
    use crate::openings::generate_project;

    /// A 12 x 8 m hip roof, 35 deg on every wall, eave 3 m, no overhang.
    fn hip_house() -> Project {
        let mut p = Project::new("probe");
        let w = |x: f64, y: f64| plain_edge(Point2::new(x, y), 35.0, 3.0, 0.0);
        p.parts.push(RoofPart {
            height: None,
            name: "A".into(),
            edges: vec![w(0.0, 0.0), w(12.0, 0.0), w(12.0, 8.0), w(0.0, 8.0)],
        });
        p
    }

    fn tan35() -> f64 {
        35f64.to_radians().tan()
    }

    /// The house with a 2 m gable dormer on the front slope (eave and
    /// ridge heights from the section dialog, as the editor sets them).
    fn house_with_dormer() -> Project {
        use crate::edit::dormers::{SectionTarget, eave_for, host_slope};
        let mut p = hip_house();
        let mut d = default_params(DormerKind::Gable, Point2::new(5.0, 1.0), 0.0, 3.0);
        d.width = 2.0;
        p.dormers.push(build_dormer(&d));
        let host = host_slope(&p, 0).unwrap();
        d.eave_height = eave_for(&d, host, SectionTarget::WallHeight(0.9)).unwrap();
        p.dormers[0] = build_dormer(&d);
        p
    }

    fn near(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-4, "{a} vs {b}");
    }

    #[test]
    fn the_roof_height_follows_the_faces_of_a_hip_roof() {
        // Faces: the long front/back slopes rise 35 deg from y = 0 and
        // y = 8 to the ridge at y = 4 (height 3 + 4 tan35); the end hips
        // rise from x = 0 and x = 12 to the ridge ends at x = 4 and 8.
        let geos = generate_project(&hip_house());
        near(
            roof_height_at(&geos, Point2::new(6.0, 4.0)).unwrap(),
            3.0 + 4.0 * tan35(),
        );
        near(
            roof_height_at(&geos, Point2::new(6.0, 0.5)).unwrap(),
            3.0 + 0.5 * tan35(),
        );
        near(
            roof_height_at(&geos, Point2::new(1.0, 4.0)).unwrap(),
            3.0 + tan35(),
        );
        assert_eq!(roof_height_at(&geos, Point2::new(-1.0, -1.0)), None);
        assert_eq!(roof_height_at(&geos, Point2::new(13.0, 4.0)), None);
        assert_eq!(roof_height_at(&[], Point2::new(6.0, 4.0)), None);
    }

    #[test]
    fn the_roof_height_over_a_dormer_is_the_highest_face_not_the_lowest() {
        // At (6, 1.8) the front slope (3 + 1.8 tan35 = 4.2604) lies under
        // the dormer's roof; the dormer has its eave at 4.6 m on x = 5 and
        // 7 and a 45 deg roof, so its ridge on x = 6 is 5.6 m.
        let geos = generate_project(&house_with_dormer());
        let q = Point2::new(6.0, 1.8);
        near(roof_height_at(&geos, q).unwrap(), 5.6);
        // Beside the dormer there is only the host.
        near(
            roof_height_at(&geos, Point2::new(10.0, 1.8)).unwrap(),
            3.0 + 1.8 * tan35(),
        );
    }

    #[test]
    fn the_host_height_ignores_the_dormers_own_faces() {
        let geos = generate_project(&house_with_dormer());
        let q = Point2::new(6.0, 1.8);
        let host = 3.0 + 1.8 * tan35();
        near(host_height_at(&geos, q).unwrap(), host);
        // The same height the bare roof gives.
        let bare = crate::edit::position::bare_roof(&house_with_dormer());
        near(host_height_at(&bare, q).unwrap(), host);
        near(roof_height_at(&bare, q).unwrap(), host);
        assert_eq!(host_height_at(&geos, Point2::new(-1.0, -1.0)), None);
    }

    #[test]
    fn the_host_plane_is_the_slope_under_the_point_with_its_gradient() {
        let geos = generate_project(&house_with_dormer());
        let (z, [zx, zy]) = host_plane_at(&geos, Point2::new(6.0, 1.8)).unwrap();
        near(z, 3.0 + 1.8 * tan35());
        near(zx, 0.0);
        near(zy, tan35());
    }

    #[test]
    fn a_point_near_a_roof_corner_snaps_to_it_with_its_height() {
        let geos = generate_project(&hip_house());
        let on = |p: Point2, tol: f64| snap_to_roof(&geos, p, tol);
        // The eave corner (0, 0) at 3 m, 5 cm away.
        let q = on(Point2::new(0.03, 0.04), 0.1);
        near(q[0], 0.0);
        near(q[1], 0.0);
        near(q[2], 3.0);
        // The ridge end (4, 4) at 3 + 4 tan35.
        let q = on(Point2::new(4.02, 3.97), 0.1);
        near(q[0], 4.0);
        near(q[1], 4.0);
        near(q[2], 3.0 + 4.0 * tan35());
    }

    #[test]
    fn a_point_clear_of_the_roof_points_stays_where_it_is_at_height_0() {
        let geos = generate_project(&hip_house());
        let q = snap_to_roof(&geos, Point2::new(1.0, 0.5), 0.1);
        assert_eq!(q, [1.0, 0.5, 0.0]);
        // Nothing to snap to.
        assert_eq!(
            snap_to_roof(&[], Point2::new(2.0, 3.0), 0.1),
            [2.0, 3.0, 0.0]
        );
        // A corner just outside the tolerance is not taken.
        let q = snap_to_roof(&geos, Point2::new(0.2, 0.0), 0.1);
        assert_eq!(q, [0.2, 0.0, 0.0]);
    }

    #[test]
    fn the_roof_points_are_listed_once_each() {
        let geos = generate_project(&hip_house());
        let pts = roof_point_list(&geos);
        // 4 eave corners and 2 ridge ends.
        assert_eq!(pts.len(), 6, "{pts:?}");
    }

    #[test]
    fn the_corner_points_have_no_eave_lines_and_start_with_the_highest() {
        let geos = generate_project(&hip_house());
        let kinded = roof_corner_points_kinded(&geos);
        assert!(
            kinded.iter().all(|(_, k)| *k != LineKind::Eave),
            "{kinded:?}"
        );
        let pts = roof_corner_points(&geos);
        assert_eq!(pts.len(), 6, "{pts:?}");
        assert!(pts.windows(2).all(|w| w[0][2] >= w[1][2]), "{pts:?}");
        // The ridge ends come first.
        for q in &pts[..2] {
            near(q[2], 3.0 + 4.0 * tan35());
        }
        for q in &pts[2..] {
            near(q[2], 3.0);
        }
    }

    #[test]
    fn a_roof_line_that_is_only_an_eave_gives_no_corner_point() {
        // A gable roof: the eave lines run along y = 0 and y = 8 and the
        // gable ends are verge lines, so only the ends of the non-eave
        // lines are offered: every point is on a verge or the ridge.
        let mut p = hip_house();
        for (i, e) in p.parts[0].edges.iter_mut().enumerate() {
            e.pitch_deg = if i % 2 == 1 { 90.0 } else { 35.0 };
        }
        let geos = generate_project(&p);
        let eave_ends: Vec<[f64; 3]> = geos
            .iter()
            .flat_map(|g| &g.lines)
            .filter(|l| l.kind == LineKind::Eave)
            .flat_map(|l| [l.a, l.b])
            .collect();
        assert!(!eave_ends.is_empty());
        let kinded = roof_corner_points_kinded(&geos);
        assert!(kinded.iter().all(|(_, k)| *k != LineKind::Eave));
        assert!(!kinded.is_empty());
    }
}
