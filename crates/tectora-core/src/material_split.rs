//! One roof plane over walls in line with different roof materials, split
//! per wall.
//!
//! The core roofs neighbouring walls in line with the same eave height,
//! overhang and cladding as one plane (`dcore::s1::merge`): it does not
//! compare the material, so the plane's faces all name one of the walls and
//! carry its material, and the other wall's sheet metal (or tiles) is lost
//! from the area per material. Tectora departs from DACAD here (Jakub,
//! 2026-11-21): each such face is cut, in plan, by lines square to the wall
//! line at the corners where the material changes. The strip above each
//! wall is that wall's, with its material: the roof falls square to its
//! eave, so the strip is the part of the plane that drains onto the wall.
//!
//! Only the material changes. The roof, its faces' outlines taken together,
//! its area, its line lengths and every deduction stay exactly what the
//! core gave; a face is split only when each of its deductions can be
//! placed on one side or the other:
//!
//! * a roof window wholly on the piece its corner lies on (as
//!   [`crate::openings::window_face`] finds a window's face);
//! * chimneys, holes and dormers by the share of their plan outline over
//!   each piece, scaled to the deduction the face carries. Where that
//!   share does not account for the deduction (a cut by another building
//!   of a joined roof), the face stays whole and
//!   [`crate::check::merged_materials`] warns instead.
//!
//! Lines on the plane that cross a cut (the eave, a ridge or valley running
//! along it) are split at the cut, so lines per material follow the faces.
//! No new line is drawn: the cut is no line of the roof.

use geo::{Area, BooleanOps, MultiPolygon};

use crate::geom::Point2;
use crate::model::{Project, RoofPart};
use crate::openings::{plan, window_face};
use crate::roof::{RoofFace, RoofGeometry, RoofLine, counted_material};
use crate::union::{Plane, from_geo, to_geo};

/// Smallest piece of a face kept, m² in plan.
const MIN_PIECE: f64 = 1e-6;
/// How far a line's end may lie from a cut and still not be split, m.
const ON_CUT: f64 = 1e-6;

/// Splits the faces of every plane over walls in line with different
/// materials (see the module docs). `geos[i]` is the roof of
/// `project.parts[i]`, as `openings::generate_valid` gives them.
pub(crate) fn split(project: &Project, geos: &mut [RoofGeometry]) {
    for (pi, part) in project.parts.iter().enumerate() {
        let Some(geo) = geos.get(pi) else { break };
        if geo.error.is_some() || geo.faces.is_empty() {
            continue;
        }
        for run in runs(part) {
            let mut all = geos.to_vec();
            if split_run(project, part, &run, pi, &mut all) {
                geos.clone_from_slice(&all);
            }
        }
    }
}

/// The runs of two or more neighbouring walls the core roofs as one plane
/// ([`crate::check::one_plane`]) with more than one material among them,
/// each in wall order.
fn runs(part: &RoofPart) -> Vec<Vec<usize>> {
    let n = part.edges.len();
    if n < 3 {
        return Vec::new();
    }
    let joined = |j: usize| crate::check::one_plane(part, j, (j + 1) % n);
    // Start after a break, so no run is cut where the loop closes.
    let Some(start) = (0..n).find(|&j| !joined((j + n - 1) % n)) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut run = vec![start];
    for step in 1..=n {
        let prev = (start + step - 1) % n;
        let j = (start + step) % n;
        if step < n && joined(prev) {
            run.push(j);
            continue;
        }
        let mats = run
            .iter()
            .map(|&w| counted_material(part.edges[w].material))
            .collect::<std::collections::BTreeSet<_>>();
        if run.len() >= 2 && mats.len() >= 2 {
            out.push(std::mem::take(&mut run));
        }
        run = vec![j];
    }
    out
}

/// A strip of the plane: from `lo` to `hi` along the wall line, the wall
/// whose strip it is and its material.
struct Strip {
    lo: f64,
    hi: f64,
    wall: usize,
    material: u8,
}

/// One face cut into strips: per strip, its plan pieces.
struct Cut {
    face: usize,
    pieces: Vec<(usize, Vec<Point2>)>,
}

/// Splits the faces of `run` in `geos[pi]`; false (and `geos` as it may
/// now be, to be dropped) where the faces cannot be split exactly.
fn split_run(
    project: &Project,
    part: &RoofPart,
    run: &[usize],
    pi: usize,
    geos: &mut [RoofGeometry],
) -> bool {
    let n = part.edges.len();
    let start = |w: usize| part.edges[w].start;
    let first = run[0];
    let (o, end) = (start(first), start((first + 1) % n));
    let len = (end.x - o.x).hypot(end.y - o.y);
    if len < 1e-9 {
        return false;
    }
    let u = Point2::new((end.x - o.x) / len, (end.y - o.y) / len);
    let along = |p: Point2| (p.x - o.x) * u.x + (p.y - o.y) * u.y;

    // The faces the core gave the plane, all on one wall of the run.
    let geo = &geos[pi];
    let faces: Vec<usize> = (0..geo.faces.len())
        .filter(|&k| run.contains(&geo.faces[k].edge))
        .collect();
    let owners: std::collections::BTreeSet<usize> =
        faces.iter().map(|&k| geo.faces[k].edge).collect();
    if owners.len() != 1 {
        return false;
    }

    // Strips: neighbouring walls of one material are one strip; the first
    // and the last run on past the run's ends (an overhang at a gable).
    let mut strips: Vec<Strip> = Vec::new();
    for (i, &w) in run.iter().enumerate() {
        let m = counted_material(part.edges[w].material);
        let at = if i == 0 {
            f64::NEG_INFINITY
        } else {
            along(start(w))
        };
        match strips.last_mut() {
            Some(s) if s.material == m => {
                if owners.contains(&w) {
                    s.wall = w;
                }
            }
            _ => {
                if let Some(s) = strips.last_mut() {
                    s.hi = at;
                }
                strips.push(Strip {
                    lo: at,
                    hi: f64::INFINITY,
                    wall: w,
                    material: m,
                });
            }
        }
    }
    let cuts_at: Vec<f64> = strips.iter().skip(1).map(|s| s.lo).collect();
    if strips.windows(2).any(|s| s[1].lo <= s[0].lo + ON_CUT) {
        return false;
    }

    // Each face's plan pieces, strip by strip.
    let reach = 10.0
        * (1.0
            + geo
                .faces
                .iter()
                .flat_map(|f| &f.polygon)
                .map(|p| p[0].abs().max(p[1].abs()))
                .fold(o.x.abs().max(o.y.abs()), f64::max));
    let v = Point2::new(-u.y, u.x);
    let band = |lo: f64, hi: f64| {
        let (lo, hi) = (lo.max(-reach), hi.min(reach));
        let at = |t: f64, s: f64| Point2::new(o.x + u.x * t + v.x * s, o.y + u.y * t + v.y * s);
        vec![at(lo, -reach), at(hi, -reach), at(hi, reach), at(lo, reach)]
    };
    let mut cuts = Vec::new();
    for &k in &faces {
        let face = &geo.faces[k];
        let shape = MultiPolygon::new(vec![to_geo(&plan(face))]);
        let mut pieces = Vec::new();
        for (si, s) in strips.iter().enumerate() {
            let part = shape.intersection(&MultiPolygon::new(vec![to_geo(&band(s.lo, s.hi))]));
            for poly in part.0.iter().filter(|p| p.unsigned_area() > MIN_PIECE) {
                if !poly.interiors().is_empty() {
                    return false;
                }
                pieces.push((si, from_geo(poly)));
            }
        }
        if pieces.is_empty() {
            return false;
        }
        cuts.push(Cut { face: k, pieces });
    }

    // Windows of this roof in the order of its window cuts.
    let windows: Vec<&crate::model::RoofWindow> = project
        .windows
        .iter()
        .filter(|w| window_face(geos, w).is_some_and(|(g, _)| g == pi))
        .collect();
    if windows.len() != geos[pi].window_cuts.len() {
        return false;
    }
    let objects: Vec<Vec<Point2>> = project
        .chimneys
        .iter()
        .chain(&project.roof_openings)
        .map(|c| c.corners.clone())
        .chain(geos[pi].footprints.iter().cloned())
        .filter(|c| c.len() >= 3)
        .collect();

    let geo = &mut geos[pi];
    let mut new_faces: Vec<RoofFace> = Vec::new();
    let mut moved: Vec<(usize, usize)> = Vec::new();
    for cut in &cuts {
        let face = geo.faces[cut.face].clone();
        let plane = Plane::of(&face);
        let sec = plane.sec();
        let mut built: Vec<RoofFace> = cut
            .pieces
            .iter()
            .map(|(si, poly)| RoofFace {
                edge: strips[*si].wall,
                pitch_deg: face.pitch_deg,
                polygon: poly.iter().map(|&p| [p.x, p.y, plane.z(p)]).collect(),
                area: crate::geom::signed_area(poly).abs() * sec,
                deducted: 0.0,
                material: strips[*si].material,
            })
            .collect();
        let gross: f64 = built.iter().map(|f| f.area).sum();
        if (gross - (face.area + face.deducted)).abs() > 1e-6 * (1.0 + gross) {
            return false;
        }

        // Roof windows: whole on the piece their corner lies on.
        let pieces_geo = [RoofGeometry {
            faces: built.clone(),
            ..RoofGeometry::default()
        }];
        let mut windows_d = 0.0;
        let mut window_piece = Vec::new();
        for (ci, wc) in geo.window_cuts.iter().enumerate() {
            if wc.face != cut.face {
                continue;
            }
            let Some((_, piece)) = window_face(&pieces_geo, windows[ci]) else {
                return false;
            };
            let d = wc.width * wc.height;
            built[piece].deducted += d;
            windows_d += d;
            window_piece.push((ci, piece));
        }

        // Chimneys, holes and dormers: by their plan share over each piece.
        let rest = face.deducted - windows_d;
        if rest > 1e-6 {
            let share: Vec<f64> = built
                .iter()
                .map(|b| {
                    let piece = MultiPolygon::new(vec![to_geo(&plan(b))]);
                    objects
                        .iter()
                        .map(|c| {
                            piece
                                .intersection(&MultiPolygon::new(vec![to_geo(c)]))
                                .unsigned_area()
                        })
                        .sum::<f64>()
                        * sec
                })
                .collect();
            let total: f64 = share.iter().sum();
            if total <= 0.0 || (total - rest).abs() > 1e-3 + 0.01 * rest {
                return false;
            }
            for (b, s) in built.iter_mut().zip(&share) {
                b.deducted += s / total * rest;
            }
        }
        for b in built.iter_mut() {
            b.area -= b.deducted;
        }
        // The last piece takes what rounding left, so the face's figures
        // add up to the core's exactly.
        let Some((last, others)) = built.split_last_mut() else {
            return false;
        };
        last.area = face.area - others.iter().map(|f| f.area).sum::<f64>();
        last.deducted = face.deducted - others.iter().map(|f| f.deducted).sum::<f64>();
        if built
            .iter()
            .any(|f| !(f.area > -1e-9 && f.area.is_finite() && f.deducted.is_finite()))
        {
            return false;
        }

        // The first piece keeps the face's place; the others go at the end.
        let base = geo.faces.len() + new_faces.len();
        let index = |piece: usize| {
            if piece == 0 {
                cut.face
            } else {
                base + piece - 1
            }
        };
        for (ci, piece) in window_piece {
            moved.push((ci, index(piece)));
        }
        let mut it = built.into_iter();
        if let Some(f) = it.next() {
            geo.faces[cut.face] = f;
        }
        new_faces.extend(it);
    }
    geo.faces.extend(new_faces);
    for (ci, k) in moved {
        geo.window_cuts[ci].face = k;
    }

    // Lines on the plane crossing a cut, split there.
    let split_faces: Vec<RoofFace> = geo
        .faces
        .iter()
        .filter(|f| run.contains(&f.edge))
        .cloned()
        .collect();
    let mut lines = Vec::with_capacity(geo.lines.len());
    for line in geo.lines.drain(..) {
        lines.extend(split_line(line, &cuts_at, o, u, &split_faces));
    }
    geo.lines = lines;
    true
}

/// `line` cut where it crosses one of the cut lines (square to the wall
/// line through `o` along `u`, at the distances `cuts_at`) on one of the
/// split `faces`.
fn split_line(
    line: RoofLine,
    cuts_at: &[f64],
    o: Point2,
    u: Point2,
    faces: &[RoofFace],
) -> Vec<RoofLine> {
    let along = |p: [f64; 3]| (p[0] - o.x) * u.x + (p[1] - o.y) * u.y;
    let (ta, tb) = (along(line.a), along(line.b));
    let mut at: Vec<f64> = cuts_at
        .iter()
        .filter(|&&t| t > ta.min(tb) + ON_CUT && t < ta.max(tb) - ON_CUT)
        .map(|&t| (t - ta) / (tb - ta))
        .filter(|&s| {
            let p = crate::openings::lerp3(line.a, line.b, s);
            faces.iter().any(|f| crate::quantities::bounds(f, p))
        })
        .collect();
    if at.is_empty() {
        return vec![line];
    }
    at.sort_by(f64::total_cmp);
    let mut out = Vec::with_capacity(at.len() + 1);
    let mut from = line.a;
    for s in at {
        let to = crate::openings::lerp3(line.a, line.b, s);
        out.push(RoofLine {
            a: from,
            b: to,
            ..line.clone()
        });
        from = to;
    }
    out.push(RoofLine { a: from, ..line });
    out
}
