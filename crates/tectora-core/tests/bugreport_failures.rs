//! Fix round 1, ERR-001 (target/work/bug-report/fix-r1.md): a part the user
//! ticked that could not be packed is named in `left_out` with a reason code,
//! and `included.*` says what was really packed. Own binary: the log folder
//! here is one that cannot be written.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tectora_core::bugreport::{self, Dest, Include, Request, Source};
use tectora_core::store::{Store, now_ms};

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "tectora-bugfail-{name}-{}-{}",
        std::process::id(),
        now_ms()
    ));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// Every test here: the log folder is below a plain file.
fn blocked_log(base: &Path) {
    let file = base.join("not-a-folder");
    fs::write(&file, "x").unwrap();
    unsafe { std::env::set_var("TECTORA_LOG_DIR", file.join("logs")) };
}

fn request(id: Option<&str>) -> Request {
    Request {
        description: "d".into(),
        job_id: id.map(str::to_string),
        include: Include {
            job: true,
            customer: true,
            files: true,
            history: true,
            snapshot: false,
            log: false,
        },
        source: Source::App,
        reporter: "user".into(),
        local_stamp: Some("20261128-143012".into()),
        client: json!({}),
        editor_project: None,
        plan_svg: None,
        model_png: None,
    }
}

fn read_zip(p: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut z = zip::ZipArchive::new(fs::File::open(p).unwrap()).unwrap();
    (0..z.len())
        .map(|i| {
            let mut f = z.by_index(i).unwrap();
            let mut b = Vec::new();
            f.read_to_end(&mut b).unwrap();
            (f.name().unwrap().to_string(), b)
        })
        .collect()
}

fn made(store: &Store, req: &Request, out: &Path) -> (Value, Value, BTreeMap<String, Vec<u8>>) {
    let c = bugreport::create(store, req, &Dest::Folder(out.to_path_buf())).unwrap();
    let z = read_zip(&c.path);
    let m: Value = serde_json::from_slice(&z["manifest.json"]).unwrap();
    let r: Value = serde_json::from_slice(&z["report.json"]).unwrap();
    (m, r, z)
}

fn has_left_out(m: &Value, part: &str) -> Option<String> {
    m["left_out"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["part"] == part)
        .map(|e| {
            e["reason"]
                .as_str()
                .unwrap_or_else(|| panic!("no reason: {e}"))
                .to_string()
        })
}

#[test]
fn a_ticked_log_that_cannot_be_read_is_left_out_and_not_claimed() {
    let d = tmp("log");
    blocked_log(&d);
    let store = Store::new(d.join("lib")).unwrap();
    let out = d.join("out");
    fs::create_dir_all(&out).unwrap();
    let mut req = request(None);
    req.include.log = true;
    let (m, r, z) = made(&store, &req, &out);
    assert!(has_left_out(&m, "log").is_some(), "{m}");
    assert_eq!(r["included"]["log"], false);
    assert!(!z.keys().any(|k| k.starts_with("logs/")));
    let _ = fs::remove_dir_all(d);
}

#[cfg(unix)]
#[test]
fn drawings_that_cannot_be_packed_are_left_out_and_not_claimed() {
    let d = tmp("files");
    blocked_log(&d);
    let store = Store::new(d.join("lib")).unwrap();
    let out = d.join("out");
    fs::create_dir_all(&out).unwrap();
    let p = tectora_core::examples::find("hip").unwrap().project();
    let id = store.save(None, &p).unwrap();
    store
        .add_attachment(&id, "plan.pdf", b"%PDF-1.4 x")
        .unwrap();
    // The job's drawings folder becomes a link: the library refuses it.
    let real = d.join("elsewhere");
    fs::create_dir_all(&real).unwrap();
    fs::write(real.join("plan.pdf"), b"%PDF-1.4 x").unwrap();
    let folder = store.dir.join(".files").join(&id);
    fs::remove_dir_all(&folder).unwrap();
    std::os::unix::fs::symlink(&real, &folder).unwrap();
    let (m, r, z) = made(&store, &request(Some(&id)), &out);
    assert!(has_left_out(&m, "files").is_some(), "{m}");
    assert_eq!(r["included"]["files"], false);
    assert!(!z.keys().any(|k| k.starts_with("job/files/")));
    assert!(
        r["job"]["attachments"]
            .as_array()
            .unwrap()
            .iter()
            .all(|a| a["included"] != true),
        "{r}"
    );
    let _ = fs::remove_dir_all(d);
}

#[test]
fn more_than_200_drawings_are_left_out_with_too_many() {
    let d = tmp("many-files");
    blocked_log(&d);
    let store = Store::new(d.join("lib")).unwrap();
    let out = d.join("out");
    fs::create_dir_all(&out).unwrap();
    let p = tectora_core::examples::find("hip").unwrap().project();
    let id = store.save(None, &p).unwrap();
    for i in 0..201 {
        store
            .add_attachment(&id, &format!("f{i}.pdf"), b"%PDF-1.4")
            .unwrap();
    }
    let (m, r, z) = made(&store, &request(Some(&id)), &out);
    assert_eq!(
        has_left_out(&m, "files").as_deref(),
        Some("too_many"),
        "{m}"
    );
    assert_eq!(r["included"]["files"], false);
    assert!(!z.keys().any(|k| k.starts_with("job/files/")));
    // The rest of the report is there.
    assert!(z.contains_key("job/project.tectora"));
    let _ = fs::remove_dir_all(d);
}

#[test]
fn a_change_log_that_cannot_be_read_is_left_out_and_not_claimed() {
    let d = tmp("history");
    blocked_log(&d);
    let store = Store::new(d.join("lib")).unwrap();
    let out = d.join("out");
    fs::create_dir_all(&out).unwrap();
    let p = tectora_core::examples::find("hip").unwrap().project();
    let id = store.save(None, &p).unwrap();
    let log = store.activity_dir().join(format!("{id}.jsonl"));
    let _ = fs::remove_file(&log);
    fs::create_dir_all(&log).unwrap();
    let (m, r, z) = made(&store, &request(Some(&id)), &out);
    assert!(has_left_out(&m, "history").is_some(), "{m}");
    assert_eq!(r["included"]["history"], false);
    assert!(!z.contains_key("job/history.jsonl"));
    let _ = fs::remove_dir_all(d);
}
