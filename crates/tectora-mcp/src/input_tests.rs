//! MCP input checks from the 2026-11-21 audit: values of the wrong type or
//! outside the schema's enums are refused (ERR-002, ERR-005), sizes the
//! editor refuses are refused here too (ARCH-001, HUNTC-007, ARCH-003,
//! MATH-001, MATH-003), and the answers say what happened (ERR-006,
//! ERR-007, ERR-008, HUNTIO-007, ARCH-005).

use serde_json::{Value, json};
use tectora_core::store::{Store, now_ms};

use super::Server;

fn server() -> Server {
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "tectora-mcp-input-{}-{}-{n}",
        now_ms(),
        std::process::id()
    ));
    Server::new(Store::new(dir).unwrap())
}

fn hip_job(s: &mut Server) -> String {
    let r = s
        .call_tool(
            "create_standard_roof",
            &json!({ "name": "Hip", "type": "hip", "length": 12, "width": 8, "pitch_deg": 35, "summary": "start" }),
        )
        .unwrap();
    r["id"].as_str().unwrap().to_string()
}

fn gable_job(s: &mut Server) -> String {
    let r = s
        .call_tool(
            "create_standard_roof",
            &json!({ "name": "Gable", "type": "gable", "length": 12, "width": 8, "pitch_deg": 40, "eave_height": 3, "overhang": 0, "summary": "start" }),
        )
        .unwrap();
    r["id"].as_str().unwrap().to_string()
}

#[test]
fn a_misspelt_hip_neighbour_or_measure_is_refused_and_nothing_saved() {
    let mut s = server();
    let id = hip_job(&mut s);
    let before = s.store.load(&id).unwrap();
    for (key, value) in [
        ("neighbour", json!("previous")),
        ("neighbour", json!(7)),
        ("by", json!("eave")),
    ] {
        let mut args = json!({ "id": id, "edge": 0, "tool": "hip", "hip": 7.5, "first": 6, "second": 5, "apply": true, "summary": "s" });
        args[key] = value;
        let e = s.call_tool("roof_helper", &args).unwrap_err();
        assert!(e.contains(&format!("`{key}`")), "{e}");
    }
    assert_eq!(s.store.load(&id).unwrap(), before);
    // The right values still work.
    let r = s
        .call_tool(
            "roof_helper",
            &json!({ "id": id, "edge": 0, "tool": "hip", "neighbour": "prev", "by": "rafters", "hip": 7.5, "first": 6, "second": 5 }),
        )
        .unwrap();
    assert!(
        r["patches"].as_array().is_some_and(|p| !p.is_empty()),
        "{r}"
    );
}

#[test]
fn a_job_number_that_is_not_text_is_refused_by_create_and_duplicate() {
    let mut s = server();
    let e = s
        .call_tool(
            "create_project",
            &json!({ "name": "N", "summary": "s", "job": { "number": 2026017, "customer": "Novák" },
                     "parts": [{ "points": [[0, 0], [10, 0], [10, 8], [0, 8]] }] }),
        )
        .unwrap_err();
    assert!(e.contains("`job.number` must be string"), "{e}");
    let e = s
        .call_tool(
            "create_project",
            &json!({ "name": "N", "summary": "s", "notes": 42,
                     "parts": [{ "points": [[0, 0], [10, 0], [10, 8], [0, 8]] }] }),
        )
        .unwrap_err();
    assert!(e.contains("`notes`"), "{e}");
    let id = gable_job(&mut s);
    let e = s
        .call_tool(
            "duplicate_project",
            &json!({ "id": id, "name": "Copy", "job": { "number": 2026018 }, "summary": "s" }),
        )
        .unwrap_err();
    assert!(e.contains("number"), "{e}");
    let (all, _) = s.store.scan().unwrap();
    assert_eq!(all.len(), 1, "nothing new was saved");
}

#[test]
fn a_write_to_an_offered_job_says_its_stored_figures_differ() {
    let mut s = server();
    let id = gable_job(&mut s);
    s.call_tool(
        "update_job",
        &json!({ "id": id, "status": "done", "summary": "s" }),
    )
    .unwrap();
    let r = s
        .call_tool(
            "update_edges",
            &json!({ "id": id, "changes": [{ "index": 0, "pitch_deg": 30 }], "summary": "s" }),
        )
        .unwrap();
    let note = r["note"].as_str().unwrap();
    assert!(
        note.contains("`done`") && note.contains("stored offer figures"),
        "{note}"
    );
    assert!(note.contains("roof area stored"), "{note}");
    let diffs = r["stored_result"]["differences"].as_array().unwrap();
    assert!(diffs.iter().any(|d| d["key"] == "roof_area"), "{r}");
    // A draft says nothing of the kind.
    let id2 = gable_job(&mut s);
    let r = s
        .call_tool(
            "update_edges",
            &json!({ "id": id2, "changes": [{ "index": 0, "pitch_deg": 30 }], "summary": "s" }),
        )
        .unwrap();
    assert!(r.get("stored_result").is_none(), "{r}");
}

#[test]
fn editing_a_part_without_walls_says_so() {
    let mut s = server();
    let id = gable_job(&mut s);
    let mut p = s.store.load(&id).unwrap();
    p.parts[0].edges.clear();
    s.store.save(Some(id.clone()), &p).unwrap();
    let e = s
        .call_tool(
            "update_edges",
            &json!({ "id": id, "changes": [{ "index": 0, "pitch_deg": 30 }], "summary": "s" }),
        )
        .unwrap_err();
    assert!(
        e.contains("no walls") && !e.contains("18446744073709551615"),
        "{e}"
    );
}

#[test]
fn an_export_of_a_roofless_job_is_a_plain_refusal() {
    let mut s = server();
    let id = gable_job(&mut s);
    let mut p = s.store.load(&id).unwrap();
    p.parts[0].edges[0].pitch_deg = 0.5;
    s.store.save(Some(id.clone()), &p).unwrap();
    for (tool, args) in [
        ("export_list", json!({ "id": id })),
        ("export_plan", json!({ "id": id, "summary": "s" })),
    ] {
        let e = s.call_tool(tool, &args).unwrap_err();
        assert!(e.starts_with("not exported:"), "{e}");
        assert!(
            !e.contains("Saved") && !e.contains("revert") && !e.contains('{'),
            "{e}"
        );
        assert!(e.contains("pitch_range"), "{e}");
    }
}

#[test]
fn a_standard_roof_outside_the_forms_bounds_is_refused() {
    let mut s = server();
    for (l, w) in [(10.0, 0.004), (1e-300, 1e-300), (99_999.0, 99_999.0)] {
        let e = s
            .call_tool(
                "create_standard_roof",
                &json!({ "name": "T", "type": "gable", "length": l, "width": w, "pitch_deg": 40, "summary": "s" }),
            )
            .unwrap_err();
        assert!(e.contains("out of range"), "{e}");
    }
    let (all, _) = s.store.scan().unwrap();
    assert!(all.is_empty());
}

#[test]
fn a_regular_polygon_with_tiny_sides_is_refused() {
    let mut s = server();
    let id = gable_job(&mut s);
    let before = s.store.load(&id).unwrap();
    let e = s
        .call_tool(
            "add_shape_part",
            &json!({ "id": id, "shape": "regular_polygon", "sides": 64, "side": 0.01, "at": [30, 0], "summary": "s" }),
        )
        .unwrap_err();
    assert!(e.contains("side"), "{e}");
    assert_eq!(s.store.load(&id).unwrap(), before);
    s.call_tool(
        "add_shape_part",
        &json!({ "id": id, "shape": "regular_polygon", "sides": 8, "side": 1, "at": [30, 0], "summary": "s" }),
    )
    .unwrap();
}

#[test]
fn a_helper_that_gives_a_wall_no_wall_takes_saves_nothing() {
    let mut s = server();
    let id = hip_job(&mut s);
    let before = s.store.load(&id).unwrap();
    let e = s
        .call_tool(
            "roof_helper",
            &json!({ "id": id, "edge": 0, "tool": "hip", "hip": 7.072, "first": 5, "second": 5, "by": "eaves", "apply": true, "summary": "s" }),
        )
        .unwrap_err();
    // About 1.3° on both walls: the refusal names the pitch, not "no
    // pitches fit" (lengths away from the helper's limits, so every
    // platform's libm agrees).
    assert!(
        e.contains("nothing was saved") && e.contains("pitch_deg"),
        "{e}"
    );
    let e = s
        .call_tool(
            "roof_helper",
            &json!({ "id": id, "edge": 0, "tool": "build_up", "thickness": 99999, "apply": true, "summary": "s" }),
        )
        .unwrap_err();
    assert!(e.contains("nothing was saved"), "{e}");
    assert_eq!(s.store.load(&id).unwrap(), before);
}

#[test]
fn a_dormer_below_the_roof_or_flatter_than_2_degrees_is_refused() {
    let mut s = server();
    let id = gable_job(&mut s);
    let before = s.store.load(&id).unwrap();
    // The roof at y = 1.6 is 4.34 m high (40°, no overhang).
    let e = s
        .call_tool(
            "add_dormer",
            &json!({ "id": id, "kind": "gable", "start": [4, 1.6], "eave_height": 3.9, "summary": "s" }),
        )
        .unwrap_err();
    assert!(e.starts_with("dormer_below_roof"), "{e}");
    for args in [
        json!({ "id": id, "kind": "gable", "start": [4, 1.6], "pitch_deg": 1, "summary": "s" }),
        json!({ "id": id, "kind": "shed", "start": [4, 1.6], "front_pitch_deg": 1, "summary": "s" }),
    ] {
        let e = s.call_tool("add_dormer", &args).unwrap_err();
        assert!(e.starts_with("dormer_pitch"), "{e}");
    }
    assert_eq!(s.store.load(&id).unwrap(), before);
    s.call_tool(
        "add_dormer",
        &json!({ "id": id, "kind": "gable", "start": [4, 1.6], "pitch_deg": 2, "summary": "s" }),
    )
    .unwrap();
}

#[test]
fn a_request_that_is_not_an_object_gets_invalid_request() {
    let mut s = server();
    for line in [&b"\"str\""[..], b"5", b"true", b"null"] {
        let r = s.respond_line(line).unwrap();
        assert_eq!(r["error"]["code"], -32600, "{r}");
        assert_eq!(r["id"], Value::Null);
    }
    let r = s.respond_line(br#"{"jsonrpc":"2.0","id":13}"#).unwrap();
    assert_eq!(
        (r["error"]["code"].clone(), r["id"].clone()),
        (json!(-32600), json!(13))
    );
    // A batch of them gets one answer each.
    let r = s
        .respond_line(br#"[1, {"jsonrpc":"2.0","id":2,"method":"ping"}]"#)
        .unwrap();
    assert_eq!(r.as_array().unwrap().len(), 2);
}

#[test]
fn update_settings_explains_a_refused_pitch_and_publishes_its_range() {
    let tools = super::tools();
    let t = tools
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "update_settings")
        .unwrap();
    let pitch = &t["inputSchema"]["properties"]["roof_defaults"]["properties"]["pitch_deg"];
    assert_eq!(pitch["minimum"], json!(tectora_core::model::MIN_PITCH_DEG));
    let e = tectora_core::settings::SettingsError::InvalidValue("pitch_deg").explain();
    assert!(e.contains("from 2 to 89"), "{e}");
    assert!(!e.contains("invalid_value:"), "{e}");
}

#[test]
fn a_gable_made_an_eave_takes_the_neighbours_pitch() {
    let mut s = server();
    let id = hip_job(&mut s);
    s.call_tool(
        "update_edges",
        &json!({ "id": id, "changes": [{ "index": 1, "gable": true }], "summary": "s" }),
    )
    .unwrap();
    s.call_tool(
        "update_edges",
        &json!({ "id": id, "changes": [{ "index": 1, "gable": false }], "summary": "s" }),
    )
    .unwrap();
    let p = s.store.load(&id).unwrap();
    assert_eq!(p.parts[0].edges[1].pitch_deg, 35.0);
}
