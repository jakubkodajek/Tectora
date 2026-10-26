//! Built-in example roofs shown in the "new from example" gallery.

use crate::geom::Point2;
use crate::model::{EdgeDef, HalfHip, Project, RoofPart};

pub struct Example {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    build: fn() -> Project,
}

impl Example {
    pub fn project(&self) -> Project {
        (self.build)()
    }
}

const EAVE_HEIGHT: f64 = 3.0;
const OVERHANG: f64 = 0.5;

/// Builds a project with one CCW part; `pitches[i]` belongs to the edge
/// starting at `points[i]` (90 = gable).
fn single_part(name: &str, points: &[(f64, f64)], pitches: &[f64]) -> Project {
    let edges = points
        .iter()
        .zip(pitches)
        .map(|(&(x, y), &p)| EdgeDef {
            start: Point2::new(x, y),
            pitch_deg: p,
            eave_height: EAVE_HEIGHT,
            overhang: OVERHANG,
            half_hip: None,
            direction_rad: None,
            breaks: Vec::new(),
            priority: 0,
        })
        .collect();
    let mut part = RoofPart {
        name: "Main".into(),
        edges,
    };
    part.normalize_ccw();
    let mut project = Project::new(name);
    project.parts.push(part);
    project
}

const RECT: [(f64, f64); 4] = [(0.0, 0.0), (12.0, 0.0), (12.0, 8.0), (0.0, 8.0)];

pub const EXAMPLES: &[Example] = &[
    Example {
        id: "gable",
        name: "Sedlová střecha",
        description: "Obdélník 12 × 8 m, dva štíty, sklon 40°",
        build: || single_part("Sedlová střecha", &RECT, &[40.0, 90.0, 40.0, 90.0]),
    },
    Example {
        id: "hip",
        name: "Valbová střecha",
        description: "Obdélník 12 × 8 m, čtyři roviny, sklon 35°",
        build: || single_part("Valbová střecha", &RECT, &[35.0; 4]),
    },
    Example {
        id: "half-hip",
        name: "Polovalbová střecha",
        description: "Sedlová s polovalbami na štítech",
        build: || {
            let mut p = single_part("Polovalbová střecha", &RECT, &[40.0, 90.0, 40.0, 90.0]);
            for e in p.parts[0].edges.iter_mut().filter(|e| e.is_gable()) {
                e.half_hip = Some(HalfHip {
                    height: 5.5,
                    pitch_deg: 50.0,
                });
            }
            p
        },
    },
    Example {
        id: "pyramid",
        name: "Stanová střecha",
        description: "Čtverec 9 × 9 m, sklon 30°",
        build: || {
            let sq = [(0.0, 0.0), (9.0, 0.0), (9.0, 9.0), (0.0, 9.0)];
            single_part("Stanová střecha", &sq, &[30.0; 4])
        },
    },
    Example {
        id: "shed",
        name: "Pultová střecha",
        description: "Obdélník 10 × 6 m, jedna rovina, sklon 15°",
        build: || {
            let r = [(0.0, 0.0), (10.0, 0.0), (10.0, 6.0), (0.0, 6.0)];
            single_part("Pultová střecha", &r, &[15.0, 90.0, 90.0, 90.0])
        },
    },
    Example {
        id: "l-shape",
        name: "Valbová do L",
        description: "Půdorys L s úžlabím, sklon 38°",
        build: || {
            let l = [
                (0.0, 0.0),
                (14.0, 0.0),
                (14.0, 7.0),
                (7.0, 7.0),
                (7.0, 14.0),
                (0.0, 14.0),
            ];
            single_part("Valbová do L", &l, &[38.0; 6])
        },
    },
    Example {
        id: "t-shape",
        name: "Sedlová do T",
        description: "Hlavní dům se štíty a příčným křídlem",
        build: || {
            let t = [
                (0.0, 0.0),
                (16.0, 0.0),
                (16.0, 7.0),
                (11.0, 7.0),
                (11.0, 14.0),
                (5.0, 14.0),
                (5.0, 7.0),
                (0.0, 7.0),
            ];
            single_part(
                "Sedlová do T",
                &t,
                &[40.0, 90.0, 40.0, 40.0, 90.0, 40.0, 40.0, 90.0],
            )
        },
    },
    Example {
        id: "u-shape",
        name: "Valbová do U",
        description: "Půdorys U se dvěma úžlabími, sklon 35°",
        build: || {
            let u = [
                (0.0, 0.0),
                (18.0, 0.0),
                (18.0, 12.0),
                (13.0, 12.0),
                (13.0, 5.0),
                (5.0, 5.0),
                (5.0, 12.0),
                (0.0, 12.0),
            ];
            single_part("Valbová do U", &u, &[35.0; 8])
        },
    },
];

/// Basic roof types of DACAD's "Standard-Dach" dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StandardRoof {
    /// Satteldach: eaves on the long sides, gables on the short ones.
    Gable,
    /// Pultdach: one eave, three gable walls.
    Shed,
    /// Walmdach: eaves on all sides.
    Hip,
    /// Zeltdach: hip roof on a square.
    Pyramid,
    /// Krüppelwalmdach: gable roof with clipped gables.
    HalfHip,
}

/// Rectangular `length` × `width` footprint (length along x) with the given
/// roof type, starting at the origin.
pub fn standard_roof(
    kind: StandardRoof,
    length: f64,
    width: f64,
    pitch_deg: f64,
    eave_height: f64,
    overhang: f64,
) -> RoofPart {
    let width = if kind == StandardRoof::Pyramid {
        length
    } else {
        width
    };
    let pts = [(0.0, 0.0), (length, 0.0), (length, width), (0.0, width)];
    let g = crate::model::GABLE_PITCH_DEG.ceil();
    let pitches = match kind {
        StandardRoof::Gable | StandardRoof::HalfHip => [pitch_deg, g, pitch_deg, g],
        StandardRoof::Shed => [pitch_deg, g, g, g],
        StandardRoof::Hip | StandardRoof::Pyramid => [pitch_deg; 4],
    };
    let rise = width / 2.0 * pitch_deg.to_radians().tan();
    let edges = pts
        .iter()
        .zip(pitches)
        .map(|(&(x, y), p)| EdgeDef {
            start: Point2::new(x, y),
            pitch_deg: p,
            eave_height,
            overhang,
            half_hip: (kind == StandardRoof::HalfHip && p >= g).then(|| HalfHip {
                height: ((eave_height + 0.7 * rise) * 100.0).round() / 100.0,
                pitch_deg: (pitch_deg + 15.0).min(70.0),
            }),
            direction_rad: None,
            breaks: Vec::new(),
            priority: 0,
        })
        .collect();
    RoofPart {
        name: "Main".into(),
        edges,
    }
}

pub fn find(id: &str) -> Option<&'static Example> {
    EXAMPLES.iter().find(|e| e.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::roof;

    #[test]
    fn every_example_generates_a_roof() {
        for ex in EXAMPLES {
            let p = ex.project();
            for part in &p.parts {
                let q = roof::generate(part).quantities();
                assert!(q.roof_area > 10.0, "{} has area {}", ex.id, q.roof_area);
            }
        }
    }

    #[test]
    fn standard_roofs_generate() {
        use StandardRoof::*;
        for kind in [Gable, Shed, Hip, Pyramid, HalfHip] {
            let part = standard_roof(kind, 10.0, 8.0, 35.0, 3.0, 0.5);
            assert_eq!(part.edges.len(), 4);
            assert!(
                roof::generate(&part).quantities().roof_area > 50.0,
                "{kind:?}"
            );
        }
    }

    #[test]
    fn ids_are_unique() {
        for (i, a) in EXAMPLES.iter().enumerate() {
            assert!(EXAMPLES[i + 1..].iter().all(|b| b.id != a.id));
        }
    }
}
