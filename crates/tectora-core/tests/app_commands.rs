//! The browser preview's bridge (`examples/ui_bridge.rs`) must answer every
//! Tauri command of the desktop app (`app/src-tauri/src/lib.rs`) under the
//! same name and read the same arguments, and the UI may only call commands
//! the app has: the end-to-end tests drive the UI through the bridge, so a
//! command missing there, renamed or reading another argument would pass
//! every E2E run and fail in the shipped app (TEST-008). The app crate needs
//! WebKitGTK and is not built in the Linux CI container, so this reads the
//! sources.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

fn read(rel: &str) -> String {
    let p = Path::new(ROOT).join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// Commands of the bridge that exist only as stubs there: they need the
/// operating system (open a file or folder, install an update, the file the
/// app was started with), so the bridge answers without reading arguments.
const PREVIEW_STUBS: &[&str] = &[
    "open_attachment",
    "open_folder",
    "show_bug_report",
    "update_install",
    "take_pending_open",
    "startup_warnings",
];

/// The command names in `tauri::generate_handler![...]`, without module paths.
fn tauri_commands(lib: &str) -> Vec<String> {
    let start = lib
        .find("generate_handler![")
        .expect("generate_handler! in lib.rs")
        + 18;
    let end = start + lib[start..].find(']').expect("end of generate_handler!");
    lib[start..end]
        .split(',')
        .map(|s| s.trim().rsplit("::").next().unwrap_or("").to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// The parameters of `fn name(...)` that come from the UI (not the app's
/// state or handle), in the camelCase the UI sends them as.
fn ui_params(src: &str, name: &str) -> Vec<String> {
    let at = [format!("fn {name}("), format!("fn {name}<")]
        .iter()
        .find_map(|p| src.find(p.as_str()))
        .unwrap_or_else(|| panic!("fn {name} not found"));
    let open = at + src[at..].find('(').unwrap() + 1;
    let mut depth = 1;
    let mut close = open;
    for (i, ch) in src[open..].char_indices() {
        match ch {
            '(' | '<' | '[' => depth += 1,
            ')' | '>' | ']' if depth > 1 => depth -= 1,
            ')' => {
                close = open + i;
                break;
            }
            _ => {}
        }
    }
    // Split at top-level commas.
    let mut params = Vec::new();
    let (mut depth, mut cur) = (0, String::new());
    for ch in src[open..close].chars() {
        match ch {
            '(' | '<' | '[' => depth += 1,
            ')' | '>' | ']' => depth -= 1,
            ',' if depth == 0 => {
                params.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        cur.push(ch);
    }
    params.push(cur);
    params
        .iter()
        .filter_map(|p| {
            let (name, ty) = p.split_once(':')?;
            let ty = ty.trim();
            if ty.contains("State<") || ty.contains("AppHandle") || ty.contains("Window") {
                return None;
            }
            Some(camel(name.trim().trim_start_matches("mut ").trim()))
        })
        .collect()
}

fn camel(s: &str) -> String {
    let mut out = String::new();
    let mut up = false;
    for ch in s.chars() {
        if ch == '_' {
            up = true;
        } else if up {
            out.extend(ch.to_uppercase());
            up = false;
        } else {
            out.push(ch);
        }
    }
    out
}

/// The bridge's match arms by command name: the text of every arm that
/// names it (a command can be answered in more than one function).
fn bridge_arms(src: &str) -> BTreeMap<String, String> {
    let lines: Vec<&str> = src.lines().collect();
    let names_of = |line: &str| -> Option<Vec<String>> {
        let t = line.trim();
        let (head, _) = t.split_once("=>")?;
        let names: Option<Vec<String>> = head
            .split('|')
            .map(|n| {
                let n = n.trim();
                let inner = n.strip_prefix('"')?.strip_suffix('"')?;
                inner
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '_')
                    .then(|| inner.to_string())
            })
            .collect();
        names.filter(|n| !n.is_empty())
    };
    let starts: Vec<(usize, Vec<String>)> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| names_of(l).map(|n| (i, n)))
        .collect();
    let mut arms: BTreeMap<String, String> = BTreeMap::new();
    for (k, (i, names)) in starts.iter().enumerate() {
        let end = starts.get(k + 1).map_or(lines.len(), |s| s.0);
        // An arm ends at the next arm or where its match ends.
        let body: String = lines[*i..end]
            .iter()
            .take_while(|l| !l.trim_start().starts_with("_ =>"))
            .copied()
            .collect::<Vec<_>>()
            .join("\n");
        // Arguments read before the `match` of the enclosing function (e.g.
        // `store_command` reads `id` once) count for each of its arms.
        let prologue: String = lines[..*i]
            .iter()
            .rposition(|l| l.trim_start().starts_with("fn ") || l.contains(" fn "))
            .map(|f| {
                let m = (f..*i).find(|&j| lines[j].contains("match ")).unwrap_or(*i);
                lines[f..m].join("\n")
            })
            .unwrap_or_default();
        for n in names {
            let arm = arms.entry(n.clone()).or_default();
            arm.push_str(&prologue);
            arm.push_str(&body);
        }
    }
    arms
}

fn ui_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("app/src").flatten() {
        let p = entry.path();
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if p.is_dir() {
            if name != "dev" {
                ui_sources(&p, out);
            }
        } else if (name.ends_with(".ts") || name.ends_with(".svelte"))
            && !name.ends_with(".test.ts")
        {
            out.push(p);
        }
    }
}

/// Command names the UI passes to `invoke`.
fn invoked(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = src;
    while let Some(i) = rest.find("invoke") {
        rest = &rest[i + 6..];
        // invoke("name" or invoke<T>("name"
        let Some(open) = rest.find("(\"") else { break };
        let between = &rest[..open];
        let generic = between.starts_with('<') && between.ends_with('>') && !between.contains('\n');
        if !(between.is_empty() || generic) {
            continue;
        }
        let name: String = rest[open + 2..]
            .chars()
            .take_while(|c| c.is_ascii_lowercase() || *c == '_')
            .collect();
        if rest[open + 2 + name.len()..].starts_with('"') && !name.is_empty() {
            out.push(name);
        }
    }
    out
}

#[test]
fn the_bridge_answers_every_tauri_command_with_the_same_arguments() {
    let lib = read("app/src-tauri/src/lib.rs");
    let updates = read("app/src-tauri/src/updates.rs");
    let bridge = read("crates/tectora-core/examples/ui_bridge.rs");
    let commands = tauri_commands(&lib);
    assert!(commands.len() > 30, "{commands:?}");
    let arms = bridge_arms(&bridge);

    let missing: Vec<&String> = commands.iter().filter(|c| !arms.contains_key(*c)).collect();
    assert!(
        missing.is_empty(),
        "Tauri commands the bridge does not answer: {missing:?}"
    );
    let tauri: BTreeSet<&String> = commands.iter().collect();
    let extra: Vec<&String> = arms.keys().filter(|c| !tauri.contains(c)).collect();
    assert!(
        extra.is_empty(),
        "bridge commands the app does not have: {extra:?}"
    );

    let mut wrong = Vec::new();
    for c in &commands {
        if PREVIEW_STUBS.contains(&c.as_str()) {
            continue;
        }
        let src = if lib.contains(&format!("fn {c}(")) {
            &lib
        } else {
            &updates
        };
        let arm = &arms[c];
        for p in ui_params(src, c) {
            let read = arm.contains(&format!("\"{p}\"")) || arm.contains(&format!("{p}()"));
            if !read {
                wrong.push(format!("{c}: argument {p}"));
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "arguments of Tauri commands the bridge does not read: {wrong:?}"
    );
}

#[test]
fn the_ui_calls_only_commands_the_app_has() {
    let commands: BTreeSet<String> = tauri_commands(&read("app/src-tauri/src/lib.rs"))
        .into_iter()
        .collect();
    let mut files = Vec::new();
    ui_sources(&Path::new(ROOT).join("app/src"), &mut files);
    let mut calls = BTreeSet::new();
    for f in &files {
        let src = std::fs::read_to_string(f).unwrap();
        for name in invoked(&src) {
            calls.insert((name, f.display().to_string()));
        }
    }
    assert!(
        calls.iter().any(|(n, _)| n == "calc"),
        "found no invoke(\"calc\") in app/src"
    );
    let unknown: Vec<_> = calls
        .iter()
        .filter(|(n, _)| !commands.contains(n))
        .collect();
    assert!(
        unknown.is_empty(),
        "the UI invokes commands the app does not register: {unknown:?}"
    );
}

#[test]
fn the_parser_reads_parameters() {
    let src = "fn a(state: State<AppState>, id: String, ts_ms: u64, job: Option<tectora_core::JobInfo>) -> X {}";
    assert_eq!(ui_params(src, "a"), vec!["id", "tsMs", "job"]);
    assert_eq!(
        invoked("invoke<Project>(\"load_project\", { id })"),
        vec!["load_project"]
    );
    assert_eq!(invoked("await invoke(\"calc\", { project })"), vec!["calc"]);
}

/// Audit 2026-11-24 HUNT-1903: no app command (and no command of the
/// preview's bridge) calculates a roof outside the work bound: every
/// calculation goes through `tectora_core::commands` (bounded with
/// `work::calculate`), a store call that bounds its own figures
/// (`review_status`, `refresh_result`), or a `work::calculate` closure. A
/// direct call of the kernel would keep the app's command busy for minutes
/// on a job too complex to calculate.
#[test]
fn no_app_command_calculates_a_roof_outside_the_work_bound() {
    let direct = [
        "generate_project(",
        "host_roof(",
        "snapshot::take(",
        "take_checked(",
        "change_in_figures(",
        "breakdown::breakdown(",
        "report::calc_of(",
        // Placing by distances calculates the bare roof (HUNT-2001): only
        // through `commands::item_placement` / `commands::place_item`.
        "position::place(",
        "position::placement(",
        "position::placements(",
        "bare_roof(",
    ];
    let mut bad = Vec::new();
    for file in [
        "app/src-tauri/src/lib.rs",
        "crates/tectora-core/examples/ui_bridge.rs",
    ] {
        let src = read(file);
        let lines: Vec<&str> = src.lines().collect();
        for (i, l) in lines.iter().enumerate() {
            let t = l.trim_start();
            if t.starts_with("//") {
                continue;
            }
            for what in direct {
                if !l.contains(what) || l.contains(&format!("fn {what}")) {
                    continue;
                }
                let bounded = lines[i.saturating_sub(8)..=i]
                    .iter()
                    .any(|l| l.contains("work::calculate("));
                if !bounded {
                    bad.push(format!("{file}:{}: {t}", i + 1));
                }
            }
        }
        // The scan reads the commands: the review read is one of them.
        assert!(src.contains("review_status"), "{file}: no review_status");
    }
    assert!(
        bad.is_empty(),
        "roof calculations outside the bound:\n{}",
        bad.join("\n")
    );
}

// ---- bug reports go through the relay (plan-delivery D11, ADR 0008) ----

const REPORT_COMMANDS: [&str; 7] = [
    "create_bug_report",
    "bug_report_name",
    "show_bug_report",
    "bug_report_outbox",
    "send_bug_reports",
    "save_queued_bug_report",
    "discard_bug_report",
];

#[test]
fn the_report_commands_are_in_the_app_and_answered_by_the_bridge() {
    let lib = read("app/src-tauri/src/lib.rs");
    let bridge = read("crates/tectora-core/examples/ui_bridge.rs");
    let commands = tauri_commands(&lib);
    let arms = bridge_arms(&bridge);
    for c in REPORT_COMMANDS {
        assert!(
            commands.iter().any(|x| x == c),
            "{c} is not in generate_handler!"
        );
        assert!(arms.contains_key(c), "the bridge does not answer {c}");
        assert!(lib.contains(&format!("fn {c}(")), "no fn {c}");
    }
}

#[test]
fn the_report_commands_that_use_the_network_or_the_disk_run_off_the_main_thread() {
    let lib = read("app/src-tauri/src/lib.rs");
    for c in [
        "create_bug_report",
        "bug_report_outbox",
        "send_bug_reports",
        "save_queued_bug_report",
        "discard_bug_report",
    ] {
        let at = lib
            .find(&format!("fn {c}("))
            .unwrap_or_else(|| panic!("no fn {c}"));
        let before: Vec<&str> = lib[..at].lines().rev().take(3).collect();
        assert!(
            before
                .iter()
                .any(|l| l.contains("#[tauri::command(async)]")),
            "{c} must be #[tauri::command(async)], saw {before:?}"
        );
    }
}

#[test]
fn the_reports_folder_is_gone_from_the_app_and_the_bridge() {
    let lib = read("app/src-tauri/src/lib.rs");
    let bridge = read("crates/tectora-core/examples/ui_bridge.rs");
    let at = lib.find("fn open_folder(").expect("fn open_folder");
    let body = &lib[at..at + lib[at..].find("\n}\n").expect("end of open_folder")];
    assert!(
        !body.contains("\"reports\""),
        "open_folder still accepts \"reports\""
    );
    for (name, src) in [("lib.rs", &lib), ("ui_bridge.rs", &bridge)] {
        assert!(
            !src.contains("reports_dir"),
            "{name} still mentions reports_dir"
        );
        assert!(!src.contains("ensure_library_reports"), "{name}");
    }
}

// ---- fix round 2 (target/work/bug-report/fix-r2.md) ----

/// The text of `fn name` up to the closing brace at column 0 (or the next
/// item); the sources are read, not compiled (WebKitGTK).
fn fn_text(src: &str, name: &str) -> String {
    let at = src
        .find(&format!("fn {name}("))
        .unwrap_or_else(|| panic!("no fn {name}"));
    let rest = &src[at..];
    let end = rest.find("\n}\n").map_or(rest.len(), |e| e + 3);
    rest[..end].to_string()
}

/// ERR-002: Send in the dialog and Send now never wait behind the
/// background drain: `try_lock`, and the report is queued as waiting.
#[test]
fn sending_a_report_never_blocks_behind_the_background_drain() {
    let lib = read("app/src-tauri/src/lib.rs");
    for c in ["create_bug_report", "send_bug_reports"] {
        let body = fn_text(&lib, c);
        assert!(
            body.contains("try_lock"),
            "{c} must use try_lock on the drain lock"
        );
        assert!(
            !body.contains("drain_lock.lock()"),
            "{c} waits for the drain lock with a blocking lock()"
        );
    }
}

/// ERR-007: the background drain survives a poisoned lock and a panic in one
/// pass, and a thread that cannot start is logged.
#[test]
fn the_background_drain_survives_a_poisoned_lock_a_panic_and_a_failed_spawn() {
    let lib = read("app/src-tauri/src/lib.rs");
    let body = fn_text(&lib, "start_report_drain");
    assert!(
        body.contains("Poisoned") || body.contains("into_inner"),
        "a poisoned drain lock ends the thread's work for good"
    );
    assert!(
        body.contains("catch_unwind"),
        "a panic in one pass ends the thread"
    );
    assert!(
        !body.contains("let _ = std::thread::Builder"),
        "a spawn failure is dropped without a log line"
    );
    assert!(
        body.contains("applog"),
        "nothing is logged by the drain thread"
    );
}

/// ERR-001 / UI-003: `failed` reaches the UI as `failed`, in the app and in
/// the preview bridge, never as `waiting`.
#[test]
fn a_failed_report_is_reported_as_failed_by_the_app_and_the_bridge() {
    let lib = read("app/src-tauri/src/lib.rs");
    let bridge = read("crates/tectora-core/examples/ui_bridge.rs");
    let app = fn_text(&lib, "create_bug_report");
    assert!(
        app.contains("\"failed\"")
            || lib.contains("fn result_state")
                && fn_text(&lib, "result_state").contains("\"failed\""),
        "create_bug_report maps `failed` to something else"
    );
    assert!(
        fn_text(&bridge, "report_state").contains("\"failed\""),
        "the bridge's report_state maps `failed` to `waiting`"
    );
}

/// ARCH-002: the preview and the end-to-end run never use the user's own
/// outbox (their reports would be `local` there and could be sent later).
#[test]
fn the_preview_bridge_and_the_e2e_use_a_scratch_config_folder() {
    let preview = read("app/scripts/preview.mjs");
    assert!(
        preview.contains("TECTORA_CONFIG_DIR"),
        "preview.mjs does not set a scratch TECTORA_CONFIG_DIR for the bridge"
    );
    let serve = read("app/e2e/serve.mjs");
    assert!(serve.contains("TECTORA_CONFIG_DIR"));
}
