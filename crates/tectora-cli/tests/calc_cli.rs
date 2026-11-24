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
    // Every wall leaning outward: valid input (DACAD takes 2-178°), but the
    // faces never close. A pitch of 1° is now refused before the engine.
    let mut p = rect(0.0, 12.0);
    for e in p["parts"][0]["edges"].as_array_mut().unwrap() {
        e["pitch_deg"] = json!(120);
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

#[test]
fn a_closed_output_pipe_ends_quietly() {
    // ARCH-008: `tectora calc job.tectora | head -n 1` panicked with
    // "failed printing to stdout: Broken pipe" and a backtrace.
    use std::process::Stdio;
    let path = job("pipe", &rect(0.0, 12.0));
    let mut child = Command::new(env!("CARGO_BIN_EXE_tectora"))
        .arg("calc")
        .arg(&path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // The reader goes away before the answer is written.
    drop(child.stdout.take());
    let out = child.wait_with_output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("panicked"), "{stderr}");
    assert!(
        out.status.code().is_some(),
        "ended by a signal: {:?}",
        out.status
    );
}

fn calc_status(path: &PathBuf) -> (Option<i32>, Value) {
    let out = Command::new(env!("CARGO_BIN_EXE_tectora"))
        .args(["calc", "--summary"])
        .arg(path)
        .output()
        .unwrap();
    (
        out.status.code(),
        serde_json::from_slice(&out.stdout).unwrap(),
    )
}

#[test]
fn one_bad_building_is_left_out_and_reported_as_mcp_does() {
    // HUNTC-004: a 1 mm wall in the second building refused the whole job
    // ("invalid project", exit 1) while MCP and the app calculated the rest.
    let mut p = rect(0.0, 12.0);
    let mut bad = rect(30.0, 12.0)["parts"][0].clone();
    bad["name"] = json!("Part 2");
    bad["edges"].as_array_mut().unwrap().insert(
        1,
        json!({ "start": { "x": 42.001, "y": 0.0 }, "pitch_deg": 40, "eave_height": 3, "overhang": 0.5 }),
    );
    p["parts"].as_array_mut().unwrap().push(bad);
    let alone = calc(&job("one-good", &rect(0.0, 12.0)));
    let (code, r) = calc_status(&job("one-bad", &p));
    assert_eq!(code, Some(2), "{r}");
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["partial"], true, "{r}");
    let problems = r["problems"].as_array().unwrap();
    assert!(
        problems
            .iter()
            .any(|x| x["item"] == "part" && x["index"] == 1 && x["code"] == "zero_length_wall"),
        "{r}"
    );
    assert_eq!(
        r["quantities"]["roof_area"], alone["quantities"]["roof_area"],
        "{r}"
    );
    // A job without problems is not partial and exits 0.
    let (code, r) = calc_status(&job("all-good", &rect(0.0, 12.0)));
    assert_eq!(code, Some(0), "{r}");
    assert!(r.get("partial").is_none(), "{r}");
    assert!(r.get("problems").is_none(), "{r}");
}

#[test]
fn area_by_material_lists_every_material_by_number_in_english() {
    // MATH-004: two materials with one name lost one area; ERR-004: the
    // default names were the reference program's German ones.
    let mut p = rect(0.0, 12.0);
    p["parts"][0]["edges"][1]["material"] = json!(2);
    p["parts"][0]["edges"][3]["material"] = json!(2);
    p["material_names"] = json!(["Bramac", "Bramac"]);
    let r = calc(&job("same-name", &p));
    let rows = r["quantities"]["area_by_material"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "{r}");
    let sum: f64 = rows.iter().map(|x| x["area"].as_f64().unwrap()).sum();
    let total = r["quantities"]["roof_area"].as_f64().unwrap();
    assert!((sum - total).abs() < 0.02, "{sum} vs {total}: {r}");
    assert_eq!(rows[0]["material"], 1, "{r}");
    assert_eq!(rows[1]["name"], "Bramac", "{r}");
    let r = calc(&job("default-name", &rect(0.0, 12.0)));
    assert_eq!(
        r["quantities"]["area_by_material"][0]["name"], "tiles",
        "{r}"
    );
    let text = r.to_string();
    for german in ["Ziegel", "Blech"] {
        assert!(!text.contains(german), "{text}");
    }
}

/// Every wall leaning outward: valid input, but the core closes no faces
/// (`open_faces`), as in `a_refused_roof_gives_its_code_and_an_english_message`.
fn refused(x0: f64) -> Value {
    let mut p = rect(x0, 12.0);
    for e in p["parts"][0]["edges"].as_array_mut().unwrap() {
        e["pitch_deg"] = json!(120);
    }
    p
}

#[test]
fn a_building_the_core_gives_no_roof_makes_the_result_partial() {
    // Round 2 ERR-103: a building without roof (core error) left the
    // result `ok` with exit 0, while a building left out for its input
    // gave `partial` and exit 2.
    let mut p = rect(0.0, 12.0);
    let mut bad = refused(30.0)["parts"][0].clone();
    bad["name"] = json!("Part 2");
    p["parts"].as_array_mut().unwrap().push(bad);
    let alone = calc(&job("core-good", &rect(0.0, 12.0)));
    let (code, r) = calc_status(&job("core-one-bad", &p));
    assert_eq!(code, Some(2), "{r}");
    assert_eq!(r["partial"], true, "{r}");
    assert_eq!(r["parts"][1]["error"]["kind"], "open_faces", "{r}");
    assert_eq!(
        r["quantities"]["roof_area"], alone["quantities"]["roof_area"],
        "{r}"
    );
    // No building with a roof: not calculated, exit 1.
    let (code, r) = calc_status(&job("core-all-bad", &refused(0.0)));
    assert_eq!(code, Some(1), "{r}");
    assert_eq!(r["ok"], false, "{r}");
    assert_eq!(r["calc_error"]["kind"], "open_faces", "{r}");
    assert!(r["error"].as_str().unwrap().contains("no building"), "{r}");
}

#[test]
fn an_answer_that_cannot_be_written_exits_with_an_error() {
    // Round 2 ERR-106: `tectora calc … > file` on a full disk warned on
    // stderr and exited 0. `/dev/full` exists on Linux only; elsewhere the
    // write cannot be made to fail this way.
    let full = std::path::Path::new("/dev/full");
    if !full.exists() {
        return;
    }
    let out = Command::new(env!("CARGO_BIN_EXE_tectora"))
        .args(["calc", "--summary"])
        .arg(job("full", &rect(0.0, 12.0)))
        .stdout(std::fs::OpenOptions::new().write(true).open(full).unwrap())
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("cannot write the answer"), "{stderr}");
}

#[test]
fn hand_drawn_faces_are_a_roof_and_an_empty_job_is_no_building() {
    // Audit 2026-11-21 ERR-202: a broken building plus a hand-drawn face
    // said `ok: false`, exit 1, next to a calculated area; HUNT-204 item
    // 2: a job with nothing said `ok: true`, exit 0.
    let face = json!([{
        "points": [{"x": 0, "y": 0}, {"x": 6, "y": 0}, {"x": 6, "y": 4}, {"x": 0, "y": 4}],
        "pitch_deg": 30
    }]);
    let mut p = refused(0.0);
    p["manual_faces"] = face.clone();
    let (code, r) = calc_status(&job("bad-plus-face", &p));
    assert_eq!(code, Some(2), "{r}");
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["partial"], true, "{r}");
    assert_eq!(r["roof_state"], "partial", "{r}");
    assert!(r["quantities"]["roof_area"].as_f64().unwrap() > 27.0, "{r}");
    // Only hand-drawn faces: calculated, exit 0, and the area per pitch
    // adds up to the roof area (HUNT-203).
    let only = json!({ "name": "Faces", "parts": [], "manual_faces": face });
    let (code, r) = calc_status(&job("faces-only", &only));
    assert_eq!(code, Some(0), "{r}");
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["roof_state"], "complete", "{r}");
    let by_pitch = r["quantities"]["area_by_pitch"].as_array().unwrap();
    assert_eq!(by_pitch.len(), 1, "{r}");
    assert_eq!(by_pitch[0]["area"], r["quantities"]["roof_area"], "{r}");
    // Nothing at all: no building, exit 1.
    let (code, r) = calc_status(&job("empty", &json!({ "name": "Empty", "parts": [] })));
    assert_eq!(code, Some(1), "{r}");
    assert_eq!(r["ok"], false, "{r}");
    assert_eq!(r["roof_state"], "no_building", "{r}");
    assert!(
        r["error"].as_str().unwrap().starts_with("no_building"),
        "{r}"
    );
}

#[test]
fn a_left_out_chimney_makes_the_roof_state_partial() {
    // Audit 2026-11-22 ERR-305: `partial: true` and exit 2 for a chimney
    // left out, but `roof_state: complete`.
    let mut p = rect(0.0, 12.0);
    p["chimneys"] = json!([{
        "corners": [{"x": 3, "y": 3}, {"x": 3, "y": 3}, {"x": 3, "y": 3}],
        "height": 9
    }]);
    let (code, r) = calc_status(&job("chim-bad", &p));
    assert_eq!(code, Some(2), "{r}");
    assert_eq!(r["partial"], true, "{r}");
    assert_eq!(r["roof_state"], "partial", "{r}");
    assert!(
        r["problems"].as_array().is_some_and(|a| !a.is_empty()),
        "{r}"
    );
}

#[test]
fn a_bundle_a_pipe_or_a_device_is_refused_with_a_reason() {
    // HUNT-204 item 4: a bundle was "not UTF-8"; ARCH-204: a pipe hung
    // and `/dev/zero` read until the memory was gone.
    let dir = std::env::temp_dir().join(format!("tectora-cli-odd-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let zip = dir.join("job.tectorajob");
    std::fs::write(&zip, b"PK\x03\x04rest of a zip").unwrap();
    let renamed = dir.join("job-as.tectora");
    std::fs::write(&renamed, b"PK\x03\x04rest of a zip").unwrap();
    for path in [&zip, &renamed] {
        let (code, r) = calc_status(path);
        assert_eq!(code, Some(1), "{r}");
        let e = r["error"].as_str().unwrap();
        assert!(e.contains("job bundle") && e.contains("import-job"), "{e}");
    }
    #[cfg(unix)]
    {
        let fifo = dir.join("fifo.tectora");
        let _ = std::fs::remove_file(&fifo);
        let made = Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .is_ok_and(|s| s.success());
        let zero = dir.join("zero.tectora");
        let _ = std::fs::remove_file(&zero);
        std::os::unix::fs::symlink("/dev/zero", &zero).unwrap();
        let mut paths = vec![zero];
        if made {
            paths.push(fifo);
        }
        for path in &paths {
            let started = std::time::Instant::now();
            let (code, r) = calc_status(path);
            assert!(started.elapsed() < std::time::Duration::from_secs(20));
            assert_eq!(code, Some(1), "{r}");
            assert!(
                r["error"].as_str().unwrap().contains("not a regular file"),
                "{r}"
            );
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Audit 2026-11-24 TEST-2009 (J03): the job-level warnings of a job of
/// hand-drawn faces only are in `--summary` (a bow tie, ERR-1901; two faces
/// on each other, HUNT-2002), with their English messages.
#[test]
fn the_summary_carries_the_job_level_warnings_of_a_job_of_faces_only() {
    let face = |pts: &[(f64, f64)]| {
        json!({
            "points": pts.iter().map(|&(x, y)| json!({ "x": x, "y": y })).collect::<Vec<_>>(),
            "pitch_deg": 30
        })
    };
    let bow = face(&[(10.0, 0.0), (14.0, 4.0), (14.0, 0.0), (10.0, 4.0)]);
    let square = face(&[(0.0, 0.0), (6.0, 0.0), (6.0, 4.0), (0.0, 4.0)]);
    let only =
        json!({ "name": "Faces", "parts": [], "manual_faces": [square.clone(), bow, square] });
    let (code, r) = calc_status(&job("faces-warned", &only));
    assert_eq!(code, Some(0), "{r}");
    let kinds: Vec<&str> = r["warnings"]
        .as_array()
        .unwrap_or_else(|| panic!("no job-level warnings: {r}"))
        .iter()
        .map(|w| w["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        ["manual_face_crosses", "manual_faces_overlap"],
        "{r}"
    );
    let w = &r["warnings"][1];
    assert_eq!(
        (w["face"].as_u64(), w["other_face"].as_u64()),
        (Some(0), Some(2)),
        "{w}"
    );
    assert!((w["value"].as_f64().unwrap() - 24.0).abs() < 1e-6, "{w}");
    assert!(
        w["message"].as_str().unwrap().contains("counted twice"),
        "{w}"
    );
    // The areas are kept: both squares and the bow tie's lobes count.
    let area = r["quantities"]["roof_area"].as_f64().unwrap();
    assert!((area - 48.0 / 30f64.to_radians().cos()).abs() < 0.01, "{r}");
}
