//! Setup probe: sends a synthetic test report of N MB to a relay and prints
//! the answer. Usage:
//!
//! ```text
//! cargo run -p tectora-relay --example probe -- <url> <key> [--mb N]
//! ```
//!
//! The report is marked `test`, so the relay's issue gets a `[test]` title.
//! Delete the file and the issue afterwards (docs/bug-reports.md).

use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;
use tectora_relay::{Config, MAX_UPLOAD, send};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut mb: u64 = 1;
    let mut plain: Vec<&String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--mb" {
            mb = args.get(i + 1).and_then(|v| v.parse().ok()).unwrap_or(0);
            i += 2;
        } else {
            plain.push(&args[i]);
            i += 1;
        }
    }
    let (Some(url), Some(key)) = (plain.first(), plain.get(1)) else {
        eprintln!("usage: probe <url> <key> [--mb N]");
        std::process::exit(2);
    };
    if mb == 0 || mb * 1024 * 1024 > MAX_UPLOAD {
        eprintln!("--mb must be between 1 and {}", MAX_UPLOAD / 1024 / 1024);
        std::process::exit(2);
    }
    let Some(cfg) = Config::checked(url, key, true) else {
        eprintln!("the address or the key does not have the expected shape");
        std::process::exit(2);
    };

    // PK header, then noise that does not compress: the size is what counts.
    let mut bytes = vec![0u8; (mb * 1024 * 1024) as usize];
    bytes[..4].copy_from_slice(b"PK\x03\x04");
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    for b in bytes[4..].iter_mut() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *b = (x >> 24) as u8;
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let id: String = (0..2)
        .map(|n| {
            format!(
                "{:016x}",
                now.wrapping_mul(0x2545_F491_4F6C_DD1D)
                    .rotate_left(n * 17 + 3)
            )
        })
        .collect();
    let name = "tectora-report-20000101-000000-probe.zip";
    let dir = std::env::temp_dir().join(format!("tectora-probe-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(name);
    if std::fs::write(&path, &bytes).is_err() {
        eprintln!("cannot write the temporary file");
        std::process::exit(1);
    }
    let meta = json!({
        "test": true,
        "description": format!("Setup probe, {mb} MB. Delete this issue and its file."),
        "source": "probe",
        "reporter": "probe",
        "created_ms": now,
    });
    let started = std::time::Instant::now();
    let result = send(&cfg, &id, name, &meta, &path);
    let _ = std::fs::remove_dir_all(&dir);
    match result {
        Ok(r) => {
            println!(
                "ok file_id={} issue_url={} ({} MB in {:.1} s)",
                r.file_id,
                r.issue_url.as_deref().unwrap_or("none"),
                mb,
                started.elapsed().as_secs_f32()
            );
        }
        Err(e) => {
            println!("failed code={} ({e:?})", e.code());
            std::process::exit(1);
        }
    }
}
