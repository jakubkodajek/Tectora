//! Runs the ported core on RD2 files and compares its SO2 (and INT) with the
//! files DACAD wrote next to them.
//!
//! Usage: `dcore_check FILE.RD2...` (or `-` to read the paths from stdin).
//! A case is run with INT output (flags 0x4a0) when a sibling `.INT` exists.

use std::io::BufRead;
use std::path::{Path, PathBuf};

use tectora_core::dcore::pipeline::Run;

fn sibling(p: &Path, ext: &str) -> Option<PathBuf> {
    [ext.to_string(), ext.to_lowercase()]
        .iter()
        .map(|e| p.with_extension(e))
        .find(|q| q.exists())
}

fn first_diff(a: &[u8], b: &[u8]) -> usize {
    a.iter()
        .zip(b)
        .position(|(x, y)| x != y)
        .unwrap_or(a.len().min(b.len()))
}

fn main() {
    let mut paths: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    if paths.len() == 1 && paths[0] == Path::new("-") {
        paths = std::io::stdin()
            .lock()
            .lines()
            .map(|l| PathBuf::from(l.unwrap()))
            .collect();
    }
    let (mut equal, mut differ, mut no_ref, mut errors) = (0, 0, 0, 0);
    for rd2 in &paths {
        let name = rd2.file_stem().unwrap().to_string_lossy();
        let so2_ref = sibling(rd2, "SO2");
        let int_ref = sibling(rd2, "INT");
        let flags = if int_ref.is_some() { 0x4a0 } else { 0x4a8 };
        let bytes = std::fs::read(rd2).unwrap();
        let mut run = Run::new(flags);
        let r = run.run(&bytes);
        let Some(so2_ref) = so2_ref else {
            no_ref += 1;
            println!("NOREF {name} {:?}", r.as_ref().map(|o| o.code));
            continue;
        };
        let out = match r {
            Ok(o) => o,
            Err(e) => {
                errors += 1;
                println!("ERROR {name} {e:?}");
                continue;
            }
        };
        let want = std::fs::read(&so2_ref).unwrap();
        let mut bad = Vec::new();
        if out.sol != want {
            bad.push(format!("SO2 @{}", first_diff(&out.sol, &want)));
        }
        if let Some(int_ref) = &int_ref {
            let want = std::fs::read(int_ref).unwrap();
            match &out.int {
                Some(got) if *got == want => {}
                Some(got) => bad.push(format!("INT @{}", first_diff(got, &want))),
                None => bad.push("INT missing".into()),
            }
        }
        if bad.is_empty() {
            equal += 1;
        } else {
            differ += 1;
            println!("DIFF {name} {}", bad.join(", "));
        }
    }
    println!("equal {equal}, differ {differ}, errors {errors}, no reference {no_ref}");
}
