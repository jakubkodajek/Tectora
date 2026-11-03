//! Parity of the core SOL, INT, W3D and SOL-with-walls writers with
//! DACAD's own files.
//!
//! Each case in `tests/fixtures/coresol` is an RD2 with the files DACAD's
//! code wrote for it: the core SOL (`.SO2`), the INT (multi-roof cases),
//! the W3D and the SOL with walls. C67 was captured from DACAD; the other
//! cases are our own synthetic RD2s (FX*, and MR*: two roofs of our random
//! mass test combined) recorded from an emulation of DACAD's code.
//!
//! - FX01: chimney hole, a K face, "-0.0000" and printf ties (x.015625).
//! - FX02: dormer (T/G/t/g models) next to a second roof.
//! - FX03: collinear walls merged, a point wall.
//! - FX04: three roofs, so the hole outlines come out in file order.
//! - MR35: an eave exactly 0.01 beside a wall line (the wall stage
//!   compares with double tolerances).
//! - MR56: a dormer front flush with another roof's eave (edge flag of
//!   the dormer wall's bottom edge).

use std::path::{Path, PathBuf};

use tectora_core::coresol::{self, CoreSol, IntGeometry, Rd2};

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/coresol")
}

fn read(p: &Path) -> String {
    String::from_utf8_lossy(&std::fs::read(p).unwrap_or_else(|e| panic!("{}: {e}", p.display())))
        .into_owned()
}

fn cases() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir())
        .expect("fixture dir")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "RD2"))
        .collect();
    v.sort();
    assert!(v.len() >= 4, "fixtures missing: {}", v.len());
    v
}

fn name(p: &Path) -> String {
    p.file_stem().unwrap().to_string_lossy().into_owned()
}

#[test]
fn core_sol_round_trips() {
    for p in cases() {
        let want = read(&p.with_extension("SO2"));
        let sol = CoreSol::parse(&want).unwrap();
        assert_eq!(sol.write(), want, "{}", name(&p));
    }
}

#[test]
fn w3d_and_wall_sol_match_dacad() {
    for p in cases() {
        let sol = CoreSol::parse(&read(&p.with_extension("SO2"))).unwrap();
        let ws = coresol::wall_stage(&sol).unwrap();
        assert_eq!(ws.w3d, read(&p.with_extension("W3D")), "{} W3D", name(&p));
        assert_eq!(ws.sol, read(&p.with_extension("SOL")), "{} SOL", name(&p));
    }
}

/// Cases where the face polygons rebuilt from the SOL lines differ from
/// DACAD's in-memory polygons (here a "-0.00000" of a shared SOL line
/// where the dormer wall's own polygon has +0).
const INT_FROM_SOL_INEXACT: &[&str] = &["FX02_dormer_two_roofs"];

#[test]
fn int_matches_dacad() {
    let mut n = 0;
    for p in cases() {
        let ip = p.with_extension("INT");
        if !ip.exists() {
            continue;
        }
        let rd2 = Rd2::parse(&read(&p)).unwrap();
        let sol = CoreSol::parse(&read(&p.with_extension("SO2"))).unwrap();
        let (got, want) = (coresol::write_int(&rd2, &sol).unwrap(), read(&ip));
        if INT_FROM_SOL_INEXACT.contains(&name(&p).as_str()) {
            let (a, b): (Vec<&str>, Vec<&str>) = (
                got.split_whitespace().collect(),
                want.split_whitespace().collect(),
            );
            assert_eq!(a.len(), b.len(), "{}", name(&p));
            for (x, y) in a.iter().zip(&b) {
                let close = x == y
                    || matches!((x.parse::<f64>(), y.parse::<f64>()),
                        (Ok(u), Ok(v)) if (u - v).abs() <= 3e-4);
                assert!(close, "{}: {x} vs {y}", name(&p));
            }
            assert_ne!(
                got,
                want,
                "{} is exact now: drop it from the list",
                name(&p)
            );
        } else {
            assert_eq!(got, want, "{}", name(&p));
        }
        n += 1;
    }
    assert!(n >= 3);
}

/// Face polygons and flags handed in directly are what gets written.
#[test]
fn int_takes_given_polygons_and_flags() {
    let p = dir().join("C67_box_same_plane_short_of_ridge.RD2");
    let rd2 = Rd2::parse(&read(&p)).unwrap();
    let sol = CoreSol::parse(&read(&p.with_extension("SO2"))).unwrap();
    let mut g = IntGeometry::from_sol(&rd2, &sol).unwrap();
    assert_eq!(
        coresol::write_int_geometry(&rd2, &g).unwrap(),
        coresol::write_int(&rd2, &sol).unwrap()
    );
    let e = &mut g.faces[0].edges[0];
    e.p[0] += 0.25;
    e.flag = 1 - e.flag;
    let (x, fl) = (e.p[0], e.flag);
    let text = coresol::write_int_geometry(&rd2, &g).unwrap();
    let want = format!("{} {}", coresol::num::f5(x as f64).trim(), fl);
    let block = text.split("Flaechen").nth(1).unwrap();
    let toks: Vec<&str> = block.split_whitespace().collect();
    // Header (6), three vectors and the angle (10), the edge count (2).
    let edge: Vec<&str> = toks[18..25].to_vec();
    assert_eq!(format!("{} {}", edge[0], edge[6]), want);
}

#[test]
fn rd2_reader_details() {
    let rd2 = Rd2::parse(&read(&dir().join("FX01_two_hips_chimney.RD2"))).unwrap();
    // The core prepends models: the last model of the file comes first.
    assert_eq!(rd2.models.iter().map(|m| m.no).collect::<Vec<_>>(), [2, 1]);
    // "-0.0000" reads as +0.0.
    let w = &rd2.models[1].walls[0];
    assert!(w.start[0] == 0.0 && w.start[0].is_sign_positive());
    assert_eq!(rd2.holes.len(), 1);
    assert_eq!(rd2.holes[0].points.len(), 4);

    // Collinear walls with equal faces are merged, the point wall stays.
    let rd2 = Rd2::parse(&read(&dir().join("FX03_merge_zero.RD2"))).unwrap();
    let m = rd2.models.iter().find(|m| m.no == 1).unwrap();
    let nos: Vec<i32> = m.ring_walls().map(|(w, _)| w.no).collect();
    assert_eq!(nos.len(), 5, "{nos:?}");
    let (w1, r1) = m.ring_walls().find(|(w, _)| w.no == 1).unwrap();
    assert_eq!(w1.end, [6.0, 0.0]);
    assert_eq!(r1.end, [11.0, 0.0]);
    assert!(m.ring_walls().any(|(w, _)| w.zero));
}
