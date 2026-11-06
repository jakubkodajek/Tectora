//! A labelled sheet-millimetre grid drawn over a rendered crop, so a model
//! can read the sheet coordinates of what it sees instead of estimating
//! pixel positions.

use hayro::vello_cpu::Pixmap;
use hayro::vello_cpu::color::PremulRgba8;

/// 3 × 5 bitmaps of the digits, rows top to bottom, bit 2 = left column.
const DIGITS: [[u8; 5]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111],
    [0b010, 0b110, 0b010, 0b010, 0b111],
    [0b111, 0b001, 0b111, 0b100, 0b111],
    [0b111, 0b001, 0b111, 0b001, 0b111],
    [0b101, 0b101, 0b111, 0b001, 0b001],
    [0b111, 0b100, 0b111, 0b001, 0b111],
    [0b111, 0b100, 0b111, 0b101, 0b111],
    [0b111, 0b001, 0b010, 0b010, 0b010],
    [0b111, 0b101, 0b111, 0b101, 0b111],
    [0b111, 0b101, 0b111, 0b001, 0b111],
];

/// Grid line colour (TERRAN orange, the AI colour) and label colours.
const LINE: [u8; 3] = [0xdb, 0x88, 0x39];
const LABEL: [u8; 3] = [0x9a, 0x4f, 0x0e];
const LINE_ALPHA: f64 = 0.45;

/// A step that gives about eight to fifteen lines over `span_mm`.
pub fn auto_step(span_mm: f64) -> f64 {
    for s in [1.0, 2.0, 5.0, 10.0, 20.0, 25.0, 50.0, 100.0, 200.0, 500.0] {
        if span_mm / s <= 15.0 {
            return s;
        }
    }
    1000.0
}

fn blend(p: &mut PremulRgba8, c: [u8; 3], alpha: f64) {
    let mix = |d: u8, s: u8| (d as f64 * (1.0 - alpha) + s as f64 * alpha).round() as u8;
    *p = PremulRgba8 {
        r: mix(p.r, c[0]),
        g: mix(p.g, c[1]),
        b: mix(p.b, c[2]),
        a: 255,
    };
}

/// Draws grid lines every `step` sheet mm over a pixmap whose pixel (u, v)
/// maps to the sheet by `m` (see `Rendered::pixel_to_sheet`), labelled with
/// the sheet coordinate along the top and left edges.
pub fn draw(pixmap: &mut Pixmap, m: [f64; 6], step: f64) {
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);
    if w == 0 || h == 0 || step.is_nan() || step <= 0.0 {
        return;
    }
    // Which sheet axis runs along image u (columns) and v (rows), and how.
    // Images are only ever turned by quarter turns, so each image axis maps
    // to exactly one sheet axis.
    let (u_axis, u_scale, u_off) = if m[0] != 0.0 {
        (0, m[0], m[2])
    } else {
        (1, m[3], m[5])
    };
    let (v_scale, v_off) = if u_axis == 0 {
        (m[4], m[5])
    } else {
        (m[1], m[2])
    };
    let scale_px = (w.min(h) as f64 / 450.0).clamp(1.0, 4.0).round() as usize;
    let data = pixmap.data_mut();

    let mut labels_u: Vec<(usize, f64)> = Vec::new();
    let mut labels_v: Vec<(usize, f64)> = Vec::new();
    // Columns where the u-axis sheet coordinate crosses a multiple of step.
    for u in 0..w {
        let c0 = u_scale * u as f64 + u_off;
        let c1 = u_scale * (u + 1) as f64 + u_off;
        let k = (c0.min(c1) / step).ceil();
        if k * step < c0.max(c1) {
            labels_u.push((u, k * step));
            for v in 0..h {
                blend(&mut data[v * w + u], LINE, LINE_ALPHA);
            }
        }
    }
    for v in 0..h {
        let c0 = v_scale * v as f64 + v_off;
        let c1 = v_scale * (v + 1) as f64 + v_off;
        let k = (c0.min(c1) / step).ceil();
        if k * step < c0.max(c1) {
            labels_v.push((v, k * step));
            for u in 0..w {
                blend(&mut data[v * w + u], LINE, LINE_ALPHA);
            }
        }
    }
    // Each label names its sheet axis ("x120", "y80"): on a turned image
    // the axis along the columns is the sheet's y.
    let (u_name, v_name) = if u_axis == 0 { ('x', 'y') } else { ('y', 'x') };
    let k = scale_px;
    // Keep the top row clear of the left column's labels and vice versa.
    let left_col = 6 * 4 * k + 3 * k;
    let top_row = 9 * k;
    for (u, val) in labels_u {
        if u >= left_col {
            text(
                data,
                w,
                h,
                u + 2 * k,
                2 * k,
                &format!("{u_name}{}", fmt(val)),
                k,
            );
        }
    }
    for (v, val) in labels_v {
        if v >= top_row {
            text(
                data,
                w,
                h,
                2 * k,
                v + 2 * k,
                &format!("{v_name}{}", fmt(val)),
                k,
            );
        }
    }
}

fn fmt(v: f64) -> String {
    let r = (v * 10.0).round() / 10.0;
    if r.fract() == 0.0 {
        format!("{}", r as i64)
    } else {
        format!("{r:.1}")
    }
}

/// Draws `s` (digits, '.', '-', 'x', 'y') with its top left corner at (x, y), on a
/// white box so it stays readable over the drawing.
fn text(data: &mut [PremulRgba8], w: usize, h: usize, x: usize, y: usize, s: &str, k: usize) {
    let adv = 4 * k;
    let bw = s.len() * adv + k;
    let bh = 7 * k;
    for yy in y.saturating_sub(k)..(y + bh).min(h) {
        for xx in x.saturating_sub(k)..(x + bw).min(w) {
            blend(&mut data[yy * w + xx], [255, 255, 255], 0.85);
        }
    }
    for (i, ch) in s.chars().enumerate() {
        let rows: [u8; 5] = match ch {
            '0'..='9' => DIGITS[ch as usize - '0' as usize],
            '-' => [0, 0, 0b111, 0, 0],
            '.' => [0, 0, 0, 0, 0b010],
            'x' => [0, 0b101, 0b010, 0b101, 0],
            'y' => [0b101, 0b101, 0b010, 0b010, 0b010],
            _ => continue,
        };
        let gx = x + i * adv;
        for (ry, bits) in rows.iter().enumerate() {
            for rx in 0..3 {
                if bits & (0b100 >> rx) == 0 {
                    continue;
                }
                for py in 0..k {
                    for px in 0..k {
                        let (xx, yy) = (gx + rx * k + px, y + ry * k + py);
                        if xx < w && yy < h {
                            blend(&mut data[yy * w + xx], LABEL, 1.0);
                        }
                    }
                }
            }
        }
    }
}
