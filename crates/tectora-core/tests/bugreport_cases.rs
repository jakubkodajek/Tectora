//! Bug report ZIP (`tectora_core::bugreport`), plan `target/work/bug-report`,
//! acceptance criteria 1-7 and the properties of criterion 5. Written before
//! the module exists: this file does not compile until `bugreport` and
//! `applog` are public modules of the core with the API sketched in the plan.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Once;

use proptest::prelude::*;
use serde_json::{Value, json};
use tectora_core::bugreport::{self, Dest, Include, Request, Source};
use tectora_core::settings::Settings;
use tectora_core::store::{Store, now_ms};
use tectora_core::{applog, store};

const CUSTOMER: &str = "Kunde-ZQX-91";
const ADDRESS: &str = "Adresa-ZQX-47";

fn tmp(name: &str) -> PathBuf {
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!(
        "tectora-bugreport-{name}-{}-{}-{n}",
        std::process::id(),
        now_ms()
    ));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// One log folder for every test of this binary (set before any report).
fn log_dir() -> PathBuf {
    static ONCE: Once = Once::new();
    let dir = std::env::temp_dir().join(format!("tectora-bugreport-logs-{}", std::process::id()));
    ONCE.call_once(|| {
        fs::create_dir_all(&dir).unwrap();
        // Set once, before a report reads it; no test changes it again.
        unsafe { std::env::set_var("TECTORA_LOG_DIR", &dir) };
        applog::init("app");
    });
    dir
}

fn all() -> Include {
    Include {
        job: true,
        customer: true,
        files: true,
        history: true,
        snapshot: true,
        log: true,
    }
}

fn png() -> Vec<u8> {
    let mut b = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    b.extend_from_slice(&[0; 64]);
    b
}

fn request(job_id: Option<&str>) -> Request {
    Request {
        description: "The roof is flat after I moved the wall".into(),
        job_id: job_id.map(str::to_string),
        include: all(),
        source: Source::App,
        reporter: "user".into(),
        local_stamp: Some("20261128-143012".into()),
        client: json!({
            "ui": { "locale": "cs", "theme": "dark", "screen": "editor", "layout": "split",
                    "window": [1440, 900], "webgl": true },
            "actions": [{ "ts_ms": 1, "kind": "command", "name": "place_item", "ok": true, "code": null, "ms": 12 }],
            "notices": [{ "ts_ms": 2, "kind": "error", "code": "io", "title": "t", "detail": "d" }],
        }),
        editor_project: None,
        plan_svg: Some("<svg xmlns=\"http://www.w3.org/2000/svg\"><rect/></svg>".into()),
        model_png: Some(png()),
    }
}

/// A job with a customer, an address, two drawings and a change log whose
/// versions name the customer too.
fn job(store: &Store) -> String {
    let mut p = tectora_core::examples::find("hip").unwrap().project();
    p.name = "Novák dům".into();
    p.job.customer = CUSTOMER.into();
    p.job.address = ADDRESS.into();
    let id = store.save(None, &p).unwrap();
    p.parts[0].edges[0].pitch_deg = 33.0;
    store
        .save_as(Some(id.clone()), &p, "AI", "steeper")
        .unwrap();
    store
        .add_attachment(&id, "pudorys.pdf", b"%PDF-1.4 a")
        .unwrap();
    store.add_attachment(&id, "rez.pdf", b"%PDF-1.4 b").unwrap();
    id
}

fn read_zip(path: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut z = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
    let mut out = BTreeMap::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let mut b = Vec::new();
        f.read_to_end(&mut b).unwrap();
        out.insert(f.name().unwrap().to_string(), b);
    }
    out
}

fn json_of(z: &BTreeMap<String, Vec<u8>>, name: &str) -> Value {
    serde_json::from_slice(
        z.get(name)
            .unwrap_or_else(|| panic!("no {name} in {:?}", z.keys())),
    )
    .unwrap()
}

fn contains(hay: &[u8], needle: &str) -> bool {
    hay.windows(needle.len()).any(|w| w == needle.as_bytes())
}

fn left_out(z: &BTreeMap<String, Vec<u8>>) -> String {
    json_of(z, "manifest.json")["left_out"]
        .to_string()
        .to_lowercase()
}

fn make(store: &Store, req: &Request, dir: &Path) -> BTreeMap<String, Vec<u8>> {
    log_dir();
    let c = bugreport::create(store, req, &Dest::Folder(dir.to_path_buf())).unwrap();
    assert_eq!(c.path.parent(), Some(dir));
    read_zip(&c.path)
}

fn files_in(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

// ------------------------------------------------------------ criterion 1

#[test]
fn a_report_has_exactly_the_planned_entries_and_the_job_loads_back() {
    let dir = tmp("entries");
    let store = Store::new(dir.join("lib")).unwrap();
    let id = job(&store);
    let out = dir.join("out");
    fs::create_dir_all(&out).unwrap();
    log_dir();
    applog::line("INFO", "marker-before-report-1");
    let z = make(&store, &request(Some(&id)), &out);

    let names: Vec<&str> = z.keys().map(String::as_str).collect();
    for must in [
        "manifest.json",
        "report.json",
        "job/project.tectora",
        "job/history.jsonl",
        "job/preview.svg",
        "job/files/pudorys.pdf",
        "job/files/rez.pdf",
        "snapshot/plan.svg",
        "snapshot/model.png",
        "logs/app.log",
    ] {
        assert!(names.contains(&must), "{must} missing from {names:?}");
    }
    let allowed = ["job/editor-project.tectora", "logs/mcp.log"];
    for n in &names {
        assert!(
            [
                "manifest.json",
                "report.json",
                "job/project.tectora",
                "job/history.jsonl",
                "job/preview.svg",
                "job/files/pudorys.pdf",
                "job/files/rez.pdf",
                "snapshot/plan.svg",
                "snapshot/model.png",
                "logs/app.log"
            ]
            .contains(n)
                || allowed.contains(n),
            "unexpected entry {n}"
        );
    }
    // The manifest lists what is in the ZIP, with the sizes.
    let m = json_of(&z, "manifest.json");
    assert_eq!(m["format"], "tectora-bug-report");
    assert_eq!(m["format_version"], 1);
    assert_eq!(m["source"], "app");
    let listed: BTreeMap<String, u64> = m["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["name"].as_str().unwrap().to_string(),
                e["size"].as_u64().unwrap(),
            )
        })
        .collect();
    for (name, bytes) in &z {
        if name == "manifest.json" {
            continue;
        }
        assert_eq!(listed.get(name), Some(&(bytes.len() as u64)), "{name}");
    }
    assert!(listed.keys().all(|n| z.contains_key(n)), "{listed:?}");
    // The job file is the job.
    let back =
        tectora_core::Project::from_json(std::str::from_utf8(&z["job/project.tectora"]).unwrap())
            .unwrap();
    assert_eq!(back, store.load(&id).unwrap());
    assert_eq!(z["job/files/pudorys.pdf"], b"%PDF-1.4 a");
    // The log tail has the line logged just before (criterion 8).
    assert!(contains(&z["logs/app.log"], "marker-before-report-1"));
    // report.json
    let r = json_of(&z, "report.json");
    assert_eq!(r["description"], "The roof is flat after I moved the wall");
    assert_eq!(r["source"], "app");
    assert_eq!(r["reporter"], "user");
    assert_eq!(r["job"]["id"], id.as_str());
    assert_eq!(r["job"]["load_error"], Value::Null);
    assert_eq!(r["job"]["anonymized"], false);
    assert_eq!(r["job"]["attachments"].as_array().unwrap().len(), 2);
    assert_eq!(r["ui"]["locale"], "cs");
    assert_eq!(r["actions"][0]["name"], "place_item");
    assert!(r["calculation"]["ok"].is_boolean());
    // The editor's own copy only when it differs from the file.
    assert!(!z.contains_key("job/editor-project.tectora"));
    let mut req = request(Some(&id));
    req.editor_project = Some(String::from_utf8(z["job/project.tectora"].clone()).unwrap());
    assert!(!make(&store, &req, &out).contains_key("job/editor-project.tectora"));
    let mut edited = store.load(&id).unwrap();
    edited.parts[0].edges[0].pitch_deg = 21.0;
    req.editor_project = Some(edited.to_json().unwrap());
    assert!(make(&store, &req, &out).contains_key("job/editor-project.tectora"));
    let _ = fs::remove_dir_all(dir);
}

// ------------------------------------------------------------ criterion 2

#[test]
fn leaving_the_customer_out_blanks_name_and_address_everywhere() {
    let dir = tmp("customer");
    let store = Store::new(dir.join("lib")).unwrap();
    let id = job(&store);
    let out = dir.join("out");
    fs::create_dir_all(&out).unwrap();
    let mut edited = store.load(&id).unwrap();
    edited.parts[0].edges[0].pitch_deg = 21.0;
    let editor_json = edited.to_json().unwrap();

    let mut req = request(Some(&id));
    req.editor_project = Some(editor_json.clone());
    // With the customer: present.
    let z = make(&store, &req, &out);
    for name in [
        "job/project.tectora",
        "job/editor-project.tectora",
        "job/history.jsonl",
    ] {
        assert!(contains(&z[name], CUSTOMER), "{name} lost the customer");
        assert!(contains(&z[name], ADDRESS), "{name} lost the address");
    }
    // Without: nowhere in the four files, and the history still loads.
    req.include.customer = false;
    let z = make(&store, &req, &out);
    for name in [
        "job/project.tectora",
        "job/editor-project.tectora",
        "job/history.jsonl",
        "report.json",
    ] {
        assert!(!contains(&z[name], CUSTOMER), "{name} names the customer");
        assert!(!contains(&z[name], ADDRESS), "{name} names the address");
    }
    let blank =
        tectora_core::Project::from_json(std::str::from_utf8(&z["job/project.tectora"]).unwrap())
            .unwrap();
    assert_eq!(
        (blank.job.customer.as_str(), blank.job.address.as_str()),
        ("", "")
    );
    // Only those two fields change.
    let mut expect = store.load(&id).unwrap();
    expect.job.customer.clear();
    expect.job.address.clear();
    assert_eq!(blank, expect);
    assert_eq!(json_of(&z, "report.json")["job"]["anonymized"], true);
    assert_eq!(json_of(&z, "report.json")["included"]["customer"], false);
    for line in std::str::from_utf8(&z["job/history.jsonl"])
        .unwrap()
        .lines()
    {
        serde_json::from_str::<Value>(line).unwrap();
    }
    let _ = fs::remove_dir_all(dir);
}

// ------------------------------------------------------------ criterion 3

#[test]
fn each_include_flag_leaves_its_entries_out_and_the_report_says_so() {
    let dir = tmp("flags");
    let store = Store::new(dir.join("lib")).unwrap();
    let id = job(&store);
    let out = dir.join("out");
    fs::create_dir_all(&out).unwrap();
    type Off = fn(&mut Include);
    let cases: [(&str, Off, &str); 5] = [
        ("files", |i| i.files = false, "job/files/"),
        ("history", |i| i.history = false, "job/history.jsonl"),
        ("snapshot", |i| i.snapshot = false, "snapshot/"),
        ("log", |i| i.log = false, "logs/"),
        ("job", |i| i.job = false, "job/"),
    ];
    for (flag, off, prefix) in cases {
        let mut req = request(Some(&id));
        off(&mut req.include);
        let z = make(&store, &req, &out);
        assert!(
            !z.keys().any(|k| k.starts_with(prefix)),
            "{flag}: {:?}",
            z.keys().collect::<Vec<_>>()
        );
        // The other groups stay.
        assert!(z.contains_key("report.json") && z.contains_key("manifest.json"));
        let r = json_of(&z, "report.json");
        for (k, v) in r["included"].as_object().unwrap() {
            assert_eq!(v, &json!(k != flag), "{flag}: included.{k}");
        }
    }
    let mut req = request(Some(&id));
    req.include.job = false;
    let z = make(&store, &req, &out);
    assert!(!z.keys().any(|k| k.starts_with("job/")));
    assert!(
        z.keys().any(|k| k.starts_with("snapshot/")),
        "snapshot is its own flag"
    );
    let _ = fs::remove_dir_all(dir);
}

// ------------------------------------------------------------ criterion 4

#[test]
fn a_broken_or_missing_job_or_none_still_gives_a_report() {
    let dir = tmp("broken");
    let store = Store::new(dir.join("lib")).unwrap();
    let out = dir.join("out");
    fs::create_dir_all(&out).unwrap();
    fs::write(store.dir.join("kaput.tectora"), "{ not json").unwrap();
    let z = make(&store, &request(Some("kaput")), &out);
    let r = json_of(&z, "report.json");
    assert!(
        r["job"]["load_error"].is_string() || r["job"]["load_error"].is_object(),
        "{r}"
    );
    assert_eq!(z["job/project.tectora"], b"{ not json");
    assert_eq!(r["calculation"]["ok"], false);
    // A job id that does not exist.
    let z = make(&store, &request(Some("neni-tu")), &out);
    let r = json_of(&z, "report.json");
    assert!(!r["job"].is_null());
    assert!(!r["job"]["load_error"].is_null());
    assert!(!z.contains_key("job/project.tectora"));
    // No job.
    let z = make(&store, &request(None), &out);
    assert!(json_of(&z, "report.json")["job"].is_null());
    assert!(!z.keys().any(|k| k.starts_with("job/")));
    assert!(z.contains_key("logs/app.log"));
    // An empty description is fine.
    let mut req = request(None);
    req.description = String::new();
    assert_eq!(
        json_of(&make(&store, &req, &out), "report.json")["description"],
        ""
    );
    let _ = fs::remove_dir_all(dir);
}

// ------------------------------------------------------------ criterion 5

const REFUSAL_CODES: [&str; 8] = [
    "too_large",
    "invalid_input",
    "invalid_name",
    "io",
    "permission_denied",
    "not_found",
    "file_not_found",
    "not_a_folder",
];

#[test]
fn a_report_that_cannot_be_written_is_refused_with_a_code_and_writes_nothing() {
    let dir = tmp("refuse");
    let store = Store::new(dir.join("lib")).unwrap();
    let id = job(&store);
    let out = dir.join("out");
    fs::create_dir_all(&out).unwrap();
    log_dir();
    let refused = |req: &Request, dest: Dest| {
        let e = bugreport::create(&store, req, &dest)
            .err()
            .expect("must be refused");
        assert!(
            REFUSAL_CODES.contains(&e.code),
            "code {} ({})",
            e.code,
            e.detail
        );
    };
    // A 5001-character description (5000 is fine).
    let mut req = request(Some(&id));
    req.description = "x".repeat(5001);
    refused(&req, Dest::Folder(out.clone()));
    req.description = "x".repeat(5000);
    bugreport::create(&store, &req, &Dest::Folder(out.clone())).unwrap();
    let before = files_in(&out);
    let req = request(Some(&id));
    // Relative, missing, and a file in place of the folder.
    refused(&req, Dest::Folder(PathBuf::from("reports")));
    refused(&req, Dest::Folder(dir.join("missing-drive-folder")));
    assert!(
        !dir.join("missing-drive-folder").exists(),
        "a missing folder is not created"
    );
    let file = dir.join("a-file");
    fs::write(&file, "x").unwrap();
    refused(&req, Dest::Folder(file.clone()));
    // Dest::File must be a plain .zip name outside the library's own folders.
    refused(&req, Dest::File(out.join("report.txt")));
    refused(&req, Dest::File(out.join("report")));
    for inside in [".activity", "files", ".trash"] {
        let d = store.dir.join(inside);
        fs::create_dir_all(&d).unwrap();
        refused(&req, Dest::File(d.join("r.zip")));
        assert!(files_in(&d).iter().all(|n| n != "r.zip"), "{inside}");
    }
    assert_eq!(files_in(&out), before, "nothing was written");
    assert!(files_in(&out).iter().all(|n| !n.ends_with(".part")));
    // Dest::File works and names the file.
    let target = out.join("mine.zip");
    let c = bugreport::create(&store, &req, &Dest::File(target.clone())).unwrap();
    assert_eq!(c.path, target);
    assert!(target.is_file());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn oversize_or_wrong_parts_are_dropped_and_named_but_the_report_is_written() {
    let dir = tmp("trim");
    let store = Store::new(dir.join("lib")).unwrap();
    let id = job(&store);
    let out = dir.join("out");
    fs::create_dir_all(&out).unwrap();

    // plan.svg: over 4 MB, or not starting with <svg.
    for svg in [
        format!("<svg>{}</svg>", "a".repeat(4 * 1024 * 1024 + 1)),
        "<html>not a plan</html>".to_string(),
    ] {
        let mut req = request(Some(&id));
        req.plan_svg = Some(svg);
        let z = make(&store, &req, &out);
        assert!(!z.contains_key("snapshot/plan.svg"));
        assert!(z.contains_key("snapshot/model.png"));
        assert!(left_out(&z).contains("plan"), "{}", left_out(&z));
    }
    // model.png: over 8 MB, or not a PNG.
    let mut big = png();
    big.resize(8 * 1024 * 1024 + 1, 0);
    for bytes in [big, b"GIF89a....".to_vec()] {
        let mut req = request(Some(&id));
        req.model_png = Some(bytes);
        let z = make(&store, &req, &out);
        assert!(!z.contains_key("snapshot/model.png"));
        assert!(z.contains_key("snapshot/plan.svg"));
        assert!(left_out(&z).contains("model"), "{}", left_out(&z));
    }
    // 60 actions and 40 notices keep the newest 50 and 30, in order.
    let mut req = request(Some(&id));
    req.client = json!({
        "actions": (0..60).map(|i| json!({ "ts_ms": i, "kind": "command", "name": format!("op{i}"), "ok": true, "ms": 1 })).collect::<Vec<_>>(),
        "notices": (0..40).map(|i| json!({ "ts_ms": i, "kind": "error", "code": "io", "title": format!("t{i}"), "detail": "d" })).collect::<Vec<_>>(),
    });
    let r = json_of(&make(&store, &req, &out), "report.json");
    let actions = r["actions"].as_array().unwrap();
    assert_eq!(actions.len(), 50);
    assert_eq!(
        (actions[0]["name"].as_str(), actions[49]["name"].as_str()),
        (Some("op10"), Some("op59"))
    );
    let notices = r["notices"].as_array().unwrap();
    assert_eq!(notices.len(), 30);
    assert_eq!(notices[0]["title"], "t10");
    // Strings are cut (300; detail 2000); unknown keys are not copied; arguments never.
    req.client = json!({
        "secret_extra": "must-not-appear-zq",
        "actions": [{ "ts_ms": 1, "kind": "command", "name": "n".repeat(900), "ok": true, "ms": 1,
                      "args": "must-not-appear-zq" }],
        "notices": [{ "ts_ms": 1, "kind": "error", "code": "io", "title": "t".repeat(900), "detail": "d".repeat(5000) }],
    });
    let z = make(&store, &req, &out);
    let r = json_of(&z, "report.json");
    assert!(r["actions"][0]["name"].as_str().unwrap().chars().count() <= 300);
    assert!(r["notices"][0]["title"].as_str().unwrap().chars().count() <= 300);
    assert!(r["notices"][0]["detail"].as_str().unwrap().chars().count() <= 2000);
    assert!(!contains(&z["report.json"], "must-not-appear-zq"));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn control_and_bidi_characters_never_reach_the_report() {
    let dir = tmp("control");
    let store = Store::new(dir.join("lib")).unwrap();
    let out = dir.join("out");
    fs::create_dir_all(&out).unwrap();
    let mut req = request(None);
    req.description = "a\u{202e}b\u{0007}c\u{1b}[31md\u{2066}e\u{200b}f\nsecond line".into();
    req.reporter = "cli\u{202e}ent\u{0000}".into();
    req.client = json!({
        "ui": { "locale": "c\u{202e}s\u{0007}" },
        "actions": [{ "ts_ms": 1, "kind": "command", "name": "pl\u{202e}ace\u{0007}", "ok": true, "ms": 1 }],
        "notices": [{ "ts_ms": 1, "kind": "error", "code": "io", "title": "t\u{202e}", "detail": "d\u{0007}\u{2029}" }],
    });
    let z = make(&store, &req, &dir.join("out"));
    let text = String::from_utf8(z["report.json"].clone()).unwrap();
    // serde_json escapes controls as \uXXXX, so look at the parsed strings.
    fn walk(v: &Value, bad: &mut Vec<String>) {
        match v {
            Value::String(s) => {
                for c in s.chars() {
                    let forbidden = (c.is_control() && c != '\n')
                        || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{200b}' | '\u{2028}' | '\u{2029}');
                    if forbidden {
                        bad.push(format!("{c:?} in {s:?}"));
                    }
                }
            }
            Value::Array(a) => a.iter().for_each(|x| walk(x, bad)),
            Value::Object(o) => o.values().for_each(|x| walk(x, bad)),
            _ => {}
        }
    }
    let mut bad = Vec::new();
    walk(&serde_json::from_str(&text).unwrap(), &mut bad);
    assert!(bad.is_empty(), "{bad:?}");
    let _ = fs::remove_dir_all(dir);
}

// ------------------------------------------------------------ criterion 6

#[test]
fn a_report_leaves_no_part_file_and_never_replaces_one_that_was_there() {
    let dir = tmp("atomic");
    let store = Store::new(dir.join("lib")).unwrap();
    let id = job(&store);
    let out = dir.join("out");
    fs::create_dir_all(&out).unwrap();
    log_dir();
    let req = request(Some(&id));
    // Two reports in the same second get two names.
    let a = bugreport::create(&store, &req, &Dest::Folder(out.clone())).unwrap();
    let b = bugreport::create(&store, &req, &Dest::Folder(out.clone())).unwrap();
    assert_ne!(a.path, b.path);
    assert!(a.path.is_file() && b.path.is_file());
    // A file already under the next name stays as it is.
    let c_name = bugreport::file_name(Some("Novák dům"), "20261128-143012");
    let stem = c_name.trim_end_matches(".zip");
    for n in ["", "-2", "-3", "-4"] {
        let p = out.join(format!("{stem}{n}.zip"));
        if !p.exists() {
            fs::write(&p, b"precious").unwrap();
        }
    }
    let c = bugreport::create(&store, &req, &Dest::Folder(out.clone())).unwrap();
    for n in ["", "-2", "-3", "-4"] {
        let p = out.join(format!("{stem}{n}.zip"));
        if p != c.path && p != a.path && p != b.path {
            assert_eq!(fs::read(&p).unwrap(), b"precious", "{p:?} was replaced");
        }
    }
    assert!(
        files_in(&out)
            .iter()
            .all(|n| !n.ends_with(".part") && !n.starts_with(".tectora-report"))
    );
    // A refused report leaves no ZIP and no part file.
    let before = files_in(&out);
    let mut bad = request(Some(&id));
    bad.description = "x".repeat(5001);
    assert!(bugreport::create(&store, &bad, &Dest::Folder(out.clone())).is_err());
    assert_eq!(files_in(&out), before);
    #[cfg(unix)]
    {
        // A link under the next free name is not written through.
        let victim = dir.join("victim.txt");
        fs::write(&victim, "keep").unwrap();
        let d = tmp("atomic-link");
        let name = bugreport::file_name(Some("Novák dům"), "20261128-143012");
        std::os::unix::fs::symlink(&victim, d.join(&name)).unwrap();
        let c = bugreport::create(&store, &req, &Dest::Folder(d.clone())).unwrap();
        assert_ne!(c.path, d.join(&name));
        assert_eq!(fs::read(&victim).unwrap(), b"keep");
        let _ = fs::remove_dir_all(d);
    }
    let _ = fs::remove_dir_all(dir);
}

// ------------------------------------------------------------ criterion 7

#[test]
fn file_names_are_plain_ascii_whatever_the_job_is_called() {
    let long = "d".repeat(300);
    let names = [
        Some("Novák dům"),
        Some("Příliš žluťoučký kůň"),
        Some("a/b\\c"),
        Some("..."),
        Some("..."),
        Some("CON"),
        Some("NUL.txt"),
        Some("😀 roof 😀"),
        Some("   "),
        Some(long.as_str()),
        Some(""),
        None,
    ];
    for n in names {
        let f = bugreport::file_name(n, "20261128-143012");
        assert!(store::valid_file_name(&f), "{f}");
        assert!(f.len() <= 255, "{f}");
        let rest = f
            .strip_prefix("tectora-report-20261128-143012-")
            .unwrap_or_else(|| panic!("{f}"));
        let rest = rest.strip_suffix(".zip").unwrap_or_else(|| panic!("{f}"));
        // slug of 1 to 40 chars of [a-z0-9-], then an optional -<number>.
        let slug = match rest.rsplit_once('-') {
            Some((s, num))
                if !s.is_empty() && num.chars().all(|c| c.is_ascii_digit()) && s.len() >= 1 =>
            {
                // `-2`-style suffixes are only added by `create`; a plain
                // name may still end in a number of the slug itself.
                rest
            }
            _ => rest,
        };
        assert!(
            (1..=45).contains(&slug.len())
                && slug
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "{f}"
        );
    }
    assert_eq!(
        bugreport::file_name(Some("Novák dům"), "20261128-143012"),
        "tectora-report-20261128-143012-novak-dum.zip"
    );
    assert_eq!(
        bugreport::file_name(None, "20261128-143012"),
        "tectora-report-20261128-143012-app.zip"
    );
    let long_slug = bugreport::file_name(Some(&long), "20261128-143012");
    assert!(long_slug.len() <= "tectora-report-20261128-143012-".len() + 40 + ".zip".len());
}

// ------------------------------------------------------------ folder and list

#[test]
fn the_reports_folder_is_the_setting_else_a_folder_in_the_library() {
    let dir = tmp("folder");
    let store = Store::new(dir.join("lib")).unwrap();
    assert_eq!(
        bugreport::reports_dir(&Settings::default(), &store),
        store.dir.join("reports")
    );
    let own = dir.join("drive");
    let s: Settings = serde_json::from_value(json!({ "reports_dir": own })).unwrap();
    assert_eq!(bugreport::reports_dir(&s, &store), own);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn reports_are_listed_newest_first_and_read_back_only_by_plain_name() {
    let dir = tmp("list");
    let store = Store::new(dir.join("lib")).unwrap();
    let id = job(&store);
    let out = dir.join("out");
    fs::create_dir_all(&out).unwrap();
    log_dir();
    let mut older = request(Some(&id));
    older.local_stamp = Some("20261101-080000".into());
    older.description = "older".into();
    let a = bugreport::create(&store, &older, &Dest::Folder(out.clone())).unwrap();
    let mut newer = request(Some(&id));
    newer.local_stamp = Some("20261128-143012".into());
    newer.description = "newer".into();
    let b = bugreport::create(&store, &newer, &Dest::Folder(out.clone())).unwrap();
    // Things that are not reports.
    fs::write(out.join("notes.zip"), b"PK not a report").unwrap();
    fs::write(out.join("tectora-report-20260101-000000-fake.zip"), b"junk").unwrap();
    fs::write(out.join("readme.txt"), "x").unwrap();
    let listed = bugreport::list(&out, 10);
    assert!(listed.len() >= 2 && listed.len() <= 3, "{}", listed.len());
    let text = serde_json::to_string(&listed).unwrap();
    assert!(
        text.find(&b.file_name).unwrap() < text.find(&a.file_name).unwrap(),
        "newest first: {text}"
    );
    assert!(!text.contains("readme.txt") && !text.contains("notes.zip"));
    assert_eq!(bugreport::list(&out, 1).len(), 1);
    assert!(bugreport::list(&dir.join("nowhere"), 10).is_empty());
    // Reading.
    let v = bugreport::read(&out, &b.file_name).unwrap();
    assert_eq!(v["manifest"]["format"], "tectora-bug-report");
    assert_eq!(v["report"]["description"], "newer");
    assert_eq!(v["report"]["job"]["id"], id.as_str());
    for bad in [
        "../out/x.zip",
        "notes.zip",
        "tectora-report-20260101-000000-fake.zip",
        "tectora-report-x.txt",
        "missing.zip",
        "",
        "/etc/passwd",
    ] {
        assert!(bugreport::read(&out, bad).is_err(), "{bad}");
    }
    // A ZIP that is a ZIP with the right name but no manifest is no report.
    let fake = out.join("tectora-report-20260102-000000-zip.zip");
    {
        let mut w = zip::ZipWriter::new(fs::File::create(&fake).unwrap());
        w.start_file("hello.txt", zip::write::SimpleFileOptions::default())
            .unwrap();
        std::io::Write::write_all(&mut w, b"hi").unwrap();
        w.finish().unwrap();
    }
    assert!(bugreport::read(&out, "tectora-report-20260102-000000-zip.zip").is_err());
    let _ = fs::remove_dir_all(dir);
}

// ------------------------------------------------------------ properties

fn any_json() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::from),
        any::<i64>().prop_map(Value::from),
        any::<f64>().prop_map(|f| json!(f)),
        ".{0,40}".prop_map(Value::from),
        "[\\u{0}-\\u{1f}\\u{200b}-\\u{202e}]{0,12}".prop_map(Value::from),
    ];
    leaf.prop_recursive(4, 64, 8, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..8).prop_map(Value::Array),
            prop::collection::vec(("(ui|actions|notices|settings|x)", inner), 0..6)
                .prop_map(|kv| Value::Object(kv.into_iter().collect())),
        ]
    })
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 24,
        rng_seed: proptest::test_runner::RngSeed::Fixed(20261128),
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    /// Whatever the UI (or a hostile caller) sends, nothing panics and the
    /// report is a valid ZIP with a `report.json` inside the limits.
    #[test]
    fn any_client_json_gives_a_valid_report_or_a_refusal(
        client in any_json(),
        description in ".{0,200}",
        svg in proptest::option::of(".{0,60}"),
        png_bytes in proptest::option::of(prop::collection::vec(any::<u8>(), 0..64)),
    ) {
        let dir = tmp("prop");
        let store = Store::new(dir.join("lib")).unwrap();
        let out = dir.join("out");
        fs::create_dir_all(&out).unwrap();
        log_dir();
        let mut req = request(None);
        req.client = client;
        req.description = description;
        req.plan_svg = svg;
        req.model_png = png_bytes;
        let made = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            bugreport::create(&store, &req, &Dest::Folder(out.clone()))
        }));
        prop_assert!(made.is_ok(), "create panicked");
        if let Ok(c) = made.unwrap() {
            let z = read_zip(&c.path);
            let r: Value = serde_json::from_slice(&z["report.json"]).unwrap();
            prop_assert!(r["actions"].as_array().is_none_or(|a| a.len() <= 50));
            prop_assert!(r["notices"].as_array().is_none_or(|a| a.len() <= 30));
            prop_assert!(r["description"].as_str().unwrap_or("").chars().count() <= 5000);
            prop_assert!(z["report.json"].len() < 512 * 1024);
        }
        prop_assert!(files_in(&out).iter().all(|n| !n.ends_with(".part")));
        let _ = fs::remove_dir_all(dir);
    }
}
