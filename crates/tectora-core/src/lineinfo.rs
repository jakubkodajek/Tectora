//! Details of every calculated roof line (ridge, eave, hip, valley, verge,
//! …): its code, length, heights of its ends, its own slope and the roof
//! faces it bounds with their pitches. The editor shows them when the
//! pointer is on a line in the plan or in 3D, and the MCP tool
//! `line_details` gives the same figures to AI clients.
//!
//! Display only: nothing here changes the roof. The lines are the core's
//! ([`RoofGeometry::lines`]); a line belongs to a face when its middle lies
//! on that face (the rule [`crate::quantities::lines_by_material`] uses).
//!
//! [`RoofGeometry::lines`]: crate::roof::RoofGeometry::lines

use std::collections::BTreeMap;

use serde::Serialize;

use crate::geom::Point2;
use crate::model::Project;
use crate::roof::{LineKind, RoofGeometry};

/// A roof face a line bounds or lies on.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LineFace {
    /// The part (building) the face belongs to, index from 0.
    pub part: usize,
    /// The face's index in that part's faces.
    pub face: usize,
    /// The wall the face rises from, index from 0; `None` for a dormer's
    /// own faces.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wall: Option<usize>,
    pub pitch_deg: f64,
    /// Roof material number of the face (1 tiles, 2 sheet metal, …).
    pub material: u8,
}

/// One calculated roof line with what the editor shows about it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LineDetail {
    /// The part whose roof the line belongs to, index from 0.
    pub part: usize,
    /// The line's index in that part's `geometry.lines`.
    pub index: usize,
    pub kind: LineKind,
    /// DACAD's line code (T, F, G, K, OL, OR, OS, P, c, S, W, WV, WH, WS).
    pub code: &'static str,
    pub a: [f64; 3],
    pub b: [f64; 3],
    /// True length, √(Δx² + Δy² + Δz²), m.
    pub length: f64,
    /// The length the results count, m: `length` less the parts on or
    /// under chimneys, roof openings and dormers that the calculation takes
    /// off this line's code (`RoofGeometry::minus_lines`). Equal to
    /// `length` for most lines; summed per code it gives the results' line
    /// totals (without lines drawn by hand).
    pub counted_length: f64,
    /// Length in plan, m.
    pub plan_length: f64,
    /// The line's own inclination against the horizontal, degrees (0 for a
    /// ridge or an eave, the hip's slope for a hip).
    pub slope_deg: f64,
    /// Height of the lower and the higher end above the ground, m.
    pub z_low: f64,
    pub z_high: f64,
    /// The faces the line bounds or lies on, in part and face order.
    pub faces: Vec<LineFace>,
}

/// Every line of roofs already calculated
/// (`openings::generate_project(project)`), part by part in line order.
/// O(lines · faces · vertices).
pub fn line_details_of(project: &Project, geos: &[RoofGeometry]) -> Vec<LineDetail> {
    let mut out = Vec::new();
    for (pi, geo) in geos.iter().enumerate() {
        let counted = counted_lengths(project, geo);
        for (li, l) in geo.lines.iter().enumerate() {
            let (dx, dy, dz) = (l.b[0] - l.a[0], l.b[1] - l.a[1], l.b[2] - l.a[2]);
            let plan = dx.hypot(dy);
            let mid = [
                (l.a[0] + l.b[0]) / 2.0,
                (l.a[1] + l.b[1]) / 2.0,
                (l.a[2] + l.b[2]) / 2.0,
            ];
            let faces = geos
                .iter()
                .enumerate()
                .flat_map(|(fp, g)| g.faces.iter().enumerate().map(move |(fi, f)| (fp, fi, f)))
                .filter(|(_, _, f)| crate::quantities::bounds(f, mid))
                .map(|(fp, fi, f)| LineFace {
                    part: fp,
                    face: fi,
                    wall: project
                        .parts
                        .get(fp)
                        .filter(|p| f.edge < p.edges.len())
                        .map(|_| f.edge),
                    pitch_deg: f.pitch_deg,
                    material: f.material,
                })
                .collect();
            out.push(LineDetail {
                part: pi,
                index: li,
                kind: l.kind,
                code: l.dacad_code(),
                a: l.a,
                b: l.b,
                length: l.length(),
                counted_length: counted[li],
                plan_length: plan,
                slope_deg: if plan < 1e-9 && dz.abs() < 1e-9 {
                    0.0
                } else {
                    dz.abs().atan2(plan).to_degrees()
                },
                z_low: l.a[2].min(l.b[2]),
                z_high: l.a[2].max(l.b[2]),
                faces,
            });
        }
    }
    out
}

/// One roof line of a tower ([`crate::tower::TowerLine`]): a closed ring
/// (eave T, pitch break c, Pult P) or a hip G up a sharp corner.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TowerLineDetail {
    /// The tower, index into `project.towers` from 0.
    pub tower: usize,
    /// The tier the line belongs to, from 0 at the bottom.
    pub tier: usize,
    /// DACAD's line code (T, c, P, G).
    pub code: &'static str,
    /// Whether the line is a closed ring around the tower (`a` = `b`, its
    /// first point).
    pub closed: bool,
    pub a: [f64; 3],
    pub b: [f64; 3],
    /// True length, m (a ring's along the tower's fine outline).
    pub length: f64,
    /// The length the results count, m: the whole line (nothing is cut
    /// out of a tower's roof).
    pub counted_length: f64,
    /// Length in plan, m (a level ring's is its length).
    pub plan_length: f64,
    /// The line's own inclination against the horizontal, degrees.
    pub slope_deg: f64,
    pub z_low: f64,
    pub z_high: f64,
}

/// Every roof line of the towers of `project`, tower by tower in line
/// order (audit MATH-013: the line details left them out, so their lines
/// did not add up to the results). Towers with problems have none.
pub fn tower_line_details(project: &Project) -> Vec<TowerLineDetail> {
    let mut out = Vec::new();
    for (ti, r) in crate::tower::results(project).iter().enumerate() {
        let Some(r) = r else { continue };
        for l in &r.lines {
            let a = l.points.first().copied().unwrap_or([0.0; 3]);
            let b = if l.closed {
                a
            } else {
                l.points.last().copied().unwrap_or(a)
            };
            let zs = l.points.iter().map(|p| p[2]);
            let z_low = zs.clone().fold(f64::INFINITY, f64::min);
            let z_high = zs.fold(f64::NEG_INFINITY, f64::max);
            let (z_low, z_high) = if z_low <= z_high {
                (z_low, z_high)
            } else {
                (a[2], a[2])
            };
            let (plan, slope) = if l.closed {
                (l.length, 0.0)
            } else {
                let plan = (b[0] - a[0]).hypot(b[1] - a[1]);
                let dz = (b[2] - a[2]).abs();
                let slope = if plan < 1e-9 && dz < 1e-9 {
                    0.0
                } else {
                    dz.atan2(plan).to_degrees()
                };
                (plan, slope)
            };
            out.push(TowerLineDetail {
                tower: ti,
                tier: l.tier,
                code: l.code,
                closed: l.closed,
                a,
                b,
                length: l.length,
                counted_length: l.length,
                plan_length: plan,
                slope_deg: slope,
                z_low,
                z_high,
            });
        }
    }
    out
}

/// The length each line of `geo` counts with in the results. The core
/// takes the parts of lines on or under chimneys, openings and dormers off
/// per code (`minus_lines`), not per line: each code's total is shared out
/// over that code's lines by how much of each lies on or under those
/// objects in plan, so the lines of a code add up to the results exactly.
/// What lies on no line of its code (a dormer on the wall line, its eave
/// on the overhang) comes off the lines of that code nearest an object.
fn counted_lengths(project: &Project, geo: &RoofGeometry) -> Vec<f64> {
    let mut out: Vec<f64> = geo.lines.iter().map(|l| l.length()).collect();
    if geo.minus_lines.is_empty() {
        return out;
    }
    let objects: Vec<Vec<Point2>> = project
        .chimneys
        .iter()
        .chain(&project.roof_openings)
        .map(|c| c.corners.clone())
        .chain(geo.footprints.iter().cloned())
        // A tower's shaft, cut out of the roof like a chimney
        // (`openings::with_tower_shafts`, audit MATH-013).
        .chain(
            project
                .towers
                .iter()
                .filter(|t| t.problems().is_empty())
                .map(|t| t.footprint()),
        )
        // A dormer's walls closed along the roof wall it stands on.
        .chain(project.dormers.iter().map(|d| {
            let mut o: Vec<Point2> = d.edges.iter().map(|e| e.start).collect();
            o.push(d.end);
            o
        }))
        .filter(|c| c.len() >= 3)
        .collect();
    let mut minus: BTreeMap<&str, f64> = BTreeMap::new();
    for (code, len) in &geo.minus_lines {
        *minus.entry(code.as_str()).or_default() += len;
    }
    let under: Vec<f64> = geo
        .lines
        .iter()
        .map(|l| {
            if minus.contains_key(l.dacad_code()) {
                covered(&objects, l.a, l.b) * l.length()
            } else {
                0.0
            }
        })
        .collect();
    for (code, m) in minus {
        let mut lines: Vec<usize> = (0..geo.lines.len())
            .filter(|&i| geo.lines[i].dacad_code() == code)
            .collect();
        let total: f64 = lines.iter().map(|&i| under[i]).sum();
        // First the parts that lie on or under an object, scaled down if
        // they come to more than the total.
        let share = if total > m { m / total } else { 1.0 };
        let mut left = m;
        for &i in &lines {
            let d = (under[i] * share).min(out[i]);
            out[i] -= d;
            left -= d;
        }
        // What lies on no line of the code in plan (a dormer standing on
        // the wall, its eave on the overhang line; a chimney beside a
        // verge): taken off the lines of the code nearest to an object.
        if left > 1e-9 && !objects.is_empty() {
            let gap = |i: usize| {
                let l = &geo.lines[i];
                objects
                    .iter()
                    .map(|o| distance(o, l.a, l.b))
                    .fold(f64::INFINITY, f64::min)
            };
            lines.sort_by(|&a, &b| gap(a).total_cmp(&gap(b)));
            for i in lines {
                let d = left.min(out[i]);
                out[i] -= d;
                left -= d;
                if left <= 1e-9 {
                    break;
                }
            }
        }
    }
    out
}

/// Plan distance between the line from `a` to `b` and the outline `poly`
/// (0 where they touch or the line runs inside).
fn distance(poly: &[Point2], a: [f64; 3], b: [f64; 3]) -> f64 {
    let (pa, pb) = (Point2::new(a[0], a[1]), Point2::new(b[0], b[1]));
    if covered(&[poly.to_vec()], a, b) > 0.0 {
        return 0.0;
    }
    let to_seg = |p: Point2, s: Point2, e: Point2| {
        let (dx, dy) = (e.x - s.x, e.y - s.y);
        let l2 = dx * dx + dy * dy;
        let t = if l2 < 1e-18 {
            0.0
        } else {
            (((p.x - s.x) * dx + (p.y - s.y) * dy) / l2).clamp(0.0, 1.0)
        };
        p.distance(Point2::new(s.x + t * dx, s.y + t * dy))
    };
    let n = poly.len();
    let corners = poly.iter().map(|&p| to_seg(p, pa, pb));
    let ends = (0..n).flat_map(|i| {
        let (s, e) = (poly[i], poly[(i + 1) % n]);
        [to_seg(pa, s, e), to_seg(pb, s, e)]
    });
    corners.chain(ends).fold(f64::INFINITY, f64::min)
}

/// How much of the line from `a` to `b` lies on or inside any of `polys`
/// in plan, as a share from 0 to 1.
fn covered(polys: &[Vec<Point2>], a: [f64; 3], b: [f64; 3]) -> f64 {
    const ON: f64 = 1e-4;
    let (pa, pb) = (Point2::new(a[0], a[1]), Point2::new(b[0], b[1]));
    let d = Point2::new(pb.x - pa.x, pb.y - pa.y);
    let l2 = d.x * d.x + d.y * d.y;
    if l2 < 1e-18 {
        return 0.0;
    }
    let mut ts = vec![0.0, 1.0];
    for poly in polys {
        let n = poly.len();
        for i in 0..n {
            let (p, q) = (poly[i], poly[(i + 1) % n]);
            // Where an outline side crosses the line …
            let e = Point2::new(q.x - p.x, q.y - p.y);
            let den = d.x * e.y - d.y * e.x;
            if den.abs() > 1e-12 {
                let w = Point2::new(p.x - pa.x, p.y - pa.y);
                let t = (w.x * e.y - w.y * e.x) / den;
                let u = (w.x * d.y - w.y * d.x) / den;
                if (0.0..=1.0).contains(&u) && t > 0.0 && t < 1.0 {
                    ts.push(t);
                }
            }
            // … and where a corner lies on it (a side along the line).
            let t = ((p.x - pa.x) * d.x + (p.y - pa.y) * d.y) / l2;
            if t > 0.0 && t < 1.0 {
                ts.push(t);
            }
        }
    }
    ts.sort_by(f64::total_cmp);
    ts.windows(2)
        .filter(|s| s[1] - s[0] > 1e-12)
        .filter(|s| {
            let m = (s[0] + s[1]) / 2.0;
            let at = Point2::new(pa.x + m * d.x, pa.y + m * d.y);
            polys.iter().any(|poly| {
                crate::openings::inside(poly, at) || crate::union::near_boundary(poly, at, ON)
            })
        })
        .map(|s| s[1] - s[0])
        .sum::<f64>()
        .min(1.0)
}

/// [`line_details_of`] of the project's roofs.
pub fn line_details(project: &Project) -> Vec<LineDetail> {
    line_details_of(project, &crate::openings::generate_project(project))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::examples;
    use approx::assert_relative_eq;

    /// Hip roof 12 × 8 m, 35°, eave 3 m at the 0.5 m overhang: plan 13 × 9,
    /// ridge 13 − 9 = 4 m long at 3 + 4.5 · tan 35°, each hip from an eave
    /// corner to a ridge end, 4.5 m in x and y below the ridge.
    #[test]
    fn hip_roof_lines_have_closed_form_heights_and_faces() {
        let p = examples::find("hip").unwrap().project();
        let d = line_details(&p);
        let ridge_z = 3.0 + 4.5 * 35f64.to_radians().tan();

        let ridges: Vec<_> = d.iter().filter(|l| l.code == "F").collect();
        assert_eq!(ridges.len(), 1);
        let r = ridges[0];
        assert_relative_eq!(r.length, 4.0, epsilon = 1e-3);
        assert_relative_eq!(r.z_low, ridge_z, epsilon = 1e-3);
        assert_relative_eq!(r.z_high, ridge_z, epsilon = 1e-3);
        assert_relative_eq!(r.slope_deg, 0.0, epsilon = 1e-3);
        // Between the two long faces, both 35°.
        assert_eq!(r.faces.len(), 2, "{:?}", r.faces);
        assert!(r.faces.iter().all(|f| (f.pitch_deg - 35.0).abs() < 1e-6));

        let hips: Vec<_> = d.iter().filter(|l| l.code == "G").collect();
        assert_eq!(hips.len(), 4);
        let plan = 4.5f64 * 2f64.sqrt();
        let rise = ridge_z - 3.0;
        for h in hips {
            assert_relative_eq!(h.plan_length, plan, epsilon = 1e-3);
            assert_relative_eq!(h.length, plan.hypot(rise), epsilon = 1e-3);
            assert_relative_eq!(h.slope_deg, rise.atan2(plan).to_degrees(), epsilon = 1e-3);
            assert_relative_eq!(h.z_low, 3.0, epsilon = 1e-3);
            assert_relative_eq!(h.z_high, ridge_z, epsilon = 1e-3);
            // A hip lies between a long face and an end face.
            assert_eq!(h.faces.len(), 2, "{:?}", h.faces);
            assert_ne!(h.faces[0].wall, h.faces[1].wall);
        }

        // Eaves: level at 3 m, one face each, the wall it belongs to.
        let eaves: Vec<_> = d.iter().filter(|l| l.code == "T").collect();
        assert_eq!(eaves.len(), 4);
        for e in eaves {
            assert_relative_eq!(e.z_low, 3.0, epsilon = 1e-3);
            assert_relative_eq!(e.z_high, 3.0, epsilon = 1e-3);
            assert_eq!(e.faces.len(), 1, "{:?}", e.faces);
            assert!(e.faces[0].wall.is_some());
        }
    }

    /// The details list every line of the result, in its order, with the
    /// same length the result gives it.
    #[test]
    fn every_line_of_every_example_is_listed_once() {
        for ex in examples::EXAMPLES {
            let p = ex.project();
            let geos = crate::openings::generate_project(&p);
            let d = line_details_of(&p, &geos);
            let n: usize = geos.iter().map(|g| g.lines.len()).sum();
            assert_eq!(d.len(), n, "{}", ex.id);
            for l in &d {
                let line = &geos[l.part].lines[l.index];
                assert_eq!(l.code, line.dacad_code(), "{}", ex.id);
                assert_relative_eq!(l.length, line.length(), epsilon = 1e-12);
                assert!(l.z_low <= l.z_high);
                assert!(l.length.is_finite() && l.slope_deg.is_finite());
                // Every roof line bounds at least one face.
                assert!(!l.faces.is_empty(), "{} line {:?} has no face", ex.id, line);
            }
        }
    }

    /// A gable roof's verge rises with the roof: from the eave up to the
    /// ridge at the face's own pitch (the line lies in a 40° plane and runs
    /// straight up its slope).
    #[test]
    fn a_verge_rises_at_the_face_pitch() {
        let p = examples::find("gable").unwrap().project();
        let d = line_details(&p);
        let verges: Vec<_> = d.iter().filter(|l| l.code.starts_with('O')).collect();
        assert_eq!(verges.len(), 4);
        for v in verges {
            assert_relative_eq!(v.slope_deg, 40.0, epsilon = 1e-3);
            assert_eq!(v.faces.len(), 1);
            assert_relative_eq!(v.faces[0].pitch_deg, 40.0, epsilon = 1e-9);
        }
    }

    fn square(x0: f64, y0: f64, x1: f64, y1: f64) -> crate::Chimney {
        crate::Chimney {
            corners: vec![
                Point2::new(x0, y0),
                Point2::new(x1, y0),
                Point2::new(x1, y1),
                Point2::new(x0, y1),
            ],
            height: 12.0,
        }
    }

    /// Per code, the lines' counted lengths add up to the results' line
    /// totals (`dacad_line_totals_of`, no lines drawn by hand here).
    fn assert_counted_add_up(name: &str, p: &Project) -> Vec<LineDetail> {
        let geos = crate::openings::generate_project(p);
        let totals = crate::quantities::dacad_line_totals_of(&geos, p);
        let d = line_details_of(p, &geos);
        let mut by: BTreeMap<String, f64> = BTreeMap::new();
        for l in &d {
            assert!(l.counted_length <= l.length + 1e-9, "{name}: {l:?}");
            assert!(l.counted_length >= 0.0, "{name}: {l:?}");
            *by.entry(l.code.to_string()).or_default() += l.counted_length;
        }
        // The towers' own lines are in the results too (MATH-013).
        for l in tower_line_details(p) {
            *by.entry(l.code.to_string()).or_default() += l.counted_length;
        }
        for (code, t) in &totals {
            let got = by.get(code).copied().unwrap_or(0.0);
            assert!(
                (got - t).abs() < 0.006,
                "{name} {code}: lines {got}, results {t}"
            );
        }
        d
    }

    /// Audit MATH-013: a tower's shaft is cut out like a chimney, so the
    /// parts of lines under it come off as under a chimney: on the ridge,
    /// across the eave and over the whole roof.
    #[test]
    fn lines_count_without_their_parts_under_a_towers_shaft() {
        use crate::edit::towers::{TowerPreset, preset};
        let base = examples::find("hip").unwrap().project();
        for (c, w) in [((6.0, 4.0), 1.0), ((6.0, 0.0), 1.5), ((6.0, 4.0), 30.0)] {
            let mut p = base.clone();
            p.towers
                .push(preset(TowerPreset::Pyramid, Point2::new(c.0, c.1), w, 12.0));
            assert_counted_add_up(&format!("tower {w} m at {c:?}"), &p);
        }
    }

    /// Hip roof 12 × 8 m: a 1 m chimney standing on the eave takes 1 m off
    /// that eave (13 → 12 m, 44 → 43 m in all), one across the ridge 1 m
    /// off the ridge (4 → 3 m); a hole over the whole roof leaves no line
    /// counted. The other lines count whole.
    #[test]
    fn lines_count_without_their_parts_under_chimneys_and_openings() {
        let base = examples::find("hip").unwrap().project();

        let mut p = base.clone();
        p.chimneys.push(square(5.0, -0.5, 6.0, 0.5));
        let d = assert_counted_add_up("chimney on the eave", &p);
        let front = d
            .iter()
            .find(|l| l.code == "T" && l.a[1] < 0.0 && l.b[1] < 0.0)
            .unwrap();
        assert_relative_eq!(front.length, 13.0, epsilon = 1e-3);
        assert_relative_eq!(front.counted_length, 12.0, epsilon = 1e-3);
        for l in d
            .iter()
            .filter(|l| l.code == "T" && !std::ptr::eq(*l, front))
        {
            assert_relative_eq!(l.counted_length, l.length, epsilon = 1e-9);
        }

        let mut p = base.clone();
        p.chimneys.push(square(5.5, 3.5, 6.5, 4.5));
        let d = assert_counted_add_up("chimney on the ridge", &p);
        let ridge = d.iter().find(|l| l.code == "F").unwrap();
        assert_relative_eq!(ridge.length, 4.0, epsilon = 1e-3);
        assert_relative_eq!(ridge.counted_length, 3.0, epsilon = 1e-3);

        let mut p = base.clone();
        p.roof_openings.push(square(-1.0, -1.0, 13.0, 9.0));
        let d = assert_counted_add_up("hole over the roof", &p);
        assert!(!d.is_empty());
        assert!(d.iter().all(|l| l.counted_length.abs() < 1e-6), "{d:?}");
    }

    /// Every example with a chimney, a hole or a dormer of each kind on it: the lines
    /// still add up to the results per code.
    #[test]
    fn counted_lengths_add_up_to_the_results_on_every_example() {
        use crate::edit::dormers::{DORMER_KINDS, build_dormer, default_params};
        for ex in examples::EXAMPLES {
            let base = ex.project();
            let pts = base.parts[0].outline();
            let x0 = pts.iter().fold(f64::MAX, |a, p| a.min(p.x));
            let y0 = pts.iter().fold(f64::MAX, |a, p| a.min(p.y));
            let y1 = pts.iter().fold(f64::MIN, |a, p| a.max(p.y));
            let mid_y = (y0 + y1) / 2.0;
            let mut p = base.clone();
            p.chimneys
                .push(square(x0 + 2.0, mid_y - 0.4, x0 + 2.8, mid_y + 0.4));
            assert_counted_add_up(&format!("{} chimney on the ridge", ex.id), &p);
            let mut p = base.clone();
            p.chimneys
                .push(square(x0 + 2.0, y0 - 0.5, x0 + 2.8, y0 + 0.3));
            assert_counted_add_up(&format!("{} chimney on the eave", ex.id), &p);
            let mut p = base.clone();
            p.roof_openings
                .push(square(x0 + 1.0, y0 - 1.0, x0 + 3.0, mid_y));
            assert_counted_add_up(&format!("{} hole", ex.id), &p);
            // Dormers up the slope and standing on the wall line (their
            // eave taken off the roof's eave on the overhang line, which no
            // dormer outline covers: audit HUNT-104).
            for (k, kind) in DORMER_KINDS.iter().enumerate() {
                for up in [1.2f64, 0.0, -0.5] {
                    let e0 = &base.parts[0].edges[0];
                    let host = e0.eave_height + up.max(0.0) * e0.pitch_deg.to_radians().tan();
                    let mut p = base.clone();
                    p.dormers.push(build_dormer(&default_params(
                        *kind,
                        Point2::new(x0 + 2.0 + k as f64 * 0.1, y0 + up),
                        0.0,
                        (host * 100.0).round() / 100.0 + 1.0,
                    )));
                    assert_counted_add_up(&format!("{} dormer {k} at {up}", ex.id), &p);
                }
            }
            // Chimneys on, across and beside the first and last verge.
            let x1 = pts.iter().fold(f64::MIN, |a, p| a.max(p.x));
            for x in [x0 - 0.6, x0 - 0.3, x0, x1 - 0.3, x1] {
                let mut p = base.clone();
                p.chimneys.push(square(x, y0 + 1.5, x + 0.6, y0 + 2.1));
                assert_counted_add_up(&format!("{} chimney at x {x}", ex.id), &p);
            }
        }
    }

    /// The faces a line names are the right ones: a dormer's own faces name
    /// no wall, a valley where two buildings meet names a face of each, and
    /// every face carries its wall's material.
    #[test]
    fn lines_name_their_faces_with_wall_and_material() {
        use crate::edit::dormers::{DormerKind, build_dormer, default_params};
        use crate::examples::{StandardRoof, standard_roof};

        // Dormer faces: no wall.
        let mut p = Project::new("dormer");
        p.parts.push(standard_roof(
            StandardRoof::Gable,
            12.0,
            8.0,
            40.0,
            3.0,
            0.0,
        ));
        p.dormers.push(build_dormer(&default_params(
            DormerKind::Gable,
            Point2::new(5.0, 1.2),
            0.0,
            5.0,
        )));
        let d = line_details(&p);
        let geos = crate::openings::generate_project(&p);
        let dormer_faces: Vec<usize> = (0..geos[0].faces.len())
            .filter(|&k| geos[0].faces[k].edge >= p.parts[0].edges.len())
            .collect();
        assert!(!dormer_faces.is_empty());
        let on_dormer: Vec<&LineFace> = d
            .iter()
            .flat_map(|l| &l.faces)
            .filter(|f| dormer_faces.contains(&f.face))
            .collect();
        assert!(!on_dormer.is_empty());
        assert!(on_dormer.iter().all(|f| f.wall.is_none()), "{on_dormer:?}");
        let on_roof = d
            .iter()
            .flat_map(|l| &l.faces)
            .filter(|f| !dormer_faces.contains(&f.face));
        for f in on_roof {
            let w = f.wall.expect("a roof face names its wall");
            assert_eq!(geos[0].faces[f.face].edge, w);
        }

        // Joined wing: a valley names a face of the house and one of the
        // wing; the wing's sheet metal shows on its faces.
        let mut p = Project::new("joined");
        p.parts.push(standard_roof(
            StandardRoof::Gable,
            12.0,
            8.0,
            40.0,
            3.0,
            0.5,
        ));
        let mut w = standard_roof(StandardRoof::Gable, 6.0, 5.0, 40.0, 3.0, 0.5);
        for e in w.edges.iter_mut() {
            let (x, y) = (e.start.x, e.start.y);
            e.start = Point2::new(3.0 + y, -4.0 + x);
            e.material = 2;
        }
        w.normalize_ccw();
        p.parts.push(w);
        let geos = crate::openings::generate_project(&p);
        assert!(!geos[0].joined_with.is_empty());
        let d = line_details_of(&p, &geos);
        let valleys: Vec<&LineDetail> = d.iter().filter(|l| l.code == "K").collect();
        assert!(!valleys.is_empty());
        for v in valleys {
            let parts: std::collections::BTreeSet<usize> = v.faces.iter().map(|f| f.part).collect();
            assert_eq!(parts.len(), 2, "{v:?}");
            for f in &v.faces {
                assert_eq!(f.material, geos[f.part].faces[f.face].material);
                assert_eq!(f.material, if f.part == 1 { 2 } else { 1 }, "{v:?}");
            }
        }
    }
}
