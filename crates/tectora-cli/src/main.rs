//! `tectora` command-line interface. Output is JSON so AI agents can drive it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use serde_json::json;
use tectora_core::Project;
use tectora_core::quantities::{dacad_line_totals, plan_quantities};
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
    Calc {
        input: PathBuf,
        /// Print only quantities (no geometry).
        #[arg(long)]
        summary: bool,
    },
}

fn load(path: &Path) -> Result<Project> {
    let bytes = fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    let text = String::from_utf8(bytes).context("project file is not UTF-8")?;
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
            if let Err(errors) = project.validate() {
                bail!("invalid project:\n{}", errors.join("\n"));
            }
            let geos = tectora_core::openings::generate_project(&project);
            // DACAD refused the roof: its error is the result, no quantities.
            let dacad_error = geos.iter().find_map(|g| g.error.clone());
            let parts: Vec<_> = project
                .parts
                .iter()
                .zip(geos)
                .map(|(part, geo)| {
                    let q = geo.quantities();
                    if summary {
                        json!({ "name": part.name, "quantities": q })
                    } else {
                        json!({ "name": part.name, "quantities": q, "geometry": geo })
                    }
                })
                .collect();
            let lines_by_type = dacad_line_totals(&project);
            let totals = tectora_core::quantities::roof_totals(&project);
            let mut by_material = serde_json::Map::new();
            for &(m, a) in &totals.area_by_material {
                by_material.insert(project.material_name(m), json!(round2(a)));
            }
            let mut out = json!({
                "ok": true,
                "name": project.name,
                "parts": parts,
                "quantities": {
                    "lines_by_type": lines_by_type,
                    "roof_area": round2(totals.roof_area),
                    "area_by_material": by_material,
                },
            });
            if let Some(e) = dacad_error {
                out["dacad_error"] = json!({
                    "code": e.code,
                    "message_id": e.message_id,
                    "message": e.message,
                });
            }
            Ok(out)
        }
    }
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    if let Command::Mcp { dir } = &cli.command {
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
            println!(
                "{}",
                serde_json::to_string_pretty(&v).expect("serializable")
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            println!("{}", json!({ "ok": false, "error": format!("{e:#}") }));
            ExitCode::FAILURE
        }
    }
}
