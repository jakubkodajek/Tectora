//! `tectora export-job` never writes over the job file it exports
//! (audit 2026-11-21 HUNTIO-002): `u.tectora` became a ZIP nothing could
//! open.

use std::process::Command;

#[test]
fn an_export_to_the_jobs_own_file_name_keeps_the_job() {
    let dir = std::env::temp_dir().join(format!(
        "tectora-cli-export-{}-{}",
        std::process::id(),
        tectora_core::store::now_ms()
    ));
    let store = tectora_core::store::Store::new(dir.clone()).unwrap();
    let p = tectora_core::examples::find("gable").unwrap().project();
    let id = store.save(None, &p).unwrap();
    let own = store.path(&id).unwrap();
    let before = std::fs::read(&own).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_tectora"))
        .args(["export-job", &id])
        .arg(&own)
        .arg("--dir")
        .arg(&dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let answer: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let written = std::path::PathBuf::from(answer["output"].as_str().unwrap());
    assert_eq!(
        written,
        own.with_file_name(format!("{id}.tectora.tectorajob"))
    );
    assert!(written.is_file());
    assert_eq!(std::fs::read(&own).unwrap(), before, "the job file is kept");
    assert_eq!(store.load(&id).unwrap(), p);
    let _ = std::fs::remove_dir_all(dir);
}
