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
use tectora_core::{Chimney, Dormer, EdgeDef, HalfHip, PitchBreak, Project, RoofPart, RoofWindow};

/// Half a unit of DACAD's 2-decimal output plus float32 noise.
const TOL: f64 = 0.006;
/// DACAD's area group for material 1 (tiles), as named in this DACAD build.
const TILES: &str = "Škridla - ";

fn fixtures() -> Vec<(String, Value)> {
    fixtures_in("dacad_cases")
}

fn fixtures_in(sub: &str) -> Vec<(String, Value)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(sub);
    let mut out: Vec<(String, Value)> = std::fs::read_dir(&dir)
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

/// One wall's stacked DACAD definitions as an edge: a gable followed by a
/// sloped definition is a Krüppelwalm, further definitions are pitch breaks.
fn edge(wall: &Value) -> EdgeDef {
    let priority = wall["definitions"][0]["priority"].as_f64().unwrap_or(0.0) > 0.5;
    let defs: Vec<(f64, f64, f64)> = wall["definitions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| {
            (
                d["pitch_deg"].as_f64().unwrap(),
                d["eave_height_m"].as_f64().unwrap(),
                d["overhang_m"].as_f64().unwrap(),
            )
        })
        .collect();
    let (pitch_deg, eave_height, overhang) = defs[0];
    let mut rest = defs[1..].iter();
    let half_hip = if pitch_deg >= GABLE_PITCH_DEG {
        rest.next().map(|&(p, h, _)| HalfHip {
            height: h,
            pitch_deg: p,
        })
    } else {
        None
    };
    let (start, end) = (pt(&wall["start"]), pt(&wall["end"]));
    EdgeDef {
        start,
        pitch_deg,
        eave_height,
        overhang,
        half_hip,
        direction_rad: (start.distance(end) < 1e-6)
            .then(|| wall["direction_deg"].as_f64().map(f64::to_radians))
            .flatten(),
        breaks: rest
            .map(|&(p, h, _)| PitchBreak {
                height: h,
                pitch_deg: p,
            })
            .collect(),
        priority: u8::from(priority),
    }
}

fn roof_no(wall: &Value) -> u64 {
    wall["definitions"][0]["roof_no"].as_u64().unwrap_or(1)
}

/// The main roof: the walls of roof 1, chained end to start.
fn part(case: &Value) -> RoofPart {
    let walls: Vec<&Value> = case["input"]["walls"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|w| roof_no(w) == 1)
        .collect();
    let mut order = vec![0];
    while order.len() < walls.len() {
        let end = pt(&walls[*order.last().unwrap()]["end"]);
        let next = (0..walls.len())
            .find(|&i| !order.contains(&i) && pt(&walls[i]["start"]).approx_eq(end, 1e-6))
            .expect("closed wall chain");
        order.push(next);
    }
    let mut part = RoofPart {
        name: "case".into(),
        edges: order.iter().map(|&i| edge(walls[i])).collect(),
    };
    part.normalize_ccw();
    part
}

/// Dormers: the walls of every other roof number, in authored order.
fn dormers(case: &Value) -> Vec<Dormer> {
    let mut by_roof: BTreeMap<u64, Vec<&Value>> = BTreeMap::new();
    for w in case["input"]["walls"].as_array().unwrap() {
        if roof_no(w) != 1 {
            by_roof.entry(roof_no(w)).or_default().push(w);
        }
    }
    by_roof
        .values()
        .map(|walls| Dormer {
            edges: walls.iter().map(|w| edge(w)).collect(),
            end: pt(&walls.last().unwrap()["end"]),
        })
        .collect()
}

fn project(name: &str, case: &Value) -> Project {
    let input = &case["input"];
    let mut project = Project::new(name);
    project.parts.push(part(case));
    project.dormers = dormers(case);
    for w in input["windows"].as_array().into_iter().flatten() {
        project.windows.push(RoofWindow {
            x: w["x"].as_f64().unwrap(),
            y: w["y"].as_f64().unwrap(),
            width: w["width"].as_f64().unwrap(),
            height: w["height"].as_f64().unwrap(),
        });
    }
    for c in input["chimneys"].as_array().into_iter().flatten() {
        project.chimneys.push(Chimney {
            corners: c["corners"].as_array().unwrap().iter().map(pt).collect(),
            height: c["height_m"].as_f64().unwrap_or(0.0),
        });
    }
    project
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
    assert!(cases.len() >= 42, "fixtures missing");
    let mut failures = Vec::new();
    for (name, case) in cases {
        let project = project(&name, &case);
        let geo = generate_project(&project).remove(0);
        let mut ours: BTreeMap<String, f64> = BTreeMap::new();
        for l in &geo.lines {
            *ours.entry(l.dacad_code().to_string()).or_default() += l.length();
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
            if (a - b).abs() > TOL {
                failures.push(format!("{name}: {code} ours {a:.4} DACAD {b:.4}"));
            }
        }
        let area: f64 = geo.faces.iter().map(|f| f.area).sum();
        let dacad_area = results["area_groups"][TILES].as_f64().unwrap();
        if (area - dacad_area).abs() > TOL {
            failures.push(format!("{name}: area ours {area:.4} DACAD {dacad_area:.4}"));
        }
    }
    assert!(
        failures.is_empty(),
        "differences from DACAD:\n{}",
        failures.join("\n")
    );
}
