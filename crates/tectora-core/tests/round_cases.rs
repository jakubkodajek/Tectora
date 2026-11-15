//! The round-roof helper against the pitches and pitch breaks DACAD itself
//! set (TEST-006): runs RD1-RD4 (Runddach on a 6 x 4 and a 6 x 10 house:
//! default arc, radius 4, ridge 4.5) and RGa-RGs (Rundgaube fronts of
//! several widths and overhangs).
//!
//! The captures are DACAD material and are not in the repository: point
//! `DACAD_ROUND` at the folder that holds `knicks.txt` (and
//! `rundgaube/knicks.txt`), the wall-by-wall dump of those runs, to run
//! this. Each round wall lists one row per plane: the first row is the
//! wall's pitch, eave height and overhang, the others the pitch breaks.

use std::path::Path;

use tectora_core::edit::round::{ArcInput, ArcMode, arc_for};

#[derive(Debug)]
struct Wall {
    a: (f64, f64),
    b: (f64, f64),
    /// (pitch, eave or break height, overhang) per plane.
    rows: Vec<(f64, f64, f64)>,
}

#[derive(Debug)]
struct Case {
    name: String,
    walls: Vec<Wall>,
}

fn num(s: &str) -> f64 {
    s.trim()
        .parse()
        .unwrap_or_else(|_| panic!("not a number: {s:?}"))
}

fn point(s: &str) -> (f64, f64) {
    let s = s.trim().trim_start_matches('(').trim_end_matches(')');
    let (x, y) = s.split_once(',').expect("x, y");
    (num(x), num(y))
}

fn parse(text: &str) -> Vec<Case> {
    let mut cases: Vec<Case> = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if !line.starts_with(' ') {
            let name = t
                .split_whitespace()
                .next()
                .unwrap()
                .trim_end_matches(".DAD");
            cases.push(Case {
                name: name.to_string(),
                walls: Vec::new(),
            });
        } else if let Some(rest) = t.strip_prefix("wall ") {
            let (_, pts) = rest.split_once(' ').expect("wall n (..)->(..)");
            let (a, b) = pts.split_once("->").expect("->");
            let case = cases.last_mut().expect("a case before its walls");
            case.walls.push(Wall {
                a: point(a),
                b: point(b),
                rows: Vec::new(),
            });
        } else if t.starts_with("pitch ") {
            let f: Vec<&str> = t.split_whitespace().collect();
            assert_eq!((f[0], f[2], f[4]), ("pitch", "eave", "ov"), "{line}");
            let wall = cases
                .last_mut()
                .and_then(|c| c.walls.last_mut())
                .expect("a wall before its rows");
            wall.rows.push((num(f[1]), num(f[3]), num(f[5])));
        }
    }
    cases
}

/// The arc DACAD was asked for, from the run's name.
fn mode(name: &str) -> (ArcMode, f64) {
    let after = |key: &str| {
        name.split_once(key).map(|(_, v)| {
            num(v
                .split(|c: char| !(c.is_ascii_digit() || c == '.'))
                .next()
                .unwrap())
        })
    };
    if let Some(r) = after("radius") {
        (ArcMode::Radius, r)
    } else if let Some(h) = after("ridge") {
        (ArcMode::Ridge, h)
    } else {
        (ArcMode::Default, 0.0)
    }
}

/// Distance of `q` from the line through wall `w`.
fn distance(w: &Wall, q: (f64, f64)) -> f64 {
    let (dx, dy) = (w.b.0 - w.a.0, w.b.1 - w.a.1);
    let l = dx.hypot(dy);
    ((q.0 - w.a.0) * dy - (q.1 - w.a.1) * dx).abs() / l
}

fn check(dir: &Path, file: &str) -> (usize, Vec<String>) {
    let path = dir.join(file);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let cases = parse(&text);
    assert!(!cases.is_empty(), "no runs in {}", path.display());
    let mut bad = Vec::new();
    for case in &cases {
        let round: Vec<&Wall> = case.walls.iter().filter(|w| w.rows.len() > 1).collect();
        assert_eq!(round.len(), 2, "{}: two round walls", case.name);
        let (first, other) = (round[0], round[1]);
        let (pitch, eave, overhang) = first.rows[0];
        let (mode, value) = mode(&case.name);
        let input = ArcInput {
            span: distance(first, other.a),
            overhang,
            eave_height: eave,
            segments: 0,
            mode,
            value,
        };
        let Some(arc) = arc_for(&input) else {
            bad.push(format!("{}: no arc for {input:?}", case.name));
            continue;
        };
        let want: Vec<(f64, f64)> = first.rows[1..].iter().map(|r| (r.1, r.0)).collect();
        let got: Vec<(f64, f64)> = arc.breaks.iter().map(|b| (b.height, b.pitch_deg)).collect();
        let close = |a: f64, b: f64| (a - b).abs() < 2e-4;
        let same = close(arc.pitch_deg, pitch)
            && got.len() == want.len()
            && got
                .iter()
                .zip(&want)
                .all(|(g, w)| close(g.0, w.0) && close(g.1, w.1));
        if !same {
            bad.push(format!(
                "{}: {input:?}\n  DACAD pitch {pitch}, breaks (height, pitch) {want:?}\n  ours  pitch {}, breaks {got:?}",
                case.name, arc.pitch_deg
            ));
        }
        // Both walls get the same arc.
        assert_eq!(
            first.rows, other.rows,
            "{}: the facing wall differs",
            case.name
        );
    }
    (cases.len(), bad)
}

#[test]
#[ignore = "needs the private DACAD captures: set DACAD_ROUND to their folder"]
fn round_roof_and_round_dormer_match_dacad() {
    let dir = std::env::var("DACAD_ROUND").expect("DACAD_ROUND");
    let dir = Path::new(&dir);
    let mut bad = Vec::new();
    let mut runs = 0;
    for file in ["knicks.txt", "rundgaube/knicks.txt"] {
        let (n, b) = check(dir, file);
        runs += n;
        bad.extend(b);
    }
    assert!(
        bad.is_empty(),
        "{} of {runs} runs differ:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

#[test]
fn the_capture_format_is_read() {
    // The dump format of the captures, with made-up numbers: no DACAD
    // values in the repository.
    let text = "RX_radius4\n  wall 1 (0.0, 0.0)->(6.0, 0.0)\n    pitch 30.0000  eave 3.0000  ov 0.500\n    pitch 10.0000  eave 3.5000  ov 0.000\n  wall 2 (6.0, 0.0)->(6.0, 4.0)\n    pitch 90.0000  eave 3.0000  ov 0.500\nRY.DAD  totals {'F': 1.0}\n  wall 5 (4.0, 3.0)->(4.0, 0.5)\n    pitch 33.0000  eave 4.1000  ov 0.000\n";
    let cases = parse(text);
    assert_eq!(cases.len(), 2);
    assert_eq!(cases[0].name, "RX_radius4");
    assert_eq!(cases[1].name, "RY");
    assert_eq!(
        cases[0].walls[0].rows,
        vec![(30.0, 3.0, 0.5), (10.0, 3.5, 0.0)]
    );
    assert_eq!(cases[1].walls[0].a, (4.0, 3.0));
    assert_eq!(mode("RD2_radius4"), (ArcMode::Radius, 4.0));
    assert_eq!(mode("RD3_ridge4.5"), (ArcMode::Ridge, 4.5));
    assert_eq!(mode("RGs_145"), (ArcMode::Default, 0.0));
    let w = &cases[0].walls[0];
    assert!((distance(w, (6.0, 4.0)) - 4.0).abs() < 1e-12);
}
