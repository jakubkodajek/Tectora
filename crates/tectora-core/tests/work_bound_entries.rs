//! Every entry point that calculates a whole job stops at the work bound
//! (audit HUNTIO-004, round 2 ARCH-102, ARCH-106, TEST-102): the library
//! card, the dormer bar's check, the bundle preview and the PDF / SVG
//! sheet. The bound is lowered for this debug build through its
//! environment variable (its own test binary, so no other test sees it);
//! each call runs under a clock bound too, so a lost work bound fails the
//! test instead of hanging it.

use std::sync::Once;
use std::time::Duration;

use tectora_core::geom::Point2;
use tectora_core::model::{DEFAULT_MATERIAL, EdgeDef, Project, RoofPart};

fn low_bound() {
    static SET: Once = Once::new();
    // SAFETY: set once, before any test of this binary calculates; no test
    // here reads the environment otherwise.
    SET.call_once(|| unsafe { std::env::set_var(tectora_core::work::LIMIT_ENV, "5000000") });
}

/// Runs `f` on its own thread and fails when it takes longer than `secs`
/// (the unbounded calculation of [`stacked`] takes many minutes).
fn within<T: Send + 'static>(secs: u64, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(Duration::from_secs(secs))
        .unwrap_or_else(|_| panic!("still calculating after {secs} s: the work bound is lost"))
}

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

/// `n` copies of a 10-wall round building, each turned 0.01 rad.
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

fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("tectora-bound-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn the_card_of_a_job_too_complex_to_calculate_says_so_and_frees_its_core() {
    // ARCH-102: the card's figures ran unbounded on a background thread, so
    // the card stayed "pending" for as long as the app ran.
    low_bound();
    let dir = temp_dir("card");
    let store = tectora_core::store::Store::new(dir.clone()).unwrap();
    let stack = store.save(None, &stacked(50)).unwrap();
    let small = store.save(None, &stacked(2)).unwrap();
    let card = within(120, move || {
        let start = std::time::Instant::now();
        loop {
            let s = store.summarize(&stack).unwrap();
            if !s.figures_pending {
                let ok = store.summarize(&small).unwrap();
                return (s, ok);
            }
            assert!(start.elapsed() < Duration::from_secs(110), "never resolved");
            std::thread::sleep(Duration::from_millis(50));
        }
    });
    let (s, ok) = card;
    assert!(s.too_complex, "{s:?}");
    assert_eq!((s.parts, s.parts_failed), (50, 50), "{s:?}");
    assert_eq!(s.roof_area, 0.0);
    // A job inside the bound keeps its figures.
    assert!(
        !ok.too_complex && ok.parts_failed == 0 && ok.roof_area > 0.0,
        "{ok:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A stacked job with a gable dormer on the south face of its buildings.
fn stacked_with_dormer(n: usize) -> Project {
    use tectora_core::edit::dormers::{self, DormerKind};
    let mut p = stacked(n);
    let params = dormers::default_params(DormerKind::Gable, Point2::new(-0.6, -3.5), 0.0, 4.5);
    p.dormers.push(dormers::build_dormer(&params));
    p
}

#[test]
fn the_dormer_check_of_a_job_too_complex_answers_too_complex() {
    // ARCH-106 item 3: `check_dormer` calculated the host roof unbounded on
    // every committed field of the dormer bar.
    low_bound();
    let e = within(120, || {
        tectora_core::commands::check_dormer(&stacked_with_dormer(50), 0).unwrap_err()
    });
    assert_eq!(e.code, "too_complex", "{e}");
    // The same dormer on a job inside the bound is checked as before.
    let small = stacked_with_dormer(1);
    let host = tectora_core::commands::dormer_host(&small, 0);
    assert!(host.is_ok(), "{host:?}");
    let r = tectora_core::commands::check_dormer(&small, 0);
    assert!(
        r.as_ref().is_ok() || r.as_ref().is_err_and(|e| e.code.starts_with("dormer_")),
        "{r:?}"
    );
}

#[test]
fn the_plan_sheet_of_a_job_too_complex_is_refused_in_both_formats() {
    // TEST-102: the app's PDF / SVG sheet (`checked_sheet_file_named`).
    low_bound();
    for format in ["pdf", "svg"] {
        let e = within(120, move || {
            tectora_core::export::checked_sheet_file_named(
                &stacked(50),
                "01.01.2026 00:00",
                format,
                &Default::default(),
            )
            .unwrap_err()
        });
        assert_eq!(e, tectora_core::work::TOO_COMPLEX, "{format}");
    }
    let ok = tectora_core::export::checked_sheet_file_named(
        &stacked(2),
        "01.01.2026 00:00",
        "svg",
        &Default::default(),
    );
    assert!(ok.is_ok_and(|b| !b.is_empty()));
}

#[test]
fn a_job_too_complex_is_exported_as_a_bundle_without_its_preview() {
    // TEST-102: the bundle's preview is calculated in the bound; the job,
    // its history and files are still exported.
    low_bound();
    let dir = temp_dir("bundle");
    let lib = dir.join("lib");
    let store = tectora_core::store::Store::new(lib).unwrap();
    let id = store.save(None, &stacked(50)).unwrap();
    let dest = dir.join("stack.tectorajob");
    let to = dest.clone();
    let manifest = within(120, move || {
        tectora_core::bundle::export(&store, &id, &to).unwrap()
    });
    assert!(
        !manifest.files.iter().any(|f| f.contains("preview")),
        "{manifest:?}"
    );
    let zip = zip::ZipArchive::new(std::fs::File::open(&dest).unwrap()).unwrap();
    let names: Vec<String> = zip
        .file_names()
        .filter_map(Result::ok)
        .map(|n| n.to_string())
        .collect();
    assert!(names.iter().any(|n| n == "project.tectora"), "{names:?}");
    assert!(!names.iter().any(|n| n == "preview.svg"), "{names:?}");
    let _ = std::fs::remove_dir_all(&dir);
}
