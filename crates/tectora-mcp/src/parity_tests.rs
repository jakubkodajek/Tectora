//! UI/MCP parity guard (AGENTS.md "MCP parity"; Jakub 2026-11-28: every
//! thing the UI shows, the AI must see too). `docs/ui-mcp-parity.json`
//! lists every Tauri command and every Svelte component and TypeScript
//! module of the UI with the MCP tools that give an AI client the same
//! information and actions, or why it carries none. This test fails when
//! - a Tauri command, component or module is not listed (a new feature
//!   must say how the AI reaches it),
//! - a listed entry no longer exists (the list stays true),
//! - a named MCP tool does not exist,
//! - a `checks` path (`tool:path.to.field`) is missing from that tool's
//!   answer for a busy test job (chimney, window, opening, dormer,
//!   hand-drawn face, half-hip, clad gable, stored result, review).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tectora_core::store::{Store, now_ms};

use super::Server;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn manifest() -> Value {
    let path = root().join("docs/ui-mcp-parity.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).expect("docs/ui-mcp-parity.json is JSON")
}

/// The commands the app registers (`generate_handler![…]` in the Tauri
/// shell), without a module prefix.
fn tauri_commands() -> BTreeSet<String> {
    let src = std::fs::read_to_string(root().join("app/src-tauri/src/lib.rs"))
        .expect("Tauri shell source");
    let start = src
        .find("generate_handler![")
        .expect("generate_handler! in the Tauri shell")
        + "generate_handler![".len();
    let end = start + src[start..].find(']').expect("end of generate_handler!");
    src[start..end]
        .split(',')
        .map(|s| s.trim().rsplit("::").next().unwrap_or("").to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// The UI's Svelte components and TypeScript modules, relative to `app/src`
/// (tests, type declarations and the browser-preview stand-in left out).
fn ui_files() -> BTreeSet<String> {
    let src = root().join("app/src");
    let mut out = BTreeSet::new();
    for (dir, prefix) in [(src.clone(), ""), (src.join("lib"), "lib/")] {
        for e in std::fs::read_dir(&dir).expect("app/src") {
            let name = e.unwrap().file_name().to_string_lossy().to_string();
            let code = name.ends_with(".svelte") || name.ends_with(".ts");
            let skip = name.ends_with(".test.ts") || name.ends_with(".d.ts") || name == "main.ts";
            if code && !skip {
                out.insert(format!("{prefix}{name}"));
            }
        }
    }
    out
}

fn tool_names() -> BTreeSet<String> {
    crate::tools()
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect()
}

/// Every entry has exactly one of `mcp` (non-empty, existing tools),
/// `user_only` or `no_data` (a reason).
fn check_entry(what: &str, v: &Value, tools: &BTreeSet<String>, bad: &mut Vec<String>) {
    let reason = |k: &str| {
        v.get(k)
            .and_then(Value::as_str)
            .is_some_and(|s| s.trim().len() >= 5)
    };
    match (v.get("mcp"), reason("user_only"), reason("no_data")) {
        (Some(Value::Array(list)), false, false) if !list.is_empty() => {
            for t in list {
                let t = t.as_str().unwrap_or("");
                if !tools.contains(t) {
                    bad.push(format!("{what}: no MCP tool `{t}`"));
                }
            }
        }
        (None, true, false) | (None, false, true) => {}
        _ => bad.push(format!(
            "{what}: give `mcp` (the tools), or `user_only` / `no_data` with the reason"
        )),
    }
}

#[test]
fn every_tauri_command_and_ui_module_says_how_the_ai_reaches_it() {
    let m = manifest();
    let tools = tool_names();
    let mut bad = Vec::new();
    for (section, have) in [("tauri_commands", tauri_commands()), ("ui", ui_files())] {
        let listed: BTreeMap<String, Value> =
            serde_json::from_value(m[section].clone()).expect(section);
        for name in &have {
            match listed.get(name) {
                None => bad.push(format!(
                    "{section}: `{name}` is not in docs/ui-mcp-parity.json; list the MCP tools that give an AI the same, or why it carries no information"
                )),
                Some(v) => check_entry(&format!("{section}.{name}"), v, &tools, &mut bad),
            }
        }
        for name in listed.keys() {
            if !have.contains(name) {
                bad.push(format!(
                    "{section}: `{name}` is listed but no longer exists"
                ));
            }
        }
    }
    assert!(bad.is_empty(), "UI/MCP parity:\n{}", bad.join("\n"));
}

fn call(s: &mut Server, name: &str, args: Value) -> Value {
    s.call_tool(name, &args)
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// A job with something of every kind the editor shows.
fn busy_job(s: &mut Server) -> String {
    let r = call(
        s,
        "create_standard_roof",
        json!({ "name": "Parity", "type": "gable", "length": 12, "width": 8, "pitch_deg": 40,
                "eave_height": 3, "overhang": 0.5, "notes": "parity job",
                "job": { "number": "P-1", "customer": "Parity", "address": "Here" }, "summary": "start" }),
    );
    let id = r["id"].as_str().unwrap().to_string();
    // The user has seen the start, so the review lists what follows.
    let ts = s.store.activity_items(&id)[0].ts_ms;
    s.store.mark_reviewed(&id, ts).unwrap();
    call(
        s,
        "add_object",
        json!({ "id": id, "kind": "chimney", "x": 5, "y": 5, "summary": "chimney" }),
    );
    call(
        s,
        "add_object",
        json!({ "id": id, "kind": "window", "x": 8, "y": 1, "summary": "window" }),
    );
    call(
        s,
        "add_object",
        json!({ "id": id, "kind": "roof_opening", "x": 2, "y": 0.5, "summary": "opening" }),
    );
    call(
        s,
        "add_dormer",
        json!({ "id": id, "kind": "gable", "start": [3, 6.2], "angle_deg": 180, "width": 1.6, "summary": "dormer" }),
    );
    call(
        s,
        "set_manual",
        json!({ "id": id, "summary": "face", "faces": [{ "points": [[20, 0], [24, 0], [24, 3], [20, 3]], "pitch_deg": 30, "label": "porch" }] }),
    );
    // A half-hip above the ridge (an input notice) on a clad gable.
    call(
        s,
        "update_edges",
        json!({ "id": id, "summary": "gable", "changes": [{ "index": 1, "clad": true, "half_hip": { "height": 9.5, "pitch_deg": 50 } }] }),
    );
    call(s, "store_result", json!({ "id": id, "summary": "offer" }));
    id
}

/// The value at `path` (dot-separated keys and list indices), if present.
fn at<'a>(mut v: &'a Value, path: &str) -> Option<&'a Value> {
    for k in path.split('.').filter(|k| !k.is_empty()) {
        v = match k.parse::<usize>() {
            Ok(i) => v.get(i)?,
            Err(_) => v.get(k)?,
        };
    }
    Some(v)
}

#[test]
fn every_parity_check_path_is_in_the_tools_answer() {
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "tectora-mcp-parity-{}-{}-{n}",
        now_ms(),
        std::process::id()
    ));
    let mut s = Server::new(Store::new(dir).unwrap());
    let id = busy_job(&mut s);
    let args = |tool: &str| match tool {
        "list_projects" | "get_settings" | "list_examples" | "list_deleted" => json!({}),
        "roof_helper" => json!({ "id": id, "edge": 0, "tool": "context" }),
        "roof_at" => json!({ "id": id, "points": [[5, 2]] }),
        _ => json!({ "id": id }),
    };
    let m = manifest();
    let mut answers: BTreeMap<String, Value> = BTreeMap::new();
    let mut bad = Vec::new();
    let mut checked = 0;
    for section in ["tauri_commands", "ui"] {
        for (name, v) in m[section].as_object().unwrap() {
            for c in v
                .get("checks")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let c = c.as_str().unwrap();
                let (tool, path) = c
                    .split_once(':')
                    .unwrap_or_else(|| panic!("{name}: check `{c}` is not tool:path"));
                let answer = answers.entry(tool.to_string()).or_insert_with(|| {
                    s.call_tool(tool, &args(tool))
                        .unwrap_or_else(|e| panic!("{tool}: {e}"))
                });
                checked += 1;
                if at(answer, path).is_none() {
                    bad.push(format!("{name}: `{path}` is not in the answer of {tool}"));
                }
            }
        }
    }
    assert!(
        checked >= 60,
        "only {checked} checks: the list lost its checks"
    );
    assert!(
        bad.is_empty(),
        "UI facts missing from MCP:\n{}",
        bad.join("\n")
    );
}

#[test]
fn the_busy_job_has_the_facts_the_object_editor_shows() {
    let dir = std::env::temp_dir().join(format!(
        "tectora-mcp-parity-facts-{}-{}",
        now_ms(),
        std::process::id()
    ));
    let mut s = Server::new(Store::new(dir).unwrap());
    let id = busy_job(&mut s);
    let p = call(&mut s, "get_project", json!({ "id": id }));
    let ch = &p["placements"]["chimneys"][0];
    // add_object: the top 1 m above the roof at the chimney's middle.
    let up = ch["above_roof"]["upper"].as_f64().unwrap();
    let down = ch["above_roof"]["lower"].as_f64().unwrap();
    assert!(up > 0.5 && up < 1.0 && down > 1.0 && down < 1.5, "{ch}");
    assert!(ch.get("top_below_roof").is_none(), "{ch}");
    let slope = ch["slope_area"].as_f64().unwrap();
    assert!(
        (slope - 0.36 / 40f64.to_radians().cos()).abs() < 1e-3,
        "{ch}"
    );
    let notices = &p["input_notices"];
    assert_eq!(notices[0]["kind"], "half_hip_above_ridge", "{notices}");
    assert_eq!(p["parts"][0]["edges"][1]["clad"], true);
    let c = call(&mut s, "calc", json!({ "id": id }));
    assert_eq!(c["objects"]["chimneys"]["on_roof"], 1, "{}", c["objects"]);
    // 4 × 3 m in plan at 30°.
    let face = c["manual_faces"][0]["area_m2"].as_f64().unwrap();
    assert!(
        (face - 12.0 / 30f64.to_radians().cos()).abs() < 0.01,
        "{face}"
    );
    let r = call(&mut s, "get_review", json!({ "id": id }));
    let changes = r["pending"]["changes"]
        .as_array()
        .expect("changes since the approved start");
    assert!(
        changes.iter().any(|c| c["field"] == "chimney"),
        "{changes:?}"
    );
}
