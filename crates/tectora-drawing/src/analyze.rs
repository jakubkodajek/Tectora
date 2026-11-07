//! Finding dimension chains and tracing outlines on a sheet.
//!
//! A model reads a dimension's digits well but places points on an image only
//! roughly. These helpers take the exact geometry from the vector lines:
//! [`PageContent::dimensions`] finds the dimension lines and the marks on
//! them (extension lines or ticks), so every dimension's drawn length is
//! known without placing any point; [`PageContent::trace`] snaps a polygon
//! given roughly (e.g. read off the grid of a rendered crop) to the drawn
//! corners.

use serde::Serialize;

use crate::{PageContent, RectMm, Segment, dist, round};

/// Shortest dimension line considered, sheet mm.
const MIN_DIM_LINE_MM: f64 = 4.0;
/// Marks (extension lines, ticks) are between these lengths, sheet mm.
const MARK_MIN_MM: f64 = 1.2;
const MARK_MAX_MM: f64 = 25.0;
/// A mark that ends this close to the dimension line touches it, sheet mm.
const TOUCH_MM: f64 = 0.35;
/// Marks closer than this along the line are one mark, sheet mm.
const SAME_MARK_MM: f64 = 0.4;
/// Shortest dimension between two marks, sheet mm.
const MIN_SPAN_MM: f64 = 1.0;
/// Most chains returned.
pub const MAX_CHAINS: usize = 400;
/// Most lines looked at for dimensions in one call.
const MAX_LINES: usize = 60_000;

/// One dimension: the part of a dimension line between two marks.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Dimension {
    pub from: [f64; 2],
    pub to: [f64; 2],
    /// Where the dimension text normally sits (the middle of the span).
    pub mid: [f64; 2],
    pub sheet_mm: f64,
}

/// A dimension line with its marks, split into dimensions.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct DimensionChain {
    /// Direction of the line on the sheet, degrees in [0, 180).
    pub angle_deg: f64,
    /// First and last mark.
    pub from: [f64; 2],
    pub to: [f64; 2],
    /// Total length from the first to the last mark, sheet mm.
    pub sheet_mm: f64,
    pub dimensions: Vec<Dimension>,
}

/// A polygon snapped to the drawing.
#[derive(Clone, Debug, Serialize)]
pub struct Trace {
    /// Corners on the sheet, mm.
    pub points: Vec<[f64; 2]>,
    /// Whether each corner snapped to a drawn corner or line crossing.
    pub snapped: Vec<bool>,
    /// Edge i runs from point i to point i + 1 (the last one closes the polygon).
    pub edges_sheet_mm: Vec<f64>,
    /// Edge directions on the sheet, degrees in [0, 180).
    pub edges_angle_deg: Vec<f64>,
    /// Enclosed area on the sheet, mm².
    pub area_sheet_mm2: f64,
}

fn angle_of(a: [f64; 2], b: [f64; 2]) -> f64 {
    let ang = (b[1] - a[1]).atan2(b[0] - a[0]).to_degrees();
    ang.rem_euclid(180.0)
}

/// Smallest angle between two undirected directions, degrees in [0, 90].
fn angle_between(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(180.0);
    d.min(180.0 - d)
}

impl PageContent {
    /// Dimension chains in `region`: lines with at least two marks
    /// (extension lines or ticks of one kind) touching or crossing them.
    pub fn dimensions(&self, region: Option<RectMm>) -> Vec<DimensionChain> {
        let mut lines = self.lines(region, 0.0, MAX_LINES);
        lines.retain(|s| s.length() >= MARK_MIN_MM);
        let marks: Vec<&Segment> = lines
            .iter()
            .filter(|s| (MARK_MIN_MM..=MARK_MAX_MM).contains(&s.length()))
            .collect();
        let index = GridIndex::new(&marks, 5.0);

        let mut chains: Vec<(DimensionChain, f64, usize)> = Vec::new();
        for l in lines.iter().filter(|s| s.length() >= MIN_DIM_LINE_MM) {
            if let Some(c) = chain_on(l, &marks, &index) {
                chains.push(c);
            }
        }
        // A doubled line (or a line drawn twice) gives the same chain twice:
        // keep the one with more marks.
        chains.sort_by_key(|c| std::cmp::Reverse(c.2));
        let mut kept: Vec<(DimensionChain, f64, usize)> = Vec::new();
        for c in chains {
            let dup = kept.iter().any(|k| {
                angle_between(k.0.angle_deg, c.0.angle_deg) < 0.5
                    && (k.1 - c.1).abs() < 0.5
                    && overlap(&k.0, &c.0) > 0.8
            });
            if !dup {
                kept.push(c);
            }
        }
        let mut out: Vec<DimensionChain> = kept.into_iter().map(|c| c.0).collect();
        out.sort_by(|a, b| {
            let ka = (round(a.from[1].min(a.to[1]), 0), a.from[0].min(a.to[0]));
            let kb = (round(b.from[1].min(b.to[1]), 0), b.from[0].min(b.to[0]));
            ka.0.total_cmp(&kb.0).then(ka.1.total_cmp(&kb.1))
        });
        out.truncate(MAX_CHAINS);
        out
    }

    /// Snaps each of `points` (sheet mm) to the drawing within `snap_mm` and
    /// measures the closed polygon. A corner snaps first to the crossing of
    /// drawn lines that run along its two edges (so hatching and patterns
    /// near the corner do not pull it off), otherwise to the nearest drawn
    /// corner or line crossing.
    pub fn trace(&self, points: &[[f64; 2]], snap_mm: f64) -> Trace {
        let n = points.len();
        let snapped: Vec<Option<[f64; 2]>> = (0..n)
            .map(|i| {
                let p = points[i];
                let before = points[(i + n - 1) % n];
                let after = points[(i + 1) % n];
                let corner = if n >= 3 {
                    self.snap_corner(p, before, after, snap_mm)
                } else {
                    None
                };
                corner.or_else(|| self.snap(p, snap_mm))
            })
            .collect();
        let pts: Vec<[f64; 2]> = points
            .iter()
            .zip(&snapped)
            .map(|(&p, s)| s.unwrap_or(p))
            .collect();
        let n = pts.len();
        let mut edges = Vec::with_capacity(n);
        let mut angles = Vec::with_capacity(n);
        let mut area2 = 0.0;
        for i in 0..n {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            edges.push(round(dist(a, b), 3));
            angles.push(round(angle_of(a, b), 2));
            area2 += a[0] * b[1] - b[0] * a[1];
        }
        Trace {
            points: pts
                .iter()
                .map(|p| [round(p[0], 3), round(p[1], 3)])
                .collect(),
            snapped: snapped.iter().map(Option::is_some).collect(),
            edges_sheet_mm: edges,
            edges_angle_deg: angles,
            area_sheet_mm2: round(area2.abs() / 2.0, 2),
        }
    }
}

/// Fraction of the shorter chain covered by the other one.
fn overlap(a: &DimensionChain, b: &DimensionChain) -> f64 {
    let r = a.angle_deg.to_radians();
    let (dx, dy) = (r.cos(), r.sin());
    let t = |p: [f64; 2]| p[0] * dx + p[1] * dy;
    let (a0, a1) = minmax(t(a.from), t(a.to));
    let (b0, b1) = minmax(t(b.from), t(b.to));
    let inter = (a1.min(b1) - a0.max(b0)).max(0.0);
    let shorter = (a1 - a0).min(b1 - b0).max(1e-9);
    inter / shorter
}

fn minmax(a: f64, b: f64) -> (f64, f64) {
    if a <= b { (a, b) } else { (b, a) }
}

/// The chain on dimension line `l`, its offset from the origin and its mark count.
fn chain_on(
    l: &Segment,
    marks: &[&Segment],
    index: &GridIndex,
) -> Option<(DimensionChain, f64, usize)> {
    let len = l.length();
    let (dx, dy) = ((l.b[0] - l.a[0]) / len, (l.b[1] - l.a[1]) / len);
    let (nx, ny) = (-dy, dx);
    let line_angle = angle_of(l.a, l.b);
    // (position along the line, mark direction, side: 0 crossing, ±1 the
    // side the mark leaves the line to)
    let mut found: Vec<(f64, f64, i8)> = Vec::new();
    for &mi in &index.near(l) {
        let m = marks[mi];
        if std::ptr::eq(m, l) {
            continue;
        }
        let ma = angle_of(m.a, m.b);
        if angle_between(ma, line_angle) < 25.0 {
            continue;
        }
        let sa = (m.a[0] - l.a[0]) * nx + (m.a[1] - l.a[1]) * ny;
        let sb = (m.b[0] - l.a[0]) * nx + (m.b[1] - l.a[1]) * ny;
        let crosses = sa * sb <= 0.0;
        let side = if crosses && sa.abs().min(sb.abs()) > TOUCH_MM {
            0
        } else if sa.abs() > sb.abs() {
            sa.signum() as i8
        } else {
            sb.signum() as i8
        };
        let p = if crosses {
            let k = sa / (sa - sb);
            [
                m.a[0] + k * (m.b[0] - m.a[0]),
                m.a[1] + k * (m.b[1] - m.a[1]),
            ]
        } else if sa.abs().min(sb.abs()) <= TOUCH_MM {
            if sa.abs() < sb.abs() { m.a } else { m.b }
        } else {
            continue;
        };
        let t = (p[0] - l.a[0]) * dx + (p[1] - l.a[1]) * dy;
        if t < -TOUCH_MM || t > len + TOUCH_MM {
            continue;
        }
        found.push((t, ma, side));
    }
    if found.len() < 2 {
        return None;
    }
    // Marks of one chain are all of one kind: extension lines coming from
    // one side, or ticks at one slant crossing the line. Strokes of the
    // dimension text touch the line from the other side, and other lines
    // of the drawing cross it at other angles. Take the largest group.
    let mut best: Vec<f64> = Vec::new();
    for &(_, ang, side) in &found {
        let group: Vec<f64> = found
            .iter()
            .filter(|(_, a, s)| *s == side && angle_between(*a, ang) <= 6.0)
            .map(|(t, _, _)| *t)
            .collect();
        if group.len() > best.len() {
            best = group;
        }
    }
    best.sort_by(f64::total_cmp);
    best.dedup_by(|a, b| (*a - *b).abs() < SAME_MARK_MM);
    if best.len() < 2 {
        return None;
    }
    let at = |t: f64| [l.a[0] + t * dx, l.a[1] + t * dy];
    let mut dims = Vec::new();
    for w in best.windows(2) {
        let span = w[1] - w[0];
        if span < MIN_SPAN_MM {
            continue;
        }
        let (f, t) = (at(w[0]), at(w[1]));
        dims.push(Dimension {
            from: [round(f[0], 2), round(f[1], 2)],
            to: [round(t[0], 2), round(t[1], 2)],
            mid: [round((f[0] + t[0]) / 2.0, 2), round((f[1] + t[1]) / 2.0, 2)],
            sheet_mm: round(span, 3),
        });
    }
    if dims.is_empty() || is_pattern(&dims) {
        return None;
    }
    let (f, t) = (at(best[0]), at(*best.last().unwrap_or(&best[0])));
    let offset = l.a[0] * nx + l.a[1] * ny;
    Some((
        DimensionChain {
            angle_deg: round(line_angle, 2),
            from: [round(f[0], 2), round(f[1], 2)],
            to: [round(t[0], 2), round(t[1], 2)],
            sheet_mm: round(dist(f, t), 3),
            dimensions: dims,
        },
        offset,
        best.len(),
    ))
}

/// Many equal short spans are a hatch or tile pattern crossing a line,
/// not a dimension chain.
fn is_pattern(dims: &[Dimension]) -> bool {
    if dims.len() < 8 {
        return false;
    }
    let mut spans: Vec<f64> = dims.iter().map(|d| d.sheet_mm).collect();
    spans.sort_by(f64::total_cmp);
    let median = spans[spans.len() / 2];
    let alike = spans
        .iter()
        .filter(|s| (*s - median).abs() <= median * 0.05)
        .count();
    // Tiles drawn in plan give a dozen and more marks a few mm apart.
    alike * 10 >= spans.len() * 7 || (spans.len() >= 12 && median < 5.0)
}

/// Buckets segments by the grid cells their bounding boxes touch.
struct GridIndex {
    cell: f64,
    map: std::collections::HashMap<(i64, i64), Vec<usize>>,
}

impl GridIndex {
    fn new(segs: &[&Segment], cell: f64) -> Self {
        let mut map: std::collections::HashMap<(i64, i64), Vec<usize>> = Default::default();
        for (i, s) in segs.iter().enumerate() {
            let (x0, x1) = minmax(s.a[0], s.b[0]);
            let (y0, y1) = minmax(s.a[1], s.b[1]);
            for cx in (x0 / cell).floor() as i64..=(x1 / cell).floor() as i64 {
                for cy in (y0 / cell).floor() as i64..=(y1 / cell).floor() as i64 {
                    map.entry((cx, cy)).or_default().push(i);
                }
            }
        }
        GridIndex { cell, map }
    }

    /// Indices of segments in the cells along `l` and next to them.
    fn near(&self, l: &Segment) -> Vec<usize> {
        let mut out = Vec::new();
        let c = self.cell;
        // Walk only the cells along the line, not its whole bounding box.
        let steps = ((l.length() / c).ceil() as i64 * 2).max(1);
        let mut cells = std::collections::HashSet::new();
        for k in 0..=steps {
            let f = k as f64 / steps as f64;
            let p = [
                l.a[0] + f * (l.b[0] - l.a[0]),
                l.a[1] + f * (l.b[1] - l.a[1]),
            ];
            for ox in -1..=1 {
                for oy in -1..=1 {
                    cells.insert((
                        (p[0] / c).floor() as i64 + ox,
                        (p[1] / c).floor() as i64 + oy,
                    ));
                }
            }
        }
        let mut seen = std::collections::HashSet::new();
        for cell in cells {
            if let Some(v) = self.map.get(&cell) {
                for &i in v {
                    if seen.insert(i) {
                        out.push(i);
                    }
                }
            }
        }
        out
    }
}
