//! dcore against the emulator of DACAD's core.
//!
//! `fixtures` runs the committed cases in `tests/fixtures/dcore` (our own
//! cases with their emulator dumps); `emulator_dumps` (ignored) runs every
//! case of locally generated dump sets, see `src/dcore/PORTING.md`:
//!
//! ```text
//! DCORE_SETS="list_core.txt=dumps/core;list_s1.txt=dumps/s1" \
//!     cargo test --release -p tectora-core --test dcore_emulator -- --ignored --nocapture
//! ```
//!
//! Each list line is `path/to/CASE.RD2[<TAB>...]`, the dump of a case is
//! `DIR/CASE/state.json` with the core's outputs `DIR/CASE/CASE.SO2/.INT`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;
use tectora_core::dcore::{self, dump, out, s1, x87};

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
        e.bad.push(format!("{name}:\n      {}", diffs.join("\n      ")));
    }
}

/// Stages of the run from the RD2 (only the first model reaches S1 until
/// S2 is ported).
fn run_from_rd2(name: &str, rd2: &[u8], doc: &dump::DumpDoc, t: &mut Tallies) {
    let mut snaps: Vec<(String, Value)> = Vec::new();
    let mut run = dcore::pipeline::Run::new(doc.flags());
    run.hook = Some(Box::new(|stage: &str, c: &dcore::Core| {
        snaps.push((stage.to_string(), dump::to_json(c)));
    }));
    let res = run.run(rd2);
    drop(run);
    for stage in ["after_read", "s1_ring", "s1_frames", "s1_end"] {
        let want = doc.stage(stage);
        let got = snaps.iter().find(|s| s.0 == stage);
        match (want.first(), got) {
            (Some(w), Some(g)) => tally(t, &format!("rd2 {stage}"), name, dump::compare(w, &g.1, 6)),
            (None, None) => {}
            (w, g) => tally(
                t,
                &format!("rd2 {stage}"),
                name,
                vec![format!(
                    "stage reached: emulator {} port {} ({:?})",
                    w.is_some(),
                    g.is_some(),
                    res.as_ref().err()
                )],
            ),
        }
    }
    // The uninitialised local guarding the hole relink must never be 1.
    for s in doc.stage("s1_ring") {
        if s["g"]["relink_slot"] == 1 {
            tally(t, "relink guard", name, vec!["relink_slot == 1".into()]);
        }
    }
}

/// S1 of every model, each started from the emulator's state at the entry
/// of the per-model routine.
fn s1_from_entry(name: &str, doc: &dump::DumpDoc, t: &mut Tallies) {
    let entries = doc.stage("s1_entry");
    for (k, e) in entries.iter().enumerate() {
        let mut c = match dump::from_json(e) {
            Ok(c) => c,
            Err(err) => {
                tally(t, "entry load", name, vec![err.to_string()]);
                continue;
            }
        };
        let mut snaps: Vec<(String, Value)> = Vec::new();
        let mut hook: s1::Hook = Some(Box::new(|stage: &str, c: &dcore::Core| {
            snaps.push((stage.to_string(), dump::to_json(c)));
        }));
        let res = s1::model_start(&mut c, &mut hook);
        drop(hook);
        let call = e["call"].clone();
        for stage in ["s1_ring", "s1_frames", "s1_end"] {
            let want = doc.snaps().iter().find(|s| s["stage"] == stage && s["call"] == call);
            let got = snaps.iter().find(|s| s.0 == stage);
            let label = format!("{name} model {k}");
            match (want, got) {
                (Some(w), Some(g)) => tally(t, &format!("entry {stage}"), &label, dump::compare(w, &g.1, 6)),
                (None, None) => {}
                (w, g) => tally(
                    t,
                    &format!("entry {stage}"),
                    &label,
                    vec![format!("stage reached: emulator {} port {} ({:?})", w.is_some(), g.is_some(), res)],
                ),
            }
        }
    }
}

fn first_diff(a: &[u8], b: &[u8]) -> String {
    let i = a.iter().zip(b).position(|(x, y)| x != y).unwrap_or(a.len().min(b.len()));
    let ctx = |s: &[u8]| String::from_utf8_lossy(&s[i.saturating_sub(40).min(s.len())..(i + 40).min(s.len())]).to_string();
    format!("differs at byte {i}: want {:?} got {:?}", ctx(a), ctx(b))
}

/// The writers on the emulator's final state.
fn writers(name: &str, dir: &Path, doc: &dump::DumpDoc, t: &mut Tallies) {
    if let Some(s) = doc.stage("after_codes2").first() {
        let mut c = dump::from_json(s).unwrap();
        match out::write_sol(&mut c) {
            Ok(b) => {
                let want = std::fs::read(dir.join(format!("{name}.SO2"))).unwrap_or_default();
                let d = if b == want { vec![] } else { vec![first_diff(&want, &b)] };
                tally(t, "SO2 bytes", name, d);
                if let Some(w) = doc.stage("after_sol").first() {
                    tally(t, "SO2 state", name, dump::compare(w, &dump::to_json(&c), 6));
                }
            }
            Err(e) => tally(t, "SO2 bytes", name, vec![e.to_string()]),
        }
    }
    if doc.doc["int"] == true
        && let Some(s) = doc.stage("after_sol").first()
    {
        let mut c = dump::from_json(s).unwrap();
        match out::write_int(&mut c) {
            Ok(b) => {
                let want = std::fs::read(dir.join(format!("{name}.INT"))).unwrap_or_default();
                let d = if b == want { vec![] } else { vec![first_diff(&want, &b)] };
                tally(t, "INT bytes", name, d);
                if let Some(w) = doc.stage("after_int").first() {
                    tally(t, "INT state", name, dump::compare(w, &dump::to_json(&c), 6));
                }
            }
            Err(e) => tally(t, "INT bytes", name, vec![e.to_string()]),
        }
    }
}

fn run_case(rd2: &Path, dir: &Path, t: &mut Tallies) -> bool {
    let name = rd2.file_stem().unwrap().to_string_lossy().to_string();
    let Ok(doc) = dump::DumpDoc::load(&dir.join("state.json")) else {
        return false;
    };
    if let Ok(bytes) = std::fs::read(rd2) {
        run_from_rd2(&name, &bytes, &doc, t);
    }
    s1_from_entry(&name, &doc, t);
    writers(&name, dir, &doc, t);
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
fn emulator_dumps() {
    let sets = std::env::var("DCORE_SETS").expect("DCORE_SETS=LIST=DIR;...");
    let mut t = Tallies::new();
    let mut n = 0;
    for set in sets.split(';').filter(|s| !s.is_empty()) {
        let (list, dir) = set.split_once('=').unwrap();
        for line in std::fs::read_to_string(list).unwrap().lines() {
            let rd2 = PathBuf::from(line.split('\t').next().unwrap());
            let stem = rd2.file_stem().unwrap().to_string_lossy().to_string();
            n += usize::from(run_case(&rd2, &Path::new(dir).join(&stem), &mut t));
        }
    }
    println!("{n} cases");
    assert!(!report(&t, 5));
}

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/dcore")
}

/// The committed cases: `tests/fixtures/dcore/CASE/{CASE.RD2,state.json,CASE.SO2,CASE.INT}`.
#[test]
fn fixtures() {
    let mut t = Tallies::new();
    let mut n = 0;
    for e in std::fs::read_dir(fixture_dir()).unwrap() {
        let dir = e.unwrap().path();
        if !dir.is_dir() {
            continue;
        }
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        assert!(run_case(&dir.join(format!("{name}.RD2")), &dir, &mut t), "{name}");
        n += 1;
    }
    assert!(n > 0);
    assert!(!report(&t, 20));
}

fn hex10(s: &str) -> [u8; 10] {
    let mut b = [0u8; 10];
    for (i, x) in b.iter_mut().enumerate() {
        *x = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap();
    }
    b
}

/// The transcendentals at the inputs the emulator evaluated (op, input,
/// second input of atan2, result; 80-bit images in hex).
fn x87_vectors(path: &Path) -> (usize, Vec<String>) {
    let mut bad = Vec::new();
    let mut n = 0;
    for line in std::fs::read_to_string(path).unwrap().lines() {
        let f: Vec<&str> = line.split('\t').collect();
        let a = x87::X80::from_bytes(hex10(f[1]));
        let x = a.to_f64();
        if x87::X80::from_f64(x) != a {
            continue; // not a double input
        }
        let got = match f[0] {
            "sin" => x87::fsin(x),
            "cos" => x87::fcos(x),
            "tan" => x87::fptan(x),
            "atan2" => {
                let b = x87::X80::from_bytes(hex10(f[2]));
                let xx = b.to_f64();
                if x87::X80::from_f64(xx) != b {
                    continue;
                }
                x87::fpatan(x, xx)
            }
            _ => continue,
        };
        n += 1;
        if got.to_bytes() != hex10(f[3]) {
            bad.push(format!("{line} -> {:02x?}", got.to_bytes()));
        }
    }
    (n, bad)
}

#[test]
fn x87_fixture_vectors() {
    let (n, bad) = x87_vectors(&fixture_dir().join("x87.tsv"));
    assert!(n > 0);
    assert!(bad.is_empty(), "{n} vectors, wrong: {bad:#?}");
}

#[test]
#[ignore]
fn x87_all_vectors() {
    let p = std::env::var("DCORE_X87").expect("DCORE_X87=file");
    let (n, bad) = x87_vectors(Path::new(&p));
    println!("{n} vectors, {} wrong", bad.len());
    for b in bad.iter().take(20) {
        println!("{b}");
    }
    assert!(bad.is_empty());
}
