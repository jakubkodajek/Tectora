//! Opening files from the operating system (double-click on a `.tectora` or
//! `.tectorajob` file, start-up arguments, a second instance): the desktop
//! app hands the paths to `bundle::open_paths` (`open_files` in
//! `app/src-tauri/src/lib.rs`). The app crate needs WebKitGTK and is not
//! built in the Linux container, so the part of that path that lives in the
//! core is tested here (TEST-008); `file_args` itself has a unit test in
//! the app crate (run on Windows CI).

use std::fs;
use std::path::{Path, PathBuf};

use tectora_core::bundle::{self, open_paths};
use tectora_core::store::{Store, now_ms};

fn scratch(tag: &str) -> PathBuf {
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "tectora-open-files-{tag}-{}-{}-{n}",
        now_ms(),
        std::process::id()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_job(path: &Path, example: &str, name: &str) {
    let mut p = tectora_core::examples::find(example).unwrap().project();
    p.name = name.to_string();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, p.to_json().unwrap()).unwrap();
}

#[test]
fn files_named_in_any_case_open_as_new_jobs() {
    let dir = scratch("case");
    let store = Store::new(dir.join("lib")).unwrap();
    // Windows and macOS file systems keep the user's case: TERRAN's files
    // come as .TECTORA from some mail clients.
    let upper = dir.join("Novák.TECTORA");
    write_job(&upper, "hip", "Novák");
    let mixed = dir.join("Svoboda.Tectora");
    write_job(&mixed, "gable", "Svoboda");
    let r = open_paths(&store, [upper, mixed]);
    assert!(r.failed.is_empty(), "{:?}", r.failed);
    let last = r.last.expect("the last file is shown");
    assert_eq!(store.load(&last).unwrap().name, "Svoboda");
    assert_eq!(store.list().len(), 2);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_bundle_in_any_case_opens_with_its_files() {
    let dir = scratch("bundle");
    let from = Store::new(dir.join("from")).unwrap();
    let p = tectora_core::examples::find("l-shape").unwrap().project();
    let id = from.save(None, &p).unwrap();
    from.add_attachment(&id, "plan.pdf", b"%PDF-1.4").unwrap();
    let job = dir.join("Zakázka.TECTORAJOB");
    bundle::export(&from, &id, &job).unwrap();
    let store = Store::new(dir.join("lib")).unwrap();
    let r = open_paths(&store, [job]);
    assert!(r.failed.is_empty(), "{:?}", r.failed);
    let opened = r.last.unwrap();
    assert_eq!(
        serde_json::to_value(&store.load(&opened).unwrap().parts).unwrap(),
        serde_json::to_value(&p.parts).unwrap()
    );
    let names: Vec<String> = store
        .attachments(&opened)
        .into_iter()
        .map(|a| a.name)
        .collect();
    assert_eq!(names, ["plan.pdf"]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn paths_with_spaces_and_diacritics_open() {
    let dir = scratch("names");
    let store = Store::new(dir.join("Zakázky TERRAN")).unwrap();
    let file = dir.join("Nový Dům").join("Střecha č. 2.tectora");
    write_job(&file, "hip", "Střecha č. 2");
    let r = open_paths(&store, [file]);
    assert!(r.failed.is_empty(), "{:?}", r.failed);
    assert_eq!(store.load(&r.last.unwrap()).unwrap().name, "Střecha č. 2");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_folder_a_missing_file_and_another_type_are_refused_and_named() {
    let dir = scratch("refused");
    let store = Store::new(dir.join("lib")).unwrap();
    let folder = dir.join("looks-like.tectora");
    fs::create_dir_all(&folder).unwrap();
    let missing = dir.join("gone.tectora");
    let other = dir.join("plan.pdf");
    fs::write(&other, b"%PDF").unwrap();
    let good = dir.join("good.tectora");
    write_job(&good, "gable", "Dobrá");
    let r = open_paths(
        &store,
        [folder.clone(), good, missing.clone(), other.clone()],
    );
    let failed: Vec<(&PathBuf, &str)> = r.failed.iter().map(|(p, e)| (p, e.code)).collect();
    assert_eq!(failed.len(), 3, "{failed:?}");
    assert_eq!(failed[0], (&folder, "not_a_file"));
    assert_eq!(failed[1].0, &missing);
    assert_eq!(failed[2], (&other, "unsupported_type"));
    for (p, e) in &r.failed {
        assert!(
            e.detail.contains(&p.display().to_string()),
            "{}: {}",
            e.code,
            e.detail
        );
    }
    // The good file still opens, and nothing else was added.
    assert_eq!(store.load(&r.last.unwrap()).unwrap().name, "Dobrá");
    assert_eq!(store.list().len(), 1);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_file_of_the_library_opens_itself_twice_without_a_copy() {
    let dir = scratch("library");
    let store = Store::new(dir.join("lib")).unwrap();
    let p = tectora_core::examples::find("hip").unwrap().project();
    let id = store.save(None, &p).unwrap();
    let path = store.dir.join(format!("{id}.tectora"));
    let r = open_paths(&store, [path.clone(), path]);
    assert!(r.failed.is_empty(), "{:?}", r.failed);
    assert_eq!(r.last.as_deref(), Some(id.as_str()));
    assert_eq!(store.list().len(), 1);
    let _ = fs::remove_dir_all(&dir);
}
