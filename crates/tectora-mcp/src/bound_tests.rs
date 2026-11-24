//! Audit 2026-11-24 HUNT-1802: every roof calculation of an MCP tool stays
//! within a bound. A tool that snaps to the roof or needs a dormer's host
//! answers within the write's bound on a job too complex to calculate:
//! with a note (the lines saved as given, the items moved without following
//! the roof) or with the "takes too long" error; a refused argument answers
//! at once, before any calculation.

use super::*;
use std::time::{Duration, Instant};
use tectora_core::store::now_ms;

/// A job of 51 hip roofs stacked on one another (what `transform_part`
/// with 50 copies and no offset makes) with a chimney: within every input
/// limit, far too complex to calculate within a write. The commit timeout
/// is short, so the first calculation gives up at once and every later one
/// sees it still running.
fn heavy() -> (Server, String) {
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "tectora-mcp-bound-{}-{}-{n}",
        now_ms(),
        std::process::id()
    ));
    let mut s = Server::new(Store::new(dir).unwrap());
    s.commit_timeout = Duration::from_millis(50);
    let mut p = tectora_core::examples::find("hip").unwrap().project();
    let part = p.parts[0].clone();
    for _ in 0..50 {
        p.parts.push(part.clone());
    }
    assert!(p.parts.len() >= Server::HEAVY_PARTS);
    p.chimneys.push(tectora_core::model::Chimney {
        corners: [(2.0, 2.0), (2.6, 2.0), (2.6, 2.6), (2.0, 2.6)]
            .iter()
            .map(|&(x, y)| Point2::new(x, y))
            .collect(),
        height: 9.0,
    });
    p.check_limits().unwrap();
    let id = s.store.save(None, &p).unwrap();
    (s, id)
}

/// The answer of `name` and how long it took.
fn timed(s: &mut Server, name: &str, args: Value) -> (ToolResult, Duration) {
    let t = Instant::now();
    let r = s.call_tool(name, &args);
    (r, t.elapsed())
}

/// Well under the minutes a snap took before; the bound is the commit
/// timeout, the rest is the save of a 51-building job in a debug build.
const QUICK: Duration = Duration::from_secs(20);

#[test]
fn the_snapping_tools_answer_within_the_bound_on_a_job_too_complex_to_calculate() {
    let (mut s, id) = heavy();
    let saved = s.store.load(&id).unwrap();

    // set_manual with a line: saved as given, said.
    let (r, t) = timed(
        &mut s,
        "set_manual",
        json!({ "id": id, "summary": "line", "lines": [{ "a": [0, 0], "b": [12, 0], "code": "T" }] }),
    );
    let r = r.unwrap();
    assert!(t < QUICK, "set_manual took {t:?}");
    assert!(
        r["snap_note"].as_str().unwrap().contains("takes too long"),
        "{r}"
    );
    assert_eq!(s.store.load(&id).unwrap().manual_lines.len(), 1);

    // transform_items move (follow_roof by default): moved as given, said.
    let (r, t) = timed(
        &mut s,
        "transform_items",
        json!({ "id": id, "summary": "move", "items": [{ "kind": "chimney", "index": 0 }], "op": "move", "dx": 0.1, "dy": 0 }),
    );
    let r = r.unwrap();
    assert!(t < QUICK, "transform_items took {t:?}");
    assert!(
        r["follow_roof_note"]
            .as_str()
            .unwrap()
            .contains("takes too long"),
        "{r}"
    );
    let moved = s.store.load(&id).unwrap().chimneys[0].corners[0].x;
    assert!((moved - 2.1).abs() < 1e-9, "{moved}");

    // add_dormer and roof_helper need the roof: the error, nothing saved.
    let before = s.store.load(&id).unwrap();
    for (tool, args) in [
        (
            "add_dormer",
            json!({ "id": id, "summary": "dormer", "kind": "gable", "start": [3, 0] }),
        ),
        (
            "roof_helper",
            json!({ "id": id, "edge": 0, "tool": "point", "target": "pitch" }),
        ),
    ] {
        let (r, t) = timed(&mut s, tool, args);
        assert!(t < QUICK, "{tool} took {t:?}");
        let e = r.unwrap_err();
        assert!(e.contains("takes too long"), "{tool}: {e}");
    }
    assert_eq!(s.store.load(&id).unwrap(), before);
    let _ = saved;
    let _ = std::fs::remove_dir_all(&s.store.dir);
}

#[test]
fn a_refused_argument_answers_before_any_roof_calculation() {
    let (mut s, id) = heavy();
    let before = s.store.load(&id).unwrap();
    for (tool, args, want) in [
        (
            "set_manual",
            json!({ "id": id, "summary": "x", "faces": [{ "points": [[0, 0]] }],
                "lines": [{ "a": [0, 0], "b": [12, 0], "code": "T" }] }),
            "face 0",
        ),
        (
            "set_manual",
            json!({ "id": id, "summary": "x", "lines": [{ "a": [0], "b": [12, 0], "code": "T" }] }),
            "line 0",
        ),
        (
            "add_dormer",
            json!({ "id": id, "summary": "x", "kind": "round", "start": [3, 0], "segments": 1_000_000 }),
            "0 to 30",
        ),
        (
            "roof_helper",
            json!({ "id": id, "edge": 0, "tool": "point", "target": "pitch", "point": [1, 2] }),
            "[x, y, z]",
        ),
    ] {
        let (r, t) = timed(&mut s, tool, args);
        let e = r.unwrap_err();
        assert!(e.contains(want), "{tool}: {e}");
        // No calculation was started: the next one is not refused as busy.
        assert!(t < QUICK, "{tool} took {t:?}");
        assert!(
            !s.calc_busy.load(std::sync::atomic::Ordering::SeqCst),
            "{tool} calculated before refusing"
        );
    }
    assert_eq!(s.store.load(&id).unwrap(), before);
    let _ = std::fs::remove_dir_all(&s.store.dir);
}

/// Every `generate_project(` and `host_roof(` of the MCP server's own code
/// (not its tests) is in [`Server::roof_of`] / [`Server::host_roof_of`], in a
/// `work::calculate` closure, or in a tool that `worker::bounds_whole_call`
/// bounds as a whole; `roof_trouble` only through `export_ready`, which only
/// such tools call. A new unbounded call fails here.
#[test]
fn every_roof_calculation_of_a_tool_is_bounded() {
    let files = [
        ("lib.rs", include_str!("lib.rs")),
        ("more.rs", include_str!("more.rs")),
        ("drawings.rs", include_str!("drawings.rs")),
        ("worker.rs", include_str!("worker.rs")),
    ];
    // The code before the file's test modules.
    let code = |src: &'static str| -> Vec<&'static str> {
        let mut out = Vec::new();
        let lines: Vec<&str> = src.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            if lines[i].trim() == "#[cfg(test)]"
                && lines
                    .get(i + 1)
                    .is_some_and(|l| l.trim_start().starts_with("mod "))
            {
                // Skip to the module's closing brace at column 0 (or a `mod x;`).
                if lines[i + 1].trim_end().ends_with(';') {
                    i += 2;
                    continue;
                }
                i += 2;
                while i < lines.len() && lines[i] != "}" {
                    out.push("");
                    i += 1;
                }
                i += 1;
                continue;
            }
            out.push(lines[i]);
            i += 1;
        }
        out
    };
    let fn_of = |lines: &[&str], at: usize| -> String {
        lines[..=at]
            .iter()
            .rev()
            .find_map(|l| {
                let t = l.trim_start();
                let t = t
                    .strip_prefix("pub(crate) ")
                    .or(t.strip_prefix("pub "))
                    .unwrap_or(t);
                t.strip_prefix("fn ")
                    .map(|r| r.split(['(', '<']).next().unwrap_or("").to_string())
            })
            .unwrap_or_default()
    };
    // The tool arm (`"name" =>`) the line is in, when it is in one.
    let arm_of = |lines: &[&str], at: usize| -> Option<String> {
        lines[..=at].iter().rev().take(80).find_map(|l| {
            let t = l.trim_start();
            let rest = t.strip_prefix('"')?;
            let (name, tail) = rest.split_once('"')?;
            (tail.trim_start().starts_with("=>") && !name.contains(' ')).then(|| name.to_string())
        })
    };
    let mut bad = Vec::new();
    let mut calls = 0;
    for (file, src) in files {
        let lines = code(src);
        for (i, l) in lines.iter().enumerate() {
            let t = l.trim_start();
            if t.starts_with("//") {
                continue;
            }
            for what in [
                "generate_project(",
                "host_roof(",
                "roof_trouble(",
                "export_ready(",
            ] {
                if !l.contains(what) || l.contains(&format!("fn {what}")) {
                    continue;
                }
                calls += 1;
                let f = fn_of(&lines, i);
                let in_closure = lines[i.saturating_sub(8)..=i]
                    .iter()
                    .any(|l| l.contains("work::calculate("));
                let whole = arm_of(&lines, i).is_some_and(|a| worker::bounds_whole_call(&a));
                let ok = match what {
                    "generate_project(" | "host_roof(" => {
                        matches!(f.as_str(), "roof_of" | "host_roof_of" | "roof_trouble")
                            || in_closure
                            || whole
                    }
                    "roof_trouble(" => f == "export_ready",
                    _ => whole,
                };
                if !ok {
                    bad.push(format!("{file}:{} in fn {f}: {}", i + 1, t));
                }
            }
        }
    }
    assert!(
        calls >= 6,
        "the scan found {calls} calls: it no longer reads the code"
    );
    assert!(
        bad.is_empty(),
        "roof calculations outside the bound:\n{}",
        bad.join("\n")
    );
}
