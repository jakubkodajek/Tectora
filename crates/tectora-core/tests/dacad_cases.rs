//! Parity with roofs calculated by DACAD itself.
//!
//! Each fixture in `tests/fixtures/dacad_cases` holds the walls with their
//! stacked roof definitions and the line and area totals the real DACAD
//! (`DACAD DE.exe` under Wine) computed for them. `tests/fixtures/dacad_norms`
//! holds the same cases measured to DIN, ÖNORM and SIA.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde_json::Value;
use tectora_core::geom::Point2;
use tectora_core::model::GABLE_PITCH_DEG;
use tectora_core::openings::generate_project;
use tectora_core::walls::{PlanWall, WallList};
use tectora_core::{Chimney, HalfHip, PitchBreak, Project, RoofWindow};

/// Half a unit of DACAD's 2-decimal output plus float32 noise.
const TOL: f64 = 0.006;
/// DACAD's area group for material 1 (tiles), as named in this DACAD build.
const TILES: &str = "Škridla - ";

fn fixtures() -> Vec<(String, Value)> {
    fixtures_in("dacad_cases")
}

fn fixtures_in(sub: &str) -> Vec<(String, Value)> {
    read_fixtures(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures")
            .join(sub),
    )
}

fn read_fixtures(dir: &std::path::Path) -> Vec<(String, Value)> {
    let mut out: Vec<(String, Value)> = std::fs::read_dir(dir)
        .expect("fixture dir")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .map(|p| {
            let name = p.file_stem().unwrap().to_string_lossy().into_owned();
            let v = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
            (name, v)
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn pt(v: &Value) -> Point2 {
    Point2::new(v[0].as_f64().unwrap(), v[1].as_f64().unwrap())
}

/// One wall with its stacked DACAD definitions, as the V1 importer reads
/// it: a gable followed by a sloped definition is a Krüppelwalm, further
/// definitions are pitch breaks.
fn wall(id: usize, w: &Value) -> PlanWall {
    let defs = w["definitions"].as_array().unwrap();
    let d0 = &defs[0];
    let pitch_deg = d0["pitch_deg"].as_f64().unwrap();
    let stacked = |d: &Value| PitchBreak {
        height: d["eave_height_m"].as_f64().unwrap(),
        pitch_deg: d["pitch_deg"].as_f64().unwrap(),
    };
    let mut rest = defs[1..].iter();
    let half_hip = if (pitch_deg - 90.0).abs() <= 90.0 - GABLE_PITCH_DEG {
        rest.next().map(|d| {
            let b = stacked(d);
            HalfHip {
                height: b.height,
                pitch_deg: b.pitch_deg,
            }
        })
    } else {
        None
    };
    let (start, end) = (pt(&w["start"]), pt(&w["end"]));
    let length = start.distance(end);
    // The direction as DACAD stored it (e.g. 270 rather than -90), which
    // it writes into the RD2.
    let stored = w["direction_deg"].as_f64();
    let angle_rad = if length > 1e-6 {
        let a = (end.y - start.y).atan2(end.x - start.x);
        stored
            .map(f64::to_radians)
            .filter(|s| {
                ((s - a).rem_euclid(std::f64::consts::TAU) + 1e-4) % std::f64::consts::TAU < 2e-4
            })
            .unwrap_or(a)
    } else {
        stored.unwrap_or(0.0).to_radians()
    };
    PlanWall {
        id: id as u16 + 1,
        group: d0["roof_no"].as_u64().unwrap_or(1) as u16,
        start,
        length,
        stored_length: length as f32 as f64,
        angle_rad,
        eave_height: d0["eave_height_m"].as_f64().unwrap(),
        overhang: d0["overhang_m"].as_f64().unwrap(),
        pitch_deg,
        priority: u8::from(d0["priority"].as_f64().unwrap_or(0.0) > 0.5),
        half_hip,
        breaks: rest.map(stacked).collect(),
        clad: pitch_deg >= GABLE_PITCH_DEG && d0["material"].as_u64().unwrap_or(0) != 0,
        material: tectora_core::model::DEFAULT_MATERIAL,
    }
}

/// The case as the V1 importer would turn it into a project: closed wall
/// loops become roof parts, open chains dormers.
fn project(name: &str, case: &Value) -> Project {
    let input = &case["input"];
    let mut file = WallList::default();
    for (i, w) in input["walls"].as_array().unwrap().iter().enumerate() {
        file.walls.push(wall(i, w));
    }
    for w in input["windows"].as_array().into_iter().flatten() {
        file.windows.push(RoofWindow {
            x: w["x"].as_f64().unwrap(),
            y: w["y"].as_f64().unwrap(),
            width: w["width"].as_f64().unwrap(),
            height: w["height"].as_f64().unwrap(),
        });
    }
    for c in input["chimneys"].as_array().into_iter().flatten() {
        file.chimneys.push(Chimney {
            corners: c["corners"].as_array().unwrap().iter().map(pt).collect(),
            height: c["height_m"].as_f64().unwrap_or(0.0),
        });
    }
    // In V1 files the roof record names a dormer's host wall; the V2
    // export's record means something else.
    if case["input_format"]
        .as_str()
        .is_some_and(|f| f.contains("V1"))
    {
        for (r, roof) in input["roofs"].as_object().into_iter().flatten() {
            let host = roof["kind"].as_u64().unwrap_or(0) as u16;
            file.roofs.push((r.parse().unwrap_or(0), host));
        }
    }
    file.to_project(name)
}

/// DACAD's line group names in this (Slovak) build and their codes.
const LINE_GROUPS: [(&str, &str); 13] = [
    ("1 Hreben -", "F"),
    ("2 Nárožie -", "G"),
    ("3 Úžlabie -", "K"),
    ("4 Krajné lavé -", "OL"),
    ("5 Krajné pravé -", "OR"),
    ("6 Odkvap -", "T"),
    ("7 Pult -", "P"),
    ("91 Predné napojenie -", "WV"),
    ("92 Bocné napojenie -", "WS"),
    ("93 Zadné napojenie -", "WH"),
    ("94 Krajné šikmo -", "OS"),
    ("Zlom -", "c"),
    ("Schlucht -", "S"),
];

#[test]
fn norm_groups_match_dacad() {
    use tectora_core::quantities::{Norm, norm_totals};
    let mut failures = Vec::new();
    let mut count = 0;
    for (dir, norm) in [
        ("DIN", Norm::Din),
        ("ONORM", Norm::Onorm),
        ("SIA", Norm::Sia),
    ] {
        for (name, case) in fixtures_in(&format!("dacad_norms/{dir}")) {
            count += 1;
            let ours = norm_totals(&project(&name, &case), norm);
            let r = &case["dacad_results"];
            // DACAD stores a "-Norm" group only when it differs from the base.
            let group = |base: &Value, key: &str| {
                base.get(format!("{key}-Norm"))
                    .or_else(|| base.get(key))
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0)
            };
            let area = group(&r["area_groups"], TILES);
            if (ours.roof_area - area).abs() > TOL {
                failures.push(format!(
                    "{dir} {name}: area ours {:.4} DACAD {area:.4}",
                    ours.roof_area
                ));
            }
            for (key, code) in LINE_GROUPS {
                let want = group(&r["line_groups"], key);
                let got = ours.lines_by_type.get(code).copied().unwrap_or(0.0);
                if (got - want).abs() > 0.011 {
                    failures.push(format!(
                        "{dir} {name}: {code} ours {got:.4} DACAD {want:.4}"
                    ));
                }
            }
        }
    }
    assert!(count >= 33, "norm fixtures missing");
    assert!(
        failures.is_empty(),
        "differences from DACAD:\n{}",
        failures.join("\n")
    );
}

#[test]
fn engine_matches_dacad() {
    let cases = fixtures();
    assert!(cases.len() >= 51, "fixtures missing");
    let failures = compare(cases);
    assert!(
        failures.is_empty(),
        "differences from DACAD:\n{}",
        failures.join("\n")
    );
}

/// Random projects of two or three roofs of our own (the mr3 stress set),
/// calculated by DACAD: joined by the Modellverschnitt.
#[test]
fn joined_roofs_match_dacad() {
    let cases = fixtures_in("multiroof");
    assert!(cases.len() >= 17, "fixtures missing");
    let failures = compare(cases);
    assert!(
        failures.is_empty(),
        "differences from DACAD:\n{}",
        failures.join("\n")
    );
}

/// The INT Tectora hands to the Modellverschnitt for three of them is the
/// one DACAD's core wrote, to the last printed digit but for signs of zero
/// and float noise.
#[test]
fn int_matches_dacad() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/multiroof");
    let mut n = 0;
    for (name, case) in fixtures_in("multiroof") {
        let Ok(want) = std::fs::read_to_string(dir.join(format!("{name}.INT"))) else {
            continue;
        };
        let mut p = project(&name, &case);
        for part in &mut p.parts {
            part.normalize_ccw();
        }
        let files =
            tectora_core::multiroof::files_with(&p, &tectora_core::multiroof::SweepProducer)
                .expect("files");
        if let Err(d) = tectora_core::modell::compare_sol(&files.int, &want, 2e-5) {
            panic!(
                "{name}: INT differs at token {}: ours {:?} DACAD {:?}",
                d.token, d.a, d.b
            );
        }
        n += 1;
    }
    assert_eq!(n, 3);
}

/// DACAD's own sample roofs, recalculated with the current DACAD. They are
/// derived from third-party files and stay outside the repository; point
/// `DACAD_SAMPLES` at their directory to run this.
#[test]
#[ignore]
fn samples_match_dacad() {
    let dir = std::env::var("DACAD_SAMPLES").expect("DACAD_SAMPLES");
    let cases = read_fixtures(std::path::Path::new(&dir));
    let only = std::env::var("DACAD_ONLY").ok();
    let mut bad = Vec::new();
    let mut total = 0;
    for (name, case) in cases {
        if only
            .as_deref()
            .is_some_and(|o| !o.split(',').any(|x| x == name))
        {
            continue;
        }
        total += 1;
        let n = name.clone();
        let Ok(f) = std::panic::catch_unwind(move || compare_tol(vec![(n, case)], sample_tol))
        else {
            println!("{name}: not supported");
            bad.push(name);
            continue;
        };
        if f.is_empty() {
            println!("{name}: ok");
        } else {
            println!("{}", f.join("\n"));
            bad.push(name);
        }
    }
    println!("{} of {total} match", total - bad.len());
    assert!(bad.is_empty(), "{bad:?}");
}

fn compare(cases: Vec<(String, Value)>) -> Vec<String> {
    compare_tol(cases, |_| TOL)
}

/// DACAD snaps the vertices of joined buildings to whole centimetres, which
/// the engine does not yet; allow for it on the samples.
fn sample_tol(v: f64) -> f64 {
    std::env::var("DACAD_TOL")
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or(0.03f64)
        .max(v.abs() * 0.002)
}

fn compare_tol(cases: Vec<(String, Value)>, tol: fn(f64) -> f64) -> Vec<String> {
    let mut failures = Vec::new();
    for (name, case) in cases {
        let project = project(&name, &case);
        let geos = generate_project(&project);
        // DACAD_FILES=<dir> writes the RD2 the engine hands to DACAD's
        // core, for comparing with DACAD's.
        if let Ok(dir) = std::env::var("DACAD_FILES") {
            let mut p = project.clone();
            for part in &mut p.parts {
                part.normalize_ccw();
            }
            match tectora_core::engine::rd2_text(&p) {
                Ok((rd2, _)) => std::fs::write(format!("{dir}/{name}.RD2"), rd2).unwrap(),
                Err(e) => println!("{name}: no RD2: {e}"),
            }
            if let Err(e) = tectora_core::engine::generate(&p) {
                println!("{name}: {e}");
            }
        }
        if std::env::var("DACAD_PARTS").is_ok() {
            for p in &project.parts {
                println!("part {}", p.name);
                for e in &p.edges {
                    println!(
                        "  ({:.3},{:.3}) {:.2} eave {:.4} ov {:.2} dir {:?}",
                        e.start.x,
                        e.start.y,
                        e.pitch_deg,
                        e.eave_height,
                        e.overhang,
                        e.direction_rad
                    );
                }
            }
        }
        if std::env::var("DACAD_DUMP").is_ok() {
            for (gi, g) in geos.iter().enumerate() {
                for l in &g.lines {
                    let f = |p: [f64; 3]| format!("({:.3},{:.3},{:.3})", p[0], p[1], p[2]);
                    println!(
                        "{gi} {:3} {:7.3} {} {}",
                        l.dacad_code(),
                        l.length(),
                        f(l.a),
                        f(l.b)
                    );
                }
                for f in &g.faces {
                    println!("{gi} face edge {} area {:.4}", f.edge, f.area);
                }
            }
        }
        let mut ours: BTreeMap<String, f64> = BTreeMap::new();
        for l in geos.iter().flat_map(|g| &g.lines) {
            *ours.entry(l.dacad_code().to_string()).or_default() += l.length();
        }
        for (code, len) in geos.iter().flat_map(|g| &g.minus_lines) {
            *ours.entry(code.clone()).or_default() -= len;
        }
        ours.retain(|_, v| *v > TOL);
        let results = &case["dacad_results"];
        let theirs: BTreeMap<String, f64> = results["lines_by_type"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.as_f64().unwrap()))
            .filter(|(_, v)| *v > TOL)
            .collect();
        let codes: BTreeSet<&String> = ours.keys().chain(theirs.keys()).collect();
        for code in codes {
            let a = ours.get(code).copied().unwrap_or(0.0);
            let b = theirs.get(code).copied().unwrap_or(0.0);
            if (a - b).abs() > tol(b) {
                failures.push(format!("{name}: {code} ours {a:.4} DACAD {b:.4}"));
            }
        }
        let area: f64 = geos
            .iter()
            .map(|g| g.faces.iter().map(|f| f.area).sum::<f64>() + g.clad_area)
            .sum();
        // No tiles group when DACAD gave no roof (non-convex chimney).
        let dacad_area = results["area_groups"][TILES].as_f64().unwrap_or(0.0);
        if (area - dacad_area).abs() > tol(dacad_area) {
            failures.push(format!("{name}: area ours {area:.4} DACAD {dacad_area:.4}"));
        }
    }
    failures
}

/// The step-by-step calculation the app shows adds up to the results for
/// every DACAD case, and every face's `plan area / cos(pitch) - deducted`
/// gives its area (joined roofs: within 1 %, flagged per face).
#[test]
fn breakdown_adds_up_for_every_case() {
    let mut failures = Vec::new();
    for sub in ["dacad_cases", "multiroof"] {
        for (name, case) in fixtures_in(sub) {
            let p = project(&name, &case);
            let b = tectora_core::breakdown::breakdown(&p);
            let t = tectora_core::quantities::roof_totals(&p);
            if (b.roof_area - t.roof_area).abs() > 1e-9 {
                failures.push(format!("{name}: area {} vs {}", b.roof_area, t.roof_area));
            }
            let codes = tectora_core::quantities::dacad_line_totals(&p);
            let mine: BTreeMap<String, f64> =
                b.lines.iter().map(|g| (g.code.clone(), g.total)).collect();
            if mine != codes {
                failures.push(format!("{name}: lines {mine:?} vs {codes:?}"));
            }
            // The results by line type (the Results panel) take the same
            // pieces off as the totals by code (a ridge through a chimney).
            let code = |c: &str| codes.get(c).copied().unwrap_or(0.0);
            for (what, by_type, by_code) in [
                ("eave", t.eave_length, code("T")),
                ("ridge", t.ridge_length, code("F")),
                ("hip", t.hip_length, code("G")),
                ("valley", t.valley_length, code("K")),
                (
                    "verge",
                    t.verge_length,
                    code("O") + code("OL") + code("OR") + code("OS"),
                ),
            ] {
                if (by_type - by_code).abs() > 0.016 {
                    failures.push(format!("{name}: {what} {by_type} vs codes {by_code}"));
                }
            }
            for part in &b.parts {
                // The faces' deductions are the openings plus the other
                // cut-outs of joined roofs, so the app can explain them.
                let ded: f64 = part.faces.iter().map(|f| f.deducted).sum();
                let listed: f64 =
                    part.openings.iter().map(|o| o.area).sum::<f64>() + part.other_cuts;
                if (ded - listed).abs() > 1e-6 {
                    failures.push(format!(
                        "{name}/{}: deducted {ded} vs listed {listed}",
                        part.name
                    ));
                }
                // The clad gables listed one by one make up the clad area.
                let clad: f64 = part.clad.iter().map(|c| c.1).sum();
                let listed: f64 = part.clad_faces.iter().map(|c| c.area).sum();
                if (clad - listed).abs() > 1e-9 {
                    failures.push(format!(
                        "{name}/{}: clad {clad} vs listed {listed}",
                        part.name
                    ));
                }
                for c in &part.clad_faces {
                    if let Some(fa) = c.formula_area {
                        failures.push(format!(
                            "{name}/{}: clad wall {:?} corners {fa} vs core {}",
                            part.name, c.wall, c.area
                        ));
                    }
                }
                for f in &part.faces {
                    // Where roofs are joined, the core's area (DACAD's) and the
                    // face polygon differ slightly (M3_0001, M3_0011, M3_0013:
                    // up to 0.4 %); the app shows both numbers. Single roofs
                    // match the formula.
                    if let Some(e) = f.engine_gross {
                        let rel = (f.gross - e).abs() / e.max(1e-9);
                        if sub != "multiroof" || rel > 0.01 {
                            failures.push(format!(
                                "{name}/{}: face {} pitch {} formula {:.4} engine {e:.4}",
                                part.name, f.no, f.pitch_deg, f.gross
                            ));
                        }
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Audit of the detailed steps the app shows: from the numbers as shown
/// (rounded to `breakdown::DECIMALS`), every step redone by hand gives the
/// next number shown to within one unit of its last decimal (plus half a
/// unit per term of a sum), and the chain ends within 0.005 of the result.
/// Runs over every example and every DACAD case.
#[test]
fn displayed_steps_recompute() {
    use tectora_core::breakdown::{Breakdown, DECIMALS as D};
    fn r(v: f64, d: u32) -> f64 {
        let k = 10f64.powi(d as i32);
        (v * k).round() / k
    }
    fn unit(d: u32) -> f64 {
        10f64.powi(-(d as i32))
    }
    fn audit(name: &str, b: &Breakdown, out: &mut Vec<String>) {
        let mut check = |what: String, got: f64, shown: f64, tol: f64| {
            // Float noise of the f64 values themselves aside.
            if (got - shown).abs() > tol + 1e-12 * shown.abs().max(1.0) {
                out.push(format!("{name}: {what}: recomputed {got} shown {shown}"));
            }
        };
        for (pi, part) in b.parts.iter().enumerate() {
            for f in &part.faces {
                let id = |s: &str| format!("part {pi} face {} {s}", f.no);
                let n = f.polygon.len();
                let c: Vec<[f64; 2]> = f
                    .polygon
                    .iter()
                    .map(|p| [r(p[0], D.coord), r(p[1], D.coord)])
                    .collect();
                let mut sum = 0.0;
                for i in 0..n {
                    let (a, bb) = (c[i], c[(i + 1) % n]);
                    let shown = [
                        r(f.products[i][0], D.product),
                        r(f.products[i][1], D.product),
                    ];
                    check(
                        id(&format!("x{i}*y{}", i + 1)),
                        r(a[0] * bb[1], D.product),
                        shown[0],
                        unit(D.product),
                    );
                    check(
                        id(&format!("x{}*y{i}", i + 1)),
                        r(bb[0] * a[1], D.product),
                        shown[1],
                        unit(D.product),
                    );
                    sum += shown[0] - shown[1];
                }
                let shown_sum = r(f.cross_sum, D.plan);
                // Half a unit per rounded product (2 per corner).
                check(
                    id("cross sum"),
                    r(sum, D.plan),
                    shown_sum,
                    unit(D.plan) * (1.0 + n as f64),
                );
                let shown_plan = r(f.plan_area, D.plan);
                check(
                    id("plan"),
                    r(shown_sum.abs() / 2.0, D.plan),
                    shown_plan,
                    unit(D.plan),
                );
                let shown_pitch = r(f.pitch_deg, D.pitch);
                let shown_cos = r(f.cos_pitch, f.cos_digits);
                check(
                    id("cos"),
                    r(shown_pitch.to_radians().cos(), f.cos_digits),
                    shown_cos,
                    unit(f.cos_digits),
                );
                if shown_cos.abs() < 1e-6 {
                    continue;
                }
                let shown_k = r(f.factor, D.factor);
                check(
                    id("1/cos"),
                    r(1.0 / shown_cos.abs(), D.factor),
                    shown_k,
                    unit(D.factor),
                );
                // Steps 4 and 5 are shown with the face's own decimals.
                let d = f.digits;
                let shown_gross = r(f.gross, d);
                check(
                    id("gross"),
                    r(shown_plan * shown_k, d),
                    shown_gross,
                    unit(d) + 2e-6,
                );
                let base = f.engine_gross.map_or(shown_gross, |e| r(e, d));
                let shown_area = r(f.area, d);
                let step = base - r(f.deducted, d);
                check(id("area"), r(step, d), shown_area, unit(d));
                // Rounding the numbers shown to hundredths gives the result
                // (no double rounding).
                check(id("hundredths"), r(shown_area, 2), r(f.area, 2), 0.0);
                check(id("step hundredths"), r(step, 2), r(f.area, 2), 0.0);
                if f.deducted.abs() < 1e-12 {
                    check(id("base hundredths"), r(base, 2), r(f.area, 2), 0.0);
                }
                // The chain from the first numbers shown lands on the result.
                let chain = (sum.abs() / 2.0) / shown_pitch.to_radians().cos().abs();
                if f.engine_gross.is_none() {
                    check(id("chain"), chain - f.deducted, f.area, 0.005);
                }
            }
        }
        for (pi, part) in b.parts.iter().enumerate() {
            for (ci, c) in part.clad_faces.iter().enumerate() {
                for (k, piece) in c.pieces.iter().enumerate() {
                    let id = |s: &str| format!("part {pi} clad {ci} piece {k} {s}");
                    let n = piece.uz.len();
                    let uz: Vec<[f64; 2]> = piece
                        .uz
                        .iter()
                        .map(|q| [r(q[0], D.coord), r(q[1], D.coord)])
                        .collect();
                    let mut sum = 0.0;
                    for i in 0..n {
                        let (a, bb) = (uz[i], uz[(i + 1) % n]);
                        let shown = [
                            r(piece.products[i][0], D.product),
                            r(piece.products[i][1], D.product),
                        ];
                        check(
                            id("u*z"),
                            r(a[0] * bb[1], D.product),
                            shown[0],
                            unit(D.product),
                        );
                        check(
                            id("u*z'"),
                            r(bb[0] * a[1], D.product),
                            shown[1],
                            unit(D.product),
                        );
                        sum += shown[0] - shown[1];
                    }
                    let shown_sum = r(piece.cross_sum, D.plan);
                    check(
                        id("sum"),
                        r(sum, D.plan),
                        shown_sum,
                        unit(D.plan) * (1.0 + n as f64),
                    );
                    check(
                        id("area"),
                        r(shown_sum.abs() / 2.0, D.area),
                        r(piece.area, D.area),
                        unit(D.area),
                    );
                    // u is the horizontal distance along the wall: the plan
                    // distance between corners equals the difference of u.
                    for i in 0..n {
                        let (p, q) = (piece.polygon[i], piece.polygon[(i + 1) % n]);
                        let plan = (q[0] - p[0]).hypot(q[1] - p[1]);
                        check(
                            id("u along wall"),
                            (piece.uz[(i + 1) % n][0] - piece.uz[i][0]).abs(),
                            plan,
                            1e-6,
                        );
                    }
                }
            }
        }
        // Every sum in the panel adds up as shown with its decimals.
        let mut sum_check = |what: String, terms: Vec<f64>, total: f64, d: u32| {
            // 0: shown with "≈" at `sum_max` decimals, off by float noise only.
            let (d, tol) = if d == 0 {
                (D.sum_max, unit(D.sum_max) * terms.len() as f64)
            } else {
                (d, 0.0)
            };
            let shown: f64 = terms.iter().map(|&t| r(t, d)).sum();
            check(format!("sum {what}"), r(shown, d), r(total, d), tol);
            check(
                format!("sum {what} hundredths"),
                r(r(total, d), 2),
                r(total, 2),
                0.0,
            );
        };
        for (pi, part) in b.parts.iter().enumerate() {
            let terms = part
                .faces
                .iter()
                .map(|f| f.area)
                .chain(part.clad_faces.iter().map(|c| c.area))
                .collect();
            sum_check(format!("part {pi}"), terms, part.roof_area, part.digits);
        }
        for m in &b.materials {
            sum_check(
                format!("material {}", m.material),
                m.items.iter().map(|i| i.area).collect(),
                m.area,
                m.digits,
            );
        }
        for g in &b.lines {
            let terms = g
                .items
                .iter()
                .map(|l| l.length)
                .chain(std::iter::once(-g.deducted))
                .collect();
            sum_check(format!("line {}", g.code), terms, g.sum, g.digits);
            sum_check(format!("line {} total", g.code), vec![g.sum], g.total, 2);
        }
        sum_check(
            "roof".into(),
            b.parts
                .iter()
                .map(|p| p.roof_area)
                .chain(b.manual_faces.iter().map(|f| f.area))
                .collect(),
            b.roof_area,
            b.digits,
        );
        for g in &b.lines {
            for (li, l) in g.items.iter().enumerate() {
                if l.typed {
                    continue;
                }
                let id = |s: &str| format!("line {} #{li} {s}", g.code);
                let mut sum = 0.0;
                for k in 0..3 {
                    let d = r(r(l.b[k], D.coord) - r(l.a[k], D.coord), D.coord);
                    let shown_d = r(l.delta[k], D.coord);
                    check(id(&format!("delta{k}")), d, shown_d, unit(D.coord));
                    let sq = l.sq_digits;
                    let shown_sq = r(l.squares[k], sq);
                    check(
                        id(&format!("sq{k}")),
                        r(shown_d * shown_d, sq),
                        shown_sq,
                        unit(sq),
                    );
                    sum += shown_sq;
                }
                let shown_sum = r(l.sum_sq, l.sq_digits);
                check(
                    id("sum"),
                    r(sum, l.sq_digits),
                    shown_sum,
                    unit(l.sq_digits) * 2.5,
                );
                let dl = l.digits;
                let shown_len = r(l.length, dl);
                check(id("sqrt"), r(shown_sum.sqrt(), dl), shown_len, unit(dl));
                check(id("hundredths"), r(shown_len, 2), r(l.length, 2), 0.0);
                check(id("chain"), sum.sqrt(), l.length, 0.005);
            }
        }
        for f in &b.manual_faces {
            let n = f.points.len();
            let mut sum = 0.0;
            for i in 0..n {
                let (a, bb) = (f.points[i], f.points[(i + 1) % n]);
                let p = |v: f64| r(v, D.coord);
                sum += r(p(a[0]) * p(bb[1]), D.product) - r(p(bb[0]) * p(a[1]), D.product);
            }
            let id = |s: &str| format!("manual face {} {s}", f.no);
            let shown_plan = r(f.plan_area, D.plan);
            check(
                id("plan"),
                r(sum.abs() / 2.0, D.plan),
                shown_plan,
                unit(D.plan) * (1.0 + n as f64),
            );
            if f.pitch_out_of_range {
                check(id("out of range"), f.area, 0.0, 0.0);
                continue;
            }
            let shown_cos = r(f.cos_pitch, f.cos_digits);
            check(
                id("cos"),
                r(r(f.pitch_deg, D.pitch).to_radians().cos(), f.cos_digits),
                shown_cos,
                unit(f.cos_digits),
            );
            let shown_k = r(f.factor, D.factor);
            check(
                id("1/cos"),
                r(1.0 / shown_cos, D.factor),
                shown_k,
                unit(D.factor),
            );
            let d = f.digits;
            check(
                id("area"),
                r(shown_plan * shown_k, d),
                r(f.area, d),
                unit(d),
            );
            check(id("hundredths"), r(r(f.area, d), 2), r(f.area, 2), 0.0);
        }
    }

    let mut failures = Vec::new();
    for ex in tectora_core::examples::EXAMPLES {
        let p = ex.project();
        audit(
            ex.id,
            &tectora_core::breakdown::breakdown(&p),
            &mut failures,
        );
    }
    // Standard roofs of random size and pitch, placed up to 2 km from the
    // origin (large coordinates make the products and squares long), with
    // a chimney and hand-added faces and lines.
    let mut seed = 0x2545_f491_4f6c_dd1du64;
    let mut rnd = move |lo: f64, hi: f64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        lo + (hi - lo) * (seed >> 11) as f64 / (1u64 << 53) as f64
    };
    use tectora_core::examples::{StandardRoof, standard_roof};
    let kinds = [
        StandardRoof::Gable,
        StandardRoof::Hip,
        StandardRoof::Shed,
        StandardRoof::HalfHip,
        StandardRoof::Pyramid,
    ];
    for i in 0..400 {
        let kind = kinds[i % kinds.len()];
        let (l, w) = (rnd(6.0, 40.0), rnd(6.0, 20.0));
        let mut part = standard_roof(kind, l, w, rnd(5.0, 75.0), rnd(2.0, 8.0), rnd(0.0, 1.0));
        let (ox, oy) = if i % 2 == 0 {
            (rnd(-2000.0, 2000.0), rnd(-2000.0, 2000.0))
        } else {
            (rnd(0.0, 300.0), rnd(0.0, 300.0))
        };
        for e in &mut part.edges {
            e.start = Point2::new(e.start.x + ox, e.start.y + oy);
        }
        let mut p = tectora_core::examples::find("gable").unwrap().project();
        p.parts = vec![part];
        if i % 4 == 1 {
            // A chimney somewhere on the roof, on the ridge every 8th roof.
            let cx = ox + l * rnd(0.25, 0.7);
            let cy = if i % 8 == 1 {
                oy + w / 2.0 - 0.4
            } else {
                oy + w * rnd(0.1, 0.7)
            };
            p.chimneys.push(Chimney {
                corners: vec![
                    Point2::new(cx, cy),
                    Point2::new(cx + 0.8, cy),
                    Point2::new(cx + 0.8, cy + 0.8),
                    Point2::new(cx, cy + 0.8),
                ],
                height: 12.0,
            });
        }
        if i % 3 == 0 {
            let (cx, cy) = (ox + l * rnd(0.2, 0.8), oy + w * rnd(0.2, 0.8));
            p.manual_faces.push(tectora_core::model::ManualFace {
                points: vec![
                    Point2::new(cx, cy),
                    Point2::new(cx + 2.0, cy),
                    Point2::new(cx + 2.0, cy + 1.5),
                ],
                pitch_deg: [rnd(0.0, 80.0), 89.9999, 95.0, 90.0][i / 3 % 4],
                label: String::new(),
            });
        }
        audit(
            &format!("random {i} {kind:?} at ({ox:.1}, {oy:.1})"),
            &tectora_core::breakdown::breakdown(&p),
            &mut failures,
        );
    }
    for sub in ["dacad_cases", "multiroof"] {
        for (name, case) in fixtures_in(sub) {
            let p = project(&name, &case);
            audit(
                &name,
                &tectora_core::breakdown::breakdown(&p),
                &mut failures,
            );
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures
            .iter()
            .take(40)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
