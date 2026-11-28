//! Towers, audit round 2 (2026-11-28-towers-r2-c5514aa): the shaft's cut
//! in the roof, the tower's own lines and what the editor operations do
//! with towers. Checked against closed forms; towers have no DACAD
//! counterpart (ADR 0008).

mod support;

use serde_json::Value;
use support::rect;
use tectora_core::edit::towers::{TOWER_PRESETS, TowerPreset, preset};
use tectora_core::geom::Point2;
use tectora_core::report::calc;
use tectora_core::tower::{MIN_SHAFT_EDGE, Tower, TowerShape, TowerTier};

const PITCH: f64 = 40.0;
const EAVE: f64 = 8.0;

/// A hip roof 30 × 12 m, 40°, eave 8 m, no overhang; (15, 3) lies on its
/// front face, which falls towards -y.
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

fn code(v: &Value, c: &str) -> f64 {
    v["codes"][c].as_f64().unwrap_or(0.0)
}

/// Audit r2 MATH-001: a square shaft with rounded corners on one face has
/// its front (WV) and back (WH) abutment, each as long as the straight
/// side, as the sharp square has; the dialog's baroque preset included.
#[test]
fn a_rounded_shaft_on_a_face_keeps_its_front_and_back_abutments() {
    let mut cases: Vec<(String, Tower, f64)> = Vec::new();
    for w in [0.8, 1.0, 1.2, 1.5, 3.0, 4.4] {
        let t = preset(TowerPreset::Baroque, Point2::new(15.0, 3.0), w, 20.0);
        let side = w - 2.0 * t.corner_radius;
        cases.push((format!("baroque {w}"), t, side));
    }
    for r in [0.0, 0.01, 0.02, 0.03, 0.05, 0.08, 0.2, 0.5, 0.8] {
        let mut t = Tower::pyramid(Point2::new(15.0, 3.0), 2.0, 20.0, 1.5);
        t.corner_radius = r;
        // A corner whose whole arc is under the shortest edge of the cut
        // stays a sharp corner of it: the side is then the full width.
        let side = if r * 2f64.sqrt() < MIN_SHAFT_EDGE {
            2.0
        } else {
            2.0 - 2.0 * r
        };
        cases.push((format!("square 2, radius {r}"), t, side));
    }
    for (name, t, side) in cases {
        let mut p = host();
        p.towers.push(t);
        let v = calc(&p);
        let (wv, wh) = (code(&v, "WV"), code(&v, "WH"));
        assert!(
            (wv - side).abs() < 0.03 * side + 0.011 && (wh - side).abs() < 0.03 * side + 0.011,
            "{name}: WV {wv}, WH {wh}, straight side {side}, codes {}",
            v["codes"]
        );
    }
}

/// Audit r2 MATH-001: for every preset at the usual sizes the cut keeps
/// the tower's own symmetry (mirror image about both of its axes for the
/// shapes that have it), so its straight sides keep their direction.
#[test]
fn every_presets_cut_is_mirror_symmetric_to_the_towers_axes() {
    for kind in TOWER_PRESETS {
        for w in [0.8, 1.0, 1.2, 2.0, 3.0, 4.4, 12.0] {
            let mut t = preset(kind, Point2::new(0.0, 0.0), w, 20.0);
            t.rotation_deg = 0.0;
            let fp = t.footprint();
            assert!(fp.len() >= 4, "{kind:?} {w}: {fp:?}");
            for q in &fp {
                for m in [Point2::new(-q.x, q.y), Point2::new(q.x, -q.y)] {
                    assert!(
                        fp.iter().any(|p| p.distance(m) < 1e-6),
                        "{kind:?} {w}: {q:?} has no mirror image {m:?} in {fp:?}"
                    );
                }
            }
        }
    }
}

/// Audit r2 MATH-011: the smallest shafts are cut as a polygon of a few
/// corners of their own area, never a sliver or corners 1 cm apart, and
/// the roof keeps the standard calculation.
#[test]
fn a_tiny_shaft_is_cut_as_a_small_regular_polygon() {
    let alone = calc(&host());
    let parts = |v: &Value| -> f64 {
        v["parts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["quantities"]["roof_area"].as_f64().unwrap())
            .sum()
    };
    let mut cases: Vec<(String, Tower)> = Vec::new();
    for w in [0.1, 0.105, 0.12, 0.15, 0.2] {
        let mut c = Tower::pyramid(Point2::new(15.0, 3.0), w, 14.0, 0.5);
        c.shape = TowerShape::Circle;
        cases.push((format!("circle {w}"), c));
        let mut g = Tower::pyramid(Point2::new(15.0, 3.0), w, 14.0, 0.5);
        g.shape = TowerShape::Polygon;
        g.sides = 64;
        cases.push((format!("64-gon {w}"), g));
    }
    let mut sq = Tower::pyramid(Point2::new(15.0, 3.0), 0.1, 14.0, 0.5);
    sq.corner_radius = 0.05;
    cases.push(("square 0.1, radius 0.05".into(), sq));
    for (name, t) in cases {
        assert!(t.problems().is_empty(), "{name}: {:?}", t.problems());
        let fp = t.footprint();
        let n = fp.len();
        assert!(n >= 4, "{name}: {n} corners");
        let shortest = (0..n)
            .map(|i| fp[i].distance(fp[(i + 1) % n]))
            .fold(f64::MAX, f64::min);
        // At least the minimum edge, or the square of the shaft's area.
        assert!(
            shortest >= MIN_SHAFT_EDGE - 1e-9 || n == 4,
            "{name}: an edge of {shortest} m"
        );
        let plan = t.calculate().unwrap().base_area;
        let mut p = host();
        p.towers.push(t);
        let v = calc(&p);
        let cut = parts(&alone) - parts(&v);
        let hole = plan / PITCH.to_radians().cos();
        assert!((cut - hole).abs() < 0.002, "{name}: cut {cut}, hole {hole}");
        let warnings = serde_json::to_string(&v["parts"][0]["geometry"]["warnings"]).unwrap();
        assert!(!warnings.contains("not_calculated_by_core"), "{name}");
        // The abutment is about the shaft's perimeter (an ellipse on the
        // 40° face), not three times it.
        let abut = code(&v, "WV") + code(&v, "WH") + code(&v, "WS");
        let perimeter = t_perimeter_on_face(plan);
        assert!(
            abut > 0.0 && abut < 1.35 * perimeter,
            "{name}: abutment {abut}, about {perimeter}"
        );
    }
}

/// The perimeter of a circle of plan area `plan` on the 40° face, as an
/// ellipse (Ramanujan): the upper bound of a fair cut's abutment.
fn t_perimeter_on_face(plan: f64) -> f64 {
    let r = (plan / std::f64::consts::PI).sqrt();
    let (a, b) = (r / PITCH.to_radians().cos(), r);
    let h = ((a - b) / (a + b)).powi(2);
    std::f64::consts::PI * (a + b) * (1.0 + 3.0 * h / (10.0 + (4.0 - 3.0 * h).sqrt()))
}

/// The tower's line lengths by code.
fn lines_of(t: &Tower) -> std::collections::BTreeMap<&'static str, f64> {
    let mut out = std::collections::BTreeMap::new();
    for l in t.calculate().unwrap().lines {
        *out.entry(l.code).or_insert(0.0) += l.length;
    }
    out
}

fn square2(tiers: Vec<TowerTier>) -> Tower {
    let mut t = Tower::pyramid(Point2::new(0.0, 0.0), 2.0, 10.0, 1.0);
    t.tiers = tiers;
    t
}

/// Audit r2 MATH-007: a flat top has its edge (an eave, T) whether it lies
/// on the shaft or on a vertical tier (a lantern or plain turret with a
/// flat sheet-metal top), as a roof tier above a wall starts with one.
#[test]
fn a_flat_top_over_a_vertical_tier_has_its_eave() {
    let on_shaft = lines_of(&square2(vec![tier(0.0, 2.0, 0.0)]));
    assert_eq!(on_shaft.get("T").copied(), Some(8.0), "{on_shaft:?}");
    let wall_flat = lines_of(&square2(vec![tier(1.0, 2.0, 0.0)]));
    assert_eq!(wall_flat.get("T").copied(), Some(8.0), "{wall_flat:?}");
    // A frustum 2 → 1 m, then a 1 m wall, then the flat top: the eave at
    // the bottom (8 m), the Pult against the wall (4 m) and the top's
    // edge (4 m) as an eave.
    let roof_wall_flat = lines_of(&square2(vec![tier(1.0, 1.0, 0.0), tier(1.0, 1.0, 0.0)]));
    assert!(
        (roof_wall_flat["T"] - 12.0).abs() < 1e-9 && (roof_wall_flat["P"] - 4.0).abs() < 1e-9,
        "{roof_wall_flat:?}"
    );
    assert!(!roof_wall_flat.contains_key("c"), "{roof_wall_flat:?}");
}

/// Audit r2 MATH-010: a tier of no size (no rise, the width of the tier
/// below) adds no line: the tower has the lines it has without it.
#[test]
fn a_tier_of_no_size_adds_no_line() {
    let plain = lines_of(&square2(vec![tier(3.0, 0.0, 0.0)]));
    let with_empty = lines_of(&square2(vec![tier(0.0, 2.0, 0.0), tier(3.0, 0.0, 0.0)]));
    assert_eq!(
        plain.keys().collect::<Vec<_>>(),
        with_empty.keys().collect::<Vec<_>>()
    );
    for (k, v) in &plain {
        assert!(
            (with_empty[k] - v).abs() < 1e-9,
            "{k}: {with_empty:?} vs {plain:?}"
        );
    }
    let mid_empty = lines_of(&square2(vec![
        tier(1.0, 1.0, 0.0),
        tier(0.0, 1.0, 0.0),
        tier(1.0, 0.0, 0.0),
    ]));
    let mid_plain = lines_of(&square2(vec![tier(1.0, 1.0, 0.0), tier(1.0, 0.0, 0.0)]));
    assert_eq!(mid_empty, mid_plain);
}

/// Audit r2 MATH-009: a square shaft 4 × 4 m on the ridge of the 40° hip
/// roof, base 17 m: its front and back walls stand on the roof 2 m off the
/// ridge line, its sides run over the ridge (the mean of both ends'
/// heights): 2 · 4 · (17 − 11.3564) + 2 · 4 · (17 − 12.1955) = 83.585 m².
#[test]
fn the_shaft_walls_over_a_ridge_match_the_closed_form() {
    let mut p = host();
    p.towers
        .push(Tower::pyramid(Point2::new(15.0, 6.0), 4.0, 17.0, 3.0));
    let v = calc(&p);
    let tan = PITCH.to_radians().tan();
    let ridge = EAVE + 6.0 * tan;
    let front = EAVE + 4.0 * tan;
    let exact = 2.0 * 4.0 * (17.0 - front) + 2.0 * 4.0 * (17.0 - (front + ridge) / 2.0);
    let wall = v["towers"][0]["shaft"]["wall_area"].as_f64().unwrap();
    assert!((wall - exact).abs() < 0.01, "{wall} vs {exact}");
}

/// Audit r2 ERR-003: a turret whose axis stands just outside the walls of
/// a building whose roof failed, its shaft in the building, is not "beside
/// the roofs, measured from the ground": the failed roof is said, and its
/// cladding has no figure, as for a turret with its axis inside.
#[test]
fn a_turret_on_the_wall_line_of_a_failed_building_says_the_roof_failed() {
    // A hip roof 12 × 8 with a 5 mm wall: the building is left out.
    let pts = [
        (0.0, 0.0),
        (12.0, 0.0),
        (12.005, 0.0),
        (12.0, 8.0),
        (0.0, 8.0),
    ];
    let failed = support::uniform(&pts, 30.0, 3.0, 0.5);
    let good = support::uniform(
        &[(0.0, 0.0), (12.0, 0.0), (12.0, 8.0), (0.0, 8.0)],
        30.0,
        3.0,
        0.5,
    );
    for (name, centre) in [
        ("on the wall, axis outside", Point2::new(6.0, -0.4)),
        ("over the corner, axis outside", Point2::new(-0.3, -0.3)),
    ] {
        let t = Tower::pyramid(centre, 2.0, 9.0, 3.0);
        let mut p = good.clone();
        p.towers.push(t.clone());
        let v = calc(&p);
        assert_eq!(v["towers"][0]["shaft"]["on_roof"], true, "{name}");
        let mut p = failed.clone();
        p.towers.push(t);
        let v = calc(&p);
        let shaft = &v["towers"][0]["shaft"];
        assert_eq!(shaft["roof_failed"], true, "{name}: {shaft}");
        assert_eq!(shaft["wall_area"].as_f64(), Some(0.0), "{name}: {shaft}");
    }
}

/// Audit r2 MATH-008 (round 1 MATH-012): a round shaft standing where a
/// lower wing joins the house leaves no plan "without a roof": its hole is
/// the shaft's, not a face the join lost. A chimney of the same shape
/// neither.
#[test]
fn a_shaft_at_the_join_of_two_buildings_is_no_uncovered_plan() {
    let mut p = host();
    let wing = support::uniform(
        &[(11.0, -8.0), (19.0, -8.0), (19.0, 2.0), (11.0, 2.0)],
        40.0,
        5.0,
        0.0,
    );
    p.parts.push(wing.parts[0].clone());
    let mut t = Tower::pyramid(Point2::new(15.0, 0.0), 2.0, 16.0, 3.0);
    t.shape = TowerShape::Circle;
    let mut with_tower = p.clone();
    with_tower.towers.push(t.clone());
    let mut with_chimney = p.clone();
    with_chimney.chimneys.push(tectora_core::model::Chimney {
        corners: t.footprint(),
        height: 16.0,
    });
    for (name, q) in [("tower", with_tower), ("chimney", with_chimney)] {
        let v = calc(&q);
        let w = serde_json::to_string(&v["parts"]).unwrap();
        assert!(!w.contains("uncovered_plan"), "{name}: {w}");
    }
}

/// Audit r2 ARCH-003: a job at the towers' limits (64-sided shafts, every
/// tier curved, as many tiers as a job may have) answered `calc` with
/// 12 MB and froze the editor; it now stays under 1.5 MB (most of it one
/// line per hip, 64 a tier, which the hip count is taken from).
#[test]
fn a_job_at_the_tower_limits_answers_calc_in_a_bounded_size() {
    use tectora_core::tower::{MAX_TIERS, MAX_TIERS_TOTAL};
    let mut t = Tower::pyramid(Point2::new(15.0, 6.0), 2.0, 15.0, 1.0);
    t.shape = TowerShape::Polygon;
    t.sides = 64;
    t.tiers = (0..MAX_TIERS)
        .map(|k| tier(1.0, if k % 2 == 0 { 3.0 } else { 1.0 }, 0.05))
        .collect();
    assert!(t.problems().is_empty(), "{:?}", t.problems());
    let mut p = host();
    while p.towers.iter().map(|t| t.tiers.len()).sum::<usize>() + MAX_TIERS <= MAX_TIERS_TOTAL {
        let mut c = t.clone();
        c.center.x = 3.0 + 5.0 * p.towers.len() as f64;
        p.towers.push(c);
    }
    assert!(p.check_limits().is_ok());
    let size = serde_json::to_string(&calc(&p)).unwrap().len();
    assert!(size < 1_500_000, "{size} bytes");
}

/// Audit r2 UX-001: a cone on a round building with an eave overhang: the
/// roof runs on at its own pitch to the overhang line, where the eave is,
/// `overhang · tan(pitch)` below the base height (the roof over the wall
/// line, as a wall's roof is measured); the tip stays where it was.
#[test]
fn a_tower_overhang_carries_the_first_tier_on_to_the_eave() {
    let mut t = Tower::pyramid(Point2::new(0.0, 0.0), 8.0, 10.0, 10.0);
    t.shape = TowerShape::Circle;
    t.overhang = 0.5;
    assert!(t.problems().is_empty(), "{:?}", t.problems());
    let r = t.calculate().unwrap();
    let tan = 10.0 / 4.0;
    let (r1, h1) = (4.5, 4.5 * tan);
    let exact = std::f64::consts::PI * r1 * (r1 * r1 + h1 * h1).sqrt();
    assert!(
        (r.roof_area() - exact).abs() < 1e-3 * exact,
        "{} vs {exact}",
        r.roof_area()
    );
    let eave = r.lines.iter().find(|l| l.code == "T").unwrap();
    assert!((eave.length - std::f64::consts::PI * 9.0).abs() < 1e-3 * 28.3);
    assert!(
        (eave.points[0][2] - (10.0 - 0.5 * tan)).abs() < 1e-3,
        "{:?}",
        eave.points[0]
    );
    assert!((r.top_height - 20.0).abs() < 1e-9);
    assert!((r.tiers[0].pitch_deg - tan.atan().to_degrees()).abs() < 1e-9);
    // The shaft is the wall's: its cut and base are as without overhang.
    let mut flush = t.clone();
    flush.overhang = 0.0;
    assert_eq!(t.footprint(), flush.footprint());
    assert!(
        r.base_area > 50.0 && (r.base_area - flush.calculate().unwrap().base_area).abs() < 1e-9
    );
}

/// Audit r2 UX-001: the overhang is saved only when given, and refused
/// where it cannot be: negative, too large, or under a first tier that
/// does not slope down outward (a lantern wall).
#[test]
fn a_tower_overhang_is_optional_and_refused_where_it_cannot_be() {
    let t = Tower::pyramid(Point2::new(0.0, 0.0), 4.0, 10.0, 3.0);
    let json = serde_json::to_string(&t).unwrap();
    assert!(!json.contains("overhang"), "{json}");
    let back: Tower = serde_json::from_str(&json).unwrap();
    assert_eq!(back.overhang, 0.0);
    let code = |t: &Tower| t.problems().into_iter().map(|p| p.0).collect::<Vec<_>>();
    for o in [-0.1, 1e3, f64::NAN] {
        let mut b = t.clone();
        b.overhang = o;
        assert!(!code(&b).is_empty(), "{o}");
    }
    let mut wall = t.clone();
    wall.tiers = vec![tier(1.0, 4.0, 0.0), tier(2.0, 0.0, 0.0)];
    wall.overhang = 0.3;
    assert_eq!(code(&wall), ["tower_overhang"]);
    let mut ok = t.clone();
    ok.overhang = 0.3;
    assert!(code(&ok).is_empty());
}

/// Audit r2 UX-002: a tower whose roof starts below the roof around it (its
/// roof sunk into the building's) gets a warning on that building naming
/// the tower, not only a flag in its row; one standing clear has none.
#[test]
fn a_tower_sunk_into_the_roof_is_a_warning_naming_it() {
    let mut p = host();
    p.towers
        .push(Tower::pyramid(Point2::new(15.0, 6.0), 2.0, 17.0, 2.0));
    p.towers
        .push(Tower::pyramid(Point2::new(20.0, 6.0), 2.0, 9.0, 2.0));
    let v = calc(&p);
    let w: Vec<&Value> = v["parts"][0]["geometry"]["warnings"]
        .as_array()
        .map(|a| a.iter().collect())
        .unwrap_or_default();
    let sunk: Vec<&&Value> = w
        .iter()
        .filter(|w| w["kind"] == "tower_base_below_roof")
        .collect();
    assert_eq!(sunk.len(), 1, "{w:?}");
    assert_eq!(sunk[0]["towers"], serde_json::json!([1]), "{w:?}");
}
