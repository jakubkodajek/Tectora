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
use tectora_core::examples::{StandardRoof, standard_roof};
use tectora_core::settings::{self, Settings};
use tectora_core::store::{self, BrokenFile, LiveStatus, Store, USER_ACTOR, card_outlines};
use tectora_core::{Project, assistants, bundle, commands, examples, snapshot};

use error::{AppError, guard};

struct AppState {
    /// Replaced when the projects folder is changed in the settings.
    store: RwLock<Store>,
    /// Keeps the file watcher alive; replaced together with the store.
    watcher: Mutex<Option<notify::RecommendedWatcher>>,
    /// A job opened from a file (double-click) before the UI asked for it.
    pending_open: Mutex<Option<String>>,
    /// Files that did not open before the UI asked (start-up arguments);
    /// later ones go to the UI as `open-failed` events.
    open_errors: Mutex<Vec<AppError>>,
    /// Why the folder watcher could not start (AI and CLI edits then do not
    /// show up live).
    watcher_error: Mutex<Option<String>>,
    /// Example jobs seeded on this first start (example id, job id), until
    /// the UI names them in its language (`name_seeded_examples`).
    seeded: Mutex<Vec<(String, String)>>,
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
    /// What in the library's own folders blocks work, with its `kind`
    /// (UX-601, ARCH-701).
    foreign: Vec<store::ForeignItem>,
}

/// Runs off the main thread: summarising computes every roof, and the
/// window must stay responsive while it does.
#[tauri::command(async)]
fn list_projects(state: State<AppState>) -> CmdResult<Library> {
    guard(|| {
        let (projects, broken) = state.store().scan()?;
        let foreign = state.store().foreign_entries();
        Ok(Library {
            projects,
            broken,
            foreign,
        })
    })
}

/// Problems found at startup that the user should know about: a damaged
/// settings file (the defaults, so another jobs folder, are in use, ERR-005)
/// and a folder watcher that could not start.
#[tauri::command]
fn startup_warnings(state: State<AppState>) -> Vec<AppError> {
    let settings = Settings::load_checked()
        .1
        .map(|d| AppError::new("settings_unreadable").detail(d));
    settings
        .into_iter()
        .chain(
            state
                .watcher_error
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .iter()
                .map(|d| AppError::new("watcher").detail(d.clone())),
        )
        .collect()
}

/// Recorded changes of a project, newest first, as the core maps them
/// (`Store::activity_items`: the engine's recalculation with its figures
/// old -> new).
#[tauri::command(async)]
fn project_activity(state: State<AppState>, id: String) -> Vec<store::ActivityItem> {
    state.store().activity_items(&id)
}

/// The engine's recalculation the job's stored result is, if it still is
/// one (`Store::recalculation`): the result card's "recalculated" line and
/// the history's "put the offer's figures back" show for this alone
/// (audit 2026-11-22 ERR-802, ARCH-802).
#[tauri::command(async)]
fn result_recalculation(state: State<AppState>, id: String) -> Option<store::Recalculation> {
    state.store().recalculation(&id)
}

/// Puts the offer's own stored result back in place of the engine's
/// recalculation logged at `entry_ts_ms`, and nothing else
/// (`Store::restore_offer_result`; `revert_stale` when the stored result
/// is no longer that recalculation).
#[tauri::command(async)]
fn restore_offer_result(
    state: State<AppState>,
    id: String,
    entry_ts_ms: u64,
) -> CmdResult<Project> {
    guard(|| {
        Ok(state
            .store()
            .restore_offer_result(&id, entry_ts_ms, USER_ACTOR, None)?)
    })
}

/// Undoes the latest recorded change. `expect_ts`: the entry the button
/// was drawn on; when another change is the latest by now nothing is
/// undone (`revert_stale`, audit 2026-11-22 ERR-801).
#[tauri::command(async)]
fn revert_last_change(
    state: State<AppState>,
    id: String,
    expect_ts: Option<u64>,
) -> CmdResult<Project> {
    guard(|| {
        Ok(state
            .store()
            .revert_last_expecting(&id, USER_ACTOR, None, expect_ts)?)
    })
}

/// Undoes the change recorded at `ts_ms` and everything after it.
#[tauri::command(async)]
fn revert_changes(state: State<AppState>, id: String, ts_ms: u64) -> CmdResult<Project> {
    guard(|| Ok(state.store().revert_to(&id, ts_ms, USER_ACTOR, None)?))
}

/// The AI changes of `id` waiting for review, with the version before them
/// and what they did to the figures. Async like every command below that
/// calculates a roof or copies files: a command without it runs on the
/// window's main thread and freezes the window meanwhile (ARCH-003).
#[tauri::command(async)]
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

/// Off the main thread: a job file can be large and its lock can be held
/// by another process's save (audit 2026-11-22 HUNT-703). The stored
/// result is recalculated before, by `refresh_stored_result`.
#[tauri::command(async)]
fn load_project(state: State<AppState>, id: String) -> CmdResult<Project> {
    guard(|| Ok(state.store().load(&id)?))
}

/// What reading the job left out of it, for the editor to say when it
/// opens the job: a stored result with an impossible time (HUNT-802).
/// Empty when the job cannot be read; `load_project` says why.
#[tauri::command(async)]
fn load_warnings(state: State<AppState>, id: String) -> Vec<AppError> {
    state
        .store()
        .load_warnings(&id)
        .map(|ws| {
            ws.into_iter()
                .map(|w| AppError::new(w.code).detail(w.detail))
                .collect()
        })
        .unwrap_or_default()
}

/// Recalculates the job's stored result where an older engine stored it
/// (Jakub's decision 2026-11-21; the change log keeps the old figures),
/// before the editor loads the job. Returns the change-log entry (its time
/// and summary) when it did. Off the main thread: it calculates the roof
/// under the job's lock (HUNT-703), which it takes only when a
/// recalculation is due and waits for a few seconds at most (ERR-803). An
/// error leaves the stored result as it was and is only logged: the job
/// still opens.
#[tauri::command(async)]
fn refresh_stored_result(state: State<AppState>, id: String) -> Option<store::RefreshedEntry> {
    let r = guard(|| Ok(state.store().refresh_entry(&id)?));
    r.unwrap_or_else(|e| {
        eprintln!(
            "tectora: stored result of {id} not recalculated: {} {}",
            e.code,
            e.detail.as_deref().unwrap_or_default()
        );
        None
    })
}

#[tauri::command]
fn save_project(state: State<AppState>, id: Option<String>, project: Project) -> CmdResult<String> {
    guard(|| {
        let before = id.as_deref().and_then(|i| state.store().load(i).ok());
        commands::check_write(before.as_ref(), &project)?;
        Ok(state.store().save(id, &project)?)
    })
}

/// Saves the editor's edits only if the file still holds `base`, the
/// version the editor last read or wrote (`Store::save_checked`); otherwise
/// answers `conflict` with the current file, for the editor to merge its
/// edits onto and save again, so an AI save in between is never lost.
#[tauri::command(async)]
fn save_project_checked(
    state: State<AppState>,
    id: String,
    project: Project,
    base: Project,
) -> CmdResult<tectora_core::store::CheckedSave> {
    guard(|| {
        commands::check_write(Some(&base), &project)?;
        Ok(state.store().save_checked(&id, &project, &base)?)
    })
}

/// The project as the store writes and reads it (defaults, nulls and empty
/// lists dropped), without touching the file. The editor compares file
/// changes with this to tell its own saves from external edits.
#[tauri::command]
fn normalize_project(project: Project) -> Project {
    project
}

/// Moves the job to the library's trash (`Store::delete`); returns the
/// trash entry, which `restore_deleted` (the library's Trash, the toast's
/// Restore) and the MCP `restore_project` tool bring back.
#[tauri::command(async)]
fn delete_project(
    state: State<AppState>,
    id: String,
) -> CmdResult<tectora_core::store::TrashEntry> {
    guard(|| {
        Ok(state
            .store()
            .delete(&id, USER_ACTOR, "Deleted in the app")?)
    })
}

/// Jobs and files in the library's trash, newest first (`Store::trash`, the
/// MCP tool `list_deleted`).
#[tauri::command(async)]
fn list_deleted(state: State<AppState>) -> CmdResult<Vec<tectora_core::store::TrashEntry>> {
    guard(|| Ok(state.store().trash()))
}

/// Brings trash entry `entry` back (`Store::restore`, the MCP tool
/// `restore_project`); returns the job's id, a new one when the old id was
/// taken meanwhile. An entry that is not in the trash is `not_found`.
#[tauri::command(async)]
fn restore_deleted(state: State<AppState>, entry: String) -> CmdResult<String> {
    guard(|| Ok(state.store().restore(&entry)?))
}

/// Gives the example jobs seeded on this first start the names `names`
/// (example id to name, in the UI's language), so a German first start does
/// not list Czech names (UX-012). Only jobs seeded in this run whose name
/// is still the built-in one are renamed; the list is used once.
#[tauri::command(async)]
fn name_seeded_examples(
    state: State<AppState>,
    names: std::collections::HashMap<String, String>,
) -> CmdResult<usize> {
    let seeded = std::mem::take(&mut *state.seeded.lock().unwrap_or_else(|e| e.into_inner()));
    guard(|| Ok(examples::rename_seeded(&state.store(), &seeded, &names)?))
}

/// A copy is a new variant: draft, no stored result, the same files. With
/// `job` it starts a new job for another customer from a past roof.
#[tauri::command(async)]
fn duplicate_project(
    state: State<AppState>,
    id: String,
    name: String,
    job: Option<tectora_core::JobInfo>,
    with_files: Option<bool>,
) -> CmdResult<String> {
    guard(|| {
        tectora_core::edit::check_job_name(&name)
            .map_err(|d| AppError::new("out_of_range").detail(d))?;
        let mut project = state.store().load(&id)?;
        tectora_core::library::prepare_copy(&mut project, &name, job);
        // A refused copy leaves no new job (ARCH-602).
        Ok(state
            .store()
            .copy_job(&id, &project, with_files.unwrap_or(true), None)?)
    })
}

#[tauri::command]
fn list_attachments(state: State<AppState>, id: String) -> Vec<store::Attachment> {
    state.store().attachments(&id)
}

#[tauri::command(async)]
fn attach_files(state: State<AppState>, id: String, paths: Vec<String>) -> CmdResult<Vec<String>> {
    guard(|| {
        Ok(paths
            .iter()
            .map(|p| state.store().attach_file(&id, &PathBuf::from(p)))
            .collect::<Result<_, _>>()?)
    })
}

#[tauri::command(async)]
fn remove_attachment(state: State<AppState>, id: String, name: String) -> CmdResult<()> {
    guard(|| {
        state
            .store()
            .remove_attachment(&id, &name, USER_ACTOR, "Removed in the app")?;
        Ok(())
    })
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
    // A name typed without the ending gets it: never the job file itself
    // or a file of another type (HUNTIO-002).
    let dest = bundle::with_extension(&PathBuf::from(path));
    guard(|| Ok(bundle::export(&state.store(), &id, &dest).map(|_| ())?))
}

/// The text list for the price sheet and the plan sheet (SVG) of the open
/// project, for the export dialog.
/// `line_names` (code → name in the UI language) label the sheet's line
/// codes; `summary` is the list as numbers for the dialog to show
/// translated, while `list` stays the price sheet's text.
#[tauri::command(async)]
fn export_preview(
    project: Project,
    title: String,
    stamp: String,
    line_names: Option<std::collections::BTreeMap<String, String>>,
) -> CmdResult<Value> {
    guard(|| {
        let names = line_names.unwrap_or_default();
        Ok(commands::export_preview(&project, title, stamp, &names)?)
    })
}

/// Writes the plan sheet to `path` (from the save dialog) as PDF or SVG,
/// by its extension.
#[tauri::command(async)]
fn export_plan_file(
    project: Project,
    stamp: String,
    path: String,
    line_names: Option<std::collections::BTreeMap<String, String>>,
) -> CmdResult<()> {
    guard(|| {
        let path = PathBuf::from(path);
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        let names = line_names.unwrap_or_default();
        let bytes = tectora_core::export::checked_sheet_file_named(&project, &stamp, &ext, &names)
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

#[tauri::command(async)]
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

/// Files that did not open, each once: the UI takes them when it starts
/// and on every `open-failed` event, so none is lost or shown twice.
#[tauri::command]
fn take_open_errors(state: State<AppState>) -> Vec<AppError> {
    state
        .open_errors
        .lock()
        .map(|mut q| std::mem::take(&mut *q))
        .unwrap_or_default()
}

// Tauri passes each command argument separately from the UI's object.
#[allow(clippy::too_many_arguments)]
#[tauri::command(async)]
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
        commands::check_standard_roof(
            Some(&name),
            length,
            width,
            pitch_deg,
            eave_height,
            overhang,
        )?;
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
        commands::check_standard_roof(None, length, width, pitch_deg, eave_height, overhang)?;
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

#[tauri::command(async)]
fn list_examples() -> Vec<ExampleInfo> {
    examples::EXAMPLES
        .iter()
        .map(|e| ExampleInfo {
            id: e.id,
            name: e.name,
            description: e.description,
            outlines: card_outlines(&e.project()),
            lines: store::roof_lines(&e.project()),
        })
        .collect()
}

#[tauri::command(async)]
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

#[tauri::command(async)]
fn new_project(state: State<AppState>, name: String) -> CmdResult<String> {
    guard(|| {
        tectora_core::edit::check_job_name(&name)
            .map_err(|d| AppError::new("out_of_range").detail(d))?;
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

/// The first roof window larger than what is left of its face
/// (`tectora_core::edit::check_windows`), or null; the editor refuses such
/// a window, as MCP `set_openings` does.
#[tauri::command(async)]
fn check_windows(project: Project) -> CmdResult<Option<tectora_core::edit::OversizeWindow>> {
    guard(|| {
        commands::check_input(&project)?;
        tectora_core::model::guarded(|| tectora_core::edit::check_windows(&project).err())
            .map_err(|d| AppError::new("internal").detail(d))
    })
}

/// The roof under dormer `index` for the dormer dialog
/// (`tectora_core::commands::dormer_host`), or null off the roof.
#[tauri::command(async)]
fn dormer_host(project: Project, index: usize) -> CmdResult<Value> {
    guard(|| Ok(commands::dormer_host(&project, index)?))
}

/// Whether dormer `index` can be stored as it is
/// (`tectora_core::commands::check_dormer`): the dormer bar and dialog ask
/// before they store a pitch or eave height (MATH-001, MATH-003).
#[tauri::command(async)]
fn check_dormer(project: Project, index: usize) -> CmdResult<()> {
    guard(|| Ok(commands::check_dormer(&project, index)?))
}

/// What the calculation dialog needs to know about wall `edge` of part `part`
/// (`tectora_core::commands::roof_helper_context`): the facing wall, span,
/// parallel walls and, when asked, the roof's corner points.
#[tauri::command(async)]
fn roof_helper_context(
    project: Project,
    part: usize,
    edge: usize,
    with_points: bool,
) -> CmdResult<Value> {
    guard(|| {
        Ok(commands::roof_helper_context(
            &project,
            part,
            edge,
            with_points,
        )?)
    })
}

/// The calculation dialog's live figures for the values typed so far
/// (`tectora_core::commands::roof_helper_preview`, the MCP tool roof_helper
/// without `apply`). Nothing is changed; no solution is `refused`.
#[tauri::command(async)]
fn roof_helper_preview(
    project: Project,
    part: usize,
    edge: usize,
    request: tectora_core::edit::helper::HelperRequest,
) -> CmdResult<Value> {
    guard(|| {
        Ok(commands::roof_helper_preview(
            &project, part, edge, &request,
        )?)
    })
}

/// The calculation dialog's OK: the project with the helper's values applied
/// (`tectora_core::commands::roof_helper_apply`, the MCP tool roof_helper
/// with `apply`).
#[tauri::command(async)]
fn roof_helper_apply(
    project: Project,
    part: usize,
    edge: usize,
    request: tectora_core::edit::helper::HelperRequest,
) -> CmdResult<Project> {
    guard(|| Ok(commands::roof_helper_apply(&project, part, edge, &request)?))
}

/// The round-roof dialog's arc for the typed values and the values it gives
/// both walls (`tectora_core::commands::round_arc`); out of range is `refused`.
#[tauri::command]
fn round_arc(input: tectora_core::edit::round::ArcInput) -> CmdResult<Value> {
    guard(|| Ok(commands::round_arc(&input)?))
}

/// Shape operations of the object bar and the plan tools: reshape a chimney or
/// opening to a new box, a chimney from a rectangle, the area of a hand-drawn
/// face (`tectora_core::commands::item_geometry`, the operations behind the
/// MCP tools `update_opening` and `set_openings`).
#[tauri::command]
fn item_geometry(request: commands::GeometryRequest) -> CmdResult<Value> {
    guard(|| Ok(commands::item_geometry(&request)?))
}

/// The transform dialog's OK and "Duplicate part": part `part` moved,
/// rotated, mirrored or copied with what lies on it
/// (`tectora_core::commands::transform_part`, the MCP tool transform_part).
/// Returns `{project, parts}`.
#[tauri::command(async)]
fn transform_part(
    project: Project,
    part: usize,
    map: tectora_core::edit::transform::PlanMap,
    copies: u32,
) -> CmdResult<Value> {
    guard(|| Ok(commands::transform_part(&project, part, map, copies)?))
}

/// The transform dialog's wall shift (`tectora_core::commands::offset_wall`).
#[tauri::command(async)]
fn offset_wall(project: Project, part: usize, edge: usize, dist: f64) -> CmdResult<Project> {
    guard(|| Ok(commands::offset_wall(&project, part, edge, dist)?))
}

/// The editor's "Make it a building": hand-drawn face `index` replaced by a
/// part of the same outline (`tectora_core::edit::face_to_part`, the MCP
/// tool's operation).
#[tauri::command]
fn face_to_part(
    mut project: Project,
    index: usize,
    name: String,
    pitch_deg: f64,
    eave_height: f64,
    overhang: f64,
) -> CmdResult<Project> {
    tectora_core::edit::face_to_part(&mut project, index, &name, pitch_deg, eave_height, overhang)
        .map_err(|d| AppError::new("refused").detail(d))?;
    Ok(project)
}

/// Where a dormer, chimney, roof window or opening sits on its roof face
/// (`tectora_core::edit::position::placement`): the dormer window asks it
/// for the dormer being edited. Calculates the roof, so guarded like `calc`.
#[tauri::command(async)]
fn item_placement(
    project: Project,
    kind: tectora_core::edit::transform::ItemKind,
    index: usize,
) -> CmdResult<Option<tectora_core::edit::position::Placement>> {
    guard(|| {
        project
            .check_input()
            .map_err(|e| AppError::new(e.code).detail(e.message))?;
        let it = tectora_core::edit::transform::ItemRef { kind, index };
        Ok(tectora_core::edit::position::placement(&project, it))
    })
}

/// The editor's dimension fields of a dormer, chimney, roof window or
/// opening: moves it to `from_eave` (along the slope) and `from_left` or
/// `from_right` on its roof face (`tectora_core::edit::position::place`, the
/// MCP tool place_item). Calculates the roof, so off the main thread and
/// guarded like `calc`.
#[tauri::command(async)]
fn place_item(
    mut project: Project,
    kind: tectora_core::edit::transform::ItemKind,
    index: usize,
    from_eave: Option<f64>,
    from_left: Option<f64>,
    from_right: Option<f64>,
) -> CmdResult<Project> {
    guard(|| {
        project
            .check_input()
            .map_err(|e| AppError::new(e.code).detail(e.message))?;
        let it = tectora_core::edit::transform::ItemRef { kind, index };
        tectora_core::edit::position::place(&mut project, it, from_eave, from_left, from_right)
            .map_err(|d| AppError::new("refused").detail(d))?;
        Ok(project)
    })
}

/// The editor's "Building height": part `part` at `height` (top of the
/// walls), its roof (eaves, breaks, half-hips) and the objects on it moved
/// by the change (`tectora_core::edit::height::set_part_height`, the MCP
/// tool's operation).
#[tauri::command]
fn set_part_height(mut project: Project, part: usize, height: f64) -> CmdResult<Project> {
    guard(|| {
        project
            .check_input()
            .map_err(|e| AppError::new(e.code).detail(e.message))?;
        tectora_core::edit::height::set_part_height(&mut project, part, height)
            .map_err(|d| AppError::new("refused").detail(d))?;
        Ok(project)
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
    // Read, changed and written under the settings lock: an AI client's
    // change saved at the same moment is not lost (TEST-307).
    let (_, next) = Settings::update(|s| s.patched(&patch)).map_err(|e| e.code().to_string())?;
    switch_library(&app, &next).map_err(|_| "not_writable".to_string())?;
    Ok(next.describe())
}

/// The user's answer to a folder change an AI client asked for
/// (`Settings::folder_request`, ARCH-052): `allow` moves the folders.
#[tauri::command]
fn answer_folder_request(
    app: tauri::AppHandle,
    changes: serde_json::Map<String, Value>,
    allow: bool,
) -> Result<Value, String> {
    // `changes` is what the dialog showed; a request replaced meanwhile is
    // not answered (ARCH-005).
    let (_, next) = Settings::update(|s| s.answer_folder_request(&changes, allow))
        .map_err(|e| e.code().to_string())?;
    switch_library(&app, &next).map_err(|_| "not_writable".to_string())?;
    Ok(next.describe())
}

/// The user's answer to a file outside the job folders an AI client asked
/// to read (`Settings::file_request`, ARCH-002): `allow` lets AI clients
/// read that one file.
#[tauri::command]
fn answer_file_request(path: std::path::PathBuf, allow: bool) -> Result<Value, String> {
    // `path` is what the dialog showed; a request replaced meanwhile is not
    // answered (ARCH-005).
    let (_, next) = Settings::update(|s| Ok(s.answer_file_request(&path, allow)))
        .map_err(|e| e.code().to_string())?;
    Ok(next.describe())
}

/// The user's answer to a settings change an AI client made
/// (`Settings::ai_changes`, UX-003): `revert` puts the previous values back,
/// otherwise it is accepted. A change answered meanwhile is left alone.
#[tauri::command]
fn answer_ai_change(id: u64, revert: bool) -> Result<Value, String> {
    let (_, next) =
        Settings::update(|s| s.answer_ai_change(id, revert)).map_err(|e| e.code().to_string())?;
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
                        } else if p.extension().and_then(|e| e.to_str()) == Some(store::EXTENSION)
                            && let Some(id) = p.file_stem().and_then(|s| s.to_str())
                            && !ids.iter().any(|i| i == id)
                        {
                            ids.push(id.to_string());
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
            if live && let Some(status) = handle.state::<AppState>().store().live() {
                let _ = handle.emit("live-status", status);
            }
        }
    });
    Ok(watcher)
}

/// Opens files given on the command line or by the OS (double-click on a
/// `.tectora` or `.tectorajob` file): each becomes a library job, the last
/// one is shown.
///
/// On its own thread: importing a received bundle can take a while, and
/// this runs from the event loop (macOS file open, a second launch), which
/// must never wait for it (audit 2026-11-22 HUNT-401).
fn open_files(app: &tauri::AppHandle, paths: impl IntoIterator<Item = PathBuf>) {
    let paths: Vec<PathBuf> = paths.into_iter().collect();
    if paths.is_empty() {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || open_files_now(&app, paths));
}

/// [`open_files`] on the calling thread.
fn open_files_now(app: &tauri::AppHandle, paths: Vec<PathBuf>) {
    let state = app.state::<AppState>();
    let opened = bundle::open_paths(&state.store(), paths);
    // A file that does not open is told to the user like a failed import
    // (ERR-006), not only written to stderr, which a GUI app has none of.
    for (p, e) in opened.failed {
        eprintln!("open {}: {e}", p.display());
        let err = AppError::from(e);
        if let Ok(mut queued) = state.open_errors.lock() {
            queued.push(err.clone());
        }
        let _ = app.emit("open-failed", err);
    }
    if let Some(id) = opened.last {
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
            // The jobs seeded now, by example id: the UI names them in its
            // language once it knows it (`name_seeded_examples`, UX-012).
            let mut seeded: Vec<(String, String)> = Vec::new();
            // First run: seed the library with a couple of examples. Not
            // when the settings are damaged: the folder is then the default
            // one, not the user's (ERR-005).
            if store.list().is_empty() && Settings::load_checked().1.is_none() {
                for id in ["gable", "l-shape"] {
                    if let Some(ex) = examples::find(id)
                        && let Ok(job) = store.save(None, &ex.project())
                    {
                        seeded.push((id.to_string(), job));
                    }
                }
            }
            app.manage(AppState {
                store: RwLock::new(store),
                watcher: Mutex::new(None),
                pending_open: Mutex::new(None),
                open_errors: Mutex::new(Vec::new()),
                watcher_error: Mutex::new(None),
                seeded: Mutex::new(seeded),
            });
            set_watcher(app.handle(), dir);
            open_files(app.handle(), file_args(std::env::args().skip(1)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            projects_dir,
            list_projects,
            startup_warnings,
            take_open_errors,
            load_project,
            load_warnings,
            refresh_stored_result,
            save_project,
            save_project_checked,
            normalize_project,
            delete_project,
            list_deleted,
            restore_deleted,
            name_seeded_examples,
            duplicate_project,
            project_activity,
            result_recalculation,
            restore_offer_result,
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
            check_windows,
            dormer_host,
            check_dormer,
            roof_helper_context,
            roof_helper_preview,
            roof_helper_apply,
            round_arc,
            item_geometry,
            transform_part,
            offset_wall,
            face_to_part,
            set_part_height,
            place_item,
            item_placement,
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
            answer_folder_request,
            answer_file_request,
            answer_ai_change,
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

#[cfg(test)]
mod tests {
    /// Commands that calculate a roof (twice for `review_status`) or read and
    /// copy whole files must not run on the window's main thread (ARCH-003).
    #[test]
    fn slow_commands_run_off_the_main_thread() {
        let src = include_str!("lib.rs");
        for name in [
            "list_projects",
            "load_project",
            "refresh_stored_result",
            "project_activity",
            "result_recalculation",
            "restore_offer_result",
            "review_status",
            "revert_last_change",
            "revert_changes",
            "duplicate_project",
            "delete_project",
            "attach_files",
            "remove_attachment",
            "import_job",
            "export_job",
            "list_examples",
            "calc",
            "calc_breakdown",
            "take_snapshot",
            "dormer_host",
            "check_dormer",
        ] {
            assert!(
                src.contains(&format!("#[tauri::command(async)]\nfn {name}(")),
                "`{name}` must be #[tauri::command(async)]"
            );
        }
    }

    /// Double-click open and start-up arguments (TEST-008): Tectora files
    /// in any case of their extension are taken in order; folders that look
    /// like one, missing files, other types and flags are not.
    #[test]
    fn file_args_take_tectora_files_in_any_case_and_nothing_else() {
        let dir = std::env::temp_dir().join(format!(
            "tectora-file-args-{}-{}",
            std::process::id(),
            tectora_core::store::now_ms()
        ));
        std::fs::create_dir_all(dir.join("folder.tectora")).unwrap();
        let file = |name: &str| {
            let p = dir.join(name);
            std::fs::write(&p, "{}").unwrap();
            p.display().to_string()
        };
        let a = file("a.tectora");
        let b = file("B.TECTORA");
        let c = file("Zakázka Nový Dům.TectoraJob");
        let txt = file("notes.txt");
        let no_ext = file("tectora");
        let args = vec![
            "/Applications/Tectora.app/Contents/MacOS/Tectora".to_string(),
            "--flag".to_string(),
            a.clone(),
            dir.join("folder.tectora").display().to_string(),
            b.clone(),
            dir.join("missing.tectora").display().to_string(),
            txt,
            no_ext,
            c.clone(),
        ];
        let got = super::file_args(args);
        let want: Vec<std::path::PathBuf> =
            [a, b, c].iter().map(std::path::PathBuf::from).collect();
        assert_eq!(got, want);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
