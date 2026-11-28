//! Sending queued bug reports: the glue between the core's outbox (no
//! network) and the relay client. Used by the MCP tools and by the app.

use std::path::Path;

use serde_json::Value;
use tectora_core::outbox::{DrainResult, Outbox, Receipt, SendFailure};
use tectora_core::store::now_ms;
pub use tectora_relay::{Config, Timeouts};

/// What the MCP tools wait for one request: an AI client has no background
/// thread to finish the upload later.
pub const MCP_TIMEOUTS: Timeouts = Timeouts {
    connect: std::time::Duration::from_secs(15),
    total: std::time::Duration::from_secs(60),
};

fn sender<'a>(
    cfg: &'a Config,
    t: Timeouts,
    budget: Option<std::time::Duration>,
    issue: &'a mut Option<String>,
) -> impl FnMut(&Value, &Path) -> Result<Receipt, SendFailure> + 'a {
    let started = std::time::Instant::now();
    move |job: &Value, zip: &Path| {
        let id = job["id"].as_str().unwrap_or("");
        let name = job["file_name"].as_str().unwrap_or("");
        let mut t = t;
        if let Some(b) = budget {
            // The whole run stays within the budget: what is left is the
            // time of this send, and none left ends the run.
            let left = b.saturating_sub(started.elapsed());
            if left < std::time::Duration::from_secs(2) {
                return Err(SendFailure::Retry {
                    code: "offline".into(),
                    after_s: None,
                });
            }
            t.total = t.total.min(left);
        }
        match tectora_relay::send_with(cfg, id, name, &job["meta"], zip, t) {
            Ok(r) => {
                *issue = r.issue_url.clone();
                Ok(Receipt {
                    file_id: r.file_id,
                    issue_url: r.issue_url,
                })
            }
            Err(e) => {
                let code = e.code().to_string();
                Err(if e.permanent() {
                    SendFailure::Permanent { code }
                } else {
                    SendFailure::Retry {
                        code,
                        after_s: e.retry_after_s(),
                    }
                })
            }
        }
    }
}

/// Sends what is due (everything, also failed and local reports, with
/// `force`: Send now). `budget` bounds the whole run.
pub fn drain(
    outbox: &Outbox,
    cfg: &Config,
    force: bool,
    t: Timeouts,
    budget: Option<std::time::Duration>,
) -> DrainResult {
    let mut issue = None;
    outbox.drain(now_ms(), force, &mut sender(cfg, t, budget, &mut issue))
}

/// Tries the one report `name` once; the issue link when it went.
pub fn send_one(outbox: &Outbox, cfg: &Config, name: &str, t: Timeouts) -> Option<String> {
    let mut issue = None;
    outbox.drain_only(
        Some(name),
        now_ms(),
        true,
        &mut sender(cfg, t, None, &mut issue),
    );
    issue
}

/// What the app waits for one request.
pub const APP_TIMEOUTS: Timeouts = Timeouts::DEFAULT;
