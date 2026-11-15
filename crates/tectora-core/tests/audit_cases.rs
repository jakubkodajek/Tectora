//! Regression tests of the pre-release audit of 2026-11-15 (boards math,
//! math2, math-core, hunt-core, errors). One test per finding, the
//! geometry written out as literals.
//!
//! Where a finding is the standard calculation's own result, the result is
//! kept and the test pins the warning that now says so; the figures the
//! calculation gives are asserted only where they are a closed form.

use tectora_core::check::WarningKind;
use tectora_core::edit::dormers::{DormerKind, build_dormer, default_params};
use tectora_core::geom::Point2;
use tectora_core::model::{Chimney, PitchBreak, RoofWindow};
use tectora_core::{Project, RoofPart};

mod support;
use support::*;

const G: f64 = 90.0;

/// A rectangle `w × d` at `(x0, y0)`, walls front, right, back, left, each
/// `(pitch, eave height, overhang)`.
fn block(name: &str, x0: f64, y0: f64, w: f64, d: f64, defs: [(f64, f64, f64); 4]) -> RoofPart {
    let corners = [(x0, y0), (x0 + w, y0), (x0 + w, y0 + d), (x0, y0 + d)];
    RoofPart {
        height: None,
        name: name.into(),
        edges: corners
            .iter()
            .zip(defs)
            .map(|(&(x, y), (p, h, o))| edge(x, y, p, h, o))
            .collect(),
    }
}

/// A gable roof: front and back eaves at `pitch`, the ends gables.
fn gable(name: &str, x0: f64, y0: f64, w: f64, d: f64, pitch: f64, eave: f64) -> RoofPart {
    let (e, g) = ((pitch, eave, 0.5), (G, eave, 0.5));
    block(name, x0, y0, w, d, [e, g, e, g])
}

/// A hip roof with one pitch.
fn hip(name: &str, x0: f64, y0: f64, w: f64, d: f64, pitch: f64, eave: f64) -> RoofPart {
    block(name, x0, y0, w, d, [(pitch, eave, 0.5); 4])
}

fn job(parts: Vec<RoofPart>) -> Project {
    let mut p = Project::new("audit");
    p.parts = parts;
    p
}

fn square(x: f64, y: f64, w: f64, d: f64, height: f64) -> Chimney {
    Chimney {
        corners: vec![
            Point2::new(x, y),
            Point2::new(x + w, y),
            Point2::new(x + w, y + d),
            Point2::new(x, y + d),
        ],
        height,
    }
}

fn kinds(roof: &Roof, part: usize) -> Vec<WarningKind> {
    roof.geos[part].warnings.iter().map(|w| w.kind).collect()
}

fn all_kinds(roof: &Roof) -> Vec<WarningKind> {
    (0..roof.geos.len()).flat_map(|i| kinds(roof, i)).collect()
}

fn near(what: &str, got: f64, want: f64, tol: f64) {
    assert!(
        (got - want).abs() <= tol,
        "{what}: got {got}, want {want} (tolerance {tol})"
    );
}

// ------------------------------------------------------------- MATH-001

/// The 12 × 8 m gable at 45° (ridge at 3 + 4.5 = 7.50 m) with a pitch
/// break typed at the ridge height.
fn break_at_the_ridge() -> RoofPart {
    let mut a = gable("A", 0.0, 0.0, 12.0, 8.0, 45.0, 3.0);
    for i in [0, 2] {
        a.edges[i].breaks.push(PitchBreak {
            height: 7.5,
            pitch_deg: 20.0,
        });
    }
    a
}

/// MATH-001: the calculation gives the two break faces one corner and an
/// area that depends on where the building stands (220.80 m² here, 165.46
/// is the roof). The result stays; the area check must see those faces.
#[test]
fn a_pitch_break_at_the_ridge_height_is_warned_as_an_area_mismatch() {
    let roof = calc(&job(vec![break_at_the_ridge()]));
    assert!(roof.geos[0].error.is_none());
    let exact = 13.0 * 9.0 / cos(45.0);
    if (roof.area - exact).abs() > 0.01 * exact {
        assert!(
            kinds(&roof, 0).contains(&WarningKind::AreaMismatch),
            "area {} against {exact} with no warning: {:?}",
            roof.area,
            kinds(&roof, 0)
        );
    }
    // Wherever it is drawn.
    let mut far = break_at_the_ridge();
    for e in far.edges.iter_mut() {
        e.start.y += 100.0;
    }
    let roof = calc(&job(vec![far]));
    if (roof.area - exact).abs() > 0.01 * exact {
        assert!(kinds(&roof, 0).contains(&WarningKind::AreaMismatch));
    }
    // A break below the ridge is a sound mansard: no warning.
    let mut sound = break_at_the_ridge();
    for i in [0, 2] {
        sound.edges[i].breaks[0].height = 6.0;
    }
    let roof = calc(&job(vec![sound]));
    assert_eq!(kinds(&roof, 0), vec![]);
}

/// MATH-001, hipped: every wall at 45° with the break at the ridge height.
/// The calculation leaves four 20° faces standing over the ridge (17 m²
/// more than the roof); they lie over the roof's own faces.
#[test]
fn faces_left_over_the_ridge_by_a_break_at_its_height_are_warned() {
    let mut a = hip("A", 0.0, 0.0, 12.0, 8.0, 45.0, 3.0);
    for e in a.edges.iter_mut() {
        e.breaks.push(PitchBreak {
            height: 7.5,
            pitch_deg: 20.0,
        });
    }
    let roof = calc(&job(vec![a]));
    assert!(roof.geos[0].error.is_none());
    let exact = 13.0 * 9.0 / cos(45.0);
    if (roof.area - exact).abs() > 0.01 * exact {
        let k = kinds(&roof, 0);
        assert!(
            k.contains(&WarningKind::OverlappingFaces) || k.contains(&WarningKind::AreaMismatch),
            "area {} against {exact} with no warning: {k:?}",
            roof.area
        );
    }
}

// ------------------------------------------------------------ MATHC-001

/// MATHC-001: two hip roofs that touch, the second with a higher eave.
/// The join reports the lower roof's north-east hip once as an oblique
/// verge of 7.40 m and once more as a hip of 0.82 m lying on its first
/// 0.82 m. The result stays; the doubled piece is said.
#[test]
fn a_roof_line_lying_on_another_line_is_warned() {
    let low = hip("A", 0.0, 0.0, 10.0, 8.0, 40.0, 3.0);
    let high = hip("B", 10.0, 0.0, 10.0, 8.0, 40.0, 4.5);
    let roof = calc(&job(vec![low.clone(), high]));
    assert!(roof.geos.iter().all(|g| g.error.is_none()));
    // Two hip roofs have no verge: where the result has one, it must warn.
    let verge = roof.line("OS") + roof.line("OL") + roof.line("OR");
    if verge > 0.05 {
        let w = roof.geos[0]
            .warnings
            .iter()
            .find(|w| w.kind == WarningKind::DoubledLine)
            .unwrap_or_else(|| panic!("verge {verge} m, no warning: {:?}", kinds(&roof, 0)));
        // The hip stub from the wall corner (10, 8) to the eave corner
        // (10.5, 8.5): 0.5 in x and y, rising 0.5 tan 40°.
        near(
            "doubled length",
            w.value,
            (0.5f64 * 0.5 * 2.0 + (0.5 * tan(40.0)).powi(2)).sqrt(),
            0.02,
        );
    }
    // The lower roof alone has every line once.
    let roof = calc(&job(vec![low]));
    assert_eq!(kinds(&roof, 0), vec![]);
}

// ------------------------------------------------------------ MATHC-002

/// MATHC-002: the calculation works in single precision on the plan's own
/// coordinates. A 10 × 8 m hip roof (99 / cos 40° = 129.24 m²) comes out
/// 129.37 m² at 30 km and 2855.85 m² at 70 km from the origin. The result
/// stays; from 2.5 km on the building says that it stands far out.
#[test]
fn a_building_far_from_the_origin_is_warned() {
    for d in [2500.0, 30_000.0, 70_000.0] {
        let roof = calc(&job(vec![hip("A", d, d, 10.0, 8.0, 40.0, 3.0)]));
        assert!(roof.geos[0].error.is_none(), "{d}");
        let w = roof.geos[0]
            .warnings
            .iter()
            .find(|w| w.kind == WarningKind::FarFromOrigin)
            .unwrap_or_else(|| panic!("{d} m out: {:?}", kinds(&roof, 0)));
        near("distance", w.value, d + 10.0, 1e-6);
    }
    // Within 2.5 km the area is the closed form and nothing is said.
    for d in [0.0, 500.0, 2000.0] {
        let roof = calc(&job(vec![hip("A", d, d, 10.0, 8.0, 40.0, 3.0)]));
        near("area", roof.area, 11.0 * 9.0 / cos(40.0), 0.05);
        assert_eq!(kinds(&roof, 0), vec![], "{d} m out");
    }
}

// ------------------------------------------------- MATHC-003, HUNTC-004

/// MATHC-003: every eave at 100 m, the calculation's own top level: it
/// closes every face empty and reports success with 0 m². The result
/// stays; the building says that its roof has no area.
#[test]
fn a_roof_the_calculation_closes_empty_is_warned() {
    for eave in [100.0, 99.995, 100.006] {
        let roof = calc(&job(vec![hip("A", 0.0, 0.0, 10.0, 8.0, 40.0, eave)]));
        let g = &roof.geos[0];
        if g.error.is_none() && roof.area.abs() < 0.005 {
            assert!(
                kinds(&roof, 0).contains(&WarningKind::NoRoofArea),
                "eave {eave}: 0 m2 with {:?}",
                kinds(&roof, 0)
            );
        }
    }
    // An ordinary roof has an area and no such warning.
    let roof = calc(&job(vec![hip("A", 0.0, 0.0, 10.0, 8.0, 40.0, 3.0)]));
    assert_eq!(kinds(&roof, 0), vec![]);
}

/// HUNTC-004: a chimney typed in centimetres (90 × 90 m) or a roof opening
/// drawn around the whole roof takes every face away. The roof calculates
/// with 0 m²; it must say so.
#[test]
fn a_chimney_or_opening_that_takes_the_whole_roof_is_warned() {
    let house = hip("A", 0.0, 0.0, 10.0, 8.0, 35.0, 3.0);
    let mut p = job(vec![house.clone()]);
    p.chimneys.push(square(-40.0, -40.0, 90.0, 90.0, 7.0));
    let roof = calc(&p);
    assert!(roof.geos[0].error.is_none());
    near("area", roof.area, 0.0, 0.005);
    assert!(
        kinds(&roof, 0).contains(&WarningKind::NoRoofArea),
        "chimney: {:?}",
        kinds(&roof, 0)
    );
    let mut p = job(vec![house]);
    p.roof_openings.push(square(-1.0, -1.0, 12.0, 10.0, 0.0));
    let roof = calc(&p);
    assert!(roof.geos[0].error.is_none());
    assert!(
        kinds(&roof, 0).contains(&WarningKind::NoRoofArea),
        "opening: {:?}",
        kinds(&roof, 0)
    );
}

// ------------------------------------------------------------- MATH-003

/// MATH-003: objects on top of each other are each deducted in full. The
/// figures stay (a window is its width × height, as the reference deducts
/// it); the building says that objects overlap.
#[test]
fn objects_on_top_of_each_other_are_warned() {
    let house = || job(vec![gable("A", 0.0, 0.0, 12.0, 8.0, 40.0, 3.0)]);
    let overlap = |roof: &Roof| {
        roof.geos[0]
            .warnings
            .iter()
            .find(|w| w.kind == WarningKind::ObjectsOverlap)
            .map(|w| w.value)
    };
    // Two windows on one spot.
    let mut p = house();
    p.windows = vec![
        RoofWindow {
            x: 5.0,
            y: 1.0,
            width: 1.0,
            height: 1.0,
        };
        2
    ];
    assert_eq!(overlap(&calc(&p)), Some(1.0), "two windows");
    // Two chimneys overlapping by a quarter.
    let mut p = house();
    p.chimneys = vec![
        square(5.0, 1.0, 1.0, 1.0, 7.0),
        square(5.5, 1.5, 1.0, 1.0, 7.0),
    ];
    assert_eq!(overlap(&calc(&p)), Some(1.0), "two chimneys");
    // A window on a chimney and an opening on the same chimney.
    let mut p = house();
    p.chimneys = vec![square(5.0, 1.0, 1.0, 1.0, 7.0)];
    p.roof_openings = vec![square(5.2, 1.2, 0.5, 0.5, 0.0)];
    p.windows = vec![RoofWindow {
        x: 5.5,
        y: 1.5,
        width: 0.6,
        height: 0.6,
    }];
    assert_eq!(overlap(&calc(&p)), Some(3.0), "window, chimney, opening");
    // The same dormer twice.
    let mut p = house();
    let mut d = default_params(DormerKind::Gable, Point2::new(5.0, 1.0), 0.0, 5.0);
    d.width = 2.0;
    p.dormers = vec![build_dormer(&d); 2];
    assert_eq!(overlap(&calc(&p)), Some(1.0), "two dormers");
    // Objects side by side: nothing.
    let mut p = house();
    p.chimneys = vec![
        square(5.0, 1.0, 1.0, 1.0, 7.0),
        square(7.0, 1.0, 1.0, 1.0, 7.0),
    ];
    p.windows = vec![RoofWindow {
        x: 2.0,
        y: 1.5,
        width: 0.8,
        height: 1.0,
    }];
    assert_eq!(kinds(&calc(&p), 0), vec![], "side by side");
}

// ------------------------------------------------------------- MATH-004

/// MATH-004: a dormer whose roof misses its host face before the ridge is
/// cut with the face's plane continued past the ridge (cheeks at 85°: the
/// dormer's ridge at 16.4 m over a house ridge at 6.78 m, 309 m² instead
/// of about 153). The figures stay; the building says that the dormer
/// rises above the roof.
#[test]
fn a_dormer_rising_above_the_roof_it_sits_on_is_warned() {
    let house = || job(vec![gable("A", 0.0, 0.0, 12.0, 8.0, 40.0, 3.0)]);
    let ridge = 3.0 + 4.5 * tan(40.0);
    let above = |roof: &Roof| {
        roof.geos[0]
            .warnings
            .iter()
            .find(|w| w.kind == WarningKind::DormerAboveRoof)
            .map(|w| w.value)
    };
    // Cheeks at 85°.
    let mut p = house();
    let mut d = default_params(DormerKind::Gable, Point2::new(5.0, 1.0), 0.0, 5.0);
    d.pitch_deg = 85.0;
    p.dormers = vec![build_dormer(&d)];
    let roof = calc(&p);
    if roof.geos[0].error.is_none() && roof.area > 160.0 {
        let v = above(&roof).unwrap_or_else(|| panic!("steep: {:?}", kinds(&roof, 0)));
        near("above the ridge", v, 5.0 + tan(85.0) - ridge, 0.05);
    }
    // A dormer whose eave is above the house's ridge.
    let mut p = house();
    let d = default_params(DormerKind::Gable, Point2::new(5.0, 1.0), 0.0, 8.0);
    p.dormers = vec![build_dormer(&d)];
    let roof = calc(&p);
    if roof.geos[0].error.is_none() && !kinds(&roof, 0).contains(&WarningKind::DormerOffRoof) {
        assert!(above(&roof).is_some(), "eave 8: {:?}", kinds(&roof, 0));
    }
    // An ordinary dormer (eave 5 m, ridge 6 m) stays below the ridge.
    let mut p = house();
    let d = default_params(DormerKind::Gable, Point2::new(5.0, 1.0), 0.0, 5.0);
    p.dormers = vec![build_dormer(&d)];
    let roof = calc(&p);
    assert!(roof.geos[0].error.is_none());
    assert_eq!(kinds(&roof, 0), vec![]);
}

// ------------------------------------------------------------- MATH-008

/// MATH-008: a building drawn twice, the copy with a lower eave. The join
/// leaves the copy the strip under the first one's overhang (27.41 m²,
/// 26 m of eave more). The result stays; the copy says that it stands
/// inside another building.
#[test]
fn a_building_drawn_inside_another_one_is_warned() {
    let house = gable("House", 0.0, 0.0, 12.0, 8.0, 40.0, 3.0);
    let copy = gable("Copy", 0.0, 0.0, 12.0, 8.0, 40.0, 2.0);
    let roof = calc(&job(vec![house.clone(), copy]));
    assert!(roof.geos.iter().all(|g| g.error.is_none()));
    if !roof.geos[1].faces.is_empty() {
        assert!(
            kinds(&roof, 1).contains(&WarningKind::InsideAnother),
            "copy: {:?}",
            kinds(&roof, 1)
        );
    }
    // A smaller building inside the house, lower.
    let shed = gable("Shed", 2.0, 2.0, 4.0, 3.0, 30.0, 1.0);
    let roof = calc(&job(vec![house.clone(), shed]));
    let k = kinds(&roof, 1);
    assert!(
        roof.geos[1].error.is_some()
            || k.contains(&WarningKind::InsideAnother)
            || k.contains(&WarningKind::Covered),
        "shed inside: {k:?}"
    );
    // An annex against the house is not inside it.
    let annex = gable("Annex", 12.0, 2.0, 5.0, 4.0, 30.0, 2.5);
    let roof = calc(&job(vec![house, annex]));
    assert!(!all_kinds(&roof).contains(&WarningKind::InsideAnother));
}

// ------------------------------------------------------------ HUNTC-007

/// A 10 × 8 m building with every wall a gable, the front one clad: the
/// standard calculation cannot run on it and Tectora's own engine gives
/// it no face.
fn all_gables_clad(x0: f64) -> RoofPart {
    let mut a = block("Gables", x0, 0.0, 10.0, 8.0, [(G, 3.0, 0.5); 4]);
    a.edges[0].clad = true;
    a
}

/// HUNTC-007: such a building used to pass as a roof of 0 m² ("ok", no
/// error, the stored result could be taken). A roof that is not the
/// standard calculation's is shown only when it passes the self-check:
/// this one is an error.
#[test]
fn a_building_the_fallback_gives_no_roof_is_an_error_not_an_empty_roof() {
    let roof = calc(&job(vec![all_gables_clad(0.0)]));
    let g = &roof.geos[0];
    assert!(
        g.error.is_some(),
        "no error: {} faces, {:?}",
        g.faces.len(),
        kinds(&roof, 0)
    );
    assert!(g.faces.is_empty());
    // Tectora's own wording, nothing of the reference's.
    let e = g.error.as_ref().unwrap();
    assert!(e.code.is_none() && e.message.is_ascii(), "{e:?}");
}

/// HUNTC-007, several buildings: the building without a roof carries the
/// error and is named; the hip roof beside it keeps its roof, which is the
/// standard calculation's (it runs on that building alone).
#[test]
fn one_building_without_a_roof_leaves_the_others_theirs_and_says_so() {
    let house = hip("House", 30.0, 0.0, 10.0, 8.0, 40.0, 3.0);
    let roof = calc(&job(vec![house, all_gables_clad(0.0)]));
    let e = roof.geos[1]
        .error
        .as_ref()
        .expect("the gable box is an error");
    assert_eq!(e.parts, vec![1]);
    assert!(roof.geos[0].error.is_none());
    near("house area", roof.area, 11.0 * 9.0 / cos(40.0), 0.05);
    assert_eq!(kinds(&roof, 0), vec![]);
}

// ------------------------------------------------------------ MATH2-005

/// MATH2-005: a chimney without a top height (what an AI client sends, and
/// every file that leaves `height` out) was calculated as a roof opening:
/// eave, pent and verge lines around it instead of the flashing lines. A
/// chimney is a chimney whatever its height: 0.6 × 0.6 m on the 40° front
/// face gives WV 0.6, WH 0.6 and WS 2 · 0.6 / cos 40°.
#[test]
fn a_chimney_without_a_top_height_is_still_a_chimney() {
    let with = |height: f64| {
        let mut p = job(vec![gable("A", 0.0, 0.0, 15.0, 5.0, 40.0, 3.0)]);
        p.chimneys.push(square(7.0, 1.0, 0.6, 0.6, height));
        calc(&p)
    };
    let want = with(7.0);
    near("WV", want.line("WV"), 0.6, 0.005);
    near("WH", want.line("WH"), 0.6, 0.005);
    near("WS", want.line("WS"), 1.2 / cos(40.0), 0.005);
    for height in [0.0, 0.005, 0.04, 0.05, 0.1] {
        let roof = with(height);
        assert!(roof.geos[0].error.is_none(), "height {height}");
        assert_eq!(
            roof.lines.keys().collect::<Vec<_>>(),
            want.lines.keys().collect::<Vec<_>>(),
            "height {height}"
        );
        for (code, len) in &want.lines {
            near(
                &format!("height {height}: {code}"),
                roof.line(code),
                *len,
                1e-6,
            );
        }
        near(
            &format!("height {height}: area"),
            roof.area,
            want.area,
            1e-6,
        );
        assert_eq!(roof.line("P"), 0.0, "height {height}: a pent line");
    }
    // A roof opening is still an opening: its sides are roof edges.
    let mut p = job(vec![gable("A", 0.0, 0.0, 15.0, 5.0, 40.0, 3.0)]);
    p.roof_openings.push(square(7.0, 1.0, 0.6, 0.6, 0.0));
    let roof = calc(&p);
    near("opening: P", roof.line("P"), 0.6, 0.005);
    assert_eq!(roof.line("WV"), 0.0);
}

// ------------------------------------------------------------ MATH2-006

/// The T house whose wing front lies in the house's plane (front eave
/// 2.9 m), which Tectora calculates as one face (`MergedPlanes`), or below
/// it (2.0 m: a lean-to wing, `WingJoined`).
fn t_house(front: f64) -> Project {
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
    let mut p = project(
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
    );
    p.parts[0].height = Some(5.0);
    p
}

/// MATH2-006: a dormer on a roof Tectora calculated itself took its
/// footprint off the host face's area without recording it as a
/// deduction, so the area check saw "1.8 % missing" on a right roof and
/// the calculation sheet showed `gross - 0 = area`.
#[test]
fn a_dormer_on_a_departed_roof_is_recorded_as_a_deduction() {
    for (front, notice) in [
        (2.9, WarningKind::MergedPlanes),
        (2.0, WarningKind::WingJoined),
    ] {
        let mut p = t_house(front);
        let mut d = default_params(DormerKind::Gable, Point2::new(11.5, 3.5), 0.0, 6.4);
        d.width = 2.0;
        p.dormers = vec![build_dormer(&d)];
        let roof = calc(&p);
        let g = &roof.geos[0];
        assert!(g.error.is_none(), "front {front}");
        assert_eq!(kinds(&roof, 0), vec![notice], "front {front}");
        let dormer: f64 = g
            .openings
            .iter()
            .filter(|o| o.kind == tectora_core::roof::OpeningKind::Dormer)
            .map(|o| o.area)
            .sum();
        assert!(dormer > 1.0, "front {front}: dormer deduction {dormer}");
        let deducted: f64 = g
            .faces
            .iter()
            .filter(|f| f.edge != usize::MAX)
            .map(|f| f.deducted)
            .sum();
        near(&format!("front {front}: deducted"), deducted, dormer, 1e-6);
    }
}

// ------------------------------------------------------------ MATH2-007

/// MATH2-007: a chimney across the ridge takes its width off the ridge
/// once and has no back flashing (both of its upper sides lie on the
/// ridge), on every roof. On the house with a lean-to wing the two faces
/// come from the join, on its centimetre grid, and the ridge was taken
/// off twice with 1.2 m of back flashing added.
#[test]
fn a_chimney_on_the_ridge_of_a_roof_with_a_joined_wing_takes_the_ridge_off_once() {
    for front in [2.9, 2.0] {
        let bare = calc(&t_house(front));
        let mut p = t_house(front);
        // 0.6 × 0.6 m centred on the ridge (y = 5.0).
        p.chimneys.push(square(12.0, 4.7, 0.6, 0.6, 9.0));
        let roof = calc(&p);
        assert!(roof.geos[0].error.is_none(), "front {front}");
        near(
            &format!("front {front}: ridge"),
            roof.line("F"),
            bare.line("F") - 0.6,
            0.005,
        );
        near(
            &format!("front {front}: WH"),
            roof.line("WH"),
            bare.line("WH"),
            0.005,
        );
        near(
            &format!("front {front}: WV"),
            roof.line("WV"),
            bare.line("WV") + 1.2,
            0.005,
        );
        near(
            &format!("front {front}: WS"),
            roof.line("WS"),
            bare.line("WS") + 4.0 * 0.3 / cos(40.0),
            0.005,
        );
    }
}
