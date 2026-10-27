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

/// Relative tolerance for comparing totals of the same roof entered twice.
const REL: f64 = 1e-6;

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

fn fail(p: &Project, e: String) -> TestCaseError {
    TestCaseError::fail(format!("{e}\ninput:\n{}", describe(p)))
}

fn finite(geos: &[RoofGeometry]) -> Result<(), String> {
    for g in geos {
        for f in &g.faces {
            if !f.area.is_finite() || f.polygon.iter().flatten().any(|v| !v.is_finite()) {
                return Err(format!("non-finite face {f:?}"));
            }
            if f.area < -1e-9 {
                return Err(format!("negative face area {f:?}"));
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

fn close(a: f64, b: f64, scale: f64) -> bool {
    (a - b).abs() <= REL * scale.max(1.0) + 1e-7
}

/// Compares two totals; `swap` exchanges OL and OR (a mirrored roof).
fn same_totals(
    x: &(BTreeMap<String, f64>, f64),
    y: &(BTreeMap<String, f64>, f64),
    swap: bool,
) -> Result<(), String> {
    let scale = x.1.abs().max(y.1.abs());
    if !close(x.1, y.1, scale) {
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
        if !close(a, b, a.abs().max(b.abs())) {
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
                let d = depth * h;
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

fn config(cases: u32) -> ProptestConfig {
    ProptestConfig {
        cases: std::env::var("PROPTEST_CASES")
            .ok()
            .and_then(|c| c.parse().ok())
            .unwrap_or(cases),
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

    /// Degenerate geometry must not panic or produce NaN either.
    #[test]
    fn degenerate_input_never_panics(p in degenerate_project()) {
        let geos = run(&p).map_err(|e| fail(&p, e))?;
        finite(&geos).map_err(|e| fail(&p, e))?;
    }

    /// The same roof entered from another start vertex gives the same totals.
    #[test]
    fn start_vertex_does_not_matter(p in any_project(), k in 1usize..12) {
        let a = totals(&run(&p).map_err(|e| fail(&p, e))?);
        let mut q = p.clone();
        let n = q.parts[0].edges.len();
        q.parts[0].edges.rotate_left(k % n);
        let b = totals(&run(&q).map_err(|e| fail(&p, e))?);
        same_totals(&a, &b, false).map_err(|e| fail(&p, e))?;
    }

    /// Entering the outline clockwise is normalised to the same roof.
    #[test]
    fn orientation_does_not_matter(p in any_project()) {
        let a = totals(&run(&p).map_err(|e| fail(&p, e))?);
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
        let b = totals(&run(&q).map_err(|e| fail(&p, e))?);
        same_totals(&a, &b, false).map_err(|e| fail(&p, e))?;
    }

    /// Moving the roof (also far from the origin) changes nothing.
    #[test]
    fn translation_does_not_matter(
        p in any_project(),
        dx in prop_oneof![-50.0..50.0f64, Just(1000.0), Just(-12345.678)],
        dy in prop_oneof![-50.0..50.0f64, Just(1000.0), Just(54321.0)],
    ) {
        let a = totals(&run(&p).map_err(|e| fail(&p, e))?);
        let q = transform(&p, |v| Point2::new(v.x + dx, v.y + dy), |a| a);
        let b = totals(&run(&q).map_err(|e| fail(&p, e))?);
        same_totals(&a, &b, false).map_err(|e| fail(&p, e))?;
    }

    /// Turning the roof in plan changes nothing.
    #[test]
    fn rotation_does_not_matter(p in any_project(), deg in prop_oneof![Just(90.0), Just(180.0), Just(270.0), 0.0..360.0f64]) {
        let a = totals(&run(&p).map_err(|e| fail(&p, e))?);
        let (s, c) = deg.to_radians().sin_cos();
        let q = transform(&p, |v| Point2::new(v.x * c - v.y * s, v.x * s + v.y * c), |a| a + deg.to_radians());
        let b = totals(&run(&q).map_err(|e| fail(&p, e))?);
        same_totals(&a, &b, false).map_err(|e| fail(&p, e))?;
    }

    /// A mirrored roof has the same totals with left and right verges swapped.
    #[test]
    fn mirror_swaps_verge_sides(p in any_project()) {
        // A window extends from its corner along the eave, so it is not
        // mirror-symmetric; leave windows out.
        let mut p = p;
        p.windows.clear();
        let a = totals(&run(&p).map_err(|e| fail(&p, e))?);
        let b = totals(&run(&mirrored(&p)).map_err(|e| fail(&p, e))?);
        same_totals(&a, &b, true).map_err(|e| fail(&p, e))?;
    }

    /// Scaling plan and heights by k scales lines by k and area by k².
    #[test]
    fn scaling(p in any_project(), k in 0.5..3.0f64) {
        let a = totals(&run(&p).map_err(|e| fail(&p, e))?);
        let mut q = transform(&p, |v| Point2::new(v.x * k, v.y * k), |a| a);
        for e in q.parts.iter_mut().flat_map(|p| &mut p.edges) {
            e.eave_height *= k;
            e.overhang *= k;
        }
        for w in &mut q.windows {
            w.width *= k;
            w.height *= k;
        }
        let b = totals(&run(&q).map_err(|e| fail(&p, e))?);
        let scaled = (
            a.0.iter().map(|(c, v)| (c.clone(), v * k)).collect(),
            a.1 * k * k,
        );
        same_totals(&scaled, &b, false).map_err(|e| fail(&p, e))?;
    }
}

proptest! {
    #![proptest_config(config(256))]

    /// A hip roof with one pitch, eave and overhang on a convex outline:
    /// its plan is the outline pushed out by the overhang (mitred), every
    /// face's area is its plan area / cos(pitch), and the eave runs round
    /// the pushed-out outline.
    #[test]
    fn uniform_hip_on_convex_outline(
        pts in convex_outline(),
        pitch in 15.0..65.0f64,
        eave in 2.0..6.0f64,
        ov in 0.0..1.2f64,
    ) {
        let p = project(pts.iter().map(|&v| edge(v, pitch, eave, ov)).collect());
        let geos = run(&p).map_err(|e| fail(&p, e))?;
        let (lines, area) = totals(&geos);
        let off = offset(&p.parts[0].outline(), ov);
        let plan = signed_area(&off);
        let want = plan / pitch.to_radians().cos();
        prop_assert!((area - want).abs() < 1e-6 * want.max(1.0), "area {area} want {want}");
        let eave_len: f64 = (0..off.len()).map(|i| off[i].distance(off[(i + 1) % off.len()])).sum();
        let t = lines.get("T").copied().unwrap_or(0.0);
        prop_assert!((t - eave_len).abs() < 1e-6 * eave_len, "T {t} want {eave_len}");
        for code in ["K", "OL", "OR", "OS", "WV", "WH", "WS", "P", "c", "S"] {
            prop_assert!(!lines.contains_key(code), "{code} on a uniform hip roof: {lines:?}");
        }
        // Every face is planar at its pitch and its 3D outline matches its area.
        for f in geos.iter().flat_map(|g| &g.faces) {
            let a3 = area3(&f.polygon);
            prop_assert!((a3 - f.area).abs() < 1e-6 * f.area.max(1.0), "face {} area {} vs polygon {}", f.edge, f.area, a3);
            for v in &f.polygon {
                prop_assert!(v[2] >= eave - 1e-6 - ov * pitch.to_radians().tan(), "vertex below eave {v:?}");
            }
        }
    }

    /// Orthogonal outline with notches, uniform hip roof: mitred offset area
    /// is A + o·P + 4o² and the eave is P + 8o.
    #[test]
    fn uniform_hip_on_orthogonal_outline(
        pts in orthogonal_outline(),
        pitch in 15.0..65.0f64,
        eave in 2.0..6.0f64,
        // Notches are at least 1 m wide: a larger overhang closes them.
        ov in 0.0..0.45f64,
    ) {
        let p = project(pts.iter().map(|&v| edge(v, pitch, eave, ov)).collect());
        let geos = run(&p).map_err(|e| fail(&p, e))?;
        let (lines, area) = totals(&geos);
        let outline = p.parts[0].outline();
        let a = signed_area(&outline);
        let per: f64 = (0..outline.len()).map(|i| outline[i].distance(outline[(i + 1) % outline.len()])).sum();
        let plan = a + ov * per + 4.0 * ov * ov;
        let want = plan / pitch.to_radians().cos();
        prop_assert!((area - want).abs() < 1e-6 * want, "area {area} want {want}");
        let t = lines.get("T").copied().unwrap_or(0.0);
        prop_assert!((t - (per + 8.0 * ov)).abs() < 1e-6 * per, "T {t} want {}", per + 8.0 * ov);
    }

    /// Gable roof on a rectangle: closed forms for every quantity.
    #[test]
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
        let geos = run(&p).map_err(|e| fail(&p, e))?;
        let (lines, area) = totals(&geos);
        let c = pitch.to_radians().cos();
        let len = w + 2.0 * ov_gable;
        let slope = (d / 2.0 + ov_eave) / c;
        let want_area = 2.0 * len * slope;
        prop_assert!((area - want_area).abs() < 1e-6 * want_area, "area {area} want {want_area}");
        let get = |k: &str| lines.get(k).copied().unwrap_or(0.0);
        prop_assert!((get("F") - len).abs() < 1e-6 * len, "F {} want {len}", get("F"));
        prop_assert!((get("T") - 2.0 * len).abs() < 1e-6 * len, "T {} want {}", get("T"), 2.0 * len);
        let verges = get("OL") + get("OR");
        prop_assert!((verges - 4.0 * slope).abs() < 1e-6 * slope, "verges {verges} want {}", 4.0 * slope);
        prop_assert!((get("OL") - get("OR")).abs() < 1e-6 * slope, "OL {} OR {}", get("OL"), get("OR"));
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
        let geos = run(&p).map_err(|e| fail(&p, e))?;
        let (lines, area) = totals(&geos);
        let c = pitch.to_radians().cos();
        let len = w + 2.0 * ov;
        let slope = (d + 2.0 * ov) / c;
        prop_assert!((area - len * slope).abs() < 1e-6 * area, "area {area} want {}", len * slope);
        let get = |k: &str| lines.get(k).copied().unwrap_or(0.0);
        prop_assert!((get("T") - len).abs() < 1e-6 * len, "{lines:?}");
        prop_assert!((get("P") - len).abs() < 1e-6 * len, "{lines:?}");
    }

    /// Adding a chimney or window never adds roof area.
    #[test]
    fn openings_only_remove_area(p in any_project()) {
        let mut bare = p.clone();
        bare.chimneys.clear();
        bare.windows.clear();
        let with = totals(&run(&p).map_err(|e| fail(&p, e))?).1;
        let without = totals(&run(&bare).map_err(|e| fail(&p, e))?).1;
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

// ---------------------------------------------------------------- .DAD parser

/// A minimal V1 file in the layout `dad.rs` reads.
fn v1_file(walls: &[(f32, f32, f32, f32, f32)]) -> Vec<u8> {
    let mut b = b"DACAD\x1a".to_vec();
    let rec = |b: &mut Vec<u8>, sub: u8, tag: u8, payload: &[u8]| {
        b.extend([sub, tag]);
        b.extend((payload.len() as u16).to_le_bytes());
        b.extend(payload);
    };
    rec(&mut b, 0x00, 0x01, b"DACAD V1.13\0");
    for (i, &(x, y, len, ang, pitch)) in walls.iter().enumerate() {
        let mut p = Vec::new();
        p.extend((i as u16 + 1).to_le_bytes());
        p.extend(1u16.to_le_bytes());
        for v in [x, y, len, ang, 0.0, 3.8, 0.9, pitch, 0.0] {
            p.extend(v.to_le_bytes());
        }
        p.extend(0u16.to_le_bytes());
        rec(&mut b, 0x00, 0x12, &p);
    }
    b.extend([0xff, 0xff]);
    b
}

fn rect_file() -> Vec<u8> {
    use std::f32::consts::{FRAC_PI_2, PI};
    v1_file(&[
        (0.0, 0.0, 10.0, 0.0, 40.0),
        (10.0, 0.0, 6.0, FRAC_PI_2, 90.0),
        (10.0, 6.0, 10.0, PI, 40.0),
        (0.0, 6.0, 6.0, -FRAC_PI_2, 90.0),
    ])
}

proptest! {
    #![proptest_config(config(512))]

    /// Random bytes after the magic never panic the parser or the engine.
    #[test]
    fn dad_random_bytes(tail in prop::collection::vec(any::<u8>(), 0..400)) {
        let mut b = b"DACAD\x1a".to_vec();
        b.extend(tail);
        let r = catch_unwind(|| tectora_core::dad::import(&b, "fuzz"));
        prop_assert!(r.is_ok(), "parser panicked on {b:?}");
        if let Ok(Ok(p)) = r {
            run(&p).map_err(|e| fail(&p, e))?;
        }
    }

    /// Mutated and truncated valid files never panic.
    #[test]
    fn dad_mutations(
        flips in prop::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 1..8),
        cut in any::<prop::sample::Index>(),
        truncate in any::<bool>(),
    ) {
        let mut b = rect_file();
        for (i, v) in flips {
            let i = i.index(b.len());
            b[i] = v;
        }
        if truncate {
            b.truncate(cut.index(b.len()));
        }
        let r = catch_unwind(|| tectora_core::dad::import(&b, "fuzz"));
        prop_assert!(r.is_ok(), "parser panicked on {b:?}");
        if let Ok(Ok(p)) = r {
            let geos = run(&p).map_err(|e| fail(&p, e))?;
            finite(&geos).map_err(|e| fail(&p, e))?;
        }
    }

    /// Walls with arbitrary f32 values (NaN, infinities, huge, denormal).
    #[test]
    fn dad_wild_floats(
        vals in prop::collection::vec(
            prop_oneof![
                Just(f32::NAN), Just(f32::INFINITY), Just(f32::NEG_INFINITY),
                Just(f32::MAX), Just(f32::MIN_POSITIVE), Just(-0.0f32), Just(1e-30f32),
                -100.0..100.0f32,
            ],
            5..25,
        ),
    ) {
        let walls: Vec<(f32, f32, f32, f32, f32)> =
            vals.chunks_exact(5).map(|c| (c[0], c[1], c[2], c[3], c[4])).collect();
        let b = v1_file(&walls);
        let r = catch_unwind(|| tectora_core::dad::import(&b, "fuzz"));
        prop_assert!(r.is_ok(), "parser panicked on {walls:?}");
        if let Ok(Ok(p)) = r {
            run(&p).map_err(|e| fail(&p, e))?;
        }
    }
}

#[test]
fn rect_file_imports() {
    let p = tectora_core::dad::import(&rect_file(), "rect").unwrap();
    assert_eq!(p.parts.len(), 1);
    let (lines, area) = totals(&generate_project(&p));
    assert!(area > 0.0 && lines.contains_key("F"), "{lines:?} {area}");
}
