//! Input checks every write path shares (audit 2026-11-21): the
//! standard-roof bounds (ARCH-001), the pitch a gable gets back as an eave
//! (ARCH-005, UI-007), the wall ranges a helper's OK keeps (ARCH-003) and
//! the dormer checks (MATH-001, MATH-003).

use tectora_core::commands;
use tectora_core::edit::calc::BuildUpEave;
use tectora_core::edit::dormers::{self, DormerKind, HostSlope};
use tectora_core::edit::helper::{self, HelperRequest, HipBy, Neighbour};
use tectora_core::edit::{self, check_standard_size};
use tectora_core::geom::Point2;
use tectora_core::{Project, examples};

fn hip() -> Project {
    examples::find("hip").expect("example").project()
}

#[test]
fn a_standard_roof_takes_the_forms_bounds() {
    assert!(check_standard_size(12.0, 8.0).is_ok());
    assert!(check_standard_size(0.1, 10_000.0).is_ok());
    for (l, w) in [
        (10.0, 0.004),
        (1e-300, 1e-300),
        (99_999.0, 99_999.0),
        (f64::NAN, 5.0),
    ] {
        assert!(check_standard_size(l, w).is_err(), "{l} x {w}");
    }
    let e = commands::check_standard_roof(Some("x"), 10.0, 0.004, 40.0, 3.0, 0.5).unwrap_err();
    assert_eq!(e.code, "out_of_range");
    assert!(e.detail.contains("width"), "{}", e.detail);
    assert!(commands::check_standard_roof(None, 10.0, 8.0, 40.0, 3.0, 0.5).is_ok());
}

#[test]
fn a_gable_turned_back_takes_its_own_then_the_nearest_eave_pitch() {
    let mut p = hip();
    let es = &mut p.parts[0].edges;
    for (i, pitch) in [35.0, 33.0, 31.0, 29.0].into_iter().enumerate() {
        es[i].pitch_deg = pitch;
    }
    es[1].pitch_deg = 90.0;
    // Remembered pitch first, then the previous wall's, then the next.
    assert_eq!(edit::eave_pitch(es, 1, Some(37.0), 40.0), 37.0);
    assert_eq!(edit::eave_pitch(es, 1, None, 40.0), 35.0);
    es[0].pitch_deg = 90.0;
    assert_eq!(edit::eave_pitch(es, 1, None, 40.0), 31.0);
    for e in es.iter_mut() {
        e.pitch_deg = 90.0;
    }
    assert_eq!(edit::eave_pitch(es, 1, None, 40.0), 40.0);
}

#[test]
fn a_helper_ok_refuses_values_no_wall_takes() {
    let p = hip();
    // A hip barely longer than its plan diagonal gives both walls a pitch
    // around 1.3°, flatter than any wall takes. The lengths stay well away
    // from the helper's own limits, so every platform's libm agrees.
    let req = HelperRequest::Hip {
        neighbour: Neighbour::Next,
        by: HipBy::Eaves,
        hip: 7.072,
        first: 5.0,
        second: 5.0,
    };
    let e = commands::roof_helper_apply(&p, 0, 0, &req).unwrap_err();
    assert_eq!(e.code, "refused");
    assert!(e.detail.contains("pitch_deg"), "{}", e.detail);
    // A build-up of 99 999 m gives a 57 km overhang.
    let req = HelperRequest::BuildUp {
        thickness: 99_999.0,
        eave: BuildUpEave::default(),
    };
    let e = commands::roof_helper_apply(&p, 0, 0, &req).unwrap_err();
    assert_eq!(e.code, "refused");
    // A sensible build-up still applies.
    let req = HelperRequest::BuildUp {
        thickness: 0.2,
        eave: BuildUpEave::default(),
    };
    assert!(commands::roof_helper_apply(&p, 0, 0, &req).is_ok());
    let mut q = p.clone();
    assert!(helper::apply(&mut q, 0, 0, &req).is_ok());
}

/// The gable example with a gable dormer at y = 1.6 on its 40° face.
fn dormer_job(eave: f64, pitch: f64) -> (Project, f64) {
    let mut p = examples::find("gable").expect("example").project();
    let geos = dormers::host_roof(&p);
    let mut params = dormers::default_params(DormerKind::Gable, Point2::new(4.0, 1.6), 0.0, eave);
    params.pitch_deg = pitch;
    let host = dormers::host_slope_in(&geos, &params).expect("on the roof");
    p.dormers.push(dormers::build_dormer(&params));
    (p, host.height)
}

#[test]
fn a_dormer_below_the_roof_or_flatter_than_2_degrees_is_refused() {
    let (_, roof) = dormer_job(5.0, 45.0);
    let params = |eave: f64, pitch: f64| {
        let mut q = dormers::default_params(DormerKind::Gable, Point2::new(4.0, 1.6), 0.0, eave);
        q.pitch_deg = pitch;
        q
    };
    let host = HostSlope {
        height: roof,
        tan: 0.8,
    };
    assert!(dormers::check_params(&params(roof + 1.0, 45.0), Some(host)).is_ok());
    assert!(dormers::check_params(&params(roof, 45.0), Some(host)).is_ok());
    let (code, why) = dormers::check_params(&params(roof - 0.3, 45.0), Some(host)).unwrap_err();
    assert_eq!(code, "dormer_below_roof");
    assert!(why.contains("wall_height must be 0 or more"), "{why}");
    let (code, _) = dormers::check_params(&params(roof + 1.0, 1.0), None).unwrap_err();
    assert_eq!(code, "dormer_pitch");
    assert!(dormers::check_params(&params(roof + 1.0, 2.0), None).is_ok());
    let mut shed =
        dormers::default_params(DormerKind::Shed, Point2::new(4.0, 1.6), 0.0, roof + 1.0);
    shed.front_pitch_deg = 1.0;
    assert_eq!(
        dormers::check_params(&shed, None).unwrap_err().0,
        "dormer_pitch"
    );

    // The command the dormer bar and dialog ask.
    let (p, _) = dormer_job(roof + 1.0, 45.0);
    assert!(commands::check_dormer(&p, 0).is_ok());
    let (p, _) = dormer_job(roof - 0.262, 45.0);
    assert_eq!(
        commands::check_dormer(&p, 0).unwrap_err().code,
        "dormer_below_roof"
    );
    let (p, _) = dormer_job(roof + 1.0, 1.0);
    assert_eq!(
        commands::check_dormer(&p, 0).unwrap_err().code,
        "dormer_pitch"
    );
    assert_eq!(
        commands::check_dormer(&p, 5).unwrap_err().code,
        "out_of_range"
    );
}

#[test]
fn the_dormer_checks_hold_at_their_edges() {
    // Round 2 TEST-103: the checks of MATH-001 / MATH-003 were tested at one
    // point each, so a 10 cm tolerance below the roof, a 90° pitch and an
    // unchecked half-hip pitch all passed.
    let (_, roof) = dormer_job(5.0, 45.0);
    let host = HostSlope {
        height: roof,
        tan: 0.8,
    };
    let at = |kind: DormerKind, eave: f64| {
        let mut q = dormers::default_params(kind, Point2::new(4.0, 1.6), 0.0, eave);
        q.pitch_deg = 45.0;
        q
    };
    let code = |q: &dormers::DormerParams, host: Option<HostSlope>| {
        dormers::check_params(q, host).err().map(|e| e.0)
    };
    // Below the roof by 5 mm is refused, by half a millimetre (rounding of a
    // pointed dormer's eave) is not.
    assert_eq!(
        code(&at(DormerKind::Gable, roof - 0.005), Some(host)),
        Some("dormer_below_roof")
    );
    assert_eq!(code(&at(DormerKind::Gable, roof - 0.0005), Some(host)), None);
    // A pitch just under 90° is taken, 90° is not.
    let mut q = at(DormerKind::Gable, roof + 1.0);
    q.pitch_deg = 89.9;
    assert_eq!(code(&q, None), None);
    q.pitch_deg = 90.0;
    assert_eq!(code(&q, None), Some("dormer_pitch"));
    // Every pitch a kind uses is checked, not only the main one.
    let mut q = at(DormerKind::HalfHip, roof + 1.0);
    q.half_hip_pitch_deg = 1.0;
    assert_eq!(code(&q, None), Some("dormer_pitch"));
    q.half_hip_pitch_deg = 60.0;
    assert_eq!(code(&q, None), None);
    let mut q = at(DormerKind::Trapezoid, roof + 1.0);
    q.front_pitch_deg = 1.0;
    assert_eq!(code(&q, None), Some("dormer_pitch"));
    q.front_pitch_deg = 30.0;
    assert_eq!(code(&q, None), None);
}
