//! Writes the INT, the W3D and the SOL with walls for an RD2 and its core
//! SOL, and the core SOL again (to check its writer).
//!
//! ```text
//! cargo run -p tectora-core --example int_w3d -- IN.RD2 IN.SO2 OUT_PREFIX
//! cargo run -p tectora-core --example int_w3d -- --list LIST
//! ```
//!
//! Writes OUT_PREFIX.INT, .W3D, .SOL and .SO2. A LIST holds one case per
//! line: `RD2<TAB>SO2<TAB>OUT_PREFIX` (RD2 may be `-` for the W3D only).

use std::process::ExitCode;

use tectora_core::coresol::{self, CoreSol, Rd2};

fn read(p: &str) -> Result<String, String> {
    std::fs::read(p)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .map_err(|e| format!("{p}: {e}"))
}

fn run(rd2: &str, so2: &str, out: &str) -> Result<(), String> {
    let sol = CoreSol::parse(&read(so2)?).map_err(|e| format!("{so2}: {e}"))?;
    let write = |ext: &str, text: &str| {
        std::fs::write(format!("{out}.{ext}"), text).map_err(|e| format!("{out}.{ext}: {e}"))
    };
    write("SO2", &sol.write())?;
    match coresol::wall_stage(&sol) {
        Ok(ws) => {
            write("W3D", &ws.w3d)?;
            write("SOL", &ws.sol)?;
        }
        Err(e) => eprintln!("{so2}: {e}"),
    }
    if rd2 != "-" {
        let r = Rd2::parse(&read(rd2)?).map_err(|e| format!("{rd2}: {e}"))?;
        let int = coresol::write_int(&r, &sol).map_err(|e| format!("{rd2}: {e}"))?;
        write("INT", &int)?;
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cases: Vec<(String, String, String)> = if args.first().map(String::as_str) == Some("--list")
    {
        let Some(list) = args.get(1) else {
            eprintln!("usage: int_w3d --list LIST");
            return ExitCode::from(2);
        };
        match read(list) {
            Ok(t) => t
                .lines()
                .filter_map(|l| {
                    let v: Vec<&str> = l.split('\t').collect();
                    (v.len() == 3).then(|| (v[0].into(), v[1].into(), v[2].into()))
                })
                .collect(),
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::from(1);
            }
        }
    } else if args.len() == 3 {
        vec![(args[0].clone(), args[1].clone(), args[2].clone())]
    } else {
        eprintln!("usage: int_w3d IN.RD2 IN.SO2 OUT_PREFIX | --list LIST");
        return ExitCode::from(2);
    };
    let mut ok = true;
    for (rd2, so2, out) in &cases {
        if let Err(e) = run(rd2, so2, out) {
            eprintln!("{e}");
            ok = false;
        }
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
