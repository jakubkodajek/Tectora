//! The report outbox (ADR 0008): every bug report is queued here as a ZIP
//! with a small sidecar, then sent by whoever has a relay (the app, the MCP
//! server). Std only and no network: sending is a closure the caller passes
//! to [`Outbox::drain`], so the queue is tested with a fake sender.
//!
//! ```text
//! <root>/outbox/<name>.zip     the report (format of `bugreport`)
//! <root>/outbox/<name>.json    sidecar, written before the ZIP is visible
//! <root>/outbox/<name>.lock    claim while one process sends it
//! <root>/sent.jsonl            the newest SENT_KEPT sent reports
//! <root>/staging/              where a report is built before it is queued
//! ```
//!
//! Everything read back from the folder is untrusted: a sidecar is parsed
//! field by field, a ZIP must be a plain file within the upload cap.

use std::collections::hash_map::RandomState;
use std::fs;
use std::hash::{BuildHasher, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::bugreport::{self, Dest, LeftOut, Manifest, Request, Source};
use crate::store::{FileError, Store, now_ms};

/// Largest ZIP that is queued for sending, bytes.
pub const MAX_UPLOAD: u64 = 20 * 1024 * 1024;
pub const MAX_QUEUED: usize = 50;
pub const MAX_QUEUED_BYTES: u64 = 256 * 1024 * 1024;
pub const SENT_KEPT: usize = 200;

const MAX_SIDECAR: u64 = 1024 * 1024;
const MAX_SENT_LOG: u64 = 4 * 1024 * 1024;
const MAX_NAMES: usize = 1000;
const STALE_LOCK: Duration = Duration::from_secs(15 * 60);
const MAX_BACKOFF_MS: u64 = 6 * 3600 * 1000;
const MAX_NAME_FIELD: usize = 80;

/// What the queue accepts; the defaults are the constants above.
#[derive(Debug, Clone)]
pub struct Limits {
    pub max_upload: u64,
    pub max_queued: usize,
    pub max_queued_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_upload: MAX_UPLOAD,
            max_queued: MAX_QUEUED,
            max_queued_bytes: MAX_QUEUED_BYTES,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Queued {
    pub file_name: String,
    pub id: String,
    pub size: u64,
    /// `waiting`, or `local` when this build cannot send.
    pub state: String,
    pub entries: usize,
    pub left_out: Vec<LeftOut>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Counts {
    pub waiting: usize,
    pub failed: usize,
    pub local: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct DrainResult {
    pub sent: usize,
    pub waiting: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Receipt {
    pub file_id: String,
    pub issue_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendFailure {
    Retry { code: String, after_s: Option<u64> },
    Permanent { code: String },
}

/// One report of [`Outbox::list`], queued or sent.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Listed {
    pub file_name: String,
    /// `waiting`, `failed`, `local` or `sent`.
    pub state: String,
    pub size: u64,
    pub created_ms: u64,
    pub source: String,
    pub job_id: Option<String>,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Listing {
    pub reports: Vec<Listed>,
    pub skipped: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Waiting,
    Failed,
    Local,
}

impl State {
    fn as_str(self) -> &'static str {
        match self {
            State::Waiting => "waiting",
            State::Failed => "failed",
            State::Local => "local",
        }
    }
}

/// A sidecar that passed every check.
struct Side {
    id: String,
    file_name: String,
    size: u64,
    queued_ms: u64,
    attempts: u64,
    next_try_ms: u64,
    state: State,
    last_error: Option<String>,
    job_id: Option<String>,
    meta: Value,
}

impl Side {
    fn to_json(&self) -> Value {
        json!({
            "v": 1,
            "id": self.id,
            "file_name": self.file_name,
            "size": self.size,
            "queued_ms": self.queued_ms,
            "attempts": self.attempts,
            "next_try_ms": self.next_try_ms,
            "state": self.state.as_str(),
            "last_error": self.last_error,
            "job_id": self.job_id,
            "meta": self.meta,
        })
    }
}

pub struct Outbox {
    root: PathBuf,
}

// ------------------------------------------------------------ small helpers

fn random_hex(len: usize) -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    let mut out = String::new();
    while out.len() < len {
        let mut h = RandomState::new().build_hasher();
        h.write_u128(
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos()),
        );
        h.write_u64(N.fetch_add(1, Ordering::Relaxed));
        h.write_u32(std::process::id());
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out.truncate(len);
    out
}

fn is_hex(s: &str, len: usize) -> bool {
    s.len() == len && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// A code or short label: ASCII letters, digits, `_` and `-` only.
fn code_text(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
        .take(40)
        .collect()
}

fn github_url(v: Option<&str>) -> Option<String> {
    v.filter(|u| u.starts_with("https://github.com/") && u.len() <= 300)
        .map(str::to_string)
}

fn err_io(path: &Path, e: std::io::Error) -> FileError {
    FileError::io(path, e)
}

fn bad_name(name: &str) -> FileError {
    FileError::new(
        "invalid_name",
        format!(
            "`{}` is not the name of a bug report",
            bugreport::line(name, 120)
        ),
    )
}

fn not_found(name: &str) -> FileError {
    FileError::new(
        "file_not_found",
        format!("no queued or sent report `{}`", bugreport::line(name, 120)),
    )
}

/// The install id: 8 hex characters made once per computer.
fn install_id() -> String {
    let path = crate::settings::config_dir().join("install-id");
    if let Ok(bytes) = crate::store::read_regular(&path, 64)
        && let Ok(s) = String::from_utf8(bytes)
        && is_hex(s.trim(), 8)
    {
        return s.trim().to_string();
    }
    let id = random_hex(8);
    let _ = fs::create_dir_all(crate::settings::config_dir());
    let _ = crate::store::write_atomic_bytes(&path, id.as_bytes());
    id
}

/// The part of a report the issue is built from, field by field. Customer
/// name and address, steps, notices, settings, paths and log text are never
/// in it; the job name only while the report carries customer data anyway.
pub fn issue_meta(report: &Value, manifest: &Manifest) -> Value {
    let str_at = |v: &Value, max: usize| -> Option<Value> {
        v.as_str().map(|s| json!(bugreport::line(s, max)))
    };
    let mut m = Map::new();
    m.insert(
        "description".into(),
        json!(bugreport::text(
            report["description"].as_str().unwrap_or(""),
            bugreport::MAX_DESCRIPTION,
            true
        )),
    );
    m.insert(
        "app_version".into(),
        str_at(&report["app"]["version"], 40).unwrap_or(Value::Null),
    );
    m.insert(
        "engine_version".into(),
        str_at(&report["app"]["engine_version"], 40).unwrap_or(Value::Null),
    );
    m.insert(
        "os".into(),
        str_at(&report["system"]["os"], 40).unwrap_or(Value::Null),
    );
    if let Some(v) = str_at(&report["system"]["os_version"], 60) {
        m.insert("os_version".into(), v);
    }
    m.insert(
        "arch".into(),
        str_at(&report["system"]["arch"], 40).unwrap_or(Value::Null),
    );
    if let Some(v) = str_at(&report["ui"]["locale"], 20) {
        m.insert("locale".into(), v);
    }
    m.insert(
        "source".into(),
        json!(bugreport::line(&manifest.source, 20)),
    );
    m.insert(
        "reporter".into(),
        json!(bugreport::line(
            report["reporter"].as_str().unwrap_or(""),
            100
        )),
    );
    if let Some(v) = str_at(&report["ui"]["screen"], 100) {
        m.insert("screen".into(), v);
    }
    let job = &report["job"];
    if job["anonymized"] == json!(false)
        && let Some(v) = str_at(&job["name"], 200)
    {
        m.insert("job_name".into(), v);
    }
    if let Some(v) = str_at(&job["status"], 40) {
        m.insert("job_status".into(), v);
    }
    m.insert(
        "calculation_ok".into(),
        json!(report["calculation"]["ok"].as_bool().unwrap_or(false)),
    );
    if let Some(v) = str_at(&report["calculation"]["error"]["code"], 60) {
        m.insert("calculation_error_code".into(), v);
    }
    let mut included = Map::new();
    for k in ["job", "customer", "files", "history", "snapshot", "log"] {
        included.insert(
            k.into(),
            json!(report["included"][k].as_bool().unwrap_or(false)),
        );
    }
    m.insert("included".into(), Value::Object(included));
    let left: Vec<Value> = manifest
        .left_out
        .iter()
        .take(20)
        .map(|l| json!({ "part": code_text(&l.part), "reason": code_text(&l.reason) }))
        .collect();
    m.insert("left_out".into(), json!(left));
    m.insert("created_ms".into(), json!(manifest.created_ms));
    Value::Object(m)
}

impl Outbox {
    /// `<config folder>/reports`, shared by the app and the MCP server.
    pub fn open_default() -> Outbox {
        Outbox {
            root: crate::settings::config_dir().join("reports"),
        }
    }

    pub fn at(root: &Path) -> Outbox {
        Outbox {
            root: root.to_path_buf(),
        }
    }

    fn dir(&self) -> PathBuf {
        self.root.join("outbox")
    }

    fn staging(&self) -> PathBuf {
        self.root.join("staging")
    }

    fn sent_path(&self) -> PathBuf {
        self.root.join("sent.jsonl")
    }

    fn zip_path(&self, name: &str) -> PathBuf {
        self.dir().join(name)
    }

    fn side_path(&self, name: &str) -> PathBuf {
        self.zip_path(name).with_extension("json")
    }

    fn lock_path(&self, name: &str) -> PathBuf {
        self.zip_path(name).with_extension("lock")
    }

    fn ensure_dirs(&self) -> Result<(), FileError> {
        fs::create_dir_all(&self.root).map_err(|e| err_io(&self.root, e))?;
        for d in [self.dir(), self.staging()] {
            crate::store::own_folder(&d).map_err(|e| err_io(&d, e))?;
        }
        Ok(())
    }

    /// Names of the report files in the outbox, oldest first.
    fn names(&self) -> Vec<String> {
        let Ok(rd) = fs::read_dir(self.dir()) else {
            return Vec::new();
        };
        let mut v: Vec<String> = rd
            .filter_map(Result::ok)
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| bugreport::is_report_name(n))
            .take(MAX_NAMES)
            .collect();
        v.sort();
        v
    }

    /// The size of a queued ZIP when it is a plain file within the cap.
    fn zip_size(&self, name: &str) -> Option<u64> {
        let m = fs::symlink_metadata(self.zip_path(name)).ok()?;
        (m.is_file() && m.len() <= MAX_UPLOAD).then_some(m.len())
    }

    /// The sidecar of `name`, checked; `None` when anything is wrong.
    fn read_side(&self, name: &str) -> Option<Side> {
        let size = self.zip_size(name)?;
        let bytes = crate::store::read_regular(&self.side_path(name), MAX_SIDECAR).ok()?;
        let v: Value = serde_json::from_slice(&bytes).ok()?;
        let o = v.as_object()?;
        if o.get("v")?.as_u64()? != 1 {
            return None;
        }
        let id = o.get("id")?.as_str()?;
        if !is_hex(id, 32) || o.get("file_name")?.as_str()? != name {
            return None;
        }
        let state = match o.get("state")?.as_str()? {
            "waiting" => State::Waiting,
            "failed" => State::Failed,
            "local" => State::Local,
            _ => return None,
        };
        let meta = o.get("meta")?.clone();
        if !meta.is_object() {
            return None;
        }
        let last_error = match o.get("last_error") {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) => Some(code_text(s)),
            Some(_) => return None,
        };
        let job_id = o
            .get("job_id")
            .and_then(Value::as_str)
            .map(|s| bugreport::line(s, 100));
        Some(Side {
            id: id.to_string(),
            file_name: name.to_string(),
            size,
            queued_ms: o.get("queued_ms")?.as_u64()?,
            attempts: o.get("attempts")?.as_u64()?,
            next_try_ms: o.get("next_try_ms")?.as_u64()?,
            state,
            last_error,
            job_id,
            meta,
        })
    }

    fn write_side(&self, side: &Side) -> Result<(), FileError> {
        let path = self.side_path(&side.file_name);
        let bytes = serde_json::to_vec(&side.to_json())
            .map_err(|e| FileError::new("internal", e.to_string()))?;
        crate::store::write_atomic_bytes(&path, &bytes).map_err(|e| err_io(&path, e))
    }

    // ------------------------------------------------------------ enqueue

    pub fn enqueue(
        &self,
        store: &Store,
        req: &Request,
        local_only: bool,
    ) -> Result<Queued, FileError> {
        self.enqueue_with(store, req, local_only, &Limits::default())
    }

    /// Builds the report (dropping drawings, history and the 3D picture, in
    /// this order, until it fits `limits.max_upload`) and queues it.
    pub fn enqueue_with(
        &self,
        store: &Store,
        req: &Request,
        local_only: bool,
        limits: &Limits,
    ) -> Result<Queued, FileError> {
        self.ensure_dirs()?;
        let (count, bytes) = self.totals();
        if count >= limits.max_queued {
            return Err(outbox_full());
        }
        let staging = self.staging();
        const LEVELS: [&[&str]; 4] = [
            &[],
            &["files"],
            &["files", "history"],
            &["files", "history", "model"],
        ];
        // Attachments that alone exceed the cap make the first build pointless.
        let mut level = 0;
        if req.include.job
            && req.include.files
            && let Some(id) = req.job_id.as_deref()
            && let Ok(list) = store.try_attachments(id)
            && list.iter().map(|a| a.size).sum::<u64>() > limits.max_upload
        {
            level = 1;
        }
        let mut built = None;
        while level < LEVELS.len() {
            let c =
                bugreport::create_with(store, req, &Dest::Folder(staging.clone()), LEVELS[level])?;
            if c.size <= limits.max_upload {
                built = Some(c);
                break;
            }
            let _ = fs::remove_file(&c.path);
            level += 1;
        }
        let Some(created) = built else {
            return Err(FileError::new(
                "too_large",
                format!(
                    "the report is larger than {} MB even without drawings, history and the 3D picture",
                    limits.max_upload >> 20
                ),
            ));
        };
        let cleanup = |e: FileError| {
            let _ = fs::remove_file(&created.path);
            e
        };
        if bytes.saturating_add(created.size) > limits.max_queued_bytes {
            return Err(cleanup(outbox_full()));
        }
        let read = bugreport::read(&staging, &created.file_name).map_err(&cleanup)?;
        let mut meta = issue_meta(&read["report"], &created.manifest);
        if let Some(m) = meta.as_object_mut() {
            m.insert("install".into(), json!(install_id()));
            if req.source == Source::App
                && let Some(n) = req.client.get("user_name").and_then(Value::as_str)
            {
                let n = bugreport::line(n.trim(), MAX_NAME_FIELD);
                if !n.is_empty() {
                    m.insert("user_name".into(), json!(n));
                }
            }
        }
        let id = random_hex(32);
        let stem = created
            .file_name
            .strip_suffix(".zip")
            .unwrap_or(&created.file_name)
            .to_string();
        let mut placed = None;
        for n in 1..1000 {
            let name = if n == 1 {
                format!("{stem}.zip")
            } else {
                format!("{stem}-{n}.zip")
            };
            let zip = self.zip_path(&name);
            if fs::symlink_metadata(&zip).is_ok() {
                continue;
            }
            // The sidecar claims the name and is complete before the ZIP
            // shows up under it.
            let side = Side {
                id: id.clone(),
                file_name: name.clone(),
                size: created.size,
                queued_ms: now_ms(),
                attempts: 0,
                next_try_ms: 0,
                state: if local_only {
                    State::Local
                } else {
                    State::Waiting
                },
                last_error: None,
                job_id: req.job_id.as_deref().map(|s| bugreport::line(s, 100)),
                meta: meta.clone(),
            };
            let sp = self.side_path(&name);
            let bytes = serde_json::to_vec(&side.to_json())
                .map_err(|e| cleanup(FileError::new("internal", e.to_string())))?;
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&sp)
            {
                Ok(mut f) => {
                    let ok = f.write_all(&bytes).and_then(|()| f.sync_all());
                    drop(f);
                    if let Err(e) = ok {
                        let _ = fs::remove_file(&sp);
                        return Err(cleanup(err_io(&sp, e)));
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(cleanup(err_io(&sp, e))),
            }
            if let Err(e) = fs::rename(&created.path, &zip) {
                let _ = fs::remove_file(&sp);
                return Err(cleanup(err_io(&zip, e)));
            }
            crate::store::sync_parent(&zip);
            placed = Some((name, side.state));
            break;
        }
        let Some((file_name, state)) = placed else {
            return Err(cleanup(FileError::new(
                "invalid_name",
                "too many reports with the same name",
            )));
        };
        crate::applog::line("INFO", "report queued");
        Ok(Queued {
            file_name,
            id,
            size: created.size,
            state: state.as_str().into(),
            entries: created.manifest.entries.len(),
            left_out: created.manifest.left_out,
        })
    }

    /// Number and bytes of the queued ZIPs.
    fn totals(&self) -> (usize, u64) {
        let names = self.names();
        let bytes = names
            .iter()
            .map(|n| {
                fs::symlink_metadata(self.zip_path(n))
                    .map(|m| m.len())
                    .unwrap_or(0)
            })
            .sum();
        (names.len(), bytes)
    }

    // ------------------------------------------------------------ queries

    pub fn counts(&self) -> Counts {
        let mut c = Counts::default();
        for n in self.names() {
            match self.read_side(&n).map(|s| s.state) {
                Some(State::Waiting) => c.waiting += 1,
                Some(State::Local) => c.local += 1,
                Some(State::Failed) | None => c.failed += 1,
            }
        }
        c
    }

    fn sent_lines(&self) -> (Vec<Value>, usize) {
        let Ok(bytes) = crate::store::read_regular(&self.sent_path(), MAX_SENT_LOG) else {
            return (Vec::new(), 0);
        };
        let text = String::from_utf8_lossy(&bytes);
        let mut out = Vec::new();
        let mut skipped = 0;
        for l in text.lines().filter(|l| !l.trim().is_empty()) {
            match serde_json::from_str::<Value>(l) {
                Ok(v)
                    if v["id"].as_str().is_some_and(|i| is_hex(i, 32))
                        && v["file_name"]
                            .as_str()
                            .is_some_and(bugreport::is_report_name) =>
                {
                    out.push(v)
                }
                _ => skipped += 1,
            }
        }
        (out, skipped)
    }

    /// Queued reports and sent lines together, newest first (by the time in
    /// the file name), at most `limit`.
    pub fn list(&self, limit: usize) -> Result<Listing, FileError> {
        let mut all: Vec<Listed> = Vec::new();
        for name in self.names() {
            let size = fs::symlink_metadata(self.zip_path(&name)).map_or(0, |m| m.len());
            all.push(match self.read_side(&name) {
                Some(s) => Listed {
                    file_name: name,
                    state: s.state.as_str().into(),
                    size: s.size,
                    created_ms: s.queued_ms,
                    source: bugreport::line(s.meta["source"].as_str().unwrap_or(""), 20),
                    job_id: s.job_id.clone(),
                    description: bugreport::line(
                        &s.meta["description"]
                            .as_str()
                            .unwrap_or("")
                            .chars()
                            .take(120)
                            .collect::<String>(),
                        120,
                    ),
                    job_name: s.meta["job_name"].as_str().map(|n| bugreport::line(n, 200)),
                    last_error: s.last_error,
                    issue_url: None,
                },
                None => Listed {
                    file_name: name,
                    state: "failed".into(),
                    size,
                    created_ms: 0,
                    source: String::new(),
                    job_id: None,
                    description: String::new(),
                    job_name: None,
                    last_error: Some("unreadable".into()),
                    issue_url: None,
                },
            });
        }
        let (sent, skipped) = self.sent_lines();
        for l in sent {
            let name = l["file_name"].as_str().unwrap_or("").to_string();
            if all.iter().any(|r| r.file_name == name) {
                continue;
            }
            all.push(Listed {
                file_name: name,
                state: "sent".into(),
                size: l["size"].as_u64().unwrap_or(0),
                created_ms: l["sent_ms"].as_u64().unwrap_or(0),
                source: bugreport::line(l["source"].as_str().unwrap_or(""), 20),
                job_id: l["job_id"].as_str().map(|s| bugreport::line(s, 100)),
                description: bugreport::line(
                    &l["description"]
                        .as_str()
                        .unwrap_or("")
                        .chars()
                        .take(120)
                        .collect::<String>(),
                    120,
                ),
                job_name: None,
                last_error: None,
                issue_url: github_url(l["issue_url"].as_str()),
            });
        }
        all.sort_by(|a, b| b.file_name.cmp(&a.file_name));
        all.truncate(limit);
        Ok(Listing {
            reports: all,
            skipped,
        })
    }

    /// A queued report (`state`, `manifest`, `report`) or a sent one
    /// (`state: "sent"`, `sent`).
    pub fn read(&self, name: &str) -> Result<Value, FileError> {
        if !bugreport::is_report_name(name) {
            return Err(bad_name(name));
        }
        if fs::symlink_metadata(self.zip_path(name)).is_ok() {
            let state = self.read_side(name).map_or("failed", |s| s.state.as_str());
            if self.zip_size(name).is_none() {
                return Err(FileError::new(
                    "unreadable",
                    format!("{}: not a readable report", bugreport::line(name, 120)),
                ));
            }
            let mut v = bugreport::read(&self.dir(), name)?;
            v["state"] = json!(state);
            return Ok(v);
        }
        let (sent, _) = self.sent_lines();
        match sent.into_iter().find(|l| l["file_name"] == name) {
            Some(l) => Ok(json!({ "state": "sent", "sent": l })),
            None => Err(not_found(name)),
        }
    }

    pub fn discard(&self, name: &str) -> Result<(), FileError> {
        if !bugreport::is_report_name(name) {
            return Err(bad_name(name));
        }
        let zip = self.zip_path(name);
        if fs::symlink_metadata(&zip).is_err() {
            return Err(not_found(name));
        }
        if let Ok(m) = fs::symlink_metadata(self.lock_path(name))
            && !lock_is_stale(&m)
        {
            return Err(FileError::new(
                "busy",
                "the report is being sent right now; try again in a moment",
            ));
        }
        fs::remove_file(&zip).map_err(|e| err_io(&zip, e))?;
        let _ = fs::remove_file(self.side_path(name));
        let _ = fs::remove_file(self.lock_path(name));
        Ok(())
    }

    /// Copies a queued report to `dest` (Save as): an absolute plain `*.zip`
    /// name in an existing folder. The queued report stays.
    pub fn export(&self, name: &str, dest: &Path) -> Result<(), FileError> {
        if !bugreport::is_report_name(name) {
            return Err(bad_name(name));
        }
        let invalid =
            |why: &str| FileError::new("invalid_name", format!("{}: {why}", dest.display()));
        if !dest.is_absolute() {
            return Err(FileError::new(
                "invalid_input",
                format!("{}: the path must be absolute", dest.display()),
            ));
        }
        let file = dest.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let stem = file
            .get(..file.len().saturating_sub(4))
            .unwrap_or("")
            .trim_matches([' ', '.']);
        if !file.to_ascii_lowercase().ends_with(".zip")
            || stem.is_empty()
            || !crate::store::valid_file_name(file)
        {
            return Err(invalid("the file must be a plain file name ending in .zip"));
        }
        let parent = dest
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .ok_or_else(|| invalid("no folder"))?;
        if !fs::metadata(parent).is_ok_and(|m| m.is_dir()) {
            return Err(invalid("the folder does not exist"));
        }
        if let (Ok(a), Ok(b)) = (parent.canonicalize(), self.root.canonicalize())
            && a.starts_with(&b)
        {
            return Err(invalid("choose a folder outside the report queue"));
        }
        if let Ok(m) = fs::symlink_metadata(dest)
            && !m.is_file()
        {
            return Err(invalid("not a file"));
        }
        if self.zip_size(name).is_none() {
            return Err(not_found(name));
        }
        let src = self.zip_path(name);
        let part = parent.join(format!(".tectora-export-{}.part", std::process::id()));
        let _ = fs::remove_file(&part);
        let copied = fs::copy(&src, &part)
            .map_err(|e| err_io(dest, e))
            .and_then(|_| fs::rename(&part, dest).map_err(|e| err_io(dest, e)));
        if copied.is_err() {
            let _ = fs::remove_file(&part);
        }
        copied
    }

    /// A report made by a build without a relay waits as `local`; a build
    /// that has one makes them `waiting`.
    pub fn promote_local(&self) {
        for n in self.names() {
            if let Some(mut s) = self.read_side(&n)
                && s.state == State::Local
            {
                s.state = State::Waiting;
                let _ = self.write_side(&s);
            }
        }
    }

    // ------------------------------------------------------------ drain

    /// Sends what is due through `send` (everything with `force`, except
    /// `local` ones). `send` gets `{ id, file_name, meta }` and the ZIP path.
    pub fn drain(
        &self,
        now_ms: u64,
        force: bool,
        send: &mut dyn FnMut(&Value, &Path) -> Result<Receipt, SendFailure>,
    ) -> DrainResult {
        let mut sent = 0;
        for name in self.names() {
            let Some(side) = self.read_side(&name) else {
                continue;
            };
            let due = match side.state {
                State::Local => false,
                State::Failed => force,
                State::Waiting => force || side.next_try_ms <= now_ms,
            };
            if !due {
                continue;
            }
            let Some(_claim) = self.claim(&name) else {
                continue;
            };
            // Looked at again under the claim: another process may have
            // finished it meanwhile.
            let Some(mut side) = self.read_side(&name) else {
                continue;
            };
            if side.state == State::Local {
                continue;
            }
            if self.already_sent(&side.id) {
                self.remove_entry(&name);
                continue;
            }
            let zip = self.zip_path(&name);
            let job = json!({ "id": side.id, "file_name": name, "meta": side.meta });
            match send(&job, &zip) {
                Ok(r) => {
                    self.append_sent(&side, &r);
                    self.remove_entry(&name);
                    sent += 1;
                    crate::applog::line("INFO", "report sent");
                }
                Err(SendFailure::Retry { code, after_s }) => {
                    let code = code_text(&code);
                    side.attempts = side.attempts.saturating_add(1);
                    let mins = 1u64 << side.attempts.min(20);
                    let mut wait = mins.saturating_mul(60_000).min(MAX_BACKOFF_MS);
                    if let Some(s) = after_s {
                        wait = wait.max(s.saturating_mul(1000));
                    }
                    side.next_try_ms = now_ms.saturating_add(wait);
                    side.state = State::Waiting;
                    side.last_error = Some(code.clone());
                    let _ = self.write_side(&side);
                    crate::applog::line("WARN", &format!("report send failed: {code}"));
                }
                Err(SendFailure::Permanent { code }) => {
                    let code = code_text(&code);
                    side.attempts = side.attempts.saturating_add(1);
                    side.state = State::Failed;
                    side.last_error = Some(code.clone());
                    let _ = self.write_side(&side);
                    crate::applog::line("WARN", &format!("report send failed: {code}"));
                }
            }
        }
        let c = self.counts();
        DrainResult {
            sent,
            waiting: c.waiting,
            failed: c.failed,
        }
    }

    fn claim(&self, name: &str) -> Option<Claim> {
        let path = self.lock_path(name);
        for _ in 0..2 {
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut f) => {
                    let _ = write!(f, "{}", std::process::id());
                    return Some(Claim { path });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    match fs::symlink_metadata(&path) {
                        Ok(m) if lock_is_stale(&m) => {
                            let _ = fs::remove_file(&path);
                        }
                        _ => return None,
                    }
                }
                Err(_) => return None,
            }
        }
        None
    }

    fn already_sent(&self, id: &str) -> bool {
        self.sent_lines().0.iter().any(|l| l["id"] == id)
    }

    fn remove_entry(&self, name: &str) {
        let zip = self.zip_path(name);
        // A virus scanner on Windows may still hold a fresh file: once more.
        if fs::remove_file(&zip).is_err() {
            std::thread::sleep(Duration::from_millis(200));
            let _ = fs::remove_file(&zip);
        }
        if !zip.exists() {
            let _ = fs::remove_file(self.side_path(name));
        }
    }

    fn append_sent(&self, side: &Side, r: &Receipt) {
        let mut lines: Vec<Value> = self.sent_lines().0;
        if lines.iter().any(|l| l["id"] == side.id) {
            return;
        }
        lines.push(json!({
            "id": side.id,
            "file_name": side.file_name,
            "sent_ms": now_ms(),
            "size": side.size,
            "issue_url": github_url(r.issue_url.as_deref()),
            "file_id": bugreport::line(&r.file_id, 200),
            "source": bugreport::line(side.meta["source"].as_str().unwrap_or(""), 20),
            "job_id": side.job_id,
            "description": bugreport::line(
                &side.meta["description"].as_str().unwrap_or("").chars().take(200).collect::<String>(),
                200,
            ),
        }));
        let skip = lines.len().saturating_sub(SENT_KEPT);
        let mut text = String::new();
        for l in &lines[skip..] {
            text.push_str(&l.to_string());
            text.push('\n');
        }
        let path = self.sent_path();
        let _ = fs::create_dir_all(&self.root);
        if crate::store::write_atomic_bytes(&path, text.as_bytes()).is_err() {
            crate::applog::line("WARN", "report sent log not written");
        }
    }
}

fn outbox_full() -> FileError {
    FileError::new(
        "outbox_full",
        "too many reports are waiting to be sent; send or discard some first",
    )
}

/// Holds the `.lock` of an entry until dropped.
struct Claim {
    path: PathBuf,
}

impl Drop for Claim {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn lock_is_stale(m: &fs::Metadata) -> bool {
    m.modified()
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|age| age > STALE_LOCK)
}
