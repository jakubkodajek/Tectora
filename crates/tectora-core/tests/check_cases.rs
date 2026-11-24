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
    // Through `from_json`, so the older files migrate as they do in the store.
    Project::from_json(&std::fs::read_to_string(path).unwrap()).unwrap()
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

// ------------------------------------------------ error walls (2026-11-14)

/// A shallow wing between two gables whose plane lies 3 cm under the
/// house's: the calculation stops, Tectora has no sound roof for it.
fn shallow_wing_under_the_house_plane() -> Project {
    let tan25 = 25f64.to_radians().tan();
    let mut p = Project::new("shallow wing");
    p.parts.push(RoofPart::new(
        "A",
        vec![
            edge(5.0, 0.0, 25.0, 12.0 - tan25 * 0.3 - 0.03, 0.2),
            edge(10.0, 0.0, 90.0, 12.0, 0.0),
            edge(10.0, 0.6, 25.0, 12.0, 0.5),
            edge(15.0, 0.6, 90.0, 12.0, 0.0),
            edge(15.0, 5.6, 25.0, 12.0, 0.5),
            edge(0.0, 5.6, 90.0, 12.0, 0.0),
            edge(0.0, 0.6, 25.0, 12.0, 0.5),
            edge(5.0, 0.6, 90.0, 12.0, 0.0),
        ],
    ));
    p
}

/// The error of a building whose parallel planes never meet names the
/// outer and the inner wall, as the walls were entered: also for an
/// outline drawn clockwise (review TEST-006).
#[test]
fn a_failing_part_names_its_parallel_walls_also_drawn_clockwise() {
    let p = shallow_wing_under_the_house_plane();
    let geos = generate_project(&p);
    let e = geos[0].error.as_ref().expect("the calculation stops");
    assert_eq!(e.edges, [0, 2], "{e:?}");

    // The same building drawn the other way round: wall k is wall 7 - k.
    let mut cw = p.clone();
    let part = &mut cw.parts[0];
    let n = part.edges.len();
    let starts: Vec<_> = part.edges.iter().map(|e| e.start).collect();
    let defs = part.edges.clone();
    for k in 0..n {
        // Wall k of the clockwise outline runs from the end of wall
        // n-1-k back to its start.
        part.edges[k] = EdgeDef {
            start: starts[(n - k) % n],
            ..defs[n - 1 - k].clone()
        };
    }
    let geos = generate_project(&cw);
    let e = geos[0].error.as_ref().expect("the calculation stops");
    assert_eq!(e.edges, [7, 5], "{e:?}");
}

/// The walls are named only where parallel planes can be the cause
/// (review ARCH-006, ERR-006): not for a chimney that is not convex, and
/// not where the outer plane lies far below the inner one, where "raise
/// the eave a little" is no advice.
#[test]
fn error_walls_are_named_only_where_parallel_planes_can_be_the_cause() {
    let mut p = shallow_wing_under_the_house_plane();
    p.chimneys.push(tectora_core::Chimney {
        corners: [(2.0, 2.0), (3.0, 2.0), (2.5, 2.3), (3.0, 3.0), (2.0, 3.0)]
            .iter()
            .map(|&(x, y)| tectora_core::geom::Point2::new(x, y))
            .collect(),
        height: 14.0,
    });
    let geos = generate_project(&p);
    let e = geos[0].error.as_ref().expect("no roof");
    assert_eq!(e.kind, tectora_core::roof::CalcErrorKind::NonConvexOpening);
    assert!(e.edges.is_empty(), "{e:?}");

    // Two low wings between gables, their planes 0.9 m under the house's
    // (no single cut closes this roof, so the error stands).
    let mut comb = Project::new("comb");
    comb.parts.push(RoofPart::new(
        "A",
        vec![
            edge(2.5, 2.5, 90.0, 5.0, 0.5),
            edge(2.5, 0.0, 40.0, 2.0, 0.5),
            edge(7.5, 0.0, 90.0, 5.0, 0.5),
            edge(7.5, 2.5, 90.0, 5.0, 0.5),
            edge(12.5, 2.5, 90.0, 5.0, 0.5),
            edge(12.5, 0.0, 40.0, 2.0, 0.5),
            edge(17.5, 0.0, 90.0, 5.0, 0.5),
            edge(17.5, 2.5, 90.0, 5.0, 0.5),
            edge(20.0, 2.5, 90.0, 5.0, 0.5),
            edge(20.0, 7.5, 40.0, 5.0, 0.5),
            edge(0.0, 7.5, 90.0, 5.0, 0.5),
            edge(0.0, 2.5, 40.0, 5.0, 0.5),
        ],
    ));
    let geos = generate_project(&comb);
    let e = geos[0].error.as_ref().expect("no roof");
    assert!(e.edges.is_empty(), "{e:?}");
}

// Audit 2026-11-24 HUNT-1801 (owner decision 2): a hand-drawn face whose
// corners were given out of order (a bow tie) is refused when it is new and
// warned about when a file holds one; its area stays as counted.
mod manual_face_crossing {
    use tectora_core::check::WarningKind;
    use tectora_core::edit::check_manual_face;
    use tectora_core::geom::Point2;
    use tectora_core::model::ManualFace;
    use tectora_core::openings::generate_project;

    fn face(pts: &[(f64, f64)]) -> ManualFace {
        ManualFace {
            points: pts.iter().map(|&(x, y)| Point2::new(x, y)).collect(),
            pitch_deg: 30.0,
            label: String::new(),
        }
    }

    const BOW: [(f64, f64); 4] = [(1.0, 1.0), (6.0, 6.0), (6.0, 1.0), (1.0, 6.0)];
    const IN_ORDER: [(f64, f64); 4] = [(1.0, 1.0), (6.0, 1.0), (6.0, 6.0), (1.0, 6.0)];

    #[test]
    fn a_bow_tie_is_refused_and_the_same_corners_in_order_are_accepted() {
        assert!(face(&BOW).crosses());
        assert_eq!(
            check_manual_face(&face(&BOW)).unwrap_err().0,
            "face_crosses"
        );
        assert!(!face(&IN_ORDER).crosses());
        assert_eq!(check_manual_face(&face(&IN_ORDER)), Ok(()));
        // An uneven bow tie too.
        let uneven = face(&[(1.0, 1.0), (7.0, 6.0), (7.0, 1.0), (1.0, 2.0)]);
        assert_eq!(check_manual_face(&uneven).unwrap_err().0, "face_crosses");
    }

    #[test]
    fn a_face_touching_itself_at_a_corner_or_a_triangle_is_accepted() {
        // Two squares that share the corner (2, 2): no proper crossing.
        let eight = face(&[
            (0.0, 0.0),
            (2.0, 0.0),
            (2.0, 2.0),
            (4.0, 2.0),
            (4.0, 4.0),
            (2.0, 4.0),
            (2.0, 2.0),
            (0.0, 2.0),
        ]);
        assert!(!eight.crosses());
        assert_eq!(check_manual_face(&eight), Ok(()));
        assert_eq!(
            check_manual_face(&face(&[(0.0, 0.0), (3.0, 0.0), (0.0, 3.0)])),
            Ok(())
        );
    }

    /// Audit 2026-11-24 HUNT-1902: a face that touches itself (the bow
    /// tie's crossing point clicked as a corner, also 0.5 mm off), folds an
    /// edge back along another one, or lies on one line is refused like a
    /// crossing one, in the editor (`check_face`) and over MCP; a stored one
    /// is warned about. A double click (a repeated corner in a row) and two
    /// squares meeting corner to corner stay accepted.
    #[test]
    fn a_face_touching_or_folding_over_itself_is_refused_and_warned_about() {
        let touch_bowtie = [
            (0.0, 0.0),
            (2.0, 2.0),
            (4.0, 4.0),
            (4.0, 0.0),
            (2.0, 2.0),
            (0.0, 4.0),
        ];
        let near_bowtie = [
            (0.0, 0.0),
            (2.0, 2.0),
            (4.0, 4.0),
            (4.0, 0.0),
            (2.0005, 2.0),
            (0.0, 4.0),
        ];
        let fold = [
            (0.0, 0.0),
            (4.0, 0.0),
            (4.0, 3.0),
            (2.0, 3.0),
            (2.0, 0.0),
            (0.0, 0.0),
            (0.0, 3.0),
        ];
        let collinear3 = [(0.0, 0.0), (2.0, 0.0), (4.0, 0.0005)];
        let spike = [
            (0.0, 0.0),
            (4.0, 0.0),
            (4.0, 4.0),
            (0.0, 4.0),
            (0.0, 8.0),
            (0.0, 4.0),
        ];
        for (name, pts) in [
            ("touch_bowtie", &touch_bowtie[..]),
            ("near_bowtie", &near_bowtie[..]),
            ("fold", &fold[..]),
            ("collinear3", &collinear3[..]),
            ("spike", &spike[..]),
            ("plain_bowtie", &BOW[..]),
        ] {
            let f = face(pts);
            assert!(f.crosses(), "{name}");
            assert_eq!(
                check_manual_face(&f).unwrap_err().0,
                "face_crosses",
                "{name}"
            );
        }
        for (name, pts) in [
            ("square", &IN_ORDER[..]),
            (
                "double click",
                &[(0.0, 0.0), (4.0, 0.0), (4.0, 0.0), (0.0, 4.0), (0.0, 4.0)][..],
            ),
            (
                "corner on the line",
                &[(0.0, 0.0), (2.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0)][..],
            ),
            (
                "closing click",
                &[(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 0.0)][..],
            ),
        ] {
            let f = face(pts);
            assert!(!f.crosses(), "{name}");
            assert_eq!(check_manual_face(&f), Ok(()), "{name}");
        }
        // Stored in a file: warned about, the area counted as before.
        let mut p = tectora_core::examples::find("gable").unwrap().project();
        p.manual_faces.push(face(&touch_bowtie));
        let warned = generate_project(&p)
            .iter()
            .flat_map(|g| g.warnings.iter())
            .any(|w| w.kind == WarningKind::ManualFaceCrosses && w.face == Some(0));
        assert!(warned);
    }

    /// Audit 2026-11-24 ERR-1901 (and TEST-1906 F03): a crossing face in a
    /// job without a building is warned about at job level in `calc`; with
    /// two buildings, on the one nearest to the face.
    #[test]
    fn a_crossing_face_without_a_building_is_warned_about_at_job_level() {
        let mut p = tectora_core::examples::find("gable").unwrap().project();
        p.parts.clear();
        p.manual_faces.push(face(&IN_ORDER));
        p.manual_faces.push(face(&BOW));
        let geos = generate_project(&p);
        let job = tectora_core::check::job_warnings(&p, &geos);
        assert_eq!(job.len(), 1, "{job:?}");
        assert_eq!(
            (job[0].kind, job[0].face),
            (WarningKind::ManualFaceCrosses, Some(1))
        );
        let report = tectora_core::report::calc(&p);
        assert_eq!(
            report["warnings"][0]["kind"], "manual_face_crosses",
            "{report}"
        );
        assert_eq!(report["warnings"][0]["face"], 1);
        // In order: nothing.
        p.manual_faces.remove(1);
        assert!(tectora_core::report::calc(&p).get("warnings").is_none());

        // Two buildings, the face beside the second: on the second, none
        // at job level.
        let mut two = tectora_core::examples::find("gable").unwrap().project();
        let mut second = two.parts[0].clone();
        for e in &mut second.edges {
            e.start.x += 40.0;
        }
        two.parts.push(second);
        two.manual_faces
            .push(face(&[(41.0, 1.0), (46.0, 6.0), (46.0, 1.0), (41.0, 6.0)]));
        let geos = generate_project(&two);
        let on: Vec<usize> = geos
            .iter()
            .enumerate()
            .filter(|(_, g)| {
                g.warnings
                    .iter()
                    .any(|w| w.kind == WarningKind::ManualFaceCrosses)
            })
            .map(|(i, _)| i)
            .collect();
        assert_eq!(on, vec![1]);
        assert!(tectora_core::check::job_warnings(&two, &geos).is_empty());
        assert!(tectora_core::report::calc(&two).get("warnings").is_none());
    }

    #[test]
    fn a_face_of_fewer_than_three_different_corners_or_a_bad_pitch_is_refused() {
        let flat = face(&[(1.0, 1.0), (1.0, 1.0), (5.0, 1.0), (5.0, 1.002)]);
        assert_eq!(check_manual_face(&flat).unwrap_err().0, "face_corners");
        let mut steep = face(&IN_ORDER);
        steep.pitch_deg = 95.0;
        assert_eq!(check_manual_face(&steep).unwrap_err().0, "pitch_range");
        let nan = face(&[(f64::NAN, 0.0), (1.0, 0.0), (0.0, 1.0)]);
        assert_eq!(check_manual_face(&nan).unwrap_err().0, "face_corners");
    }

    #[test]
    fn a_stored_bow_tie_keeps_its_area_and_is_warned_about_on_the_nearest_building() {
        let mut p = tectora_core::examples::find("gable").unwrap().project();
        p.manual_faces.push(face(&IN_ORDER));
        p.manual_faces
            .push(face(&[(1.0, 1.0), (7.0, 6.0), (7.0, 1.0), (1.0, 2.0)]));
        let geos = generate_project(&p);
        let w: Vec<_> = geos
            .iter()
            .enumerate()
            .flat_map(|(i, g)| g.warnings.iter().map(move |w| (i, w)))
            .filter(|(_, w)| w.kind == WarningKind::ManualFaceCrosses)
            .collect();
        assert_eq!(w.len(), 1, "{w:?}");
        assert_eq!((w[0].0, w[0].1.face, w[0].1.item), (0, Some(1), None));
        let json = serde_json::to_value(w[0].1).unwrap();
        assert_eq!(json["kind"], "manual_face_crosses");
        assert_eq!(json["face"], 1);
        // The area is counted as before (the shoelace of its lobes).
        let report = tectora_core::report::calc(&p);
        let area = report["manual_faces"][1]["area"].as_f64().unwrap();
        assert!(
            (area - 12.0 / 30f64.to_radians().cos()).abs() < 1e-9,
            "{area}"
        );
        // Corners in order: no warning.
        p.manual_faces.remove(1);
        let none = generate_project(&p)
            .iter()
            .flat_map(|g| g.warnings.iter())
            .any(|w| w.kind == WarningKind::ManualFaceCrosses);
        assert!(!none);
    }
}
