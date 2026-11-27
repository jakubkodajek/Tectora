//! What the library's note names in its own folders (store.rs
//! `foreign_entries`), audit 2026-11-22 round 9.

use std::fs;

use tectora_core::store::{Store, USER_ACTOR};

fn temp_store(name: &str) -> Store {
    let ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    Store::new(std::env::temp_dir().join(format!("tectora-notes-{name}-{ns}"))).unwrap()
}

fn job(store: &Store, id: &str) {
    let p = tectora_core::examples::find("hip").unwrap().project();
    store
        .save_as(Some(id.into()), &p, USER_ACTOR, "Made.")
        .unwrap();
}

/// ARCH-803: a folder at a lock's `.fix` helper file does not stop a save
/// of the job, so it is not announced as the lock (`lock`: "the job cannot
/// be saved") but as its own kind.
#[test]
fn a_folder_at_a_locks_helper_file_has_its_own_kind() {
    let store = temp_store("fix");
    job(&store, "ref");
    let activity = store.activity_dir();
    fs::create_dir_all(activity.join("ref.lock.fix").join("x")).unwrap();
    let found = store.foreign_entries();
    let got: Vec<(&str, &str, Option<&str>)> = found
        .iter()
        .map(|f| (f.id.as_str(), f.kind, f.job.as_deref()))
        .collect();
    assert_eq!(got, [(".activity/ref.lock.fix", "lock_fix", Some("ref"))]);
    let _ = fs::remove_dir_all(&store.dir);
}

/// ARCH-804: the lock of a job whose id ends in `.lock` names that job,
/// not the id without it (`trim_end_matches` removed `.lock` twice).
#[test]
fn the_lock_of_a_job_whose_id_ends_in_lock_names_that_job() {
    let store = temp_store("lock-lock");
    job(&store, "x.lock");
    job(&store, "x");
    let activity = store.activity_dir();
    let _ = fs::remove_file(activity.join("x.lock.lock"));
    fs::create_dir_all(activity.join("x.lock.lock")).unwrap();
    fs::write(activity.join("x.lock.lock").join("f"), "x").unwrap();
    let _ = fs::remove_file(activity.join("x.lock.lock.fix"));
    fs::create_dir_all(activity.join("x.lock.lock.fix")).unwrap();
    let found = store.foreign_entries();
    let got: Vec<(&str, &str, Option<&str>)> = found
        .iter()
        .map(|f| (f.id.as_str(), f.kind, f.job.as_deref()))
        .collect();
    assert_eq!(
        got,
        [
            (".activity/x.lock.lock", "lock", Some("x.lock")),
            (".activity/x.lock.lock.fix", "lock_fix", Some("x.lock")),
        ]
    );
    let _ = fs::remove_dir_all(&store.dir);
}

/// TEST-802: at most 20 odd locks of `.activity` are named, sorted.
#[test]
fn at_most_twenty_odd_locks_are_named_sorted() {
    let store = temp_store("twenty");
    job(&store, "a");
    let activity = store.activity_dir();
    for n in 0..25 {
        let lock = activity.join(format!("j{n:02}.lock"));
        fs::create_dir_all(&lock).unwrap();
        fs::write(lock.join("f"), "x").unwrap();
    }
    let found = store.foreign_entries();
    let ids: Vec<&str> = found.iter().map(|f| f.id.as_str()).collect();
    let want: Vec<String> = (0..20).map(|n| format!(".activity/j{n:02}.lock")).collect();
    assert_eq!(ids, want);
    assert!(found.iter().all(|f| f.kind == "lock"));
    let _ = fs::remove_dir_all(&store.dir);
}
