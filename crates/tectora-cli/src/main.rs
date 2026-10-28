//! `tectora` command-line interface. Output is JSON so AI agents can drive it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use serde_json::json;
use tectora_core::quantities::{dacad_line_totals, plan_quantities};
use tectora_core::store::Store;
use tectora_core::{Project, dad};

#[derive(Parser)]
#[command(name = "tectora", version, about = "Tectora roof design toolkit")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Convert a DACAD .DAD file into a .tectora project.
    ImportDad {
        input: PathBuf,
        /// Output path (default: input with .tectora extension).
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Print a JSON summary of a .tectora or .DAD file.
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
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .to_string();
    let is_dad = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("dad"));
    if is_dad {
        Ok(dad::import(&bytes, name)?)
    } else {
        let text = String::from_utf8(bytes).context("project file is not UTF-8")?;
        Ok(Project::from_json(&text)?)
    }
}

fn run(cli: Cli) -> Result<serde_json::Value> {
    match cli.command {
        Command::Mcp { .. } => unreachable!("handled in main"),
        Command::ImportDad { input, output } => {
            let project = load(&input)?;
            let out = output.unwrap_or_else(|| input.with_extension("tectora"));
            if out == input {
                bail!("output would overwrite the input file");
            }
            fs::write(&out, project.to_json()?)?;
            Ok(json!({ "ok": true, "output": out, "quantities": plan_quantities(&project) }))
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
            let area = tectora_core::quantities::roof_totals(&project).roof_area;
            let mut out = json!({
                "ok": true,
                "name": project.name,
                "parts": parts,
                "quantities": {
                    "lines_by_type": lines_by_type,
                    "area_by_material": { "roof": round2(area) },
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
