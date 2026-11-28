//! Towers and arcs: flat tops, tier materials, shafts beside or under the
//! roofs, turning, saving, copying, clockwise outlines, reflex corners,
//! extreme towers, ledges, curved hips and Pappus for a bell (audit
//! 2026-11-28 TEST-001..013: behaviour no other test pinned).

mod support;

use serde_json::Value;
use support::rect;
use tectora_core::Project;
use tectora_core::edit::arcs::{bend_wall, round_corner};
use tectora_core::edit::transform::{ItemKind, ItemRef, PlanMap, transform_items, transform_part};
use tectora_core::geom::Point2;
use tectora_core::report::calc;
use tectora_core::tower::{Tower, TowerShape, TowerTier};

const PI: f64 = std::f64::consts::PI;

fn host() -> Project {
    rect(30.0, 12.0, [(40.0, 8.0, 0.0); 4])
}
fn tier(rise: f64, top_width: f64, bulge: f64, material: u8) -> TowerTier {
    TowerTier {
        rise,
        top_width,
        bulge,
        material,
    }
}
fn f(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("not a number: {v}"))
}
fn roof(v: &Value) -> f64 {
    f(&v["totals"]["roof_area"])
}
fn parts(v: &Value) -> f64 {
    v["parts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| f(&p["quantities"]["roof_area"]))
        .sum()
}

#[test]
fn p01_a_flat_topped_tower_has_its_top_in_every_total() {
    // Square 4 -> 2 over 1 m beside the roofs: 12*sqrt2 + cap 4.
    let mut p = host();
    let mut t = Tower::pyramid(Point2::new(60.0, 6.0), 4.0, 5.0, 1.0);
    t.tiers = vec![tier(1.0, 2.0, 0.0, 2)];
    p.towers.push(t);
    let alone = calc(&host());
    let with = calc(&p);
    let expect = 12.0 * 2f64.sqrt() + 4.0;
    assert!(
        (roof(&with) - roof(&alone) - expect).abs() < 1e-6,
        "calc totals {}",
        roof(&with) - roof(&alone)
    );
    let geos = tectora_core::openings::generate_project(&p);
    let q = tectora_core::quantities::roof_totals_of(&geos, &p);
    assert!(
        (q.roof_area - roof(&with)).abs() < 1e-6,
        "roof_totals_of {}",
        q.roof_area
    );
    let by_pitch: f64 = tectora_core::quantities::area_by_pitch_in(&geos, &p)
        .iter()
        .map(|x| x.1)
        .sum();
    assert!(
        (by_pitch - roof(&with)).abs() < 0.02,
        "area_by_pitch {by_pitch} vs {}",
        roof(&with)
    );
    let b = tectora_core::breakdown::breakdown(&p);
    let mats: f64 = b
        .materials
        .iter()
        .flat_map(|m| m.items.iter().map(|i| i.area))
        .sum();
    assert!(
        (mats - roof(&with)).abs() < 1e-6,
        "breakdown materials {mats}"
    );
    use tectora_core::export::{ListOptions, list_summary};
    let s = list_summary(&p, &ListOptions::new("t", "28.11.2026 10:00"));
    let net: f64 = s.faces.iter().map(|f| f.area - f.deducted).sum();
    assert!((net - roof(&with)).abs() < 0.01, "list {net}");
    // K: the flat top's edge 8 m; no pult.
    assert!(
        (f(&with["totals"]["knick_length"]) - f(&alone["totals"]["knick_length"]) - 8.0).abs()
            < 1e-9
    );
}

#[test]
fn p02_tier_materials_reach_the_totals_and_the_lines_per_material() {
    let mut p = host();
    let mut t = Tower::pyramid(Point2::new(60.0, 6.0), 4.0, 5.0, 1.0);
    // Roof tier in material 3, a second, steeper one in material 4, flat top (material 4).
    t.tiers = vec![tier(1.0, 2.0, 0.0, 3), tier(2.0, 1.0, 0.0, 4)];
    p.towers.push(t.clone());
    let r = t.calculate().unwrap();
    let with = calc(&p);
    let by = with["totals"]["area_by_material"].as_array().unwrap();
    let of = |m: u64| {
        by.iter()
            .find(|x| x[0].as_u64() == Some(m))
            .map(|x| f(&x[1]))
            .unwrap_or(0.0)
    };
    assert!(
        (of(3) - r.tiers[0].area).abs() < 1e-9,
        "material 3: {} vs {}",
        of(3),
        r.tiers[0].area
    );
    assert!(
        (of(4) - r.tiers[1].area - r.cap.as_ref().unwrap().area).abs() < 1e-9,
        "material 4: {}",
        of(4)
    );
    let lines = tectora_core::quantities::lines_by_material(&p);
    let get = |m: u8, code: &str| {
        lines
            .iter()
            .find(|x| x.0 == m)
            .and_then(|x| x.1.get(code).copied())
            .unwrap_or(0.0)
    };
    // Eave 16 m on material 3; the break between the tiers (8 m) on both; the top's edge (4 m) on 4.
    assert!((get(3, "T") - 16.0).abs() < 0.01, "T3 {}", get(3, "T"));
    // Pitch breaks are code c (K is the valley: audit MATH-001).
    assert!((get(3, "c") - 8.0).abs() < 0.01, "c3 {}", get(3, "c"));
    assert!((get(4, "c") - 12.0).abs() < 0.01, "c4 {}", get(4, "c"));
    assert_eq!(get(3, "K") + get(4, "K"), 0.0);
}

#[test]
fn p03_a_tower_beside_the_roofs_cuts_nothing_and_stands_on_the_ground() {
    let mut p = host();
    p.towers
        .push(Tower::pyramid(Point2::new(60.0, 6.0), 4.0, 5.0, 3.0));
    let alone = calc(&host());
    let with = calc(&p);
    assert!((parts(&with) - parts(&alone)).abs() < 1e-9);
    let sh = &with["towers"][0]["shaft"];
    assert_eq!(sh["on_roof"], false);
    assert_eq!(sh["base_below_roof"], false);
    assert!(
        (f(&sh["wall_area"]) - 16.0 * 5.0).abs() < 1e-9,
        "{}",
        sh["wall_area"]
    );
}

#[test]
fn p04_a_tower_whose_roof_starts_under_the_host_roof_is_flagged() {
    // Ridge at 13.03 m; base at 9 m.
    let mut p = host();
    p.towers
        .push(Tower::pyramid(Point2::new(15.0, 6.0), 2.0, 9.0, 3.0));
    let with = calc(&p);
    let sh = &with["towers"][0]["shaft"];
    assert_eq!(sh["on_roof"], true);
    assert_eq!(sh["base_below_roof"], true);
    assert!(f(&sh["wall_area"]).abs() < 1e-9, "{}", sh["wall_area"]);
}

#[test]
fn p05_a_square_shaft_on_one_face_has_its_exact_wall_area_and_hole() {
    // On the south face (y 0..6), x 14..16, y 2..4, base 14 m, no rotation:
    // roof height at y is 8 + y*tan40. The probe points lie 5 cm outside
    // the corners along the diagonal.
    let mut p = host();
    p.towers
        .push(Tower::pyramid(Point2::new(15.0, 3.0), 2.0, 14.0, 2.0));
    let alone = calc(&host());
    let with = calc(&p);
    let tan = 40f64.to_radians().tan();
    let hole = 4.0 / 40f64.to_radians().cos();
    assert!(
        (parts(&alone) - parts(&with) - hole).abs() < 1e-6,
        "hole {}",
        parts(&alone) - parts(&with)
    );
    // Exact wall above the roof: two side walls (mean roof height at y=3),
    // the low wall at y=2 and the high wall at y=4.
    let exact = 2.0 * 2.0 * (14.0 - (8.0 + 3.0 * tan))
        + 2.0 * (14.0 - (8.0 + 2.0 * tan))
        + 2.0 * (14.0 - (8.0 + 4.0 * tan));
    let wall = f(&with["towers"][0]["shaft"]["wall_area"]);
    assert!(
        (wall - exact).abs() < 0.02 * exact,
        "shaft wall {wall} vs exact {exact}"
    );
}

#[test]
fn p06_a_round_shaft_cuts_its_circle_over_the_cosine() {
    let mut p = host();
    let mut t = Tower::pyramid(Point2::new(15.0, 3.0), 2.0, 14.0, 2.0);
    t.shape = TowerShape::Circle;
    p.towers.push(t);
    let alone = calc(&host());
    let with = calc(&p);
    let hole = PI / 40f64.to_radians().cos();
    let cut = parts(&alone) - parts(&with);
    assert!(
        (cut - hole).abs() < 0.01 * hole,
        "round hole {cut} vs {hole}"
    );
}

#[test]
fn p07_turning_a_tower_changes_no_figure() {
    let mut a = Tower::pyramid(Point2::new(60.0, 6.0), 4.0, 5.0, 3.0);
    a.depth = Some(2.5);
    a.corner_radius = 0.5;
    a.tiers = vec![tier(1.0, 3.0, -0.1, 2), tier(2.0, 0.0, 0.2, 2)];
    let mut b = a.clone();
    b.rotation_deg = 33.3;
    let (ra, rb) = (a.calculate().unwrap(), b.calculate().unwrap());
    assert!((ra.roof_area() - rb.roof_area()).abs() < 1e-9);
    assert!((ra.base_perimeter - rb.base_perimeter).abs() < 1e-9);
}

#[test]
fn p08_a_file_with_a_bad_tower_opens_and_the_buildings_keep_their_roof() {
    let mut p = host();
    p.towers
        .push(Tower::pyramid(Point2::new(15.0, 3.0), 2.0, 14.0, 2.0));
    let mut v: Value = serde_json::from_str(&p.to_json().unwrap()).unwrap();
    v["towers"][0]["width"] = (-1.0).into();
    v["towers"][0]["tiers"] = serde_json::json!([]);
    let q = Project::from_json(&v.to_string()).expect("the job opens");
    assert!(q.check_limits().is_ok(), "{:?}", q.check_limits());
    let c = calc(&q);
    assert!(
        (parts(&c) - parts(&calc(&host()))).abs() < 1e-9,
        "the host roof is whole"
    );
    assert!(c["towers"][0]["result"].is_null());
    let codes: Vec<&str> = c["problems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["code"].as_str().unwrap())
        .collect();
    assert!(
        codes.contains(&"tower_size") && codes.contains(&"tower_no_tiers"),
        "{codes:?}"
    );
    // And every writer answers without a null number.
    let s = c.to_string();
    assert!(!s.contains("NaN") && !s.contains("inf"));
}

#[test]
fn p09_a_tower_survives_a_save_and_load_with_its_defaults_left_out() {
    let mut p = host();
    let mut t = Tower::pyramid(Point2::new(15.0, 3.0), 2.0, 14.0, 2.0);
    t.shape = TowerShape::Polygon;
    t.sides = 6;
    t.rotation_deg = 12.5;
    t.depth = None;
    t.tiers = vec![tier(1.0, 1.0, 0.0, 1), tier(1.0, 0.0, -0.05, 2)];
    p.towers.push(t);
    let s = p.to_json().unwrap();
    let q = Project::from_json(&s).unwrap();
    assert_eq!(p.towers, q.towers);
    // A tier written without `material` is sheet metal (2), not tiles.
    let mut v: Value = serde_json::from_str(&s).unwrap();
    v["towers"][0]["tiers"][0]
        .as_object_mut()
        .unwrap()
        .remove("material");
    let q = Project::from_json(&v.to_string()).unwrap();
    assert_eq!(q.towers[0].tiers[0].material, 2);
}

#[test]
fn p10_copies_of_a_building_with_a_tower_stay_within_the_tower_limit() {
    let mut p = host();
    p.towers
        .push(Tower::pyramid(Point2::new(15.0, 3.0), 2.0, 14.0, 2.0));
    p.towers
        .push(Tower::pyramid(Point2::new(10.0, 3.0), 2.0, 14.0, 2.0));
    let added = transform_part(&mut p, 0, PlanMap::Move { dx: 40.0, dy: 0.0 }, 30);
    assert_eq!(added.len(), 30);
    assert_eq!(p.towers.len(), 62);
    // 62 towers are over MAX_TOWERS (50): the commands refuse the job as too many.
    let e = p.check_input().unwrap_err();
    assert_eq!(e.code, "too_many", "{}", e.message);
}

#[test]
fn p11_a_single_tower_moves_turns_and_copies_as_an_item() {
    let mut p = host();
    let mut t = Tower::pyramid(Point2::new(15.0, 3.0), 2.0, 14.0, 2.0);
    t.rotation_deg = 30.0;
    p.towers.push(t);
    let it = [ItemRef {
        kind: ItemKind::Tower,
        index: 0,
    }];
    transform_items(&mut p, &it, PlanMap::Move { dx: 1.0, dy: -0.5 }, 0).unwrap();
    assert_eq!(p.towers[0].center, Point2::new(16.0, 2.5));
    assert_eq!(p.towers[0].rotation_deg, 30.0);
    let out = transform_items(
        &mut p,
        &it,
        PlanMap::Rotate {
            centre: Point2::new(16.0, 2.5),
            deg: 90.0,
        },
        1,
    )
    .unwrap();
    assert_eq!(
        out,
        vec![ItemRef {
            kind: ItemKind::Tower,
            index: 1
        }]
    );
    assert_eq!(p.towers[1].rotation_deg, 120.0);
    assert_eq!(p.towers[0].rotation_deg, 30.0);
}

fn square_cw(s: f64) -> tectora_core::RoofPart {
    let mut p = rect(s, s, [(30.0, 3.0, 0.5); 4]).parts.remove(0);
    p.edges.reverse();
    p
}

#[test]
fn p12_a_wall_of_a_clockwise_outline_bends_outward_too() {
    let mut p = square_cw(10.0);
    let a = p.edges[0].start;
    let b = p.edges[1].start;
    bend_wall(&mut p, 0, 1.0, Some(4)).unwrap();
    let mid = p.edges[2].start;
    // Outward: farther from the square's centre (5, 5) than the wall's middle.
    let wall_mid = Point2::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
    let c = Point2::new(5.0, 5.0);
    assert!(mid.distance(c) > wall_mid.distance(c) + 0.9, "{mid:?}");
}

#[test]
fn p13_arc_walls_take_the_eave_not_the_gable() {
    // Walls: 0 eave, 1 gable, 2 eave, 3 gable. Corner 1 lies between the
    // eave 0 (before) and the gable 1 (after): the arc is eave. Corner 2
    // lies between gable 1 (before) and eave 2 (after): the arc is eave too.
    let mut p = rect(
        10.0,
        10.0,
        [
            (30.0, 3.0, 0.5),
            (90.0, 3.0, 0.0),
            (30.0, 3.0, 0.5),
            (90.0, 3.0, 0.0),
        ],
    )
    .parts
    .remove(0);
    let n = round_corner(&mut p, 2, 2.0, None).unwrap();
    assert!(
        p.edges[2..2 + n].iter().all(|e| !e.is_gable()),
        "arc after a gable is the eave after it"
    );
    let mut p = rect(
        10.0,
        10.0,
        [
            (30.0, 3.0, 0.5),
            (90.0, 3.0, 0.0),
            (30.0, 3.0, 0.5),
            (90.0, 3.0, 0.0),
        ],
    )
    .parts
    .remove(0);
    let n = round_corner(&mut p, 1, 2.0, None).unwrap();
    assert!(p.edges[1..1 + n].iter().all(|e| !e.is_gable()));
}

#[test]
fn p14_a_rounded_reflex_corner_of_an_l_stays_a_valid_outline() {
    let mut p = support::uniform(
        &[
            (0.0, 0.0),
            (12.0, 0.0),
            (12.0, 5.0),
            (5.0, 5.0),
            (5.0, 10.0),
            (0.0, 10.0),
        ],
        38.0,
        3.0,
        0.5,
    );
    let before = tectora_core::geom::signed_area(&p.parts[0].outline());
    let n = round_corner(&mut p.parts[0], 3, 2.0, None).unwrap();
    let after = tectora_core::geom::signed_area(&p.parts[0].outline());
    // The inner corner is filled by the arc: the plan grows by 4 - (sector as n triangles).
    let sector = n as f64 * 0.5 * 4.0 * (PI / 2.0 / n as f64).sin();
    assert!(
        (after - before - (4.0 - sector)).abs() < 1e-3,
        "{before} -> {after}"
    );
    assert!(p.problems().is_empty(), "{:?}", p.problems());
    let geos = tectora_core::openings::generate_project(&p);
    assert!(geos[0].error.is_none(), "{:?}", geos[0].error);
}

#[test]
fn p15_the_first_corner_and_the_last_wall_wrap_around() {
    let mut p = rect(10.0, 10.0, [(30.0, 3.0, 0.5); 4]);
    let n = round_corner(&mut p.parts[0], 0, 3.0, Some(4)).unwrap();
    assert_eq!(n, 4);
    assert_eq!(p.parts[0].edges.len(), 8);
    let area = tectora_core::geom::signed_area(&p.parts[0].outline());
    let sector = 4.0 * 0.5 * 9.0 * (PI / 8.0).sin();
    assert!((area - (100.0 - 9.0 + sector)).abs() < 1e-3, "{area}");
    assert!(
        p.parts[0]
            .edges
            .iter()
            .all(|e| e.start.distance(Point2::new(0.0, 0.0)) > 0.5)
    );
    let mut q = rect(10.0, 10.0, [(30.0, 3.0, 0.5); 4]);
    bend_wall(&mut q.parts[0], 3, 5.0, Some(8)).unwrap();
    // A semicircle on the west wall (x = 0): its apex at (-5, 5).
    assert!(
        q.parts[0]
            .edges
            .iter()
            .any(|e| e.start.approx_eq(Point2::new(-5.0, 5.0), 1e-3))
    );
    assert!(q.problems().is_empty(), "{:?}", q.problems());
}

#[test]
fn p16_extreme_but_valid_towers_calculate_to_finite_figures() {
    for (w, rise, top, bulge) in [
        (0.1, 100.0, 0.0, 0.0),
        (100.0, 0.0, 200.0, 0.0),
        (100.0, 100.0, 0.0, 50.0),
        (0.1, 0.001, 0.0, 0.0005),
        (4.0, 0.0, 4.0, 0.0),
    ] {
        let mut t = Tower::pyramid(Point2::new(0.0, 0.0), w, 0.0, 1.0);
        t.shape = TowerShape::Circle;
        t.tiers = vec![tier(rise, top, bulge, 2)];
        if !t.problems().is_empty() {
            continue;
        }
        let mut p = Project::new("x");
        p.towers.push(t.clone());
        let c = calc(&p);
        let total = &c["totals"]["roof_area"];
        assert!(
            total.is_number() && f(total).is_finite() && f(total) >= 0.0,
            "{t:?}: {total}"
        );
        let s = c.to_string();
        assert!(
            !s.contains(":null") || c["towers"][0]["result"].is_object(),
            "{s}"
        );
    }
}

#[test]
fn p17_a_ledge_is_an_annulus_and_a_cornice_leans_over_ninety_degrees() {
    // Circle r 2 -> r 1 on a flat ledge (rise 0): pi (4 - 1); then a cone.
    let mut t = Tower::pyramid(Point2::new(0.0, 0.0), 4.0, 10.0, 1.0);
    t.shape = TowerShape::Circle;
    t.tiers = vec![
        tier(0.0, 2.0, 0.0, 2),
        tier(0.5, 3.0, 0.0, 2),
        tier(2.0, 0.0, 0.0, 2),
    ];
    let r = t.calculate().unwrap();
    assert!(
        (r.tiers[0].area - 3.0 * PI).abs() < 1e-3 * 3.0 * PI,
        "{}",
        r.tiers[0].area
    );
    assert_eq!(r.tiers[0].pitch_deg, 0.0);
    // The cornice widens by 0.5 over 0.5: 135 degrees.
    assert!(
        (r.tiers[1].pitch_deg - 135.0).abs() < 1e-9,
        "{}",
        r.tiers[1].pitch_deg
    );
}

#[test]
fn p18_the_hips_of_a_curved_helm_are_as_long_as_the_lines_drawn() {
    let t = tectora_core::edit::towers::preset(
        tectora_core::edit::towers::TowerPreset::Helm,
        Point2::new(0.0, 0.0),
        4.0,
        10.0,
    );
    let r = t.calculate().unwrap();
    let g: Vec<_> = r
        .lines
        .iter()
        .filter(|l| l.code == "G" && l.tier == 0)
        .collect();
    assert_eq!(g.len(), 4);
    let drawn: f64 = g[0]
        .points
        .windows(2)
        .map(|w| {
            ((w[1][0] - w[0][0]).powi(2)
                + (w[1][1] - w[0][1]).powi(2)
                + (w[1][2] - w[0][2]).powi(2))
            .sqrt()
        })
        .sum();
    assert!(
        (g[0].length - drawn).abs() < 0.005 * drawn,
        "{} vs drawn {drawn}",
        g[0].length
    );
    assert!((r.tiers[0].hip_length - g[0].length).abs() < 1e-12);
}

#[test]
fn p19_a_bell_follows_pappus() {
    // A concave quarter circle of radius 1 centred at (h, z) = (2, 0)...
    // profile from (1, 0) up to (2, 1) would widen; take the mirrored bell:
    // from (2, 0) to (1, 1), centre (2, 1), radius 1 (concave, toward the
    // axis). Pappus: A = 2 pi * (centroid radius) * arc length, centroid
    // of a quarter arc 2R/pi from the centre along each axis:
    // radius = 2 - 2/pi, length = pi/2.
    let mut t = Tower::pyramid(Point2::new(0.0, 0.0), 4.0, 0.0, 1.0);
    t.shape = TowerShape::Circle;
    let sag = 1.0 - (PI / 4.0).cos();
    t.tiers = vec![tier(1.0, 2.0, -sag, 2)];
    let r = t.calculate().unwrap();
    let exact = 2.0 * PI * (2.0 - 2.0 / PI) * (PI / 2.0);
    assert!(
        (r.tiers[0].area - exact).abs() < 1e-4 * exact,
        "{} vs {exact}",
        r.tiers[0].area
    );
}

#[test]
fn p20_a_flat_roof_job_loses_the_shaft_and_gains_the_tower() {
    let mut p = host();
    p.flat_roof = true;
    let alone = calc(&p);
    p.towers
        .push(Tower::pyramid(Point2::new(15.0, 6.0), 2.0, 14.0, 2.0));
    let with = calc(&p);
    let tower = f(&with["towers"][0]["roof_area"]);
    assert!(
        (parts(&alone) - parts(&with) - 4.0).abs() < 1e-6,
        "flat hole {}",
        parts(&alone) - parts(&with)
    );
    assert!((roof(&with) - (roof(&alone) - 4.0 + tower)).abs() < 1e-6);
}
