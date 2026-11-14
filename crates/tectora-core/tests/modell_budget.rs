//! A second building a few mm off the first and turned by a fraction of a
//! degree made the Modellverschnitt split its pieces without end until the
//! process ran out of memory and aborted (review MATH-004, 2026-11-14): the
//! app or the MCP server died, and the saved job crashed it again on open.
//! The split stage now stops past `modell::split::MAX_POLY_EDGES` and the
//! engine falls back to its own sweep with its usual warning.

use tectora_core::check::WarningKind;
use tectora_core::openings::generate_project;
use tectora_core::{EdgeDef, Project, RoofPart};

fn part(name: &str, corners: &[(f64, f64)]) -> RoofPart {
    RoofPart {
        height: None,
        name: name.into(),
        edges: corners
            .iter()
            .map(|&(x, y)| -> EdgeDef {
                serde_json::from_value(serde_json::json!({
                    "start": { "x": x, "y": y }, "pitch_deg": 40.0,
                    "eave_height": 3.0, "overhang": 0.0,
                }))
                .unwrap()
            })
            .collect(),
    }
}

#[test]
fn a_wing_5_mm_off_and_turned_0_2_degrees_is_calculated_without_running_out_of_memory() {
    let mut p = Project::new("turned wing");
    p.parts.push(part(
        "A",
        &[(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)],
    ));
    p.parts.push(part(
        "B",
        &[
            (10.005, 3.0),
            (18.004951261262324, 3.02792521132179),
            (17.98400735277098, 9.027888657268532),
            (9.98405609150866, 8.999963445946742),
        ],
    ));
    let geos = generate_project(&p);
    assert_eq!(geos.len(), 2);
    for g in &geos {
        assert!(g.error.is_none(), "{:?}", g.error);
        assert!(!g.faces.is_empty());
    }
    let notes: Vec<_> = geos
        .iter()
        .flat_map(|g| &g.warnings)
        .map(|w| w.kind)
        .collect();
    assert!(
        notes.contains(&WarningKind::NotCalculatedByCore),
        "{notes:?}"
    );
}
