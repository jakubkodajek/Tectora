//! Tectora desktop app: Tauri commands over `tectora-core` plus a watcher
//! that tells the UI when project files change on disk (CLI, MCP, AI agents).

use std::path::PathBuf;
use std::sync::{Mutex, RwLock, RwLockReadGuard};
use std::time::Duration;

use notify::{RecursiveMode, Watcher};
use serde::Serialize;
use serde_json::{Value, json};
use tauri::{Emitter, Manager, State};
use tectora_core::diff::Change;
use tectora_core::examples::{StandardRoof, standard_roof};
use tectora_core::quantities::dacad_line_totals;
use tectora_core::settings::{self, Settings};
use tectora_core::store::{self, LiveStatus, Store, USER_ACTOR, outlines};
use tectora_core::{Project, dad, examples, roof};

struct AppState {
    /// Replaced when the projects folder is changed in the settings.
    store: RwLock<Store>,
    /// Keeps the file watcher alive; replaced together with the store.
    watcher: Mutex<Option<notify::RecommendedWatcher>>,
}

impl AppState {
    fn store(&self) -> RwLockReadGuard<'_, Store> {
        self.store.read().unwrap_or_else(|e| e.into_inner())
    }
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
    state.store().dir.display().to_string()
}

#[tauri::command]
fn list_projects(state: State<AppState>) -> Vec<store::ProjectSummary> {
    state.store().list()
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
fn revert_last_change(state: State<AppState>, id: String) -> Result<Project, String> {
    state.store().revert_last(&id, USER_ACTOR)
}

/// Undoes the change recorded at `ts_ms` and everything after it.
#[tauri::command]
fn revert_changes(state: State<AppState>, id: String, ts_ms: u64) -> Result<Project, String> {
    state.store().revert_to(&id, ts_ms, USER_ACTOR)
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
fn load_project(state: State<AppState>, id: String) -> Result<Project, String> {
    state.store().load(&id)
}

#[tauri::command]
fn save_project(
    state: State<AppState>,
    id: Option<String>,
    project: Project,
) -> Result<String, String> {
    state.store().save(id, &project)
}

#[tauri::command]
fn delete_project(state: State<AppState>, id: String) -> Result<(), String> {
    state.store().delete(&id)
}

#[tauri::command]
fn duplicate_project(state: State<AppState>, id: String, name: String) -> Result<String, String> {
    let mut project = state.store().load(&id)?;
    project.name = name;
    state.store().save(None, &project)
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
    state.store().save(None, &project)
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
    state.store().save(None, &project)
}

#[tauri::command]
fn new_project(state: State<AppState>, name: String) -> Result<String, String> {
    let mut project = examples::find("gable").expect("built-in example").project();
    project.name = name;
    state.store().save(None, &project)
}

#[tauri::command]
fn import_dad(state: State<AppState>, path: String) -> Result<String, String> {
    let path = PathBuf::from(path);
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("DACAD")
        .to_string();
    let project = dad::import(&bytes, name).map_err(|e| e.to_string())?;
    state.store().save(None, &project)
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
    json!({ "parts": parts, "totals": total, "codes": dacad_line_totals(&project) })
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
    let watcher = start_watcher(app, dir).ok();
    *state.watcher.lock().unwrap_or_else(|e| e.into_inner()) = watcher;
    let _ = app.emit("projects-changed", Vec::<String>::new());
    Ok(())
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

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Same folder as `tectora mcp`, so AI edits show up here.
            // A chosen folder that is gone (e.g. an unplugged network drive)
            // falls back to Documents/Tectora instead of failing to start.
            let mut dir = store::default_dir();
            let store = match Store::new(dir.clone()) {
                Ok(s) => s,
                Err(_) => {
                    dir = settings::builtin_projects_dir();
                    Store::new(dir.clone())?
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
            });
            let watcher = start_watcher(app.handle(), dir).ok();
            *app.state::<AppState>()
                .watcher
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = watcher;
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
            import_dad,
            calc,
            get_settings,
            update_settings,
            open_folder
        ])
        .run(tauri::generate_context!())
        .expect("error while running Tectora");
}
