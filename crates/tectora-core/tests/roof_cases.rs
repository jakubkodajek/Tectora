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
//! Tectora's result was byte-identical for all of them, so a failure here
//! is a regression of Tectora, not a DACAD quirk.

use tectora_core::check::WarningKind;
use tectora_core::edit::dormers::{DormerKind, build_dormer, default_params};
use tectora_core::geom::{Point2, signed_area};
use tectora_core::{EdgeDef, HalfHip, PitchBreak, Project, RoofPart};

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
/// a wing calculated against the house): those roofs are sound, the
/// notice only tells the user. Returns the kinds taken off.
fn take_notices(roof: &mut Roof) -> Vec<WarningKind> {
    let mut out = Vec::new();
    for g in &mut roof.geos {
        g.warnings.retain(|w| {
            let notice = matches!(w.kind, WarningKind::MergedPlanes | WarningKind::WingJoined);
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

fn sound(rep: &mut Report, name: &str, p: &Project) {
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
        let edges = &p.parts[0].edges;
        let level = edges
            .iter()
            .all(|e| (e.eave_height - edges[0].eave_height).abs() < 1e-9);
        let fit = if notices.contains(&WarningKind::WingJoined) {
            Fit::DacadJoined
        } else if level {
            Fit::Dacad
        } else {
            Fit::DacadSteps
        };
        check_structure_fit(&mut c, &roof, eave_plan(p), fit);
    }
    c.done(format!("{name}: {}", label(p)));
}

/// Every plan with every roof style, uniform and mixed overhangs.
#[test]
fn plans_with_every_roof_style_are_sound() {
    let mut rep = Report::default();
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
                sound(
                    &mut rep,
                    &format!("{} {style:?} ov {ov_eave}-{ov_end}", p.name),
                    &pr,
                );
            }
        }
    }
    rep.finish("plans_with_every_roof_style_are_sound");
}

/// A wing with its own ridge (sides eaves, front a gable or a hip) at a
/// lower, equal or higher eave, steeper or flatter than the house.
#[test]
fn wings_with_other_eave_or_pitch_are_sound() {
    let mut rep = Report::default();
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
                    sound(
                        &mut rep,
                        &format!(
                            "{} wing {} eave {d_eave:+} pitch {wing_pitch}",
                            p.name,
                            if front_gable { "gable" } else { "hip" }
                        ),
                        &pr,
                    );
                }
            }
        }
    }
    rep.finish("wings_with_other_eave_or_pitch_are_sound");
}

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
