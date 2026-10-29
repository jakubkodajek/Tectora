//! DACAD's flat roof calculation (Berechnung › Flachdach). DACAD.HLP: the
//! flat roof lies at the highest eave, dormers are ignored, windows,
//! chimneys and roof openings are calculated. Measured in DACAD DE.exe
//! 2026-10-29 (cases FD1, FD2): the face is the outline moved out by each
//! wall's overhang, every edge of it is an eave (T), a chimney is a hole
//! with all four sides as front abutments (WV).

use crate::geom::{Point2, signed_area};
use crate::model::{Project, RoofPart};
use crate::roof::{LineKind, Opening, OpeningKind, RoofFace, RoofGeometry, RoofLine};

/// The flat roof of every part (same order as `project.parts`).
pub fn generate_project(project: &Project) -> Vec<RoofGeometry> {
    project.parts.iter().map(|p| part(project, p)).collect()
}

/// The outline moved out by each wall's overhang; zero-length walls (direction
/// markers) are left out.
fn overhang_outline(part: &RoofPart) -> Vec<Point2> {
    let mut p = part.clone();
    p.normalize_ccw();
    let n = p.edges.len();
    // Each wall as a line moved out (to the right of a counter-clockwise wall).
    let lines: Vec<(Point2, Point2)> = (0..n)
        .filter_map(|i| {
            let a = p.edges[i].start;
            let b = p.edges[(i + 1) % n].start;
            let len = ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
            (len > 1e-9).then(|| {
                let u = Point2::new((b.x - a.x) / len, (b.y - a.y) / len);
                let ov = p.edges[i].overhang;
                (Point2::new(a.x + u.y * ov, a.y - u.x * ov), u)
            })
        })
        .collect();
    let m = lines.len();
    (0..m)
        .map(|i| {
            let (p0, u0) = lines[(i + m - 1) % m];
            let (p1, u1) = lines[i];
            let den = u0.x * u1.y - u0.y * u1.x;
            if den.abs() < 1e-12 {
                // Collinear walls: the corner stays on the later wall's line.
                return p1;
            }
            let t = ((p1.x - p0.x) * u1.y - (p1.y - p0.y) * u1.x) / den;
            Point2::new(p0.x + u0.x * t, p0.y + u0.y * t)
        })
        .collect()
}

fn inside(poly: &[Point2], q: Point2) -> bool {
    let mut c = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a.y > q.y) != (b.y > q.y) && q.x < a.x + (b.x - a.x) * (q.y - a.y) / (b.y - a.y) {
            c = !c;
        }
    }
    c
}

fn part(project: &Project, part: &RoofPart) -> RoofGeometry {
    let z = part
        .edges
        .iter()
        .map(|e| e.eave_height)
        .fold(f64::NEG_INFINITY, f64::max);
    let poly = overhang_outline(part);
    let mut geo = RoofGeometry::default();
    if poly.len() < 3 || !z.is_finite() {
        return geo;
    }
    let p3 = |p: Point2| [p.x, p.y, z];
    let n = poly.len();
    for i in 0..n {
        geo.lines.push(RoofLine {
            kind: LineKind::Eave,
            a: p3(poly[i]),
            b: p3(poly[(i + 1) % n]),
            side: None,
        });
    }
    let mut deducted = 0.0;
    let holes = project
        .chimneys
        .iter()
        .map(|c| (c, OpeningKind::Chimney))
        .chain(project.roof_openings.iter().map(|c| (c, OpeningKind::Hole)));
    for (c, kind) in holes {
        if c.corners.len() < 3 || !c.corners.iter().all(|&q| inside(&poly, q)) {
            continue;
        }
        let area = signed_area(&c.corners).abs();
        deducted += area;
        geo.openings.push(Opening { kind, area });
        if kind == OpeningKind::Chimney {
            let k = c.corners.len();
            for i in 0..k {
                geo.lines.push(RoofLine {
                    kind: LineKind::AbutmentFront,
                    a: p3(c.corners[i]),
                    b: p3(c.corners[(i + 1) % k]),
                    side: None,
                });
            }
        }
    }
    for w in &project.windows {
        if inside(&poly, Point2::new(w.x, w.y)) {
            let area = w.width * w.height;
            deducted += area;
            geo.openings.push(Opening {
                kind: OpeningKind::Window,
                area,
            });
        }
    }
    geo.faces.push(RoofFace {
        edge: 0,
        pitch_deg: 0.0,
        polygon: poly.iter().map(|&p| p3(p)).collect(),
        area: signed_area(&poly).abs() - deducted,
        deducted,
    });
    geo
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Chimney, EdgeDef};

    fn wall(x: f64, y: f64, eave: f64, ov: f64) -> EdgeDef {
        EdgeDef {
            start: Point2::new(x, y),
            pitch_deg: 40.0,
            eave_height: eave,
            overhang: ov,
            half_hip: None,
            breaks: Vec::new(),
            priority: 0,
            direction_rad: None,
            clad: false,
            source: None,
        }
    }

    // DACAD case FD2: L-shaped outline, a lower and a higher eave, one wall
    // with overhang 0.3, a 0.6 m chimney: face 82.2 m² less 0.36, eaves
    // 40 m at height 3.5, chimney sides 2.4 m WV.
    #[test]
    fn flat_roof_matches_dacad_fd2() {
        let mut p = Project::new("FD2");
        p.flat_roof = true;
        p.parts.push(RoofPart {
            name: "L".into(),
            edges: vec![
                wall(0.0, 0.0, 3.0, 0.5),
                wall(10.0, 0.0, 3.0, 0.5),
                wall(10.0, 4.0, 3.5, 0.5),
                wall(6.0, 4.0, 3.5, 0.3),
                wall(6.0, 8.0, 3.0, 0.5),
                wall(0.0, 8.0, 3.0, 0.5),
            ],
        });
        p.chimneys.push(Chimney {
            corners: vec![
                Point2::new(1.0, 1.0),
                Point2::new(1.6, 1.0),
                Point2::new(1.6, 1.6),
                Point2::new(1.0, 1.6),
            ],
            height: 4.0,
        });
        let g = &generate_project(&p)[0];
        assert!((g.faces[0].area - (82.2 - 0.36)).abs() < 1e-6);
        let eave: f64 = g
            .lines
            .iter()
            .filter(|l| l.kind == LineKind::Eave)
            .map(|l| l.length())
            .sum();
        let wv: f64 = g
            .lines
            .iter()
            .filter(|l| l.kind == LineKind::AbutmentFront)
            .map(|l| l.length())
            .sum();
        assert!((eave - 40.0).abs() < 1e-9);
        assert!((wv - 2.4).abs() < 1e-9);
        assert!(g.lines.iter().all(|l| l.a[2] == 3.5));
    }
}
