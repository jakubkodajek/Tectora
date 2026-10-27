//! Parity of the model intersection with DACAD's own SOL output.
//!
//! Each case in `tests/fixtures/modell` is an INT file (with a W3D file
//! when the case has walls) and the SOL that DACAD's Modellverschnitt
//! wrote for it: captured from DACAD for our own test buildings, or
//! recorded from an emulation of DACAD's code for the synthetic stress
//! cases.

use std::path::{Path, PathBuf};

use tectora_core::modell::{self, FaceKind, Tol};

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/modell")
}

fn read(p: &Path) -> String {
    String::from_utf8_lossy(&std::fs::read(p).unwrap_or_else(|e| panic!("{}: {e}", p.display())))
        .into_owned()
}

/// (name, INT, W3D, expected SOL) for every case.
fn cases() -> Vec<(String, String, Option<String>, String)> {
    let mut v: Vec<_> = std::fs::read_dir(dir())
        .expect("fixture dir")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "INT"))
        .map(|p| {
            let name = p.file_stem().unwrap().to_string_lossy().into_owned();
            let w3d = p.with_extension("W3D");
            let w3d = w3d.exists().then(|| read(&w3d));
            (name, read(&p), w3d, read(&p.with_extension("SOL")))
        })
        .collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

#[test]
fn sol_matches_dacad() {
    let cases = cases();
    assert!(cases.len() >= 16, "fixtures missing: {}", cases.len());
    let tol = Tol::default();
    let mut bad = Vec::new();
    for (name, int, w3d, want) in &cases {
        let mut st =
            modell::parse(int, w3d.as_deref(), &tol).unwrap_or_else(|e| panic!("{name}: {e}"));
        modell::run_state(&mut st, &tol);
        let got = modell::write_sol(&st, &tol);
        if let Err(d) = modell::compare_sol(&got, want, 1e-5) {
            bad.push(format!("{name}: token {} {:?} vs {:?}", d.token, d.a, d.b));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// The captured case also matches byte for byte, line ends included.
#[test]
fn captured_case_is_byte_exact() {
    let p = dir().join("C67_box_same_plane_short_of_ridge.INT");
    let tol = Tol::default();
    let mut st = modell::parse(&read(&p), Some(&read(&p.with_extension("W3D"))), &tol).unwrap();
    modell::run_state(&mut st, &tol);
    assert_eq!(modell::write_sol(&st, &tol), read(&p.with_extension("SOL")));
}

/// The structured result agrees with the SOL text.
#[test]
fn output_agrees_with_sol() {
    for (name, int, w3d, want) in cases() {
        let out = modell::run(&int, w3d.as_deref()).unwrap();
        let t: Vec<&str> = want.split_whitespace().collect();
        let n_lines: usize = t[1].parse().unwrap();
        assert_eq!(out.lines.len(), n_lines, "{name}");
        let n_faces: usize = t[2 + 9 * n_lines].parse().unwrap();
        assert_eq!(out.faces.len(), n_faces, "{name}");
        for (i, l) in out.lines.iter().enumerate() {
            let code: i32 = t[2 + 9 * i + 7].parse().unwrap();
            assert_eq!(l.code, code, "{name} line {}", i + 1);
        }
        let mut pos = 3 + 9 * n_lines;
        for f in &out.faces {
            assert_eq!(f.face, t[pos].parse::<i32>().unwrap(), "{name}");
            let kind: i32 = t[pos + 2].parse().unwrap();
            match f.kind {
                FaceKind::Surface => assert_eq!(kind, 0, "{name}"),
                FaceKind::Cut(k) => assert_eq!(kind, k, "{name}"),
            }
            assert!(
                (f.area - t[pos + 3].parse::<f64>().unwrap()).abs() <= 1e-5,
                "{name}"
            );
            let n: usize = t[pos + 4].parse().unwrap();
            assert_eq!(f.edges.len(), n, "{name}");
            for (k, &(line, sign)) in f.edges.iter().enumerate() {
                assert!(line < out.lines.len(), "{name}");
                let r: i64 = t[pos + 5 + k].parse().unwrap();
                assert_eq!(r, sign as i64 * (line as i64 + 1), "{name}");
            }
            pos += 5 + n;
        }
    }
}
