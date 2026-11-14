//! Roof windows and chimneys, applied to generated roof faces.
//!
//! Checked against roofs calculated by DACAD itself (cases C24–C28):
//!
//! * A window deducts `width × height` from the face it sits on (both
//!   measured in the roof plane) and adds no lines.
//! * A chimney is a plan polygon. Each face loses the part of it inside the
//!   face, as plan area / cos(pitch). Its sides become wall-abutment lines:
//!   WV on the downhill side and WH uphill for sides level in the roof
//!   plane, WS for all others. Roof lines
//!   (e.g. the ridge) inside the chimney are removed.
//! * A roof opening (DACAD: Öffnung) is cut like a chimney, but per DACAD.HLP
//!   (topic "Öffnung") its sides are roof edges: the uphill side is an eave
//!   of the roof above, the downhill side a Pult of the roof below and the
//!   others verges (OL/OR when square to the eave, otherwise OS). Its full
//!   sloped area is deducted under every norm. Checked against DACAD runs
//!   OP1-OP4 (/mnt/project-files/dacad-round/openings/).

use geo::{Area, BooleanOps, MultiPolygon};

use crate::check::{RoofWarning, WarningKind};
use crate::geom::{Point2, signed_area};
use crate::model::{Chimney, Project, RoofPart, RoofWindow};
use crate::roof::{
    LineKind, Opening, OpeningKind, RoofFace, RoofGeometry, RoofLine, VergeSide, WindowCut,
    generate,
};
use crate::union::to_geo;

const TOL: f64 = 1e-9;
/// A side counts as level in the roof plane (WV/WH) only when it is square
/// to the slope; every oblique side is WS (DACAD cases C32–C36).
const LEVEL: f64 = 1.0 - 1e-6;

/// Generates every part of the project with its dormers, windows and
/// chimneys applied, one geometry per part.
///
/// Parts entered clockwise are generated as their counter-clockwise
/// equivalent (face edge indices still refer to the part as given). Input
/// that [`Project::validate`] rejects is left out instead of producing NaN:
/// an invalid part gets an empty geometry.
pub fn generate_project(project: &Project) -> Vec<RoofGeometry> {
    if project.flat_roof {
        return crate::flat::generate_project(project);
    }
    let (clean, slots, ids) = cleaned(project);
    let mut geos = generate_valid(&clean);
    let warnings = crate::check::check(&clean.parts, &geos);
    for (geo, w) in geos.iter_mut().zip(warnings) {
        geo.warnings.extend(w);
    }
    // Where the calculation stops: the parallel planes that never meet,
    // on the buildings that fail (all of them when the job has one).
    for (i, geo) in geos.iter_mut().enumerate() {
        if let (Some(e), Some(part)) = (geo.error.as_mut(), clean.parts.get(i))
            && (e.parts.is_empty() || e.parts.contains(&i))
        {
            e.edges = crate::check::coplanar_error_walls(part, e.kind);
        }
    }
    // Windows, chimneys and openings the calculation left out: said, the
    // result unchanged.
    for (part, mut w) in crate::check::off_roof(&clean, &geos) {
        let kept = match w.kind {
            WarningKind::WindowOffRoof => &ids.windows,
            WarningKind::ChimneyOffRoof => &ids.chimneys,
            _ => &ids.openings,
        };
        w.item = w.item.map(|i| kept.get(i).copied().unwrap_or(i));
        geos[part].warnings.push(w);
    }
    slots
        .into_iter()
        .map(|slot| match slot {
            None => RoofGeometry::default(),
            Some((i, reversed)) => {
                let mut g = std::mem::take(&mut geos[i]);
                for j in g.joined_with.iter_mut() {
                    *j = ids.parts.get(*j).copied().unwrap_or(*j);
                }
                if let Some(e) = g.error.as_mut() {
                    for j in e.parts.iter_mut() {
                        *j = ids.parts.get(*j).copied().unwrap_or(*j);
                    }
                }
                for d in g.warnings.iter_mut().filter_map(|w| w.dormer.as_mut()) {
                    *d = ids.dormers.get(*d).copied().unwrap_or(*d);
                }
                if let Some(n) = reversed {
                    // Dormer faces (edge usize::MAX) belong to no edge.
                    for f in g.faces.iter_mut().filter(|f| f.edge < n) {
                        f.edge = n - 1 - f.edge;
                    }
                    let flip = |e: &mut usize| {
                        if *e < n {
                            *e = n - 1 - *e;
                        }
                    };
                    g.warnings
                        .iter_mut()
                        .flat_map(|w| w.edges.iter_mut())
                        .for_each(flip);
                    g.error
                        .iter_mut()
                        .flat_map(|e| e.edges.iter_mut())
                        .for_each(flip);
                }
                g
            }
        })
        .collect()
}

/// Index in the cleaned project of each part, and its edge count when it
/// was reversed; `None` for a part that is not valid on its own.
type Slots = Vec<Option<(usize, Option<usize>)>>;

/// For each item kept in the cleaned project, its index in the project.
struct KeptIds {
    parts: Vec<usize>,
    dormers: Vec<usize>,
    chimneys: Vec<usize>,
    openings: Vec<usize>,
    windows: Vec<usize>,
}

/// The project without what cannot be calculated: invalid parts, dormers,
/// chimneys, openings and windows with bad numbers; parts counter-clockwise.
fn cleaned(project: &Project) -> (Project, Slots, KeptIds) {
    let mut clean = project.clone();
    clean.parts.clear();
    // For each part: its index in `clean` and, if reversed, its edge count.
    let mut slots: Slots = Vec::new();
    for part in &project.parts {
        let mut alone = Project::new("");
        alone.parts.push(part.clone());
        if alone.validate().is_err() {
            slots.push(None);
            continue;
        }
        let mut p = part.clone();
        let reversed = signed_area(&p.outline()) < 0.0;
        p.normalize_ccw();
        slots.push(Some((clean.parts.len(), reversed.then_some(p.edges.len()))));
        clean.parts.push(p);
    }
    // Index in `project.dormers` of each dormer kept.
    let dormer_ids: Vec<usize> = (0..clean.dormers.len())
        .filter(|&i| {
            let d = &clean.dormers[i];
            d.end.x.is_finite()
                && d.end.y.is_finite()
                && d.edges.iter().all(|e| e.problems().next().is_none())
        })
        .collect();
    clean.dormers = dormer_ids
        .iter()
        .map(|&i| project.dormers[i].clone())
        .collect();
    // Index in `project.parts` of each part in `clean`.
    let part_ids: Vec<usize> = (0..slots.len()).filter(|&i| slots[i].is_some()).collect();
    let ok_poly = |c: &Chimney| {
        c.corners.len() >= 3 && c.corners.iter().all(|v| v.x.is_finite() && v.y.is_finite())
    };
    let ok_window = |w: &RoofWindow| {
        [w.x, w.y, w.width, w.height].iter().all(|v| v.is_finite())
            && w.width > 0.0
            && w.height > 0.0
    };
    // Index in `project` of each chimney, opening and window kept.
    let kept = |ok: Vec<bool>| -> Vec<usize> { (0..ok.len()).filter(|&i| ok[i]).collect() };
    let chimney_ids = kept(clean.chimneys.iter().map(ok_poly).collect());
    let opening_ids = kept(clean.roof_openings.iter().map(ok_poly).collect());
    let window_ids = kept(clean.windows.iter().map(ok_window).collect());
    clean.chimneys.retain(ok_poly);
    clean.roof_openings.retain(ok_poly);
    clean.windows.retain(ok_window);
    let ids = KeptIds {
        parts: part_ids,
        dormers: dormer_ids,
        chimneys: chimney_ids,
        openings: opening_ids,
        windows: window_ids,
    };
    (clean, slots, ids)
}

/// The wall area of the project (DACAD's `Stena` group): every roof that
/// [`generate_project`] gives has one. `None` for a flat roof and where a
/// building has no roof.
///
/// Where the standard calculation gives the roofs, the area is its wall
/// stage's. Where it stops and a building is calculated another way
/// ([`depart`]), that building's walls are measured under the roof it got
/// ([`wall_area_under`]), the same way the wall stage measures them.
pub fn wall_area(project: &Project) -> Option<f64> {
    if project.flat_roof {
        return None;
    }
    let (clean, _, _) = cleaned(project);
    if let Ok(a) = crate::engine::wall_area(&clean) {
        return Some(a);
    }
    // The buildings one by one: the wall stage measures every building
    // under its own roof, before the roofs are joined.
    let mut total = 0.0;
    for k in 0..clean.parts.len() {
        let one = without_objects(&only_part(&clean, k));
        total += match crate::engine::wall_area(&one) {
            Ok(a) => a,
            Err(crate::engine::Failure::Dacad(e)) => {
                let geo = depart(&one, e.kind)?;
                wall_area_under(&one.parts[0], &geo)?
            }
            Err(crate::engine::Failure::Ours(_)) => return None,
        };
    }
    (!clean.parts.is_empty() && total.is_finite()).then_some(total)
}

/// The area of the walls of `part` (counter-clockwise) from the ground up
/// to its roof `geo`, built the way DACAD's wall stage builds the wall
/// polygons from the core's result (`coresol/walls.rs`, which this
/// follows step by step on the roof's faces instead of the core SOL):
///
/// 1. every roof face against its own wall: the stretches of the wall
///    inside the face's outline, at the face's height;
/// 2. a sloped wall without a face of its own (in line with another wall,
///    or its plane joined with another's) against the faces in its plane;
/// 3. every wall still open (a gable; a face that does not cover its
///    wall) against the other faces in turn, until one covers it whole;
/// 4. stretches at the ground and outlines that repeat another's are
///    dropped; every stretch left stands down to z = 0.
///
/// Equal to the wall stage's area within 1e-3 m² on the catalogue roofs
/// (test `the_measured_wall_area_is_the_wall_stages`).
pub(crate) fn wall_area_under(part: &RoofPart, geo: &RoofGeometry) -> Option<f64> {
    use crate::union::Plane;
    type P3 = [f64; 3];
    /// The wall stage's point tolerance, m.
    const T: f64 = 0.01;
    /// Its tolerance for matching outlines, m.
    const T25: f64 = 0.025;
    let pts = part.outline();
    let n = pts.len();
    let faces: Vec<(usize, Vec<Point2>, Plane)> = geo
        .faces
        .iter()
        .filter(|f| f.edge < n && f.pitch_deg < 89.9 && f.polygon.len() >= 3)
        .map(|f| (f.edge, plan(f), Plane::of(f)))
        .collect();
    let eq2 = |p: Point2, q: Point2| (p.x - q.x).abs() < T && (p.y - q.y).abs() < T;
    let eq3 = |p: P3, q: P3, t: f64| (0..3).all(|k| (p[k] - q[k]).abs() < t);
    // The crossing list of wall `k` against face `j`: the wall's ends
    // (flags 1 and 2) and where the lines of the face's sides cross the
    // wall's line (flag 0), sorted by x, then y.
    let collect = |(a, b): (Point2, Point2), j: usize| -> Vec<(Point2, u8)> {
        let mut out = vec![(a, 1u8), (b, 2u8)];
        let poly = &faces[j].1;
        let m = poly.len();
        for i in 0..m {
            let (c, d) = (poly[i], poly[(i + 1) % m]);
            let den = (d.x - c.x) * (b.y - a.y) - (d.y - c.y) * (b.x - a.x);
            if den.abs() < 1e-9 {
                continue;
            }
            let t = ((a.x - c.x) * (b.y - a.y) - (a.y - c.y) * (b.x - a.x)) / den;
            let o = Point2::new(c.x + (d.x - c.x) * t, c.y + (d.y - c.y) * t);
            let within = o.x >= c.x.min(d.x) - T
                && o.x <= c.x.max(d.x) + T
                && o.y >= c.y.min(d.y) - T
                && o.y <= c.y.max(d.y) + T;
            if within && !out.iter().any(|&(q, f)| f == 0 && eq2(o, q)) {
                out.push((o, 0));
            }
        }
        // The wall stage's two bubble passes (by x, then by y among equal
        // x), which keep the wall's ends before a crossing at one place.
        let m = out.len();
        let mut swapped = true;
        while swapped {
            swapped = false;
            for i in 0..m - 1 {
                if out[i + 1].0.x < out[i].0.x - T {
                    out.swap(i, i + 1);
                    swapped = true;
                }
            }
        }
        swapped = true;
        while swapped {
            swapped = false;
            for i in 0..m - 1 {
                if (out[i + 1].0.x - out[i].0.x).abs() < T && out[i + 1].0.y < out[i].0.y - T {
                    out.swap(i, i + 1);
                    swapped = true;
                }
            }
        }
        out
    };
    let on_or_in =
        |poly: &[Point2], p: Point2| inside(poly, p) || crate::union::near_boundary(poly, p, T);
    // Walks wall `k`'s list against face `j`, adding the stretches inside
    // the face to `segs`; false when a stretch of the wall is not inside.
    let walk = |segs: &mut Vec<(P3, P3)>, j: usize, list: &[(Point2, u8)]| -> bool {
        let (poly, plane) = (&faces[j].1, &faces[j].2);
        let (mut all, mut within, mut prev) = (true, false, Point2::new(0.0, 0.0));
        for &(cur, flag) in list {
            if within && !eq2(cur, prev) {
                let mid = Point2::new((cur.x + prev.x) / 2.0, (cur.y + prev.y) / 2.0);
                if !on_or_in(poly, mid) {
                    all = false;
                } else {
                    let (p, q) = (
                        [prev.x, prev.y, plane.z(prev)],
                        [cur.x, cur.y, plane.z(cur)],
                    );
                    let known = segs.iter().any(|&(a, b)| {
                        (eq3(p, a, T) && eq3(q, b, T)) || (eq3(p, b, T) && eq3(q, a, T))
                    });
                    if !eq3(p, q, T) && !known {
                        segs.push((p, q));
                    }
                }
            }
            if within && eq2(cur, prev) {
                continue;
            }
            if flag != 0 {
                within = !within;
            }
            prev = cur;
        }
        all
    };
    // The wall faces: every roof face with its wall, and every wall
    // without a roof face.
    struct Item {
        wall: usize,
        own: Option<usize>,
        /// The wall's line as the wall stage takes it (see `line_of`).
        line: (Point2, Point2),
        segs: Vec<([f64; 3], [f64; 3])>,
        ok: bool,
    }
    // The core gives a wall with a roof face its line as far as the wall
    // stands: past an inner corner it runs on under its own face until
    // that face ends (a higher wing's side wall standing on the house's
    // roof; the house wall over a lower wing's roof). Here: until the
    // face's outline or the next corner of the building on the line, and
    // no stretch twice.
    let mut claimed: Vec<(Point2, Point2)> = Vec::new();
    let mut line_of = |k: usize, j: usize| -> (Point2, Point2) {
        let (mut a, mut b) = (pts[k], pts[(k + 1) % n]);
        let poly = &faces[j].1;
        // The wall's end, the corner before or after it, the way out.
        for (end, from, beside, neighbour) in [
            (b, a, pts[(k + 2) % n], (k + 1) % n),
            (a, b, pts[(k + n - 1) % n], (k + n - 1) % n),
        ] {
            // Only a wall whose eave is higher than its neighbour's.
            if part.edges[k].eave_height < part.edges[neighbour].eave_height + T {
                continue;
            }
            let len = from.distance(end);
            let (ux, uy) = ((end.x - from.x) / len, (end.y - from.y) / len);
            // An inner corner: the neighbouring wall turns away to the
            // outside (the right of a counter-clockwise outline).
            let turn = ux * (beside.y - end.y) - uy * (beside.x - end.x);
            let inner = if end == b { turn < -1e-9 } else { turn > 1e-9 };
            if !inner {
                continue;
            }
            let along = |p: Point2| {
                (
                    (p.x - end.x) * ux + (p.y - end.y) * uy,
                    (p.x - end.x) * uy - (p.y - end.y) * ux,
                )
            };
            let mut stops: Vec<f64> = pts
                .iter()
                .map(|&p| along(p))
                .filter(|&(s, off)| s > T && off.abs() < T)
                .map(|(s, _)| s)
                .collect();
            let m = poly.len();
            for i in 0..m {
                let (c, d) = (along(poly[i]), along(poly[(i + 1) % m]));
                if (c.1 > 0.0) != (d.1 > 0.0) {
                    let sx = c.0 + (d.0 - c.0) * c.1 / (c.1 - d.1);
                    if sx > T {
                        stops.push(sx);
                    }
                }
            }
            let Some(stop) = stops.into_iter().reduce(f64::min) else {
                continue;
            };
            let at = |t: f64| Point2::new(end.x + ux * t, end.y + uy * t);
            let mid = at(stop / 2.0);
            let free = !claimed
                .iter()
                .any(|&(p, q)| crate::union::near_boundary(&[p, q], mid, T));
            if on_or_in(poly, mid) && free {
                claimed.push((end, at(stop)));
                if end == b {
                    b = at(stop);
                } else {
                    a = at(stop);
                }
            }
        }
        (a, b)
    };
    // The wall's own first plane, where it is sloped.
    let plane_of = |k: usize| -> Option<Plane> {
        let e = &part.edges[k];
        let (a, b) = (pts[k], pts[(k + 1) % n]);
        let len = a.distance(b);
        if e.is_gable() || e.direction_rad.is_some() {
            return None;
        }
        let tan = e.pitch_deg.to_radians().tan();
        let (mx, my) = ((b.y - a.y) / len, -(b.x - a.x) / len);
        Some(Plane {
            a: -mx * tan,
            b: -my * tan,
            c: e.eave_height + tan * (e.overhang + mx * a.x + my * a.y),
        })
    };
    let in_plane = |mine: &Plane, j: usize| {
        let f = &faces[j].2;
        (f.a - mine.a).abs() < T && (f.b - mine.b).abs() < T && (f.c - mine.c).abs() < T
    };
    let mut items: Vec<Item> = Vec::new();
    for k in 0..n {
        if pts[k].distance(pts[(k + 1) % n]) < 1e-9 {
            continue;
        }
        let own: Vec<usize> = (0..faces.len()).filter(|&j| faces[j].0 == k).collect();
        if own.is_empty() {
            // A sloped wall whose face another wall carries stands under
            // that face.
            let carried =
                plane_of(k).and_then(|mine| (0..faces.len()).find(|&j| in_plane(&mine, j)));
            items.push(Item {
                wall: k,
                own: None,
                line: carried.map_or((pts[k], pts[(k + 1) % n]), |j| line_of(k, j)),
                segs: Vec::new(),
                ok: false,
            });
        }
        let line = own.first().map(|&j| line_of(k, j));
        for j in own {
            items.push(Item {
                wall: k,
                own: Some(j),
                line: line.unwrap_or((pts[k], pts[(k + 1) % n])),
                segs: Vec::new(),
                ok: false,
            });
        }
    }
    for it in items.iter_mut() {
        if let Some(j) = it.own {
            let list = collect(it.line, j);
            if list.len() > 2 {
                it.ok = walk(&mut it.segs, j, &list);
            }
        }
    }
    for it in items.iter_mut().filter(|it| it.own.is_none()) {
        let Some(mine) = plane_of(it.wall) else {
            continue;
        };
        for j in 0..faces.len() {
            if !in_plane(&mine, j) {
                continue;
            }
            let list = collect(it.line, j);
            if list.len() != 2 && walk(&mut it.segs, j, &list) {
                it.ok = true;
                break;
            }
        }
    }
    for it in items.iter_mut().filter(|it| !it.ok) {
        for j in 0..faces.len() {
            if it.own == Some(j) {
                continue;
            }
            let list = collect(it.line, j);
            if list.len() != 2 && walk(&mut it.segs, j, &list) {
                break;
            }
        }
    }
    for it in items.iter_mut() {
        it.segs.retain(|(p, q)| !(p[2].abs() < T && q[2].abs() < T));
    }
    for i in 0..items.len() {
        for j in 0..items.len() {
            if i == j || items[i].segs.len() != items[j].segs.len() {
                continue;
            }
            let same = |a: &(P3, P3), b: &(P3, P3)| {
                (eq3(a.0, b.0, T25) && eq3(a.1, b.1, T25))
                    || (eq3(a.1, b.0, T25) && eq3(a.0, b.1, T25))
            };
            if items[i]
                .segs
                .iter()
                .all(|a| items[j].segs.iter().any(|b| same(a, b)))
            {
                items[j].segs.clear();
            }
        }
    }
    let total: f64 = items
        .iter()
        .flat_map(|it| &it.segs)
        .map(|(p, q)| (p[2] + q[2]) / 2.0 * (q[0] - p[0]).hypot(q[1] - p[1]))
        .sum();
    (total.is_finite() && total > 0.0).then_some(total)
}

/// For tests: [`wall_area_under`] for the one building of `project`
/// (counter-clockwise) under the roof the calculation gives it.
#[doc(hidden)]
pub fn measured_wall_area(project: &Project) -> Option<f64> {
    let geos = generate_project(project);
    wall_area_under(project.parts.first()?, geos.first()?)
}

/// Buildings with more walls are not calculated by the sweep after a core
/// error.
const MAX_SWEEP_WALLS: usize = 256;

fn generate_valid(project: &Project) -> Vec<RoofGeometry> {
    // DACAD's own core on the project's RD2 (joined in the
    // Modellverschnitt when there are two or more roofs), always first:
    // where it gives a roof, that roof is the result.
    match crate::engine::generate(project) {
        Ok(mut geos) => {
            // The one exception: a wing's plane a few centimetres above
            // the house's, which DACAD calculates with a step of those few
            // centimetres (Jakub, 2026-11-14; `coplanar.rs`).
            merge_steps(project, &mut geos);
            geos
        }
        // Where the core stops, Tectora calculates the building another
        // way if the result passes its self-check (Jakub, 2026-11-14;
        // `depart`); else DACAD's error is the result.
        Err(crate::engine::Failure::Dacad(e)) => {
            if project.parts.len() == 1 {
                return match depart(project, e.kind) {
                    Some(geo) => vec![geo],
                    None => vec![RoofGeometry {
                        error: Some(e),
                        ..RoofGeometry::default()
                    }],
                };
            }
            several_after_error(project, e)
        }
        // A failure of ours: the sweep, said.
        Err(crate::engine::Failure::Ours(_)) => {
            let mut geos = fallback(project);
            // Not DACAD's result: say so, once for the job.
            if let Some(g) = geos.first_mut() {
                g.warnings
                    .push(RoofWarning::new(WarningKind::NotCalculatedByCore, 0.0));
            }
            geos
        }
    }
}

/// `project` with only its building `k` (and every dormer, chimney,
/// opening and window: those beside the building apply to nothing).
fn only_part(project: &Project, k: usize) -> Project {
    let mut alone = project.clone();
    alone.parts = project.parts.get(k).cloned().into_iter().collect();
    alone
}

/// `project` without dormers, chimneys, openings and windows.
fn without_objects(project: &Project) -> Project {
    let mut bare = project.clone();
    bare.dormers.clear();
    bare.chimneys.clear();
    bare.roof_openings.clear();
    bare.windows.clear();
    bare
}

fn has_objects(project: &Project) -> bool {
    !(project.dormers.is_empty()
        && project.chimneys.is_empty()
        && project.roof_openings.is_empty()
        && project.windows.is_empty())
}

/// Whether building `k` stands clear of every other building of
/// `project`: their eave outlines' bounding boxes do not meet. Only such a
/// building can be calculated on its own; one that touches another is
/// joined with it by the standard calculation.
fn stands_alone(project: &Project, k: usize) -> bool {
    let bbox = |part: &RoofPart| {
        let mut b = [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        for p in crate::check::eave_outline(part).iter().flatten() {
            b = [b[0].min(p.x), b[1].min(p.y), b[2].max(p.x), b[3].max(p.y)];
        }
        b
    };
    let Some(mine) = project.parts.get(k).map(bbox) else {
        return false;
    };
    const GAP: f64 = 0.05;
    project.parts.iter().enumerate().all(|(j, other)| {
        let o = bbox(other);
        j == k
            || mine[2] + GAP < o[0]
            || o[2] + GAP < mine[0]
            || mine[3] + GAP < o[1]
            || o[3] + GAP < mine[1]
    })
}

/// Dormers, chimneys, openings and windows of `alone` (one building)
/// applied to its roof `geo` calculated without them. `None` when nothing
/// is left of the roof.
fn with_objects(alone: &Project, geo: RoofGeometry) -> Option<RoofGeometry> {
    if !has_objects(alone) {
        return Some(geo);
    }
    let mut geos = vec![geo];
    apply_extras(alone, &mut geos);
    geos.pop().filter(|g| !g.faces.is_empty())
}

/// The roof of the one building of `alone`, on which the standard
/// calculation stops with an error of `kind`, calculated another way
/// (Jakub's decision 2026-11-14: where DACAD's core stops, Tectora gives a
/// practical roof and says so). In order: parallel planes within a few
/// centimetres joined into one ([`crate::coplanar`]), a low wing as a
/// building of its own against the house ([`crate::wing_split`]),
/// Tectora's own sweep engine. Each is calculated without dormers,
/// chimneys, openings and windows and must pass [`crate::check::sound`];
/// the first that does is the roof, with its notice (`MergedPlanes`,
/// `WingJoined`, `NotCalculatedByCore`) and the objects applied to it.
///
/// `None`, and DACAD's error stands, when none is sound; when the error
/// belongs to an object (the building without its objects calculates, or
/// a chimney is not convex); and for a flat roof.
fn depart(alone: &Project, kind: crate::roof::CalcErrorKind) -> Option<RoofGeometry> {
    depart_logged(alone, kind, &mut Vec::new())
}

/// For dev tools and tests: why each way of [`depart`] was not taken for
/// the one building of `project` (counter-clockwise, valid), in order,
/// and whether one gave the roof.
#[doc(hidden)]
pub fn departure_log(project: &Project, kind: crate::roof::CalcErrorKind) -> (bool, Vec<String>) {
    let mut log = Vec::new();
    let roof = depart_logged(project, kind, &mut log);
    (roof.is_some(), log)
}

fn depart_logged(
    alone: &Project,
    kind: crate::roof::CalcErrorKind,
    log: &mut Vec<String>,
) -> Option<RoofGeometry> {
    use crate::roof::CalcErrorKind as K;
    let [part] = &alone.parts[..] else {
        return None;
    };
    if alone.flat_roof || kind == K::NonConvexOpening {
        return None;
    }
    let convex = alone
        .chimneys
        .iter()
        .chain(&alone.roof_openings)
        .all(|c| dacad_convex(&c.corners));
    if !convex {
        return None;
    }
    let bare = without_objects(alone);
    let kind = if has_objects(alone) {
        match crate::engine::generate(&bare) {
            Err(crate::engine::Failure::Dacad(e)) => e.kind,
            // The building calculates without its objects: the error is
            // an object's and stands.
            _ => return None,
        }
    } else {
        kind
    };
    match crate::coplanar::merged(&bare) {
        Ok(geo) => return with_objects(alone, geo),
        Err(why) => log.push(format!("planes joined: {why}")),
    }
    if matches!(kind, K::OpenFaces | K::Cut) {
        match crate::wing_split::split_roof(&bare).0 {
            Ok(geo) => return with_objects(alone, geo),
            Err(why) => log.push(format!("wing split: {why}")),
        }
    }
    // The sweep, with its own line clean-up, checked without the objects.
    let sweep = |p: &Project| {
        let mut geos = build_roofs(&bare);
        apply_extras(p, &mut geos);
        geos.pop().filter(|_| geos.is_empty())
    };
    // Bounded like everything here: the sweep of a building at the wall
    // limit costs more than the standard calculation.
    if part.edges.len() > MAX_SWEEP_WALLS {
        log.push("sweep: too many walls".into());
        return None;
    }
    let mut plain = sweep(&bare)?;
    ridges_between_opposite_walls(part, &mut plain);
    if let Err(why) = crate::check::sound(part, &plain, crate::check::Fit::STANDARD) {
        log.push(format!("sweep: {why}"));
        return None;
    }
    let mut geo = if has_objects(alone) {
        let mut g = sweep(alone).filter(|g| !g.faces.is_empty())?;
        ridges_between_opposite_walls(part, &mut g);
        g
    } else {
        plain
    };
    geo.warnings
        .push(RoofWarning::new(WarningKind::NotCalculatedByCore, 0.0));
    Some(geo)
}

/// The sweep calls a line between two faces a hip when it slopes. Where
/// the core stops on a skewed outline, the line where the faces of two
/// opposite walls meet slopes a little and is still the ridge: a hip runs
/// between the faces of two neighbouring walls, down to their corner.
/// Lines bordering faces of the same or neighbouring walls stay as they are.
fn ridges_between_opposite_walls(part: &crate::model::RoofPart, geo: &mut RoofGeometry) {
    const NEAR: f64 = 1e-3;
    let n = part.edges.len();
    let close = |p: [f64; 3], q: [f64; 3]| dist3_(p, q) < NEAR;
    let borders = |f: &RoofFace, a: [f64; 3], b: [f64; 3]| {
        let m = f.polygon.len();
        (0..m).any(|i| {
            let (p, q) = (f.polygon[i], f.polygon[(i + 1) % m]);
            (close(p, a) && close(q, b)) || (close(p, b) && close(q, a))
        })
    };
    for l in geo.lines.iter_mut().filter(|l| l.kind == LineKind::Hip) {
        let walls: Vec<usize> = geo
            .faces
            .iter()
            .filter(|f| f.edge < n && borders(f, l.a, l.b))
            .map(|f| f.edge)
            .collect();
        let &[i, j] = &walls[..] else {
            continue;
        };
        let neighbours = i == j || (i + 1) % n == j || (j + 1) % n == i;
        if !neighbours {
            l.kind = LineKind::Ridge;
        }
    }
}

/// After a calculation that gave roofs: a building that stands alone and
/// has a wing whose plane lies a few centimetres above the house's (which
/// DACAD calculates with a step of those few centimetres) gets the two as
/// one plane, if that roof is sound. Every other roof stays as the
/// standard calculation gives it.
fn merge_steps(project: &Project, geos: &mut [RoofGeometry]) {
    for k in 0..project.parts.len().min(geos.len()) {
        let stepped = crate::coplanar::pairs(&project.parts[k])
            .iter()
            .any(|p| p.rise > 0.0);
        let free = geos[k].error.is_none()
            && geos[k].joined_with.is_empty()
            && (project.parts.len() == 1 || stands_alone(project, k));
        if !stepped || !free || project.flat_roof {
            continue;
        }
        let alone = only_part(project, k);
        let Some(mut geo) = crate::coplanar::merged(&without_objects(&alone))
            .ok()
            .and_then(|geo| with_objects(&alone, geo))
        else {
            continue;
        };
        // What the standard calculation said about dormers it left out.
        geo.warnings.extend(
            geos[k]
                .warnings
                .iter()
                .filter(|w| w.kind == WarningKind::DormerOffRoof)
                .cloned(),
        );
        geos[k] = geo;
    }
}

/// With two or more buildings, after the standard calculation stopped
/// with `e`: DACAD stops the whole job; the user asked that one bad
/// building never takes the others' roofs (2026-11-14). A building the
/// standard calculation also stops on alone is calculated alone through
/// [`depart`] where it stands clear of the others; where that gives no
/// roof it carries the error and is named in `e.parts`. The other
/// buildings are calculated together without those. When no building
/// fails alone (the error comes from the join) or the rest fails as well,
/// every building without a roof of its own gets the error, as in DACAD.
fn several_after_error(project: &Project, mut e: crate::roof::CalcError) -> Vec<RoofGeometry> {
    let n = project.parts.len();
    // Per building: `None` calculates alone; `Some(None)` fails alone;
    // `Some(Some(roof))` fails alone and was calculated another way.
    let alone: Vec<Option<Option<RoofGeometry>>> = (0..n)
        .map(|k| {
            let one = only_part(project, k);
            match crate::engine::generate(&one) {
                Err(crate::engine::Failure::Dacad(ek)) => Some(
                    stands_alone(project, k)
                        .then_some(ek.kind)
                        .and_then(|kind| depart(&one, kind)),
                ),
                _ => None,
            }
        })
        .collect();
    e.parts = (0..n).filter(|&k| matches!(alone[k], Some(None))).collect();
    let keep: Vec<usize> = (0..n).filter(|&k| alone[k].is_none()).collect();
    let failed = |e: &crate::roof::CalcError| RoofGeometry {
        error: Some(e.clone()),
        ..RoofGeometry::default()
    };
    if keep.len() == n {
        // Every building calculates alone: the join fails.
        return vec![failed(&e); n];
    }
    let mut rest = project.clone();
    rest.parts = keep.iter().map(|&k| project.parts[k].clone()).collect();
    let sub: Option<Vec<RoofGeometry>> = if keep.is_empty() {
        Some(Vec::new())
    } else {
        match crate::engine::generate(&rest) {
            Ok(geos) => Some(geos),
            Err(crate::engine::Failure::Ours(_)) => {
                let mut geos = fallback(&rest);
                if let Some(g) = geos.first_mut() {
                    g.warnings
                        .push(RoofWarning::new(WarningKind::NotCalculatedByCore, 0.0));
                }
                Some(geos)
            }
            Err(crate::engine::Failure::Dacad(_)) => None,
        }
        .filter(|geos| geos.len() == keep.len())
    };
    let mut out = vec![failed(&e); n];
    let departed: Vec<usize> = (0..n)
        .filter(|&k| matches!(alone[k], Some(Some(_))))
        .collect();
    for (k, a) in alone.into_iter().enumerate() {
        if let Some(Some(geo)) = a {
            out[k] = geo;
        }
    }
    for (i, mut g) in sub.into_iter().flatten().enumerate() {
        let Some(&k) = keep.get(i) else {
            continue;
        };
        for j in g.joined_with.iter_mut() {
            *j = keep.get(*j).copied().unwrap_or(*j);
        }
        // A dormer on a building calculated alone is on that building's
        // roof, not off the others'.
        g.warnings.retain(|w| {
            let on_departed = w.kind == WarningKind::DormerOffRoof
                && w.dormer
                    .and_then(|d| project.dormers.get(d))
                    .is_some_and(|d| {
                        let at = d.edges.first().map_or(d.end, |e| e.start);
                        let near = (0..n).min_by(|&a, &b| {
                            crate::engine::outline_distance(&project.parts[a], at)
                                .total_cmp(&crate::engine::outline_distance(&project.parts[b], at))
                        });
                        near.is_some_and(|p| departed.contains(&p))
                    });
            !on_departed
        });
        out[k] = g;
    }
    out
}

/// Tectora's own sweep engine, where the core path failed for a reason of
/// ours.
fn fallback(project: &Project) -> Vec<RoofGeometry> {
    // With two or more roofs DACAD joins the roofs it built one by one in
    // its "Modellverschnitt" stage, which replaces everything below.
    if let Some(geos) = crate::multiroof::generate(project) {
        return geos;
    }
    let mut geos = build_roofs(project);
    apply_extras(project, &mut geos);
    geos
}

/// The roof of every part, joined where the parts touch, without dormers,
/// windows and chimneys.
pub(crate) fn build_roofs(project: &Project) -> Vec<RoofGeometry> {
    // Parts with standing gables are joined as several buildings.
    let cuts: Vec<crate::gable_wall::Pieces> = project
        .parts
        .iter()
        .map(|part| {
            let mut c = crate::gable_wall::cut(part);
            if c.cuts.is_empty() {
                c.parts[0] = crate::roof::snap_in_step_planes(part);
            }
            c
        })
        .collect();
    let pieces: Vec<RoofPart> = cuts.iter().flat_map(|c| c.parts.clone()).collect();
    let mut all: Vec<RoofGeometry> = pieces.iter().map(generate).collect();
    crate::union::join(&mut all, &pieces);
    let mut geos = Vec::with_capacity(cuts.len());
    let mut rest = all.into_iter();
    for c in &cuts {
        let mut mine: Vec<RoofGeometry> = rest.by_ref().take(c.parts.len()).collect();
        for &(lo, hi, a, b, top) in &c.cuts {
            let mut higher = std::mem::take(&mut mine[hi]);
            crate::gable_wall::abut(&mut mine[lo], &mut higher, a, b, top);
            mine[hi] = higher;
        }
        let mut geo = RoofGeometry::default();
        for (g, map) in mine.into_iter().zip(&c.edges) {
            geo.faces.extend(g.faces.into_iter().map(|mut f| {
                f.edge = map[f.edge];
                f
            }));
            geo.lines.extend(g.lines);
            geo.gables.extend(g.gables);
            geo.clad_area += g.clad_area;
            geo.clad_materials.extend(g.clad_materials);
            geo.clad_faces.extend(g.clad_faces.into_iter().map(|mut c| {
                c.edge = map.get(c.edge).copied().unwrap_or(usize::MAX);
                c
            }));
            geo.openings.extend(g.openings);
            geo.cut_lines.extend(g.cut_lines);
            geo.minus_lines.extend(g.minus_lines);
        }
        geos.push(geo);
    }
    geos
}

/// The project's dormers, cut into the roofs they sit on.
fn apply_dormers(project: &Project, geos: &mut [RoofGeometry]) {
    for d in &project.dormers {
        let host = d.host_wall.and_then(|[a, b]| {
            project.parts.iter().find_map(|part| {
                let n = part.edges.len();
                (0..n).find_map(|i| {
                    let e = &part.edges[i];
                    let next = part.edges[(i + 1) % n].start;
                    (e.start.distance(a) < 1e-3 && next.distance(b) < 1e-3)
                        .then(|| (e.clone(), next))
                })
            })
        });
        crate::dormer::apply_dormer(geos, d, host.as_ref());
    }
}

/// Dormers, windows and chimneys, then the line clean-up.
fn apply_extras(project: &Project, geos: &mut Vec<RoofGeometry>) {
    apply_dormers(project, geos);
    for w in &project.windows {
        apply_window(geos, w);
    }
    let walls: Vec<(Point2, Point2)> = project
        .parts
        .iter()
        .flat_map(|p| {
            let o = p.outline();
            (0..o.len()).map(move |i| (o[i], o[(i + 1) % o.len()]))
        })
        .collect();
    // A non-convex chimney or opening makes DACAD's core give up
    // (FUN_00da53f0: "Nicht konvexer Schornstein/Öffnung %d wird
    // ignoriert!") and the calculation yields no roof at all, whatever the
    // order (DACAD runs dacad-penetrations P1a–P1c).
    let cutouts: Vec<(&Chimney, bool)> = project
        .chimneys
        .iter()
        .map(|c| (c, false))
        .chain(project.roof_openings.iter().map(|o| (o, true)))
        .collect();
    if cutouts.iter().any(|(c, _)| !dacad_convex(&c.corners)) {
        *geos = vec![RoofGeometry::default(); geos.len()];
        return;
    }
    for geo in geos.iter_mut() {
        for &(c, hole) in &cutouts {
            apply_cutout(geo, c, hole, &walls);
        }
    }
    for geo in geos.iter_mut() {
        // With more than one roof DACAD's C++ model intersection derives
        // the lines again and gives the covered remainder back (TRAING06).
        if project.parts.len() == 1 {
            drop_covered_lines(geo);
        }
        merge_collinear(&mut geo.lines);
        split_verges_at_knicks(&mut geo.lines);
    }
}

/// DACAD's convexity test for a chimney (FUN_00da6920): no two turns of
/// opposite sign between its consecutive sides, in f32 input. Like DACAD
/// it skips the turn back into the first corner.
pub fn dacad_convex(corners: &[Point2]) -> bool {
    let p: Vec<(f64, f64)> = corners
        .iter()
        .map(|c| (c.x as f32 as f64, c.y as f32 as f64))
        .collect();
    let n = p.len();
    let (mut left, mut right) = (0, 0);
    for i in 0..n.saturating_sub(1) {
        let (a, b, c) = (p[i], p[i + 1], p[(i + 2) % n]);
        let turn = (c.1 - b.1) * (b.0 - a.0) - (c.0 - b.0) * (b.1 - a.1);
        if turn > 0.0 {
            left += 1;
        } else if turn < 0.0 {
            right += 1;
        }
    }
    left == 0 || right == 0
}

/// A piece of a ridge, hip, valley or knick with one of its own planes on
/// top on both sides is no roof line: there that plane passes over the other, and
/// DACAD's merge pass (FUN_00da4ec0) makes the lower face's edge code 17
/// (DACAD variants of mass case R1_0368: a bay's ridge under the higher
/// roof's overhang). Under a dormer face the host's lines stay (R1_0398
/// variants).
fn drop_covered_lines(geo: &mut RoofGeometry) {
    use crate::union::Plane;
    let faces: Vec<(Vec<Point2>, Plane, bool)> = geo
        .faces
        .iter()
        .filter(|f| signed_area(&plan(f)).abs() > 1e-6)
        .map(|f| (plan(f), Plane::of(f), f.edge == usize::MAX))
        .collect();
    let top = |p: Point2| {
        faces
            .iter()
            .filter(|(poly, _, _)| inside(poly, p))
            .max_by(|a, b| a.1.z(p).total_cmp(&b.1.z(p)))
            .filter(|f| !f.2)
            .map(|f| (&f.1, f as *const _))
    };
    // DACAD's merge keeps the shorter edge, of the plane on top, only where
    // the lower face's edge touches an abutment.
    let abutments: Vec<[f64; 3]> = geo
        .lines
        .iter()
        .filter(|l| {
            matches!(
                l.kind,
                LineKind::AbutmentFront | LineKind::AbutmentBack | LineKind::AbutmentSide
            )
        })
        .flat_map(|l| [l.a, l.b])
        .collect();
    let mut out = Vec::with_capacity(geo.lines.len());
    for l in std::mem::take(&mut geo.lines) {
        let (a, b) = (Point2::new(l.a[0], l.a[1]), Point2::new(l.b[0], l.b[1]));
        let len = a.distance(b);
        let touches = abutments
            .iter()
            .any(|&p| dist3_(p, l.a) < 0.02 || dist3_(p, l.b) < 0.02);
        if !matches!(
            l.kind,
            LineKind::Ridge | LineKind::Hip | LineKind::Valley | LineKind::Knick
        ) || len < 1e-6
            || !touches
        {
            out.push(l);
            continue;
        }
        // Pieces between the face corners on the line.
        let (ux, uy) = ((b.x - a.x) / len, (b.y - a.y) / len);
        let mut ts = vec![0.0, 1.0];
        for (poly, _, _) in &faces {
            for q in poly {
                let t = ((q.x - a.x) * ux + (q.y - a.y) * uy) / len;
                let off = ((q.x - a.x) * -uy + (q.y - a.y) * ux).abs();
                if off < 1e-6 && t > 1e-9 && t < 1.0 - 1e-9 {
                    ts.push(t);
                }
            }
        }
        ts.sort_by(f64::total_cmp);
        ts.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
        let mut keep: Vec<(f64, f64)> = Vec::new();
        for w in ts.windows(2) {
            let t = (w[0] + w[1]) / 2.0;
            let m = Point2::new(a.x + t * len * ux, a.y + t * len * uy);
            const D: f64 = 1e-4;
            let (pl, pr) = (
                Point2::new(m.x - D * uy, m.y + D * ux),
                Point2::new(m.x + D * uy, m.y - D * ux),
            );
            let (left, right) = (top(pl), top(pr));
            // ... over a face below it.
            #[allow(clippy::type_complexity)]
            let under = |p: Point2, over: Option<(&Plane, *const (Vec<Point2>, Plane, bool))>| {
                over.is_some_and(|(o, _)| {
                    faces
                        .iter()
                        .any(|(poly, q, _)| inside(poly, p) && !q.same(o) && q.z(p) < o.z(p) - 1e-9)
                })
            };
            let passes = under(pl, left) || under(pr, right);
            // The plane on top must be one of the line's own planes.
            let z = lerp3(l.a, l.b, t)[2];
            // Two faces of that plane: one face around the line is a flaw
            // of ours, not a plane passing over.
            let covered = matches!((left, right),
                (Some((p, f)), Some((q, g))) if f != g && p.same(q) && (p.z(m) - z).abs() < 1e-4)
                && passes;
            if covered {
                continue;
            }
            match keep.last_mut() {
                Some(k) if (k.1 - w[0]).abs() < 1e-9 => k.1 = w[1],
                _ => keep.push((w[0], w[1])),
            }
        }
        for (s, e) in keep {
            out.push(RoofLine {
                a: lerp3(l.a, l.b, s),
                b: lerp3(l.a, l.b, e),
                ..l.clone()
            });
        }
    }
    geo.lines = out;
}

/// Joins lines of the same kind that continue each other, as DACAD lists
/// them: cutting roofs against each other leaves them in pieces.
pub(crate) fn merge_collinear(lines: &mut Vec<RoofLine>) {
    let dir = |l: &RoofLine| {
        let d = [l.b[0] - l.a[0], l.b[1] - l.a[1], l.b[2] - l.a[2]];
        let n = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-12);
        [d[0] / n, d[1] / n, d[2] / n]
    };
    let near = |p: [f64; 3], q: [f64; 3]| {
        (p[0] - q[0]).abs() < 1e-6 && (p[1] - q[1]).abs() < 1e-6 && (p[2] - q[2]).abs() < 1e-6
    };
    lines.retain(|l| l.length() > 1e-6);
    // Two walls' eaves stay apart where another line starts between them,
    // even on one straight line (mass case R1_0229).
    let ends: Vec<[f64; 3]> = lines
        .iter()
        .filter(|l| l.kind != LineKind::Eave)
        .flat_map(|l| [l.a, l.b])
        .collect();
    let split =
        |l: &RoofLine, p: [f64; 3]| l.kind == LineKind::Eave && ends.iter().any(|&e| near(e, p));
    loop {
        let mut joined = false;
        'outer: for i in 0..lines.len() {
            for j in 0..lines.len() {
                if i == j || lines[i].dacad_code() != lines[j].dacad_code() {
                    continue;
                }
                let (di, dj) = (dir(&lines[i]), dir(&lines[j]));
                let cos = di[0] * dj[0] + di[1] * dj[1] + di[2] * dj[2];
                if cos > 1.0 - 1e-9 && near(lines[i].b, lines[j].a) {
                    if split(&lines[i], lines[i].b) {
                        continue;
                    }
                    lines[i].b = lines[j].b;
                } else if cos < -1.0 + 1e-9 && near(lines[i].b, lines[j].b) {
                    if split(&lines[i], lines[i].b) {
                        continue;
                    }
                    lines[i].b = lines[j].a;
                } else {
                    continue;
                }
                lines.swap_remove(j);
                joined = true;
                break 'outer;
            }
        }
        if !joined {
            break;
        }
    }
}

/// DACAD ends a verge where a pitch break (Knick) meets it, even when both
/// definitions have the same pitch and the verge runs on straight (mass
/// cases R1_0166, R1_0197).
fn split_verges_at_knicks(lines: &mut Vec<RoofLine>) {
    let ends: Vec<[f64; 3]> = lines
        .iter()
        .filter(|l| l.kind == LineKind::Knick)
        .flat_map(|l| [l.a, l.b])
        .collect();
    let mut out = Vec::with_capacity(lines.len());
    for l in lines.drain(..) {
        if l.kind != LineKind::Verge {
            out.push(l);
            continue;
        }
        let len = l.length();
        let mut ts: Vec<f64> = ends
            .iter()
            .filter_map(|&p| {
                let t = (0..3)
                    .map(|k| (p[k] - l.a[k]) * (l.b[k] - l.a[k]))
                    .sum::<f64>()
                    / (len * len);
                let foot = lerp3(l.a, l.b, t);
                let d = (0..3).map(|k| (p[k] - foot[k]).powi(2)).sum::<f64>().sqrt();
                (t * len > 1e-6 && (1.0 - t) * len > 1e-6 && d < 1e-6).then_some(t)
            })
            .collect();
        ts.sort_by(f64::total_cmp);
        ts.dedup_by(|x, y| (*x - *y).abs() * len < 1e-6);
        let mut t0 = 0.0;
        for t in ts.into_iter().chain(std::iter::once(1.0)) {
            out.push(RoofLine {
                a: lerp3(l.a, l.b, t0),
                b: lerp3(l.a, l.b, t),
                ..l.clone()
            });
            t0 = t;
        }
    }
    *lines = out;
}

/// Plan outline of a face.
pub(crate) fn plan(face: &RoofFace) -> Vec<Point2> {
    face.polygon
        .iter()
        .map(|p| Point2::new(p[0], p[1]))
        .collect()
}

/// Unit uphill direction in plan and `tan(pitch)` of a face's plane.
pub(crate) fn slope(face: &RoofFace) -> (Point2, f64) {
    let (mut nx, mut ny, mut nz) = (0.0, 0.0, 0.0);
    let poly = &face.polygon;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        nx += (a[1] - b[1]) * (a[2] + b[2]);
        ny += (a[2] - b[2]) * (a[0] + b[0]);
        nz += (a[0] - b[0]) * (a[1] + b[1]);
    }
    if nz < 0.0 {
        (nx, ny, nz) = (-nx, -ny, -nz);
    }
    let h = nx.hypot(ny);
    if h < TOL || nz < TOL {
        return (Point2::new(0.0, 0.0), 0.0);
    }
    (Point2::new(-nx / h, -ny / h), h / nz)
}

fn dist3_(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

pub(crate) fn inside(poly: &[Point2], p: Point2) -> bool {
    let mut c = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a.y > p.y) != (b.y > p.y) {
            let x = a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y);
            if p.x < x {
                c = !c;
            }
        }
    }
    c
}

/// The window's corner lies on the face's boundary at the eave; test a
/// point just inside it, up the slope and along the eave.
pub(crate) fn apply_window(geos: &mut [RoofGeometry], w: &RoofWindow) {
    if let Some((g, k)) = window_face(geos, w) {
        let geo = &mut geos[g];
        let d = w.width * w.height;
        geo.faces[k].area -= d;
        geo.faces[k].deducted += d;
        geo.openings.push(Opening {
            kind: OpeningKind::Window,
            area: d,
        });
        geo.window_cuts.push(WindowCut {
            face: k,
            width: w.width,
            height: w.height,
        });
    }
}

/// A roof window's outline in plan: `width` level along the face it sits
/// on and `height` up its slope (shortened by the pitch in plan), from the
/// lower left corner `(x, y)`. As the editor draws it; a window on no face
/// is drawn as if on a level roof.
pub fn window_outline(geos: &[RoofGeometry], w: &RoofWindow) -> Vec<Point2> {
    let (up, tan) = window_face(geos, w)
        .and_then(|(g, k)| geos.get(g)?.faces.get(k))
        .map(slope)
        .filter(|(up, _)| up.x.hypot(up.y) > 0.5)
        .unwrap_or((Point2::new(0.0, 1.0), 0.0));
    let along = Point2::new(up.y, -up.x);
    let h = w.height / (1.0 + tan * tan).sqrt();
    let at =
        |s: f64, t: f64| Point2::new(w.x + along.x * s + up.x * t, w.y + along.y * s + up.y * t);
    vec![at(0.0, 0.0), at(w.width, 0.0), at(w.width, h), at(0.0, h)]
}

/// The face a window sits on: (geometry, face) of the first face that
/// holds a point just inside the window's corner or, where the corner lies
/// exactly on a ridge, hip or valley so that no face does, of the first
/// face whose outline holds the corner itself.
///
/// DACAD keeps each window under the wall whose face it was placed on and
/// always takes its full width × height off that face, also where the
/// window lies across a line or on another face in plan (DACAD runs P2b and
/// a window record moved onto the opposite face, 2026-11-10). So a window
/// on the roof is never left out just because its corner is on a line.
pub(crate) fn window_face(geos: &[RoofGeometry], w: &RoofWindow) -> Option<(usize, usize)> {
    for (g, geo) in geos.iter().enumerate() {
        let hit = geo.faces.iter().position(|face| {
            let (up, _) = slope(face);
            let along = Point2::new(up.y, -up.x);
            let probe = |s: f64| {
                Point2::new(
                    w.x + 1e-4 * up.x + s * 1e-4 * along.x,
                    w.y + 1e-4 * up.y + s * 1e-4 * along.y,
                )
            };
            let poly = plan(face);
            inside(&poly, probe(1.0)) || inside(&poly, probe(-1.0))
        });
        if let Some(k) = hit {
            return Some((g, k));
        }
    }
    let corner = Point2::new(w.x, w.y);
    geos.iter().enumerate().find_map(|(g, geo)| {
        geo.faces
            .iter()
            .position(|face| on_or_inside(&plan(face), corner, 1e-4))
            .map(|k| (g, k))
    })
}

/// Whether `p` lies inside the polygon or within `eps` of its outline.
fn on_or_inside(poly: &[Point2], p: Point2, eps: f64) -> bool {
    if inside(poly, p) {
        return true;
    }
    let n = poly.len();
    (0..n).any(|i| {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let len2 = dx * dx + dy * dy;
        let t = if len2 > 0.0 {
            (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        (p.x - a.x - t * dx).hypot(p.y - a.y - t * dy) <= eps
    })
}

/// Sutherland–Hodgman: `subject` clipped by the convex CCW polygon `clip`.
pub(crate) fn clip_polygon(subject: &[Point2], clip: &[Point2]) -> Vec<Point2> {
    let mut out = subject.to_vec();
    let n = clip.len();
    for i in 0..n {
        let (a, b) = (clip[i], clip[(i + 1) % n]);
        let side = |p: Point2| (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
        let input = std::mem::take(&mut out);
        let m = input.len();
        for k in 0..m {
            let (p, q) = (input[k], input[(k + 1) % m]);
            let (sp, sq) = (side(p), side(q));
            if sp >= -TOL {
                out.push(p);
            }
            if (sp >= -TOL) != (sq >= -TOL) {
                let t = sp / (sp - sq);
                out.push(Point2::new(p.x + t * (q.x - p.x), p.y + t * (q.y - p.y)));
            }
        }
        if out.is_empty() {
            break;
        }
    }
    out
}

/// Parameter intervals of segment `a`–`b` that lie inside `poly`.
pub(crate) fn inside_spans(poly: &[Point2], a: Point2, b: Point2) -> Vec<(f64, f64)> {
    let d = Point2::new(b.x - a.x, b.y - a.y);
    let mut ts = vec![0.0, 1.0];
    let n = poly.len();
    for i in 0..n {
        let (p, q) = (poly[i], poly[(i + 1) % n]);
        let e = Point2::new(q.x - p.x, q.y - p.y);
        let den = d.x * e.y - d.y * e.x;
        if den.abs() < TOL {
            continue;
        }
        let w = Point2::new(p.x - a.x, p.y - a.y);
        let t = (w.x * e.y - w.y * e.x) / den;
        let u = (w.x * d.y - w.y * d.x) / den;
        if (-TOL..=1.0 + TOL).contains(&u) && t > 0.0 && t < 1.0 {
            ts.push(t);
        }
    }
    ts.sort_by(f64::total_cmp);
    ts.windows(2)
        .filter(|s| s[1] - s[0] > 1e-9)
        .filter(|s| {
            let m = (s[0] + s[1]) / 2.0;
            inside(poly, Point2::new(a.x + m * d.x, a.y + m * d.y))
        })
        .map(|s| (s[0], s[1]))
        .collect()
}

pub(crate) fn lerp3(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    [
        a[0] + t * (b[0] - a[0]),
        a[1] + t * (b[1] - a[1]),
        a[2] + t * (b[2] - a[2]),
    ]
}

/// Height of a face's plane at a plan point.
fn height_at(face: &RoofFace, up: Point2, tan: f64, p: Point2) -> f64 {
    let o = face.polygon[0];
    o[2] + tan * ((p.x - o[0]) * up.x + (p.y - o[1]) * up.y)
}

/// Where two plan lines meet.
fn cross_lines(a: Point2, b: Point2, c: Point2, d: Point2) -> Option<Point2> {
    let (u, v) = (
        Point2::new(b.x - a.x, b.y - a.y),
        Point2::new(d.x - c.x, d.y - c.y),
    );
    let den = u.x * v.y - u.y * v.x;
    if den.abs() < 1e-12 {
        return None;
    }
    let t = ((c.x - a.x) * v.y - (c.y - a.y) * v.x) / den;
    Some(Point2::new(a.x + t * u.x, a.y + t * u.y))
}

/// DACAD extends each chimney side across its face from where it crosses
/// the eave. Where that line passes less than POINT_EPS (2 cm) beyond the
/// end of the face's ridge, the side is turned to run through the ridge's
/// end, pivoting on the eave crossing, and its two corners slide along the
/// neighbouring sides. Crossings on the ridge itself stay (sweep
/// /mnt/project-files/dacad-chimney-ridge-end/: 19.9 mm snapped, 20.4 mm
/// not; mass case R1_0231).
fn snap_sides_to_ridge_ends(geo: &RoofGeometry, corners: &mut [Point2]) {
    const POINT_EPS: f64 = 0.02;
    let n = corners.len();
    let mid = Point2::new(
        corners.iter().map(|p| p.x).sum::<f64>() / n as f64,
        corners.iter().map(|p| p.y).sum::<f64>() / n as f64,
    );
    let Some(face) = geo.faces.iter().find(|f| inside(&plan(f), mid)) else {
        return;
    };
    let on_face = |l: &RoofLine| {
        let has = |p: [f64; 3]| {
            face.polygon
                .iter()
                .any(|q| (0..3).all(|i| (p[i] - q[i]).abs() < 1e-6))
        };
        has(l.a) && has(l.b)
    };
    let flat = |p: [f64; 3]| Point2::new(p[0], p[1]);
    let lines_of = |k: LineKind| -> Vec<(Point2, Point2)> {
        geo.lines
            .iter()
            .filter(|l| l.kind == k && on_face(l))
            .map(|l| (flat(l.a), flat(l.b)))
            .collect()
    };
    let (eaves, ridges) = (lines_of(LineKind::Eave), lines_of(LineKind::Ridge));
    for i in 0..n {
        let (p, q) = (corners[i], corners[(i + 1) % n]);
        let Some(e) = eaves.iter().find_map(|&(a, b)| {
            let x = cross_lines(p, q, a, b)?;
            let len = a.distance(b);
            let t = ((x.x - a.x) * (b.x - a.x) + (x.y - a.y) * (b.y - a.y)) / len;
            (-TOL..=len + TOL).contains(&t).then_some(x)
        }) else {
            continue;
        };
        let end = ridges.iter().find_map(|&(a, b)| {
            let x = cross_lines(p, q, a, b)?;
            let len = a.distance(b);
            let t = ((x.x - a.x) * (b.x - a.x) + (x.y - a.y) * (b.y - a.y)) / len;
            if t < 0.0 && -t < POINT_EPS {
                Some(a)
            } else if t > len && t - len < POINT_EPS {
                Some(b)
            } else {
                None
            }
        });
        let Some(r) = end else {
            continue;
        };
        let (pp, qq) = (corners[(i + n - 1) % n], corners[(i + 2) % n]);
        if let (Some(a), Some(b)) = (cross_lines(e, r, pp, p), cross_lines(e, r, q, qq)) {
            corners[i] = a;
            corners[(i + 1) % n] = b;
        }
    }
}

/// Cuts a chimney, or with `hole` a roof opening, out of the faces.
fn apply_cutout(geo: &mut RoofGeometry, c: &Chimney, hole: bool, walls: &[(Point2, Point2)]) {
    let mut corners = c.corners.clone();
    if corners.len() < 3 {
        return;
    }
    if signed_area(&corners) < 0.0 {
        corners.reverse();
    }
    if !hole {
        snap_sides_to_ridge_ends(geo, &mut corners);
        cut_chimney(geo, &corners, walls);
        return;
    }
    // Roof lines under the opening disappear.
    let mut kept: Vec<RoofLine> = Vec::new();
    for l in std::mem::take(&mut geo.lines) {
        let (a, b) = (Point2::new(l.a[0], l.a[1]), Point2::new(l.b[0], l.b[1]));
        let under = inside_spans(&corners, a, b);
        for &(s, e) in &under {
            geo.cut_lines
                .push((l.dacad_code().to_string(), (e - s) * l.length()));
        }
        let mut t0 = 0.0;
        for (s, e) in under.iter().copied().chain(std::iter::once((1.0, 1.0))) {
            if s - t0 > 1e-9 {
                kept.push(RoofLine {
                    a: lerp3(l.a, l.b, t0),
                    b: lerp3(l.a, l.b, s),
                    ..l.clone()
                });
            }
            t0 = e;
        }
    }
    geo.lines = kept;
    let outline = corners.clone();
    let mut total = 0.0;
    let n = outline.len();
    for face in &mut geo.faces {
        let poly = plan(face);
        let (up, tan) = slope(face);
        let cut = signed_area(&clip_polygon(&poly, &corners)).abs();
        if cut < 1e-9 {
            continue;
        }
        let d = cut * (1.0 + tan * tan).sqrt();
        face.area -= d;
        face.deducted += d;
        total += d;
        for i in 0..n {
            let (a, b) = (outline[i], outline[(i + 1) % n]);
            let dir = Point2::new(b.x - a.x, b.y - a.y);
            let len = dir.x.hypot(dir.y);
            if len < TOL {
                continue;
            }
            // Outward normal of a CCW polygon side.
            let out = (dir.y / len) * up.x - (dir.x / len) * up.y;
            let (kind, side) = if hole {
                // Outward normal across the slope, + to the right looking uphill.
                let across = (dir.y / len) * up.y + (dir.x / len) * up.x;
                if out < -LEVEL {
                    (LineKind::Pult, None)
                } else if out > LEVEL {
                    (LineKind::Eave, None)
                } else if across > LEVEL {
                    // The roof right of the hole ends here on its left.
                    (LineKind::Verge, Some(VergeSide::Left))
                } else if across < -LEVEL {
                    (LineKind::Verge, Some(VergeSide::Right))
                } else {
                    (LineKind::Verge, Some(VergeSide::Oblique))
                }
            } else if out < -LEVEL {
                (LineKind::AbutmentFront, None)
            } else if out > LEVEL {
                (LineKind::AbutmentBack, None)
            } else {
                (LineKind::AbutmentSide, None)
            };
            for (s, e) in inside_spans(&poly, a, b) {
                let p = Point2::new(a.x + s * dir.x, a.y + s * dir.y);
                let q = Point2::new(a.x + e * dir.x, a.y + e * dir.y);
                geo.lines.push(RoofLine {
                    kind,
                    a: [p.x, p.y, height_at(face, up, tan, p)],
                    b: [q.x, q.y, height_at(face, up, tan, q)],
                    side,
                });
            }
        }
    }
    if total > 0.0 {
        geo.openings.push(Opening {
            kind: if hole {
                OpeningKind::Hole
            } else {
                OpeningKind::Chimney
            },
            area: total,
        });
    }
}

/// One side of a chimney hole in one face.
struct HoleEdge {
    c: [f64; 3],
    d: [f64; 3],
    face: usize,
    shared: bool,
    /// The same edge in the neighbouring face's hole, when dropped.
    twin: Option<([f64; 3], [f64; 3])>,
}

/// A chimney as DACAD's core cuts it (decompiled FUN_00da5160 and its
/// callers): every face gets its own hole, the chimney clipped by the face,
/// with corners closer than POINT_EPS in every coordinate (or VALUE_EPS in
/// plan) merged; a hole left with fewer than three corners is dropped
/// (DACAD sweep dacad-chimney-across-line, V45_02 against H45_02). Roof
/// lines stay whole. A hole edge lying on a roof line (see [`on_line`])
/// becomes a negative line of that line's kind; a hole edge of only one
/// face is also an abutment, unless the line is a verge, Pult or eave.
fn cut_chimney(geo: &mut RoofGeometry, corners: &[Point2], walls: &[(Point2, Point2)]) {
    let mut corners = corners.to_vec();
    for _ in 0..corners.len() {
        if !pivot_corner(geo, &mut corners) {
            break;
        }
    }
    let mut edges: Vec<HoleEdge> = Vec::new();
    let mut total = 0.0;
    for fi in 0..geo.faces.len() {
        let Some((pts, _)) = face_hole(&geo.faces[fi], &corners) else {
            continue;
        };
        // The roof under a dormer is already gone (DACAD variants of mass
        // case R1_0398).
        let face = &mut geo.faces[fi];
        let (_, tan) = slope(face);
        let mut under = MultiPolygon::new(vec![to_geo(&clip_polygon(&plan(face), &corners))]);
        // Dormer faces have no footprint edge.
        for f in geo.footprints.iter().filter(|_| face.edge != usize::MAX) {
            under = under.difference(&MultiPolygon::new(vec![to_geo(f)]));
        }
        let cut = under.unsigned_area();
        let d = cut * (1.0 + tan * tan).sqrt();
        face.area -= d;
        face.deducted += d;
        total += d;
        push_edges(&mut edges, &pts, fi);
    }
    join_coplanar_edges(&geo.faces, &mut edges);
    // An edge two holes share lies on the line between their faces; the
    // hole of the lower face keeps it.
    let on_edge = |p: [f64; 3], e: &HoleEdge| {
        let (a, b) = (Point2::new(e.c[0], e.c[1]), Point2::new(e.d[0], e.d[1]));
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let l2 = dx * dx + dy * dy;
        if l2 < TOL {
            return false;
        }
        let t = ((p[0] - a.x) * dx + (p[1] - a.y) * dy) / l2;
        (-1e-9..=1.0 + 1e-9).contains(&t) && dist3_(p, lerp3(e.c, e.d, t)) < 1e-4
    };
    let mut drop = vec![false; edges.len()];
    for i in 0..edges.len() {
        let mid = lerp3(edges[i].c, edges[i].d, 0.5);
        if let Some(j) =
            (0..edges.len()).find(|&j| edges[j].face != edges[i].face && on_edge(mid, &edges[j]))
        {
            // A shorter edge lying on a longer one is DACAD's code 17
            // (FUN_00da5060; R1_0745).
            let (li, lj) = (
                dist3_(edges[i].c, edges[i].d),
                dist3_(edges[j].c, edges[j].d),
            );
            drop[i] = li < lj - 1e-6 || (li <= lj + 1e-6 && edges[i].face > edges[j].face);
            edges[i].shared = true;
            if drop[i] && li > lj - 1e-6 {
                edges[j].twin = Some((edges[i].c, edges[i].d));
            }
        }
    }
    // DACAD also holes the dormer's footprint face on the host plane
    // (see `dormer`): a dormer face's hole edge along the footprint's
    // outline is shared with that hole (mass case R1_0440: −K, no WS).
    for e in edges.iter_mut() {
        if geo.faces[e.face].edge != usize::MAX {
            continue;
        }
        let (c, d) = (Point2::new(e.c[0], e.c[1]), Point2::new(e.d[0], e.d[1]));
        if geo.footprints.iter().any(|f| {
            (0..f.len()).any(|i| {
                let (a, b) = (f[i], f[(i + 1) % f.len()]);
                seg_dist(a, b, c) < 1e-4 && seg_dist(a, b, d) < 1e-4
            })
        }) {
            e.shared = true;
        }
    }
    let mut keep = drop.iter().map(|d| !d);
    edges.retain(|_| keep.next().unwrap());
    // Lines whole, as DACAD keeps them; ours may still be in pieces. Wall
    // abutments count too (mass case R1_0745: −WS along a step).
    let mut roof = geo.lines.clone();
    merge_collinear(&mut roof);
    for e in &edges {
        // An edge shorter than POINT_EPS gives no line (ck2 B1; the chord
        // under a knick, dacad-chimney-knick).
        if dist3_(e.c, e.d) < 0.02 {
            continue;
        }
        let on = roof.iter().find(|r| {
            std::iter::once((e.c, e.d))
                .chain(e.twin)
                .any(|(c, d)| lies_on(r, c, d))
        });
        let abutment = on.is_some_and(|r| {
            matches!(
                r.kind,
                LineKind::AbutmentFront | LineKind::AbutmentBack | LineKind::AbutmentSide
            )
        });
        // Along a wall, building or dormer cheek, the hole edge lies on the
        // wall's own edge and is DACAD's code 17 (FUN_00da5060; xm case
        // Q398_xp030_yp00): no line at all.
        let (c, d) = (Point2::new(e.c[0], e.c[1]), Point2::new(e.d[0], e.d[1]));
        let along = |a: Point2, b: Point2| seg_dist(a, b, c) < 1e-4 && seg_dist(a, b, d) < 1e-4;
        let on_wall = walls.iter().any(|&(a, b)| along(a, b))
            || geo
                .footprints
                .iter()
                .any(|f| (0..f.len()).any(|i| along(f[i], f[(i + 1) % f.len()])));
        if abutment && on_wall {
            continue;
        }
        if let Some(r) = on {
            geo.minus_lines
                .push((r.dacad_code().to_string(), dist3_(e.c, e.d)));
        }
        // On an abutment off any wall DACAD's copy comes back as code 0, no
        // line (mass case R1_0745: −WS along a step, no +WS).
        let edge_of_roof = abutment
            || on.is_some_and(|r| {
                matches!(r.kind, LineKind::Verge | LineKind::Pult | LineKind::Eave)
            });
        if e.shared || edge_of_roof {
            continue;
        }
        let face = &geo.faces[e.face];
        let (up, _) = slope(face);
        let (dx, dy) = (e.d[0] - e.c[0], e.d[1] - e.c[1]);
        let len = dx.hypot(dy);
        if len < TOL {
            continue;
        }
        // Outward normal of a CCW hole side.
        let out = (dy / len) * up.x - (dx / len) * up.y;
        let kind = if out < -LEVEL {
            LineKind::AbutmentFront
        } else if out > LEVEL {
            LineKind::AbutmentBack
        } else {
            LineKind::AbutmentSide
        };
        geo.lines.push(RoofLine {
            kind,
            a: e.c,
            b: e.d,
            side: None,
        });
    }
    if total > 0.0 {
        geo.openings.push(Opening {
            kind: OpeningKind::Chimney,
            area: total,
        });
    }
}

fn push_edges(edges: &mut Vec<HoleEdge>, pts: &[[f64; 3]], face: usize) {
    for i in 0..pts.len() {
        edges.push(HoleEdge {
            c: pts[i],
            d: pts[(i + 1) % pts.len()],
            face,
            shared: false,
            twin: None,
        });
    }
}

/// Ours may keep a plane in pieces where DACAD has one face (an overhang
/// piece beside a step, mass case R1_0745: one hole edge along the whole
/// valley). Hole edges of two pieces of one plane that continue each other
/// in a straight line become one edge.
fn join_coplanar_edges(faces: &[RoofFace], edges: &mut Vec<HoleEdge>) {
    const EPS: f64 = 1e-6;
    let coplanar = |i: usize, j: usize| {
        let (f, g) = (&faces[i], &faces[j]);
        let ((u, t), (v, w)) = (slope(f), slope(g));
        f.edge == g.edge
            && f.edge != usize::MAX
            && (t - w).abs() < EPS
            && u.distance(v) < EPS
            && g.polygon
                .iter()
                .all(|p| (height_at(f, u, t, Point2::new(p[0], p[1])) - p[2]).abs() < EPS)
    };
    let straight = |a: [f64; 3], b: [f64; 3], c: [f64; 3]| {
        let (ab, bc) = (dist3_(a, b), dist3_(b, c));
        ab > EPS && bc > EPS && (ab + bc - dist3_(a, c)).abs() < EPS
    };
    'again: loop {
        for i in 0..edges.len() {
            for j in 0..edges.len() {
                let (e, f) = (&edges[i], &edges[j]);
                if i == j || e.face == f.face || dist3_(e.d, f.c) > EPS {
                    continue;
                }
                if straight(e.c, e.d, f.d) && coplanar(e.face, f.face) {
                    edges[i].d = edges[j].d;
                    edges.remove(j);
                    continue 'again;
                }
            }
        }
        break;
    }
}

/// Plan distance from `p` to segment A→B.
fn seg_dist(a: Point2, b: Point2, p: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let l2 = dx * dx + dy * dy;
    let t = if l2 < TOL {
        0.0
    } else {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / l2).clamp(0.0, 1.0)
    };
    (p.x - a.x - t * dx).hypot(p.y - a.y - t * dy)
}

/// A corner dropped from a hole: the corner's index, the point it merged
/// into and whether that point lies on the side after the corner.
type Merge = (usize, Point2, bool);

/// The chimney's hole in one face, CCW, in 3D: the chimney clipped by the
/// face, without a corner next to the point where a neighbouring side
/// leaves the face that lies near it (every coordinate within POINT_EPS)
/// or within VALUE_EPS of the face's outline in plan. `None` when under three corners are left (DACAD sweeps
/// dacad-chimney-across-line V45_02, dacad-chimney-over-ridge; points where
/// sides cross the face's edge never merge, ck2 B1).
fn face_hole(face: &RoofFace, corners: &[Point2]) -> Option<(Vec<[f64; 3]>, Vec<Merge>)> {
    const POINT_EPS: f32 = 0.02;
    const VALUE_EPS: f32 = 0.01;
    let near = |p: [f64; 3], q: [f64; 3]| (0..3).all(|i| ((p[i] - q[i]) as f32).abs() < POINT_EPS);
    // Plan distance from the face's outline.
    let outline = plan(face);
    let off_edge = |p: Point2| {
        let m = outline.len();
        (0..m)
            .map(|i| {
                let (a, b) = (outline[i], outline[(i + 1) % m]);
                let (dx, dy) = (b.x - a.x, b.y - a.y);
                let l2 = dx * dx + dy * dy;
                let t = if l2 < TOL {
                    0.0
                } else {
                    (((p.x - a.x) * dx + (p.y - a.y) * dy) / l2).clamp(0.0, 1.0)
                };
                (p.x - a.x - t * dx).hypot(p.y - a.y - t * dy)
            })
            .fold(f64::INFINITY, f64::min)
    };
    // Degenerate faces the sweep leaves along a line are not DACAD faces.
    if signed_area(&plan(face)).abs() < 1e-6 {
        return None;
    }
    let mut hole = clip_polygon(&plan(face), corners);
    hole.dedup_by(|a, b| a.distance(*b) < 1e-9);
    while hole.len() > 1 && hole[0].distance(hole[hole.len() - 1]) < 1e-9 {
        hole.pop();
    }
    let cut = signed_area(&hole);
    if cut.abs() < 1e-9 {
        return None;
    }
    if cut < 0.0 {
        hole.reverse();
    }
    let (up, tan) = slope(face);
    let pts: Vec<[f64; 3]> = hole
        .iter()
        .map(|&p| [p.x, p.y, height_at(face, up, tan, p)])
        .collect();
    // A corner on the outline is where its sides leave the face.
    let corner = |p: Point2| {
        corners
            .iter()
            .position(|c| c.distance(p) < 1e-9)
            .filter(|_| off_edge(p) > 1e-9)
    };
    let n = pts.len();
    let mut merges = Vec::new();
    let mut keep = vec![true; n];
    for i in 0..n {
        let Some(k) = corner(hole[i]) else {
            continue;
        };
        for (j, next) in [((i + 1) % n, true), ((i + n - 1) % n, false)] {
            if corner(hole[j]).is_none()
                && (near(pts[i], pts[j]) || (off_edge(hole[i]) as f32) < VALUE_EPS)
            {
                keep[i] = false;
                merges.push((k, hole[j], next));
                break;
            }
        }
    }
    // A sliver of a hole whose face corner lies on a crossing beside it
    // closes to nothing (mass case R2_0610: a triangle with a 1.3 mm side;
    // in larger holes the corner stays, R2_0537).
    let face_corner = |p: Point2| outline.iter().any(|q| q.distance(p) < 1e-9);
    if keep.iter().filter(|&&k| k).count() == 3 {
        let sliver = (0..n).any(|i| {
            keep[i]
                && face_corner(hole[i])
                && [(i + 1) % n, (i + n - 1) % n].iter().any(|&j| {
                    keep[j]
                        && corner(hole[j]).is_none()
                        && !face_corner(hole[j])
                        && near(pts[i], pts[j])
                })
        });
        if sliver {
            return None;
        }
    }
    let pts: Vec<[f64; 3]> = pts
        .into_iter()
        .zip(keep)
        .filter_map(|(p, k)| k.then_some(p))
        .collect();
    (pts.len() >= 3).then_some((pts, merges))
}

/// Where a chimney corner merges into the point its neighbouring side
/// leaves a face at (see [`face_hole`]), DACAD turns the corner's other
/// side to run through that point, pivoting on the side's eave crossing
/// (mass case R1_0681). Returns whether a corner moved.
fn pivot_corner(geo: &RoofGeometry, corners: &mut [Point2]) -> bool {
    let n = corners.len();
    for face in &geo.faces {
        let Some((_, merges)) = face_hole(face, corners) else {
            continue;
        };
        let Some(&(k, w, next)) = merges.first() else {
            continue;
        };
        // The side that turns runs from corner r to corner k.
        let (r, rr) = if next {
            ((k + n - 1) % n, (k + n - 2) % n)
        } else {
            ((k + 1) % n, (k + 2) % n)
        };
        let has = |p: [f64; 3]| {
            face.polygon
                .iter()
                .any(|q| (0..3).all(|i| (p[i] - q[i]).abs() < 1e-6))
        };
        let flat = |p: [f64; 3]| Point2::new(p[0], p[1]);
        let eave = geo
            .lines
            .iter()
            .filter(|l| l.kind == LineKind::Eave && has(l.a) && has(l.b))
            .find_map(|l| {
                let (a, b) = (flat(l.a), flat(l.b));
                let x = cross_lines(corners[r], corners[k], a, b)?;
                let len = a.distance(b);
                let t = ((x.x - a.x) * (b.x - a.x) + (x.y - a.y) * (b.y - a.y)) / len;
                (-TOL..=len + TOL).contains(&t).then_some(x)
            });
        if let Some(e) = eave
            && let Some(x) = cross_lines(e, w, corners[rr], corners[r])
        {
            corners[r] = x;
        }
        corners[k] = w;
        return true;
    }
    false
}

/// Whether a hole edge lies on roof line `r` by DACAD's test [`on_line`].
/// DACAD measures from the line's start as its edge table stores it: the
/// edge's direction in the first face listing it, and faces run either way
/// (SOL files of the mass test). Not modelled; a sloped line is measured
/// from its lower end, which DACAD's sweep dacad-chimney-near-line/near3
/// needs (P0a–P1b: from the upper end a side 6.5 cm off a hip near the
/// eave would pass). Mass case R1_0808 needs the upper end and fails. The
/// edge may run either way [no source — extrapolation]. A level line is
/// tried from both ends.
fn lies_on(r: &RoofLine, c: [f64; 3], d: [f64; 3]) -> bool {
    let (lo, hi) = if r.a[2] <= r.b[2] {
        (r.a, r.b)
    } else {
        (r.b, r.a)
    };
    let from = |a, b| on_line((a, b), c, d) || on_line((a, b), d, c);
    from(lo, hi) || ((hi[2] - lo[2]) as f32) < 0.01 && from(hi, lo)
}

/// DACAD's test whether hole edge C→D lies on roof line A→B: the ends
/// agree within VALUE_EPS, or the unit vectors B−A, C−A and D−C agree in
/// every coordinate within VALUE_EPS (either sign) with C and D inside the
/// line's box grown by VALUE_EPS; all in f32. See [`lies_on`] for which
/// end is A.
fn on_line((a, b): ([f64; 3], [f64; 3]), c: [f64; 3], d: [f64; 3]) -> bool {
    const VALUE_EPS: f32 = 0.01;
    let eq = |p: [f64; 3], q: [f64; 3]| {
        (0..3).all(|i| ((p[i] as f32) - (q[i] as f32)).abs() < VALUE_EPS)
    };
    if (eq(a, c) && eq(b, d)) || (eq(a, d) && eq(b, c)) {
        return true;
    }
    let unit = |p: [f64; 3], q: [f64; 3]| -> [f32; 3] {
        let v = [0, 1, 2].map(|i| (q[i] as f32) - (p[i] as f32));
        let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if len > 0.0 { v.map(|x| x / len) } else { v }
    };
    let parallel = |u: [f32; 3], v: [f32; 3]| {
        (0..3).all(|i| (u[i] - v[i]).abs() < VALUE_EPS)
            || (0..3).all(|i| (u[i] + v[i]).abs() < VALUE_EPS)
    };
    let inside_box = |p: [f64; 3]| {
        (0..3).all(|i| {
            let (lo, hi) = (a[i].min(b[i]) as f32, a[i].max(b[i]) as f32);
            let x = p[i] as f32;
            x >= lo - VALUE_EPS && x <= hi + VALUE_EPS
        })
    };
    if !inside_box(c) || !inside_box(d) {
        return false;
    }
    // [no source — extrapolation] an edge starting at A is measured from D.
    let uc = if eq(a, c) { unit(a, d) } else { unit(a, c) };
    let (ub, ud) = (unit(a, b), unit(c, d));
    parallel(ub, uc) && parallel(ud, uc) && parallel(ub, ud)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 10 × 8 m gable roof, 45°, eave 3 m, no overhang; ridge along x at y = 4.
    fn gable() -> Project {
        let e = |x: f64, y: f64, pitch: f64| {
            serde_json::json!({ "start": { "x": x, "y": y }, "pitch_deg": pitch,
                "eave_height": 3.0, "overhang": 0.0 })
        };
        let part = serde_json::json!({ "name": "A", "edges": [
            e(0.0, 0.0, 45.0), e(10.0, 0.0, 90.0), e(10.0, 8.0, 45.0), e(0.0, 8.0, 90.0)] });
        let mut p = Project::new("t");
        p.parts.push(serde_json::from_value(part).unwrap());
        p
    }

    fn square(x: f64, y: f64, w: f64, d: f64) -> Chimney {
        let c = |x, y| Point2::new(x, y);
        Chimney {
            corners: vec![c(x, y), c(x + w, y), c(x + w, y + d), c(x, y + d)],
            height: 0.0,
        }
    }

    #[test]
    fn a_window_with_its_corner_on_the_ridge_or_a_hip_is_still_deducted() {
        // DACAD takes a window's full width × height off its face wherever
        // it lies; a corner exactly on a line must not drop it.
        let area = |p: &Project| -> f64 {
            generate_project(p)
                .iter()
                .flat_map(|g| &g.faces)
                .map(|f| f.area)
                .sum()
        };
        let window = |x, y| RoofWindow {
            x,
            y,
            width: 0.78,
            height: 1.18,
        };
        let bare = gable();
        let gross = area(&bare);
        for (x, y) in [(5.0, 4.0), (5.0, 3.99), (0.0, 4.0), (5.0, 0.0)] {
            let mut p = gable();
            p.windows.push(window(x, y));
            let geos = generate_project(&p);
            assert!(window_face(&geos, &p.windows[0]).is_some(), "({x}, {y})");
            assert!(
                (gross - area(&p) - 0.78 * 1.18).abs() < 1e-4,
                "({x}, {y}): {} of {gross}",
                area(&p)
            );
        }
        // On a hip roof: a corner on a hip and on the ridge's end.
        let mut hip = Project::new("h");
        hip.parts.push(crate::examples::standard_roof(
            crate::examples::StandardRoof::Hip,
            10.0,
            8.0,
            35.0,
            3.0,
            0.0,
        ));
        let gross = area(&hip);
        for (x, y) in [(1.5, 1.5), (8.5, 6.5), (4.0, 4.0)] {
            let mut p = hip.clone();
            p.windows.push(window(x, y));
            assert!(
                (gross - area(&p) - 0.78 * 1.18).abs() < 1e-4,
                "({x}, {y}): {} of {gross}",
                area(&p)
            );
        }
        // Off the roof stays off the roof.
        let mut p = gable();
        p.windows.push(window(12.0, 4.0));
        assert!(window_face(&generate_project(&p), &p.windows[0]).is_none());
    }

    fn total(geo: &RoofGeometry, code: &str) -> f64 {
        geo.lines
            .iter()
            .filter(|l| l.dacad_code() == code)
            .map(|l| l.length())
            .sum()
    }

    #[test]
    fn non_convex_chimney_leaves_no_roof() {
        let l_shape = Chimney {
            corners: [
                (4.0, 2.0),
                (5.2, 2.0),
                (5.2, 2.6),
                (4.6, 2.6),
                (4.6, 3.2),
                (4.0, 3.2),
            ]
            .iter()
            .map(|&(x, y)| Point2::new(x, y))
            .collect(),
            ..square(0.0, 0.0, 1.0, 1.0)
        };
        assert!(!dacad_convex(&l_shape.corners));
        assert!(dacad_convex(&square(6.0, 2.0, 0.6, 0.6).corners));
        for chimneys in [
            vec![l_shape.clone()],
            vec![square(6.0, 2.0, 0.6, 0.6), l_shape],
        ] {
            let mut p = gable();
            p.chimneys = chimneys;
            let geo = &generate_project(&p)[0];
            assert!(geo.faces.is_empty() && geo.lines.is_empty());
            let err = geo.error.as_ref().expect("DACAD's error is reported");
            assert_eq!(err.kind, crate::roof::CalcErrorKind::NonConvexOpening);
        }
    }

    #[test]
    fn roof_opening_sides_are_roof_edges() {
        let mut p = gable();
        let before = generate_project(&p)[0]
            .faces
            .iter()
            .map(|f| f.area)
            .sum::<f64>();
        p.roof_openings.push(square(4.0, 1.0, 1.0, 2.0));
        let geo = &generate_project(&p)[0];
        let s = 2f64.sqrt();
        // Uphill side is an eave, downhill side a Pult, the sides verges.
        assert!((total(geo, "T") - 21.0).abs() < 1e-6);
        assert!((total(geo, "P") - 1.0).abs() < 1e-6);
        assert!((total(geo, "OL") - (8.0 * s + 2.0 * s)).abs() < 1e-6);
        assert!((total(geo, "OR") - (8.0 * s + 2.0 * s)).abs() < 1e-6);
        assert!(total(geo, "WV") + total(geo, "WH") + total(geo, "WS") < 1e-9);
        let after = geo.faces.iter().map(|f| f.area).sum::<f64>();
        // DACAD's core writes its areas as f32.
        assert!((before - after - 2.0 * s).abs() < 1e-5);
        assert_eq!(geo.openings[0].kind, OpeningKind::Hole);
    }

    /// DACAD run OP1: 12 × 8 m hip roof, 35°, eave 3 m, opening 5..6 × 1..2.
    #[test]
    fn roof_opening_matches_dacad_op1() {
        let e = |x: f64, y: f64| {
            serde_json::json!({ "start": { "x": x, "y": y }, "pitch_deg": 35.0,
                "eave_height": 3.0, "overhang": 0.0 })
        };
        let part = serde_json::json!({ "name": "A", "edges": [
            e(0.0, 0.0), e(12.0, 0.0), e(12.0, 8.0), e(0.0, 8.0)] });
        let mut p = Project::new("op1");
        p.parts.push(serde_json::from_value(part).unwrap());
        p.roof_openings.push(square(5.0, 1.0, 1.0, 1.0));
        let geo = &generate_project(&p)[0];
        let area = geo.faces.iter().map(|f| f.area).sum::<f64>();
        assert!((area - 115.974).abs() < 1e-3, "{area}");
        assert!((total(geo, "P") - 1.0).abs() < 1e-6);
        assert!((total(geo, "OL") - 1.221).abs() < 1e-3);
        assert!((total(geo, "OR") - 1.221).abs() < 1e-3);
        let west = geo.lines.iter().find(|l| l.dacad_code() == "OR").unwrap();
        assert!((west.a[0] - 5.0).abs() < 1e-9);
    }

    #[test]
    fn rotated_roof_opening_has_only_oblique_verges() {
        let mut p = gable();
        let c = Point2::new(5.0, 2.0);
        let (s, co) = (30f64.to_radians().sin(), 30f64.to_radians().cos());
        let mut corners = Vec::new();
        for (u, v) in [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)] {
            corners.push(Point2::new(c.x + u * co - v * s, c.y + u * s + v * co));
        }
        p.roof_openings.push(Chimney {
            corners,
            height: 0.0,
        });
        let geo = &generate_project(&p)[0];
        // Base gable: T 20, OL = OR = 8√2; the opening adds four OS sides.
        assert!((total(geo, "T") - 20.0).abs() < 1e-6);
        assert!(total(geo, "P") < 1e-9);
        assert!(total(geo, "OS") > 4.0);
    }
}
