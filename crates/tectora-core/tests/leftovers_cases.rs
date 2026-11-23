//! Core leftovers (slug `core-leftovers`, plan acceptance criteria 1 to 11
//! and 13 to 15, and the fix-round-1 findings at the end): buildings with
//! several low wings, centimetre-rounded turns, and a dormer that sits on
//! no face of a roof Tectora calculated where DACAD's core stops.
//!
//! Expected figures are closed forms, not copied outputs. Wing roofs are
//! Tectora's own departure (Jakub's decision 2026-11-14): the roof is
//! DACAD's join of the buildings a roofer would draw. The straightening of
//! centimetre-rounded turns is `src/straighten.rs` (unit tests there).

use tectora_core::check::WarningKind;
use tectora_core::edit::dormers::{DormerKind, build_dormer, default_params};
use tectora_core::geom::{Point2, signed_area};
use tectora_core::{Project, RoofPart};

mod support;
use support::*;

const G: f64 = 90.0;

fn kinds(roof: &Roof, part: usize) -> Vec<WarningKind> {
    roof.geos[part].warnings.iter().map(|w| w.kind).collect()
}

fn has_height(mut p: Project) -> Project {
    p.parts[0].height = Some(5.0);
    p
}

/// How the wings' sides are drawn.
#[derive(Clone, Copy, Debug)]
enum Side {
    Gable,
    Hip,
}

/// A house `w` wide (y 2.5 to 7.5, ends gables, front and back 40°, eave 5,
/// overhang 0.5) with 5 m wide low wings, 2.5 deep, in front of it at the
/// `xs`; wing fronts at the eave heights `fronts`. Walls: 4 per wing
/// (front, side, house piece, side), then end, back, end, house piece,
/// side.
fn wings_front(w: f64, xs: &[f64], fronts: &[f64], side: Side) -> Project {
    let sd = |i: usize| match side {
        Side::Gable => (G, 5.0),
        Side::Hip => (40.0, fronts[i]),
    };
    let n = xs.len();
    let mut e2 = Vec::new();
    for i in 0..n {
        let x = xs[i];
        let (p, h) = sd(i);
        e2.push(edge(x, 0.0, 40.0, fronts[i], 0.5));
        e2.push(edge(x + 5.0, 0.0, p, h, 0.5));
        e2.push(edge(x + 5.0, 2.5, 40.0, 5.0, 0.5));
        if i + 1 < n {
            let (p, h) = sd(i + 1);
            e2.push(edge(xs[i + 1], 2.5, p, h, 0.5));
        }
    }
    e2.push(edge(w, 2.5, G, 5.0, 0.5));
    e2.push(edge(w, 7.5, 40.0, 5.0, 0.5));
    e2.push(edge(0.0, 7.5, G, 5.0, 0.5));
    e2.push(edge(0.0, 2.5, 40.0, 5.0, 0.5));
    let (p, h) = sd(0);
    e2.push(edge(xs[0], 2.5, p, h, 0.5));
    has_height(project(e2))
}

/// Two wings in front (criterion 1, 2).
fn two_wings(front: f64, side: Side) -> Project {
    wings_front(20.0, &[2.5, 12.5], &[front, front], side)
}

/// Wings in front and at the back (criterion 3).
fn wings_front_and_back(front: f64, side: Side) -> Project {
    let sd = |h: f64| match side {
        Side::Gable => (G, 5.0),
        Side::Hip => (40.0, h),
    };
    let (sp, sh) = sd(front);
    let defs: Vec<(f64, f64, f64, f64)> = vec![
        (2.5, 0.0, 40.0, front),
        (7.5, 0.0, sp, sh),
        (7.5, 2.5, 40.0, 5.0),
        (20.0, 2.5, G, 5.0),
        (20.0, 7.5, 40.0, 5.0),
        (17.5, 7.5, sp, sh),
        (17.5, 10.0, 40.0, front),
        (12.5, 10.0, sp, sh),
        (12.5, 7.5, 40.0, 5.0),
        (0.0, 7.5, G, 5.0),
        (0.0, 2.5, 40.0, 5.0),
        (2.5, 2.5, sp, sh),
    ];
    has_height(project(
        defs.iter()
            .map(|&(x, y, p, h)| edge(x, y, p, h, 0.5))
            .collect(),
    ))
}

/// Three wings in front of a 30 m house (criterion 4).
fn three_wings() -> Project {
    wings_front(30.0, &[2.5, 12.5, 22.5], &[2.0, 2.0, 2.0], Side::Gable)
}

/// One L-shaped low wing between two inner corners (criterion 6).
fn l_wing() -> Project {
    let defs: Vec<(f64, f64, f64, f64)> = vec![
        (5.0, -5.0, 40.0, 2.0),
        (15.0, -5.0, G, 5.0),
        (15.0, -1.0, 40.0, 2.0),
        (10.0, -1.0, 40.0, 2.0),
        (10.0, 2.5, 40.0, 5.0),
        (15.0, 2.5, G, 5.0),
        (15.0, 7.5, 40.0, 5.0),
        (0.0, 7.5, G, 5.0),
        (0.0, 2.5, 40.0, 5.0),
        (5.0, 2.5, 40.0, 2.0),
    ];
    has_height(project(
        defs.iter()
            .map(|&(x, y, p, h)| edge(x, y, p, h, 0.5))
            .collect(),
    ))
}

fn turned(p: &Project, deg: f64) -> Project {
    let (s, c) = deg.to_radians().sin_cos();
    let mm = |v: f64| (v * 1000.0).round() / 1000.0;
    let mut out = p.clone();
    for e in out.parts.iter_mut().flat_map(|part| part.edges.iter_mut()) {
        let (x, y) = (e.start.x, e.start.y);
        e.start = Point2::new(mm(x * c - y * s), mm(x * s + y * c));
    }
    out
}

/// Turned and rounded to centimetres, as a drawing by hand comes.
fn turned_cm(p: &Project, deg: f64) -> Project {
    let (s, c) = deg.to_radians().sin_cos();
    let cm = |v: f64| (v * 100.0).round() / 100.0;
    let mut out = p.clone();
    for e in out.parts.iter_mut().flat_map(|part| part.edges.iter_mut()) {
        let (x, y) = (e.start.x, e.start.y);
        e.start = Point2::new(cm(x * c - y * s), cm(x * s + y * c));
    }
    out
}

// ---------------------------------------------------- structure checks

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

fn eave_plan(p: &Project) -> f64 {
    let part = &p.parts[0];
    let d: Vec<f64> = part.edges.iter().map(|e| e.overhang).collect();
    signed_area(&offset_each(&part.outline(), &d))
}

/// The structure of a roof Tectora calculated where the core stops: a wing
/// joined to the house carries the join's rounding (`Fit::DacadJoined`).
fn assert_sound(name: &str, p: &Project, roof: &Roof) {
    assert_sound_plan(name, p, roof, eave_plan(p));
}

/// As `assert_sound`, against the plan the faces must have: the eave plan
/// plus the house's overhang strip over each low wing (wing widths incl.
/// overhang times the house overhang), because a wing's roof runs on under
/// the house's eave to the house wall (`overcover.md`: 162 for two wings,
/// 240 for three).
fn assert_sound_plan(name: &str, _p: &Project, roof: &Roof, plan: f64) {
    let mut roof = Roof {
        geos: roof.geos.clone(),
        lines: roof.lines.clone(),
        area: roof.area,
    };
    for g in &mut roof.geos {
        g.warnings.retain(|w| {
            !matches!(
                w.kind,
                WarningKind::MergedPlanes
                    | WarningKind::WingJoined
                    | WarningKind::NotCalculatedByCore
            )
        });
    }
    let mut rep = Report::default();
    let mut c = rep.case();
    no_error(&mut c, &roof);
    check_structure_fit(&mut c, &roof, plan, Fit::DacadJoined);
    c.done(name.to_string());
    rep.finish(name);
}

fn near(what: &str, got: f64, want: f64, tol: f64) {
    assert!(
        (got - want).abs() <= tol,
        "{what}: {got} want {want} +- {tol}"
    );
}

/// Calculates `p` and checks the joined-wings result: no error, exactly one
/// `WingJoined` naming `edges`, the area, WV, no P and K, faces on walls
/// of the drawn part, structure against the closed-form plan (`wings` low
/// wings, each 6 m wide with overhangs, under the house's 0.5 m overhang).
/// `ws` is the wall abutment of the wing sides, `g` the hips.
#[allow(clippy::too_many_arguments)]
fn check_wings(
    name: &str,
    p: &Project,
    edges: &[usize],
    area: f64,
    wv: f64,
    wings: usize,
    ws: Option<f64>,
    g: Option<f64>,
) -> Roof {
    let roof = calc(p);
    assert!(
        roof.geos[0].error.is_none(),
        "{name}: {:?}",
        roof.geos[0].error
    );
    let w = &roof.geos[0].warnings;
    assert_eq!(
        w.iter().map(|w| w.kind).collect::<Vec<_>>(),
        vec![WarningKind::WingJoined],
        "{name}: {w:?}"
    );
    assert_eq!(w[0].edges, edges, "{name}: walls of the wings");
    near(&format!("{name} area"), roof.area, area, 0.01);
    near(&format!("{name} WV"), roof.line("WV"), wv, 0.01);
    near(&format!("{name} P"), roof.line("P"), 0.0, 1e-6);
    near(&format!("{name} K"), roof.line("K"), 0.0, 1e-6);
    if let Some(g) = g {
        near(&format!("{name} G"), roof.line("G"), g, 0.02);
    }
    if let Some(ws) = ws {
        near(&format!("{name} WS"), roof.line("WS"), ws, 0.02);
    }
    let n = p.parts[0].edges.len();
    for f in roof.faces() {
        assert!(f.edge < n, "{name}: face on wall {} of {n}", f.edge);
    }
    let plan = eave_plan(p) + wings as f64 * 6.0 * 0.5;
    assert_sound_plan(name, p, &roof, plan);
    roof
}

// ------------------------------------------------------ criteria 1 to 4

/// Criterion 1: two low wings in front of the house.
#[test]
fn two_low_wings_in_front_are_lean_tos_joined_to_the_house() {
    let want = (21.0 * 6.0 + 2.0 * 6.0 * 3.0) / cos(40.0);
    for front in [2.0, 1.5, 2.5] {
        let p = two_wings(front, Side::Gable);
        check_wings(
            &format!("two wings gable, front {front}"),
            &p,
            &[0, 1, 3, 4, 5, 11],
            want,
            12.0,
            2,
            None,
            None,
        );
    }
}

/// Criterion 2: the same wings with hipped sides.
#[test]
fn two_hipped_low_wings_in_front_are_joined_to_the_house() {
    let want = (21.0 * 6.0 + 2.0 * 6.0 * 3.0) / cos(40.0);
    let p = two_wings(2.0, Side::Hip);
    check_wings(
        "two wings hip",
        &p,
        &[0, 1, 3, 4, 5, 11],
        want,
        // Each wing is 5 m wide and 3 m deep: the two hips meet on the house
        // wall, the front face is a triangle touching it in a point, so no
        // WV; the side faces meet the wall along 4 x 3/cos 40 = 15.665 (overcover.md).
        0.0,
        2,
        Some(4.0 * 3.0 / cos(40.0)),
        Some(19.733),
    );
}

/// Criterion 3: one wing in front, one at the back.
#[test]
fn a_wing_in_front_and_one_at_the_back_are_joined_to_the_house() {
    let want = (21.0 * 6.0 + 2.0 * 6.0 * 3.0) / cos(40.0);
    for front in [2.0, 1.5, 2.5] {
        let p = wings_front_and_back(front, Side::Gable);
        check_wings(
            &format!("front and back gable {front}"),
            &p,
            &[0, 1, 5, 6, 7, 11],
            want,
            12.0,
            2,
            None,
            None,
        );
    }
    let p = wings_front_and_back(2.0, Side::Hip);
    check_wings(
        "front and back hip",
        &p,
        &[0, 1, 5, 6, 7, 11],
        want,
        0.0,
        2,
        Some(4.0 * 3.0 / cos(40.0)),
        Some(19.733),
    );
}

/// Criterion 4: three wings, within the peel's bounds.
#[test]
fn three_low_wings_are_joined_to_the_house() {
    let want = (31.0 * 6.0 + 3.0 * 18.0) / cos(40.0);
    let p = three_wings();
    check_wings(
        "three wings",
        &p,
        &[0, 1, 3, 4, 5, 7, 8, 9, 15],
        want,
        18.0,
        3,
        None,
        None,
    );
}

// ----------------------------------------------------------- criterion 5

/// Criterion 5: the same buildings turned (millimetre rounding) get the
/// same notice and the same roof.
#[test]
fn turned_buildings_with_two_low_wings_get_the_same_roof() {
    for (name, p) in [
        ("A", two_wings(2.0, Side::Gable)),
        ("B", wings_front_and_back(2.0, Side::Gable)),
    ] {
        let straight = calc(&p);
        assert!(straight.geos[0].error.is_none(), "{name} unturned");
        for deg in [17.3, 45.0, 200.0] {
            let t = turned(&p, deg);
            let roof = calc(&t);
            let n = format!("{name} turned {deg}");
            assert!(
                roof.geos[0].error.is_none(),
                "{n}: {:?}",
                roof.geos[0].error
            );
            assert_eq!(kinds(&roof, 0), vec![WarningKind::WingJoined], "{n}");
            near(&format!("{n} area"), roof.area, straight.area, 0.15);
            for code in ["T", "F", "WV"] {
                near(
                    &format!("{n} {code}"),
                    roof.line(code),
                    straight.line(code),
                    0.05,
                );
            }
            assert_eq!(
                roof.geos[0].faces.len(),
                straight.geos[0].faces.len(),
                "{n}"
            );
        }
    }
}

// ------------------------------------------------------- criteria 6, 7, 9

/// Criterion 6 (pin, passes today): one L-shaped low wing between two inner
/// corners.
#[test]
fn one_l_shaped_low_wing_is_joined_to_the_house() {
    let p = l_wing();
    let roof = calc(&p);
    assert!(roof.geos[0].error.is_none(), "{:?}", roof.geos[0].error);
    assert_eq!(kinds(&roof, 0), vec![WarningKind::WingJoined]);
    near("area", roof.area, 220.615, 0.01);
    assert_sound("L wing", &p, &roof);
}

/// Criterion 7 (pin): one low wing still gives the single wing's roof with
/// one notice naming the wing's walls.
#[test]
fn one_low_wing_keeps_its_single_wing_result() {
    let p = wings_front(15.0, &[5.0], &[2.0], Side::Gable);
    let roof = calc(&p);
    assert!(roof.geos[0].error.is_none(), "{:?}", roof.geos[0].error);
    assert_eq!(kinds(&roof, 0), vec![WarningKind::WingJoined]);
    assert_eq!(roof.geos[0].warnings[0].edges, vec![0, 1, 7]);
    near(
        "area",
        roof.area,
        (16.0 * 6.0 + 6.0 * 3.0) / cos(40.0),
        0.01,
    );
    assert_sound("one wing", &p, &roof);
}

/// Criterion 9 (pin): a low wing next to a wing in the house's plane (wall 0
/// at 2.9, wall 4 at 2.0) either keeps the core's error or is sound,
/// never a wrong roof. Today the error stays; if that changes the roof
/// must be sound with one notice at most per kind.
#[test]
fn a_low_wing_and_a_wing_in_the_house_plane_keep_the_error_or_are_sound() {
    let mut p = two_wings(2.0, Side::Gable);
    p.parts[0].edges[0].eave_height = 2.9;
    let roof = calc(&p);
    match &roof.geos[0].error {
        Some(_) => {}
        None => assert_sound("merge and lean-to", &p, &roof),
    }
}

// ---------------------------------------------------- criteria 13 to 15

fn jakub_t(front: f64) -> Project {
    let pts = [
        (5.0, 0.0),
        (10.0, 0.0),
        (10.0, 2.5),
        (15.0, 2.5),
        (15.0, 7.5),
        (0.0, 7.5),
        (0.0, 2.5),
        (5.0, 2.5),
    ];
    has_height(project(
        pts.iter()
            .enumerate()
            .map(|(i, &(x, y))| {
                edge(
                    x,
                    y,
                    if i % 2 == 1 { G } else { 40.0 },
                    if i == 0 { front } else { 5.0 },
                    0.5,
                )
            })
            .collect(),
    ))
}

/// One of the 12 hip roofs (corner moved) the standard calculation stops
/// on.
fn departed_hip() -> Project {
    let corners = [(0.0, 0.0), (12.0, 0.0), (12.0, 8.0), (0.0, 8.0)];
    for c in 0..4 {
        for (dx, dy) in [(0.2, 0.0), (-0.2, 0.0), (0.0, 0.2), (0.0, -0.2), (0.2, 0.2)] {
            let mut pts = corners;
            pts[c] = (pts[c].0 + dx, pts[c].1 + dy);
            let p = uniform(&pts, 35.0, 3.0, 0.5);
            if tectora_core::engine::generate(&p).is_err() {
                return p;
            }
        }
    }
    panic!("no departed hip roof");
}

fn gable_dormer(at: Point2, eave: f64) -> tectora_core::Dormer {
    let mut d = default_params(DormerKind::Gable, at, 0.0, eave);
    d.width = 2.0;
    build_dormer(&d)
}

fn off_roof(roof: &Roof, part: usize) -> Vec<Option<usize>> {
    roof.geos[part]
        .warnings
        .iter()
        .filter(|w| w.kind == WarningKind::DormerOffRoof)
        .map(|w| w.dormer)
        .collect()
}

fn departed_roofs() -> Vec<(&'static str, Project, WarningKind, Point2)> {
    vec![
        (
            "merged T",
            jakub_t(2.9),
            WarningKind::MergedPlanes,
            Point2::new(8.0, 4.0),
        ),
        (
            "lean-to T",
            jakub_t(2.0),
            WarningKind::WingJoined,
            Point2::new(8.0, 4.0),
        ),
        (
            "departed hip",
            departed_hip(),
            WarningKind::NotCalculatedByCore,
            Point2::new(5.0, 3.0),
        ),
    ]
}

fn dormer_off(index: usize) {
    let (name, base, notice, _) = departed_roofs().swap_remove(index);
    let bare = calc(&base);
    assert!(
        bare.geos[0].error.is_none(),
        "{name}: {:?}",
        bare.geos[0].error
    );
    for at in [Point2::new(30.0, 30.0), Point2::new(-4.0, 4.0)] {
        let mut p = base.clone();
        p.dormers.push(gable_dormer(at, 6.0));
        let roof = calc(&p);
        let n = format!("{name}, dormer at {at:?}");
        assert!(
            roof.geos[0].error.is_none(),
            "{n}: {:?}",
            roof.geos[0].error
        );
        assert_eq!(off_roof(&roof, 0), vec![Some(0)], "{n}");
        assert!(
            kinds(&roof, 0).contains(&notice),
            "{n}: {:?}",
            kinds(&roof, 0)
        );
        near(&format!("{n} area"), roof.area, bare.area, 1e-9);
        assert_eq!(
            roof.lines.keys().collect::<Vec<_>>(),
            bare.lines.keys().collect::<Vec<_>>(),
            "{n}"
        );
        for (code, len) in &bare.lines {
            near(&format!("{n} {code}"), roof.line(code), *len, 1e-9);
        }
    }
}

/// Criterion 13: a dormer beside the building gets the warning, as on a
/// standard roof, and changes nothing else. Merged planes ...
#[test]
fn a_dormer_off_merged_planes_is_warned() {
    dormer_off(0);
}

/// ... a lean-to wing ...
#[test]
fn a_dormer_off_a_lean_to_wing_roof_is_warned() {
    dormer_off(1);
}

/// ... and a roof from the sweep.
#[test]
fn a_dormer_off_a_swept_roof_is_warned() {
    dormer_off(2);
}

/// Criterion 14 (pin): a dormer on the roof gets no warning.
#[test]
fn a_dormer_on_a_departed_roof_is_not_warned() {
    for (name, base, _, at) in departed_roofs() {
        let mut p = base.clone();
        p.dormers.push(gable_dormer(at, 8.0));
        let roof = calc(&p);
        assert!(
            roof.geos[0].error.is_none(),
            "{name}: {:?}",
            roof.geos[0].error
        );
        assert_eq!(off_roof(&roof, 0), vec![], "{name}");
        assert!(
            roof.geos[0].faces.iter().any(|f| f.edge == usize::MAX),
            "{name}: the dormer has faces"
        );
    }
}

/// Criterion 15: each building carries its own warning only.
#[test]
fn each_building_carries_the_off_roof_warning_of_its_own_dormer() {
    for first in [true, false] {
        let mut p = jakub_t(2.0);
        let rect = RoofPart::new(
            "rect",
            [(45.0, 0.0), (55.0, 0.0), (55.0, 8.0), (45.0, 8.0)]
                .iter()
                .map(|&(x, y)| edge(x, y, 35.0, 3.0, 0.5))
                .collect(),
        );
        let (t, r) = if first {
            p.parts.push(rect);
            (0, 1)
        } else {
            p.parts.insert(0, rect);
            (1, 0)
        };
        // Dormer 0 beside the T, dormer 1 beside the rectangle.
        p.dormers.push(gable_dormer(Point2::new(-4.0, 4.0), 6.0));
        p.dormers.push(gable_dormer(Point2::new(60.0, 4.0), 6.0));
        let roof = calc(&p);
        let n = format!("T first {first}");
        for g in &roof.geos {
            assert!(g.error.is_none(), "{n}: {:?}", g.error);
        }
        assert_eq!(off_roof(&roof, t), vec![Some(0)], "{n}: the T");
        assert_eq!(off_roof(&roof, r), vec![Some(1)], "{n}: the rectangle");
        assert_eq!(
            kinds(&roof, t)
                .iter()
                .filter(|k| **k == WarningKind::WingJoined)
                .count(),
            1,
            "{n}"
        );
    }
}

// --------------------------------------------------- criteria 10 and 11

const CM_ANGLES: [f64; 7] = [1.0, 5.0, 17.3, 33.0, 45.0, 61.7, 200.0];

/// Criterion 10: Jakub's T turned and rounded to centimetres gets the roof
/// of the unturned one (the straightening, owner-approved 2026-11-15).
#[test]
fn jakub_t_turned_and_rounded_to_centimetres_gets_the_unturned_roof() {
    let merged = calc(&jakub_t(2.9));
    near("unturned merged area", merged.area, 144.900, 0.01);
    let lean = calc(&jakub_t(2.0));
    assert!(lean.geos[0].error.is_none(), "{:?}", lean.geos[0].error);
    for deg in CM_ANGLES {
        let n = format!("merged, turned {deg}");
        let roof = calc(&turned_cm(&jakub_t(2.9), deg));
        assert!(
            roof.geos[0].error.is_none(),
            "{n}: {:?}",
            roof.geos[0].error
        );
        assert_eq!(kinds(&roof, 0), vec![WarningKind::MergedPlanes], "{n}");
        assert_eq!(roof.geos[0].faces.len(), 2, "{n}");
        near(&format!("{n} area"), roof.area, 144.900, 0.15);
        near(&format!("{n} K"), roof.line("K"), 0.0, 1e-6);
        near(&format!("{n} P"), roof.line("P"), 0.0, 1e-6);

        let n = format!("lean-to, turned {deg}");
        let roof = calc(&turned_cm(&jakub_t(2.0), deg));
        assert!(
            roof.geos[0].error.is_none(),
            "{n}: {:?}",
            roof.geos[0].error
        );
        assert_eq!(kinds(&roof, 0), vec![WarningKind::WingJoined], "{n}");
        assert_eq!(roof.geos[0].faces.len(), 3, "{n}");
        near(&format!("{n} area"), roof.area, lean.area, 0.2);
        near(&format!("{n} WV"), roof.line("WV"), 6.0, 0.1);
        near(&format!("{n} K"), roof.line("K"), 0.0, 1e-6);
        near(&format!("{n} P"), roof.line("P"), 0.0, 1e-6);
    }
}

/// Criterion 11 (pin): where the core calculates a centimetre-rounded
/// turned T (front 5.0: one roof; front 3.05: stepped planes), the result is
/// the core's own, with no departure notice and nothing straightened.
#[test]
fn a_cm_turned_t_the_core_calculates_is_not_straightened() {
    for front in [5.0, 3.05] {
        for deg in CM_ANGLES {
            let p = turned_cm(&jakub_t(front), deg);
            let n = format!("front {front} turned {deg}");
            let want = tectora_core::engine::generate(&p)
                .unwrap_or_else(|e| panic!("{n}: the core stops: {e:?}"));
            let roof = calc(&p);
            assert!(
                roof.geos[0].error.is_none(),
                "{n}: {:?}",
                roof.geos[0].error
            );
            assert_eq!(roof.geos.len(), want.len(), "{n}");
            for (got, want) in roof.geos.iter().zip(&want) {
                let sum = |g: &tectora_core::roof::RoofGeometry| -> f64 {
                    g.faces.iter().map(|f| f.area).sum()
                };
                assert_eq!(got.faces.len(), want.faces.len(), "{n}");
                near(&format!("{n} area"), sum(got), sum(want), 1e-9);
                // generate_project adds its own checks' warnings; what must
                // be absent is every departure notice.
                assert!(
                    got.warnings.iter().all(|w| !matches!(
                        w.kind,
                        WarningKind::MergedPlanes
                            | WarningKind::WingJoined
                            | WarningKind::NotCalculatedByCore
                    )),
                    "{n}: {:?}",
                    got.warnings
                );
                for (a, b) in got.faces.iter().zip(&want.faces) {
                    assert_eq!(a.polygon, b.polygon, "{n}: polygon moved");
                }
            }
        }
    }
}

// ============================================================ fix round 1
// Findings of review core-leftovers-r1 (math.md, errors.md, test.md) and of
// the bug hunt (findings-bugs.md). Each test names its finding.

/// `p` moved on the site plan by (`dx`, `dy`) metres, to the millimetre.
fn moved(p: &Project, dx: f64, dy: f64) -> Project {
    let mm = |v: f64| (v * 1000.0).round() / 1000.0;
    let mut out = p.clone();
    for e in out.parts.iter_mut().flat_map(|part| part.edges.iter_mut()) {
        e.start = Point2::new(mm(e.start.x + dx), mm(e.start.y + dy));
    }
    out
}

/// Area of a 3D polygon from its own corners (Newell, taken from its first
/// corner so a position far from the origin costs no digits).
fn polygon_area(poly: &[[f64; 3]]) -> f64 {
    let o = poly[0];
    let rel: Vec<[f64; 3]> = poly.iter().map(|v| sub(*v, o)).collect();
    norm(area_vector(&rel))
}

fn own_polygons_area(roof: &Roof) -> f64 {
    roof.faces().map(|f| polygon_area(&f.polygon)).sum()
}

/// MATH-001 (S1): a turned building drawn far from the origin gets the roof
/// it has at the origin: the reported area is the area of the roof's own
/// faces (the merged roof took it from float32 core runs at the position).
#[test]
fn departed_roofs_far_from_the_origin_keep_their_area() {
    let mut bad = Vec::new();
    // (name, building, turn in degrees, rounding to centimetres, offset).
    // Merged-plane roofs report the area of their own f64 polygons, so the
    // two must agree. A wing joined as a lean-to reports the core's exact
    // face areas while its polygons sit on the join's 1 cm grid (about
    // 0.03 m2 apart even at the origin), so for those only far == origin
    // is asserted.
    let merged: [(&str, Project, f64, bool, f64); 6] = [
        ("T 2.9", jakub_t(2.9), 33.0, false, 1000.0),
        ("T 2.9", jakub_t(2.9), 61.7, false, 2000.0),
        ("T 2.9", jakub_t(2.9), 17.3, false, 1000.0),
        ("T 2.9", jakub_t(2.9), 17.3, false, 200.0),
        ("T 2.9", jakub_t(2.9), 17.3, true, 1000.0),
        ("T 2.9", jakub_t(2.9), 17.3, true, 500.0),
    ];
    let joined: [(&str, Project, f64, bool, f64); 2] = [
        ("T 2.0", jakub_t(2.0), 33.0, true, 1000.0),
        (
            "two wings",
            two_wings(2.0, Side::Gable),
            33.0,
            false,
            1000.0,
        ),
    ];
    let cases = merged
        .into_iter()
        .map(|c| (c, true))
        .chain(joined.into_iter().map(|c| (c, false)));
    for ((name, p, deg, cm, off), polygons_exact) in cases {
        let at_origin = if cm {
            turned_cm(&p, deg)
        } else {
            turned(&p, deg)
        };
        let near_origin = calc(&at_origin);
        let far = calc(&moved(&at_origin, off, off));
        let n = format!("{name} turned {deg} cm {cm} at +{off}");
        if let Some(e) = &far.geos[0].error {
            bad.push(format!("{n}: error {e:?}"));
            continue;
        }
        let own = own_polygons_area(&far);
        if polygons_exact && (far.area - own).abs() > 0.01 {
            bad.push(format!("{n}: reported {:.4}, own faces {own:.4}", far.area));
        }
        if (far.area - near_origin.area).abs() > 0.01 {
            bad.push(format!(
                "{n}: {:.4} against {:.4} at the origin",
                far.area, near_origin.area
            ));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// Verge length (DACAD codes OL, OR, OS).
fn verge(roof: &Roof) -> f64 {
    roof.line("OL") + roof.line("OR") + roof.line("OS")
}

/// MATH-002 (S1): where the drawn outline of a centimetre-rounded T gets a
/// merged roof with three faces (two front planes a few millimetres apart),
/// it has a stray valley (K) and extra verge (G). The roof is the unturned one.
#[test]
fn a_cm_rounded_t_has_no_stray_valley_at_any_of_these_angles() {
    let straight = calc(&jakub_t(2.9));
    near("unturned K", straight.line("K"), 0.0, 1e-6);
    let mut bad = Vec::new();
    for deg in [2.1, 4.2, 6.5, 7.5, 12.2] {
        let roof = calc(&turned_cm(&jakub_t(2.9), deg));
        let n = format!("turned {deg}");
        if let Some(e) = &roof.geos[0].error {
            bad.push(format!("{n}: error {e:?}"));
            continue;
        }
        let (k, o) = (roof.line("K"), verge(&roof));
        let faces = roof.geos[0].faces.len();
        if k.abs() > 1e-6
            || (o - verge(&straight)).abs() > 0.05
            || faces != straight.geos[0].faces.len()
        {
            bad.push(format!(
                "{n}: K {k:.3} (want 0), verge {o:.3} (want {:.3}), faces {faces}",
                verge(&straight)
            ));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// MATH-004 (S3): the centimetre-rounded T at 4.9 degrees calculates like
/// the unturned one (a line's corners fell into two clusters).
#[test]
fn a_cm_rounded_t_turned_4_9_degrees_calculates() {
    let straight = calc(&jakub_t(2.9));
    let roof = calc(&turned_cm(&jakub_t(2.9), 4.9));
    assert!(roof.geos[0].error.is_none(), "{:?}", roof.geos[0].error);
    assert_eq!(kinds(&roof, 0), vec![WarningKind::MergedPlanes]);
    assert_eq!(roof.geos[0].faces.len(), straight.geos[0].faces.len());
    near("area", roof.area, straight.area, 0.15);
    near("K", roof.line("K"), 0.0, 1e-6);
}

/// MATH-003 (S3): Jakub's lean-to T with the right house piece 2 cm out of
/// line with the left one. Unturned, the core stops on the step (it is a
/// real step). Turned and rounded to centimetres, the straightening must not
/// make the step vanish: at 2.5, 3.2 and 5.6 degrees it currently does and
/// the building gets a roof as if it had not been drawn.
#[test]
fn a_two_centimetre_step_between_parallel_wall_lines_survives_a_turn() {
    let stepped = |deg: f64| {
        let mut p = jakub_t(2.0);
        p.parts[0].edges[2].start.y += 0.02;
        p.parts[0].edges[3].start.y += 0.02;
        if deg == 0.0 { p } else { turned_cm(&p, deg) }
    };
    let straight = calc(&stepped(0.0));
    assert!(
        straight.geos[0].error.is_some(),
        "the unturned step is a DACAD error today: {:?}",
        kinds(&straight, 0)
    );
    let mut bad = Vec::new();
    for deg in [2.5, 3.2, 5.6] {
        let roof = calc(&stepped(deg));
        if roof.geos[0].error.is_none() {
            bad.push(format!(
                "turned {deg}: roof {:?}, area {:.3}, as if the step were not drawn",
                kinds(&roof, 0),
                roof.area
            ));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// A copy of the T `dx` metres along x, as a second building.
fn t_copy(front: f64, dx: f64) -> RoofPart {
    let mut part = jakub_t(front).parts.remove(0);
    for e in &mut part.edges {
        e.start.x += dx;
    }
    part.name = "copy".into();
    part
}

/// ERR-001 (S3): a stepped-plane roof (`merge_steps`, front 2.95 and 3.0)
/// warns once for its own dormer beside it ...
#[test]
fn a_dormer_off_a_stepped_plane_roof_is_warned_once() {
    for front in [2.95, 3.0] {
        let mut p = jakub_t(front);
        p.dormers.push(gable_dormer(Point2::new(-4.0, 4.0), 6.0));
        let roof = calc(&p);
        let n = format!("front {front}");
        assert!(
            roof.geos[0].error.is_none(),
            "{n}: {:?}",
            roof.geos[0].error
        );
        assert_eq!(
            off_roof(&roof, 0),
            vec![Some(0)],
            "{n}: {:?}",
            kinds(&roof, 0)
        );
    }
}

/// ... and a dormer standing on another building is not "off" this one.
#[test]
fn a_dormer_on_another_building_is_not_warned_on_a_stepped_plane_roof() {
    for front in [2.95, 3.0] {
        for (first, other_front) in [(true, front), (false, front), (true, 2.0), (false, 2.0)] {
            let mut p = jakub_t(front);
            let (t, other) = if first {
                p.parts.push(t_copy(other_front, 40.0));
                (0, 1)
            } else {
                p.parts.insert(0, t_copy(other_front, 40.0));
                (1, 0)
            };
            // On the copy: its house piece spans x 40 to 55, y 2.5 to 7.5.
            p.dormers.push(gable_dormer(Point2::new(48.0, 4.0), 8.0));
            let roof = calc(&p);
            let n = format!("front {front}, stepped T first {first}, other {other_front}");
            for g in &roof.geos {
                assert!(g.error.is_none(), "{n}: {:?}", g.error);
            }
            assert!(
                roof.geos[other].faces.iter().any(|f| f.edge == usize::MAX),
                "{n}: the dormer is cut into the other building"
            );
            assert_eq!(off_roof(&roof, t), vec![], "{n}: the stepped T");
            assert_eq!(off_roof(&roof, other), vec![], "{n}: the other building");
        }
    }
}

/// TEST-001 (S1): a wing whose roof reaches above the house's roof at the
/// house wall is no lean-to (the wing-top test of `Search::accept`). The low
/// wing (eave 2.0) is joined, the high one (3.5) is not; two high wings
/// give no join at all.
#[test]
fn a_wing_reaching_above_the_house_wall_is_not_joined_as_a_lean_to() {
    let joined = |roof: &Roof| -> Vec<Vec<usize>> {
        roof.geos[0]
            .warnings
            .iter()
            .filter(|w| w.kind == WarningKind::WingJoined)
            .map(|w| w.edges.clone())
            .collect()
    };
    for (fronts, edges) in [([2.0, 3.5], vec![0, 1, 11]), ([3.5, 2.0], vec![3, 4, 5])] {
        let p = wings_front(20.0, &[2.5, 12.5], &fronts, Side::Gable);
        let roof = calc(&p);
        let n = format!("fronts {fronts:?}");
        assert!(
            roof.geos[0].error.is_none(),
            "{n}: {:?}",
            roof.geos[0].error
        );
        assert_eq!(joined(&roof), vec![edges], "{n}: only the low wing");
        near(&format!("{n} T"), roof.line("T"), 48.0, 0.01);
    }
    let roof = calc(&wings_front(20.0, &[2.5, 12.5], &[3.5, 3.5], Side::Gable));
    assert_eq!(joined(&roof), Vec::<Vec<usize>>::new(), "two high wings");
}

/// Pins `MAX_WINGS`: five low wings keep the core's error, with no panic and
/// no hang (the bound itself is `wing_split::tests::the_peel_is_bounded`).
#[test]
fn five_low_wings_keep_the_core_error() {
    let p = wings_front(50.0, &[2.5, 12.5, 22.5, 32.5, 42.5], &[2.0; 5], Side::Gable);
    let roof = calc(&p);
    assert!(
        roof.geos[0].error.is_some(),
        "five wings are over the peel's bound: {:?}",
        kinds(&roof, 0)
    );
}

/// The corners of a wing building as literal `(x, y, pitch, eave)` rows
/// (overhang 0.5, height 5.0), copied from the bug hunt's reproducers.
fn literal(rows: &[(f64, f64, f64, f64)]) -> Project {
    has_height(project(
        rows.iter()
            .map(|&(x, y, p, h)| edge(x, y, p, h, 0.5))
            .collect(),
    ))
}

/// BUG-2 (S3): three low wings in front of a house, turned 15 degrees and
/// rounded to centimetres, calculate like the unturned building.
#[test]
fn three_wings_turned_15_degrees_and_rounded_calculate_like_the_unturned_one() {
    let unturned = literal(&[
        (0.0, 0.0, 40.0, 5.0),
        (4.0, 0.0, 90.0, 5.0),
        (4.0, -2.5, 40.0, 2.0),
        (9.0, -2.5, 90.0, 5.0),
        (9.0, 0.0, 40.0, 5.0),
        (12.0, 0.0, 90.0, 5.0),
        (12.0, -2.5, 40.0, 2.0),
        (17.0, -2.5, 90.0, 5.0),
        (17.0, 0.0, 40.0, 5.0),
        (20.0, 0.0, 90.0, 5.0),
        (20.0, -2.5, 40.0, 2.0),
        (25.0, -2.5, 90.0, 5.0),
        (25.0, 0.0, 40.0, 5.0),
        (29.0, 0.0, 90.0, 5.0),
        (29.0, 9.0, 40.0, 5.0),
        (0.0, 9.0, 90.0, 5.0),
    ]);
    let turned15 = literal(&[
        (0.0, 0.0, 40.0, 5.0),
        (3.86, 1.04, 90.0, 5.0),
        (4.51, -1.38, 40.0, 2.0),
        (9.34, -0.09, 90.0, 5.0),
        (8.69, 2.33, 40.0, 5.0),
        (11.59, 3.11, 90.0, 5.0),
        (12.24, 0.69, 40.0, 2.0),
        (17.07, 1.99, 90.0, 5.0),
        (16.42, 4.4, 40.0, 5.0),
        (19.32, 5.18, 90.0, 5.0),
        (19.97, 2.76, 40.0, 2.0),
        (24.8, 4.06, 90.0, 5.0),
        (24.15, 6.47, 40.0, 5.0),
        (28.01, 7.51, 90.0, 5.0),
        (25.68, 16.2, 40.0, 5.0),
        (-2.33, 8.69, 90.0, 5.0),
    ]);
    let straight = calc(&unturned);
    assert!(
        straight.geos[0].error.is_none(),
        "{:?}",
        straight.geos[0].error
    );
    assert_eq!(kinds(&straight, 0), vec![WarningKind::WingJoined]);
    let roof = calc(&turned15);
    assert!(
        roof.geos[0].error.is_none(),
        "turned 15: {:?}",
        roof.geos[0].error
    );
    assert_eq!(kinds(&roof, 0), vec![WarningKind::WingJoined]);
    near("area", roof.area, straight.area, 0.5);
    near("T", roof.line("T"), straight.line("T"), 0.1);
}

/// BUG-1 (S2): a turned comb building (36 walls, wings on the front side,
/// cm-rounded) must give a result or an error, never a panic (index out of
/// bounds in `drop_short_junction_hips`, reached through the sweep).
#[test]
fn a_turned_comb_building_gives_a_result_or_an_error_and_never_panics() {
    let rows: &[(f64, f64, f64, f64)] = &[
        (0.0, 0.0, 40.0, 5.0),
        (2.1, 0.06, 12.91770491962296, 1.6561418681328373),
        (2.22, -3.89, 12.91770491962296, 1.6561418681328373),
        (14.38, -3.54, 12.91770491962296, 1.6561418681328373),
        (14.26, 0.41, 40.0, 5.0),
        (25.18, 0.73, 27.76335139090779, 1.5210112030146732),
        (25.27, -2.54, 27.76335139090779, 1.5210112030146732),
        (35.45, -2.24, 27.76335139090779, 1.5210112030146732),
        (35.36, 1.02, 40.0, 5.0),
        (39.28, 1.13, 90.0, 5.0),
        (39.18, 4.54, 90.0, 5.0),
        (41.03, 4.6, 58.823945531022325, 4.744635913197156),
        (40.99, 5.88, 90.0, 5.0),
        (39.15, 5.83, 90.0, 5.0),
        (38.94, 12.83, 90.0, 5.0),
        (42.1, 12.92, 29.892683270974988, 3.8296573962249707),
        (42.05, 14.52, 90.0, 5.0),
        (38.9, 14.43, 90.0, 5.0),
        (38.89, 14.83, 40.0, 5.0),
        (33.93, 14.69, 37.496201914704216, 2.0449269293662784),
        (33.83, 18.12, 37.496201914704216, 2.0449269293662784),
        (22.09, 17.78, 37.496201914704216, 2.0449269293662784),
        (22.19, 14.35, 40.0, 5.0),
        (-0.39, 13.7, 90.0, 5.0),
        (-0.36, 12.59, 22.68455010830401, 1.3632346894249725),
        (-3.59, 12.49, 22.68455010830401, 1.3632346894249725),
        (-3.52, 10.21, 22.68455010830401, 1.3632346894249725),
        (-0.3, 10.3, 90.0, 5.0),
        (-0.24, 8.16, 41.5368681166465, 1.3042488280676645),
        (-2.22, 8.11, 41.5368681166465, 1.3042488280676645),
        (-2.15, 5.74, 41.5368681166465, 1.3042488280676645),
        (-0.17, 5.8, 90.0, 5.0),
        (-0.12, 4.05, 27.718791491250514, 1.713579277432596),
        (-3.09, 3.97, 27.718791491250514, 1.713579277432596),
        (-3.01, 0.96, 27.718791491250514, 1.713579277432596),
        (-0.03, 1.05, 90.0, 5.0),
    ];
    let p = literal(rows);
    let geos = tectora_core::openings::generate_project(&p);
    assert_eq!(geos.len(), 1);
}

// ============================================================ fix round 2
// Findings of review core-leftovers-r2 (math.md) and of the second bug hunt
// (findings-bugs-r2.md). Each test names its finding.

fn departed(roof: &Roof) -> bool {
    roof.geos[0].warnings.iter().any(|w| {
        matches!(
            w.kind,
            WarningKind::MergedPlanes | WarningKind::WingJoined | WarningKind::NotCalculatedByCore
        )
    })
}

/// MATH-001 (S2): Jakub's T turned by every tenth of a degree of a quarter
/// turn and rounded to centimetres gets the unturned roof. The pieces of a
/// wall line were taken for two lines at 135 (joined plane) and 56
/// (lean-to) of 3600 turns; what is left are the turns where the rounding
/// drives the pieces of one line as far apart as it leaves a 2 cm step
/// (`straighten.rs`). The budgets are the counts measured, not a target.
#[test]
fn a_cm_rounded_t_gets_the_unturned_roof_at_nearly_every_turn() {
    for (front, notice, faces, budget) in [
        (2.9, WarningKind::MergedPlanes, 2, 3),
        (2.0, WarningKind::WingJoined, 3, 3),
    ] {
        let mut off = Vec::new();
        for k in 0..900 {
            let deg = k as f64 * 0.1;
            let roof = calc(&turned_cm(&jakub_t(front), deg));
            let same = roof.geos[0].error.is_none()
                && kinds(&roof, 0) == vec![notice]
                && roof.geos[0].faces.len() == faces
                && roof.line("K").abs() < 1e-6;
            if !same {
                off.push(deg);
            }
        }
        assert!(
            off.len() <= budget,
            "front {front}: not the unturned roof at {} of 900 turns (budget {budget}): {off:?}",
            off.len()
        );
    }
    // The turns the review named: an error where round 1 gave the roof,
    // and the sweep's roof with 5 m of the eave called a lean-to edge.
    let straight = calc(&jakub_t(2.9));
    for deg in [4.1, 5.3] {
        let roof = calc(&turned_cm(&jakub_t(2.9), deg));
        let n = format!("front 2.9 turned {deg}");
        assert!(
            roof.geos[0].error.is_none(),
            "{n}: {:?}",
            roof.geos[0].error
        );
        assert_eq!(kinds(&roof, 0), vec![WarningKind::MergedPlanes], "{n}");
        near(&format!("{n} T"), roof.line("T"), straight.line("T"), 0.05);
        near(&format!("{n} P"), roof.line("P"), 0.0, 1e-6);
        near(&format!("{n} area"), roof.area, straight.area, 0.15);
    }
    let roof = calc(&turned_cm(&jakub_t(2.0), 1.9));
    assert!(roof.geos[0].error.is_none(), "{:?}", roof.geos[0].error);
    assert_eq!(kinds(&roof, 0), vec![WarningKind::WingJoined]);
}

/// MATH-002 (S2): whether a stepped roof is merged, and whether the wing's
/// plane is said to run over the house wall, does not depend on where the
/// building stands. The rise between two parallel planes was measured from
/// the project's origin: 500 m away, a wall 0.01 degrees off its direction
/// was centimetres of rise.
#[test]
fn a_stepped_roof_is_merged_wherever_the_building_stands() {
    let at_origin = turned(&jakub_t(2.95), 0.1);
    let far = moved(&at_origin, 500.0, 500.0);
    let (a, b) = (calc(&at_origin), calc(&far));
    assert_eq!(kinds(&a, 0), vec![WarningKind::MergedPlanes]);
    assert_eq!(
        kinds(&b, 0),
        kinds(&a, 0),
        "merged at the origin, not at +500"
    );
    assert_eq!(b.geos[0].faces.len(), a.geos[0].faces.len());
    near("valley at +500", b.line("K"), a.line("K"), 0.01);
    near("verge at +500", verge(&b), verge(&a), 0.01);
    near("area at +500", b.area, a.area, 0.01);
    // The planes themselves: same rise and depth at both places.
    let planes = |p: &Project| tectora_core::check::wing_planes(&p.parts[0]);
    let (pa, pb) = (planes(&at_origin), planes(&far));
    assert_eq!(pa.len(), pb.len());
    for (x, y) in pa.iter().zip(&pb) {
        assert_eq!((x.outer, x.inner), (y.outer, y.inner));
        near("rise", y.rise, x.rise, 1e-6);
        near("depth", y.depth, x.depth, 1e-6);
    }
    // The two house-front pieces in line were reported as a plane over a
    // wall at +500 (turned 4.8 degrees, millimetres).
    let far = calc(&moved(&turned(&jakub_t(2.9), 4.8), 500.0, 500.0));
    assert_eq!(kinds(&far, 0), vec![WarningKind::MergedPlanes]);
}

/// MATH-003 (S2): a stepped roof the core calculates (fronts 2.95 and 2.9)
/// on a centimetre-rounded turned T is merged without a stray valley: the
/// merge of a roof the core calculated did not straighten the outline, and
/// one of the two house-front pieces stayed a face of its own.
#[test]
fn a_merged_stepped_roof_has_no_stray_valley() {
    for (front, deg) in [(2.95, 97.5), (2.9, 12.9), (3.0, 97.5)] {
        let straight = calc(&jakub_t(front));
        assert_eq!(kinds(&straight, 0), vec![WarningKind::MergedPlanes]);
        let roof = calc(&turned_cm(&jakub_t(front), deg));
        let n = format!("front {front} turned {deg}");
        assert!(
            roof.geos[0].error.is_none(),
            "{n}: {:?}",
            roof.geos[0].error
        );
        assert_eq!(kinds(&roof, 0), vec![WarningKind::MergedPlanes], "{n}");
        assert_eq!(roof.geos[0].faces.len(), 2, "{n}");
        near(&format!("{n} K"), roof.line("K"), 0.0, 1e-6);
        near(&format!("{n} verge"), verge(&roof), verge(&straight), 0.05);
    }
}

/// `roof` against `want`, the roof of the same building (`dx`, `dy`) away:
/// the same notices, faces and lines, every corner where it was.
fn assert_same_roof_moved(n: &str, roof: &Roof, want: &Roof, dx: f64, dy: f64) {
    assert_eq!(
        roof.geos[0].error.as_ref().map(|e| e.kind),
        want.geos[0].error.as_ref().map(|e| e.kind),
        "{n}: error"
    );
    assert_eq!(kinds(roof, 0), kinds(want, 0), "{n}: notices");
    assert_eq!(
        roof.geos[0].faces.len(),
        want.geos[0].faces.len(),
        "{n}: faces"
    );
    near(&format!("{n} area"), roof.area, want.area, 1e-6);
    for (code, len) in &want.lines {
        near(&format!("{n} {code}"), roof.line(code), *len, 1e-6);
    }
    for (f, g) in roof.geos[0].faces.iter().zip(&want.geos[0].faces) {
        assert_eq!(f.edge, g.edge, "{n}: face wall");
        assert_eq!(f.polygon.len(), g.polygon.len(), "{n}: corners");
        for (p, q) in f.polygon.iter().zip(&g.polygon) {
            let d = norm(sub([p[0] - dx, p[1] - dy, p[2]], *q));
            assert!(d < 1e-6, "{n}: corner {p:?} is {d} m from {q:?} moved");
        }
    }
}

/// BUG-R2-1 (S2): a roof Tectora calculates where the core stops is the
/// same roof wherever the building stands: every way is tried in the
/// building's own frame. The literal is building 452 of hunt seed 42 (a
/// house with one wing, turned 167.9 degrees, centimetres), which got the
/// sweep's roof at the origin and an error 2.5 km away.
#[test]
fn a_departed_roof_does_not_depend_on_where_the_building_stands() {
    let hunt = has_height(literal(&[
        (-0.0, 0.0, 40.0, 5.0),
        (-7.03, 1.51, 90.0, 5.0),
        (-6.69, 3.08, 29.906062598917956, 5.142476495333693),
        (-19.87, 5.9, 90.0, 5.0),
        (-20.2, 4.33, 40.0, 5.0),
        (-25.95, 5.57, 90.0, 5.0),
        (-28.49, -6.26, 40.0, 5.0),
        (-2.54, -11.83, 90.0, 5.0),
    ]));
    let cases = [
        ("hunt 42/452", hunt),
        ("T 2.9 cm 17.3", turned_cm(&jakub_t(2.9), 17.3)),
        ("T 2.0 cm 33", turned_cm(&jakub_t(2.0), 33.0)),
        (
            "two wings cm 33",
            turned_cm(&two_wings(2.0, Side::Gable), 33.0),
        ),
        ("hip corner moved", departed_hip()),
    ];
    for (name, p) in cases {
        let want = calc(&p);
        assert!(
            want.geos[0].error.is_none() && departed(&want),
            "{name}: not a departed roof at the origin: {:?} {:?}",
            want.geos[0].error,
            kinds(&want, 0)
        );
        for (dx, dy) in [(-1422.91, 2113.9), (503.2, -437.08), (37.0, 12.5)] {
            let far = calc(&moved(&p, dx, dy));
            let n = format!("{name} at ({dx}, {dy})");
            assert_same_roof_moved(&n, &far, &want, dx, dy);
        }
    }
}

/// BUG-R2-2 (S2): a 44-wall building with wings on all sides, turned and
/// rounded to centimetres (hunt seed 47, building 281), took 45 to 60 s:
/// 22 of the wing search's core runs ended at the core's own iteration
/// limit, half a second each. All ways of one building share one budget of
/// core runs, and of runs that end at that limit. Counted, not timed.
#[test]
fn the_ways_tried_for_one_building_share_one_budget_of_core_runs() {
    let comb = has_height(literal(&[
        (0.0, 0.0, 40.0, 5.0),
        (1.5, -0.84, 15.650285199662658, 4.394324198693518),
        (-0.2, -3.86, 15.650285199662658, 4.394324198693518),
        (2.36, -5.31, 15.650285199662658, 4.394324198693518),
        (4.06, -2.29, 40.0, 5.0),
        (6.08, -3.42, 90.0, 5.0),
        (4.93, -5.46, 22.4679022737027, 1.4624954502122707),
        (8.51, -7.47, 90.0, 5.0),
        (9.65, -5.44, 40.0, 5.0),
        (11.47, -6.47, 90.0, 5.0),
        (9.79, -9.46, 16.03772502023963, 0.616776835849669),
        (12.47, -10.97, 90.0, 5.0),
        (14.16, -7.98, 40.0, 5.0),
        (16.26, -9.17, 41.65503842413344, 3.5450663172972647),
        (14.72, -11.91, 41.65503842413344, 3.5450663172972647),
        (17.84, -13.67, 41.65503842413344, 3.5450663172972647),
        (19.39, -10.93, 40.0, 5.0),
        (21.52, -12.13, 90.0, 5.0),
        (20.52, -13.91, 57.37358818089856, 2.3020997221822395),
        (23.57, -15.63, 90.0, 5.0),
        (24.58, -13.85, 40.0, 5.0),
        (25.7, -14.49, 90.0, 5.0),
        (26.13, -13.72, 90.0, 5.0),
        (27.77, -14.64, 39.58014157518594, 1.188915497676895),
        (28.37, -13.57, 90.0, 5.0),
        (26.73, -12.65, 90.0, 5.0),
        (27.37, -11.53, 48.49378150751611, 4.964554119740982),
        (30.73, -13.43, 48.49378150751611, 4.964554119740982),
        (31.71, -11.69, 48.49378150751611, 4.964554119740982),
        (28.34, -9.79, 90.0, 5.0),
        (28.79, -9.0, 37.34544105408092, 2.4744416575381427),
        (30.25, -9.82, 37.34544105408092, 2.4744416575381427),
        (31.22, -8.09, 37.34544105408092, 2.4744416575381427),
        (29.77, -7.27, 90.0, 5.0),
        (33.02, -1.49, 40.0, 5.0),
        (31.83, -0.82, 13.28371709256814, 0.5006464958228725),
        (33.5, 2.13, 13.28371709256814, 0.5006464958228725),
        (27.63, 5.44, 13.28371709256814, 0.5006464958228725),
        (25.96, 2.49, 40.0, 5.0),
        (21.94, 4.75, 57.058586948512605, 2.4807819674015885),
        (23.56, 7.62, 57.058586948512605, 2.4807819674015885),
        (20.04, 9.61, 57.058586948512605, 2.4807819674015885),
        (18.42, 6.74, 40.0, 5.0),
        (7.32, 12.99, 90.0, 5.0),
    ]));
    let kind = match tectora_core::engine::generate(&comb) {
        Err(tectora_core::engine::Failure::Dacad(e)) => e.kind,
        other => panic!("the core stops on the comb: {:?}", other.map(|g| g.len())),
    };
    let (runs, at_limit) = tectora_core::openings::departure_runs(&comb, kind);
    assert!(runs <= 64, "{runs} core runs");
    assert!(
        at_limit <= 2,
        "{at_limit} core runs ended at the core's limit"
    );
    assert!(runs > 0);
    // Whatever the result, it is a result: an error or a sound roof.
    let roof = calc(&comb);
    assert_eq!(roof.geos.len(), 1);
    assert!(roof.geos[0].error.is_some() || !roof.geos[0].faces.is_empty());
    // A building the search closes is not cut short by the budget.
    let (runs, at_limit) = tectora_core::openings::departure_runs(
        &three_wings(),
        tectora_core::roof::CalcErrorKind::Cut,
    );
    assert!(
        runs <= 8 && at_limit == 0,
        "{runs} runs, {at_limit} at the limit"
    );
}

// ------------------------------------------------- windows on departed roofs

/// TEST-001: a small window put anywhere on a roof Tectora calculated
/// where the standard calculation stops (merged planes, joined wings, the
/// sweep) is deducted from the face it lies on, and from that face only.
#[test]
fn windows_anywhere_on_a_departed_roof_stay_on_their_face() {
    let mut roofs: Vec<(&str, Project)> = departed_roofs()
        .into_iter()
        .map(|(n, p, _, _)| (n, p))
        .collect();
    roofs.push(("two lean-to wings", two_wings(2.0, Side::Gable)));
    roofs.push(("L wing", l_wing()));
    let (mut tried, mut bad) = (0, Vec::new());
    for (name, base) in &roofs {
        let (t, b) = windows_stay_on_their_face(name, base);
        tried += t;
        bad.extend(b);
    }
    assert!(tried > 200, "only {tried} windows placed");
    assert!(
        bad.is_empty(),
        "{} of {tried}:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

// ------------------------------------------- audit 2026-11-23 HUNT-1502

/// Building A, a gable roof, and building B with four gable walls, which
/// the core cannot roof (`open_faces`).
fn gable_and_failing_building() -> Project {
    let mut p = Project::new("Two buildings");
    let a = [
        (0.0, 0.0, 40.0),
        (12.0, 0.0, G),
        (12.0, 8.0, 40.0),
        (0.0, 8.0, G),
    ];
    let b = [(40.0, 0.0), (46.0, 0.0), (46.0, 6.0), (40.0, 6.0)];
    p.parts = vec![
        RoofPart::new(
            "A",
            a.iter().map(|&(x, y, d)| edge(x, y, d, 3.0, 0.5)).collect(),
        ),
        RoofPart::new(
            "B",
            b.iter().map(|&(x, y)| edge(x, y, G, 3.0, 0.5)).collect(),
        ),
    ];
    p
}

fn square(x: f64, y: f64, s: f64) -> tectora_core::model::Chimney {
    tectora_core::model::Chimney {
        corners: vec![
            Point2::new(x, y),
            Point2::new(x + s, y),
            Point2::new(x + s, y + s),
            Point2::new(x, y + s),
        ],
        height: 1.0,
    }
}

fn window(x: f64, y: f64) -> tectora_core::model::RoofWindow {
    tectora_core::model::RoofWindow {
        x,
        y,
        width: 0.78,
        height: 1.18,
    }
}

/// The off-roof warnings of `part` as `(kind, item)`.
fn off_items(roof: &Roof, part: usize) -> Vec<(WarningKind, Option<usize>)> {
    roof.geos[part]
        .warnings
        .iter()
        .filter(|w| {
            matches!(
                w.kind,
                WarningKind::WindowOffRoof
                    | WarningKind::ChimneyOffRoof
                    | WarningKind::OpeningOffRoof
            )
        })
        .map(|w| (w.kind, w.item))
        .collect()
}

/// HUNT-1502: an object on no roof face next to a building that calculated
/// is reported while another building of the job fails; objects over the
/// failing building cannot be judged and are not; nothing else changes.
#[test]
fn an_object_off_the_roof_is_warned_while_another_building_fails() {
    let base = gable_and_failing_building();
    let bare = calc(&base);
    assert!(bare.geos[0].error.is_none(), "{:?}", bare.geos[0].error);
    assert!(bare.geos[1].error.is_some(), "B fails");
    let mut p = base.clone();
    // Below A: off every face, nearest to A.
    p.windows.push(window(6.0, -5.0));
    p.chimneys.push(square(6.0, -7.0, 0.6));
    p.roof_openings.push(square(3.0, -6.0, 0.8));
    // Over B, which has no roof: not judged.
    p.windows.push(window(43.0, 3.0));
    p.chimneys.push(square(42.0, 2.0, 0.6));
    let roof = calc(&p);
    assert_eq!(
        off_items(&roof, 0),
        vec![
            (WarningKind::WindowOffRoof, Some(0)),
            (WarningKind::ChimneyOffRoof, Some(0)),
            (WarningKind::OpeningOffRoof, Some(0)),
        ]
    );
    assert_eq!(off_items(&roof, 1), vec![]);
    // The roof itself is the one without the objects.
    for (g, b) in roof.geos.iter().zip(&bare.geos) {
        assert_eq!(g.faces, b.faces);
        assert_eq!(
            g.error.as_ref().map(|e| e.kind),
            b.error.as_ref().map(|e| e.kind)
        );
    }
    near("area", roof.area, bare.area, 1e-9);
    assert_eq!(roof.lines, bare.lines);
}

/// HUNT-1502 (pin): when every building fails nothing is reported, as
/// before (the calculation's errors say enough).
#[test]
fn no_object_is_warned_when_every_building_fails() {
    let mut p = gable_and_failing_building();
    p.parts.remove(0);
    p.windows.push(window(30.0, -5.0));
    p.chimneys.push(square(30.0, -7.0, 0.6));
    let roof = calc(&p);
    assert!(roof.geos.iter().all(|g| g.error.is_some()));
    assert_eq!(off_items(&roof, 0), vec![]);
}
