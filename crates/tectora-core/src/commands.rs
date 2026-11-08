//! The bodies of the desktop app's calculation commands (`calc`,
//! `calc_breakdown`, `take_snapshot` in `app/src-tauri/src/lib.rs`), shared
//! with the browser preview's bridge (`examples/ui_bridge.rs`) so the end-to-end
//! tests run the code the shipped app runs, and tested here: the app crate
//! needs WebKitGTK and is not built in the Linux CI container (TEST-008).
//!
//! Errors carry the stable code the UI translates (`app/src/lib/errors.ts`).

use serde::Serialize;
use serde_json::Value;

use crate::{Project, snapshot};

/// Why a command refused: `out_of_range` (a number of the project is out of
/// range) or `internal` (the roof kernel failed), with the English detail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommandError {
    pub code: &'static str,
    pub detail: String,
}

impl CommandError {
    fn out_of_range(detail: String) -> Self {
        Self {
            code: "out_of_range",
            detail,
        }
    }
    fn internal(detail: String) -> Self {
        Self {
            code: "internal",
            detail,
        }
    }
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.detail)
    }
}

/// Roof geometry and quantities for every part, plus totals, what kept
/// parts from being calculated, and the stored result's status. Out-of-range
/// input and a kernel panic are errors, never a crash.
pub fn calc(project: &Project) -> Result<Value, CommandError> {
    project.check_limits().map_err(CommandError::out_of_range)?;
    let mut v =
        crate::model::guarded(|| crate::report::calc(project)).map_err(CommandError::internal)?;
    v["stored_result"] = serde_json::to_value(snapshot::status(project)).unwrap_or(Value::Null);
    Ok(v)
}

/// How every face area and line length was calculated, step by step, with
/// the same guards as [`calc`].
pub fn calc_breakdown(project: &Project) -> Result<Value, CommandError> {
    project.check_limits().map_err(CommandError::out_of_range)?;
    let b = crate::model::guarded(|| crate::breakdown::breakdown(project))
        .map_err(CommandError::internal)?;
    Ok(serde_json::to_value(b).unwrap_or(Value::Null))
}

/// The current result, to store as the job's result.
pub fn take_snapshot(project: &Project) -> Result<snapshot::ResultSnapshot, CommandError> {
    snapshot::take_checked(project).map_err(|e| match e {
        snapshot::Refused::OutOfRange(d) => CommandError::out_of_range(d),
        snapshot::Refused::Internal(d) => CommandError::internal(d),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hip() -> Project {
        crate::examples::find("hip").expect("example").project()
    }

    fn far_off() -> Project {
        let mut p = hip();
        p.parts[0].edges[0].start.x = 1e12;
        p
    }

    #[test]
    fn calc_gives_totals_and_the_stored_result_status() {
        let v = calc(&hip()).expect("calculates");
        assert!(v["totals"]["roof_area"].as_f64().unwrap() > 0.0);
        assert!(v["totals"]["hip_length"].as_f64().unwrap() > 0.0);
        assert!(v.get("stored_result").is_some());
        assert_eq!(v["parts"].as_array().map(Vec::len), Some(1));
    }

    #[test]
    fn calc_refuses_out_of_range_input_with_its_code() {
        let e = calc(&far_off()).unwrap_err();
        assert_eq!(e.code, "out_of_range");
        assert!(!e.detail.is_empty());
        assert_eq!(calc_breakdown(&far_off()).unwrap_err().code, "out_of_range");
        assert_eq!(take_snapshot(&far_off()).unwrap_err().code, "out_of_range");
    }

    #[test]
    fn breakdown_and_snapshot_match_the_calculation() {
        let p = hip();
        let v = calc(&p).unwrap();
        let s = take_snapshot(&p).unwrap();
        let area = v["totals"]["roof_area"].as_f64().unwrap();
        assert!(
            (s.roof_area - area).abs() < 0.006,
            "{} vs {area}",
            s.roof_area
        );
        let b = calc_breakdown(&p).unwrap();
        assert!(!b.is_null());
    }

    #[test]
    fn errors_serialize_as_the_ui_reads_them() {
        let e = CommandError::out_of_range("x = 1e12".into());
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            serde_json::json!({ "code": "out_of_range", "detail": "x = 1e12" })
        );
    }
}
