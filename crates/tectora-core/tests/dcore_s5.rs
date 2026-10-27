//! S5 (line codes) against the emulator, stage by stage: each stage is
//! started from the emulator's state before it and compared with the
//! emulator's state after it. The chain runs the four stages in a row from
//! `after_dormer` (taking the emulator's `after_holeedge` when the hole
//! edges, not ported here, changed anything) and writes the SO2.
//!
//! `fixtures` runs the committed cases; `s5_dumps` (ignored) runs local
//! dump sets made with the S5 stages (see `src/dcore/PORTING.md`):
//!
//! ```text
//! DCORE_SETS="list_core.txt=dumps/core;list_s1.txt=dumps/s1" \
//!     cargo test --release -p tectora-core --test dcore_s5 -- --ignored --nocapture
//! ```

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::Value;
use tectora_core::dcore::{Core, classify, dump, out};

#[derive(Default)]
struct Tally {
    ok: usize,
    bad: Vec<String>,
}

type Tallies = BTreeMap<String, Tally>;

fn tally(t: &mut Tallies, stage: &str, name: &str, diffs: Vec<String>) {
    let e = t.entry(stage.to_string()).or_default();
    if diffs.is_empty() {
        e.ok += 1;
    } else {
        e.bad
            .push(format!("{name}:\n      {}", diffs.join("\n      ")));
    }
}

/// Each stage with the snapshot before and after it.
const STAGES: [(&str, &str, fn(&mut Core)); 5] = [
    ("after_dormer", "after_classify", classify::classify),
    ("after_classify", "after_codes1", classify::codes),
    ("after_codes1", "after_merge17", classify::merge_d17),
    ("after_merge17", "after_mergeLG", classify::merge_lg17),
    ("after_holeedge", "after_codes2", classify::codes),
];

fn load_doc(dir: &Path) -> Option<dump::DumpDoc> {
    if let Ok(doc) = dump::DumpDoc::load(&dir.join("state.json")) {
        return Some(doc);
    }
    let gz = std::fs::File::open(dir.join("state.json.gz")).ok()?;
    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(gz)
        .read_to_end(&mut bytes)
        .ok()?;
    Some(dump::DumpDoc::from_slice(&bytes).unwrap())
}

fn stage<'a>(doc: &'a dump::DumpDoc, name: &str) -> Option<&'a Value> {
    doc.stage(name).first().copied()
}

fn run_case(name: &str, dir: &Path, t: &mut Tallies) -> bool {
    let Some(doc) = load_doc(dir) else {
        return false;
    };
    for (before, after, f) in STAGES {
        let (Some(b), Some(a)) = (stage(&doc, before), stage(&doc, after)) else {
            continue;
        };
        let mut c = dump::from_json(b).unwrap();
        f(&mut c);
        tally(t, after, name, dump::compare(a, &dump::to_json(&c), 6));
    }
    // The chain and the SO2.
    let Some(start) = stage(&doc, "after_dormer") else {
        return true;
    };
    let (Some(lg), Some(he), Some(end)) = (
        stage(&doc, "after_mergeLG"),
        stage(&doc, "after_holeedge"),
        stage(&doc, "after_codes2"),
    ) else {
        return true;
    };
    let mut c = dump::from_json(start).unwrap();
    for (_, _, f) in &STAGES[..4] {
        f(&mut c);
    }
    let d = dump::compare(lg, &dump::to_json(&c), 6);
    let clean = d.is_empty();
    tally(t, "chain after_mergeLG", name, d);
    let holes = !dump::compare(lg, he, 1).is_empty();
    if holes {
        c = dump::from_json(he).unwrap();
    }
    classify::codes(&mut c);
    tally(
        t,
        "chain after_codes2",
        name,
        dump::compare(end, &dump::to_json(&c), 6),
    );
    tally(
        t,
        if holes {
            "SO2 via emulator hole edges"
        } else {
            "SO2 end to end"
        },
        name,
        match out::write_sol(&mut c) {
            Ok(b) => {
                let want = std::fs::read(dir.join(format!("{name}.SO2"))).unwrap_or_default();
                if b == want && (clean || holes) {
                    vec![]
                } else if b == want {
                    vec!["SO2 equal, state before the hole edges differs".into()]
                } else {
                    vec!["SO2 differs".into()]
                }
            }
            Err(e) => vec![e.to_string()],
        },
    );
    true
}

fn report(t: &Tallies, show: usize) -> bool {
    let mut fail = false;
    for (stage, x) in t {
        println!("{stage}: {} equal, {} differ", x.ok, x.bad.len());
        for b in x.bad.iter().take(show) {
            println!("    {b}");
        }
        fail |= !x.bad.is_empty();
    }
    fail
}

#[test]
#[ignore]
fn s5_dumps() {
    let sets = std::env::var("DCORE_SETS").expect("DCORE_SETS=LIST=DIR;...");
    let mut t = Tallies::new();
    let mut n = 0;
    for set in sets.split(';').filter(|s| !s.is_empty()) {
        let (list, dir) = set.split_once('=').unwrap();
        for line in std::fs::read_to_string(list).unwrap().lines() {
            let rd2 = PathBuf::from(line.split('\t').next().unwrap());
            let stem = rd2.file_stem().unwrap().to_string_lossy().to_string();
            n += usize::from(run_case(&stem, &Path::new(dir).join(&stem), &mut t));
        }
    }
    println!("{n} cases");
    assert!(!report(&t, 5));
}

#[test]
fn fixtures() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/dcore");
    let mut t = Tallies::new();
    for e in std::fs::read_dir(root).unwrap() {
        let dir = e.unwrap().path();
        if dir.is_dir() {
            let name = dir.file_name().unwrap().to_string_lossy().to_string();
            assert!(run_case(&name, &dir, &mut t), "{name}");
        }
    }
    assert!(!report(&t, 20));
}

/// Emulator vectors of the segment primitives (`segseg`: seg_seg with a
/// mode; `segon`: seg_on_seg): `fn TAB inputs TAB eax TAB outputs`, words in hex.
fn s5_vectors(path: &Path) -> (usize, Vec<String>) {
    use tectora_core::dcore::{geom, num::Tol};
    let t = Tol::default();
    let text = std::fs::read_to_string(path).unwrap();
    let mut bad = Vec::new();
    let mut n = 0;
    for line in text.lines().filter(|l| !l.is_empty()) {
        let cols: Vec<&str> = line.split('\t').collect();
        let words = |s: &str| -> Vec<u32> {
            s.split_whitespace()
                .map(|w| u32::from_str_radix(w, 16).unwrap())
                .collect()
        };
        let ins = words(cols[1]);
        let f = |i: usize| f32::from_bits(ins[i]);
        let v3 = |i: usize| [f(i), f(i + 1), f(i + 2)];
        let eax: i64 = cols[2].parse().unwrap();
        let outs = words(cols.get(3).copied().unwrap_or(""));
        let (got, got_out) = match cols[0] {
            "segseg" => {
                let (m, o) = geom::seg_seg((v3(0), v3(3)), (v3(6), v3(9)), ins[12], &t);
                (i64::from(m), o.iter().map(|x| x.to_bits()).collect::<Vec<_>>())
            }
            "segon" => (
                i64::from(geom::seg_on_seg(v3(0), v3(3), v3(6), v3(9), &t)),
                vec![],
            ),
            other => panic!("unknown vector kind {other}"),
        };
        n += 1;
        // The output point only matters when the lines meet.
        let out_ok = cols[0] != "segseg" || eax == 0 || got_out == outs;
        if got != eax || !out_ok {
            bad.push(format!("{line}\n      got {got} {got_out:08x?}"));
        }
    }
    (n, bad)
}

#[test]
fn fixture_vectors() {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/dcore/s5.tsv");
    let (n, bad) = s5_vectors(&p);
    assert!(n > 0);
    assert!(bad.is_empty(), "{} of {n} differ:\n{}", bad.len(), bad[..bad.len().min(10)].join("\n"));
}

#[test]
#[ignore]
fn all_vectors() {
    let p = std::env::var("DCORE_S5VEC").expect("DCORE_S5VEC=file");
    let (n, bad) = s5_vectors(Path::new(&p));
    println!("{n} vectors, {} differ", bad.len());
    assert!(bad.is_empty(), "{}", bad[..bad.len().min(10)].join("\n"));
}
