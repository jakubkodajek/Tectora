//! Every place that shows a roof's figures shows the same figures: the
//! results (totals, by material, by line code), the calculation breakdown,
//! the price-sheet list (summary and text) and the stored result. And the
//! deductions the faces carry are the windows, chimneys and openings the
//! roof lists. Checked on the example roofs, on roofs Tectora calculates
//! itself where the standard calculation stops, and on a fixed set of
//! generated roofs.

mod support;

use std::collections::BTreeMap;

use support::edge;
use tectora_core::geom::Point2;
use tectora_core::model::{Chimney, HalfHip, PitchBreak, RoofWindow};
use tectora_core::openings::generate_project;
use tectora_core::{Project, RoofPart};

fn square(x: f64, y: f64, x2: f64, y2: f64) -> Chimney {
    Chimney {
        corners: vec![
            Point2::new(x, y),
            Point2::new(x2, y),
            Point2::new(x2, y2),
            Point2::new(x, y2),
        ],
        height: 10.2,
    }
}

fn one_building(edges: Vec<tectora_core::EdgeDef>) -> Project {
    let mut p = Project::new("consistency");
    p.parts.push(RoofPart {
        height: None,
        name: "A".into(),
        edges,
    });
    p
}

/// A hip roof whose pitch break keeps the pitch, with a half-hip on the
/// gable: the standard calculation stops and Tectora's sweep calculates it,
/// joining the faces of one plane. The window stays deducted after the join.
fn swept_with_window() -> Project {
    let mut e0 = edge(0.0, 0.0, 15.0, 6.0, 0.5);
    e0.breaks.push(PitchBreak {
        height: 7.31,
        pitch_deg: 15.0,
    });
    let mut e3 = edge(0.0, 10.8, 90.0, 6.0, 0.5);
    e3.half_hip = Some(HalfHip {
        height: 7.7,
        pitch_deg: 60.0,
    });
    let mut p = one_building(vec![
        e0,
        edge(10.18, 0.0, 15.0, 6.0, 0.5),
        edge(10.18, 10.8, 15.0, 6.0, 0.5),
        e3,
    ]);
    p.windows.push(RoofWindow {
        x: 2.88,
        y: 1.34,
        width: 0.78,
        height: 1.18,
    });
    p
}

/// A T-shaped house with two pitch breaks and two chimneys, which the
/// standard calculation stops on and the sweep calculates.
fn swept_with_chimneys() -> Project {
    let mut e1 = edge(11.71, 0.0, 22.0, 3.4, 0.0);
    e1.breaks.push(PitchBreak {
        height: 4.24,
        pitch_deg: 15.0,
    });
    let mut e4 = edge(7.67, 10.31, 22.0, 4.2, 0.0);
    e4.breaks.push(PitchBreak {
        height: 5.48,
        pitch_deg: 15.0,
    });
    let mut p = one_building(vec![
        edge(0.0, 0.0, 22.0, 4.2, 0.0),
        e1,
        edge(11.71, 6.23, 22.0, 4.2, 0.0),
        edge(7.67, 6.23, 22.0, 4.2, 0.0),
        e4,
        edge(4.04, 10.31, 22.0, 4.2, 0.9),
        edge(4.04, 6.23, 22.0, 4.2, 0.0),
        edge(0.0, 6.23, 22.0, 4.2, 0.0),
    ]);
    p.chimneys.push(square(3.27, 4.26, 4.1, 5.1));
    p.chimneys.push(square(6.1, 0.41, 7.07, 1.38));
    p
}

/// What the faces deduct equals what the roof lists as deducted (windows,
/// chimneys, openings, dormers and the join's other cuts).
fn deductions_add_up(name: &str, p: &Project) -> Vec<String> {
    let mut bad = Vec::new();
    for (k, g) in generate_project(p).iter().enumerate() {
        let faces: f64 = g.faces.iter().map(|f| f.deducted).sum();
        let listed: f64 = g.openings.iter().map(|o| o.area).sum::<f64>() + g.other_cuts;
        if (faces - listed).abs() > 2e-3 {
            bad.push(format!(
                "{name} part {k}: faces deduct {faces:.4} m2, the roof lists {listed:.4} m2"
            ));
        }
        for w in &g.window_cuts {
            if w.face >= g.faces.len() || g.faces[w.face].deducted + 1e-9 < w.width * w.height {
                bad.push(format!(
                    "{name} part {k}: a window of {:.4} m2 is booked on face {} which deducts {:?}",
                    w.width * w.height,
                    w.face,
                    g.faces.get(w.face).map(|f| f.deducted)
                ));
            }
        }
    }
    bad
}

/// The results, the breakdown, the price-sheet list and the stored result
/// give the same figures.
fn surfaces_agree(name: &str, p: &Project) -> Vec<String> {
    let mut bad = Vec::new();
    let mut check = |what: &str, a: f64, b: f64, tol: f64| {
        if (a - b).is_nan() || (a - b).abs() > tol {
            bad.push(format!("{name}: {what}: {a} vs {b}"));
        }
    };
    let geos = generate_project(p);
    let calc = tectora_core::report::calc_of(p, &geos);
    let totals = &calc["totals"];
    let area = totals["roof_area"].as_f64().unwrap();
    let faces: f64 = geos
        .iter()
        .map(|g| g.faces.iter().map(|f| f.area).sum::<f64>() + g.clad_area)
        .sum::<f64>()
        + p.manual_faces.iter().map(|f| f.area()).sum::<f64>();
    check("total vs faces", area, faces, 1e-6);
    let by_material: BTreeMap<u8, f64> = totals["area_by_material"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| (e[0].as_u64().unwrap() as u8, e[1].as_f64().unwrap()))
        .collect();
    check("materials vs total", by_material.values().sum(), area, 1e-6);
    let codes: BTreeMap<String, f64> = serde_json::from_value(calc["codes"].clone()).unwrap();
    let code = |c: &[&str]| -> f64 { c.iter().filter_map(|k| codes.get(*k)).sum() };
    let field = |k: &str| totals[k].as_f64().unwrap();
    for (k, c) in [
        ("eave_length", &["T"][..]),
        ("ridge_length", &["F"]),
        ("hip_length", &["G"]),
        ("valley_length", &["K"]),
        ("verge_length", &["OL", "OR", "OS"]),
        ("pult_length", &["P"]),
        ("knick_length", &["c"]),
        ("gorge_length", &["S"]),
    ] {
        check(k, field(k), code(c), 0.006 * c.len() as f64);
    }

    let b = tectora_core::breakdown::from_geometry(p, &geos);
    check("breakdown area", b.roof_area, area, 1e-6);
    check(
        "breakdown materials",
        b.materials.iter().map(|m| m.area).sum(),
        area,
        1e-6,
    );
    for g in &b.lines {
        check(
            &format!("breakdown {}", g.code),
            g.total,
            codes.get(&g.code).copied().unwrap_or(0.0),
            0.0051,
        );
    }
    for part in &b.parts {
        for f in &part.faces {
            check(
                &format!("breakdown {} face {}: gross - deducted", part.name, f.no),
                f.engine_gross.unwrap_or(f.gross) - f.deducted,
                f.area,
                6e-4,
            );
        }
    }

    let opts = tectora_core::export::ListOptions::new("t", "01.01.2026 00:00");
    let list = tectora_core::export::list_summary(p, &opts);
    let unpitched: f64 = p
        .manual_faces
        .iter()
        .filter(|f| !f.pitch_counts())
        .map(|f| f.area())
        .sum();
    check(
        "list faces net vs total",
        list.faces.iter().map(|f| f.area - f.deducted).sum(),
        area - unpitched,
        2e-3,
    );
    for m in &list.materials {
        check(
            &format!("list material {}", m.material),
            m.area,
            by_material.get(&m.material).copied().unwrap_or(0.0),
            2e-3,
        );
    }
    for l in &list.lines {
        check(
            &format!("list {}", l.code),
            l.length,
            codes.get(&l.code).copied().unwrap_or(0.0),
            0.0055,
        );
    }
    let snap = tectora_core::snapshot::take_of(&geos, p);
    check(
        "stored area",
        snap.roof_area,
        (area * 100.0).round() / 100.0,
        0.0051,
    );
    for (c, v) in &snap.lines_by_code {
        check(
            &format!("stored {c}"),
            *v,
            codes.get(c).copied().unwrap_or(0.0),
            0.0051,
        );
    }
    bad
}

#[test]
fn a_window_stays_deducted_when_the_sweep_joins_faces_of_one_plane() {
    let p = swept_with_window();
    let geos = generate_project(&p);
    assert!(geos[0].error.is_none());
    let deducted: f64 = geos[0].faces.iter().map(|f| f.deducted).sum();
    assert!(
        (deducted - 0.78 * 1.18).abs() < 1e-6,
        "faces deduct {deducted}"
    );
    let bad = [
        deductions_add_up("window", &p),
        surfaces_agree("window", &p),
    ]
    .concat();
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn chimneys_stay_deducted_when_the_sweep_joins_faces_of_one_plane() {
    let p = swept_with_chimneys();
    let bad = [
        deductions_add_up("chimneys", &p),
        surfaces_agree("chimneys", &p),
    ]
    .concat();
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn example_roofs_show_the_same_figures_everywhere() {
    let mut bad = Vec::new();
    for ex in tectora_core::examples::EXAMPLES {
        let p = ex.project();
        bad.extend(deductions_add_up(ex.id, &p));
        bad.extend(surfaces_agree(ex.id, &p));
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

// ------------------------------------------------------------ generated roofs

/// xorshift64, fixed seeds: the same roofs on every run.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        let mut r = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        r.next();
        r
    }
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, a: f64, b: f64) -> f64 {
        ((a + (b - a) * self.next()) * 100.0).round() / 100.0
    }
    fn pick(&mut self, v: &[f64]) -> f64 {
        v[((self.next() * v.len() as f64) as usize).min(v.len() - 1)]
    }
    fn chance(&mut self, p: f64) -> bool {
        self.next() < p
    }
}

/// A rectangle or an L with mixed walls (pitches, gables, half-hips,
/// pitch breaks, lowered eaves, a second material), chimneys and a window.
fn generated(seed: u64) -> Project {
    let mut r = Rng::new(seed);
    let (w, d) = (r.range(5.0, 16.0), r.range(4.0, 11.0));
    let pts: Vec<(f64, f64)> = if r.chance(0.5) {
        vec![(0.0, 0.0), (w, 0.0), (w, d), (0.0, d)]
    } else {
        let (a, b) = (r.range(w * 0.35, w * 0.65), r.range(d * 1.4, d * 2.0));
        vec![(0.0, 0.0), (w, 0.0), (w, d), (a, d), (a, b), (0.0, b)]
    };
    let pitch = r.pick(&[15.0, 22.0, 30.0, 40.0, 45.0, 50.0]);
    let eave = r.pick(&[2.8, 3.5, 4.2, 6.0]);
    let ov = r.pick(&[0.0, 0.3, 0.5, 0.8]);
    let mut edges = Vec::new();
    for (i, &(x, y)) in pts.iter().enumerate() {
        let mut e = edge(x, y, pitch, eave, ov);
        if r.chance(0.15) {
            e.pitch_deg = r.pick(&[25.0, 35.0, 55.0]);
        }
        if pts.len() == 4 && i % 2 == 1 && r.chance(0.5) {
            e.pitch_deg = 90.0;
            if r.chance(0.3) {
                e.half_hip = Some(HalfHip {
                    height: eave + r.range(1.0, 2.0),
                    pitch_deg: r.pick(&[45.0, 60.0]),
                });
            }
        }
        if r.chance(0.1) {
            e.eave_height = eave + r.pick(&[-0.8, 0.5]);
        }
        if e.pitch_deg < 89.0 && r.chance(0.12) {
            e.breaks.push(PitchBreak {
                height: e.eave_height + r.range(0.6, 1.4),
                pitch_deg: r.pick(&[15.0, 25.0, 60.0]),
            });
        }
        if r.chance(0.1) {
            e.material = 2;
        }
        edges.push(e);
    }
    let mut p = one_building(edges);
    p.parts[0].normalize_ccw();
    let inside = |x: f64, y: f64| {
        let n = pts.len();
        let mut c = false;
        for i in 0..n {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            if (a.1 > y) != (b.1 > y) && x < a.0 + (y - a.1) / (b.1 - a.1) * (b.0 - a.0) {
                c = !c;
            }
        }
        c
    };
    for _ in 0..2 {
        let (x, y, s) = (r.range(0.0, w), r.range(0.0, d), r.range(0.4, 1.0));
        if r.chance(0.4)
            && [(x, y), (x + s, y), (x, y + s), (x + s, y + s)]
                .iter()
                .all(|&(a, b)| inside(a, b))
        {
            p.chimneys.push(square(x, y, x + s, y + s));
        }
    }
    let (x, y) = (r.range(0.5, w - 1.5), r.range(0.3, 1.5));
    if r.chance(0.3) && inside(x, y) {
        p.windows.push(RoofWindow {
            x,
            y,
            width: 0.78,
            height: 1.18,
        });
    }
    p
}

#[test]
fn generated_roofs_show_the_same_figures_everywhere() {
    let cases: usize = std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(400);
    let mut bad = Vec::new();
    for seed in 1..=cases as u64 {
        let p = generated(seed);
        if !p.problems().is_empty() {
            continue;
        }
        let name = format!("seed {seed}");
        bad.extend(deductions_add_up(&name, &p));
        bad.extend(surfaces_agree(&name, &p));
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn a_window_on_a_flat_roof_is_listed_and_drawn_as_a_window() {
    let mut p = one_building(vec![
        edge(0.0, 0.0, 45.0, 6.0, 0.4),
        edge(13.0, 0.0, 45.0, 6.0, 0.4),
        edge(13.0, 8.0, 45.0, 6.0, 0.4),
        edge(0.0, 8.0, 45.0, 6.0, 0.4),
    ]);
    p.flat_roof = true;
    p.windows.push(RoofWindow {
        x: 11.0,
        y: 0.6,
        width: 0.78,
        height: 1.18,
    });
    let opts = tectora_core::export::ListOptions::new("t", "01.01.2026 00:00");
    let list = tectora_core::export::list_summary(&p, &opts);
    assert_eq!(list.windows.len(), 1, "{list:?}");
    assert!(tectora_core::export::dacad_list(&p, &opts).contains("Fenster1.01/1"));
    // On the plan sheet the face's label less the window's is the total.
    let shapes = tectora_core::export::plan_sheet(
        &p,
        &tectora_core::export::SheetInfo {
            stamp: String::new(),
            decimals: 3,
        },
    );
    let num = |t: &str| t.replace(',', ".").parse::<f64>().ok();
    let (mut sum, mut total) = (0.0, None);
    for s in &shapes {
        if let tectora_core::export::Shape::Text { text, .. } = s {
            if let Some(v) = text
                .strip_prefix("A = ")
                .and_then(|t| t.strip_suffix(" m²"))
            {
                total = num(v);
            } else if let Some(v) = text.strip_suffix("m²").and_then(num) {
                sum += v;
            }
        }
    }
    let total = total.expect("the sheet has a total");
    assert!((sum - total).abs() < 2e-3, "labels {sum} total {total}");
    let bad = [deductions_add_up("flat", &p), surfaces_agree("flat", &p)].concat();
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// A shed dormer at the pitch of the roof it stands on never meets that
/// roof. The standard calculation refuses it; where it stops on the house
/// already (a half-hip it cannot take) Tectora's own calculation must
/// refuse it too, not give a dormer running on past the ridge.
#[test]
fn a_dormer_that_never_meets_the_roof_is_refused_where_the_core_stops() {
    let mut gable = edge(10.8, 0.0, 90.0, 4.2, 0.3);
    gable.half_hip = Some(HalfHip {
        height: 5.96,
        pitch_deg: 45.0,
    });
    let mut p = one_building(vec![
        edge(0.0, 0.0, 15.0, 4.2, 0.9),
        gable,
        edge(10.8, 8.41, 15.0, 4.2, 0.3),
        edge(0.0, 8.41, 15.0, 4.2, 0.3),
    ]);
    let mut d = tectora_core::edit::dormers::default_params(
        tectora_core::edit::dormers::DormerKind::Shed,
        Point2::new(4.48, 2.0),
        0.0,
        5.18,
    );
    d.width = 2.36;
    d.front_pitch_deg = 15.0;
    d.side_overhang = 0.2;
    p.dormers
        .push(tectora_core::edit::dormers::build_dormer(&d));
    let geos = generate_project(&p);
    assert!(
        geos[0].error.is_some(),
        "faces {:?}",
        geos[0]
            .faces
            .iter()
            .map(|f| (f.edge, f.area))
            .collect::<Vec<_>>()
    );
}

/// A pitch break at the wall's own pitch is no line once the sweep joins
/// the two faces of that one plane; a chimney on it must not leave a
/// negative pitch-break length in the totals.
#[test]
fn a_chimney_on_a_joined_pitch_break_leaves_no_negative_line() {
    let mut e0 = edge(0.0, 0.0, 15.0, 3.5, 0.3);
    e0.breaks.push(PitchBreak {
        height: 4.14,
        pitch_deg: 15.0,
    });
    let mut gable = edge(12.62, 0.0, 90.0, 3.5, 0.3);
    gable.half_hip = Some(HalfHip {
        height: 5.44,
        pitch_deg: 45.0,
    });
    let mut p = one_building(vec![
        e0,
        gable,
        edge(12.62, 4.72, 25.0, 3.5, 0.3),
        edge(0.0, 4.72, 15.0, 3.5, 0.3),
    ]);
    p.chimneys.push(square(7.06, 3.86, 7.49, 4.28));
    p.chimneys.push(square(8.77, 2.04, 9.5, 2.76));
    let codes = tectora_core::quantities::dacad_line_totals(&p);
    assert!(
        codes.values().all(|v| *v >= 0.0),
        "negative line totals: {codes:?}"
    );
    let bad = [deductions_add_up("break", &p), surfaces_agree("break", &p)].concat();
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// The house of `a_chimney_on_a_joined_pitch_break_leaves_no_negative_line`,
/// with or without the pitch break at the wall's own pitch that makes the
/// standard calculation stop and the sweep join two faces of one plane.
fn half_hip_house(joined_break: bool) -> Project {
    let mut e0 = edge(0.0, 0.0, 15.0, 3.5, 0.3);
    if joined_break {
        e0.breaks.push(PitchBreak {
            height: 4.14,
            pitch_deg: 15.0,
        });
    }
    let mut gable = edge(12.62, 0.0, 90.0, 3.5, 0.3);
    if joined_break {
        gable.half_hip = Some(HalfHip {
            height: 5.44,
            pitch_deg: 45.0,
        });
    }
    one_building(vec![
        e0,
        gable,
        edge(12.62, 4.72, 25.0, 3.5, 0.3),
        edge(0.0, 4.72, 15.0, 3.5, 0.3),
    ])
}

/// A chimney on a face the sweep joined keeps its flashing lines: the
/// same chimney on the same plane of a roof the standard calculation
/// calculates (review 2026-11-21 MATH-004).
#[test]
fn a_chimney_on_a_joined_face_keeps_its_flashing_lines() {
    let line = |p: &Project, code: &str| {
        tectora_core::quantities::dacad_line_totals(p)
            .get(code)
            .copied()
            .unwrap_or(0.0)
    };
    let mut joined = half_hip_house(true);
    let mut core = half_hip_house(false);
    for p in [&mut joined, &mut core] {
        p.chimneys.push(square(8.77, 0.6, 9.5, 1.32));
    }
    for code in ["WS", "WV", "WH"] {
        let (a, b) = (line(&joined, code), line(&core, code));
        assert!(b > 0.5, "{code}: {b} on the core's roof");
        assert!((a - b).abs() < 0.01, "{code}: {a} joined, {b} core");
    }
}

/// A chimney on a pitch break that stays keeps its deduction when another
/// pitch break of the roof is joined away (review 2026-11-21 MATH-002).
#[test]
fn a_chimney_on_a_pitch_break_that_stays_keeps_its_deduction() {
    let house = |joined: bool| {
        let mut e0 = edge(0.0, 0.0, 15.0, 6.0, 0.5);
        if joined {
            e0.breaks.push(PitchBreak {
                height: 7.31,
                pitch_deg: 15.0,
            });
        }
        let mut e1 = edge(10.18, 0.0, 15.0, 6.0, 0.5);
        e1.breaks.push(PitchBreak {
            height: 6.8,
            pitch_deg: 25.0,
        });
        let mut e3 = edge(0.0, 10.8, 90.0, 6.0, 0.5);
        e3.half_hip = Some(HalfHip {
            height: 7.7,
            pitch_deg: 60.0,
        });
        let mut p = one_building(vec![e0, e1, edge(10.18, 10.8, 15.0, 6.0, 0.5), e3]);
        p.chimneys.push(square(7.4, 5.0, 8.0, 5.6));
        p
    };
    let c = |p: &Project| {
        tectora_core::quantities::dacad_line_totals(p)
            .get("c")
            .copied()
            .unwrap_or(0.0)
    };
    let (a, b) = (c(&house(true)), c(&house(false)));
    assert!(
        (a - b).abs() < 0.01,
        "pitch break {a} with the joined one, {b} without"
    );
}

/// A gable dormer on the joined face of a swept roof that runs on along
/// the roof and leaves the host face with a negative area is refused, as
/// the standard calculation refuses it (review 2026-11-21 MATH-003).
#[test]
fn a_dormer_that_leaves_a_negative_face_is_refused_where_the_core_stops() {
    let mut p = swept_with_window();
    p.windows.clear();
    let wall = |x: f64, pitch: f64, dir: Option<f64>| {
        let mut e = edge(x, 2.5, pitch, 6.3, 0.0);
        e.direction_rad = dir;
        e
    };
    p.dormers.push(tectora_core::Dormer {
        edges: vec![
            wall(7.0, 45.0, Some(3.0 * std::f64::consts::FRAC_PI_2)),
            wall(7.0, 90.0, None),
            wall(9.4, 45.0, Some(std::f64::consts::FRAC_PI_2)),
        ],
        end: Point2::new(9.4, 2.5),
        host_wall: None,
    });
    let geos = generate_project(&p);
    assert!(
        geos[0].error.is_some(),
        "faces {:?}",
        geos[0]
            .faces
            .iter()
            .map(|f| (f.edge, f.area))
            .collect::<Vec<_>>()
    );
}

/// TEST-001: a window anywhere on a roof the sweep calculates, where the
/// sweep joins faces of one plane, is deducted from the face it lies on.
#[test]
fn windows_anywhere_on_a_swept_roof_stay_on_their_face() {
    let (mut tried, mut bad) = (0, Vec::new());
    for (name, base) in [
        ("swept half-hip", swept_with_window()),
        ("swept T", swept_with_chimneys()),
    ] {
        let (t, b) = support::windows_stay_on_their_face(name, &base);
        tried += t;
        bad.extend(b);
    }
    assert!(tried > 100, "only {tried} windows placed");
    assert!(
        bad.is_empty(),
        "{} of {tried}:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

/// The house of `r2_winneg2` (Fable round 2): a pitch break that keeps
/// its pitch and a half-hip, which the standard calculation stops on.
fn joined_break_half_hip() -> Project {
    let mut e0 = edge(0.0, 0.0, 15.0, 3.5, 0.3);
    e0.breaks.push(PitchBreak {
        height: 4.14,
        pitch_deg: 15.0,
    });
    let mut e1 = edge(12.62, 0.0, 90.0, 3.5, 0.3);
    e1.half_hip = Some(HalfHip {
        height: 5.44,
        pitch_deg: 45.0,
    });
    one_building(vec![
        e0,
        e1,
        edge(12.62, 4.72, 25.0, 3.5, 0.3),
        edge(0.0, 4.72, 15.0, 3.5, 0.3),
    ])
}

/// R2-4: windows larger than their face on a roof Tectora calculates are
/// not a dormer that never meets the roof: the roof stays, with the
/// warning the core's roofs give (`cut_exceeds_face`), not an error.
#[test]
fn windows_larger_than_their_face_on_a_departed_roof_keep_the_roof_and_warn() {
    let mut p = joined_break_half_hip();
    for (x, y) in [(0.5, 1.2), (0.9, 3.0)] {
        p.windows.push(RoofWindow {
            x,
            y,
            width: 2.0,
            height: 2.5,
        });
    }
    let geo = &generate_project(&p)[0];
    assert!(geo.error.is_none(), "{:?}", geo.error);
    assert!(
        geo.warnings
            .iter()
            .any(|w| w.kind == tectora_core::check::WarningKind::CutExceedsFace),
        "{:?}",
        geo.warnings
    );
}

/// The hip roof of Fable round 2 (`r2_minus`, `r2_minus2`): a real pitch
/// break 15 -> 25 deg on wall 1, a half-hip on wall 3 and, when `joined`,
/// a pitch break on wall 0 that keeps the pitch, which the sweep joins away.
fn break_kept_and_joined(joined: bool) -> Project {
    let mut e0 = edge(0.0, 0.0, 15.0, 6.0, 0.5);
    if joined {
        e0.breaks.push(PitchBreak {
            height: 7.31,
            pitch_deg: 15.0,
        });
    }
    let mut e1 = edge(10.18, 0.0, 15.0, 6.0, 0.5);
    e1.breaks.push(PitchBreak {
        height: 6.8,
        pitch_deg: 25.0,
    });
    let mut e3 = edge(0.0, 10.8, 90.0, 6.0, 0.5);
    e3.half_hip = Some(HalfHip {
        height: 7.7,
        pitch_deg: 60.0,
    });
    one_building(vec![e0, e1, edge(10.18, 10.8, 15.0, 6.0, 0.5), e3])
}

/// R2-2 and R2-3: a chimney on or beside a pitch break that keeps the
/// pitch gives the lines of the same roof drawn without that break: the
/// break's deduction goes with the break, not off another pitch break
/// (`c` 5.23 instead of 5.83), and the chimney keeps its abutments where
/// a side runs along the seam (`WS`, `WV`, `WH`).
#[test]
fn a_chimney_by_a_joined_pitch_break_gives_the_lines_of_the_roof_without_it() {
    let lines = |p: &Project| -> BTreeMap<String, f64> {
        let geos = generate_project(p);
        assert!(geos[0].error.is_none(), "{:?}", geos[0].error);
        let mut by: BTreeMap<String, f64> = BTreeMap::new();
        for l in &geos[0].lines {
            *by.entry(l.dacad_code().to_string()).or_default() += l.length();
        }
        for (c, m) in &geos[0].minus_lines {
            *by.entry(c.clone()).or_default() -= m;
        }
        by.retain(|_, v| v.abs() > 1e-6);
        by
    };
    let mut bad = Vec::new();
    for x in [3.0, 4.0, 6.0] {
        // 3.76 to 3.788: the chimney's top side 3 to 8 mm below the break,
        // inside the core's booking tolerance (Fable R3-1: c -0.6).
        for y in [
            3.76, 3.77, 3.78, 3.785, 3.788, 3.79, 3.9, 4.0, 4.05, 4.08, 4.09, 4.1, 4.12, 4.2, 4.38,
            4.39,
        ] {
            // At x 6 a chimney from y 4.38 also crosses the hip, where the
            // core's own result jumps within a centimetre (WV 0.59 at 4.38,
            // 1.2 at 4.39 on the roof without the break): no reference.
            if x == 6.0 && y > 4.3 {
                continue;
            }
            let mut pair = [true, false].map(break_kept_and_joined);
            for p in &mut pair {
                p.chimneys.push(square(x, y, x + 0.6, y + 0.6));
            }
            let (got, want) = (lines(&pair[0]), lines(&pair[1]));
            let codes: std::collections::BTreeSet<&String> =
                got.keys().chain(want.keys()).collect();
            for code in codes {
                let (g, w) = (
                    got.get(code).copied().unwrap_or(0.0),
                    want.get(code).copied().unwrap_or(0.0),
                );
                if (g - w).abs() > 0.02 {
                    bad.push(format!(
                        "chimney at ({x}, {y}): {code} {g:.3}, without the break {w:.3}"
                    ));
                }
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// Fable R3-3 (`g2_2106`): a gable dormer whose valley crosses a joined
/// pitch break at its middle keeps the valley (K 4.47 m, as on the same
/// roof without the break), with its chimney and half-hip.
#[test]
fn a_dormer_valley_crossing_a_joined_break_stays() {
    let joined = Project::from_json(
        r#"{"name": "g2_2106", "parts": [{"name": "A", "edges": [{"start": {"x": 0.0, "y": 0.0}, "pitch_deg": 30.0, "eave_height": 6.0, "overhang": 0.0, "breaks": [{"height": 7.69, "pitch_deg": 30.0}]}, {"start": {"x": 12.06, "y": 0.0}, "pitch_deg": 30.0, "eave_height": 6.0, "overhang": 0.0}, {"start": {"x": 12.06, "y": 7.21}, "pitch_deg": 30.0, "eave_height": 6.0, "overhang": 0.0}, {"start": {"x": 0.0, "y": 7.21}, "pitch_deg": 90.0, "eave_height": 6.0, "overhang": 0.0, "half_hip": {"height": 8.08, "pitch_deg": 45.0}}]}], "dormers": [{"edges": [{"start": {"x": 2.33, "y": 0.95}, "pitch_deg": 45.0, "eave_height": 7.19, "overhang": 0.0, "direction_rad": 4.71238898038469}, {"start": {"x": 2.33, "y": 0.95}, "pitch_deg": 90.0, "eave_height": 7.19, "overhang": 0.0}, {"start": {"x": 4.33, "y": 0.95}, "pitch_deg": 45.0, "eave_height": 7.19, "overhang": 0.0, "direction_rad": 1.5707963267948966}], "end": {"x": 4.33, "y": 0.95}}], "chimneys": [{"corners": [{"x": 9.78, "y": 2.97}, {"x": 10.18, "y": 2.97}, {"x": 10.18, "y": 4.17}, {"x": 9.78, "y": 4.17}], "height": 11.081347720428601}]}"#,
    )
    .unwrap();
    let mut plain = joined.clone();
    plain.parts[0].edges[0].breaks.clear();
    let k = |p: &Project| -> f64 {
        let geos = generate_project(p);
        assert!(geos[0].error.is_none(), "{:?}", geos[0].error);
        let lines: f64 = geos[0]
            .lines
            .iter()
            .filter(|l| l.dacad_code() == "K")
            .map(|l| l.length())
            .sum();
        let minus: f64 = geos[0]
            .minus_lines
            .iter()
            .filter(|(c, _)| c == "K")
            .map(|(_, m)| m)
            .sum();
        lines - minus
    };
    let (got, want) = (k(&joined), k(&plain));
    assert!(
        (got - want).abs() < 0.02,
        "K {got:.3}, without the break {want:.3}"
    );
}

/// Fable R4 S3-a (`m1_209`): a T-shaped hip roof with one eave 2 cm higher,
/// which the core refuses. Keeping lines across the step (as the dormer
/// valley rule does) turned it into a swept roof with 2.5 m of valley and
/// hip along the step; it stays refused, as on the main line.
#[test]
fn a_centimetre_step_the_core_refuses_gets_no_seam_of_valley_and_hip() {
    let p = Project::from_json(r#"{"name": "m1_209", "parts": [{"name": "A", "edges": [{"start": {"x": 0, "y": 0}, "pitch_deg": 45, "eave_height": 6.0, "overhang": 0.5}, {"start": {"x": 10.1, "y": 0}, "pitch_deg": 45, "eave_height": 6.0, "overhang": 0.5}, {"start": {"x": 10.1, "y": 9.71}, "pitch_deg": 45, "eave_height": 6.02, "overhang": 0.5}, {"start": {"x": 7.21, "y": 9.71}, "pitch_deg": 45, "eave_height": 6.0, "overhang": 0.5}, {"start": {"x": 7.21, "y": 14.89}, "pitch_deg": 45, "eave_height": 6.0, "overhang": 0.5}, {"start": {"x": 2.58, "y": 14.89}, "pitch_deg": 45, "eave_height": 6.0, "overhang": 0.5}, {"start": {"x": 2.58, "y": 9.71}, "pitch_deg": 45, "eave_height": 6.0, "overhang": 0.5}, {"start": {"x": 0, "y": 9.71}, "pitch_deg": 45, "eave_height": 6.0, "overhang": 0.5}]}]}"#).unwrap();
    let geos = generate_project(&p);
    let g = &geos[0];
    if g.error.is_none() {
        let k: f64 = g
            .lines
            .iter()
            .filter(|l| l.dacad_code() == "K")
            .map(|l| l.length())
            .sum();
        assert!(k < 10.0, "K {k:.2} m: the step became a valley");
    }
}
