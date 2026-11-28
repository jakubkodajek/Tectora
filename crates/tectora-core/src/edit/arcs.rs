//! Curved outlines: a rounded corner and a wall bent into an arc, both as
//! short straight walls along the arc. DACAD has no curved wall; its help
//! takes a round building as a polygon of walls (DACAD.HLP via the
//! 2026-11-08 gap analysis), and its round roof (Runddach) splits an arc
//! into segments of 11.25°, or of 15° where such a segment would be
//! shorter than 0.4 m ([`super::round`], DACAD runs RD1-RD4, RGa-RGs). The
//! arcs here use the same steps, so the roof core sees what DACAD would be
//! given for such a building: each new wall carries the roof definition of
//! the wall it replaces.

// `!(x > 0.0)` and the like are deliberate: they also refuse NaN.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use std::f64::consts::PI;

use super::round_to;
use crate::geom::Point2;
use crate::model::{EdgeDef, RoofPart};

/// The codes that end the refusal of an arc on a wall, corner or building
/// the outline does not have: no value of the arc is wrong (audit
/// ERR-004). The app translates by them.
pub const ARC_NO_WALL: &str = "arc_no_wall";
pub const ARC_NO_CORNER: &str = "arc_no_corner";
pub const ARC_NO_PART: &str = "arc_no_part";
/// The codes that end the refusal of an arc's values (audit r2 ARCH-005):
/// a wall count out of range, walls too short, corners turning too little,
/// a bend or radius out of range, walls in line, a wall without length, an
/// arc touching another wall, an outline made invalid.
pub const ARC_SEGMENTS: &str = "arc_segments";
pub const ARC_TOO_SHORT: &str = "arc_too_short";
pub const ARC_FLAT: &str = "arc_flat";
pub const ARC_BULGE: &str = "arc_bulge";
pub const ARC_RADIUS_MIN: &str = "arc_radius_min";
pub const ARC_RADIUS_MAX: &str = "arc_radius_max";
pub const ARC_STRAIGHT: &str = "arc_straight";
pub const ARC_NO_LENGTH: &str = "arc_no_length";
pub const ARC_TOUCHES: &str = "arc_touches";
pub const ARC_INVALID: &str = "arc_invalid";
/// The code of a straightened or reshaped arc on a wall that is no part of
/// an arc (audit r2 UX-007).
pub const ARC_NONE: &str = "arc_none";
/// Every code an arc's refusal ends with.
pub const ARC_CODES: [&str; 14] = [
    ARC_NO_WALL,
    ARC_NO_CORNER,
    ARC_NO_PART,
    ARC_SEGMENTS,
    ARC_TOO_SHORT,
    ARC_FLAT,
    ARC_BULGE,
    ARC_RADIUS_MIN,
    ARC_RADIUS_MAX,
    ARC_STRAIGHT,
    ARC_NO_LENGTH,
    ARC_TOUCHES,
    ARC_INVALID,
    ARC_NONE,
];

/// Most walls one arc may become.
pub const MAX_ARC_SEGMENTS: usize = 64;

/// DACAD's segment angle and the coarser one for short segments (as
/// [`super::round`]).
const SEGMENT_ARC: f64 = 11.25;
const SHORT_SEGMENT_ARC: f64 = 15.0;
const SHORT_CHORD: f64 = 0.4;

/// Walls for an arc of `sweep` radians on radius `r`: DACAD's 11.25° steps,
/// 15° when a step's chord would be under 0.4 m, at least 2.
pub fn default_segments(r: f64, sweep: f64) -> usize {
    let chord = |deg: f64| 2.0 * r * (deg.to_radians() / 2.0).sin();
    let step = if chord(SEGMENT_ARC) < SHORT_CHORD {
        SHORT_SEGMENT_ARC
    } else {
        SEGMENT_ARC
    };
    ((sweep.abs().to_degrees() / step).ceil() as usize).clamp(2, MAX_ARC_SEGMENTS)
}

/// The shortest wall an arc may have, m. The roof core merges points
/// within 2 cm and takes walls under about 12.5 cm with visible error
/// (audit MATH-003: at r = 0.1 m the roof was 0.49 m² too large; r2
/// MATH-003: four corners of 10 to 12 cm walls 0.08 to 0.11 m² over, no
/// warning). DACAD's own round roof never makes a step under 0.4 m.
pub const MIN_ARC_WALL: f64 = 0.15;

/// The least an arc's corners may turn, degrees: the roof core fails on
/// walls meeting nearly in line, or takes those under its 0.5° angle
/// tolerance as one wall with the area off (audit r2 MATH-002: a
/// semicircle of 54 walls, corners of 1.67° where it meets the walls,
/// had no roof; MATH-004: a 5 cm bow of a 10 m wall, 1.15°, went to the
/// fallback, a 2 cm one was 0.11 m² off). DACAD's own steps are 11.25°
/// and 15°; the round dormer refuses segments under 2° alike.
pub const MIN_ARC_TURN_DEG: f64 = 2.0;

/// The walls of an arc of `sweep` radians on radius `r`: the given count,
/// or DACAD's steps with fewer walls where they would be shorter than
/// [`MIN_ARC_WALL`]; refused when even that leaves a wall too short.
fn segments_or_default(segments: Option<usize>, r: f64, sweep: f64) -> Result<usize, String> {
    let chord = |n: usize| 2.0 * r * (sweep.abs() / (2.0 * n as f64)).sin();
    let n = match segments {
        None => {
            let mut n = default_segments(r, sweep);
            while n > 2 && chord(n) < MIN_ARC_WALL {
                n -= 1;
            }
            n
        }
        Some(n) if (2..=MAX_ARC_SEGMENTS).contains(&n) => n,
        Some(n) => {
            return Err(format!(
                "an arc has 2 to {MAX_ARC_SEGMENTS} walls, not {n} ({ARC_SEGMENTS})"
            ));
        }
    };
    if chord(n) < MIN_ARC_WALL {
        return Err(format!(
            "the arc's walls would be shorter than {MIN_ARC_WALL} m: give a larger radius or fewer walls ({ARC_TOO_SHORT})"
        ));
    }
    Ok(n)
}

/// The refusal of an arc whose corners turn under [`MIN_ARC_TURN_DEG`].
fn too_flat() -> String {
    format!(
        "the arc's corners would turn by less than {MIN_ARC_TURN_DEG}°: give a larger bend or fewer walls ({ARC_FLAT})"
    )
}

/// How much the outline `pts` turns at corner `k`, degrees.
fn turn_deg(pts: &[Point2], k: usize) -> f64 {
    let n = pts.len();
    let (a, b, c) = (pts[(k + n - 1) % n], pts[k], pts[(k + 1) % n]);
    let (ux, uy) = (b.x - a.x, b.y - a.y);
    let (vx, vy) = (c.x - b.x, c.y - b.y);
    (ux * vy - uy * vx)
        .atan2(ux * vx + uy * vy)
        .abs()
        .to_degrees()
}

/// Whether some corner `from..=to` (indices into `part`'s outline, taken
/// round) turns under [`MIN_ARC_TURN_DEG`].
fn turns_too_little(part: &RoofPart, from: usize, to: usize) -> bool {
    let pts = part.outline();
    let n = pts.len();
    (from..=to).any(|k| turn_deg(&pts, k % n) < MIN_ARC_TURN_DEG)
}

/// The plan distance of `p` from the wall `a`–`b`.
fn to_segment(p: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let l2 = dx * dx + dy * dy;
    let s = if l2 > 0.0 {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    p.distance(Point2::new(a.x + dx * s, a.y + dy * s))
}

fn p3(p: Point2) -> Point2 {
    Point2::new(round_to(p.x, 4), round_to(p.y, 4))
}

/// Replaces wall `i` of `part` (counter-clockwise outline) by an arc whose
/// middle stands `bulge` m off the straight wall, outward for a positive
/// bulge, inward for a negative one; at most half the wall's length (a
/// semicircle). The new walls carry wall `i`'s roof definition; a gable
/// takes a neighbouring eave's instead (an apse). Returns how many walls
/// the arc has.
pub fn bend_wall(
    part: &mut RoofPart,
    i: usize,
    bulge: f64,
    segments: Option<usize>,
) -> Result<usize, String> {
    let (done, segs) = bent(part, i, bulge, segments)?;
    *part = done;
    Ok(segs)
}

/// [`bend_wall`] on a copy of `part`: the new part and its arc's walls.
fn bent(
    part: &RoofPart,
    i: usize,
    bulge: f64,
    segments: Option<usize>,
) -> Result<(RoofPart, usize), String> {
    let n = part.edges.len();
    if i >= n {
        return Err(format!("wall {i} does not exist ({ARC_NO_WALL})"));
    }
    let a = part.edges[i].start;
    let b = part.edges[(i + 1) % n].start;
    let l = a.distance(b);
    if !(l > 0.01) {
        return Err(format!("the wall has no length ({ARC_NO_LENGTH})"));
    }
    if !(bulge.is_finite() && bulge.abs() >= 0.005 && bulge.abs() <= l / 2.0 + 1e-9) {
        return Err(format!(
            "the bend must be at least 0.005 m and at most half the wall ({:.3} m) ({ARC_BULGE})",
            l / 2.0
        ));
    }
    let ccw = crate::geom::signed_area(&part.outline()) >= 0.0;
    // Outward is to the right of a wall of a counter-clockwise outline.
    let (ux, uy) = ((b.x - a.x) / l, (b.y - a.y) / l);
    let (rx, ry) = if ccw { (uy, -ux) } else { (-uy, ux) };
    let s = bulge.abs().min(l / 2.0);
    let r = (l * l / 4.0 + s * s) / (2.0 * s);
    let sweep = 4.0 * (2.0 * s / l).atan();
    let segs = segments_or_default(segments, r, sweep)?;
    let mid = Point2::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
    let sign = bulge.signum();
    let apex = Point2::new(mid.x + rx * s * sign, mid.y + ry * s * sign);
    let c = Point2::new(apex.x - rx * r * sign, apex.y - ry * r * sign);
    let a0 = (a.y - c.y).atan2(a.x - c.x);
    let am = (apex.y - c.y).atan2(apex.x - c.x);
    let dir = if ((a0 + sweep / 2.0) - am)
        .rem_euclid(2.0 * PI)
        .min(2.0 * PI - ((a0 + sweep / 2.0) - am).rem_euclid(2.0 * PI))
        < 1e-6
    {
        1.0
    } else {
        -1.0
    };
    if sweep.to_degrees() / (segs as f64) < MIN_ARC_TURN_DEG {
        return Err(too_flat());
    }
    let mut work = part.clone();
    let part = &mut work;
    // A gable bent into an arc is an apse: its walls are eaves with the
    // roof of a neighbouring eave wall, a conical half hip, as a curved
    // gable (a verge along an arc) is not built (audit UX-001).
    let mut template = part.edges[i].clone();
    if template.is_gable() {
        let prev = part.edges[(i + n - 1) % n].clone();
        let next = part.edges[(i + 1) % n].clone();
        if let Some(eave) = [prev, next].into_iter().find(|e| !e.is_gable()) {
            template = EdgeDef {
                start: template.start,
                ..eave
            };
            part.edges[i] = template.clone();
        }
    }
    let new: Vec<EdgeDef> = (1..segs)
        .map(|k| {
            let t = a0 + dir * sweep * k as f64 / segs as f64;
            EdgeDef {
                start: p3(Point2::new(c.x + r * t.cos(), c.y + r * t.sin())),
                source: None,
                direction_rad: None,
                ..template.clone()
            }
        })
        .collect();
    // An arc reaching another wall of the building (an inward one up to
    // the opposite wall) leaves no roof (audit r2 MATH-002 "also seen").
    if n >= 4 {
        let outline = part.outline();
        let far_walls = (0..n).filter(|&j| j != i && j != (i + 1) % n && j != (i + n - 1) % n);
        for j in far_walls {
            let (wa, wb) = (outline[j], outline[(j + 1) % n]);
            if new
                .iter()
                .any(|e| to_segment(e.start, wa, wb) < MIN_ARC_WALL)
            {
                return Err(format!(
                    "the arc would touch another wall of the building ({ARC_TOUCHES})"
                ));
            }
        }
    }
    part.edges[i].source = None;
    part.edges.splice(i + 1..i + 1, new);
    // Where the arc meets the walls beside it, too.
    if turns_too_little(part, i, i + segs) {
        return Err(too_flat());
    }
    let done = std::mem::replace(part, RoofPart::new("", Vec::new()));
    Ok((done, segs))
}

/// Rounds the corner at the start of wall `v` of `part` with radius
/// `radius`: both walls are shortened by the tangent length and an arc of
/// walls joins them. The arc walls carry the roof definition of the wall
/// before the corner, or of wall `v` when that one is a gable and wall `v`
/// is not. Returns how many walls the arc has.
pub fn round_corner(
    part: &mut RoofPart,
    v: usize,
    radius: f64,
    segments: Option<usize>,
) -> Result<usize, String> {
    let n = part.edges.len();
    if v >= n || n < 3 {
        return Err(format!("corner {v} does not exist ({ARC_NO_CORNER})"));
    }
    if !(radius.is_finite() && radius >= 0.01) {
        return Err(format!(
            "the radius must be at least 0.01 m ({ARC_RADIUS_MIN})"
        ));
    }
    let prev = part.edges[(v + n - 1) % n].start;
    let p = part.edges[v].start;
    let next = part.edges[(v + 1) % n].start;
    let (l1, l2) = (p.distance(prev), p.distance(next));
    if !(l1 > 0.01 && l2 > 0.01) {
        return Err(format!(
            "a wall at the corner has no length ({ARC_NO_LENGTH})"
        ));
    }
    let e1 = Point2::new((prev.x - p.x) / l1, (prev.y - p.y) / l1);
    let e2 = Point2::new((next.x - p.x) / l2, (next.y - p.y) / l2);
    let theta = (e1.x * e2.x + e1.y * e2.y).clamp(-1.0, 1.0).acos();
    if !(theta > 1e-3 && theta < PI - 1e-3) {
        return Err(format!(
            "the walls at the corner run straight on ({ARC_STRAIGHT})"
        ));
    }
    let half = theta / 2.0;
    let t = radius / half.tan();
    // Each wall keeps at least MIN_ARC_WALL, or ends exactly at the arc
    // is not allowed either: a zero-length wall is a problem.
    if t > l1 - MIN_ARC_WALL || t > l2 - MIN_ARC_WALL {
        return Err(format!(
            "the radius is too large for the walls at the corner (at most {:.3} m) ({ARC_RADIUS_MAX})",
            ((l1.min(l2) - MIN_ARC_WALL).max(0.0)) * half.tan()
        ));
    }
    let t1 = Point2::new(p.x + e1.x * t, p.y + e1.y * t);
    let t2 = Point2::new(p.x + e2.x * t, p.y + e2.y * t);
    let bis = Point2::new(e1.x + e2.x, e1.y + e2.y);
    let bl = bis.x.hypot(bis.y);
    let c = Point2::new(
        p.x + bis.x / bl * radius / half.sin(),
        p.y + bis.y / bl * radius / half.sin(),
    );
    let sweep = PI - theta;
    let segs = segments_or_default(segments, radius, sweep)?;
    // The arc's corners turn by its step, half a step where it meets the
    // walls (tangent to them).
    if sweep.to_degrees() / (segs as f64) / 2.0 < MIN_ARC_TURN_DEG {
        return Err(too_flat());
    }
    let a0 = (t1.y - c.y).atan2(t1.x - c.x);
    let a1 = (t2.y - c.y).atan2(t2.x - c.x);
    // The short way round from t1 to t2.
    let mut d = a1 - a0;
    while d > PI {
        d -= 2.0 * PI;
    }
    while d < -PI {
        d += 2.0 * PI;
    }
    let before = part.edges[(v + n - 1) % n].clone();
    let after = part.edges[v].clone();
    let template = if before.is_gable() && !after.is_gable() {
        after.clone()
    } else {
        before
    };
    let mut arc: Vec<EdgeDef> = Vec::with_capacity(segs + 1);
    // The arc starts at t1 (where the wall before now ends) and its last
    // wall ends at t2, where wall v now starts.
    for k in 0..segs {
        let a = a0 + d * k as f64 / segs as f64;
        let q = if k == 0 {
            t1
        } else {
            Point2::new(c.x + radius * a.cos(), c.y + radius * a.sin())
        };
        arc.push(EdgeDef {
            start: p3(q),
            source: None,
            direction_rad: None,
            ..template.clone()
        });
    }
    let mut moved = after;
    moved.start = p3(t2);
    moved.source = None;
    part.edges[v] = moved;
    part.edges.splice(v..v, arc);
    if let Some(e) = part.edges.get_mut((v + n - 1 + segs) % (n + segs)) {
        e.source = None;
    }
    Ok(segs)
}

/// A run of walls that an arc made: `walls` consecutive walls from wall
/// `start` (taken round the outline) of one length whose corners all turn
/// by the same angle, with the bend and wall count [`bend_wall`] would
/// give it on its chord (audit r2 UX-007). The outline stores no arc
/// (ADR 0008): the run is found by its shape.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct ArcRun {
    pub start: usize,
    pub walls: usize,
    /// How far the arc's middle stands off its chord, m, + outward.
    pub bulge: f64,
    /// The chord's length, m.
    pub chord: f64,
}

/// The fewest walls a run must have to be taken as an arc: two equal walls
/// at a corner are any isosceles corner.
pub const MIN_RUN_WALLS: usize = 3;
/// The most an arc's corners may turn, degrees: three walls of a
/// semicircle turn by 60°; a rectangle's 90° corners are no arc.
const MAX_RUN_TURN_DEG: f64 = 60.0;
/// How much the walls of one arc may differ in length, m, and their turns,
/// degrees: arc corners are stored to 0.1 mm, so a 0.15 m wall's direction
/// is off by at most 0.03°.
const RUN_LENGTH_TOL: f64 = 2e-3;
const RUN_TURN_TOL_DEG: f64 = 0.1;

/// The signed turn of the outline at corner `k`, degrees (+ to the left).
fn signed_turn_deg(pts: &[Point2], k: usize) -> f64 {
    let n = pts.len();
    let (a, b, c) = (pts[(k + n - 1) % n], pts[k], pts[(k + 1) % n]);
    let (ux, uy) = (b.x - a.x, b.y - a.y);
    let (vx, vy) = (c.x - b.x, c.y - b.y);
    (ux * vy - uy * vx).atan2(ux * vx + uy * vy).to_degrees()
}

/// The arc that wall `wall` of `part` belongs to, or `None` when it is no
/// part of one: the longest run of at least [`MIN_RUN_WALLS`] walls of the
/// wall's length whose corners inside the run turn alike (between
/// [`MIN_ARC_TURN_DEG`] and 60°), leaving at least three other walls'
/// worth of outline (a regular polygon is a building, not an arc).
pub fn arc_run(part: &RoofPart, wall: usize) -> Option<ArcRun> {
    let n = part.edges.len();
    if wall >= n || n < MIN_RUN_WALLS + 2 {
        return None;
    }
    let pts = part.outline();
    if !pts.iter().all(|p| p.x.is_finite() && p.y.is_finite()) {
        return None;
    }
    let len = |k: usize| pts[k % n].distance(pts[(k + 1) % n]);
    let l = len(wall);
    if !(l > 0.01) || part.edges[wall].direction_rad.is_some() {
        return None;
    }
    let same = |k: usize| (len(k) - l).abs() <= RUN_LENGTH_TOL;
    // The run grown both ways from `wall` with corners turning by `turn`.
    let grow = |turn: f64| -> (usize, usize) {
        let fits =
            |corner: usize| (signed_turn_deg(&pts, corner % n) - turn).abs() <= RUN_TURN_TOL_DEG;
        let (mut start, mut walls) = (wall, 1usize);
        // Back: wall start-1 joins when it has the length and the corner at
        // `start` turns alike.
        while walls < n - 2 {
            let prev = (start + n - 1) % n;
            if !(same(prev) && fits(start)) {
                break;
            }
            start = prev;
            walls += 1;
        }
        while walls < n - 2 {
            let next = (start + walls) % n;
            if !(same(next) && fits(next)) {
                break;
            }
            walls += 1;
        }
        // A run that ends only at the cap is a regular outline (an octagon),
        // no arc: one more wall would close it.
        let prev = (start + n - 1) % n;
        let next = (start + walls) % n;
        let bounded = !(same(prev) && fits(start)) && !(same(next) && fits(next));
        (start, if bounded { walls } else { 0 })
    };
    let seeds = [wall, (wall + 1) % n].map(|c| signed_turn_deg(&pts, c));
    let (start, walls, turn) = seeds
        .into_iter()
        // Both bounds with the tolerance: a three-wall semicircle turns by
        // 60° a few thousandths over or under once stored (audit r3
        // ARCH-001).
        .filter(|t| {
            (MIN_ARC_TURN_DEG - RUN_TURN_TOL_DEG..=MAX_RUN_TURN_DEG + RUN_TURN_TOL_DEG)
                .contains(&t.abs())
        })
        .map(|t| {
            let (s, w) = grow(t);
            (s, w, t)
        })
        .max_by_key(|&(_, w, _)| w)?;
    if walls < MIN_RUN_WALLS {
        return None;
    }
    let (a, b) = (pts[start], pts[(start + walls) % n]);
    let chord = a.distance(b);
    // The bulge from the circle through the arc's ends and its corner
    // farthest from the chord (all lie on it): the turns, summed, would
    // carry each corner's 0.1 mm rounding 16 times into a semicircle's.
    let off = |p: Point2| to_segment(p, a, b);
    let mid = (1..walls)
        .map(|k| pts[(start + k) % n])
        .max_by(|p, q| off(*p).total_cmp(&off(*q)))?;
    let (pa, pb) = (mid.distance(a), mid.distance(b));
    let twice_area = ((mid.x - a.x) * (b.y - a.y) - (mid.y - a.y) * (b.x - a.x)).abs();
    if !(twice_area > 1e-9) {
        return None;
    }
    let r = pa * pb * chord / (2.0 * twice_area);
    let s = (r - (r * r - chord * chord / 4.0).max(0.0).sqrt()).min(chord / 2.0);
    let ccw = crate::geom::signed_area(&pts) >= 0.0;
    let bulge = if (turn > 0.0) == ccw { s } else { -s };
    (bulge.is_finite() && chord > 0.01).then_some(ArcRun {
        start,
        walls,
        bulge: round_to(bulge, 4),
        chord,
    })
}

/// The sharp corner a rounded corner replaced: when the arc `run` of
/// `part` meets the walls on either side tangentially (its end corners
/// turn by half its step, as [`round_corner`] makes them) and those walls,
/// run on, meet ahead of both, the point where they meet. `None` for any
/// other arc (a bent wall, or a semicircle between parallel walls).
pub fn rounded_corner(part: &RoofPart, run: &ArcRun) -> Option<Point2> {
    let n = part.edges.len();
    if run.walls < MIN_RUN_WALLS || run.walls + 2 > n {
        return None;
    }
    let pts = part.outline();
    let (first, last) = (run.start, (run.start + run.walls) % n);
    let step = signed_turn_deg(&pts, (run.start + 1) % n);
    let tangent = |k: usize| (signed_turn_deg(&pts, k) - step / 2.0).abs() <= RUN_TURN_TOL_DEG;
    if !(tangent(first) && tangent(last)) {
        return None;
    }
    // The wall before the arc ends at its start, the one after starts at
    // its end.
    let (a0, a1) = (pts[(first + n - 1) % n], pts[first]);
    let (b0, b1) = (pts[last], pts[(last + 1) % n]);
    let (dx, dy) = (a1.x - a0.x, a1.y - a0.y);
    let (ex, ey) = (b1.x - b0.x, b1.y - b0.y);
    let den = dx * ey - dy * ex;
    if !(den.abs() > 1e-9 * dx.hypot(dy) * ex.hypot(ey)) {
        return None;
    }
    // a1 + s·d = b0 - t·e, with s and t ahead of both walls.
    let (wx, wy) = (b0.x - a1.x, b0.y - a1.y);
    let s = (wx * ey - wy * ex) / den;
    let t = (dx * wy - dy * wx) / den;
    let p = Point2::new(a1.x + s * dx, a1.y + s * dy);
    (s > 0.0 && t > 0.0 && p.x.is_finite() && p.y.is_finite()).then_some(p)
}

/// Replaces the arc that wall `wall` belongs to ([`arc_run`]) by straight
/// walls again. A rounded corner ([`rounded_corner`]) becomes its sharp
/// corner: the walls on either side run on to where they meet, each with
/// its own roof definition (audit r3 UX-004: it became a chamfer). Any
/// other arc becomes one straight wall from its start to its end, with the
/// roof definition of its first wall: a gable bent into an apse comes back
/// as that eave (the bend gave its walls the neighbouring eave's
/// definition and the outline keeps no note of the gable). Returns the
/// index of the straight wall, or of the wall starting at the restored
/// corner; refused with [`ARC_NONE`] when the wall is no part of an arc.
pub fn straighten(part: &mut RoofPart, wall: usize) -> Result<usize, String> {
    let n = part.edges.len();
    if wall >= n {
        return Err(format!("wall {wall} does not exist ({ARC_NO_WALL})"));
    }
    let Some(run) = arc_run(part, wall) else {
        return Err(format!("wall {wall} is no part of an arc ({ARC_NONE})"));
    };
    let corner = rounded_corner(part, &run);
    // A rounded corner loses all its walls, the wall after it starting at
    // the corner; any other arc keeps its first wall as the chord.
    let from = usize::from(corner.is_none());
    let gone: Vec<usize> = (from..run.walls).map(|k| (run.start + k) % n).collect();
    let kept = run.start - gone.iter().filter(|&&k| k < run.start).count();
    let mut k = 0;
    part.edges.retain(|_| {
        let keep = !gone.contains(&k);
        k += 1;
        keep
    });
    let kept = kept % part.edges.len().max(1);
    if let Some(e) = part.edges.get_mut(kept) {
        if let Some(p) = corner {
            e.start = p3(p);
        }
        e.source = None;
    }
    Ok(kept)
}

/// "Change the radius" of a rounded corner: the corner wall `wall`'s arc
/// rounded ([`rounded_corner`]) made sharp again ([`straighten`]) and
/// rounded with `radius` and `segments` ([`round_corner`]). Returns the
/// corner's index (the wall starting there) and the new arc's wall count;
/// refused with [`ARC_NONE`] when the wall is no part of a rounded corner,
/// and the part is left as it was when either step is refused.
pub fn change_corner_radius(
    part: &mut RoofPart,
    wall: usize,
    radius: f64,
    segments: Option<usize>,
) -> Result<(usize, usize), String> {
    let n = part.edges.len();
    if wall >= n {
        return Err(format!("wall {wall} does not exist ({ARC_NO_WALL})"));
    }
    if arc_run(part, wall).is_none_or(|run| rounded_corner(part, &run).is_none()) {
        return Err(format!(
            "wall {wall} is no part of a rounded corner ({ARC_NONE})"
        ));
    }
    let mut work = part.clone();
    let v = straighten(&mut work, wall)?;
    let segs = round_corner(&mut work, v, radius, segments)?;
    *part = work;
    Ok((v, segs))
}

/// "Change the arc": the arc that wall `wall` belongs to straightened
/// ([`straighten`]) and its chord bent again with `bulge` and `segments`
/// ([`bend_wall`]). Returns the new arc's first wall and its wall count;
/// the part is left as it was when either step is refused.
pub fn reshape(
    part: &mut RoofPart,
    wall: usize,
    bulge: f64,
    segments: Option<usize>,
) -> Result<(usize, usize), String> {
    let mut work = part.clone();
    let i = straighten(&mut work, wall)?;
    let segs = bend_wall(&mut work, i, bulge, segments)?;
    *part = work;
    Ok((i, segs))
}

/// Why an arc made an outline invalid: the first problem of a building in
/// `after` that `before` did not have (an outline crossing itself, a wall
/// too short); `None` when there is none. The editor's command and MCP
/// `edit_walls` both refuse such an arc (audit ARCH-005).
pub fn outline_spoiled(before: &crate::Project, after: &crate::Project) -> Option<String> {
    let had: std::collections::HashSet<_> = before
        .problems()
        .into_iter()
        .map(|p| (p.item, p.index, p.code))
        .collect();
    after
        .problems()
        .into_iter()
        .find(|p| {
            p.item == crate::model::ProblemItem::Part && !had.contains(&(p.item, p.index, p.code))
        })
        .map(|p| {
            format!(
                "the arc would make the outline invalid: {} ({ARC_INVALID})",
                after.describe(&p)
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::plain_edge;

    fn square(s: f64) -> RoofPart {
        let pts = [(0.0, 0.0), (s, 0.0), (s, s), (0.0, s)];
        RoofPart::new(
            "A",
            pts.iter()
                .map(|&(x, y)| plain_edge(Point2::new(x, y), 30.0, 3.0, 0.5))
                .collect(),
        )
    }

    #[test]
    fn a_rounded_corner_is_tangent_to_both_walls_and_keeps_the_radius() {
        let mut p = square(10.0);
        let segs = round_corner(&mut p, 2, 3.0, None).unwrap();
        // 90° at 11.25° steps (chords of 0.59 m): 8 walls.
        assert_eq!(segs, 8);
        assert_eq!(p.edges.len(), 4 + 8);
        // The corner (10, 10) is gone; the arc runs from (10, 7) to (7, 10)
        // around (7, 7).
        assert!(
            p.edges
                .iter()
                .all(|e| e.start.distance(Point2::new(10.0, 10.0)) > 0.5)
        );
        let arc: Vec<Point2> = p.edges[2..2 + 8].iter().map(|e| e.start).collect();
        assert!(arc[0].approx_eq(Point2::new(10.0, 7.0), 1e-4));
        for q in &arc {
            assert!((q.distance(Point2::new(7.0, 7.0)) - 3.0).abs() < 1e-3);
        }
        assert!(p.edges[10].start.approx_eq(Point2::new(7.0, 10.0), 1e-4));
        // Plan area: the square less the 3 × 3 corner plus the quarter
        // circle as 8 triangles of 11.25°.
        let area = crate::geom::signed_area(&p.outline());
        let sector = 8.0 * 0.5 * 9.0 * (PI / 16.0).sin();
        assert!((area - (100.0 - 9.0 + sector)).abs() < 1e-3, "{area}");
        let mut prj = crate::Project::new("t");
        prj.parts.push(p);
        assert!(prj.problems().is_empty(), "{:?}", prj.problems());
    }

    #[test]
    fn a_small_radius_takes_dacads_coarser_steps() {
        // Radius 2: an 11.25° chord would be 0.39 m, under 0.4 m, so 15°.
        let mut p = square(10.0);
        assert_eq!(round_corner(&mut p, 2, 2.0, None), Ok(6));
    }

    #[test]
    fn a_bent_wall_bulges_outward_by_the_given_amount() {
        let mut p = square(10.0);
        let segs = bend_wall(&mut p, 0, 1.0, Some(6)).unwrap();
        assert_eq!(segs, 6);
        assert_eq!(p.edges.len(), 4 + 5);
        // Wall 0 runs from (0, 0) to (10, 0); outward is -y.
        let mid = p.edges[3].start;
        assert!(mid.approx_eq(Point2::new(5.0, -1.0), 1e-4), "{mid:?}");
        assert!(p.edges[1..6].iter().all(|e| e.start.y < 0.0));
        let mut inward = square(10.0);
        bend_wall(&mut inward, 0, -1.0, Some(6)).unwrap();
        assert!(inward.edges[3].start.approx_eq(Point2::new(5.0, 1.0), 1e-4));
    }

    #[test]
    fn arcs_refuse_what_does_not_fit() {
        let mut p = square(10.0);
        assert!(round_corner(&mut p, 1, 10.5, None).is_err());
        assert!(round_corner(&mut p, 7, 1.0, None).is_err());
        assert!(bend_wall(&mut p, 0, 5.5, None).is_err());
        assert!(bend_wall(&mut p, 0, 1.0, Some(1)).is_err());
        assert!(bend_wall(&mut p, 0, f64::NAN, None).is_err());
        assert_eq!(p, square(10.0));
    }

    /// Audit ERR-004: a wall or corner that does not exist is no value to
    /// change; it ends with its own code, the sentence before it kept.
    #[test]
    fn a_missing_wall_or_corner_is_refused_with_its_own_code() {
        let mut p = square(10.0);
        let e = round_corner(&mut p, 7, 1.0, None).unwrap_err();
        assert!(e.starts_with("corner 7 does not exist"), "{e}");
        assert!(e.ends_with(&format!("({ARC_NO_CORNER})")), "{e}");
        let e = bend_wall(&mut p, 9, 1.0, None).unwrap_err();
        assert!(e.starts_with("wall 9 does not exist"), "{e}");
        assert!(e.ends_with(&format!("({ARC_NO_WALL})")), "{e}");
    }

    #[test]
    fn a_rounded_building_still_calculates() {
        let mut p = square(10.0);
        for v in [3, 2, 1, 0] {
            round_corner(&mut p, v, 2.0, None).unwrap();
        }
        let mut prj = crate::Project::new("t");
        prj.parts.push(p);
        let geos = crate::openings::generate_project(&prj);
        assert!(geos[0].error.is_none(), "{:?}", geos[0].error);
        let a: f64 = geos[0].faces.iter().map(|f| f.area).sum();
        // Plan with the overhang, over cos 30°: a little under the square's
        // (11 × 11) / cos 30° = 139.7 m².
        assert!(a > 130.0 && a < 139.7, "{a}");
    }

    #[test]
    fn an_arc_never_makes_walls_the_core_cannot_take() {
        // Audit MATH-003 / ARCH-005: r = 0.012 saved five zero-length walls
        // over MCP; r = 0.05 and 0.1 gave wrong areas.
        for r in [0.012, 0.05, 0.1, 0.2] {
            let mut part = square(10.0);
            match round_corner(&mut part, 1, r, None) {
                Ok(_) => {
                    let pts = part.outline();
                    let n = pts.len();
                    for k in 0..n {
                        let l = pts[k].distance(pts[(k + 1) % n]);
                        assert!(l >= MIN_ARC_WALL - 1e-3, "r {r}: wall {k} is {l} m");
                    }
                }
                Err(e) => assert!(e.contains("shorter than"), "r {r}: {e}"),
            }
        }
        let mut part = square(10.0);
        assert!(round_corner(&mut part, 1, 0.012, None).is_err());
        assert!(round_corner(&mut part, 1, 1.0, Some(64)).is_err());
        assert!(bend_wall(&mut square(0.12), 0, 0.06, None).is_err());
    }

    #[test]
    fn a_gable_bent_into_an_arc_is_an_apse_of_eaves() {
        // Audit UX-001: a saddle roof's gable end made semicircular.
        let mut part = square(10.0);
        part.edges[1].pitch_deg = 90.0;
        part.edges[3].pitch_deg = 90.0;
        let n = bend_wall(&mut part, 1, 5.0, None).unwrap();
        for e in &part.edges[1..1 + n] {
            assert!(!e.is_gable());
            assert_eq!(e.pitch_deg, 30.0);
        }
        assert!(part.edges[2 + n].is_gable());
    }

    fn rect(w: f64, d: f64) -> RoofPart {
        let pts = [(0.0, 0.0), (w, 0.0), (w, d), (0.0, d)];
        RoofPart::new(
            "A",
            pts.iter()
                .map(|&(x, y)| plain_edge(Point2::new(x, y), 30.0, 3.0, 0.0))
                .collect(),
        )
    }

    /// The roof of a single part over its plan / cos 30° (no overhang),
    /// or the calculation's error.
    fn roof_and_plan(part: &RoofPart) -> Result<(f64, f64), String> {
        let mut prj = crate::Project::new("t");
        prj.parts.push(part.clone());
        let geos = crate::openings::generate_project(&prj);
        if let Some(e) = &geos[0].error {
            return Err(format!("{:?}", e.kind));
        }
        if geos[0]
            .warnings
            .iter()
            .any(|w| w.kind == crate::check::WarningKind::NotCalculatedByCore)
        {
            return Err("not calculated by the core".into());
        }
        let roof: f64 = geos[0].faces.iter().map(|f| f.area).sum();
        let plan = crate::geom::signed_area(&part.outline()).abs() / 30f64.to_radians().cos();
        Ok((roof, plan))
    }

    /// Audit r2 MATH-002: a semicircle of many walls left the building
    /// without a roof from 54 walls on (corners turning under 2°): every
    /// count is refused with its code or gives the roof.
    #[test]
    fn an_arc_of_many_walls_is_refused_or_calculates() {
        for segs in [16, 32, 40, 44, 46, 48, 52, 54, 56, 64] {
            let mut part = rect(20.0, 10.0);
            match bend_wall(&mut part, 1, 5.0, Some(segs)) {
                Ok(_) => {
                    let (roof, plan) =
                        roof_and_plan(&part).unwrap_or_else(|e| panic!("{segs}: {e}"));
                    assert!((roof - plan).abs() < 0.01, "{segs}: {roof} vs {plan}");
                }
                Err(e) => assert!(e.ends_with(&format!("({ARC_FLAT})")), "{segs}: {e}"),
            }
        }
    }

    /// Audit r2 MATH-003: four corners rounded with radii of 0.3 to 0.5 m
    /// give the plan / cos 30° (the 10 to 12 cm walls they had were 0.08 to
    /// 0.11 m² over).
    #[test]
    fn four_small_rounded_corners_give_the_plan_over_the_cosine() {
        for k in 0..=10 {
            let r = 0.3 + 0.02 * k as f64;
            let mut part = rect(10.0, 10.0);
            for v in [3, 2, 1, 0] {
                round_corner(&mut part, v, r, None).unwrap();
            }
            let (roof, plan) = roof_and_plan(&part).unwrap_or_else(|e| panic!("{r}: {e}"));
            assert!((roof - plan).abs() < 0.01, "r {r}: {roof} vs {plan}");
        }
    }

    /// Audit r2 MATH-004: a slight bow of a 10 m wall is taken only where
    /// its walls turn enough for the roof core; a flatter one is refused
    /// with its code, never calculated off by centimetres.
    #[test]
    fn a_slight_bend_is_refused_or_gives_the_plan_over_the_cosine() {
        for b in [0.005, 0.01, 0.02, 0.05, 0.08, 0.1, 0.2] {
            for sign in [1.0, -1.0] {
                let mut part = rect(20.0, 10.0);
                match bend_wall(&mut part, 1, sign * b, None) {
                    Ok(_) => {
                        let (roof, plan) =
                            roof_and_plan(&part).unwrap_or_else(|e| panic!("{b}: {e}"));
                        assert!((roof - plan).abs() < 0.01, "{}: {roof} vs {plan}", sign * b);
                    }
                    Err(e) => assert!(e.ends_with(&format!("({ARC_FLAT})")), "{b}: {e}"),
                }
            }
        }
        // A bow the core takes stays possible.
        assert!(bend_wall(&mut rect(20.0, 10.0), 1, 0.1, None).is_ok());
    }

    /// Audit r2 MATH-002 "also seen": an inward arc that touches the
    /// opposite wall is refused (it left the building without a roof).
    #[test]
    fn an_inward_arc_touching_another_wall_is_refused() {
        for (w, d, wall, bulge) in [
            (5.0, 10.0, 1, -5.0),
            (12.0, 4.0, 0, -4.0),
            (12.0, 4.0, 0, -3.9),
        ] {
            let mut part = rect(w, d);
            let e = bend_wall(&mut part, wall, bulge, None).unwrap_err();
            assert!(
                e.ends_with(&format!("({ARC_TOUCHES})")),
                "{w} x {d}, {bulge}: {e}"
            );
            assert_eq!(part, rect(w, d));
        }
        assert!(bend_wall(&mut rect(12.0, 4.0), 0, -3.5, None).is_ok());
    }

    /// Audit r2 ARCH-005: every refusal of a value of an arc ends with its
    /// own code from [`ARC_CODES`], which the app translates by.
    #[test]
    fn every_arc_refusal_ends_with_its_code() {
        let mut errs = vec![
            bend_wall(&mut square(10.0), 0, 1.0, Some(1)).unwrap_err(),
            bend_wall(&mut square(10.0), 0, 6.0, None).unwrap_err(),
            bend_wall(&mut square(10.0), 0, 0.001, None).unwrap_err(),
            bend_wall(&mut square(10.0), 0, 0.02, None).unwrap_err(),
            round_corner(&mut square(10.0), 1, 0.001, None).unwrap_err(),
            round_corner(&mut square(10.0), 1, 20.0, None).unwrap_err(),
            round_corner(&mut square(10.0), 1, 0.02, None).unwrap_err(),
            round_corner(&mut square(10.0), 1, 1.0, Some(64)).unwrap_err(),
        ];
        let mut flat = square(10.0);
        flat.edges
            .insert(1, plain_edge(Point2::new(5.0, 0.0), 30.0, 3.0, 0.5));
        errs.push(round_corner(&mut flat, 1, 1.0, None).unwrap_err());
        let mut zero = square(10.0);
        zero.edges
            .insert(1, plain_edge(Point2::new(0.0, 0.0), 30.0, 3.0, 0.5));
        errs.push(bend_wall(&mut zero, 0, 1.0, None).unwrap_err());
        errs.push(round_corner(&mut zero, 1, 1.0, None).unwrap_err());
        for e in &errs {
            let code = e
                .rsplit_once(" (")
                .and_then(|(_, c)| c.strip_suffix(')'))
                .unwrap_or_else(|| panic!("no code: {e}"));
            assert!(ARC_CODES.contains(&code), "{code} not in ARC_CODES: {e}");
        }
    }
    /// Audit r2 UX-007: a bent wall straightened is the wall it was, with
    /// its roof definition; the other walls keep their indices.
    #[test]
    fn a_bent_wall_straightened_is_the_wall_again() {
        for (wall, bulge) in [(0, 1.0), (1, -2.0), (3, 5.0), (2, 0.8)] {
            let before = rect(12.0, 10.0);
            let mut part = before.clone();
            let segs = bend_wall(&mut part, wall, bulge, None).unwrap();
            let run = arc_run(&part, wall + segs / 2).expect("an arc");
            assert_eq!((run.start, run.walls), (wall, segs));
            assert!((run.bulge - bulge).abs() < 5e-4, "{} vs {bulge}", run.bulge);
            assert!(run.bulge.abs() <= run.chord / 2.0, "{run:?}");
            let i = straighten(&mut part, wall + segs - 1).unwrap();
            assert_eq!(i, wall);
            assert_eq!(part.edges.len(), 4);
            for (a, b) in part.edges.iter().zip(&before.edges) {
                assert!(a.start.approx_eq(b.start, 1e-4), "{wall}: {a:?}");
                assert_eq!(a.pitch_deg, b.pitch_deg);
            }
        }
    }

    /// An arc running over the outline's first corner is found and
    /// straightened round it.
    #[test]
    fn an_arc_across_the_first_corner_is_straightened() {
        let mut part = rect(12.0, 10.0);
        let segs = round_corner(&mut part, 0, 3.0, None).unwrap();
        // The arc's walls are 0..segs; turned by two, they run over the
        // end of the list into its start.
        part.edges.rotate_left(2);
        let n = part.edges.len();
        assert_eq!(n, 4 + segs);
        let run = arc_run(&part, n - 1).expect("an arc");
        assert_eq!((run.start, run.walls), (n - 2, segs));
        straighten(&mut part, 1).unwrap();
        // A straightened rounded corner is the sharp corner again (UX-004
        // of the third tower audit), not the chord from (0, 3) to (3, 0).
        assert_eq!(part.edges.len(), 4);
        assert!(
            part.edges
                .iter()
                .any(|e| e.start.approx_eq(Point2::new(0.0, 0.0), 1e-4)),
            "{:?}",
            part.edges.iter().map(|e| e.start).collect::<Vec<_>>()
        );
        assert!(
            !part
                .edges
                .iter()
                .any(|e| e.start.approx_eq(Point2::new(0.0, 3.0), 1e-4)
                    || e.start.approx_eq(Point2::new(3.0, 0.0), 1e-4))
        );
        let mut prj = crate::Project::new("t");
        prj.parts.push(part);
        assert!(prj.problems().is_empty(), "{:?}", prj.problems());
    }

    /// A wall of a rectangle, of a regular polygon or a marker is no arc.
    #[test]
    fn a_wall_of_no_arc_is_refused_with_its_code() {
        let mut part = square(10.0);
        let e = straighten(&mut part, 1).unwrap_err();
        assert!(e.ends_with(&format!("({ARC_NONE})")), "{e}");
        let mut octagon = RoofPart::new(
            "O",
            crate::edit::transform::regular_polygon(Point2::new(0.0, 0.0), 8, 3.0, 0.0)
                .into_iter()
                .map(|p| plain_edge(p, 30.0, 3.0, 0.5))
                .collect(),
        );
        assert_eq!(arc_run(&octagon, 2), None);
        assert!(straighten(&mut octagon, 2).is_err());
        assert!(
            straighten(&mut square(10.0), 9)
                .unwrap_err()
                .ends_with(&format!("({ARC_NO_WALL})"))
        );
        assert_eq!(part, square(10.0));
    }

    /// "Change the arc": a bend of an arc wall rebuilds the arc on its
    /// chord, the same as bending the straight wall with that bulge.
    #[test]
    fn reshaping_an_arc_bends_its_chord_again() {
        let mut part = rect(12.0, 10.0);
        bend_wall(&mut part, 0, 1.0, None).unwrap();
        let (i, segs) = reshape(&mut part, 3, 3.0, Some(8)).unwrap();
        let mut direct = rect(12.0, 10.0);
        bend_wall(&mut direct, 0, 3.0, Some(8)).unwrap();
        assert_eq!((i, segs), (0, 8));
        assert_eq!(part, direct);
        let before = part.clone();
        assert!(reshape(&mut part, 3, 9.0, None).is_err());
        assert_eq!(part, before);
        assert!(
            reshape(&mut rect(12.0, 10.0), 0, 1.0, None)
                .unwrap_err()
                .ends_with(&format!("({ARC_NONE})"))
        );
    }

    /// Two equal walls at a corner are any corner (a pointed end): a bow
    /// of two walls is not taken as an arc.
    #[test]
    fn an_arc_of_two_walls_is_not_taken_for_one() {
        let mut part = rect(12.0, 10.0);
        assert_eq!(bend_wall(&mut part, 2, 0.5, None), Ok(2));
        assert_eq!(arc_run(&part, 2), None);
    }
}
