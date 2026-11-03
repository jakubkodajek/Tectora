//! Several buildings under one roof: each closed wall loop gets its own roof,
//! and DACAD joins them like solids.
//!
//! A building is the prism over its walls up to its roof; the overhang is a
//! sheet without volume. So:
//!
//! * a roof is hidden where it lies inside another building's walls and below
//!   that building's roof. Under another roof's overhang it stays, and the
//!   lower roof runs on to the higher building's wall;
//! * where two roofs cross, they meet in a valley K; a horizontal one is a
//!   Schlucht S, or a Knick line c where the planes slope the same way;
//! * where a lower roof meets a higher building's wall, the wall makes an
//!   abutment line: WV, WH or WS by how the wall lies in the roof plane, as
//!   at a chimney.
//!
//! * an eave where another roof goes on at the same height, e.g. along a
//!   wall two buildings share, is the gutter between them: a Schlucht S.
//!
//! Where two roofs coincide, the one of the earlier building is kept.

use geo::{Area, BooleanOps, Coord, LineString, MultiPolygon, Polygon};

use crate::geom::{Point2, signed_area};
use crate::model::RoofPart;
use crate::openings::{inside, lerp3, plan, slope};
use crate::roof::{LineKind, RoofFace, RoofGeometry, RoofLine};

pub(crate) const EPS: f64 = 1e-6;
/// DACAD snaps outline points to whole centimetres, so roofs meant to meet
/// at one height can miss each other by millimetres (TRAING07).
const SNAP: f64 = 5e-3;
/// Two uphill directions closer than this count as the same (cos).
const SAME_WAY: f64 = 1.0 - 1e-6;

/// `z = a x + b y + c`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Plane {
    pub(crate) a: f64,
    pub(crate) b: f64,
    pub(crate) c: f64,
}

impl Plane {
    pub(crate) fn of(face: &RoofFace) -> Self {
        let (up, tan) = slope(face);
        let o = face.polygon[0];
        let (a, b) = (up.x * tan, up.y * tan);
        Plane {
            a,
            b,
            c: o[2] - a * o[0] - b * o[1],
        }
    }

    pub(crate) fn z(&self, p: Point2) -> f64 {
        self.a * p.x + self.b * p.y + self.c
    }

    pub(crate) fn sec(&self) -> f64 {
        (1.0 + self.a * self.a + self.b * self.b).sqrt()
    }

    pub(crate) fn same(&self, o: &Plane) -> bool {
        (self.a - o.a).abs() < EPS && (self.b - o.b).abs() < EPS && (self.c - o.c).abs() < EPS
    }
}

/// One building as generated on its own.
struct Building {
    walls: Vec<Point2>,
    faces: Vec<(Vec<Point2>, Plane)>,
}

impl Building {
    /// Height of this building's roof at `p`, if its roof covers `p`.
    fn roof_z(&self, p: Point2) -> Option<f64> {
        self.faces
            .iter()
            .find(|(poly, _)| covers(poly, p))
            .map(|(_, pl)| pl.z(p))
    }

    /// Plane of the face covering `p`.
    fn plane_at(&self, p: Point2) -> Option<Plane> {
        self.faces
            .iter()
            .find(|(poly, _)| covers(poly, p))
            .map(|(_, pl)| *pl)
    }
}

pub(crate) fn near_boundary(poly: &[Point2], p: Point2, tol: f64) -> bool {
    let n = poly.len();
    (0..n).any(|i| {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let len2 = dx * dx + dy * dy;
        if len2 < 1e-18 {
            return a.distance(p) < tol;
        }
        let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0);
        Point2::new(a.x + t * dx, a.y + t * dy).distance(p) < tol
    })
}

pub(crate) fn covers(poly: &[Point2], p: Point2) -> bool {
    inside(poly, p) || near_boundary(poly, p, EPS)
}

pub(crate) fn strictly_inside(poly: &[Point2], p: Point2) -> bool {
    inside(poly, p) && !near_boundary(poly, p, EPS)
}

/// Whether building `j` hides a point of building `i`'s roof.
fn hides(j: usize, b: &Building, i: usize, p: Point2, z: f64) -> bool {
    // An earlier roof running on in the same plane, also over its overhang
    // (DACAD cases C60, C61).
    if j < i
        && b.faces
            .iter()
            .any(|(f, pl)| strictly_inside(f, p) && (pl.z(p) - z).abs() <= EPS)
    {
        return true;
    }
    if !strictly_inside(&b.walls, p) {
        return false;
    }
    match b.roof_z(p) {
        Some(zj) => zj > z + EPS || (j < i && (zj - z).abs() <= EPS),
        // Inside the walls the roof always covers; be safe.
        None => false,
    }
}

pub(crate) fn to_geo(poly: &[Point2]) -> Polygon<f64> {
    let mut pts: Vec<Coord<f64>> = poly.iter().map(|p| Coord { x: p.x, y: p.y }).collect();
    if let Some(&f) = pts.first() {
        pts.push(f);
    }
    Polygon::new(LineString::new(pts), vec![])
}

pub(crate) fn from_geo(poly: &Polygon<f64>) -> Vec<Point2> {
    let mut pts: Vec<Point2> = poly
        .exterior()
        .0
        .iter()
        .map(|c| Point2::new(c.x, c.y))
        .collect();
    pts.pop();
    if signed_area(&pts) < 0.0 {
        pts.reverse();
    }
    pts
}

/// The half-plane `a x + b y + c > 0` as a big convex polygon.
pub(crate) fn half_plane(a: f64, b: f64, c: f64, r: f64) -> Vec<Point2> {
    let sq = [
        Point2::new(-r, -r),
        Point2::new(r, -r),
        Point2::new(r, r),
        Point2::new(-r, r),
    ];
    let f = |p: Point2| a * p.x + b * p.y + c;
    let mut out = Vec::new();
    for k in 0..4 {
        let (p, q) = (sq[k], sq[(k + 1) % 4]);
        let (fp, fq) = (f(p), f(q));
        if fp >= 0.0 {
            out.push(p);
        }
        if (fp >= 0.0) != (fq >= 0.0) {
            let t = fp / (fp - fq);
            out.push(Point2::new(p.x + t * (q.x - p.x), p.y + t * (q.y - p.y)));
        }
    }
    out
}

/// Parameters in (0, 1) where segment `a`–`b` crosses a polygon edge or
/// where its height crosses a plane.
pub(crate) fn splits(a: [f64; 3], b: [f64; 3], polys: &[&[Point2]], planes: &[Plane]) -> Vec<f64> {
    let (pa, pb) = (Point2::new(a[0], a[1]), Point2::new(b[0], b[1]));
    let d = Point2::new(pb.x - pa.x, pb.y - pa.y);
    let mut ts = vec![0.0, 1.0];
    for poly in polys {
        let n = poly.len();
        for i in 0..n {
            let (p, q) = (poly[i], poly[(i + 1) % n]);
            let e = Point2::new(q.x - p.x, q.y - p.y);
            let den = d.x * e.y - d.y * e.x;
            if den.abs() < 1e-12 {
                continue;
            }
            let w = Point2::new(p.x - pa.x, p.y - pa.y);
            let t = (w.x * e.y - w.y * e.x) / den;
            let u = (w.x * d.y - w.y * d.x) / den;
            if (-1e-9..=1.0 + 1e-9).contains(&u) && t > 0.0 && t < 1.0 {
                ts.push(t);
            }
        }
    }
    for pl in planes {
        let (fa, fb) = (pl.z(pa) - a[2], pl.z(pb) - b[2]);
        if (fa > 0.0) != (fb > 0.0) && (fa - fb).abs() > 1e-12 {
            let t = fa / (fa - fb);
            if t > 0.0 && t < 1.0 {
                ts.push(t);
            }
        }
    }
    ts.sort_by(f64::total_cmp);
    ts.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
    ts
}

/// Sub-segments of `a`–`b` whose midpoint passes `keep`, joined where they
/// touch.
pub(crate) fn keep_parts(
    a: [f64; 3],
    b: [f64; 3],
    ts: &[f64],
    keep: impl Fn(Point2, f64) -> bool,
) -> Vec<([f64; 3], [f64; 3])> {
    let mut out: Vec<([f64; 3], [f64; 3])> = Vec::new();
    let mut last_t = f64::NAN;
    for s in ts.windows(2) {
        if s[1] - s[0] < 1e-9 {
            continue;
        }
        let m = lerp3(a, b, (s[0] + s[1]) / 2.0);
        if keep(Point2::new(m[0], m[1]), m[2]) {
            match out.last_mut() {
                Some(l) if (last_t - s[0]).abs() < 1e-12 => l.1 = lerp3(a, b, s[1]),
                _ => out.push((lerp3(a, b, s[0]), lerp3(a, b, s[1]))),
            }
            last_t = s[1];
        }
    }
    out
}

pub(crate) fn len3(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Joins the roofs of several buildings. `geos[i]` is the roof generated on
/// its own for `parts[i]`.
pub fn join(geos: &mut [RoofGeometry], parts: &[RoofPart]) {
    if geos.len() < 2 {
        return;
    }
    let bs: Vec<Building> = geos
        .iter()
        .zip(parts)
        .map(|(g, p)| {
            let mut walls = p.outline();
            walls.dedup_by(|x, y| x.distance(*y) < 1e-9);
            if walls.len() > 1 && walls[0].distance(*walls.last().unwrap()) < 1e-9 {
                walls.pop();
            }
            Building {
                walls,
                faces: g.faces.iter().map(|f| (plan(f), Plane::of(f))).collect(),
            }
        })
        .collect();
    let r = bs
        .iter()
        .flat_map(|b| b.faces.iter().flat_map(|(p, _)| p.iter()))
        .fold(0.0f64, |m, p| m.max(p.x.abs()).max(p.y.abs()))
        * 4.0
        + 10.0;
    let hidden = |i: usize, p: Point2, z: f64| {
        bs.iter()
            .enumerate()
            .any(|(j, b)| j != i && hides(j, b, i, p, z))
    };

    // All plan polygons and planes, to split lines at.
    let all_polys: Vec<&[Point2]> = bs
        .iter()
        .flat_map(|b| {
            std::iter::once(b.walls.as_slice()).chain(b.faces.iter().map(|(p, _)| p.as_slice()))
        })
        .collect();
    let all_planes: Vec<Plane> = bs
        .iter()
        .flat_map(|b| b.faces.iter().map(|(_, pl)| *pl))
        .collect();

    // Faces: drop what other buildings hide.
    let mut new_faces: Vec<Vec<RoofFace>> = Vec::with_capacity(geos.len());
    for (i, g) in geos.iter().enumerate() {
        let mut faces = Vec::new();
        for f in &g.faces {
            let fp = Plane::of(f);
            let mut vis = MultiPolygon::new(vec![to_geo(&plan(f))]);
            let mut touched = false;
            for (j, b) in bs.iter().enumerate() {
                if j == i {
                    continue;
                }
                let walls = MultiPolygon::new(vec![to_geo(&b.walls)]);
                for (gp, gpl) in &b.faces {
                    let higher = if gpl.same(&fp) {
                        if j < i {
                            MultiPolygon::new(vec![to_geo(gp)])
                        } else {
                            continue;
                        }
                    } else {
                        let hp = half_plane(gpl.a - fp.a, gpl.b - fp.b, gpl.c - fp.c, r);
                        if hp.len() < 3 {
                            continue;
                        }
                        to_geo(gp).intersection(&to_geo(&hp))
                    };
                    let cut = if gpl.same(&fp) {
                        higher
                    } else {
                        higher.intersection(&walls)
                    };
                    if cut.unsigned_area() > 1e-12 {
                        vis = vis.difference(&cut);
                        touched = true;
                    }
                }
            }
            if !touched {
                faces.push(f.clone());
                continue;
            }
            let total = vis.unsigned_area();
            if total < 1e-9 {
                continue;
            }
            let share = if f.area > 0.0 {
                f.deducted / (f.area + f.deducted)
            } else {
                0.0
            };
            for piece in vis.0.iter() {
                let a = piece.unsigned_area();
                if a < 1e-9 {
                    continue;
                }
                let poly = from_geo(piece);
                let gross = a * fp.sec();
                faces.push(RoofFace {
                    edge: f.edge,
                    pitch_deg: f.pitch_deg,
                    polygon: poly.iter().map(|p| [p.x, p.y, fp.z(*p)]).collect(),
                    area: gross * (1.0 - share),
                    deducted: gross * share,
                    material: f.material,
                });
            }
        }
        new_faces.push(faces);
    }

    // Own lines where they show.
    let mut new_lines: Vec<Vec<RoofLine>> = Vec::with_capacity(geos.len());
    let mut gorges: Vec<RoofLine> = Vec::new();
    for (i, g) in geos.iter_mut().enumerate() {
        let mut kept = Vec::new();
        for l in std::mem::take(&mut g.lines) {
            let ts = splits(l.a, l.b, &all_polys, &all_planes);
            let d = unit(Point2::new(l.b[0] - l.a[0], l.b[1] - l.a[1]));
            let nrm = Point2::new(-d.y, d.x);
            // A line between two roofs running on in one plane is no line
            // (DACAD case C61).
            let seam = |m: Point2, z: f64| {
                let planes = |k: f64| -> Vec<(usize, Plane)> {
                    let o = Point2::new(m.x + nrm.x * k, m.y + nrm.y * k);
                    bs.iter()
                        .enumerate()
                        .filter_map(|(j, b)| b.plane_at(o).map(|pl| (j, pl)))
                        .filter(|(_, pl)| (pl.z(m) - z).abs() < 1e-6)
                        .collect()
                };
                let (p1, p2) = (planes(1e-4), planes(-1e-4));
                p1.iter()
                    .any(|(j, x)| p2.iter().any(|(k, y)| j != k && x.same(y)))
            };
            let parts = keep_parts(l.a, l.b, &ts, |p, z| !hidden(i, p, z) && !seam(p, z));
            let shown: f64 = parts.iter().map(|(a, b)| len3(*a, *b)).sum();
            if l.length() - shown > 1e-9 {
                g.cut_lines
                    .push((l.dacad_code().to_string(), l.length() - shown));
            }
            // Where two roofs in one plane share an eave and the narrower
            // one reaches at least as far up as the wider, DACAD drops the
            // shared eave (cases C63-C65; not C60, C66).
            let cancel = |m: Point2, z: f64| {
                let faces_at = |b: &Building| -> Vec<(Vec<Point2>, Plane)> {
                    [1e-4, -1e-4]
                        .iter()
                        .filter_map(|&k| {
                            let o = Point2::new(m.x + nrm.x * k, m.y + nrm.y * k);
                            b.faces.iter().find(|(f, _)| covers(f, o)).cloned()
                        })
                        .filter(|(_, pl)| (pl.z(m) - z).abs() < 1e-6)
                        .collect()
                };
                let span = |f: &[Point2], v: Point2| {
                    f.iter()
                        .map(|p| p.x * v.x + p.y * v.y)
                        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), t| {
                            (lo.min(t), hi.max(t))
                        })
                };
                let own = faces_at(&bs[i]);
                bs.iter().enumerate().any(|(k, b)| {
                    k != i
                        && faces_at(b).iter().any(|(g, gpl)| {
                            own.iter().any(|(f, fpl)| {
                                if !fpl.same(gpl) {
                                    return false;
                                }
                                let up = unit(Point2::new(fpl.a, fpl.b));
                                let (wf, wg) = (span(f, d), span(g, d));
                                let (rf, rg) = (span(f, up).1, span(g, up).1);
                                if wf.1 - wf.0 < wg.1 - wg.0 {
                                    rf >= rg - EPS
                                } else {
                                    rg >= rf - EPS
                                }
                            })
                        })
                })
            };
            for (a, b) in parts {
                if l.kind != LineKind::Eave {
                    kept.push(RoofLine { a, b, ..l.clone() });
                    continue;
                }
                // An eave where another roof goes on at the same height is
                // the gutter between the two: a Schlucht S, counted once
                // (DACAD case C48).
                let d = unit(Point2::new(b[0] - a[0], b[1] - a[1]));
                let nrm = Point2::new(-d.y, d.x);
                let shared = |m: Point2, z: f64| {
                    [1e-4, -1e-4].iter().any(|&k| {
                        let o = Point2::new(m.x + nrm.x * k, m.y + nrm.y * k);
                        !bs[i].faces.iter().any(|(f, _)| covers(f, o))
                            && bs.iter().enumerate().any(|(j, bj)| {
                                j != i
                                    && bj.plane_at(o).is_some_and(|pl| (pl.z(m) - z).abs() < SNAP)
                            })
                    })
                };
                let ts = splits(a, b, &all_polys, &[]);
                for (p, q) in keep_parts(a, b, &ts, |m, z| !shared(m, z) && !cancel(m, z)) {
                    kept.push(RoofLine {
                        a: p,
                        b: q,
                        ..l.clone()
                    });
                }
                for (p, q) in keep_parts(a, b, &ts, |m, z| shared(m, z) && !cancel(m, z)) {
                    gorges.push(RoofLine {
                        kind: LineKind::Gorge,
                        a: p,
                        b: q,
                        side: None,
                    });
                }
            }
        }
        new_lines.push(kept);
    }
    for l in gorges {
        crate::raised::push_once(&mut new_lines[0], l);
    }

    // Where two roofs cross.
    for i in 0..bs.len() {
        for j in i + 1..bs.len() {
            for (fp, fpl) in &bs[i].faces {
                for (gp, gpl) in &bs[j].faces {
                    let (da, db, dc) = (fpl.a - gpl.a, fpl.b - gpl.b, fpl.c - gpl.c);
                    let h = da.hypot(db);
                    if h < 1e-9 {
                        continue; // parallel or the same plane
                    }
                    // The plan line da x + db y + dc = 0, long enough.
                    let o = Point2::new(-da * dc / (h * h), -db * dc / (h * h));
                    let dir = Point2::new(-db / h, da / h);
                    let pa = Point2::new(o.x - dir.x * r, o.y - dir.y * r);
                    let pb = Point2::new(o.x + dir.x * r, o.y + dir.y * r);
                    let (a, b) = ([pa.x, pa.y, fpl.z(pa)], [pb.x, pb.y, fpl.z(pb)]);
                    let ts = splits(a, b, &all_polys, &all_planes);
                    let segs = keep_parts(a, b, &ts, |p, z| {
                        covers(fp, p)
                            && covers(gp, p)
                            && (strictly_inside(&bs[i].walls, p)
                                || strictly_inside(&bs[j].walls, p))
                            && bs[i].plane_at(p) == Some(*fpl)
                            && bs[j].plane_at(p) == Some(*gpl)
                            && !bs
                                .iter()
                                .enumerate()
                                .any(|(k, bk)| k != i && k != j && hides(k, bk, usize::MAX, p, z))
                    });
                    let (ui, uj) = (
                        unit(Point2::new(fpl.a, fpl.b)),
                        unit(Point2::new(gpl.a, gpl.b)),
                    );
                    let same_way = ui.x * uj.x + ui.y * uj.y > SAME_WAY;
                    for (p, q) in segs {
                        if len3(p, q) < 1e-6 {
                            continue;
                        }
                        // Flat as in the roof engine: a rise under 0.5% of
                        // the run is float noise (TRAING09).
                        let run = (p[0] - q[0]).hypot(p[1] - q[1]);
                        let flat = (p[2] - q[2]).abs() < 1e-6f64.max(0.005 * run);
                        let kind = match (same_way, flat) {
                            (true, true) => LineKind::Knick,
                            (false, true) => LineKind::Gorge,
                            _ => LineKind::Valley,
                        };
                        new_lines[i].push(RoofLine {
                            kind,
                            a: p,
                            b: q,
                            side: None,
                        });
                    }
                }
            }
        }
    }

    // A lower roof against a higher building's wall.
    for (j, bj) in bs.iter().enumerate() {
        let n = bj.walls.len();
        for e in 0..n {
            let (wa, wb) = (bj.walls[e], bj.walls[(e + 1) % n]);
            let len = wa.distance(wb);
            if len < 1e-6 {
                continue;
            }
            let dir = Point2::new((wb.x - wa.x) / len, (wb.y - wa.y) / len);
            let out = Point2::new(dir.y, -dir.x);
            for (i, bi) in bs.iter().enumerate() {
                if i == j {
                    continue;
                }
                for (fp, fpl) in &bi.faces {
                    let (a, b) = ([wa.x, wa.y, fpl.z(wa)], [wb.x, wb.y, fpl.z(wb)]);
                    let ts = splits(a, b, &all_polys, &all_planes);
                    let segs = keep_parts(a, b, &ts, |p, z| {
                        let o = Point2::new(p.x + out.x * 1e-4, p.y + out.y * 1e-4);
                        let inn = Point2::new(p.x - out.x * 1e-4, p.y - out.y * 1e-4);
                        // A roof that ends at the wall with its own gable
                        // keeps its verges and pult there (TRAING03).
                        covers(fp, p)
                            && covers(fp, inn)
                            && bi.plane_at(o) == Some(*fpl)
                            && bj.roof_z(p).is_some_and(|zj| zj > z + EPS)
                            && !hidden(i, o, fpl.z(o))
                    });
                    let up = unit(Point2::new(fpl.a, fpl.b));
                    let facing = out.x * up.x + out.y * up.y;
                    let kind = if facing < -SAME_WAY {
                        LineKind::AbutmentFront
                    } else if facing > SAME_WAY {
                        LineKind::AbutmentBack
                    } else {
                        LineKind::AbutmentSide
                    };
                    for (p, q) in segs {
                        if len3(p, q) < 1e-6 {
                            continue;
                        }
                        new_lines[i].push(RoofLine {
                            kind,
                            a: p,
                            b: q,
                            side: None,
                        });
                    }
                }
            }
        }
    }

    // A line an earlier roof already has is counted once (C60).
    let mut seen: Vec<RoofLine> = Vec::new();
    for lines in new_lines.iter_mut() {
        let mut own = Vec::new();
        for l in std::mem::take(lines) {
            let before = seen.len();
            crate::raised::push_once(&mut seen, l);
            own.extend(seen[before..].iter().cloned());
        }
        *lines = own;
    }
    for ((g, faces), lines) in geos.iter_mut().zip(new_faces).zip(new_lines) {
        g.faces = faces;
        g.lines = lines;
    }
}

pub(crate) fn unit(v: Point2) -> Point2 {
    let l = v.x.hypot(v.y).max(1e-12);
    Point2::new(v.x / l, v.y / l)
}
