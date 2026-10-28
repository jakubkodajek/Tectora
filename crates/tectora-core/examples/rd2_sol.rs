//! The SOL DACAD's totals come from, for an RD2: the core SOL for one
//! roof, the Modellverschnitt's SOL when the core writes an INT (two or
//! more roofs).
//!
//! Usage: `rd2_sol IN.RD2 OUT.SOL [--int]` (`--int` forces the INT run).

use std::process::ExitCode;

use tectora_core::coresol::{CoreSol, Rd2, wall_stage};
use tectora_core::dcore::pipeline::Run;
use tectora_core::modell;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: rd2_sol IN.RD2 OUT.SOL [--int]");
        return ExitCode::FAILURE;
    }
    let text = std::fs::read_to_string(&args[0]).expect("RD2");
    let rd2 = Rd2::parse(&text).expect("RD2 parse");
    let roofs = rd2.models.iter().filter(|m| m.kind == b'D').count();
    let int = roofs > 1 || args.iter().any(|a| a == "--int");
    let out = match Run::new(if int { 0x4a0 } else { 0x4a8 }).run(text.as_bytes()) {
        Ok(o) => o,
        Err(e) => {
            println!("ERROR {e}");
            return ExitCode::FAILURE;
        }
    };
    let sol_text = String::from_utf8_lossy(&out.sol).into_owned();
    let result = if let Some(int) = out.int.as_deref().filter(|_| int) {
        let sol = CoreSol::parse(&sol_text).expect("SO2");
        let w3d = wall_stage(&sol).expect("walls").w3d;
        let st = modell::run(&String::from_utf8_lossy(int), Some(&w3d)).expect("modell");
        modell::write_sol(&st.state, &modell::Tol::default())
    } else {
        sol_text
    };
    std::fs::write(&args[1], result).expect("write");
    ExitCode::SUCCESS
}
