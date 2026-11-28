//! Bug reports: one ZIP with everything a developer (or an AI client) needs
//! to reproduce a problem, written into a reports folder (a shared Drive or
//! iCloud folder) or to a place the user chose. Offline: nothing is sent.
//!
//! ```text
//! manifest.json               format, versions, entry list with sizes, what was left out
//! report.json                 description, versions, system, UI state, calculation, steps
//! job/project.tectora         the job file (customer and address blanked on request)
//! job/editor-project.tectora  the editor's copy, only when it differs from the file
//! job/history.jsonl           the newest change-log lines
//! job/preview.svg             the core's plan preview, when the job calculates
//! job/files/<name>            attached drawings
//! snapshot/plan.svg           the plan as the user sees it
//! snapshot/model.png          the 3D view
//! logs/app.log, logs/mcp.log  the end of each log
//! ```
//!
//! Everything from the UI and from MCP is untrusted: it is size-checked,
//! cleaned and picked field by field here.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use zip::write::SimpleFileOptions;

use crate::bundle::{MAX_ENTRY, MAX_FILES, MAX_FILES_TOTAL, MAX_TEXT_ENTRY};
use crate::model::Project;
use crate::settings::Settings;
use crate::store::{FileError, Store, now_ms};

pub const FORMAT: &str = "tectora-bug-report";
pub const FORMAT_VERSION: u32 = 1;
const PREFIX: &str = "tectora-report-";
const EXT: &str = "zip";

/// Longest description, characters. Longer is refused.
pub const MAX_DESCRIPTION: usize = 5000;
pub const MAX_ACTIONS: usize = 50;
pub const MAX_NOTICES: usize = 30;
const MAX_STRING: usize = 300;
const MAX_DETAIL: usize = 2000;
const MAX_PLAN_SVG: usize = 4 * 1024 * 1024;
const MAX_MODEL_PNG: usize = 8 * 1024 * 1024;
const MAX_LOG_BYTES: u64 = 256 * 1024;
const MAX_LOG_LINES: usize = 400;
/// Largest report file `read` opens, bytes.
pub const MAX_REPORT_FILE: u64 = MAX_FILES_TOTAL + 4 * MAX_TEXT_ENTRY;
/// Largest `manifest.json` or `report.json` `read` takes from a report.
const MAX_JSON_ENTRY: u64 = 2 * 1024 * 1024;
/// How many files of a reports folder `list` looks at.
const MAX_LISTED_FILES: usize = 2000;
const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    App,
    Mcp,
}

impl Source {
    fn as_str(self) -> &'static str {
        match self {
            Source::App => "app",
            Source::Mcp => "mcp",
        }
    }
}

/// What the report contains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Include {
    /// The open job (file, preview, calculation); the whole `job/` folder.
    pub job: bool,
    /// Customer name and address; off blanks them everywhere.
    pub customer: bool,
    /// Attached drawings.
    pub files: bool,
    pub history: bool,
    /// The plan and 3D pictures.
    pub snapshot: bool,
    /// The technical log.
    pub log: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    pub description: String,
    pub job_id: Option<String>,
    pub include: Include,
    pub source: Source,
    /// Who wrote it: `user` for the app, the client's name over MCP.
    pub reporter: String,
    /// The user's local time `YYYYMMDD-HHMMSS` for the file name; UTC when
    /// absent or not of that shape.
    #[serde(default)]
    pub local_stamp: Option<String>,
    /// UI state, recent steps and notices; only known fields are copied.
    #[serde(default)]
    pub client: Value,
    /// The editor's job when it differs from the file (unsaved edits).
    #[serde(default)]
    pub editor_project: Option<String>,
    #[serde(default)]
    pub plan_svg: Option<String>,
    #[serde(default)]
    pub model_png: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub enum Dest {
    /// An existing folder; the file gets the report name.
    Folder(PathBuf),
    /// Exactly this file, a plain `*.zip` name.
    File(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub name: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub format: String,
    pub format_version: u32,
    pub app_version: String,
    pub engine_version: String,
    pub created_ms: u64,
    pub source: String,
    pub entries: Vec<Entry>,
    pub left_out: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Created {
    pub path: PathBuf,
    pub file_name: String,
    pub size: u64,
    pub manifest: Manifest,
}

/// One report in a folder, for listings.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReportInfo {
    pub file_name: String,
    pub size: u64,
    pub created_ms: u64,
    pub source: String,
    pub job_id: Option<String>,
    /// The first characters of the description.
    pub description: String,
}

/// `tectora-report-<stamp>-<slug>.zip`, plain ASCII: the slug of the job
/// name (at most 40 characters), `app` without a job.
pub fn file_name(job_name: Option<&str>, stamp: &str) -> String {
    let slug = match job_name {
        Some(n) => {
            let s: String = crate::store::slug(n).chars().take(40).collect();
            let s = s.trim_matches('-').to_string();
            if s.is_empty() { "app".into() } else { s }
        }
        None => "app".into(),
    };
    format!("{PREFIX}{stamp}-{slug}.{EXT}")
}

/// The folder reports go to: the setting, else `<library>/reports`.
pub fn reports_dir(settings: &Settings, store: &Store) -> PathBuf {
    settings
        .reports_dir
        .clone()
        .unwrap_or_else(|| store.dir.join("reports"))
}

/// The folder inside the library, created when missing and refused when a
/// link or a file is in its place.
pub fn ensure_library_reports(store: &Store) -> Result<PathBuf, FileError> {
    let dir = store.dir.join("reports");
    fs::create_dir_all(&store.dir).map_err(|e| FileError::io(&store.dir, e))?;
    crate::store::own_folder(&dir).map_err(|e| FileError::io(&dir, e))?;
    Ok(dir)
}

/// Whether `name` is the name of a report file (not that it is one).
pub fn is_report_name(name: &str) -> bool {
    name.starts_with(PREFIX)
        && name.len() > PREFIX.len() + EXT.len() + 1
        && name.to_ascii_lowercase().ends_with(".zip")
        && crate::store::valid_file_name(name)
}

fn stamp_ok(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 15
        && b[8] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 8 || c.is_ascii_digit())
}

fn utc_stamp(ms: u64) -> String {
    let (y, mo, d, h, mi, s) = crate::applog::utc_parts(ms);
    format!("{y:04}{mo:02}{d:02}-{h:02}{mi:02}{s:02}")
}

fn local_text(stamp: &str) -> String {
    format!(
        "{}-{}-{} {}:{}:{}",
        &stamp[0..4],
        &stamp[4..6],
        &stamp[6..8],
        &stamp[9..11],
        &stamp[11..13],
        &stamp[13..15]
    )
}

// ------------------------------------------------------------ cleaning

fn bad_char(c: char) -> bool {
    c.is_control() || crate::store::is_invisible(c) || matches!(c, '\u{2028}' | '\u{2029}')
}

/// Plain text: control, invisible and bidi characters removed (a line break
/// kept when `multiline`), cut to `max` characters.
fn text(s: &str, max: usize, multiline: bool) -> String {
    let kept: String = s
        .chars()
        .filter(|&c| (multiline && c == '\n') || !bad_char(c))
        .collect();
    crate::edit::capped(&kept, max)
}

fn line(s: &str, max: usize) -> String {
    text(s, max, false)
}

fn string_field(v: &Value, key: &str, max: usize) -> Value {
    match v.get(key) {
        Some(Value::String(s)) => json!(line(s, max)),
        _ => Value::Null,
    }
}

fn number_field(v: &Value, key: &str) -> Value {
    match v.get(key) {
        Some(Value::Number(n)) => match n.as_u64() {
            Some(u) => json!(u),
            None => json!(0),
        },
        _ => json!(0),
    }
}

fn bool_field(v: &Value, key: &str) -> Value {
    json!(v.get(key).and_then(Value::as_bool).unwrap_or(false))
}

fn pick_ui(client: &Value) -> Value {
    let Some(ui) = client.get("ui").filter(|u| u.is_object()) else {
        return Value::Null;
    };
    let window = match ui.get("window").and_then(Value::as_array) {
        Some(w) if w.len() == 2 => {
            json!(
                w.iter()
                    .map(|n| n.as_u64().unwrap_or(0))
                    .collect::<Vec<_>>()
            )
        }
        _ => Value::Null,
    };
    json!({
        "locale": string_field(ui, "locale", 20),
        "theme": string_field(ui, "theme", 20),
        "screen": string_field(ui, "screen", 40),
        "layout": string_field(ui, "layout", 40),
        "window": window,
        "webgl": bool_field(ui, "webgl"),
    })
}

/// The newest `max` objects of the array `client[key]`, each reduced to the
/// fields `pick` makes.
fn pick_list(client: &Value, key: &str, max: usize, pick: impl Fn(&Value) -> Value) -> Vec<Value> {
    let Some(list) = client.get(key).and_then(Value::as_array) else {
        return Vec::new();
    };
    let items: Vec<&Value> = list.iter().filter(|v| v.is_object()).collect();
    let skip = items.len().saturating_sub(max);
    items[skip..].iter().map(|v| pick(v)).collect()
}

fn pick_action(a: &Value) -> Value {
    json!({
        "ts_ms": number_field(a, "ts_ms"),
        "kind": string_field(a, "kind", MAX_STRING),
        "name": string_field(a, "name", MAX_STRING),
        "ok": bool_field(a, "ok"),
        "code": string_field(a, "code", MAX_STRING),
        "ms": number_field(a, "ms"),
    })
}

fn pick_notice(n: &Value) -> Value {
    json!({
        "ts_ms": number_field(n, "ts_ms"),
        "kind": string_field(n, "kind", MAX_STRING),
        "code": string_field(n, "code", MAX_STRING),
        "title": string_field(n, "title", MAX_STRING),
        "detail": string_field(n, "detail", MAX_DETAIL),
    })
}

// ------------------------------------------------------------ the job

fn blank_customer(p: &mut Project) {
    p.job.customer.clear();
    p.job.address.clear();
}

/// The change log with the customer fields of every version blanked and
/// `changes` worked out again; lines that do not read are left out.
fn blank_history(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for l in bytes.split(|&b| b == b'\n') {
        let Ok(mut e) = serde_json::from_slice::<crate::store::ActivityEntry>(l) else {
            continue;
        };
        for p in [&mut e.before, &mut e.after].into_iter().flatten() {
            blank_customer(p);
        }
        e.changes = match (&e.before, &e.after) {
            (Some(b), Some(a)) => crate::diff::diff(b, a),
            _ => Vec::new(),
        };
        if let Ok(mut s) = serde_json::to_vec(&e) {
            s.push(b'\n');
            out.extend_from_slice(&s);
        }
    }
    out
}

enum Body {
    Bytes(Vec<u8>),
    File(PathBuf),
}

struct Item {
    name: String,
    size: u64,
    body: Body,
}

struct Gathered {
    items: Vec<Item>,
    left_out: Vec<String>,
    job: Value,
    calculation: Value,
    job_name: Option<String>,
}

fn calculation_of(project: &Project) -> Value {
    match crate::commands::calc(project) {
        Err(e) => json!({
            "ok": false,
            "error": { "code": line(e.code, 60), "detail": line(&e.detail, MAX_STRING) },
            "problems": [], "warnings": [], "notices": [],
        }),
        Ok(v) => {
            let mut error = Value::Null;
            let mut warnings = Vec::new();
            if let Some(parts) = v["parts"].as_array() {
                for (i, part) in parts.iter().enumerate() {
                    let g = &part["geometry"];
                    if error.is_null()
                        && let Some(e) = g.get("error").filter(|e| e.is_object())
                    {
                        error = json!({
                            "code": string_field(e, "kind", 60),
                            "detail": string_field(e, "code", 60),
                            "part": i,
                        });
                    }
                    for w in g["warnings"].as_array().into_iter().flatten().take(50) {
                        warnings.push(json!({ "code": string_field(w, "kind", 60), "part": i }));
                    }
                }
            }
            let problems: Vec<Value> = v["problems"]
                .as_array()
                .into_iter()
                .flatten()
                .take(50)
                .map(|p| json!(line(&p.to_string(), MAX_STRING)))
                .collect();
            json!({
                "ok": error.is_null(),
                "error": error,
                "problems": problems,
                "warnings": warnings.into_iter().take(100).collect::<Vec<_>>(),
                "notices": [],
            })
        }
    }
}

fn gather(store: &Store, req: &Request) -> Gathered {
    let inc = req.include;
    let mut g = Gathered {
        items: Vec::new(),
        left_out: Vec::new(),
        job: Value::Null,
        calculation: Value::Null,
        job_name: None,
    };
    let (Some(id), true) = (req.job_id.as_deref(), inc.job) else {
        return g;
    };
    let mut job = json!({
        "id": line(id, 100), "name": null, "status": null, "schema_version": null,
        "load_error": null, "anonymized": !inc.customer, "pending_review": false,
        "attachments": [],
    });
    let put = |items: &mut Vec<Item>, name: &str, bytes: Vec<u8>| {
        items.push(Item {
            name: name.into(),
            size: bytes.len() as u64,
            body: Body::Bytes(bytes),
        });
    };
    let loaded = store.load(id);
    let file = store
        .path(id)
        .ok()
        .and_then(|p| crate::store::read_regular(&p, crate::store::MAX_JOB_FILE).ok());
    match &loaded {
        Err(e) => {
            job["load_error"] = json!(line(
                &format!("{}: {}", e.code(), e.detail().unwrap_or("")),
                MAX_STRING
            ));
            g.calculation = json!({
                "ok": false,
                "error": { "code": e.code(), "detail": line(e.detail().unwrap_or(""), MAX_STRING) },
                "problems": [], "warnings": [], "notices": [],
            });
            // The raw file is kept for a developer unless it may hold the
            // customer and cannot be blanked.
            if let Some(raw) = file {
                if inc.customer {
                    put(&mut g.items, "job/project.tectora", raw);
                } else {
                    g.left_out
                        .push("job/project.tectora (unreadable, may name the customer)".into());
                }
            }
        }
        Ok(project) => {
            g.job_name = Some(project.name.clone());
            job["name"] = json!(line(&project.name, MAX_STRING));
            job["status"] = serde_json::to_value(project.job.status).unwrap_or(Value::Null);
            job["schema_version"] = json!(project.schema_version);
            job["pending_review"] = json!(store.pending_review(id).is_some());
            let bytes = if inc.customer {
                file.or_else(|| project.to_json().ok().map(String::into_bytes))
            } else {
                let mut p = project.clone();
                blank_customer(&mut p);
                p.to_json().ok().map(String::into_bytes)
            };
            if let Some(b) = bytes {
                put(&mut g.items, "job/project.tectora", b);
            }
            // The editor's copy, only when it differs.
            if let Some(t) = req.editor_project.as_deref() {
                if t.len() as u64 > MAX_TEXT_ENTRY {
                    g.left_out
                        .push("job/editor-project.tectora (too large)".into());
                } else {
                    match Project::from_json(t) {
                        Ok(ed) if &ed == project => {}
                        Ok(mut ed) => {
                            if !inc.customer {
                                blank_customer(&mut ed);
                            }
                            if let Ok(s) = ed.to_json() {
                                put(&mut g.items, "job/editor-project.tectora", s.into_bytes());
                            }
                        }
                        Err(_) if inc.customer => {
                            put(
                                &mut g.items,
                                "job/editor-project.tectora",
                                t.as_bytes().to_vec(),
                            );
                        }
                        Err(_) => g.left_out.push(
                            "job/editor-project.tectora (unreadable, may name the customer)".into(),
                        ),
                    }
                }
            }
            g.calculation = calculation_of(project);
            if project.check_limits().is_ok()
                && let Ok(svg) = crate::work::calculate(|| crate::bundle::preview_svg(project))
            {
                put(&mut g.items, "job/preview.svg", svg.into_bytes());
            }
        }
    }
    if inc.history
        && let Ok(path) = store.activity_path(id)
        && let Ok(history) = crate::store::read_newest_lines(&path, MAX_TEXT_ENTRY)
    {
        let newest = crate::bundle::newest_lines(&history, MAX_TEXT_ENTRY as usize);
        let out = if inc.customer {
            newest.to_vec()
        } else {
            blank_history(newest)
        };
        if !out.is_empty() {
            put(&mut g.items, "job/history.jsonl", out);
        }
    }
    // Attached drawings: listed always, packed when asked and within limits.
    let attachments = store.try_attachments(id).unwrap_or_default();
    let total: u64 = attachments.iter().map(|a| a.size).sum();
    let packable = attachments.len() <= MAX_FILES && total <= MAX_FILES_TOTAL;
    let mut list = Vec::new();
    for a in attachments.iter().take(MAX_FILES + 1) {
        let fits = packable && a.size <= MAX_ENTRY && crate::store::valid_file_name(&a.name);
        let included = inc.files && fits;
        if included && let Ok(path) = store.attachment_path(id, &a.name) {
            g.items.push(Item {
                name: format!("job/files/{}", a.name),
                size: a.size,
                body: Body::File(path),
            });
        }
        list.push(
            json!({ "name": line(&a.name, MAX_STRING), "size": a.size, "included": included }),
        );
    }
    if inc.files && (!packable || attachments.iter().any(|a| a.size > MAX_ENTRY)) {
        g.left_out.push("job/files (over the size limits)".into());
    }
    job["attachments"] = json!(list);
    g.job = job;
    g
}

fn settings_value() -> Value {
    let s = Settings::load();
    json!({
        "follow_ai": s.follow_ai,
        "roof_defaults": s.roof_defaults,
        "projects_dir_is_default": s.projects_dir.is_none()
            && crate::settings::env_projects_dir().is_none(),
    })
}

// ------------------------------------------------------------ writing

fn zip_err(dest: &Path) -> impl Fn(zip::result::ZipError) -> FileError + '_ {
    move |e| match e {
        zip::result::ZipError::Io(e) => FileError::io(dest, e),
        e => FileError::new("io", format!("{}: {e}", dest.display())),
    }
}

/// Plain `*.zip` check and the library's folders, for [`Dest::File`].
fn check_file_dest(store: &Store, dest: &Path) -> Result<(), FileError> {
    if !dest.is_absolute() {
        return Err(FileError::new(
            "invalid_input",
            format!("{}: the path must be absolute", dest.display()),
        ));
    }
    crate::bundle::check_dest_ext(store, dest, EXT)?;
    // The library's own folders, also the plain `files` one.
    if let (Some(parent), Ok(lib)) = (dest.parent(), store.dir.canonicalize())
        && let Ok(at) = parent.canonicalize()
        && at.starts_with(lib.join("files"))
    {
        return Err(FileError::new(
            "invalid_name",
            format!(
                "{}: choose a folder outside the library's own folders",
                dest.display()
            ),
        ));
    }
    if let Ok(m) = fs::symlink_metadata(dest)
        && !m.is_file()
    {
        return Err(FileError::new(
            "invalid_name",
            format!("{}: not a file", dest.display()),
        ));
    }
    Ok(())
}

fn check_folder(dir: &Path) -> Result<(), FileError> {
    if !dir.is_absolute() {
        return Err(FileError::new(
            "invalid_input",
            format!("{}: the folder must be an absolute path", dir.display()),
        ));
    }
    match fs::metadata(dir) {
        Ok(m) if m.is_dir() => Ok(()),
        Ok(_) => Err(FileError::new(
            "invalid_input",
            format!("{}: not a folder", dir.display()),
        )),
        Err(e) => Err(FileError::io(dir, e)),
    }
}

/// Writes a report; see the module's documentation. Nothing is left in the
/// destination when it fails: the ZIP is built beside it as a `.part` file
/// and moved into place at the end.
pub fn create(store: &Store, req: &Request, dest: &Dest) -> Result<Created, FileError> {
    if req.description.chars().count() > MAX_DESCRIPTION {
        return Err(FileError::new(
            "too_large",
            format!("the description is longer than {MAX_DESCRIPTION} characters"),
        ));
    }
    let dir = match dest {
        Dest::Folder(d) => {
            check_folder(d)?;
            d.clone()
        }
        Dest::File(f) => {
            check_file_dest(store, f)?;
            f.parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."))
                .to_path_buf()
        }
    };
    let now = now_ms();
    let local = req
        .local_stamp
        .as_deref()
        .filter(|s| req.source == Source::App && stamp_ok(s));
    let stamp = local.map_or_else(|| utc_stamp(now), str::to_string);

    let mut g = gather(store, req);
    let inc = req.include;
    if inc.snapshot {
        match req.plan_svg.as_deref() {
            Some(svg) if svg.len() <= MAX_PLAN_SVG && svg.trim_start().starts_with("<svg") => {
                g.items.push(Item {
                    name: "snapshot/plan.svg".into(),
                    size: svg.len() as u64,
                    body: Body::Bytes(svg.as_bytes().to_vec()),
                });
            }
            Some(_) => g
                .left_out
                .push("snapshot/plan.svg (too large or not an SVG)".into()),
            None => {}
        }
        match req.model_png.as_deref() {
            Some(png) if png.len() <= MAX_MODEL_PNG && png.starts_with(&PNG_SIGNATURE) => {
                g.items.push(Item {
                    name: "snapshot/model.png".into(),
                    size: png.len() as u64,
                    body: Body::Bytes(png.to_vec()),
                });
            }
            Some(_) => g
                .left_out
                .push("snapshot/model.png (too large or not a PNG)".into()),
            None => {}
        }
    }
    if inc.log {
        for role in ["app", "mcp"] {
            let t = crate::applog::tail(role, MAX_LOG_BYTES, MAX_LOG_LINES);
            if !t.is_empty() {
                g.items.push(Item {
                    name: format!("logs/{role}.log"),
                    size: t.len() as u64,
                    body: Body::Bytes(t.into_bytes()),
                });
            }
        }
    }

    let from_ui = req.source == Source::App;
    let client = if from_ui { &req.client } else { &Value::Null };
    let report = json!({
        "format_version": FORMAT_VERSION,
        "description": text(&req.description, MAX_DESCRIPTION, true),
        "created_ms": now,
        "created_local": local.map(local_text),
        "source": req.source.as_str(),
        "reporter": line(&req.reporter, 100),
        "app": {
            "version": env!("CARGO_PKG_VERSION"),
            "engine_version": crate::snapshot::ENGINE_VERSION,
            "schema_version": crate::model::SCHEMA_VERSION,
        },
        "system": { "os": std::env::consts::OS, "arch": std::env::consts::ARCH },
        "ui": if from_ui { pick_ui(client) } else { Value::Null },
        "job": g.job,
        "calculation": g.calculation,
        "actions": pick_list(client, "actions", MAX_ACTIONS, pick_action),
        "notices": pick_list(client, "notices", MAX_NOTICES, pick_notice),
        "settings": settings_value(),
        "included": {
            "job": inc.job, "customer": inc.customer, "files": inc.files,
            "history": inc.history, "snapshot": inc.snapshot, "log": inc.log,
        },
    });
    let report_bytes = serde_json::to_vec_pretty(&report)
        .map_err(|e| FileError::new("internal", e.to_string()))?;
    let mut items = vec![Item {
        name: "report.json".into(),
        size: report_bytes.len() as u64,
        body: Body::Bytes(report_bytes),
    }];
    items.append(&mut g.items);
    let manifest = Manifest {
        format: FORMAT.into(),
        format_version: FORMAT_VERSION,
        app_version: env!("CARGO_PKG_VERSION").into(),
        engine_version: crate::snapshot::ENGINE_VERSION.into(),
        created_ms: now,
        source: req.source.as_str().into(),
        entries: items
            .iter()
            .map(|i| Entry {
                name: i.name.clone(),
                size: i.size,
            })
            .collect(),
        left_out: g.left_out,
    };

    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let part = dir.join(format!(".tectora-report-{}-{n}.part", std::process::id()));
    let written = write_zip(&part, dest_label(dest), &manifest, items);
    let moved = written.and_then(|()| {
        let target = match dest {
            Dest::File(f) => f.clone(),
            Dest::Folder(d) => free_name(d, &file_name(g.job_name.as_deref(), &stamp))?,
        };
        fs::rename(&part, &target).map_err(|e| FileError::io(&target, e))?;
        crate::store::sync_parent(&target);
        Ok(target)
    });
    match moved {
        Err(e) => {
            let _ = fs::remove_file(&part);
            Err(e)
        }
        Ok(path) => {
            let size = fs::metadata(&path).map_or(0, |m| m.len());
            let file_name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            Ok(Created {
                path,
                file_name,
                size,
                manifest,
            })
        }
    }
}

fn dest_label(dest: &Dest) -> &Path {
    match dest {
        Dest::Folder(p) | Dest::File(p) => p,
    }
}

/// The first of `name`, `name-2`, … that nothing (not even a link) is at.
fn free_name(dir: &Path, name: &str) -> Result<PathBuf, FileError> {
    let stem = name.strip_suffix(".zip").unwrap_or(name);
    for n in 1..1000 {
        let file = if n == 1 {
            format!("{stem}.zip")
        } else {
            format!("{stem}-{n}.zip")
        };
        let path = dir.join(file);
        if fs::symlink_metadata(&path).is_err() {
            return Ok(path);
        }
    }
    Err(FileError::new(
        "invalid_name",
        format!("{}: too many reports with the name {name}", dir.display()),
    ))
}

fn write_zip(
    part: &Path,
    label: &Path,
    manifest: &Manifest,
    items: Vec<Item>,
) -> Result<(), FileError> {
    // Never through a link or a file left in its place.
    if fs::symlink_metadata(part).is_ok_and(|m| !m.is_dir()) {
        let _ = fs::remove_file(part);
    }
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(part)
        .map_err(|e| FileError::io(label, e))?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let json = serde_json::to_vec_pretty(manifest)
        .map_err(|e| FileError::new("internal", e.to_string()))?;
    zip.start_file("manifest.json", opts)
        .map_err(zip_err(label))?;
    zip.write_all(&json).map_err(|e| FileError::io(label, e))?;
    for item in items {
        zip.start_file(&item.name, opts).map_err(zip_err(label))?;
        match item.body {
            Body::Bytes(b) => zip.write_all(&b).map_err(|e| FileError::io(label, e))?,
            Body::File(path) => {
                let mut f = crate::store::open_regular(&path, MAX_ENTRY)?;
                std::io::copy(&mut f, &mut zip).map_err(|e| FileError::io(label, e))?;
            }
        }
    }
    let file = zip.finish().map_err(zip_err(label))?;
    file.sync_all().map_err(|e| FileError::io(label, e))
}

// ------------------------------------------------------------ reading

fn bad_name(name: &str) -> FileError {
    FileError::new(
        "invalid_name",
        format!("`{}` is not the name of a bug report", line(name, 120)),
    )
}

fn open_archive(dir: &Path, name: &str) -> Result<zip::ZipArchive<fs::File>, FileError> {
    if !is_report_name(name) {
        return Err(bad_name(name));
    }
    let file = crate::store::open_regular(&dir.join(name), MAX_REPORT_FILE)?;
    zip::ZipArchive::new(file).map_err(|e| {
        FileError::new(
            "unreadable",
            format!("{}: not a bug report: {e}", line(name, 120)),
        )
    })
}

fn json_entry(
    z: &mut zip::ZipArchive<fs::File>,
    name: &str,
    what: &str,
) -> Result<Value, FileError> {
    let bytes = crate::bundle::read_entry(z, name, MAX_JSON_ENTRY)?.ok_or_else(|| {
        FileError::new(
            "unreadable",
            format!("{what}: not a bug report ({name} is missing)"),
        )
    })?;
    serde_json::from_slice(&bytes)
        .map_err(|e| FileError::new("unreadable", format!("{what}: {name}: {e}")))
}

/// The manifest and the report of the bug report `name` (a plain file name)
/// in `dir`.
pub fn read(dir: &Path, name: &str) -> Result<Value, FileError> {
    let mut z = open_archive(dir, name)?;
    let manifest = json_entry(&mut z, "manifest.json", name)?;
    if manifest["format"] != FORMAT {
        return Err(FileError::new(
            "unreadable",
            format!("{}: not a bug report", line(name, 120)),
        ));
    }
    let report = json_entry(&mut z, "report.json", name)?;
    Ok(json!({ "manifest": manifest, "report": report }))
}

/// The reports in `dir`, newest first (by the time in the file name), at
/// most `limit`. Files that are not reports are left out.
pub fn list(dir: &Path, limit: usize) -> Vec<ReportInfo> {
    let Ok(rd) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = rd
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| is_report_name(n))
        .take(MAX_LISTED_FILES)
        .collect();
    names.sort_by(|a, b| b.cmp(a));
    let mut out = Vec::new();
    for name in names {
        if out.len() >= limit {
            break;
        }
        let Ok(v) = read(dir, &name) else {
            continue;
        };
        let size = fs::metadata(dir.join(&name)).map_or(0, |m| m.len());
        let r = &v["report"];
        out.push(ReportInfo {
            file_name: name,
            size,
            created_ms: v["manifest"]["created_ms"].as_u64().unwrap_or(0),
            source: line(v["manifest"]["source"].as_str().unwrap_or(""), 20),
            job_id: r["job"]["id"].as_str().map(|s| line(s, 100)),
            description: line(
                &r["description"]
                    .as_str()
                    .unwrap_or("")
                    .chars()
                    .take(120)
                    .collect::<String>(),
                120,
            ),
        });
    }
    out
}
