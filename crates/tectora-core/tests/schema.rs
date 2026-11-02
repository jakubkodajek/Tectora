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
