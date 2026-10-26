//! Parity with roofs calculated by DACAD itself.
//!
//! Each fixture in `tests/fixtures/dacad_cases` holds the walls with their
//! stacked roof definitions and the line and area totals the real DACAD
//! (`DACAD DE.exe` under Wine) computed for them.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde_json::Value;
use tectora_core::geom::Point2;
use tectora_core::model::GABLE_PITCH_DEG;
use tectora_core::roof::generate;
use tectora_core::{EdgeDef, HalfHip, PitchBreak, RoofPart};

/// Half a unit of DACAD's 2-decimal output plus float32 noise.
const TOL: f64 = 0.006;
/// DACAD's area group for material 1 (tiles), as named in this DACAD build.
const TILES: &str = "Škridla - ";

fn fixtures() -> Vec<(String, Value)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/dacad_cases");
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
    EdgeDef {
        start: pt(&wall["start"]),
        pitch_deg,
        eave_height,
        overhang,
        half_hip,
        direction_rad: None,
        breaks: rest
            .map(|&(p, h, _)| PitchBreak {
                height: h,
                pitch_deg: p,
            })
            .collect(),
        priority: u8::from(priority),
    }
}

/// Chains the walls end to start (DACAD does not store them in order).
fn part(case: &Value) -> RoofPart {
    let walls = case["input"]["walls"].as_array().unwrap();
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
        edges: order.iter().map(|&i| edge(&walls[i])).collect(),
    };
    part.normalize_ccw();
    part
}

#[test]
fn engine_matches_dacad() {
    let cases = fixtures();
    assert!(cases.len() >= 23, "fixtures missing");
    let mut failures = Vec::new();
    for (name, case) in cases {
        let geo = generate(&part(&case));
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
