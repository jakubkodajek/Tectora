//! Errors the Tauri commands return to the UI.
//!
//! Every error is serialized as `{ "code": "...", "id"?: "...",
//! "detail"?: "..." }`. The UI shows a translated message for the `code`
//! (see `app/src/lib/errors.ts`) and keeps `detail` (the English text from
//! the operating system or parser) under "Details" for support.

use serde::Serialize;
use tectora_core::dad::DadError;
use tectora_core::store::{StoreError, panic_message};

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AppError {
    /// Stable machine-readable code, e.g. `not_found`, `dad_not_dacad`.
    pub code: &'static str,
    /// The project the error is about, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// The original English message, for the details view and logs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl AppError {
    pub fn new(code: &'static str) -> Self {
        Self {
            code,
            id: None,
            detail: None,
        }
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code)?;
        if let Some(id) = &self.id {
            write!(f, " ({id})")?;
        }
        if let Some(d) = &self.detail {
            write!(f, ": {d}")?;
        }
        Ok(())
    }
}

impl From<StoreError> for AppError {
    fn from(e: StoreError) -> Self {
        AppError {
            code: e.code(),
            id: e.id().map(str::to_string),
            detail: Some(e.to_string()),
        }
    }
}

impl From<DadError> for AppError {
    fn from(e: DadError) -> Self {
        let code = match e {
            DadError::NotDacad => "dad_not_dacad",
            DadError::UnsupportedV2 => "dad_unsupported_v2",
            DadError::Truncated(_) | DadError::NotFinite(_) | DadError::BadWallRecord { .. } => {
                "dad_damaged"
            }
        };
        AppError::new(code).detail(e.to_string())
    }
}

/// Runs a command body, turning a panic into an `internal` error so the UI
/// gets an answer instead of a promise that never settles.
pub fn guard<T>(f: impl FnOnce() -> Result<T, AppError>) -> Result<T, AppError> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or_else(|payload| {
        let msg = panic_message(&*payload);
        eprintln!("tectora: internal error: {msg}");
        Err(AppError::new("internal").detail(msg))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_errors_keep_code_and_id() {
        let e: AppError = StoreError::NotFound("dum".into()).into();
        assert_eq!(e.code, "not_found");
        assert_eq!(e.id.as_deref(), Some("dum"));
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["code"], "not_found");
        assert_eq!(v["id"], "dum");
    }

    #[test]
    fn dad_errors_map_to_codes() {
        assert_eq!(AppError::from(DadError::NotDacad).code, "dad_not_dacad");
        assert_eq!(AppError::from(DadError::Truncated(12)).code, "dad_damaged");
    }

    #[test]
    fn a_panic_becomes_an_internal_error() {
        let r: Result<(), AppError> = guard(|| panic!("boom"));
        let e = r.unwrap_err();
        assert_eq!(e.code, "internal");
        assert_eq!(e.detail.as_deref(), Some("boom"));
    }
}
