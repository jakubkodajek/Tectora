//! The work bound of a whole-job calculation (audit HUNTIO-004): buildings
//! saved on top of each other, inside every count limit, kept a core busy
//! for minutes (25 copies 50 s, 200 never ended) in the app, the CLI and
//! the browser preview. Counted, not timed: these tests measure steps.

use tectora_core::geom::Point2;
use tectora_core::model::{DEFAULT_MATERIAL, EdgeDef, Project, RoofPart};
use tectora_core::work::{LIMIT, TooComplex, bounded_by};

fn edge(x: f64, y: f64) -> EdgeDef {
    EdgeDef {
        start: Point2::new(x, y),
        pitch_deg: 35.0,
        eave_height: 3.0,
        overhang: 0.5,
        half_hip: None,
        breaks: Vec::new(),
        priority: 0,
        direction_rad: None,
        clad: false,
        source: None,
        material: DEFAULT_MATERIAL,
    }
}

/// `n` copies of a 10-wall round building, each turned 0.01 rad: a job
/// saved onto itself, or one MCP rotate with many copies.
fn stacked(n: usize) -> Project {
    let mut p = Project::new("stacked");
    for k in 0..n {
        let edges = (0..10)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / 10.0 + k as f64 * 0.01;
                edge(5.0 * a.cos(), 5.0 * a.sin())
            })
            .collect();
        p.parts.push(RoofPart::new(format!("P{k}"), edges));
    }
    p
}

/// `n` separate 10 x 8 m buildings in rows of 20 (a terraced estate).
fn separate(n: usize) -> Project {
    let mut p = Project::new("estate");
    for i in 0..n {
        let (x, y) = ((i % 20) as f64 * 15.0, (i / 20) as f64 * 12.0);
        let edges = [(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)]
            .iter()
            .map(|&(dx, dy)| edge(x + dx, y + dy))
            .collect();
        p.parts.push(RoofPart::new(format!("P{i}"), edges));
    }
    p
}

#[test]
fn buildings_stacked_on_each_other_stop_at_the_bound_instead_of_running_for_minutes() {
    let p = stacked(50);
    p.check_limits().expect("inside every count limit");
    // With a small bound the calculation stops there: the joins count
    // their work, wherever it goes.
    let (r, used) = bounded_by(5_000_000, || {
        tectora_core::openings::generate_project(&p).len()
    });
    assert_eq!(r, Err(TooComplex));
    assert_eq!(used, 5_000_000);
}

#[test]
fn ordinary_jobs_stay_far_below_the_bound() {
    // A few stacked copies and a large estate calculate with a small part
    // of the bound, so no real job meets it.
    for p in [stacked(3), separate(200)] {
        p.check_limits().unwrap();
        let (r, used) = bounded_by(LIMIT, || tectora_core::openings::generate_project(&p).len());
        assert_eq!(r, Ok(p.parts.len()), "{}", p.name);
        assert!(used < LIMIT / 50, "{}: {used} steps", p.name);
    }
    for name in [
        "gable", "hip", "half-hip", "pyramid", "shed", "l-shape", "t-shape", "u-shape",
    ] {
        let p = tectora_core::examples::find(name).expect(name).project();
        let (r, used) = bounded_by(LIMIT, || tectora_core::openings::generate_project(&p).len());
        assert!(r.is_ok() && used < LIMIT / 250, "{name}: {used}");
    }
}

#[test]
fn the_app_commands_answer_too_complex_with_its_code() {
    // The bound of `commands`, lowered for this debug build; the other tests
    // here give their bound explicitly.
    // SAFETY: no other test of this binary reads or writes the environment.
    unsafe { std::env::set_var(tectora_core::work::LIMIT_ENV, "5000000") };
    let e = tectora_core::commands::calc(&stacked(50)).unwrap_err();
    assert_eq!(e.code, "too_complex", "{e}");
    assert!(e.detail.contains("too complex to calculate"), "{e}");
    // A nested guard does not swallow the stop into an empty roof.
    let e = tectora_core::commands::export_preview(
        &stacked(50),
        "x".into(),
        "01.01.2026 00:00".into(),
        &Default::default(),
    )
    .unwrap_err();
    assert_eq!(e.code, "too_complex", "{e}");
    // A small job still calculates under the same bound.
    assert!(tectora_core::commands::calc(&stacked(2)).is_ok());
}
