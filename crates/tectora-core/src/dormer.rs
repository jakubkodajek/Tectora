//! Dormers: a small roof of its own that cuts into a host roof face.
//!
//! DACAD stores a dormer as an open wall chain with its own roof number:
//! the front wall(s) plus zero-length side walls whose direction says where
//! the cheeks run. Checked against roofs calculated by DACAD itself (cases
//! C29–C31):
//!
//! * The dormer roof is generated like any roof, with its side walls
//!   extended far back and closed by a gable, then kept only where it lies
//!   above the plane of the host face under its front. That plane counts
//!   beyond the face, e.g. past the ridge (C42).
//! * The host roof loses the area under the dormer inside its walls, on
//!   every face (C42), but not under the dormer's overhang (C40).
//! * Where the dormer planes leave the host plane they make valleys K, or a
//!   Knick line c when the two planes are parallel (shed dormer).
//! * The dormer's walls meet the host plane in wall-abutment lines: WS along
//!   the cheeks, WV below a front wall set back from the eave. A front on the
//!   host's eave removes that stretch of eave instead; other host lines under
//!   the dormer stay (C41, C42).

use crate::geom::{Point2, signed_area};
use crate::model::{Dormer, EdgeDef, GABLE_PITCH_DEG, RoofPart};
use crate::openings::{clip_polygon, inside, lerp3, plan, slope};
use crate::roof::{LineKind, Opening, OpeningKind, RoofFace, RoofGeometry, RoofLine, generate};

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
        clad: false,
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

    // Seeds: points just behind each real dormer wall.
    let mut seeds: Vec<Point2> = Vec::new();
    for (s, e, _) in walls(d) {
        if s.distance(e) < 1e-6 {
            continue;
        }
        let dir = unit(Point2::new(e.x - s.x, e.y - s.y));
        seeds.push(Point2::new(
            (s.x + e.x) / 2.0 - dir.y * 1e-3,
            (s.y + e.y) / 2.0 + dir.x * 1e-3,
        ));
    }
    let Some(home) = geos.iter().position(|g| {
        g.faces
            .iter()
            .any(|f| seeds.iter().any(|&m| inside(&plan(f), m)))
    }) else {
        return;
    };
    // The host loses roof only inside the dormer's walls, not under its
    // overhang (DACAD case C40).
    let walls_poly = part.outline();

    // Candidate pieces: each dormer face where it is above the host plane.
    struct Candidate {
        piece: Piece,
        host: (usize, usize),
        /// Plan region inside the dormer walls covered by this piece.
        under: Vec<Point2>,
        chord: Option<RoofLine>,
    }
    let mut cands: Vec<Candidate> = Vec::new();
    for df in &roof.faces {
        let dp = Plane::of(df);
        let dpoly = plan(df);
        for (gi, g) in geos.iter().enumerate() {
            for (fi, hf) in g.faces.iter().enumerate() {
                if !seeds.iter().any(|&m| inside(&plan(hf), m)) {
                    continue;
                }
                let hp = Plane::of(hf);
                let above = clip_half(&dpoly, dp.a - hp.a, dp.b - hp.b, dp.c - hp.c);
                if above.len() < 3 || signed_area(&above).abs() < 1e-9 {
                    continue;
                }
                let under = clip_polygon(&above, &walls_poly);
                // The edge of the piece where the planes meet.
                let on: Vec<Point2> = above
                    .iter()
                    .copied()
                    .filter(|&p| (dp.z(p) - hp.z(p)).abs() < 1e-6)
                    .collect();
                let mut chord = None;
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
                    chord = Some(RoofLine {
                        kind,
                        a: [a.x, a.y, za],
                        b: [b.x, b.y, zb],
                        side: None,
                    });
                }
                cands.push(Candidate {
                    piece: Piece {
                        poly: above,
                        plane: dp,
                        pitch_deg: df.pitch_deg,
                    },
                    host: (gi, fi),
                    under,
                    chord,
                });
            }
        }
    }
    // Keep the pieces connected to the dormer's front: further back the
    // dormer may come out of the roof again, which DACAD does not do.
    let touches = |a: &Piece, b: &Piece| {
        a.poly.iter().any(|&p| covers(&b.poly, p)) || b.poly.iter().any(|&p| covers(&a.poly, p))
    };
    let mut keep: Vec<bool> = cands
        .iter()
        .map(|c| seeds.iter().any(|&m| covers(&c.piece.poly, m)))
        .collect();
    loop {
        let mut grew = false;
        for i in 0..cands.len() {
            if !keep[i]
                && (0..cands.len()).any(|j| keep[j] && touches(&cands[i].piece, &cands[j].piece))
            {
                keep[i] = true;
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    let mut pieces: Vec<Piece> = Vec::new();
    let mut chords: Vec<RoofLine> = Vec::new();
    let mut hosts: Vec<(usize, usize)> = Vec::new();
    let mut removed = 0.0;
    for (c, k) in cands.into_iter().zip(keep) {
        if !k {
            continue;
        }
        // Every face of the host roof loses what lies under the dormer
        // (DACAD case C42: a dormer through the ridge also cuts the far face).
        if c.under.len() >= 3 {
            for f in &mut geos[c.host.0].faces {
                let cut = signed_area(&clip_polygon(&plan(f), &c.under)).abs();
                if cut > 1e-12 {
                    f.area -= cut * Plane::of(f).sec();
                    removed += cut * Plane::of(f).sec();
                }
            }
        }
        if !hosts.contains(&c.host) {
            hosts.push(c.host);
        }
        chords.extend(c.chord);
        pieces.push(c.piece);
    }
    let piece_polys: Vec<&[Point2]> = pieces.iter().map(|p| p.poly.as_slice()).collect();

    // A host line under a dormer wall disappears (a front on the eave, C29);
    // lines merely covered by the dormer stay (C41, C42).
    let wall_segs: Vec<(Point2, Point2)> = walls(d)
        .into_iter()
        .filter(|(s, e, _)| s.distance(*e) > 1e-6)
        .map(|(s, e, _)| (s, e))
        .collect();
    let on_wall = |p: Point2| {
        wall_segs.iter().any(|&(a, b)| {
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let len2 = dx * dx + dy * dy;
            let t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / len2;
            (-1e-9..=1.0 + 1e-9).contains(&t)
                && Point2::new(a.x + t * dx, a.y + t * dy).distance(p) < 1e-6
        })
    };
    for g in geos.iter_mut() {
        let mut kept = Vec::new();
        for l in std::mem::take(&mut g.lines) {
            let mut left = l.length();
            for (a, b) in split_keep(l.a, l.b, &piece_polys, |p, z| {
                !(on_wall(p)
                    && pieces
                        .iter()
                        .any(|pc| covers(&pc.poly, p) && pc.plane.z(p) > z + 1e-6))
            }) {
                left -= dist3(a, b);
                kept.push(RoofLine { a, b, ..l.clone() });
            }
            if left > 1e-9 {
                g.cut_lines.push((l.dacad_code().to_string(), left));
            }
        }
        g.lines = kept;
    }

    // A dormer whose eave lies on the host face: the lower edge of a clipped
    // gable above it is a Knick line c, not an eave (DACAD cases C76-C78).
    let eave = walls(d)
        .iter()
        .map(|w| w.2.eave_height)
        .fold(f64::INFINITY, f64::min);
    let on_face = wall_segs.iter().any(|&(a, _)| {
        geos[home]
            .faces
            .iter()
            .any(|f| covers(&plan(f), a) && (Plane::of(f).z(a) - eave).abs() < 2e-3)
    });
    // Dormer lines where the dormer shows.
    let mut lines: Vec<RoofLine> = Vec::new();
    for l in &roof.lines {
        let kink = walls(d).iter().any(|w| {
            w.2.half_hip.as_ref().is_some_and(|h| {
                (l.a[2] - h.height).abs() < 1e-6 && (l.b[2] - h.height).abs() < 1e-6
            })
        });
        let kind = if on_face && l.kind == LineKind::Eave && kink {
            LineKind::Knick
        } else {
            l.kind
        };
        for (a, b) in split_keep(l.a, l.b, &piece_polys, |p, _| {
            pieces.iter().any(|pc| covers(&pc.poly, p))
        }) {
            lines.push(RoofLine {
                kind,
                a,
                b,
                ..l.clone()
            });
        }
    }
    lines.extend(chords);

    // Wall abutments: dormer walls standing on a host face.
    let host_planes: Vec<(Plane, Point2)> = hosts
        .iter()
        .map(|&(gi, fi)| {
            let f = &geos[gi].faces[fi];
            (Plane::of(f), slope(f).0)
        })
        .collect();
    // The host plane counts beyond its face (DACAD case C42), but only over
    // the host roof.
    let home_polys: Vec<Vec<Point2>> = geos[home].faces.iter().map(plan).collect();
    let mut cuts: Vec<&[Point2]> = piece_polys.clone();
    cuts.extend(home_polys.iter().map(Vec::as_slice));
    let n = part.edges.len();
    // A front leaning outward (pitch over 90°) stands on the host with its
    // eave and abuts nothing (DACAD case C71).
    let leaning = |i: usize| part.edges[i].pitch_deg > 90.0 + 1e-9;
    for i in (0..n).filter(|&i| i != back && !leaning(i)) {
        let (a, b) = (part.edges[i].start, part.edges[(i + 1) % n].start);
        let dir = unit(Point2::new(b.x - a.x, b.y - a.y));
        let out = Point2::new(dir.y, -dir.x);
        for (hp, up) in &host_planes {
            let segs = split_keep([a.x, a.y, hp.z(a)], [b.x, b.y, hp.z(b)], &cuts, |p, _| {
                let outside = Point2::new(p.x + out.x * 1e-4, p.y + out.y * 1e-4);
                home_polys.iter().any(|f| covers(f, p))
                    && home_polys.iter().any(|f| inside(f, outside))
                    && pieces.iter().any(|pc| covers(&pc.poly, p))
            });
            // Level within float noise of the host plane (TRAING06).
            let level = (dir.x * up.x + dir.y * up.y).abs() < 1e-4;
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
    target.openings.push(Opening {
        kind: OpeningKind::Dormer,
        area: removed,
    });
}
