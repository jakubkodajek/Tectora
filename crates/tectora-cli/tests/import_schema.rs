//! `tectora import-job` and `tectora schema` (audit 2026-11-24 TEST-1809):
//! a bundle and a `.tectora` file become new jobs, a second import gets a
//! new id, a damaged bundle and a missing path fail with exit 1 and say
//! why ("not found" for a path that is not there, not "not a Tectora
//! file"), and `schema` prints the checked-in `docs/tectora.schema.json`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tectora-cli-{name}-{}-{}",
        std::process::id(),
        tectora_core::store::now_ms()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn tectora(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tectora"))
        .args(args)
        .output()
        .unwrap()
}

fn import(input: &Path, dir: &Path) -> (Output, serde_json::Value) {
    let out = tectora(&[
        "import-job".as_ref(),
        input.as_os_str(),
        "--dir".as_ref(),
        dir.as_os_str(),
    ]);
    let text = if out.stdout.is_empty() {
        &out.stderr
    } else {
        &out.stdout
    };
    let v = serde_json::from_slice(text)
        .unwrap_or_else(|_| serde_json::json!(String::from_utf8_lossy(text)));
    (out, v)
}

#[test]
fn import_job_adds_a_bundle_and_a_file_as_new_jobs_and_refuses_bad_input() {
    let root = temp("import");
    let from = root.join("from");
    let to = root.join("to");
    let source = tectora_core::store::Store::new(from.clone()).unwrap();
    let p = tectora_core::examples::find("gable").unwrap().project();
    let id = source.save(None, &p).unwrap();
    let bundle = root.join("job.tectorajob");
    tectora_core::bundle::export(&source, &id, &bundle).unwrap();

    // A bundle, twice: two jobs with two ids.
    let (out, first) = import(&bundle, &to);
    assert!(out.status.success(), "{first}");
    assert_eq!(first["ok"], true);
    let (out, second) = import(&bundle, &to);
    assert!(out.status.success(), "{second}");
    assert_ne!(first["id"], second["id"]);
    let library = tectora_core::store::Store::new(to.clone()).unwrap();
    for v in [&first, &second] {
        assert_eq!(
            library.load(v["id"].as_str().unwrap()).unwrap().parts,
            p.parts
        );
    }

    // A .tectora file from outside the library.
    let file = root.join("loose.tectora");
    std::fs::copy(source.path(&id).unwrap(), &file).unwrap();
    let (out, loose) = import(&file, &to);
    assert!(out.status.success(), "{loose}");
    assert_eq!(
        library.load(loose["id"].as_str().unwrap()).unwrap().parts,
        p.parts
    );

    // A damaged bundle: exit 1, the reason, nothing added.
    let jobs = library.list().len();
    let bytes = std::fs::read(&bundle).unwrap();
    let broken = root.join("broken.tectorajob");
    std::fs::write(&broken, &bytes[..bytes.len() / 2]).unwrap();
    let (out, v) = import(&broken, &to);
    assert_eq!(out.status.code(), Some(1), "{v}");
    assert_eq!(v["ok"], false, "{v}");
    assert!(v["error"].as_str().unwrap().contains("bundle"), "{v}");
    assert_eq!(library.list().len(), jobs);

    // A path that is not there says so, for a bundle and a job file.
    for missing in [
        root.join("nothing.tectorajob"),
        root.join("nothing.tectora"),
        root.join("nothing"),
    ] {
        let (out, v) = import(&missing, &to);
        assert_eq!(out.status.code(), Some(1), "{v}");
        let e = v["error"].as_str().unwrap();
        assert!(e.contains("not found"), "{e}");
        assert!(!e.contains("not a Tectora file"), "{e}");
    }
    assert_eq!(library.list().len(), jobs);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn schema_prints_the_checked_in_project_schema() {
    let out = tectora(&["schema".as_ref()]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let printed: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/tectora.schema.json");
    let file: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(printed, file);
}
