//! The roof calculation as the desktop app shows it: geometry and
//! quantities per part, totals, DACAD line codes, and the problems that
//! kept a part from being calculated.

use serde_json::{Value, json};

use crate::Project;
use crate::quantities::{dacad_line_totals, manual_quantities, part_quantities};
use crate::roof::RoofQuantities;

/// Roof geometry and quantities for every part, the area of each
/// hand-drawn face (`manual_faces`, already in the totals), plus totals. A part the
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
    // Each hand-drawn face's area, to list beside the parts' (already in
    // the totals; display only).
    let manual_faces: Vec<Value> = project
        .manual_faces
        .iter()
        .map(|f| json!({ "label": f.label, "pitch_deg": f.pitch_deg, "area": f.area() }))
        .collect();
    json!({
        "parts": parts,
        "manual_faces": manual_faces,
        "totals": total,
        "codes": dacad_line_totals(project),
        "problems": project.problems(),
        // Each object's distances from the eave and the sides of its face
        // (editor dimension lines; `edit::position`).
        "placements": crate::edit::position::placements(project),
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
    fn the_hand_drawn_faces_are_listed_and_the_listed_areas_add_up_to_the_total() {
        // UX-009: the Results area list had no row for a hand-drawn face
        // although it counts in the total.
        let mut p = examples::find("gable").unwrap().project();
        let alone = calc(&p)["totals"]["roof_area"].as_f64().unwrap();
        let pt = crate::geom::Point2::new;
        p.manual_faces.push(crate::model::ManualFace {
            points: vec![pt(20.0, 0.0), pt(22.0, 0.0), pt(22.0, 3.0)],
            pitch_deg: 60.0,
            label: "Stříška".into(),
        });
        p.manual_faces.push(crate::model::ManualFace {
            points: vec![pt(30.0, 0.0), pt(34.0, 0.0), pt(34.0, 2.0), pt(30.0, 2.0)],
            pitch_deg: 0.0,
            label: String::new(),
        });
        let v = calc(&p);
        let faces = v["manual_faces"].as_array().unwrap();
        assert_eq!(faces.len(), 2);
        assert_eq!(faces[0]["label"], "Stříška");
        assert_eq!(faces[0]["pitch_deg"], 60.0);
        assert!((faces[0]["area"].as_f64().unwrap() - 6.0).abs() < 1e-9);
        assert!((faces[1]["area"].as_f64().unwrap() - 8.0).abs() < 1e-9);
        let total = v["totals"]["roof_area"].as_f64().unwrap();
        // The total is what it was plus the faces: display data only.
        assert!((total - alone - 14.0).abs() < 1e-9, "{total} vs {alone}");
        let listed: f64 = v["parts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["quantities"]["roof_area"].as_f64().unwrap())
            .chain(faces.iter().map(|f| f["area"].as_f64().unwrap()))
            .sum();
        assert!((listed - total).abs() < 1e-9, "{listed} vs {total}");
        // Without hand-drawn faces the list is empty.
        let none = calc(&examples::find("gable").unwrap().project());
        assert_eq!(none["manual_faces"], json!([]));
    }

    #[test]
    fn a_chimney_or_opening_without_area_is_a_problem() {
        // ERR-002: four corners on one point, as width 0 gives them.
        let mut p = examples::find("gable").unwrap().project();
        let dot = crate::model::Chimney {
            corners: vec![crate::geom::Point2::new(2.0, 2.0); 4],
            height: 6.0,
        };
        p.chimneys.push(dot.clone());
        p.roof_openings.push(dot);
        let codes: Vec<(String, String)> = p
            .problems()
            .iter()
            .map(|x| (format!("{:?}", x.item), x.code.to_string()))
            .collect();
        assert_eq!(
            codes,
            [
                ("Chimney".to_string(), "bad_size".to_string()),
                ("Opening".to_string(), "bad_size".to_string())
            ]
        );
        // A small real one (2 cm x 2 cm) is fine.
        p.roof_openings.clear();
        p.chimneys[0] = crate::edit::probe::chimney_from_rect(2.0, 2.0, 0.02, 0.02, 0.0, 6.0);
        assert!(p.problems().is_empty(), "{:?}", p.problems());
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
