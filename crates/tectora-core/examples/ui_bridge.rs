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
//! Keep the command list in sync with `app/src-tauri/src/lib.rs`. Unknown
//! commands return an error that the preview shows in the browser console.
//! Listens on 127.0.0.1 only.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::{Value, json};
use tectora_core::examples::{StandardRoof, standard_roof};
use tectora_core::settings::Settings;
use tectora_core::store::{self, Store, USER_ACTOR, outlines};
use tectora_core::{Project, assistants, bundle, examples, snapshot};

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

fn arg<T: serde::de::DeserializeOwned>(a: &Value, key: &str) -> Result<T, String> {
    serde_json::from_value(a.get(key).cloned().unwrap_or(Value::Null))
        .map_err(|e| format!("argument {key}: {e}"))
}

fn to_json<T: serde::Serialize>(v: T) -> Result<Value, String> {
    serde_json::to_value(v).map_err(|e| e.to_string())
}

/// Same as the app's `calc` command: limits, panic guard, problems and
/// the stored result's status.
fn calc(project: &Project) -> Result<Value, String> {
    project.check_limits()?;
    let mut v = tectora_core::model::guarded(|| tectora_core::report::calc(project))?;
    v["stored_result"] = to_json(snapshot::status(project))?;
    Ok(v)
}

fn handle(store: &Store, cmd: &str, a: &Value) -> Result<Value, String> {
    let id = || arg::<String>(a, "id");
    match cmd {
        "projects_dir" => to_json(store.dir.display().to_string()),
        "list_projects" => {
            let (projects, broken) = store.scan()?;
            Ok(json!({ "projects": projects, "broken": broken }))
        }
        "startup_warnings" => Ok(json!([])),
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
        "revert_last_change" => to_json(store.revert_last(&id()?, USER_ACTOR)?),
        "revert_changes" => to_json(store.revert_to(&id()?, arg(a, "tsMs")?, USER_ACTOR)?),
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
        "delete_project" => to_json(store.delete(&id()?)?),
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
            let names: Result<Vec<String>, String> = paths
                .iter()
                .map(|p| store.attach_file(&job, &PathBuf::from(p)))
                .collect();
            to_json(names?)
        }
        "remove_attachment" => {
            to_json(store.remove_attachment(&id()?, &arg::<String>(a, "name")?)?)
        }
        "open_attachment" => Err("open_attachment is not available in the browser preview".into()),
        "export_job" => {
            bundle::export(store, &id()?, &PathBuf::from(arg::<String>(a, "path")?))?;
            Ok(Value::Null)
        }
        "import_job" => to_json(bundle::open_path(
            store,
            &PathBuf::from(arg::<String>(a, "path")?),
        )?),
        "take_snapshot" => to_json(snapshot::take(&arg::<Project>(a, "project")?)),
        "take_pending_open" => Ok(Value::Null),
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
        "calc" => calc(&arg::<Project>(a, "project")?),
        "calc_breakdown" => {
            let project = arg::<Project>(a, "project")?;
            project.check_limits()?;
            to_json(tectora_core::model::guarded(|| {
                tectora_core::breakdown::breakdown(&project)
            })?)
        }
        // Settings live in `target/ui-preview-config` (see `main`). The
        // preview keeps serving its own library even if the projects folder
        // setting changes.
        "get_settings" => Ok(Settings::load().describe()),
        "update_settings" => {
            let patch = a.get("patch").cloned().unwrap_or(Value::Null);
            let next = Settings::load()
                .patched(&patch)
                .map_err(|e| e.code().to_string())?;
            next.save().map_err(|e| e.code().to_string())?;
            Ok(next.describe())
        }
        "open_folder" => Err("open_folder is not available in the browser preview".into()),
        _ => Err(format!(
            "ui_bridge: unknown command {cmd} (add it to crates/tectora-core/examples/ui_bridge.rs)"
        )),
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

fn respond(mut stream: TcpStream, status: &str, body: &str) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n\
         Access-Control-Allow-Headers: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

fn serve(
    store: &Store,
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
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header.trim().is_empty() {
            break;
        }
        if let Some(v) = header.to_ascii_lowercase().strip_prefix("content-length:") {
            len = v.trim().parse().unwrap_or(0);
        }
    }
    if method == "OPTIONS" {
        respond(stream, "204 No Content", "");
        return Ok(());
    }
    if len > MAX_BODY {
        respond(stream, "413 Payload Too Large", "{}");
        return Ok(());
    }
    let mut body = vec![0; len];
    reader.read_exact(&mut body)?;
    let out = if path == "__events" {
        json!({ "ok": events(store, seen) })
    } else {
        let cmd = path.replace("%7C", "|").replace("%3A", ":");
        let args: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
        match handle(store, &cmd, &args) {
            Ok(v) => json!({ "ok": v }),
            Err(e) => json!({ "err": e }),
        }
    };
    respond(stream, "200 OK", &out.to_string());
    Ok(())
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
    let store = Store::new(dir.clone())?;
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
        if let Err(e) = serve(&store, &mut seen, stream) {
            eprintln!("ui_bridge: {e}");
        }
    }
    Ok(())
}
