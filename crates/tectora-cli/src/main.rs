//! `tectora` command-line interface. Output is JSON so AI agents can drive it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use serde_json::json;
use tectora_core::quantities::plan_quantities;
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
    }
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
