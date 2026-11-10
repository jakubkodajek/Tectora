//! DACAD's dormer types (Gaubenarten) as parameter sets over the engine's
//! dormer: an open wall chain of a zero-length cheek, the front wall and a
//! second zero-length cheek, as DACAD stores it (cases C29-C31, C39, C76).
//! Port of the editor's `dormers.ts`.

use std::f64::consts::{FRAC_PI_2, TAU};

use serde::{Deserialize, Serialize};

use super::round::{ArcInput, ArcMode, MAX_ROUND_SEGMENTS, arc_for};
use super::{plain_edge, round_to};
use crate::geom::Point2;
use crate::model::{Dormer, EdgeDef, HalfHip, RoofPart};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DormerKind {
    Gable,
    Pointed,
    Hip,
    HalfHip,
    Shed,
    Trapezoid,
    Round,
    /// Any other chain, kept as it is.
    General,
}

/// The kinds that can be placed.
pub const DORMER_KINDS: [DormerKind; 7] = [
    DormerKind::Gable,
    DormerKind::Pointed,
    DormerKind::Hip,
    DormerKind::HalfHip,
    DormerKind::Shed,
    DormerKind::Trapezoid,
    DormerKind::Round,
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DormerParams {
    pub kind: DormerKind,
    /// Left corner of the front, seen from outside.
    pub start: Point2,
    pub width: f64,
    /// Direction of the front wall, radians (the roof rises to its left).
    pub angle: f64,
    pub eave_height: f64,
    /// Pitch of the cheek planes (gable, hip, half-hip, trapezoid).
    pub pitch_deg: f64,
    /// Pitch of the front plane (hip, shed, trapezoid).
    pub front_pitch_deg: f64,
    /// Overhang of the front (DACAD's dialog: front overhang in the section).
    pub overhang: f64,
    /// Overhang of the cheeks (DACAD's dialog: side overhang in the front
    /// view, the dormer roof's width = width + 2 × side overhang).
    pub side_overhang: f64,
    /// Krüppelwalm: height above the eave and pitch of the clipped front.
    pub half_hip_height: f64,
    pub half_hip_pitch_deg: f64,
    /// Rundgaube: polygon segments per cheek; 0 = DACAD's count.
    pub segments: u32,
    /// Roof material of all the dormer's walls.
    pub material: u8,
}

/// The values the editor fills in when a dormer is placed.
pub fn default_params(
    kind: DormerKind,
    start: Point2,
    angle: f64,
    eave_height: f64,
) -> DormerParams {
    DormerParams {
        kind,
        start,
        width: 2.0,
        angle,
        eave_height,
        pitch_deg: 45.0,
        front_pitch_deg: if matches!(kind, DormerKind::Shed | DormerKind::Trapezoid) {
            15.0
        } else {
            45.0
        },
        overhang: 0.0,
        side_overhang: 0.0,
        half_hip_height: 0.6,
        half_hip_pitch_deg: 60.0,
        segments: 0,
        material: crate::model::DEFAULT_MATERIAL,
    }
}

const GABLE_PITCH: f64 = 90.0;

pub fn build_dormer(p: &DormerParams) -> Dormer {
    let (sin, cos) = p.angle.sin_cos();
    let end = Point2::new(
        round_to(p.start.x + cos * p.width, 3),
        round_to(p.start.y + sin * p.width, 3),
    );
    let sloped = p.kind != DormerKind::Shed;
    let front_sloped = matches!(
        p.kind,
        DormerKind::Hip | DormerKind::Shed | DormerKind::Trapezoid
    );
    let base = |start: Point2, pitch: f64, overhang: f64| {
        let mut e = plain_edge(start, pitch, p.eave_height, overhang);
        e.material = p.material;
        e
    };
    // A Rundgaube is a gable dormer whose cheeks follow the Runddach arc
    // (DACAD.HLP); DACAD's default arc is assumed.
    let arc = (p.kind == DormerKind::Round)
        .then(|| {
            arc_for(&ArcInput {
                span: p.width,
                overhang: p.side_overhang,
                eave_height: p.eave_height,
                // A count above the cap (an older file, the editor before the
                // cap) keeps the dormer round with the most segments allowed,
                // instead of turning it into another kind (MATH-018).
                segments: p.segments.min(MAX_ROUND_SEGMENTS),
                mode: ArcMode::Default,
                value: 0.0,
            })
        })
        .flatten();
    let cheek = |dir: f64, start: Point2| {
        let pitch = match &arc {
            Some(a) => a.pitch_deg,
            None if sloped => p.pitch_deg,
            None => GABLE_PITCH,
        };
        let mut e = base(start, pitch, p.side_overhang);
        e.direction_rad = Some(dir.rem_euclid(TAU));
        if let Some(a) = &arc {
            e.breaks = a.breaks.clone();
        }
        e
    };
    let front_pitch = if !front_sloped {
        GABLE_PITCH
    } else if p.kind == DormerKind::Hip {
        p.pitch_deg
    } else {
        p.front_pitch_deg
    };
    let mut front = base(p.start, front_pitch, p.overhang);
    if p.kind == DormerKind::HalfHip {
        front.half_hip = Some(HalfHip {
            height: round_to(p.eave_height + p.half_hip_height, 3),
            pitch_deg: p.half_hip_pitch_deg,
        });
    }
    Dormer {
        edges: vec![
            cheek(p.angle - FRAC_PI_2, p.start),
            front,
            cheek(p.angle + FRAC_PI_2, end),
        ],
        end,
        host_wall: None,
    }
}

fn is_gable(e: &EdgeDef) -> bool {
    (e.pitch_deg - 90.0).abs() <= 0.001
}

/// Reads a dormer back as parameters; any other chain is `General`.
pub fn parse_dormer(d: &Dormer) -> DormerParams {
    let f = d.edges.get(1);
    let start = f.or(d.edges.first()).map_or(d.end, |e| e.start);
    let angle = (d.end.y - start.y).atan2(d.end.x - start.x);
    let mut p = DormerParams {
        width: start.distance(d.end),
        overhang: f.map_or(0.0, |f| f.overhang),
        side_overhang: d.edges.first().or(f).map_or(0.0, |e| e.overhang),
        material: f.map_or(crate::model::DEFAULT_MATERIAL, |f| f.material),
        ..default_params(
            DormerKind::General,
            start,
            angle,
            f.or(d.edges.first()).map_or(0.0, |e| e.eave_height),
        )
    };
    let [a, f, b] = d.edges.as_slice() else {
        return p;
    };
    if a.direction_rad.is_none() || b.direction_rad.is_none() || f.direction_rad.is_some() {
        return p;
    }
    let cheeks_gable = is_gable(a) && is_gable(b);
    let cheeks_sloped = !is_gable(a) && !is_gable(b) && (a.pitch_deg - b.pitch_deg).abs() < 1e-6;
    if cheeks_gable && !is_gable(f) {
        p.kind = DormerKind::Shed;
        p.front_pitch_deg = f.pitch_deg;
        return p;
    }
    if !cheeks_sloped {
        return p;
    }
    p.pitch_deg = a.pitch_deg;
    if is_gable(f) && !a.breaks.is_empty() {
        p.kind = DormerKind::Round;
        p.segments = a.breaks.len() as u32 + 1;
    } else if is_gable(f) {
        if let Some(h) = &f.half_hip {
            p.kind = DormerKind::HalfHip;
            p.half_hip_height = round_to(h.height - f.eave_height, 3);
            p.half_hip_pitch_deg = h.pitch_deg;
        } else {
            p.kind = DormerKind::Gable;
        }
    } else if (f.pitch_deg - a.pitch_deg).abs() < 1e-6 {
        p.kind = DormerKind::Hip;
    } else {
        p.kind = DormerKind::Trapezoid;
        p.front_pitch_deg = f.pitch_deg;
    }
    p
}

/// Direction of the outline wall nearest to `p` (a dormer's front runs along it).
pub fn nearest_wall_angle(parts: &[RoofPart], p: Point2) -> f64 {
    let mut best = (f64::INFINITY, 0.0);
    for part in parts {
        let es = &part.edges;
        for (i, e) in es.iter().enumerate() {
            let b = es[(i + 1) % es.len()].start;
            let (dx, dy) = (b.x - e.start.x, b.y - e.start.y);
            let l2 = dx * dx + dy * dy;
            if l2 < 1e-12 || is_gable(e) {
                continue;
            }
            let t = (((p.x - e.start.x) * dx + (p.y - e.start.y) * dy) / l2).clamp(0.0, 1.0);
            let d = (e.start.x + t * dx - p.x).hypot(e.start.y + t * dy - p.y);
            if d < best.0 {
                best = (d, dy.atan2(dx));
            }
        }
    }
    best.1
}

/// The roof a dormer stands on, at the middle of its front: height and
/// rise per metre along the dormer's axis (into the roof, to the left of
/// the front), as DACAD's dormer dialog shows it (host pitch, e.g. 38°).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HostSlope {
    pub height: f64,
    pub tan: f64,
}

/// The side section of DACAD's dormer dialog (Gaubeneingabe): what follows
/// from the dormer's parameters and the roof under it. Lengths are measured
/// at the walls, without overhangs. `None` where the dormer does not meet
/// the roof behind it (its top is as steep as the roof or steeper).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DormerSection {
    pub host_pitch_deg: f64,
    /// Front wall height above the roof at the middle of the front.
    pub wall_height: f64,
    /// Ridge height above ground; `None` for a shed dormer.
    pub ridge_height: Option<f64>,
    /// Horizontal distance from the front to where the dormer's ridge (or
    /// shed roof) runs into the roof.
    pub depth: Option<f64>,
    /// Height of that point above the roof at the front.
    pub top_height: Option<f64>,
    /// Ridge length from its front end to the roof; `None` for a shed dormer.
    pub ridge_length: Option<f64>,
    /// Slope length of the dormer roof: a shed dormer's roof from the front
    /// to the roof, the other kinds' cheek from eave to ridge.
    pub roof_length: Option<f64>,
    /// Slope length of the roof from the front to the dormer's top.
    pub host_length: Option<f64>,
    /// Trapezoid dormer: width of its shed roof where it meets the roof
    /// (0 when the cheeks close in a ridge first).
    pub back_width: Option<f64>,
}

/// Which section value the eave height is solved for.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionTarget {
    WallHeight(f64),
    Depth(f64),
    RidgeLength(f64),
}

/// Smallest difference of slopes for which a dormer's top still meets the
/// roof [no source: editor choice, ~0.06°].
const MIN_TAN: f64 = 1e-3;

fn tan_deg(d: f64) -> f64 {
    d.to_radians().tan()
}

/// Rise and slope length of a wall's profile (pitch, then its breaks) over
/// a horizontal run from the wall.
fn profile(e: &EdgeDef, run: f64) -> (f64, f64) {
    let (mut z, mut x, mut len, mut pitch) = (e.eave_height, 0.0, 0.0, e.pitch_deg);
    let mut breaks: Vec<_> = e.breaks.iter().filter(|b| b.height > z).collect();
    breaks.sort_by(|a, b| a.height.total_cmp(&b.height));
    for b in breaks {
        let t = tan_deg(pitch);
        if t <= 0.0 {
            break;
        }
        let dx = (b.height - z) / t;
        if x + dx >= run {
            break;
        }
        len += dx / pitch.to_radians().cos();
        x += dx;
        z = b.height;
        pitch = b.pitch_deg;
    }
    let rest = run - x;
    (
        z + rest * tan_deg(pitch) - e.eave_height,
        len + rest / pitch.to_radians().cos(),
    )
}

/// The cheek wall a ridge dormer's profile starts from (its eave lies
/// `side_overhang` outside it, as on any wall).
fn cheek(p: &DormerParams) -> Option<EdgeDef> {
    if p.kind == DormerKind::Shed || p.kind == DormerKind::General {
        return None;
    }
    build_dormer(p).edges.into_iter().next()
}

/// Height of the dormer's wall top above its eave: the eave is at the
/// overhang's edge, the wall that overhang further in (DACAD eave height,
/// engine check `section_depth_matches_the_calculated_dormer`). A shed
/// dormer's front wall carries its roof; the other kinds' cheek walls do.
fn wall_rise(p: &DormerParams) -> f64 {
    match cheek(p) {
        Some(c) => profile(&c, p.side_overhang).0,
        None => p.overhang * tan_deg(p.front_pitch_deg),
    }
}

/// Ridge rise above the dormer eave and the cheek's slope length from the
/// wall to the ridge.
fn ridge_rise(p: &DormerParams) -> Option<(f64, f64)> {
    let c = cheek(p)?;
    let (rise, len) = profile(&c, p.width / 2.0 + p.side_overhang);
    Some((rise, len - profile(&c, p.side_overhang).1))
}

/// How far behind the front wall the ridge starts; a gable front's ridge
/// runs out over its verge overhang (negative).
fn ridge_start(p: &DormerParams, rise: f64) -> f64 {
    let ov = p.overhang;
    let start = match p.kind {
        DormerKind::Hip => rise / tan_deg(p.pitch_deg) - ov,
        DormerKind::Trapezoid => rise / tan_deg(p.front_pitch_deg) - ov,
        DormerKind::HalfHip => (rise - p.half_hip_height) / tan_deg(p.half_hip_pitch_deg) - ov,
        _ => -ov,
    };
    start.max(-ov)
}

/// Smallest slope difference between a shed dormer and its roof the depth
/// is taken from: below it 1 mm of eave moves the meeting line by more than
/// 10 cm [no source: editor choice, MATH-003].
const MIN_SHED_TAN: f64 = 1e-2;

/// Where a sloped front plane (shed, trapezoid) runs into the roof: the
/// depth behind the front wall and the plane's rise above the eave there.
/// `None` when the plane is as steep as the roof or the front wall's top
/// is under the roof.
fn front_plane_top(p: &DormerParams, host: HostSlope) -> Option<(f64, f64)> {
    let tf = tan_deg(p.front_pitch_deg);
    let wall = p.eave_height + p.overhang * tf - host.height;
    let k = host.tan - tf;
    let d = (k > MIN_SHED_TAN && wall >= -1e-6).then(|| wall.max(0.0) / k)?;
    Some((d, (d + p.overhang) * tf))
}

/// A trapezoid dormer whose shed roof runs into the roof before its cheeks
/// meet: (depth, rise of the shed roof's top above the eave). Otherwise
/// the cheeks close in a ridge and it is calculated like a hip dormer.
fn trapezoid_top(p: &DormerParams, host: HostSlope) -> Option<(f64, f64)> {
    if p.kind != DormerKind::Trapezoid {
        return None;
    }
    let (ridge, _) = ridge_rise(p)?;
    front_plane_top(p, host).filter(|&(_, rise)| rise < ridge)
}

/// Width of a trapezoid dormer's shed roof where it meets the roof, between
/// the lines along which the cheeks cut it: the eave width (width + 2 ×
/// side overhang) less the cheeks' run up to the shed roof's top.
fn back_width(p: &DormerParams, rise: f64) -> f64 {
    p.width + 2.0 * p.side_overhang - 2.0 * rise / tan_deg(p.pitch_deg)
}

/// DACAD's dormer dialog section for a dormer on `host` (closed form; the
/// shed case is checked against the dialog's own numbers in the tests).
pub fn section(p: &DormerParams, host: HostSlope) -> DormerSection {
    let wall = p.eave_height + wall_rise(p) - host.height;
    let host_cos = host.tan.atan().cos();
    let mut s = DormerSection {
        host_pitch_deg: host.tan.atan().to_degrees(),
        wall_height: wall,
        ridge_height: None,
        depth: None,
        top_height: None,
        ridge_length: None,
        roof_length: None,
        host_length: None,
        back_width: None,
    };
    let depth = if p.kind == DormerKind::Shed {
        let d = front_plane_top(p, host).map(|(d, _)| d);
        s.roof_length = d.map(|d| d / p.front_pitch_deg.to_radians().cos());
        d
    } else if let Some((d, rise)) = trapezoid_top(p, host) {
        s.roof_length = Some(d / p.front_pitch_deg.to_radians().cos());
        s.back_width = Some(back_width(p, rise));
        Some(d)
    } else if let Some((rise, cheek)) = ridge_rise(p) {
        let top = p.eave_height + rise - host.height;
        s.ridge_height = Some(p.eave_height + rise);
        s.roof_length = Some(cheek);
        let d = (host.tan > MIN_TAN && top >= 0.0).then(|| top / host.tan);
        s.ridge_length = d.map(|d| (d - ridge_start(p, rise)).max(0.0));
        if p.kind == DormerKind::Trapezoid && d.is_some() {
            s.back_width = Some(0.0);
        }
        d
    } else {
        None
    };
    let finite = |v: Option<f64>| v.filter(|v| v.is_finite());
    s.depth = finite(depth);
    s.ridge_length = finite(s.ridge_length);
    s.roof_length = finite(s.roof_length);
    s.back_width = finite(s.back_width);
    s.top_height = s.depth.map(|d| d * host.tan);
    s.host_length = s.depth.map(|d| d / host_cos);
    s
}

fn pitches_ok(p: &DormerParams) -> bool {
    let ok = |v: f64| v > 0.0 && v < 90.0;
    let pitches = match p.kind {
        DormerKind::Shed => ok(p.front_pitch_deg),
        DormerKind::Trapezoid => ok(p.pitch_deg) && ok(p.front_pitch_deg),
        DormerKind::HalfHip => ok(p.pitch_deg) && ok(p.half_hip_pitch_deg),
        _ => ok(p.pitch_deg) || p.kind == DormerKind::Round,
    };
    let len = |v: f64| v.is_finite() && v >= 0.0;
    pitches && p.width > 0.0 && p.width.is_finite() && len(p.overhang) && len(p.side_overhang)
}

const BAD_PARAMS: &str = "the dormer's pitches must be between 0 and 90 degrees, its width positive and its overhangs 0 or more";

/// The eave height that gives the dormer `target` on `host`.
pub fn eave_for(p: &DormerParams, host: HostSlope, target: SectionTarget) -> Result<f64, String> {
    let positive = |v: f64, name: &str| {
        if v.is_finite() && v > 0.0 {
            Ok(v)
        } else {
            Err(format!("{name} must be a positive number"))
        }
    };
    if !pitches_ok(p) {
        return Err(BAD_PARAMS.into());
    }
    let eave = match target {
        SectionTarget::WallHeight(h) => {
            if !h.is_finite() || h < 0.0 {
                return Err("wall_height must be 0 or more".into());
            }
            host.height + h - wall_rise(p)
        }
        SectionTarget::Depth(d) | SectionTarget::RidgeLength(d) => {
            let d = positive(
                d,
                if matches!(target, SectionTarget::Depth(_)) {
                    "depth"
                } else {
                    "ridge_length"
                },
            )?;
            let tf = tan_deg(p.front_pitch_deg);
            if p.kind == DormerKind::Shed {
                if matches!(target, SectionTarget::RidgeLength(_)) {
                    return Err("a shed dormer has no ridge; give depth".into());
                }
                let k = host.tan - tf;
                if k <= MIN_SHED_TAN {
                    return Err("the dormer roof is as steep as the roof or steeper, so its depth follows from nothing; make it flatter".into());
                }
                host.height + d * k - wall_rise(p)
            } else {
                let (rise, _) = ridge_rise(p).ok_or("this dormer has no ridge")?;
                if host.tan <= MIN_TAN {
                    return Err(
                        "the roof under the dormer is flat, so its depth follows from nothing"
                            .into(),
                    );
                }
                // A trapezoid dormer whose shed roof reaches the roof first.
                let shed_top = (d + p.overhang) * tf;
                if p.kind == DormerKind::Trapezoid
                    && matches!(target, SectionTarget::Depth(_))
                    && host.tan - tf > MIN_SHED_TAN
                    && shed_top < rise
                {
                    host.height + d * host.tan - shed_top
                } else {
                    let depth = match target {
                        SectionTarget::RidgeLength(l) => l + ridge_start(p, rise),
                        _ => d,
                    };
                    host.height + depth * host.tan - rise
                }
            }
        }
    };
    if !eave.is_finite() {
        return Err("the dormer's pitches give no eave height for this; check them".into());
    }
    if eave + wall_rise(p) < host.height - 1e-9 {
        return Err(
            "the dormer's front wall would end below the roof; give a longer depth or ridge".into(),
        );
    }
    Ok(round_to(eave, 3))
}

/// The cheek pitch that gives a trapezoid dormer on `host` the back width
/// `width` (DACAD's trapezoid dialog derives the cheek pitch from its
/// widths). The shed roof's top does not depend on the cheeks, so the pitch
/// follows in closed form; the result is rounded to 0.01°.
pub fn pitch_for_back_width(p: &DormerParams, host: HostSlope, width: f64) -> Result<f64, String> {
    if p.kind != DormerKind::Trapezoid {
        return Err("only a trapezoid dormer has a back width".into());
    }
    let eave_width = p.width + 2.0 * p.side_overhang;
    if !width.is_finite() || width <= 0.0 || width >= eave_width {
        return Err(format!(
            "back_width must be more than 0 and less than the dormer roof's width {eave_width:.3} m"
        ));
    }
    let mut q = p.clone();
    q.pitch_deg = 45.0;
    if !pitches_ok(&q) {
        return Err(BAD_PARAMS.into());
    }
    let (_, rise) = front_plane_top(&q, host).ok_or(
        "the dormer roof does not run into the roof (it is as steep as the roof or under it), so it has no back width",
    )?;
    if rise <= 1e-6 {
        return Err("the dormer has no front wall above the roof, so it has no back width".into());
    }
    let pitch = (2.0 * rise / (eave_width - width)).atan().to_degrees();
    let pitch = round_to(pitch, 2);
    if !(pitch > 0.0 && pitch < 90.0) {
        return Err(
            "this back width needs cheeks steeper than 90 degrees; give a wider one".into(),
        );
    }
    Ok(pitch)
}

/// The dormer with `target` (solved for its eave height) and a trapezoid's
/// `back_width` (solved for its cheek pitch) on `host`. A trapezoid's wall
/// height depends on the cheek pitch through the side overhang, so the two
/// are solved in turn until the rounded values settle.
pub fn solve(
    p: &DormerParams,
    host: HostSlope,
    target: Option<SectionTarget>,
    back_width: Option<f64>,
) -> Result<DormerParams, String> {
    let mut q = p.clone();
    for _ in 0..20 {
        let before = (q.eave_height, q.pitch_deg);
        if let Some(t) = target {
            q.eave_height = eave_for(&q, host, t)?;
        }
        if let Some(b) = back_width {
            q.pitch_deg = pitch_for_back_width(&q, host, b)?;
        }
        if target.is_none() || back_width.is_none() || before == (q.eave_height, q.pitch_deg) {
            break;
        }
    }
    Ok(q)
}

/// The roof under dormer `index` at the middle of its front (DACAD's
/// dialog reads the host pitch from that roof face).
pub fn host_slope(project: &crate::Project, index: usize) -> Option<HostSlope> {
    let p = parse_dormer(project.dormers.get(index)?);
    host_slope_in(&host_roof(project), &p)
}

/// The calculated roof without its dormers: what every dormer stands on.
pub fn host_roof(project: &crate::Project) -> Vec<crate::roof::RoofGeometry> {
    let mut rest = project.clone();
    rest.dormers.clear();
    // A roof the core cannot calculate gives no host, not a crash.
    crate::model::guarded(|| crate::openings::generate_project(&rest)).unwrap_or_default()
}

/// [`host_slope`] on a calculated roof that leaves the dormer out.
pub fn host_slope_in(geos: &[crate::roof::RoofGeometry], p: &DormerParams) -> Option<HostSlope> {
    let (sin, cos) = p.angle.sin_cos();
    let mid = Point2::new(
        p.start.x + cos * p.width / 2.0,
        p.start.y + sin * p.width / 2.0,
    );
    let axis = (-sin, cos);
    // Just inside the front, so a front standing on a ridge or a valley
    // takes the face the dormer stands on.
    let probe = Point2::new(mid.x + axis.0 * 0.01, mid.y + axis.1 * 0.01);
    let (z, g) = super::probe::host_plane_at(geos, probe)?;
    let tan = g[0] * axis.0 + g[1] * axis.1;
    let height = z - (g[0] * axis.0 + g[1] * axis.1) * 0.01;
    (height.is_finite() && tan.is_finite()).then_some(HostSlope { height, tan })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_reads_back_as_itself() {
        for kind in DORMER_KINDS {
            let mut p = default_params(kind, Point2::new(3.0, 0.5), 0.0, 4.2);
            p.width = 2.4;
            let d = build_dormer(&p);
            assert_eq!(d.edges.len(), 3);
            let back = parse_dormer(&d);
            // A pointed dormer is a gable dormer with other numbers.
            let want = if kind == DormerKind::Pointed {
                DormerKind::Gable
            } else {
                kind
            };
            assert_eq!(back.kind, want, "{kind:?}");
            assert!((back.width - 2.4).abs() < 1e-9);
        }
    }

    #[test]
    fn round_dormer_cheeks_carry_the_arc() {
        let mut p = default_params(DormerKind::Round, Point2::new(0.0, 0.0), 0.0, 4.0);
        p.width = 2.0;
        let d = build_dormer(&p);
        assert!(!d.edges[0].breaks.is_empty());
        assert_eq!(d.edges[0].breaks, d.edges[2].breaks);
        assert!(is_gable(&d.edges[1]));
    }

    #[test]
    fn a_round_dormer_with_too_many_segments_stays_round() {
        // MATH-018: 40 segments (the editor before its cap) used to rebuild as
        // a gable dormer with 45° cheeks on any MCP update_dormer.
        let mut p = default_params(DormerKind::Round, Point2::new(0.0, 0.0), 0.0, 4.0);
        p.width = 2.0;
        p.segments = 40;
        let d = build_dormer(&p);
        assert_eq!(parse_dormer(&d).kind, DormerKind::Round);
        assert_eq!(d.edges[0].breaks.len() + 1, MAX_ROUND_SEGMENTS as usize);
    }

    fn near(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn shed_section_matches_the_dormer_dialog() {
        // DACAD's dormer dialog (shed, screenshot 2026-11-10): roof 38°,
        // dormer 22°, front wall 1.400 m -> depth 3.711, top 2.899, dormer
        // roof 4.002, roof slope 4.709.
        let mut p = default_params(DormerKind::Shed, Point2::new(0.0, 0.0), 0.0, 4.4);
        p.front_pitch_deg = 22.0;
        let host = HostSlope {
            height: 3.0,
            tan: tan_deg(38.0),
        };
        let s = section(&p, host);
        assert!(near(s.wall_height, 1.4, 1e-9));
        assert!(near(s.host_pitch_deg, 38.0, 1e-9));
        assert!(near(s.depth.unwrap(), 3.711, 5e-4), "{s:?}");
        assert!(near(s.top_height.unwrap(), 2.899, 5e-4));
        assert!(near(s.roof_length.unwrap(), 4.002, 5e-4));
        assert!(near(s.host_length.unwrap(), 4.709, 5e-4));
        assert_eq!(s.ridge_length, None);
        // And back: the depth gives the wall height.
        let e = eave_for(&p, host, SectionTarget::Depth(s.depth.unwrap())).unwrap();
        assert!(near(e, 4.4, 1e-3));
        assert_eq!(
            eave_for(&p, host, SectionTarget::WallHeight(1.4)).unwrap(),
            4.4
        );
    }

    #[test]
    fn a_dormer_steeper_than_its_roof_has_no_depth() {
        let mut p = default_params(DormerKind::Shed, Point2::new(0.0, 0.0), 0.0, 4.0);
        p.front_pitch_deg = 40.0;
        let host = HostSlope {
            height: 3.0,
            tan: tan_deg(38.0),
        };
        assert_eq!(section(&p, host).depth, None);
        assert!(eave_for(&p, host, SectionTarget::Depth(2.0)).is_err());
        let flat = HostSlope {
            height: 3.0,
            tan: 0.0,
        };
        let g = default_params(DormerKind::Gable, Point2::new(0.0, 0.0), 0.0, 4.0);
        assert_eq!(section(&g, flat).depth, None);
        assert!(eave_for(&g, flat, SectionTarget::RidgeLength(2.0)).is_err());
        assert!(eave_for(&g, host, SectionTarget::WallHeight(-0.5)).is_err());
        assert!(eave_for(&g, host, SectionTarget::Depth(f64::NAN)).is_err());
    }

    #[test]
    fn degenerate_pitches_give_an_error_not_an_endless_eave() {
        // MATH-001: half-hip pitch 0 made the eave infinite, saved as null.
        let host = HostSlope {
            height: 3.0,
            tan: tan_deg(40.0),
        };
        let mut p = default_params(DormerKind::HalfHip, Point2::new(0.0, 0.0), 0.0, 4.0);
        p.half_hip_pitch_deg = 0.0;
        assert!(eave_for(&p, host, SectionTarget::RidgeLength(1.0)).is_err());
        p.kind = DormerKind::Hip;
        p.pitch_deg = 0.0;
        assert!(eave_for(&p, host, SectionTarget::RidgeLength(1.0)).is_err());
        let s = section(&p, host);
        assert!(
            [s.depth, s.ridge_length, s.roof_length]
                .iter()
                .flatten()
                .all(|v| v.is_finite())
        );
        // MATH-006: a shed with no front wall still meets the roof at its front.
        let mut shed = default_params(DormerKind::Shed, Point2::new(0.0, 0.0), 0.0, 4.0);
        shed.eave_height = eave_for(&shed, host, SectionTarget::WallHeight(0.0)).unwrap();
        assert!(section(&shed, host).depth.is_some());
    }

    #[test]
    fn ridge_lengths_follow_the_front() {
        // Gable 2 m wide, 45°: ridge 1 m above the eave; roof 40°.
        let host = HostSlope {
            height: 3.0,
            tan: tan_deg(40.0),
        };
        let mut p = default_params(DormerKind::Gable, Point2::new(0.0, 0.0), 0.0, 4.0);
        let s = section(&p, host);
        let d = 2.0 / tan_deg(40.0);
        assert!(near(s.ridge_height.unwrap(), 5.0, 1e-9));
        assert!(near(s.depth.unwrap(), d, 1e-9));
        assert!(near(s.ridge_length.unwrap(), d, 1e-9));
        assert!(near(s.roof_length.unwrap(), 2f64.sqrt(), 1e-9));
        // A hip front starts the ridge half the width behind it.
        p.kind = DormerKind::Hip;
        assert!(near(section(&p, host).ridge_length.unwrap(), d - 1.0, 1e-9));
        // Ridge length back to the eave.
        let e = eave_for(&p, host, SectionTarget::RidgeLength(d - 1.0)).unwrap();
        assert!(near(e, 4.0, 1e-3));
    }

    #[test]
    fn a_trapezoid_dormer_has_a_back_width_and_gives_its_cheek_pitch() {
        // 2 m wide, side overhang 0.2, front 15° with 0.3 overhang, cheeks
        // 45°, front wall 1.2 m on a 40° roof: the shed roof meets the roof
        // at d = 1.2 / (tan 40° − tan 15°) before the cheeks close.
        let host = HostSlope {
            height: 3.0,
            tan: tan_deg(40.0),
        };
        let mut p = default_params(DormerKind::Trapezoid, Point2::new(0.0, 0.0), 0.0, 0.0);
        p.overhang = 0.3;
        p.side_overhang = 0.2;
        p.eave_height = eave_for(&p, host, SectionTarget::WallHeight(1.0)).unwrap();
        let s = section(&p, host);
        let front_wall = p.eave_height + 0.3 * tan_deg(15.0) - host.height;
        let d = front_wall / (tan_deg(40.0) - tan_deg(15.0));
        let rise = (d + 0.3) * tan_deg(15.0);
        assert!(near(s.depth.unwrap(), d, 1e-9), "{s:?}");
        assert!(near(s.back_width.unwrap(), 2.4 - 2.0 * rise, 1e-9));
        assert_eq!(s.ridge_height, None);
        assert!(near(
            s.roof_length.unwrap(),
            d / 15f64.to_radians().cos(),
            1e-9
        ));
        // The depth gives the eave back.
        let e = eave_for(&p, host, SectionTarget::Depth(d)).unwrap();
        assert!(near(e, p.eave_height, 1e-3));
        // A back width gives the cheek pitch that makes it.
        let pitch = pitch_for_back_width(&p, host, 1.0).unwrap();
        p.pitch_deg = pitch;
        assert!(near(section(&p, host).back_width.unwrap(), 1.0, 1e-3));
        assert!(near(
            pitch_for_back_width(&p, host, s.back_width.unwrap()).unwrap(),
            45.0,
            1e-2
        ));
        for bad in [0.0, -1.0, 2.4, 3.0, f64::NAN] {
            assert!(pitch_for_back_width(&p, host, bad).is_err(), "{bad}");
        }
        // Wall height and back width together, with a side overhang that
        // ties the wall to the cheek pitch.
        let q = solve(&p, host, Some(SectionTarget::WallHeight(1.1)), Some(0.8)).unwrap();
        let qs = section(&q, host);
        assert!(near(qs.wall_height, 1.1, 2e-3), "{qs:?}");
        assert!(near(qs.back_width.unwrap(), 0.8, 2e-3), "{qs:?}");
        let mut shed = p.clone();
        shed.kind = DormerKind::Shed;
        assert!(pitch_for_back_width(&shed, host, 1.0).is_err());
        p.overhang = -0.1;
        assert!(eave_for(&p, host, SectionTarget::WallHeight(1.0)).is_err());
    }

    #[test]
    fn front_and_side_overhangs_go_to_their_walls_and_read_back() {
        for kind in DORMER_KINDS {
            let mut p = default_params(kind, Point2::new(3.0, 0.5), 0.0, 4.2);
            p.overhang = 0.5;
            p.side_overhang = 0.25;
            let d = build_dormer(&p);
            assert_eq!(d.edges[1].overhang, 0.5, "{kind:?}");
            assert_eq!(d.edges[0].overhang, 0.25, "{kind:?}");
            assert_eq!(d.edges[2].overhang, 0.25, "{kind:?}");
            let back = parse_dormer(&d);
            assert_eq!((back.overhang, back.side_overhang), (0.5, 0.25), "{kind:?}");
        }
    }

    /// The dormer's faces in the calculated roof, as plan points.
    fn dormer_points(p: &crate::Project) -> Vec<[f64; 3]> {
        crate::openings::generate_project(p)
            .iter()
            .flat_map(|g| &g.faces)
            .filter(|f| f.edge == usize::MAX)
            .flat_map(|f| f.polygon.iter().copied())
            .collect()
    }

    #[test]
    fn section_depth_matches_the_calculated_dormer() {
        // A 12 × 8 house, eave 3 m, 38° on the long walls, gables on the
        // short ones; a dormer on the front slope, no overhangs.
        let mut house = crate::Project::new("t");
        let w = |x: f64, y: f64, pitch: f64| plain_edge(Point2::new(x, y), pitch, 3.0, 0.0);
        house.parts.push(RoofPart {
            name: "A".into(),
            edges: vec![
                w(0.0, 0.0, 38.0),
                w(12.0, 0.0, 90.0),
                w(12.0, 8.0, 38.0),
                w(0.0, 8.0, 90.0),
            ],
        });
        // Front and side overhangs apart and together; the trapezoid both
        // with a shed top (15°) and closing in a ridge (30°).
        let overhangs = [(0.0, 0.0), (0.3, 0.3), (0.5, 0.2), (0.1, 0.4)];
        let mut cases = Vec::new();
        for k in DORMER_KINDS {
            let fronts: &[f64] = if k == DormerKind::Trapezoid {
                &[15.0, 30.0]
            } else {
                &[15.0]
            };
            for &f in fronts {
                cases.extend(overhangs.iter().map(|&o| (k, f, o)));
            }
        }
        for (kind, front, (ov, side)) in cases {
            let ov = (ov, side);
            let mut p = default_params(kind, Point2::new(5.0, 1.0), 0.0, 3.0);
            p.width = 2.0;
            p.overhang = ov.0;
            p.side_overhang = ov.1;
            p.front_pitch_deg = front;
            let mut project = house.clone();
            project.dormers.push(build_dormer(&p));
            let host = host_slope(&project, 0).expect("host");
            assert!(near(host.tan, tan_deg(38.0), 1e-6), "{kind:?} {host:?}");
            assert!(near(host.height, 3.0 + tan_deg(38.0), 1e-6));
            p.eave_height = eave_for(&p, host, SectionTarget::WallHeight(0.9)).unwrap();
            if kind == DormerKind::Pointed {
                p.eave_height = round_to(host.height - wall_rise(&p), 3);
            }
            project.dormers[0] = build_dormer(&p);
            let s = section(&p, host);
            let pts = dormer_points(&project);
            assert!(!pts.is_empty(), "{kind:?}");
            // Deepest dormer point behind the front (y = 1).
            let deep = pts.iter().map(|q| q[1] - 1.0).fold(f64::MIN, f64::max);
            assert!(
                near(deep, s.depth.unwrap(), 2e-2),
                "{kind:?} {front}° ov {ov:?}: engine {deep} vs {s:?}"
            );
            if let Some(r) = s.ridge_height {
                let top = pts.iter().map(|q| q[2]).fold(f64::MIN, f64::max);
                assert!(near(top, r, 2e-2), "{kind:?} ov {ov:?}: ridge {top} vs {r}");
            }
            if let Some(want) = s.back_width {
                // The dormer's width at its deepest points.
                let xs: Vec<f64> = pts
                    .iter()
                    .filter(|q| q[1] - 1.0 > deep - 1e-3)
                    .map(|q| q[0])
                    .collect();
                let span = xs.iter().fold(f64::MIN, |a, &b| a.max(b))
                    - xs.iter().fold(f64::MAX, |a, &b| a.min(b));
                assert!(
                    near(span, want, 2e-2),
                    "{kind:?} {front}° ov {ov:?}: back width {span} vs {want}"
                );
            }
            assert_eq!(
                s.back_width.is_some(),
                kind == DormerKind::Trapezoid,
                "{kind:?}"
            );
            if let Some(want) = s.ridge_length.filter(|_| kind != DormerKind::Round) {
                // The ridge lines over the dormer at its ridge height.
                let ridge: f64 = crate::openings::generate_project(&project)
                    .iter()
                    .flat_map(|g| &g.lines)
                    .filter(|l| l.kind == crate::roof::LineKind::Ridge)
                    .filter(|l| {
                        near(l.a[2], s.ridge_height.unwrap(), 1e-2) && near(l.b[2], l.a[2], 1e-6)
                    })
                    .filter(|l| (5.0..=7.0).contains(&l.a[0]))
                    .map(|l| (l.a[0] - l.b[0]).hypot(l.a[1] - l.b[1]))
                    .sum();
                assert!(
                    near(ridge, want, 2e-2),
                    "{kind:?} {front}° ov {ov:?}: ridge length {ridge} vs {want}"
                );
            }
        }
    }
}
