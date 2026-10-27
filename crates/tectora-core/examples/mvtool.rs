//! Scratch tool (not committed): runs the Modellverschnitt on captured
//! files and sums the result like the DAD line groups.

use std::collections::BTreeMap;

use tectora_core::modell::{self, FaceKind};

fn read(p: &str) -> String {
    String::from_utf8_lossy(&std::fs::read(p).unwrap()).into_owned()
}

fn code_name(c: i32) -> String {
    match c {
        1 => "T",
        2 => "F",
        3 => "G",
        4 => "K",
        5 => "P",
        6 => "OL",
        7 => "OR",
        8 => "OS",
        9 => "WV",
        10 => "WH",
        11 => "WS",
        12 => "c",
        15 => "S",
        _ => return format!("#{c}"),
    }
    .to_string()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args[1].as_str() {
        "oracle" => {
            let caps: serde_json::Value =
                serde_json::from_str(&read("/tmp/claude-0/wiremv/caps.json")).unwrap();
            for case in &args[2..] {
                let c = &caps[case];
                let int = read(c["INT"].as_str().unwrap());
                let w3d = read(c["W3D"].as_str().unwrap());
                let nw = std::env::var("MR3_W3DDIR").is_ok_and(|d| d == "none");
                let out = modell::run(&int, (!nw).then_some(w3d.as_str())).unwrap();
                let mut s: BTreeMap<String, f64> = BTreeMap::new();
                for l in &out.lines {
                    let len = l.p1.sub(l.p2).length();
                    *s.entry(code_name(l.code)).or_default() += len * l.sign as f64;
                }
                let mut area: BTreeMap<String, f64> = BTreeMap::new();
                let mut k = 0;
                for f in &out.state.faces {
                    let n = f.pieces.len() + f.cutters.len();
                    let v = if (f.rec.angle - 90.0).abs() < 1e-3 { "v" } else { "r" };
                    for of in &out.faces[k..k + n] {
                        let kind = match of.kind {
                            FaceKind::Surface => "S".to_string(),
                            FaceKind::Cut(c) => format!("C{c}"),
                        };
                        *area
                            .entry(format!("{v}{}{kind}", f.rec.kind as u8 as char))
                            .or_default() += of.area;
                    }
                    k += n;
                }
                for of in &out.faces[k..] {
                    *area.entry("wall".into()).or_default() += of.area;
                }
                let g: serde_json::Value = serde_json::from_str(&read(&format!(
                    "/mnt/project-files/dacad-golden-current/{case}.json"
                )))
                .unwrap();
                let r = &g["dacad_results"];
                println!("== {case}");
                let mut keys: Vec<String> = s.keys().cloned().collect();
                for k in r["lines_by_type"].as_object().unwrap().keys() {
                    if !keys.contains(k) {
                        keys.push(k.clone());
                    }
                }
                for k in keys {
                    let a = s.get(&k).copied().unwrap_or(0.0);
                    let b = r["lines_by_type"][&k].as_f64().unwrap_or(0.0);
                    let mark = if (a - b).abs() > 0.011 { "  <<" } else { "" };
                    println!("  {k:4} mv {a:10.4} dad {b:10.4}{mark}");
                }
                println!("  area {area:?} tiles {}", r["area_groups"]["Škridla - "]);
            }
        }
        "mvlines" => {
            let caps: serde_json::Value =
                serde_json::from_str(&read("/tmp/claude-0/wiremv/caps.json")).unwrap();
            let c = &caps[&args[2]];
            let out = modell::run(
                &read(c["INT"].as_str().unwrap()),
                Some(&read(c["W3D"].as_str().unwrap())),
            )
            .unwrap();
            for (i, l) in out.lines.iter().enumerate() {
                let n = code_name(l.code);
                if args.len() > 3 && n != args[3] {
                    continue;
                }
                println!(
                    "{i:4} {n:4} {:8.4} ({:.3},{:.3},{:.4}) ({:.3},{:.3},{:.4})",
                    l.p1.sub(l.p2).length() * l.sign as f64,
                    l.p1.x, l.p1.y, l.p1.z, l.p2.x, l.p2.y, l.p2.z
                );
            }
        }
        "mr3" => {
            let base = "/mnt/project-files/core-stress/mr3";
            let mut ok = 0;
            let mut n = 0;
            let mut names: Vec<String> = std::fs::read_dir(format!("{base}/results/golden"))
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().replace(".json", ""))
                .collect();
            names.sort();
            let neg = args.get(2).is_some_and(|a| a == "neg");
            for name in names {
                n += 1;
                let int = format!("{base}/core/{name}.INT");
                if !std::path::Path::new(&int).exists() {
                    println!("{name}: no INT");
                    continue;
                }
                let w3d = match std::env::var("MR3_W3DDIR") {
                    Ok(d) if d == "none" => None,
                    Ok(d) => Some(read(&format!("{d}/{name}.W3D"))),
                    Err(_) => Some(read(&format!("{base}/core/{name}.W3D"))),
                };
                let out = modell::run(&read(&int), w3d.as_deref()).unwrap();
                let mut s: BTreeMap<String, f64> = BTreeMap::new();
                for l in &out.lines {
                    if l.sign < 0 && !neg {
                        continue;
                    }
                    let len = l.p1.sub(l.p2).length();
                    *s.entry(code_name(l.code)).or_default() += len * l.sign as f64;
                }
                let g: serde_json::Value = serde_json::from_str(&read(&format!(
                    "{base}/results/golden/{name}.json"
                )))
                .unwrap();
                let d = g["dacad_results"]["lines_by_type"].as_object().unwrap();
                let mut diff = Vec::new();
                let mut keys: Vec<String> = d.keys().cloned().collect();
                keys.extend(s.keys().filter(|k| !k.starts_with('#')).cloned());
                keys.sort();
                keys.dedup();
                for k in keys {
                    let a = s.get(&k).copied().unwrap_or(0.0);
                    let b = d.get(&k).and_then(|v| v.as_f64()).unwrap_or(0.0);
                    if (a - b).abs() > 0.03 {
                        diff.push(format!("{k} {a:.2}/{b:.2}"));
                    }
                }
                if diff.is_empty() {
                    ok += 1;
                } else {
                    println!("{name}: {}", diff.join(", "));
                }
            }
            println!("{ok}/{n}");
        }
        _ => panic!("?"),
    }
}
