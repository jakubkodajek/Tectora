//! Regression tests of the pre-release audit of 2026-11-15 (boards math,
//! math2, math-core, hunt-core, errors). One test per finding, the
//! geometry written out as literals.
//!
//! Where a finding is the standard calculation's own result, the result is
//! kept and the test pins the warning that now says so; the figures the
//! calculation gives are asserted only where they are a closed form.

use tectora_core::check::WarningKind;
use tectora_core::geom::Point2;
use tectora_core::model::{Chimney, PitchBreak};
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
