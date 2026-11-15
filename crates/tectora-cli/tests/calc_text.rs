//! `tectora calc` never prints the calculation's own (German) error text
//! or the reference product's name (audit ERR-005): with and without
//! `--summary` an error is the English sentence MCP `calc` gives.

use std::path::PathBuf;
use std::process::Command;

use serde_json::{Value, json};

fn job(name: &str, project: &Value) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tectora-cli-text-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.tectora"));
    std::fs::write(&path, project.to_string()).unwrap();
    path
}

fn calc(path: &PathBuf, summary: bool) -> String {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_tectora"));
    cmd.arg("calc");
    if summary {
        cmd.arg("--summary");
    }
    let out = cmd.arg(path).output().unwrap();
    String::from_utf8(out.stdout).unwrap()
}

/// A 10 x 8 m hip roof with an L-shaped (not convex) chimney: the
/// calculation refuses it.
fn hip_with_l_chimney() -> Value {
    let pts = [(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)];
    let l = [
        (4.0, 1.0),
        (5.0, 1.0),
        (5.0, 1.4),
        (4.4, 1.4),
        (4.4, 2.0),
        (4.0, 2.0),
    ];
    json!({
        "name": "Main",
        "parts": [{
            "name": "Main",
            "edges": pts.iter().map(|&(x, y)| json!({
                "start": { "x": x, "y": y }, "pitch_deg": 40, "eave_height": 3, "overhang": 0.5
            })).collect::<Vec<_>>(),
        }],
        "chimneys": [{
            "corners": l.iter().map(|&(x, y)| json!({ "x": x, "y": y })).collect::<Vec<_>>(),
            "height": 7,
        }],
    })
}

#[test]
fn a_refused_roof_is_explained_in_english_with_and_without_summary() {
    let path = job("l-chimney", &hip_with_l_chimney());
    for summary in [true, false] {
        let text = calc(&path, summary);
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["calc_error"]["kind"], "non_convex_opening", "{text}");
        for banned in ["Nicht", "Schornstein", "DACAD", "\\u00d6", "Ö"] {
            assert!(
                !text.contains(banned),
                "summary {summary}: `{banned}` in {text}"
            );
        }
        if !summary {
            let e = &v["parts"][0]["geometry"]["error"];
            assert_eq!(e["kind"], "non_convex_opening", "{text}");
            assert!(
                e["message"]
                    .as_str()
                    .unwrap()
                    .starts_with("a chimney or roof opening is not convex"),
                "{text}"
            );
        }
    }
}
