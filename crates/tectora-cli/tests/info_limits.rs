//! Audit 2026-11-23 ERR-1303: `tectora info` checks the same limits as
//! `calc` (Jakub's decision 2026-11-23) and answers an out-of-limit file
//! as `calc` does: `{"ok": false, "error": "<code>: <detail>"}`, exit 1,
//! never `"ok": true` with figures printed as null.

use std::path::PathBuf;
use std::process::Command;

use serde_json::{Value, json};

fn job(name: &str, project: &Value) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tectora-cli-info-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.tectora"));
    std::fs::write(&path, project.to_string()).unwrap();
    path
}

/// The answer of `tectora info` and its exit code.
fn info(path: &PathBuf) -> (Value, Option<i32>) {
    let out = Command::new(env!("CARGO_BIN_EXE_tectora"))
        .arg("info")
        .arg(path)
        .output()
        .unwrap();
    let v = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "{e}: {} / {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    });
    (v, out.status.code())
}

fn rect(x0: f64) -> Value {
    let pts = [(x0, 0.0), (x0 + 12.0, 0.0), (x0 + 12.0, 8.0), (x0, 8.0)];
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

/// `n` levels of unknown keys, objects and arrays in turn.
fn nested(n: usize) -> Value {
    let mut v = json!({});
    for k in 1..n {
        v = if k % 2 == 1 {
            Value::Array(vec![v])
        } else {
            json!({ "a": v })
        };
    }
    v
}

#[test]
fn info_refuses_what_calc_refuses() {
    let max = tectora_core::model::MAX_EXTRA_DEPTH;
    // Unknown keys one level too deep: too_many, as calc.
    let mut deep = rect(0.0);
    deep["crm"] = nested(max + 1);
    let (r, code) = info(&job("deep33", &deep));
    assert_eq!(r["ok"], false, "{r}");
    let e = r["error"].as_str().unwrap();
    assert!(
        e.starts_with("too_many: ") && e.contains("nested too deep"),
        "{e}"
    );
    assert_eq!(code, Some(1));
    // A corner at 1e308: out_of_range, no figures printed as null.
    let (r, code) = info(&job("huge_x", &rect(1e308)));
    assert_eq!(r["ok"], false, "{r}");
    let e = r["error"].as_str().unwrap();
    assert!(e.starts_with("out_of_range: "), "{e}");
    assert!(r.get("quantities").is_none(), "{r}");
    assert_eq!(code, Some(1));
    // 200 km off: out of the app's range too.
    let (r, code) = info(&job("far", &rect(200_000.0)));
    assert!(r["error"].as_str().unwrap().contains("out of range"), "{r}");
    assert_eq!(code, Some(1));
    // At the deepest nesting allowed: the summary, as before.
    let mut ok = rect(0.0);
    ok["crm"] = nested(max);
    let (r, code) = info(&job("deep32", &ok));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["name"], "Main");
    assert!(
        r["quantities"]["footprint_area"].as_f64().unwrap() > 90.0,
        "{r}"
    );
    assert_eq!(code, Some(0));
}
