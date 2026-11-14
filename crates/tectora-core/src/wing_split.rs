//! A wing whose roof DACAD cannot close within the house's outline,
//! calculated as a building of its own against the house.
//!
//! A wing (a T's stem, a bay, a cross arm) drawn in one outline with the
//! house shares one roof calculation with it. Where the wing's eaves are
//! much lower than the house's, its roof stays under the house's eave and
//! never meets the house's planes, and DACAD's core stops with an E00002
//! error ("Nicht geschlossene Fläche(n)", "Einzelne Fläche", "Kein
//! Durchstoßpunkt"; measured in DACAD's core in the emulator for 11 such
//! wings of `tests/roof_cases.rs`, 2026-11-14). A roofer then draws the
//! wing as a separate building against the house, and DACAD joins the two
//! roofs (Modellverschnitt): the wing's roof runs to the house wall.
//!
//! Jakub decided (2026-11-14) that Tectora does that itself instead of
//! stopping: where the core stops on a building, the building is cut along
//! the house's wall line at the wing's inner corners, the wing gets a
//! gable against the house, and both are calculated joined. The result is
//! put back together as the building's one roof, with a notice. When no
//! cut makes the roof calculate, DACAD's error stands.

use crate::check::{RoofWarning, WarningKind};
use crate::geom::{Point2, signed_area};
use crate::model::{EdgeDef, Project, RoofPart};
use crate::roof::RoofGeometry;

/// A cut of a building into house and wing.
#[derive(Debug, Clone)]
struct Cut {
    /// Corners of the house and, for each of its walls, the wall of the
    /// building it comes from.
    house: Vec<usize>,
    house_walls: Vec<usize>,
    wing: Vec<usize>,
    /// For each wall of the wing, the wall of the building; `None` for the
    /// new gable against the house.
    wing_walls: Vec<Option<usize>>,
}

const EPS: f64 = 1e-6;
/// How far the cut-off wing reaches into the house, m.
const INTO_HOUSE: f64 = 0.3;
/// How much lower than the house's eave a wing's eaves must be, m.
const LOWER: f64 = 0.01;

fn cross(o: Point2, a: Point2, b: Point2) -> f64 {
    (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
}

/// Whether `p` lies on the line through `a` and `b`.
fn on_line(a: Point2, b: Point2, p: Point2) -> bool {
    let len = a.distance(b);
    len > EPS && (cross(a, b, p) / len).abs() < 1e-6
}

/// The cuts along a house wall line between two inner corners of `part`
/// (counter-clockwise), the smaller side being the wing.
fn cuts(part: &RoofPart) -> Vec<Cut> {
    let pts = part.outline();
    let n = pts.len();
    if n < 6 {
        return Vec::new();
    }
    let reflex: Vec<usize> = (0..n)
        .filter(|&i| cross(pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n]) < -EPS)
        .collect();
    let total = signed_area(&pts);
    let mut out = Vec::new();
    for &i in &reflex {
        for &j in &reflex {
            if i == j {
                continue;
            }
            // The wing runs from corner i to corner j (counter-clockwise),
            // the house from j back to i. The cut j -> i continues the wall
            // ending at i... along the house's wall line: the wall into i
            // and the wall out of j lie on the cut's line.
            let (a, b) = (pts[j], pts[i]);
            let into_i = pts[(i + n - 1) % n];
            let out_j = pts[(j + 1) % n];
            if !on_line(a, b, into_i) || !on_line(a, b, out_j) {
                continue;
            }
            let wing: Vec<usize> = (0..n)
                .map(|k| (i + k) % n)
                .take_while_inclusive(j)
                .collect();
            let house: Vec<usize> = (0..n)
                .map(|k| (j + k) % n)
                .take_while_inclusive(i)
                .collect();
            if wing.len() < 4 || house.len() < 4 {
                continue;
            }
            let wa = signed_area(&wing.iter().map(|&k| pts[k]).collect::<Vec<_>>());
            let ha = signed_area(&house.iter().map(|&k| pts[k]).collect::<Vec<_>>());
            if wa <= EPS || ha <= wa || (wa + ha - total).abs() > 1e-6 * total.max(1.0) {
                continue;
            }
            // Only a wing lower than the house: its eaves under the eave of
            // the house walls on the cut's line. A wing at the house's eave
            // that the core cannot close has another cause, and the house's
            // overhang would lie on its roof.
            let house_eave = part.edges[(i + n - 1) % n]
                .eave_height
                .min(part.edges[j].eave_height);
            let lower = wing[..wing.len() - 1]
                .iter()
                .map(|&k| &part.edges[k])
                .filter(|e| !e.is_gable())
                .all(|e| e.eave_height < house_eave - LOWER);
            if !lower {
                continue;
            }
            // The house must be deeper than the wing reaches into it.
            let hp: Vec<Point2> = house.iter().map(|&k| pts[k]).collect();
            let len = a.distance(b);
            let (nx, ny) = ((b.y - a.y) / len, -(b.x - a.x) / len);
            let probe =
                |p: Point2| Point2::new(p.x + nx * 2.0 * INTO_HOUSE, p.y + ny * 2.0 * INTO_HOUSE);
            let mid = Point2::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
            if !crate::openings::inside(&hp, probe(mid)) {
                continue;
            }
            // Wing walls: i..j-1, then the gable j -> i. House walls:
            // j..i-1, then the cut i -> j on the house's wall line (it takes
            // the definition of the wall into i, which lies on it).
            let wing_walls = wing[..wing.len() - 1]
                .iter()
                .map(|&k| Some(k))
                .chain([None])
                .collect();
            let house_walls = house[..house.len() - 1]
                .iter()
                .copied()
                .chain([(i + n - 1) % n])
                .collect();
            out.push(Cut {
                house,
                house_walls,
                wing,
                wing_walls,
            });
        }
    }
    out
}

trait TakeWhileInclusive: Iterator<Item = usize> + Sized {
    /// Items up to and including `last`.
    fn take_while_inclusive(self, last: usize) -> std::vec::IntoIter<usize> {
        let mut v = Vec::new();
        for k in self {
            v.push(k);
            if k == last {
                break;
            }
        }
        v.into_iter()
    }
}

impl<I: Iterator<Item = usize>> TakeWhileInclusive for I {}

/// The two buildings of a cut, and for each wall of the house the wall of
/// the building it comes from.
fn split(part: &RoofPart, c: &Cut) -> (RoofPart, RoofPart, Vec<usize>) {
    let pts = part.outline();
    let def = |k: usize, at: Point2| EdgeDef {
        start: at,
        ..part.edges[k].clone()
    };
    // House walls, the cut joined with the walls in line with it where
    // they carry the same definition (one wall, as drawn without a wing).
    let mut hv: Vec<(usize, usize)> = c
        .house
        .iter()
        .copied()
        .zip(c.house_walls.iter().copied())
        .collect();
    loop {
        let m = hv.len();
        let joinable = (0..m).find(|&k| {
            let (prev, cur, next) = (hv[(k + m - 1) % m], hv[k], hv[(k + 1) % m]);
            let same = EdgeDef {
                start: pts[0],
                ..part.edges[prev.1].clone()
            } == EdgeDef {
                start: pts[0],
                ..part.edges[cur.1].clone()
            };
            m > 3 && same && on_line(pts[prev.0], pts[next.0], pts[cur.0])
        });
        match joinable {
            Some(k) => {
                hv.remove(k);
            }
            None => break,
        }
    }
    let house = RoofPart {
        name: part.name.clone(),
        edges: hv.iter().map(|&(v, w)| def(w, pts[v])).collect(),
        height: part.height,
    };
    // The wing reaches a little into the house: its roof then ends at the
    // house wall as a wall abutment. Ending exactly on the wall line, the
    // join leaves its top edge a free Pult edge instead (measured: DACAD's
    // Modellverschnitt gives P at 0 and 1 cm, WV at 30 cm into the house,
    // same area).
    let (ci, cj) = (pts[c.wing[0]], pts[c.wing[c.wing.len() - 1]]);
    let len = ci.distance(cj).max(EPS);
    let into = Point2::new(
        (ci.y - cj.y) / len * INTO_HOUSE,
        -(ci.x - cj.x) / len * INTO_HOUSE,
    );
    let first = c.wing[0];
    let last = c.wing[c.wing.len() - 1];
    let at = |v: usize| {
        if v == first || v == last {
            Point2::new(pts[v].x + into.x, pts[v].y + into.y)
        } else {
            pts[v]
        }
    };
    let wing = RoofPart {
        name: part.name.clone(),
        edges: c
            .wing
            .iter()
            .zip(&c.wing_walls)
            .map(|(&v, w)| match w {
                Some(w) => def(*w, at(v)),
                None => {
                    // A gable against the house at the wing's eave.
                    let side = &part.edges[c.wing[c.wing.len() - 2]];
                    EdgeDef {
                        start: at(v),
                        pitch_deg: 90.0,
                        overhang: 0.0,
                        half_hip: None,
                        breaks: Vec::new(),
                        priority: 0,
                        direction_rad: None,
                        clad: false,
                        source: None,
                        ..side.clone()
                    }
                }
            })
            .collect(),
        height: part.height,
    };
    // A gable of the wing starts at the wing's eave: one standing higher
    // (the building height the editor gave every wall) leaves the wing's
    // roof alone without a closing face (the core faults on it).
    let mut wing = wing;
    let low = wing
        .edges
        .iter()
        .filter(|e| !e.is_gable())
        .map(|e| e.eave_height)
        .reduce(f64::min);
    if let Some(low) = low {
        for e in wing.edges.iter_mut().filter(|e| e.is_gable()) {
            e.eave_height = e.eave_height.min(low);
        }
    }
    let house_map = hv.iter().map(|&(_, w)| w).collect();
    (house, wing, house_map)
}

/// The roof of a one-building project the core stops on, with a wing cut
/// off and joined to the house. `None` when no cut calculates.
pub(crate) fn split_roof(project: &Project) -> Option<Vec<RoofGeometry>> {
    if project.parts.len() != 1 || project.flat_roof {
        return None;
    }
    let part = &project.parts[0];
    for c in cuts(part) {
        let (house, wing, house_map) = split(part, &c);
        let mut p = project.clone();
        p.parts = vec![house, wing];
        let Ok(geos) = crate::engine::generate(&p) else {
            continue;
        };
        if geos.len() != 2 || geos.iter().any(|g| g.error.is_some() || g.faces.is_empty()) {
            continue;
        }
        return Some(vec![combine(geos, &c, &house_map)]);
    }
    None
}

/// The roofs of house and wing as the one building's roof, faces and
/// warnings naming the building's walls.
fn combine(geos: Vec<RoofGeometry>, c: &Cut, house_map: &[usize]) -> RoofGeometry {
    let mut it = geos.into_iter();
    let (mut house, mut wing) = (it.next().unwrap_or_default(), it.next().unwrap_or_default());
    let map_house = |e: usize| house_map.get(e).copied().unwrap_or(usize::MAX);
    // Faces of the gable against the house (none; it is vertical) or of a
    // dormer (usize::MAX) keep no wall.
    let map_wing = |e: usize| c.wing_walls.get(e).copied().flatten().unwrap_or(usize::MAX);
    for f in house.faces.iter_mut().filter(|f| f.edge != usize::MAX) {
        f.edge = map_house(f.edge);
    }
    for f in wing.faces.iter_mut().filter(|f| f.edge != usize::MAX) {
        f.edge = map_wing(f.edge);
    }
    for w in house.warnings.iter_mut() {
        w.edges = w.edges.iter().map(|&e| map_house(e)).collect();
    }
    for w in wing.warnings.iter_mut() {
        w.edges = w.edges.iter().map(|&e| map_wing(e)).collect();
    }
    let mut g = house;
    g.faces.append(&mut wing.faces);
    g.lines.append(&mut wing.lines);
    g.gables.append(&mut wing.gables);
    g.clad_area += wing.clad_area;
    g.clad_faces.append(&mut wing.clad_faces);
    g.clad_materials.append(&mut wing.clad_materials);
    g.openings.append(&mut wing.openings);
    g.other_cuts += wing.other_cuts;
    g.cut_lines.append(&mut wing.cut_lines);
    g.minus_lines.append(&mut wing.minus_lines);
    g.footprints.append(&mut wing.footprints);
    g.window_cuts.append(&mut wing.window_cuts);
    g.warnings.append(&mut wing.warnings);
    g.joined_with.clear();
    let mut walls: Vec<usize> = c.wing_walls.iter().flatten().copied().collect();
    walls.sort_unstable();
    g.warnings.push(RoofWarning {
        edges: walls,
        ..RoofWarning::new(WarningKind::WingJoined, 0.0)
    });
    g
}
