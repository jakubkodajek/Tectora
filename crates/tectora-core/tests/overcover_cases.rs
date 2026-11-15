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
