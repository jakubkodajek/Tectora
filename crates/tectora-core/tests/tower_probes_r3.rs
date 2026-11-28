//! Towers and arcs, audit round 3 test review (2026-11-28-towers-r3-2f87935,
//! test.md): behaviour the suite did not pin before, each case named by the
//! mutant it kills (TEST-002 to TEST-010). Checked against closed forms;
//! towers have no DACAD counterpart (ADR 0008).
mod support;

use serde_json::Value;
use support::rect;
use tectora_core::edit::arcs;
use tectora_core::edit::towers as edit;
use tectora_core::geom::Point2;
use tectora_core::report::calc;
use tectora_core::tower::{Tower, TowerShape, TowerTier};

const PITCH: f64 = 40.0;
const EAVE: f64 = 8.0;
fn host() -> tectora_core::Project {
    rect(30.0, 12.0, [(PITCH, EAVE, 0.0); 4])
}
fn tier(rise: f64, top_width: f64, bulge: f64) -> TowerTier {
    let mut t = Tower::pyramid(Point2::new(0.0, 0.0), 1.0, 1.0, 1.0).tiers[0].clone();
    t.rise = rise;
    t.top_width = top_width;
    t.bulge = bulge;
    t
}
fn codes(t: &Tower) -> Vec<&'static str> {
    t.problems().into_iter().map(|p| p.0).collect()
}

/// M19-M23 (no test reaches them since round 1): each rule of
/// `Tower::problems` has a case that names its code.
#[test]
fn r01_every_tower_rule_names_its_code() {
    let ok = Tower::pyramid(Point2::new(0.0, 0.0), 4.0, 10.0, 3.0);
    assert!(codes(&ok).is_empty());
    let with = |f: &dyn Fn(&mut Tower)| {
        let mut t = ok.clone();
        f(&mut t);
        codes(&t)
    };
    assert_eq!(
        with(&|t| {
            t.shape = TowerShape::Polygon;
            t.sides = 2;
        }),
        ["tower_sides"]
    );
    assert_eq!(
        with(&|t| {
            t.shape = TowerShape::Polygon;
            t.sides = 65;
        }),
        ["tower_sides"]
    );
    assert_eq!(with(&|t| t.base_height = -0.1), ["tower_base"]);
    assert_eq!(with(&|t| t.corner_radius = -0.1), ["tower_corner_radius"]);
    assert_eq!(
        with(&|t| t.tiers = (0..tectora_core::tower::MAX_TIERS + 1)
            .map(|_| tier(0.1, 4.0, 0.0))
            .collect()),
        ["tower_tiers"]
    );
    assert_eq!(
        with(&|t| t.tiers = vec![tier(-1.0, 0.0, 0.0)]),
        ["tower_tier_size"]
    );
    assert_eq!(
        with(&|t| t.tiers = vec![tier(101.0, 0.0, 0.0)]),
        ["tower_tier_size"]
    );
    assert_eq!(
        with(&|t| t.tiers = vec![tier(1.0, 201.0, 0.0)]),
        ["tower_tier_size"]
    );
    assert_eq!(with(&|t| t.width = 1e9), ["tower_nan"]);
}

/// M221, M220: the pitch of an upper tier is measured from the tier below
/// it, and a pitch that is not 45° gives run · tan(pitch).
#[test]
fn r02_an_upper_tiers_pitch_uses_the_tier_below_and_the_tangent() {
    let mut t = Tower::pyramid(Point2::new(0.0, 0.0), 6.0, 10.0, 1.0);
    t.tiers = vec![tier(1.0, 4.0, 0.0), tier(1.0, 1.0, 0.0)];
    assert_eq!(edit::tier_bottom_width(&t, 1), Some(4.0));
    let v = tectora_core::commands::item_geometry(
        &tectora_core::commands::GeometryRequest::TowerTierRise {
            tower: t.clone(),
            tier: 1,
            pitch_deg: 60.0,
        },
    )
    .unwrap();
    let want = 1.5 * 60f64.to_radians().tan();
    assert!(
        (v["rise"].as_f64().unwrap() - want).abs() < 1e-3,
        "{v} vs {want}"
    );
    // And the tier with that rise has that pitch.
    t.tiers[1].rise = v["rise"].as_f64().unwrap();
    let r = t.calculate().unwrap();
    assert!(
        (r.tiers[1].pitch_deg - 60.0).abs() < 0.01,
        "{}",
        r.tiers[1].pitch_deg
    );
    // 30° is not 60°.
    let lo = edit::rise_for_pitch(6.0, 4.0, 30.0).unwrap();
    assert!((lo - 30f64.to_radians().tan()).abs() < 1e-3, "{lo}");
}

/// M222: more towers than a job takes are refused by the count check that
/// transform_part and check_write use, with the tower limit's code.
#[test]
fn r03_the_count_check_refuses_the_51st_tower() {
    let mut p = host();
    let t = Tower::pyramid(Point2::new(3.0, 3.0), 1.0, 12.0, 1.0);
    p.towers = vec![t; tectora_core::tower::MAX_TOWERS + 1];
    let e = edit::check_tower_counts(&p).unwrap_err();
    assert!(e.ends_with(&format!("({})", edit::TOWER_LIMIT)), "{e}");
    let mut before = p.clone();
    before.towers.pop();
    let e = tectora_core::commands::check_write(Some(&before), &p).unwrap_err();
    assert_eq!(e.code, "too_many");
    assert!(
        e.detail.ends_with(&format!("({})", edit::TOWER_LIMIT)),
        "{e}"
    );
}

/// M243, M241: of two buildings the warning of a sunk tower lands on the
/// one whose roof it stands in, with the depth it is sunk by.
#[test]
fn r04_a_sunk_towers_warning_is_on_its_own_building_with_the_depth() {
    for other_first in [false, true] {
        let mut p = host();
        let far = support::uniform(
            &[(100.0, 0.0), (110.0, 0.0), (110.0, 8.0), (100.0, 8.0)],
            30.0,
            20.0,
            0.0,
        );
        if other_first {
            p.parts.insert(0, far.parts[0].clone());
        } else {
            p.parts.push(far.parts[0].clone());
        }
        let own = usize::from(other_first);
        // On the ridge (8 + 6·tan 40° = 13.03 m), the tower's roof from 10 m.
        p.towers
            .push(Tower::pyramid(Point2::new(15.0, 6.0), 2.0, 10.0, 2.0));
        let v = calc(&p);
        let find = |i: usize| -> Vec<Value> {
            v["parts"][i]["geometry"]["warnings"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter(|w| w["kind"] == "tower_base_below_roof")
                        .cloned()
                        .collect()
                })
                .unwrap_or_default()
        };
        assert_eq!(
            find(own).len(),
            1,
            "own building {own}: {}",
            v["parts"][own]["geometry"]["warnings"]
        );
        assert!(find(1 - own).is_empty(), "the other building is warned");
        let depth = find(own)[0]["value"].as_f64().unwrap();
        let ridge = EAVE + 6.0 * PITCH.to_radians().tan();
        assert!(
            (depth - (ridge - 10.0)).abs() < 0.2,
            "{depth} vs {}",
            ridge - 10.0
        );
    }
}

/// M273: a rounded corner of so many walls that its corners turn by less
/// than 2° is refused as too flat (the core would merge them).
#[test]
fn r05_a_rounded_corner_of_too_many_walls_is_too_flat() {
    let mut p = rect(20.0, 20.0, [(30.0, 3.0, 0.0); 4]);
    let part = &mut p.parts[0];
    let before = part.clone();
    let e = arcs::round_corner(part, 1, 5.0, Some(24)).unwrap_err();
    assert!(e.ends_with(&format!("({})", arcs::ARC_FLAT)), "{e}");
    assert_eq!(*part, before);
    assert!(arcs::round_corner(part, 1, 5.0, Some(8)).is_ok());
}

/// M277: a wall that is a direction marker is no arc wall.
#[test]
fn r06_a_marker_wall_is_no_arc() {
    let mut p = rect(12.0, 10.0, [(30.0, 3.0, 0.0); 4]);
    let part = &mut p.parts[0];
    let segs = arcs::bend_wall(part, 0, 2.0, None).unwrap();
    assert!(arcs::arc_run(part, 1).is_some());
    part.edges[1].direction_rad = Some(0.0);
    assert_eq!(arcs::arc_run(part, 1), None, "{segs}");
}

/// M47: a flat-topped tower's top is in the breakdown's materials, so the
/// materials add up to the roof area of the job.
#[test]
fn r07_a_flat_top_is_in_the_breakdowns_materials() {
    let mut p = host();
    let mut t = Tower::pyramid(Point2::new(15.0, 6.0), 4.0, 16.0, 1.0);
    t.tiers = vec![tier(1.0, 2.0, 0.0)];
    p.towers.push(t.clone());
    let r = t.calculate().unwrap();
    let cap = r.cap.as_ref().unwrap().area;
    assert!((cap - 4.0).abs() < 1e-9);
    let b = tectora_core::breakdown::breakdown(&p);
    let materials: f64 = b.materials.iter().map(|m| m.area).sum();
    let total = calc(&p)["totals"]["roof_area"].as_f64().unwrap();
    assert!(
        (materials - total).abs() < 1e-6,
        "materials {materials} vs total {total}"
    );
    let towers: f64 = b
        .materials
        .iter()
        .flat_map(|m| &m.items)
        .filter(|i| i.tower.is_some())
        .map(|i| i.area)
        .sum();
    assert!(
        (towers - r.roof_area()).abs() < 1e-6,
        "{towers} vs {}",
        r.roof_area()
    );
}

/// M210 / M105 on a rectangle: with an overhang the pitch of both pairs of
/// sides stays the tier's own.
#[test]
fn r08_a_rectangular_tower_with_an_overhang_keeps_both_pitches() {
    let mut t = Tower::pyramid(Point2::new(0.0, 0.0), 6.0, 10.0, 3.0);
    t.shape = TowerShape::Rectangle;
    t.depth = Some(4.0);
    let flush = t.calculate().unwrap();
    t.overhang = 0.5;
    assert!(codes(&t).is_empty(), "{:?}", t.problems());
    let r = t.calculate().unwrap();
    let (a, b) = (&flush.tiers[0], &r.tiers[0]);
    assert!(
        (a.pitch_deg - b.pitch_deg).abs() < 1e-9,
        "{} vs {}",
        a.pitch_deg,
        b.pitch_deg
    );
    let (da, db) = (a.depth_sides.unwrap(), b.depth_sides.unwrap());
    assert!(
        (da.pitch_deg - db.pitch_deg).abs() < 1e-9,
        "{} vs {}",
        da.pitch_deg,
        db.pitch_deg
    );
    // How far the eave runs out on the depth sides is MATH-001's question
    // (round 3): today the plan is scaled, 0.5 m on the width sides is
    // 0.333 m on the depth sides. Pin the figure here once it is decided.
    assert!(r.roof_area() > flush.roof_area() + 1.0);
}

/// M214: a job of many towers is drawn coarser, with the same figures.
#[test]
fn r09_the_figures_do_not_depend_on_how_finely_the_job_is_drawn() {
    let mut t = Tower::pyramid(Point2::new(15.0, 6.0), 2.0, 15.0, 1.0);
    t.shape = TowerShape::Circle;
    t.tiers = vec![tier(1.0, 3.0, 0.3), tier(2.0, 0.0, -0.2)];
    let alone = t.calculate().unwrap();
    let mut p = host();
    for k in 0..50 {
        let mut c = t.clone();
        c.center.x = 100.0 + 5.0 * k as f64;
        p.towers.push(c);
    }
    assert_ne!(
        tectora_core::tower::job_detail(&p),
        tectora_core::tower::Detail::FULL,
        "pick a job that is drawn coarser"
    );
    let rs = tectora_core::tower::results(&p);
    let r = rs[0].as_ref().unwrap();
    assert!(
        (r.roof_area() - alone.roof_area()).abs() < 1e-9,
        "{} vs {}",
        r.roof_area(),
        alone.roof_area()
    );
    for (a, b) in r.lines.iter().zip(&alone.lines) {
        assert_eq!(a.code, b.code);
        assert!(
            (a.length - b.length).abs() < 1e-9,
            "{}: {} vs {}",
            a.code,
            a.length,
            b.length
        );
    }
    assert!(r.lines[0].points.len() < alone.lines[0].points.len());
}

/// M230: a tower that stands on two buildings stays when one is deleted.
#[test]
fn r10_a_tower_on_two_buildings_stays_when_one_goes() {
    let mut p = host();
    let wing = support::uniform(
        &[(11.0, -8.0), (19.0, -8.0), (19.0, 2.0), (11.0, 2.0)],
        40.0,
        5.0,
        0.0,
    );
    p.parts.push(wing.parts[0].clone());
    p.towers
        .push(Tower::pyramid(Point2::new(15.0, 1.0), 1.0, 16.0, 1.0)); // in both
    p.towers
        .push(Tower::pyramid(Point2::new(15.0, -5.0), 1.0, 16.0, 1.0)); // wing only
    p.towers
        .push(Tower::pyramid(Point2::new(50.0, 50.0), 1.0, 16.0, 1.0)); // on no building
    let gone = tectora_core::edit::transform::delete_part(&mut p, 1).unwrap();
    assert_eq!(gone, vec![1]);
    assert_eq!(p.towers.len(), 2);
    assert_eq!(p.towers[0].center, Point2::new(15.0, 1.0));
    assert_eq!(p.towers[1].center, Point2::new(50.0, 50.0));
}

fn failed_building() -> tectora_core::Project {
    // A hip roof 12 × 8 with a 5 mm wall: the building is left out.
    support::uniform(
        &[
            (0.0, 0.0),
            (12.0, 0.0),
            (12.005, 0.0),
            (12.0, 8.0),
            (0.0, 8.0),
        ],
        30.0,
        3.0,
        0.5,
    )
}

/// M7, M8 (surviving since round 1): what the Results panel reads
/// (`totals.hip_length`, `hip_count`, `pult_length`, `eave_length`,
/// `knick_length`) follows the tower's lines by code.
#[test]
fn r12_the_totals_fields_follow_the_towers_lines() {
    let alone = calc(&host());
    let mut t = Tower::pyramid(Point2::new(100.0, 100.0), 2.0, 9.0, 1.0);
    // A frustum, a wall (lantern), a steeper helm: eave, Pult, hips, a second eave.
    t.tiers = vec![
        tier(1.0, 1.0, 0.0),
        tier(1.0, 1.0, 0.0),
        tier(0.5, 0.6, 0.0),
        tier(2.0, 0.0, 0.0),
    ];
    let r = t.calculate().unwrap();
    let by = |code: &str| -> f64 {
        r.lines
            .iter()
            .filter(|l| l.code == code)
            .map(|l| l.length)
            .sum()
    };
    let n = |code: &str| r.lines.iter().filter(|l| l.code == code).count() as f64;
    assert!(
        by("G") > 0.0 && by("P") > 0.0 && by("T") > 0.0 && by("c") > 0.0,
        "{:?}",
        r.lines.iter().map(|l| l.code).collect::<Vec<_>>()
    );
    let mut p = host();
    p.towers.push(t);
    let v = calc(&p);
    let d = |k: &str| v["totals"][k].as_f64().unwrap() - alone["totals"][k].as_f64().unwrap();
    assert!(
        (d("hip_length") - by("G")).abs() < 1e-6,
        "hips {} vs {}",
        d("hip_length"),
        by("G")
    );
    assert!(
        (d("hip_count") - n("G")).abs() < 1e-9,
        "hip count {} vs {}",
        d("hip_count"),
        n("G")
    );
    assert!(
        (d("pult_length") - by("P")).abs() < 1e-6,
        "Pult {} vs {}",
        d("pult_length"),
        by("P")
    );
    assert!(
        (d("eave_length") - by("T")).abs() < 1e-6,
        "eave {} vs {}",
        d("eave_length"),
        by("T")
    );
    assert!(
        (d("knick_length") - by("c")).abs() < 1e-6,
        "break {} vs {}",
        d("knick_length"),
        by("c")
    );
    assert!(d("valley_length").abs() < 1e-9);
}

/// M204: an overhang under a first tier that leans outward is refused
/// (its line carried on would rise, not fall).
#[test]
fn r13_an_overhang_under_a_tier_leaning_out_is_refused() {
    let mut t = Tower::pyramid(Point2::new(0.0, 0.0), 4.0, 10.0, 3.0);
    t.tiers = vec![tier(1.0, 5.0, 0.0), tier(3.0, 0.0, 0.0)];
    assert!(codes(&t).is_empty());
    t.overhang = 0.3;
    assert_eq!(codes(&t), ["tower_overhang"]);
    assert!(t.calculate().is_none());
}

/// M1: two tiers of one pitch meet in no break line; two of different
/// pitch do.
#[test]
fn r14_tiers_of_one_pitch_have_no_break_line() {
    let mut t = Tower::pyramid(Point2::new(0.0, 0.0), 4.0, 10.0, 1.0);
    t.tiers = vec![tier(1.0, 3.0, 0.0), tier(3.0, 0.0, 0.0)];
    let r = t.calculate().unwrap();
    assert!((r.tiers[0].pitch_deg - r.tiers[1].pitch_deg).abs() < 1e-9);
    assert!(
        !r.lines.iter().any(|l| l.code == "c"),
        "{:?}",
        r.lines.iter().map(|l| l.code).collect::<Vec<_>>()
    );
    t.tiers = vec![tier(1.0, 3.0, 0.0), tier(1.0, 0.0, 0.0)];
    let r = t.calculate().unwrap();
    let c: f64 = r
        .lines
        .iter()
        .filter(|l| l.code == "c")
        .map(|l| l.length)
        .sum();
    assert!((c - 12.0).abs() < 1e-9, "{c}");
}

/// M50, M51: the price list has every line code of a tower and its tiers'
/// own materials.
#[test]
fn r15_the_list_has_the_towers_lines_and_materials() {
    use tectora_core::export::{ListOptions, list_summary};
    let opts = ListOptions::new("t", "28.11.2026 10:00");
    let alone = list_summary(&host(), &opts);
    let mut t = Tower::pyramid(Point2::new(100.0, 100.0), 2.0, 9.0, 1.0);
    t.tiers = vec![tier(1.0, 1.0, 0.0), tier(2.0, 0.0, 0.0)];
    t.tiers[0].material = 4;
    t.tiers[1].material = 3;
    let r = t.calculate().unwrap();
    let mut p = host();
    p.towers.push(t);
    let s = list_summary(&p, &opts);
    let line = |s: &tectora_core::export::ListSummary, c: &str| -> f64 {
        s.lines
            .iter()
            .filter(|l| l.code == c)
            .map(|l| l.length)
            .sum()
    };
    for code in ["T", "G", "c"] {
        let own: f64 = r
            .lines
            .iter()
            .filter(|l| l.code == code)
            .map(|l| l.length)
            .sum();
        assert!(own > 0.0, "{code}");
        assert!(
            (line(&s, code) - line(&alone, code) - own).abs() < 1e-3,
            "{code}: {} vs {} + {own}",
            line(&s, code),
            line(&alone, code)
        );
    }
    let mat = |s: &tectora_core::export::ListSummary, m: u8| -> f64 {
        s.materials
            .iter()
            .filter(|x| x.material == m)
            .map(|x| x.area)
            .sum()
    };
    assert!(
        (mat(&s, 4) - mat(&alone, 4) - r.tiers[0].area).abs() < 1e-3,
        "slate {}",
        mat(&s, 4)
    );
    assert!(
        (mat(&s, 3) - mat(&alone, 3) - r.tiers[1].area).abs() < 1e-3,
        "stone {}",
        mat(&s, 3)
    );
}

/// M107: a curve leaving its tier's heights (a dome dimpled over its top)
/// is tower_bulge.
#[test]
fn r16_a_curve_outside_its_tiers_heights_is_refused() {
    let mut t = Tower::pyramid(Point2::new(0.0, 0.0), 4.0, 10.0, 1.0);
    // Wide and low: a bulge of 0.9 rises above the tier's top before it ends.
    t.tiers = vec![tier(0.2, 0.0, 0.9)];
    assert_eq!(codes(&t), ["tower_bulge"], "{:?}", t.problems());
}

/// M216: a turret wholly outside the walls of a failed building but under
/// its overhang says the roof failed, as on the good building it stands on
/// the roof.
#[test]
fn r17_a_turret_under_the_overhang_of_a_failed_building_says_so() {
    let t = Tower::pyramid(Point2::new(6.0, -0.75), 1.0, 9.0, 2.0);
    let good = support::uniform(
        &[(0.0, 0.0), (12.0, 0.0), (12.0, 8.0), (0.0, 8.0)],
        30.0,
        3.0,
        0.5,
    );
    let mut p = good.clone();
    p.towers.push(t.clone());
    assert_eq!(calc(&p)["towers"][0]["shaft"]["on_roof"], true);
    let mut p = failed_building();
    p.towers.push(t);
    let shaft = calc(&p)["towers"][0]["shaft"].clone();
    assert_eq!(shaft["roof_failed"], true, "{shaft}");
}

/// M217: a shaft over a whole corner of a failed building, none of its own
/// corners in the building, says the roof failed.
#[test]
fn r18_a_shaft_over_a_corner_of_a_failed_building_says_so() {
    let mut t = Tower::pyramid(Point2::new(-1.0, -1.0), 3.6, 9.0, 2.0);
    t.rotation_deg = 45.0;
    let fp = t.footprint();
    assert!(fp.iter().all(|p| p.x < -0.6 || p.y < -0.6), "{fp:?}");
    let mut p = failed_building();
    p.towers.push(t);
    let shaft = calc(&p)["towers"][0]["shaft"].clone();
    assert_eq!(shaft["roof_failed"], true, "{shaft}");
}

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap_or(f64::NAN)
}
fn parts(v: &Value) -> f64 {
    v["parts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| f(&p["quantities"]["roof_area"]))
        .sum()
}
fn area(pts: &[Point2]) -> f64 {
    let n = pts.len();
    (0..n)
        .map(|i| pts[i].x * pts[(i + 1) % n].y - pts[(i + 1) % n].x * pts[i].y)
        .sum::<f64>()
        .abs()
        / 2.0
}

/// Every valid shaft, whatever its shape, size and turn: the cut's outline
/// has the plan area, is convex as the core reads it, has no side under the
/// minimum unless the shaft itself is that small, and at most 32 corners.
#[test]
fn r20_every_valid_shafts_cut_is_a_polygon_the_core_takes() {
    let mut bad = Vec::new();
    let mut n = 0;
    for &w in &[
        0.1, 0.12, 0.15, 0.19, 0.2, 0.25, 0.3, 0.5, 1.0, 2.0, 4.36, 10.0, 100.0,
    ] {
        for &rot in &[0.0, 10.0, 33.0, 45.0, 81.01] {
            let mut shapes: Vec<(String, Tower)> = Vec::new();
            for &d in &[0.1, 0.3, 1.0, 7.0] {
                for &cr in &[0.0, 0.01, 0.05, 0.3] {
                    let mut t = Tower::pyramid(Point2::new(15.0, 3.0), w, 14.0, 1.5);
                    t.depth = Some(d);
                    t.corner_radius = cr;
                    t.rotation_deg = rot;
                    shapes.push((format!("rect {w}x{d} r{cr} rot{rot}"), t));
                }
            }
            for &s in &[3u32, 5, 6, 8, 12, 17, 32, 64] {
                for &cr in &[0.0, 0.01, 0.05] {
                    let mut t = Tower::pyramid(Point2::new(15.0, 3.0), w, 14.0, 1.5);
                    t.shape = TowerShape::Polygon;
                    t.sides = s;
                    t.corner_radius = cr;
                    t.rotation_deg = rot;
                    shapes.push((format!("{s}-gon {w} r{cr} rot{rot}"), t));
                }
            }
            let mut c = Tower::pyramid(Point2::new(15.0, 3.0), w, 14.0, 1.5);
            c.shape = TowerShape::Circle;
            c.rotation_deg = rot;
            shapes.push((format!("circle {w} rot{rot}"), c));
            for (name, t) in shapes {
                if !t.problems().is_empty() {
                    continue;
                }
                n += 1;
                let fp = t.footprint();
                let plan = t.calculate().unwrap().base_area;
                let m = fp.len();
                let shortest = (0..m)
                    .map(|i| fp[i].distance(fp[(i + 1) % m]))
                    .fold(f64::INFINITY, f64::min);
                let mut why = Vec::new();
                if !(3..=32).contains(&m) {
                    why.push(format!("{m} corners"));
                }
                if (area(&fp) - plan).abs() > 1e-6 * plan.max(1.0) {
                    why.push(format!("area {} vs plan {plan}", area(&fp)));
                }
                if !tectora_core::openings::dacad_convex(&fp) {
                    why.push("not convex as the core reads it".into());
                }
                if shortest < 0.02 {
                    why.push(format!(
                        "a side of {shortest:.4} m (the core merges points within 2 cm)"
                    ));
                }
                if !why.is_empty() {
                    bad.push(format!("{name}: {}", why.join("; ")));
                }
            }
        }
    }
    assert!(
        bad.is_empty(),
        "{} of {n} shafts:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

/// The same shafts on a roof: the building keeps its roof and loses the
/// shaft's plan area over the cosine (only shafts wholly on one face).
#[test]
fn r21_small_and_odd_shafts_on_one_face_are_cut_by_their_plan_area() {
    let base = parts(&calc(&host()));
    let cos = PITCH.to_radians().cos();
    let mut bad = Vec::new();
    let mut cases: Vec<(String, Tower)> = Vec::new();
    for &w in &[0.1, 0.12, 0.15, 0.19] {
        for &s in &[3u32, 5, 8, 17, 64] {
            let mut t = Tower::pyramid(Point2::new(15.0, 3.0), w, 14.0, 1.5);
            t.shape = TowerShape::Polygon;
            t.sides = s;
            t.rotation_deg = 33.0;
            cases.push((format!("{s}-gon {w}"), t));
        }
        let mut c = Tower::pyramid(Point2::new(15.0, 3.0), w, 14.0, 1.5);
        c.shape = TowerShape::Circle;
        cases.push((format!("circle {w}"), c));
        let mut r = Tower::pyramid(Point2::new(15.0, 3.0), 3.0, 14.0, 1.5);
        r.depth = Some(w);
        r.rotation_deg = 33.0;
        cases.push((format!("rect 3x{w} turned"), r));
    }
    for &(w, d, cr) in &[(2.0, 1.0, 0.45), (1.0, 3.0, 0.2), (0.4, 0.4, 0.19)] {
        let mut r = Tower::pyramid(Point2::new(15.0, 3.0), w, 14.0, 1.5);
        r.depth = Some(d);
        r.corner_radius = cr;
        r.rotation_deg = 17.0;
        cases.push((format!("rect {w}x{d} r{cr} turned"), r));
    }
    for (name, t) in cases {
        if !t.problems().is_empty() {
            continue;
        }
        let plan = t.calculate().unwrap().base_area;
        let mut p = host();
        p.towers.push(t);
        let v = calc(&p);
        let cut = base - parts(&v);
        let hole = plan / cos;
        let err = v["parts"][0]["geometry"]["error"].clone();
        let warn = serde_json::to_string(&v["parts"][0]["geometry"]["warnings"]).unwrap();
        if !err.is_null()
            || (cut - hole).abs() > 0.005 * hole + 1e-3
            || warn.contains("not_calculated_by_core")
        {
            bad.push(format!(
                "{name}: cut {cut:.4} vs {hole:.4}, error {err}, warnings {warn}"
            ));
        }
    }
    assert!(bad.is_empty(), "\n{}", bad.join("\n"));
}

/// M281-M283: what is no arc stays no arc. A run of walls whose lengths
/// differ, whose corners turn differently, or turn by more than 60° is not
/// offered "Straighten" (which would delete the user's corners).
#[test]
fn r19_walls_that_are_no_arc_are_not_taken_for_one() {
    let part = |pts: &[(f64, f64)]| support::uniform(pts, 30.0, 3.0, 0.0).parts[0].clone();
    // A bay of three walls of unequal length, equal 30° turns.
    let step = |a: f64, l: f64, p: (f64, f64)| {
        (
            p.0 + l * a.to_radians().cos(),
            p.1 + l * a.to_radians().sin(),
        )
    };
    let chain = |lens: &[f64], turns: &[f64]| -> Vec<(f64, f64)> {
        // From (20, 0) heading up (90°), turning left by `turns` between walls; closed by a big box to the left.
        let mut pts = vec![(0.0, 0.0), (20.0, 0.0)];
        let (mut dir, mut p) = (90.0, (20.0, 0.0));
        for (k, l) in lens.iter().enumerate() {
            p = step(dir, *l, p);
            pts.push(p);
            if k < turns.len() {
                dir += turns[k];
            }
        }
        pts.push((0.0, p.1.max(12.0) + 5.0));
        pts
    };
    // The real thing first: four walls of 2 m turning 20° each is an arc.
    let arc = part(&chain(&[2.0, 2.0, 2.0, 2.0], &[20.0, 20.0, 20.0]));
    assert!(
        arcs::arc_run(&arc, 2).is_some(),
        "four equal walls turning alike"
    );
    let uneven = part(&chain(&[2.0, 2.6, 2.0, 2.9], &[20.0, 20.0, 20.0]));
    for w in 1..=4 {
        assert_eq!(arcs::arc_run(&uneven, w), None, "unequal lengths, wall {w}");
    }
    let wobble = part(&chain(&[2.0, 2.0, 2.0, 2.0], &[20.0, 35.0, 8.0]));
    for w in 1..=4 {
        assert_eq!(arcs::arc_run(&wobble, w), None, "unequal turns, wall {w}");
    }
    let zigzag = part(&chain(&[2.0, 2.0, 2.0, 2.0], &[25.0, -25.0, 25.0]));
    for w in 1..=4 {
        assert_eq!(arcs::arc_run(&zigzag, w), None, "a zigzag, wall {w}");
    }
    let sharp = part(&chain(&[2.0, 2.0, 2.0], &[70.0, 70.0]));
    for w in 1..=3 {
        assert_eq!(arcs::arc_run(&sharp, w), None, "70° corners, wall {w}");
    }
}

/// Towers on each other, and a tower on a chimney: the overlap is warned.
#[test]
fn r22_shafts_on_each_other_or_on_a_chimney_carry_the_overlap_warning() {
    let kinds = |p: &tectora_core::Project| -> String {
        let v = calc(p);
        serde_json::to_string(&v["parts"][0]["geometry"]["warnings"]).unwrap()
    };
    let mut p = host();
    p.towers
        .push(Tower::pyramid(Point2::new(15.0, 3.0), 2.0, 14.0, 1.5));
    assert!(!kinds(&p).contains("objects_overlap"), "one tower alone");
    p.towers
        .push(Tower::pyramid(Point2::new(16.0, 3.0), 2.0, 14.0, 1.5));
    assert!(
        kinds(&p).contains("objects_overlap"),
        "two shafts 1 m apart, 2 m wide: {}",
        kinds(&p)
    );
    let mut c = host();
    c.towers
        .push(Tower::pyramid(Point2::new(15.0, 3.0), 2.0, 14.0, 1.5));
    c.chimneys.push(tectora_core::model::Chimney {
        corners: [(14.5, 2.5), (15.5, 2.5), (15.5, 3.5), (14.5, 3.5)]
            .map(|(x, y)| Point2::new(x, y))
            .to_vec(),
        height: 10.0,
    });
    assert!(
        kinds(&c).contains("objects_overlap"),
        "a chimney inside a shaft: {}",
        kinds(&c)
    );
    // A tower with problems is cut out of nothing and overlaps nothing.
    let mut b = host();
    b.towers
        .push(Tower::pyramid(Point2::new(15.0, 3.0), 2.0, 14.0, 1.5));
    let mut broken = Tower::pyramid(Point2::new(15.0, 3.0), 2.0, 14.0, 1.5);
    broken.tiers.clear();
    b.towers.push(broken);
    assert!(!kinds(&b).contains("objects_overlap"), "{}", kinds(&b));
}

/// M135 / M137: an arc that makes the outline cross itself is refused with
/// a reason and the job stays as it was (not only "the editor and MCP
/// agree"); a problem the building had before blocks no arc elsewhere.
#[test]
fn r23_an_arc_that_crosses_the_outline_is_refused_with_its_reason() {
    use tectora_core::commands::{ArcEdit, arc_edit};
    let p = rect(
        10.0,
        8.0,
        [
            (30.0, 3.0, 0.0),
            (90.0, 3.0, 0.0),
            (30.0, 3.0, 0.0),
            (90.0, 3.0, 0.0),
        ],
    );
    let once = arc_edit(
        &p,
        0,
        ArcEdit::BendWall {
            index: 0,
            bulge: -5.0,
            segments: None,
        },
    )
    .expect("one inward arc fits");
    let top = once.parts[0].edges.len() - 2;
    let e = arc_edit(
        &once,
        0,
        ArcEdit::BendWall {
            index: top,
            bulge: -5.0,
            segments: None,
        },
    )
    .unwrap_err();
    assert_eq!(e.code, "refused");
    eprintln!("refusal: {}", e.detail);
    assert!(
        e.detail.ends_with("(arc_invalid)") || e.detail.ends_with("(arc_touches)"),
        "{}",
        e.detail
    );
    assert!(
        arc_edit(
            &once,
            0,
            ArcEdit::BendWall {
                index: top,
                bulge: -1.0,
                segments: None
            }
        )
        .is_ok()
    );
    assert!(arcs::outline_spoiled(&once, &once).is_none());
    // outline_spoiled on its own: an outline that crosses itself after, not before.
    let mut crossed = p.clone();
    crossed.parts[0].edges.swap(1, 2);
    let (a, b) = (
        crossed.parts[0].edges[1].start,
        crossed.parts[0].edges[2].start,
    );
    crossed.parts[0].edges[1].start = b;
    crossed.parts[0].edges[2].start = a;
    let mut bow = p.clone();
    bow.parts[0].edges[1].start = Point2::new(10.0, 9.0);
    bow.parts[0].edges[2].start = Point2::new(10.0, 1.0);
    let why = arcs::outline_spoiled(&p, &bow);
    assert!(
        why.as_deref().is_some_and(|w| w.ends_with("(arc_invalid)")),
        "{why:?}"
    );
    // The same broken outline before and after: not the arc's doing.
    assert!(arcs::outline_spoiled(&bow, &bow).is_none());
    let _ = crossed;
}

/// M162: a tower label with a line break is refused on a write, as every
/// other label is; one the job already had stays.
#[test]
fn r24_a_tower_label_with_a_line_break_is_refused_on_a_write() {
    let before = host();
    let mut after = host();
    let mut t = Tower::pyramid(Point2::new(15.0, 3.0), 2.0, 14.0, 1.5);
    t.label = "a\nb".into();
    after.towers.push(t);
    assert!(
        tectora_core::edit::check_texts_change(Some(&before), &after).is_err(),
        "a new label with a line break"
    );
    assert!(tectora_core::edit::check_texts_change(Some(&after), &after).is_ok());
}

/// M176, M183: the breakdown keeps a rectangle's second pitch, and the
/// line details give a tower's hip its slope.
#[test]
fn r25_the_breakdown_and_line_details_keep_a_towers_pitches() {
    let mut p = host();
    let mut t = Tower::pyramid(Point2::new(100.0, 100.0), 3.0, 9.0, 3.0);
    t.shape = TowerShape::Rectangle;
    t.depth = Some(2.0);
    p.towers.push(t.clone());
    let r = t.calculate().unwrap();
    let b = tectora_core::breakdown::breakdown(&p);
    let d = b.towers[0].tiers[0]
        .depth_sides
        .expect("both pitches in the breakdown");
    assert!((d.pitch_deg - r.tiers[0].depth_sides.unwrap().pitch_deg).abs() < 1e-9);
    let v = serde_json::to_value(tectora_core::lineinfo::tower_line_details(&p)).unwrap();
    let s = v.to_string();
    // A hip of the 3 × 2 pyramid: plan √(1.5² + 1²), rise 3.
    let slope = (3.0f64).atan2((1.5f64 * 1.5 + 1.0).sqrt()).to_degrees();
    let hips: Vec<&Value> = v
        .as_array()
        .map(|a| a.iter().filter(|l| l["code"] == "G").collect())
        .unwrap_or_default();
    assert!(!hips.is_empty(), "{}", &s[..s.len().min(600)]);
    for h in hips {
        let got = h["slope_deg"].as_f64().unwrap_or(f64::NAN);
        assert!((got - slope).abs() < 0.01, "{got} vs {slope}: {h}");
    }
}
