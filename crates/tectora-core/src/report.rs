//! The roof calculation as the desktop app shows it: geometry and
//! quantities per part, totals, DACAD line codes, and the problems that
//! kept a part from being calculated.

use serde_json::{Value, json};

use crate::Project;
use crate::quantities::{dacad_line_totals, manual_quantities, part_quantities};
use crate::roof::RoofQuantities;

/// Roof geometry and quantities for every part, plus totals. A part the
/// engine could not take has empty geometry; `problems` says why, and a
/// DACAD error the calculation ended with is in each part's
/// `geometry.error`.
pub fn calc(project: &Project) -> Value {
    let mut total = RoofQuantities::default();
    let parts: Vec<Value> = project
        .parts
        .iter()
        .zip(crate::openings::generate_project(project))
        .map(|(part, geometry)| {
            // Net of the core's negative lines, as the totals.
            let q = part_quantities(&geometry);
            total.add(&q);
            json!({ "name": part.name, "geometry": geometry, "quantities": q })
        })
        .collect();
    // Lines and faces added by hand (DACAD: Dachlinieneingabe, Neue Dachfläche).
    total.add(&manual_quantities(project));
    json!({
        "parts": parts,
        "totals": total,
        "codes": dacad_line_totals(project),
        "problems": project.problems(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::examples;

    #[test]
    fn reports_input_problems_per_edge() {
        let mut p = examples::find("gable").unwrap().project();
        p.parts[0].edges[1].pitch_deg = 0.0;
        p.parts[0].edges[2].overhang = -0.5;
        let v = calc(&p);
        let problems = v["problems"].as_array().unwrap();
        assert_eq!(problems.len(), 2);
        assert_eq!(problems[0]["code"], "pitch_range");
        assert_eq!(problems[0]["item"], "part");
        assert_eq!(problems[0]["edge"], 1);
        assert_eq!(problems[1]["code"], "overhang_negative");
        // The part is not calculated.
        assert_eq!(
            v["parts"][0]["geometry"]["faces"].as_array().unwrap().len(),
            0
        );
    }

    #[test]
    fn a_valid_project_has_no_problems() {
        let p = examples::find("gable").unwrap().project();
        let v = calc(&p);
        assert!(v["problems"].as_array().unwrap().is_empty());
        assert!(v["totals"]["roof_area"].as_f64().unwrap() > 0.0);
    }

    #[test]
    fn part_lines_are_net_of_deductions_like_the_totals() {
        // MATH-004: a chimney on the ridge books a negative ridge line
        // (DACAD's minus line); the part's figures must match the totals.
        let mut p = Project::new("chimney");
        p.parts.push(crate::examples::standard_roof(
            crate::examples::StandardRoof::Gable,
            12.0,
            8.0,
            40.0,
            3.0,
            0.5,
        ));
        let pt = crate::geom::Point2::new;
        p.chimneys.push(crate::model::Chimney {
            corners: vec![pt(5.0, 3.5), pt(6.0, 3.5), pt(6.0, 4.5), pt(5.0, 4.5)],
            height: 8.0,
        });
        let geos = crate::openings::generate_project(&p);
        assert!(
            !geos[0].minus_lines.is_empty(),
            "the case books a minus line"
        );
        let v = calc(&p);
        let part = &v["parts"][0]["quantities"];
        let total = &v["totals"];
        for key in [
            "ridge_length",
            "eave_length",
            "verge_length",
            "abutment_length",
        ] {
            let (a, b) = (part[key].as_f64().unwrap(), total[key].as_f64().unwrap());
            assert!((a - b).abs() < 1e-9, "{key}: part {a}, total {b}");
        }
    }
}
