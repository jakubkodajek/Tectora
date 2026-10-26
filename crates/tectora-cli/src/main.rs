//! `tectora` command-line interface. Output is JSON so AI agents can drive it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use serde_json::json;
use tectora_core::quantities::plan_quantities;
use tectora_core::{Project, dad, roof};

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
            let parts: Vec<_> = project
                .parts
                .iter()
                .map(|part| {
                    let geo = roof::generate(part);
                    let q = geo.quantities();
                    if summary {
                        json!({ "name": part.name, "quantities": q })
                    } else {
                        json!({ "name": part.name, "quantities": q, "geometry": geo })
                    }
                })
                .collect();
            let lines_by_type = dacad_line_totals(&project);
            let area: f64 = project
                .parts
                .iter()
                .map(|p| roof::generate(p).quantities().roof_area)
                .sum();
            Ok(json!({
                "ok": true,
                "name": project.name,
                "parts": parts,
                "quantities": {
                    "lines_by_type": lines_by_type,
                    "area_by_material": { "roof": round2(area) },
                },
            }))
        }
    }
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Line totals keyed by DACAD's line codes. Verges are not split into
/// left/right yet (`O` = OL + OR + OS) and abutments not by side (`W`).
fn dacad_line_totals(project: &Project) -> serde_json::Map<String, serde_json::Value> {
    let mut totals: std::collections::BTreeMap<&str, f64> = Default::default();
    for part in &project.parts {
        for line in roof::generate(part).lines {
            let code = match line.kind {
                roof::LineKind::Eave => "T",
                roof::LineKind::Ridge => "F",
                roof::LineKind::Hip => "G",
                roof::LineKind::Valley => "K",
                roof::LineKind::Verge => "O",
                roof::LineKind::Abutment => "W",
            };
            *totals.entry(code).or_default() += line.length();
        }
    }
    totals
        .into_iter()
        .map(|(k, v)| (k.to_string(), json!(round2(v))))
        .collect()
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
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
