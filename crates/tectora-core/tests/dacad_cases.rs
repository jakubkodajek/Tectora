//! Parity with roofs calculated by DACAD itself.
//!
//! Each fixture in `tests/fixtures/dacad_cases` holds the walls with their
//! stacked roof definitions and the line and area totals the real DACAD
//! (`DACAD DE.exe` under Wine) computed for them. `tests/fixtures/dacad_norms`
//! holds the same cases measured to DIN, ÖNORM and SIA.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde_json::Value;
use tectora_core::dad::{DadFile, DadWall};
use tectora_core::geom::Point2;
use tectora_core::model::GABLE_PITCH_DEG;
use tectora_core::openings::generate_project;
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
fn wall(id: usize, w: &Value) -> DadWall {
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
            .filter(|s| ((s - a).rem_euclid(std::f64::consts::TAU) + 1e-4) % std::f64::consts::TAU < 2e-4)
            .unwrap_or(a)
    } else {
        stored.unwrap_or(0.0).to_radians()
    };
    DadWall {
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
        clad: (pitch_deg - 90.0).abs() <= 90.0 - GABLE_PITCH_DEG
            && d0["material"].as_u64().unwrap_or(0) != 0,
    }
}

/// The case as the V1 importer would turn it into a project: closed wall
/// loops become roof parts, open chains dormers.
fn project(name: &str, case: &Value) -> Project {
    let input = &case["input"];
    let mut file = DadFile {
        version: "test".into(),
        ..DadFile::default()
    };
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
