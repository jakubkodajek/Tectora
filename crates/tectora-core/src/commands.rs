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

/// Why a command refused: the code of [`Project::check_input`]
/// (`too_many`: more parts, walls or objects than the limits allow;
/// `out_of_range`: a number of the project is out of range) or `internal`
/// (the roof kernel failed), with the English detail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommandError {
    pub code: &'static str,
    pub detail: String,
}

impl CommandError {
    fn input(e: crate::model::InputError) -> Self {
        Self {
            code: e.code,
            detail: e.message,
        }
    }
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
    check_input(project)?;
    let mut v =
        crate::model::guarded(|| crate::report::calc(project)).map_err(CommandError::internal)?;
    v["stored_result"] = serde_json::to_value(snapshot::status(project)).unwrap_or(Value::Null);
    Ok(v)
}

/// How every face area and line length was calculated, step by step, with
/// the same guards as [`calc`].
pub fn calc_breakdown(project: &Project) -> Result<Value, CommandError> {
    check_input(project)?;
    let b = crate::model::guarded(|| crate::breakdown::breakdown(project))
        .map_err(CommandError::internal)?;
    Ok(serde_json::to_value(b).unwrap_or(Value::Null))
}

/// The current result, to store as the job's result.
pub fn take_snapshot(project: &Project) -> Result<snapshot::ResultSnapshot, CommandError> {
    check_input(project)?;
    snapshot::take_checked(project).map_err(|e| match e {
        snapshot::Refused::OutOfRange(d) => CommandError::out_of_range(d),
        snapshot::Refused::Internal(d) => CommandError::internal(d),
    })
}

/// The roof windows that do not fit their roof face (`edit::check_windows`),
/// `null` when all fit, with the same guards as [`calc`].
pub fn check_windows(project: &Project) -> Result<Value, CommandError> {
    check_input(project)?;
    let bad = crate::model::guarded(|| crate::edit::check_windows(project).err())
        .map_err(CommandError::internal)?;
    Ok(serde_json::to_value(bad).unwrap_or(Value::Null))
}

/// The roof under dormer `index` (`edit::dormers::host_slope`): height and
/// slope at the middle of its front, `null` off the roof, with the same
/// guards as [`calc`]. The editor's dormer dialog derives its section from it.
pub fn dormer_host(project: &Project, index: usize) -> Result<Value, CommandError> {
    check_input(project)?;
    let host = crate::model::guarded(|| crate::edit::dormers::host_slope(project, index))
        .map_err(CommandError::internal)?;
    Ok(serde_json::to_value(host).unwrap_or(Value::Null))
}

/// [`Project::check_input`] as a command error, with its code.
pub fn check_input(project: &Project) -> Result<(), CommandError> {
    project.check_input().map_err(CommandError::input)
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
    fn too_many_parts_are_refused_with_their_own_code() {
        let mut p = hip();
        let part = p.parts[0].clone();
        p.parts = vec![part; crate::model::MAX_PARTS + 1];
        for e in [
            calc(&p).unwrap_err(),
            calc_breakdown(&p).unwrap_err(),
            take_snapshot(&p).unwrap_err(),
            check_windows(&p).unwrap_err(),
        ] {
            assert_eq!(e.code, "too_many", "{e}");
        }
        assert_eq!(check_windows(&far_off()).unwrap_err().code, "out_of_range");
        assert!(check_windows(&hip()).unwrap().is_null());
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
