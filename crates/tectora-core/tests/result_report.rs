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
