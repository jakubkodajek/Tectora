//! Reading technical drawings (PDF) for the AI drawing-to-roof flow.
//!
//! The drawings TERRAN receives are CAD plots: almost all are vector PDFs,
//! most with their text converted to outlines (so there is no text layer to
//! extract), a few are scans. An AI model reads numbers and meaning well
//! from a sharp crop but cannot measure from pixels, so this crate gives it
//! the two things it lacks:
//!
//! - [`Drawing::render`]: a crop of a page at a chosen resolution, so small
//!   dimension text becomes legible (models downscale whole sheets until the
//!   text is a few pixels high);
//! - [`PageContent`]: the page's vector lines in sheet millimetres, so a read
//!   dimension can be checked against the drawn length and the plot scale can
//!   be derived from the drawing itself ([`PageContent::measure`]).
//!
//! Sheet coordinates are millimetres on the page as it is displayed (after
//! the PDF's own `/Rotate`), origin at the top left corner, x to the right,
//! y down, the same orientation as the rendered image.

use std::fmt;

mod analyze;
mod grid;

pub use analyze::{Dimension, DimensionChain, MAX_CHAINS, Trace};
pub use grid::auto_step as grid_auto_step;

use hayro::hayro_interpret::font::GlyphRun;
use hayro::hayro_interpret::hayro_cmap::BfString;
use hayro::hayro_interpret::hayro_syntax::Pdf;
use hayro::hayro_interpret::util::TransformExt;
use hayro::hayro_interpret::{
    BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps,
    InterpreterSettings, SoftMask, interpret_page,
};
use hayro::kurbo::{Affine, BezPath, PathEl, Point, Rect, Shape};
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::vello_cpu::{Pixmap, RasterizerSettings, RenderContext, Resources, TargetInit};
use hayro::{RenderCache, RenderSettings, render_into};
use serde::Serialize;

/// Millimetres per PDF point.
pub const MM_PER_PT: f64 = 25.4 / 72.0;
/// Largest PDF accepted, bytes.
pub const MAX_PDF_BYTES: usize = 200 * 1024 * 1024;
/// Largest rendered image edge, pixels.
pub const MAX_RENDER_PX: u32 = 4096;
/// Smallest rendered image edge, pixels (models misread tiny images).
pub const MIN_RENDER_PX: u32 = 200;
/// Highest render resolution, dots per inch.
pub const MAX_DPI: f64 = 1200.0;
/// Segments kept per page; a dense A0 plot has about 200 000.
const MAX_SEGMENTS: usize = 2_000_000;
/// Filled paths smaller than this (mm, larger bbox side) are glyphs or
/// arrowheads, not drawn lines.
const MIN_FILL_EXTENT_MM: f64 = 8.0;

#[derive(Debug, thiserror::Error)]
pub enum DrawingError {
    #[error("not a readable PDF file")]
    NotPdf,
    #[error("the PDF file is larger than {} MB", MAX_PDF_BYTES / 1024 / 1024)]
    TooLarge,
    #[error("page {0} does not exist (the drawing has {1} pages)")]
    NoPage(usize, usize),
    #[error("the region lies outside the page")]
    EmptyRegion,
    #[error("cannot encode the image: {0}")]
    Encode(String),
}

/// A rectangle on the sheet, mm.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct RectMm {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl RectMm {
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    fn to_pt(self) -> Rect {
        Rect::new(
            self.x / MM_PER_PT,
            self.y / MM_PER_PT,
            (self.x + self.width) / MM_PER_PT,
            (self.y + self.height) / MM_PER_PT,
        )
    }

    fn intersect(self, o: RectMm) -> Option<RectMm> {
        let x0 = self.x.max(o.x);
        let y0 = self.y.max(o.y);
        let x1 = (self.x + self.width).min(o.x + o.width);
        let y1 = (self.y + self.height).min(o.y + o.height);
        (x1 > x0 && y1 > y0).then(|| RectMm::new(x0, y0, x1 - x0, y1 - y0))
    }

    pub fn contains(&self, p: [f64; 2]) -> bool {
        p[0] >= self.x
            && p[0] <= self.x + self.width
            && p[1] >= self.y
            && p[1] <= self.y + self.height
    }
}

/// Clockwise rotation of a rendered image, to straighten sheets plotted sideways.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub enum Turn {
    #[default]
    None,
    Cw90,
    Cw180,
    Cw270,
}

impl Turn {
    pub fn from_degrees(deg: i64) -> Option<Turn> {
        match deg.rem_euclid(360) {
            0 => Some(Turn::None),
            90 => Some(Turn::Cw90),
            180 => Some(Turn::Cw180),
            270 => Some(Turn::Cw270),
            _ => None,
        }
    }

    pub fn degrees(self) -> u32 {
        match self {
            Turn::None => 0,
            Turn::Cw90 => 90,
            Turn::Cw180 => 180,
            Turn::Cw270 => 270,
        }
    }
}

/// What kind of content a page holds; decides how it can be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PageKind {
    /// Vector lines and a text layer: text can be extracted directly.
    VectorWithText,
    /// Vector lines, text drawn as outlines: read text from renders, measure from vectors.
    VectorOutlinedText,
    /// A scan or a picture: read and estimate from renders only.
    Raster,
    /// Nothing recognisable.
    Empty,
}

#[derive(Clone, Debug, Serialize)]
pub struct PageInfo {
    pub index: usize,
    pub width_mm: f64,
    pub height_mm: f64,
    /// The PDF's own `/Rotate`, degrees (already applied to sheet coordinates).
    pub rotate: u32,
    pub kind: PageKind,
    pub segments: usize,
    pub text_items: usize,
    pub images: usize,
    /// ISO paper size name when the page matches one (either orientation).
    pub paper: Option<String>,
}

/// Direction tolerance of [`PageContent::lines_along`], degrees.
pub const DIRECTION_TOL_DEG: f64 = 1.0;

/// A straight line on the sheet, mm.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Segment {
    pub a: [f64; 2],
    pub b: [f64; 2],
    /// Stroke width, mm (0 for edges of filled shapes).
    pub width: f64,
    pub dashed: bool,
}

impl Segment {
    pub fn length(&self) -> f64 {
        dist(self.a, self.b)
    }

    /// Direction angle in degrees, 0..180 (sheet orientation, y down).
    pub fn angle_deg(&self) -> f64 {
        let a = (self.b[1] - self.a[1])
            .atan2(self.b[0] - self.a[0])
            .to_degrees();
        a.rem_euclid(180.0)
    }
}

/// A piece of real text of the PDF's text layer.
#[derive(Clone, Debug, Serialize)]
pub struct TextItem {
    pub text: String,
    /// Position of the first glyph's origin, mm.
    pub at: [f64; 2],
    /// Approximate glyph height, mm.
    pub size: f64,
    /// Baseline direction, degrees (sheet orientation, y down).
    pub angle_deg: f64,
}

/// The vector content of one page.
#[derive(Clone, Debug, Default)]
pub struct PageContent {
    pub segments: Vec<Segment>,
    pub texts: Vec<TextItem>,
    pub images: usize,
}

/// A rendered crop of a page.
#[derive(Clone, Debug)]
pub struct Rendered {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// The sheet area shown, mm.
    pub region: RectMm,
    pub px_per_mm: f64,
    pub turn: Turn,
    /// Maps an image pixel (u, v) to sheet mm: `x = m[0]*u + m[1]*v + m[2]`,
    /// `y = m[3]*u + m[4]*v + m[5]`.
    pub pixel_to_sheet: [f64; 6],
}

impl Rendered {
    pub fn sheet_point(&self, u: f64, v: f64) -> [f64; 2] {
        let m = self.pixel_to_sheet;
        [m[0] * u + m[1] * v + m[2], m[3] * u + m[4] * v + m[5]]
    }
}

/// An opened PDF drawing.
pub struct Drawing {
    pdf: Pdf,
}

impl fmt::Debug for Drawing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Drawing")
            .field("pages", &self.page_count())
            .finish()
    }
}

impl Drawing {
    pub fn open(bytes: Vec<u8>) -> Result<Self, DrawingError> {
        if bytes.len() > MAX_PDF_BYTES {
            return Err(DrawingError::TooLarge);
        }
        if !bytes.starts_with(b"%PDF") && !bytes.windows(4).take(1024).any(|w| w == b"%PDF") {
            return Err(DrawingError::NotPdf);
        }
        let pdf = Pdf::new(bytes).map_err(|_| DrawingError::NotPdf)?;
        Ok(Self { pdf })
    }

    pub fn page_count(&self) -> usize {
        self.pdf.pages().len()
    }

    fn page(
        &self,
        index: usize,
    ) -> Result<&hayro::hayro_interpret::hayro_syntax::page::Page<'_>, DrawingError> {
        self.pdf
            .pages()
            .get(index)
            .ok_or(DrawingError::NoPage(index, self.page_count()))
    }

    /// Page size as displayed, mm.
    pub fn page_size_mm(&self, index: usize) -> Result<(f64, f64), DrawingError> {
        let (w, h) = self.page(index)?.render_dimensions();
        Ok((w as f64 * MM_PER_PT, h as f64 * MM_PER_PT))
    }

    /// Size, kind and content counts of a page.
    pub fn page_info(&self, index: usize) -> Result<PageInfo, DrawingError> {
        let (width_mm, height_mm) = self.page_size_mm(index)?;
        let content = self.content(index)?;
        let page = self.page(index)?;
        let rotate = match page.rotation() {
            hayro::hayro_interpret::hayro_syntax::page::Rotation::None => 0,
            hayro::hayro_interpret::hayro_syntax::page::Rotation::Horizontal => 90,
            hayro::hayro_interpret::hayro_syntax::page::Rotation::Flipped => 180,
            hayro::hayro_interpret::hayro_syntax::page::Rotation::FlippedHorizontal => 270,
        };
        let text_chars: usize = content.texts.iter().map(|t| t.text.chars().count()).sum();
        let kind = if content.segments.len() >= 50 && content.texts.len() >= 30 && text_chars >= 100
        {
            PageKind::VectorWithText
        } else if content.segments.len() >= 50 {
            PageKind::VectorOutlinedText
        } else if content.images > 0 {
            PageKind::Raster
        } else {
            PageKind::Empty
        };
        Ok(PageInfo {
            index,
            width_mm: round(width_mm, 1),
            height_mm: round(height_mm, 1),
            rotate,
            kind,
            segments: content.segments.len(),
            text_items: content.texts.len(),
            images: content.images,
            paper: paper_name(width_mm, height_mm),
        })
    }

    /// The vector lines and text of a page in sheet mm.
    pub fn content(&self, index: usize) -> Result<PageContent, DrawingError> {
        let page = self.page(index)?;
        let (w, h) = page.render_dimensions();
        let to_mm = Affine::scale(MM_PER_PT) * page.initial_transform(true).to_kurbo();
        let cache = hayro::hayro_interpret::InterpreterCache::new();
        let mut ctx = Context::new(
            to_mm,
            Rect::new(0.0, 0.0, w as f64 * MM_PER_PT, h as f64 * MM_PER_PT),
            &cache,
            page.xref(),
            InterpreterSettings::default(),
        );
        let mut collector = Collector::default();
        interpret_page(page, &mut ctx, &mut collector);
        Ok(collector.content)
    }

    /// Renders `region` (default: the whole page) of a page to PNG.
    ///
    /// The resolution is `dpi` when given, otherwise the largest that keeps
    /// the longer image edge at `max_px`; it is always clamped so the image
    /// stays between [`MIN_RENDER_PX`] and [`MAX_RENDER_PX`]. With `grid_mm`
    /// a labelled grid of sheet coordinates is drawn over the image every
    /// that many mm.
    pub fn render(
        &self,
        index: usize,
        region: Option<RectMm>,
        dpi: Option<f64>,
        max_px: u32,
        turn: Turn,
        grid_mm: Option<f64>,
    ) -> Result<Rendered, DrawingError> {
        let page = self.page(index)?;
        let (wmm, hmm) = self.page_size_mm(index)?;
        let full = RectMm::new(0.0, 0.0, wmm, hmm);
        let region = match region {
            Some(r) => r.intersect(full).ok_or(DrawingError::EmptyRegion)?,
            None => full,
        };
        let max_px = max_px.clamp(MIN_RENDER_PX, MAX_RENDER_PX) as f64;
        let long_mm = region.width.max(region.height);
        let fit = max_px / long_mm;
        let mut px_per_mm = match dpi {
            Some(d) if d.is_finite() && d > 0.0 => (d.min(MAX_DPI) / 25.4).min(fit),
            _ => fit,
        };
        let short_mm = region.width.min(region.height);
        if short_mm * px_per_mm < MIN_RENDER_PX as f64 {
            px_per_mm =
                (MIN_RENDER_PX as f64 / short_mm).min(fit.max(MAX_RENDER_PX as f64 / long_mm));
        }
        let wpx = (region.width * px_per_mm).round().max(1.0) as u16;
        let hpx = (region.height * px_per_mm).round().max(1.0) as u16;

        let px_per_pt = px_per_mm * MM_PER_PT;
        let origin = region.to_pt();
        let transform = Affine::scale(px_per_pt)
            * Affine::translate((-origin.x0, -origin.y0))
            * page.initial_transform(true).to_kurbo();
        let mut rc = RenderContext::new(wpx, hpx);
        let cache = RenderCache::new();
        render_into(
            page,
            &cache,
            &InterpreterSettings::default(),
            &RenderSettings::default(),
            &mut rc,
            transform,
        );
        rc.flush();
        let mut pixmap = Pixmap::new(wpx, hpx);
        rc.render_with(
            &mut pixmap,
            &mut Resources::default(),
            RasterizerSettings {
                target_init: TargetInit::Clear(WHITE),
                ..Default::default()
            },
        );
        let mut pixmap = turn_pixmap(pixmap, turn);
        let (width, height) = (pixmap.width() as u32, pixmap.height() as u32);
        let s = 1.0 / px_per_mm;
        let (x0, y0) = (region.x, region.y);
        let (rw, rh) = (region.width, region.height);
        // Pixel (u, v) of the turned image back to the unturned crop, then to the sheet.
        let pixel_to_sheet = match turn {
            Turn::None => [s, 0.0, x0, 0.0, s, y0],
            Turn::Cw90 => [0.0, s, x0, -s, 0.0, y0 + rh],
            Turn::Cw180 => [-s, 0.0, x0 + rw, 0.0, -s, y0 + rh],
            Turn::Cw270 => [0.0, -s, x0 + rw, s, 0.0, y0],
        };
        if let Some(step) = grid_mm.filter(|g| g.is_finite() && *g > 0.0) {
            // At least 8 px between lines, or the labels run into each other.
            let step = step.max(8.0 / px_per_mm);
            grid::draw(&mut pixmap, pixel_to_sheet, step);
        }
        let png = pixmap
            .into_png()
            .map_err(|e| DrawingError::Encode(e.to_string()))?;
        Ok(Rendered {
            png,
            width,
            height,
            region,
            px_per_mm,
            turn,
            pixel_to_sheet,
        })
    }
}

/// Rotates a pixmap clockwise by `turn`.
fn turn_pixmap(src: Pixmap, turn: Turn) -> Pixmap {
    if turn == Turn::None {
        return src;
    }
    let (w, h) = (src.width() as usize, src.height() as usize);
    let (nw, nh) = match turn {
        Turn::Cw180 => (w, h),
        _ => (h, w),
    };
    let mut out = Pixmap::new(nw as u16, nh as u16);
    let s = src.data();
    let d = out.data_mut();
    for y in 0..h {
        for x in 0..w {
            let (nx, ny) = match turn {
                Turn::Cw90 => (h - 1 - y, x),
                Turn::Cw180 => (w - 1 - x, h - 1 - y),
                Turn::Cw270 => (y, w - 1 - x),
                Turn::None => (x, y),
            };
            d[ny * nw + nx] = s[y * w + x];
        }
    }
    out
}

/// Collects lines, text and image counts while a page is interpreted.
#[derive(Default)]
struct Collector {
    content: PageContent,
}

impl Collector {
    fn push_path(&mut self, path: &BezPath, transform: Affine, width: f64, dashed: bool) {
        let mut start = Point::ZERO;
        let mut last = Point::ZERO;
        for el in path.elements() {
            if self.content.segments.len() >= MAX_SEGMENTS {
                return;
            }
            match *el {
                PathEl::MoveTo(p) => {
                    start = transform * p;
                    last = start;
                }
                PathEl::LineTo(p) => {
                    let p = transform * p;
                    self.push(last, p, width, dashed);
                    last = p;
                }
                PathEl::QuadTo(_, p) | PathEl::CurveTo(_, _, p) => {
                    // Curves (arcs, circles) are not dimensioned lines; keep the
                    // pen position only.
                    last = transform * p;
                }
                PathEl::ClosePath => {
                    self.push(last, start, width, dashed);
                    last = start;
                }
            }
        }
    }

    fn push(&mut self, a: Point, b: Point, width: f64, dashed: bool) {
        if !(a.x.is_finite() && a.y.is_finite() && b.x.is_finite() && b.y.is_finite()) {
            return;
        }
        if (a - b).hypot() < 1e-6 {
            return;
        }
        self.content.segments.push(Segment {
            a: [a.x, a.y],
            b: [b.x, b.y],
            width,
            dashed,
        });
    }
}

impl<'a> Device<'a> for Collector {
    fn draw_path(&mut self, path: &BezPath, props: DrawProps<'a>, draw_mode: &DrawMode) {
        let t = props.transform;
        match draw_mode {
            DrawMode::Stroke(sp) | DrawMode::FillAndStroke(_, sp) => {
                let width = sp.line_width as f64 * t.as_coeffs()[0].hypot(t.as_coeffs()[1]);
                self.push_path(path, t, width, !sp.dash_array.is_empty());
            }
            DrawMode::Fill(_) => {
                let bb = t.transform_rect_bbox(path.bounding_box());
                if bb.width().max(bb.height()) >= MIN_FILL_EXTENT_MM {
                    self.push_path(path, t, 0.0, false);
                }
            }
            DrawMode::Invisible => {}
        }
    }

    fn push_clip_path(&mut self, _clip_path: &ClipPath) {}

    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}

    fn draw_glyph_run(&mut self, run: &GlyphRun<'_, 'a>, props: DrawProps<'a>, _: &DrawMode) {
        let mut text = String::new();
        let mut first: Option<Affine> = None;
        for g in run.glyphs() {
            match g.as_unicode() {
                Some(BfString::Char(c)) => text.push(c),
                Some(BfString::String(u)) => text.push_str(&u),
                None => {}
            }
            if first.is_none() {
                first = Some(props.transform * g.transform());
            }
        }
        let text = text.trim().to_string();
        let Some(t) = first else { return };
        if text.is_empty() {
            return;
        }
        let at = t * Point::ZERO;
        let x_dir = t * Point::new(1.0, 0.0) - at;
        let y_dir = t * Point::new(0.0, 1.0) - at;
        // Glyph space is 1/1000 em in most fonts; the cap height is about 0.7 em.
        let size = y_dir.hypot() * 700.0;
        self.content.texts.push(TextItem {
            text,
            at: [round(at.x, 2), round(at.y, 2)],
            size: round(size, 2),
            angle_deg: round(x_dir.y.atan2(x_dir.x).to_degrees(), 1),
        });
    }

    fn draw_image(&mut self, _image: Image<'a, '_>, _props: ImageDrawProps<'a>) {
        self.content.images += 1;
    }

    fn pop_clip(&mut self) {}

    fn pop_transparency_group(&mut self) {}
}

/// A measured distance between two snapped points.
#[derive(Clone, Debug, Serialize)]
pub struct Measurement {
    pub from: [f64; 2],
    pub to: [f64; 2],
    /// Whether each end was snapped to a line end or crossing.
    pub snapped: [bool; 2],
    /// Distance on the sheet, mm.
    pub sheet_mm: f64,
    /// Horizontal and vertical components on the sheet, mm.
    pub dx_mm: f64,
    pub dy_mm: f64,
}

impl PageContent {
    /// Line segments inside `region`, collinear pieces merged, longest first.
    pub fn lines(&self, region: Option<RectMm>, min_len_mm: f64, limit: usize) -> Vec<Segment> {
        self.lines_along(region, min_len_mm, limit, &[])
    }

    /// Like [`PageContent::lines`], keeping only lines within
    /// [`DIRECTION_TOL_DEG`] of one of `directions_deg` (sheet degrees,
    /// any sign; empty keeps all). Hatching and tile patterns in other
    /// directions then no longer crowd the outline out of the limit.
    pub fn lines_along(
        &self,
        region: Option<RectMm>,
        min_len_mm: f64,
        limit: usize,
        directions_deg: &[f64],
    ) -> Vec<Segment> {
        let along = |s: &Segment| {
            directions_deg.is_empty()
                || directions_deg.iter().any(|&d| {
                    let off = (s.angle_deg() - d).rem_euclid(180.0);
                    off.min(180.0 - off) <= DIRECTION_TOL_DEG
                })
        };
        let inside: Vec<Segment> = self
            .segments
            .iter()
            .filter(|s| region.is_none_or(|r| r.contains(s.a) || r.contains(s.b)))
            .filter(|s| along(s))
            .copied()
            .collect();
        let mut merged = merge_collinear(inside);
        merged.retain(|s| s.length() >= min_len_mm);
        merged.sort_by(|a, b| b.length().total_cmp(&a.length()));
        merged.truncate(limit);
        merged
    }

    /// Text items inside `region`.
    pub fn texts_in(&self, region: Option<RectMm>) -> Vec<&TextItem> {
        self.texts
            .iter()
            .filter(|t| region.is_none_or(|r| r.contains(t.at)))
            .collect()
    }

    /// The nearest line end or line crossing to `p` within `radius` mm.
    pub fn snap(&self, p: [f64; 2], radius: f64) -> Option<[f64; 2]> {
        let mut best: Option<([f64; 2], f64)> = None;
        let mut consider = |q: [f64; 2]| {
            let d = dist(p, q);
            if d <= radius && best.is_none_or(|(_, bd)| d < bd) {
                best = Some((q, d));
            }
        };
        let near: Vec<&Segment> = self
            .segments
            .iter()
            .filter(|s| seg_point_dist(s, p) <= radius)
            .take(5000)
            .collect();
        for s in &near {
            consider(s.a);
            consider(s.b);
        }
        for (i, s) in near.iter().enumerate() {
            for t in &near[i + 1..] {
                if let Some(x) = intersect(s, t) {
                    consider(x);
                }
            }
        }
        best.map(|(q, _)| q)
    }

    /// Snaps the corner `p` of a traced polygon whose edges arrive from
    /// `before` and leave towards `after`: the crossing of a drawn line along
    /// the incoming edge with a drawn line along the outgoing edge, nearest
    /// to `p` within `radius`. Lines in other directions (hatching, tile
    /// patterns, dimension ticks) are ignored, and the lines are extended to
    /// meet, so a corner drawn with a small gap or overlap still snaps.
    pub fn snap_corner(
        &self,
        p: [f64; 2],
        before: [f64; 2],
        after: [f64; 2],
        radius: f64,
    ) -> Option<[f64; 2]> {
        const DIR_TOL_DEG: f64 = 6.0;
        const MIN_LEN_MM: f64 = 1.0;
        const MIN_TURN_DEG: f64 = 15.0;
        let dir = |a: [f64; 2], b: [f64; 2]| {
            (b[1] - a[1])
                .atan2(b[0] - a[0])
                .to_degrees()
                .rem_euclid(180.0)
        };
        let off = |a: f64, b: f64| {
            let d = (a - b).rem_euclid(180.0);
            d.min(180.0 - d)
        };
        let (d_in, d_out) = (dir(before, p), dir(p, after));
        if off(d_in, d_out) < MIN_TURN_DEG {
            return None;
        }
        let near: Vec<&Segment> = self
            .segments
            .iter()
            .filter(|s| s.length() >= MIN_LEN_MM && seg_point_dist(s, p) <= radius)
            .take(5000)
            .collect();
        let along_in: Vec<&&Segment> = near
            .iter()
            .filter(|s| off(s.angle_deg(), d_in) <= DIR_TOL_DEG)
            .collect();
        let along_out: Vec<&&Segment> = near
            .iter()
            .filter(|s| off(s.angle_deg(), d_out) <= DIR_TOL_DEG)
            .collect();
        let mut best: Option<([f64; 2], f64)> = None;
        for s in &along_in {
            for t in &along_out {
                if let Some(x) = line_intersection(s, t) {
                    let d = dist(p, x);
                    if d <= radius && best.is_none_or(|(_, bd)| d < bd) {
                        best = Some((x, d));
                    }
                }
            }
        }
        best.map(|(q, _)| q)
    }

    /// Distance between `from` and `to`, each snapped to the drawing within `snap_mm`.
    pub fn measure(&self, from: [f64; 2], to: [f64; 2], snap_mm: f64) -> Measurement {
        let a = self.snap(from, snap_mm);
        let b = self.snap(to, snap_mm);
        let (pa, pb) = (a.unwrap_or(from), b.unwrap_or(to));
        Measurement {
            from: [round(pa[0], 3), round(pa[1], 3)],
            to: [round(pb[0], 3), round(pb[1], 3)],
            snapped: [a.is_some(), b.is_some()],
            sheet_mm: round(dist(pa, pb), 3),
            dx_mm: round((pb[0] - pa[0]).abs(), 3),
            dy_mm: round((pb[1] - pa[1]).abs(), 3),
        }
    }
}

/// Merges overlapping or touching collinear segments.
pub fn merge_collinear(mut segs: Vec<Segment>) -> Vec<Segment> {
    const ANGLE_TOL: f64 = 0.15; // degrees
    const OFFSET_TOL: f64 = 0.05; // mm
    const GAP_TOL: f64 = 0.05; // mm
    // Bucket by angle and by perpendicular offset, then merge along the line.
    let key = |s: &Segment| {
        let ang = s.angle_deg();
        let r = ang.to_radians();
        let (nx, ny) = (-r.sin(), r.cos());
        let off = s.a[0] * nx + s.a[1] * ny;
        (ang, off)
    };
    segs.sort_by(|a, b| {
        let (aa, ao) = key(a);
        let (ba, bo) = key(b);
        aa.total_cmp(&ba).then(ao.total_cmp(&bo))
    });
    let mut out: Vec<Segment> = Vec::with_capacity(segs.len());
    let mut i = 0;
    while i < segs.len() {
        let (ang, off) = key(&segs[i]);
        let mut j = i + 1;
        while j < segs.len() {
            let (a2, o2) = key(&segs[j]);
            if (a2 - ang).abs() > ANGLE_TOL || (o2 - off).abs() > OFFSET_TOL {
                break;
            }
            j += 1;
        }
        // segs[i..j] lie on one line; project and merge intervals.
        let r = ang.to_radians();
        let (dx, dy) = (r.cos(), r.sin());
        let mut iv: Vec<(f64, f64, &Segment)> = segs[i..j]
            .iter()
            .map(|s| {
                let ta = s.a[0] * dx + s.a[1] * dy;
                let tb = s.b[0] * dx + s.b[1] * dy;
                (ta.min(tb), ta.max(tb), s)
            })
            .collect();
        iv.sort_by(|a, b| a.0.total_cmp(&b.0));
        let base = segs[i].a;
        let t0 = base[0] * dx + base[1] * dy;
        let point_at = |t: f64| [base[0] + (t - t0) * dx, base[1] + (t - t0) * dy];
        let mut cur = (iv[0].0, iv[0].1, iv[0].2.width, iv[0].2.dashed);
        for &(s, e, seg) in &iv[1..] {
            if s <= cur.1 + GAP_TOL && seg.dashed == cur.3 {
                cur.1 = cur.1.max(e);
                cur.2 = cur.2.max(seg.width);
            } else {
                out.push(Segment {
                    a: point_at(cur.0),
                    b: point_at(cur.1),
                    width: cur.2,
                    dashed: cur.3,
                });
                cur = (s, e, seg.width, seg.dashed);
            }
        }
        out.push(Segment {
            a: point_at(cur.0),
            b: point_at(cur.1),
            width: cur.2,
            dashed: cur.3,
        });
        i = j;
    }
    out
}

pub(crate) fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn seg_point_dist(s: &Segment, p: [f64; 2]) -> f64 {
    let (vx, vy) = (s.b[0] - s.a[0], s.b[1] - s.a[1]);
    let len2 = vx * vx + vy * vy;
    if len2 == 0.0 {
        return dist(s.a, p);
    }
    let t = (((p[0] - s.a[0]) * vx + (p[1] - s.a[1]) * vy) / len2).clamp(0.0, 1.0);
    dist([s.a[0] + t * vx, s.a[1] + t * vy], p)
}

/// Crossing of the infinite lines through two segments.
fn line_intersection(s: &Segment, t: &Segment) -> Option<[f64; 2]> {
    let (x1, y1, x2, y2) = (s.a[0], s.a[1], s.b[0], s.b[1]);
    let (x3, y3, x4, y4) = (t.a[0], t.a[1], t.b[0], t.b[1]);
    let den = (x1 - x2) * (y3 - y4) - (y1 - y2) * (x3 - x4);
    if den.abs() < 1e-9 {
        return None;
    }
    let u = ((x1 - x3) * (y3 - y4) - (y1 - y3) * (x3 - x4)) / den;
    Some([x1 + u * (x2 - x1), y1 + u * (y2 - y1)])
}

fn intersect(s: &Segment, t: &Segment) -> Option<[f64; 2]> {
    let (x1, y1, x2, y2) = (s.a[0], s.a[1], s.b[0], s.b[1]);
    let (x3, y3, x4, y4) = (t.a[0], t.a[1], t.b[0], t.b[1]);
    let den = (x1 - x2) * (y3 - y4) - (y1 - y2) * (x3 - x4);
    if den.abs() < 1e-9 {
        return None;
    }
    let u = ((x1 - x3) * (y3 - y4) - (y1 - y3) * (x3 - x4)) / den;
    let v = -((x1 - x2) * (y1 - y3) - (y1 - y2) * (x1 - x3)) / den;
    ((-1e-6..=1.0 + 1e-6).contains(&u) && (-1e-6..=1.0 + 1e-6).contains(&v))
        .then_some([x1 + u * (x2 - x1), y1 + u * (y2 - y1)])
}

/// Rounds to `d` decimals.
pub fn round(v: f64, d: i32) -> f64 {
    let f = 10f64.powi(d);
    (v * f).round() / f
}

/// ISO 216 A-series name of a page size, within 2 % (either orientation).
pub fn paper_name(w: f64, h: f64) -> Option<String> {
    let (s, l) = (w.min(h), w.max(h));
    const A: [(f64, f64); 7] = [
        (841.0, 1189.0),
        (594.0, 841.0),
        (420.0, 594.0),
        (297.0, 420.0),
        (210.0, 297.0),
        (148.0, 210.0),
        (105.0, 148.0),
    ];
    A.iter().enumerate().find_map(|(n, &(ps, pl))| {
        ((s / ps - 1.0).abs() < 0.02 && (l / pl - 1.0).abs() < 0.02).then(|| format!("A{n}"))
    })
}

/// Parses a scale like `1:50`, `M 1:100` or `50`; returns the denominator.
pub fn parse_scale(s: &str) -> Option<f64> {
    let t = s.trim().trim_start_matches(['M', 'm']).trim();
    let den = match t.split_once(':') {
        Some((one, den)) if one.trim() == "1" => den.trim(),
        Some(_) => return None,
        None => t,
    };
    let v: f64 = den.replace(',', ".").parse().ok()?;
    (v.is_finite() && v > 0.0 && v <= 10_000.0).then_some(v)
}

#[cfg(test)]
mod tests;
