//! Towers and turrets on a roof (`tectora_core::tower`): the case that made
//! them, a church-like turret on the ridge of a hip roof (a real job's
//! drawings: a shaft 4.36 m square with rounded corners, a bell helm, an
//! onion, a lantern, a small onion and a spire), checked against what can
//! be worked out independently of the tower code:
//!
//! - the tower's roof area against a triangulation of the rings it draws,
//! - the hole the shaft cuts into the roof against its plan area over the
//!   cosine of the pitch,
//! - the totals against the host roof alone plus the tower minus the hole.

mod support;

use serde_json::Value;
use support::rect;
use tectora_core::geom::Point2;
use tectora_core::report::calc;
use tectora_core::tower::{TierKind, Tower, TowerShape, TowerTier};

const PITCH: f64 = 40.0;
const EAVE: f64 = 8.0;

/// A hip roof 30 × 12 m, 40°, eave 8 m, no overhang: its ridge runs along
/// y = 6 from x = 6 to x = 24 at 8 + 6·tan 40° = 13.03 m.
fn host() -> tectora_core::Project {
    rect(30.0, 12.0, [(PITCH, EAVE, 0.0); 4])
}

fn tier(rise: f64, top_width: f64, bulge: f64) -> TowerTier {
    TowerTier {
        rise,
        top_width,
        bulge,
        material: 2,
    }
}

/// The turret of the drawings (profile measured from the elevation, 1:50):
/// its roof starts at 17 m on a 4.36 m shaft with 0.8 m corner radius.
fn turret() -> Tower {
    let mut t = Tower::pyramid(Point2::new(15.0, 6.0), 4.36, 17.0, 1.0);
    t.label = "Věž".into();
    t.corner_radius = 0.8;
    t.tiers = vec![
        tier(1.23, 3.0, -0.08), // bell helm, concave
        tier(1.54, 2.95, 0.0),  // steep upper helm
        tier(0.15, 3.2, 0.0),   // cornice under the onion, leaning out
        tier(1.49, 1.5, 0.5),   // big onion
        tier(1.95, 1.5, 0.0),   // lantern (vertical walls)
        tier(0.05, 1.95, 0.0),  // cornice
        tier(1.44, 0.8, 0.25),  // small onion
        tier(1.44, 0.0, -0.1),  // spire, concave
    ];
    t
}

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("not a number: {v}"))
}

/// Area of a band between two rings with matching points, as triangles.
fn band_area(a: &[[f64; 3]], b: &[[f64; 3]]) -> f64 {
    let tri = |p: [f64; 3], q: [f64; 3], r: [f64; 3]| {
        let u = [q[0] - p[0], q[1] - p[1], q[2] - p[2]];
        let v = [r[0] - p[0], r[1] - p[1], r[2] - p[2]];
        let c = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt() / 2.0
    };
    let n = a.len();
    assert_eq!(n, b.len(), "rings of one band have the same points");
    (0..n)
        .map(|i| {
            let j = (i + 1) % n;
            tri(a[i], a[j], b[j]) + tri(a[i], b[j], b[i])
        })
        .sum()
}

#[test]
fn the_turrets_roof_area_matches_the_surface_it_draws() {
    let t = turret();
    assert!(t.problems().is_empty(), "{:?}", t.problems());
    let r = t.calculate().expect("a tower without problems calculates");
    assert!(
        (r.top_height - 17.0 - 9.29).abs() < 1e-9,
        "{}",
        r.top_height
    );
    let mut drawn_roof = 0.0;
    let mut drawn_wall = 0.0;
    for tier in &r.tiers {
        let area: f64 = tier.rings.windows(2).map(|w| band_area(&w[0], &w[1])).sum();
        // The drawn rings are coarser than the exact calculation (16 steps
        // a turn, 12 chords per curved tier): within 3 %.
        let rel = (tier.area - area) / tier.area;
        assert!(
            rel.abs() < 0.03,
            "tier {}: exact {} drawn {area} ({rel})",
            tier.index,
            tier.area
        );
        match tier.kind {
            TierKind::Roof => drawn_roof += area,
            TierKind::Wall => drawn_wall += area,
        }
    }
    // The lantern is the only vertical tier: wall, not roof.
    assert_eq!(
        r.tiers.iter().filter(|t| t.kind == TierKind::Wall).count(),
        1
    );
    assert!((r.wall_area() - drawn_wall).abs() / r.wall_area() < 0.03);
    assert!((r.roof_area() - drawn_roof).abs() / r.roof_area() < 0.03);
    // The lantern is a rounded square 1.5 m wide (corner radius scaled
    // with it) and 1.95 m high.
    let scale = 1.5 / 4.36;
    let side = 1.5 - 2.0 * 0.8 * scale;
    let perimeter = 4.0 * side + 2.0 * std::f64::consts::PI * 0.8 * scale;
    assert!(
        (r.wall_area() - perimeter * 1.95).abs() < 0.01 * perimeter * 1.95,
        "{} vs {}",
        r.wall_area(),
        perimeter * 1.95
    );
    // Rounded corners: no hips anywhere.
    assert!(r.lines.iter().all(|l| l.code != "G"));
}

#[test]
fn the_turret_cuts_its_shaft_out_of_the_ridge_and_adds_its_roof() {
    let alone = calc(&host());
    let mut p = host();
    p.towers.push(turret());
    let with = calc(&p);
    let roof = |v: &Value| f(&v["totals"]["roof_area"]);
    let parts = |v: &Value| -> f64 {
        v["parts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| f(&p["quantities"]["roof_area"]))
            .sum()
    };
    let tower = &with["towers"][0];
    let tower_roof = f(&tower["roof_area"]);
    let t = turret();
    let rr = t.calculate().unwrap();
    assert!((tower_roof - rr.roof_area()).abs() < 1e-9);
    // The shaft stands on both faces of the ridge: its hole is its plan
    // area (a rounded square) over cos 40°, the same on either side.
    let plan = 4.36 * 4.36 - (4.0 - std::f64::consts::PI) * 0.8 * 0.8;
    let hole = plan / PITCH.to_radians().cos();
    let cut = parts(&alone) - parts(&with);
    assert!(
        (cut - hole).abs() < 0.01 * hole,
        "hole {cut} m², expected {hole} m²"
    );
    // Totals: the roof minus the hole plus the tower's roof.
    assert!(
        (roof(&with) - (roof(&alone) - cut + tower_roof)).abs() < 1e-6,
        "{} vs {} - {cut} + {tower_roof}",
        roof(&with),
        roof(&alone)
    );
    // The shaft stands in the roof, its base above the ridge; its walls go
    // from the roof (lowest at the shaft's sides away from the ridge) up to
    // 17 m.
    let shaft = &tower["shaft"];
    assert_eq!(shaft["on_roof"], true);
    assert_eq!(shaft["base_below_roof"], false);
    let ridge = EAVE + 6.0 * PITCH.to_radians().tan();
    let zs: Vec<f64> = shaft["bottom"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| f(&p[2]))
        .collect();
    let lowest = ridge - (2.18 + 0.05) * PITCH.to_radians().tan();
    assert!(
        zs.iter().all(|&z| z <= ridge + 1e-6 && z >= lowest - 1e-6),
        "{zs:?}"
    );
    // Shaft walls: perimeter × (17 − mean roof height around it), the
    // mean being between the lowest point and the ridge.
    let wall = f(&shaft["wall_area"]);
    let perimeter = 4.0 * (4.36 - 1.6) + 2.0 * std::f64::consts::PI * 0.8;
    assert!(
        wall > perimeter * (17.0 - ridge) && wall < perimeter * (17.0 - lowest),
        "{wall}"
    );
}

#[test]
fn a_cone_on_a_round_tower_is_pi_r_times_its_slant() {
    let mut t = Tower::pyramid(Point2::new(0.0, 0.0), 4.0, 10.0, 6.0);
    t.shape = TowerShape::Circle;
    let r = t.calculate().unwrap();
    let exact = std::f64::consts::PI * 2.0 * (2.0f64 * 2.0 + 6.0 * 6.0).sqrt();
    // The circle is a fine polygon, inscribed: a hair under the exact cone.
    assert!(
        (r.roof_area() - exact).abs() / exact < 1e-3,
        "{} vs {exact}",
        r.roof_area()
    );
    let eave = r.lines.iter().find(|l| l.code == "T").unwrap();
    assert!((eave.length - 4.0 * std::f64::consts::PI).abs() < 4e-3 * 4.0);
}

#[test]
fn the_price_sheet_list_has_the_turrets_faces_lines_and_shaft_hole() {
    use tectora_core::export::{ListOptions, list_summary};
    let mut p = host();
    p.towers.push(turret());
    let s = list_summary(&p, &ListOptions::new("t", "28.11.2026 10:00"));
    let totals = calc(&p);
    // The list's faces less their deductions are the roof area of the
    // results, the turret included.
    let net: f64 = s.faces.iter().map(|f| f.area - f.deducted).sum();
    let roof = f(&totals["totals"]["roof_area"]);
    assert!((net - roof).abs() < 0.01, "list {net} vs results {roof}");
    // The turret is roof 2 (after the building's, numbered like dormers'
    // roofs): seven roof tiers, the lantern is no roof face.
    let tower: Vec<_> = s
        .faces
        .iter()
        .filter(|f| f.name.starts_with("2."))
        .collect();
    assert_eq!(tower.len(), 7, "{:?}", s.faces);
    assert!(tower.iter().all(|f| f.pitch <= 90.0));
    // The shaft hole: on the two long faces it straddles.
    let cut: f64 = s.faces.iter().map(|f| f.deducted).sum();
    let plan = 4.36 * 4.36 - (4.0 - std::f64::consts::PI) * 0.8 * 0.8;
    assert!(
        (cut - plan / PITCH.to_radians().cos()).abs() < 0.01 * cut,
        "{cut}"
    );
    // Its lines are the list's too.
    let r = turret().calculate().unwrap();
    let eaves: f64 = r
        .lines
        .iter()
        .filter(|l| l.code == "T")
        .map(|l| l.length)
        .sum();
    assert!(eaves > 0.0);
    let t = s.lines.iter().find(|l| l.code == "T").unwrap().length;
    let alone = list_summary(&host(), &ListOptions::new("t", "28.11.2026 10:00"));
    let t0 = alone.lines.iter().find(|l| l.code == "T").unwrap().length;
    assert!((t - t0 - eaves).abs() < 1e-3, "{t} vs {t0} + {eaves}");
}
