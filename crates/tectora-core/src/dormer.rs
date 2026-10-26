//! Dormers: a small roof of its own that cuts into a host roof face.
//!
//! DACAD stores a dormer as an open wall chain with its own roof number:
//! the front wall(s) plus zero-length side walls whose direction says where
//! the cheeks run. Checked against roofs calculated by DACAD itself (cases
//! C29–C31):
//!
//! * The dormer roof is generated like any roof, with its side walls
//!   extended far back and closed by a gable, then kept only where it lies
//!   above the host face it sits on. The host face loses that area.
//! * Where the dormer planes leave the host face they make valleys K, or a
//!   Knick line c when the two planes are parallel (shed dormer).
//! * The dormer's walls meet the host face in wall-abutment lines: WS along
//!   the cheeks, WV below a front wall set back from the eave. A front on the
//!   host's eave removes that stretch of eave instead.

use crate::geom::{Point2, signed_area};
use crate::model::{Dormer, EdgeDef, GABLE_PITCH_DEG, RoofPart};
use crate::openings::{clip_polygon, inside, lerp3, plan, slope};
use crate::roof::{LineKind, RoofFace, RoofGeometry, RoofLine, generate};

const TOL: f64 = 1e-7;

/// `z = a x + b y + c`.
#[derive(Debug, Clone, Copy)]
struct Plane {
    a: f64,
    b: f64,
    c: f64,
}

impl Plane {
    fn of(face: &RoofFace) -> Self {
        let (up, tan) = slope(face);
        let o = face.polygon[0];
        let (a, b) = (up.x * tan, up.y * tan);
        Plane {
            a,
            b,
            c: o[2] - a * o[0] - b * o[1],
        }
    }

    fn z(&self, p: Point2) -> f64 {
        self.a * p.x + self.b * p.y + self.c
    }

    fn sec(&self) -> f64 {
        (1.0 + self.a * self.a + self.b * self.b).sqrt()
    }
}

/// A visible part of a dormer face above its host face.
struct Piece {
    poly: Vec<Point2>,
    plane: Plane,
    pitch_deg: f64,
}

/// `poly` clipped to `a x + b y + c >= 0`.
fn clip_half(poly: &[Point2], a: f64, b: f64, c: f64) -> Vec<Point2> {
    let f = |p: Point2| a * p.x + b * p.y + c;
    let mut out = Vec::new();
    let n = poly.len();
    for k in 0..n {
        let (p, q) = (poly[k], poly[(k + 1) % n]);
        let (fp, fq) = (f(p), f(q));
        if fp >= -TOL {
            out.push(p);
        }
        if (fp >= -TOL) != (fq >= -TOL) {
            let t = fp / (fp - fq);
            out.push(Point2::new(p.x + t * (q.x - p.x), p.y + t * (q.y - p.y)));
        }
    }
    out
}

/// Inside or on the boundary of `poly`.
fn covers(poly: &[Point2], p: Point2) -> bool {
    if inside(poly, p) {
        return true;
    }
    let n = poly.len();
    (0..n).any(|i| {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let len2 = dx * dx + dy * dy;
        if len2 < 1e-18 {
            return a.distance(p) < 1e-6;
        }
        let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0);
        Point2::new(a.x + t * dx, a.y + t * dy).distance(p) < 1e-6
    })
}

/// Splits segment `a`–`b` at every crossing with the polygons' edges and
/// returns the sub-segments whose midpoint satisfies `keep`.
fn split_keep(
    a: [f64; 3],
    b: [f64; 3],
    polys: &[&[Point2]],
    keep: impl Fn(Point2, f64) -> bool,
) -> Vec<([f64; 3], [f64; 3])> {
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
    ts.sort_by(f64::total_cmp);
    let mut out: Vec<([f64; 3], [f64; 3])> = Vec::new();
    for s in ts.windows(2) {
        if s[1] - s[0] < 1e-9 {
            continue;
        }
        let m = lerp3(a, b, (s[0] + s[1]) / 2.0);
        if keep(Point2::new(m[0], m[1]), m[2]) {
            let (x, y) = (lerp3(a, b, s[0]), lerp3(a, b, s[1]));
            // Join with the previous kept piece when they touch.
            match out.last_mut() {
                Some(last) if dist3(last.1, x) < 1e-9 => last.1 = y,
                _ => out.push((x, y)),
            }
        }
    }
    out
}

fn dist3(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn unit(v: Point2) -> Point2 {
    let l = v.x.hypot(v.y).max(1e-12);
    Point2::new(v.x / l, v.y / l)
}

/// The dormer's walls as `(start, end, definition)`, with zero-length side
/// walls oriented by their direction.
fn walls(d: &Dormer) -> Vec<(Point2, Point2, &EdgeDef)> {
    let n = d.edges.len();
    (0..n)
        .map(|i| {
            let end = d.edges.get(i + 1).map_or(d.end, |e| e.start);
            (d.edges[i].start, end, &d.edges[i])
        })
        .collect()
}

fn direction(start: Point2, end: Point2, e: &EdgeDef) -> Point2 {
    if start.distance(end) > 1e-6 {
        unit(Point2::new(end.x - start.x, end.y - start.y))
    } else {
        let a = e.direction_rad.unwrap_or(0.0);
        Point2::new(a.cos(), a.sin())
    }
}

/// Closes the dormer's open chain: the first and last walls run back by
/// `reach` metres and a gable joins them.
fn footprint(d: &Dormer, reach: f64) -> Option<(RoofPart, usize)> {
    let ws = walls(d);
    if ws.len() < 2 {
        return None;
    }
    let (s0, e0, d0) = ws[0];
    let (sn, en, dn) = ws[ws.len() - 1];
    let dir0 = direction(s0, e0, d0);
    let dirn = direction(sn, en, dn);
    let mut edges: Vec<EdgeDef> = Vec::new();
    let mut first = d0.clone();
    first.start = Point2::new(e0.x - dir0.x * reach, e0.y - dir0.y * reach);
    first.direction_rad = None;
    edges.push(first);
    for &(s, e, def) in &ws[1..ws.len() - 1] {
        if s.distance(e) > 1e-6 {
            let mut x = def.clone();
            x.direction_rad = None;
            edges.push(x);
        }
    }
    let mut last = dn.clone();
    last.start = sn;
    last.direction_rad = None;
    edges.push(last);
    let back = edges.len();
    edges.push(EdgeDef {
        start: Point2::new(sn.x + dirn.x * reach, sn.y + dirn.y * reach),
        pitch_deg: GABLE_PITCH_DEG.ceil(),
        eave_height: d0.eave_height,
        overhang: 0.0,
        half_hip: None,
        direction_rad: None,
        breaks: Vec::new(),
        priority: 0,
    });
    let mut part = RoofPart {
        name: "dormer".into(),
        edges,
    };
    if signed_area(&part.outline()) < 0.0 {
        return None; // walls not oriented with the roof on their left
    }
    part.normalize_ccw();
    Some((part, back))
}

pub fn apply_dormer(geos: &mut [RoofGeometry], d: &Dormer) {
    // Far enough to pass any host face.
    let reach = geos
        .iter()
        .flat_map(|g| g.faces.iter().flat_map(|f| f.polygon.iter()))
        .fold(0.0f64, |m, p| m.max(p[0].abs()).max(p[1].abs()))
        * 4.0
        + 10.0;
    let Some((part, back)) = footprint(d, reach) else {
        return;
    };
    let roof = generate(&part);

    // Host faces: those under a point just behind each real dormer wall.
    let mut hosts: Vec<(usize, usize)> = Vec::new();
    for (s, e, _) in walls(d) {
        if s.distance(e) < 1e-6 {
            continue;
        }
        let dir = unit(Point2::new(e.x - s.x, e.y - s.y));
        let m = Point2::new(
            (s.x + e.x) / 2.0 - dir.y * 1e-3,
            (s.y + e.y) / 2.0 + dir.x * 1e-3,
        );
        for (gi, g) in geos.iter().enumerate() {
            for (fi, f) in g.faces.iter().enumerate() {
                if inside(&plan(f), m) && !hosts.contains(&(gi, fi)) {
                    hosts.push((gi, fi));
                }
            }
        }
    }
    let Some(&(home, _)) = hosts.first() else {
        return;
    };

    // Visible pieces: dormer face ∩ host face, where the dormer is higher.
    let mut pieces: Vec<Piece> = Vec::new();
    let mut chords: Vec<RoofLine> = Vec::new();
    for df in &roof.faces {
        let dp = Plane::of(df);
        let dpoly = plan(df);
        for &(gi, fi) in &hosts {
            let hf = &geos[gi].faces[fi];
            let hp = Plane::of(hf);
            let common = clip_polygon(&plan(hf), &dpoly);
            if common.len() < 3 {
                continue;
            }
            let above = clip_half(&common, dp.a - hp.a, dp.b - hp.b, dp.c - hp.c);
            let area = signed_area(&above).abs();
            if above.len() < 3 || area < 1e-9 {
                continue;
            }
            geos[gi].faces[fi].area -= area * hp.sec();
            // The edge of the piece where the planes meet.
            let on: Vec<Point2> = above
                .iter()
                .copied()
                .filter(|&p| (dp.z(p) - hp.z(p)).abs() < 1e-6)
                .collect();
            if on.len() >= 2 {
                let (mut i0, mut i1, mut best) = (0, 0, 0.0);
                for i in 0..on.len() {
                    for j in i + 1..on.len() {
                        let dd = on[i].distance(on[j]);
                        if dd > best {
                            (i0, i1, best) = (i, j, dd);
                        }
                    }
                }
                if best > 1e-6 {
                    let (a, b) = (on[i0], on[i1]);
                    let (za, zb) = (dp.z(a), dp.z(b));
                    let kind = if (za - zb).abs() < 1e-6 {
                        LineKind::Knick
                    } else {
                        LineKind::Valley
                    };
                    chords.push(RoofLine {
                        kind,
                        a: [a.x, a.y, za],
                        b: [b.x, b.y, zb],
                        side: None,
                    });
                }
            }
            pieces.push(Piece {
                poly: above,
                plane: dp,
                pitch_deg: df.pitch_deg,
            });
        }
    }
    let piece_polys: Vec<&[Point2]> = pieces.iter().map(|p| p.poly.as_slice()).collect();

    // Host lines under the dormer disappear.
    for g in geos.iter_mut() {
        let mut kept = Vec::new();
        for l in std::mem::take(&mut g.lines) {
            for (a, b) in split_keep(l.a, l.b, &piece_polys, |p, z| {
                !pieces
                    .iter()
                    .any(|pc| covers(&pc.poly, p) && pc.plane.z(p) > z + 1e-6)
            }) {
                kept.push(RoofLine { a, b, ..l.clone() });
            }
        }
        g.lines = kept;
    }

    // Dormer lines where the dormer shows.
    let mut lines: Vec<RoofLine> = Vec::new();
    for l in &roof.lines {
        for (a, b) in split_keep(l.a, l.b, &piece_polys, |p, _| {
            pieces.iter().any(|pc| covers(&pc.poly, p))
        }) {
            lines.push(RoofLine { a, b, ..l.clone() });
        }
    }
    lines.extend(chords);

    // Wall abutments: dormer walls standing on a host face.
    let host_polys: Vec<(Vec<Point2>, Plane, Point2)> = hosts
        .iter()
        .map(|&(gi, fi)| {
            let f = &geos[gi].faces[fi];
            (plan(f), Plane::of(f), slope(f).0)
        })
        .collect();
    let mut cuts: Vec<&[Point2]> = piece_polys.clone();
    cuts.extend(host_polys.iter().map(|h| h.0.as_slice()));
    let n = part.edges.len();
    for i in (0..n).filter(|&i| i != back) {
        let (a, b) = (part.edges[i].start, part.edges[(i + 1) % n].start);
        let dir = unit(Point2::new(b.x - a.x, b.y - a.y));
        let out = Point2::new(dir.y, -dir.x);
        for (hpoly, hp, up) in &host_polys {
            let segs = split_keep([a.x, a.y, hp.z(a)], [b.x, b.y, hp.z(b)], &cuts, |p, _| {
                let outside = Point2::new(p.x + out.x * 1e-4, p.y + out.y * 1e-4);
                covers(hpoly, p)
                    && inside(hpoly, outside)
                    && pieces.iter().any(|pc| covers(&pc.poly, p))
            });
            let level = (dir.x * up.x + dir.y * up.y).abs() < 1e-6;
            let facing = out.x * up.x + out.y * up.y;
            let kind = match (level, facing < 0.0) {
                (true, true) => LineKind::AbutmentFront,
                (true, false) => LineKind::AbutmentBack,
                _ => LineKind::AbutmentSide,
            };
            for (p, q) in segs {
                lines.push(RoofLine {
                    kind,
                    a: p,
                    b: q,
                    side: None,
                });
            }
        }
    }

    // Dormer faces: one per visible piece.
    let target = &mut geos[home];
    for pc in &pieces {
        target.faces.push(RoofFace {
            edge: usize::MAX,
            pitch_deg: pc.pitch_deg,
            polygon: pc.poly.iter().map(|p| [p.x, p.y, pc.plane.z(*p)]).collect(),
            area: signed_area(&pc.poly).abs() * pc.plane.sec(),
            deducted: 0.0,
        });
    }
    target.lines.extend(lines);
}
