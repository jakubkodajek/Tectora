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

use std::collections::HashMap;
use std::fmt::Write as _;

use crate::coresol::{
    CoreSol, IntEdge, IntFace, IntGeometry, Rd2, SolFace, SolLine, wall_stage, write_int_geometry,
};
use crate::geom::Point2;
use crate::model::{EdgeDef, Project, RoofPart};
use crate::modell::{self, FaceKind};
use crate::roof::{LineKind, RoofFace, RoofGeometry, RoofLine, VergeSide};

/// Where each part of the project went in the RD2.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Rd2Map {
    pub parts: Vec<PartMap>,
}

/// The RD2 model of a part and the wall number of each of its edges.
#[derive(Debug, Clone, PartialEq)]
pub struct PartMap {
    pub model: i32,
    pub walls: Vec<i32>,
}

impl Rd2Map {
    /// The part and edge index of a wall.
    pub fn find(&self, model: i32, wall: i32) -> Option<(usize, usize)> {
        let pi = self.parts.iter().position(|p| p.model == model)?;
        let ei = self.parts[pi].walls.iter().position(|&w| w == wall)?;
        Some((pi, ei))
    }
}

/// Whether DACAD joins the roofs of this project in its Modellverschnitt:
/// it does for two or more roofs, dormers not counted.
pub fn applies(project: &Project) -> bool {
    project.parts.len() >= 2
}

/// Whether the sweep builds this roof the way DACAD's core does, face by
/// face: every sloped wall rises from one eave level, no wall has
/// priority and no standing gable splits the part.
pub fn sweep_fits(part: &RoofPart) -> bool {
    let mut eaves = part
        .edges
        .iter()
        .filter(|e| !e.is_gable())
        .map(|e| e.eave_height as f32);
    let first = eaves.next();
    let level = first.is_none_or(|h| eaves.all(|x| (x - h).abs() < 0.01));
    level
        && part.edges.iter().all(|e| e.priority == 0)
        && crate::gable_wall::cut(part).cuts.is_empty()
}

/// The RD2 text of the project's roofs and holes, and where each part
/// went. Parts are written last first (DACAD's core reads its models in
/// reverse). A part imported from DACAD lists its walls as recorded, in
/// file order with their own numbers, lengths and directions; other parts
/// get the next free numbers and their edges as they are. The parts must
/// be counter-clockwise (see [`RoofPart::normalize_ccw`]).
pub fn write_rd2(project: &Project) -> (String, Rd2Map) {
    let mut map = Rd2Map::default();
    let sourced = |p: &RoofPart| p.edges.iter().all(|e| e.source.is_some());
    let mut no = project
        .parts
        .iter()
        .filter(|p| sourced(p))
        .flat_map(|p| p.edges.iter().filter_map(|e| e.source.as_ref().map(|s| s.id as i32)))
        .max()
        .unwrap_or(0);
    for (pi, part) in project.parts.iter().enumerate() {
        let walls = part
            .edges
            .iter()
            .map(|e| match &e.source {
                Some(s) if sourced(part) => s.id as i32,
                _ => {
                    no += 1;
                    no
                }
            })
            .collect();
        map.parts.push(PartMap {
            model: pi as i32 + 1,
            walls,
        });
    }
    let mut s = String::from("0\n");
    for (part, pm) in project.parts.iter().zip(&map.parts).rev() {
        let n = part.edges.len();
        let mut order: Vec<usize> = (0..n).collect();
        if sourced(part) {
            order.sort_by_key(|&i| pm.walls[i]);
        }
        for i in order {
            let e = &part.edges[i];
            let (start, len, angle) = match &e.source {
                Some(src) if sourced(part) => (src.start, src.length, src.angle_deg),
                _ => {
                    let next = part.edges[(i + 1) % n].start;
                    let len = e.start.distance(next);
                    let angle = match e.direction_rad {
                        Some(a) if len <= 1e-6 => a.to_degrees(),
                        _ => (next.y - e.start.y).atan2(next.x - e.start.x).to_degrees(),
                    };
                    (e.start, len, angle)
                }
            };
            let _ = writeln!(
                s,
                "W {:2} {:2} {:.4} {:.4}{:10.5}{:10.4}{:5.1}",
                pm.model, pm.walls[i], start.x, start.y, len, angle, e.priority as f64
            );
            for (letter, sub, flag, h, ov, pitch) in records(e) {
                let _ = writeln!(
                    s,
                    "{letter} {sub:2} {:2} {flag:2}{h:10.4} {ov:.4}{pitch:10.4}",
                    0
                );
            }
        }
    }
    let holes = project
        .chimneys
        .iter()
        .map(|c| (c, c.height))
        .chain(project.roof_openings.iter().map(|c| (c, 0.0)));
    for (k, (c, h)) in holes.enumerate() {
        let _ = writeln!(s, "L {:2} {h:4.1} {:2}", k + 1, c.corners.len());
        for p in &c.corners {
            let _ = writeln!(s, "{:8.4} {:.4}", p.x, p.y);
        }
    }
    (s, map)
}

/// The `D`/`K` records of a wall: `(letter, sub, flag, height, overhang,
/// pitch)`. The first is the wall's own definition, the others the
/// half-hip and the pitch breaks.
fn records(e: &EdgeDef) -> Vec<(char, i32, i32, f64, f64, f64)> {
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
            let geo = crate::openings::build_roofs(&alone).pop().unwrap_or_default();
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
    for g in &geo.gables {
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
    let up = if rf.pitch_deg as f64 >= crate::model::GABLE_PITCH_DEG && (rf.pitch_deg as f64) < 90.001
    {
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
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), p| (a.min(p.1), b.max(p.1)));
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
    let mut geos = vec![RoofGeometry::default(); project.parts.len()];
    let p3 = |v: modell::V3| [v.x, v.y, v.z];
    // The part of each line: that of the first face piece using it.
    let mut owner: HashMap<usize, usize> = HashMap::new();
    let mut k = 0;
    for f in &out.state.faces {
        let n = f.pieces.len() + f.cutters.len();
        let part = map.find(f.rec.model, f.rec.face);
        for of in &out.faces[k..k + n] {
            if let (Some((pi, _)), FaceKind::Surface) = (part, of.kind) {
                for &(li, _) in &of.edges {
                    owner.entry(li).or_insert(pi);
                }
            }
        }
        k += n;
    }
    for (i, l) in out.lines.iter().enumerate() {
        if l.sign < 0 {
            continue;
        }
        let Some((kind, side)) = line_kind(l.code) else {
            continue;
        };
        let pi = owner.get(&i).copied().unwrap_or(0);
        if let Some(g) = geos.get_mut(pi) {
            g.lines.push(RoofLine {
                kind,
                a: p3(l.p1),
                b: p3(l.p2),
                side,
            });
        }
    }
    let mut k = 0;
    for f in &out.state.faces {
        let n = f.pieces.len() + f.cutters.len();
        let mine = &out.faces[k..k + n];
        k += n;
        let Some((pi, ei)) = map.find(f.rec.model, f.rec.face) else {
            continue;
        };
        let geo = &mut geos[pi];
        let vertical = (f.rec.angle - 90.0).abs() < 1e-3;
        let cut: f64 = mine
            .iter()
            .filter(|o| matches!(o.kind, FaceKind::Cut(_)))
            .map(|o| o.area)
            .sum();
        if vertical {
            if project.parts[pi].edges[ei].clad {
                geo.clad_area += mine.iter().map(|o| o.area).sum::<f64>();
            }
            continue;
        }
        let first = geo.faces.len();
        for o in mine.iter().filter(|o| o.kind == FaceKind::Surface) {
            let polygon = o
                .edges
                .iter()
                .filter_map(|&(li, s)| out.lines.get(li).map(|l| if s > 0 { l.p1 } else { l.p2 }))
                .map(p3)
                .collect();
            geo.faces.push(RoofFace {
                edge: ei,
                pitch_deg: f.rec.angle,
                polygon,
                area: o.area,
                deducted: 0.0,
            });
        }
        if let Some(face) = geo.faces.get_mut(first) {
            face.area += cut;
            face.deducted -= cut;
        }
    }
    geos
}
