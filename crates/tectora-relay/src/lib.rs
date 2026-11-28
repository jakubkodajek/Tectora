//! Skeleton so the tests of `plan-delivery.md` (D9) build and fail on
//! behaviour, not on a missing crate. The implementer replaces every body;
//! the signatures are what `tests/relay.rs` assumes (see "Test-side API").

use std::path::Path;
use std::time::Duration;

/// Largest ZIP `send` takes, bytes.
pub const MAX_UPLOAD: u64 = 20 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub url: String,
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Receipt {
    pub file_id: String,
    pub issue_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendError {
    Offline(String),
    Refused {
        code: String,
        retry_after_s: Option<u64>,
    },
    BadAnswer(String),
    TooLarge,
    Io(String),
}

#[derive(Debug, Clone, Copy)]
pub struct Timeouts {
    pub connect: Duration,
    pub total: Duration,
}

impl Config {
    pub fn baked() -> Option<Config> {
        None
    }

    /// The pure rule behind `baked`: compile-time pair, run-time pair, debug.
    pub fn resolve(
        _baked: Option<(&str, &str)>,
        _env: Option<(&str, &str)>,
        _debug: bool,
    ) -> Option<Config> {
        None
    }

    pub fn checked(_url: &str, _key: &str, _allow_loopback_http: bool) -> Option<Config> {
        None
    }
}

impl SendError {
    pub fn code(&self) -> &str {
        "not_implemented"
    }
    pub fn permanent(&self) -> bool {
        false
    }
}

pub fn send(
    cfg: &Config,
    id: &str,
    file_name: &str,
    meta: &serde_json::Value,
    zip: &Path,
) -> Result<Receipt, SendError> {
    send_with(
        cfg,
        id,
        file_name,
        meta,
        zip,
        Timeouts {
            connect: Duration::from_secs(15),
            total: Duration::from_secs(180),
        },
    )
}

pub fn send_with(
    _cfg: &Config,
    _id: &str,
    _file_name: &str,
    _meta: &serde_json::Value,
    _zip: &Path,
    _timeouts: Timeouts,
) -> Result<Receipt, SendError> {
    Err(SendError::Io("not implemented".into()))
}
