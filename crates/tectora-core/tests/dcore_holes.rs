//! S6 (chimney holes and hole edges) against the emulator, each stage run
//! in isolation: load the emulator's state before the stage, run the port's
//! stage, compare with the emulator's state after it.
//!
//! - chimney holes: `after_cut` → `after_holes`, every model (flag 0x8);
//! - hole edges: `after_mergeLG` → `after_holeedge`.
//!
//! `fixtures` runs the committed cases in `tests/fixtures/dcore_holes`
//! (own cases: C27, s1 R1_0815 with three models, the chimney sweep case
//! V70_0930 with hole-edge copies, P1a with a non-convex chimney, and the
//! mr3 joint case M3_0348 dumped with `--flags=0x4a8 --noint` so that its
//! chimney is cut in the core); `hole_dumps` (ignored) runs local dump sets:
//!
//! ```text
//! DCORE_HOLES="list.txt=dumps/dir;..." \
//!     cargo test --release -p tectora-core --test dcore_holes -- --ignored --nocapture
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;
use tectora_core::dcore::{self, dump};

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

fn load_doc(dir: &Path) -> Option<dump::DumpDoc> {
    if let Ok(doc) = dump::DumpDoc::load(&dir.join("state.json")) {
        return Some(doc);
    }
    let gz = std::fs::File::open(dir.join("state.json.gz")).ok()?;
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut flate2::read::GzDecoder::new(gz), &mut bytes).ok()?;
    Some(dump::DumpDoc::from_slice(&bytes).unwrap())
}

/// The emulator's first error is the cancel after a warning box.
fn emu_cancelled(doc: &dump::DumpDoc) -> bool {
    doc.doc["errors"]
        .as_array()
        .and_then(|a| a.first())
        .is_some_and(|e| e["kind"] == "fatal" && e["code"] == 0)
}

/// Runs `stage` on the state of each `before` snapshot and compares with
/// the `after` snapshot of the same model run.
fn isolated(
    name: &str,
    doc: &dump::DumpDoc,
    before: &str,
    after: &str,
    label: &str,
    f: fn(&mut dcore::Core) -> Result<(), dcore::CoreError>,
    t: &mut Tallies,
) {
    let afters = doc.stage(after);
    for (k, b) in doc.stage(before).iter().enumerate() {
        let Some(want) = afters.iter().find(|a| a["call"] == b["call"]) else {
            continue;
        };
        let mut c = match dump::from_json(b) {
            Ok(c) => c,
            Err(e) => {
                tally(t, &format!("{label} load"), name, vec![e.to_string()]);
                continue;
            }
        };
        let r = f(&mut c);
        let mut d = match &r {
            Ok(()) => vec![],
            Err(dcore::CoreError::Fatal { code: 0, .. }) if emu_cancelled(doc) => vec![],
            Err(e) => vec![format!("port: {e}")],
        };
        d.extend(dump::compare(want, &dump::to_json(&c), 8));
        let lab = if afters.len() > 1 {
            format!("{name} model {k}")
        } else {
            name.to_string()
        };
        tally(t, label, &lab, d);
        if r.is_err() {
            tally(t, &format!("{label} cancelled"), &lab, vec![]);
        }
    }
}

/// Counts what the case exercises (hole pieces and hole-edge copies).
fn features(doc: &dump::DumpDoc, t: &mut Tallies, name: &str) {
    let pieces = |s: &Value| {
        s["recs"]
            .as_array()
            .map(|r| r.iter().filter(|x| x["bt"] == "HOLE").count())
            .unwrap_or(0)
    };
    if let Some(s) = doc.stage("after_holes").last()
        && pieces(s) > 0
    {
        tally(t, "cases with hole pieces", name, vec![]);
    }
    let lines = |s: &Value| s["g"]["n_lines"].as_i64().unwrap_or(0);
    if let (Some(a), Some(b)) = (
        doc.stage("after_mergeLG").first(),
        doc.stage("after_holeedge").first(),
    ) && lines(b) > lines(a)
    {
        tally(t, "cases with hole-edge copies", name, vec![]);
    }
}

fn run_case(name: &str, dir: &Path, t: &mut Tallies) -> bool {
    let Some(doc) = load_doc(dir) else {
        return false;
    };
    if doc.flags() & 0x8 != 0 {
        isolated(
            name,
            &doc,
            "after_cut",
            "after_holes",
            "chimney holes",
            dcore::stages::chimney_holes,
            t,
        );
    }
    isolated(
        name,
        &doc,
        "after_mergeLG",
        "after_holeedge",
        "hole edges",
        dcore::stages::hole_edges,
        t,
    );
    features(&doc, t, name);
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
fn hole_dumps() {
    let sets = std::env::var("DCORE_HOLES").expect("DCORE_HOLES=LIST=DIR;...");
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

/// The committed cases: `tests/fixtures/dcore_holes/CASE/state.json.gz`.
#[test]
fn fixtures() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/dcore_holes");
    let mut t = Tallies::new();
    let mut n = 0;
    for e in std::fs::read_dir(root).unwrap() {
        let dir = e.unwrap().path();
        if !dir.is_dir() {
            continue;
        }
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        assert!(run_case(&name, &dir, &mut t), "{name}");
        n += 1;
    }
    assert!(n > 0);
    assert!(!report(&t, 20));
}
