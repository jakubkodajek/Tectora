//! Property-based tests of the roof engine.
//!
//! These check invariants that hold whatever DACAD does: the engine never
//! panics, its output is finite, closed-form roofs have their textbook
//! quantities, and the result does not depend on how the same roof is
//! entered (start vertex, orientation, position, rotation, mirroring).
//! DACAD parity itself is tested in `dacad_cases.rs`.

use std::collections::BTreeMap;
use std::f64::consts::PI;
use std::panic::{AssertUnwindSafe, catch_unwind};

use proptest::prelude::*;
use tectora_core::geom::{Point2, signed_area};
use tectora_core::openings::generate_project;
use tectora_core::roof::RoofGeometry;
use tectora_core::{Chimney, EdgeDef, Project, RoofPart, RoofWindow};

fn edge(start: Point2, pitch_deg: f64, eave_height: f64, overhang: f64) -> EdgeDef {
    EdgeDef {
        start,
        pitch_deg,
        eave_height,
        overhang,
        half_hip: None,
        breaks: Vec::new(),
        priority: 0,
        direction_rad: None,
        clad: false,
        source: None,
        material: tectora_core::model::DEFAULT_MATERIAL,
    }
}

fn project(edges: Vec<EdgeDef>) -> Project {
    let mut part = RoofPart {
        name: "P".into(),
        edges,
    };
    part.normalize_ccw();
    let mut p = Project::new("prop");
    p.parts.push(part);
    p
}

/// One line per edge: `(x, y) pitch eave overhang`, plus openings.
fn describe(p: &Project) -> String {
    let mut s = String::new();
    for part in &p.parts {
        for e in &part.edges {
            s += &format!(
                "  ({:?}, {:?}) p={:?} h={:?} o={:?} dir={:?}\n",
                e.start.x, e.start.y, e.pitch_deg, e.eave_height, e.overhang, e.direction_rad
            );
        }
    }
    for c in &p.chimneys {
        s += &format!(
            "  chimney {:?}\n",
            c.corners.iter().map(|v| (v.x, v.y)).collect::<Vec<_>>()
        );
    }
    for w in &p.windows {
        s += &format!("  window {:?}\n", (w.x, w.y, w.width, w.height));
    }
    s
}

/// Runs the engine and turns a panic into an `Err` carrying the input.
fn run(p: &Project) -> Result<Vec<RoofGeometry>, String> {
    catch_unwind(AssertUnwindSafe(|| generate_project(p)))
        .map_err(|_| format!("engine panicked on\n{}", describe(p)))
}

/// Line totals by DACAD code (minus the negative lines) and the roof area.
fn totals(geos: &[RoofGeometry]) -> (BTreeMap<String, f64>, f64) {
    let mut lines: BTreeMap<String, f64> = BTreeMap::new();
    for l in geos.iter().flat_map(|g| &g.lines) {
        *lines.entry(l.dacad_code().to_string()).or_default() += l.length();
    }
    for (code, len) in geos.iter().flat_map(|g| &g.minus_lines) {
        *lines.entry(code.clone()).or_default() -= len;
    }
    lines.retain(|_, v| v.abs() > 1e-9);
    let area = geos
        .iter()
        .map(|g| g.faces.iter().map(|f| f.area).sum::<f64>() + g.clad_area)
        .sum();
    (lines, area)
}

/// Runs the engine and rejects the case when DACAD itself reports an
/// error for it (the roof then has no faces or lines): the properties
/// below are about roofs DACAD calculates. Which inputs DACAD refuses is
/// its own behaviour, checked in `dacad_cases.rs`.
fn roof(p: &Project) -> Result<Vec<RoofGeometry>, TestCaseError> {
    let geos = run(p).map_err(|e| fail(p, e))?;
    if let Some(e) = geos.iter().find_map(|g| g.error.as_ref()) {
        return Err(TestCaseError::reject(format!("DACAD error {e}")));
    }
    Ok(geos)
}

fn fail(p: &Project, e: String) -> TestCaseError {
    TestCaseError::fail(format!("{e}\ninput:\n{}", describe(p)))
}

fn finite(geos: &[RoofGeometry]) -> Result<(), String> {
    for g in geos {
        for f in &g.faces {
            if !f.area.is_finite() || f.polygon.iter().flatten().any(|v| !v.is_finite()) {
                return Err(format!("non-finite face {f:?}"));
            }
        }
        for l in &g.lines {
            if l.a.iter().chain(&l.b).any(|v| !v.is_finite()) {
                return Err(format!("non-finite line {l:?}"));
            }
        }
        if !g.clad_area.is_finite() {
            return Err("non-finite clad area".into());
        }
    }
    Ok(())
}

/// Totals of one roof entered in two ways may differ by DACAD's own
/// resolution: near-simultaneous sweep events within its centimetre
/// tolerances are taken in input order (see core-stress/findings.md).
const LINE_TOL: f64 = 0.011;
const AREA_TOL: f64 = 0.02;

/// Relative tolerance of closed-form quantities: DACAD's core reads the
/// RD2 with four decimals and writes its results as f32.
const CLOSED_TOL: f64 = 3e-4;

/// Compares two totals; `swap` exchanges OL and OR (a mirrored roof).
fn same_totals(
    x: &(BTreeMap<String, f64>, f64),
    y: &(BTreeMap<String, f64>, f64),
    swap: bool,
) -> Result<(), String> {
    if (x.1 - y.1).abs() > AREA_TOL {
        return Err(format!("area {} vs {}", x.1, y.1));
    }
    let map = |c: &str| -> String {
        match (swap, c) {
            (true, "OL") => "OR".into(),
            (true, "OR") => "OL".into(),
            _ => c.into(),
        }
    };
    let ym: BTreeMap<String, f64> = y.0.iter().map(|(k, v)| (map(k), *v)).collect();
    for code in x.0.keys().chain(ym.keys()) {
        let a = x.0.get(code).copied().unwrap_or(0.0);
        let b = ym.get(code).copied().unwrap_or(0.0);
        if (a - b).abs() > LINE_TOL {
            return Err(format!("{code}: {a} vs {b}\n{:?}\n{:?}", x.0, ym));
        }
    }
    Ok(())
}

fn transform(p: &Project, f: impl Fn(Point2) -> Point2, dir: impl Fn(f64) -> f64) -> Project {
    let mut q = p.clone();
    for part in &mut q.parts {
        for e in &mut part.edges {
            e.start = f(e.start);
            e.direction_rad = e.direction_rad.map(&dir);
        }
    }
    for c in q.chimneys.iter_mut().chain(&mut q.roof_openings) {
        for v in &mut c.corners {
            *v = f(*v);
        }
    }
    for w in &mut q.windows {
        let v = f(Point2::new(w.x, w.y));
        (w.x, w.y) = (v.x, v.y);
    }
    for d in &mut q.dormers {
        for e in &mut d.edges {
            e.start = f(e.start);
            e.direction_rad = e.direction_rad.map(&dir);
        }
        d.end = f(d.end);
    }
    q
}

/// Reverses the order of every part's edges and mirrors it in x, which
/// keeps the outline counter-clockwise.
fn mirrored(p: &Project) -> Project {
    let mut q = transform(p, |v| Point2::new(-v.x, v.y), |a| PI - a);
    for part in &mut q.parts {
        let n = part.edges.len();
        let starts: Vec<Point2> = part.edges.iter().map(|e| e.start).collect();
        let mut edges = Vec::with_capacity(n);
        for i in (0..n).rev() {
            let mut e = part.edges[i].clone();
            e.start = starts[(i + 1) % n];
            e.direction_rad = e.direction_rad.map(|a| a + PI);
            edges.push(e);
        }
        part.edges = edges;
    }
    q
}

// ---------------------------------------------------------------- inputs

fn pitch() -> impl Strategy<Value = f64> {
    prop_oneof![
        6 => 10.0..70.0f64,
        2 => Just(90.0),
        1 => prop_oneof![Just(30.0), Just(45.0), Just(60.0)],
    ]
}

/// An orthogonal outline: a rectangle with notches cut into its sides,
/// counter-clockwise, all edges at least `min_edge` long.
fn orthogonal_outline() -> impl Strategy<Value = Vec<Point2>> {
    (
        4.0..20.0f64,
        4.0..16.0f64,
        prop::collection::vec((0.15..0.85f64, 0.1..0.4f64, 0.1..0.45f64), 0..3),
    )
        .prop_map(|(w, h, notches)| {
            // Notches only on the bottom side, left to right, not overlapping.
            let mut pts = vec![Point2::new(0.0, 0.0)];
            let mut x0 = 0.0;
            let mut ns = notches;
            ns.sort_by(|a, b| a.0.total_cmp(&b.0));
            for (c, half, depth) in ns {
                let a = (c - half / 2.0) * w;
                let b = (c + half / 2.0) * w;
                if a < x0 + 1.0 || b > w - 1.0 || b - a < 1.0 {
                    continue;
                }
                // Whole decimetres: notch backs a centimetre apart fall under
                // DACAD's step-snapping rule (core-stress/cases/S04).
                let d = (depth * h * 10.0).round() / 10.0;
                pts.push(Point2::new(a, 0.0));
                pts.push(Point2::new(a, d));
                pts.push(Point2::new(b, d));
                pts.push(Point2::new(b, 0.0));
                x0 = b;
            }
            pts.push(Point2::new(w, 0.0));
            pts.push(Point2::new(w, h));
            pts.push(Point2::new(0.0, h));
            pts
        })
}

/// A convex outline with vertices at random angles on an ellipse.
fn convex_outline() -> impl Strategy<Value = Vec<Point2>> {
    (
        3.0..12.0f64,
        3.0..12.0f64,
        prop::collection::btree_set(0u32..360, 3..9),
    )
        .prop_filter_map("vertices too close", |(rx, ry, angles)| {
            let pts: Vec<Point2> = angles
                .iter()
                .map(|&a| {
                    let t = f64::from(a).to_radians();
                    Point2::new(rx * t.cos(), ry * t.sin())
                })
                .collect();
            let n = pts.len();
            let ok =
                (0..n).all(|i| pts[i].distance(pts[(i + 1) % n]) > 0.5) && signed_area(&pts) > 4.0;
            ok.then_some(pts)
        })
}

/// Any edge definitions on an outline.
fn defs(pts: Vec<Point2>) -> impl Strategy<Value = Vec<EdgeDef>> {
    let n = pts.len();
    prop::collection::vec((pitch(), 2.0..6.0f64, 0.0..1.2f64), n).prop_map(move |d| {
        pts.iter()
            .zip(d)
            .map(|(&p, (pitch, eave, ov))| edge(p, pitch, eave, ov))
            .collect()
    })
}

fn chimney() -> impl Strategy<Value = Chimney> {
    (0.5..10.0f64, 0.5..8.0f64, 0.3..1.5f64, 0.3..1.5f64, 0.0..PI).prop_map(
        |(cx, cy, w, h, rot)| {
            let (s, c) = rot.sin_cos();
            let corners = [(-w, -h), (w, -h), (w, h), (-w, h)]
                .iter()
                .map(|&(x, y)| Point2::new(cx + (x * c - y * s) / 2.0, cy + (x * s + y * c) / 2.0))
                .collect();
            Chimney {
                corners,
                height: 9.0,
            }
        },
    )
}

fn window() -> impl Strategy<Value = RoofWindow> {
    (0.5..10.0f64, 0.5..8.0f64, 0.4..1.6f64, 0.4..1.8f64).prop_map(|(x, y, width, height)| {
        RoofWindow {
            x,
            y,
            width,
            height,
        }
    })
}

/// A general project: orthogonal or convex outline, mixed definitions,
/// optional chimneys and windows.
fn any_project() -> impl Strategy<Value = Project> {
    let outline = prop_oneof![orthogonal_outline(), convex_outline()];
    (
        outline.prop_flat_map(defs),
        prop::collection::vec(chimney(), 0..2),
        prop::collection::vec(window(), 0..2),
    )
        .prop_map(|(edges, chimneys, windows)| {
            let mut p = project(edges);
            p.chimneys = chimneys;
            p.windows = windows;
            p
        })
}

/// Like [`any_project`] with one eave height and overhang, pitches of
/// 30°, 45° or a gable and no openings: no near-simultaneous sweep events.
fn regular_project() -> impl Strategy<Value = Project> {
    (
        any_project(),
        2.0..6.0f64,
        0.0..0.45f64,
        prop::collection::vec(prop_oneof![Just(30.0), Just(45.0), Just(90.0)], 16),
    )
        .prop_map(|(mut p, h, ov, pitches)| {
            p.chimneys.clear();
            p.windows.clear();
            for (e, pitch) in p.parts[0].edges.iter_mut().zip(pitches) {
                e.eave_height = h;
                e.overhang = ov;
                e.pitch_deg = pitch;
            }
            p
        })
}

/// Like [`any_project`] with every eave at one height.
fn level_project() -> impl Strategy<Value = Project> {
    (any_project(), 2.0..6.0f64).prop_map(|(mut p, h)| {
        for e in &mut p.parts[0].edges {
            e.eave_height = h;
        }
        p
    })
}

/// Like [`level_project`] with no overhang reaching past its neighbours:
/// each one at most 0.45 × the shortest of its own and adjacent edges, so
/// overhangs never cross another eave or each other. `uniform` gives every
/// edge one overhang, limited by the shortest edge.
fn tidy_project(uniform: bool) -> impl Strategy<Value = Project> {
    level_project().prop_map(move |mut p| {
        let pts = p.parts[0].outline();
        let n = pts.len();
        let len = |i: usize| pts[i % n].distance(pts[(i + 1) % n]);
        let shortest = (0..n).map(len).fold(f64::MAX, f64::min);
        let first = p.parts[0].edges[0].overhang.min(0.45 * shortest);
        for (i, e) in p.parts[0].edges.iter_mut().enumerate() {
            e.overhang = if uniform {
                first
            } else {
                e.overhang
                    .min(0.45 * len(i + n - 1).min(len(i)).min(len(i + 1)))
            };
        }
        p
    })
}

/// Degenerate input: tiny and near-collinear edges, pitches at the
/// borders, very large overhangs, coordinates far from the origin.
fn degenerate_project() -> impl Strategy<Value = Project> {
    let tiny = prop_oneof![
        Just(0.0),
        1e-9..1e-6f64,
        1e-4..0.03f64,
        Just(0.01),
        Just(0.02),
        Just(0.05)
    ];
    let wild_pitch = prop_oneof![
        Just(0.0),
        0.0..1.0f64,
        Just(89.999),
        Just(89.9989),
        89.99..90.01f64,
        Just(90.0),
        90.0..120.0f64,
        Just(85.0),
        10.0..70.0f64,
    ];
    let off = prop_oneof![Just(0.0), Just(1e3), Just(-2.5e4), Just(1e5)];
    (
        4.0..15.0f64,
        3.0..10.0f64,
        tiny,
        prop::collection::vec((wild_pitch, 0.0..8.0f64, 0.0..6.0f64), 6),
        off.clone(),
        off,
        0usize..6,
    )
        .prop_map(|(w, h, t, d, ox, oy, rot)| {
            // A rectangle with a tiny step and a near-collinear vertex.
            let raw = [
                Point2::new(0.0, 0.0),
                Point2::new(w / 2.0, t),
                Point2::new(w, 0.0),
                Point2::new(w, h),
                Point2::new(w / 2.0 + t, h),
                Point2::new(0.0, h),
            ];
            let mut edges: Vec<EdgeDef> = raw
                .iter()
                .zip(d)
                .map(|(&p, (pitch, eave, ov))| {
                    edge(Point2::new(p.x + ox, p.y + oy), pitch, eave, ov)
                })
                .collect();
            edges.rotate_left(rot);
            project(edges)
        })
}

/// Seed of the property runs (see [`config`]).
const SEED: u64 = 20261103;

fn config(cases: u32) -> ProptestConfig {
    let cases = std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|c| c.parse().ok())
        .unwrap_or(cases);
    ProptestConfig {
        cases,
        // Inputs DACAD refuses are rejected, not failed; with many cases
        // their count outgrows proptest's default limit of 1024.
        max_global_rejects: cases * 4,
        max_local_rejects: 1 << 20,
        // A fixed seed unless PROPTEST_RNG_SEED is set, so a pull request's
        // checks do not depend on which rare roof a random seed draws (PR
        // #16 drew P16_merged_apex). The nightly run explores new seeds.
        rng_seed: std::env::var("PROPTEST_RNG_SEED")
            .ok()
            .and_then(|s| s.parse().ok())
            .map_or(
                proptest::test_runner::RngSeed::Fixed(SEED),
                proptest::test_runner::RngSeed::Fixed,
            ),
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

// ---------------------------------------------------------------- properties

proptest! {
    #![proptest_config(config(256))]

    /// Any reasonable roof: no panic, finite output.
    #[test]
    fn never_panics_and_stays_finite(p in any_project()) {
        let geos = run(&p).map_err(|e| fail(&p, e))?;
        finite(&geos).map_err(|e| fail(&p, e))?;
    }

    /// Openings never take away more than a face holds.
    #[test]
    #[ignore = "known bug #8: window and chimney deductions can exceed the face"]
    fn face_areas_are_not_negative(p in any_project()) {
        let geos = run(&p).map_err(|e| fail(&p, e))?;
        for f in geos.iter().flat_map(|g| &g.faces) {
            prop_assert!(f.area >= -1e-9, "negative face area {f:?}\n{}", describe(&p));
        }
    }

    /// Degenerate geometry must not panic or produce NaN either.
    #[test]
    fn degenerate_input_never_panics(p in degenerate_project()) {
        let geos = run(&p).map_err(|e| fail(&p, e))?;
        finite(&geos).map_err(|e| fail(&p, e))?;
    }
}

/// Face geometry checks, for one overhang everywhere or one per edge.
macro_rules! face_tests {
    ($name:ident, uniform: $uniform:expr, [$($attr:meta),*]) => {
        mod $name {
            use super::*;

            proptest! {
                #![proptest_config(config(256))]

            /// Every face is a flat polygon at its own pitch (Newell normal).
            #[test]
            $(#[$attr])*
            fn faces_are_flat_at_their_pitch(p in tidy_project($uniform)) {
                let geos = roof(&p)?;
                for f in geos.iter().flat_map(|g| &g.faces) {
                    // Collapsed faces (zero area, two points) carry nothing.
                    if f.pitch_deg >= 89.0 || f.area < 1e-3 {
                        continue;
                    }
                    prop_assert!(f.polygon.len() >= 3, "face {} has {} points\n{}", f.edge, f.polygon.len(), describe(&p));
                    let n = newell(&f.polygon);
                    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                    if single_line_face(f) {
                        continue;
                    }
                    let c = f.polygon[0];
                    for v in &f.polygon {
                        let d = ((v[0] - c[0]) * n[0] + (v[1] - c[1]) * n[1] + (v[2] - c[2]) * n[2]) / len;
                        prop_assert!(d.abs() < 0.01, "face {} point {v:?} {d} off its plane\n{}", f.edge, describe(&p));
                    }
                    let pitch = n[0].hypot(n[1]).atan2(n[2].abs()).to_degrees();
                    prop_assert!((pitch - f.pitch_deg).abs() < 0.5, "face {} at {pitch}° says {}°\n{}", f.edge, f.pitch_deg, describe(&p));
                }
            }

            /// A face at its edge's pitch lies in the edge's plane
            /// (see [`edge_plane_misfits`]).
            #[test]
            $(#[$attr])*
            fn faces_lie_in_their_edge_planes(p in tidy_project($uniform)) {
                let geos = roof(&p)?;
                let bad = edge_plane_misfits(&p, &geos);
                prop_assert!(bad.is_empty(), "{}\n{}", bad.join("\n"), describe(&p));
            }
            }
        }
    };
}

/// A face DACAD's core writes bounded by a single line: its polygon has no
/// area (two of its three corners coincide) but its reported area, taken
/// from its edge list, is a sliver of a few hundredths of a square metre.
/// It has no plane to be flat in (cases `F5_*`, `F6_*`, `F8_*`, `G6_*`, SO2
/// byte-identical to DACAD's; see `single_line_faces_are_dacads`).
fn single_line_face(f: &tectora_core::roof::RoofFace) -> bool {
    // Corners within a millimetre are one (the coinciding pair can differ
    // in the last float32 digit).
    let mut corners: Vec<[f64; 3]> = Vec::new();
    for v in &f.polygon {
        if !corners
            .iter()
            .any(|c| (0..3).all(|k| (c[k] - v[k]).abs() < 1e-3))
        {
            corners.push(*v);
        }
    }
    corners.len() < 3 && f.area < 0.05
}

/// DACAD's point merge distance in plan (POINT_EPS, 2 cm).
const POINT_EPS: f64 = 0.02;

/// The face corners that are off their edge's plane: through the eave line
/// (the edge pushed out by its overhang) at eave height. DACAD takes events
/// within 1 cm of height as one sweep level and merges points within 2 cm
/// in plan, so where several planes nearly meet in one point a corner may
/// be the merged point of its neighbours, off the face's own plane by up
/// to `slope * POINT_EPS` (case `P16_merged_apex`, SO2 byte-identical to
/// DACAD's). Where overhangs differ, the eave corners between them follow
/// DACAD's own rules, so the tolerance also grows by what the overhang step
/// can lift.
fn edge_plane_misfits(p: &Project, geos: &[RoofGeometry]) -> Vec<String> {
    let edges = &p.parts[0].edges;
    let ovs = edges.iter().map(|e| e.overhang);
    let step = ovs.clone().fold(0.0, f64::max) - ovs.fold(f64::MAX, f64::min);
    let mut bad = Vec::new();
    for f in geos.iter().flat_map(|g| &g.faces) {
        let e = &edges[f.edge];
        if e.pitch_deg >= 89.0
            || (f.pitch_deg - e.pitch_deg).abs() > 1e-6
            || e.half_hip.is_some()
            || !e.breaks.is_empty()
            || e.direction_rad.is_some()
        {
            continue;
        }
        let b = edges[(f.edge + 1) % edges.len()].start;
        let (dx, dy) = (b.x - e.start.x, b.y - e.start.y);
        let len = dx.hypot(dy);
        let slope = e.pitch_deg.to_radians().tan();
        for v in &f.polygon {
            let inward = ((v[0] - e.start.x) * -dy + (v[1] - e.start.y) * dx) / len;
            let z = e.eave_height + slope * (inward + e.overhang);
            let tol = 0.01 + slope * (step + POINT_EPS);
            if (v[2] - z).abs() >= tol {
                bad.push(format!(
                    "face {} point {v:?} off its plane (z {z}, tolerance {tol})",
                    f.edge
                ));
            }
        }
    }
    bad
}

face_tests!(faces_uniform_overhang, uniform: true, []);

/// The counterexample a random seed drew on PR #16: on a 5-gon with one
/// overhang, four roof planes nearly meet at the top and DACAD merges their
/// corners there, so a corner of the 45° face is the merged point of its
/// neighbours, 1.16 cm off its own plane. The engine's SO2 is byte-identical
/// to DACAD's (DACAD core emulator), so this is DACAD's behaviour within its
/// point tolerance, not a port bug.
/// The engine writes the RD2 DACAD was run on and gives DACAD's SO2 (from
/// the DACAD core emulator) for fixture `name` in `tests/fixtures/check`.
fn dacad_fixture(name: &str) -> Project {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/check");
    let read = |ext: &str| std::fs::read(dir.join(format!("{name}.{ext}"))).unwrap();
    let p: Project = serde_json::from_slice(&read("json")).unwrap();
    let (rd2, _) = tectora_core::engine::rd2_text(&p).unwrap();
    assert_eq!(rd2.as_bytes(), read("RD2").as_slice(), "{name} RD2");
    let out = tectora_core::dcore::pipeline::Run::new(0x4a8)
        .run(&read("RD2"))
        .unwrap();
    assert!(out.sol == read("SO2"), "{name} SO2 differs from DACAD's");
    p
}

/// Counterexamples of `faces_are_flat_at_their_pitch` under other seeds:
/// with one overhang, where a short wall meets its neighbours at a sharp
/// angle DACAD writes a face bounded by a single line (zero polygon area,
/// a reported sliver of 0.004 to 0.016 m2).
#[test]
fn single_line_faces_are_dacads() {
    for (name, edge) in [
        ("F5_single_line_face", 3),
        ("F6_single_line_face", 0),
        ("F8_single_line_face", 1),
        ("G6_single_line_face", 0),
        ("V6_vertical_single_line_face", 0),
    ] {
        let p = dacad_fixture(name);
        let geos = generate_project(&p);
        let f = geos[0].faces.iter().find(|f| f.edge == edge).unwrap();
        assert!(single_line_face(f), "{name}: {f:?}");
        assert!(f.area > 1e-3, "{name}: {f:?}");
    }
}

/// The PR #18 CI counterexample of `faces_are_flat_at_their_pitch` (before
/// single-line faces were skipped): the face's Newell normal is horizontal,
/// so the polygon reads as vertical while DACAD labels it with its edge's
/// pitch. Its two distinct corners make one line, so the normal has no
/// direction; same class as `single_line_faces_are_dacads`.
#[test]
fn single_line_face_reads_as_vertical() {
    let p = dacad_fixture("V6_vertical_single_line_face");
    let geos = generate_project(&p);
    let f = geos[0].faces.iter().find(|f| f.edge == 0).unwrap();
    let n = newell(&f.polygon);
    let pitch = n[0].hypot(n[1]).atan2(n[2].abs()).to_degrees();
    assert!(pitch > 89.0, "{pitch} {f:?}");
    assert!(
        (f.pitch_deg - p.parts[0].edges[0].pitch_deg).abs() < 1e-4,
        "{f:?}"
    );
    assert!(single_line_face(f), "{f:?}");
}

#[test]
fn merged_apex_corner_is_within_dacad_tolerance() {
    let p = dacad_fixture("P16_merged_apex");

    let geos = generate_project(&p);
    assert!(geos[0].warnings.is_empty(), "{:?}", geos[0].warnings);
    assert_eq!(edge_plane_misfits(&p, &geos), Vec::<String>::new());
    // The corner itself: on faces 0 and 1, 1.16 cm above face 4's plane.
    let e = &p.parts[0].edges[4];
    let b = p.parts[0].edges[0].start;
    let (dx, dy) = (b.x - e.start.x, b.y - e.start.y);
    let face = geos[0].faces.iter().find(|f| f.edge == 4).unwrap();
    let off = face
        .polygon
        .iter()
        .map(|v| {
            let inward = ((v[0] - e.start.x) * -dy + (v[1] - e.start.y) * dx) / dx.hypot(dy);
            (v[2] - (e.eave_height + inward + e.overhang)).abs()
        })
        .fold(0.0, f64::max);
    assert!((off - 0.0116).abs() < 2e-4, "{off}");
}
face_tests!(
    faces_mixed_overhang,
    uniform: false,
    [ignore = "DACAD-dependent: with unequal overhangs at sharp corners DACAD's core writes faces bounded by a single line (confirmed: SO2 byte-identical to DACAD's)"]
);

/// Invariance tests, instantiated for a generator; `$attr` lets known bugs
/// be ignored for one generator only.
macro_rules! invariance_tests {
    ($name:ident, $gen:expr, order: [$($order:meta),*], placement: [$($place:meta),*], moved: [$($moved:meta),*]) => {
        mod $name {
            use super::*;

            proptest! {
                #![proptest_config(config(256))]

            /// The same roof entered from another start vertex gives the same totals.
            #[test]
            $(#[$order])*
            fn start_vertex_does_not_matter(p in $gen, k in 1usize..12) {
                let a = totals(&roof(&p)?);
                let mut q = p.clone();
                let n = q.parts[0].edges.len();
                q.parts[0].edges.rotate_left(k % n);
                let b = totals(&roof(&q)?);
                same_totals(&a, &b, false).map_err(|e| fail(&p, e))?;
            }

            /// Entering the outline clockwise is normalised to the same roof.
            #[test]
            $(#[$place])*
            fn orientation_does_not_matter(p in $gen) {
                let a = totals(&roof(&p)?);
                let mut q = p.clone();
                let part = &mut q.parts[0];
                let n = part.edges.len();
                let starts: Vec<Point2> = part.edges.iter().map(|e| e.start).collect();
                let mut edges = Vec::with_capacity(n);
                for i in (0..n).rev() {
                    let mut e = part.edges[i].clone();
                    e.start = starts[(i + 1) % n];
                    edges.push(e);
                }
                part.edges = edges;
                part.normalize_ccw();
                let b = totals(&roof(&q)?);
                same_totals(&a, &b, false).map_err(|e| fail(&p, e))?;
            }

            /// Moving the roof changes nothing. Offsets stay near the origin:
            /// the engine snaps in absolute f32 like DACAD, so at ~1000 m a merge
            /// can flip (DACAD's own areas drift there too) and that is not a bug.
            #[test]
            $(#[$moved])*
            fn translation_does_not_matter(
                p in $gen,
                dx in -50.0..50.0f64,
                dy in -50.0..50.0f64,
            ) {
                let a = totals(&roof(&p)?);
                let q = transform(&p, |v| Point2::new(v.x + dx, v.y + dy), |a| a);
                let b = totals(&roof(&q)?);
                same_totals(&a, &b, false).map_err(|e| fail(&p, e))?;
            }

            /// Turning the roof in plan changes nothing.
            #[test]
            $(#[$order])*
            fn rotation_does_not_matter(p in $gen, deg in prop_oneof![Just(90.0), Just(180.0), Just(270.0), 0.0..360.0f64]) {
                let a = totals(&roof(&p)?);
                let (s, c) = deg.to_radians().sin_cos();
                let q = transform(&p, |v| Point2::new(v.x * c - v.y * s, v.x * s + v.y * c), |a| a + deg.to_radians());
                let b = totals(&roof(&q)?);
                same_totals(&a, &b, false).map_err(|e| fail(&p, e))?;
            }

            /// A mirrored roof has the same totals with left and right verges swapped.
            #[test]
            $(#[$order])*
            fn mirror_swaps_verge_sides(p in $gen) {
                // A window extends from its corner along the eave, so it is not
                // mirror-symmetric; leave windows out.
                let mut p = p;
                p.windows.clear();
                let a = totals(&roof(&p)?);
                let b = totals(&roof(&mirrored(&p))?);
                same_totals(&a, &b, true).map_err(|e| fail(&p, e))?;
            }

            /// Scaling plan and heights by k scales the area by k². Line totals are
            /// not compared: the engine drops pieces below DACAD's centimetre
            /// thresholds, which do not scale.
            #[test]
            $(#[$moved])*
            fn scaling(p in $gen, k in 0.5..3.0f64) {
                // Chimneys near a line follow absolute DACAD thresholds.
                let mut p = p;
                p.chimneys.clear();
                p.windows.clear();
                let a = totals(&roof(&p)?);
                let mut q = transform(&p, |v| Point2::new(v.x * k, v.y * k), |a| a);
                for e in q.parts.iter_mut().flat_map(|p| &mut p.edges) {
                    e.eave_height *= k;
                    e.overhang *= k;
                }
                for w in &mut q.windows {
                    w.width *= k;
                    w.height *= k;
                }
                let b = totals(&roof(&q)?);
                let want = a.1 * k * k;
                // Merges at DACAD's absolute tolerance shift the area by mm².
                prop_assert!(
                    (b.1 - want).abs() <= AREA_TOL.max(1e-4 * want),
                    "area {} want {want}\n{}",
                    b.1,
                    describe(&p)
                );
            }
            }
        }
    };
}

// The "order" tests (start vertex, mirror, rotation) compare one roof
// entered differently. DACAD itself is not invariant there: its tolerances
// are per coordinate and its sweep takes near-simultaneous events in input
// order (core-stress S02, P3), so failures near the centimetre threshold
// are expected; larger ones need a DACAD run before they count as bugs.
// Orientation must hold. The "moved" tests (translation, scale) do not
// hold for DACAD either: its core reads the RD2 coordinates with four
// decimals and merges at absolute tolerances, and confirmed DACAD runs of
// failing cases give the same totals as ours (a 7-wall hip roof moved by
// (39.68, -15.09): 123.0159 and 123.0571 m², as in DACAD's SO2; RD2s
// identical). They stay to be run with --ignored.
invariance_tests!(
    regular,
    regular_project(),
    order: [ignore = "DACAD-dependent (12 mirrored failures run in the emulator: SO2 byte-equal to ours in both orientations): G/K differ by up to 10 cm, and DACAD leaves one WS segment uncoded (00) in one orientation, so WS differs by up to 2 m"],
    placement: [],
    moved: [ignore = "DACAD-dependent: its tolerances are absolute, so moved or scaled roofs differ (confirmed in DACAD)"]
);
invariance_tests!(
    regular_with_openings,
    (regular_project(), prop::collection::vec(chimney(), 1..3), prop::collection::vec(window(), 0..2))
        .prop_map(|(mut p, c, w)| {
            p.chimneys = c;
            p.windows = w;
            p
        }),
    order: [ignore = "DACAD-dependent: chimney side tests use the line's stored direction"],
    placement: [],
    moved: [ignore = "DACAD-dependent: its tolerances are absolute, so moved or scaled roofs differ (confirmed in DACAD)"]
);
invariance_tests!(
    level_eaves,
    level_project(),
    order: [ignore = "DACAD-dependent: near-simultaneous events follow input order"],
    placement: [],
    moved: [ignore = "DACAD-dependent: its tolerances are absolute, so moved or scaled roofs differ (confirmed in DACAD)"]
);
// Mixed eave heights (raised eaves, walls standing above the roof).
invariance_tests!(
    mixed_eaves,
    any_project(),
    order: [ignore = "DACAD-dependent: near-simultaneous events follow input order"],
    placement: [ignore = "known bug: raised eaves give position dependent faces"],
    moved: [ignore = "DACAD-dependent: its tolerances are absolute, so moved or scaled roofs differ (confirmed in DACAD)"]
);

proptest! {
    #![proptest_config(config(256))]

    /// A hip roof with one pitch, eave and overhang on a convex outline:
    /// its plan is the outline pushed out by the overhang (mitred), every
    /// face's area is its plan area / cos(pitch), and the eave runs round
    /// the pushed-out outline.
    #[test]
    #[ignore = "DACAD-dependent: at low pitch with a small overhang DACAD's core lets hips cross (confirmed in DACAD: a 4-gon at 15 deg, overhang 0.025, gives G 14.546 and 14.675 m2 instead of 10.29)"]
    fn uniform_hip_on_convex_outline(
        pts in convex_outline(),
        pitch in 15.0..65.0f64,
        eave in 2.0..6.0f64,
        ov in 0.0..1.2f64,
    ) {
        let p = project(pts.iter().map(|&v| edge(v, pitch, eave, ov)).collect());
        let geos = roof(&p)?;
        let (lines, area) = totals(&geos);
        let off = offset(&p.parts[0].outline(), ov);
        let plan = signed_area(&off);
        let want = plan / pitch.to_radians().cos();
        prop_assert!((area - want).abs() < CLOSED_TOL * want.max(1.0), "area {area} want {want}");
        let eave_len: f64 = (0..off.len()).map(|i| off[i].distance(off[(i + 1) % off.len()])).sum();
        let t = lines.get("T").copied().unwrap_or(0.0);
        prop_assert!((t - eave_len).abs() < CLOSED_TOL * eave_len.max(1.0), "T {t} want {eave_len}");
        for code in ["K", "OL", "OR", "OS", "WV", "WH", "WS", "P", "c", "S"] {
            // Pieces below DACAD's line resolution are tolerated.
            let len = lines.get(code).copied().unwrap_or(0.0);
            prop_assert!(len < LINE_TOL, "{code} on a uniform hip roof: {lines:?}");
        }
        // Every face is planar at its pitch and its 3D outline matches its area.
        for f in geos.iter().flat_map(|g| &g.faces) {
            let a3 = area3(&f.polygon);
            prop_assert!((a3 - f.area).abs() < 1e-3, "face {} area {} vs polygon {}", f.edge, f.area, a3);
            for v in &f.polygon {
                prop_assert!(v[2] >= eave - 1e-6 - ov * pitch.to_radians().tan(), "vertex below eave {v:?}");
            }
        }
    }

    /// Orthogonal outline with notches, uniform hip roof: mitred offset area
    /// is A + o·P + 4o² and the eave is P + 8o.
    #[test]
    #[ignore = "DACAD-dependent: DACAD's core lets hips cross on long narrow roofs and refuses some notches (confirmed in DACAD: 4 x 15.09 m at 15 deg, overhang 0.03, gives 127.40 m2 instead of 63.69)"]
    fn uniform_hip_on_orthogonal_outline(
        pts in orthogonal_outline(),
        pitch in 15.0..65.0f64,
        eave in 2.0..6.0f64,
        // Notches are at least 1 m wide: a larger overhang closes them.
        ov in 0.0..0.45f64,
    ) {
        let p = project(pts.iter().map(|&v| edge(v, pitch, eave, ov)).collect());
        let geos = roof(&p)?;
        let (lines, area) = totals(&geos);
        let outline = p.parts[0].outline();
        let a = signed_area(&outline);
        let per: f64 = (0..outline.len()).map(|i| outline[i].distance(outline[(i + 1) % outline.len()])).sum();
        let plan = a + ov * per + 4.0 * ov * ov;
        let want = plan / pitch.to_radians().cos();
        prop_assert!((area - want).abs() < AREA_TOL, "area {area} want {want}");
        let t = lines.get("T").copied().unwrap_or(0.0);
        prop_assert!((t - (per + 8.0 * ov)).abs() < LINE_TOL, "T {t} want {}", per + 8.0 * ov);
    }

    /// Gable roof on a rectangle: closed forms for every quantity.
    #[test]
    #[ignore = "DACAD-dependent: DACAD's core shortens overhangs of about a centimetre (confirmed in DACAD: 2 x 2 m at 10 deg, eave overhang 0.0115, gives 4.0851 m2 instead of 4.1085)"]
    fn gable_rectangle_closed_form(
        w in 2.0..20.0f64,
        d in 2.0..14.0f64,
        pitch in 10.0..70.0f64,
        eave in 2.0..6.0f64,
        ov_eave in 0.0..1.2f64,
        ov_gable in 0.0..1.2f64,
    ) {
        let pts = [(0.0, 0.0), (w, 0.0), (w, d), (0.0, d)];
        let edges = pts.iter().enumerate().map(|(i, &(x, y))| {
            if i % 2 == 0 {
                edge(Point2::new(x, y), pitch, eave, ov_eave)
            } else {
                edge(Point2::new(x, y), 90.0, eave, ov_gable)
            }
        }).collect();
        let p = project(edges);
        let geos = roof(&p)?;
        let (lines, area) = totals(&geos);
        let c = pitch.to_radians().cos();
        let len = w + 2.0 * ov_gable;
        let slope = (d / 2.0 + ov_eave) / c;
        let want_area = 2.0 * len * slope;
        prop_assert!((area - want_area).abs() < CLOSED_TOL * want_area.max(1.0), "area {area} want {want_area}");
        let get = |k: &str| lines.get(k).copied().unwrap_or(0.0);
        prop_assert!((get("F") - len).abs() < CLOSED_TOL * len.max(1.0), "F {} want {len}", get("F"));
        prop_assert!((get("T") - 2.0 * len).abs() < CLOSED_TOL * len.max(1.0), "T {} want {}", get("T"), 2.0 * len);
        let verges = get("OL") + get("OR");
        prop_assert!((verges - 4.0 * slope).abs() < CLOSED_TOL * slope.max(1.0), "verges {verges} want {}", 4.0 * slope);
        prop_assert!((get("OL") - get("OR")).abs() < CLOSED_TOL * slope.max(1.0), "OL {} OR {}", get("OL"), get("OR"));
        prop_assert!(get("G") == 0.0 && get("K") == 0.0, "{lines:?}");
    }

    /// Shed roof on a rectangle (gables on the sides, wall at the top).
    #[test]
    fn shed_rectangle_closed_form(
        w in 2.0..20.0f64,
        d in 2.0..10.0f64,
        pitch in 5.0..60.0f64,
        ov in 0.0..1.0f64,
    ) {
        let edges = vec![
            edge(Point2::new(0.0, 0.0), pitch, 3.0, ov),
            edge(Point2::new(w, 0.0), 90.0, 3.0, ov),
            edge(Point2::new(w, d), 90.0, 3.0, ov),
            edge(Point2::new(0.0, d), 90.0, 3.0, ov),
        ];
        let p = project(edges);
        let geos = roof(&p)?;
        let (lines, area) = totals(&geos);
        let c = pitch.to_radians().cos();
        let len = w + 2.0 * ov;
        let slope = (d + 2.0 * ov) / c;
        prop_assert!((area - len * slope).abs() < CLOSED_TOL * area.max(1.0), "area {area} want {}", len * slope);
        let get = |k: &str| lines.get(k).copied().unwrap_or(0.0);
        prop_assert!((get("T") - len).abs() < CLOSED_TOL * len.max(1.0), "{lines:?}");
        prop_assert!((get("P") - len).abs() < CLOSED_TOL * len.max(1.0), "{lines:?}");
    }

    /// Adding a chimney or window never adds roof area.
    #[test]
    fn openings_only_remove_area(p in any_project()) {
        let mut bare = p.clone();
        bare.chimneys.clear();
        bare.windows.clear();
        let with = totals(&roof(&p)?).1;
        let without = totals(&roof(&bare)?).1;
        prop_assert!(with <= without + 1e-6, "with openings {with} > without {without}");
    }
}

/// Pushes a CCW polygon out by `d` with mitred corners.
fn offset(pts: &[Point2], d: f64) -> Vec<Point2> {
    let n = pts.len();
    let line = |i: usize| {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let len = a.distance(b);
        let (nx, ny) = ((b.y - a.y) / len, -(b.x - a.x) / len);
        (
            Point2::new(a.x + nx * d, a.y + ny * d),
            Point2::new(b.x - a.x, b.y - a.y),
        )
    };
    (0..n)
        .map(|i| {
            let (p0, d0) = line((i + n - 1) % n);
            let (p1, d1) = line(i);
            let den = d0.x * d1.y - d0.y * d1.x;
            let t = ((p1.x - p0.x) * d1.y - (p1.y - p0.y) * d1.x) / den;
            Point2::new(p0.x + d0.x * t, p0.y + d0.y * t)
        })
        .collect()
}

/// Area of a planar 3D polygon.
fn newell(poly: &[[f64; 3]]) -> [f64; 3] {
    let mut n = [0.0; 3];
    for (i, a) in poly.iter().enumerate() {
        let b = poly[(i + 1) % poly.len()];
        n[0] += (a[1] - b[1]) * (a[2] + b[2]);
        n[1] += (a[2] - b[2]) * (a[0] + b[0]);
        n[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    n
}

fn area3(poly: &[[f64; 3]]) -> f64 {
    let n = poly.len();
    let mut s = [0.0; 3];
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        s[0] += a[1] * b[2] - a[2] * b[1];
        s[1] += a[2] * b[0] - a[0] * b[2];
        s[2] += a[0] * b[1] - a[1] * b[0];
    }
    (s[0] * s[0] + s[1] * s[1] + s[2] * s[2]).sqrt() / 2.0
}
