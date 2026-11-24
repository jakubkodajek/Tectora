//! Audit 2026-11-23 HUNT-1503: `tectora calc` and `info` call a file a
//! newer Tectora saved "newer", also when this version cannot parse it
//! (nested deeper than serde reads, or cut short by a sync), as the app
//! and the MCP server do.

use std::path::PathBuf;
use std::process::Command;

fn job(name: &str, text: &[u8]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tectora-cli-newer-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{name}.tectora"));
    std::fs::write(&path, text).unwrap();
    path
}

/// stdout and stderr of `tectora <cmd> <path>`, and its exit code.
fn run(cmd: &str, path: &PathBuf) -> (String, Option<i32>) {
    let mut c = Command::new(env!("CARGO_BIN_EXE_tectora"));
    c.arg(cmd).arg(path);
    if cmd == "calc" {
        c.arg("--summary");
    }
    let out = c.output().unwrap();
    (
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
        out.status.code(),
    )
}

#[test]
fn calc_and_info_call_an_unparsable_newer_file_newer() {
    let v = tectora_core::model::SCHEMA_VERSION + 96;
    let plain = format!("{{\"schema_version\":{v},\"name\":\"Střecha\",\"parts\":[]}}");
    let deep = format!(
        "{{\"schema_version\":{v},\"name\":\"Střecha\",\"crm\":{}{}}}",
        "[".repeat(200),
        "]".repeat(200)
    );
    let cut = &plain.as_bytes()[..plain.find("ře").unwrap() + 1];
    for (name, text) in [
        ("plain", plain.as_bytes()),
        ("deep", deep.as_bytes()),
        ("cut", cut),
    ] {
        let path = job(name, text);
        for cmd in ["calc", "info"] {
            let (out, code) = run(cmd, &path);
            assert!(
                out.contains(&format!("saved by a newer Tectora (format {v}")),
                "{cmd} {name}: {out}"
            );
            assert_ne!(code, Some(0), "{cmd} {name}");
        }
    }
}
