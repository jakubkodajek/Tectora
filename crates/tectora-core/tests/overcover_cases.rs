//! `check::sound` and plan covered more than once (`core-leftovers`): above
//! the 3 % cap a roof passes only when the extra cover is higher overhangs
//! over lower roofs (two low wings: the house's front over their roofs).

use tectora_core::check::{Fit, sound};
use tectora_core::openings::generate_project;
use tectora_core::{Project, RoofPart};

mod support;
use support::*;

/// A 20 m house (y 2.5 to 7.5, gable ends) with two 5 m wide low wings in
/// front of it, sides gables.
fn two_wings() -> Project {
    let mut e = Vec::new();
    for x in [2.5, 12.5] {
        e.push(edge(x, 0.0, 40.0, 2.0, 0.5));
        e.push(edge(x + 5.0, 0.0, 90.0, 5.0, 0.5));
        e.push(edge(x + 5.0, 2.5, 40.0, 5.0, 0.5));
        e.push(edge(if x < 3.0 { 12.5 } else { 20.0 }, 2.5, 90.0, 5.0, 0.5));
    }
    e.push(edge(20.0, 7.5, 40.0, 5.0, 0.5));
    e.push(edge(0.0, 7.5, 90.0, 5.0, 0.5));
    e.push(edge(0.0, 2.5, 40.0, 5.0, 0.5));
    e.push(edge(2.5, 2.5, 90.0, 5.0, 0.5));
    let mut p = project(e);
    p.parts[0].height = Some(5.0);
    p
}

#[test]
fn the_overhang_over_two_low_wings_is_explained_and_nothing_else_is() {
    let p = two_wings();
    let part: &RoofPart = &p.parts[0];
    let geo = generate_project(&p).remove(0);
    assert!(geo.error.is_none(), "{:?}", geo.error);
    sound(part, &geo, Fit::JOINED).expect("two low wings");
    // A face emitted twice covers the plan over again with no gap between.
    let mut twice = geo.clone();
    let wing = twice.faces.iter().find(|f| f.edge == 0).cloned().unwrap();
    twice.faces.push(wing);
    assert!(sound(part, &twice, Fit::JOINED).is_err(), "a face twice");
    // A wing roof 5 cm under the house's plane lies in its plane.
    let mut flat = geo.clone();
    for f in flat.faces.iter_mut().filter(|f| f.edge != 0 && f.edge != 4) {
        for q in f.polygon.iter_mut() {
            q[2] -= 0.05;
        }
    }
    assert!(sound(part, &flat, Fit::JOINED).is_err(), "5 cm under");
}

// ============================================================ fix round 1
// TEST-002: `sound` refuses a plan covered more than once above the cap
// (3 % where eaves differ, none where the roof is level) unless the extra
// cover is a higher overhang over a lower roof. The input is a real roof
// with one face copied, shrunk, lifted and labelled with a gable wall that gets a
// pitch break at the copy's pitch (so no earlier check of `sound` stops
// it), with its roof lines copied with it.

use tectora_core::check::sound as sound_check;
use tectora_core::model::PitchBreak;
use tectora_core::roof::{LineKind, RoofGeometry, RoofLine};

/// `geo` with the face of wall `face` copied: shrunk to 80 % about its
/// centre (so no side lies on an eave line), `dz` higher, as a face of wall
/// `as_wall` (a gable wall of `part`, which gets a pitch break at the
/// copy's pitch so that no earlier check of `sound` stops it), with a roof
/// line along each side of the copy.
fn with_lifted_copy(
    part: &RoofPart,
    geo: &RoofGeometry,
    face: usize,
    as_wall: usize,
    dz: f64,
) -> (RoofPart, RoofGeometry) {
    let mut part = part.clone();
    let mut geo = geo.clone();
    let mut copy = geo.faces.iter().find(|f| f.edge == face).cloned().unwrap();
    let n = copy.polygon.len() as f64;
    let mut c = [0.0; 3];
    for v in &copy.polygon {
        for k in 0..3 {
            c[k] += v[k] / n;
        }
    }
    for v in &mut copy.polygon {
        for k in 0..3 {
            v[k] = c[k] + 0.8 * (v[k] - c[k]);
        }
        v[2] += dz;
    }
    copy.area *= 0.64;
    part.edges[as_wall].breaks.push(PitchBreak {
        height: 100.0,
        pitch_deg: copy.pitch_deg,
    });
    copy.edge = as_wall;
    let m = copy.polygon.len();
    for i in 0..m {
        geo.lines.push(RoofLine {
            kind: LineKind::Ridge,
            a: copy.polygon[i],
            b: copy.polygon[(i + 1) % m],
            side: None,
        });
    }
    geo.faces.push(copy);
    (part, geo)
}

fn cover_refused(part: &RoofPart, geo: &RoofGeometry, what: &str) {
    match sound_check(part, geo, Fit::JOINED) {
        Err(e) if e.starts_with("faces cover") => {}
        other => panic!("{what}: want the cover refusal, got {other:?}"),
    }
}

/// Over the cap and not explained: the lifted copy of a wing roof lies over
/// the plan of the whole wing, not in its own wall's overhang strip.
#[test]
fn sound_refuses_a_cover_that_lies_outside_the_overhang_strip() {
    let p = two_wings();
    let geo = generate_project(&p).remove(0);
    assert!(geo.error.is_none());
    let (part, lifted) = with_lifted_copy(&p.parts[0], &geo, 0, 1, 0.3);
    cover_refused(&part, &lifted, "wing roof copied 0.3 m higher");
}

/// `geo` with a rectangle of the face of wall `face` (corners `rect` in
/// plan, clear of every eave line) copied `dz` higher as a face of the gable
/// wall `as_wall` (which gets a pitch break at the face's pitch), with a
/// roof line along each side.
fn with_strip_copy(
    part: &RoofPart,
    geo: &RoofGeometry,
    face: usize,
    as_wall: usize,
    rect: [(f64, f64); 4],
    dz: f64,
) -> (RoofPart, RoofGeometry) {
    let src = geo.faces.iter().find(|f| f.edge == face).unwrap();
    let nrm = area_vector(&src.polygon);
    let z_at = |x: f64, y: f64| {
        let o = src.polygon[0];
        o[2] - (nrm[0] * (x - o[0]) + nrm[1] * (y - o[1])) / nrm[2]
    };
    let mut part = part.clone();
    part.edges[as_wall].breaks.push(PitchBreak {
        height: 100.0,
        pitch_deg: src.pitch_deg,
    });
    let mut out = geo.clone();
    let polygon: Vec<[f64; 3]> = rect.iter().map(|&(x, y)| [x, y, z_at(x, y) + dz]).collect();
    for i in 0..4 {
        out.lines.push(RoofLine {
            kind: LineKind::Ridge,
            a: polygon[i],
            b: polygon[(i + 1) % 4],
            side: None,
        });
    }
    let mut copy = src.clone();
    copy.edge = as_wall;
    copy.polygon = polygon;
    copy.area = norm(area_vector(&copy.polygon));
    out.faces.push(copy);
    (part, out)
}

/// A level roof (the eaves do not differ) has no explained cover: a copy of
/// the front roof that lies wholly in the gable wall's overhang strip and
/// 0.3 m over the front roof (the explanation that two low wings get) is
/// still refused.
#[test]
fn sound_refuses_any_extra_cover_on_a_level_roof() {
    let mut p = project(vec![
        edge(0.0, 0.0, 40.0, 3.0, 0.5),
        edge(10.0, 0.0, 90.0, 3.0, 0.5),
        edge(10.0, 8.0, 40.0, 3.0, 0.5),
        edge(0.0, 8.0, 90.0, 3.0, 0.5),
    ]);
    p.parts[0].height = Some(3.0);
    let geo = generate_project(&p).remove(0);
    assert!(geo.error.is_none(), "{:?}", geo.error);
    sound_check(&p.parts[0], &geo, Fit::JOINED).expect("the level roof is sound");
    // The strip of wall 1 (x 10 to 10.5).
    let rect = [(10.0, 0.5), (10.5, 0.5), (10.5, 4.0), (10.0, 4.0)];
    let (part, lifted) = with_strip_copy(&p.parts[0], &geo, 0, 1, rect, 0.3);
    cover_refused(&part, &lifted, "strip roof 0.3 m over a level roof");
}

/// Where the eaves differ an overhang strip over a roof counts only when it
/// lies clear above it (10 cm): the same strip copy is explained at 0.3 m
/// and refused at 5 cm.
#[test]
fn sound_explains_a_strip_only_where_it_lies_clear_above() {
    let p = two_wings();
    let geo = generate_project(&p).remove(0);
    assert!(geo.error.is_none(), "{:?}", geo.error);
    // The strip of wall 1 ((7.5, 0) to (7.5, 2.5)): x 7.5 to 8, in front of
    // the house's eave line.
    let rect = [(7.5, 0.0), (8.0, 0.0), (8.0, 1.5), (7.5, 1.5)];
    let (part, clear) = with_strip_copy(&p.parts[0], &geo, 0, 1, rect, 0.3);
    // (the strip's cover alone is below the cap; the house's overhang over
    // the wings, 6 m2, is what makes the roof need an explanation)
    sound_check(&part, &clear, Fit::JOINED).expect("a strip 0.3 m above is explained");
    let (part, near) = with_strip_copy(&p.parts[0], &geo, 0, 1, rect, 0.05);
    cover_refused(&part, &near, "a strip 5 cm above is not explained");
}
