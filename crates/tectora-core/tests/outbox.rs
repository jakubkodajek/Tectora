//! The report outbox (`tectora_core::outbox`), plan-delivery D1-D7 and the
//! core part of D16 (ADR 0008). Written before the module exists: this file
//! does not compile until `outbox` is a public module with the API sketched in
//! the plan, plus the test-side additions listed under "Test-side API" in
//! `target/work/bug-report/plan-delivery.md` (`Limits`, `enqueue_with`).
//!
//! No network and no sleeps: the sender is a closure, time is a parameter.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, Once};
use std::time::Duration;

use proptest::prelude::*;
use serde_json::{Value, json};
use tectora_core::applog;
use tectora_core::bugreport::{self, Dest, Include, Manifest, Request, Source};
use tectora_core::outbox::{self, Limits, Outbox, Receipt, SendFailure};
use tectora_core::store::{Store, now_ms};

const CUSTOMER: &str = "Jan Novák";
const ADDRESS: &str = "Adresa-ZQX-47";

fn tmp(name: &str) -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!(
        "tectora-outbox-{name}-{}-{}-{n}",
        std::process::id(),
        now_ms()
    ));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// One log folder and one config folder for every test of this binary, set
/// before anything reads them.
fn env() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let base = std::env::temp_dir().join(format!("tectora-outbox-env-{}", std::process::id()));
        fs::create_dir_all(base.join("logs")).unwrap();
        fs::create_dir_all(base.join("cfg")).unwrap();
        unsafe {
            std::env::set_var("TECTORA_LOG_DIR", base.join("logs"));
            std::env::set_var("TECTORA_CONFIG_DIR", base.join("cfg"));
        }
        applog::init("app");
    });
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

fn bare() -> Include {
    Include {
        job: false,
        customer: true,
        files: false,
        history: false,
        snapshot: false,
        log: false,
    }
}

fn request(job_id: Option<&str>, stamp: &str, include: Include) -> Request {
    Request {
        description: format!("The roof is flat after I moved the wall {stamp}"),
        job_id: job_id.map(str::to_string),
        include,
        source: Source::App,
        reporter: "user".into(),
        local_stamp: Some(stamp.into()),
        client: json!({
            "ui": { "locale": "cs", "theme": "dark", "screen": "editor", "layout": "split",
                    "window": [1440, 900], "webgl": true },
            "actions": [{ "ts_ms": 1, "kind": "command", "name": "ACTION-NAME-ZQX", "ok": true, "code": null, "ms": 12 }],
            "notices": [{ "ts_ms": 2, "kind": "error", "code": "io", "title": "t", "detail": "NOTICE-DETAIL-ZQX" }],
        }),
        editor_project: None,
        plan_svg: None,
        model_png: None,
    }
}

/// A distinct, valid `YYYYMMDD-HHMMSS` per `n` (up to 1440).
fn stamp(n: usize) -> String {
    format!("20261128-{:02}{:02}00", n / 60, n % 60)
}

/// Deterministic bytes that do not compress.
fn noise(len: usize, seed: u64) -> Vec<u8> {
    let mut x = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x >> 24) as u8
        })
        .collect()
}

fn png(len: usize) -> Vec<u8> {
    let mut b = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    b.extend(noise(len, 9));
    b
}

fn job_with(store: &Store, name: &str, attachment_len: usize, saves: usize) -> String {
    let mut p = tectora_core::examples::find("hip").unwrap().project();
    p.name = name.into();
    p.job.customer = CUSTOMER.into();
    p.job.address = ADDRESS.into();
    let id = store.save(None, &p).unwrap();
    for i in 0..saves {
        p.parts[0].edges[0].pitch_deg = 20.0 + i as f64 * 0.25;
        let why: String = noise(150, i as u64)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        store.save_as(Some(id.clone()), &p, "AI", &why).unwrap();
    }
    store
        .add_attachment(
            &id,
            "pudorys.pdf",
            &[b"%PDF-1.4 ".to_vec(), noise(attachment_len, 1)].concat(),
        )
        .unwrap();
    store
        .add_attachment(
            &id,
            "rez.pdf",
            &[b"%PDF-1.4 ".to_vec(), noise(attachment_len, 2)].concat(),
        )
        .unwrap();
    id
}

fn setup(name: &str) -> (PathBuf, Store, Outbox) {
    env();
    let d = tmp(name);
    let store = Store::new(d.join("lib")).unwrap();
    let ob = Outbox::at(&d.join("reports"));
    (d, store, ob)
}

fn files_in(dir: &Path) -> BTreeSet<String> {
    match fs::read_dir(dir) {
        Ok(rd) => rd
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect(),
        Err(_) => BTreeSet::new(),
    }
}

fn zip_path(root: &Path, file_name: &str) -> PathBuf {
    root.join("outbox").join(file_name)
}
fn sidecar_path(root: &Path, file_name: &str) -> PathBuf {
    zip_path(root, file_name).with_extension("json")
}
fn lock_path(root: &Path, file_name: &str) -> PathBuf {
    zip_path(root, file_name).with_extension("lock")
}
fn sidecar(root: &Path, file_name: &str) -> Value {
    serde_json::from_slice(&fs::read(sidecar_path(root, file_name)).unwrap()).unwrap()
}
fn write_sidecar(root: &Path, file_name: &str, v: &Value) {
    fs::write(
        sidecar_path(root, file_name),
        serde_json::to_vec(v).unwrap(),
    )
    .unwrap();
}

fn root_of(d: &Path) -> PathBuf {
    d.join("reports")
}

fn ok_receipt() -> Result<Receipt, SendFailure> {
    Ok(Receipt {
        file_id: "FILE1".into(),
        issue_url: Some("https://github.com/o/r/issues/7".into()),
    })
}

fn is_hex32(s: &str) -> bool {
    s.len() == 32 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn state_of(ob: &Outbox, file_name: &str) -> (String, Option<String>) {
    let l = serde_json::to_value(ob.list(100).unwrap()).unwrap();
    let r = l["reports"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["file_name"] == file_name)
        .unwrap_or_else(|| panic!("{file_name} not listed: {l}"))
        .clone();
    (
        r["state"].as_str().unwrap().to_string(),
        r["last_error"].as_str().map(str::to_string),
    )
}

const T0: u64 = 2_000_000_000_000;

// =================================================================== D1

#[test]
fn enqueue_writes_a_valid_report_and_a_sidecar_and_nothing_else() {
    let (d, store, ob) = setup("d1");
    let id = job_with(&store, "Novák dům", 2000, 1);
    let q = ob
        .enqueue(&store, &request(Some(&id), "20261128-143012", all()), false)
        .unwrap();
    let root = root_of(&d);
    let dir = root.join("outbox");

    let names = files_in(&dir);
    let zip_name = q.file_name.clone();
    let side = sidecar_path(&root, &zip_name);
    let expect: BTreeSet<String> = [
        zip_name.clone(),
        side.file_name().unwrap().to_string_lossy().to_string(),
    ]
    .into();
    assert_eq!(names, expect, "no .part, temp or lock file is left");
    assert!(bugreport::is_report_name(&zip_name));

    let r = bugreport::read(&dir, &zip_name).expect("bugreport::read accepts the queued ZIP");
    let entries: Vec<String> = r["manifest"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap().to_string())
        .collect();
    assert!(
        entries.contains(&"job/files/pudorys.pdf".to_string()),
        "{entries:?}"
    );
    assert!(
        entries.contains(&"job/files/rez.pdf".to_string()),
        "{entries:?}"
    );

    let s = sidecar(&root, &zip_name);
    assert_eq!(s["v"], 1);
    assert!(is_hex32(s["id"].as_str().unwrap()), "{s}");
    assert_eq!(s["id"], q.id);
    assert_eq!(s["state"], "waiting");
    assert_eq!(s["attempts"], 0);
    assert_eq!(s["file_name"], zip_name);
    assert_eq!(
        s["size"],
        fs::metadata(zip_path(&root, &zip_name)).unwrap().len()
    );
    assert!(s["queued_ms"].as_u64().unwrap() > 0);
    assert!(s["last_error"].is_null());
    assert!(s["meta"].is_object());
    assert_eq!(ob.counts().waiting, 1);
}

#[test]
fn a_refused_report_leaves_the_outbox_empty() {
    let (d, store, ob) = setup("d1-fail");
    let mut req = request(None, "20261128-143012", bare());
    req.description = "x".repeat(bugreport::MAX_DESCRIPTION + 1);
    let e = ob.enqueue(&store, &req, false).unwrap_err();
    assert!(!e.code.is_empty());
    assert!(
        files_in(&root_of(&d).join("outbox")).is_empty(),
        "no leftovers"
    );
    assert_eq!(ob.counts().waiting, 0);
}

#[test]
fn every_queued_report_gets_its_own_random_id() {
    let (_d, store, ob) = setup("d1-ids");
    let mut ids = BTreeSet::new();
    for n in 0..5 {
        let q = ob
            .enqueue(&store, &request(None, &stamp(n), bare()), false)
            .unwrap();
        assert!(is_hex32(&q.id), "{}", q.id);
        ids.insert(q.id);
    }
    assert_eq!(ids.len(), 5);
}

#[test]
fn a_build_without_a_relay_keeps_reports_as_local_and_counts_none_as_waiting() {
    let (d, store, ob) = setup("d1-local");
    let q = ob
        .enqueue(&store, &request(None, "20261128-143012", bare()), true)
        .unwrap();
    assert_eq!(sidecar(&root_of(&d), &q.file_name)["state"], "local");
    let c = ob.counts();
    assert_eq!((c.waiting, c.failed, c.local), (0, 0, 1));
    assert_eq!(state_of(&ob, &q.file_name).0, "local");
}

#[test]
fn the_install_id_is_eight_hex_chars_and_the_same_for_every_report() {
    let (d, store, ob) = setup("d1-install");
    let a = ob
        .enqueue(&store, &request(None, &stamp(1), bare()), false)
        .unwrap();
    let b = ob
        .enqueue(&store, &request(None, &stamp(2), bare()), false)
        .unwrap();
    let root = root_of(&d);
    let ia = sidecar(&root, &a.file_name)["meta"]["install"].clone();
    let ib = sidecar(&root, &b.file_name)["meta"]["install"].clone();
    let s = ia.as_str().expect("meta.install");
    assert!(
        s.len() == 8 && s.bytes().all(|c| c.is_ascii_hexdigit()),
        "{s}"
    );
    assert_eq!(ia, ib);
}

// =================================================================== D2

#[test]
fn fitting_drops_drawings_then_history_then_the_3d_picture() {
    let (d, store, ob) = setup("d2");
    let id = job_with(&store, "Fit-Dum", 300_000, 60);
    let mk = |inc: Include| {
        let mut r = request(Some(&id), "20261128-143012", inc);
        r.model_png = Some(png(400_000));
        r
    };
    let full = Include {
        job: true,
        customer: true,
        files: true,
        history: true,
        snapshot: true,
        log: false,
    };
    let measure = |inc: Include| -> u64 {
        let o = Outbox::at(&tmp("d2-measure").join("reports"));
        o.enqueue(&store, &mk(inc), false).unwrap().size
    };
    let s_full = measure(full);
    let s_nf = measure(Include {
        files: false,
        ..full
    });
    let s_nfnh = measure(Include {
        files: false,
        history: false,
        ..full
    });
    let s_none = measure(Include {
        files: false,
        history: false,
        snapshot: false,
        ..full
    });
    assert!(
        s_full > s_nf + 500_000,
        "setup: drawings must dominate {s_full} {s_nf}"
    );
    assert!(
        s_nf > s_nfnh + 3000,
        "setup: history must be sizeable {s_nf} {s_nfnh}"
    );
    assert!(
        s_nfnh > s_none + 300_000,
        "setup: the picture must be sizeable {s_nfnh} {s_none}"
    );
    let margin = 1500;

    let run = |cap: u64, stamp: &str| {
        let mut r = mk(full);
        r.local_stamp = Some(stamp.into());
        ob.enqueue_with(
            &store,
            &r,
            false,
            &Limits {
                max_upload: cap,
                ..Limits::default()
            },
        )
    };
    let left = |q: &outbox::Queued| -> Vec<(String, String)> {
        q.left_out
            .iter()
            .map(|l| (l.part.clone(), l.reason.clone()))
            .collect()
    };

    // Drawings only.
    let q = run(s_nf + margin, "20261128-100000").unwrap();
    assert_eq!(
        left(&q),
        vec![("files".to_string(), "too_large".to_string())]
    );
    assert!(q.size <= s_nf + margin);
    let read = bugreport::read(&root_of(&d).join("outbox"), &q.file_name).unwrap();
    assert_eq!(read["report"]["included"]["files"], false);
    assert_eq!(read["report"]["included"]["history"], true);
    assert!(
        fs::metadata(zip_path(&root_of(&d), &q.file_name))
            .unwrap()
            .len()
            <= s_nf + margin
    );

    // Drawings and history.
    let q = run(s_nfnh + margin, "20261128-100100").unwrap();
    assert_eq!(
        left(&q),
        vec![
            ("files".to_string(), "too_large".to_string()),
            ("history".to_string(), "too_large".to_string())
        ]
    );
    let read = bugreport::read(&root_of(&d).join("outbox"), &q.file_name).unwrap();
    assert_eq!(read["report"]["included"]["files"], false);
    assert_eq!(read["report"]["included"]["history"], false);

    // Drawings, history and the 3D picture.
    let q = run(s_none + margin, "20261128-100200").unwrap();
    assert_eq!(
        left(&q).iter().map(|l| l.0.as_str()).collect::<Vec<_>>(),
        vec!["files", "history", "model"]
    );
    assert!(left(&q).iter().all(|l| l.1 == "too_large"));
    let read = bugreport::read(&root_of(&d).join("outbox"), &q.file_name).unwrap();
    let manifest_left = read["manifest"]["left_out"].to_string();
    assert!(manifest_left.contains("model"), "{manifest_left}");
}

#[test]
fn a_report_that_cannot_fit_is_too_large_and_leaves_the_outbox_empty() {
    let (d, store, ob) = setup("d2-no");
    let id = job_with(&store, "Fit-Dum", 5000, 1);
    let e = ob
        .enqueue_with(
            &store,
            &request(Some(&id), "20261128-143012", all()),
            false,
            &Limits {
                max_upload: 1,
                ..Limits::default()
            },
        )
        .unwrap_err();
    assert_eq!(e.code, "too_large");
    assert!(files_in(&root_of(&d).join("outbox")).is_empty());
}

#[test]
fn attachments_over_the_cap_alone_skip_the_first_build_and_a_small_report_is_untouched() {
    let (_d, store, ob) = setup("d2-att");
    let id = job_with(&store, "Fit-Dum", 200_000, 1);
    let inc = Include {
        log: false,
        snapshot: false,
        ..all()
    };
    let q = ob
        .enqueue_with(
            &store,
            &request(Some(&id), "20261128-143012", inc),
            false,
            &Limits {
                max_upload: 150_000,
                ..Limits::default()
            },
        )
        .unwrap();
    assert!(
        q.left_out
            .iter()
            .any(|l| l.part == "files" && l.reason == "too_large")
    );
    assert!(q.size <= 150_000);
    let q = ob
        .enqueue(&store, &request(Some(&id), "20261128-143013", inc), false)
        .unwrap();
    assert!(
        q.left_out.is_empty(),
        "under the real cap nothing is dropped: {:?}",
        q.left_out
    );
}

// =================================================================== D3

#[test]
fn a_sent_report_is_removed_and_logged_once() {
    let (d, store, ob) = setup("d3");
    let q = ob
        .enqueue(&store, &request(None, "20261128-143012", bare()), false)
        .unwrap();
    let root = root_of(&d);
    let calls = AtomicUsize::new(0);
    let mut seen_id = String::new();
    let r = ob.drain(T0, false, &mut |v: &Value, zip: &Path| {
        calls.fetch_add(1, Ordering::SeqCst);
        assert!(zip.is_file(), "the sender gets the ZIP to read");
        assert!(v["meta"].is_object());
        seen_id = v["id"].as_str().unwrap().to_string();
        ok_receipt()
    });
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(r.sent, 1);
    assert_eq!(seen_id, q.id);
    assert!(
        files_in(&root.join("outbox")).is_empty(),
        "ZIP, sidecar and lock are gone"
    );
    let c = ob.counts();
    assert_eq!((c.waiting, c.failed, c.local), (0, 0, 0));

    let log = fs::read_to_string(root.join("sent.jsonl")).unwrap();
    let lines: Vec<Value> = log
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["id"], q.id);
    assert_eq!(lines[0]["file_id"], "FILE1");
    assert_eq!(lines[0]["issue_url"], "https://github.com/o/r/issues/7");
    assert_eq!(lines[0]["file_name"], q.file_name);
    assert_eq!(lines[0]["source"], "app");
    assert!(lines[0]["sent_ms"].is_u64());
    assert!(
        lines[0]["description"]
            .as_str()
            .unwrap()
            .starts_with("The roof is flat"),
        "{}",
        lines[0]
    );

    let r = ob.drain(T0, true, &mut |_, _| panic!("nothing is left to send"));
    assert_eq!(r.sent, 0);
    assert_eq!(
        fs::read_to_string(root.join("sent.jsonl"))
            .unwrap()
            .lines()
            .count(),
        1
    );

    let l = serde_json::to_value(ob.list(10).unwrap()).unwrap();
    assert_eq!(l["reports"][0]["state"], "sent", "{l}");
}

#[test]
fn the_sent_log_keeps_the_newest_two_hundred() {
    let (d, store, ob) = setup("d3-trim");
    let mut first = String::new();
    let mut last = String::new();
    let mut n = 0;
    while n < 201 {
        for _ in 0..40.min(201 - n) {
            let q = ob
                .enqueue(&store, &request(None, &stamp(n), bare()), false)
                .unwrap();
            if n == 0 {
                first = q.id.clone();
            }
            last = q.id;
            n += 1;
        }
        let r = ob.drain(T0, false, &mut |_, _| ok_receipt());
        assert!(r.sent > 0);
    }
    let log = fs::read_to_string(root_of(&d).join("sent.jsonl")).unwrap();
    assert_eq!(log.lines().count(), outbox::SENT_KEPT);
    assert_eq!(outbox::SENT_KEPT, 200);
    assert!(!log.contains(&first), "the oldest line went");
    assert!(log.contains(&last));
    assert_eq!(ob.counts().waiting, 0);
}

// =================================================================== D4

#[test]
fn a_retry_failure_keeps_the_report_waiting_with_a_backoff() {
    let (d, store, ob) = setup("d4");
    let root = root_of(&d);
    let q = ob
        .enqueue(&store, &request(None, "20261128-143012", bare()), false)
        .unwrap();
    let calls = AtomicUsize::new(0);
    let mut fail = |_: &Value, _: &Path| -> Result<Receipt, SendFailure> {
        calls.fetch_add(1, Ordering::SeqCst);
        Err(SendFailure::Retry {
            code: "offline".into(),
            after_s: None,
        })
    };
    let r = ob.drain(T0, false, &mut fail);
    assert_eq!((r.sent, r.waiting), (0, 1));
    let s = sidecar(&root, &q.file_name);
    assert_eq!(s["state"], "waiting");
    assert_eq!(s["attempts"], 1);
    assert_eq!(s["last_error"], "offline");
    assert_eq!(s["next_try_ms"], T0 + 2 * 60_000, "2^attempts minutes");
    assert!(
        !lock_path(&root, &q.file_name).exists(),
        "the claim is released"
    );
    assert_eq!(ob.counts().waiting, 1);

    // Not due: the sender is not called.
    ob.drain(T0 + 60_000, false, &mut fail);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    // Force ignores the wait.
    ob.drain(T0 + 60_000, true, &mut fail);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(sidecar(&root, &q.file_name)["attempts"], 2);
    // Due: sent without force.
    let due = sidecar(&root, &q.file_name)["next_try_ms"]
        .as_u64()
        .unwrap();
    let r = ob.drain(due, false, &mut |_, _| ok_receipt());
    assert_eq!(r.sent, 1);
}

#[test]
fn the_backoff_never_exceeds_six_hours_and_does_not_overflow() {
    let (d, store, ob) = setup("d4-cap");
    let root = root_of(&d);
    let q = ob
        .enqueue(&store, &request(None, "20261128-143012", bare()), false)
        .unwrap();
    for attempts in [20u64, 59, 60, 63, 64, 1000, u64::MAX - 1] {
        let mut s = sidecar(&root, &q.file_name);
        s["attempts"] = json!(attempts);
        s["next_try_ms"] = json!(0);
        write_sidecar(&root, &q.file_name, &s);
        ob.drain(T0, true, &mut |_, _| {
            Err(SendFailure::Retry {
                code: "offline".into(),
                after_s: None,
            })
        });
        let next = sidecar(&root, &q.file_name)["next_try_ms"]
            .as_u64()
            .unwrap();
        assert!(
            next > T0 && next <= T0 + 6 * 3600 * 1000,
            "attempts {attempts}: {next}"
        );
    }
}

#[test]
fn the_relays_longer_wait_wins_over_the_backoff() {
    let (d, store, ob) = setup("d4-after");
    let root = root_of(&d);
    let q = ob
        .enqueue(&store, &request(None, "20261128-143012", bare()), false)
        .unwrap();
    ob.drain(T0, false, &mut |_, _| {
        Err(SendFailure::Retry {
            code: "rate_limited".into(),
            after_s: Some(10 * 3600),
        })
    });
    let s = sidecar(&root, &q.file_name);
    assert_eq!(s["next_try_ms"], T0 + 10 * 3600 * 1000);
    assert_eq!(s["last_error"], "rate_limited");
}

#[test]
fn a_permanent_failure_marks_the_report_failed_and_is_not_retried_without_force() {
    let (d, store, ob) = setup("d4-perm");
    let root = root_of(&d);
    let q = ob
        .enqueue(&store, &request(None, "20261128-143012", bare()), false)
        .unwrap();
    let calls = AtomicUsize::new(0);
    let mut perm = |_: &Value, _: &Path| -> Result<Receipt, SendFailure> {
        calls.fetch_add(1, Ordering::SeqCst);
        Err(SendFailure::Permanent {
            code: "bad_key".into(),
        })
    };
    let r = ob.drain(T0, false, &mut perm);
    assert_eq!((r.sent, r.failed), (0, 1));
    let s = sidecar(&root, &q.file_name);
    assert_eq!(s["state"], "failed");
    assert_eq!(s["last_error"], "bad_key");
    assert_eq!(
        state_of(&ob, &q.file_name),
        ("failed".into(), Some("bad_key".into()))
    );
    let c = ob.counts();
    assert_eq!((c.waiting, c.failed), (0, 1));

    ob.drain(T0 + 10 * 24 * 3600 * 1000, false, &mut perm);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "not retried by itself, even much later"
    );
    ob.drain(T0, true, &mut |_, _| ok_receipt());
    assert_eq!(ob.counts().failed, 0, "force retries it, and it went");
    assert!(files_in(&root.join("outbox")).is_empty());
}

/// ARCH-002 (fix round 2): a report made by a build without a relay is never
/// uploaded by a background drain, whoever runs it later; only an explicit
/// Send now (`force`) promotes it. (Replaces the round 1 test that also
/// forbade `force`; the owner's decision, fix-r2.md.)
#[test]
fn a_local_report_is_never_sent_by_a_drain_that_is_not_forced() {
    let (d, store, ob) = setup("d4-local");
    let q = ob
        .enqueue(&store, &request(None, "20261128-143012", bare()), true)
        .unwrap();
    for _ in 0..3 {
        let r = ob.drain(T0, false, &mut |_, _| {
            panic!("a local report must not be sent by itself")
        });
        assert_eq!(r.sent, 0);
    }
    assert_eq!(ob.counts().local, 1);
    assert_eq!(ob.counts().waiting, 0, "a drain does not promote it");
    assert_eq!(sidecar(&root_of(&d), &q.file_name)["state"], "local");
    // A later build with a relay opens the same outbox: still nothing by itself.
    let later = Outbox::at(&root_of(&d));
    later.drain(T0 + 86_400_000, false, &mut |_, _| {
        panic!("a local report must not be sent by itself")
    });
    assert_eq!(later.counts().local, 1);
}

#[test]
fn an_explicit_send_now_sends_a_local_report() {
    let (_d, store, ob) = setup("d4-local-force");
    ob.enqueue(&store, &request(None, "20261128-143012", bare()), true)
        .unwrap();
    let calls = AtomicUsize::new(0);
    let r = ob.drain(T0, true, &mut |_, _| {
        calls.fetch_add(1, Ordering::SeqCst);
        ok_receipt()
    });
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(r.sent, 1);
    assert_eq!(ob.counts().local, 0);
}

/// ARCH-001 (fix round 2): the first offline failure ends the drain; the
/// other reports are not tried and count no attempt.
#[test]
fn a_drain_stops_at_the_first_offline_failure() {
    let (d, store, ob) = setup("r2-offline");
    let mut names = Vec::new();
    for n in 0..4 {
        names.push(
            ob.enqueue(&store, &request(None, &stamp(n), bare()), false)
                .unwrap()
                .file_name,
        );
    }
    let calls = AtomicUsize::new(0);
    let r = ob.drain(T0, false, &mut |_, _| {
        calls.fetch_add(1, Ordering::SeqCst);
        Err(SendFailure::Retry {
            code: "offline".into(),
            after_s: None,
        })
    });
    assert_eq!(calls.load(Ordering::SeqCst), 1, "one try, then stop");
    assert_eq!(r.waiting, 4);
    let attempts: u64 = names
        .iter()
        .map(|n| sidecar(&root_of(&d), n)["attempts"].as_u64().unwrap())
        .sum();
    assert_eq!(attempts, 1, "the reports not tried count no attempt");
    assert!(
        names.iter().all(|n| !lock_path(&root_of(&d), n).exists()),
        "no claim is left"
    );
}

/// BUG-3 (fix round 2): a `next_try_ms` far beyond the longest backoff (a
/// clock that jumped, a hostile sidecar) does not park the report.
#[test]
fn a_next_try_far_in_the_future_counts_as_due() {
    let (d, store, ob) = setup("r2-clock");
    let q = ob
        .enqueue(&store, &request(None, "20261128-143012", bare()), false)
        .unwrap();
    let root = root_of(&d);
    for far in [
        T0 + 5 * 365 * 24 * 3600 * 1000,
        T0 + 7 * 3600 * 1000,
        u64::MAX,
    ] {
        let mut s = sidecar(&root, &q.file_name);
        s["next_try_ms"] = json!(far);
        write_sidecar(&root, &q.file_name, &s);
        let calls = AtomicUsize::new(0);
        ob.drain(T0, false, &mut |_, _| {
            calls.fetch_add(1, Ordering::SeqCst);
            Err(SendFailure::Retry {
                code: "server".into(),
                after_s: None,
            })
        });
        assert_eq!(calls.load(Ordering::SeqCst), 1, "next_try_ms {far}");
        let next = sidecar(&root, &q.file_name)["next_try_ms"]
            .as_u64()
            .unwrap();
        assert!(next <= T0 + 6 * 3600 * 1000, "{next}");
    }
}

/// BUG-2 (fix round 2): two processes finishing sends at the same time keep
/// every line of the sent log.
#[test]
fn two_drains_finishing_together_lose_no_sent_line() {
    let (d, store, ob) = setup("r2-sentlog");
    for n in 0..40 {
        ob.enqueue(&store, &request(None, &stamp(n), bare()), false)
            .unwrap();
    }
    let root = root_of(&d);
    let start = Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let (root, start) = (root.clone(), start.clone());
            std::thread::spawn(move || {
                let o = Outbox::at(&root);
                start.wait();
                o.drain(T0, false, &mut |_, _| ok_receipt()).sent
            })
        })
        .collect();
    let sent: usize = handles.into_iter().map(|h| h.join().unwrap()).sum();
    assert_eq!(sent, 40, "each report was sent once");
    let log = fs::read_to_string(root.join("sent.jsonl")).unwrap();
    assert_eq!(log.lines().count(), 40, "one line per sent report");
    let ids: BTreeSet<&str> = log
        .lines()
        .map(|l| l.split("\"id\":\"").nth(1).unwrap().get(..32).unwrap())
        .collect();
    assert_eq!(ids.len(), 40);
    assert!(
        log.lines()
            .all(|l| serde_json::from_str::<Value>(l).is_ok()),
        "no torn line"
    );
}

/// BUG-4 (fix round 2): two reports with the same file name, the first
/// already sent and deleted, are both listed and read back as themselves.
#[test]
fn two_sent_reports_with_one_file_name_are_both_listed() {
    let (_d, store, ob) = setup("r2-samename");
    let mut ids = Vec::new();
    for text in ["FIRST-ZQX", "SECOND-ZQX"] {
        let mut r = request(None, "20261128-143012", bare());
        r.description = text.into();
        ids.push(ob.enqueue(&store, &r, false).unwrap().id);
        assert_eq!(ob.drain(T0, false, &mut |_, _| ok_receipt()).sent, 1);
    }
    let l = serde_json::to_value(ob.list(10).unwrap()).unwrap();
    let rows = l["reports"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "{l}");
    let names: BTreeSet<&str> = rows
        .iter()
        .map(|r| r["file_name"].as_str().unwrap())
        .collect();
    assert_eq!(names.len(), 2, "two sent reports never share a name: {l}");
    let mut seen = BTreeSet::new();
    for n in names {
        let r = ob.read(n).unwrap();
        assert_eq!(r["state"], "sent");
        seen.insert(r["sent"]["id"].as_str().unwrap().to_string());
    }
    assert_eq!(seen, ids.into_iter().collect());
}

#[test]
fn the_drain_result_counts_what_is_left() {
    let (_d, store, ob) = setup("d4-counts");
    for n in 0..3 {
        ob.enqueue(&store, &request(None, &stamp(n), bare()), false)
            .unwrap();
    }
    let calls = AtomicUsize::new(0);
    let r = ob.drain(
        T0,
        false,
        &mut |_, _| match calls.fetch_add(1, Ordering::SeqCst) {
            0 => ok_receipt(),
            1 => Err(SendFailure::Retry {
                code: "server".into(),
                after_s: None,
            }),
            _ => Err(SendFailure::Permanent {
                code: "bad_request".into(),
            }),
        },
    );
    assert_eq!((r.sent, r.waiting, r.failed), (1, 1, 1));
}

// =================================================================== D5

#[test]
fn two_threads_draining_one_outbox_send_each_report_once() {
    let (_d, store, ob) = setup("d5");
    for n in 0..3 {
        ob.enqueue(&store, &request(None, &stamp(n), bare()), false)
            .unwrap();
    }
    let ob = Arc::new(ob);
    let calls = Arc::new(AtomicUsize::new(0));
    // The first sender waits until the other drain has returned, so the two
    // drains really overlap. The timeout only guards a broken claim.
    let finished = Arc::new((Mutex::new(0usize), Condvar::new()));
    let mut handles = Vec::new();
    for _ in 0..2 {
        let (ob, calls, finished) = (ob.clone(), calls.clone(), finished.clone());
        handles.push(std::thread::spawn(move || {
            ob.drain(T0, false, &mut |_, _| {
                calls.fetch_add(1, Ordering::SeqCst);
                let (m, cv) = &*finished;
                let g = m.lock().unwrap();
                let _ = cv
                    .wait_timeout_while(g, Duration::from_secs(3), |n| *n == 0)
                    .unwrap();
                ok_receipt()
            });
            let (m, cv) = &*finished;
            *m.lock().unwrap() += 1;
            cv.notify_all();
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    assert_eq!(
        calls.load(Ordering::SeqCst),
        3,
        "once per report, not twice"
    );
    assert_eq!(ob.counts().waiting, 0);
}

#[test]
fn a_fresh_lock_is_respected_and_an_old_one_is_taken_over() {
    let (d, store, ob) = setup("d5-lock");
    let root = root_of(&d);
    let q = ob
        .enqueue(&store, &request(None, "20261128-143012", bare()), false)
        .unwrap();
    let lock = lock_path(&root, &q.file_name);
    fs::write(&lock, "pid").unwrap();
    let now = now_ms();
    let r = ob.drain(now, true, &mut |_, _| {
        panic!("a claimed report is not sent")
    });
    assert_eq!(r.sent, 0);
    assert!(lock.exists(), "somebody else's lock stays");
    assert_eq!(ob.counts().waiting, 1);

    let old = std::time::SystemTime::now() - Duration::from_secs(20 * 60);
    fs::OpenOptions::new()
        .write(true)
        .open(&lock)
        .unwrap()
        .set_modified(old)
        .unwrap();
    let calls = AtomicUsize::new(0);
    let r = ob.drain(now, false, &mut |_, _| {
        calls.fetch_add(1, Ordering::SeqCst);
        ok_receipt()
    });
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "a lock older than 15 minutes is stale"
    );
    assert_eq!(r.sent, 1);
    assert!(!lock.exists());
}

// =================================================================== D6

/// Queue one report and return its name; the caller then damages it.
fn queued(name: &str) -> (PathBuf, Store, Outbox, String) {
    let (d, store, ob) = setup(name);
    let q = ob
        .enqueue(&store, &request(None, "20261128-143012", bare()), false)
        .unwrap();
    (d, store, ob, q.file_name)
}

fn assert_unreadable_and_never_sent(ob: &Outbox, file_name: &str, what: &str) {
    let r = ob.drain(T0, true, &mut |_, _| {
        panic!("{what}: a damaged report must not be sent")
    });
    assert_eq!(r.sent, 0, "{what}");
    let (state, err) = state_of(ob, file_name);
    assert_eq!(state, "failed", "{what}");
    assert!(err.is_some(), "{what}");
    assert!(ob.counts().failed >= 1, "{what}");
}

#[test]
fn a_zip_without_a_sidecar_is_listed_as_unreadable() {
    let (d, _s, ob, name) = queued("d6-nosidecar");
    fs::remove_file(sidecar_path(&root_of(&d), &name)).unwrap();
    assert_unreadable_and_never_sent(&ob, &name, "missing sidecar");
    assert_eq!(state_of(&ob, &name).1.as_deref(), Some("unreadable"));
}

#[test]
fn damaged_sidecars_are_unreadable_and_never_sent() {
    let (d, _s, ob, name) = queued("d6-damaged");
    let root = root_of(&d);
    let good = fs::read(sidecar_path(&root, &name)).unwrap();
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("truncated", good[..good.len() / 2].to_vec()),
        ("not json", b"\xff\xfe not json at all".to_vec()),
        ("empty", Vec::new()),
        ("an array", b"[1,2,3]".to_vec()),
        ("oversized", vec![b' '; 6 * 1024 * 1024]),
        ("bad id", br#"{"v":1,"id":"../../etc","file_name":"x","size":1,"queued_ms":1,"attempts":0,"next_try_ms":0,"state":"waiting","last_error":null,"meta":{}}"#.to_vec()),
        ("unknown version", br#"{"v":99}"#.to_vec()),
    ];
    for (what, bytes) in cases {
        fs::write(sidecar_path(&root, &name), &bytes).unwrap();
        assert_unreadable_and_never_sent(&ob, &name, what);
        assert_eq!(
            state_of(&ob, &name).1.as_deref(),
            Some("unreadable"),
            "{what}"
        );
    }
}

#[test]
fn a_sidecar_that_names_another_file_is_not_trusted() {
    let (d, _s, ob, name) = queued("d6-name");
    let root = root_of(&d);
    let outside = d.join("secret.txt");
    fs::write(&outside, "secret").unwrap();
    for other in [
        "../x",
        "../../secret.txt",
        outside.to_str().unwrap(),
        "other.zip",
        "a/b.zip",
    ] {
        let mut s = sidecar(&root, &name);
        s["file_name"] = json!(other);
        write_sidecar(&root, &name, &s);
        let paths = Mutex::new(Vec::new());
        ob.drain(T0, true, &mut |_, zip| {
            paths.lock().unwrap().push(zip.to_path_buf());
            ok_receipt()
        });
        for p in paths.lock().unwrap().iter() {
            assert_eq!(
                p,
                &zip_path(&root, &name),
                "{other}: only the queued ZIP is ever given out"
            );
        }
        assert!(
            outside.exists(),
            "{other}: nothing outside the outbox is touched"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_link_in_place_of_the_zip_is_never_followed() {
    let (d, _s, ob, name) = queued("d6-link");
    let root = root_of(&d);
    let target = d.join("secret.zip");
    fs::write(&target, b"PK\x03\x04 secret").unwrap();
    let zip = zip_path(&root, &name);
    fs::remove_file(&zip).unwrap();
    std::os::unix::fs::symlink(&target, &zip).unwrap();
    assert_unreadable_and_never_sent(&ob, &name, "symlink");
    assert!(target.exists());
}

#[test]
fn a_huge_sparse_zip_is_refused_without_reading_it() {
    let (d, _s, ob, name) = queued("d6-sparse");
    let zip = zip_path(&root_of(&d), &name);
    fs::File::create(&zip)
        .unwrap()
        .set_len(300 * 1024 * 1024)
        .unwrap();
    assert_unreadable_and_never_sent(&ob, &name, "300 MB sparse file");
}

#[test]
fn a_stray_non_report_file_in_the_outbox_is_ignored() {
    let (d, _s, ob, name) = queued("d6-stray");
    let dir = root_of(&d).join("outbox");
    fs::write(dir.join("notes.txt"), "x").unwrap();
    fs::write(dir.join("tectora-report-bad.zip"), b"junk").unwrap();
    let r = ob.drain(T0, true, &mut |_, _| ok_receipt());
    assert_eq!(r.sent, 1, "only the real report");
    assert!(dir.join("notes.txt").exists());
    let _ = name;
}

#[test]
fn the_outbox_refuses_a_fifty_first_report_and_removes_nothing() {
    let (d, store, ob) = setup("d6-full");
    assert_eq!(outbox::MAX_QUEUED, 50);
    for n in 0..50 {
        ob.enqueue(&store, &request(None, &stamp(n), bare()), false)
            .unwrap();
    }
    let e = ob
        .enqueue(&store, &request(None, &stamp(50), bare()), false)
        .unwrap_err();
    assert_eq!(e.code, "outbox_full");
    let zips = files_in(&root_of(&d).join("outbox"))
        .into_iter()
        .filter(|f| f.ends_with(".zip"))
        .count();
    assert_eq!(zips, 50);
    assert_eq!(ob.counts().waiting, 50);
}

#[test]
fn the_outbox_refuses_a_report_over_the_byte_cap() {
    let (d, store, ob) = setup("d6-bytes");
    let one = ob
        .enqueue(&store, &request(None, &stamp(0), bare()), false)
        .unwrap()
        .size;
    assert_eq!(outbox::MAX_QUEUED_BYTES, 256 * 1024 * 1024);
    let limits = Limits {
        max_queued_bytes: one * 2 + one / 2,
        ..Limits::default()
    };
    ob.enqueue_with(&store, &request(None, &stamp(1), bare()), false, &limits)
        .unwrap();
    let e = ob
        .enqueue_with(&store, &request(None, &stamp(2), bare()), false, &limits)
        .unwrap_err();
    assert_eq!(e.code, "outbox_full");
    assert_eq!(
        files_in(&root_of(&d).join("outbox"))
            .into_iter()
            .filter(|f| f.ends_with(".zip"))
            .count(),
        2
    );
}

#[test]
fn read_and_discard_take_only_plain_report_names() {
    let (d, _s, ob, name) = queued("d6-names");
    for bad in [
        "../x.zip",
        "/etc/passwd",
        "a/b.zip",
        "tectora-report-x.txt",
        "",
        "..",
        "C:\\x.zip",
    ] {
        assert!(ob.read(bad).is_err(), "{bad}");
        assert!(ob.discard(bad).is_err(), "{bad}");
        assert!(ob.export(bad, &d.join("o.zip")).is_err(), "{bad}");
    }
    let r = ob.read(&name).unwrap();
    assert_eq!(r["state"], "waiting");
    assert!(r["manifest"].is_object() && r["report"].is_object());
    ob.discard(&name).unwrap();
    assert!(
        files_in(&root_of(&d).join("outbox")).is_empty(),
        "ZIP and sidecar are gone"
    );
    assert!(ob.discard(&name).is_err(), "an unknown name is refused");
}

#[test]
fn a_sent_report_is_read_from_the_log_and_cannot_be_discarded() {
    let (_d, store, ob) = setup("d6-sent");
    let q = ob
        .enqueue(&store, &request(None, "20261128-143012", bare()), false)
        .unwrap();
    ob.drain(T0, false, &mut |_, _| ok_receipt());
    let r = ob.read(&q.file_name).unwrap();
    assert_eq!(r["state"], "sent");
    assert_eq!(r["sent"]["id"], q.id);
    assert!(ob.discard(&q.file_name).is_err());
}

#[test]
fn a_queued_report_is_exported_with_save_as_rules() {
    let (d, _s, ob, name) = queued("d6-export");
    let dest = d.join("out.zip");
    ob.export(&name, &dest).unwrap();
    assert!(dest.is_file());
    let bytes = fs::read(&dest).unwrap();
    assert_eq!(bytes, fs::read(zip_path(&root_of(&d), &name)).unwrap());
    assert!(
        ob.export(&name, &d.join("not-a-zip.txt")).is_err(),
        "a plain *.zip name only"
    );
    assert!(ob.export(&name, Path::new("relative.zip")).is_err());
    assert!(
        zip_path(&root_of(&d), &name).exists(),
        "export keeps the queued report"
    );
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 48,
        rng_seed: proptest::test_runner::RngSeed::Fixed(20261129),
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    /// A sidecar is untrusted input: whatever it holds, listing and draining
    /// never panic, and the sender only ever gets a 32-hex id.
    #[test]
    fn any_sidecar_content_never_panics_and_never_leaks_a_bad_id(
        raw in proptest::collection::vec(any::<u8>(), 0..400),
        id in ".{0,40}",
        file_name in ".{0,60}",
        size in any::<i64>(),
        attempts in any::<i64>(),
        next in any::<f64>(),
        state in prop_oneof![Just("waiting"), Just("failed"), Just("local"), Just("sent"), Just("")],
        last in proptest::option::of(".{0,30}"),
        meta in prop_oneof![Just(json!(null)), Just(json!({})), Just(json!([1])), Just(json!("s"))],
        use_raw in any::<bool>(),
    ) {
        let (d, store, ob) = setup("d6-prop");
        let q = ob.enqueue(&store, &request(None, "20261128-143012", bare()), false).unwrap();
        let root = root_of(&d);
        let bytes = if use_raw {
            raw
        } else {
            serde_json::to_vec(&json!({
                "v": 1, "id": id, "file_name": file_name, "size": size, "queued_ms": 1,
                "attempts": attempts, "next_try_ms": next, "state": state,
                "last_error": last, "meta": meta,
            })).unwrap()
        };
        fs::write(sidecar_path(&root, &q.file_name), bytes).unwrap();
        let _ = ob.list(10);
        let _ = ob.counts();
        let _ = ob.read(&q.file_name);
        ob.drain(T0, true, &mut |v, _| {
            assert!(is_hex32(v["id"].as_str().unwrap_or("")), "the sender got a bad id: {v}");
            ok_receipt()
        });
        let _ = ob.list(10);
        let _ = fs::remove_dir_all(&d);
    }
}

// =================================================================== D7

const ALLOWED_META: [&str; 20] = [
    "description",
    "app_version",
    "engine_version",
    "os",
    "os_version",
    "arch",
    "locale",
    "source",
    "reporter",
    "screen",
    "job_name",
    "job_status",
    "calculation_ok",
    "calculation_error_code",
    "included",
    "left_out",
    "created_ms",
    "install",
    "user_name",
    "test",
];
const REQUIRED_META: [&str; 11] = [
    "description",
    "app_version",
    "engine_version",
    "os",
    "arch",
    "source",
    "reporter",
    "calculation_ok",
    "included",
    "left_out",
    "created_ms",
];

fn report_for(include: Include) -> (Value, Manifest, PathBuf) {
    env();
    let d = tmp("d7");
    let store = Store::new(d.join("lib")).unwrap();
    let id = job_with(&store, "Novák dům", 2000, 2);
    let out = d.join("out");
    fs::create_dir_all(&out).unwrap();
    let mut req = request(Some(&id), "20261128-143012", include);
    req.plan_svg = Some("<svg xmlns=\"http://www.w3.org/2000/svg\"><rect/></svg>".into());
    req.model_png = Some(png(64));
    let c = bugreport::create(&store, &req, &Dest::Folder(out.clone())).unwrap();
    let r = bugreport::read(&out, &c.file_name).unwrap();
    let manifest: Manifest = serde_json::from_value(r["manifest"].clone()).unwrap();
    (r["report"].clone(), manifest, d)
}

#[test]
fn the_issue_has_only_allow_listed_keys_and_no_customer_data() {
    let (report, manifest, d) = report_for(all());
    let meta = outbox::issue_meta(&report, &manifest);
    let text = serde_json::to_string(&meta).unwrap();
    for secret in [
        CUSTOMER,
        "Novák, Jan",
        ADDRESS,
        "NOTICE-DETAIL-ZQX",
        "ACTION-NAME-ZQX",
    ] {
        assert!(!text.contains(secret), "{secret} leaked: {text}");
    }
    assert!(
        !text.contains(d.to_str().unwrap()),
        "an absolute path leaked"
    );
    assert!(!text.contains("/tmp") && !text.contains("C:\\"), "{text}");
    let obj = meta.as_object().expect("meta is an object");
    for k in obj.keys() {
        assert!(
            ALLOWED_META.contains(&k.as_str()),
            "{k} is not allow-listed"
        );
    }
    for k in REQUIRED_META {
        assert!(obj.contains_key(k), "{k} missing: {text}");
    }
    for never in [
        "customer",
        "address",
        "actions",
        "notices",
        "settings",
        "paths",
        "log",
        "job",
        "calculation",
    ] {
        assert!(!obj.contains_key(never), "{never}");
    }
    assert_eq!(
        meta["job_name"], "Novák dům",
        "the name stays while customer data is ticked"
    );
    assert_eq!(meta["source"], "app");
    assert_eq!(meta["reporter"], "user");
    assert!(
        meta["description"]
            .as_str()
            .unwrap()
            .starts_with("The roof is flat")
    );
    assert_eq!(meta["included"]["files"], true);
}

#[test]
fn without_customer_data_the_issue_has_no_job_name() {
    let (report, manifest, _d) = report_for(Include {
        customer: false,
        ..all()
    });
    let meta = outbox::issue_meta(&report, &manifest);
    let text = serde_json::to_string(&meta).unwrap();
    assert!(meta.get("job_name").is_none_or(Value::is_null), "{text}");
    assert!(!text.contains("Novák"), "{text}");
    assert!(!text.contains(CUSTOMER) && !text.contains(ADDRESS));
}

#[test]
fn the_issue_text_is_cleaned_and_capped_again() {
    let (mut report, manifest, _d) = report_for(all());
    let evil = format!(
        "line one\nA\u{202e}B\u{0007}C\u{2066}D\u{2069}E\u{200f}F\u{0000}G{}",
        "x".repeat(8000)
    );
    report["description"] = json!(evil);
    report["reporter"] = json!("Cl\u{202e}aude\n\u{0007}");
    report["ui"]["screen"] = json!(format!("ed\u{202a}itor{}", "y".repeat(500)));
    let meta = outbox::issue_meta(&report, &manifest);
    for key in ["description", "reporter", "screen"] {
        let s = meta[key]
            .as_str()
            .unwrap_or_else(|| panic!("{key}: {meta}"));
        for c in s.chars() {
            let bidi = matches!(c, '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}');
            assert!(
                !bidi && (!c.is_control() || (key == "description" && c == '\n')),
                "{key}: U+{:04X}",
                c as u32
            );
        }
    }
    assert!(meta["description"].as_str().unwrap().chars().count() <= 5000);
    assert!(
        meta["description"]
            .as_str()
            .unwrap()
            .starts_with("line one")
    );
    assert!(
        meta["screen"].as_str().unwrap().chars().count() <= 100,
        "short fields stay short"
    );
}

// =================================================================== D16

#[test]
fn the_log_says_what_happened_but_names_no_file_or_job() {
    let (_d, store, ob) = setup("d16-log");
    let id = job_with(&store, "Zqx-Tajny-Dum", 100, 1);
    let q = ob
        .enqueue(&store, &request(Some(&id), "20261128-143012", all()), false)
        .unwrap();
    ob.drain(T0, false, &mut |_, _| {
        Err(SendFailure::Retry {
            code: "offline".into(),
            after_s: None,
        })
    });
    ob.drain(T0, true, &mut |_, _| ok_receipt());
    let log = applog::tail("app", 512 * 1024, 5000);
    for needle in [
        "report queued",
        "report send failed: offline",
        "report sent",
    ] {
        assert!(log.contains(needle), "{needle} missing in\n{log}");
    }
    for secret in [
        "zqx-tajny-dum",
        "Zqx-Tajny-Dum",
        q.file_name.as_str(),
        "github.com",
        "script.google.com",
    ] {
        assert!(
            !log.to_lowercase().contains(&secret.to_lowercase()),
            "{secret} in the log"
        );
    }
}

#[test]
fn the_state_files_hold_no_url_or_key() {
    let (d, store, ob) = setup("d16-state");
    let q = ob
        .enqueue(&store, &request(None, "20261128-143012", bare()), false)
        .unwrap();
    ob.drain(T0, false, &mut |_, _| {
        Err(SendFailure::Retry {
            code: "server".into(),
            after_s: None,
        })
    });
    let side = fs::read_to_string(sidecar_path(&root_of(&d), &q.file_name)).unwrap();
    ob.drain(T0, true, &mut |_, _| ok_receipt());
    let sent = fs::read_to_string(root_of(&d).join("sent.jsonl")).unwrap();
    for text in [side, sent] {
        assert!(!text.contains("script.google.com"));
        assert!(!text.to_lowercase().contains("key"), "{text}");
    }
}

#[test]
fn both_workflows_pass_both_report_secrets_to_the_build() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.github/workflows");
    for f in ["build.yml", "release.yml"] {
        let text = fs::read_to_string(root.join(f)).unwrap();
        for name in ["TECTORA_REPORT_URL", "TECTORA_REPORT_KEY"] {
            assert!(
                text.contains(&format!("secrets.{name}")),
                "{f} does not pass secrets.{name}"
            );
        }
        assert!(
            !text.contains("echo \"$TECTORA_REPORT"),
            "{f} echoes a secret"
        );
    }
}
