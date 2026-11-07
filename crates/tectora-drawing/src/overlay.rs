//! Shapes drawn over a rendered crop in sheet millimetres, such as a roof
//! calculated from the drawing, so the drawing and the model can be
//! compared in one image.

use hayro::vello_cpu::Pixmap;
use hayro::vello_cpu::color::PremulRgba8;

/// A filled polygon, blended over the drawing.
#[derive(Clone, Debug)]
pub struct OverlayFill {
    /// Corners in sheet mm.
    pub points: Vec<[f64; 2]>,
    pub color: [u8; 3],
    /// Opacity, 0 to 1.
    pub alpha: f64,
    /// Height over the sheet, `z = p[0] * x + p[1] * y + p[2]` (x, y in
    /// sheet mm). Where fills with a plane overlap, only the highest one is
    /// drawn, as seen from above.
    pub z_plane: Option<[f64; 3]>,
}

/// A straight line between two sheet points (mm).
#[derive(Clone, Debug)]
pub struct OverlayLine {
    pub a: [f64; 2],
    pub b: [f64; 2],
    pub color: [u8; 3],
}

/// What [`crate::Drawing::render_overlaid`] draws over the page: fills
/// first, then lines.
#[derive(Clone, Debug, Default)]
pub struct Overlay {
    pub fills: Vec<OverlayFill>,
    pub lines: Vec<OverlayLine>,
}

/// More shapes than this are not drawn (the caller validates its input;
/// this only bounds the work).
pub const MAX_SHAPES: usize = 20_000;

fn blend(p: &mut PremulRgba8, c: [u8; 3], alpha: f64) {
    let mix = |d: u8, s: u8| (d as f64 * (1.0 - alpha) + s as f64 * alpha).round() as u8;
    *p = PremulRgba8 {
        r: mix(p.r, c[0]),
        g: mix(p.g, c[1]),
        b: mix(p.b, c[2]),
        a: 255,
    };
}

/// The inverse of a pixel-to-sheet map: sheet mm to image pixel.
fn sheet_to_pixel(m: [f64; 6]) -> Option<impl Fn([f64; 2]) -> [f64; 2]> {
    let det = m[0] * m[4] - m[1] * m[3];
    if !det.is_finite() || det.abs() < 1e-12 {
        return None;
    }
    Some(move |p: [f64; 2]| {
        let (x, y) = (p[0] - m[2], p[1] - m[5]);
        [(m[4] * x - m[1] * y) / det, (-m[3] * x + m[0] * y) / det]
    })
}

/// Draws `overlay` over a pixmap whose pixel (u, v) maps to the sheet by
/// `m` (see `Rendered::pixel_to_sheet`). Lines are about `line_px` wide.
pub fn draw(pixmap: &mut Pixmap, m: [f64; 6], overlay: &Overlay, line_px: f64) {
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);
    let Some(to_px) = sheet_to_pixel(m) else {
        return;
    };
    if w == 0 || h == 0 {
        return;
    }
    let data = pixmap.data_mut();
    let mut planar: Vec<Planar> = Vec::new();
    for f in overlay.fills.iter().take(MAX_SHAPES) {
        let pts: Vec<[f64; 2]> = f.points.iter().map(|p| to_px(*p)).collect();
        if pts.len() < 3 || pts.iter().flatten().any(|v| !v.is_finite()) {
            continue;
        }
        let alpha = f.alpha.clamp(0.0, 1.0);
        match f.z_plane.filter(|z| z.iter().all(|v| v.is_finite())) {
            Some(p) => {
                // The plane over image pixels: z = a*u + b*v + c.
                let z = [
                    p[0] * m[0] + p[1] * m[3],
                    p[0] * m[1] + p[1] * m[4],
                    p[0] * m[2] + p[1] * m[5] + p[2],
                ];
                let lo = pts.iter().map(|q| q[1]).fold(f64::MAX, f64::min);
                let hi = pts.iter().map(|q| q[1]).fold(f64::MIN, f64::max);
                planar.push(Planar {
                    pts,
                    z,
                    lo,
                    hi,
                    color: f.color,
                    alpha,
                });
            }
            None => fill_polygon(data, w, h, &pts, f.color, alpha),
        }
    }
    fill_visible(data, w, h, &planar);
    let r = (line_px / 2.0).clamp(0.5, 8.0);
    for l in overlay.lines.iter().take(MAX_SHAPES) {
        let (a, b) = (to_px(l.a), to_px(l.b));
        if a.iter().chain(&b).any(|v| !v.is_finite()) {
            continue;
        }
        draw_line(data, w, h, a, b, r, l.color);
    }
}

/// A fill with a height plane, in image pixels.
struct Planar {
    pts: Vec<[f64; 2]>,
    z: [f64; 3],
    lo: f64,
    hi: f64,
    color: [u8; 3],
    alpha: f64,
}

/// The x of each crossing of the row at height `y` with the polygon's edges,
/// sorted: the even-odd spans are the pairs.
fn crossings(pts: &[[f64; 2]], y: f64, xs: &mut Vec<f64>) {
    xs.clear();
    let n = pts.len();
    for i in 0..n {
        let (p, q) = (pts[i], pts[(i + 1) % n]);
        if (p[1] <= y) != (q[1] <= y) {
            xs.push(p[0] + (y - p[1]) / (q[1] - p[1]) * (q[0] - p[0]));
        }
    }
    xs.sort_by(f64::total_cmp);
}

/// Fills each pixel with the highest of the planar fills covering it, row by
/// row (a z-buffer one row high).
fn fill_visible(data: &mut [PremulRgba8], w: usize, h: usize, fills: &[Planar]) {
    if fills.is_empty() {
        return;
    }
    let mut top = vec![f64::NEG_INFINITY; w];
    let mut owner = vec![usize::MAX; w];
    let mut xs = Vec::new();
    for v in 0..h {
        let y = v as f64 + 0.5;
        top.fill(f64::NEG_INFINITY);
        owner.fill(usize::MAX);
        let mut any = false;
        for (i, f) in fills.iter().enumerate() {
            if y < f.lo || y > f.hi {
                continue;
            }
            crossings(&f.pts, y, &mut xs);
            for pair in xs.chunks_exact(2) {
                let u0 = (pair[0] - 0.5).ceil().max(0.0);
                let u1 = (pair[1] - 0.5).floor().min(w as f64 - 1.0);
                if u0 > u1 {
                    continue;
                }
                for u in (u0 as usize)..=(u1 as usize) {
                    let z = f.z[0] * (u as f64 + 0.5) + f.z[1] * y + f.z[2];
                    if z > top[u] {
                        top[u] = z;
                        owner[u] = i;
                        any = true;
                    }
                }
            }
        }
        if !any {
            continue;
        }
        for u in 0..w {
            if let Some(f) = fills.get(owner[u]) {
                blend(&mut data[v * w + u], f.color, f.alpha);
            }
        }
    }
}

/// Even-odd scanline fill, sampled at pixel centres.
fn fill_polygon(
    data: &mut [PremulRgba8],
    w: usize,
    h: usize,
    pts: &[[f64; 2]],
    c: [u8; 3],
    alpha: f64,
) {
    let ymin = pts.iter().map(|p| p[1]).fold(f64::MAX, f64::min).max(0.0);
    let ymax = pts
        .iter()
        .map(|p| p[1])
        .fold(f64::MIN, f64::max)
        .min(h as f64);
    if ymin >= ymax {
        return;
    }
    let mut xs: Vec<f64> = Vec::with_capacity(pts.len());
    for v in (ymin.floor() as usize)..(ymax.ceil() as usize).min(h) {
        crossings(pts, v as f64 + 0.5, &mut xs);
        for pair in xs.chunks_exact(2) {
            let u0 = (pair[0] - 0.5).ceil().max(0.0);
            let u1 = (pair[1] - 0.5).floor().min(w as f64 - 1.0);
            if u0 > u1 {
                continue;
            }
            for u in (u0 as usize)..=(u1 as usize) {
                blend(&mut data[v * w + u], c, alpha);
            }
        }
    }
}

/// Clips the segment a-b to the rectangle [lo, hi] (Liang-Barsky).
fn clip(a: [f64; 2], b: [f64; 2], lo: [f64; 2], hi: [f64; 2]) -> Option<([f64; 2], [f64; 2])> {
    let d = [b[0] - a[0], b[1] - a[1]];
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for k in 0..2 {
        for (p, q) in [(-d[k], a[k] - lo[k]), (d[k], hi[k] - a[k])] {
            if p == 0.0 {
                if q < 0.0 {
                    return None;
                }
            } else {
                let t = q / p;
                if p < 0.0 {
                    t0 = t0.max(t);
                } else {
                    t1 = t1.min(t);
                }
            }
        }
    }
    (t0 <= t1).then(|| {
        (
            [a[0] + t0 * d[0], a[1] + t0 * d[1]],
            [a[0] + t1 * d[0], a[1] + t1 * d[1]],
        )
    })
}

/// A line of radius `r` px: discs stamped every half pixel along it.
fn draw_line(
    data: &mut [PremulRgba8],
    w: usize,
    h: usize,
    a: [f64; 2],
    b: [f64; 2],
    r: f64,
    c: [u8; 3],
) {
    let pad = r + 1.0;
    let Some((a, b)) = clip(a, b, [-pad, -pad], [w as f64 + pad, h as f64 + pad]) else {
        return;
    };
    let len = (b[0] - a[0]).hypot(b[1] - a[1]);
    let steps = (len * 2.0).ceil().max(1.0) as usize;
    let ri = r.ceil() as i64;
    let (mut last_u, mut last_v) = (i64::MIN, i64::MIN);
    for s in 0..=steps {
        let t = s as f64 / steps as f64;
        let (x, y) = (a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1]));
        let (cu, cv) = (x.floor() as i64, y.floor() as i64);
        if (cu, cv) == (last_u, last_v) {
            continue;
        }
        (last_u, last_v) = (cu, cv);
        for dv in -ri..=ri {
            for du in -ri..=ri {
                let (u, v) = (cu + du, cv + dv);
                if u < 0 || v < 0 || u >= w as i64 || v >= h as i64 {
                    continue;
                }
                let (px, py) = (u as f64 + 0.5 - x, v as f64 + 0.5 - y);
                if px * px + py * py <= r * r + 0.25 {
                    blend(&mut data[v as usize * w + u as usize], c, 1.0);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_keeps_the_inside_part() {
        let (a, b) = clip([-10.0, 5.0], [20.0, 5.0], [0.0, 0.0], [10.0, 10.0]).unwrap();
        assert_eq!((a, b), ([0.0, 5.0], [10.0, 5.0]));
        assert!(clip([-10.0, -5.0], [-1.0, -1.0], [0.0, 0.0], [10.0, 10.0]).is_none());
    }

    #[test]
    fn only_the_highest_plane_is_drawn() {
        // Two squares overlapping on [4, 6] x [0, 10]: the left one rises
        // to the right and is higher there.
        let mut pm = Pixmap::new(10, 10);
        for p in pm.data_mut() {
            *p = PremulRgba8 {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            };
        }
        let square = |x0: f64, x1: f64| vec![[x0, 0.0], [x1, 0.0], [x1, 10.0], [x0, 10.0]];
        let o = Overlay {
            fills: vec![
                OverlayFill {
                    points: square(0.0, 6.0),
                    color: [255, 0, 0],
                    alpha: 1.0,
                    z_plane: Some([1.0, 0.0, 0.0]),
                },
                OverlayFill {
                    points: square(4.0, 10.0),
                    color: [0, 0, 255],
                    alpha: 1.0,
                    z_plane: Some([0.0, 0.0, 3.0]),
                },
            ],
            lines: vec![],
        };
        draw(&mut pm, [1.0, 0.0, 0.0, 0.0, 1.0, 0.0], &o, 1.0);
        let px = |u: usize| pm.data()[5 * 10 + u];
        assert_eq!((px(1).r, px(1).b), (255, 0), "left only: red");
        assert_eq!((px(4).r, px(4).b), (255, 0), "z 4.5 over 3: red");
        assert_eq!((px(2).r, px(2).b), (255, 0));
        assert_eq!((px(8).r, px(8).b), (0, 255), "right only: blue");
        // At u = 4 (z 4.5) red wins; the blue square is hidden only where
        // the red plane is higher, so u = 5 (z 5.5) is red as well.
        assert_eq!(px(5).r, 255);
    }

    #[test]
    fn sheet_to_pixel_inverts_the_map() {
        // A crop turned by 90°: x = v/4 + 10, y = -u/4 + 50.
        let m = [0.0, 0.25, 10.0, -0.25, 0.0, 50.0];
        let f = sheet_to_pixel(m).unwrap();
        let p = f([12.0, 40.0]);
        assert!((p[0] - 40.0).abs() < 1e-9 && (p[1] - 8.0).abs() < 1e-9);
    }
}
