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
    /// Plan area of the wing, m².
    area: f64,
    /// Height of the house's roof over the cut's line (the lowest of the
    /// house walls on it), infinite where those walls are gables.
    house_top: f64,
}

const EPS: f64 = 1e-6;
/// How far the cut-off wing reaches into the house, m.
const INTO_HOUSE: f64 = 0.3;
/// How much lower than the house's eave a wing's eaves must be, m.
const LOWER: f64 = 0.01;

/// A corner this far from a wall line lies on it, m: corners of a building
/// turned out of the axes are rounded to millimetres.
const ON_LINE: f64 = 5e-3;
/// Buildings with more walls are not searched for a wing (the search and
/// the calculations it leads to must stay bounded on any input).
pub(crate) const MAX_WALLS: usize = 64;
/// The most cuts calculated for one building.
pub(crate) const MAX_CUTS: usize = 8;

fn cross(o: Point2, a: Point2, b: Point2) -> f64 {
    (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
}

/// Whether `p` lies on the line through `a` and `b`.
fn on_line(a: Point2, b: Point2, p: Point2) -> bool {
    let len = a.distance(b);
    len > EPS && (cross(a, b, p) / len).abs() < ON_LINE
}

/// The cuts along a house wall line between two inner corners of `part`
/// (counter-clockwise), the smaller side being the wing.
fn cuts(part: &RoofPart) -> Vec<Cut> {
    let pts = part.outline();
    let n = pts.len();
    if !(6..=MAX_WALLS).contains(&n) {
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
            let house_top = [(i + n - 1) % n, j]
                .iter()
                .map(|&k| &part.edges[k])
                .filter(|e| !e.is_gable())
                .map(|e| e.eave_height + e.pitch_deg.to_radians().tan() * e.overhang)
                .fold(f64::INFINITY, f64::min);
            out.push(Cut {
                house,
                house_walls,
                wing,
                wing_walls,
                area: wa,
                house_top,
            });
        }
    }
    // The smallest wings first, and a bounded number of them.
    out.sort_by(|a, b| a.area.total_cmp(&b.area));
    out.truncate(MAX_CUTS);
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

/// The roof of the one building of `bare` (no dormers, chimneys, openings
/// or windows; the caller applies those to the result) with a wing cut
/// off and joined to the house, and the number of cuts calculated (at most
/// [`MAX_CUTS`]). `None` when no cut gives a roof that passes
/// [`crate::check::sound`].
pub(crate) fn split_roof(bare: &Project) -> (Result<RoofGeometry, String>, usize) {
    let [part] = &bare.parts[..] else {
        return (Err("not one pitched building".into()), 0);
    };
    if bare.flat_roof {
        return (Err("not one pitched building".into()), 0);
    }
    let pts = part.outline();
    // A wing whose plane lies within a few centimetres of the house's is
    // one plane with it (`coplanar.rs`), never a lean-to: cut off, its
    // roof would lie in the house's overhang.
    let in_plane: Vec<usize> = crate::coplanar::pairs(part)
        .iter()
        .map(|p| p.outer)
        .collect();
    let mut tried = 0;
    let mut why = String::from("no wing to cut off");
    for c in cuts(part) {
        if c.wing_walls.iter().flatten().any(|w| in_plane.contains(w)) {
            why = "the wing's plane lies in the house's".into();
            continue;
        }
        tried += 1;
        let (house, wing, house_map) = split(part, &c);
        let mut p = bare.clone();
        p.parts = vec![house, wing];
        let geos = match crate::engine::generate(&p) {
            Ok(geos) => geos,
            Err(e) => {
                why = e.to_string();
                continue;
            }
        };
        if geos.len() != 2 || geos.iter().any(|g| g.error.is_some() || g.faces.is_empty()) {
            why = "house or wing without roof".into();
            continue;
        }
        // The wing's roof must end under the house's roof at the house
        // wall: one that reaches the wall line above the house's plane is
        // no lean-to, and the part reaching into the house would show in
        // the figures.
        let (Some(&ci), Some(&cj)) = (
            c.wing.first().and_then(|&v| pts.get(v)),
            c.wing.last().and_then(|&v| pts.get(v)),
        ) else {
            continue;
        };
        let len = ci.distance(cj).max(EPS);
        // Positive towards the house.
        let behind =
            |q: &[f64; 3]| ((ci.y - cj.y) * (q[0] - cj.x) - (ci.x - cj.x) * (q[1] - cj.y)) / len;
        let wing_top = geos[1]
            .faces
            .iter()
            .flat_map(|f| f.polygon.iter())
            .filter(|q| behind(q) > -ON_LINE)
            .map(|q| q[2])
            .fold(f64::NEG_INFINITY, f64::max);
        if wing_top > c.house_top + 1e-3 {
            why = format!(
                "the wing reaches the house wall at {wing_top} m, the house's roof is at {} m",
                c.house_top
            );
            continue;
        }
        let geo = combine(geos, &c, &house_map);
        match crate::check::sound(part, &geo, crate::check::Fit::JOINED) {
            Ok(()) => return (Ok(geo), tried),
            Err(e) => why = e,
        }
    }
    (Err(why), tried)
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
    // A warning naming the wing's new gable keeps the walls it can name.
    for w in house.warnings.iter_mut() {
        w.edges = w.edges.iter().map(|&e| map_house(e)).collect();
        w.edges.retain(|&e| e != usize::MAX);
    }
    for w in wing.warnings.iter_mut() {
        w.edges = w.edges.iter().map(|&e| map_wing(e)).collect();
        w.edges.retain(|&e| e != usize::MAX);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DEFAULT_MATERIAL;

    fn edge(x: f64, y: f64, pitch_deg: f64, eave_height: f64) -> EdgeDef {
        EdgeDef {
            start: Point2::new(x, y),
            pitch_deg,
            eave_height,
            overhang: 0.5,
            half_hip: None,
            breaks: Vec::new(),
            priority: 0,
            direction_rad: None,
            clad: false,
            source: None,
            material: DEFAULT_MATERIAL,
        }
    }

    /// A house 5 m deep with `k` low wings 5 m wide in front of it, the
    /// house's front pieces gables: 4k + 4 walls, and no cut closes it.
    fn comb(k: usize) -> Project {
        let mut edges = Vec::new();
        for i in 0..k {
            let x = 2.5 + 10.0 * i as f64;
            edges.push(edge(x, 2.5, 90.0, 5.0));
            edges.push(edge(x, 0.0, 40.0, 2.0));
            edges.push(edge(x + 5.0, 0.0, 90.0, 5.0));
            edges.push(edge(x + 5.0, 2.5, 90.0, 5.0));
        }
        let w = 10.0 * k as f64;
        edges.push(edge(w, 2.5, 90.0, 5.0));
        edges.push(edge(w, 7.5, 40.0, 5.0));
        edges.push(edge(0.0, 7.5, 90.0, 5.0));
        edges.push(edge(0.0, 2.5, 40.0, 5.0));
        let mut p = Project::new("comb");
        p.parts.push(RoofPart::new("A", edges));
        p
    }

    /// The search is bounded on any input (review ERR-001, ARCH-005: a
    /// 244-wall comb cost 7 000 core runs, 89 s): at most `MAX_CUTS` cuts
    /// are calculated, and none for a building over `MAX_WALLS` walls.
    /// Counted, not timed.
    #[test]
    fn the_cuts_calculated_for_one_building_are_bounded() {
        for k in [2, 4, 10, 15] {
            let p = comb(k);
            assert!(p.parts[0].edges.len() <= MAX_WALLS);
            // Every ordered pair of inner corners on the front line is a
            // candidate: far more than are tried.
            let (roof, tried) = split_roof(&p);
            assert!(roof.is_err(), "{k} wings: no cut closes the comb");
            assert!(tried <= MAX_CUTS, "{k} wings: {tried} cuts calculated");
            assert!(tried > 0, "{k} wings");
        }
        for k in [16, 60, 124] {
            let p = comb(k);
            assert!(p.parts[0].edges.len() > MAX_WALLS);
            assert_eq!(split_roof(&p).1, 0, "{k} wings");
        }
    }

    /// The smallest wings come first, so one low wing among many inner
    /// corners is found within the bound.
    #[test]
    fn the_smallest_wing_is_cut_first() {
        let p = comb(4);
        let c = cuts(&p.parts[0]);
        assert_eq!(c.len(), MAX_CUTS);
        assert!(c.windows(2).all(|w| w[0].area <= w[1].area));
        assert!((c[0].area - 12.5).abs() < 1e-9, "{}", c[0].area);
    }
}
