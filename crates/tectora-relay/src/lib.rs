//! Client of the report relay (ADR 0008): one JSON `POST` with the report ZIP
//! base64-encoded to the owner's Apps Script web app, which answers through a
//! `302` redirect to a `googleusercontent.com` host. Everything the relay
//! says is untrusted: bounded read, strict JSON, redirects only to Google
//! hosts, `issue_url` only on github.com. No roof logic and no dependency on
//! `tectora-core`.
//!
//! Error values never hold the key, the address or the relay's own text.

use std::io::Read;
use std::path::Path;
use std::time::Duration;

use base64::Engine;
use serde_json::{Value, json};
use url::Url;

/// Largest ZIP `send` takes, bytes.
pub const MAX_UPLOAD: u64 = 20 * 1024 * 1024;

/// Most bytes read from an answer.
const MAX_ANSWER: u64 = 64 * 1024;
/// Redirects followed after the `POST`.
const MAX_HOPS: usize = 5;
/// Longest wait the relay may ask for, seconds.
const MAX_WAIT_S: u64 = 86_400;

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
    /// No connection, or no answer in time.
    Offline(String),
    /// The relay (or the deployment) said no; `code` is from a fixed set.
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

impl Timeouts {
    pub const DEFAULT: Timeouts = Timeouts {
        connect: Duration::from_secs(15),
        total: Duration::from_secs(180),
    };
}

fn valid_key(key: &str) -> bool {
    (16..=128).contains(&key.len())
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn is_loopback_host(u: &Url) -> bool {
    matches!(u.host_str(), Some("127.0.0.1"))
}

impl Config {
    /// The pair compiled in (`TECTORA_REPORT_URL` / `TECTORA_REPORT_KEY`).
    /// Debug builds let the same variable names override it at run time.
    pub fn baked() -> Option<Config> {
        let baked = match (
            option_env!("TECTORA_REPORT_URL"),
            option_env!("TECTORA_REPORT_KEY"),
        ) {
            (Some(u), Some(k)) => Some((u, k)),
            _ => None,
        };
        let env_url = std::env::var("TECTORA_REPORT_URL").ok();
        let env_key = std::env::var("TECTORA_REPORT_KEY").ok();
        let env = match (&env_url, &env_key) {
            (Some(u), Some(k)) => Some((u.as_str(), k.as_str())),
            _ => None,
        };
        Config::resolve(baked, env, cfg!(debug_assertions))
    }

    /// The pure rule behind `baked`: compile-time pair, run-time pair, debug.
    /// A release build ignores the environment.
    pub fn resolve(
        baked: Option<(&str, &str)>,
        env: Option<(&str, &str)>,
        debug: bool,
    ) -> Option<Config> {
        if debug && let Some((u, k)) = env {
            return Config::checked(u, k, true);
        }
        let (u, k) = baked?;
        Config::checked(u, k, false)
    }

    /// `None` means sending is disabled.
    pub fn checked(url: &str, key: &str, allow_loopback_http: bool) -> Option<Config> {
        if !valid_key(key) {
            return None;
        }
        let u = Url::parse(url).ok()?;
        if !u.username().is_empty() || u.password().is_some() {
            return None;
        }
        if u.query().is_some() || u.fragment().is_some() {
            return None;
        }
        let ok_origin = match u.scheme() {
            "https" => u.host_str() == Some("script.google.com") && u.port().is_none(),
            "http" => allow_loopback_http && is_loopback_host(&u),
            _ => false,
        };
        if !ok_origin {
            return None;
        }
        let segs: Vec<&str> = u.path_segments()?.collect();
        let shaped = segs.len() == 4
            && segs[0] == "macros"
            && segs[1] == "s"
            && !segs[2].is_empty()
            && segs[3] == "exec";
        if !shaped {
            return None;
        }
        Some(Config {
            url: url.to_string(),
            key: key.to_string(),
        })
    }
}

impl SendError {
    pub fn code(&self) -> &str {
        match self {
            SendError::Offline(_) => "offline",
            SendError::Refused { code, .. } => code,
            SendError::BadAnswer(_) => "bad_answer",
            SendError::TooLarge => "too_large",
            SendError::Io(_) => "io",
        }
    }

    /// A permanent failure is not retried by itself.
    pub fn permanent(&self) -> bool {
        match self {
            SendError::Refused { code, .. } => {
                matches!(code.as_str(), "bad_key" | "bad_request" | "too_large")
            }
            SendError::TooLarge => true,
            _ => false,
        }
    }

    /// The wait the relay asked for, seconds.
    pub fn retry_after_s(&self) -> Option<u64> {
        match self {
            SendError::Refused { retry_after_s, .. } => *retry_after_s,
            _ => None,
        }
    }
}

pub fn send(
    cfg: &Config,
    id: &str,
    file_name: &str,
    meta: &Value,
    zip: &Path,
) -> Result<Receipt, SendError> {
    send_with(cfg, id, file_name, meta, zip, Timeouts::DEFAULT)
}

pub fn send_with(
    cfg: &Config,
    id: &str,
    file_name: &str,
    meta: &Value,
    zip: &Path,
    timeouts: Timeouts,
) -> Result<Receipt, SendError> {
    let bytes = read_zip(zip)?;
    let body = json!({
        "v": 1,
        "key": cfg.key,
        "id": id,
        "file_name": file_name,
        "size": bytes.len(),
        "meta": meta,
        "zip_b64": base64::engine::general_purpose::STANDARD.encode(&bytes),
    })
    .to_string();
    drop(bytes);
    let url = cfg.url.clone();
    // A blocking client must not be created inside an async runtime: every
    // send runs on its own thread.
    let handle = std::thread::Builder::new()
        .name("tectora-relay-send".into())
        .spawn(move || exchange(&url, body, timeouts))
        .map_err(|_| SendError::Io("thread".into()))?;
    handle
        .join()
        .unwrap_or_else(|_| Err(SendError::Io("thread".into())))
}

/// Read the ZIP after checking it is a plain file within the cap.
fn read_zip(zip: &Path) -> Result<Vec<u8>, SendError> {
    let meta = std::fs::symlink_metadata(zip).map_err(|_| SendError::Io("open".into()))?;
    if !meta.is_file() {
        return Err(SendError::Io("not a file".into()));
    }
    if meta.len() > MAX_UPLOAD {
        return Err(SendError::TooLarge);
    }
    let f = std::fs::File::open(zip).map_err(|_| SendError::Io("open".into()))?;
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    f.take(MAX_UPLOAD + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| SendError::Io("read".into()))?;
    if bytes.len() as u64 > MAX_UPLOAD {
        return Err(SendError::TooLarge);
    }
    Ok(bytes)
}

fn ensure_crypto_provider() {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        // Another thread may install it first; either way one is set.
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
}

/// May a redirect go to `next`, given the configured endpoint `start`?
fn redirect_allowed(start: &Url, next: &Url) -> bool {
    if !next.username().is_empty() || next.password().is_some() {
        return false;
    }
    if start.scheme() == "http" {
        // Loopback config (tests, preview): the same origin only.
        return next.scheme() == "http"
            && next.host_str() == start.host_str()
            && next.port_or_known_default() == start.port_or_known_default();
    }
    if next.scheme() != "https" || next.port().is_some() {
        return false;
    }
    match next.host_str() {
        Some("script.google.com") | Some("googleusercontent.com") => true,
        Some(h) => h.ends_with(".googleusercontent.com"),
        None => false,
    }
}

fn exchange(url: &str, body: String, t: Timeouts) -> Result<Receipt, SendError> {
    ensure_crypto_provider();
    let start = Url::parse(url).map_err(|_| SendError::Io("url".into()))?;
    let mut builder = reqwest::blocking::Client::builder()
        .connect_timeout(t.connect)
        .timeout(t.total)
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("tectora");
    if is_loopback_host(&start) {
        builder = builder.no_proxy();
    }
    let client = builder
        .build()
        .map_err(|_| SendError::Io("client".into()))?;

    let mut current = start.clone();
    let mut resp = client
        .post(current.clone())
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .map_err(net_error)?;
    let mut hops = 0;
    while matches!(resp.status().as_u16(), 301..=303) {
        let loc = resp
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let Some(next) = loc.and_then(|l| current.join(&l).ok()) else {
            return Err(SendError::BadAnswer("redirect".into()));
        };
        if !redirect_allowed(&start, &next) {
            return Err(SendError::Refused {
                code: "relay_auth".into(),
                retry_after_s: None,
            });
        }
        if hops == MAX_HOPS {
            return Err(SendError::BadAnswer("too many redirects".into()));
        }
        hops += 1;
        resp = client.get(next.clone()).send().map_err(net_error)?;
        current = next;
    }
    let status = resp.status().as_u16();
    if status == 429 || status >= 500 {
        return Err(SendError::Refused {
            code: "server".into(),
            retry_after_s: None,
        });
    }
    if !(200..300).contains(&status) {
        return Err(SendError::BadAnswer(format!("http {status}")));
    }
    let mut text = Vec::new();
    resp.take(MAX_ANSWER + 1)
        .read_to_end(&mut text)
        .map_err(|_| SendError::Offline("read".into()))?;
    if text.len() as u64 > MAX_ANSWER {
        return Err(SendError::BadAnswer("answer too long".into()));
    }
    parse_answer(&text)
}

fn net_error(e: reqwest::Error) -> SendError {
    // Fixed words only: reqwest's text can carry the address.
    let what = if e.is_timeout() {
        "timeout"
    } else if e.is_connect() {
        "connect"
    } else {
        "request"
    };
    SendError::Offline(what.into())
}

fn parse_answer(text: &[u8]) -> Result<Receipt, SendError> {
    let bad = |what: &str| SendError::BadAnswer(what.into());
    let v: Value = serde_json::from_slice(text).map_err(|_| bad("not json"))?;
    let obj = v.as_object().ok_or_else(|| bad("not an object"))?;
    match obj.get("ok") {
        Some(Value::Bool(true)) => {
            let file_id = obj
                .get("file_id")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty() && s.len() <= 200)
                .ok_or_else(|| bad("no file id"))?;
            let issue_url = obj
                .get("issue_url")
                .and_then(Value::as_str)
                .filter(|u| u.starts_with("https://github.com/") && u.len() <= 300)
                .map(str::to_string);
            Ok(Receipt {
                file_id: file_id.to_string(),
                issue_url,
            })
        }
        Some(Value::Bool(false)) => {
            let code = obj.get("error").and_then(Value::as_str).unwrap_or("");
            let wait = obj
                .get("retry_after_s")
                .and_then(Value::as_u64)
                .map(|s| s.min(MAX_WAIT_S));
            match code {
                "bad_key" | "bad_request" | "too_large" => Err(SendError::Refused {
                    code: code.into(),
                    retry_after_s: None,
                }),
                "rate_limited" | "server" => Err(SendError::Refused {
                    code: code.into(),
                    retry_after_s: wait,
                }),
                _ => Err(bad("unknown error")),
            }
        }
        _ => Err(bad("no ok")),
    }
}
