//! A catalogue of realistic roofs a roofer draws: rectangles, L, T, U, H
//! and cross plans, bays, chamfered and trapezoid plans; hipped, gabled,
//! half-hipped and mansard roofs; wings with their own pitch, lower or
//! higher eaves and other widths; mixed overhangs; joined buildings and
//! dormers. Every roof must calculate and pass the structural checks of
//! `support::check_structure_fit` (planar faces at their pitch, faces
//! tiling the eave outline, every face side on a roof line, no
//! plausibility warning).
//!
//! Each roof here was also run through DACAD's own core (the emulator in
//! memory `dacad-emulators`) when the suite was written (2026-11-14):
//! Tectora's result was byte-identical for all of them but the pinned
//! `LEAN_TO_WINGS`, on which DACAD's core stops and Tectora calculates the
//! wing against the house (Jakub's decision 2026-11-14). A failure here is
//! a regression of Tectora, not a DACAD quirk; which roofs leave the
//! standard result is pinned, so that drift away from it shows.
//!
//! The second half holds the roofs DACAD's core stops on (review board
//! 2026-11-14-9d5d68c and ERR-001): each must either calculate soundly
//! with a notice or keep the error, never a wrong roof.

use tectora_core::check::WarningKind;
use tectora_core::edit::dormers::{DormerKind, build_dormer, default_params};
use tectora_core::geom::{Point2, signed_area};
use tectora_core::{Chimney, EdgeDef, HalfHip, PitchBreak, Project, RoofPart, RoofWindow};

mod support;
use support::*;

// ---------------------------------------------------------------- plans

/// A plan: its corners (counter-clockwise), the walls at the ends of its
/// ridges (gable or half-hip candidates) and the walls of a wing (its two
/// sides, then its front), if it has one.
struct Plan {
    name: String,
    pts: Vec<(f64, f64)>,
    ends: Vec<usize>,
    wing: Option<[usize; 3]>,
}

fn plan(name: &str, pts: &[(f64, f64)], ends: &[usize], wing: Option<[usize; 3]>) -> Plan {
    assert!(
        signed_area(
            &pts.iter()
                .map(|&(x, y)| Point2::new(x, y))
                .collect::<Vec<_>>()
        ) > 0.0,
        "{name} must be counter-clockwise"
    );
    Plan {
        name: name.into(),
        pts: pts.to_vec(),
        ends: ends.to_vec(),
        wing,
    }
}

/// Main house `w × d` with a wing `ww × wd` in front of it, `x0` from the
/// left (Jakub's T: 15 × 5 with a 5 × 2.5 wing).
fn t_plan(w: f64, d: f64, x0: f64, ww: f64, wd: f64) -> Plan {
    plan(
        &format!("T {w}x{d} wing {ww}x{wd}@{x0}"),
        &[
            (x0, 0.0),
            (x0 + ww, 0.0),
            (x0 + ww, wd),
            (w, wd),
            (w, wd + d),
            (0.0, wd + d),
            (0.0, wd),
            (x0, wd),
        ],
        &[3, 5, 0],
        Some([1, 7, 0]),
    )
}

fn plans() -> Vec<Plan> {
    let mut v = vec![
        plan(
            "rect 10x8",
            &[(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)],
            &[1, 3],
            None,
        ),
        plan(
            "rect 14x6",
            &[(0.0, 0.0), (14.0, 0.0), (14.0, 6.0), (0.0, 6.0)],
            &[1, 3],
            None,
        ),
        plan(
            "L 12x10 arms 5",
            &[
                (0.0, 0.0),
                (12.0, 0.0),
                (12.0, 5.0),
                (5.0, 5.0),
                (5.0, 10.0),
                (0.0, 10.0),
            ],
            &[1, 4],
            None,
        ),
        plan(
            "L 13x9 arms 6/4",
            &[
                (0.0, 0.0),
                (13.0, 0.0),
                (13.0, 6.0),
                (4.0, 6.0),
                (4.0, 9.0),
                (0.0, 9.0),
            ],
            &[1, 4],
            None,
        ),
        t_plan(15.0, 5.0, 5.0, 5.0, 2.5),
        t_plan(15.0, 5.0, 5.0, 3.0, 4.0),
        t_plan(15.0, 6.0, 4.0, 7.0, 4.0),
        t_plan(12.0, 8.0, 4.0, 4.0, 3.0),
        plan(
            "U 14x9 arms 4",
            &[
                (0.0, 0.0),
                (14.0, 0.0),
                (14.0, 9.0),
                (10.0, 9.0),
                (10.0, 4.0),
                (4.0, 4.0),
                (4.0, 9.0),
                (0.0, 9.0),
            ],
            &[2, 6],
            None,
        ),
        plan(
            "H 14x12",
            &[
                (0.0, 0.0),
                (4.0, 0.0),
                (4.0, 4.0),
                (10.0, 4.0),
                (10.0, 0.0),
                (14.0, 0.0),
                (14.0, 12.0),
                (10.0, 12.0),
                (10.0, 8.0),
                (4.0, 8.0),
                (4.0, 12.0),
                (0.0, 12.0),
            ],
            &[0, 4, 6, 10],
            None,
        ),
        plan(
            "cross 13x13 arms 5",
            &[
                (4.0, 0.0),
                (9.0, 0.0),
                (9.0, 4.0),
                (13.0, 4.0),
                (13.0, 9.0),
                (9.0, 9.0),
                (9.0, 13.0),
                (4.0, 13.0),
                (4.0, 9.0),
                (0.0, 9.0),
                (0.0, 4.0),
                (4.0, 4.0),
            ],
            &[0, 3, 6, 9],
            Some([1, 11, 0]),
        ),
        plan(
            "step 16x8",
            &[
                (0.0, 0.0),
                (10.0, 0.0),
                (10.0, 2.0),
                (16.0, 2.0),
                (16.0, 8.0),
                (6.0, 8.0),
                (6.0, 6.0),
                (0.0, 6.0),
            ],
            &[3, 7],
            None,
        ),
        plan(
            "bay 12x8 bay 3x1",
            &[
                (4.5, 0.0),
                (7.5, 0.0),
                (7.5, 1.0),
                (12.0, 1.0),
                (12.0, 9.0),
                (0.0, 9.0),
                (0.0, 1.0),
                (4.5, 1.0),
            ],
            &[3, 5, 0],
            Some([1, 7, 0]),
        ),
        plan(
            "chamfer 12x8",
            &[
                (0.0, 0.0),
                (10.0, 0.0),
                (12.0, 2.0),
                (12.0, 8.0),
                (0.0, 8.0),
            ],
            &[4],
            None,
        ),
        plan(
            "trapezoid 12x7",
            &[(0.0, 0.0), (12.0, 0.0), (10.0, 7.0), (2.0, 7.0)],
            &[],
            None,
        ),
        plan(
            "irregular 5 walls",
            &[(0.0, 0.0), (9.0, 0.0), (11.0, 4.0), (6.0, 8.0), (-1.0, 5.0)],
            &[],
            None,
        ),
        plan(
            "small walls 10x8 with 0.3 jog",
            &[
                (0.0, 0.0),
                (6.0, 0.0),
                (6.0, 0.3),
                (10.0, 0.3),
                (10.0, 8.0),
                (0.0, 8.0),
            ],
            &[3, 5],
            None,
        ),
    ];
    v.retain(|p| !p.pts.is_empty());
    v
}

// ---------------------------------------------------------------- roofs

/// How the walls of a plan are roofed.
#[derive(Clone, Copy, Debug)]
enum Style {
    /// All walls eaves at one pitch.
    Hip(f64),
    /// The plan's end walls are gables.
    Gable(f64),
    /// The end walls are half-hipped gables.
    HalfHip(f64),
    /// Steep lower plane, a pitch break to a flatter upper plane.
    Mansard,
    /// End walls steeper than the others.
    MixedPitch(f64, f64),
}

const EAVE: f64 = 3.0;

fn roof(p: &Plan, style: Style, ov_eave: f64, ov_end: f64) -> Project {
    let n = p.pts.len();
    let edges = p
        .pts
        .iter()
        .enumerate()
        .map(|(i, &(x, y))| {
            let next = p.pts[(i + 1) % n];
            let len = (next.0 - x).hypot(next.1 - y);
            let end = p.ends.contains(&i);
            let ov = if end { ov_end } else { ov_eave };
            let mut e = edge(x, y, 40.0, EAVE, ov);
            match style {
                Style::Hip(a) => e.pitch_deg = a,
                Style::Gable(a) => e.pitch_deg = if end { 90.0 } else { a },
                Style::HalfHip(a) => {
                    e.pitch_deg = if end { 90.0 } else { a };
                    if end {
                        // Halfway up to the ridge over this gable.
                        e.half_hip = Some(HalfHip {
                            height: EAVE + 0.25 * len * tan(a),
                            pitch_deg: 60.0,
                        });
                    }
                }
                Style::Mansard => {
                    e.pitch_deg = 65.0;
                    e.breaks = vec![PitchBreak {
                        height: EAVE + 1.5,
                        pitch_deg: 30.0,
                    }];
                }
                Style::MixedPitch(a, b) => e.pitch_deg = if end { b } else { a },
            }
            e
        })
        .collect();
    project(edges)
}

/// Plan area the faces must cover: the walls pushed out by their own
/// overhangs.
fn eave_plan(p: &Project) -> f64 {
    let part = &p.parts[0];
    let pts = part.outline();
    let d: Vec<f64> = part.edges.iter().map(|e| e.overhang).collect();
    signed_area(&offset_each(&pts, &d))
}

fn offset_each(pts: &[Point2], d: &[f64]) -> Vec<Point2> {
    let n = pts.len();
    let line = |i: usize| {
        let (a, b) = (pts[i % n], pts[(i + 1) % n]);
        let len = a.distance(b);
        let (nx, ny) = ((b.y - a.y) / len, -(b.x - a.x) / len);
        let o = d[i % n];
        (
            Point2::new(a.x + nx * o, a.y + ny * o),
            (b.x - a.x, b.y - a.y),
        )
    };
    (0..n)
        .map(|i| {
            let (p0, d0) = line(i + n - 1);
            let (p1, d1) = line(i);
            let den = d0.0 * d1.1 - d0.1 * d1.0;
            let t = ((p1.x - p0.x) * d1.1 - (p1.y - p0.y) * d1.0) / den;
            Point2::new(p0.x + d0.0 * t, p0.y + d0.1 * t)
        })
        .collect()
}

fn label(p: &Project) -> String {
    let e: Vec<String> = p.parts[0]
        .edges
        .iter()
        .map(|e| {
            format!(
                "({:.2},{:.2} p{} h{} o{}{}{})",
                e.start.x,
                e.start.y,
                e.pitch_deg,
                e.eave_height,
                e.overhang,
                e.half_hip
                    .as_ref()
                    .map_or(String::new(), |h| format!(" hh{}", h.height)),
                if e.breaks.is_empty() { "" } else { " brk" }
            )
        })
        .collect();
    e.join(" ")
}

/// With `TECTORA_DUMP_CASES=DIR` every roof is also written there as a
/// project file, to run it through DACAD's core (`engine_check --rd2`,
/// then the emulator) when the catalogue changes.
fn dump(name: &str, p: &Project) {
    if let Some(dir) = std::env::var_os("TECTORA_DUMP_CASES") {
        let file: String = name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let path = std::path::Path::new(&dir).join(format!("{file}.json"));
        std::fs::write(path, serde_json::to_string(p).expect("project serialises"))
            .expect("dump folder is writable");
    }
}

/// Takes off the notices of what Tectora does beyond DACAD (planes joined,
/// a wing calculated against the house, a roof from the sweep after the
/// core stopped): those roofs passed the self-check, the notice only
/// tells the user. Returns the kinds taken off.
fn take_notices(roof: &mut Roof) -> Vec<WarningKind> {
    let mut out = Vec::new();
    for g in &mut roof.geos {
        g.warnings.retain(|w| {
            let notice = matches!(
                w.kind,
                WarningKind::MergedPlanes
                    | WarningKind::WingJoined
                    | WarningKind::NotCalculatedByCore
            );
            // PlaneOverWall stays: `sound` checks that none of the
            // catalogue's roofs (wings with their own ridge) gets it.
            if notice {
                out.push(w.kind);
            }
            !notice
        });
    }
    out
}

/// Checks one catalogue roof and returns the notices it got.
fn sound(rep: &mut Report, name: &str, p: &Project) -> Vec<WarningKind> {
    dump(name, p);
    let mut roof = calc(p);
    let notices = take_notices(&mut roof);
    let mut c = rep.case();
    no_error(&mut c, &roof);
    let over: Vec<_> = roof
        .geos
        .iter()
        .flat_map(|g| &g.warnings)
        .filter(|w| w.kind == WarningKind::PlaneOverWall)
        .collect();
    c.ok(&format!("plane over wall {over:?}"), over.is_empty());
    if c.clean() {
        check_structure_fit(&mut c, &roof, eave_plan(p), fit_of(p, &notices));
    }
    c.done(format!("{name}: {}", label(p)));
    notices
}

/// The fit a roof must meet: level eaves tile the outline exactly, eaves
/// at different heights may overlap, a wing joined to the house carries
/// the join's rounding.
fn fit_of(p: &Project, notices: &[WarningKind]) -> Fit {
    let edges = &p.parts[0].edges;
    let level = edges
        .iter()
        .all(|e| (e.eave_height - edges[0].eave_height).abs() < 1e-9);
    if notices.contains(&WarningKind::WingJoined) {
        Fit::DacadJoined
    } else if level {
        Fit::Dacad
    } else {
        Fit::DacadSteps
    }
}

/// Whether the standard calculation alone gives the roof.
fn standard_calculates(p: &Project) -> bool {
    tectora_core::engine::generate(p).is_ok()
}

/// Every plan with every roof style, uniform and mixed overhangs.
#[test]
fn plans_with_every_roof_style_are_sound() {
    let mut rep = Report::default();
    let mut noticed: Vec<String> = Vec::new();
    let styles = [
        Style::Hip(25.0),
        Style::Hip(40.0),
        Style::Hip(55.0),
        Style::Gable(40.0),
        Style::HalfHip(45.0),
        Style::Mansard,
        Style::MixedPitch(35.0, 50.0),
        Style::MixedPitch(45.0, 30.0),
    ];
    for p in plans() {
        for &style in &styles {
            let gabled = matches!(style, Style::Gable(_) | Style::HalfHip(_));
            if gabled && p.ends.is_empty() {
                continue;
            }
            for (ov_eave, ov_end) in [(0.0, 0.0), (0.5, 0.5), (0.6, 0.3)] {
                let pr = roof(&p, style, ov_eave, ov_end);
                let name = format!("{} {style:?} ov {ov_eave}-{ov_end}", p.name);
                if !sound(&mut rep, &name, &pr).is_empty() {
                    noticed.push(name);
                }
            }
        }
    }
    rep.finish("plans_with_every_roof_style_are_sound");
    // Every one of them is the standard result.
    assert!(noticed.is_empty(), "roofs with a notice: {noticed:?}");
}

/// A wing with its own ridge (sides eaves, front a gable or a hip) at a
/// lower, equal or higher eave, steeper or flatter than the house.
#[test]
fn wings_with_other_eave_or_pitch_are_sound() {
    let mut rep = Report::default();
    let mut lean_to: Vec<String> = Vec::new();
    for p in plans() {
        let Some([s1, s2, front]) = p.wing else {
            continue;
        };
        for front_gable in [true, false] {
            for d_eave in [-1.5, -1.0, -0.5, 0.0, 0.4] {
                for wing_pitch in [40.0, 30.0, 50.0] {
                    let mut pr = roof(&p, Style::Gable(40.0), 0.5, 0.5);
                    let edges = &mut pr.parts[0].edges;
                    for &k in &[s1, s2, front] {
                        edges[k].eave_height = EAVE + d_eave;
                        edges[k].pitch_deg = wing_pitch;
                    }
                    if front_gable {
                        edges[front].pitch_deg = 90.0;
                    }
                    let name = format!(
                        "{} wing {} eave {d_eave:+} pitch {wing_pitch}",
                        p.name,
                        if front_gable { "gable" } else { "hip" }
                    );
                    let notices = sound(&mut rep, &name, &pr);
                    // A notice only where the standard calculation stops.
                    assert_eq!(
                        notices.is_empty(),
                        standard_calculates(&pr),
                        "{name}: {notices:?}"
                    );
                    if !notices.is_empty() {
                        assert_eq!(notices, vec![WarningKind::WingJoined], "{name}");
                        lean_to.push(name);
                    }
                }
            }
        }
    }
    rep.finish("wings_with_other_eave_or_pitch_are_sound");
    lean_to.sort();
    let mut want: Vec<String> = LEAN_TO_WINGS.iter().map(|s| s.to_string()).collect();
    want.sort();
    assert_eq!(lean_to, want, "the wings calculated against the house");
}

/// The catalogue's wings DACAD's core stops on (emulator, 2026-11-14):
/// Tectora calculates them against the house (`wing_split.rs`). Every
/// other catalogue roof is the standard result.
const LEAN_TO_WINGS: [&str; 11] = [
    "T 12x8 wing 4x3@4 wing gable eave -1.5 pitch 30",
    "T 12x8 wing 4x3@4 wing hip eave -1.5 pitch 30",
    "T 15x5 wing 3x4@5 wing gable eave -1.5 pitch 30",
    "T 15x5 wing 3x4@5 wing hip eave -1.5 pitch 30",
    "bay 12x8 bay 3x1 wing gable eave -1.5 pitch 30",
    "bay 12x8 bay 3x1 wing hip eave -0.5 pitch 30",
    "bay 12x8 bay 3x1 wing hip eave -1.5 pitch 30",
    "bay 12x8 bay 3x1 wing hip eave -1.5 pitch 40",
    "bay 12x8 bay 3x1 wing hip eave -1.5 pitch 50",
    "bay 12x8 bay 3x1 wing hip eave -1 pitch 30",
    "bay 12x8 bay 3x1 wing hip eave -1 pitch 40",
];

// ------------------------------------------------- Jakub's T (2026-11-14)

/// Jakub's T: 15 × 5 house, 5 × 2.5 wing, all 40°, eave 5, overhang 0.5,
/// house ends and wing sides gables, wing front an eave at `front_eave`.
fn jakub_t(front_eave: f64) -> Project {
    let p = t_plan(15.0, 5.0, 5.0, 5.0, 2.5);
    let mut pr = roof(&p, Style::Gable(40.0), 0.5, 0.5);
    for (i, e) in pr.parts[0].edges.iter_mut().enumerate() {
        e.eave_height = 5.0;
        e.pitch_deg = match i {
            1 | 3 | 5 | 7 => 90.0,
            _ => 40.0,
        };
    }
    pr.parts[0].edges[0].eave_height = front_eave;
    pr.parts[0].height = Some(5.0);
    pr
}

/// With the wing's sides gables, the wing is one plane parallel to the
/// house's front plane; lowered to 2.90 m it lies in that plane. DACAD
/// stops with "Einzelne Fläche vorhanden" (emulator, 2026-11-14); Tectora
/// joins the two planes into one face (Jakub's decision, `coplanar.rs`):
/// the house's front face runs on over the wing, no valley, no step, the
/// wing's sides verges from its eave to the house's eave line.
#[test]
fn wing_plane_in_the_house_plane_is_one_face() {
    for front in [2.82, 2.9, 2.95, 3.0] {
        let roof = calc(&jakub_t(front));
        let g = &roof.geos[0];
        assert!(g.error.is_none(), "{front}: {:?}", g.error);
        let kinds: Vec<_> = g.warnings.iter().map(|w| w.kind).collect();
        assert_eq!(kinds, vec![WarningKind::MergedPlanes], "{front}");
        assert!(
            (roof.area - 144.9002).abs() < 2e-3,
            "{front}: {}",
            roof.area
        );
        // Three faces: the front plane with the wing, the back plane and
        // nothing else (gable ends are not faces).
        assert_eq!(g.faces.len(), 2, "{front}: {:?}", g.faces);
        assert!(roof.line("K").abs() < 1e-3, "{front}: {:?}", roof.lines);
        // Verges: house ends 4 × 3.916, wing sides 2 × 2.5 / cos 40°.
        let verges: f64 = roof
            .lines
            .iter()
            .filter(|(c, _)| c.starts_with('O'))
            .map(|(_, l)| l)
            .sum();
        let want = 4.0 * slant(3.0, 3.0 * tan(40.0)) + 2.0 * 2.5 / cos(40.0);
        assert!(
            (verges - want).abs() < 5e-3,
            "{front}: verges {verges} want {want}"
        );
        assert!(
            (roof.line("T") - 32.0).abs() < 1e-3,
            "{front}: {:?}",
            roof.lines
        );
        let mut roof = roof;
        take_notices(&mut roof);
        let mut c_faces = Report::default();
        let mut c = c_faces.case();
        check_structure_fit(&mut c, &roof, eave_plan(&jakub_t(front)), Fit::DacadSteps);
        c.done(format!("{front}"));
        c_faces.finish("merged T structure");
    }
}

/// Lower still, the wing's plane lies under the house's and DACAD has no
/// roof ("Kein Durchstoßpunkt"). Tectora calculates the wing as a lean-to
/// against the house (`wing_split.rs`): the house roof whole, the wing's
/// top edge a wall abutment at the house wall.
#[test]
fn wing_plane_below_the_house_plane_is_a_lean_to_against_the_house() {
    for front in [1.5, 2.0, 2.5, 2.8] {
        let roof = calc(&jakub_t(front));
        let g = &roof.geos[0];
        assert!(g.error.is_none(), "{front}: {:?}", g.error);
        let w: Vec<_> = g.warnings.iter().map(|w| w.kind).collect();
        assert_eq!(w, vec![WarningKind::WingJoined], "{front}");
        // House 16 × 6 m plan, wing 6 × 3 m plan up to the house wall, all
        // at 40°.
        let want = (16.0 * 6.0 + 6.0 * 3.0) / cos(40.0);
        assert!(
            (roof.area - want).abs() < 2e-3,
            "{front}: {} want {want}",
            roof.area
        );
        assert!(
            (roof.line("WV") - 6.0).abs() < 1e-3,
            "{front}: {:?}",
            roof.lines
        );
        assert!(roof.line("P").abs() < 1e-3, "{front}: {:?}", roof.lines);
        for f in &g.faces {
            assert!(f.edge < 8, "every face names a wall of the building: {f:?}");
        }
    }
}

/// Above the merge band the wing's plane runs on over the house's front
/// plane up to the ridge, as in DACAD (SO2 byte-identical): calculated,
/// with a warning that the wing has no ridge of its own.
#[test]
fn wing_plane_above_the_house_plane_is_warned() {
    for (front, rise) in [(3.05, 0.1477), (5.0, 2.0977)] {
        let roof = calc(&jakub_t(front));
        assert!(roof.geos[0].error.is_none());
        assert!((roof.area - 144.9002).abs() < 1e-3, "{}", roof.area);
        let w: Vec<_> = roof.geos[0]
            .warnings
            .iter()
            .filter(|w| w.kind == WarningKind::PlaneOverWall)
            .collect();
        assert_eq!(w.len(), 1, "{:?}", roof.geos[0].warnings);
        assert_eq!(w[0].edges[0], 0);
        assert!((w[0].value - rise).abs() < 1e-3, "{}", w[0].value);
    }
}

/// The same T entered the way a cross gable is built (wing sides eaves,
/// front a gable or a hip, eaves lowered) gets its own lower ridge and no
/// warning.
#[test]
fn lowered_wing_with_its_own_ridge_is_not_warned() {
    for front_gable in [true, false] {
        for eave in [3.0, 4.0, 5.0] {
            let mut pr = jakub_t(eave);
            let e = &mut pr.parts[0].edges;
            for k in [0, 1, 7] {
                e[k].pitch_deg = 40.0;
                e[k].eave_height = eave;
            }
            if front_gable {
                e[0].pitch_deg = 90.0;
            }
            let roof = calc(&pr);
            assert!(roof.geos[0].error.is_none());
            assert!(
                roof.geos[0].warnings.is_empty(),
                "{:?}",
                roof.geos[0].warnings
            );
            assert!(roof.line("K") > 1.0, "valleys {:?}", roof.lines);
        }
    }
}

// ---------------------------------------------------------- joined, dormers

fn rect_part(name: &str, x: f64, y: f64, w: f64, d: f64, pitch: f64, eave: f64) -> RoofPart {
    let pts = [(x, y), (x + w, y), (x + w, y + d), (x, y + d)];
    RoofPart::new(
        name,
        pts.iter()
            .map(|&(px, py)| edge(px, py, pitch, eave, 0.5))
            .collect::<Vec<EdgeDef>>(),
    )
}

/// Two buildings side by side or a lower annex against a house: every
/// building keeps a roof, faces stay planar and inside the outlines.
#[test]
fn joined_buildings_are_sound() {
    let mut rep = Report::default();
    let cases: Vec<(&str, Vec<RoofPart>)> = vec![
        (
            "garage beside house, lower",
            vec![
                rect_part("house", 0.0, 0.0, 12.0, 8.0, 40.0, 5.0),
                rect_part("garage", 12.0, 1.0, 6.0, 5.0, 30.0, 3.0),
            ],
        ),
        (
            "annex in front, same eave",
            vec![
                rect_part("house", 0.0, 0.0, 12.0, 8.0, 40.0, 4.0),
                rect_part("annex", 4.0, -4.0, 4.0, 4.0, 40.0, 4.0),
            ],
        ),
        (
            "two houses in a row, different pitch",
            vec![
                rect_part("a", 0.0, 0.0, 10.0, 8.0, 45.0, 4.0),
                rect_part("b", 10.0, 0.0, 10.0, 8.0, 35.0, 4.0),
            ],
        ),
    ];
    for (name, parts) in cases {
        let mut pr = Project::new(name);
        pr.parts = parts;
        dump(name, &pr);
        let roof = calc(&pr);
        let mut c = rep.case();
        no_error(&mut c, &roof);
        for (i, g) in roof.geos.iter().enumerate() {
            c.ok(&format!("building {i} has no roof"), !g.faces.is_empty());
            c.ok(
                &format!("building {i} warnings {:?}", g.warnings),
                g.warnings.is_empty(),
            );
            for f in &g.faces {
                c.ok(
                    "finite",
                    f.area.is_finite() && f.polygon.iter().flatten().all(|v| v.is_finite()),
                );
            }
        }
        c.done(name.to_string());
    }
    rep.finish("joined_buildings_are_sound");
}

/// Every dormer kind on the front plane of a hipped and a gabled house:
/// calculated, no warning, with faces of its own, and the roof area
/// within a few m² of the house without it.
#[test]
fn every_dormer_kind_on_a_house_is_sound() {
    let mut rep = Report::default();
    let p = &plans()[0];
    for style in [Style::Hip(40.0), Style::Gable(45.0)] {
        let base = roof(p, style, 0.5, 0.5);
        let bare = calc(&base).area;
        for kind in [
            DormerKind::Gable,
            DormerKind::Pointed,
            DormerKind::Hip,
            DormerKind::HalfHip,
            DormerKind::Shed,
            DormerKind::Trapezoid,
            DormerKind::Round,
        ] {
            let mut pr = base.clone();
            // Front 1.5 m behind the front wall, its eave 1 m above the
            // roof there, as the editor places it.
            let pitch = base.parts[0].edges[0].pitch_deg;
            let host = EAVE + (1.5 + 0.5) * tan(pitch);
            let mut d = default_params(kind, Point2::new(4.0, 1.5), 0.0, host + 1.0);
            d.width = 2.0;
            pr.dormers.push(build_dormer(&d));
            dump(&format!("dormer {style:?} {kind:?}"), &pr);
            let roof = calc(&pr);
            let mut c = rep.case();
            no_error(&mut c, &roof);
            let g = &roof.geos[0];
            c.ok(&format!("warnings {:?}", g.warnings), g.warnings.is_empty());
            // A dormer roof flatter than the house (shed, trapezoid, round)
            // has less area than the house roof it replaces, a steeper one
            // more, one at the house's pitch the same (every face is its
            // plan area / cos(pitch)): a few m² either way.
            c.ok(
                &format!("area {:.3} vs bare {bare:.3}", roof.area),
                roof.area.is_finite() && (roof.area - bare).abs() < 10.0,
            );
            c.ok(
                "dormer has faces",
                g.faces.iter().any(|f| f.edge == usize::MAX && f.area > 0.1),
            );
            c.done(format!("{style:?} {kind:?}"));
        }
    }
    rep.finish("every_dormer_kind_on_a_house_is_sound");
}

// ------------------------------------- where DACAD's core stops (2026-11-14)

fn kinds(roof: &Roof, part: usize) -> Vec<WarningKind> {
    roof.geos[part].warnings.iter().map(|w| w.kind).collect()
}

/// A project turned by `deg` around the origin, corners rounded to
/// millimetres as the editor's Rotate rounds them.
fn turned(p: &Project, deg: f64) -> Project {
    let (s, c) = deg.to_radians().sin_cos();
    let mm = |v: f64| (v * 1000.0).round() / 1000.0;
    let mut out = p.clone();
    for e in out.parts.iter_mut().flat_map(|part| part.edges.iter_mut()) {
        let (x, y) = (e.start.x, e.start.y);
        e.start = Point2::new(mm(x * c - y * s), mm(x * s + y * c));
    }
    out
}

/// The structure of a roof Tectora calculated where the core stops.
fn assert_sound(name: &str, p: &Project, roof: &Roof) {
    let mut roof = Roof {
        geos: roof.geos.clone(),
        lines: roof.lines.clone(),
        area: roof.area,
    };
    let notices = take_notices(&mut roof);
    let mut rep = Report::default();
    let mut c = rep.case();
    no_error(&mut c, &roof);
    check_structure_fit(&mut c, &roof, eave_plan(p), fit_of(p, &notices));
    c.done(name.to_string());
    rep.finish(name);
}

/// A hipped house with a small jog in its front (insulation, a facade
/// offset): the two front planes are parallel and a few centimetres apart,
/// but hips and valleys close the roof, and the standard result is sound.
/// It stays untouched (review MATH-001, TEST-001: the plane merge replaced
/// it by a roof with a negative face reaching 1 m outside the outline).
#[test]
fn a_small_jog_in_a_hipped_house_keeps_the_standard_roof() {
    for (jog, pitch) in [
        (0.05, 40.0),
        (0.08, 40.0),
        (0.05, 25.0),
        (0.3, 15.0),
        (0.5, 10.0),
        (1.0, 5.0),
    ] {
        for style in [Style::Hip(pitch), Style::Gable(pitch)] {
            let p = plan(
                "jog",
                &[
                    (0.0, 0.0),
                    (6.0, 0.0),
                    (6.0, jog),
                    (10.0, jog),
                    (10.0, 8.0),
                    (0.0, 8.0),
                ],
                &[3, 5],
                None,
            );
            let pr = roof(&p, style, 0.5, 0.5);
            let name = format!("jog {jog} {style:?}");
            let core = tectora_core::engine::generate(&pr)
                .unwrap_or_else(|e| panic!("{name}: the standard calculation gives a roof: {e}"));
            let got = calc(&pr);
            assert_eq!(kinds(&got, 0), vec![], "{name}");
            assert_eq!(got.geos[0].faces, core[0].faces, "{name}");
            assert_eq!(got.geos[0].lines, core[0].lines, "{name}");
        }
    }
}

/// A hipped L whose lower arm's plane lies 5.6 cm over the other arm's
/// (review ARCH-002) and a hipped wing whose hip face lies in the house's
/// front plane (MATH-002: the merge deleted 7.83 m of wall abutment):
/// the standard results are sound and stay untouched, without a notice.
#[test]
fn hipped_l_and_hipped_wing_keep_the_standard_roof() {
    let l = project(vec![
        edge(0.0, 0.0, 40.0, 1.7, 0.5),
        edge(6.0, 0.0, 40.0, 1.7, 0.5),
        edge(6.0, 4.0, 40.0, 5.0, 0.5),
        edge(12.0, 4.0, 40.0, 5.0, 0.5),
        edge(12.0, 10.0, 40.0, 5.0, 0.5),
        edge(0.0, 10.0, 40.0, 5.0, 0.5),
    ]);
    let hipped_wing = |d: f64| {
        let low = 6.0 - tan(40.0) * 2.5 + d;
        project(vec![
            edge(5.0, 0.0, 40.0, low, 0.5),
            edge(10.0, 0.0, 40.0, low, 0.5),
            edge(10.0, 2.5, 40.0, 6.0, 0.5),
            edge(15.0, 2.5, 90.0, 6.0, 0.5),
            edge(15.0, 7.5, 40.0, 6.0, 0.5),
            edge(0.0, 7.5, 90.0, 6.0, 0.5),
            edge(0.0, 2.5, 40.0, 6.0, 0.5),
            edge(5.0, 2.5, 40.0, low, 0.5),
        ])
    };
    for (name, pr) in [
        ("hipped L", l),
        ("hipped wing in the house plane", hipped_wing(0.0)),
        ("hipped wing 5 cm over", hipped_wing(0.05)),
    ] {
        let core = tectora_core::engine::generate(&pr)
            .unwrap_or_else(|e| panic!("{name}: the standard calculation gives a roof: {e}"));
        let got = calc(&pr);
        assert_eq!(kinds(&got, 0), vec![], "{name}");
        assert_eq!(got.geos[0].faces, core[0].faces, "{name}");
        assert_eq!(got.geos[0].lines, core[0].lines, "{name}");
    }
    // The wing's abutment against the house wall is in the result.
    let roof = calc(&hipped_wing(0.0));
    assert!(
        (roof.line("WS") - 7.83).abs() < 0.01,
        "abutment {:?}",
        roof.lines
    );
}

fn window() -> RoofWindow {
    RoofWindow {
        x: 12.0,
        y: 4.0,
        width: 0.78,
        height: 1.18,
    }
}

/// A roof window, a chimney or a second building far away changes nothing
/// about the roof it does not touch: Jakub's T keeps its joined plane, in
/// the band where DACAD stops and in the band where it leaves a step
/// (review ERR-002, ARCH-004, TEST-004, MATH-006: a window brought back
/// 7.8 m of valley and 7.8 m of verge, or turned the wing into a lean-to).
#[test]
fn a_window_or_chimney_on_the_joined_plane_keeps_it() {
    for front in [2.8523, 2.9, 2.95, 3.0] {
        let bare = calc(&jakub_t(front));
        assert_eq!(kinds(&bare, 0), vec![WarningKind::MergedPlanes], "{front}");

        let mut pr = jakub_t(front);
        pr.windows.push(window());
        let roof = calc(&pr);
        assert_eq!(kinds(&roof, 0), vec![WarningKind::MergedPlanes], "{front}");
        assert!(
            (roof.area - (bare.area - 0.78 * 1.18)).abs() < 1e-6,
            "{front}: {} against {}",
            roof.area,
            bare.area
        );
        assert_eq!(roof.lines, bare.lines, "{front}: a window adds no line");
        assert_eq!(roof.geos[0].faces.len(), 2, "{front}");

        let mut pr = jakub_t(front);
        pr.chimneys.push(Chimney {
            corners: vec![
                Point2::new(12.0, 6.0),
                Point2::new(12.6, 6.0),
                Point2::new(12.6, 6.6),
                Point2::new(12.0, 6.6),
            ],
            height: 9.0,
        });
        let roof = calc(&pr);
        assert_eq!(kinds(&roof, 0), vec![WarningKind::MergedPlanes], "{front}");
        // 0.36 m² in plan off a 40° face.
        assert!(
            (roof.area - (bare.area - 0.36 / cos(40.0))).abs() < 2e-3,
            "{front}: {} against {}",
            roof.area,
            bare.area
        );
        for code in ["T", "F", "OL", "OR"] {
            assert!(
                (roof.line(code) - bare.line(code)).abs() < 1e-3,
                "{front} {code}: {:?}",
                roof.lines
            );
        }
        assert!(roof.line("K").abs() < 1e-3, "{front}: {:?}", roof.lines);
    }
}

/// The lean-to keeps its roof under a window as well: only the window's
/// area goes.
#[test]
fn a_window_on_the_house_of_a_lean_to_wing_keeps_the_lean_to() {
    let bare = calc(&jakub_t(2.0));
    let mut pr = jakub_t(2.0);
    pr.windows.push(window());
    let roof = calc(&pr);
    assert_eq!(kinds(&roof, 0), vec![WarningKind::WingJoined]);
    assert!(
        (roof.area - (bare.area - 0.78 * 1.18)).abs() < 1e-6,
        "{} against {}",
        roof.area,
        bare.area
    );
    assert_eq!(roof.lines, bare.lines);
}

/// A garage 40 m away takes no roof and changes none (review ARCH-003,
/// TEST-002, ERR-004: the house lost its roof, or got the stepped roof
/// back, as soon as a second building was drawn).
#[test]
fn a_far_garage_leaves_the_t_house_its_roof() {
    for (front, notice) in [
        (2.0, WarningKind::WingJoined),
        (2.9, WarningKind::MergedPlanes),
        (3.0, WarningKind::MergedPlanes),
    ] {
        let alone = calc(&jakub_t(front));
        for garage_first in [false, true] {
            let mut pr = jakub_t(front);
            let garage = rect_part("garage", 40.0, 0.0, 6.0, 5.0, 30.0, 3.0);
            let house = if garage_first {
                pr.parts.insert(0, garage);
                1
            } else {
                pr.parts.push(garage);
                0
            };
            let roof = calc(&pr);
            let name = format!("front {front}, garage first {garage_first}");
            for g in &roof.geos {
                assert!(g.error.is_none(), "{name}: {:?}", g.error);
                assert!(!g.faces.is_empty(), "{name}");
            }
            assert_eq!(kinds(&roof, house), vec![notice], "{name}");
            assert_eq!(kinds(&roof, 1 - house), vec![], "{name}");
            let area: f64 = roof.geos[house].faces.iter().map(|f| f.area).sum();
            assert!((area - alone.area).abs() < 1e-6, "{name}: {area}");
            assert_eq!(roof.geos[house].lines, alone.geos[0].lines, "{name}");
        }
    }
}

/// The same house turned on the site plan gives the same roof: Jakub's T
/// with the joined plane and with the lean-to, turned by angles that are
/// no multiple of 90°, corners rounded to millimetres (review MATH-003,
/// MATH-004: a phantom valley and verge at 17.3°, the old error at 1°,
/// 33° and 45°).
#[test]
fn the_t_house_turned_gives_the_same_roof() {
    for (front, notice, area_tol) in [
        (2.9, WarningKind::MergedPlanes, 0.02),
        // The join of two buildings snaps corners to centimetres.
        (2.0, WarningKind::WingJoined, 0.1),
    ] {
        let straight = calc(&jakub_t(front));
        for deg in [1.0, 5.0, 17.3, 33.0, 45.0, 61.7, 90.0, 200.0] {
            let pr = turned(&jakub_t(front), deg);
            let roof = calc(&pr);
            let name = format!("front {front} turned {deg}");
            assert!(
                roof.geos[0].error.is_none(),
                "{name}: {:?}",
                roof.geos[0].error
            );
            assert_eq!(kinds(&roof, 0), vec![notice], "{name}");
            assert!(
                (roof.area - straight.area).abs() < area_tol,
                "{name}: {} against {}",
                roof.area,
                straight.area
            );
            // Eave, ridge, abutment and all verges together (left, right
            // and oblique are told apart by the walls' directions).
            let verges = |r: &Roof| -> f64 {
                r.lines
                    .iter()
                    .filter(|(c, _)| c.starts_with('O'))
                    .map(|(_, l)| l)
                    .sum()
            };
            for (what, got, want) in [
                ("T", roof.line("T"), straight.line("T")),
                ("F", roof.line("F"), straight.line("F")),
                ("WV", roof.line("WV"), straight.line("WV")),
                ("K", roof.line("K"), 0.0),
                ("P", roof.line("P"), 0.0),
                ("verges", verges(&roof), verges(&straight)),
            ] {
                assert!(
                    (got - want).abs() < 0.05,
                    "{name} {what}: {got} want {want}"
                );
            }
            assert_eq!(
                roof.geos[0].faces.len(),
                straight.geos[0].faces.len(),
                "{name}"
            );
        }
    }
}

/// An error that belongs to an object stays that error: a chimney with an
/// inward corner on a house with a low wing is not answered by cutting
/// the wing off (review ARCH-001: the job calculated with other valleys
/// and a notice naming a cause that did not exist).
#[test]
fn a_non_convex_chimney_stays_an_error_on_a_house_with_a_low_wing() {
    let bad = Chimney {
        corners: vec![
            Point2::new(2.0, 4.0),
            Point2::new(3.0, 4.0),
            Point2::new(2.5, 4.3),
            Point2::new(3.0, 5.0),
            Point2::new(2.0, 5.0),
        ],
        height: 9.0,
    };
    // Wing with its own ridge, eaves 1 m under the house's.
    let mut hipped = jakub_t(5.0);
    for k in [0, 1, 7] {
        hipped.parts[0].edges[k].pitch_deg = 40.0;
        hipped.parts[0].edges[k].eave_height = 4.0;
    }
    assert_eq!(
        kinds(&calc(&hipped), 0),
        vec![],
        "sound without the chimney"
    );
    for mut pr in [hipped, jakub_t(2.0), jakub_t(2.9)] {
        pr.chimneys.push(bad.clone());
        let roof = calc(&pr);
        let e = roof.geos[0].error.as_ref().expect("no roof");
        assert_eq!(e.kind, tectora_core::roof::CalcErrorKind::NonConvexOpening);
        assert!(roof.geos[0].faces.is_empty());
    }
}

/// A shallow wing whose plane lies 3 cm under the house's is neither
/// joined (the core changes topology on the way) nor a lean-to (review
/// MATH-005: cut off, it was calculated 0.3 m into the house, with a free
/// top edge and an abutment that do not exist). Without a sound roof the
/// error stands.
#[test]
fn a_wing_a_few_centimetres_under_the_house_plane_is_never_a_lean_to() {
    // House 15 × 5 at 25°, eave 12, wing 5 × 0.6.
    // Eave lines 0.3 m apart: the wing's plane 3 cm under the house's.
    let front = 12.0 - tan(25.0) * 0.3 - 0.03;
    let pr = project(vec![
        edge(5.0, 0.0, 25.0, front, 0.2),
        edge(10.0, 0.0, 90.0, 12.0, 0.0),
        edge(10.0, 0.6, 25.0, 12.0, 0.5),
        edge(15.0, 0.6, 90.0, 12.0, 0.0),
        edge(15.0, 5.6, 25.0, 12.0, 0.5),
        edge(0.0, 5.6, 90.0, 12.0, 0.0),
        edge(0.0, 0.6, 25.0, 12.0, 0.5),
        edge(5.0, 0.6, 90.0, 12.0, 0.0),
    ]);
    let roof = calc(&pr);
    assert!(!kinds(&roof, 0).contains(&WarningKind::WingJoined));
    if roof.geos[0].error.is_none() {
        assert_sound("shallow wing 3 cm under", &pr, &roof);
        assert!(roof.line("P").abs() < 1e-3, "{:?}", roof.lines);
        assert!(roof.line("WH").abs() < 1e-3, "{:?}", roof.lines);
    }
}

/// A back bump with its eave at 2.0 m under a house at 5.0 m. Tectora's
/// sweep ignores the bump's eave height (two faces, 144.9 m², no eave at
/// 2.0 m) and must never be the result; the roof is the lean-to.
#[test]
fn a_low_bump_is_a_lean_to_never_the_roof_that_ignores_its_eave() {
    let mut pr = project(vec![
        edge(1.15, 0.0, 90.0, 5.0, 0.5),
        edge(1.15, 5.0, 40.0, 5.0, 0.5),
        edge(6.15, 5.0, 90.0, 5.0, 0.5),
        edge(6.15, 7.5, 40.0, 2.0, 0.5),
        edge(11.15, 7.5, 90.0, 5.0, 0.5),
        edge(11.15, 5.0, 40.0, 5.0, 0.5),
        edge(16.15, 5.0, 90.0, 5.0, 0.5),
        edge(16.15, 0.0, 40.0, 5.0, 0.5),
    ]);
    // Drawn clockwise, as the job that showed it.
    pr.parts[0].normalize_ccw();
    // What the sweep alone gives is refused by the self-check.
    let part = pr.parts[0].clone();
    let sweep = tectora_core::roof::generate(&part);
    assert!(!sweep.faces.is_empty());
    let refused = tectora_core::check::sound(&part, &sweep, tectora_core::check::Fit::STANDARD);
    assert!(refused.is_err(), "the sweep ignores the bump's eave height");

    let roof = calc(&pr);
    assert!(roof.geos[0].error.is_none(), "{:?}", roof.geos[0].error);
    assert_eq!(kinds(&roof, 0), vec![WarningKind::WingJoined]);
    let want = (16.0 * 6.0 + 6.0 * 3.0) / cos(40.0);
    assert!((roof.area - want).abs() < 2e-3, "{}", roof.area);
    // The bump's eave is at 2.0 m.
    let low = roof
        .geos
        .iter()
        .flat_map(|g| &g.lines)
        .filter(|l| l.dacad_code() == "T")
        .any(|l| (l.a[2] - 2.0).abs() < 2e-3 && (l.b[2] - 2.0).abs() < 2e-3);
    assert!(low, "an eave line at the bump's eave height");
    assert_sound("low bump", &pr, &roof);
}

/// ERR-001: a hip roof 12 × 8 with one corner moved by 0.2 m. DACAD's core
/// stops on 12 of the 20 ("Kein Schnittpunkt berechenbar (P2)", emulator
/// logs of 2026-11-14); every one calculates, soundly, and the 12 say that
/// the roof is not the standard calculation's.
#[test]
fn a_hip_roof_with_one_corner_moved_calculates() {
    let corners = [(0.0, 0.0), (12.0, 0.0), (12.0, 8.0), (0.0, 8.0)];
    let mut departed = 0;
    for c in 0..4 {
        for (dx, dy) in [(0.2, 0.0), (-0.2, 0.0), (0.0, 0.2), (0.0, -0.2), (0.2, 0.2)] {
            let mut pts = corners;
            pts[c] = (pts[c].0 + dx, pts[c].1 + dy);
            let pr = uniform(&pts, 35.0, 3.0, 0.5);
            let name = format!("hip corner {c} moved {dx} {dy}");
            let roof = calc(&pr);
            assert!(
                roof.geos[0].error.is_none(),
                "{name}: {:?}",
                roof.geos[0].error
            );
            let want = if standard_calculates(&pr) {
                vec![]
            } else {
                departed += 1;
                vec![WarningKind::NotCalculatedByCore]
            };
            assert_eq!(kinds(&roof, 0), want, "{name}");
            assert_sound(&name, &pr, &roof);
        }
    }
    assert_eq!(departed, 12, "roofs the standard calculation stops on");
}

/// A hip roof on a skewed rectangle: the line where the two long faces
/// meet slopes a little, and it is still the ridge (two faces of opposite
/// walls), not a hip; a hip runs down to a corner between two neighbouring
/// walls. The offer counts ridge and hip tiles from these lines.
#[test]
fn the_sloping_ridge_of_a_skewed_hip_roof_is_a_ridge() {
    for (c, dy) in [(0, 0.2), (1, -0.2), (2, 0.2), (3, -0.2)] {
        let mut pts = [(0.0, 0.0), (12.0, 0.0), (12.0, 8.0), (0.0, 8.0)];
        pts[c].1 += dy;
        let pr = uniform(&pts, 35.0, 3.0, 0.5);
        let name = format!("hip corner {c} moved {dy}");
        let roof = calc(&pr);
        let hips = roof
            .geos
            .iter()
            .flat_map(|g| &g.lines)
            .filter(|l| l.dacad_code() == "G")
            .count();
        assert_eq!(
            hips, 4,
            "{name}: one hip per corner, lines {:?}",
            roof.lines
        );
        assert!(roof.line("F") > 3.0, "{name}: ridge {:?}", roof.lines);
    }
}

/// From a real drawing (a wing flush with the house's side wall, its
/// front plane the house's back plane): DACAD's core stops ("Kein
/// Schnittpunkt berechenbar"), and the sweep gives the house's back face
/// and the wing's face as two faces in one plane side by side. They are
/// one face, and the roof calculates with the notice.
#[test]
fn a_wing_flush_with_the_house_side_in_the_house_plane_calculates() {
    let low = 4.0 - 2.0 * tan(38.0);
    let pr = project(vec![
        edge(0.0, 0.0, 38.0, 4.0, 0.0),
        edge(10.0, 0.0, 38.0, 4.0, 0.0),
        edge(10.0, 6.0, 90.0, low, 0.0),
        edge(10.0, 8.0, 38.0, low, 0.0),
        edge(6.0, 8.0, 90.0, low, 0.0),
        edge(6.0, 6.0, 38.0, 4.0, 0.0),
        edge(0.0, 6.0, 38.0, 4.0, 0.0),
    ]);
    assert!(!standard_calculates(&pr), "the core stops");
    let roof = calc(&pr);
    assert!(roof.geos[0].error.is_none(), "{:?}", roof.geos[0].error);
    assert_eq!(kinds(&roof, 0), vec![WarningKind::NotCalculatedByCore]);
    assert_eq!(roof.geos[0].faces.len(), 4, "the back plane is one face");
    // Equal pitch everywhere: area = plan / cos(pitch).
    let plan = 10.0 * 6.0 + 4.0 * 2.0;
    assert!(
        (roof.area - plan / cos(38.0)).abs() < 1e-3,
        "area {}",
        roof.area
    );
    // (Walls 1 and 2 are in line, which `eave_plan` cannot offset; no
    // overhang, so the plan is the outline.)
    let mut sound = Roof {
        geos: roof.geos.clone(),
        lines: roof.lines.clone(),
        area: roof.area,
    };
    let notices = take_notices(&mut sound);
    let mut rep = Report::default();
    let mut c = rep.case();
    check_structure_fit(&mut c, &sound, plan, fit_of(&pr, &notices));
    c.done("wing flush with the house side".into());
    rep.finish("wing flush with the house side");
}

/// From a real drawing drawn as one outline: a house with hips, a lower
/// wing whose roof breaks into the house's back plane, and a narrow
/// wing between gables. DACAD's core stops; the roof calculates with the
/// notice, the faces of one plane are one, a steeply falling line between
/// two opposite walls stays a hip, and no millimetre crumbs of lines
/// count as valleys or verges.
#[test]
fn a_house_with_a_wing_breaking_into_its_back_plane_calculates() {
    let mut pr = project(vec![
        edge(0.0, 0.0, 38.0, 4.19, 0.0),
        edge(23.0, 0.0, 38.0, 5.19, 0.0),
        edge(23.0, 9.7, 90.0, 2.6, 0.0),
        edge(23.0, 15.66, 22.0, 2.6, 0.0),
        edge(13.93, 15.66, 90.0, 2.6, 0.0),
        edge(13.93, 10.58, 38.0, 4.65, 0.0),
        edge(9.12, 10.58, 38.0, 4.19, 0.0),
        edge(9.12, 16.84, 90.0, 4.19, 0.0),
        edge(0.0, 16.84, 38.0, 4.19, 0.0),
    ]);
    pr.parts[0].edges[3].breaks = vec![PitchBreak {
        height: 4.65,
        pitch_deg: 38.0,
    }];
    assert!(!standard_calculates(&pr), "the core stops");
    let roof = calc(&pr);
    assert!(roof.geos[0].error.is_none(), "{:?}", roof.geos[0].error);
    assert_eq!(kinds(&roof, 0), vec![WarningKind::NotCalculatedByCore]);
    for l in &roof.geos[0].lines {
        assert!(l.length() >= 0.01, "crumb {l:?}");
        if l.dacad_code() == "F" {
            let run = (l.a[0] - l.b[0]).hypot(l.a[1] - l.b[1]);
            assert!((l.a[2] - l.b[2]).abs() <= 0.05 * run, "falling ridge {l:?}");
        }
    }
}

/// ERR-001: the built-in U example with the pitch of wall 1 or wall 5
/// changed to 20..34° ("Nicht geschlossene Fläche(n) vorhanden" in DACAD's
/// core): every one calculates, soundly, with the notice.
#[test]
fn the_u_house_with_one_flatter_wall_calculates() {
    let pts = [
        (0.0, 0.0),
        (18.0, 0.0),
        (18.0, 12.0),
        (13.0, 12.0),
        (13.0, 5.0),
        (5.0, 5.0),
        (5.0, 12.0),
        (0.0, 12.0),
    ];
    for wall in [0, 4] {
        for pitch in 20..=34 {
            let mut pr = uniform(&pts, 35.0, 3.0, 0.5);
            pr.parts[0].edges[wall].pitch_deg = f64::from(pitch);
            let name = format!("U wall {wall} at {pitch}");
            assert!(!standard_calculates(&pr), "{name}: the core stops");
            let roof = calc(&pr);
            assert!(
                roof.geos[0].error.is_none(),
                "{name}: {:?}",
                roof.geos[0].error
            );
            assert_eq!(
                kinds(&roof, 0),
                vec![WarningKind::NotCalculatedByCore],
                "{name}"
            );
            assert_sound(&name, &pr, &roof);
        }
    }
}

/// The checks of `Fit::DacadSteps` and `Fit::DacadJoined` refuse a face
/// emitted twice (review TEST-003: only a lower bound on the plan
/// coverage let a roof with 43 % too much area pass).
#[test]
fn a_doubled_face_fails_the_structure_check() {
    let mut pr = roof(
        &t_plan(15.0, 5.0, 5.0, 5.0, 2.5),
        Style::Gable(40.0),
        0.5,
        0.5,
    );
    for k in [0, 1, 7] {
        pr.parts[0].edges[k].eave_height = EAVE - 1.0;
    }
    pr.parts[0].edges[0].pitch_deg = 90.0;
    let mut roof = calc(&pr);
    assert_eq!(kinds(&roof, 0), vec![]);
    let largest = roof.geos[0]
        .faces
        .iter()
        .max_by(|a, b| a.area.total_cmp(&b.area))
        .cloned()
        .expect("faces");
    roof.area += largest.area;
    roof.geos[0].faces.push(largest);
    for fit in [Fit::DacadSteps, Fit::DacadJoined] {
        let mut rep = Report::default();
        let mut c = rep.case();
        check_structure_fit(&mut c, &roof, eave_plan(&pr), fit);
        assert!(!c.clean(), "a doubled face passes");
        c.done(String::new());
    }
}

/// A notice on a roof with level eaves is a finding for the closed-form
/// and level fits (review TEST-008: the shared helper ignored notices for
/// every caller).
#[test]
fn a_notice_fails_the_level_fits() {
    let p = &plans()[0];
    let pr = roof(p, Style::Hip(40.0), 0.5, 0.5);
    let mut roof = calc(&pr);
    roof.geos[0]
        .warnings
        .push(tectora_core::check::RoofWarning::new(
            WarningKind::MergedPlanes,
            0.01,
        ));
    for (fit, passes) in [
        (Fit::Exact, false),
        (Fit::Dacad, false),
        (Fit::DacadSteps, true),
    ] {
        let mut rep = Report::default();
        let mut c = rep.case();
        check_structure_fit(&mut c, &roof, eave_plan(&pr), fit);
        assert_eq!(c.clean(), passes);
        c.done(String::new());
    }
}

/// The wall area of a roof Tectora calculates where the core stops is
/// measured under that roof the way the wall stage measures it (review
/// ERR-003: the list lost its wall row for every such roof). Proof of the
/// measure: on every catalogue wing roof the core calculates it equals
/// the wall stage's area (5 mm² per m²: the core's own float32 rounding
/// is up to 4e-3 m² on these roofs).
#[test]
fn the_measured_wall_area_is_the_wall_stages() {
    let mut compared = 0;
    for p in plans() {
        let Some([s1, s2, front]) = p.wing else {
            continue;
        };
        for front_gable in [true, false] {
            for d_eave in [-1.5, -0.5, 0.0, 0.4] {
                for wing_pitch in [40.0, 30.0, 50.0] {
                    let mut pr = roof(&p, Style::Gable(40.0), 0.5, 0.5);
                    let edges = &mut pr.parts[0].edges;
                    for &k in &[s1, s2, front] {
                        edges[k].eave_height = EAVE + d_eave;
                        edges[k].pitch_deg = wing_pitch;
                    }
                    if front_gable {
                        edges[front].pitch_deg = 90.0;
                    }
                    let Ok(core) = tectora_core::engine::wall_area(&pr) else {
                        continue;
                    };
                    let measured = tectora_core::openings::measured_wall_area(&pr)
                        .expect("a wall area under a calculated roof");
                    assert!(
                        (measured - core).abs() < 5e-3,
                        "{} {front_gable} {d_eave} {wing_pitch}: {measured} against {core}",
                        p.name
                    );
                    compared += 1;
                }
            }
        }
    }
    assert!(compared > 100, "{compared} roofs compared");
    for p in plans() {
        for style in [Style::Hip(40.0), Style::Gable(40.0), Style::Mansard] {
            if matches!(style, Style::Gable(_)) && p.ends.is_empty() {
                continue;
            }
            let pr = roof(&p, style, 0.6, 0.3);
            let core = tectora_core::engine::wall_area(&pr).expect("the core calculates");
            let measured = tectora_core::openings::measured_wall_area(&pr).expect("a wall area");
            assert!(
                (measured - core).abs() < 5e-3,
                "{} {style:?}: {measured} against {core}",
                p.name
            );
        }
    }
}

/// Every roof Tectora gives has a wall area (review ERR-003).
#[test]
fn a_roof_calculated_where_the_core_stops_has_a_wall_area() {
    // Jakub's T at 3.05 m, which the core calculates: the reference.
    let standard = tectora_core::openings::wall_area(&jakub_t(3.05)).expect("wall area");
    // House walls 2 × 15 m and its two gable ends, wing front and sides.
    for (front, notice) in [
        (2.9, WarningKind::MergedPlanes),
        (2.0, WarningKind::WingJoined),
    ] {
        let pr = jakub_t(front);
        assert_eq!(kinds(&calc(&pr), 0), vec![notice]);
        let area = tectora_core::openings::wall_area(&pr)
            .unwrap_or_else(|| panic!("front {front}: no wall area"));
        // Lower front eave, lower walls: within the wing's 10 m of wall
        // times the eave's drop, and never more than the reference.
        let drop = 3.05 - front;
        assert!(
            area < standard && area > standard - 10.0 * drop - 15.0,
            "front {front}: {area} against {standard}"
        );
    }
    // With a garage beside it the job still has one.
    let mut pr = jakub_t(2.0);
    pr.parts
        .push(rect_part("garage", 40.0, 0.0, 6.0, 5.0, 30.0, 3.0));
    assert!(tectora_core::openings::wall_area(&pr).is_some());
    // The moved hip corner (sweep).
    let quad = uniform(
        &[(0.0, 0.2), (12.0, 0.0), (12.0, 8.0), (0.0, 8.0)],
        35.0,
        3.0,
        0.5,
    );
    let area = tectora_core::openings::wall_area(&quad).expect("wall area");
    // Eave walls only: perimeter × (eave + overhang rise).
    let perimeter = 12.0f64.hypot(0.2) + 8.0 + 12.0 + 7.8;
    assert!(
        (area - perimeter * (3.0 + 0.5 * tan(35.0))).abs() < 0.02,
        "{area}"
    );
}
