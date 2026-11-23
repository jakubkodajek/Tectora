//! Jobs whose numbers JavaScript changes on the app's round trip
//! (audit 2026-11-22 HUNT-802, HUNT-804, Design B).
//!
//! The app reads a job with `JSON.parse` and sends it back with
//! `JSON.stringify` as the base of its checked save. Every number becomes
//! an f64 there: `1.0` comes back as `1`, `-0.0` as `0`, `1e3` as `1000`,
//! and an integer above 2^53 as the nearest f64. `save_checked` must take
//! such a base for the file it came from, or the job can never be saved
//! from the app. MCP's checked save has no JavaScript in between and stays
//! exact. A stored result dated after the last date a calendar shows is
//! dropped when the job is read, with a warning; the job still opens.

use serde_json::{Value, json};
use tectora_core::Project;
use tectora_core::store::{CheckedSave, Store};

fn temp_store(name: &str) -> Store {
    let ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("tectora-js-{name}-{ns}"));
    Store::new(dir)
        .unwrap()
        .with_summary_budget(std::time::Duration::from_secs(60))
}

/// Every number as JavaScript keeps it: an f64, a whole one written
/// without a fraction.
fn js_numbers(v: Value) -> Value {
    match v {
        Value::Number(n) => {
            let f = n.as_f64().unwrap();
            if f.fract() == 0.0 && f.abs() < 9.2e18 {
                if f < 0.0 {
                    json!(f as i64)
                } else {
                    json!(f as u64)
                }
            } else {
                json!(f)
            }
        }
        Value::Array(a) => Value::Array(a.into_iter().map(js_numbers).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, js_numbers(v))).collect()),
        other => other,
    }
}

/// The project as the app sends it back after `JSON.parse` /
/// `JSON.stringify`.
fn through_js(p: &Project) -> Project {
    let v = js_numbers(serde_json::to_value(p).unwrap());
    serde_json::from_value(v).unwrap()
}

/// A job file with a stored result taken at `taken_ms` and `extra` as its
/// unknown top-level keys, written as raw text, the way another tool would.
fn plant(store: &Store, id: &str, taken_ms: &str, extra: &str) {
    let p = tectora_core::examples::find("hip").unwrap().project();
    let mut v = serde_json::to_value(&p).unwrap();
    v["result_snapshot"] = json!({
        "engine_version": tectora_core::snapshot::ENGINE_VERSION,
        "taken_ms": 1,
        "roof_area": 120.5,
        "ridge_height": 6.2,
        "lines_by_code": { "T": 30.0 },
    });
    let text = serde_json::to_string_pretty(&v).unwrap();
    let text = text.replacen("\"taken_ms\": 1", &format!("\"taken_ms\": {taken_ms}"), 1);
    let text = match extra {
        "" => text,
        e => text.replacen('{', &format!("{{ {e},"), 1),
    };
    std::fs::write(store.dir.join(format!("{id}.tectora")), text).unwrap();
}

/// The editor's save of a notes edit: load, through JS, checked save.
fn app_save(store: &Store, id: &str) -> CheckedSave {
    let base = through_js(&store.load(id).unwrap());
    let mut mine = base.clone();
    mine.notes = "typed in the app".into();
    store.save_checked(id, &mine, &base).unwrap()
}

#[test]
fn an_app_save_of_a_job_with_whole_floats_in_unknown_keys_is_saved() {
    let store = temp_store("floats");
    for (id, extra) in [
        ("ext-float", r#""vendor_scale": 1.0"#),
        ("ext-neg0", r#""vendor_z": -0.0"#),
        ("ext-exp", r#""vendor_e": 1e3"#),
        ("ext-nested", r#""vendor_o": {"a": [2.0, {"b": 1E2}]}"#),
    ] {
        plant(&store, id, "1700000000000", extra);
        assert!(
            matches!(app_save(&store, id), CheckedSave::Saved { .. }),
            "{id}: the app's save was refused"
        );
        assert_eq!(store.load(id).unwrap().notes, "typed in the app", "{id}");
    }
}

#[test]
fn an_app_save_still_refuses_a_change_saved_meanwhile() {
    let store = temp_store("still");
    plant(&store, "j", "1700000000000", r#""vendor_scale": 1.0"#);
    let base = through_js(&store.load("j").unwrap());
    // Another writer changes the unknown key's value and the notes.
    let mut other = store.load("j").unwrap();
    other.notes = "from the AI".into();
    other.extra.insert("vendor_scale".into(), json!(1.5));
    store.save(Some("j".into()), &other).unwrap();
    let mut mine = base.clone();
    mine.notes = "typed in the app".into();
    assert!(matches!(
        store.save_checked("j", &mine, &base).unwrap(),
        CheckedSave::Conflict { .. }
    ));
    // A number that differs in the f64 a JS client sees is a change too.
    let mut moved = store.load("j").unwrap();
    moved
        .extra
        .insert("vendor_scale".into(), json!(1.5000000000000002));
    store.save(Some("j".into()), &moved).unwrap();
    assert!(matches!(
        store.save_checked("j", &mine, &through_js(&other)).unwrap(),
        CheckedSave::Conflict { .. }
    ));
}

#[test]
fn mcp_checked_saves_compare_exactly() {
    let store = temp_store("exact");
    plant(&store, "j", "1700000000000", r#""vendor_scale": 1.0"#);
    let base = through_js(&store.load("j").unwrap());
    let mut mine = base.clone();
    mine.notes = "from the AI".into();
    assert!(matches!(
        store
            .save_as_checked("j", &mine, "claude", "Notes", Some(&base))
            .unwrap(),
        CheckedSave::Conflict { .. }
    ));
}

#[test]
fn a_stored_result_dated_after_the_last_calendar_date_is_dropped_and_the_job_saves() {
    let store = temp_store("taken");
    let max = tectora_core::snapshot::MAX_TAKEN_MS;
    assert_eq!(max, 8_640_000_000_000_000);
    for (id, taken) in [
        ("cas-53", "9007199254740993".to_string()),
        ("cas-above", (max + 1).to_string()),
        ("cas-max", u64::MAX.to_string()),
    ] {
        plant(&store, id, &taken, "");
        let p = store.load(id).unwrap();
        assert_eq!(p.result_snapshot, None, "{id}: the stored result was kept");
        let warnings = store.load_warnings(id).unwrap();
        assert_eq!(
            warnings.iter().map(|w| w.code).collect::<Vec<_>>(),
            ["result_dropped"],
            "{id}"
        );
        assert!(warnings[0].detail.contains(&taken), "{:?}", warnings[0]);
        assert!(
            matches!(app_save(&store, id), CheckedSave::Saved { .. }),
            "{id}: the app's save was refused"
        );
        let saved = store.load(id).unwrap();
        assert_eq!(saved.notes, "typed in the app");
        // Saved without it: the next open has nothing to warn about.
        assert!(store.load_warnings(id).unwrap().is_empty(), "{id}");
        let text = std::fs::read_to_string(store.dir.join(format!("{id}.tectora"))).unwrap();
        assert!(!text.contains("result_snapshot"), "{id}");
    }
}

#[test]
fn a_stored_result_at_the_last_calendar_date_is_kept() {
    let store = temp_store("edge");
    let max = tectora_core::snapshot::MAX_TAKEN_MS;
    plant(&store, "j", &max.to_string(), "");
    let p = store.load("j").unwrap();
    assert_eq!(p.result_snapshot.as_ref().unwrap().taken_ms, max);
    assert!(store.load_warnings("j").unwrap().is_empty());
    assert!(matches!(app_save(&store, "j"), CheckedSave::Saved { .. }));
    assert!(store.load("j").unwrap().result_snapshot.is_some());
}

#[test]
fn load_warnings_of_a_job_that_is_not_there_is_an_error_not_a_panic() {
    let store = temp_store("missing");
    assert!(store.load_warnings("no-such-job").is_err());
    assert!(store.load_warnings("../x").is_err());
}

#[test]
fn an_imported_job_with_an_impossible_result_time_comes_in_without_it() {
    let from = temp_store("imp-from");
    plant(&from, "cas-53", "9007199254740993", "");
    let store = temp_store("imp-to");
    let id = tectora_core::bundle::open_path(&store, &from.dir.join("cas-53.tectora")).unwrap();
    assert_eq!(store.load(&id).unwrap().result_snapshot, None);
    let text = std::fs::read_to_string(store.dir.join(format!("{id}.tectora"))).unwrap();
    assert!(!text.contains("result_snapshot"));
}

mod fuzz {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 96,
            rng_seed: proptest::test_runner::RngSeed::Fixed(20261122),
            failure_persistence: None,
            ..ProptestConfig::default()
        })]

        /// Job files with numbers replaced by ones JavaScript changes
        /// (whole floats, -0.0, exponents, integers above 2^53) and by
        /// impossible result times: every one the store reads can be saved
        /// from the app with the base the app sends back.
        #[test]
        fn every_job_the_store_reads_saves_from_the_app(
            picks in prop::collection::vec((any::<prop::sample::Index>(), prop_oneof![
                Just("1.0"), Just("-0.0"), Just("1e3"), Just("2.50"), Just("0.0"),
                Just("9007199254740993"), Just("8640000000000001"),
                Just("18446744073709551615"), Just("1E2"),
            ]), 1..5),
        ) {
            let store = temp_store("fuzz");
            plant(&store, "seed", "1700000000000", r#""vendor_o": {"a": [2.5, 7]}, "vendor_s": 3"#);
            let text = std::fs::read_to_string(store.dir.join("seed.tectora")).unwrap();
            let bytes = text.as_bytes();
            let mut spans = Vec::new();
            let mut i = 0;
            while i < bytes.len() {
                if bytes[i] == b'-' || bytes[i].is_ascii_digit() {
                    let s = i;
                    while i < bytes.len() && (bytes[i].is_ascii_digit() || b"-+.eE".contains(&bytes[i])) {
                        i += 1;
                    }
                    spans.push((s, i));
                } else {
                    i += 1;
                }
            }
            let mut chosen: Vec<(usize, &str)> = picks.iter().map(|(ix, v)| (ix.index(spans.len()), *v)).collect();
            chosen.sort_by_key(|c| std::cmp::Reverse(c.0));
            chosen.dedup_by_key(|c| c.0);
            let mut text = text.clone();
            for (k, v) in chosen {
                let (s, e) = spans[k];
                text.replace_range(s..e, v);
            }
            std::fs::write(store.dir.join("j.tectora"), &text).unwrap();
            if let Ok(p) = store.load("j") {
                if let Some(s) = &p.result_snapshot {
                    prop_assert!(s.taken_ms <= tectora_core::snapshot::MAX_TAKEN_MS);
                }
                let base: Project = serde_json::from_value(js_numbers(serde_json::to_value(&p).unwrap()))
                    .map_err(|e| TestCaseError::fail(format!("the app's base does not read: {e}\n{text}")))?;
                let mut mine = base.clone();
                mine.notes = "typed in the app".into();
                let r = store.save_checked("j", &mine, &base);
                prop_assert!(matches!(r, Ok(CheckedSave::Saved { .. })), "{r:?}\n{text}");
            }
            let _ = std::fs::remove_dir_all(&store.dir);
        }
    }
}

/// Audit 2026-11-23 TEST-901: a key or an array element another writer
/// removed meanwhile is a change the app's checked save must not write
/// back over (`same_for_js` compares lengths, not only the common part).
#[test]
fn an_app_save_refuses_a_key_or_element_removed_meanwhile() {
    let store = temp_store("removed");
    plant(
        &store,
        "j",
        "1700000000000",
        r#""vendor_a": 1, "vendor_b": 2, "vendor_l": [1, 2, 3]"#,
    );
    let base = through_js(&store.load("j").unwrap());
    let mut mine = base.clone();
    mine.notes = "typed in the app".into();
    // Another writer removes a key.
    let mut other = store.load("j").unwrap();
    other.extra.remove("vendor_b");
    store.save(Some("j".into()), &other).unwrap();
    assert!(matches!(
        store.save_checked("j", &mine, &base).unwrap(),
        CheckedSave::Conflict { .. }
    ));
    // And one that shortens an array.
    let mut other = store.load("j").unwrap();
    other.extra.insert("vendor_b".into(), json!(2));
    other.extra.insert("vendor_l".into(), json!([1, 2]));
    store.save(Some("j".into()), &other).unwrap();
    assert!(matches!(
        store.save_checked("j", &mine, &base).unwrap(),
        CheckedSave::Conflict { .. }
    ));
    assert_eq!(store.load("j").unwrap().notes, "");
}
