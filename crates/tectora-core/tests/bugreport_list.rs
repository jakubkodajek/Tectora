//! Fix round 1, ERR-007 / BUG-3 / TEST-011 (target/work/bug-report/fix-r1.md):
//! `bugreport::list_checked` tells a missing folder from an empty one and
//! counts the reports it could not read. Own file: it needs the new API.

use std::fs;

use serde_json::json;
use tectora_core::bugreport::{self, Dest, Include, Request, Source};
use tectora_core::store::{Store, now_ms};

#[test]
fn a_missing_folder_is_an_error_and_damaged_reports_are_counted() {
    let d = std::env::temp_dir().join(format!(
        "tectora-buglist-{}-{}",
        std::process::id(),
        now_ms()
    ));
    let _ = fs::remove_dir_all(&d);
    let store = Store::new(d.join("lib")).unwrap();
    let out = d.join("out");
    fs::create_dir_all(&out).unwrap();
    let req = |stamp: &str| Request {
        description: stamp.into(),
        job_id: None,
        include: Include {
            job: false,
            customer: true,
            files: false,
            history: false,
            snapshot: false,
            log: false,
        },
        source: Source::App,
        reporter: "user".into(),
        local_stamp: Some(stamp.into()),
        client: json!({}),
        editor_project: None,
        plan_svg: None,
        model_png: None,
    };
    for s in ["20261101-080000", "20261128-143012"] {
        bugreport::create(&store, &req(s), &Dest::Folder(out.clone())).unwrap();
    }
    fs::write(out.join("tectora-report-20260101-000000-fake.zip"), b"junk").unwrap();
    fs::write(out.join("notes.zip"), b"PK").unwrap();
    fs::write(out.join("readme.txt"), "x").unwrap();

    let l = bugreport::list_checked(&out, 10).unwrap();
    assert_eq!(l.reports.len(), 2, "only real reports are listed");
    assert_eq!(
        l.skipped, 1,
        "the damaged report is counted, other files are not"
    );
    assert!(l.reports[0].file_name.contains("20261128"), "newest first");
    assert_eq!(bugreport::list_checked(&out, 1).unwrap().reports.len(), 1);
    // A folder that is not there is an error, not an empty list.
    assert!(bugreport::list_checked(&d.join("unplugged-drive"), 10).is_err());
    let _ = fs::remove_dir_all(d);
}
