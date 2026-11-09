//! `tectora calc` takes the same input as the app and the MCP server
//! (MATH-005), and a part's lines are net of deductions like the totals
//! (MATH-004).

use std::path::PathBuf;
use std::process::Command;

use serde_json::{Value, json};

fn job(name: &str, project: &Value) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tectora-cli-calc-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.tectora"));
    std::fs::write(&path, project.to_string()).unwrap();
    path
}

fn calc(path: &PathBuf) -> Value {
    let out = Command::new(env!("CARGO_BIN_EXE_tectora"))
        .args(["calc", "--summary"])
        .arg(path)
        .output()
        .unwrap();
    serde_json::from_slice(&out.stdout).unwrap()
}

fn rect(x0: f64, size: f64) -> Value {
    let pts = [(x0, 0.0), (x0 + size, 0.0), (x0 + size, 8.0), (x0, 8.0)];
    json!({
        "name": "Main",
        "parts": [{
            "name": "Main",
            "edges": pts.iter().map(|&(x, y)| json!({
                "start": { "x": x, "y": y }, "pitch_deg": 40, "eave_height": 3, "overhang": 0.5
            })).collect::<Vec<_>>(),
        }],
    })
}

#[test]
fn coordinates_the_app_refuses_are_refused_as_out_of_range() {
    // 200 km from the origin: within the engine's limit, beyond the app's.
    let r = calc(&job("far", &rect(200_000.0, 12.0)));
    assert_eq!(r["ok"], false, "{r}");
    let e = r["error"].as_str().unwrap();
    assert!(e.contains("out of range"), "{e}");
    // A corner past the engine's limit is out of range too, not "not a number".
    let r = calc(&job("huge", &rect(1.08e6, 12.0)));
    let e = r["error"].as_str().unwrap();
    assert!(
        e.contains("out of range") && !e.contains("not a number"),
        "{e}"
    );
    // A job in range calculates.
    let r = calc(&job("near", &rect(0.0, 12.0)));
    assert_eq!(r["ok"], true, "{r}");
}

#[test]
fn part_lines_are_net_of_the_chimney_on_the_ridge() {
    let mut p = rect(0.0, 12.0);
    for (i, e) in p["parts"][0]["edges"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        if i % 2 == 1 {
            e["pitch_deg"] = json!(90);
        }
    }
    p["chimneys"] = json!([{ "corners": [
        { "x": 5, "y": 3.5 }, { "x": 6, "y": 3.5 }, { "x": 6, "y": 4.5 }, { "x": 5, "y": 4.5 }
    ], "height": 8 }]);
    let r = calc(&job("chimney", &p));
    let ridge = r["parts"][0]["quantities"]["ridge_length"]
        .as_f64()
        .unwrap();
    let total = r["quantities"]["lines_by_type"]["F"].as_f64().unwrap();
    assert!((ridge - total).abs() < 0.01, "part {ridge}, total {total}");
}

#[test]
fn the_summary_carries_each_parts_warnings() {
    // Re-check: `--summary` dropped them (cut_exceeds_face showed only
    // without it).
    let mut p = rect(0.0, 12.0);
    p["windows"] = json!([{ "x": -50, "y": -50, "width": 0.78, "height": 1.18 }]);
    let r = calc(&job("stray-window", &p));
    let w = &r["parts"][0]["warnings"][0];
    assert_eq!(w["kind"], "window_off_roof", "{r}");
    assert!(
        w["message"].as_str().unwrap().contains("no roof face"),
        "{w}"
    );
}

#[test]
fn a_refused_roof_gives_its_code_and_an_english_message() {
    // Re-check: the raw German message of the calculation was printed.
    let mut p = rect(0.0, 12.0);
    for e in p["parts"][0]["edges"].as_array_mut().unwrap() {
        e["pitch_deg"] = json!(1);
    }
    let r = calc(&job("refused", &p));
    let text = r.to_string();
    for german in ["Fläche", "Lösung", "Kein", "Nicht", "vorhanden"] {
        assert!(!text.contains(german), "{german}: {r}");
    }
    let e = &r["parts"][0]["error"];
    assert_eq!(e["kind"], "open_faces", "{r}");
    assert!(
        e["message"]
            .as_str()
            .unwrap()
            .contains("faces do not close"),
        "{r}"
    );
    assert_eq!(r["calc_error"]["kind"], "open_faces", "{r}");
    assert!(r.get("dacad_error").is_none(), "{r}");
}
