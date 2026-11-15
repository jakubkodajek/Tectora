//! The gate behind the straightening of centimetre-rounded turns
//! (`core-leftovers`, criterion 12): `check::sound` accepts the roof of the
//! walls it was calculated for and refuses it for walls with a corner moved
//! 5 cm, so a straightening that moved more than rounding would not pass.

use tectora_core::check::{Fit, sound};
use tectora_core::geom::Point2;
use tectora_core::openings::generate_project;
use tectora_core::{Project, RoofPart};

mod support;
use support::*;

fn jakub_t(front: f64) -> Project {
    let pts = [
        (5.0, 0.0),
        (10.0, 0.0),
        (10.0, 2.5),
        (15.0, 2.5),
        (15.0, 7.5),
        (0.0, 7.5),
        (0.0, 2.5),
        (5.0, 2.5),
    ];
    project(
        pts.iter()
            .enumerate()
            .map(|(i, &(x, y))| {
                let (pitch, eave) = if i % 2 == 1 { (90.0, 5.0) } else { (40.0, 5.0) };
                edge(x, y, pitch, if i == 0 { front } else { eave }, 0.5)
            })
            .collect(),
    )
}

#[test]
fn sound_refuses_a_roof_for_walls_with_a_corner_moved_5_cm() {
    let p = jakub_t(5.0);
    let geos = generate_project(&p);
    let geo = &geos[0];
    assert!(geo.error.is_none(), "{:?}", geo.error);
    let part: &RoofPart = &p.parts[0];
    sound(part, geo, Fit::JOINED).expect("the roof fits the walls it was calculated for");
    for (corner, dx, dy) in [
        (3, 0.0, 0.05),
        (4, 0.0, 0.05),
        (5, 0.0, 0.05),
        (0, 0.05, 0.0),
    ] {
        let mut moved = part.clone();
        let s = moved.edges[corner].start;
        moved.edges[corner].start = Point2::new(s.x + dx, s.y + dy);
        assert!(
            sound(&moved, geo, Fit::JOINED).is_err(),
            "corner {corner} moved 5 cm"
        );
    }
}
