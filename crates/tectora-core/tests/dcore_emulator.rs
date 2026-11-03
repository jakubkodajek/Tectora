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
        e.bad
            .push(format!("{name}:\n      {}", diffs.join("\n      ")));
    }
}

/// Stages of the run from the RD2 compared by `run_from_rd2`, in run order.
const RD2_STAGES: [&str; 4] = ["after_read", "s1_ring", "s1_frames", "s1_end"];

/// The emulator's outcome: (pipeline code, message id) of its first error.
fn emu_error(doc: &dump::DumpDoc) -> Option<(String, u64, Option<u32>)> {
    let e = doc.doc["errors"].as_array()?.first()?;
    let id = e["msg_id"]
        .as_str()
        .map(|h| u32::from_str_radix(h.trim_start_matches("0x"), 16).unwrap());
    Some((e["kind"].as_str()?.to_string(), e["code"].as_u64()?, id))
}

/// How the port's run ended compared with the emulator's: fatal errors
/// (RD2, interval, broken wall chain with its fallback SOL) must match now;
/// cut errors and the final SO2 are checked once the stages before them
/// stop returning `Unimplemented`.
fn outcome(
    name: &str,
    dir: &Path,
    doc: &dump::DumpDoc,
    res: &Result<dcore::pipeline::Outputs, dcore::CoreError>,
    t: &mut Tallies,
) {
    use dcore::CoreError as E;
    let so2 = std::fs::read(dir.join(format!("{name}.SO2"))).unwrap_or_default();
    let mut d = Vec::new();
    match (emu_error(doc), res) {
        (_, Err(E::Unimplemented(_))) => return,
        (Some((k, 3, _)), Ok(o)) if k == "fatal" => {
            if o.code != 3 {
                d.push(format!("code {} instead of 3", o.code));
            }
            if o.sol != so2 {
                d.push(first_diff(&so2, &o.sol));
            }
        }
        (Some((k, code, id)), Err(E::Fatal { code: c, msg })) if k == "fatal" => {
            if (code, id) != (u64::from(*c), msg.id) {
                d.push(format!(
                    "emulator {code}/{id:x?}, port {c}/{:x?} {msg}",
                    msg.id
                ));
            }
            if so2 != dcore::out::PLACEHOLDER_SOL {
                d.push("emulator SO2 is not the placeholder".into());
            }
        }
        (Some((k, code, id)), Err(E::Cut { code: c, msg })) if k == "cut" => {
            if (code, id) != (u64::from(*c), msg.id) {
                d.push(format!(
                    "emulator {code}/{id:x?}, port {c}/{:x?} {msg}",
                    msg.id
                ));
            }
        }
        (None, Ok(o)) => {
            if o.sol != so2 {
                d.push(first_diff(&so2, &o.sol));
            }
            if doc.doc["int"] == true {
                let want = std::fs::read(dir.join(format!("{name}.INT"))).unwrap_or_default();
                if o.int.as_deref() != Some(&want[..]) {
                    d.push("INT differs".into());
                }
            }
        }
        (e, r) => d.push(format!(
            "emulator {e:?}, port {:?}",
            r.as_ref().map(|o| o.code)
        )),
    }
    tally(t, "outcome", name, d);
}

/// Stages of the run from the RD2 (only the first model reaches S1 until
/// S2 is ported).
fn run_from_rd2(name: &str, dir: &Path, rd2: &[u8], doc: &dump::DumpDoc, t: &mut Tallies) {
    let mut snaps: Vec<(String, Value)> = Vec::new();
    let mut run = dcore::pipeline::Run::new(doc.flags());
    run.want_int = doc.doc["int"] == true;
    run.hook = Some(Box::new(|stage: &str, c: &dcore::Core| {
        snaps.push((stage.to_string(), dump::to_json(c)));
    }));
    let res = run.run(rd2);
    drop(run);
    for (k, stage) in RD2_STAGES.iter().enumerate() {
        let want = doc.stage(stage);
        let got = snaps.iter().find(|s| s.0 == *stage);
        // A dump made with --only lacks stages the emulator did pass.
        let later = RD2_STAGES[k + 1..].iter().any(|l| !doc.stage(l).is_empty());
        match (want.first(), got) {
            (Some(w), Some(g)) => {
                tally(t, &format!("rd2 {stage}"), name, dump::compare(w, &g.1, 6))
            }
            (None, None) => {}
            (None, Some(_)) if later => {}
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
    outcome(name, dir, doc, &res, t);
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
            let want = doc
                .snaps()
                .iter()
                .find(|s| s["stage"] == stage && s["call"] == call);
            let got = snaps.iter().find(|s| s.0 == stage);
            let label = format!("{name} model {k}");
            match (want, got) {
                (Some(w), Some(g)) => tally(
                    t,
                    &format!("entry {stage}"),
                    &label,
                    dump::compare(w, &g.1, 6),
                ),
                (None, None) => {}
                (w, g) => tally(
                    t,
                    &format!("entry {stage}"),
                    &label,
                    vec![format!(
                        "stage reached: emulator {} port {} ({:?})",
                        w.is_some(),
                        g.is_some(),
                        res
                    )],
                ),
            }
        }
    }
}

/// S4 in isolation: each stage of each model started from the emulator's
/// state before it (`after_holes` → merge → `after_merge` → edge table →
/// `after_edgetab` → areas → `after_area`; the last `after_area` → dormer
/// feet → `after_dormer`).
fn s4_slices(name: &str, doc: &dump::DumpDoc, t: &mut Tallies) {
    use dcore::state::{Core, ctr};
    use dcore::{CoreError, post};
    let holes = doc.stage("after_holes");
    let merge = doc.stage("after_merge");
    let edget = doc.stage("after_edgetab");
    let area = doc.stage("after_area");
    let load = |v: &Value| dump::from_json(v).map_err(|e| e.to_string());
    let mut check = |stage: &str,
                     label: &str,
                     from: &Value,
                     want: Option<&Value>,
                     f: &dyn Fn(&mut Core) -> Result<(), CoreError>| {
        let Some(want) = want else { return };
        let mut c = match load(from) {
            Ok(c) => c,
            Err(e) => return tally(t, stage, label, vec![e]),
        };
        let d = match f(&mut c) {
            Ok(()) => dump::compare(want, &dump::to_json(&c), 6),
            Err(e) => vec![e.to_string()],
        };
        tally(t, stage, label, d);
    };
    for (k, h) in holes.iter().enumerate() {
        let label = format!("{name} model {k}");
        // the merge runs only after a successful cut; only the last model can lack it
        let has_merge = k < merge.len();
        if has_merge {
            check("s4 merge", &label, h, Some(merge[k]), &|c| {
                let n = post::collinear_merge(c)?;
                c.g.counters[ctr::LINES] -= n;
                Ok(())
            });
        }
        let from = if has_merge { merge[k] } else { h };
        check("s4 edge table", &label, from, edget.get(k).copied(), &|c| {
            post::edge_table(c)
        });
        if let Some(e) = edget.get(k) {
            check("s4 areas", &label, e, area.get(k).copied(), &|c| {
                post::areas(c)
            });
        }
    }
    if let (Some(last), Some(want)) = (area.last(), doc.stage("after_dormer").first()) {
        check("s4 dormer feet", name, last, Some(want), &|c| {
            let m = c.g.cur_model.unwrap();
            if c.model(m).typ == b'G' {
                c.g.n_g_models += 1;
            }
            // the model loop ran to the end of the list
            c.g.cur_model = None;
            post::dormer_feet(c)
        });
    }
}

/// S2/S3 of every model, each started from the emulator's state at the
/// first slab (`s1_end`) and compared with its state after the plane cut
/// (`after_cut`, taken at the cut error when there is one). The cut error
/// sites are tallied as pairs of the emulator's and the port's site names.
fn sweep_from_s1_end(name: &str, doc: &dump::DumpDoc, t: &mut Tallies) {
    let snaps = doc.snaps();
    for (k, (i, e)) in snaps
        .iter()
        .enumerate()
        .filter(|(_, s)| s["stage"] == "s1_end")
        .enumerate()
    {
        let label = format!("{name} model {k}");
        let Some(want) = snaps[i + 1..]
            .iter()
            .take_while(|s| s["stage"] != "s1_end")
            .find(|s| s["stage"] == "after_cut")
        else {
            continue;
        };
        let mut c = match dump::from_json(e) {
            Ok(c) => c,
            Err(err) => {
                tally(t, "sweep load", &label, vec![err.to_string()]);
                continue;
            }
        };
        let mut iters = 0u32;
        let res = dcore::sweep::sweep_with(&mut c, &mut |id, _| iters += u32::from(id == 0xa16));
        let e = t.entry("sweep max iterations".into()).or_default();
        if iters as usize > e.ok {
            e.ok = iters as usize;
        }
        let last = !snaps[i + 1..].iter().any(|s| s["stage"] == "s1_end");
        let emu = if last { emu_error(doc) } else { None };
        let mut d = Vec::new();
        match (&emu, &res) {
            (None, Ok(_)) => {}
            (Some((kind, code, id)), Err(dcore::CoreError::Cut { code: c2, msg }))
                if kind == "cut" =>
            {
                if (*code, *id) != (u64::from(*c2), msg.id) {
                    d.push(format!("emulator {code}/{id:x?}, port {c2}/{:x?}", msg.id));
                }
                let site = doc.doc["errors"][0]["site"]
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                tally(
                    t,
                    &format!("sweep site {site} = {}", dcore::sweep::last_cut_site()),
                    &label,
                    vec![],
                );
            }
            (e, r) => d.push(format!("emulator {e:?}, port {r:?}")),
        }
        d.extend(dump::compare(want, &dump::to_json(&c), 6));
        tally(t, "sweep after_cut", &label, d);
    }
}

fn first_diff(a: &[u8], b: &[u8]) -> String {
    let i = a
        .iter()
        .zip(b)
        .position(|(x, y)| x != y)
        .unwrap_or(a.len().min(b.len()));
    let ctx = |s: &[u8]| {
        String::from_utf8_lossy(&s[i.saturating_sub(40).min(s.len())..(i + 40).min(s.len())])
            .to_string()
    };
    format!("differs at byte {i}: want {:?} got {:?}", ctx(a), ctx(b))
}

/// The writers on the emulator's final state.
fn writers(name: &str, dir: &Path, doc: &dump::DumpDoc, t: &mut Tallies) {
    if let Some(s) = doc.stage("after_codes2").first() {
        let mut c = dump::from_json(s).unwrap();
        match out::write_sol(&mut c) {
            Ok(b) => {
                let want = std::fs::read(dir.join(format!("{name}.SO2"))).unwrap_or_default();
                let d = if b == want {
                    vec![]
                } else {
                    vec![first_diff(&want, &b)]
                };
                tally(t, "SO2 bytes", name, d);
                if let Some(w) = doc.stage("after_sol").first() {
                    tally(
                        t,
                        "SO2 state",
                        name,
                        dump::compare(w, &dump::to_json(&c), 6),
                    );
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
                let d = if b == want {
                    vec![]
                } else {
                    vec![first_diff(&want, &b)]
                };
                tally(t, "INT bytes", name, d);
                if let Some(w) = doc.stage("after_int").first() {
                    tally(
                        t,
                        "INT state",
                        name,
                        dump::compare(w, &dump::to_json(&c), 6),
                    );
                }
            }
            Err(e) => tally(t, "INT bytes", name, vec![e.to_string()]),
        }
    }
}

/// `DIR/state.json`, or the gzipped `DIR/state.json.gz` of the committed fixtures.
fn load_doc(dir: &Path) -> Option<dump::DumpDoc> {
    if let Ok(doc) = dump::DumpDoc::load(&dir.join("state.json")) {
        return Some(doc);
    }
    let gz = std::fs::File::open(dir.join("state.json.gz")).ok()?;
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut flate2::read::GzDecoder::new(gz), &mut bytes).ok()?;
    Some(dump::DumpDoc::from_slice(&bytes).unwrap())
}

fn run_case(rd2: &Path, dir: &Path, t: &mut Tallies) -> bool {
    let name = rd2.file_stem().unwrap().to_string_lossy().to_string();
    let Some(doc) = load_doc(dir) else {
        return false;
    };
    if let Ok(bytes) = std::fs::read(rd2) {
        run_from_rd2(&name, dir, &bytes, &doc, t);
    }
    s1_from_entry(&name, &doc, t);
    sweep_from_s1_end(&name, &doc, t);
    s4_slices(&name, &doc, t);
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
    // DCORE_REPORT=prefix,... limits the report to the tallies of some stages
    // (e.g. `s4` for dump sets made with only the S4 snapshots).
    if let Ok(p) = std::env::var("DCORE_REPORT") {
        t.retain(|k, _| p.split(',').any(|x| k.starts_with(x)));
    }
    assert!(!report(&t, 5));
}

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/dcore")
}

/// The committed cases: `tests/fixtures/dcore/CASE/{CASE.RD2,state.json.gz,CASE.SO2,CASE.INT}`.
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
        assert!(
            run_case(&dir.join(format!("{name}.RD2")), &dir, &mut t),
            "{name}"
        );
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

/// The S0 primitives against the emulator's results on random inputs
/// (`geom_vectors.py`: fn, input words, eax, output words).
fn geom_vectors(path: &Path) -> (usize, Vec<String>) {
    use dcore::geom::{self, Line, Plane};
    use dcore::num;
    let t = num::Tol::default();
    let mut bad = Vec::new();
    let mut n = 0;
    for line in std::fs::read_to_string(path).unwrap().lines() {
        let c: Vec<&str> = line.split('\t').collect();
        let words = |s: &str| -> Vec<u32> {
            s.split_whitespace()
                .map(|w| u32::from_str_radix(w, 16).unwrap())
                .collect()
        };
        let fl: Vec<f32> = words(c[1]).into_iter().map(f32::from_bits).collect();
        let eax: i64 = c[2].parse().unwrap();
        let out = words(c[3]);
        let v3 = |k: usize| -> [f32; 3] { [fl[k], fl[k + 1], fl[k + 2]] };
        let bits = |v: &[f32]| -> Vec<u32> { v.iter().map(|x| x.to_bits()).collect() };
        let plane = |k: usize| Plane {
            o: [0.0; 3],
            u: [0.0; 3],
            w: [0.0; 3],
            n: v3(k),
            d: fl[k + 3],
        };
        let ln = |k: usize| Line {
            p: v3(k),
            d: v3(k + 3),
        };
        let ok = match c[0] {
            "cross" => bits(&geom::cross(v3(0), v3(3))) == out,
            "norm" => {
                let r = num::norm(v3(0), &t);
                bits(&r) == out && (eax == 1) == !num::zero(num::len(v3(0)) as f32, t.value)
            }
            "par" => i64::from(num::par(v3(0), v3(3), &t)) == eax,
            "orient" => i64::from(num::orient(v3(0), v3(3), v3(6), &t)) == eax,
            "solve2" => {
                let r = num::solve2(&[fl[0], fl[1], fl[2], fl[3]], [fl[4], fl[5]], &t);
                match r {
                    Some(x) => eax == 0 && bits(&x) == out,
                    None => eax == -1,
                }
            }
            "solve3" => {
                let a: [f32; 9] = fl[..9].try_into().unwrap();
                match num::solve3(&a, v3(9), &t) {
                    Some(x) => eax == 0 && bits(&x) == out,
                    None => eax == -1,
                }
            }
            "horiz" => i64::from(geom::horizontal(&ln(0), &t)) == eax,
            "copl" => i64::from(geom::coplanar(&plane(0), &plane(4), &t)) == eax,
            "pp" => {
                let (r, l) = geom::plane_plane(&plane(0), &plane(4), &t);
                (eax == 1) == r.is_some() && bits(&[l.p, l.d].concat()) == out
            }
            "lp" => {
                let f = Plane {
                    o: v3(0),
                    u: v3(3),
                    w: v3(6),
                    n: [0.0; 3],
                    d: 0.0,
                };
                let r = geom::line_plane(&f, &ln(9), &t);
                (eax == 1) == r.is_ok() && bits(&r.unwrap_or_else(|p| p)) == out
            }
            "ll" | "ll_slab" => {
                let slab = (c[0] == "ll_slab").then(|| (fl[12], fl[13]));
                let (found, p) = geom::line_line(&ln(0), &ln(6), slab, &t);
                (eax == 1) == found && bits(&p) == out
            }
            other => panic!("unknown vector kind {other}"),
        };
        n += 1;
        if !ok {
            bad.push(line.to_string());
        }
    }
    (n, bad)
}

#[test]
fn geom_fixture_vectors() {
    let (n, bad) = geom_vectors(&fixture_dir().join("geom.tsv"));
    assert!(n > 0);
    assert!(bad.is_empty(), "{n} vectors, wrong: {bad:#?}");
}

#[test]
#[ignore]
fn geom_all_vectors() {
    let p = std::env::var("DCORE_GEOM").expect("DCORE_GEOM=file");
    let (n, bad) = geom_vectors(Path::new(&p));
    println!("{n} vectors, {} wrong", bad.len());
    for b in bad.iter().take(20) {
        println!("{b}");
    }
    assert!(bad.is_empty());
}

/// The S2/S3 segment primitives against the emulator (`seg_vectors.py`).
fn seg_vectors(path: &Path) -> (usize, Vec<String>) {
    use dcore::geom;
    use dcore::num;
    let t = num::Tol::default();
    let mut bad = Vec::new();
    let mut n = 0;
    for line in std::fs::read_to_string(path).unwrap().lines() {
        let c: Vec<&str> = line.split('\t').collect();
        let words = |s: &str| -> Vec<u32> {
            s.split_whitespace()
                .map(|w| u32::from_str_radix(w, 16).unwrap())
                .collect()
        };
        let fl: Vec<f32> = words(c[1]).into_iter().map(f32::from_bits).collect();
        let eax: i64 = c[2].parse().unwrap();
        let out = words(c[3]);
        let v3 = |k: usize| -> [f32; 3] { [fl[k], fl[k + 1], fl[k + 2]] };
        let ab = || ((v3(0), v3(3)), (v3(6), v3(9)));
        let ok = match c[0] {
            m if m.starts_with("segseg") => {
                let mode: u32 = m[6..].parse().unwrap();
                let (a, b) = ab();
                let (r, x) = geom::seg_seg(a, b, mode, &t);
                i64::from(r) == eax && x.map(f32::to_bits).to_vec() == out
            }
            "cjoin" => {
                let (a, b) = ab();
                i64::from(geom::collinear_join(a, b, &t)) == eax
            }
            "seg_on" => {
                let (a, b) = ab();
                i64::from(geom::seg_on_seg(a.0, a.1, b.0, b.1, &t)) == eax
            }
            "ovl" => {
                let (a, b) = ab();
                i64::from(geom::seg_overlap(a, b, &t)) == eax
            }
            "sdir" => i64::from(geom::same_dir_within(v3(0), v3(3), &t)) == eax,
            other => panic!("unknown vector kind {other}"),
        };
        n += 1;
        if !ok {
            bad.push(line.to_string());
        }
    }
    (n, bad)
}

#[test]
fn seg_fixture_vectors() {
    let (n, bad) = seg_vectors(&fixture_dir().join("geom_seg.tsv"));
    assert!(n > 0);
    assert!(bad.is_empty(), "{n} vectors, wrong: {bad:#?}");
}

#[test]
#[ignore]
fn seg_all_vectors() {
    let p = std::env::var("DCORE_SEG").expect("DCORE_SEG=file");
    let (n, bad) = seg_vectors(Path::new(&p));
    println!("{n} vectors, {} wrong", bad.len());
    for b in bad.iter().take(20) {
        println!("{b}");
    }
    assert!(bad.is_empty());
}

/// Runs the port on the RD2s in `DCORE_RD2` (space separated) and prints
/// each outcome (for cases without dumps, e.g. one the original never
/// finishes).
#[test]
#[ignore]
fn run_rd2() {
    let files = std::env::var("DCORE_RD2").expect("DCORE_RD2=files");
    let flags = std::env::var("DCORE_FLAGS")
        .ok()
        .map(|f| u32::from_str_radix(f.trim_start_matches("0x"), 16).unwrap())
        .unwrap_or(0x4a8);
    for f in files.split_whitespace() {
        let bytes = std::fs::read(f).unwrap();
        let t0 = std::time::Instant::now();
        let mut stages = Vec::new();
        let mut run = dcore::pipeline::Run::new(flags);
        run.hook = Some(Box::new(|stage: &str, _: &dcore::Core| {
            stages.push(stage.to_string())
        }));
        let r = run.run(&bytes);
        drop(run);
        println!(
            "{f}: {:?} after {:?}, stages {stages:?}",
            r.map(|o| o.code),
            t0.elapsed()
        );
    }
}
