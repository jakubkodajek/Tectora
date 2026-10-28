//! Projects with two or more roofs, joined the way DACAD does it.
//!
//! DACAD builds every roof on its own in its core and then joins them in
//! the "Modellverschnitt" ([`crate::modell`]), which re-derives every line
//! and face of the result. Tectora follows the same files:
//!
//! 1. [`write_rd2`]: the project as DACAD's RD2 (the core's input), read
//!    back with [`Rd2::parse`] for the face frames and the wall ring.
//! 2. A [`CoreProducer`] builds the roof of each model on its own and hands
//!    over its face polygons ([`IntGeometry`]) and, when it has one, its
//!    core SOL. [`SweepProducer`] does it with Tectora's sweep engine; a
//!    port of DACAD's core can take its place without touching the rest.
//! 3. The INT ([`write_int_geometry`]) and the W3D (the wall stage of the
//!    core SOL) go through [`modell::run`].
//! 4. [`to_geometries`] turns its lines and faces back into one
//!    [`RoofGeometry`] per part.
//!
//! Windows are applied afterwards, as DACAD deducts them from the result;
//! chimneys and openings go into the RD2 as holes and are cut by the
//! Modellverschnitt itself.
//!
//! A producer says which roofs it can build face by face as DACAD's core
//! does ([`CoreProducer::builds`]); for a project with another roof, or
//! with dormers the producer does not hand over, the engine keeps its own
//! join ([`crate::union`]). The sweep builds DACAD's faces for roofs on one
//! eave level without priorities and standing gables, and no dormers yet:
//! on other roofs its faces differ from the core's (raised eaves, the
//! priority plane ending at a step, edges bordering a hidden wall face).
//!
//! Lines are left with flag 0: DACAD flags an edge that borders a wall's
//! hidden wall face, which happens on the raised roofs the sweep does not
//! take; on the cases it takes the flags of DACAD's INT are all 0.

use std::collections::HashMap;
use std::fmt::Write as _;

use crate::coresol::num::put_f;
use crate::coresol::{
    CoreSol, IntEdge, IntFace, IntGeometry, Rd2, SolFace, SolLine, wall_stage, write_int_geometry,
};
use crate::geom::Point2;
use crate::model::{EdgeDef, Project, RoofPart};
use crate::modell::{self, FaceKind};
use crate::roof::{LineKind, Opening, OpeningKind, RoofFace, RoofGeometry, RoofLine, VergeSide};

/// Where each part of the project went in the RD2.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Rd2Map {
    pub parts: Vec<PartMap>,
    /// The dormers written to the RD2.
    pub dormers: Vec<DormerMap>,
}

/// The RD2 model of a part and the wall number of each of its edges.
#[derive(Debug, Clone, PartialEq)]
pub struct PartMap {
    pub model: i32,
    pub walls: Vec<i32>,
}

/// The RD2 models of a dormer (`T`/`G` and `t`/`g` records share the
/// number) and the wall number of each of its edges.
#[derive(Debug, Clone, PartialEq)]
pub struct DormerMap {
    /// Index into the project's dormers.
    pub dormer: usize,
    pub model: i32,
    /// The part the dormer sits on: its faces belong to that part.
    pub part: usize,
    pub walls: Vec<i32>,
}

impl Rd2Map {
    /// The part and edge index of a wall. A dormer's walls give the part it
    /// sits on and [`usize::MAX`] for the edge.
    pub fn find(&self, model: i32, wall: i32) -> Option<(usize, usize)> {
        if let Some(pi) = self.parts.iter().position(|p| p.model == model) {
            let ei = self.parts[pi].walls.iter().position(|&w| w == wall)?;
            return Some((pi, ei));
        }
        self.dormers
            .iter()
            .find(|d| d.model == model)
            .map(|d| (d.part, usize::MAX))
    }
}

/// The main-roof wall a dormer sits on, as DACAD writes it into the
/// dormer's `T` record.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DormerHost {
    pub part: usize,
    pub edge: usize,
    /// The `D`/`K` record of the wall whose plane carries the dormer.
    pub level: usize,
}

/// Whether DACAD joins the roofs of this project in its Modellverschnitt:
/// it does for two or more roofs, dormers not counted.
pub fn applies(project: &Project) -> bool {
    project.parts.len() >= 2
}

/// Whether the sweep builds this roof the way DACAD's core does, face by
/// face: every sloped wall (marker walls of no length aside) rises from
/// one eave level, no wall has
/// priority and no standing gable splits the part.
pub fn sweep_fits(part: &RoofPart) -> bool {
    let mut eaves = part
        .edges
        .iter()
        .enumerate()
        .filter(|(i, e)| {
            let next = part.edges[(i + 1) % part.edges.len()].start;
            !e.is_gable() && e.start.distance(next) > 1e-6
        })
        .map(|(_, e)| e.eave_height as f32);
    let first = eaves.next();
    let level = first.is_none_or(|h| eaves.all(|x| (x - h).abs() < 0.01));
    level
        && part.edges.iter().all(|e| e.priority == 0)
        && crate::gable_wall::cut(part).cuts.is_empty()
}

/// The RD2 text of the project's roofs and holes, and where each part
/// went, without dormers (see [`write_rd2_with`]).
pub fn write_rd2(project: &Project) -> (String, Rd2Map) {
    write_rd2_with(project, &[])
}

/// The RD2 text of the project's roofs, dormers and holes, and where each
/// part went. Parts are models 1, 2, ... in project order, the dormers
/// with a host in `hosts` (by dormer index) follow; models are written
/// from the highest number down (DACAD's core reads its models in
/// reverse). A part or dormer imported from DACAD lists its walls as
/// recorded, in file order with their own numbers, lengths and
/// directions; others get the next free numbers and their edges as they
/// are. The parts must be counter-clockwise (see
/// [`RoofPart::normalize_ccw`]).
///
/// A dormer is written as DACAD writes it (captured RD2s of TRAING01,
/// E008, 92091101 and the s1/s2 mass sets): a `T` wall, the host roof
/// plane as a wall through the dormer's front, followed by the dormer's
/// walls as `G` walls; then the same as `t`/`g` walls, where every wall is
/// a cheek (`D -1`, vertical, at the height of the host plane) with the
/// wall's own sloped plane as `K 0`, its eave moved onto the wall line.
pub fn write_rd2_with(project: &Project, hosts: &[Option<DormerHost>]) -> (String, Rd2Map) {
    let mut map = Rd2Map::default();
    let written: Vec<(usize, DormerHost)> = project
        .dormers
        .iter()
        .enumerate()
        .filter_map(|(di, _)| hosts.get(di).copied().flatten().map(|h| (di, h)))
        .filter(|(_, h)| {
            project
                .parts
                .get(h.part)
                .is_some_and(|p| h.edge < p.edges.len())
        })
        .collect();
    let mut no = project
        .parts
        .iter()
        .map(|p| p.edges.as_slice())
        .chain(
            written
                .iter()
                .map(|&(di, _)| project.dormers[di].edges.as_slice()),
        )
        .filter(|e| sourced(e))
        .flat_map(|e| {
            e.iter()
                .filter_map(|e| e.source.as_ref().map(|s| s.id as i32))
        })
        .max()
        .unwrap_or(0);
    let mut numbers = |edges: &[EdgeDef]| -> Vec<i32> {
        edges
            .iter()
            .map(|e| match &e.source {
                Some(s) if sourced(edges) => s.id as i32,
                _ => {
                    no += 1;
                    no
                }
            })
            .collect()
    };
    for (pi, part) in project.parts.iter().enumerate() {
        let walls = numbers(&part.edges);
        map.parts.push(PartMap {
            model: pi as i32 + 1,
            walls,
        });
    }
    for (k, &(di, h)) in written.iter().enumerate() {
        let walls = numbers(&project.dormers[di].edges);
        map.dormers.push(DormerMap {
            dormer: di,
            model: (project.parts.len() + k) as i32 + 1,
            part: h.part,
            walls,
        });
    }
    let mut s = String::from("0\n");
    for (dm, &(_, h)) in map.dormers.iter().zip(&written).rev() {
        write_dormer(&mut s, project, dm, h, &map.parts[h.part]);
    }
    for (part, pm) in project.parts.iter().zip(&map.parts).rev() {
        let n = part.edges.len();
        let mut order: Vec<usize> = (0..n).collect();
        if sourced(&part.edges) {
            order.sort_by_key(|&i| pm.walls[i]);
        }
        for i in order {
            let e = &part.edges[i];
            let next = part.edges[(i + 1) % n].start;
            wall_line(
                &mut s,
                'W',
                pm.model,
                pm.walls[i],
                e,
                next,
                sourced(&part.edges),
            );
            for r in records(e) {
                record(&mut s, r);
            }
        }
    }
    let holes = project
        .chimneys
        .iter()
        .map(|c| (c, c.height))
        .chain(project.roof_openings.iter().map(|c| (c, 0.0)));
    for (k, (c, h)) in holes.enumerate() {
        // DACAD prints the float it stores, ties rounded up (14.65 is
        // 14.6, 15.25 is 15.3).
        let _ = write!(s, "L {:2} ", k + 1);
        put_f(&mut s, h as f32 as f64, 4, 1);
        let _ = writeln!(s, " {:2}", c.corners.len());
        for p in &c.corners {
            let _ = writeln!(s, "  {:.4} {:.4}", p.x, p.y);
        }
    }
    (s, map)
}

/// Every wall was imported from DACAD: the walls are written as recorded.
fn sourced(edges: &[EdgeDef]) -> bool {
    edges.iter().all(|e| e.source.is_some())
}

/// Start, length and direction (degrees) of wall `e` running to `next`,
/// as recorded when imported from DACAD.
fn wall_geometry(e: &EdgeDef, next: Point2, sourced: bool) -> (Point2, f64, f64) {
    match &e.source {
        Some(src) if sourced => (src.start, src.length, src.angle_deg),
        _ => {
            let len = e.start.distance(next);
            let angle = match e.direction_rad {
                Some(a) if len <= 1e-6 => a.to_degrees(),
                _ => (next.y - e.start.y).atan2(next.x - e.start.x).to_degrees(),
            };
            (e.start, len, angle)
        }
    }
}

/// A wall record `c model wall x y len angle priority`.
fn wall_line(
    s: &mut String,
    letter: char,
    model: i32,
    wall: i32,
    e: &EdgeDef,
    next: Point2,
    sourced: bool,
) {
    let (start, len, angle) = wall_geometry(e, next, sourced);
    let _ = writeln!(
        s,
        "{letter} {:2} {:2} {:.4} {:.4}{:10.5}{:10.4}{:5.1}",
        model, wall, start.x, start.y, len, angle, e.priority as f64
    );
}

/// One `D`/`K` record.
fn record(s: &mut String, (letter, sub, flag, h, ov, pitch): Record) {
    let _ = writeln!(
        s,
        "{letter} {sub:2} {:2} {flag:2}{h:10.4} {ov:.4}{pitch:10.4}",
        0
    );
}

/// The `T`/`G` and `t`/`g` models of a dormer.
fn write_dormer(s: &mut String, project: &Project, dm: &DormerMap, h: DormerHost, host: &PartMap) {
    let d = &project.dormers[dm.dormer];
    let part = &project.parts[h.part];
    let n = part.edges.len();
    let he = &part.edges[h.edge];
    let hnext = part.edges[(h.edge + 1) % n].start;
    let (hstart, hlen, hangle) = wall_geometry(he, hnext, sourced(&part.edges));
    let dsourced = sourced(&d.edges);
    let next = |i: usize| d.edges.get(i + 1).map_or(d.end, |e| e.start);
    // The front: the dormer's wall start lowest down the host plane.
    let rad = hangle.to_radians();
    let inward = Point2::new(-rad.sin(), rad.cos());
    let dist = |p: Point2| (p.x - hstart.x) * inward.x + (p.y - hstart.y) * inward.y;
    let mut front = d.edges.first().map_or(d.end, |e| e.start);
    for (i, e) in d.edges.iter().enumerate() {
        let (p, _, _) = wall_geometry(e, next(i), dsourced);
        if dist(p) < dist(front) - 1e-6 {
            front = p;
        }
    }
    // The host's planes as (sub, flag, distance from the wall line and
    // height where each starts, pitch).
    let mut levels: Vec<(i32, i32, f64, f64, f64)> = Vec::new();
    for (k, r) in records(he).iter().enumerate() {
        let d0 = match levels.last() {
            None => -r.4,
            Some(&(_, _, dp, zp, pp)) if (pp - 90.0).abs() > 1e-9 => {
                dp + (r.3 - zp) / pp.to_radians().tan()
            }
            Some(&(_, _, dp, _, _)) => dp,
        };
        levels.push((k as i32, r.2, d0, r.3, r.5));
    }
    let level = h.level.min(levels.len().saturating_sub(1));
    let (_, _, d0, z0, p0) = levels[level];
    let h_front = z0 + (dist(front) - d0) * p0.to_radians().tan();
    let back = if hangle < 180.0 {
        hangle + 180.0
    } else {
        hangle - 180.0
    };
    let host_wall = host.walls[h.edge];
    let t_block = |s: &mut String, letter: char| {
        let _ = writeln!(
            s,
            "{letter} {:2} {:2} {:.4} {:.4}{:10.5}{:10.4}{:5.1}",
            dm.model, host_wall, front.x, front.y, hlen, back, 0.0
        );
        for (j, &(sub, flag, _, z, p)) in levels.iter().enumerate().skip(level) {
            let (letter, z) = if j == level { ('D', h_front) } else { ('K', z) };
            record(s, (letter, sub, flag, z, 0.0, 180.0 - p));
        }
    };
    t_block(s, 'T');
    for (i, e) in d.edges.iter().enumerate() {
        wall_line(s, 'G', dm.model, dm.walls[i], e, next(i), dsourced);
        for r in records(e) {
            record(s, r);
        }
    }
    t_block(s, 't');
    for (i, e) in d.edges.iter().enumerate() {
        wall_line(s, 'g', dm.model, dm.walls[i], e, next(i), dsourced);
        record(s, ('D', -1, 0, h_front, 0.0, 90.0));
        if !e.is_gable() {
            let z = e.eave_height + e.overhang * e.pitch_deg.to_radians().tan();
            record(s, ('K', 0, 1, z, 0.0, e.pitch_deg));
        }
    }
}

/// A `D`/`K` record: `(letter, sub, flag, height, overhang, pitch)`.
type Record = (char, i32, i32, f64, f64, f64);

/// The `D`/`K` records of a wall: `(letter, sub, flag, height, overhang,
/// pitch)`. The first is the wall's own definition, the others the
/// half-hip and the pitch breaks.
fn records(e: &EdgeDef) -> Vec<Record> {
    let roofed = |p: f64| i32::from(p < crate::model::GABLE_PITCH_DEG);
    // The flag says the wall carries roof material: every sloped wall, and
    // a gable only when it is clad.
    let mut out = vec![(
        'D',
        0,
        i32::from(!e.is_gable() || e.clad),
        e.eave_height,
        e.overhang,
        e.pitch_deg,
    )];
    if e.is_gable()
        && let Some(h) = &e.half_hip
    {
        out.push(('K', 1, roofed(h.pitch_deg), h.height, 0.0, h.pitch_deg));
    }
    let mut breaks = e.breaks.clone();
    breaks.sort_by(|a, b| a.height.total_cmp(&b.height));
    for b in breaks {
        let sub = out.len() as i32;
        out.push(('K', sub, roofed(b.pitch_deg), b.height, 0.0, b.pitch_deg));
    }
    out
}

/// What a producer hands over for the joined roofs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Produced {
    /// The face polygons, in the order of the INT.
    pub int: IntGeometry,
    /// The core SOL, for the wall stage (W3D); without it no walls are
    /// passed on.
    pub sol: Option<CoreSol>,
}

/// Builds the roof of every model of an RD2 on its own.
pub trait CoreProducer {
    fn produce(&self, project: &Project, rd2: &Rd2, map: &Rd2Map) -> Option<Produced>;

    /// Whether the producer builds this roof face by face as DACAD's core
    /// does; when it cannot for some part, the project is not joined this
    /// way.
    fn builds(&self, _part: &RoofPart) -> bool {
        true
    }

    /// Whether the producer hands over dormers (`G`/`g` models with their
    /// faces and ground plans).
    fn builds_dormers(&self) -> bool {
        true
    }
}

/// The producer with Tectora's sweep engine: each part is generated as if
/// it were the only one.
#[derive(Debug, Clone, Copy, Default)]
pub struct SweepProducer;

impl CoreProducer for SweepProducer {
    fn builds(&self, part: &RoofPart) -> bool {
        sweep_fits(part)
    }

    /// Not yet: the sweep cuts dormers into the finished roof.
    fn builds_dormers(&self) -> bool {
        false
    }

    fn produce(&self, project: &Project, rd2: &Rd2, map: &Rd2Map) -> Option<Produced> {
        let mut by_model: HashMap<i32, Vec<IntFace>> = HashMap::new();
        for (part, pm) in project.parts.iter().zip(&map.parts) {
            let mut alone = Project::new("");
            alone.parts.push(part.clone());
            let geo = crate::openings::build_roofs(&alone)
                .pop()
                .unwrap_or_default();
            by_model.insert(pm.model, part_faces(part, pm, &geo, rd2));
        }
        let mut faces = Vec::new();
        for m in &rd2.models {
            let Some(fs) = by_model.remove(&m.no) else {
                continue;
            };
            // DACAD writes the faces in ring order, sub numbers ascending.
            let pos = |f: &IntFace| {
                m.ring
                    .iter()
                    .position(|r| m.walls[r.wall].no == f.wall)
                    .unwrap_or(usize::MAX)
            };
            let mut fs = fs;
            fs.sort_by_key(|f| (pos(f), f.sub));
            faces.extend(fs);
        }
        let sol = core_sol(rd2, &faces);
        Some(Produced {
            int: IntGeometry {
                faces,
                footprints: Vec::new(),
            },
            sol: Some(sol),
        })
    }
}

/// The face records of one part's own roof.
fn part_faces(part: &RoofPart, pm: &PartMap, geo: &RoofGeometry, rd2: &Rd2) -> Vec<IntFace> {
    let n = part.edges.len();
    // A wall merged into its collinear neighbour has no face of its own;
    // its faces belong to the ring wall that absorbed it.
    let ring_wall = |e: usize| -> i32 {
        for k in 0..n {
            let i = (e + n - k) % n;
            if rd2.find_wall(b'W', pm.walls[i]).is_some() {
                return pm.walls[i];
            }
        }
        pm.walls[e]
    };
    let mut out = Vec::new();
    let mut push = |wall: i32, sub: i32, poly: &[[f64; 3]]| {
        let poly = dacad_order(rd2, wall, sub, poly);
        if poly.len() >= 3 {
            out.push(IntFace {
                ch: b'W',
                wall,
                sub,
                edges: ring(&poly),
            });
        }
    };
    for f in &geo.faces {
        if f.edge >= n {
            continue;
        }
        let e = &part.edges[f.edge];
        push(ring_wall(f.edge), plane_sub(e, f), &f.polygon);
    }
    // DACAD's core keeps a vertical face only where it closes the roof
    // between two or more sloped faces (a gable under a ridge); the walls
    // of a shed roof, under one plane only, get no face record.
    let near = |p: [f64; 3], q: [f64; 3]| dist(p, q) < 1e-4;
    for g in &geo.gables {
        let touching = geo
            .faces
            .iter()
            .filter(|f| {
                g.iter()
                    .filter(|&&p| f.polygon.iter().any(|&q| near(p, q)))
                    .count()
                    >= 2
            })
            .count();
        if touching < 2 {
            continue;
        }
        if let Some(i) = gable_edge(part, g) {
            push(ring_wall(i), 0, g);
        }
    }
    out
}

/// A face polygon in the vertex order of DACAD's core, as observed in its
/// INT files (in the face's frame, `s` along the eave, `t` up the slope):
/// a polygon of four or more corners runs clockwise from the top corner
/// with the smallest `s`; a triangle starts at a bottom corner and runs to
/// the apex first, from the corner with the larger `s` (anticlockwise),
/// except on the last wall of its ring, where it runs clockwise from the
/// smaller one. Repeated corners are dropped.
fn dacad_order(rd2: &Rd2, wall: i32, sub: i32, poly: &[[f64; 3]]) -> Vec<[f64; 3]> {
    let mut pts: Vec<[f64; 3]> = Vec::with_capacity(poly.len());
    for &p in poly {
        if pts.last().is_none_or(|&q| dist(p, q) > 1e-6) {
            pts.push(p);
        }
    }
    while pts.len() > 1 && dist(pts[0], pts[pts.len() - 1]) <= 1e-6 {
        pts.pop();
    }
    let Some((mi, ri)) = rd2.find_wall(b'W', wall) else {
        return pts;
    };
    let m = &rd2.models[mi];
    let w = &m.walls[m.ring[ri].wall];
    let Some(rf) = w.faces.iter().skip(1).find(|x| x.sub == sub) else {
        return pts;
    };
    let last = ri + 1 == m.ring.len();
    let (o, ev) = (rf.o.map(f64::from), rf.w.map(f64::from));
    let up =
        if rf.pitch_deg as f64 >= crate::model::GABLE_PITCH_DEG && (rf.pitch_deg as f64) < 90.001 {
            [0.0, 0.0, 1.0]
        } else {
            rf.u.map(f64::from)
        };
    let loc = |p: [f64; 3]| {
        let d = [p[0] - o[0], p[1] - o[1], p[2] - o[2]];
        (
            d[0] * ev[0] + d[1] * ev[1] + d[2] * ev[2],
            d[0] * up[0] + d[1] * up[1] + d[2] * up[2],
        )
    };
    let n = pts.len();
    if n < 3 {
        return pts;
    }
    let l: Vec<(f64, f64)> = pts.iter().map(|&p| loc(p)).collect();
    let area: f64 = (0..n)
        .map(|i| l[i].0 * l[(i + 1) % n].1 - l[(i + 1) % n].0 * l[i].1)
        .sum();
    let want_cw = n != 3 || last;
    if (area < 0.0) != want_cw {
        pts.reverse();
    }
    let l: Vec<(f64, f64)> = pts.iter().map(|&p| loc(p)).collect();
    let (tmin, tmax) = l
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), p| {
            (a.min(p.1), b.max(p.1))
        });
    let pick = |level: f64, smallest: bool| {
        (0..n)
            .filter(|&k| (l[k].1 - level).abs() < 1e-4)
            .min_by(|&a, &b| {
                let c = l[a].0.total_cmp(&l[b].0);
                if smallest { c } else { c.reverse() }
            })
            .unwrap_or(0)
    };
    let start = if n == 3 {
        pick(tmin, last)
    } else {
        pick(tmax, true)
    };
    pts.rotate_left(start);
    pts
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// The `D`/`K` record a face of wall `e` belongs to: the plane with its
/// pitch, the lowest one starting at or below the face's foot.
fn plane_sub(e: &EdgeDef, f: &RoofFace) -> i32 {
    let recs = records(e);
    let zmin = f.polygon.iter().map(|p| p[2]).fold(f64::INFINITY, f64::min);
    let mut best = None;
    for (k, r) in recs.iter().enumerate() {
        if (r.5 - f.pitch_deg).abs() < 1e-6 && r.3 <= zmin + 1e-3 {
            best = Some(k as i32);
        }
    }
    best.or_else(|| {
        recs.iter()
            .position(|r| (r.5 - f.pitch_deg).abs() < 1e-6)
            .map(|k| k as i32)
    })
    .unwrap_or(0)
}

/// The gable edge whose outline line (the wall line pushed out by its
/// overhang) a vertical polygon stands on.
fn gable_edge(part: &RoofPart, poly: &[[f64; 3]]) -> Option<usize> {
    let n = part.edges.len();
    let mut best: Option<(f64, usize)> = None;
    for (i, e) in part.edges.iter().enumerate() {
        if !e.is_gable() {
            continue;
        }
        let (a, b) = (e.start, part.edges[(i + 1) % n].start);
        let len = a.distance(b);
        if len < 1e-6 {
            continue;
        }
        let d = Point2::new((b.x - a.x) / len, (b.y - a.y) / len);
        // Outward normal of a counter-clockwise outline.
        let nrm = Point2::new(d.y, -d.x);
        let off = poly
            .iter()
            .map(|p| ((p[0] - a.x) * nrm.x + (p[1] - a.y) * nrm.y - e.overhang).abs())
            .fold(0.0, f64::max);
        if off < 1e-3 && best.is_none_or(|(o, _)| off < o) {
            best = Some((off, i));
        }
    }
    best.map(|b| b.1)
}

fn f32s(p: [f64; 3]) -> [f32; 3] {
    [p[0] as f32, p[1] as f32, p[2] as f32]
}

/// The edges of a closed polygon.
fn ring(poly: &[[f64; 3]]) -> Vec<IntEdge> {
    let n = poly.len();
    (0..n)
        .map(|i| IntEdge {
            p: f32s(poly[i]),
            q: f32s(poly[(i + 1) % n]),
            flag: 0,
        })
        .collect()
}

/// A core SOL with the faces' own polygons as lines (code 17, shared
/// edges once), enough for the wall stage.
fn core_sol(rd2: &Rd2, faces: &[IntFace]) -> CoreSol {
    let mut sol = CoreSol::default();
    let key = |p: [f32; 3]| [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()];
    let mut seen: HashMap<([u32; 3], [u32; 3]), i32> = HashMap::new();
    for f in faces {
        let Some((mi, ri)) = rd2.find_wall(f.ch, f.wall) else {
            continue;
        };
        let m = &rd2.models[mi];
        let rw = m.ring[ri];
        let w = &m.walls[rw.wall];
        let Some(rf) = w.faces.iter().skip(1).find(|x| x.sub == f.sub) else {
            continue;
        };
        let mut edges = Vec::new();
        for e in &f.edges {
            let (kp, kq) = (key(e.p), key(e.q));
            let id = if let Some(&k) = seen.get(&(kp, kq)) {
                k
            } else if let Some(&k) = seen.get(&(kq, kp)) {
                -k
            } else {
                let d = [e.q[0] - e.p[0], e.q[1] - e.p[1], e.q[2] - e.p[2]];
                sol.lines.push(SolLine {
                    p: e.p,
                    q: e.q,
                    code: 17,
                    len: (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt(),
                });
                let k = sol.lines.len() as i32;
                seen.insert((kp, kq), k);
                k
            };
            edges.push(id);
        }
        // Plane n·x = d with n the face normal (up-slope u × eave w).
        let (u, wv) = (rf.u.map(f64::from), rf.w.map(f64::from));
        let mut nrm = [
            u[1] * wv[2] - u[2] * wv[1],
            u[2] * wv[0] - u[0] * wv[2],
            u[0] * wv[1] - u[1] * wv[0],
        ];
        if nrm[2] < 0.0 {
            nrm = nrm.map(|v| -v);
        }
        let o = rf.o.map(f64::from);
        let d = nrm[0] * o[0] + nrm[1] * o[1] + nrm[2] * o[2];
        sol.faces.push(SolFace {
            ch: f.ch,
            wall: f.wall,
            sub: f.sub,
            kind: 0,
            area: 0.0,
            wall_line: [w.start[0], w.start[1], rw.end[0], rw.end[1]],
            plane: [nrm[0] as f32, nrm[1] as f32, nrm[2] as f32, d as f32],
            edges,
        });
    }
    sol
}

/// The joined roofs of a project, or `None` when DACAD would not join them
/// (fewer than two roofs) or something could not be passed on.
pub fn generate(project: &Project) -> Option<Vec<RoofGeometry>> {
    generate_with(project, &SweepProducer)
}

/// [`generate`] with another producer of the single roofs.
pub fn generate_with(project: &Project, producer: &dyn CoreProducer) -> Option<Vec<RoofGeometry>> {
    if !applies(project)
        || !project.parts.iter().all(|p| producer.builds(p))
        || (!project.dormers.is_empty() && !producer.builds_dormers())
    {
        return None;
    }
    let files = files_with(project, producer)?;
    let out = modell::run(&files.int, files.w3d.as_deref()).ok()?;
    let mut geos = to_geometries(project, &files.map, &out);
    for w in &project.windows {
        crate::openings::apply_window(&mut geos, w);
    }
    Some(geos)
}

/// The files of the joined roofs.
#[derive(Debug, Clone, PartialEq)]
pub struct Files {
    pub rd2: String,
    pub map: Rd2Map,
    pub int: String,
    pub w3d: Option<String>,
}

/// The RD2, INT and W3D of a project.
pub fn files_with(project: &Project, producer: &dyn CoreProducer) -> Option<Files> {
    let (rd2_text, map) = write_rd2(project);
    let rd2 = Rd2::parse(&rd2_text).ok()?;
    let produced = producer.produce(project, &rd2, &map)?;
    let int = write_int_geometry(&rd2, &produced.int).ok()?;
    let w3d = produced
        .sol
        .as_ref()
        .and_then(|s| wall_stage(s).ok())
        .map(|w| w.w3d);
    Some(Files {
        rd2: rd2_text,
        map,
        int,
        w3d,
    })
}

/// The line kind of a DACAD line code; `None` for what is no roof line.
pub fn line_kind(code: i32) -> Option<(LineKind, Option<VergeSide>)> {
    Some(match code {
        1 => (LineKind::Eave, None),
        2 => (LineKind::Ridge, None),
        3 => (LineKind::Hip, None),
        4 => (LineKind::Valley, None),
        5 => (LineKind::Pult, None),
        6 => (LineKind::Verge, Some(VergeSide::Left)),
        7 => (LineKind::Verge, Some(VergeSide::Right)),
        8 => (LineKind::Verge, Some(VergeSide::Oblique)),
        9 => (LineKind::AbutmentFront, None),
        10 => (LineKind::AbutmentBack, None),
        11 => (LineKind::AbutmentSide, None),
        12 => (LineKind::Knick, None),
        15 => (LineKind::Gorge, None),
        _ => return None,
    })
}

/// The result of the Modellverschnitt as one geometry per part: its lines
/// (lines DACAD writes with a negative length are left out, as DACAD's
/// totals leave them out), the visible pieces of its sloped faces with
/// their areas (less the cut-outs inside them) and, for clad gables, the
/// area of their vertical faces.
pub fn to_geometries(project: &Project, map: &Rd2Map, out: &modell::Output) -> Vec<RoofGeometry> {
    let p3 = |v: modell::V3| [v.x, v.y, v.z];
    let lines = out
        .lines
        .iter()
        .map(|l| SolResultLine {
            a: p3(l.p1),
            b: p3(l.p2),
            code: l.code,
            negative: l.sign < 0,
        })
        .collect();
    let mut faces = Vec::new();
    let mut k = 0;
    for f in &out.state.faces {
        let n = f.pieces.len() + f.cutters.len();
        let mine = &out.faces[k..k + n];
        k += n;
        let part = map.find(f.rec.model, f.rec.face);
        let clad =
            part.is_some_and(|(pi, ei)| project.parts[pi].edges.get(ei).is_some_and(|e| e.clad));
        let mut face = SolResultFace {
            model: f.rec.model,
            wall: f.rec.face,
            pitch_deg: f.rec.angle,
            clad,
            surfaces: Vec::new(),
            cuts: Vec::new(),
        };
        for o in mine.iter().filter(|o| o.kind == FaceKind::Surface) {
            face.surfaces.push((o.edges.clone(), o.area));
        }
        // The chimneys and openings cut out of this face, for the norms.
        let mut src = f.cut_src.iter();
        for o in mine.iter().filter(|o| matches!(o.kind, FaceKind::Cut(_))) {
            let opening = src.next().and_then(|s| match s.kind as u8 {
                b'T' => Some((OpeningKind::Chimney, s.sub as i64)),
                b'L' => Some((OpeningKind::Hole, s.sub as i64)),
                _ => None,
            });
            face.cuts.push((o.area, opening));
        }
        faces.push(face);
    }
    sol_geometries(
        project,
        map,
        &SolResult {
            lines,
            faces,
            negative_lines: NegativeLines::Drop,
        },
    )
}

/// A roof line of a SOL (the core's or the Modellverschnitt's).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SolResultLine {
    pub a: [f64; 3],
    pub b: [f64; 3],
    /// DACAD's line code.
    pub code: i32,
    /// DACAD writes the length negative.
    pub negative: bool,
}

/// A face of a SOL: its visible pieces and what was cut out of it.
#[derive(Debug, Clone, PartialEq)]
pub struct SolResultFace {
    /// The RD2 model and wall the face belongs to.
    pub model: i32,
    pub wall: i32,
    pub pitch_deg: f64,
    /// A vertical face that carries roof material (a clad gable).
    pub clad: bool,
    /// Pieces as line references (0-based index, +1 or -1 for the
    /// direction) with their areas.
    pub surfaces: Vec<(Vec<(usize, i32)>, f64)>,
    /// Cut-outs: the (negative) area and, for a chimney, opening or dormer,
    /// its kind and a key telling them apart.
    pub cuts: Vec<(f64, Option<(OpeningKind, i64)>)>,
}

/// How DACAD's totals treat the lines it writes with a negative length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NegativeLines {
    /// Left out (the Modellverschnitt's SOL).
    Drop,
    /// Subtracted (the core SOL: a chimney edge on a roof line).
    Subtract,
}

/// The lines and faces of a SOL, ready to be turned into geometries.
#[derive(Debug, Clone, PartialEq)]
pub struct SolResult {
    pub lines: Vec<SolResultLine>,
    pub faces: Vec<SolResultFace>,
    pub negative_lines: NegativeLines,
}

/// One geometry per part from the lines and faces of a SOL: each line goes
/// to the part of the first face using it, each sloped face piece becomes
/// a face of its wall's part (a dormer's faces are its host part's, with
/// edge [`usize::MAX`]), clad vertical faces count as `clad_area`, other
/// vertical faces are gables. The cut-outs inside a face are deducted from
/// its first piece and recorded as openings for the norms.
pub fn sol_geometries(project: &Project, map: &Rd2Map, res: &SolResult) -> Vec<RoofGeometry> {
    let mut geos = vec![RoofGeometry::default(); project.parts.len()];
    let mut owner: HashMap<usize, usize> = HashMap::new();
    for f in &res.faces {
        if let Some((pi, _)) = map.find(f.model, f.wall) {
            for (edges, _) in &f.surfaces {
                for &(li, _) in edges {
                    owner.entry(li).or_insert(pi);
                }
            }
        }
    }
    for (i, l) in res.lines.iter().enumerate() {
        let Some((kind, side)) = line_kind(l.code) else {
            continue;
        };
        let pi = owner.get(&i).copied().unwrap_or(0);
        let Some(g) = geos.get_mut(pi) else {
            continue;
        };
        let line = RoofLine {
            kind,
            a: l.a,
            b: l.b,
            side,
        };
        match (l.negative, res.negative_lines) {
            (false, _) => g.lines.push(line),
            (true, NegativeLines::Drop) => {}
            (true, NegativeLines::Subtract) => g
                .minus_lines
                .push((line.dacad_code().to_string(), line.length())),
        }
    }
    let mut holes: std::collections::BTreeMap<(usize, u8, i64), (OpeningKind, f64)> =
        Default::default();
    for f in &res.faces {
        let Some((pi, ei)) = map.find(f.model, f.wall) else {
            continue;
        };
        let geo = &mut geos[pi];
        let polygon = |edges: &[(usize, i32)]| -> Vec<[f64; 3]> {
            edges
                .iter()
                .filter_map(|&(li, s)| res.lines.get(li).map(|l| if s > 0 { l.a } else { l.b }))
                .collect()
        };
        let cut: f64 = f.cuts.iter().map(|c| c.0).sum();
        if (f.pitch_deg - 90.0).abs() <= 90.0 - crate::model::GABLE_PITCH_DEG {
            if f.clad {
                geo.clad_area += f.surfaces.iter().map(|s| s.1).sum::<f64>() + cut;
            }
            for (edges, _) in &f.surfaces {
                let p = polygon(edges);
                if p.len() >= 3 {
                    geo.gables.push(p);
                }
            }
            continue;
        }
        let first = geo.faces.len();
        for (edges, area) in &f.surfaces {
            geo.faces.push(RoofFace {
                edge: ei,
                pitch_deg: f.pitch_deg,
                polygon: polygon(edges),
                area: *area,
                deducted: 0.0,
            });
        }
        if let Some(face) = geo.faces.get_mut(first) {
            face.area += cut;
            face.deducted -= cut;
        }
        for &(area, opening) in &f.cuts {
            if let Some((kind, key)) = opening {
                let tag = match kind {
                    OpeningKind::Chimney => 0,
                    OpeningKind::Hole => 1,
                    OpeningKind::Dormer => 2,
                    OpeningKind::Window => 3,
                };
                holes.entry((pi, tag, key)).or_insert((kind, 0.0)).1 -= area;
            }
        }
    }
    for ((pi, _, _), (kind, area)) in holes {
        geos[pi].openings.push(Opening { kind, area });
    }
    geos
}
