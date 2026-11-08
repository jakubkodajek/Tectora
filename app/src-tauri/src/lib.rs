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
use tectora_core::{Project, assistants, bundle, commands, examples, snapshot};

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

/// The AI changes of `id` waiting for review, with the version before them
/// and what they did to the figures.
#[tauri::command]
fn review_status(state: State<AppState>, id: String) -> CmdResult<Option<store::ReviewStatus>> {
    guard(|| Ok(state.store().review_status(&id)?))
}

/// Accepts every change of `id` recorded at or before `ts_ms`.
#[tauri::command]
fn accept_changes(state: State<AppState>, id: String, ts_ms: u64) -> CmdResult<u64> {
    guard(|| Ok(state.store().mark_reviewed(&id, ts_ms)?))
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
    json!({ "command": exe, "args": [assistants::MCP_ARG] })
}

/// The command AI apps start Tectora's MCP server with.
fn server_command() -> assistants::ServerCommand {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("tectora-app"));
    assistants::ServerCommand::new(&exe)
}

/// The AI apps Tectora can connect itself to, and whether each is connected.
#[tauri::command]
fn ai_assistants() -> Vec<assistants::AssistantInfo> {
    assistants::list(&server_command())
}

/// Adds Tectora's MCP server to an AI app's configuration (the Connect AI
/// dialog's one-click Connect), keeping everything else in the file.
#[tauri::command]
fn connect_ai_assistant(id: assistants::Assistant) -> CmdResult<assistants::ConnectChange> {
    guard(|| {
        id.connect(&server_command())
            .map_err(|e| AppError::new(e.code()).detail(e.to_string()))
    })
}

#[tauri::command]
fn load_project(state: State<AppState>, id: String) -> CmdResult<Project> {
    guard(|| Ok(state.store().load(&id)?))
}

#[tauri::command]
fn save_project(state: State<AppState>, id: Option<String>, project: Project) -> CmdResult<String> {
    guard(|| Ok(state.store().save(id, &project)?))
}

/// The project as the store writes and reads it (defaults, nulls and empty
/// lists dropped), without touching the file. The editor compares file
/// changes with this to tell its own saves from external edits.
#[tauri::command]
fn normalize_project(project: Project) -> Project {
    project
}

#[tauri::command]
fn delete_project(state: State<AppState>, id: String) -> CmdResult<()> {
    guard(|| Ok(state.store().delete(&id)?))
}

/// A copy is a new variant: draft, no stored result, the same files. With
/// `job` it starts a new job for another customer from a past roof.
#[tauri::command]
fn duplicate_project(
    state: State<AppState>,
    id: String,
    name: String,
    job: Option<tectora_core::JobInfo>,
    with_files: Option<bool>,
) -> CmdResult<String> {
    guard(|| {
        let mut project = state.store().load(&id)?;
        tectora_core::library::prepare_copy(&mut project, &name, job);
        let new_id = state.store().save(None, &project)?;
        if with_files.unwrap_or(true) {
            state.store().copy_attachments(&id, &new_id)?;
        }
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

/// Opens an attached file in the system's default app; returns false when
/// the file's type is not one that opens directly (`Store::opens_directly`)
/// and its folder was shown instead, so a program that came in a received
/// bundle is never run by one click.
#[tauri::command]
fn open_attachment(
    app: tauri::AppHandle,
    state: State<AppState>,
    id: String,
    name: String,
) -> CmdResult<bool> {
    guard(|| {
        let path = state.store().attachment_path(&id, &name)?;
        if !Store::opens_directly(&name) {
            let dir = path.parent().unwrap_or(&path).to_path_buf();
            show_folder(&dir).map_err(|e| AppError::new("open_failed").detail(e))?;
            return Ok(false);
        }
        app.opener()
            .open_path(path.display().to_string(), None::<&str>)
            .map_err(|e| AppError::new("open_failed").detail(e.to_string()))?;
        Ok(true)
    })
}

/// Shows a folder in Finder / Explorer (the file manager on Linux).
fn show_folder(dir: &std::path::Path) -> Result<(), String> {
    let program = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
    std::process::Command::new(program)
        .arg(dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Async: the preview in the bundle runs the roof kernel.
#[tauri::command(async)]
fn export_job(state: State<AppState>, id: String, path: String) -> CmdResult<()> {
    guard(|| Ok(bundle::export(&state.store(), &id, &PathBuf::from(path)).map(|_| ())?))
}

/// The text list for the price sheet and the plan sheet (SVG) of the open
/// project, for the export dialog.
#[tauri::command(async)]
fn export_preview(project: Project, title: String, stamp: String) -> CmdResult<Value> {
    guard(|| {
        let opts = tectora_core::export::ListOptions::new(title, stamp);
        let (list, svg) = tectora_core::export::checked(&project, &opts)
            .map_err(|d| AppError::new("out_of_range").detail(d))?;
        Ok(serde_json::json!({ "list": list, "svg": svg }))
    })
}

/// Writes the plan sheet to `path` (from the save dialog) as PDF or SVG,
/// by its extension.
#[tauri::command(async)]
fn export_plan_file(project: Project, stamp: String, path: String) -> CmdResult<()> {
    guard(|| {
        let path = PathBuf::from(path);
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        let bytes = tectora_core::export::checked_sheet_file(&project, &stamp, &ext)
            .map_err(|d| AppError::new("out_of_range").detail(d))?;
        std::fs::write(&path, bytes)
            .map_err(|e| AppError::new("io").detail(format!("{}: {e}", path.display())))?;
        Ok(())
    })
}

/// Writes the PNG the export dialog rendered to `path` (from the save dialog).
#[tauri::command(async)]
fn save_png(path: String, bytes: Vec<u8>) -> CmdResult<()> {
    guard(|| {
        let path = PathBuf::from(path);
        let png = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("png"));
        if !png || !bytes.starts_with(b"\x89PNG") || bytes.len() > 64 << 20 {
            return Err(AppError::new("invalid_input").detail("not a PNG image"));
        }
        std::fs::write(&path, bytes)
            .map_err(|e| AppError::new("io").detail(format!("{}: {e}", path.display())))?;
        Ok(())
    })
}

#[tauri::command]
fn import_job(state: State<AppState>, path: String) -> CmdResult<String> {
    guard(|| Ok(bundle::open_path(&state.store(), &PathBuf::from(path))?))
}

/// The current result, to store as the job's result. Async and guarded like
/// `calc`: it runs the whole roof kernel (ERR-008).
#[tauri::command(async)]
fn take_snapshot(project: Project) -> CmdResult<snapshot::ResultSnapshot> {
    guard(|| Ok(commands::take_snapshot(&project)?))
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
/// main thread aborts it). The body is `tectora_core::commands::calc`,
/// shared with the browser preview and tested there.
#[tauri::command(async)]
fn calc(project: Project) -> CmdResult<Value> {
    guard(|| Ok(commands::calc(&project)?))
}

/// How every face area and line length was calculated, step by step
/// (`tectora_core::breakdown`), with the same guards as `calc`.
#[tauri::command(async)]
fn calc_breakdown(project: Project) -> CmdResult<Value> {
    guard(|| Ok(commands::calc_breakdown(&project)?))
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
    show_folder(&dir).map_err(|_| "io".to_string())
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

/// The id of Tectora's own Quit item (macOS, see [`app_menu`]).
const QUIT_ID: &str = "tectora-quit";

/// Tauri's default menu, except that on macOS Quit (Cmd+Q) is Tectora's own
/// item: the predefined one ends the app through `terminate:` without any
/// close request, so the window's guard for unsaved edits never ran
/// (ERR-022; traced in tao 0.37, which sends only `LoopDestroyed`).
fn app_menu<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> tauri::Result<tauri::menu::Menu<R>> {
    let menu = tauri::menu::Menu::default(app)?;
    #[cfg(target_os = "macos")]
    let items = menu.items()?;
    #[cfg(target_os = "macos")]
    if let Some(app_menu) = items.first().and_then(|m| m.as_submenu()) {
        // The app submenu ends with the predefined Quit.
        let n = app_menu.items()?.len();
        if n > 0 {
            app_menu.remove_at(n - 1)?;
            let name = app.package_info().name.clone();
            app_menu.append(&tauri::menu::MenuItem::with_id(
                app,
                QUIT_ID,
                format!("Quit {name}"),
                true,
                Some("CmdOrCtrl+Q"),
            )?)?;
        }
    }
    Ok(menu)
}

/// Quit as a window close: the window's close-requested guard saves or asks
/// first, then the last closed window ends the app.
fn quit_through_close_guard<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let windows = app.webview_windows();
    if windows.is_empty() {
        app.exit(0);
    }
    for w in windows.values() {
        let _ = w.close();
    }
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
        .menu(app_menu)
        .on_menu_event(|app, event| {
            if event.id() == QUIT_ID {
                quit_through_close_guard(app);
            }
        })
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
            normalize_project,
            delete_project,
            duplicate_project,
            project_activity,
            revert_last_change,
            revert_changes,
            review_status,
            accept_changes,
            live_status,
            mcp_command,
            ai_assistants,
            connect_ai_assistant,
            create_standard,
            standard_part,
            list_examples,
            create_from_example,
            new_project,
            calc,
            calc_breakdown,
            list_attachments,
            attach_files,
            remove_attachment,
            open_attachment,
            export_job,
            export_preview,
            export_plan_file,
            save_png,
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
