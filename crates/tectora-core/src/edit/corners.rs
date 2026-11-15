//! Dragging the corners of a part's outline: a direction change (a
//! zero-length marker wall) moves with the corner it sits on, a corner
//! dropped onto its neighbour merges into it, and a drawn footprint loses
//! its repeated corners. The plan draws the drag itself; what is saved comes
//! from here (the editor's `moveend`, finishing a drawn part; the MCP tool
//! `edit_walls` uses [`corner_group`] for `move_point`).

use super::is_marker;
use crate::geom::Point2;
use crate::model::{EdgeDef, MIN_WALL_LENGTH};

/// Corners that are the same point within 1e-6 m.
fn at(a: Point2, b: Point2, tol: f64) -> bool {
    (a.x - b.x).abs() < tol && (a.y - b.y).abs() < tol
}

/// The direction a new direction change at the start of wall `i` gets
/// (radians, 0 to 2 pi): the bisector of the previous wall's direction and
/// this wall's. Walls are cut by their own planes, so a marker at the
/// previous wall's direction (or its reverse) never calculates, which the old
/// default (this wall's direction + 90 degrees) was at every right-angled
/// corner: all 42 default markers on the built-in examples failed, the
/// bisector calculates all 42 (HUNTC-008). This is our choice; DACAD's own
/// proposal in its direction dialog is not known. On a straight run the walls
/// have one direction and the marker is turned 45 degrees from it.
pub fn default_direction(edges: &[EdgeDef], i: usize) -> f64 {
    use std::f64::consts::{FRAC_PI_4, TAU};
    let n = edges.len();
    let Some(this) = edges.get(i) else { return 0.0 };
    let next = edges[(i + 1) % n].start;
    let d_this = (next.y - this.start.y).atan2(next.x - this.start.x);
    // The previous wall that has a direction (not a marker, not zero length).
    let prev = (1..n)
        .map(|k| &edges[(i + n - k) % n])
        .find(|e| !is_marker(e) && !at(e.start, this.start, 1e-9));
    let Some(prev) = prev else {
        return (d_this + FRAC_PI_4).rem_euclid(TAU);
    };
    let d_prev = (this.start.y - prev.start.y).atan2(this.start.x - prev.start.x);
    let (x, y) = (d_prev.cos() + d_this.cos(), d_prev.sin() + d_this.sin());
    let ang = if x.hypot(y) < 1e-6 || ((d_prev - d_this).sin().abs() < 1e-9 && x > 0.0) {
        d_this + FRAC_PI_4
    } else {
        y.atan2(x)
    };
    ang.rem_euclid(TAU)
}

/// A drawn footprint without repeated corners: a corner on the one before it
/// and closing corners on the first one are dropped, so no wall has zero
/// length (closer than the core's `MIN_WALL_LENGTH`). O(n).
pub fn without_repeats(pts: &[Point2]) -> Vec<Point2> {
    let same = |a: Point2, b: Point2| a.distance(b) < MIN_WALL_LENGTH;
    let mut out: Vec<Point2> = Vec::new();
    for &p in pts {
        if out.last().is_none_or(|&l| !same(l, p)) {
            out.push(p);
        }
    }
    while out.len() > 1 && same(out[out.len() - 1], out[0]) {
        out.pop();
    }
    out
}

/// Corner `v` and the direction changes (zero-length marker walls) on the
/// same point next to it, sorted. Empty for a corner that does not exist. O(n).
pub fn corner_group(es: &[EdgeDef], v: usize) -> Vec<usize> {
    let n = es.len();
    let Some(first) = es.get(v) else {
        return Vec::new();
    };
    let p = first.start;
    let mut out = vec![v];
    // Markers just before v on the same point.
    for k in 1..n {
        let j = (v + n - k) % n;
        if !is_marker(&es[j]) || !at(es[j].start, p, 1e-6) {
            break;
        }
        out.push(j);
    }
    // v is a marker: the corners after it on the same point.
    for k in 1..n {
        let (prev, j) = ((v + k - 1) % n, (v + k) % n);
        if !is_marker(&es[prev]) || !at(es[j].start, p, 1e-6) {
            break;
        }
        out.push(j);
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// After a drag: when corner `v` lies on the next or previous wall corner
/// (within 5 mm) it is removed with its direction changes, as long as three
/// walls remain. Returns whether it was merged.
pub fn merge_dropped_corner(es: &mut Vec<EdgeDef>, v: usize) -> bool {
    let n = es.len();
    if v >= n || is_marker(&es[v]) || es.iter().filter(|e| !is_marker(e)).count() <= 3 {
        return false;
    }
    let p = es[v].start;
    let wall_at = |dir: isize| -> Option<usize> {
        (1..n).find_map(|k| {
            let j = (v as isize + dir * k as isize).rem_euclid(n as isize) as usize;
            (!is_marker(&es[j])).then_some(j)
        })
    };
    let hit = [wall_at(1), wall_at(-1)]
        .into_iter()
        .flatten()
        .any(|j| at(es[j].start, p, 0.005));
    if !hit {
        return false;
    }
    for j in corner_group(es, v).into_iter().rev() {
        es.remove(j);
    }
    true
}

/// Wall `i` set to `len` metres with the outline's angles kept (DACAD:
/// Länge ändern of one wall in a rectangle): its end corner moves along the
/// wall, and so do the corners after it up to the next wall that runs
/// parallel to it, which stretches; a rectangle stays a rectangle. Where no
/// wall after it runs parallel (a triangle), only the end corner moves, with
/// its direction changes. Corners are rounded to the millimetre. Returns
/// false for a wall that does not exist, a direction change, a wall of no
/// length or a length that is not a positive finite number. O(n).
pub fn set_wall_length(es: &mut [EdgeDef], i: usize, len: f64) -> bool {
    let n = es.len();
    if n < 3 || i >= n || is_marker(&es[i]) || !(len.is_finite() && len > 0.0) {
        return false;
    }
    let (a, b) = (es[i].start, es[(i + 1) % n].start);
    let cur = a.distance(b);
    if cur < MIN_WALL_LENGTH {
        return false;
    }
    let u = Point2::new((b.x - a.x) / cur, (b.y - a.y) / cur);
    let d = Point2::new(u.x * (len - cur), u.y * (len - cur));
    // sin of the angle under which a wall counts as parallel (about 0.6°):
    // drawn outlines are rounded to the millimetre, so opposite walls of a
    // turned rectangle are parallel only within that.
    const PARALLEL: f64 = 0.01;
    let mut moved = Vec::new();
    let mut j = (i + 1) % n;
    let mut closed = false;
    while j != i {
        moved.push(j);
        let next = (j + 1) % n;
        let w = Point2::new(
            es[next].start.x - es[j].start.x,
            es[next].start.y - es[j].start.y,
        );
        let l = w.x.hypot(w.y);
        if !is_marker(&es[j])
            && l >= MIN_WALL_LENGTH
            && (u.x * w.y - u.y * w.x).abs() / l < PARALLEL
        {
            closed = true;
            break;
        }
        j = next;
    }
    if !closed {
        moved = corner_group(es, (i + 1) % n);
    }
    let mm = |v: f64| {
        let r = (v * 1000.0).round() / 1000.0;
        if r == 0.0 { 0.0 } else { r }
    };
    for k in moved {
        let p = es[k].start;
        es[k].start = Point2::new(mm(p.x + d.x), mm(p.y + d.y));
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::plain_edge;

    fn wall(x: f64, y: f64) -> EdgeDef {
        plain_edge(Point2::new(x, y), 40.0, 3.0, 0.5)
    }
    fn marker(x: f64, y: f64) -> EdgeDef {
        EdgeDef {
            direction_rad: Some(1.0),
            ..wall(x, y)
        }
    }
    fn square() -> Vec<EdgeDef> {
        vec![
            wall(0.0, 0.0),
            wall(4.0, 0.0),
            wall(4.0, 4.0),
            wall(0.0, 4.0),
        ]
    }

    #[test]
    fn a_marker_moves_with_its_corner() {
        let es = vec![
            marker(0.0, 0.0),
            wall(0.0, 0.0),
            wall(4.0, 0.0),
            wall(4.0, 4.0),
        ];
        assert_eq!(corner_group(&es, 1), vec![0, 1]);
        assert_eq!(corner_group(&es, 0), vec![0, 1]);
        assert_eq!(corner_group(&es, 2), vec![2]);
        assert!(corner_group(&es, 9).is_empty());
    }

    #[test]
    fn a_corner_dropped_on_its_neighbour_merges_while_three_walls_remain() {
        let mut es = square();
        es[1].start = Point2::new(0.003, 0.0);
        assert!(merge_dropped_corner(&mut es, 1));
        assert_eq!(es.len(), 3);
        // Three walls stay three.
        es[1].start = es[0].start;
        assert!(!merge_dropped_corner(&mut es, 1));
        assert_eq!(es.len(), 3);
        // Far apart: nothing merges; a missing corner is no panic.
        let mut es = square();
        assert!(!merge_dropped_corner(&mut es, 1) && !merge_dropped_corner(&mut es, 9));
    }

    #[test]
    fn the_markers_of_a_merged_corner_go_with_it() {
        let mut es = vec![
            wall(0.0, 0.0),
            marker(4.0, 0.0),
            wall(4.0, 0.0),
            wall(4.0, 4.0),
            wall(0.0, 4.0),
            wall(0.001, 4.0),
        ];
        assert!(merge_dropped_corner(&mut es, 4));
        assert_eq!(es.len(), 5);
    }

    #[test]
    fn a_corner_only_near_its_neighbour_stays_and_a_square_corner_merges_with_its_cell() {
        // The editor's former tests of `mergeDroppedCorner`.
        let mut es = vec![
            wall(0.0, 0.0),
            wall(9.9, 0.0),
            wall(10.0, 0.0),
            wall(10.0, 6.0),
            wall(0.0, 6.0),
        ];
        assert!(!merge_dropped_corner(&mut es, 1));
        let mut es = vec![
            wall(0.0, 0.0),
            wall(5.0, 0.0),
            wall(10.0, 0.0),
            wall(10.0, 6.0),
            wall(0.0, 6.0),
        ];
        es[1].start = Point2::new(10.002, 0.0);
        assert!(merge_dropped_corner(&mut es, 1));
        assert_eq!(
            es.iter().map(|e| e.start.x).collect::<Vec<_>>(),
            vec![0.0, 10.0, 10.0, 0.0]
        );
        let mut tri = vec![wall(0.0, 0.0), wall(10.0, 0.0), wall(0.0, 6.0)];
        tri[1].start = Point2::new(0.0, 6.0);
        assert!(!merge_dropped_corner(&mut tri, 1) && tri.len() == 3);
    }

    #[test]
    fn corners_a_few_mm_apart_are_one_point_and_one_cm_is_not() {
        // The editor's former tests of `withoutRepeats` (review ERR-001).
        let p = Point2::new;
        let a = [
            p(0.0, 0.0),
            p(8.0, 0.0),
            p(8.0, 0.0),
            p(8.0, 6.0),
            p(0.0, 6.0),
            p(0.0, 0.0),
        ];
        assert_eq!(
            without_repeats(&a),
            vec![p(0.0, 0.0), p(8.0, 0.0), p(8.0, 6.0), p(0.0, 6.0)]
        );
        let b = [
            p(0.0, 0.0),
            p(8.0, 0.0),
            p(8.0, 0.006),
            p(8.0, 6.0),
            p(0.0, 6.0),
        ];
        assert_eq!(
            without_repeats(&b),
            vec![p(0.0, 0.0), p(8.0, 0.0), p(8.0, 6.0), p(0.0, 6.0)]
        );
        let c = [p(0.0, 0.0), p(0.01, 0.0), p(8.0, 6.0)];
        assert_eq!(without_repeats(&c), c.to_vec());
    }

    #[test]
    fn repeated_corners_are_dropped() {
        let p = Point2::new;
        let pts = [
            p(0.0, 0.0),
            p(0.0, 0.004),
            p(4.0, 0.0),
            p(4.0, 4.0),
            p(0.003, 0.0),
        ];
        assert_eq!(
            without_repeats(&pts),
            vec![p(0.0, 0.0), p(4.0, 0.0), p(4.0, 4.0)]
        );
        assert!(without_repeats(&[]).is_empty());
    }

    #[test]
    fn a_default_direction_change_calculates_on_every_example_wall() {
        // HUNTC-008: the old default (wall direction + 90 degrees) made the
        // roof fail at all 42 walls of the built-in examples.
        let mut tried = 0;
        for ex in crate::examples::EXAMPLES {
            let base = ex.project();
            for pi in 0..base.parts.len() {
                for wi in 0..base.parts[pi].edges.len() {
                    let mut p = base.clone();
                    let edges = &mut p.parts[pi].edges;
                    let mut m = edges[wi].clone();
                    m.half_hip = None;
                    m.breaks.clear();
                    m.direction_rad = Some(default_direction(edges, wi));
                    edges.insert(wi, m);
                    let geos = crate::openings::generate_project(&p);
                    assert!(
                        geos.iter()
                            .all(|g| g.error.is_none() && !g.faces.is_empty()),
                        "{} part {pi} wall {wi}",
                        ex.id
                    );
                    tried += 1;
                }
            }
        }
        assert!(tried >= 42, "{tried}");
    }

    #[test]
    fn a_longer_wall_keeps_a_rectangle_a_rectangle() {
        let mut es = vec![
            wall(0.0, 0.0),
            wall(12.0, 0.0),
            wall(12.0, 8.0),
            wall(0.0, 8.0),
        ];
        assert!(set_wall_length(&mut es, 0, 13.0));
        let pts: Vec<(f64, f64)> = es.iter().map(|e| (e.start.x, e.start.y)).collect();
        assert_eq!(pts, vec![(0.0, 0.0), (13.0, 0.0), (13.0, 8.0), (0.0, 8.0)]);
    }

    #[test]
    fn a_shorter_wall_moves_the_corners_up_to_the_next_parallel_wall() {
        // L: walls 0 → 14 m, 1 ↑ 7 m, 2 ← 7 m (parallel to wall 0), ...
        let mut es = vec![
            wall(0.0, 0.0),
            wall(14.0, 0.0),
            wall(14.0, 7.0),
            wall(7.0, 7.0),
            wall(7.0, 14.0),
            wall(0.0, 14.0),
        ];
        assert!(set_wall_length(&mut es, 0, 12.0));
        let pts: Vec<(f64, f64)> = es.iter().map(|e| (e.start.x, e.start.y)).collect();
        assert_eq!(pts[1..4], [(12.0, 0.0), (12.0, 7.0), (7.0, 7.0)]);
        assert_eq!(pts[4..], [(7.0, 14.0), (0.0, 14.0)]);
    }

    #[test]
    fn a_direction_change_on_a_moved_corner_moves_with_it() {
        let mut es = vec![
            wall(0.0, 0.0),
            marker(4.0, 0.0),
            wall(4.0, 0.0),
            wall(4.0, 4.0),
            wall(0.0, 4.0),
        ];
        assert!(set_wall_length(&mut es, 0, 5.0));
        assert_eq!(
            (es[1].start.x, es[2].start.x, es[3].start.x),
            (5.0, 5.0, 5.0)
        );
        assert_eq!(es[4].start.x, 0.0);
    }

    #[test]
    fn without_a_parallel_wall_only_the_end_corner_moves() {
        let mut tri = vec![wall(0.0, 0.0), wall(4.0, 0.0), wall(0.0, 3.0)];
        assert!(set_wall_length(&mut tri, 0, 8.0));
        let pts: Vec<(f64, f64)> = tri.iter().map(|e| (e.start.x, e.start.y)).collect();
        assert_eq!(pts, vec![(0.0, 0.0), (8.0, 0.0), (0.0, 3.0)]);
    }

    #[test]
    fn a_bad_wall_or_length_changes_nothing() {
        let mut es = square();
        for (i, l) in [
            (9, 5.0),
            (0, 0.0),
            (0, -1.0),
            (0, f64::NAN),
            (0, f64::INFINITY),
        ] {
            assert!(!set_wall_length(&mut es, i, l));
        }
        assert_eq!(es, square());
        let mut m = vec![
            marker(0.0, 0.0),
            wall(0.0, 0.0),
            wall(4.0, 0.0),
            wall(4.0, 4.0),
        ];
        assert!(!set_wall_length(&mut m, 0, 3.0));
    }
}
