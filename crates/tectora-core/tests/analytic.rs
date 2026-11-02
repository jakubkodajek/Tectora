//! Roofs whose every quantity follows from first principles.
//!
//! Each test builds a textbook roof over a grid of dimensions, pitches,
//! eave heights and overhangs, derives the expected ridge, hip, valley,
//! eave, verge and pitch-break lengths, face areas and ridge heights by hand
//! (plane geometry only, nothing taken from the engine or from DACAD), and
//! compares them with the engine. On top of that every roof must pass the
//! structural checks in [`check_structure`]: planar faces at their pitch,
//! face areas equal to their 3D polygons, faces that tile the roof's plan,
//! and every face side covered by exactly the right roof lines.
//!
//! Inputs are kept away from DACAD's own tolerances (overhangs of a few
//! millimetres, coordinates far from the origin, near-simultaneous sweep
//! events), where the engine deliberately follows DACAD instead of the
//! closed form; those cases live in `properties.rs` and `dacad_cases.rs`.
//!
//! Notation: `W`, `D` are the plan length and depth of the eave outline (the
//! wall outline pushed out by the overhang), `t = tan(pitch)`. A line of
//! plan length `l` that rises `h` is `sqrt(l² + h²)` long.

use std::f64::consts::PI;

use tectora_core::geom::{Point2, signed_area};
use tectora_core::{Chimney, HalfHip, PitchBreak, Project, RoofWindow};

mod support;
use support::*;

// ---------------------------------------------------------------- grids

const PITCHES: [f64; 9] = [12.0, 18.0, 22.5, 30.0, 35.0, 42.0, 45.0, 55.0, 68.0];

/// Gable roof on a rectangle: ridge `F = L`, eaves `T = 2L`, verges
/// `OL = OR = 2s`, area `2·L·s`, ridge at `e + (D/2)·t`, where `L = w + 2og`,
/// `D = d + 2oe` and the slope length is `s = (D/2)/cos`.
#[test]
fn gable_rectangle() {
    let mut r = Report::default();
    for w in [4.0, 7.5, 12.3, 20.0] {
        for d in [3.0, 6.0, 9.4] {
            for pitch in PITCHES {
                for (oe, og) in [(0.0, 0.0), (0.3, 0.0), (0.0, 0.25), (0.75, 0.6), (0.5, 1.2)] {
                    for eave in [2.5, 4.1] {
                        let p = rect(
                            w,
                            d,
                            [
                                (pitch, eave, oe),
                                (90.0, eave, og),
                                (pitch, eave, oe),
                                (90.0, eave, og),
                            ],
                        );
                        let roof = calc(&p);
                        let mut c = r.case();
                        no_error(&mut c, &roof);
                        let (len, dd) = (w + 2.0 * og, d + 2.0 * oe);
                        let s = dd / 2.0 / cos(pitch);
                        c.eq("F", roof.line("F"), len);
                        c.eq("T", roof.line("T"), 2.0 * len);
                        c.eq("OL", roof.line("OL"), 2.0 * s);
                        c.eq("OR", roof.line("OR"), 2.0 * s);
                        c.only(&roof, &["F", "T", "OL", "OR"]);
                        c.eq("area", roof.area, 2.0 * len * s);
                        c.eq("ridge z", roof.max_z(), eave + dd / 2.0 * tan(pitch));
                        c.eq("faces", roof.faces().count() as f64, 2.0);
                        for f in roof.faces() {
                            c.eq("face area", f.area, len * s);
                        }
                        check_structure(&mut c, &roof, len * dd);
                        c.done(format!(
                            "gable w={w} d={d} p={pitch} oe={oe} og={og} e={eave}"
                        ));
                    }
                }
            }
        }
    }
    r.finish("gable_rectangle");
}

/// Hip roof on a rectangle, one pitch: ridge `F = W − D`, four hips of plan
/// length `D/√2` rising `(D/2)·t`, eaves `2(W + D)`, area `W·D/cos`; the long
/// faces are trapezoids `(2W − D)·D/4/cos`, the short ones triangles
/// `D²/4/cos`. `W = D` is a pyramid.
#[test]
fn hip_rectangle() {
    let mut r = Report::default();
    for (w, d) in [
        (10.0, 6.0),
        (6.0, 6.0),
        (15.3, 4.2),
        (8.0, 7.9),
        (21.0, 11.0),
        (5.0, 3.0),
    ] {
        for pitch in PITCHES {
            for ov in [0.0, 0.2, 0.5, 1.1] {
                for eave in [3.0, 5.6] {
                    let def = (pitch, eave, ov);
                    let p = rect(w, d, [def; 4]);
                    let roof = calc(&p);
                    let mut c = r.case();
                    no_error(&mut c, &roof);
                    let (ww, dd) = (w + 2.0 * ov, d + 2.0 * ov);
                    let h = dd / 2.0 * tan(pitch);
                    c.eq("F", roof.line("F"), ww - dd);
                    c.eq("G", roof.line("G"), 4.0 * slant(dd / 2.0 * 2f64.sqrt(), h));
                    c.eq("T", roof.line("T"), 2.0 * (ww + dd));
                    c.only(&roof, &["F", "G", "T"]);
                    c.eq("area", roof.area, ww * dd / cos(pitch));
                    c.eq("ridge z", roof.max_z(), eave + h);
                    let mut areas: Vec<f64> = roof.faces().map(|f| f.area).collect();
                    areas.sort_by(f64::total_cmp);
                    c.eq("faces", areas.len() as f64, 4.0);
                    if areas.len() == 4 {
                        let tri = dd * dd / 4.0 / cos(pitch);
                        let trap = (2.0 * ww - dd) * dd / 4.0 / cos(pitch);
                        c.eq("triangle", areas[0], tri);
                        c.eq("triangle", areas[1], tri);
                        c.eq("trapezoid", areas[2], trap);
                        c.eq("trapezoid", areas[3], trap);
                    }
                    check_structure(&mut c, &roof, ww * dd);
                    c.done(format!("hip w={w} d={d} p={pitch} o={ov} e={eave}"));
                }
            }
        }
    }
    r.finish("hip_rectangle");
}

/// Hip roof with the long sides at `α` and the ends at `β`: the ridge is at
/// `h = (D/2)·tan α`, the end planes reach it `a = h/tan β` in from the
/// eave, so `F = W − 2a` and each hip runs from an eave corner to a ridge
/// end: plan length `√(a² + D²/4)`, rise `h`. Long faces have plan area
/// `(W − a)·D/2`, end faces `D·a/2`.
#[test]
fn hip_rectangle_two_pitches() {
    let mut r = Report::default();
    for (w, d) in [(12.0, 6.0), (9.0, 7.0), (16.4, 8.8)] {
        for alpha in [20.0, 30.0, 40.0, 50.0] {
            for beta in [25.0, 38.0, 45.0, 60.0, 72.0] {
                for ov in [0.0, 0.35, 0.8] {
                    let (ww, dd) = (w + 2.0 * ov, d + 2.0 * ov);
                    let h = dd / 2.0 * tan(alpha);
                    let a = h / tan(beta);
                    if ww - 2.0 * a < 0.2 {
                        continue; // the end planes meet: a different roof
                    }
                    let side = (alpha, 3.0, ov);
                    let end = (beta, 3.0, ov);
                    let p = rect(w, d, [side, end, side, end]);
                    let roof = calc(&p);
                    let mut c = r.case();
                    no_error(&mut c, &roof);
                    c.eq("F", roof.line("F"), ww - 2.0 * a);
                    c.eq("G", roof.line("G"), 4.0 * slant(a.hypot(dd / 2.0), h));
                    c.eq("T", roof.line("T"), 2.0 * (ww + dd));
                    c.only(&roof, &["F", "G", "T"]);
                    let long = (ww - a) * dd / 2.0;
                    let short = dd * a / 2.0;
                    c.eq(
                        "area",
                        roof.area,
                        2.0 * long / cos(alpha) + 2.0 * short / cos(beta),
                    );
                    for f in roof.faces() {
                        let want = if f.edge % 2 == 0 {
                            long / cos(alpha)
                        } else {
                            short / cos(beta)
                        };
                        c.eq(&format!("face {}", f.edge), f.area, want);
                    }
                    c.eq("ridge z", roof.max_z(), 3.0 + h);
                    check_structure(&mut c, &roof, ww * dd);
                    c.done(format!("hip2 w={w} d={d} a={alpha} b={beta} o={ov}"));
                }
            }
        }
    }
    r.finish("hip_rectangle_two_pitches");
}

/// Pyramid on a regular n-gon of circumradius `R`, turned by `phase`: the
/// eave outline is a regular n-gon of apothem `r = R·cos(π/n) + o`, its
/// circumradius `R' = r/cos(π/n)`; the apex is at `e + r·t`, the `n` hips
/// run from the corners (plan length `R'`) to it, the eave is
/// `2n·r·tan(π/n)` and the area `n·r²·tan(π/n)/cos`.
#[test]
fn pyramid_regular_polygon() {
    let mut r = Report::default();
    for n in 3..=8usize {
        for rr in [3.0, 5.5] {
            for phase in [0.0, 0.3, 1.0] {
                for pitch in [20.0, 30.0, 45.0, 60.0] {
                    for ov in [0.0, 0.4] {
                        let pts: Vec<(f64, f64)> = (0..n)
                            .map(|i| {
                                let a = phase + 2.0 * PI * i as f64 / n as f64;
                                // Four decimals, as DACAD stores coordinates.
                                let q = |v: f64| (v * 1e4).round() / 1e4;
                                (q(rr * a.cos()), q(rr * a.sin()))
                            })
                            .collect();
                        let p = uniform(&pts, pitch, 3.0, ov);
                        let roof = calc(&p);
                        let mut c = r.case();
                        no_error(&mut c, &roof);
                        let k = PI / n as f64;
                        let apo = rr * k.cos() + ov;
                        let big = apo / k.cos();
                        let h = apo * tan(pitch);
                        // Rounding the corners moves them by 50 µm at most.
                        let t5 = 5e-4 * n as f64;
                        c.eq_tol("G", roof.line("G"), n as f64 * slant(big, h), t5 + tol(big));
                        c.eq_tol(
                            "T",
                            roof.line("T"),
                            2.0 * n as f64 * apo * k.tan(),
                            t5 + tol(apo),
                        );
                        c.only(&roof, &["G", "T"]);
                        let plan = n as f64 * apo * apo * k.tan();
                        c.eq_tol("area", roof.area, plan / cos(pitch), 1e-3 * rr * n as f64);
                        c.eq_tol("apex z", roof.max_z(), 3.0 + h, 1e-3);
                        c.eq("faces", roof.faces().count() as f64, n as f64);
                        let off: Vec<Point2> = offset(&pts, ov);
                        check_structure(&mut c, &roof, signed_area(&off));
                        c.done(format!("pyramid n={n} R={rr} ph={phase} p={pitch} o={ov}"));
                    }
                }
            }
        }
    }
    r.finish("pyramid_regular_polygon");
}

/// Gable with its two slopes at different pitches and eave heights: the
/// planes `z = e1 + t1·(y + o1)` and `z = e2 + t2·(d + o2 − y)` meet in the
/// ridge at `y* = (e2 − e1 + t2·(d + o2) − t1·o1)/(t1 + t2)`.
#[test]
fn gable_asymmetric() {
    let mut r = Report::default();
    for (w, d) in [(10.0, 7.0), (6.5, 9.0)] {
        for p1 in [20.0, 35.0, 45.0, 60.0] {
            for p2 in [15.0, 30.0, 45.0, 52.0] {
                for (e1, e2) in [(3.0, 3.0), (3.0, 4.5), (5.0, 2.8)] {
                    for (o1, o2, og) in [(0.0, 0.0, 0.0), (0.5, 0.3, 0.4)] {
                        let (t1, t2) = (tan(p1), tan(p2));
                        let y = (e2 - e1 + t2 * (d + o2) - t1 * o1) / (t1 + t2);
                        let (run1, run2) = (y + o1, d + o2 - y);
                        if run1 < 0.5 || run2 < 0.5 {
                            continue;
                        }
                        let z = e1 + t1 * run1;
                        if z < e1.max(e2) + 0.1 {
                            continue;
                        }
                        let p = rect(
                            w,
                            d,
                            [(p1, e1, o1), (90.0, e1, og), (p2, e2, o2), (90.0, e1, og)],
                        );
                        let roof = calc(&p);
                        let mut c = r.case();
                        no_error(&mut c, &roof);
                        let len = w + 2.0 * og;
                        let (s1, s2) = (run1 / cos(p1), run2 / cos(p2));
                        c.eq("F", roof.line("F"), len);
                        c.eq("T", roof.line("T"), 2.0 * len);
                        c.eq("OL", roof.line("OL"), s1 + s2);
                        c.eq("OR", roof.line("OR"), s1 + s2);
                        c.only(&roof, &["F", "T", "OL", "OR"]);
                        c.eq("area", roof.area, len * (s1 + s2));
                        c.eq("ridge z", roof.max_z(), z);
                        check_structure(&mut c, &roof, len * (d + o1 + o2));
                        c.done(format!(
                            "asym w={w} d={d} p={p1}/{p2} e={e1}/{e2} o={o1}/{o2}/{og}"
                        ));
                    }
                }
            }
        }
    }
    r.finish("gable_asymmetric");
}

/// Mansard (hip roof with a pitch break on every wall): the lower planes at
/// `α1` rise to the break height `hb`, which lies `k = (hb − e)/tan α1` in
/// from the eave; above it a hip roof at `α2` covers the `(W − 2k) × (D − 2k)`
/// rectangle. Break lines `c` run round that rectangle; the lower hips have
/// plan length `k√2` and rise `hb − e`.
#[test]
fn mansard_hip() {
    let mut r = Report::default();
    for (w, d) in [(10.0, 6.0), (12.0, 9.0)] {
        for a1 in [55.0, 65.0, 75.0] {
            for a2 in [15.0, 25.0, 35.0] {
                for rise in [1.2, 2.0] {
                    for ov in [0.0, 0.4] {
                        let eave = 3.0;
                        let k = rise / tan(a1);
                        let (ww, dd) = (w + 2.0 * ov, d + 2.0 * ov);
                        let (w2, d2) = (ww - 2.0 * k, dd - 2.0 * k);
                        let mut p = rect(w, d, [(a1, eave, ov); 4]);
                        for e in &mut p.parts[0].edges {
                            e.breaks.push(PitchBreak {
                                height: eave + rise,
                                pitch_deg: a2,
                            });
                        }
                        let roof = calc(&p);
                        let mut c = r.case();
                        no_error(&mut c, &roof);
                        let h2 = d2 / 2.0 * tan(a2);
                        c.eq("c", roof.line("c"), 2.0 * (w2 + d2));
                        c.eq("T", roof.line("T"), 2.0 * (ww + dd));
                        c.eq("F", roof.line("F"), w2 - d2);
                        let lower = 4.0 * slant(k * 2f64.sqrt(), rise);
                        let upper = 4.0 * slant(d2 / 2.0 * 2f64.sqrt(), h2);
                        c.eq("G", roof.line("G"), lower + upper);
                        c.only(&roof, &["c", "T", "F", "G"]);
                        let area = (ww * dd - w2 * d2) / cos(a1) + w2 * d2 / cos(a2);
                        c.eq("area", roof.area, area);
                        c.eq("ridge z", roof.max_z(), eave + rise + h2);
                        c.eq("faces", roof.faces().count() as f64, 8.0);
                        check_structure(&mut c, &roof, ww * dd);
                        c.done(format!(
                            "mansard w={w} d={d} a={a1}/{a2} rise={rise} o={ov}"
                        ));
                    }
                }
            }
        }
    }
    r.finish("mansard_hip");
}

/// Gambrel (gable roof with a pitch break on both eaves): break lines
/// `c = 2L`, verges `4·(k/cos α1 + (D/2 − k)/cos α2)`.
#[test]
fn gambrel() {
    let mut r = Report::default();
    for (w, d) in [(10.0, 8.0), (7.0, 11.0)] {
        for a1 in [50.0, 60.0, 70.0] {
            for a2 in [20.0, 30.0] {
                for rise in [1.0, 2.2] {
                    for (oe, og) in [(0.0, 0.0), (0.5, 0.3)] {
                        let eave = 3.2;
                        let k = rise / tan(a1);
                        let (len, dd) = (w + 2.0 * og, d + 2.0 * oe);
                        let mut p = rect(
                            w,
                            d,
                            [
                                (a1, eave, oe),
                                (90.0, eave, og),
                                (a1, eave, oe),
                                (90.0, eave, og),
                            ],
                        );
                        for i in [0, 2] {
                            p.parts[0].edges[i].breaks.push(PitchBreak {
                                height: eave + rise,
                                pitch_deg: a2,
                            });
                        }
                        let roof = calc(&p);
                        let mut c = r.case();
                        no_error(&mut c, &roof);
                        let (s1, s2) = (k / cos(a1), (dd / 2.0 - k) / cos(a2));
                        c.eq("c", roof.line("c"), 2.0 * len);
                        c.eq("F", roof.line("F"), len);
                        c.eq("T", roof.line("T"), 2.0 * len);
                        c.eq("OL+OR", roof.line("OL") + roof.line("OR"), 4.0 * (s1 + s2));
                        c.eq("OL-OR", roof.line("OL") - roof.line("OR"), 0.0);
                        c.only(&roof, &["c", "F", "T", "OL", "OR"]);
                        c.eq("area", roof.area, 2.0 * len * (s1 + s2));
                        let z = eave + rise + (dd / 2.0 - k) * tan(a2);
                        c.eq("ridge z", roof.max_z(), z);
                        check_structure(&mut c, &roof, len * dd);
                        c.done(format!(
                            "gambrel w={w} d={d} a={a1}/{a2} rise={rise} o={oe}/{og}"
                        ));
                    }
                }
            }
        }
    }
    r.finish("gambrel");
}

/// Half-hip (Krüppelwalm) on both gables: the clipped plane at `β` starts
/// at height `hh` on the gable's eave line and climbs inward. The main
/// ridge (height `H = e + (D/2)·tan α`) is cut back by `(H − hh)/tan β` at
/// each end; the clipped eave at `hh` runs between the main planes, which
/// reach `hh` a plan distance `s = (hh − e)/tan α` in from their eaves.
#[test]
fn half_hip() {
    let mut r = Report::default();
    for (w, d) in [(10.0, 6.0), (12.5, 8.0)] {
        for alpha in [30.0, 40.0, 50.0] {
            for beta in [45.0, 60.0, 75.0] {
                for frac in [0.35, 0.6] {
                    for (oe, og) in [(0.0, 0.0), (0.5, 0.3)] {
                        let eave = 3.0;
                        let (len, dd) = (w + 2.0 * og, d + 2.0 * oe);
                        let big_h = eave + dd / 2.0 * tan(alpha);
                        let hh = eave + frac * (big_h - eave);
                        let mut p = rect(
                            w,
                            d,
                            [
                                (alpha, eave, oe),
                                (90.0, eave, og),
                                (alpha, eave, oe),
                                (90.0, eave, og),
                            ],
                        );
                        for i in [1, 3] {
                            p.parts[0].edges[i].half_hip = Some(HalfHip {
                                height: hh,
                                pitch_deg: beta,
                            });
                        }
                        let roof = calc(&p);
                        let mut c = r.case();
                        no_error(&mut c, &roof);
                        let cut = (big_h - hh) / tan(beta);
                        let s = (hh - eave) / tan(alpha);
                        let clipped = dd - 2.0 * s;
                        c.eq("F", roof.line("F"), len - 2.0 * cut);
                        c.eq("T", roof.line("T"), 2.0 * len + 2.0 * clipped);
                        let hip = slant(cut.hypot(dd / 2.0 - s), big_h - hh);
                        c.eq("G", roof.line("G"), 4.0 * hip);
                        let verge = s / cos(alpha);
                        c.eq("OL", roof.line("OL"), 2.0 * verge);
                        c.eq("OR", roof.line("OR"), 2.0 * verge);
                        c.only(&roof, &["F", "T", "G", "OL", "OR"]);
                        let tri = clipped * cut / 2.0;
                        let area = (len * dd - 2.0 * tri) / cos(alpha) + 2.0 * tri / cos(beta);
                        c.eq("area", roof.area, area);
                        c.eq("ridge z", roof.max_z(), big_h);
                        check_structure(&mut c, &roof, len * dd);
                        c.done(format!(
                            "halfhip w={w} d={d} a={alpha} b={beta} f={frac} o={oe}/{og}"
                        ));
                    }
                }
            }
        }
    }
    r.finish("half_hip");
}

// ------------------------------------------------- orthogonal arms, one width

/// Uniform hip roof on an outline made of arms of one width `b` (after the
/// overhang): every hip and valley runs at 45° in plan over half the arm
/// width (plan length `b/√2`, rise `b·t/2`), ridges run along the arm axes.
/// `ridge` is the expected total ridge length, `hips` and `valleys` the
/// number of each.
#[allow(clippy::too_many_arguments)]
fn arms_case(
    r: &mut Report,
    label: &str,
    walls: &[(f64, f64)],
    b: f64,
    pitch: f64,
    ov: f64,
    ridge: f64,
    hips: usize,
    valleys: usize,
) {
    let p = uniform(walls, pitch, 3.0, ov);
    let roof = calc(&p);
    let mut c = r.case();
    no_error(&mut c, &roof);
    let piece = slant(b / 2f64.sqrt(), b / 2.0 * tan(pitch));
    let eaves = offset(walls, ov);
    let plan = signed_area(&eaves);
    let per: f64 = (0..eaves.len())
        .map(|i| eaves[i].distance(eaves[(i + 1) % eaves.len()]))
        .sum();
    c.eq("F", roof.line("F"), ridge);
    c.eq("G", roof.line("G"), hips as f64 * piece);
    c.eq("K", roof.line("K"), valleys as f64 * piece);
    c.eq("T", roof.line("T"), per);
    c.only(&roof, &["F", "G", "K", "T"]);
    c.eq("area", roof.area, plan / cos(pitch));
    c.eq("ridge z", roof.max_z(), 3.0 + b / 2.0 * tan(pitch));
    check_structure(&mut c, &roof, plan);
    c.done(format!("{label} b={b} p={pitch} o={ov}"));
}

/// L: arms `A` (along x) and `B` (along y), width `b0` each: ridges
/// `A' − b + B' − b`, five hips, one valley (eave outline `A' = A + 2o`,
/// `b = b0 + 2o`).
#[test]
fn l_shape() {
    let mut r = Report::default();
    for (a, bb, b0) in [(10.0, 8.0, 4.0), (12.0, 12.0, 5.0), (9.0, 15.0, 3.6)] {
        for pitch in [20.0, 30.0, 45.0, 60.0] {
            for ov in [0.0, 0.3, 0.6] {
                let walls = [(0.0, 0.0), (a, 0.0), (a, b0), (b0, b0), (b0, bb), (0.0, bb)];
                let b = b0 + 2.0 * ov;
                let ridge = (a + 2.0 * ov - b) + (bb + 2.0 * ov - b);
                arms_case(
                    &mut r,
                    &format!("L A={a} B={bb}"),
                    &walls,
                    b,
                    pitch,
                    ov,
                    ridge,
                    5,
                    1,
                );
            }
        }
    }
    r.finish("l_shape");
}

/// T: bar `A × b0`, stem of width `b0` centred at `x = cx` up to `y = B`:
/// ridges `(A' − b) + (B' − b)`, six hips, two valleys.
#[test]
fn t_shape() {
    let mut r = Report::default();
    for (a, bb, b0, cx) in [(14.0, 10.0, 4.0, 7.0), (16.0, 9.0, 3.0, 6.0)] {
        for pitch in [20.0, 35.0, 50.0] {
            for ov in [0.0, 0.4] {
                let h = b0 / 2.0;
                let walls = [
                    (0.0, 0.0),
                    (a, 0.0),
                    (a, b0),
                    (cx + h, b0),
                    (cx + h, bb),
                    (cx - h, bb),
                    (cx - h, b0),
                    (0.0, b0),
                ];
                let b = b0 + 2.0 * ov;
                let ridge = (a + 2.0 * ov - b) + (bb + 2.0 * ov - b);
                arms_case(
                    &mut r,
                    &format!("T A={a} B={bb} cx={cx}"),
                    &walls,
                    b,
                    pitch,
                    ov,
                    ridge,
                    6,
                    2,
                );
            }
        }
    }
    r.finish("t_shape");
}

/// U: base `A × b0` with legs of width `b0` up to `y = B` at both ends:
/// ridges `(A' − b) + 2(B' − b)`, six hips, two valleys.
#[test]
fn u_shape() {
    let mut r = Report::default();
    for (a, bb, b0) in [(14.0, 9.0, 4.0), (12.0, 12.0, 3.0)] {
        for pitch in [25.0, 40.0, 55.0] {
            for ov in [0.0, 0.35] {
                let walls = [
                    (0.0, 0.0),
                    (a, 0.0),
                    (a, bb),
                    (a - b0, bb),
                    (a - b0, b0),
                    (b0, b0),
                    (b0, bb),
                    (0.0, bb),
                ];
                let b = b0 + 2.0 * ov;
                let ridge = (a + 2.0 * ov - b) + 2.0 * (bb + 2.0 * ov - b);
                arms_case(
                    &mut r,
                    &format!("U A={a} B={bb}"),
                    &walls,
                    b,
                    pitch,
                    ov,
                    ridge,
                    6,
                    2,
                );
            }
        }
    }
    r.finish("u_shape");
}

/// Plus sign: arms of width `b0` reaching `A/2` and `B/2` out from the
/// centre: two crossing ridges `(A' − b) + (B' − b)`, eight hips, four
/// valleys meeting in the centre.
#[test]
fn cross_shape() {
    let mut r = Report::default();
    for (a, bb, b0) in [(14.0, 12.0, 4.0), (16.0, 16.0, 5.0)] {
        for pitch in [25.0, 40.0, 55.0] {
            for ov in [0.0, 0.3] {
                let (x, y, h) = (a / 2.0, bb / 2.0, b0 / 2.0);
                let walls = [
                    (-h, -y),
                    (h, -y),
                    (h, -h),
                    (x, -h),
                    (x, h),
                    (h, h),
                    (h, y),
                    (-h, y),
                    (-h, h),
                    (-x, h),
                    (-x, -h),
                    (-h, -h),
                ];
                let b = b0 + 2.0 * ov;
                let ridge = (a + 2.0 * ov - b) + (bb + 2.0 * ov - b);
                arms_case(
                    &mut r,
                    &format!("+ A={a} B={bb}"),
                    &walls,
                    b,
                    pitch,
                    ov,
                    ridge,
                    8,
                    4,
                );
            }
        }
    }
    r.finish("cross_shape");
}

// ---------------------------------------------------------------- openings

/// A chimney `cw × cd` standing in the front slope of a gable roof, square
/// to the eave and clear of every line: the roof loses its plan area over
/// the cosine, the lower side is a front abutment `WV = cw`, the upper side a
/// back abutment `WH = cw`, the two sides run up the slope, `WS = 2·cd/cos`.
#[test]
fn chimney_in_gable_slope() {
    let mut r = Report::default();
    for pitch in [20.0, 30.0, 45.0, 55.0] {
        for (cw, cd) in [(0.6, 0.6), (1.0, 0.5), (0.4, 1.2)] {
            for (cx, cy) in [(3.0, 1.6), (7.5, 2.2)] {
                let (w, d, ov) = (10.0, 8.0, 0.4);
                let defs = [
                    (pitch, 3.0, ov),
                    (90.0, 3.0, ov),
                    (pitch, 3.0, ov),
                    (90.0, 3.0, ov),
                ];
                let bare = calc(&rect(w, d, defs));
                let mut p = rect(w, d, defs);
                p.chimneys.push(Chimney {
                    corners: vec![
                        Point2::new(cx - cw / 2.0, cy - cd / 2.0),
                        Point2::new(cx + cw / 2.0, cy - cd / 2.0),
                        Point2::new(cx + cw / 2.0, cy + cd / 2.0),
                        Point2::new(cx - cw / 2.0, cy + cd / 2.0),
                    ],
                    height: 10.0,
                });
                let roof = calc(&p);
                let mut c = r.case();
                no_error(&mut c, &roof);
                c.eq("area loss", bare.area - roof.area, cw * cd / cos(pitch));
                c.eq("WV", roof.line("WV"), cw);
                c.eq("WH", roof.line("WH"), cw);
                c.eq("WS", roof.line("WS"), 2.0 * cd / cos(pitch));
                for code in ["F", "T", "OL", "OR"] {
                    c.eq(code, roof.line(code), bare.line(code));
                }
                c.only(&roof, &["F", "T", "OL", "OR", "WV", "WH", "WS"]);
                c.done(format!("chimney p={pitch} {cw}x{cd} at ({cx},{cy})"));
            }
        }
    }
    r.finish("chimney_in_gable_slope");
}

/// A roof window inside one slope takes its own in-plane size `w × h` off
/// the roof area and leaves every line alone.
#[test]
fn window_in_gable_slope() {
    let mut r = Report::default();
    for pitch in [25.0, 35.0, 45.0] {
        for (ww, hh) in [(0.78, 1.18), (1.14, 1.4), (0.55, 0.78)] {
            let defs = [
                (pitch, 3.0, 0.4),
                (90.0, 3.0, 0.3),
                (pitch, 3.0, 0.4),
                (90.0, 3.0, 0.3),
            ];
            let bare = calc(&rect(10.0, 8.0, defs));
            let mut p = rect(10.0, 8.0, defs);
            p.windows.push(RoofWindow {
                x: 4.0,
                y: 1.0,
                width: ww,
                height: hh,
            });
            let roof = calc(&p);
            let mut c = r.case();
            no_error(&mut c, &roof);
            c.eq("area loss", bare.area - roof.area, ww * hh);
            for (code, len) in &bare.lines {
                c.eq(code, roof.line(code), *len);
            }
            c.done(format!("window p={pitch} {ww}x{hh}"));
        }
    }
    r.finish("window_in_gable_slope");
}

// ---------------------------------------------------------------- scaling

/// Scaling a well-conditioned roof by `k` scales its lines by `k` and its
/// area by `k²`, keeps its pitches, and shifting it keeps every quantity.
#[test]
fn similarity() {
    let mut r = Report::default();
    let shapes: Vec<Project> = vec![
        rect(10.0, 6.0, [(40.0, 3.0, 0.5); 4]),
        rect(
            10.0,
            6.0,
            [
                (35.0, 3.0, 0.4),
                (90.0, 3.0, 0.3),
                (35.0, 3.0, 0.4),
                (90.0, 3.0, 0.3),
            ],
        ),
        uniform(
            &[
                (0.0, 0.0),
                (10.0, 0.0),
                (10.0, 4.0),
                (4.0, 4.0),
                (4.0, 9.0),
                (0.0, 9.0),
            ],
            30.0,
            3.0,
            0.3,
        ),
    ];
    for base in &shapes {
        let b = calc(base);
        for k in [0.5, 2.0, 3.0] {
            let mut p = base.clone();
            for e in &mut p.parts[0].edges {
                e.start = Point2::new(e.start.x * k, e.start.y * k);
                e.overhang *= k;
                e.eave_height *= k;
            }
            let s = calc(&p);
            let mut c = r.case();
            c.eq("area", s.area, b.area * k * k);
            for (code, len) in &b.lines {
                c.eq(code, s.line(code), len * k);
            }
            c.ok("same codes", s.lines.len() == b.lines.len());
            c.done(format!(
                "scale k={k} {:?}",
                b.lines.keys().collect::<Vec<_>>()
            ));
        }
        for (dx, dy) in [(37.0, -12.5), (20.0, 50.0), (-80.0, 100.0), (-60.0, -45.5)] {
            let mut p = base.clone();
            for e in &mut p.parts[0].edges {
                e.start = Point2::new(e.start.x + dx, e.start.y + dy);
            }
            let s = calc(&p);
            let mut c = r.case();
            c.eq("area", s.area, b.area);
            for (code, len) in &b.lines {
                c.eq(code, s.line(code), *len);
            }
            c.done(format!(
                "shift ({dx},{dy}) {:?}",
                b.lines.keys().collect::<Vec<_>>()
            ));
        }
    }
    r.finish("similarity");
}

/// Far from the origin the float32 core loses area precision, as DACAD's
/// own core does (it computes in absolute float32 coordinates; dcore is
/// byte-identical to it): an L-shaped hip roof of 82.86 m² comes out
/// 82.82 m² at (−120, 260), 82.77 m² at (1000, 0) and 81.81 m² (−1.3 %)
/// at (0, −2500). Lines stay within a few millimetres. This pins the size of the effect so a regression shows.
#[test]
fn far_from_origin_precision() {
    let walls = [
        (0.0, 0.0),
        (10.0, 0.0),
        (10.0, 4.0),
        (4.0, 4.0),
        (4.0, 9.0),
        (0.0, 9.0),
    ];
    let base = calc(&uniform(&walls, 30.0, 3.0, 0.3));
    let mut r = Report::default();
    // (offset, largest relative area error seen there)
    for (dx, dy, max_rel) in [
        (-120.0, 260.0, 1e-3),
        (500.0, 500.0, 2e-3),
        (1000.0, 0.0, 2e-3),
        (0.0, -2500.0, 2e-2),
    ] {
        let moved: Vec<(f64, f64)> = walls.iter().map(|&(x, y)| (x + dx, y + dy)).collect();
        let roof = calc(&uniform(&moved, 30.0, 3.0, 0.3));
        let mut c = r.case();
        let rel = (roof.area - base.area).abs() / base.area;
        c.ok(&format!("area off by {:.3} %", rel * 100.0), rel < max_rel);
        for (code, len) in &base.lines {
            c.eq_tol(code, roof.line(code), *len, 5e-3);
        }
        c.done(format!("L at ({dx},{dy})"));
    }
    r.finish("far_from_origin_precision");
}
