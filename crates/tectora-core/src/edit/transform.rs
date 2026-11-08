//! Moving, rotating, mirroring and copying a roof part with everything that
//! sits on it (DACAD: Bearbeiten › Verschieben, Drehen, Spiegeln, Elemente
//! kopieren, Kopieren-Drehen, Kopieren-Spiegeln), shifting a wall in
//! parallel and regular polygons. Port of the editor's `transform.ts`.
//! Plain plan geometry: the roof is recalculated afterwards.

use std::f64::consts::{PI, TAU};

use serde::{Deserialize, Serialize};

use super::round_to;
use crate::geom::Point2;
use crate::model::{
    Chimney, Dormer, EdgeDef, ManualFace, ManualLine, Project, RoofPart, RoofWindow,
};

/// A plan map: move, rotate (degrees, counter-clockwise, about a centre) or
/// mirror (at the line through a centre with a direction in degrees).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlanMap {
    Move { dx: f64, dy: f64 },
    Rotate { centre: Point2, deg: f64 },
    Mirror { centre: Point2, deg: f64 },
}

fn pt(p: Point2) -> Point2 {
    Point2::new(round_to(p.x, 3), round_to(p.y, 3))
}

fn norm_angle(a: f64) -> f64 {
    a.rem_euclid(TAU)
}

impl PlanMap {
    pub fn apply(&self, p: Point2) -> Point2 {
        match *self {
            PlanMap::Move { dx, dy } => pt(Point2::new(p.x + dx, p.y + dy)),
            PlanMap::Rotate { centre: c, deg } => {
                let (sin, cos) = deg.to_radians().sin_cos();
                pt(Point2::new(
                    c.x + (p.x - c.x) * cos - (p.y - c.y) * sin,
                    c.y + (p.x - c.x) * sin + (p.y - c.y) * cos,
                ))
            }
            PlanMap::Mirror { centre: c, deg } => {
                let (sin2, cos2) = (2.0 * deg.to_radians()).sin_cos();
                let (x, y) = (p.x - c.x, p.y - c.y);
                pt(Point2::new(
                    c.x + x * cos2 + y * sin2,
                    c.y + x * sin2 - y * cos2,
                ))
            }
        }
    }

    /// Maps a direction angle (radians).
    pub fn angle(&self, a: f64) -> f64 {
        match *self {
            PlanMap::Move { .. } => a,
            PlanMap::Rotate { deg, .. } => a + deg.to_radians(),
            PlanMap::Mirror { deg, .. } => 2.0 * deg.to_radians() - a,
        }
    }

    pub fn flips(&self) -> bool {
        matches!(self, PlanMap::Mirror { .. })
    }
}

/// A map applied `times` times in a row (the k-th copy), rounding after each
/// step as the editor does.
#[derive(Debug, Clone, Copy)]
struct Power(PlanMap, u32);

impl Power {
    fn apply(&self, mut p: Point2) -> Point2 {
        for _ in 0..self.1 {
            p = self.0.apply(p);
        }
        p
    }
    fn angle(&self, mut a: f64) -> f64 {
        for _ in 0..self.1 {
            a = self.0.angle(a);
        }
        a
    }
    fn flips(&self) -> bool {
        self.0.flips() && self.1 % 2 == 1
    }
}

/// Maps a chain of walls: edge i runs from `pts[i]` to `pts[i + 1]`, where
/// `pts` are the edge starts followed by `end`. A map that flips
/// orientation also reverses the chain, so the roof stays on the same side
/// of each wall; a direction marker then turns by 180°.
fn map_chain(edges: &[EdgeDef], end: Point2, m: Power) -> (Vec<EdgeDef>, Point2) {
    let map_edge = |e: &EdgeDef, start: Point2, turn: f64| {
        let mut e = e.clone();
        e.start = m.apply(start);
        e.direction_rad = e.direction_rad.map(|d| norm_angle(m.angle(d) + turn));
        e
    };
    if !m.flips() {
        return (
            edges.iter().map(|e| map_edge(e, e.start, 0.0)).collect(),
            m.apply(end),
        );
    }
    let pts: Vec<Point2> = edges.iter().map(|e| e.start).chain([end]).collect();
    let n = edges.len();
    let out = (0..n)
        .map(|j| {
            let k = n - 1 - j;
            map_edge(&edges[k], pts[k + 1], PI)
        })
        .collect();
    (out, m.apply(pts[0]))
}

fn map_part(part: &RoofPart, m: Power, name: String) -> RoofPart {
    let first = part
        .edges
        .first()
        .map_or(Point2::new(0.0, 0.0), |e| e.start);
    RoofPart {
        name,
        edges: map_chain(&part.edges, first, m).0,
    }
}

fn map_dormer(d: &Dormer, m: Power) -> Dormer {
    let (edges, end) = map_chain(&d.edges, d.end, m);
    Dormer {
        edges,
        end,
        host_wall: d.host_wall.map(|w| w.map(|p| m.apply(p))),
    }
}

fn map_polygon(points: &[Point2], m: Power) -> Vec<Point2> {
    let mut out: Vec<Point2> = points.iter().map(|p| m.apply(*p)).collect();
    if m.flips() {
        out.reverse();
    }
    out
}

fn map_chimney(c: &Chimney, m: Power) -> Chimney {
    Chimney {
        corners: map_polygon(&c.corners, m),
        height: c.height,
    }
}

/// A roof window is placed by its anchor point; the face it lands on gives its slope.
fn map_window(w: &RoofWindow, m: Power) -> RoofWindow {
    let p = m.apply(Point2::new(w.x, w.y));
    RoofWindow {
        x: p.x,
        y: p.y,
        ..w.clone()
    }
}

fn map_line(l: &ManualLine, m: Power) -> ManualLine {
    let a = m.apply(Point2::new(l.a[0], l.a[1]));
    let b = m.apply(Point2::new(l.b[0], l.b[1]));
    ManualLine {
        a: [a.x, a.y, l.a[2]],
        b: [b.x, b.y, l.b[2]],
        ..l.clone()
    }
}

fn map_face(f: &ManualFace, m: Power) -> ManualFace {
    ManualFace {
        points: map_polygon(&f.points, m),
        ..f.clone()
    }
}

/// `base (n)` with the smallest n >= 2 no part uses yet.
fn unique_name(project: &Project, base: &str) -> String {
    let root = match base.rfind(" (") {
        Some(i)
            if base.ends_with(')')
                && base[i + 2..base.len() - 1]
                    .chars()
                    .all(|c| c.is_ascii_digit())
                && base.len() > i + 3 =>
        {
            &base[..i]
        }
        _ => base,
    };
    (2..)
        .map(|n| format!("{root} ({n})"))
        .find(|name| project.parts.iter().all(|p| &p.name != name))
        .expect("unbounded")
}

/// Even-odd point-in-polygon test on the part's wall outline.
pub fn inside_outline(part: &RoofPart, p: Point2) -> bool {
    let o: Vec<Point2> = part.edges.iter().map(|e| e.start).collect();
    let mut inside = false;
    let mut j = o.len().wrapping_sub(1);
    for i in 0..o.len() {
        let (a, b) = (o[i], o[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn centre(pts: impl IntoIterator<Item = Point2>) -> Point2 {
    let (mut x, mut y, mut n) = (0.0, 0.0, 0usize);
    for p in pts {
        x += p.x;
        y += p.y;
        n += 1;
    }
    let n = n.max(1) as f64;
    Point2::new(x / n, y / n)
}

/// Centre of a part's wall outline (vertex average), the default pivot.
pub fn part_centre(part: &RoofPart) -> Point2 {
    pt(centre(part.edges.iter().map(|e| e.start)))
}

/// Most copies one transform adds (the editor's field allows 50).
pub const MAX_COPIES: u32 = 50;
/// Most walls of a regular polygon part (the editor's field allows 64).
pub const MAX_POLYGON_SIDES: usize = 64;

/// Applies `m` to part `pi` and to the dormers, chimneys, openings, windows
/// and hand-drawn lines and faces lying on it. With `copies` > 0 (at most
/// [`MAX_COPIES`]) the part stays and that many copies are added, the k-th
/// mapped k times (DACAD's copy functions). Returns the indices of the new
/// or moved parts.
pub fn transform_part(project: &mut Project, pi: usize, m: PlanMap, copies: u32) -> Vec<usize> {
    let copies = copies.min(MAX_COPIES);
    let Some(part) = project.parts.get(pi).cloned() else {
        return Vec::new();
    };
    let on = |p: Point2| inside_outline(&part, p);
    let pick = |flags: Vec<bool>| -> Vec<usize> {
        flags
            .iter()
            .enumerate()
            .filter(|f| *f.1)
            .map(|f| f.0)
            .collect()
    };
    let dormers = pick(
        project
            .dormers
            .iter()
            .map(|d| on(centre(d.edges.iter().map(|e| e.start).chain([d.end]))))
            .collect(),
    );
    let chimneys = pick(
        project
            .chimneys
            .iter()
            .map(|c| on(centre(c.corners.clone())))
            .collect(),
    );
    let holes = pick(
        project
            .roof_openings
            .iter()
            .map(|c| on(centre(c.corners.clone())))
            .collect(),
    );
    let windows = pick(
        project
            .windows
            .iter()
            .map(|w| on(Point2::new(w.x, w.y)))
            .collect(),
    );
    let lines = pick(
        project
            .manual_lines
            .iter()
            .map(|l| {
                on(Point2::new(
                    (l.a[0] + l.b[0]) / 2.0,
                    (l.a[1] + l.b[1]) / 2.0,
                ))
            })
            .collect(),
    );
    let faces = pick(
        project
            .manual_faces
            .iter()
            .map(|f| on(centre(f.points.clone())))
            .collect(),
    );

    if copies == 0 {
        let m1 = Power(m, 1);
        project.parts[pi] = map_part(&part, m1, part.name.clone());
        for &i in &dormers {
            project.dormers[i] = map_dormer(&project.dormers[i], m1);
        }
        for &i in &chimneys {
            project.chimneys[i] = map_chimney(&project.chimneys[i], m1);
        }
        for &i in &holes {
            project.roof_openings[i] = map_chimney(&project.roof_openings[i], m1);
        }
        for &i in &windows {
            project.windows[i] = map_window(&project.windows[i], m1);
        }
        for &i in &lines {
            project.manual_lines[i] = map_line(&project.manual_lines[i], m1);
        }
        for &i in &faces {
            project.manual_faces[i] = map_face(&project.manual_faces[i], m1);
        }
        return vec![pi];
    }
    let mut added = Vec::new();
    for k in 1..=copies {
        let mk = Power(m, k);
        let name = unique_name(project, &part.name);
        project.parts.push(map_part(&part, mk, name));
        added.push(project.parts.len() - 1);
        let new_dormers: Vec<_> = dormers
            .iter()
            .map(|&i| map_dormer(&project.dormers[i], mk))
            .collect();
        project.dormers.extend(new_dormers);
        let new: Vec<_> = chimneys
            .iter()
            .map(|&i| map_chimney(&project.chimneys[i], mk))
            .collect();
        project.chimneys.extend(new);
        let new: Vec<_> = holes
            .iter()
            .map(|&i| map_chimney(&project.roof_openings[i], mk))
            .collect();
        project.roof_openings.extend(new);
        let new: Vec<_> = windows
            .iter()
            .map(|&i| map_window(&project.windows[i], mk))
            .collect();
        project.windows.extend(new);
        let new: Vec<_> = lines
            .iter()
            .map(|&i| map_line(&project.manual_lines[i], mk))
            .collect();
        project.manual_lines.extend(new);
        let new: Vec<_> = faces
            .iter()
            .map(|&i| map_face(&project.manual_faces[i], mk))
            .collect();
        project.manual_faces.extend(new);
    }
    added
}

/// One object placed on the roof, by its index in the project's list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ItemRef {
    pub kind: ItemKind,
    pub index: usize,
}

/// The kinds of objects the editor selects and moves on their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Dormer,
    Chimney,
    Window,
    RoofOpening,
    ManualLine,
    ManualFace,
}

impl ItemKind {
    fn len(self, p: &Project) -> usize {
        match self {
            ItemKind::Dormer => p.dormers.len(),
            ItemKind::Chimney => p.chimneys.len(),
            ItemKind::Window => p.windows.len(),
            ItemKind::RoofOpening => p.roof_openings.len(),
            ItemKind::ManualLine => p.manual_lines.len(),
            ItemKind::ManualFace => p.manual_faces.len(),
        }
    }
}

/// The plan point an item is moved by and rotated about: the centre of a
/// chimney, opening or face, the middle of a line or of a dormer's front,
/// a window's anchor corner. `None` for an index that does not exist.
pub fn item_centre(p: &Project, it: ItemRef) -> Option<Point2> {
    if it.index >= it.kind.len(p) {
        return None;
    }
    Some(pt(match it.kind {
        ItemKind::Dormer => {
            let d = &p.dormers[it.index];
            let s = d.edges.first().map_or(d.end, |e| e.start);
            Point2::new((s.x + d.end.x) / 2.0, (s.y + d.end.y) / 2.0)
        }
        ItemKind::Chimney => centre(p.chimneys[it.index].corners.clone()),
        ItemKind::RoofOpening => centre(p.roof_openings[it.index].corners.clone()),
        ItemKind::Window => Point2::new(p.windows[it.index].x, p.windows[it.index].y),
        ItemKind::ManualLine => {
            let l = &p.manual_lines[it.index];
            Point2::new((l.a[0] + l.b[0]) / 2.0, (l.a[1] + l.b[1]) / 2.0)
        }
        ItemKind::ManualFace => centre(p.manual_faces[it.index].points.clone()),
    }))
}

/// Applies `m` to single items (not their part): the editor's drag, rotate
/// and duplicate on a selected chimney, window, opening, dormer or
/// hand-drawn line or face. With `copies` > 0 the items stay and that many
/// copies of each are appended, the k-th mapped k times. Heights are kept
/// as they are; see [`follow_roof`]. Returns the moved or new items, in the
/// order given; an index that does not exist is an error and changes nothing.
pub fn transform_items(
    project: &mut Project,
    items: &[ItemRef],
    m: PlanMap,
    copies: u32,
) -> Result<Vec<ItemRef>, String> {
    let mut seen = std::collections::HashSet::new();
    for it in items {
        if it.index >= it.kind.len(project) {
            return Err(format!(
                "{} {} does not exist",
                kind_name(it.kind),
                it.index
            ));
        }
        if !seen.insert(*it) {
            return Err(format!(
                "{} {} is listed twice",
                kind_name(it.kind),
                it.index
            ));
        }
    }
    let mut out = Vec::new();
    let rounds = if copies == 0 { 1 } else { copies };
    for k in 1..=rounds {
        let mk = Power(m, if copies == 0 { 1 } else { k });
        for &it in items {
            let i = it.index;
            let p = &mut *project;
            let index = match it.kind {
                ItemKind::Dormer => {
                    let d = map_dormer(&p.dormers[i], mk);
                    put(&mut p.dormers, i, d, copies)
                }
                ItemKind::Chimney => {
                    let c = map_chimney(&p.chimneys[i], mk);
                    put(&mut p.chimneys, i, c, copies)
                }
                ItemKind::RoofOpening => {
                    let c = map_chimney(&p.roof_openings[i], mk);
                    put(&mut p.roof_openings, i, c, copies)
                }
                ItemKind::Window => {
                    let w = map_window(&p.windows[i], mk);
                    put(&mut p.windows, i, w, copies)
                }
                ItemKind::ManualLine => {
                    let l = map_line(&p.manual_lines[i], mk);
                    put(&mut p.manual_lines, i, l, copies)
                }
                ItemKind::ManualFace => {
                    let f = map_face(&p.manual_faces[i], mk);
                    put(&mut p.manual_faces, i, f, copies)
                }
            };
            out.push(ItemRef {
                kind: it.kind,
                index,
            });
        }
    }
    Ok(out)
}

fn put<T>(v: &mut Vec<T>, i: usize, x: T, copies: u32) -> usize {
    if copies == 0 {
        v[i] = x;
        i
    } else {
        v.push(x);
        v.len() - 1
    }
}

fn kind_name(k: ItemKind) -> &'static str {
    match k {
        ItemKind::Dormer => "dormer",
        ItemKind::Chimney => "chimney",
        ItemKind::Window => "window",
        ItemKind::RoofOpening => "roof_opening",
        ItemKind::ManualLine => "manual_line",
        ItemKind::ManualFace => "manual_face",
    }
}

/// Keeps items moved by `to - from` at the same height above the roof: a
/// chimney's top, a dormer's eave and a hand-drawn line's ends rise or fall
/// with the host roof between where they were and their new place. `host`
/// gives the roof height at a plan point (without the dormers' own roofs);
/// where it has none the height stays. [no source: an editor convenience,
/// DACAD keeps absolute heights when its objects are moved.]
pub fn follow_roof(
    project: &mut Project,
    moved: &[(ItemRef, Point2, Point2)],
    host: impl Fn(Point2) -> Option<f64>,
) {
    let delta = |a: Point2, b: Point2| match (host(a), host(b)) {
        (Some(za), Some(zb)) => zb - za,
        _ => 0.0,
    };
    for &(it, from, to) in moved {
        let d = delta(from, to);
        match it.kind {
            ItemKind::Chimney => {
                if let Some(c) = project.chimneys.get_mut(it.index) {
                    c.height = round_to(c.height + d, 3);
                }
            }
            ItemKind::Dormer => {
                if let Some(dm) = project.dormers.get_mut(it.index) {
                    for e in &mut dm.edges {
                        e.eave_height = round_to(e.eave_height + d, 3);
                    }
                }
            }
            ItemKind::ManualLine => {
                if let Some(l) = project.manual_lines.get_mut(it.index) {
                    let off = Point2::new(to.x - from.x, to.y - from.y);
                    for q in [&mut l.a, &mut l.b] {
                        let now = Point2::new(q[0], q[1]);
                        let was = Point2::new(q[0] - off.x, q[1] - off.y);
                        q[2] = round_to(q[2] + delta(was, now), 3);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Moves wall `ei` of a part parallel to itself by `d` metres outward (the
/// roof side is to the left of the wall in a counter-clockwise outline, so
/// outward is to the right), its neighbours stretched to meet it (DACAD:
/// Parallele Linie / Wände ändern). Returns false if a neighbour runs
/// parallel to the wall, where no corner can be found.
pub fn offset_wall(part: &mut RoofPart, ei: usize, d: f64) -> bool {
    let n = part.edges.len() as isize;
    if n < 3 || ei as isize >= n {
        return false;
    }
    let at = |i: isize| part.edges[i.rem_euclid(n) as usize].start;
    let ei = ei as isize;
    let (a, b) = (at(ei), at(ei + 1));
    let len = (b.x - a.x).hypot(b.y - a.y);
    if len < 1e-9 {
        return false;
    }
    let area: f64 = (0..n)
        .map(|i| at(i).x * at(i + 1).y - at(i + 1).x * at(i).y)
        .sum();
    let sgn = if area >= 0.0 { 1.0 } else { -1.0 };
    let u = Point2::new((b.x - a.x) / len, (b.y - a.y) / len);
    let nrm = Point2::new(sgn * u.y, -sgn * u.x);
    let a2 = Point2::new(a.x + nrm.x * d, a.y + nrm.y * d);
    let b2 = Point2::new(b.x + nrm.x * d, b.y + nrm.y * d);
    let meet = |p: Point2, q: Point2, s: Point2| -> Option<Point2> {
        let v = Point2::new(q.x - p.x, q.y - p.y);
        let den = v.x * u.y - v.y * u.x;
        if den.abs() < 1e-9 {
            return None;
        }
        let t = ((s.x - p.x) * u.y - (s.y - p.y) * u.x) / den;
        Some(Point2::new(p.x + v.x * t, p.y + v.y * t))
    };
    let (Some(na), Some(nb)) = (meet(at(ei - 1), a, a2), meet(at(ei + 2), b, b2)) else {
        return false;
    };
    let n = n as usize;
    part.edges[ei as usize].start = pt(na);
    part.edges[(ei as usize + 1) % n].start = pt(nb);
    true
}

/// The corners of a regular polygon (DACAD: Vieleck-Eingabe),
/// counter-clockwise, the first wall horizontal at the bottom for rotation 0.
/// The corners of a regular polygon; `sides` is capped at [`MAX_POLYGON_SIDES`].
pub fn regular_polygon(c: Point2, sides: usize, side: f64, rotation_deg: f64) -> Vec<Point2> {
    let sides = sides.min(MAX_POLYGON_SIDES);
    let s = sides as f64;
    let r = side / (2.0 * (PI / s).sin());
    let a0 = -PI / 2.0 - PI / s + rotation_deg.to_radians();
    (0..sides)
        .map(|i| {
            let a = a0 + 2.0 * PI * i as f64 / s;
            pt(Point2::new(c.x + r * a.cos(), c.y + r * a.sin()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::plain_edge;
    use crate::geom::signed_area;

    fn wall(x: f64, y: f64, pitch: f64) -> EdgeDef {
        plain_edge(Point2::new(x, y), pitch, 3.0, 0.5)
    }

    // 10 x 6 counter-clockwise: south eave 30°, east gable, north eave 45°, west eave 40°.
    fn part() -> RoofPart {
        RoofPart {
            name: "A".into(),
            edges: vec![
                wall(0.0, 0.0, 30.0),
                wall(10.0, 0.0, 90.0),
                wall(10.0, 6.0, 45.0),
                wall(0.0, 6.0, 40.0),
            ],
        }
    }

    fn walls(p: &RoofPart) -> Vec<(Point2, Point2, f64)> {
        let n = p.edges.len();
        (0..n)
            .map(|i| {
                (
                    p.edges[i].start,
                    p.edges[(i + 1) % n].start,
                    p.edges[i].pitch_deg,
                )
            })
            .collect()
    }

    fn with_items() -> Project {
        let mut p = Project::new("T");
        p.parts.push(part());
        p.chimneys.push(Chimney {
            corners: vec![
                Point2::new(2.0, 2.0),
                Point2::new(3.0, 2.0),
                Point2::new(3.0, 3.0),
                Point2::new(2.0, 3.0),
            ],
            height: 6.0,
        });
        p.windows.push(RoofWindow {
            x: 5.0,
            y: 1.0,
            width: 0.78,
            height: 1.18,
        });
        p
    }

    #[test]
    fn moves_one_item_and_leaves_the_part_and_the_others() {
        let mut p = with_items();
        let before = p.clone();
        let it = ItemRef {
            kind: ItemKind::Chimney,
            index: 0,
        };
        let out = transform_items(&mut p, &[it], PlanMap::Move { dx: 1.0, dy: -0.5 }, 0).unwrap();
        assert_eq!(out, vec![it]);
        assert_eq!(p.chimneys[0].corners[0], Point2::new(3.0, 1.5));
        assert_eq!(p.chimneys[0].height, 6.0);
        assert_eq!(p.parts, before.parts);
        assert_eq!(p.windows, before.windows);
    }

    #[test]
    fn copies_append_and_rotation_turns_about_the_item_centre() {
        let mut p = with_items();
        let it = ItemRef {
            kind: ItemKind::Chimney,
            index: 0,
        };
        let c = item_centre(&p, it).unwrap();
        assert_eq!(c, Point2::new(2.5, 2.5));
        let out = transform_items(
            &mut p,
            &[it],
            PlanMap::Rotate {
                centre: c,
                deg: 45.0,
            },
            2,
        )
        .unwrap();
        assert_eq!(out.iter().map(|r| r.index).collect::<Vec<_>>(), vec![1, 2]);
        assert_eq!(p.chimneys.len(), 3);
        assert_eq!(item_centre(&p, out[0]), Some(c));
        // Two turns of 45°: the square is back on its axes.
        let mut xs: Vec<f64> = p.chimneys[2].corners.iter().map(|q| q.x).collect();
        xs.sort_by(f64::total_cmp);
        assert_eq!((xs[0], xs[3]), (2.0, 3.0));
    }

    #[test]
    fn a_missing_or_repeated_item_changes_nothing() {
        let mut p = with_items();
        let before = p.clone();
        let bad = ItemRef {
            kind: ItemKind::Dormer,
            index: 0,
        };
        assert!(transform_items(&mut p, &[bad], PlanMap::Move { dx: 1.0, dy: 0.0 }, 0).is_err());
        let w = ItemRef {
            kind: ItemKind::Window,
            index: 0,
        };
        assert!(transform_items(&mut p, &[w, w], PlanMap::Move { dx: 1.0, dy: 0.0 }, 0).is_err());
        assert_eq!(p, before);
    }

    #[test]
    fn follow_roof_lifts_a_chimney_with_the_roof() {
        let mut p = with_items();
        let it = ItemRef {
            kind: ItemKind::Chimney,
            index: 0,
        };
        transform_items(&mut p, &[it], PlanMap::Move { dx: 0.0, dy: 1.0 }, 0).unwrap();
        // A roof rising 0.5 m per metre in y.
        follow_roof(
            &mut p,
            &[(it, Point2::new(2.5, 2.5), Point2::new(2.5, 3.5))],
            |q| Some(q.y * 0.5),
        );
        assert_eq!(p.chimneys[0].height, 6.5);
    }

    #[test]
    fn rotates_walls_and_keeps_each_pitch_on_its_wall() {
        let m = map_part(
            &part(),
            Power(
                PlanMap::Rotate {
                    centre: Point2::new(0.0, 0.0),
                    deg: 90.0,
                },
                1,
            ),
            "A".into(),
        );
        assert_eq!(
            walls(&m)[0],
            (Point2::new(0.0, 0.0), Point2::new(0.0, 10.0), 30.0)
        );
        assert!((signed_area(&m.outline()) - 60.0).abs() < 1e-9);
    }

    #[test]
    fn mirrors_into_a_ccw_outline_with_each_pitch_on_its_mirrored_wall() {
        let m = map_part(
            &part(),
            Power(
                PlanMap::Mirror {
                    centre: Point2::new(0.0, 0.0),
                    deg: 90.0,
                },
                1,
            ),
            "A".into(),
        );
        assert!((signed_area(&m.outline()) - 60.0).abs() < 1e-9);
        let by = |pitch: f64| {
            walls(&m)
                .into_iter()
                .find(|w| w.2 == pitch)
                .map(|w| (w.0, w.1))
                .unwrap()
        };
        assert_eq!(by(30.0), (Point2::new(-10.0, 0.0), Point2::new(0.0, 0.0)));
        assert_eq!(by(90.0), (Point2::new(-10.0, 6.0), Point2::new(-10.0, 0.0)));
        assert_eq!(by(40.0), (Point2::new(0.0, 0.0), Point2::new(0.0, 6.0)));
    }

    #[test]
    fn mirroring_twice_gives_the_part_back() {
        let mut p = part();
        p.edges[1].direction_rad = Some(0.7);
        let m = PlanMap::Mirror {
            centre: Point2::new(3.0, 1.0),
            deg: 30.0,
        };
        let back = map_part(&p, Power(m, 2), "A".into());
        for (e, o) in back.edges.iter().zip(&p.edges) {
            assert!(e.start.approx_eq(o.start, 0.01));
            assert_eq!(e.pitch_deg, o.pitch_deg);
        }
        assert!((back.edges[1].direction_rad.unwrap() - 0.7).abs() < 1e-9);
    }

    #[test]
    fn copies_with_things_on_the_part_and_leaves_the_rest() {
        let mut pr = Project::new("x");
        pr.parts.push(part());
        let sq = |x: f64| Chimney {
            corners: vec![
                Point2::new(x, 2.0),
                Point2::new(x + 1.0, 2.0),
                Point2::new(x + 1.0, 3.0),
                Point2::new(x, 3.0),
            ],
            height: 9.0,
        };
        pr.chimneys.push(sq(4.0));
        pr.chimneys.push(sq(40.0));
        let added = transform_part(&mut pr, 0, PlanMap::Move { dx: 20.0, dy: 0.0 }, 2);
        assert_eq!(added, vec![1, 2]);
        assert_eq!(pr.parts[1].name, "A (2)");
        assert_eq!(pr.parts[2].name, "A (3)");
        assert_eq!(pr.parts[2].edges[0].start, Point2::new(40.0, 0.0));
        assert_eq!(pr.chimneys.len(), 4);
        assert_eq!(pr.chimneys[3].corners[0], Point2::new(44.0, 2.0));
    }

    #[test]
    fn moves_a_wall_outward_and_stretches_its_neighbours() {
        let mut p = part();
        assert!(offset_wall(&mut p, 0, 1.0));
        let starts: Vec<_> = p.edges.iter().map(|e| (e.start.x, e.start.y)).collect();
        assert_eq!(
            starts,
            vec![(0.0, -1.0), (10.0, -1.0), (10.0, 6.0), (0.0, 6.0)]
        );
        assert!(offset_wall(&mut p, 1, -2.0));
        assert_eq!(p.edges[1].start, Point2::new(8.0, -1.0));
        assert_eq!(p.edges[2].start, Point2::new(8.0, 6.0));
    }

    #[test]
    fn builds_a_regular_polygon_with_the_given_side() {
        let pts = regular_polygon(Point2::new(0.0, 0.0), 6, 4.0, 0.0);
        assert_eq!(pts.len(), 6);
        for i in 0..6 {
            assert!((pts[i].distance(pts[(i + 1) % 6]) - 4.0).abs() < 0.01);
        }
        assert!((pts[0].y - pts[1].y).abs() < 1e-6);
        assert!(pts[1].x > pts[0].x);
    }
}
