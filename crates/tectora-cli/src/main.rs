//! `tectora` command-line interface. Output is JSON so AI agents can drive it.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};
use serde_json::json;
use tectora_core::Project;
use tectora_core::quantities::plan_quantities;
use tectora_core::store::Store;

#[derive(Parser)]
#[command(name = "tectora", version, about = "Tectora roof design toolkit")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the JSON Schema of the .tectora project file.
    Schema,
    /// Write a library job to a .tectorajob bundle (project, files, history, preview).
    ExportJob {
        id: String,
        output: PathBuf,
        /// Project library folder (default: $TECTORA_PROJECTS_DIR or Documents/Tectora).
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Add a .tectorajob bundle or a .tectora file to the library as a new job.
    ImportJob {
        input: PathBuf,
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Print a JSON summary of a .tectora file.
    Info { input: PathBuf },
    /// Run the MCP server on stdin/stdout for AI clients (Claude, ChatGPT, Gemini).
    Mcp {
        /// Project library folder (default: $TECTORA_PROJECTS_DIR or Documents/Tectora).
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Generate the roof and print faces, lines and quantities as JSON.
    ///
    /// Exit status: 0 calculated; 2 calculated, but some buildings have no
    /// roof (each part's `error`) or some buildings or objects have input
    /// problems and are left out (`"partial": true`, the list in
    /// `problems`), as in the app and over MCP; 1 not calculated
    /// (unreadable, out of range, too complex, no building and no
    /// hand-drawn face, no building with a roof and no hand-drawn face, or
    /// the answer could not be written). `roof_state` says which:
    /// `complete`, `partial`, `none` or `no_building`.
    Calc {
        input: PathBuf,
        /// Print quantities, warnings and errors only (no geometry).
        #[arg(long)]
        summary: bool,
    },
}

fn load(path: &Path) -> Result<Project> {
    // A job bundle is a ZIP, not a project file (HUNT-204 item 4). Read
    // through `open_regular`, which never waits on a pipe.
    let zip_magic = || {
        tectora_core::store::open_regular(path, u64::MAX)
            .ok()
            .and_then(|mut f| {
                let mut magic = [0u8; 4];
                std::io::Read::read_exact(&mut f, &mut magic).ok()?;
                Some(magic)
            })
            .is_some_and(|m| m == *b"PK\x03\x04")
    };
    let bundle = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(tectora_core::bundle::EXTENSION))
        || zip_magic();
    if bundle {
        anyhow::bail!(
            "{} is a job bundle (.{}), not a project file: add it to the library with `tectora import-job`",
            path.display(),
            tectora_core::bundle::EXTENSION
        );
    }
    // Bounded, and a pipe or device is refused instead of hanging or
    // reading until the memory is gone (ARCH-204).
    let text = tectora_core::store::read_job_text(path)
        .map_err(|e| anyhow::anyhow!("cannot read {}: {}", path.display(), e.detail))?;
    Ok(Project::from_json(&text)?)
}

fn library(dir: Option<PathBuf>) -> Result<Store> {
    Ok(match dir {
        Some(d) => Store::new(d)?,
        None => Store::open_default()?,
    })
}

fn run(cli: Cli) -> Result<serde_json::Value> {
    match cli.command {
        Command::Mcp { .. } => unreachable!("handled in main"),
        Command::Schema => Ok(tectora_core::model::json_schema()),
        Command::ExportJob { id, output, dir } => {
            let store = library(dir)?;
            // `u.tectora` becomes `u.tectora.tectorajob`, never the job
            // file itself (HUNTIO-002).
            let output = tectora_core::bundle::with_extension(&output);
            let manifest =
                tectora_core::bundle::export(&store, &id, &output).map_err(anyhow::Error::msg)?;
            Ok(json!({ "ok": true, "output": output, "manifest": manifest }))
        }
        Command::ImportJob { input, dir } => {
            let store = library(dir)?;
            let id = tectora_core::bundle::open_path(&store, &input).map_err(anyhow::Error::msg)?;
            Ok(json!({ "ok": true, "id": id }))
        }
        Command::Info { input } => {
            let project = load(&input)?;
            Ok(json!({ "ok": true, "name": project.name, "quantities": plan_quantities(&project) }))
        }
        Command::Calc { input, summary } => {
            let project = load(&input)?;
            // The same limits, work bound and panic guard as the app and
            // the MCP server (`commands::generate`); a building with an
            // input problem is left out and listed, not the whole job
            // refused (HUNTC-004).
            let geos = tectora_core::commands::generate(&project)
                .map_err(|e| anyhow::Error::msg(format!("{}: {}", e.code, e.detail)))?;
            tectora_core::model::guarded(|| calc(&project, geos, summary))
                .map_err(anyhow::Error::msg)
        }
    }
}

/// `tectora calc`: the roof of a project, from one calculation of its roofs.
fn calc(
    project: &Project,
    geos: Vec<tectora_core::roof::RoofGeometry>,
    summary: bool,
) -> serde_json::Value {
    use tectora_core::quantities as q;
    // The calculation refused the roof: its error is the result, no quantities.
    let calc_error = geos.iter().find_map(|g| g.error.clone());
    // Buildings without a roof: the core gave none, or their input left
    // them out (`problems` of the part), as the library card counts them.
    let failed = q::parts_without_roof(&geos, project);
    let coverage = q::RoofCoverage::of(project, failed);
    let lines_by_type = q::dacad_line_totals_of(&geos, project);
    let totals = q::roof_totals_of(&geos, project);
    let area_by_pitch = q::area_by_pitch_of(&geos, project);
    let by_line_material = q::lines_by_material_of(&geos);
    let parts: Vec<_> = project
        .parts
        .iter()
        .zip(geos)
        .map(|(part, geo)| {
            let q = tectora_core::quantities::part_quantities(&geo);
            let mut v = json!({ "name": part.name, "quantities": q });
            // Warnings and errors as MCP `calc` gives them: code and an
            // English sentence, never the calculation's German text.
            if !geo.warnings.is_empty() {
                v["warnings"] = geo.warnings.iter().map(tectora_mcp::warning).collect();
            }
            if let Some(e) = &geo.error {
                v["error"] = tectora_mcp::calc_error(e);
            }
            if !summary {
                v["geometry"] = json!(geo);
                // The geometry's own `error` carries the calculation's
                // text (German): the sentence above replaces it here too
                // (audit ERR-005).
                if let Some(e) = &geo.error {
                    v["geometry"]["error"] = tectora_mcp::calc_error(e);
                }
            }
            v
        })
        .collect();
    // By material number, as MCP `calc`: two materials with one name are
    // two rows (MATH-004), and a default name is English (ERR-004).
    let by_material: Vec<_> = totals
        .area_by_material
        .iter()
        .map(|&(m, a)| json!({ "material": m, "name": project.material_label(m), "area": round2(a) }))
        .collect();
    let mut out = json!({
        "ok": true,
        "name": project.name,
        "parts": parts,
        "quantities": {
            "lines_by_type": lines_by_type,
            "area_by_pitch": area_by_pitch
                .iter()
                .map(|&(p, a)| json!({ "pitch_deg": p, "area": a }))
                .collect::<Vec<_>>(),
            "roof_area": round2(totals.roof_area),
            "area_by_material": by_material,
        },
    });
    if by_line_material.len() > 1 {
        out["quantities"]["lines_by_material"] = by_line_material
                    .into_iter()
                    .map(|(mat, lines)| {
                        json!({ "material": mat, "name": project.material_label(mat), "lines": lines })
                    })
                    .collect();
    }
    if let Some(e) = calc_error {
        out["calc_error"] = tectora_mcp::calc_error(&e);
    }
    let problems = project.problems();
    if !problems.is_empty() || failed > 0 {
        out["partial"] = json!(true);
    }
    if !problems.is_empty() {
        out["problems"] = json!(problems);
    }
    match coverage {
        // Nothing to calculate: the "no building" refusal of a stored
        // result (HUNT-204).
        q::RoofCoverage::NoBuilding => {
            out["ok"] = json!(false);
            out["partial"] = json!(false);
            out["error"] = json!(
                "no_building: the job has no building and no hand-drawn face, so there is no roof"
            );
        }
        // No building has a roof and there is no hand-drawn face: nothing
        // is calculated (round 2 ERR-103). Hand-drawn faces are a roof, so
        // a job with them is at most partial (ERR-202).
        q::RoofCoverage::None => {
            out["ok"] = json!(false);
            out["partial"] = json!(false);
            out["error"] = json!(match &out["calc_error"]["kind"] {
                serde_json::Value::String(k) =>
                    format!("{k}: no building has a roof; see each part's `error`"),
                _ => "no building has a roof; see each part's `error` and `problems`".to_string(),
            });
        }
        q::RoofCoverage::Partial | q::RoofCoverage::Complete => {}
    }
    out["roof_state"] = json!(coverage);
    out
}

fn round2(v: f64) -> f64 {
    // `+ 0.0`: never "-0.0" for a total a hair below zero.
    (v * 100.0).round() / 100.0 + 0.0
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    if let Command::Mcp { dir } = &cli.command {
        // A worker serves one call in the folder it was given; the default
        // library is none of its business (ARCH-001).
        if tectora_mcp::run_worker_if_requested() {
            return ExitCode::SUCCESS;
        }
        let store = match dir {
            Some(d) => Store::new(d.clone()),
            None => Store::open_default(),
        };
        return match store.and_then(tectora_mcp::run_stdio) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("tectora mcp: {e}");
                ExitCode::FAILURE
            }
        };
    }
    match run(cli) {
        Ok(v) => {
            if !print_line(&serde_json::to_string_pretty(&v).expect("serializable"))
                || v["ok"] == json!(false)
            {
                // Not written (disk full, round 2 ERR-106), or no building
                // has a roof (ERR-103).
                ExitCode::FAILURE
            } else if v["partial"] == json!(true) {
                // Calculated, with buildings left out (see `Command::Calc`).
                ExitCode::from(2)
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(e) => {
            print_line(&json!({ "ok": false, "error": format!("{e:#}") }).to_string());
            ExitCode::FAILURE
        }
    }
}

/// Prints the answer; false when it could not be written. A reader that
/// went away (`tectora calc … | head`) is not an error worth a panic
/// (ARCH-008) and counts as written; other write errors go to stderr.
fn print_line(text: &str) -> bool {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    match writeln!(out, "{text}").and_then(|()| out.flush()) {
        Err(e) if e.kind() != std::io::ErrorKind::BrokenPipe => {
            eprintln!("tectora: cannot write the answer: {e}");
            false
        }
        _ => true,
    }
}
