//! Geometric invariants of the roof engine on random roofs, checked against
//! first principles rather than against DACAD.
//!
//! * On a convex outline with one eave height and no gable, the roof is
//!   the lower envelope of its edge planes: above every plan point the roof
//!   is as high as the lowest plane there, and the face above the point
//!   belongs to that plane. Its faces tile the eave outline (the walls
//!   pushed out by their overhangs) and its eave runs round that outline.
//! * Where every plane has one pitch the area is the plan area over the
//!   cosine of the pitch, whatever the outline.
//! * Moving the walls out by the overhang and dropping the overhang gives
//!   the same roof; raising every eave by `dz` lifts the roof by `dz`.
//! * Random polygon soups and mutated project files never panic and give
//!   finite output (fuzzing).
//!
//! Every generated roof also passes the structural checks of
//! `support::check_structure` (planar faces at their pitch, areas equal to
//! their polygons, plan tiling, face sides covered by roof lines).
//!
//! Inputs stay clear of DACAD's own tolerances (overhangs under 10 cm,
//! pitches under 15°, edges under 1 m), where `properties.rs` and the DACAD
//! fixtures check the engine against DACAD's quirks instead.
//! `PROPTEST_CASES` sets the number of cases.

use std::panic::{AssertUnwindSafe, catch_unwind};

use proptest::prelude::*;
use tectora_core::geom::{Point2, signed_area};
use tectora_core::openings::generate_project;
use tectora_core::{EdgeDef, HalfHip, PitchBreak, Project};

mod support;
use support::*;

fn config(cases: u32) -> ProptestConfig {
    let cases = std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|c| c.parse().ok())
        .unwrap_or(cases);
    ProptestConfig {
        cases,
        // Ill-conditioned roofs and DACAD errors are rejected, not failed.
        max_global_rejects: cases * 4,
        max_local_rejects: 1 << 20,
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

fn describe(p: &Project) -> String {
    let mut s = String::new();
    for e in &p.parts[0].edges {
        s += &format!(
            "  edge({:?}, {:?}, {:?}, {:?}, {:?})\n",
            e.start.x, e.start.y, e.pitch_deg, e.eave_height, e.overhang
        );
    }
    s
}

/// Runs a [`Report`] check on one roof and turns its failures into a
/// proptest failure; DACAD errors reject the case.
fn verdict(
    p: &Project,
    roof: &Roof,
    check: impl FnOnce(&mut Case<'_>),
) -> Result<(), TestCaseError> {
    if let Some(e) = roof.geos.iter().find_map(|g| g.error.as_ref()) {
        return Err(TestCaseError::reject(format!("DACAD error {e:?}")));
    }
    let mut r = Report::default();
    let mut c = r.case();
    check(&mut c);
    c.done(String::new());
    if r.failures.is_empty() {
        Ok(())
    } else {
        Err(TestCaseError::fail(format!(
            "{}\ninput:\n{}",
            r.failures.join("\n"),
            describe(p)
        )))
    }
}

// ---------------------------------------------------------------- inputs

/// A convex polygon: vertices at sorted random angles on an ellipse, every
/// edge at least 1.5 m, no corner sharper than 40° or flatter than 170°.
fn convex_outline() -> impl Strategy<Value = Vec<Point2>> {
    (
        3.0..10.0f64,
        3.0..10.0f64,
        0.0..360.0f64,
        prop::collection::btree_set(0u32..72, 3..8),
    )
        .prop_filter_map("bad corner", |(rx, ry, rot, steps)| {
            let r = rot.to_radians();
            let pts: Vec<Point2> = steps
                .iter()
                .map(|&k| {
                    let t = f64::from(k) * 5f64.to_radians();
                    let (x, y) = (rx * t.cos(), ry * t.sin());
                    let q = |v: f64| (v * 1e3).round() / 1e3;
                    Point2::new(q(x * r.cos() - y * r.sin()), q(x * r.sin() + y * r.cos()))
                })
                .collect();
            let n = pts.len();
            for i in 0..n {
                let (a, b, c) = (pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n]);
                if a.distance(b) < 1.5 {
                    return None;
                }
                let u = (a.x - b.x, a.y - b.y);
                let v = (c.x - b.x, c.y - b.y);
                let ang = (u.0 * v.1 - u.1 * v.0)
                    .abs()
                    .atan2(u.0 * v.0 + u.1 * v.1)
                    .to_degrees();
                if !(40.0..=170.0).contains(&ang) {
                    return None;
                }
            }
            (signed_area(&pts) > 6.0).then_some(pts)
        })
}

/// A rectangle with rectangular notches in its bottom side, every edge at
/// least 1 m (whole decimetres).
fn orthogonal_outline() -> impl Strategy<Value = Vec<Point2>> {
    (
        6u32..20,
        5u32..14,
        prop::collection::vec((1u32..19, 1u32..6, 1u32..5), 0..3),
    )
        .prop_map(|(w, h, notches)| {
            let (w, h) = (f64::from(w), f64::from(h));
            let mut pts = vec![Point2::new(0.0, 0.0)];
            let mut x0 = 0.0;
            let mut ns = notches;
            ns.sort();
            for (a, width, depth) in ns {
                let a = f64::from(a);
                let b = a + f64::from(width);
                let d = f64::from(depth).min(h - 2.0);
                if a < x0 + 1.0 || b > w - 1.0 || d < 1.0 {
                    continue;
                }
                pts.extend([
                    Point2::new(a, 0.0),
                    Point2::new(a, d),
                    Point2::new(b, d),
                    Point2::new(b, 0.0),
                ]);
                x0 = b;
            }
            pts.extend([Point2::new(w, 0.0), Point2::new(w, h), Point2::new(0.0, h)]);
            pts
        })
}

/// Values as DACAD stores them: lengths to the millimetre, pitches to a
/// hundredth of a degree.
fn mm(v: f64) -> f64 {
    (v * 1e3).round() / 1e3
}

fn deg(v: f64) -> f64 {
    (v * 1e2).round() / 1e2
}

fn overhang() -> impl Strategy<Value = f64> {
    prop_oneof![1 => Just(0.0), 4 => (0.1..0.9f64).prop_map(mm)]
}

fn any_pitch(lo: f64, hi: f64) -> impl Strategy<Value = f64> {
    (lo..hi).prop_map(deg)
}

/// Pushes out every edge of a counter-clockwise polygon by its own
/// distance and intersects neighbouring lines.
fn offset_each(pts: &[Point2], d: &[f64]) -> Vec<Point2> {
    let n = pts.len();
    let line = |i: usize| {
        let (a, b) = (pts[i % n], pts[(i + 1) % n]);
        let len = a.distance(b);
        let (nx, ny) = ((b.y - a.y) / len, -(b.x - a.x) / len);
        let o = d[i % n];
        (
            Point2::new(a.x + nx * o, a.y + ny * o),
            (b.x - a.x, b.y - a.y),
        )
    };
    (0..n)
        .map(|i| {
            let (p0, d0) = line(i + n - 1);
            let (p1, d1) = line(i);
            let den = d0.0 * d1.1 - d0.1 * d1.0;
            let t = ((p1.x - p0.x) * d1.1 - (p1.y - p0.y) * d1.0) / den;
            Point2::new(p0.x + d0.0 * t, p0.y + d0.1 * t)
        })
        .collect()
}

fn perimeter(pts: &[Point2]) -> f64 {
    (0..pts.len())
        .map(|i| pts[i].distance(pts[(i + 1) % pts.len()]))
        .sum()
}

/// Height of edge `i`'s roof plane above a plan point.
fn plane_z(e: &EdgeDef, next: Point2, x: f64, y: f64) -> f64 {
    let (a, b) = (e.start, next);
    let len = a.distance(b);
    let inward = ((x - a.x) * -(b.y - a.y) + (y - a.y) * (b.x - a.x)) / len;
    e.eave_height + tan(e.pitch_deg) * (inward + e.overhang)
}

/// Clips a convex polygon to `a·x + b·y + c <= 0`.
fn clip(poly: &[Point2], a: f64, b: f64, c: f64) -> Vec<Point2> {
    let f = |p: Point2| a * p.x + b * p.y + c;
    let n = poly.len();
    let mut out = Vec::new();
    for i in 0..n {
        let (p, q) = (poly[i], poly[(i + 1) % n]);
        let (fp, fq) = (f(p), f(q));
        if fp <= 0.0 {
            out.push(p);
        }
        if (fp < 0.0 && fq > 0.0) || (fp > 0.0 && fq < 0.0) {
            let t = fp / (fp - fq);
            out.push(Point2::new(p.x + (q.x - p.x) * t, p.y + (q.y - p.y) * t));
        }
    }
    out
}

/// Plane of edge `i` as `z = gx·x + gy·y + g0`.
fn plane(edges: &[EdgeDef], i: usize) -> (f64, f64, f64) {
    let n = edges.len();
    let (a, b) = (edges[i].start, edges[(i + 1) % n].start);
    let len = a.distance(b);
    let t = tan(edges[i].pitch_deg);
    let (gx, gy) = (-(b.y - a.y) / len * t, (b.x - a.x) / len * t);
    let g0 = edges[i].eave_height + t * edges[i].overhang - gx * a.x - gy * a.y;
    (gx, gy, g0)
}

/// The plan region where edge `i`'s plane is the lowest, inside the eave
/// outline: an independent reference for convex roofs.
fn envelope_region(edges: &[EdgeDef], eaves: &[Point2], i: usize) -> Vec<Point2> {
    let pi = plane(edges, i);
    let mut poly = eaves.to_vec();
    for j in 0..edges.len() {
        if j != i && !poly.is_empty() {
            let pj = plane(edges, j);
            poly = clip(&poly, pi.0 - pj.0, pi.1 - pj.1, pi.2 - pj.2);
        }
    }
    poly
}

/// Whether a convex roof stays clear of DACAD's tolerances: its corners
/// (from the reference envelope) are either one point or at least 10 cm
/// apart in plan, and either at one height or at least 3 cm apart. DACAD's
/// core merges points within 2 cm (POINT_EPS) and takes events within
/// 1 cm of height (VALUE_EPS) as one sweep level, which moves faces by
/// that much; the engine does the same on purpose.
fn well_conditioned(edges: &[EdgeDef], eaves: &[Point2]) -> bool {
    let mut pts: Vec<(Point2, f64)> = Vec::new();
    for i in 0..edges.len() {
        let pi = plane(edges, i);
        for v in envelope_region(edges, eaves, i) {
            pts.push((v, pi.0 * v.x + pi.1 * v.y + pi.2));
        }
    }
    let eave = edges[0].eave_height;
    for (k, &(a, za)) in pts.iter().enumerate() {
        for &(b, zb) in &pts[k + 1..] {
            let d = a.distance(b);
            if d > 1e-6 && d < 0.1 {
                return false;
            }
            let dz = (za - zb).abs();
            if za > eave + 1e-6 && zb > eave + 1e-6 && dz > 1e-6 && dz < 0.03 {
                return false;
            }
        }
    }
    true
}

/// Overhangs of a generated roof.
#[derive(Clone, Copy, Debug)]
enum Overhangs {
    /// None: every wall top is at the eave.
    Zero,
    /// One overhang and one pitch everywhere: every wall top at one height.
    Uniform,
    /// Any overhang and pitch per edge. DACAD keeps an overhanging wall
    /// vertical up to its wall top (eave + overhang·tan pitch) before its
    /// plane moves in (dcore `s1::hidden_height`), so with wall tops at
    /// different heights its roof is not the lower envelope of the planes.
    Mixed,
}

/// A convex roof with one eave height, pitches from `min_pitch` and
/// overhangs (each at most a third of its neighbouring edges) as `mode`
/// says, with its eave outline.
fn convex_roof(min_pitch: f64, mode: Overhangs) -> impl Strategy<Value = (Project, Vec<Point2>)> {
    (
        convex_outline().prop_flat_map(move |pts| {
            let n = pts.len();
            (
                Just(pts),
                prop::collection::vec((any_pitch(min_pitch, 70.0), overhang()), n),
            )
        }),
        (2.0..6.0f64).prop_map(mm),
    )
        .prop_map(move |((pts, mut defs), eave)| {
            let n = pts.len();
            match mode {
                Overhangs::Zero => defs.iter_mut().for_each(|d| d.1 = 0.0),
                Overhangs::Uniform => {
                    let first = defs[0];
                    defs.iter_mut().for_each(|d| *d = first);
                }
                Overhangs::Mixed => {}
            }
            let len = |i: usize| pts[i % n].distance(pts[(i + 1) % n]);
            let ovs: Vec<f64> = (0..n)
                .map(|i| {
                    mm(defs[i]
                        .1
                        .min(len(i + n - 1).min(len(i)).min(len(i + 1)) / 3.0))
                })
                .collect();
            let ovs = match mode {
                // The smallest limit applies to every edge.
                Overhangs::Uniform => vec![ovs.iter().copied().fold(f64::MAX, f64::min); n],
                _ => ovs,
            };
            let p = project(
                (0..n)
                    .map(|i| edge(pts[i].x, pts[i].y, defs[i].0, eave, ovs[i]))
                    .collect(),
            );
            (p, offset_each(&pts, &ovs))
        })
        .prop_filter("an overhang swallows its edge", |(p, eaves)| {
            // Every eave keeps its wall's direction and at least 0.5 m.
            let walls = p.parts[0].outline();
            let n = walls.len();
            (0..n).all(|i| {
                let (a, b) = (walls[i], walls[(i + 1) % n]);
                let (c, d) = (eaves[i], eaves[(i + 1) % n]);
                let dot = (b.x - a.x) * (d.x - c.x) + (b.y - a.y) * (d.y - c.y);
                dot > 0.0 && c.distance(d) > 0.5
            })
        })
}

/// Largest relative error of the reported roof area against the exact
/// geometry allowed on random roofs (see `census_of_area_artefacts`).
const AREA_ARTEFACT: f64 = 5e-3;

/// How far DACAD's core may move a roof corner in plan: it merges points
/// within POINT_EPS = 2 cm (plus float32 noise). On a plane at pitch `p`
/// that is `EPS·tan(p)` in height.
const EPS: f64 = 0.025;

fn perimeter3(poly: &[[f64; 3]]) -> f64 {
    let n = poly.len();
    (0..n)
        .map(|i| {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt()
        })
        .sum()
}

// ---------------------------------------------------------------- properties

proptest! {
    #![proptest_config(config(400))]

    /// Convex outline, one eave height, any pitches without overhangs (or one
    /// pitch and overhang): the roof is the lower envelope of the edge
    /// planes. Pitches start at 25°: DACAD's sweep resolves
    /// heights to VALUE_EPS = 1 cm, which is 1 cm/tan(pitch) in plan, and
    /// shallower roofs show that (see `census_of_area_artefacts`).
    #[test]
    fn convex_roof_is_lower_envelope((p, eaves) in prop_oneof![
        3 => convex_roof(25.0, Overhangs::Zero),
        1 => convex_roof(25.0, Overhangs::Uniform),
    ]) {
        let n = eaves.len();
        let edges = &p.parts[0].edges;
        prop_assume!(well_conditioned(edges, &eaves), "within DACAD's tolerances");
        let roof = calc(&p);
        let plan = signed_area(&eaves);
        let env = |x: f64, y: f64| {
            (0..n)
                .map(|i| (plane_z(&edges[i], edges[(i + 1) % n].start, x, y), i))
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .unwrap()
        };
        let steep = edges.iter().map(|e| tan(e.pitch_deg)).fold(0.0, f64::max);
        let ztol = EPS * steep + 2e-3;
        verdict(&p, &roof, |c| {
            check_structure_fit(c, &roof, plan, Fit::Dacad);
            c.eq_tol("T", roof.line("T"), perimeter(&eaves), 2e-3 * n as f64);
            c.only(&roof, &["T", "F", "G"]);
            let mut want = 0.0;
            for f in roof.faces() {
                for v in &f.polygon {
                    let (z, _) = env(v[0], v[1]);
                    c.eq_tol(&format!("face {} vertex z", f.edge), v[2], z, ztol);
                }
                // A point well inside the face: the centroid of its plan.
                let k = f.polygon.len() as f64;
                let cx = f.polygon.iter().map(|v| v[0]).sum::<f64>() / k;
                let cy = f.polygon.iter().map(|v| v[1]).sum::<f64>() / k;
                let (z, lowest) = env(cx, cy);
                let own = plane_z(&edges[f.edge], edges[(f.edge + 1) % n].start, cx, cy);
                c.eq_tol(&format!("face {} is not the lowest plane {lowest}", f.edge), own, z, ztol);
                want += plan_of(&f.polygon) / cos(f.pitch_deg);
            }
            // Independent reference: clip the eave outline to where each
            // plane is lowest. The face polygons must match it up to a
            // sliver EPS wide along each region's outline.
            let (mut total, mut slack) = (0.0, 0.0);
            for i in 0..n {
                let region = envelope_region(edges, &eaves, i);
                let area = signed_area(&region).abs() / cos(edges[i].pitch_deg);
                let tol = 5e-4 + EPS * perimeter(&region) / cos(edges[i].pitch_deg);
                total += area;
                slack += tol;
                let got: f64 = roof
                    .faces()
                    .filter(|f| f.edge == i)
                    .map(|f| plan_of(&f.polygon) / cos(f.pitch_deg))
                    .sum();
                c.eq_tol(&format!("face {i} polygon area"), got, area, tol);
            }
            c.eq_tol("polygon area", want, total, slack);
            // The reported area (DACAD's, from the faces' edge lists).
            c.eq_tol("area", roof.area, total, AREA_ARTEFACT * total);
        })?;
    }

    /// One pitch on every edge of an outline with notches: the area is the
    /// plan of the eave outline over the cosine, whatever the hips and
    /// valleys do; every face is at that pitch.
    #[test]
    fn one_pitch_area_is_plan_over_cos(
        pts in orthogonal_outline(),
        pitch in any_pitch(20.0, 65.0),
        ov in overhang().prop_map(|o| o.min(0.45)),
        eave in 2.0..6.0f64,
    ) {
        let p = project(pts.iter().map(|v| edge(v.x, v.y, pitch, eave, ov)).collect());
        let roof = calc(&p);
        let plan = signed_area(&offset_each(&pts, &vec![ov; pts.len()]));
        verdict(&p, &roof, |c| {
            check_structure_fit(c, &roof, plan, Fit::Dacad);
            c.eq_tol("area", roof.area, plan / cos(pitch), 1e-4 * plan);
            c.eq_tol("T", roof.line("T"), perimeter(&offset_each(&pts, &vec![ov; pts.len()])), 2e-3 * pts.len() as f64);
            c.only(&roof, &["T", "F", "G", "K"]);
            for f in roof.faces() {
                // The core reads pitches with four decimals.
                c.eq_tol("face pitch", f.pitch_deg, pitch, 1e-4);
            }
        })?;
    }

    /// Gables and hips at one pitch mixed on an outline with notches, one
    /// overhang: the faces still tile the eave outline and every face side
    /// is a roof line.
    #[test]
    fn mixed_roof_structure(
        (pts, gables) in orthogonal_outline().prop_flat_map(|pts| {
            let n = pts.len();
            (Just(pts), prop::collection::vec(prop::bool::weighted(0.3), n))
        }),
        pitch in prop_oneof![any_pitch(20.0, 60.0), Just(30.0), Just(45.0)],
        ov in overhang().prop_map(|o| o.min(0.45)),
    ) {
        let p = project(pts.iter().zip(&gables).map(|(v, &g)| edge(v.x, v.y, if g { 90.0 } else { pitch }, 3.0, ov)).collect());
        let roof = calc(&p);
        let plan = signed_area(&offset_each(&pts, &vec![ov; pts.len()]));
        verdict(&p, &roof, |c| check_structure_fit(c, &roof, plan, Fit::Dacad))?;
    }

    /// Walls pushed out by the overhang with no overhang give the same roof
    /// as the walls with the overhang (one pitch and overhang: with mixed
    /// ones DACAD's wall tops differ, see [`Overhangs::Mixed`]).
    #[test]
    fn overhang_is_an_offset_of_the_walls((p, eaves) in convex_roof(25.0, Overhangs::Uniform)) {
        prop_assume!(well_conditioned(&p.parts[0].edges, &eaves), "within DACAD's tolerances");
        let a = calc(&p);
        prop_assume!(a.geos.iter().all(|g| g.error.is_none()), "DACAD error");
        let mut q = p.clone();
        for (e, v) in q.parts[0].edges.iter_mut().zip(&eaves) {
            // DACAD keeps four decimals.
            e.start = Point2::new((v.x * 1e4).round() / 1e4, (v.y * 1e4).round() / 1e4);
            e.overhang = 0.0;
        }
        let b = calc(&q);
        verdict(&q, &b, |c| {
            same_roof(c, &a, &b);
            let clean = c.clean();
            c.ok(&format!("with overhangs:\n{}", describe(&p)), clean);
        })?;
    }

    /// Whatever DACAD does with overhangs, a roof stays above its building:
    /// every face corner lies inside the eave outline and the faces do not
    /// overlap (their plan areas add up to at most the outline's).
    #[test]
    #[ignore = "candidate bug, needs a DACAD run: with mixed overhangs and pitches about 1 roof in 3000 gets hips running outside the building and overlapping faces (repro: core-tests/dacad-candidates/C1_hips_outside.json in the project files, area 60.35 m2 on a 35.2 m2 roof)"]
    fn roof_stays_inside_its_eave_outline((p, eaves) in convex_roof(15.0, Overhangs::Mixed)) {
        let roof = calc(&p);
        let plan = signed_area(&eaves);
        let n = eaves.len();
        verdict(&p, &roof, |c| {
            let mut sum = 0.0;
            for f in roof.faces() {
                sum += plan_of(&f.polygon);
                for v in &f.polygon {
                    // Distance inside every eave line (negative = outside).
                    let inside = (0..n)
                        .map(|i| {
                            let (a, b) = (eaves[i], eaves[(i + 1) % n]);
                            ((v[0] - a.x) * -(b.y - a.y) + (v[1] - a.y) * (b.x - a.x)) / a.distance(b)
                        })
                        .fold(f64::MAX, f64::min);
                    c.ok(&format!("face {} corner {v:?} {:.3} m outside", f.edge, -inside), inside > -EPS);
                }
            }
            c.ok(&format!("faces overlap: {sum:.3} m2 on {plan:.3} m2"), sum < plan * (1.0 + 1e-3) + 0.01);
        })?;
    }

    /// Raising every eave by `dz` lifts the roof by `dz` and changes nothing
    /// else.
    #[test]
    fn raising_the_eaves_lifts_the_roof(
        (p, eaves) in convex_roof(25.0, Overhangs::Mixed),
        dz in prop_oneof![Just(1.0), Just(-1.5), (0.1..5.0f64).prop_map(mm)],
    ) {
        prop_assume!(well_conditioned(&p.parts[0].edges, &eaves), "within DACAD's tolerances");
        let a = calc(&p);
        let mut q = p.clone();
        for e in &mut q.parts[0].edges {
            e.eave_height += dz;
        }
        let b = calc(&q);
        prop_assume!(a.geos.iter().all(|g| g.error.is_none()), "DACAD error");
        let steep = p.parts[0].edges.iter().map(|e| tan(e.pitch_deg)).fold(0.0, f64::max);
        verdict(&q, &b, |c| {
            same_roof(c, &a, &b);
            c.eq_tol("top", b.max_z(), a.max_z() + dz, EPS * steep + 2e-3);
        })?;
    }
}

/// Two calculations of one roof agree up to DACAD's point tolerance: each
/// line total within EPS per line end, the area within a sliver EPS wide
/// along every face.
fn same_roof(c: &mut Case<'_>, a: &Roof, b: &Roof) {
    let sliver: f64 = a.faces().map(|f| EPS * perimeter3(&f.polygon)).sum::<f64>() / 2.0;
    c.eq_tol("area", b.area, a.area, 1e-4 * a.area + sliver);
    let count = |r: &Roof, code: &str| {
        r.geos
            .iter()
            .flat_map(|g| &g.lines)
            .filter(|l| l.dacad_code() == code)
            .count()
    };
    for code in a.lines.keys().chain(b.lines.keys()) {
        let ends = 2 * count(a, code).max(count(b, code)).max(1);
        c.eq_tol(code, b.line(code), a.line(code), EPS * ends as f64);
    }
}

fn plan_of(poly: &[[f64; 3]]) -> f64 {
    let n = poly.len();
    (0..n)
        .map(|i| poly[i][0] * poly[(i + 1) % n][1] - poly[(i + 1) % n][0] * poly[i][1])
        .sum::<f64>()
        .abs()
        / 2.0
}

// ---------------------------------------------------------------- fuzzing

/// Any edge definition, including the rarely used ones.
fn wild_edge() -> impl Strategy<Value = EdgeDef> {
    (
        (-30.0..30.0f64, -30.0..30.0f64),
        prop_oneof![
            5.0..80.0f64,
            Just(90.0),
            90.0..150.0f64,
            Just(0.5),
            Just(89.9999)
        ],
        -2.0..12.0f64,
        prop_oneof![Just(0.0), 0.0..3.0f64, Just(1e-4)],
        prop::option::of((0.0..12.0f64, 5.0..85.0f64)),
        prop::collection::vec((0.0..14.0f64, 0.0..95.0f64), 0..3),
        0u8..2,
        prop::option::of(-7.0..7.0f64),
        any::<bool>(),
    )
        .prop_map(|((x, y), pitch, eave, ov, hh, breaks, prio, dir, clad)| {
            let mut e = edge(x, y, pitch, eave, ov);
            e.half_hip = hh.map(|(height, pitch_deg)| HalfHip { height, pitch_deg });
            e.breaks = breaks
                .into_iter()
                .map(|(height, pitch_deg)| PitchBreak { height, pitch_deg })
                .collect();
            e.priority = prio;
            e.direction_rad = dir;
            e.clad = clad;
            e
        })
}

proptest! {
    #![proptest_config(config(600))]

    /// Polygon soups: random points (self-intersecting, collinear, repeated)
    /// with random definitions, one or two parts. No panic, finite output.
    #[test]
    fn polygon_soup_never_panics(
        parts in prop::collection::vec(prop::collection::vec(wild_edge(), 3..10), 1..3),
    ) {
        let mut p = project(parts[0].clone());
        for (i, edges) in parts.iter().enumerate().skip(1) {
            let mut extra = p.parts[0].clone();
            extra.name = format!("P{i}");
            extra.edges = edges.clone();
            p.parts.push(extra);
        }
        let geos = catch_unwind(AssertUnwindSafe(|| generate_project(&p)))
            .map_err(|_| TestCaseError::fail(format!("panic on\n{}", describe(&p))))?;
        for g in &geos {
            let finite = g.faces.iter().all(|f| f.area.is_finite() && f.polygon.iter().flatten().all(|v| v.is_finite()))
                && g.lines.iter().all(|l| l.a.iter().chain(&l.b).all(|v| v.is_finite()))
                && g.clad_area.is_finite();
            prop_assert!(finite, "non-finite output on\n{}", describe(&p));
        }
    }

    /// Project files with numbers replaced at random: loading never panics,
    /// and a file that loads calculates without a panic.
    #[test]
    fn mutated_project_json_never_panics(
        picks in prop::collection::vec((any::<prop::sample::Index>(), prop_oneof![
            Just("0".to_string()), Just("-1".to_string()), Just("1e308".to_string()),
            Just("-1e308".to_string()), Just("90".to_string()), Just("89.9999".to_string()),
            Just("null".to_string()), Just("\"x\"".to_string()), Just("[]".to_string()),
            (-1e4..1e4f64).prop_map(|v| format!("{v}")),
        ]), 1..6),
    ) {
        let base = rect(10.0, 6.0, [(40.0, 3.0, 0.5), (90.0, 3.0, 0.3), (40.0, 3.0, 0.5), (90.0, 3.0, 0.3)]);
        let json = base.to_json().unwrap();
        // Spans of the numbers in the file.
        let bytes = json.as_bytes();
        let mut spans = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'-' || bytes[i].is_ascii_digit() {
                let s = i;
                while i < bytes.len() && (bytes[i].is_ascii_digit() || b"-+.eE".contains(&bytes[i])) {
                    i += 1;
                }
                spans.push((s, i));
            } else {
                i += 1;
            }
        }
        let mut chosen: Vec<(usize, String)> = picks.iter().map(|(ix, v)| (ix.index(spans.len()), v.clone())).collect();
        chosen.sort_by_key(|c| std::cmp::Reverse(c.0));
        chosen.dedup_by_key(|c| c.0);
        let mut text = json.clone();
        for (k, v) in chosen {
            let (s, e) = spans[k];
            text.replace_range(s..e, &v);
        }
        let loaded = catch_unwind(|| Project::from_json(&text))
            .map_err(|_| TestCaseError::fail(format!("from_json panicked on\n{text}")))?;
        if let Ok(p) = loaded {
            catch_unwind(AssertUnwindSafe(|| generate_project(&p)))
                .map_err(|_| TestCaseError::fail(format!("engine panicked on\n{text}")))?;
        }
    }
}

// ---------------------------------------------------------------- census

/// Exact (reference) area of a convex roof from the lower envelope.
fn reference_area(p: &Project, eaves: &[Point2]) -> f64 {
    let edges = &p.parts[0].edges;
    (0..edges.len())
        .map(|i| signed_area(&envelope_region(edges, eaves, i)).abs() / cos(edges[i].pitch_deg))
        .sum()
}

/// Survey, not a pass/fail test: over many random convex roofs (no
/// filtering), how far the reported area and the face polygons are from the
/// exact lower envelope of the planes, per overhang mode. Prints histograms
/// and the worst cases; with `TECTORA_CENSUS_DIR` set it writes them there
/// as project JSON for a DACAD run (`engine_check --rd2` writes their RD2). `CENSUS_CASES` sets the number of
/// roofs per mode (default 3000).
///
/// `cargo test --release --test invariants census -- --ignored --nocapture`
#[test]
#[ignore = "survey; run by hand"]
fn census_of_area_artefacts() {
    use proptest::strategy::ValueTree;
    use proptest::test_runner::TestRunner;
    let cases: usize = std::env::var("CENSUS_CASES")
        .ok()
        .and_then(|c| c.parse().ok())
        .unwrap_or(3000);
    let dir = std::env::var("TECTORA_CENSUS_DIR").ok();
    let buckets = [1e-5, 1e-4, 1e-3, 3e-3, 1e-2, 3e-2, f64::MAX];
    let label = |i: usize| {
        if i == 0 {
            format!("<= {:e}", buckets[0])
        } else if buckets[i] == f64::MAX {
            format!(" > {:e}", buckets[i - 1])
        } else {
            format!("<= {:e}", buckets[i])
        }
    };
    for mode in [Overhangs::Zero, Overhangs::Uniform, Overhangs::Mixed] {
        let mut runner = TestRunner::deterministic();
        let strategy = convex_roof(15.0, mode);
        let mut reported = vec![0usize; buckets.len()];
        let mut polygons = vec![0usize; buckets.len()];
        let (mut errors, mut conditioned, mut conditioned_bad) = (0, 0, 0);
        let mut worst: Vec<(f64, f64, Project)> = Vec::new();
        let mut done = 0;
        while done < cases {
            let Ok(tree) = strategy.new_tree(&mut runner) else {
                continue;
            };
            done += 1;
            let (p, eaves) = tree.current();
            let roof = calc(&p);
            if roof.geos.iter().any(|g| g.error.is_some()) {
                errors += 1;
                continue;
            }
            let want = reference_area(&p, &eaves);
            let poly: f64 = roof
                .faces()
                .map(|f| plan_of(&f.polygon) / cos(f.pitch_deg))
                .sum();
            let rel = (roof.area - want).abs() / want;
            let rel_poly = (poly - want).abs() / want;
            reported[buckets.iter().position(|&b| rel <= b).unwrap()] += 1;
            polygons[buckets.iter().position(|&b| rel_poly <= b).unwrap()] += 1;
            if well_conditioned(&p.parts[0].edges, &eaves) {
                conditioned += 1;
                if rel > 1e-4 {
                    conditioned_bad += 1;
                }
            }
            worst.push((rel, rel_poly, p));
        }
        worst.sort_by(|a, b| b.0.total_cmp(&a.0));
        eprintln!("\n== overhangs {mode:?}: {cases} convex roofs, {errors} with a DACAD error");
        eprintln!("relative error   reported area   face polygons");
        for i in 0..buckets.len() {
            eprintln!(
                "{:>14}   {:>13}   {:>13}",
                label(i),
                reported[i],
                polygons[i]
            );
        }
        eprintln!(
            "well-conditioned: {conditioned}, of which {conditioned_bad} with area error > 1e-4"
        );
        for (k, (rel, rel_poly, p)) in worst.iter().take(5).enumerate() {
            eprintln!(
                "#{k}: reported {:.4} %, polygons {:.4} %\n{}",
                rel * 100.0,
                rel_poly * 100.0,
                describe(p)
            );
        }
        if let Some(d) = &dir {
            std::fs::create_dir_all(d).unwrap();
            for (k, (_, _, p)) in worst.iter().take(20).enumerate() {
                std::fs::write(
                    format!("{d}/census_{mode:?}_{k:02}.json"),
                    p.to_json().unwrap(),
                )
                .unwrap();
            }
        }
    }
}
