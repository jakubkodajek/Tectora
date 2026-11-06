//! Tectora desktop app: Tauri commands over `tectora-core` plus a watcher
//! that tells the UI when project files change on disk (CLI, MCP, AI agents).

mod error;
mod updates;

use std::path::PathBuf;
use std::sync::{Mutex, RwLock, RwLockReadGuard};
use std::time::Duration;

use notify::{RecursiveMode, Watcher};
use serde::Serialize;
use serde_json::{Value, json};
use tauri::{Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;
use tectora_core::diff::Change;
use tectora_core::examples::{StandardRoof, standard_roof};
use tectora_core::settings::{self, Settings};
use tectora_core::store::{self, BrokenFile, LiveStatus, Store, USER_ACTOR, outlines};
use tectora_core::{Project, bundle, examples, snapshot};

use error::{AppError, guard};

struct AppState {
    /// Replaced when the projects folder is changed in the settings.
    store: RwLock<Store>,
    /// Keeps the file watcher alive; replaced together with the store.
    watcher: Mutex<Option<notify::RecommendedWatcher>>,
    /// A job opened from a file (double-click) before the UI asked for it.
    pending_open: Mutex<Option<String>>,
    /// Why the folder watcher could not start (AI and CLI edits then do not
    /// show up live).
    watcher_error: Mutex<Option<String>>,
}

impl AppState {
    fn store(&self) -> RwLockReadGuard<'_, Store> {
        self.store.read().unwrap_or_else(|e| e.into_inner())
    }
}

type CmdResult<T> = Result<T, AppError>;

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
    state.store().dir.display().to_string()
}

#[derive(Serialize)]
struct Library {
    projects: Vec<store::ProjectSummary>,
    /// Project files in the folder that could not be read.
    broken: Vec<BrokenFile>,
}

/// Runs off the main thread: summarising computes every roof, and the
/// window must stay responsive while it does.
#[tauri::command(async)]
fn list_projects(state: State<AppState>) -> CmdResult<Library> {
    guard(|| {
        let (projects, broken) = state.store().scan()?;
        Ok(Library { projects, broken })
    })
}

/// Problems found at startup that the user should know about.
#[tauri::command]
fn startup_warnings(state: State<AppState>) -> Vec<AppError> {
    state
        .watcher_error
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .map(|d| AppError::new("watcher").detail(d.clone()))
        .collect()
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
        .store()
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
fn revert_last_change(state: State<AppState>, id: String) -> CmdResult<Project> {
    guard(|| Ok(state.store().revert_last(&id, USER_ACTOR)?))
}

/// Undoes the change recorded at `ts_ms` and everything after it.
#[tauri::command]
fn revert_changes(state: State<AppState>, id: String, ts_ms: u64) -> CmdResult<Project> {
    guard(|| Ok(state.store().revert_to(&id, ts_ms, USER_ACTOR)?))
}

#[tauri::command]
fn live_status(state: State<AppState>) -> Option<LiveStatus> {
    state.store().live()
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
fn load_project(state: State<AppState>, id: String) -> CmdResult<Project> {
    guard(|| Ok(state.store().load(&id)?))
}

#[tauri::command]
fn save_project(state: State<AppState>, id: Option<String>, project: Project) -> CmdResult<String> {
    guard(|| Ok(state.store().save(id, &project)?))
}

#[tauri::command]
fn delete_project(state: State<AppState>, id: String) -> CmdResult<()> {
    guard(|| Ok(state.store().delete(&id)?))
}

/// A copy is a new variant: draft, no stored result, the same files.
#[tauri::command]
fn duplicate_project(state: State<AppState>, id: String, name: String) -> CmdResult<String> {
    guard(|| {
        let mut project = state.store().load(&id)?;
        project.name = name;
        project.job.status = tectora_core::JobStatus::Draft;
        project.result_snapshot = None;
        let new_id = state.store().save(None, &project)?;
        state.store().copy_attachments(&id, &new_id)?;
        Ok(new_id)
    })
}

#[tauri::command]
fn list_attachments(state: State<AppState>, id: String) -> Vec<store::Attachment> {
    state.store().attachments(&id)
}

#[tauri::command]
fn attach_files(state: State<AppState>, id: String, paths: Vec<String>) -> CmdResult<Vec<String>> {
    guard(|| {
        Ok(paths
            .iter()
            .map(|p| state.store().attach_file(&id, &PathBuf::from(p)))
            .collect::<Result<_, _>>()?)
    })
}

#[tauri::command]
fn remove_attachment(state: State<AppState>, id: String, name: String) -> CmdResult<()> {
    guard(|| Ok(state.store().remove_attachment(&id, &name)?))
}

/// Opens an attached file in the system's default app.
#[tauri::command]
fn open_attachment(
    app: tauri::AppHandle,
    state: State<AppState>,
    id: String,
    name: String,
) -> CmdResult<()> {
    guard(|| {
        let path = state.store().attachment_path(&id, &name)?;
        app.opener()
            .open_path(path.display().to_string(), None::<&str>)
            .map_err(|e| AppError::new("open_failed").detail(e.to_string()))
    })
}

#[tauri::command]
fn export_job(state: State<AppState>, id: String, path: String) -> CmdResult<()> {
    guard(|| Ok(bundle::export(&state.store(), &id, &PathBuf::from(path)).map(|_| ())?))
}

#[tauri::command]
fn import_job(state: State<AppState>, path: String) -> CmdResult<String> {
    guard(|| Ok(bundle::open_path(&state.store(), &PathBuf::from(path))?))
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
) -> CmdResult<String> {
    guard(|| {
        let mut project = Project::new(name);
        project.parts.push(standard_roof(
            kind,
            length,
            width,
            pitch_deg,
            eave_height,
            overhang,
        ));
        Ok(state.store().save(None, &project)?)
    })
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
) -> CmdResult<tectora_core::RoofPart> {
    guard(|| {
        Ok(standard_roof(
            kind,
            length,
            width,
            pitch_deg,
            eave_height,
            overhang,
        ))
    })
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
) -> CmdResult<String> {
    guard(|| {
        let example = examples::find(&example_id)
            .ok_or_else(|| AppError::new("unknown_example").detail(example_id.clone()))?;
        let mut project = example.project();
        if let Some(name) = name.filter(|n| !n.trim().is_empty()) {
            project.name = name;
        }
        Ok(state.store().save(None, &project)?)
    })
}

#[tauri::command]
fn new_project(state: State<AppState>, name: String) -> CmdResult<String> {
    guard(|| {
        let mut project = examples::find("gable").expect("built-in example").project();
        project.name = name;
        Ok(state.store().save(None, &project)?)
    })
}

/// Roof geometry and quantities for every part, plus totals, what kept
/// parts from being calculated, and the stored result's status.
///
/// Runs off the main thread so a slow roof never freezes the window, and
/// reports out-of-range input (`out_of_range`) or a kernel panic
/// (`internal`) as an error instead of crashing the app (a panic on the
/// main thread aborts it).
#[tauri::command(async)]
fn calc(project: Project) -> CmdResult<Value> {
    guard(|| {
        project
            .check_limits()
            .map_err(|d| AppError::new("out_of_range").detail(d))?;
        let mut v = tectora_core::model::guarded(|| tectora_core::report::calc(&project))
            .map_err(|d| AppError::new("internal").detail(d))?;
        v["stored_result"] =
            serde_json::to_value(snapshot::status(&project)).unwrap_or(Value::Null);
        Ok(v)
    })
}

/// Settings plus the folders in use (see `Settings::describe`).
#[tauri::command]
fn get_settings() -> Value {
    Settings::load().describe()
}

/// Applies a partial update (same keys as the MCP `update_settings` tool),
/// saves it and switches the library when the projects folder changed.
/// Errors are codes the UI translates (`settings.err.<code>`).
#[tauri::command]
fn update_settings(app: tauri::AppHandle, patch: Value) -> Result<Value, String> {
    let next = Settings::load()
        .patched(&patch)
        .map_err(|e| e.code().to_string())?;
    next.save().map_err(|e| e.code().to_string())?;
    switch_library(&app, &next).map_err(|_| "not_writable".to_string())?;
    Ok(next.describe())
}

/// Opens one of the configured folders in Finder / Explorer.
#[tauri::command]
fn open_folder(which: String) -> Result<(), String> {
    let s = Settings::load();
    let dir = match which.as_str() {
        "projects" => s.projects_dir(),
        "drawings" => s.drawings_dir(),
        "settings" => settings::config_dir(),
        _ => return Err("invalid_value".into()),
    };
    std::fs::create_dir_all(&dir).map_err(|_| "not_writable".to_string())?;
    let program = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
    std::process::Command::new(program)
        .arg(&dir)
        .spawn()
        .map(|_| ())
        .map_err(|_| "io".to_string())
}

/// Points the app at the projects folder from `settings` if it differs from
/// the current one: new store, new watcher, and the UI reloads the library.
fn switch_library(app: &tauri::AppHandle, settings: &Settings) -> std::io::Result<()> {
    let dir = settings.projects_dir();
    let state = app.state::<AppState>();
    if state.store().dir == dir {
        return Ok(());
    }
    let next = Store::new(dir.clone())?;
    *state.store.write().unwrap_or_else(|e| e.into_inner()) = next;
    set_watcher(app, dir);
    let _ = app.emit("projects-changed", Vec::<String>::new());
    Ok(())
}

/// Starts watching `dir`, replacing the previous watcher, and records why
/// it failed if it did (shown by `startup_warnings`).
fn set_watcher(app: &tauri::AppHandle, dir: PathBuf) {
    let state = app.state::<AppState>();
    let (watcher, error) = match start_watcher(app, dir) {
        Ok(w) => (Some(w), None),
        Err(e) => {
            eprintln!("tectora: cannot watch the project folder: {e}");
            (None, Some(e.to_string()))
        }
    };
    *state.watcher.lock().unwrap_or_else(|e| e.into_inner()) = watcher;
    *state
        .watcher_error
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = error;
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
    // Settings changed by the MCP server or another app window.
    let config = settings::config_dir();
    if std::fs::create_dir_all(&config).is_ok() && config != dir {
        let _ = watcher.watch(&config, RecursiveMode::NonRecursive);
    }
    std::thread::spawn(move || {
        // Debounce bursts (atomic save = create + rename) into one UI event.
        while let Ok(first) = rx.recv() {
            let mut ids: Vec<String> = Vec::new();
            let mut live = false;
            let mut settings_changed = false;
            let mut collect = |res: notify::Result<notify::Event>| {
                if let Ok(event) = res {
                    for p in event.paths {
                        let name = p.file_name().and_then(|n| n.to_str());
                        if name == Some(store::LIVE_FILE) {
                            live = true;
                        } else if name == Some(settings::SETTINGS_FILE) {
                            settings_changed = true;
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
            if settings_changed {
                let s = Settings::load();
                let _ = handle.emit("settings-changed", s.describe());
                // May replace this watcher; this thread then ends on its own.
                let _ = switch_library(&handle, &s);
            }
            if live {
                if let Some(status) = handle.state::<AppState>().store().live() {
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
        match bundle::open_path(&state.store(), &p) {
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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(updates::Updates::default())
        .setup(|app| {
            // Same folder as `tectora mcp`, so AI edits show up here.
            // A chosen folder that is gone (e.g. an unplugged network drive)
            // falls back to Documents/Tectora instead of failing to start.
            let mut dir = store::default_dir();
            let store = match Store::new(dir.clone()) {
                Ok(s) => s,
                Err(first) => {
                    dir = settings::builtin_projects_dir();
                    match Store::new(dir.clone()) {
                        Ok(s) => s,
                        Err(e) => {
                            use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
                            app.dialog()
                                .message(format!(
                                    "Tectora cannot open its project folder:\n{}\n\n{first}\n{e}\n\n\
                                     Set TECTORA_PROJECTS_DIR to a folder you can write to.",
                                    dir.display()
                                ))
                                .title("Tectora")
                                .kind(MessageDialogKind::Error)
                                .blocking_show();
                            return Err(e.into());
                        }
                    }
                }
            };
            if store.list().is_empty() {
                // First run: seed the library with a couple of examples.
                for id in ["gable", "l-shape"] {
                    if let Some(ex) = examples::find(id) {
                        let _ = store.save(None, &ex.project());
                    }
                }
            }
            app.manage(AppState {
                store: RwLock::new(store),
                watcher: Mutex::new(None),
                pending_open: Mutex::new(None),
                watcher_error: Mutex::new(None),
            });
            set_watcher(app.handle(), dir);
            open_files(app.handle(), file_args(std::env::args().skip(1)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            projects_dir,
            list_projects,
            startup_warnings,
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
            take_pending_open,
            get_settings,
            update_settings,
            open_folder,
            updates::update_check,
            updates::update_install
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
