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
    let wall = f(&shaft["wall_area"]);
    let perimeter = 4.0 * (4.36 - 1.6) + 2.0 * std::f64::consts::PI * 0.8;
    assert!(
        wall > perimeter * (17.0 - ridge) && wall < perimeter * (17.0 - lowest),
        "{wall}"
    );
}

/// Audit MATH-008: a square shaft 4 × 4 m on the ridge: the front and back
/// walls stand on the roof 2 m below the ridge's plan line, the sides run
/// over the ridge (mean roof height halfway). Exact, but for the 5 cm
/// the roof is read outside the shaft.
#[test]
fn the_shaft_walls_follow_the_roof_over_the_ridge() {
    let mut p = host();
    let mut t = Tower::pyramid(Point2::new(15.0, 6.0), 4.0, 17.0, 3.0);
    t.label = String::new();
    p.towers.push(t);
    let v = calc(&p);
    let tan = PITCH.to_radians().tan();
    let ridge = EAVE + 6.0 * tan;
    let front = ridge - 2.05 * tan;
    let sides_mean = ridge - 1.025 * tan;
    let exact = 2.0 * 4.0 * (17.0 - front) + 2.0 * 4.0 * (17.0 - sides_mean);
    let wall = f(&v["towers"][0]["shaft"]["wall_area"]);
    assert!((wall - exact).abs() < 0.01 * exact, "{wall} vs {exact}");
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

/// Audit MATH-002/004: the hole of a shaft with a tiny corner radius, a
/// small round shaft or a many-sided one is its true plan area over the
/// cosine of the pitch, and the building keeps the standard calculation.
#[test]
fn every_shaft_is_cut_by_its_true_plan_area() {
    let alone = calc(&host());
    let parts = |v: &Value| -> f64 {
        v["parts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| f(&p["quantities"]["roof_area"]))
            .sum()
    };
    let base = parts(&alone);
    let mut cases: Vec<(String, Tower)> = Vec::new();
    for r in [1e-9, 1e-7, 1e-4, 1e-3, 5e-3, 0.01, 0.02, 0.05, 0.1, 0.5] {
        let mut t = Tower::pyramid(Point2::new(15.0, 3.0), 2.0, 14.0, 1.5);
        t.corner_radius = r;
        cases.push((format!("radius {r}"), t));
    }
    for w in [0.2, 0.3, 0.5, 2.0, 4.0] {
        let mut t = Tower::pyramid(Point2::new(15.0, 3.0), w, 14.0, 1.5);
        t.shape = TowerShape::Circle;
        cases.push((format!("circle {w}"), t));
        let mut p = Tower::pyramid(Point2::new(15.0, 3.0), w, 14.0, 1.5);
        p.shape = TowerShape::Polygon;
        p.sides = 64;
        cases.push((format!("64-gon {w}"), p));
    }
    let cos = PITCH.to_radians().cos();
    for (name, t) in cases {
        assert!(t.problems().is_empty(), "{name}: {:?}", t.problems());
        let plan = t.calculate().unwrap().base_area;
        let mut p = host();
        p.towers.push(t);
        let v = calc(&p);
        let cut = base - parts(&v);
        let hole = plan / cos;
        assert!(
            (cut - hole).abs() < 0.005 * hole + 1e-3,
            "{name}: cut {cut} m², expected {hole} m²"
        );
        let warnings = serde_json::to_string(&v["parts"][0]["geometry"]["warnings"]).unwrap();
        assert!(
            !warnings.contains("not_calculated_by_core"),
            "{name}: {warnings}"
        );
    }
}

/// Audit ARCH-001: towers at the limits give an answer the editor can
/// take, and more tiers than that are refused before calculating.
#[test]
fn towers_at_the_limits_give_a_bounded_answer() {
    let mut t = Tower::pyramid(Point2::new(15.0, 6.0), 3.0, 20.0, 1.0);
    t.shape = TowerShape::Polygon;
    t.sides = 64;
    t.corner_radius = 0.2;
    t.tiers = (0..tectora_core::tower::MAX_TIERS)
        .map(|i| {
            let w = 3.0 - 0.12 * (i + 1) as f64;
            tier(0.5, w, if i % 2 == 0 { 0.05 } else { -0.05 })
        })
        .collect();
    assert!(t.problems().is_empty(), "{:?}", t.problems());
    let mut p = host();
    let n = tectora_core::tower::MAX_TIERS_TOTAL / tectora_core::tower::MAX_TIERS;
    for _ in 0..n {
        p.towers.push(t.clone());
    }
    assert!(p.check_limits().is_ok());
    let size = serde_json::to_string(&calc(&p)).unwrap().len();
    assert!(size < 8_000_000, "{size} bytes");
    p.towers.push(t);
    let e = p.check_limits().unwrap_err();
    assert!(e.contains("tiers of all towers"), "{e}");
}

/// Audit MATH-010: a mirrored building's tower is the mirror image of the
/// tower, also for a shape symmetric about one axis only (a triangle).
#[test]
fn a_mirrored_towers_shaft_is_the_mirror_image() {
    use tectora_core::edit::transform::{PlanMap, transform_part};
    for sides in [3u32, 4, 5, 8] {
        let mut t = Tower::pyramid(Point2::new(15.0, 3.0), 2.0, 14.0, 1.5);
        t.shape = TowerShape::Polygon;
        t.sides = sides;
        t.rotation_deg = 10.0;
        let mut p = host();
        p.towers.push(t.clone());
        let m = PlanMap::Mirror {
            centre: Point2::new(15.0, 6.0),
            deg: 0.0,
        };
        transform_part(&mut p, 0, m, 0);
        let got = p.towers[0].footprint();
        let want: Vec<Point2> = t.footprint().iter().map(|&q| m.apply(q)).collect();
        for w in &want {
            assert!(
                got.iter().any(|g| g.distance(*w) < 1e-3),
                "{sides} sides: {w:?} not in {got:?}"
            );
        }
    }
}

/// Audit MATH-001 / TEST-001: a tower's lines land under their own codes
/// in the job's line totals: eaves T, pitch breaks c (never K, the valley),
/// hips G, Pult lines P.
#[test]
fn the_turrets_lines_are_added_under_their_codes() {
    let r = turret().calculate().unwrap();
    let sum = |code: &str| -> f64 {
        r.lines
            .iter()
            .filter(|l| l.code == code)
            .map(|l| l.length)
            .sum()
    };
    assert!(
        r.lines
            .iter()
            .all(|l| ["T", "c", "P", "G"].contains(&l.code))
    );
    assert!(sum("c") > 0.0 && sum("P") > 0.0);
    let alone = calc(&host());
    let mut p = host();
    p.towers.push(turret());
    let with = calc(&p);
    let code = |v: &Value, c: &str| v["codes"][c].as_f64().unwrap_or(0.0);
    for c in ["T", "c", "P", "G"] {
        assert!(
            (code(&with, c) - code(&alone, c) - sum(c)).abs() < 0.011,
            "{c}: {} vs {} + {}",
            code(&with, c),
            code(&alone, c),
            sum(c)
        );
    }
    assert_eq!(code(&with, "K"), code(&alone, "K"));
}
