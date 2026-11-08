//! The roof engine: DACAD's own core ([`crate::dcore`]) on the project's
//! RD2, as DACAD calculates a roof.
//!
//! 1. The project is written as DACAD's RD2 ([`write_rd2_with`]); a dormer
//!    needs the main-roof wall it sits on, which a first core run on the
//!    roofs alone tells ([`dormer_hosts`]).
//! 2. One roof: the core (flags `0x4a8`, holes cut by the core) writes the
//!    core SOL, which is the result ([`core_sol_result`]).
//! 3. Two or more roofs: the core (flags `0x4a0`) writes the core SOL and
//!    the INT; the wall stage of the core SOL gives the W3D, and the
//!    Modellverschnitt ([`crate::modell`]) joins the roofs.
//! 4. The SOL becomes one [`RoofGeometry`] per part
//!    ([`multiroof::sol_geometries`]); roof windows are deducted afterwards,
//!    as DACAD deducts them from the result.
//!
//! When the core ends with an error DACAD also ends with (a cut error such
//! as "Nicht geschlossene Fläche(n) vorhanden", no solution, a non-convex
//! chimney, an interval error, a core that does not terminate), that error
//! is the result: [`Failure::Dacad`]. Anything else that stops this path
//! is ours (an RD2 the core cannot read, a stage the port lacks, a file the
//! later stages cannot take): [`Failure::Ours`], and the caller falls back
//! to Tectora's own sweep engine.

use crate::check::{RoofWarning, WarningKind};
use crate::coresol::{CoreSol, Rd2, SolFace, wall_stage};
use crate::dcore::CoreError;
use crate::dcore::pipeline::{Outputs, Run};
use crate::dcore::state::Notice;
use crate::geom::{Point2, signed_area};
use crate::model::{Chimney, Project};
use crate::modell;
use crate::multiroof::{
    self, DormerHost, NegativeLines, Rd2Map, SolResult, SolResultFace, SolResultLine,
    write_rd2_with,
};
use crate::openings::{apply_window, inside, inside_spans};
use crate::roof::{CalcError, CalcErrorKind, OpeningKind, RoofGeometry};

/// Why the engine gives no roof.
#[derive(Debug, Clone, PartialEq)]
pub enum Failure {
    /// DACAD's calculation ends with this error too: report it.
    Dacad(CalcError),
    /// Something DACAD would not run into (our RD2, a gap of the port):
    /// fall back to the sweep engine.
    Ours(String),
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Failure::Dacad(e) => write!(f, "DACAD error: {e}"),
            Failure::Ours(e) => write!(f, "not calculated by the core: {e}"),
        }
    }
}

/// Flags of a run that writes the INT (several roofs; holes are cut by
/// the Modellverschnitt).
const FLAGS_INT: u32 = 0x4a0;
/// Flags of a run without INT (one roof; the core cuts the holes).
const FLAGS_SOL: u32 = 0x4a8;

/// The roof of every part, joined as DACAD joins them, with dormers,
/// chimneys, openings and windows. `project` must be valid with
/// counter-clockwise parts (as [`crate::openings::generate_project`]
/// prepares it).
pub fn generate(project: &Project) -> Result<Vec<RoofGeometry>, Failure> {
    if project.parts.is_empty() {
        return Err(Failure::Ours("no roof".into()));
    }
    let hosts = dormer_hosts(project)?;
    let mut geos = roofs(project, &hosts)?;
    if !project.roof_openings.is_empty() {
        record_opening_cuts(project, &hosts, &mut geos)?;
    }
    for w in &project.windows {
        apply_window(&mut geos, w);
    }
    Ok(geos)
}

/// The roofs with dormers, chimneys and openings, without windows.
fn roofs(project: &Project, hosts: &[Option<DormerHost>]) -> Result<Vec<RoofGeometry>, Failure> {
    let (text, map) = write_rd2_with(project, hosts);
    let rd2 = Rd2::parse(&text).map_err(|e| Failure::Ours(format!("RD2: {e}")))?;
    if multiroof::applies(project) {
        let out = run_core(&text, FLAGS_INT)?;
        let int = out
            .int
            .as_deref()
            .ok_or_else(|| Failure::Ours("the core wrote no INT".into()))?;
        let int = String::from_utf8_lossy(int);
        let sol = parse_sol(&out)?;
        let w3d = wall_stage(&sol)
            .map_err(|e| Failure::Ours(e.to_string()))?
            .w3d;
        let joined =
            modell::run(&int, Some(&w3d)).map_err(|e| Failure::Ours(format!("INT: {e}")))?;
        let mut geos = multiroof::to_geometries(project, &map, &joined);
        add_notices(&mut geos, &map, &out.notices);
        Ok(geos)
    } else {
        let out = run_core(&text, FLAGS_SOL)?;
        let sol = parse_sol(&out)?;
        let res = core_sol_result(project, &rd2, &sol);
        let mut geos = multiroof::sol_geometries(project, &map, &res);
        add_notices(&mut geos, &map, &out.notices);
        Ok(geos)
    }
}

/// The core's notices as warnings of the parts they concern: the walls in
/// line it calculated as one, so that one wall's pitch or pitch breaks
/// are not used. Dormer walls are left out (a dormer is entered as a
/// whole, not wall by wall).
fn add_notices(geos: &mut [RoofGeometry], map: &Rd2Map, notices: &[Notice]) {
    for n in notices {
        let (Notice::MergedWalls {
            model,
            kept,
            absorbed,
        }
        | Notice::IgnoredBreaks {
            model,
            kept,
            absorbed,
        }) = *n;
        let (Some((pk, ek)), Some((pa, ea))) = (map.find(model, kept), map.find(model, absorbed))
        else {
            continue;
        };
        if pk != pa || ek == usize::MAX || ea == usize::MAX {
            continue;
        }
        let Some(geo) = geos.get_mut(pk) else {
            continue;
        };
        let w = RoofWarning {
            kind: WarningKind::MergedWalls,
            value: 0.0,
            edges: vec![ek, ea],
        };
        if !geo.warnings.contains(&w) {
            geo.warnings.push(w);
        }
    }
}

/// Runs the core on an RD2 text.
fn run_core(rd2: &str, flags: u32) -> Result<Outputs, Failure> {
    let out = Run::new(flags).run(rd2.as_bytes()).map_err(classify)?;
    if out.code != 0 {
        // The fallback SOL of a wall chain that does not close: our
        // parts are closed, so the RD2 is at fault.
        return Err(Failure::Ours(format!("core return code {}", out.code)));
    }
    Ok(out)
}

fn parse_sol(out: &Outputs) -> Result<CoreSol, Failure> {
    CoreSol::parse(&String::from_utf8_lossy(&out.sol)).map_err(|e| Failure::Ours(e.to_string()))
}

/// Whether DACAD ends with the same error.
fn classify(e: CoreError) -> Failure {
    let dacad = |e: &CoreError, kind: CalcErrorKind, id: Option<u32>, message: String| {
        Failure::Dacad(CalcError {
            kind,
            code: e.dacad_code(),
            message_id: id,
            message,
        })
    };
    match &e {
        CoreError::Cut { msg, .. } => {
            let kind = if msg.id == crate::dcore::error::MSG_OPEN_FACES.id {
                CalcErrorKind::OpenFaces
            } else {
                CalcErrorKind::Cut
            };
            dacad(&e, kind, msg.id, e.to_string())
        }
        CoreError::NoSolution(msg) => {
            dacad(&e, CalcErrorKind::NoSolution, msg.id, msg.text.to_string())
        }
        // A non-convex chimney or opening: DACAD warns and cancels.
        CoreError::Fatal { code: 0, .. } => {
            let m = crate::dcore::holes::MSG_NONCONVEX;
            dacad(
                &e,
                CalcErrorKind::NonConvexOpening,
                m.id,
                m.text.to_string(),
            )
        }
        // An error of the geometry (interval, eave cut, area).
        CoreError::Fatal { code: 2, msg } => {
            dacad(&e, CalcErrorKind::Geometry, msg.id, msg.text.to_string())
        }
        CoreError::Hang(_) => dacad(
            &e,
            CalcErrorKind::NoEnd,
            None,
            "Die Dachberechnung kommt zu keinem Ende (DACAD hängt)".into(),
        ),
        // File and format errors: our RD2; gaps and faults of the port.
        CoreError::Fatal { .. }
        | CoreError::NoPlan { .. }
        | CoreError::Unimplemented(_)
        | CoreError::Fault(_) => Failure::Ours(e.to_string()),
    }
}

/// The plan polygon of a SOL face (the start of each edge).
fn face_points(sol: &CoreSol, f: &SolFace) -> Vec<[f64; 3]> {
    f.edges
        .iter()
        .filter_map(|&e| sol.line(e).map(|l| l.oriented(e).0))
        .map(|p| [p[0] as f64, p[1] as f64, p[2] as f64])
        .collect()
}

fn plan(points: &[[f64; 3]]) -> Vec<Point2> {
    points.iter().map(|p| Point2::new(p[0], p[1])).collect()
}

/// The main-roof wall each dormer sits on. A dormer imported from DACAD
/// names it (the V1 roof record; DACAD's `T` record takes that wall,
/// TRAING04: wall 8). Otherwise it is the wall of the topmost roof face
/// just behind the dormer's front, from a core run on the roofs alone
/// (DACAD knows the faces from its own calculation).
pub fn dormer_hosts(project: &Project) -> Result<Vec<Option<DormerHost>>, Failure> {
    let recorded = |d: &crate::model::Dormer| {
        let [a, b] = d.host_wall?;
        project.parts.iter().enumerate().find_map(|(pi, part)| {
            let n = part.edges.len();
            (0..n)
                .find(|&i| {
                    part.edges[i].start.distance(a) < 1e-3
                        && part.edges[(i + 1) % n].start.distance(b) < 1e-3
                })
                .map(|edge| DormerHost { part: pi, edge })
        })
    };
    let mut hosts: Vec<Option<DormerHost>> = project.dormers.iter().map(recorded).collect();
    if hosts.iter().all(Option::is_some) {
        return Ok(hosts);
    }
    let mut bare = project.clone();
    bare.dormers.clear();
    bare.chimneys.clear();
    bare.roof_openings.clear();
    let (text, map) = write_rd2_with(&bare, &[]);
    let rd2 = Rd2::parse(&text).map_err(|e| Failure::Ours(format!("RD2: {e}")))?;
    let sol = parse_sol(&run_core(&text, FLAGS_SOL)?)?;
    // Sloped faces as (plan polygon, plane, host).
    let mut faces = Vec::new();
    for f in &sol.faces {
        if f.ch != b'W' || f.sub < 0 {
            continue;
        }
        let Some((mi, ri)) = rd2.find_wall(b'W', f.wall) else {
            continue;
        };
        let m = &rd2.models[mi];
        let w = &m.walls[m.ring[ri].wall];
        let Some(rf) = w.faces.iter().skip(1).find(|x| x.sub == f.sub) else {
            continue;
        };
        if (rf.pitch_deg as f64 - 90.0).abs() <= 90.0 - crate::model::GABLE_PITCH_DEG
            || f.plane[2].abs() < 1e-6
        {
            continue;
        }
        let Some((part, edge)) = map.find(m.no, f.wall) else {
            continue;
        };
        let poly = plan(&face_points(&sol, f));
        if poly.len() < 3 || signed_area(&poly).abs() < 1e-9 {
            continue;
        }
        faces.push((poly, f.plane, DormerHost { part, edge }));
    }
    let z = |pl: [f32; 4], p: Point2| {
        (pl[3] as f64 - pl[0] as f64 * p.x - pl[1] as f64 * p.y) / pl[2] as f64
    };
    for (d, host) in project.dormers.iter().zip(hosts.iter_mut()) {
        if host.is_some() {
            continue;
        }
        let mut best: Option<(f64, DormerHost)> = None;
        for i in 0..d.edges.len() {
            let s = d.edges[i].start;
            let e = d.edges.get(i + 1).map_or(d.end, |x| x.start);
            let len = s.distance(e);
            if len < 1e-6 {
                continue;
            }
            let (ux, uy) = ((e.x - s.x) / len, (e.y - s.y) / len);
            // Just behind the wall, on the dormer's side (its left).
            let seed = Point2::new((s.x + e.x) / 2.0 - uy * 1e-3, (s.y + e.y) / 2.0 + ux * 1e-3);
            let top = faces
                .iter()
                .filter(|f| inside(&f.0, seed))
                .map(|f| (z(f.1, seed), f.2))
                .max_by(|a, b| a.0.total_cmp(&b.0));
            if let Some((zt, h)) = top
                && best.is_none_or(|b| zt < b.0)
            {
                best = Some((zt, h));
            }
        }
        *host = best.map(|b| b.1);
    }
    if hosts.iter().any(Option::is_none) {
        return Err(Failure::Ours("a dormer on no roof face".into()));
    }
    Ok(hosts)
}

/// The lines and faces of a core SOL. The face of a cut-out (a hole `L`,
/// a dormer's footprint `t`) is the roof face with the same wall and sub
/// number; a hole is told by the plan polygon it lies in, a dormer by its
/// run of `t` records.
pub fn core_sol_result(project: &Project, rd2: &Rd2, sol: &CoreSol) -> SolResult {
    let lines = sol
        .lines
        .iter()
        .map(|l| SolResultLine {
            a: l.p.map(f64::from),
            b: l.q.map(f64::from),
            code: l.code,
            negative: l.len < 0.0,
        })
        .collect();
    let holes: Vec<(&Chimney, OpeningKind)> = project
        .chimneys
        .iter()
        .map(|c| (c, OpeningKind::Chimney))
        .chain(project.roof_openings.iter().map(|c| (c, OpeningKind::Hole)))
        .collect();
    let refs = |f: &SolFace| -> Vec<(usize, i32)> {
        f.edges
            .iter()
            .map(|&e| ((e.unsigned_abs() as usize).saturating_sub(1), e.signum()))
            .collect()
    };
    let mut faces: Vec<SolResultFace> = Vec::new();
    // (letter, wall, sub) of each face in `faces`.
    let mut keys: Vec<(u8, i32, i32)> = Vec::new();
    let mut dormer = 0i64;
    let mut prev_t = false;
    for f in &sol.faces {
        match f.ch {
            b'W' | b'G' if f.sub >= 0 => {
                let Some((mi, ri)) = rd2.find_wall(f.ch, f.wall) else {
                    continue;
                };
                let m = &rd2.models[mi];
                let w = &m.walls[m.ring[ri].wall];
                let Some(rf) = w.faces.iter().skip(1).find(|x| x.sub == f.sub) else {
                    continue;
                };
                keys.push((f.ch, f.wall, f.sub));
                faces.push(SolResultFace {
                    model: m.no,
                    wall: f.wall,
                    pitch_deg: rf.pitch_deg as f64,
                    clad: rf.flag == 1,
                    surfaces: vec![(refs(f), f.area as f64)],
                    cuts: Vec::new(),
                });
            }
            b'L' | b't' => {
                let at = keys
                    .iter()
                    .position(|k| k == &(b'W', f.wall, f.sub))
                    .or_else(|| keys.iter().position(|k| k.1 == f.wall && k.2 == f.sub));
                let Some(at) = at else {
                    continue;
                };
                let opening = if f.ch == b't' {
                    if !prev_t {
                        dormer += 1;
                    }
                    Some((OpeningKind::Dormer, dormer))
                } else {
                    let pts = plan(&face_points(sol, f));
                    let n = pts.len().max(1) as f64;
                    let c = Point2::new(
                        pts.iter().map(|p| p.x).sum::<f64>() / n,
                        pts.iter().map(|p| p.y).sum::<f64>() / n,
                    );
                    holes
                        .iter()
                        .position(|(h, _)| inside(&h.corners, c))
                        .map(|k| (holes[k].1, k as i64))
                };
                faces[at].cuts.push((f.area as f64, opening));
            }
            _ => {}
        }
        prev_t = f.ch == b't';
    }
    SolResult {
        lines,
        faces,
        negative_lines: NegativeLines::Subtract,
    }
}

/// The roof line pieces under each roof opening, which DACAD's core takes
/// away: the lines of the roofs without the openings, inside them. The
/// norms give short pieces back.
fn record_opening_cuts(
    project: &Project,
    hosts: &[Option<DormerHost>],
    geos: &mut [RoofGeometry],
) -> Result<(), Failure> {
    let mut whole = project.clone();
    whole.roof_openings.clear();
    let before = roofs(&whole, hosts)?;
    for (g, b) in geos.iter_mut().zip(&before) {
        for l in &b.lines {
            let (a, e) = (Point2::new(l.a[0], l.a[1]), Point2::new(l.b[0], l.b[1]));
            for o in &project.roof_openings {
                let mut corners = o.corners.clone();
                if signed_area(&corners) < 0.0 {
                    corners.reverse();
                }
                for (s, t) in inside_spans(&corners, a, e) {
                    g.cut_lines
                        .push((l.dacad_code().to_string(), (t - s) * l.length()));
                }
            }
        }
    }
    Ok(())
}

/// The RD2 text and its map for a project, as [`generate`] writes it
/// (dormer hosts from a first core run).
pub fn rd2_text(project: &Project) -> Result<(String, Rd2Map), Failure> {
    let hosts = dormer_hosts(project)?;
    Ok(write_rd2_with(project, &hosts))
}
