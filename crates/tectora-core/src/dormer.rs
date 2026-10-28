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

    fn same(&self, o: &Plane) -> bool {
        (self.a - o.a).abs() < 1e-6 && (self.b - o.b).abs() < 1e-6 && (self.c - o.c).abs() < 1e-6
    }

    /// The same plane within float noise of the wall data.
    fn near(&self, o: &Plane) -> bool {
        (self.a - o.a).abs() < 1e-4 && (self.b - o.b).abs() < 1e-4 && (self.c - o.c).abs() < 1e-3
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

/// The part of segment `a b` where `f >= 0` (`f` linear along it).
fn clip_seg(a: Point2, b: Point2, f: impl Fn(Point2) -> f64) -> Option<(Point2, Point2)> {
    let (fa, fb) = (f(a), f(b));
    let at = |t: f64| Point2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
    match (fa >= -1e-9, fb >= -1e-9) {
        (true, true) => Some((a, b)),
        (false, false) => None,
        (true, false) => Some((a, at(fa / (fa - fb)))),
        (false, true) => Some((at(fa / (fa - fb)), b)),
    }
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

/// DACAD's test for an edge lying on another (FUN_00d99590): parallel unit
/// vectors and both ends inside the other's box, per component within
/// VALUE_EPS.
fn on_line(p: [f64; 3], q: [f64; 3], l: &RoofLine) -> bool {
    const EPS: f64 = 0.01;
    let unit3 = |a: [f64; 3], b: [f64; 3]| {
        let v = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        (n >= 1e-12).then(|| [v[0] / n, v[1] / n, v[2] / n])
    };
    let parallel = |u: [f64; 3], v: [f64; 3]| {
        (0..3).all(|i| (u[i] - v[i]).abs() < EPS) || (0..3).all(|i| (u[i] + v[i]).abs() < EPS)
    };
    let (Some(e2), Some(e1)) = (unit3(l.a, l.b), unit3(p, q)) else {
        return false;
    };
    if !parallel(e1, e2) {
        return false;
    }
    if let Some(w) = unit3(l.a, p)
        && dist3(l.a, p) >= EPS
        && !parallel(e2, w)
    {
        return false;
    }
    (0..3).all(|i| {
        let (lo, hi) = (l.a[i].min(l.b[i]), l.a[i].max(l.b[i]));
        [p[i], q[i]].iter().all(|&c| c > lo - EPS && c < hi + EPS)
    })
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

/// A plane of the host surface, in force where `lo <= n·p <= hi`.
#[derive(Clone, Copy)]
struct Level {
    plane: Plane,
    n: Point2,
    lo: f64,
    hi: f64,
}

impl Level {
    fn holds(&self, p: Point2) -> bool {
        let d = self.n.x * p.x + self.n.y * p.y;
        self.lo - 1e-9 <= d && d <= self.hi + 1e-9
    }

    /// `poly` cut to the band.
    fn clip(&self, poly: &[Point2]) -> Vec<Point2> {
        let mut out = poly.to_vec();
        if self.lo.is_finite() {
            out = clip_half(&out, self.n.x, self.n.y, -self.lo);
        }
        if self.hi.is_finite() {
            out = clip_half(&out, -self.n.x, -self.n.y, self.hi);
        }
        out
    }
}

/// The host surface of a main-roof wall (`def` from its start to `end`) for
/// a dormer whose front is at `at`: the roof plane of the wall's level there
/// and of the levels above it. DACAD closes the dormer with these alone (RD2
/// `T` record with its `K` breaks), wherever the dormer stands: lower levels
/// and other walls' faces do not count.
fn wall_levels(def: &EdgeDef, end: Point2, at: Point2) -> Vec<Level> {
    let dir = unit(Point2::new(end.x - def.start.x, end.y - def.start.y));
    let n = Point2::new(-dir.y, dir.x);
    let o = n.x * def.start.x + n.y * def.start.y;
    let dist = n.x * at.x + n.y * at.y - o;
    // Each level as (pitch, distance from the wall and height where it starts).
    let mut levels = vec![(def.pitch_deg, -def.overhang, def.eave_height)];
    for b in &def.breaks {
        let (p, d, z) = *levels.last().unwrap();
        let d = if (p - 90.0).abs() < 1e-9 {
            d
        } else {
            d + (b.height - z) / p.to_radians().tan()
        };
        levels.push((b.pitch_deg, d, b.height));
    }
    let sloped: Vec<usize> = (0..levels.len())
        .filter(|&k| levels[k].0 < 90.0 - 1e-9)
        .collect();
    let next = |k: usize| levels.get(k + 1).map_or(f64::INFINITY, |l| l.1);
    // The level at the front (E008: above a 135° one).
    let Some(first) = sloped
        .iter()
        .rposition(|&k| levels[k].1 <= dist + 1e-9 && dist <= next(k) + 1e-9)
        .or((!sloped.is_empty()).then_some(0))
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (j, &k) in sloped.iter().enumerate().skip(first) {
        let (p, d, z) = levels[k];
        let t = p.to_radians().tan();
        let lo = if j == first { f64::NEG_INFINITY } else { o + d };
        let hi = sloped
            .get(j + 1)
            .map_or(f64::INFINITY, |&k2| o + levels[k2].1);
        if hi < lo {
            continue;
        }
        out.push(Level {
            plane: Plane {
                a: n.x * t,
                b: n.y * t,
                c: z - (d + o) * t,
            },
            n,
            lo,
            hi,
        });
    }
    out
}

/// Cuts dormer `d` into the roofs. `host` is the main-roof wall (definition
/// and end point) whose plane carries the dormer, as DACAD stores it.
pub fn apply_dormer(geos: &mut [RoofGeometry], d: &Dormer, host: Option<&(EdgeDef, Point2)>) {
    // Far enough to pass any host face.
    let reach = geos
        .iter()
        .flat_map(|g| g.faces.iter().flat_map(|f| f.polygon.iter()))
        .fold(0.0f64, |m, p| m.max(p[0].abs()).max(p[1].abs()))
        * 4.0
        + 10.0;
    // The dormer's front: its wall point lowest down the host wall's slope.
    let front = {
        let first = d.edges.first().map_or(Point2::new(0.0, 0.0), |e| e.start);
        host.map_or(first, |(def, end)| {
            let dir = unit(Point2::new(end.x - def.start.x, end.y - def.start.y));
            walls(d)
                .iter()
                .flat_map(|w| [w.0, w.1])
                .fold(first, |m, p| {
                    if -dir.y * p.x + dir.x * p.y < -dir.y * m.x + dir.x * m.y - 1e-9 {
                        p
                    } else {
                        m
                    }
                })
        })
    };
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
        /// Index into `host_faces`.
        level: usize,
        /// Plan region inside the dormer walls covered by this piece.
        under: Vec<Point2>,
        chord: Option<RoofLine>,
    }
    let mut cands: Vec<Candidate> = Vec::new();
    // Host faces under the dormer's walls, one per plane: coplanar faces
    // (a face split by a notch in the outline, 92091101) are the same host.
    let mut host_faces: Vec<(usize, usize, Level)> = Vec::new();
    for (gi, g) in geos.iter().enumerate() {
        for (fi, hf) in g.faces.iter().enumerate() {
            if !seeds.iter().any(|&m| inside(&plan(hf), m)) {
                continue;
            }
            let hp = Plane::of(hf);
            if !host_faces.iter().any(|f| f.2.plane.same(&hp)) {
                let n = Point2::new(1.0, 0.0);
                let (lo, hi) = (f64::NEG_INFINITY, f64::INFINITY);
                host_faces.push((
                    gi,
                    fi,
                    Level {
                        plane: hp,
                        n,
                        lo,
                        hi,
                    },
                ));
            }
        }
    }
    // DACAD closes the dormer with its host wall's planes only, wherever the
    // dormer stands (D101_0013: past the ridge, D101_0020).
    let levels = host.map_or(Vec::new(), |(def, end)| wall_levels(def, *end, front));
    if !levels.is_empty() {
        let fallback = host_faces.first().map(|f| (f.0, f.1));
        host_faces = levels
            .iter()
            .filter_map(|lv| {
                let hp = lv.plane;
                host_faces
                    .iter()
                    .find(|f| f.2.plane.near(&hp))
                    .map(|f| (f.0, f.1))
                    .or_else(|| {
                        geos.iter().enumerate().find_map(|(gi, g)| {
                            g.faces
                                .iter()
                                .position(|f| Plane::of(f).near(&hp))
                                .map(|fi| (gi, fi))
                        })
                    })
                    .or(fallback)
                    .map(|(gi, fi)| (gi, fi, *lv))
            })
            .collect();
    }
    // DACAD closes the dormer with the host plane as a wall on the line
    // through the dormer's front, across the host's slope (RD2 `T` record):
    // in front of that line the dormer is not cut, also where it runs under
    // the host's overhang (D101_0239, D101_0270).
    let front_line = (host.is_some() && !levels.is_empty())
        .then(|| host_faces.first())
        .flatten()
        .map(|f| unit(Point2::new(f.2.plane.a, f.2.plane.b)))
        .filter(|n| n.x.is_finite() && n.y.is_finite() && (n.x != 0.0 || n.y != 0.0))
        .map(|n| (n, n.x * front.x + n.y * front.y));
    let mut front_chords: Vec<RoofLine> = Vec::new();
    for df in &roof.faces {
        let dp = Plane::of(df);
        let mut dpoly = plan(df);
        if let Some((n, o)) = front_line {
            let ahead = clip_half(&dpoly, -n.x, -n.y, o);
            if ahead.len() >= 3 && signed_area(&ahead).abs() > 1e-9 {
                let (gi, fi, lv) = host_faces[0];
                let hp = lv.plane;
                // Where the dormer is under the host on the line, the cut
                // runs along it (DACAD: a valley).
                let on: Vec<Point2> = ahead
                    .iter()
                    .copied()
                    .filter(|p| (n.x * p.x + n.y * p.y - o).abs() < 1e-9)
                    .collect();
                if let [a, b, ..] = on[..] {
                    let below = clip_seg(a, b, |p| hp.z(p) - dp.z(p));
                    if let Some((a, b)) = below.filter(|(a, b)| a.distance(*b) > 1e-6) {
                        front_chords.push(RoofLine {
                            kind: LineKind::Valley,
                            a: [a.x, a.y, dp.z(a)],
                            b: [b.x, b.y, dp.z(b)],
                            side: None,
                        });
                    }
                }
                cands.push(Candidate {
                    piece: Piece {
                        poly: ahead,
                        plane: dp,
                        pitch_deg: df.pitch_deg,
                    },
                    host: (gi, fi),
                    level: 0,
                    under: Vec::new(),
                    chord: None,
                });
            }
            dpoly = clip_half(&dpoly, n.x, n.y, -o);
            if dpoly.len() < 3 {
                continue;
            }
        }
        for (hi, &(gi, fi, lv)) in host_faces.iter().enumerate() {
            {
                let hp = lv.plane;
                let above = clip_half(&lv.clip(&dpoly), dp.a - hp.a, dp.b - hp.b, dp.c - hp.c);
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
                    level: hi,
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
    let mut hosts: Vec<usize> = Vec::new();
    let mut removed = 0.0;
    for (c, k) in cands.into_iter().zip(keep) {
        if !k {
            continue;
        }
        // Every face of the host roof loses what lies under the dormer
        // (DACAD case C42: a dormer through the ridge also cuts the far face).
        // DACAD deducts the whole footprint as one negative face on the host
        // plane: also where it runs past the host face, over a ridge onto a
        // steeper face (mass case R1_0211) or past the roof's outline
        // (R1_0018). Each face still loses its own share of it.
        if c.under.len() >= 3 {
            let sec = Plane::of(&geos[c.host.0].faces[c.host.1]).sec();
            let mut left = signed_area(&c.under).abs();
            // Faces of other dormers keep their area (D101_0097).
            for f in geos[c.host.0]
                .faces
                .iter_mut()
                .filter(|f| f.edge != usize::MAX)
            {
                let cut = signed_area(&clip_polygon(&plan(f), &c.under)).abs();
                if cut > 1e-12 {
                    f.area -= cut * sec;
                    left -= cut;
                }
            }
            if left > 1e-9 {
                geos[c.host.0].faces[c.host.1].area -= left * sec;
            }
            removed += signed_area(&c.under).abs() * sec;
            geos[c.host.0].footprints.push(c.under.clone());
        }
        if !hosts.contains(&c.level) {
            hosts.push(c.level);
        }
        chords.extend(c.chord);
        pieces.push(c.piece);
    }
    let piece_polys: Vec<&[Point2]> = pieces.iter().map(|p| p.poly.as_slice()).collect();

    // Wall lines of the dormer, for the Krüppel test below.
    let wall_segs: Vec<(Point2, Point2)> = walls(d)
        .into_iter()
        .filter(|(s, e, _)| s.distance(*e) > 1e-6)
        .map(|(s, e, _)| (s, e))
        .collect();

    // A dormer whose eave lies on the host face: the lower edge of a clipped
    // gable above it is a Knick line c, not an eave (DACAD cases C76-C78).
    let eave = walls(d)
        .iter()
        .map(|w| w.2.eave_height)
        .fold(f64::INFINITY, f64::min);
    let on_face = wall_segs
        .iter()
        .any(|&(a, _)| match levels.iter().find(|l| l.holds(a)) {
            Some(lv) => (lv.plane.z(a) - eave).abs() < 2e-3,
            None => geos[home]
                .faces
                .iter()
                .any(|f| covers(&plan(f), a) && (Plane::of(f).z(a) - eave).abs() < 2e-3),
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
    lines.extend(front_chords);

    // Wall abutments: dormer walls standing on a host face.
    let host_planes: Vec<(Level, Point2)> = hosts
        .iter()
        .map(|&k| {
            let lv = host_faces[k].2;
            (lv, unit(Point2::new(lv.plane.a, lv.plane.b)))
        })
        .collect();
    // The host plane counts beyond its face (DACAD case C42), but only over
    // the host roof.
    let home_polys: Vec<Vec<Point2>> = geos[home].faces.iter().map(plan).collect();
    let mut cuts: Vec<&[Point2]> = piece_polys.clone();
    cuts.extend(home_polys.iter().map(Vec::as_slice));
    let host_lines: Vec<RoofLine> = geos.iter().flat_map(|g| g.lines.iter().cloned()).collect();
    let mut minus: Vec<(String, f64)> = Vec::new();
    let n = part.edges.len();
    // A front leaning outward (pitch over 90°) abuts only where its wall
    // rises above the host plane: none with its eave on the host face (DACAD
    // case C71), along the whole front with the eave higher (D101_0057).
    for i in (0..n).filter(|&i| i != back) {
        let e = &part.edges[i];
        let walled = |z: f64| e.pitch_deg <= 90.0 + 1e-9 || e.eave_height - z > 0.01;
        // The wall's top under a sloped dormer face.
        let top = e.eave_height + e.overhang * e.pitch_deg.to_radians().tan();
        let (a, b) = (part.edges[i].start, part.edges[(i + 1) % n].start);
        let dir = unit(Point2::new(b.x - a.x, b.y - a.y));
        let out = Point2::new(dir.y, -dir.x);
        for (lv, up) in &host_planes {
            let hp = &lv.plane;
            // The hole edge runs along the whole wall where the dormer shows,
            // on the host plane also past the host face or the roof outline
            // (R1_0997, D101_0053).
            // Under a sloped dormer face the wall stands only where the host
            // is below its top: none with the eave on the host face
            // (D101_0360); the hole edge follows it also where the dormer's
            // roof dips under the host (D101_0179).
            let (mut a, mut b) = (a, b);
            if e.pitch_deg < 90.0 - 1e-9 {
                match clip_seg(a, b, |p| top - 1e-4 - hp.z(p)) {
                    Some((p, q)) => (a, b) = (p, q),
                    None => continue,
                }
            }
            let segs = split_keep([a.x, a.y, hp.z(a)], [b.x, b.y, hp.z(b)], &cuts, |p, z| {
                walled(z)
                    && lv.holds(p)
                    && (e.pitch_deg < 90.0 - 1e-9 || pieces.iter().any(|pc| covers(&pc.poly, p)))
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
                // A hole edge lying on a roof line turns into a negative line
                // of that type; only along lines other than eaves, verges and
                // pent lines it still abuts (DACAD FUN_00da5160; front on the
                // eave, C29). One reaching past the line keeps the line and
                // abuts (92091101).
                // The dormer's own lines count too (D101_0179: its knick on
                // the host's eave).
                let own = lines.clone();
                if let Some(l) = host_lines.iter().chain(&own).find(|l| on_line(p, q, l)) {
                    let code = l.dacad_code();
                    minus.push((code.to_string(), dist3(p, q)));
                    if matches!(code, "T" | "OL" | "OR" | "OS" | "P") {
                        continue;
                    }
                }
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
    target.minus_lines.extend(minus);
    target.openings.push(Opening {
        kind: OpeningKind::Dormer,
        area: removed,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PitchBreak;

    /// A mansard wall along the x axis: 70° from 3.0 m (overhang 0.3), 30°
    /// from 5.0 m, 20° from 6.5 m.
    fn mansard() -> EdgeDef {
        EdgeDef {
            start: Point2::new(0.0, 0.0),
            pitch_deg: 70.0,
            eave_height: 3.0,
            overhang: 0.3,
            half_hip: None,
            breaks: vec![
                PitchBreak {
                    height: 5.0,
                    pitch_deg: 30.0,
                },
                PitchBreak {
                    height: 6.5,
                    pitch_deg: 20.0,
                },
            ],
            priority: 0,
            direction_rad: None,
            clad: false,
        }
    }

    #[test]
    fn host_levels_start_at_the_dormer_front() {
        let end = Point2::new(12.0, 0.0);
        // Front on the 30° level: DACAD's RD2 closes the dormer with 5.619 m
        // at the front, 150° (30°), and a break at 6.5 m, 160° (20°).
        let lv = wall_levels(&mansard(), end, Point2::new(5.0, 1.5));
        assert_eq!(lv.len(), 2);
        assert!((lv[0].plane.z(Point2::new(5.0, 1.5)) - 5.619).abs() < 1e-3);
        assert!((lv[0].plane.b - 30f64.to_radians().tan()).abs() < 1e-9);
        assert!((lv[1].plane.b - 20f64.to_radians().tan()).abs() < 1e-9);
        // The break where the 30° plane reaches 6.5 m.
        let y = lv[1].lo;
        assert!((lv[0].plane.z(Point2::new(0.0, y)) - 6.5).abs() < 1e-9);
        assert!(lv[0].lo.is_infinite() && lv[1].hi.is_infinite());
        // Front on the 70° level: 4.3737 m, all three levels.
        let lv = wall_levels(&mansard(), end, Point2::new(5.0, 0.2));
        assert_eq!(lv.len(), 3);
        assert!((lv[0].plane.z(Point2::new(5.0, 0.2)) - 4.3737).abs() < 1e-3);
    }

    #[test]
    fn host_level_above_an_overhanging_one() {
        // E008: 30° from 10.0 (overhang 0.7), 90° from 10.4041, 135° from
        // 12.6, 30° from 13.1, 35° from 13.8; DACAD: 13.7986 m at 0.71 m.
        let mut def = mansard();
        def.pitch_deg = 30.0;
        def.eave_height = 10.0;
        def.overhang = 0.7;
        def.breaks = [(10.4041, 90.0), (12.6, 135.0), (13.1, 30.0), (13.8, 35.0)]
            .map(|(height, pitch_deg)| PitchBreak { height, pitch_deg })
            .to_vec();
        let at = Point2::new(5.0, 0.71);
        let lv = wall_levels(&def, Point2::new(23.2, 0.0), at);
        assert!((lv[0].plane.z(at) - 13.7986).abs() < 1e-3);
        assert!((lv[0].plane.b - 30f64.to_radians().tan()).abs() < 1e-9);
    }
}
