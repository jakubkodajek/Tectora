//! Dev-only HTTP bridge so the desktop UI runs in a plain browser.
//!
//! `app/preview.html` replaces Tauri's `invoke` with a POST to this server,
//! which answers the same commands as `app/src-tauri/src/lib.rs` over a
//! `Store`. It also reports file changes in the library (the Tauri watcher's
//! `projects-changed` and `live-status` events) through `GET /__events`, so
//! edits made by `tectora mcp --dir <same folder>` show up live in the browser.
//!
//! Run: `cargo run -p tectora-core --example ui_bridge -- [library dir] [port]`
//! (defaults: `target/ui-preview-library`, 7878). Usually started by
//! `npm run preview` in `app/`.
//!
//! Keep the command list in sync with `app/src-tauri/src/lib.rs` (checked by
//! `tests/app_commands.rs`: names and arguments). Unknown
//! commands return an error that the preview shows in the browser console.
//! Listens on 127.0.0.1 only, and answers only the preview page
//! (`Origin: http://localhost:<TECTORA_PREVIEW_PORT>`, default 1430) and
//! clients without a browser (no `Origin`): another web page open in the
//! developer's browser could otherwise move the jobs folder, copy local files
//! into the library or write files (ARCH-009).

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::{Value, json};
use tectora_core::examples::{StandardRoof, standard_roof};
use tectora_core::settings::Settings;
use tectora_core::store::{self, Store, USER_ACTOR, outlines};
use tectora_core::{Project, assistants, bundle, examples};

/// AI apps "connected" in the preview besides Claude Desktop, which always
/// shows as connected (see `ai_assistants` below).
static PREVIEW_CONNECTED: std::sync::Mutex<Vec<assistants::Assistant>> =
    std::sync::Mutex::new(Vec::new());

fn preview_assistants() -> Vec<Value> {
    let connected = PREVIEW_CONNECTED.lock().unwrap_or_else(|e| e.into_inner());
    assistants::Assistant::ALL
        .iter()
        .map(|&a| {
            let state = if a == assistants::Assistant::Antigravity {
                "not_installed"
            } else if a == assistants::Assistant::ClaudeDesktop || connected.contains(&a) {
                "connected"
            } else {
                "not_connected"
            };
            json!({ "id": a, "name": a.name(), "state": state, "files": [] })
        })
        .collect()
}

/// Largest request body accepted (a project with attachments metadata).
const MAX_BODY: usize = 64 * 1024 * 1024;

fn arg<T: serde::de::DeserializeOwned>(a: &Value, key: &str) -> Result<T, Fail> {
    serde_json::from_value(a.get(key).cloned().unwrap_or(Value::Null))
        .map_err(|e| format!("argument {key}: {e}").into())
}

fn to_json<T: serde::Serialize>(v: T) -> Result<Value, Fail> {
    serde_json::to_value(v).map_err(|e| e.to_string().into())
}

/// A failed command, as the app's `AppError` gives it: `{code, id, detail}`
/// for errors with a code (library, files, limits), so the UI shows the same
/// translated message as in the app; plain text otherwise.
struct Fail(Value);

impl From<String> for Fail {
    fn from(e: String) -> Self {
        Fail(Value::String(e))
    }
}

impl From<&str> for Fail {
    fn from(e: &str) -> Self {
        Fail(Value::String(e.to_string()))
    }
}

impl From<tectora_core::store::StoreError> for Fail {
    fn from(e: tectora_core::store::StoreError) -> Self {
        Fail(json!({ "code": e.code(), "id": e.id(), "detail": e.to_string() }))
    }
}

impl From<tectora_core::store::FileError> for Fail {
    fn from(e: tectora_core::store::FileError) -> Self {
        Fail(json!({ "code": e.code, "id": e.id, "detail": e.detail }))
    }
}

fn coded(code: &str, detail: impl std::fmt::Display) -> Fail {
    Fail(json!({ "code": code, "detail": detail.to_string() }))
}

/// The calculation commands, with the app's own bodies
/// (`tectora_core::commands`) and its error codes (`out_of_range`,
/// `too_many`, `internal`), so the UI shows the same translated message as
/// in the app.
fn calc_command(cmd: &str, a: &Value) -> Option<Result<Value, Value>> {
    use tectora_core::commands;
    let project = || {
        arg::<Project>(a, "project")
            .map_err(|Fail(e)| json!({ "code": "invalid_input", "detail": e }))
    };
    let err = |e: commands::CommandError| to_json(e).unwrap_or(Value::Null);
    Some(match cmd {
        "calc" => project().and_then(|p| commands::calc(&p).map_err(err)),
        "calc_breakdown" => project().and_then(|p| commands::calc_breakdown(&p).map_err(err)),
        "take_snapshot" => project().and_then(|p| {
            commands::take_snapshot(&p)
                .map_err(err)
                .map(|s| to_json(s).unwrap_or(Value::Null))
        }),
        "check_windows" => project().and_then(|p| commands::check_windows(&p).map_err(err)),
        "dormer_host" => project().and_then(|p| {
            let index = arg::<usize>(a, "index")
                .map_err(|Fail(e)| json!({ "code": "invalid_input", "detail": e }))?;
            commands::dormer_host(&p, index).map_err(err)
        }),
        _ => return None,
    })
}

/// Loading and saving a job, with the library's own error (see `serve`).
fn store_command(store: &Store, cmd: &str, a: &Value) -> Option<Result<Value, Fail>> {
    let id = arg::<String>(a, "id").ok()?;
    match cmd {
        "load_project" => Some(
            store
                .load(&id)
                .map(|p| to_json(p).unwrap_or(Value::Null))
                .map_err(Fail::from),
        ),
        "save_project" => {
            let project = arg::<Project>(a, "project").ok()?;
            Some(
                store
                    .save(Some(id), &project)
                    .map(Value::String)
                    .map_err(Fail::from),
            )
        }
        // As the app: saved, or `conflict` with the current file.
        "save_project_checked" => {
            let project = arg::<Project>(a, "project").ok()?;
            let base = arg::<Project>(a, "base").ok()?;
            Some(
                store
                    .save_checked(&id, &project, &base)
                    .map(|r| to_json(r).unwrap_or(Value::Null))
                    .map_err(Fail::from),
            )
        }
        // Reverts with their codes too (`revert_conflict`, ERR-001).
        "revert_last_change" => Some(
            store
                .revert_last(&id, USER_ACTOR, None)
                .map(|p| to_json(p).unwrap_or(Value::Null))
                .map_err(Fail::from),
        ),
        "revert_changes" => {
            let ts = arg::<u64>(a, "tsMs").ok()?;
            Some(
                store
                    .revert_to(&id, ts, USER_ACTOR, None)
                    .map(|p| to_json(p).unwrap_or(Value::Null))
                    .map_err(Fail::from),
            )
        }
        _ => None,
    }
}

fn handle(store: &Store, cmd: &str, a: &Value) -> Result<Value, Fail> {
    let id = || arg::<String>(a, "id");
    match cmd {
        "projects_dir" => to_json(store.dir.display().to_string()),
        "list_projects" => {
            let (projects, broken) = store.scan()?;
            Ok(json!({ "projects": projects, "broken": broken }))
        }
        // As the app: a damaged settings file is reported (ERR-005).
        "startup_warnings" => Ok(json!(
            Settings::load_checked()
                .1
                .map(|d| json!({ "code": "settings_unreadable", "detail": d }))
                .into_iter()
                .collect::<Vec<_>>()
        )),
        // TECTORA_PREVIEW_UPDATE=0.2.0 shows the update offer in the library.
        "update_check" => Ok(std::env::var("TECTORA_PREVIEW_UPDATE").map_or(
            Value::Null,
            |version| json!({ "version": version, "notes": "- Preview release notes" }),
        )),
        "update_install" => Err("updates are not installed in the preview".into()),
        "project_activity" => {
            let mut items: Vec<Value> = store
                .activity(&id()?)
                .into_iter()
                .map(|e| {
                    json!({ "ts_ms": e.ts_ms, "actor": e.actor, "summary": e.summary, "changes": e.changes })
                })
                .collect();
            items.reverse();
            Ok(Value::Array(items))
        }
        "review_status" => to_json(store.review_status(&id()?)?),
        "accept_changes" => to_json(store.mark_reviewed(&id()?, arg(a, "tsMs")?)?),
        "live_status" => to_json(store.live()),
        "mcp_command" => Ok(json!({
            "command": "cargo",
            "args": ["run", "-q", "-p", "tectora-cli", "--", "mcp", "--dir", store.dir.display().to_string()],
        })),
        // The preview never touches the developer's own AI app settings: it
        // shows Claude Desktop connected and Antigravity missing, and Connect
        // only flips the state in memory.
        "ai_assistants" => to_json(preview_assistants()),
        "connect_ai_assistant" => {
            let which: assistants::Assistant = arg(a, "id")?;
            let mut connected = PREVIEW_CONNECTED.lock().unwrap_or_else(|e| e.into_inner());
            if which == assistants::Assistant::Antigravity {
                return Err("ai_not_installed: Antigravity is not installed for this user".into());
            }
            let change = if connected.contains(&which) {
                "unchanged"
            } else {
                "added"
            };
            if !connected.contains(&which) {
                connected.push(which);
            }
            to_json(change)
        }
        "load_project" => to_json(store.load(&id()?)?),
        "save_project" => {
            let project: Project = arg(a, "project")?;
            to_json(store.save(arg(a, "id")?, &project)?)
        }
        "normalize_project" => to_json(arg::<Project>(a, "project")?),
        "delete_project" => {
            store.delete(&id()?, USER_ACTOR, "Deleted in the app")?;
            Ok(Value::Null)
        }
        "duplicate_project" => {
            let source = id()?;
            let mut project = store.load(&source)?;
            let name: String = arg(a, "name")?;
            tectora_core::library::prepare_copy(&mut project, &name, arg(a, "job")?);
            let new_id = store.save(None, &project)?;
            if arg::<Option<bool>>(a, "withFiles")?.unwrap_or(true) {
                store.copy_attachments(&source, &new_id)?;
            }
            to_json(new_id)
        }
        "list_attachments" => to_json(store.attachments(&id()?)),
        "attach_files" => {
            let job = id()?;
            let paths: Vec<String> = arg(a, "paths")?;
            let names: Result<Vec<String>, _> = paths
                .iter()
                .map(|p| store.attach_file(&job, &PathBuf::from(p)))
                .collect();
            to_json(names?)
        }
        "remove_attachment" => {
            store.remove_attachment(
                &id()?,
                &arg::<String>(a, "name")?,
                USER_ACTOR,
                "Removed in the app",
            )?;
            Ok(Value::Null)
        }
        "open_attachment" => Err("open_attachment is not available in the browser preview".into()),
        "export_job" => {
            bundle::export(store, &id()?, &PathBuf::from(arg::<String>(a, "path")?))?;
            Ok(Value::Null)
        }
        "export_preview" => {
            let opts = tectora_core::export::ListOptions::new(
                arg::<String>(a, "title")?,
                arg::<String>(a, "stamp")?,
            );
            let (list, svg) = tectora_core::export::checked(&arg::<Project>(a, "project")?, &opts)
                .map_err(|d| coded("out_of_range", d))?;
            Ok(serde_json::json!({ "list": list, "svg": svg }))
        }
        "export_plan_file" => {
            let path = PathBuf::from(arg::<String>(a, "path")?);
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            let bytes = tectora_core::export::checked_sheet_file(
                &arg::<Project>(a, "project")?,
                &arg::<String>(a, "stamp")?,
                &ext,
            )
            .map_err(|d| coded("out_of_range", d))?;
            std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        "save_png" => {
            let bytes = arg::<Vec<u8>>(a, "bytes")?;
            std::fs::write(PathBuf::from(arg::<String>(a, "path")?), bytes)
                .map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        "import_job" => to_json(bundle::open_path(
            store,
            &PathBuf::from(arg::<String>(a, "path")?),
        )?),
        "take_pending_open" => Ok(Value::Null),
        "take_open_errors" => Ok(serde_json::json!([])),
        "create_standard" | "standard_part" => {
            let kind: StandardRoof = arg(a, "kind")?;
            let part = standard_roof(
                kind,
                arg(a, "length")?,
                arg(a, "width")?,
                arg(a, "pitchDeg")?,
                arg(a, "eaveHeight")?,
                arg(a, "overhang")?,
            );
            if cmd == "standard_part" {
                return to_json(part);
            }
            let mut project = Project::new(arg::<String>(a, "name")?);
            project.parts.push(part);
            to_json(store.save(None, &project)?)
        }
        "list_examples" => Ok(Value::Array(
            examples::EXAMPLES
                .iter()
                .map(|e| {
                    json!({
                        "id": e.id,
                        "name": e.name,
                        "description": e.description,
                        "outlines": outlines(&e.project()),
                        "lines": store::roof_lines(&e.project()),
                    })
                })
                .collect(),
        )),
        "create_from_example" => {
            let example_id: String = arg(a, "exampleId")?;
            let example = examples::find(&example_id)
                .ok_or_else(|| format!("unknown example {example_id}"))?;
            let mut project = example.project();
            if let Some(name) = arg::<Option<String>>(a, "name")?.filter(|n| !n.trim().is_empty()) {
                project.name = name;
            }
            to_json(store.save(None, &project)?)
        }
        "new_project" => {
            let mut project = examples::find("gable").expect("built-in example").project();
            project.name = arg(a, "name")?;
            to_json(store.save(None, &project)?)
        }
        "face_to_part" => {
            let mut project = arg::<Project>(a, "project")?;
            tectora_core::edit::face_to_part(
                &mut project,
                arg(a, "index")?,
                &arg::<String>(a, "name")?,
                arg(a, "pitchDeg")?,
                arg(a, "eaveHeight")?,
                arg(a, "overhang")?,
            )?;
            to_json(project)
        }
        "set_part_height" => {
            let mut project = arg::<Project>(a, "project")?;
            tectora_core::edit::height::set_part_height(
                &mut project,
                arg(a, "part")?,
                arg(a, "height")?,
            )?;
            to_json(project)
        }
        // Settings live in `target/ui-preview-config` (see `main`). A new
        // projects folder set through the bridge is served from then on
        // (`serve`); one set by another process (MCP) is not.
        "get_settings" => Ok(Settings::load().describe()),
        "update_settings" => {
            let patch = a.get("patch").cloned().unwrap_or(Value::Null);
            let next = Settings::load()
                .patched(&patch)
                .map_err(|e| e.code().to_string())?;
            next.save().map_err(|e| e.code().to_string())?;
            Ok(next.describe())
        }
        "answer_folder_request" => {
            let allow = a.get("allow").and_then(Value::as_bool).unwrap_or(false);
            let next = Settings::load()
                .answer_folder_request(allow)
                .map_err(|e| e.code().to_string())?;
            next.save().map_err(|e| e.code().to_string())?;
            Ok(next.describe())
        }
        "open_folder" => Err("open_folder is not available in the browser preview".into()),
        _ => Err(format!(
            "ui_bridge: unknown command {cmd} (add it to crates/tectora-core/examples/ui_bridge.rs)"
        )
        .into()),
    }
}

/// Modification times of the library's job files, the live status file and
/// the settings file (the app's watcher covers the settings folder too).
fn scan(dir: &Path) -> BTreeMap<PathBuf, SystemTime> {
    let mut out = BTreeMap::new();
    let settings = tectora_core::settings::settings_path();
    if let Ok(modified) = std::fs::metadata(&settings).and_then(|m| m.modified()) {
        out.insert(settings, modified);
    }
    for folder in [dir.to_path_buf(), dir.join(store::ACTIVITY_DIR)] {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let is_job = path.extension().and_then(|e| e.to_str()) == Some(store::EXTENSION);
            let is_live = path.file_name().and_then(|n| n.to_str()) == Some(store::LIVE_FILE);
            if (is_job || is_live)
                && let Ok(modified) = entry.metadata().and_then(|m| m.modified())
            {
                out.insert(path, modified);
            }
        }
    }
    out
}

/// What changed since the last poll, as the Tauri watcher would emit it.
fn events(store: &Store, seen: &mut BTreeMap<PathBuf, SystemTime>) -> Value {
    let now = scan(&store.dir);
    let mut ids = Vec::new();
    let mut live = false;
    let mut settings_changed = false;
    let settings = tectora_core::settings::settings_path();
    let changed = now
        .keys()
        .chain(seen.keys())
        .filter(|p| now.get(*p) != seen.get(*p));
    for path in changed {
        if *path == settings {
            settings_changed = true;
        } else if path.file_name().and_then(|n| n.to_str()) == Some(store::LIVE_FILE) {
            live = true;
        } else if let Some(id) = path.file_stem().and_then(|s| s.to_str())
            && !ids.iter().any(|i| i == id)
        {
            ids.push(id.to_string());
        }
    }
    *seen = now;
    let mut out = Vec::new();
    if !ids.is_empty() {
        out.push(json!({ "event": "projects-changed", "payload": ids }));
    }
    if settings_changed {
        out.push(json!({ "event": "settings-changed", "payload": Settings::load().describe() }));
    }
    if live && let Some(status) = store.live() {
        out.push(json!({ "event": "live-status", "payload": status }));
    }
    Value::Array(out)
}

/// The page origins the bridge answers: the preview's Vite server.
fn allowed_origins() -> Vec<String> {
    let port = std::env::var("TECTORA_PREVIEW_PORT").unwrap_or_else(|_| "1430".into());
    vec![
        format!("http://localhost:{port}"),
        format!("http://127.0.0.1:{port}"),
    ]
}

/// Whether a request may be answered: from the preview page or from a client
/// without a browser (no `Origin`), and addressed to this computer by its
/// loopback name (a `Host` of another name is a DNS rebinding page).
fn allowed(origin: Option<&str>, host: Option<&str>, port: u16) -> bool {
    let host_ok =
        host.is_none_or(|h| h == format!("127.0.0.1:{port}") || h == format!("localhost:{port}"));
    let origin_ok = origin.is_none_or(|o| allowed_origins().iter().any(|a| a == o));
    host_ok && origin_ok
}

fn respond(mut stream: TcpStream, status: &str, body: &str, origin: Option<&str>) {
    // Only the preview page's own origin, never `*`.
    let cors = origin
        .map(|o| {
            format!(
                "Access-Control-Allow-Origin: {o}\r\nVary: Origin\r\n\
                 Access-Control-Allow-Headers: content-type\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\n"
            )
        })
        .unwrap_or_default();
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n{cors}\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

fn serve(
    home: &std::path::Path,
    port: u16,
    store: &mut Store,
    seen: &mut BTreeMap<PathBuf, SystemTime>,
    stream: TcpStream,
) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    let mut parts = request.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts
        .next()
        .unwrap_or("/")
        .trim_start_matches('/')
        .to_string();
    let mut len = 0usize;
    let mut origin: Option<String> = None;
    let mut host: Option<String> = None;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header.trim().is_empty() {
            break;
        }
        let Some((name, value)) = header.split_once(':') else {
            continue;
        };
        let value = value.trim().to_string();
        // Not a `match` on string literals: `tests/app_commands.rs` reads
        // those as command arms.
        let key = name.trim().to_ascii_lowercase();
        if key == "content-length" {
            len = value.parse().unwrap_or(0);
        } else if key == "origin" {
            origin = Some(value);
        } else if key == "host" {
            host = Some(value);
        }
    }
    if !allowed(origin.as_deref(), host.as_deref(), port) {
        respond(stream, "403 Forbidden", "{}", None);
        return Ok(());
    }
    let origin = origin.as_deref();
    if method == "OPTIONS" {
        respond(stream, "204 No Content", "", origin);
        return Ok(());
    }
    if len > MAX_BODY {
        respond(stream, "413 Payload Too Large", "{}", origin);
        return Ok(());
    }
    let mut body = vec![0; len];
    reader.read_exact(&mut body)?;
    let out = if path == "__events" {
        json!({ "ok": events(store, seen) })
    } else if path == "__reset" {
        // Test harness: back to the library the bridge was started with
        // (a test may have switched the jobs folder), and drop pending events.
        if store.dir != home {
            match Store::new(home.to_path_buf()) {
                Ok(next) => *store = next,
                Err(e) => {
                    respond(
                        stream,
                        "200 OK",
                        &json!({ "err": e.to_string() }).to_string(),
                        origin,
                    );
                    return Ok(());
                }
            }
        }
        json!({ "ok": events(store, seen) })
    } else {
        let cmd = path.replace("%7C", "|").replace("%3A", ":");
        let args: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
        if cmd == "update_settings" || cmd == "answer_folder_request" {
            let before = effective_projects_dir();
            let out = handle(store, &cmd, &args);
            // A new jobs folder: serve it from now on, as the app's
            // `switch_library` does (only when the setting changed, so the
            // preview's own library stays put otherwise).
            let after = effective_projects_dir();
            if out.is_ok()
                && after != before
                && let Some(dir) = after.filter(|d| !d.is_empty())
                && let Ok(next) = Store::new(PathBuf::from(dir))
            {
                *store = next;
                // As the app's switch: the jobs of the new folder are not
                // "changed", but the settings change still reaches the UI.
                let settings = tectora_core::settings::settings_path();
                let old = seen.get(&settings).copied();
                *seen = scan(&store.dir);
                match old {
                    Some(t) => seen.insert(settings, t),
                    None => seen.remove(&settings),
                };
            }
            let out = match out {
                Ok(v) => json!({ "ok": v }),
                Err(Fail(e)) => json!({ "err": e }),
            };
            respond(stream, "200 OK", &out.to_string(), origin);
            return Ok(());
        }
        if let Some(out) = calc_command(&cmd, &args) {
            let out = match out {
                Ok(v) => json!({ "ok": v }),
                Err(e) => json!({ "err": e }),
            };
            respond(stream, "200 OK", &out.to_string(), origin);
            return Ok(());
        }
        match store_command(store, &cmd, &args) {
            Some(Ok(v)) => json!({ "ok": v }),
            // Library errors with their stable code, as the Tauri commands
            // give them, so the UI shows the same translated message.
            Some(Err(Fail(e))) => json!({ "err": e }),
            None => match handle(store, &cmd, &args) {
                Ok(v) => json!({ "ok": v }),
                Err(Fail(e)) => json!({ "err": e }),
            },
        }
    };
    respond(stream, "200 OK", &out.to_string(), origin);
    Ok(())
}

/// The jobs folder the settings name (`effective.projects_dir`).
fn effective_projects_dir() -> Option<String> {
    Settings::load().describe()["effective"]["projects_dir"]
        .as_str()
        .map(str::to_string)
}

fn main() -> std::io::Result<()> {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(
        args.next()
            .unwrap_or_else(|| "target/ui-preview-library".into()),
    );
    let port: u16 = args.next().and_then(|p| p.parse().ok()).unwrap_or(7878);
    if std::env::var_os("TECTORA_CONFIG_DIR").is_none() {
        // Never touch the developer's real app settings from the preview.
        // SAFETY: set before any other thread exists.
        unsafe { std::env::set_var("TECTORA_CONFIG_DIR", "target/ui-preview-config") };
    }
    let mut store = Store::new(dir.clone())?;
    if store.list().is_empty() {
        // Same first-run seed as the app, plus a few more shapes to look at.
        for id in ["gable", "l-shape", "hip", "t-shape"] {
            if let Some(example) = examples::find(id) {
                let _ = store.save(None, &example.project());
            }
        }
    }
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    eprintln!(
        "ui_bridge: library {} on http://127.0.0.1:{port}",
        dir.display()
    );
    let mut seen = scan(&dir);
    for stream in listener.incoming().flatten() {
        if let Err(e) = serve(&dir, port, &mut store, &mut seen, stream) {
            eprintln!("ui_bridge: {e}");
        }
    }
    Ok(())
}
