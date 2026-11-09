//! Three-way merge of project JSON, used to revert a recorded change while
//! keeping the edits made after it (`Store::revert_to`).
//!
//! `base` is the version the change produced, `ours` is the current file
//! (base plus the user's later edits), `theirs` is the version before the
//! change. Values only one side changed are taken from that side. Where both
//! sides changed the same value in different ways the merge refuses and
//! names the place: the later edit and the revert disagree, and neither may
//! win silently. Lists of the same length merge item by item; a list whose
//! length changed on both sides is a conflict.

use serde_json::{Map, Value};

/// Where both sides changed the same value: JSON paths such as
/// `parts[0].edges[3].pitch_deg`.
pub type Conflicts = Vec<String>;

pub fn merge3(base: &Value, ours: &Value, theirs: &Value) -> Result<Value, Conflicts> {
    let mut conflicts = Vec::new();
    let v = merge_value(base, ours, theirs, "", &mut conflicts);
    if conflicts.is_empty() {
        Ok(v)
    } else {
        Err(conflicts)
    }
}

/// Treats a missing key, `null` and an empty list alike (the store leaves
/// them out of its files).
fn same(a: &Value, b: &Value) -> bool {
    fn empty(v: &Value) -> bool {
        v.is_null() || v.as_array().is_some_and(Vec::is_empty)
    }
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let keys = x.keys().chain(y.keys());
            keys.into_iter().all(|k| {
                same(
                    x.get(k).unwrap_or(&Value::Null),
                    y.get(k).unwrap_or(&Value::Null),
                )
            })
        }
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| same(a, b))
        }
        _ => (empty(a) && empty(b)) || a == b,
    }
}

fn merge_value(
    base: &Value,
    ours: &Value,
    theirs: &Value,
    path: &str,
    conflicts: &mut Conflicts,
) -> Value {
    if same(ours, base) {
        return theirs.clone();
    }
    if same(theirs, base) || same(ours, theirs) {
        return ours.clone();
    }
    match (ours, theirs) {
        (Value::Object(o), Value::Object(t)) => {
            let empty = Map::new();
            let b = base.as_object().unwrap_or(&empty);
            let mut out = Map::new();
            let mut seen = std::collections::HashSet::new();
            for k in o.keys().chain(t.keys()) {
                if !seen.insert(k) {
                    continue;
                }
                let at = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                let v = merge_value(
                    b.get(k).unwrap_or(&Value::Null),
                    o.get(k).unwrap_or(&Value::Null),
                    t.get(k).unwrap_or(&Value::Null),
                    &at,
                    conflicts,
                );
                if !v.is_null() {
                    out.insert(k.clone(), v);
                }
            }
            Value::Object(out)
        }
        (Value::Array(o), Value::Array(t))
            if base
                .as_array()
                .is_some_and(|b| b.len() == o.len() && b.len() == t.len()) =>
        {
            let b = base.as_array().map(Vec::as_slice).unwrap_or_default();
            Value::Array(
                b.iter()
                    .zip(o)
                    .zip(t)
                    .enumerate()
                    .map(|(i, ((b, o), t))| {
                        merge_value(b, o, t, &format!("{path}[{i}]"), conflicts)
                    })
                    .collect(),
            )
        }
        _ => {
            conflicts.push(if path.is_empty() {
                "/".into()
            } else {
                path.to_string()
            });
            ours.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn takes_each_side_where_only_it_changed() {
        let base = json!({ "a": 1, "b": [1, 2], "c": { "x": 1 } });
        let ours = json!({ "a": 1, "b": [1, 5], "c": { "x": 1 } });
        let theirs = json!({ "a": 2, "b": [1, 2], "c": { "x": 1, "y": 3 } });
        assert_eq!(
            merge3(&base, &ours, &theirs).unwrap(),
            json!({ "a": 2, "b": [1, 5], "c": { "x": 1, "y": 3 } })
        );
    }

    #[test]
    fn missing_null_and_empty_list_are_the_same() {
        let base = json!({ "a": 1 });
        let ours = json!({ "a": 1, "d": [] });
        let theirs = json!({ "a": 2, "d": null });
        assert_eq!(merge3(&base, &ours, &theirs).unwrap(), theirs);
        let ours = json!({ "a": 1, "d": [], "e": 5 });
        assert_eq!(
            merge3(&base, &ours, &theirs).unwrap(),
            json!({ "a": 2, "e": 5 })
        );
    }

    #[test]
    fn both_sides_changing_one_value_is_a_conflict() {
        let base = json!({ "parts": [{ "pitch": 44 }] });
        let ours = json!({ "parts": [{ "pitch": 40 }] });
        let theirs = json!({ "parts": [{ "pitch": 35 }] });
        assert_eq!(
            merge3(&base, &ours, &theirs).unwrap_err(),
            vec!["parts[0].pitch".to_string()]
        );
        // The same change on both sides is no conflict.
        assert_eq!(merge3(&base, &theirs, &theirs).unwrap(), theirs);
    }

    #[test]
    fn a_list_resized_on_both_sides_is_a_conflict() {
        let base = json!({ "d": [1, 2] });
        let ours = json!({ "d": [1, 3] });
        let theirs = json!({ "d": [1] });
        assert_eq!(merge3(&base, &ours, &theirs).unwrap_err(), vec!["d"]);
    }
}
