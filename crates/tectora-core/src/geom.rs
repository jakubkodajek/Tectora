//! Small 2D geometry helpers (metres).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point2 {
    pub x: f64,
    pub y: f64,
}

impl Point2 {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn distance(self, other: Point2) -> f64 {
        (self.x - other.x).hypot(self.y - other.y)
    }

    pub fn approx_eq(self, other: Point2, tol: f64) -> bool {
        self.distance(other) <= tol
    }
}

/// Signed area of a closed polygon (shoelace). Positive = counter-clockwise.
pub fn signed_area(points: &[Point2]) -> f64 {
    let n = points.len();
    if n < 3 {
        return 0.0;
    }
    let mut sum = 0.0;
    for i in 0..n {
        let a = points[i];
        let b = points[(i + 1) % n];
        sum += a.x * b.y - b.x * a.y;
    }
    sum / 2.0
}

/// Perimeter of a closed polygon.
pub fn perimeter(points: &[Point2]) -> f64 {
    let n = points.len();
    (0..n)
        .map(|i| points[i].distance(points[(i + 1) % n]))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangle_area_and_perimeter() {
        let r = [
            Point2::new(0.0, 0.0),
            Point2::new(4.0, 0.0),
            Point2::new(4.0, 3.0),
            Point2::new(0.0, 3.0),
        ];
        assert_eq!(signed_area(&r), 12.0);
        assert_eq!(perimeter(&r), 14.0);
        let mut cw = r;
        cw.reverse();
        assert_eq!(signed_area(&cw), -12.0);
    }
}
