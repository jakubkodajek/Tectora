//! Towers and arcs, audit round 3 (2026-11-28-towers-r3-2f87935): the
//! tower overhang on every side, its bound on steep tiers, its tangent on
//! curved tiers, the lines per material, the eave under the roof around,
//! the height and delete operations with towers, the tower limits' codes,
//! arcs found and straightened back to their corner, the export's row
//! names in the breakdown. Checked against closed forms; towers have no
//! DACAD counterpart (ADR 0008).

mod support;

use serde_json::Value;
use std::f64::consts::PI;
use support::rect;
use tectora_core::edit::arcs;
use tectora_core::geom::Point2;
use tectora_core::report::calc;
use tectora_core::tower::{Tower, TowerShape, TowerTier};

fn tier(rise: f64, top_width: f64, bulge: f64, material: u8) -> TowerTier {
    TowerTier {
        rise,
        top_width,
        bulge,
        material,
    }
}

fn rect_tower(width: f64, depth: f64, rise: f64, overhang: f64) -> Tower {
    let mut t = Tower::pyramid(Point2::new(0.0, 0.0), width, 10.0, rise);
    t.depth = Some(depth);
    t.overhang = overhang;
    t
}

fn codes(t: &Tower) -> Vec<&'static str> {
    t.problems().into_iter().map(|p| p.0).collect()
}

/// Audit r3 MATH-001: a rectangle's overhang is the typed distance on all
/// four sides, each side's plane running on at its own pitch (the eave of
/// the steeper sides lower, as on a hip roof with two pitches), so the
/// same tower entered as 2 × 6 or 6 × 2 gives one roof. Closed form: the
/// lowest of the four planes over the eave rectangle 3 × 7.
#[test]
fn a_rectangular_tower_overhang_is_the_typed_distance_on_every_side() {
    let a = rect_tower(2.0, 6.0, 3.0, 0.5).calculate().unwrap();
    let b = rect_tower(6.0, 2.0, 3.0, 0.5).calculate().unwrap();
    // Planes z = 3 (1 - |x|) and z = 3 - |y| over [-1.5, 1.5] × [-3.5, 3.5]:
    // in a quarter the steep plane holds y < 3x.
    let steep = 49.0 / 24.0 + 3.5 / 3.0;
    let flat = 1.5 * 3.5 - steep;
    let exact = 4.0 * (steep * 10f64.sqrt() + flat * 2f64.sqrt());
    for r in [&a, &b] {
        assert!(
            (r.roof_area() - exact).abs() < 1e-6,
            "{} vs {exact}",
            r.roof_area()
        );
        // The eave: the rectangle 3 × 7, and at each corner the steep
        // plane's eave falls 1 m along the gentle side's line over 1/3 m.
        let t: f64 = r
            .lines
            .iter()
            .filter(|l| l.code == "T")
            .map(|l| l.length)
            .sum();
        let want_t = 20.0 + 4.0 * ((1.0f64 / 9.0 + 1.0).sqrt() - 1.0 / 3.0);
        assert!((t - want_t).abs() < 1e-6, "T {t} vs {want_t}");
        assert!((r.tiers[0].bottom_perimeter - 20.0).abs() < 1e-6);
        // The lowest eave is the steep sides': 0.5 · 3 below the base.
        assert!((r.tiers[0].bottom_height - 8.5).abs() < 1e-9);
        // Each hip: up from the shaft's corner to the top, and down to
        // where it meets the gentle side's eave, (1 + 1/6, 3.5) at 9.5 m.
        let hip = 19f64.sqrt() + (1.0f64 / 36.0 + 0.25 + 0.25).sqrt();
        let g: Vec<f64> = r
            .lines
            .iter()
            .filter(|l| l.code == "G")
            .map(|l| l.length)
            .collect();
        assert_eq!(g.len(), 4);
        assert!(g.iter().all(|l| (l - hip).abs() < 1e-6), "{g:?} vs {hip}");
    }
    assert!((a.roof_area() - b.roof_area()).abs() < 1e-9);
}

/// Audit r3 MATH-001 "also seen": rounded corners are pushed out by the
/// overhang (radius + overhang), not scaled. Closed form of the band: the
/// straight sides' strips and, at each corner, the surface swept by the
/// normal of the shaft's outline at the slope of the roof over it.
#[test]
fn rounded_corners_of_a_tower_are_pushed_out_by_the_overhang() {
    let (w, r0, o, rise) = (4.0, 0.5, 0.5, 3.0);
    let mut t = Tower::pyramid(Point2::new(0.0, 0.0), w, 10.0, rise);
    t.corner_radius = r0;
    let bare = t.calculate().unwrap();
    t.overhang = o;
    let with = t.calculate().unwrap();
    // The eave outline: four straight sides of 3 m and a circle of 1 m.
    let plan = 4.0 * (w - 2.0 * r0) + 2.0 * PI * (r0 + o);
    assert!(
        (with.tiers[0].bottom_perimeter - plan).abs() < 1e-3,
        "{} vs {plan}",
        with.tiers[0].bottom_perimeter
    );
    // Slope at the shaft over a point whose tangent lies `d` from the axis:
    // g · (w/2) / d with g = rise / (w/2).
    let gh = rise;
    let straight = 4.0 * (w - 2.0 * r0) * o * (1.0 + (gh / (w / 2.0)).powi(2)).sqrt();
    let c = w / 2.0 - r0;
    let (n, m) = (4000, 200);
    let mut corner = 0.0;
    let mut eave_corner = 0.0;
    for i in 0..n {
        let phi = (i as f64 + 0.5) / n as f64 * PI / 2.0;
        let d = c * (phi.cos() + phi.sin()) + r0;
        let s = gh / d;
        let ds = -gh * c * (phi.cos() - phi.sin()) / (d * d) / r0; // per m of arc
        let dsig = r0 * PI / 2.0 / n as f64;
        let kappa = 1.0 / r0;
        for j in 0..m {
            let tt = (j as f64 + 0.5) / m as f64 * o;
            let f = ((tt * ds).powi(2) + (1.0 + tt * kappa).powi(2) * (1.0 + s * s)).sqrt();
            corner += f * dsig * o / m as f64;
        }
        eave_corner += ((1.0 + o * kappa).powi(2) + (o * ds).powi(2)).sqrt() * dsig;
    }
    let band = straight + 4.0 * corner;
    let got = with.roof_area() - bare.roof_area();
    assert!((got - band).abs() < 2e-3, "{got} vs {band}");
    let t_len: f64 = with
        .lines
        .iter()
        .filter(|l| l.code == "T")
        .map(|l| l.length)
        .sum();
    let want = 4.0 * (w - 2.0 * r0) + 4.0 * eave_corner;
    assert!((t_len - want).abs() < 2e-3, "T {t_len} vs {want}");
}

/// The dome of the tests below: a round tower 4 m across, base 10 m, one
/// tier of `rise` to a point curved outward by `bulge`.
fn dome(rise: f64, bulge: f64, overhang: f64) -> Tower {
    let mut t = Tower::pyramid(Point2::new(0.0, 0.0), 4.0, 10.0, rise);
    t.shape = TowerShape::Circle;
    t.tiers = vec![tier(rise, 0.0, bulge, 2)];
    t.overhang = overhang;
    t
}

/// The slope (tan of the pitch) of a curved tier from (h0, z0) to (h1, z1)
/// at its start, from the circle through it: the tangent is square to the
/// radius.
fn tangent_slope(h0: f64, z0: f64, h1: f64, z1: f64, bulge: f64) -> f64 {
    let (dh, dz) = (h1 - h0, z1 - z0);
    let l = dh.hypot(dz);
    let (nh, nz) = (dz / l, -dh / l);
    let r = (l * l / 4.0 + bulge * bulge) / (2.0 * bulge.abs());
    let apex = ((h0 + h1) / 2.0 + nh * bulge, (z0 + z1) / 2.0 + nz * bulge);
    let c = (
        apex.0 - nh * r * bulge.signum(),
        apex.1 - nz * r * bulge.signum(),
    );
    // Radius (h0 - c.0, z0 - c.1); the tangent is (-(z0 - c.1), h0 - c.0) or
    // its opposite: the slope is the ratio.
    ((h0 - c.0) / (z0 - c.1)).abs()
}

/// Audit r3 MATH-003: the overhang of a curved first tier runs on along the
/// tier's tangent at the shaft: the band is a frustum of the tangent's
/// slope from radius 2 to 2 + overhang.
#[test]
fn a_curved_first_tier_carries_its_tangent_on_to_the_eave() {
    for (rise, bulge, o) in [(1.5, 0.3, 0.3), (1.5, -0.3, 0.5), (3.0, 0.2, 0.4)] {
        let bare = dome(rise, bulge, 0.0).calculate().unwrap();
        let t = dome(rise, bulge, o);
        assert!(t.problems().is_empty(), "{:?}", t.problems());
        let with = t.calculate().unwrap();
        let g = tangent_slope(2.0, 10.0, 0.0, 10.0 + rise, bulge);
        let band = PI * (2.0 + 2.0 + o) * o * (1.0 + g * g).sqrt();
        let got = with.roof_area() - bare.roof_area();
        assert!(
            (got - band).abs() < 2e-3,
            "rise {rise} bulge {bulge}: {got} vs {band}"
        );
        assert!(
            (with.tiers[0].bottom_height - (10.0 - o * g)).abs() < 1e-3,
            "{} vs {}",
            with.tiers[0].bottom_height,
            10.0 - o * g
        );
        // The tier's pitch is its own chord's, with or without overhang
        // (what a typed pitch sets).
        assert!((with.tiers[0].pitch_deg - bare.tiers[0].pitch_deg).abs() < 1e-9);
    }
}

/// Audit r3 MATH-002: an overhang under a first tier that starts steeper
/// than 75° at the shaft would hang a skirt metres down: refused with
/// `tower_overhang` and the pitch named; the same tower without the
/// overhang, and one starting at 75° or less, calculate.
#[test]
fn an_overhang_under_a_steep_first_tier_is_refused_with_its_pitch() {
    // Dome of rise 1.9 and bulge 0.5: 83.4° at the shaft (the audit's
    // case: its eave 2.6 m below the base, the area doubled).
    let g = tangent_slope(2.0, 10.0, 0.0, 11.9, 0.5);
    assert!(g.atan().to_degrees() > 75.0);
    let steep = dome(1.9, 0.5, 0.3);
    assert_eq!(codes(&steep), ["tower_overhang"]);
    let msg = &steep.problems()[0].1;
    assert!(msg.contains("75"), "{msg}");
    assert!(
        msg.contains(&format!("{:.1}°", g.atan().to_degrees())),
        "{msg}"
    );
    assert!(steep.calculate().is_none());
    assert!(codes(&dome(1.9, 0.5, 0.0)).is_empty());
    // A straight spire of 84°, and a rectangle whose short sides are
    // steeper than 75° though its long ones are not.
    let mut spire = Tower::pyramid(Point2::new(0.0, 0.0), 2.0, 10.0, 10.0);
    spire.overhang = 0.2;
    assert_eq!(codes(&spire), ["tower_overhang"]);
    assert_eq!(codes(&rect_tower(2.0, 6.0, 4.0, 0.3)), ["tower_overhang"]);
    assert!(codes(&rect_tower(2.0, 6.0, 3.0, 0.3)).is_empty());
    // 74° calculates.
    let mut ok = Tower::pyramid(Point2::new(0.0, 0.0), 2.0, 10.0, 74f64.to_radians().tan());
    ok.overhang = 0.3;
    assert!(codes(&ok).is_empty(), "{:?}", ok.problems());
    // The eave under the ground names its height.
    let mut low = Tower::pyramid(Point2::new(0.0, 0.0), 2.0, 0.5, 2.0);
    low.overhang = 0.5;
    let p = low.problems();
    assert_eq!(p[0].0, "tower_overhang");
    assert!(p[0].1.contains("below the ground"), "{}", p[0].1);
}

/// Audit r3 MATH-004: the pitch break between two tiers of different
/// materials counts for both, whatever the height it lies at (a rise set
/// by a pitch is not a whole millimetre).
#[test]
fn a_pitch_break_counts_for_both_materials_at_any_height() {
    for (base, rise) in [(10.0, 1.0), (10.0, 1.2345), (10.0004, 1.0), (10.0, 1.1918)] {
        let mut p = tectora_core::Project::new("t");
        let mut t = Tower::pyramid(Point2::new(0.0, 0.0), 4.0, base, 1.0);
        t.tiers = vec![tier(rise, 2.0, 0.0, 2), tier(2.0, 0.0, 0.0, 3)];
        p.towers.push(t);
        let lines = tectora_core::tower::lines_by_material(&p);
        let c = |m: u8| -> f64 {
            lines
                .iter()
                .filter(|l| l.0 == m && l.1 == "c")
                .map(|l| l.2)
                .sum()
        };
        assert!((c(2) - 8.0).abs() < 1e-9, "{base} {rise}: {lines:?}");
        assert!((c(3) - 8.0).abs() < 1e-9, "{base} {rise}: {lines:?}");
    }
}

fn warnings(v: &Value, part: usize) -> Vec<Value> {
    v["parts"][part]["geometry"]["warnings"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// Audit r3 MATH-005: a tower whose base is above the roof but whose eave
/// (carried down and out by the overhang) lies under it gets a warning on
/// that building naming the tower; without the overhang it has none.
#[test]
fn a_tower_eave_under_the_roof_around_it_is_a_warning() {
    // Hip roof 30 × 12, 40°, eave 8: ridge at 13.03 m from x 6 to 24.
    let mut p = rect(30.0, 12.0, [(40.0, 8.0, 0.0); 4]);
    let mut t = Tower::pyramid(Point2::new(15.0, 6.0), 4.0, 13.3, 3.0);
    t.overhang = 1.0;
    p.towers.push(t);
    let v = calc(&p);
    let w = warnings(&v, 0);
    let eave: Vec<&Value> = w
        .iter()
        .filter(|w| w["kind"] == "tower_eave_below_roof")
        .collect();
    assert_eq!(eave.len(), 1, "{w:?}");
    assert_eq!(eave[0]["towers"], serde_json::json!([0]));
    // The eave at 11.8 m, the ridge over it at 13.03 m.
    let value = eave[0]["value"].as_f64().unwrap();
    assert!(
        (value - (8.0 + 6.0 * 40f64.to_radians().tan() - 11.8)).abs() < 0.01,
        "{value}"
    );
    assert!(
        !w.iter().any(|w| w["kind"] == "tower_base_below_roof"),
        "{w:?}"
    );
    assert_eq!(
        v["towers"][0]["shaft"]["eave_below_roof"], true,
        "{}",
        v["towers"][0]
    );
    p.towers[0].overhang = 0.0;
    let v = calc(&p);
    assert!(
        warnings(&v, 0)
            .iter()
            .all(|w| w["kind"] != "tower_eave_below_roof"),
        "{:?}",
        warnings(&v, 0)
    );
    assert_eq!(v["towers"][0]["shaft"]["eave_below_roof"], false);
}

/// Audit r3 ARCH-002: a building height that puts a tower's eave under the
/// ground is refused by the operation itself (the app's command and MCP
/// answer alike), with the tower's code, and changes nothing.
#[test]
fn a_height_that_puts_a_tower_eave_below_ground_is_refused() {
    let mut p = rect(12.0, 8.0, [(35.0, 3.0, 0.0); 4]);
    let mut t = Tower::pyramid(Point2::new(6.0, 4.0), 2.0, 6.0, 2.5);
    t.shape = TowerShape::Circle;
    t.overhang = 2.0;
    p.towers.push(t);
    assert!(p.problems().is_empty(), "{:?}", p.problems());
    let before = p.clone();
    let e = tectora_core::edit::height::set_part_height(&mut p, 0, 1.0).unwrap_err();
    assert!(e.contains("tower index 0"), "{e}");
    assert!(e.ends_with("(tower_overhang)"), "{e}");
    assert_eq!(p, before);
    let e = tectora_core::edit::height::set_part_eave_height(&mut p, 0, 1.0).unwrap_err();
    assert!(e.ends_with("(tower_overhang)"), "{e}");
    assert_eq!(p, before);
    // A lowering that keeps the eave above ground carries the tower.
    assert!(tectora_core::edit::height::set_part_height(&mut p, 0, 2.5).unwrap());
    assert!((p.towers[0].base_height - 5.5).abs() < 1e-9);
}

/// Audit r3 ARCH-003: the core says which towers deleting a building
/// would remove, the ones `delete_part` then removes, without changing the
/// job (the editor's question names exactly those).
#[test]
fn the_towers_a_delete_would_remove_come_from_the_core() {
    let mut p = rect(12.0, 8.0, [(35.0, 3.0, 0.0); 4]);
    let mut b = rect(10.0, 8.0, [(35.0, 3.0, 0.0); 4]).parts.remove(0);
    for e in &mut b.edges {
        e.start.x += 8.0;
    }
    b.name = "B".into();
    p.parts.push(b);
    let mut both = Tower::pyramid(Point2::new(10.0, 4.0), 1.0, 9.0, 1.0);
    both.label = "in both".into();
    let mut only = Tower::pyramid(Point2::new(3.0, 4.0), 1.0, 9.0, 1.0);
    only.label = "A only".into();
    p.towers.push(both);
    p.towers.push(only);
    let before = p.clone();
    let asked = tectora_core::commands::towers_removed_by(&p, 0).unwrap();
    assert_eq!(asked["towers_removed"], serde_json::json!([1]));
    assert_eq!(p, before);
    let done = tectora_core::commands::delete_part(&p, 0).unwrap();
    assert_eq!(done["towers_removed"], asked["towers_removed"]);
    let req: tectora_core::commands::GeometryRequest = serde_json::from_value(serde_json::json!({
        "op": "towers_removed_by", "project": p, "part": 0
    }))
    .unwrap();
    let v = tectora_core::commands::item_geometry(&req).unwrap();
    assert_eq!(v["towers_removed"], serde_json::json!([1]));
    let e = tectora_core::commands::towers_removed_by(&p, 5).unwrap_err();
    assert_eq!(e.code, "refused");
}

/// Audit r3 ERR-003: the calculation's limits name the towers' own codes,
/// as a save does, so the editor says "at most 50 towers" and not that the
/// roof could not be calculated.
#[test]
fn the_tower_limits_of_a_calculation_end_with_their_codes() {
    let mut p = rect(30.0, 30.0, [(35.0, 3.0, 0.0); 4]);
    for k in 0..51 {
        p.towers.push(Tower::pyramid(
            Point2::new(1.0 + k as f64 * 0.5, 15.0),
            0.3,
            9.0,
            0.3,
        ));
    }
    let e = p.check_input().unwrap_err();
    assert_eq!(e.code, "too_many");
    assert!(e.message.ends_with("(tower_limit)"), "{}", e.message);
    let e = tectora_core::commands::calc(&p).unwrap_err();
    assert!(e.detail.ends_with("(tower_limit)"), "{}", e.detail);
    p.towers.truncate(6);
    for t in &mut p.towers {
        t.tiers = (0..21).map(|_| tier(0.01, 0.3, 0.0, 2)).collect();
    }
    let e = p.check_input().unwrap_err();
    assert!(e.message.ends_with("(tower_tiers_limit)"), "{}", e.message);
}

/// Audit r3 / errors To check 4: a pitch so slight its rise rounds to 0 is
/// refused, not saved as a flat tier.
#[test]
fn a_pitch_whose_rise_rounds_to_nothing_is_refused() {
    let e = tectora_core::edit::towers::rise_for_pitch(2.0, 0.0, 0.0001).unwrap_err();
    assert!(e.ends_with("(tower_tier_pitch)"), "{e}");
    let e = tectora_core::edit::towers::rise_for_pitch(2.0, 4.0, 179.9999).unwrap_err();
    assert!(e.ends_with("(tower_tier_pitch)"), "{e}");
    assert_eq!(
        tectora_core::edit::towers::rise_for_pitch(2.0, 0.0, 45.0),
        Ok(1.0)
    );
}

fn part_of(w: f64, d: f64) -> tectora_core::RoofPart {
    rect(w, d, [(30.0, 3.0, 0.0); 4]).parts.remove(0)
}

/// Audit r3 ARCH-001: a semicircle of three walls (corners of 60°, a few
/// thousandths over or under once stored) is found as an arc and
/// straightened on every wall length.
#[test]
fn a_three_wall_semicircle_is_found_and_straightened_on_any_wall() {
    for l in 4..=15 {
        for sign in [1.0, -1.0] {
            let mut part = part_of(l as f64, 8.0);
            let original = part.clone();
            let segs = arcs::bend_wall(&mut part, 0, sign * l as f64 / 2.0, Some(3)).unwrap();
            assert_eq!(segs, 3);
            for wall in 0..3 {
                let run = arcs::arc_run(&part, wall)
                    .unwrap_or_else(|| panic!("wall {l} {sign}: wall {wall} not found"));
                assert_eq!((run.start, run.walls), (0, 3), "{l} {sign}");
            }
            let mut flat = part.clone();
            arcs::straighten(&mut flat, 1).unwrap();
            assert_eq!(flat.outline(), original.outline(), "{l} {sign}");
        }
    }
}

fn close_outline(a: &[Point2], b: &[Point2]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(p, q)| p.distance(*q) < 1e-3)
}

/// Audit r3 UX-004: a rounded corner straightened is the sharp corner
/// again (its two walls run on to where they meet), not a chamfer; and
/// its radius can be changed (`change_corner_radius`).
#[test]
fn a_rounded_corner_straightened_is_the_corner_again() {
    let square = part_of(10.0, 10.0);
    for (corner, radius) in [(2, 2.5), (0, 1.0), (3, 4.0), (1, 0.5)] {
        let mut part = square.clone();
        let segs = arcs::round_corner(&mut part, corner, radius, None).unwrap();
        for wall in [corner, corner + segs / 2, corner + segs - 1] {
            let mut flat = part.clone();
            let at = arcs::straighten(&mut flat, wall).unwrap();
            assert!(
                close_outline(&flat.outline(), &square.outline()),
                "corner {corner} r {radius} wall {wall}: {:?}",
                flat.outline()
            );
            assert_eq!(at, corner);
            assert_eq!(flat.edges[corner].pitch_deg, square.edges[corner].pitch_deg);
        }
        // A new radius: as rounding the sharp corner with it.
        let mut again = part.clone();
        let (at, n) = arcs::change_corner_radius(&mut again, corner + 1, 3.0, None).unwrap();
        let mut direct = square.clone();
        let m = arcs::round_corner(&mut direct, corner, 3.0, None).unwrap();
        assert_eq!((at, n), (corner, m));
        assert!(close_outline(&again.outline(), &direct.outline()));
    }
    // A bent wall is no rounded corner: its radius is refused, nothing
    // changed; straightened it is its chord as before.
    let mut bent = square.clone();
    arcs::bend_wall(&mut bent, 0, 2.0, None).unwrap();
    let keep = bent.clone();
    let e = arcs::change_corner_radius(&mut bent, 1, 2.0, None).unwrap_err();
    assert!(e.ends_with(&format!("({})", arcs::ARC_NONE)), "{e}");
    assert_eq!(bent, keep);
    arcs::straighten(&mut bent, 1).unwrap();
    assert!(close_outline(&bent.outline(), &square.outline()));
    // A semicircle on a rectangle's end meets two parallel walls: its
    // chord, the end wall, comes back.
    let end = part_of(12.0, 6.0);
    let mut apse = end.clone();
    arcs::bend_wall(&mut apse, 1, 3.0, None).unwrap();
    arcs::straighten(&mut apse, 2).unwrap();
    assert!(close_outline(&apse.outline(), &end.outline()));
}

/// Audit r3 UX-004: `change_corner_radius` through the editor's command.
#[test]
fn the_corner_radius_changes_through_the_arc_command() {
    let mut p = rect(10.0, 10.0, [(30.0, 3.0, 0.0); 4]);
    arcs::round_corner(&mut p.parts[0], 2, 2.0, None).unwrap();
    let edit: tectora_core::commands::ArcEdit = serde_json::from_value(serde_json::json!({
        "op": "change_corner_radius", "index": 3, "radius": 3.0
    }))
    .unwrap();
    let next = tectora_core::commands::arc_edit(&p, 0, edit).unwrap();
    let mut direct = part_of(10.0, 10.0);
    arcs::round_corner(&mut direct, 2, 3.0, None).unwrap();
    assert!(close_outline(&next.parts[0].outline(), &direct.outline()));
}

/// Audit r3 UX-008: every tower tier of the breakdown carries the name of
/// its row in the export's list (`Z<roof>.<nn>`), and so does a flat top.
#[test]
fn the_breakdown_names_each_tower_tier_by_its_list_row() {
    let mut p = rect(12.0, 8.0, [(35.0, 3.0, 0.0); 4]);
    let b = {
        let mut q = rect(6.0, 6.0, [(35.0, 3.0, 0.0); 4]).parts.remove(0);
        for e in &mut q.edges {
            e.start.x += 20.0;
        }
        q
    };
    p.parts.push(b);
    let mut t = Tower::pyramid(Point2::new(6.0, 4.0), 2.0, 9.0, 1.0);
    t.tiers = vec![
        tier(1.0, 1.0, 0.0, 2),
        tier(1.0, 1.0, 0.0, 2),
        tier(1.0, 0.5, 0.0, 2),
    ];
    p.towers.push(t);
    let bd = serde_json::to_value(tectora_core::breakdown::breakdown(&p)).unwrap();
    let tiers = bd["towers"][0]["tiers"].as_array().unwrap().clone();
    let rows: Vec<Value> = tiers.iter().map(|t| t["list_row"].clone()).collect();
    // Two buildings: the tower is roof 3; its lantern tier is wall, no row.
    assert_eq!(
        rows,
        [
            serde_json::json!("Z3.01"),
            Value::Null,
            serde_json::json!("Z3.03")
        ]
    );
    assert_eq!(bd["towers"][0]["cap_list_row"], "Z3.04");
    let list = tectora_core::export::dacad_list(
        &p,
        &tectora_core::export::ListOptions::new("t", "01.01.2026 00:00"),
    );
    for name in ["Z3.01", "Z3.03", "Z3.04"] {
        assert!(list.contains(name), "{name} not in the list:\n{list}");
    }
}

/// Audit r3 TEST-007 (M263): the command's `reshape_arc` takes the wall
/// count asked for, not the default.
#[test]
fn reshape_through_the_command_keeps_the_wall_count_asked_for() {
    let mut p = rect(10.0, 8.0, [(30.0, 3.0, 0.0); 4]);
    arcs::bend_wall(&mut p.parts[0], 0, 2.0, Some(8)).unwrap();
    assert_eq!(p.parts[0].edges.len(), 11);
    let edit: tectora_core::commands::ArcEdit = serde_json::from_value(serde_json::json!({
        "op": "reshape_arc", "index": 1, "bulge": 1.0, "segments": 12
    }))
    .unwrap();
    let next = tectora_core::commands::arc_edit(&p, 0, edit).unwrap();
    assert_eq!(next.parts[0].edges.len(), 15);
    let run = arcs::arc_run(&next.parts[0], 3).unwrap();
    assert_eq!((run.walls, run.bulge), (12, 1.0));
}

/// Audit r3 UX-004: a rounded corner whose walls run over the end of the
/// wall list into its start is restored too.
#[test]
fn a_rounded_corner_across_the_first_wall_is_restored() {
    let square = part_of(12.0, 10.0);
    let mut part = square.clone();
    let segs = arcs::round_corner(&mut part, 0, 3.0, None).unwrap();
    part.edges.rotate_left(2);
    let n = part.edges.len();
    assert_eq!(n, 4 + segs);
    let at = arcs::straighten(&mut part, 1).unwrap();
    assert_eq!(part.edges.len(), 4);
    assert!(
        part.edges[at].start.distance(Point2::new(0.0, 0.0)) < 1e-3,
        "{:?}",
        part.outline()
    );
    let mut a = part.outline();
    let mut b = square.outline();
    let key = |p: &Point2| ((p.x * 1000.0).round() as i64, (p.y * 1000.0).round() as i64);
    a.sort_by_key(key);
    b.sort_by_key(key);
    assert!(close_outline(&a, &b), "{a:?}");
}
