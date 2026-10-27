//! Combining the two generations of a part with raised walls (see
//! [`crate::roof::generate`]).
//!
//! The roof with the raised walls at their wall lines is the body: it is
//! shown in full. The roof with every wall at its overhang line adds the
//! overhang that floats: the parts of it outside the body's outline or above
//! the body's roof. A line is kept where it borders a shown face, that is
//! where a plane it lies on is shown on one side of it only; lines that both
//! roofs share are counted once.

use geo::{Area, BooleanOps, MultiPolygon};

use crate::geom::Point2;
use crate::openings::plan;
use crate::roof::{RoofFace, RoofGeometry, RoofLine};
use crate::union::{
    EPS, Plane, covers, from_geo, half_plane, keep_parts, len3, splits, to_geo, unit,
};

/// A shown face: plan outline and plane.
struct Shown {
    poly: Vec<Point2>,
    plane: Plane,
}

pub fn combine(off: RoofGeometry, body: RoofGeometry) -> RoofGeometry {
    let r = off
        .faces
        .iter()
        .chain(&body.faces)
        .flat_map(|f| f.polygon.iter())
        .fold(0.0f64, |m, p| m.max(p[0].abs()).max(p[1].abs()))
        * 4.0
        + 10.0;
    let body_faces: Vec<(Vec<Point2>, Plane)> =
        body.faces.iter().map(|f| (plan(f), Plane::of(f))).collect();

    let mut geo = RoofGeometry {
        faces: body.faces.clone(),
        gables: body.gables.clone(),
        ..RoofGeometry::default()
    };
    let mut shown: Vec<Shown> = body_faces
        .iter()
        .map(|(p, pl)| Shown {
            poly: p.clone(),
            plane: *pl,
        })
        .collect();

    // Overhang faces where they are not at or below the body.
    for f in &off.faces {
        let fp = Plane::of(f);
        let mut vis = MultiPolygon::new(vec![to_geo(&plan(f))]);
        for (gp, gpl) in &body_faces {
            let cover = if gpl.same(&fp) {
                MultiPolygon::new(vec![to_geo(gp)])
            } else {
                let hp = half_plane(gpl.a - fp.a, gpl.b - fp.b, gpl.c - fp.c, r);
                if hp.len() < 3 {
                    continue;
                }
                to_geo(gp).intersection(&to_geo(&hp))
            };
            if cover.unsigned_area() > 1e-12 {
                vis = vis.difference(&cover);
            }
        }
        for piece in &vis.0 {
            let a = piece.unsigned_area();
            if a < 1e-9 {
                continue;
            }
            let poly = from_geo(piece);
            geo.faces.push(RoofFace {
                edge: f.edge,
                pitch_deg: f.pitch_deg,
                polygon: poly.iter().map(|p| [p.x, p.y, fp.z(*p)]).collect(),
                area: a * fp.sec(),
                deducted: 0.0,
            });
            shown.push(Shown { poly, plane: fp });
        }
    }

    let polys: Vec<&[Point2]> = shown.iter().map(|s| s.poly.as_slice()).collect();
    let planes: Vec<Plane> = shown.iter().map(|s| s.plane).collect();
    let planes_at = |p: Point2| -> Vec<Plane> {
        shown
            .iter()
            .filter(|s| covers(&s.poly, p))
            .map(|s| s.plane)
            .collect()
    };

    let mut kept: Vec<RoofLine> = Vec::new();
    for l in body.lines.iter().chain(&off.lines) {
        let d = unit(Point2::new(l.b[0] - l.a[0], l.b[1] - l.a[1]));
        let nrm = Point2::new(-d.y, d.x);
        let ts = splits(l.a, l.b, &polys, &planes);
        for (a, b) in keep_parts(l.a, l.b, &ts, |m, z| {
            let s1 = Point2::new(m.x + nrm.x * 1e-4, m.y + nrm.y * 1e-4);
            let s2 = Point2::new(m.x - nrm.x * 1e-4, m.y - nrm.y * 1e-4);
            let (p1, p2) = (planes_at(s1), planes_at(s2));
            let on = |pl: &Plane| (pl.z(m) - z).abs() < 1e-5;
            // The line must border a shown face: one it lies on that is on
            // one side only.
            let one_side =
                |a: &[Plane], b: &[Plane]| a.iter().any(|x| on(x) && !b.iter().any(|y| y.same(x)));
            one_side(&p1, &p2) || one_side(&p2, &p1)
        }) {
            push_once(&mut kept, RoofLine { a, b, ..l.clone() });
        }
    }
    geo.lines = kept;
    geo
}

/// Adds `l` without the stretches an existing line of the same code already
/// covers.
pub(crate) fn push_once(lines: &mut Vec<RoofLine>, l: RoofLine) {
    let len = len3(l.a, l.b);
    if len < 1e-6 {
        return;
    }
    let dir = [
        (l.b[0] - l.a[0]) / len,
        (l.b[1] - l.a[1]) / len,
        (l.b[2] - l.a[2]) / len,
    ];
    let along = |p: [f64; 3]| {
        let v = [p[0] - l.a[0], p[1] - l.a[1], p[2] - l.a[2]];
        let t = v[0] * dir[0] + v[1] * dir[1] + v[2] * dir[2];
        let off = [v[0] - t * dir[0], v[1] - t * dir[1], v[2] - t * dir[2]];
        (
            t,
            (off[0] * off[0] + off[1] * off[1] + off[2] * off[2]).sqrt(),
        )
    };
    let mut covered: Vec<(f64, f64)> = Vec::new();
    for o in lines.iter().filter(|o| o.dacad_code() == l.dacad_code()) {
        let ((ta, da), (tb, db)) = (along(o.a), along(o.b));
        if da < 1e-5 && db < 1e-5 {
            covered.push((ta.min(tb), ta.max(tb)));
        }
    }
    covered.sort_by(|x, y| x.0.total_cmp(&y.0));
    let mut t = 0.0;
    let at = |t: f64| {
        [
            l.a[0] + dir[0] * t,
            l.a[1] + dir[1] * t,
            l.a[2] + dir[2] * t,
        ]
    };
    let mut pieces = Vec::new();
    for (s, e) in covered {
        if s > t + EPS {
            pieces.push((t, s.min(len)));
        }
        t = t.max(e);
        if t >= len {
            break;
        }
    }
    if len > t + EPS {
        pieces.push((t, len));
    }
    for (s, e) in pieces {
        if e - s > 1e-6 {
            lines.push(RoofLine {
                a: at(s),
                b: at(e),
                ..l.clone()
            });
        }
    }
}
