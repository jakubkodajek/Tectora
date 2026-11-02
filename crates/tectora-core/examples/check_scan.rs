//! Prints the plausibility measures (`tectora_core::check::measure`) of
//! every part of the given projects, for choosing the check tolerances.
//!
//! Usage: `check_scan FILE.DAD|PROJECT.json...` (or `-` to read paths from stdin).

use std::io::BufRead;
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut paths: Vec<PathBuf> = args.iter().map(PathBuf::from).collect();
    if paths.len() == 1 && paths[0].as_os_str() == "-" {
        paths = std::io::stdin()
            .lock()
            .lines()
            .map(|l| PathBuf::from(l.unwrap()))
            .collect();
    }
    for p in &paths {
        let name = p.display().to_string();
        let Ok(bytes) = std::fs::read(p) else {
            continue;
        };
        let project: tectora_core::Project = if p
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("json"))
        {
            match serde_json::from_slice(&bytes) {
                Ok(x) => x,
                Err(_) => continue,
            }
        } else {
            match tectora_core::dad::import(&bytes, name.clone()) {
                Ok(x) => x,
                Err(_) => continue,
            }
        };
        let geos = tectora_core::openings::generate_project(&project);
        for (i, (part, geo)) in project.parts.iter().zip(&geos).enumerate() {
            if geo.error.is_some() || geo.faces.is_empty() {
                continue;
            }
            let outline: Vec<_> = project
                .parts
                .iter()
                .flat_map(tectora_core::check::eave_outline)
                .collect();
            let _ = part;
            let m = tectora_core::check::measure(&outline, geo);
            let w = &geo.warnings;
            println!(
                "{name}#{i}\toutside={:.4}\toverlap={:.4}\tarea={:.5}\tdormers={}\tparts={}\twarn={}",
                m.outside,
                m.overlap,
                m.area,
                project.dormers.len(),
                project.parts.len(),
                w.iter()
                    .map(|w| format!("{:?}", w.kind))
                    .collect::<Vec<_>>()
                    .join(",")
            );
        }
    }
}
