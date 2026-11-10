//! Position and size of a chimney or roof opening as the editor's object
//! bar shows them: the smallest box around the plan polygon that lines up
//! with its first side. Port of `openingBox` / `reshapeOpening` in the
//! editor's `openings.ts`.
//!
//! This is editor geometry, not roof logic: the corners it writes go to the
//! core unchanged, in the order they had, so a rectangle stays the
//! rectangle DACAD gets and any other convex polygon keeps its shape.

use serde::{Deserialize, Serialize};

use super::round_to;
use crate::geom::Point2;

/// Sides shorter than this, m, give no direction.
const MIN_SIDE: f64 = 1e-6;

/// The box of an opening: corner `x`, `y` (the box corner with the smallest
/// coordinates along both box axes), `width` along the first side, `depth`
/// across it, and the direction of the first side in degrees.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct OpeningBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub depth: f64,
    pub rotation_deg: f64,
}

/// Exact box frame: origin, unit axes and extents along them.
struct Frame {
    origin: Point2,
    u: Point2,
    v: Point2,
    width: f64,
    depth: f64,
    /// Corners in box units, 0..1 along each axis.
    unit: Vec<(f64, f64)>,
}

fn frame(corners: &[Point2]) -> Option<Frame> {
    let n = corners.len();
    if n < 3 || !corners.iter().all(|c| c.x.is_finite() && c.y.is_finite()) {
        return None;
    }
    let (dx, dy, len) = (0..n).find_map(|i| {
        let (a, b) = (corners[i], corners[(i + 1) % n]);
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let len = dx.hypot(dy);
        (len > MIN_SIDE).then_some((dx, dy, len))
    })?;
    let u = Point2::new(dx / len, dy / len);
    let v = Point2::new(-u.y, u.x);
    let o = corners[0];
    let st: Vec<(f64, f64)> = corners
        .iter()
        .map(|c| {
            let (x, y) = (c.x - o.x, c.y - o.y);
            (x * u.x + y * u.y, x * v.x + y * v.y)
        })
        .collect();
    let fold = |f: fn(&(f64, f64)) -> f64| {
        st.iter()
            .map(f)
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), x| {
                (lo.min(x), hi.max(x))
            })
    };
    let (s0, s1) = fold(|p| p.0);
    let (t0, t1) = fold(|p| p.1);
    let (width, depth) = (s1 - s0, t1 - t0);
    if width <= MIN_SIDE || depth <= MIN_SIDE {
        return None;
    }
    Some(Frame {
        origin: Point2::new(o.x + u.x * s0 + v.x * t0, o.y + u.y * s0 + v.y * t0),
        u,
        v,
        width,
        depth,
        unit: st
            .iter()
            .map(|&(s, t)| ((s - s0) / width, (t - t0) / depth))
            .collect(),
    })
}

/// The box of a chimney or roof opening; `None` for fewer than 3 corners,
/// non-finite corners or a polygon with no area across its first side.
pub fn opening_box(corners: &[Point2]) -> Option<OpeningBox> {
    let f = frame(corners)?;
    Some(OpeningBox {
        x: round_to(f.origin.x, 3),
        y: round_to(f.origin.y, 3),
        width: round_to(f.width, 3),
        depth: round_to(f.depth, 3),
        rotation_deg: round_to(f.u.y.atan2(f.u.x).to_degrees(), 4),
    })
}

/// The corners moved, stretched and turned so that their box becomes `to`.
/// Each corner keeps its place inside the box, so a rectangle stays a
/// rectangle and the corner order is kept. Values of `to` equal to the
/// current box keep the exact current frame, so changing only the width
/// does not turn or shift the opening by rounding.
pub fn reshape_opening(corners: &[Point2], to: &OpeningBox) -> Result<Vec<Point2>, String> {
    let f = frame(corners).ok_or("the opening has no area to resize")?;
    let ok = |v: f64| v.is_finite() && v.abs() < crate::model::MAX_COORD;
    if !(ok(to.x) && ok(to.y) && ok(to.rotation_deg)) {
        return Err("x, y and rotation must be finite numbers in range".into());
    }
    if !(to.width.is_finite() && to.depth.is_finite() && to.width >= 0.01 && to.depth >= 0.01) {
        return Err("width and depth must be at least 0.01 m".into());
    }
    if to.width > 100.0 || to.depth > 100.0 {
        return Err("width and depth must be at most 100 m".into());
    }
    let was = opening_box(corners).expect("frame exists");
    let same = |a: f64, b: f64| (a - b).abs() < 1e-9;
    let (u, v) = if same(to.rotation_deg, was.rotation_deg) {
        (f.u, f.v)
    } else {
        let (sin, cos) = to.rotation_deg.to_radians().sin_cos();
        (Point2::new(cos, sin), Point2::new(-sin, cos))
    };
    let origin = Point2::new(
        if same(to.x, was.x) { f.origin.x } else { to.x },
        if same(to.y, was.y) { f.origin.y } else { to.y },
    );
    let width = if same(to.width, was.width) {
        f.width
    } else {
        to.width
    };
    let depth = if same(to.depth, was.depth) {
        f.depth
    } else {
        to.depth
    };
    Ok(f.unit
        .iter()
        .map(|&(a, b)| {
            let (s, t) = (a * width, b * depth);
            Point2::new(
                round_to(origin.x + u.x * s + v.x * t, 3),
                round_to(origin.y + u.y * s + v.y * t, 3),
            )
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::probe::chimney_from_rect;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    #[test]
    fn reads_a_rotated_rectangle_at_any_angle() {
        // Corners rounded to mm used to fail the rectangle test at most
        // angles, which left the chimney without size fields.
        for deg in 0..360 {
            let c = chimney_from_rect(3.217, 1.433, 0.62, 0.45, deg as f64, 8.0);
            let b = opening_box(&c.corners).expect("box");
            assert!(
                (b.width - 0.62).abs() < 2e-3 && (b.depth - 0.45).abs() < 2e-3,
                "{deg}: {b:?}"
            );
            let r = (b.rotation_deg - deg as f64).rem_euclid(360.0);
            assert!(!(0.2..=359.8).contains(&r), "{deg}: {b:?}");
            assert!(
                (b.x - 3.217).abs() < 2e-3 && (b.y - 1.433).abs() < 2e-3,
                "{deg}: {b:?}"
            );
        }
    }

    #[test]
    fn resizes_a_clockwise_rectangle_and_keeps_its_corner_order() {
        // A mirrored chimney runs clockwise.
        let cw = vec![p(0.0, 0.0), p(0.0, 0.5), p(0.8, 0.5), p(0.8, 0.0)];
        let b = opening_box(&cw).unwrap();
        assert_eq!((b.width, b.depth), (0.5, 0.8));
        let out = reshape_opening(&cw, &OpeningBox { width: 1.0, ..b }).unwrap();
        assert_eq!(
            out,
            vec![p(0.0, 0.0), p(0.0, 1.0), p(0.8, 1.0), p(0.8, 0.0)]
        );
    }

    #[test]
    fn stretches_any_polygon_in_its_box() {
        let hex = vec![
            p(0.0, 0.0),
            p(1.0, 0.0),
            p(1.5, 0.5),
            p(1.0, 1.0),
            p(0.0, 1.0),
            p(-0.5, 0.5),
        ];
        let b = opening_box(&hex).unwrap();
        assert_eq!((b.x, b.y, b.width, b.depth), (-0.5, 0.0, 2.0, 1.0));
        let out = reshape_opening(
            &hex,
            &OpeningBox {
                depth: 2.0,
                x: 0.5,
                ..b
            },
        )
        .unwrap();
        assert_eq!(
            out,
            vec![
                p(1.0, 0.0),
                p(2.0, 0.0),
                p(2.5, 1.0),
                p(2.0, 2.0),
                p(1.0, 2.0),
                p(0.5, 1.0)
            ]
        );
    }

    #[test]
    fn an_unchanged_box_gives_the_same_corners_and_width_alone_does_not_turn() {
        let c = chimney_from_rect(4.0, 1.0, 0.6, 0.6, 20.0, 6.0).corners;
        let b = opening_box(&c).unwrap();
        assert_eq!(reshape_opening(&c, &b).unwrap(), c);
        let wide = reshape_opening(&c, &OpeningBox { width: 1.2, ..b }).unwrap();
        let nb = opening_box(&wide).unwrap();
        assert!(
            (nb.rotation_deg - b.rotation_deg).abs() < 0.1 && (nb.depth - b.depth).abs() < 2e-3
        );
        assert!((nb.width - 1.2).abs() < 2e-3);
        assert_eq!(wide[0], c[0]);
    }

    #[test]
    fn refuses_bad_sizes_and_degenerate_polygons() {
        let c = chimney_from_rect(0.0, 0.0, 1.0, 1.0, 0.0, 0.0).corners;
        let b = opening_box(&c).unwrap();
        for bad in [0.0, -1.0, f64::NAN, 1e6] {
            assert!(reshape_opening(&c, &OpeningBox { width: bad, ..b }).is_err());
        }
        assert!(
            reshape_opening(
                &c,
                &OpeningBox {
                    x: f64::INFINITY,
                    ..b
                }
            )
            .is_err()
        );
        assert!(opening_box(&[p(0.0, 0.0), p(1.0, 0.0), p(2.0, 0.0)]).is_none());
        assert!(opening_box(&[p(0.0, 0.0), p(1.0, 0.0)]).is_none());
    }
}
