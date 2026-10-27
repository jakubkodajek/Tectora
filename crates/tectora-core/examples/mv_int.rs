//! Runs the model intersection on an INT file (and optionally a W3D file)
//! and writes the SOL text.
//!
//! ```text
//! cargo run -p tectora-core --example mv_int -- IN.INT [IN.W3D|-] OUT.SOL
//!     [--state-in S.json --from N] [--to N] [--state-out S.json]
//! ```
//!
//! With `--state-in` the stages start from a saved state instead of the
//! INT file; `--from`/`--to` pick the stages to run (0..8), `--state-out`
//! saves the state after them. These are for comparing single stages.

use std::process::ExitCode;

use tectora_core::modell::{self, State, Tol};

fn main() -> ExitCode {
    let mut pos = Vec::new();
    let (mut state_in, mut state_out, mut from, mut to) = (None, None, 0usize, 8usize);
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--state-in" => state_in = args.next(),
            "--state-out" => state_out = args.next(),
            "--from" => from = args.next().and_then(|v| v.parse().ok()).unwrap_or(0),
            "--to" => to = args.next().and_then(|v| v.parse().ok()).unwrap_or(8),
            _ => pos.push(a),
        }
    }
    if pos.len() < 3 {
        eprintln!(
            "usage: mv_int IN.INT [IN.W3D|-] OUT.SOL [--state-in S.json --from N] [--to N] [--state-out S.json]"
        );
        return ExitCode::from(2);
    }
    let tol = Tol::default();
    let mut st: State = if let Some(p) = state_in {
        match std::fs::read_to_string(&p).map(|t| serde_json::from_str(&t)) {
            Ok(Ok(s)) => s,
            _ => {
                eprintln!("cannot read state {p}");
                return ExitCode::from(1);
            }
        }
    } else {
        let int = match std::fs::read(&pos[0]) {
            Ok(b) => String::from_utf8_lossy(&b).into_owned(),
            Err(e) => {
                eprintln!("{}: {e}", pos[0]);
                return ExitCode::from(1);
            }
        };
        let w3d = if pos[1] == "-" {
            None
        } else {
            match std::fs::read(&pos[1]) {
                Ok(b) => Some(String::from_utf8_lossy(&b).into_owned()),
                Err(e) => {
                    eprintln!("{}: {e}", pos[1]);
                    return ExitCode::from(1);
                }
            }
        };
        match modell::parse(&int, w3d.as_deref(), &tol) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::from(1);
            }
        }
    };
    modell::run_stages(&mut st, &tol, from, to);
    if let Some(p) = state_out
        && let Err(e) = std::fs::write(&p, serde_json::to_string(&st).unwrap_or_default())
    {
        eprintln!("{p}: {e}");
    }
    if let Err(e) = std::fs::write(&pos[2], modell::write_sol(&st, &tol)) {
        eprintln!("{}: {e}", pos[2]);
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}
