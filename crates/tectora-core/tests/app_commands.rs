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
            Some(camel(name.trim()))
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
        for n in names {
            arms.entry(n.clone()).or_default().push_str(&body);
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
