//! The plausibility checks (`tectora_core::check`) on roofs DACAD itself
//! calculates wrongly and on sound roofs.
//!
//! The faulty roofs in `tests/fixtures/check` were found by the property
//! tests and the census (`tests/invariants.rs`) and run in the DACAD core
//! emulator: DACAD's own result is byte-identical to the engine's, so the
//! engine keeps it and only warns (docs/core-verification.md, findings 1-3).

use tectora_core::check::WarningKind;
use tectora_core::openings::generate_project;
use tectora_core::{EdgeDef, Project, RoofPart};

fn fixture(name: &str) -> Project {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/check")
        .join(format!("{name}.json"));
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn kinds(p: &Project) -> Vec<WarningKind> {
    generate_project(p)
        .iter()
        .flat_map(|g| g.warnings.iter().map(|w| w.kind))
        .collect()
}

#[test]
fn hips_outside_the_building() {
    // 60.35 m2 of faces on a 35.2 m2 roof: hips run 3.7 m out of the walls.
    let geos = generate_project(&fixture("C1_hips_outside"));
    let w = &geos[0].warnings;
    assert_eq!(
        w.iter().map(|w| w.kind).collect::<Vec<_>>(),
        [WarningKind::OutsideOutline, WarningKind::OverlappingFaces]
    );
    assert!((w[0].value - 3.687).abs() < 1e-3, "{w:?}");
}

#[test]
fn reported_area_differs_from_the_faces() {
    // DACAD books one face 8 % too big; the roof is 1.2 % over its faces.
    let geos = generate_project(&fixture("C2_face_area_vs_polygon"));
    let w = &geos[0].warnings;
    assert_eq!(w.len(), 1, "{w:?}");
    assert_eq!(w[0].kind, WarningKind::AreaMismatch);
    assert!((w[0].value - 0.0121).abs() < 2e-4, "{w:?}");
}

#[test]
fn faces_passing_through_each_other() {
    assert_eq!(
        kinds(&fixture("C3_faces_overlap")),
        [WarningKind::OverlappingFaces]
    );
    // DACAD's mixed-overhang model folds two faces through each other; with
    // the walls moved out to where the overhangs end the roof is sound.
    assert_eq!(
        kinds(&fixture("C4_mixed_overhang_model")),
        [WarningKind::OverlappingFaces]
    );
    assert_eq!(kinds(&fixture("C4_mixed_overhang_model_walls_moved")), []);
}

#[test]
fn built_in_examples_are_sound() {
    for e in tectora_core::examples::EXAMPLES {
        assert_eq!(kinds(&e.project()), [], "example {}", e.id);
    }
}

fn edge(x: f64, y: f64, pitch: f64, eave: f64, overhang: f64) -> EdgeDef {
    serde_json::from_value(serde_json::json!({
        "start": { "x": x, "y": y }, "pitch_deg": pitch,
        "eave_height": eave, "overhang": overhang,
    }))
    .unwrap()
}

#[test]
fn stepped_overhang_is_sound() {
    // A long eave whose middle part reaches further out, entered as walls
    // in line with zero-length gable walls between them.
    let mut part = RoofPart {
        height: None,
        name: "A".into(),
        edges: vec![
            edge(0.0, 0.0, 30.0, 3.3, 0.5),
            edge(3.0, 0.0, 90.0, 0.0, 0.0),
            edge(3.0, 0.0, 30.0, 3.3 - 30f64.to_radians().tan(), 1.5),
            edge(9.0, 0.0, 90.0, 0.0, 0.0),
            edge(9.0, 0.0, 30.0, 3.3, 0.5),
            edge(12.0, 0.0, 90.0, 3.3, 0.5),
            edge(12.0, 8.0, 30.0, 3.3, 0.5),
            edge(0.0, 8.0, 90.0, 3.3, 0.5),
        ],
    };
    part.edges[1].direction_rad = Some(-std::f64::consts::FRAC_PI_2);
    part.edges[3].direction_rad = Some(std::f64::consts::FRAC_PI_2);
    let mut p = Project::new("step");
    p.parts.push(part);
    let geos = generate_project(&p);
    assert!(
        geos[0].error.is_none() && !geos[0].faces.is_empty(),
        "{:?}",
        geos[0].error
    );
    let miny = geos[0]
        .faces
        .iter()
        .flat_map(|f| &f.polygon)
        .map(|v| v[1])
        .fold(f64::MAX, f64::min);
    assert!(
        (miny + 1.5).abs() < 1e-3,
        "the middle eave reaches 1.5 m out: {miny}"
    );
    assert_eq!(kinds(&p), []);
}

#[test]
fn joined_roofs_may_run_on_to_the_other_building() {
    // A lower wing against a higher house: its roof runs on to the house
    // and under the house's overhang.
    let rect = |x0: f64, y0: f64, x1: f64, y1: f64, eave: f64| RoofPart {
        height: None,
        name: "R".into(),
        edges: vec![
            edge(x0, y0, 40.0, eave, 0.5),
            edge(x1, y0, 40.0, eave, 0.5),
            edge(x1, y1, 40.0, eave, 0.5),
            edge(x0, y1, 40.0, eave, 0.5),
        ],
    };
    let mut p = Project::new("wing");
    p.parts.push(rect(0.0, 0.0, 10.0, 8.0, 5.0));
    p.parts.push(rect(10.0, 2.0, 16.0, 6.0, 3.0));
    let geos = generate_project(&p);
    assert!(
        geos.iter()
            .all(|g| g.error.is_none() && !g.faces.is_empty())
    );
    assert_eq!(kinds(&p), []);
}

#[test]
fn a_building_inside_another_is_reported_as_covered() {
    // A small building drawn inside a larger one keeps no roof face after
    // the join, which used to show the walls with no roof and no message.
    let rect = |x0: f64, y0: f64, x1: f64, y1: f64| RoofPart {
        height: None,
        name: "R".into(),
        edges: vec![
            edge(x0, y0, 40.0, 3.0, 0.5),
            edge(x1, y0, 40.0, 3.0, 0.5),
            edge(x1, y1, 40.0, 3.0, 0.5),
            edge(x0, y1, 40.0, 3.0, 0.5),
        ],
    };
    let mut p = Project::new("inside");
    p.parts.push(rect(0.0, 0.0, 12.0, 8.0));
    p.parts.push(rect(4.0, 2.0, 7.0, 5.0));
    let geos = generate_project(&p);
    assert!(
        geos[1].error.is_none() && geos[1].faces.is_empty(),
        "{:?}",
        geos[1].faces.len()
    );
    assert!(!geos[0].faces.is_empty());
    assert!(
        geos[0]
            .warnings
            .iter()
            .all(|w| w.kind != WarningKind::Covered)
    );
    assert_eq!(
        geos[1].warnings.iter().map(|w| w.kind).collect::<Vec<_>>(),
        [WarningKind::Covered]
    );
}

/// A 10 × 8 m hip roof whose front eave is entered as two walls in line,
/// the second with its own pitch and pitch break.
fn split_front(second_pitch: f64, second_eave: f64, break_on_second: bool) -> Project {
    let mut second = edge(5.0, 0.0, second_pitch, second_eave, 0.5);
    if break_on_second {
        second.breaks = vec![tectora_core::model::PitchBreak {
            height: 4.5,
            pitch_deg: 20.0,
        }];
    }
    let mut p = Project::new("split front");
    p.parts.push(RoofPart {
        height: None,
        name: "A".into(),
        edges: vec![
            edge(0.0, 0.0, 40.0, 3.0, 0.5),
            second,
            edge(10.0, 0.0, 40.0, 3.0, 0.5),
            edge(10.0, 8.0, 40.0, 3.0, 0.5),
            edge(0.0, 8.0, 40.0, 3.0, 0.5),
        ],
    });
    p
}

#[test]
fn walls_in_line_with_the_same_eave_are_one_wall() {
    // The calculation joins walls in line with the same eave height and
    // overhang before it looks at their pitches (byte-identical with the
    // DACAD core emulator): the second wall's 25° and its pitch break are
    // not used, the whole front is 40°. The roof stays as calculated and
    // carries a warning naming both walls.
    for p in [split_front(25.0, 3.0, false), split_front(40.0, 3.0, true)] {
        let geos = generate_project(&p);
        let g = &geos[0];
        assert!(g.error.is_none());
        assert!(g.faces.iter().all(|f| f.pitch_deg == 40.0), "{:?}", g.faces);
        let w: Vec<_> = g
            .warnings
            .iter()
            .filter(|w| w.kind == WarningKind::MergedWalls)
            .collect();
        assert_eq!(w.len(), 1, "{:?}", g.warnings);
        assert_eq!(w[0].edges, [0, 1]);
    }
    // Same definitions: nothing is lost, no warning.
    assert_eq!(kinds(&split_front(40.0, 3.0, false)), []);
}

#[test]
fn merged_walls_name_the_edges_as_entered() {
    // The same roof entered clockwise: the warning names the edges in the
    // order the user entered them.
    let mut p = split_front(25.0, 3.0, false);
    let part = &mut p.parts[0];
    let n = part.edges.len();
    let starts: Vec<_> = part.edges.iter().map(|e| e.start).collect();
    let mut cw = Vec::new();
    for i in (0..n).rev() {
        let mut e = part.edges[i].clone();
        e.start = starts[(i + 1) % n];
        cw.push(e);
    }
    part.edges = cw;
    let geos = generate_project(&p);
    let w = &geos[0].warnings;
    assert_eq!(w.len(), 1, "{w:?}");
    // Entered clockwise, edge 4 is the 40° wall (5,0)-(0,0), which is
    // kept, and edge 3 the 25° wall (10,0)-(5,0), which loses its pitch.
    assert_eq!(w[0].edges, [4, 3]);
    let e = &p.parts[0].edges;
    assert_eq!(e[3].pitch_deg, 25.0);
    assert!(geos[0].faces.iter().all(|f| f.pitch_deg == 40.0));
    assert!(geos[0].faces.iter().any(|f| f.edge == 4));
}

/// A 12 × 8 m gable roof, 40°, eave 3 m, no overhang.
fn plain_gable() -> Project {
    let mut p = Project::new("gable");
    p.parts.push(RoofPart {
        height: None,
        name: "A".into(),
        edges: vec![
            edge(0.0, 0.0, 40.0, 3.0, 0.0),
            edge(12.0, 0.0, 90.0, 3.0, 0.0),
            edge(12.0, 8.0, 40.0, 3.0, 0.0),
            edge(0.0, 8.0, 90.0, 3.0, 0.0),
        ],
    });
    p
}

#[test]
fn a_window_larger_than_its_face_is_reported() {
    // MATH-001: a 10 × 9 m window on a face of 12 × 5.22 m. DACAD's
    // booking (width × height) is kept, so the face goes negative and the
    // roof total drops; the roofer must be told.
    let mut p = plain_gable();
    p.windows.push(tectora_core::model::RoofWindow {
        x: 1.0,
        y: 0.0,
        width: 10.0,
        height: 9.0,
    });
    let geos = generate_project(&p);
    assert!(geos[0].faces.iter().any(|f| f.area < 0.0));
    let w: Vec<_> = geos[0]
        .warnings
        .iter()
        .filter(|w| w.kind == WarningKind::CutExceedsFace)
        .collect();
    assert_eq!(w.len(), 1, "{:?}", geos[0].warnings);
    assert_eq!(w[0].edges, [0]);
    let missing: f64 = geos[0].faces.iter().map(|f| (-f.area).max(0.0)).sum();
    assert!(
        (w[0].value - missing).abs() < 1e-9 && missing > 20.0,
        "{}",
        w[0].value
    );
}

#[test]
fn a_window_that_fits_its_face_is_no_warning() {
    let mut p = plain_gable();
    p.windows.push(tectora_core::model::RoofWindow {
        x: 2.0,
        y: 0.0,
        width: 1.2,
        height: 1.4,
    });
    assert_eq!(kinds(&p), []);
}

#[test]
fn joined_rows_whose_cut_exceeds_a_face_are_reported() {
    // MATH-002: 8 × 2 hip roofs 10 × 8 m on a 9 × 7 m grid, each
    // overlapping its neighbours by 1 m. The join's cut grows along the
    // first row until a face's area is negative (as booked from the
    // Modellverschnitt; whether DACAD books the same needs a DACAD run).
    let mut p = Project::new("rows");
    for row in 0..2 {
        for col in 0..8 {
            let (x0, y0) = (col as f64 * 9.0, row as f64 * 7.0);
            p.parts.push(RoofPart {
                height: None,
                name: format!("R{row}{col}"),
                edges: vec![
                    edge(x0, y0, 35.0, 3.0, 0.5),
                    edge(x0 + 10.0, y0, 35.0, 3.0, 0.5),
                    edge(x0 + 10.0, y0 + 8.0, 35.0, 3.0, 0.5),
                    edge(x0, y0 + 8.0, 35.0, 3.0, 0.5),
                ],
            });
        }
    }
    let geos = generate_project(&p);
    let negative = |g: &tectora_core::roof::RoofGeometry| {
        g.faces
            .iter()
            .any(|f| f.area < -tectora_core::check::OVERCUT_TOL)
    };
    assert!(
        geos.iter().any(negative),
        "the case still has a negative face"
    );
    for (i, g) in geos.iter().enumerate() {
        let flagged = g
            .warnings
            .iter()
            .any(|w| w.kind == WarningKind::CutExceedsFace);
        assert_eq!(flagged, negative(g), "part {i}: {:?}", g.warnings);
    }
}
