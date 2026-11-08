//! The roof calculation as the desktop app shows it: geometry and
//! quantities per part, totals, DACAD line codes, and the problems that
//! kept a part from being calculated.

use serde_json::{Value, json};

use crate::Project;
use crate::quantities::{dacad_line_totals, manual_quantities, subtract_minus_lines};
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
            let q = geometry.quantities();
            total.add(&q);
            subtract_minus_lines(&mut total, &geometry.minus_lines);
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
}
