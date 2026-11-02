//! Tectora desktop app: Tauri commands over `tectora-core` plus a watcher
//! that tells the UI when project files change on disk (CLI, MCP, AI agents).

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use notify::{RecursiveMode, Watcher};
use serde::Serialize;
use serde_json::{Value, json};
use tauri::{Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;
use tectora_core::diff::Change;
use tectora_core::examples::{StandardRoof, standard_roof};
use tectora_core::quantities::dacad_line_totals;
use tectora_core::store::{self, LiveStatus, Store, USER_ACTOR, outlines};
use tectora_core::{Project, bundle, examples, roof, snapshot};

struct AppState {
    store: Store,
    /// A job opened from a file (double-click) before the UI asked for it.
    pending_open: Mutex<Option<String>>,
    /// Keeps the file watcher alive for the lifetime of the app.
    _watcher: Mutex<Option<notify::RecommendedWatcher>>,
}

#[derive(Serialize)]
struct ExampleInfo {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    outlines: Vec<Vec<[f64; 2]>>,
    lines: Vec<[[f64; 2]; 2]>,
}

#[tauri::command]
fn projects_dir(state: State<AppState>) -> String {
    state.store.dir.display().to_string()
}

#[tauri::command]
fn list_projects(state: State<AppState>) -> Vec<store::ProjectSummary> {
    state.store.list()
}

#[derive(Serialize)]
struct ActivityItem {
    ts_ms: u64,
    actor: String,
    summary: String,
    changes: Vec<Change>,
}

/// Recorded changes of a project, newest first.
#[tauri::command]
fn project_activity(state: State<AppState>, id: String) -> Vec<ActivityItem> {
    let mut out: Vec<ActivityItem> = state
        .store
        .activity(&id)
        .into_iter()
        .map(|e| ActivityItem {
            ts_ms: e.ts_ms,
            actor: e.actor,
            summary: e.summary,
            changes: e.changes,
        })
        .collect();
    out.reverse();
    out
}

#[tauri::command]
fn revert_last_change(state: State<AppState>, id: String) -> Result<Project, String> {
    state.store.revert_last(&id, USER_ACTOR)
}

/// Undoes the change recorded at `ts_ms` and everything after it.
#[tauri::command]
fn revert_changes(state: State<AppState>, id: String, ts_ms: u64) -> Result<Project, String> {
    state.store.revert_to(&id, ts_ms, USER_ACTOR)
}

#[tauri::command]
fn live_status(state: State<AppState>) -> Option<LiveStatus> {
    state.store.live()
}

/// How to start the MCP server: this executable with the `mcp` argument.
#[tauri::command]
fn mcp_command() -> Value {
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "tectora-app".into());
    json!({ "command": exe, "args": ["mcp"] })
}

#[tauri::command]
fn load_project(state: State<AppState>, id: String) -> Result<Project, String> {
    state.store.load(&id)
}

#[tauri::command]
fn save_project(
    state: State<AppState>,
    id: Option<String>,
    project: Project,
) -> Result<String, String> {
    state.store.save(id, &project)
}

#[tauri::command]
fn delete_project(state: State<AppState>, id: String) -> Result<(), String> {
    state.store.delete(&id)
}

/// A copy is a new variant: draft, no stored result, the same files.
#[tauri::command]
fn duplicate_project(state: State<AppState>, id: String, name: String) -> Result<String, String> {
    let mut project = state.store.load(&id)?;
    project.name = name;
    project.job.status = tectora_core::JobStatus::Draft;
    project.result_snapshot = None;
    let new_id = state.store.save(None, &project)?;
    state.store.copy_attachments(&id, &new_id)?;
    Ok(new_id)
}

#[tauri::command]
fn list_attachments(state: State<AppState>, id: String) -> Vec<store::Attachment> {
    state.store.attachments(&id)
}

#[tauri::command]
fn attach_files(
    state: State<AppState>,
    id: String,
    paths: Vec<String>,
) -> Result<Vec<String>, String> {
    paths
        .iter()
        .map(|p| state.store.attach_file(&id, &PathBuf::from(p)))
        .collect()
}

#[tauri::command]
fn remove_attachment(state: State<AppState>, id: String, name: String) -> Result<(), String> {
    state.store.remove_attachment(&id, &name)
}

/// Opens an attached file in the system's default app.
#[tauri::command]
fn open_attachment(
    app: tauri::AppHandle,
    state: State<AppState>,
    id: String,
    name: String,
) -> Result<(), String> {
    let path = state.store.attachment_path(&id, &name)?;
    app.opener()
        .open_path(path.display().to_string(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn export_job(state: State<AppState>, id: String, path: String) -> Result<(), String> {
    bundle::export(&state.store, &id, &PathBuf::from(path)).map(|_| ())
}

#[tauri::command]
fn import_job(state: State<AppState>, path: String) -> Result<String, String> {
    bundle::open_path(&state.store, &PathBuf::from(path))
}

/// The current result, to store as the job's result.
#[tauri::command]
fn take_snapshot(project: Project) -> snapshot::ResultSnapshot {
    snapshot::take(&project)
}

/// The job opened from a file at start-up, once.
#[tauri::command]
fn take_pending_open(state: State<AppState>) -> Option<String> {
    state.pending_open.lock().ok()?.take()
}

#[tauri::command]
fn create_standard(
    state: State<AppState>,
    name: String,
    kind: StandardRoof,
    length: f64,
    width: f64,
    pitch_deg: f64,
    eave_height: f64,
    overhang: f64,
) -> Result<String, String> {
    let mut project = Project::new(name);
    project.parts.push(standard_roof(
        kind,
        length,
        width,
        pitch_deg,
        eave_height,
        overhang,
    ));
    state.store.save(None, &project)
}

/// A standard roof part for the "standard roof" dialog in the editor.
#[tauri::command]
fn standard_part(
    kind: StandardRoof,
    length: f64,
    width: f64,
    pitch_deg: f64,
    eave_height: f64,
    overhang: f64,
) -> tectora_core::RoofPart {
    standard_roof(kind, length, width, pitch_deg, eave_height, overhang)
}

#[tauri::command]
fn list_examples() -> Vec<ExampleInfo> {
    examples::EXAMPLES
        .iter()
        .map(|e| ExampleInfo {
            id: e.id,
            name: e.name,
            description: e.description,
            outlines: outlines(&e.project()),
            lines: store::roof_lines(&e.project()),
        })
        .collect()
}

#[tauri::command]
fn create_from_example(
    state: State<AppState>,
    example_id: String,
    name: Option<String>,
) -> Result<String, String> {
    let example =
        examples::find(&example_id).ok_or_else(|| format!("unknown example {example_id}"))?;
    let mut project = example.project();
    if let Some(name) = name.filter(|n| !n.trim().is_empty()) {
        project.name = name;
    }
    state.store.save(None, &project)
}

#[tauri::command]
fn new_project(state: State<AppState>, name: String) -> Result<String, String> {
    let mut project = examples::find("gable").expect("built-in example").project();
    project.name = name;
    state.store.save(None, &project)
}

/// Roof geometry and quantities for every part, plus totals.
#[tauri::command]
fn calc(project: Project) -> Value {
    let mut total = roof::RoofQuantities::default();
    let parts: Vec<Value> = project
        .parts
        .iter()
        .zip(tectora_core::openings::generate_project(&project))
        .map(|(part, geometry)| {
            let q = geometry.quantities();
            total.add(&q);
            json!({ "name": part.name, "geometry": geometry, "quantities": q })
        })
        .collect();
    // Lines and faces added by hand (DACAD: Dachlinieneingabe, Neue Dachfläche).
    total.add(&tectora_core::quantities::manual_quantities(&project));
    json!({
        "parts": parts,
        "totals": total,
        "codes": dacad_line_totals(&project),
        "stored_result": snapshot::status(&project),
    })
}

fn start_watcher(
    app: &tauri::AppHandle,
    dir: PathBuf,
) -> notify::Result<notify::RecommendedWatcher> {
    let handle = app.clone();
    let (tx, rx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher = notify::recommended_watcher(tx)?;
    watcher.watch(&dir, RecursiveMode::NonRecursive)?;
    let activity = dir.join(store::ACTIVITY_DIR);
    std::fs::create_dir_all(&activity).map_err(notify::Error::io)?;
    watcher.watch(&activity, RecursiveMode::NonRecursive)?;
    std::thread::spawn(move || {
        // Debounce bursts (atomic save = create + rename) into one UI event.
        while let Ok(first) = rx.recv() {
            let mut ids: Vec<String> = Vec::new();
            let mut live = false;
            let mut collect = |res: notify::Result<notify::Event>| {
                if let Ok(event) = res {
                    for p in event.paths {
                        if p.file_name().and_then(|n| n.to_str()) == Some(store::LIVE_FILE) {
                            live = true;
                        } else if p.extension().and_then(|e| e.to_str()) == Some(store::EXTENSION) {
                            if let Some(id) = p.file_stem().and_then(|s| s.to_str()) {
                                if !ids.iter().any(|i| i == id) {
                                    ids.push(id.to_string());
                                }
                            }
                        }
                    }
                }
            };
            collect(first);
            while let Ok(more) = rx.recv_timeout(Duration::from_millis(150)) {
                collect(more);
            }
            if !ids.is_empty() {
                let _ = handle.emit("projects-changed", ids);
            }
            if live {
                if let Some(status) = handle.state::<AppState>().store.live() {
                    let _ = handle.emit("live-status", status);
                }
            }
        }
    });
    Ok(watcher)
}

/// Opens files given on the command line or by the OS (double-click on a
/// `.tectora` or `.tectorajob` file): each becomes a library job, the last
/// one is shown.
fn open_files(app: &tauri::AppHandle, paths: impl IntoIterator<Item = PathBuf>) {
    let state = app.state::<AppState>();
    let mut last = None;
    for p in paths {
        match bundle::open_path(&state.store, &p) {
            Ok(id) => last = Some(id),
            Err(e) => eprintln!("open {}: {e}", p.display()),
        }
    }
    if let Some(id) = last {
        if let Ok(mut pending) = state.pending_open.lock() {
            *pending = Some(id.clone());
        }
        let _ = app.emit("open-project", id);
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.unminimize();
            let _ = w.set_focus();
        }
    }
}

/// Command-line arguments that name Tectora files.
fn file_args(args: impl IntoIterator<Item = String>) -> Vec<PathBuf> {
    args.into_iter()
        .map(PathBuf::from)
        .filter(|p| {
            let ext = p
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            (ext == store::EXTENSION || ext == bundle::EXTENSION) && p.is_file()
        })
        .collect()
}

pub fn run() {
    let app = tauri::Builder::default()
        // A second launch (double-click while the app runs) hands its files
        // to this instance instead of opening another window.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            open_files(app, file_args(argv.into_iter().skip(1)));
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // Same folder as `tectora mcp`, so AI edits show up here.
            let dir = store::default_dir();
            let store = Store::new(dir.clone())?;
            if store.list().is_empty() {
                // First run: seed the library with a couple of examples.
                for id in ["gable", "l-shape"] {
                    if let Some(ex) = examples::find(id) {
                        let _ = store.save(None, &ex.project());
                    }
                }
            }
            let watcher = start_watcher(app.handle(), dir).ok();
            app.manage(AppState {
                store,
                pending_open: Mutex::new(None),
                _watcher: Mutex::new(watcher),
            });
            open_files(app.handle(), file_args(std::env::args().skip(1)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            projects_dir,
            list_projects,
            load_project,
            save_project,
            delete_project,
            duplicate_project,
            project_activity,
            revert_last_change,
            revert_changes,
            live_status,
            mcp_command,
            create_standard,
            standard_part,
            list_examples,
            create_from_example,
            new_project,
            calc,
            list_attachments,
            attach_files,
            remove_attachment,
            open_attachment,
            export_job,
            import_job,
            take_snapshot,
            take_pending_open
        ])
        .build(tauri::generate_context!())
        .expect("error while building Tectora");
    app.run(|_app, _event| {
        // macOS hands double-clicked files to a running app as an event.
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Opened { urls } = _event {
            open_files(_app, urls.into_iter().filter_map(|u| u.to_file_path().ok()));
        }
    });
}
