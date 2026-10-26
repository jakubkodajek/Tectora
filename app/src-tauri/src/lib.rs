//! Tectora desktop app: Tauri commands over `tectora-core` plus a watcher
//! that tells the UI when project files change on disk (CLI, MCP, AI agents).

mod store;

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use notify::{RecursiveMode, Watcher};
use serde::Serialize;
use serde_json::{Value, json};
use tauri::{Emitter, Manager, State};
use tectora_core::{Project, dad, examples, roof};

use store::{Store, outlines};

struct AppState {
    store: Store,
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

#[tauri::command]
fn duplicate_project(state: State<AppState>, id: String) -> Result<String, String> {
    let mut project = state.store.load(&id)?;
    project.name = format!("{} (kopie)", project.name);
    state.store.save(None, &project)
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
    state.store.save(None, &project)
}

/// Roof geometry and quantities for every part, plus totals.
#[tauri::command]
fn calc(project: Project) -> Value {
    let mut total = roof::RoofQuantities::default();
    let parts: Vec<Value> = project
        .parts
        .iter()
        .map(|part| {
            let geometry = roof::generate(part);
            let q = geometry.quantities();
            total.roof_area += q.roof_area;
            total.ridge_length += q.ridge_length;
            total.hip_length += q.hip_length;
            total.hip_count += q.hip_count;
            total.valley_length += q.valley_length;
            total.valley_count += q.valley_count;
            total.eave_length += q.eave_length;
            total.verge_length += q.verge_length;
            total.abutment_length += q.abutment_length;
            total.ridge_height = total.ridge_height.max(q.ridge_height);
            json!({ "name": part.name, "geometry": geometry, "quantities": q })
        })
        .collect();
    json!({ "parts": parts, "totals": total })
}

fn start_watcher(
    app: &tauri::AppHandle,
    dir: PathBuf,
) -> notify::Result<notify::RecommendedWatcher> {
    let handle = app.clone();
    let (tx, rx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher = notify::recommended_watcher(tx)?;
    watcher.watch(&dir, RecursiveMode::NonRecursive)?;
    std::thread::spawn(move || {
        // Debounce bursts (atomic save = create + rename) into one UI event.
        while let Ok(first) = rx.recv() {
            let mut ids: Vec<String> = Vec::new();
            let mut collect = |res: notify::Result<notify::Event>| {
                if let Ok(event) = res {
                    for p in event.paths {
                        if p.extension().and_then(|e| e.to_str()) == Some(store::EXTENSION) {
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
        }
    });
    Ok(watcher)
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = std::env::var_os("TECTORA_PROJECTS_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    app.path()
                        .document_dir()
                        .unwrap_or_else(|_| PathBuf::from("."))
                        .join("Tectora")
                });
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
                _watcher: Mutex::new(watcher),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            projects_dir,
            list_projects,
            load_project,
            save_project,
            delete_project,
            duplicate_project,
            list_examples,
            create_from_example,
            new_project,
            import_dad,
            calc
        ])
        .run(tauri::generate_context!())
        .expect("error while running Tectora");
}
