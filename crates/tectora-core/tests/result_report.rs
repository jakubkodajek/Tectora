//! What the result tells about how it was calculated: which parts the
//! Modellverschnitt joined (ARCH-008) and the dormers it left out because
//! they sit on no roof face (ERR-011).

use tectora_core::check::WarningKind;
use tectora_core::edit::dormers::{DormerKind, build_dormer, default_params};
use tectora_core::geom::Point2;
use tectora_core::openings::generate_project;
use tectora_core::{EdgeDef, Project, RoofPart};

fn edge(x: f64, y: f64, pitch: f64, eave: f64, overhang: f64) -> EdgeDef {
    serde_json::from_value(serde_json::json!({
        "start": { "x": x, "y": y }, "pitch_deg": pitch,
        "eave_height": eave, "overhang": overhang,
    }))
    .unwrap()
}

fn hip(name: &str, x0: f64, y0: f64, x1: f64, y1: f64, eave: f64) -> RoofPart {
    RoofPart {
        height: None,
        name: name.into(),
        edges: vec![
            edge(x0, y0, 35.0, eave, 0.5),
            edge(x1, y0, 35.0, eave, 0.5),
            edge(x1, y1, 35.0, eave, 0.5),
            edge(x0, y1, 35.0, eave, 0.5),
        ],
    }
}

#[test]
fn overlapping_buildings_are_reported_as_joined() {
    let mut p = Project::new("join");
    p.parts.push(hip("A", 0.0, 0.0, 12.0, 8.0, 3.0));
    p.parts.push(hip("B", 10.0, 2.0, 18.0, 6.0, 3.0));
    let geos = generate_project(&p);
    assert_eq!(geos[0].joined_with, [1]);
    assert_eq!(geos[1].joined_with, [0]);
}

#[test]
fn buildings_whose_eaves_only_come_close_are_not_joined() {
    // A gap of 0.8 m with 0.5 m overhangs: the eaves overlap in plan, but
    // the calculation leaves both roofs as they are alone (the plan-distance
    // guess the UI used called them joined).
    let mut p = Project::new("gap");
    p.parts.push(hip("A", 0.0, 0.0, 12.0, 8.0, 3.0));
    p.parts.push(hip("B", 12.8, 1.0, 18.8, 7.0, 3.0));
    let alone = |part: &RoofPart| {
        let mut q = Project::new("alone");
        q.parts.push(part.clone());
        generate_project(&q)[0].quantities().roof_area
    };
    let geos = generate_project(&p);
    let joined_area: f64 = geos.iter().map(|g| g.quantities().roof_area).sum();
    let separate = alone(&p.parts[0]) + alone(&p.parts[1]);
    assert!(
        (joined_area - separate).abs() < 0.01,
        "{joined_area} vs {separate}"
    );
    assert!(
        geos.iter().all(|g| g.joined_with.is_empty()),
        "{:?}",
        geos.iter().map(|g| &g.joined_with).collect::<Vec<_>>()
    );
}

#[test]
fn far_apart_buildings_are_not_joined_and_indices_skip_invalid_parts() {
    let mut p = Project::new("apart");
    p.parts.push(hip("A", 0.0, 0.0, 10.0, 8.0, 3.0));
    // An invalid part in between keeps its slot.
    p.parts.push(RoofPart {
        height: None,
        name: "bad".into(),
        edges: vec![edge(0.0, 0.0, 30.0, 3.0, 0.5)],
    });
    p.parts.push(hip("C", 9.0, 2.0, 16.0, 6.0, 3.0));
    p.parts.push(hip("D", 40.0, 0.0, 50.0, 8.0, 3.0));
    let geos = generate_project(&p);
    assert_eq!(geos[0].joined_with, [2]);
    assert_eq!(geos[2].joined_with, [0]);
    assert!(geos[1].joined_with.is_empty() && geos[3].joined_with.is_empty());
}

fn gable() -> Project {
    let mut p = Project::new("gable");
    p.parts.push(tectora_core::examples::standard_roof(
        tectora_core::examples::StandardRoof::Gable,
        12.0,
        8.0,
        40.0,
        3.0,
        0.5,
    ));
    p
}

#[test]
fn a_dormer_off_the_roof_is_left_out_and_reported() {
    let base = generate_project(&gable());
    let mut p = gable();
    // One dormer on the roof, one 30 m away from it.
    p.dormers.push(build_dormer(&default_params(
        DormerKind::Gable,
        Point2::new(5.0, 1.0),
        0.0,
        5.0,
    )));
    p.dormers.push(build_dormer(&default_params(
        DormerKind::Gable,
        Point2::new(30.0, 30.0),
        0.0,
        4.0,
    )));
    let geos = generate_project(&p);
    let off: Vec<_> = geos[0]
        .warnings
        .iter()
        .filter(|w| w.kind == WarningKind::DormerOffRoof)
        .collect();
    assert_eq!(off.len(), 1, "{:?}", geos[0].warnings);
    assert_eq!(off[0].dormer, Some(1));
    assert!(
        geos[0]
            .warnings
            .iter()
            .all(|w| w.kind != WarningKind::NotCalculatedByCore),
        "the roof stays on the core path"
    );
    // The dormer on the roof is still cut in.
    assert!(geos[0].faces.iter().any(|f| f.edge == usize::MAX));

    // Only the stray dormer: exactly the roof without it, from the core
    // (the fallback engine used to take over and change the last digits).
    let mut only = gable();
    only.dormers.push(p.dormers[1].clone());
    let g = generate_project(&only);
    assert_eq!(g[0].faces, base[0].faces);
    assert_eq!(g[0].lines, base[0].lines);
    assert_eq!(
        g[0].warnings.iter().map(|w| w.kind).collect::<Vec<_>>(),
        [WarningKind::DormerOffRoof]
    );
}

#[test]
fn the_dormer_index_counts_dormers_left_out_as_invalid() {
    let mut p = gable();
    let mut bad = build_dormer(&default_params(
        DormerKind::Gable,
        Point2::new(5.0, 1.0),
        0.0,
        5.0,
    ));
    bad.end.x = f64::NAN;
    p.dormers.push(bad);
    p.dormers.push(build_dormer(&default_params(
        DormerKind::Gable,
        Point2::new(30.0, 30.0),
        0.0,
        4.0,
    )));
    let geos = generate_project(&p);
    let w = geos[0]
        .warnings
        .iter()
        .find(|w| w.kind == WarningKind::DormerOffRoof)
        .expect("reported");
    assert_eq!(w.dormer, Some(1));
}

fn square(x: f64, y: f64, s: f64) -> tectora_core::model::Chimney {
    serde_json::from_value(serde_json::json!({ "corners": [
        { "x": x, "y": y }, { "x": x + s, "y": y }, { "x": x + s, "y": y + s }, { "x": x, "y": y + s }
    ], "height": 9 }))
    .unwrap()
}

fn window(x: f64, y: f64) -> tectora_core::model::RoofWindow {
    serde_json::from_value(serde_json::json!({ "x": x, "y": y, "width": 0.78, "height": 1.18 }))
        .unwrap()
}

#[test]
fn windows_chimneys_and_openings_off_the_roof_are_reported_and_change_nothing() {
    // Re-check of ERR-011: a roof window at (-50, -50) was saved and left
    // out of the result without a word; dormers already said so.
    let mut on = gable();
    on.windows.push(window(3.0, 0.5));
    on.chimneys.push(square(5.0, 5.5, 0.8));
    on.roof_openings.push(square(8.0, 1.0, 0.6));
    let base = generate_project(&on);
    assert!(base[0].warnings.is_empty(), "{:?}", base[0].warnings);

    let mut p = on.clone();
    p.windows.push(window(-50.0, -50.0));
    // An invalid chimney (two corners) is left out before the calculation;
    // the index still counts it.
    let mut bad = square(1.0, 1.0, 1.0);
    bad.corners.truncate(2);
    p.chimneys.push(bad);
    p.chimneys.push(square(40.0, 40.0, 1.0));
    p.roof_openings.insert(0, square(-30.0, 5.0, 1.0));
    let geos = generate_project(&p);
    // The roof is exactly the one without the stray objects.
    assert_eq!(geos[0].faces, base[0].faces);
    assert_eq!(geos[0].lines, base[0].lines);
    let got: Vec<_> = geos[0].warnings.iter().map(|w| (w.kind, w.item)).collect();
    assert_eq!(
        got,
        [
            (WarningKind::WindowOffRoof, Some(1)),
            (WarningKind::ChimneyOffRoof, Some(2)),
            (WarningKind::OpeningOffRoof, Some(0)),
        ]
    );
}

#[test]
fn an_object_off_the_roof_is_reported_on_the_nearest_part() {
    let mut p = Project::new("two");
    p.parts.push(hip("A", 0.0, 0.0, 10.0, 8.0, 3.0));
    p.parts.push(hip("B", 40.0, 0.0, 50.0, 8.0, 3.0));
    p.windows.push(window(55.0, 4.0));
    let geos = generate_project(&p);
    assert!(geos[0].warnings.is_empty(), "{:?}", geos[0].warnings);
    let w: Vec<_> = geos[1].warnings.iter().map(|w| (w.kind, w.item)).collect();
    assert_eq!(w, [(WarningKind::WindowOffRoof, Some(0))]);
}

/// Joined-building regression (2026-11-14, Jakub: "the buildings joined,
/// the roof was not calculated and an error came up"). Drawing the second
/// building, its last corner snapped onto its first one (or a corner was
/// clicked twice): a wall of no length and no direction. The core took the
/// missing direction as east and ended with a cut error, and one core error
/// stops the roof of every building in the job.
#[test]
fn a_wall_of_no_length_leaves_only_its_building_out_and_says_where() {
    let mut p = Project::new("drawn twice");
    p.parts.push(hip("A", 0.0, 0.0, 8.0, 6.0, 3.0));
    let mut b = hip("B", 8.0, 1.0, 12.0, 5.0, 3.0);
    // The closing corner on the first one.
    b.edges.push(edge(8.0, 1.0, 35.0, 3.0, 0.5));
    p.parts.push(b);
    let v = tectora_core::commands::calc(&p).unwrap();
    let problems = v["problems"].as_array().unwrap();
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert_eq!(problems[0]["code"], "zero_length_wall");
    assert_eq!(problems[0]["index"], 1);
    assert_eq!(problems[0]["edge"], 4);
    let a = &v["parts"][0]["geometry"];
    assert!(a["error"].is_null(), "{a}");
    assert!(!a["faces"].as_array().unwrap().is_empty());

    // A corner clicked twice in the middle, the same.
    let mut p2 = Project::new("clicked twice");
    p2.parts.push(hip("A", 0.0, 0.0, 8.0, 6.0, 3.0));
    let mut b = hip("B", 8.0, 1.0, 12.0, 5.0, 3.0);
    b.edges.insert(2, edge(12.0, 1.0, 35.0, 3.0, 0.5));
    p2.parts.push(b);
    let v = tectora_core::commands::calc(&p2).unwrap();
    assert_eq!(v["problems"][0]["code"], "zero_length_wall");
    assert_eq!(v["problems"][0]["edge"], 1);
    assert!(v["parts"][0]["geometry"]["error"].is_null());

    // A wall of a few mm (a typed 3,006 for 3, a corner snapped next to
    // another) is a point for the core too and ended the same way
    // (review ERR-001): left out with its building only.
    for jog in [0.0011, 0.004, 0.0099] {
        let mut p4 = Project::new("short wall");
        p4.parts.push(hip("A", 0.0, 0.0, 8.0, 6.0, 3.0));
        let mut b = hip("B", 8.0, 1.0, 12.0, 5.0, 3.0);
        b.edges.insert(2, edge(12.0, 1.0 + jog, 35.0, 3.0, 0.5));
        p4.parts.push(b);
        let v = tectora_core::commands::calc(&p4).unwrap();
        assert_eq!(v["problems"][0]["code"], "zero_length_wall", "jog {jog}");
        assert!(v["parts"][0]["geometry"]["error"].is_null(), "jog {jog}");
        assert!(
            !v["parts"][0]["geometry"]["faces"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }

    // Direction markers (zero length with a direction) are DACAD walls.
    let mut p3 = Project::new("marker");
    let mut a = hip("A", 0.0, 0.0, 8.0, 6.0, 3.0);
    let mut m = edge(8.0, 0.0, 35.0, 3.0, 0.5);
    m.direction_rad = Some(std::f64::consts::FRAC_PI_2);
    a.edges.insert(1, m);
    p3.parts.push(a);
    assert!(p3.problems().is_empty(), "{:?}", p3.problems());
}

/// When the core ends with an error, the error names the buildings whose
/// roof fails on its own too, so the user knows which one to fix, and the
/// other buildings are calculated without them (DACAD stops every roof of
/// the job instead; the user asked for this, 2026-11-14). Found by
/// `examples/join_fuzz.rs`.
#[test]
fn a_building_the_core_fails_on_alone_leaves_the_others_their_roofs() {
    let mut p = Project::new("one bad roof");
    p.parts.push(RoofPart {
        height: None,
        name: "A".into(),
        edges: vec![
            edge(0.0, 0.0, 30.0, 3.0, 0.6),
            edge(6.2, 0.0, 90.0, 3.0, 0.6),
            edge(6.2, 5.9, 30.0, 3.0, 0.6),
            edge(0.0, 5.9, 90.0, 3.0, 0.6),
        ],
    });
    let mut hh = edge(6.0, 12.8, 90.0, 3.0, 0.5);
    hh.half_hip = Some(tectora_core::model::HalfHip {
        height: 5.0,
        pitch_deg: 50.0,
    });
    p.parts.push(RoofPart {
        height: None,
        name: "B".into(),
        edges: vec![
            edge(2.2, 5.9, 45.0, 3.0, 0.0),
            edge(6.0, 5.9, 30.0, 2.5, 0.0),
            hh,
            edge(2.2, 12.8, 90.0, 3.5, 0.5),
        ],
    });
    let geos = generate_project(&p);
    let e = geos[1]
        .error
        .as_ref()
        .expect("building B ends with an error");
    assert_eq!(e.parts, [1], "{e:?}");
    // Building A keeps the roof it has on its own.
    let mut a = Project::new("A");
    a.parts.push(p.parts[0].clone());
    let alone = generate_project(&a);
    assert!(alone[0].error.is_none());
    assert!(geos[0].error.is_none(), "{:?}", geos[0].error);
    assert_eq!(geos[0].faces.len(), alone[0].faces.len());
    let area = |g: &tectora_core::roof::RoofGeometry| g.faces.iter().map(|f| f.area).sum::<f64>();
    assert!((area(&geos[0]) - area(&alone[0])).abs() < 1e-9);
}
