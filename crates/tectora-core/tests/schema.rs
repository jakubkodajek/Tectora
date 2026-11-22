//! The committed JSON Schema matches the model. Regenerate it with
//! `TECTORA_WRITE_SCHEMA=1 cargo test -p tectora-core --test schema`.

use std::path::PathBuf;

#[test]
fn committed_schema_matches_the_model() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/tectora.schema.json");
    let text = serde_json::to_string_pretty(&tectora_core::model::json_schema()).unwrap() + "\n";
    if std::env::var_os("TECTORA_WRITE_SCHEMA").is_some() {
        std::fs::write(&path, &text).unwrap();
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        committed == text,
        "docs/tectora.schema.json is out of date; run TECTORA_WRITE_SCHEMA=1 cargo test -p tectora-core --test schema"
    );
}

#[test]
fn examples_validate_against_the_schema() {
    // The schema accepts what the model writes: every required key is there.
    let schema = tectora_core::model::json_schema();
    let required: Vec<&str> = schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    for ex in tectora_core::examples::EXAMPLES {
        let v = serde_json::to_value(ex.project()).unwrap();
        for key in &required {
            assert!(v.get(*key).is_some(), "{} lacks {key}", ex.id);
        }
    }
}

// ---- format versions (ARCH-003, TEST-011) -------------------------------

use serde_json::{Value, json};
use tectora_core::model::{MAX_EXTRA_BYTES, Project, SCHEMA_VERSION};
use tectora_core::store::Store;

/// Drops the `description` texts: only names and types make a format.
fn strip_docs(v: &mut Value) {
    match v {
        Value::Object(m) => {
            m.remove("description");
            m.values_mut().for_each(strip_docs);
        }
        Value::Array(a) => a.iter_mut().for_each(strip_docs),
        _ => {}
    }
}

/// FNV-1a, enough to notice a changed format.
fn fnv(text: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in text.bytes() {
        h = (h ^ u64::from(b)).wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

#[test]
fn every_model_change_has_a_format_version() {
    // A field added without raising SCHEMA_VERSION is dropped silently by the
    // release before it (ARCH-003). Each version's format is recorded in
    // `tests/fixtures/schema_versions.json`; change the model, raise the
    // version, add the new hash there (the failure prints it).
    let mut schema = tectora_core::model::json_schema();
    strip_docs(&mut schema);
    let hash = fnv(&schema.to_string());
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/schema_versions.json");
    let recorded: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let key = SCHEMA_VERSION.to_string();
    assert_eq!(
        recorded[&key].as_str(),
        Some(hash.as_str()),
        "the model changed without a new format version: raise SCHEMA_VERSION in model.rs \
         (with a migration if needed) and record \"{key}\": \"{hash}\" in tests/fixtures/schema_versions.json"
    );
}

fn gable() -> Project {
    tectora_core::examples::find("gable").unwrap().project()
}

#[test]
fn unknown_top_level_keys_survive_load_and_save() {
    let mut v = serde_json::to_value(gable()).unwrap();
    v["future_field"] = json!({"a": [1, 2.5, "x"], "b": null});
    v["another"] = json!("kept");
    let p = Project::from_json(&v.to_string()).unwrap();
    assert_eq!(p.extra.len(), 2);
    let back: Value = serde_json::from_str(&p.to_json().unwrap()).unwrap();
    assert_eq!(back["future_field"], v["future_field"]);
    assert_eq!(back["another"], v["another"]);
    // Through the store, which is how the app and the MCP server save.
    let dir = std::env::temp_dir().join(format!("tectora-schema-test-{}", std::process::id()));
    let store = Store::new(dir).unwrap();
    let id = store.save(None, &p).unwrap();
    assert_eq!(store.load(&id).unwrap().extra, p.extra);
}

#[test]
fn unknown_keys_are_bounded_and_never_reach_the_calculation() {
    let mut p = gable();
    assert!(p.check_limits().is_ok());
    p.extra
        .insert("big".into(), Value::String("x".repeat(MAX_EXTRA_BYTES + 1)));
    assert_eq!(p.check_input().unwrap_err().code, "too_many");
    p.extra.clear();
    p.extra.insert("huge".into(), json!(1e300));
    assert_eq!(p.check_input().unwrap_err().code, "out_of_range");
    // The roof does not depend on them.
    p.extra.clear();
    let area = tectora_core::quantities::roof_totals(&p).roof_area;
    p.extra.insert("note".into(), json!({"x": 1}));
    assert_eq!(tectora_core::quantities::roof_totals(&p).roof_area, area);
}

#[test]
fn an_older_format_migrates_and_a_newer_one_is_refused() {
    let mut v = serde_json::to_value(gable()).unwrap();
    for old in [1u32, 2] {
        v["schema_version"] = old.into();
        let p = Project::from_json(&v.to_string()).unwrap();
        assert_eq!(p.schema_version, SCHEMA_VERSION);
    }
    v["schema_version"] = (SCHEMA_VERSION + 1).into();
    let err = Project::from_json(&v.to_string()).unwrap_err().to_string();
    assert!(err.contains("newer"), "{err}");
    // And in a library the file is refused as `newer_format`, not dropped.
    let dir = std::env::temp_dir().join(format!("tectora-schema-newer-{}", std::process::id()));
    let store = Store::new(dir.clone()).unwrap();
    std::fs::write(dir.join("later.tectora"), v.to_string()).unwrap();
    assert_eq!(store.load("later").unwrap_err().code(), "newer_format");
    // TEST-306 R7: the newer version written as a float is refused by the
    // store as well, and a save leaves the file as it is.
    let text = v.to_string().replace(
        &format!("\"schema_version\":{}", SCHEMA_VERSION + 1),
        &format!("\"schema_version\":{}.0", SCHEMA_VERSION + 1),
    );
    assert!(text.contains(".0"), "{text}");
    std::fs::write(dir.join("float.tectora"), &text).unwrap();
    assert_eq!(store.load("float").unwrap_err().code(), "newer_format");
    let e = store.save(Some("float".into()), &gable()).unwrap_err();
    assert_eq!(e.code(), "newer_format");
    assert_eq!(
        std::fs::read_to_string(dir.join("float.tectora")).unwrap(),
        text
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_schema_version_that_is_not_a_whole_number_is_refused() {
    // Round-1 TEST-008: a damaged version must not open as format 1, or a
    // newer file whose version is no integer slips past `newer_format`.
    let mut v = serde_json::to_value(gable()).unwrap();
    let bad: [Value; 5] = [
        "99".into(),
        "1".into(),
        99.5.into(),
        (-1).into(),
        serde_json::from_str("18446744073709551617").unwrap(),
    ];
    for version in bad {
        v["schema_version"] = version.clone();
        assert!(
            Project::from_json(&v.to_string()).is_err(),
            "schema_version {version} opened"
        );
    }
    // A file without the field is a format-1 file and still opens.
    v.as_object_mut().unwrap().remove("schema_version");
    let p = Project::from_json(&v.to_string()).unwrap();
    assert_eq!(p.schema_version, SCHEMA_VERSION);
}

#[test]
fn files_saved_by_older_releases_open_resave_and_calculate() {
    // TEST-011: the committed files of format 1 and 2 as older releases
    // wrote them (no `height`, `roof_openings`, `manual_faces`) go through
    // the store's load, which migrates them.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/check");
    let dir = std::env::temp_dir().join(format!("tectora-schema-old-{}", std::process::id()));
    let store = Store::new(dir.clone()).unwrap();
    let mut seen = [0usize; 2];
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let raw: Value = serde_json::from_str(&text).unwrap();
        let version = raw["schema_version"].as_u64().unwrap();
        seen[usize::from(version == 2)] += 1;
        assert!(version < u64::from(SCHEMA_VERSION), "{path:?}");
        let id = path.file_stem().unwrap().to_string_lossy().to_string();
        std::fs::write(dir.join(format!("{id}.tectora")), &text).unwrap();
        let loaded = store.load(&id).unwrap();
        assert_eq!(loaded.schema_version, SCHEMA_VERSION, "{id}");
        // Same roof as when the file is read without migrating.
        let unmigrated: Project = serde_json::from_value(raw).unwrap();
        let area = |p: &Project| tectora_core::quantities::roof_totals(p).roof_area;
        assert_eq!(area(&loaded), area(&unmigrated), "{id}");
        // It saves in the current format and opens again.
        store.save(Some(id.clone()), &loaded).unwrap();
        assert_eq!(store.load(&id).unwrap(), loaded);
    }
    assert!(seen[0] > 0, "a version 1 fixture");
}

#[test]
fn a_project_made_at_an_older_version_is_written_in_the_current_one() {
    // The editor creates new projects stamped with an old version; the file
    // must still say the current one, or older releases would open it.
    let mut p = gable();
    p.schema_version = 1;
    let v: Value = serde_json::from_str(&p.to_json().unwrap()).unwrap();
    assert_eq!(v["schema_version"], SCHEMA_VERSION);
}
