//! Which way the engine calculates each project: DACAD's core, a DACAD
//! error, or the sweep fallback (with the reason).
//!
//! Usage: `engine_check [--rd2 DIR] FILE.DAD...` (or `-` to read the paths
//! from stdin). With `--rd2` the RD2 the engine writes for each project is
//! saved as `DIR/NAME.RD2`, for comparing with the one DACAD wrote.

use std::io::BufRead;
use std::path::PathBuf;

use tectora_core::engine::{self, Failure};

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut rd2_dir = None;
    if args.first().is_some_and(|a| a == "--rd2") && args.len() >= 2 {
        rd2_dir = Some(PathBuf::from(args[1].clone()));
        args.drain(..2);
    }
    let mut paths: Vec<PathBuf> = args.iter().map(PathBuf::from).collect();
    if paths.len() == 1 && paths[0].as_os_str() == "-" {
        paths = std::io::stdin()
            .lock()
            .lines()
            .map(|l| PathBuf::from(l.unwrap()))
            .collect();
    }
    let (mut core, mut dacad, mut ours) = (0, 0, 0);
    for p in &paths {
        let name = p.file_stem().unwrap().to_string_lossy().into_owned();
        let Ok(bytes) = std::fs::read(p) else {
            println!("{name}: unreadable");
            continue;
        };
        let mut project = match tectora_core::dad::import(&bytes, name.clone()) {
            Ok(x) => x,
            Err(e) => {
                println!("{name}: import {e}");
                continue;
            }
        };
        for part in &mut project.parts {
            part.normalize_ccw();
        }
        if let Some(dir) = &rd2_dir
            && let Ok((text, _)) = engine::rd2_text(&project)
        {
            std::fs::write(dir.join(format!("{name}.RD2")), text).unwrap();
        }
        match engine::generate(&project) {
            Ok(_) => {
                core += 1;
                println!("{name}: core");
            }
            Err(Failure::Dacad(e)) => {
                dacad += 1;
                println!("{name}: DACAD error {e}");
            }
            Err(Failure::Ours(e)) => {
                ours += 1;
                println!("{name}: fallback ({e})");
            }
        }
    }
    println!("core {core}, DACAD errors {dacad}, fallback {ours}");
}
