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

/// Audit 2026-11-21 ARCH-104, ARCH-105: a summary of invisible characters
/// or a very long one is refused, and so are control and bidirectional
/// characters in the job's number, customer and address.
#[test]
fn invisible_or_long_summaries_and_control_characters_in_job_texts_are_refused() {
    let mut s = server();
    let id = gable_job(&mut s);
    for summary in [
        "\u{200b}",
        " \u{2060}\t",
        &"x".repeat(501),
        "Fix\u{202e}gnp",
    ] {
        let e = s
            .call_tool(
                "update_job",
                &json!({ "id": id, "number": "7", "summary": summary }),
            )
            .unwrap_err();
        assert!(e.contains("summary"), "{e}");
    }
    for (key, text) in [
        ("customer", "A\u{0}\u{202e}B"),
        ("address", "Main\nStreet"),
        ("number", "\u{200f}12"),
    ] {
        let e = s
            .call_tool(
                "update_job",
                &json!({ "id": id, key: text, "summary": "Customer from the order" }),
            )
            .unwrap_err();
        assert!(e.contains(key), "{e}");
    }
    let e = s
        .call_tool(
            "create_project",
            &json!({ "name": "N", "summary": "s", "job": { "customer": "A\u{0}B" },
                     "parts": [{ "points": [[0, 0], [10, 0], [10, 8], [0, 8]] }] }),
        )
        .unwrap_err();
    assert!(e.contains("customer"), "{e}");
    // Notes keep their line breaks; plain text is saved.
    s.call_tool(
        "update_job",
        &json!({ "id": id, "customer": "Novák", "notes": "Line one\nLine two", "summary": "Customer from the order" }),
    )
    .unwrap();
    let p = s.store.load(&id).unwrap();
    assert_eq!(
        (p.job.customer.as_str(), p.notes.as_str()),
        ("Novák", "Line one\nLine two")
    );
}

/// Audit 2026-11-21 HUNT-108 item 2: a footprint written as a closed
/// polygon (the first corner repeated at the end, as most tools write it)
/// gets four walls, not a fifth wall of no length that leaves the building
/// out.
#[test]
fn a_repeated_closing_corner_is_dropped() {
    let mut s = server();
    let r = s
        .call_tool(
            "create_project",
            &json!({ "name": "Closed", "summary": "Footprint from the drawing",
                     "parts": [{ "points": [[0, 0], [12, 0], [12, 8], [0, 8], [0, 0]] }] }),
        )
        .unwrap();
    assert!(r.get("roof_trouble").is_none(), "{r}");
    let id = r["id"].as_str().unwrap().to_string();
    assert_eq!(s.store.load(&id).unwrap().parts[0].edges.len(), 4);
    let r = s
        .call_tool(
            "set_outline",
            &json!({ "id": id, "summary": "Wider footprint",
                     "points": [[0, 0], [14, 0], [14, 8], [0, 8], [0, 0]] }),
        )
        .unwrap();
    assert!(r.get("roof_trouble").is_none(), "{r}");
    assert_eq!(s.store.load(&id).unwrap().parts[0].edges.len(), 4);
}

/// Audit 2026-11-21 HUNT-108 item 5: a standard roof's pitch of 90 is
/// refused with its real limit (below 90, at least 2), not "between 0 and
/// 90".
#[test]
fn a_standard_roof_pitch_is_refused_with_its_real_limits() {
    let mut s = server();
    for pitch in [90.0, 1.99] {
        let e = s
            .call_tool(
                "create_standard_roof",
                &json!({ "name": "A", "type": "gable", "length": 10, "width": 8, "pitch_deg": pitch, "summary": "Standard roof" }),
            )
            .unwrap_err();
        assert!(
            e.contains("pitch_deg") && !e.contains("between 0"),
            "{pitch}: {e}"
        );
    }
}

/// Audit 2026-11-21 TEST-205: only a closing corner within 1 cm of the
/// first is dropped; a real short last wall (a 30 cm jog) stays, and so
/// does its `edges` entry.
#[test]
fn a_short_last_wall_is_not_taken_for_a_repeated_closing_corner() {
    let mut s = server();
    let pts = json!([[0, 0], [12, 0], [12, 8], [0, 8], [0, 0.3]]);
    let r = s
        .call_tool(
            "create_project",
            &json!({ "name": "Jog", "summary": "Footprint with a jog",
                     "parts": [{ "points": pts }] }),
        )
        .unwrap();
    let id = r["id"].as_str().unwrap().to_string();
    let part = &s.store.load(&id).unwrap().parts[0];
    assert_eq!(part.edges.len(), 5);
    let last = part.edges[4].start.distance(part.edges[0].start);
    assert!((last - 0.3).abs() < 1e-9, "{last}");
    let edges: Vec<Value> = (0..5).map(|i| json!({ "pitch_deg": 30 + i })).collect();
    let r = s
        .call_tool(
            "create_project",
            &json!({ "name": "Jog 2", "summary": "Footprint with a jog",
                     "parts": [{ "points": pts, "edges": edges }] }),
        )
        .unwrap();
    let id = r["id"].as_str().unwrap().to_string();
    let part = &s.store.load(&id).unwrap().parts[0];
    assert_eq!(part.edges.len(), 5);
    assert_eq!(part.edges[4].pitch_deg, 34.0);
}

/// Audit 2026-11-21 TEST-206: a job text the job already had (an imported
/// job with a tab in its customer) does not block other writes; a new
/// text with one is refused.
#[test]
fn a_job_text_it_already_had_does_not_block_other_writes() {
    let mut s = server();
    let id = hip_job(&mut s);
    let mut p = s.store.load(&id).unwrap();
    p.job.customer = "A\tB".into();
    p.parts[0].name = "Wing\u{202e}".into();
    s.store.save(Some(id.clone()), &p).unwrap();
    s.call_tool(
        "update_edges",
        &json!({ "id": id, "changes": [{ "index": 0, "pitch_deg": 30 }], "summary": "Steeper front" }),
    )
    .unwrap();
    let e = s
        .call_tool(
            "update_job",
            &json!({ "id": id, "customer": "A\tC", "summary": "Customer" }),
        )
        .unwrap_err();
    assert!(e.contains("customer"), "{e}");
}

/// Audit 2026-11-21 TEST-207: notes up to 1 000 000 characters are kept,
/// one more is refused naming notes.
#[test]
fn notes_are_capped_at_a_million_characters() {
    let mut s = server();
    let id = hip_job(&mut s);
    s.call_tool(
        "update_job",
        &json!({ "id": id, "notes": "n".repeat(1_000_000), "summary": "Notes" }),
    )
    .unwrap();
    let e = s
        .call_tool(
            "update_job",
            &json!({ "id": id, "notes": "n".repeat(1_000_001), "summary": "Notes" }),
        )
        .unwrap_err();
    assert!(e.contains("notes"), "{e}");
}

/// Audit 2026-11-21 ARCH-202: the job name of `create_from_example`, the
/// job texts of `duplicate_project`, part names and material names keep
/// the text rules of `update_job`.
#[test]
fn every_write_keeps_the_text_rules() {
    let mut s = server();
    let e = s
        .call_tool(
            "create_from_example",
            &json!({ "example_id": "gable", "name": "Ex\u{202e}elpmaxe\u{0}", "summary": "Example" }),
        )
        .unwrap_err();
    assert!(e.contains("name"), "{e}");
    let id = hip_job(&mut s);
    for job in [
        json!({ "customer": "C\u{0}\u{202e}D" }),
        json!({ "address": "x".repeat(100_000) }),
        json!({ "number": "1\n2" }),
    ] {
        let e = s
            .call_tool(
                "duplicate_project",
                &json!({ "id": id, "name": "Copy", "job": job, "summary": "Copy for another customer" }),
            )
            .unwrap_err();
        assert!(
            e.contains("number") || e.contains("customer") || e.contains("address"),
            "{e}"
        );
    }
    let e = s
        .call_tool(
            "set_roof_options",
            &json!({ "id": id, "material_names": ["T\u{202e}iles\u{0}"], "summary": "Material" }),
        )
        .unwrap_err();
    assert!(e.contains("material name"), "{e}");
    let e = s
        .call_tool(
            "set_roof_options",
            &json!({ "id": id, "material_names": ["m".repeat(201)], "summary": "Material" }),
        )
        .unwrap_err();
    assert!(e.contains("material name"), "{e}");
    let e = s
        .call_tool(
            "create_project",
            &json!({ "name": "Wings", "summary": "Footprint",
                     "parts": [{ "name": "Wing\u{202e}\u{0}", "points": [[0, 0], [12, 0], [12, 8], [0, 8]] }] }),
        )
        .unwrap_err();
    assert!(e.contains("part name"), "{e}");
}

/// Audit 2026-11-21 ARCH-203: a summary or job name of blank-looking
/// characters only is empty, and a summary is one line (no U+2028).
#[test]
fn blank_looking_summaries_and_names_are_refused() {
    let mut s = server();
    let id = hip_job(&mut s);
    for summary in [
        "\u{3164}\u{3164}",
        "\u{2800}",
        "\u{115f}\u{1160}",
        "\u{34f}",
        "\u{fe0f}",
        "Fix\u{2028}second line",
    ] {
        let e = s
            .call_tool(
                "update_job",
                &json!({ "id": id, "notes": "x", "summary": summary }),
            )
            .unwrap_err();
        assert!(e.contains("summary"), "{summary:?}: {e}");
    }
    let e = s
        .call_tool(
            "create_project",
            &json!({ "name": "\u{3164}", "summary": "Footprint",
                     "parts": [{ "points": [[0, 0], [12, 0], [12, 8], [0, 8]] }] }),
        )
        .unwrap_err();
    assert!(e.contains("name"), "{e}");
}

/// Audit 2026-11-21 TEST-203: a job of hand-drawn faces only can be set
/// to Offer, and its figures are stored.
#[test]
fn a_job_of_hand_drawn_faces_only_stores_its_offer_figures() {
    let mut s = server();
    let p = tectora_core::Project::from_json(
        &json!({
            "name": "Faces", "parts": [],
            "manual_faces": [{ "points": [{"x": 0, "y": 0}, {"x": 4, "y": 0}, {"x": 4, "y": 4}, {"x": 0, "y": 4}], "pitch_deg": 30 }],
        })
        .to_string(),
    )
    .unwrap();
    let id = s.store.save(None, &p).unwrap();
    s.call_tool(
        "update_job",
        &json!({ "id": id, "status": "offer", "summary": "Offer sent" }),
    )
    .unwrap();
    let snap = s.store.load(&id).unwrap().result_snapshot.expect("stored");
    assert!(snap.roof_area > 18.0, "{}", snap.roof_area);
}
