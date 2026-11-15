//! Core leftovers (slug `core-leftovers`, plan acceptance criteria 1 to 9
//! and 13 to 15): buildings with several low wings, and a dormer that sits
//! on no face of a roof Tectora calculated where DACAD's core stops.
//!
//! Expected figures are closed forms, not copied outputs. Wing roofs are
//! Tectora's own departure (Jakub's decision 2026-11-14): the roof is
//! DACAD's join of the buildings a roofer would draw. The straightening of
//! centimetre-rounded turns (criteria 10 to 12) is not tested here: it
//! waits for the owner's decision.

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
        let bare = calc(&base);
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
        let _ = bare;
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
