//! Tools for reading technical drawings (PDF): what the inbox holds, what a
//! sheet is made of, sharp crops to read numbers from, the drawn lines, and
//! measurements that check a read dimension against the drawing and give the
//! plot scale. Reading is done by [`tectora_drawing`]; nothing here writes a
//! design.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tectora_core::settings::Settings;
use tectora_drawing::{Drawing, RectMm, Turn, round};

use super::{Server, ToolResult, arg_f64, schema};

/// Default longest edge of a rendered crop, px. Clients downscale larger
/// images anyway (Claude: 1568 px, or 2576 px on newer models).
const DEFAULT_MAX_PX: u32 = 1568;
/// Relative difference between a read dimension and the drawn length
/// (at the stated scale) that still counts as a match.
const DIMENSION_TOLERANCE: f64 = 0.01;
/// Default snap radius on the sheet, mm.
const DEFAULT_SNAP_MM: f64 = 1.5;
/// Files listed from the drawings inbox at most.
const MAX_INBOX_FILES: usize = 500;

/// Marks a tool result that carries its own MCP `content` (an image).
pub(crate) const CONTENT_KEY: &str = "$mcp_content";

fn source_props() -> Value {
    json!({
        "path": { "type": "string", "description": "Absolute path of the PDF (e.g. from list_drawings)." },
        "id": { "type": "string", "description": "Job id, with `file`: read an attached drawing instead of `path`." },
        "file": { "type": "string", "description": "Name of the job's attached file (see list_files)." },
        "page": { "type": "integer", "description": "Page index from 0 (default 0)." },
    })
}

fn region_prop() -> Value {
    json!({
        "type": "array", "items": { "type": "number" }, "minItems": 4, "maxItems": 4,
        "description": "[x, y, width, height] on the sheet, mm; origin top left, y down (as in the image). Default: whole page."
    })
}

fn with(mut base: Value, extra: Value) -> Value {
    if let (Some(b), Value::Object(e)) = (base.as_object_mut(), extra) {
        b.extend(e);
    }
    base
}

pub(crate) fn tools() -> Vec<Value> {
    vec![
        json!({
            "name": "list_drawings",
            "description": "PDF drawings in the user's drawings inbox (settings `drawings_dir`), newest first, with their paths.",
            "inputSchema": schema(json!({}), &[]),
        }),
        json!({
            "name": "drawing_info",
            "description": "Pages of a PDF drawing: sheet size in mm, paper size, rotation and kind. `vector_with_text`: text can be read with drawing_lines; `vector_outlined_text`: read numbers from view_drawing crops, measure with measure_drawing; `raster` (scan): only view_drawing, measurements are estimates.",
            "inputSchema": schema(source_props(), &[]),
        }),
        json!({
            "name": "view_drawing",
            "description": "Render a page or a region of it as an image. First look at the whole page, then request crops of the parts you need: a crop is sharp enough to read dimension text when it is about 300 dpi (whole sheets are not). An orange grid labelled with sheet mm is drawn over the image (turn it off with `grid: false` to read small text under it); read sheet coordinates for region, measure_drawing and trace_outline off that grid. Use `rotate` to straighten sheets plotted sideways (the grid labels stay sheet coordinates).",
            "inputSchema": schema(with(source_props(), json!({
                "region": region_prop(),
                "dpi": { "type": "number", "description": "Resolution; default: as sharp as fits `max_px`." },
                "max_px": { "type": "integer", "description": "Longest image edge, px (default 1568, at most 4096)." },
                "rotate": { "type": "integer", "enum": [0, 90, 180, 270], "description": "Turn the image clockwise, degrees." },
                "grid": { "description": "true (default): sheet-mm grid with a step that fits the crop; a number: grid step in mm; false: no grid.", "type": ["boolean", "number"] },
            })), &[]),
        }),
        json!({
            "name": "drawing_dimensions",
            "description": "Every dimension chain in a region of a vector PDF, found from the drawn dimension lines and their marks (extension lines or ticks): for each dimension its ends, its middle (where its text is) and its drawn length. With `scale` the real length in mm, which is exact when the sheet is plotted to scale: read the text at `mid`, and if it disagrees, the sheet is scaled (see `implied_scale` per dimension when you pass `read` values) or the dimension is not to scale. Prefer this to measuring dimensions one by one.",
            "inputSchema": schema(with(source_props(), json!({
                "region": region_prop(),
                "scale": { "type": "number", "description": "Scale denominator, e.g. 50 for 1:50." },
                "min_length_mm": { "type": "number", "description": "Skip dimensions shorter than this on the sheet (default 2 mm)." },
            })), &[]),
        }),
        json!({
            "name": "trace_outline",
            "description": "Snap a polygon to the drawing: give its corners roughly (sheet mm, read off the view_drawing grid), in order around the outline; each is snapped to the nearest drawn corner or line crossing. Returns the exact corners, the edge lengths, and with `scale` the polygon in metres ready for create_project (x right, y up, first corner at the origin, counter-clockwise), the real edge lengths and each edge's direction in degrees (the `angle_deg` for a dormer on that eave). Use it for the wall line or the eave line of a roof plan; check the edges against the dimensions. Corners that barely turn are reported: leave them out, the roof calculation fails on nearly straight neighbouring eaves. `edges_drawn` gives the share of each edge that lies on a drawn line; edges well under 1 mean a corner snapped to the wrong line.",
            "inputSchema": schema(with(source_props(), json!({
                "points": { "type": "array", "items": { "type": "array", "items": { "type": "number" }, "minItems": 2, "maxItems": 2 }, "minItems": 3, "maxItems": 500, "description": "Corners [x, y] in sheet mm, in order." },
                "snap_mm": { "type": "number", "description": "Snap radius, mm (default 1.5)." },
                "scale": { "type": "number", "description": "Scale denominator, e.g. 50 for 1:50." },
                "offset_m": { "type": "number", "description": "Move every edge outward by this many metres (negative: inward), e.g. -0.6 to get the wall line from an eave line with a 0.6 m overhang. Needs `scale`." },
            })), &["points"]),
        }),
        json!({
            "name": "drawing_lines",
            "description": "The straight lines drawn in a region of a vector PDF, in sheet mm, longest first (collinear pieces merged), and the text of the PDF's text layer in that region. Use it to find wall lines, dimension lines and their exact ends. For the labels only, pass `limit: 0` and `text_contains`, e.g. [\"+\", \"°\"] for every height and pitch label of a sheet.",
            "inputSchema": schema(with(source_props(), json!({
                "region": region_prop(),
                "min_length_mm": { "type": "number", "description": "Shortest line returned (default 2 mm)." },
                "limit": { "type": "integer", "description": "Most lines returned (default 200, at most 2000; 0 = texts only)." },
                "text_limit": { "type": "integer", "description": "Most texts returned (default 1000, at most 5000). `texts_total` says how many the region has." },
                "text_contains": { "type": "array", "items": { "type": "string" }, "description": "Keep only texts that contain one of these strings (at most 8), e.g. [\"+\", \"°\"]." },
                "directions_deg": { "type": "array", "items": { "type": "number" }, "description": "Keep only lines within 1° of these sheet directions, e.g. [0, 90] for the walls and roof edges of a plan drawn square to the sheet. Use it on hatched or tiled roof plans, where the pattern otherwise fills the limit." },
            })), &["region"]),
        }),
        json!({
            "name": "measure_drawing",
            "description": "Measure between two sheet points (mm), each snapped to the nearest line end or crossing. With `scale` (e.g. 50 for 1:50) gives the real length; with `expected_mm` (the dimension you read) checks it: `implied_scale` shows the plot scale, and `matches` says whether the read value agrees with the drawing at `scale`. Check every dimension you use this way; a mismatch means a misread digit, a dimension not drawn to scale, or wrong points.",
            "inputSchema": schema(with(source_props(), json!({
                "from": { "type": "array", "items": { "type": "number" }, "minItems": 2, "maxItems": 2, "description": "[x, y] sheet mm." },
                "to": { "type": "array", "items": { "type": "number" }, "minItems": 2, "maxItems": 2, "description": "[x, y] sheet mm." },
                "snap_mm": { "type": "number", "description": "Snap radius, mm (default 1.5; 0 = no snapping)." },
                "scale": { "type": "number", "description": "Scale denominator of the drawing, e.g. 50 for 1:50." },
                "expected_mm": { "type": "number", "description": "The dimension as written on the drawing, mm (real size)." },
            })), &["from", "to"]),
        }),
    ]
}

impl Server {
    pub(crate) fn call_drawing(&self, name: &str, args: &Value) -> Option<ToolResult> {
        Some(match name {
            "list_drawings" => list_drawings(),
            "drawing_info" => self.drawing_info(args),
            "view_drawing" => self.view_drawing(args),
            "drawing_lines" => self.drawing_lines(args),
            "measure_drawing" => self.measure_drawing(args),
            "drawing_dimensions" => self.drawing_dimensions(args),
            "trace_outline" => self.trace_outline(args),
            _ => return None,
        })
    }

    fn drawing_path(&self, args: &Value) -> Result<PathBuf, String> {
        match (
            args.get("path").and_then(Value::as_str),
            args.get("id").and_then(Value::as_str),
        ) {
            (Some(p), _) => {
                let p = PathBuf::from(p);
                if !p.is_absolute() {
                    return Err("`path` must be an absolute path".into());
                }
                Ok(p)
            }
            (None, Some(id)) => {
                if !self.store.exists(id) {
                    return Err(format!(
                        "no design with id `{id}`; call list_projects to see the ids"
                    ));
                }
                let file = args.get("file").and_then(Value::as_str).ok_or(
                    "with `id`, pass `file`: the name of the attached drawing (see list_files)",
                )?;
                self.store.attachment_path(id, file)
            }
            (None, None) => Err("pass `path`, or `id` and `file`".into()),
        }
    }

    fn open_drawing(&self, args: &Value) -> Result<(PathBuf, Drawing, usize), String> {
        let path = self.drawing_path(args)?;
        let meta = fs::metadata(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        if meta.len() > tectora_drawing::MAX_PDF_BYTES as u64 {
            return Err(format!(
                "{}: {}",
                path.display(),
                tectora_drawing::DrawingError::TooLarge
            ));
        }
        let bytes = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let d = Drawing::open(bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        let page = match args.get("page") {
            None | Some(Value::Null) => 0,
            Some(v) => v.as_u64().ok_or("`page` must be a whole number from 0")? as usize,
        };
        if page >= d.page_count() {
            return Err(tectora_drawing::DrawingError::NoPage(page, d.page_count()).to_string());
        }
        Ok((path, d, page))
    }

    fn drawing_info(&self, args: &Value) -> ToolResult {
        let (path, d, _) = self.open_drawing(args)?;
        let pages: Result<Vec<_>, _> = (0..d.page_count()).map(|i| d.page_info(i)).collect();
        Ok(json!({
            "path": path.display().to_string(),
            "pages": pages.map_err(|e| e.to_string())?,
        }))
    }

    fn view_drawing(&self, args: &Value) -> ToolResult {
        let (_, d, page) = self.open_drawing(args)?;
        let region = region_arg(args)?;
        let max_px = match args.get("max_px") {
            None | Some(Value::Null) => DEFAULT_MAX_PX,
            Some(v) => v
                .as_u64()
                .ok_or("`max_px` must be a whole number")?
                .min(u32::MAX as u64) as u32,
        };
        let turn = match args.get("rotate") {
            None | Some(Value::Null) => Turn::None,
            Some(v) => v
                .as_i64()
                .and_then(Turn::from_degrees)
                .ok_or("`rotate` must be 0, 90, 180 or 270")?,
        };
        let grid = match args.get("grid") {
            None | Some(Value::Null) | Some(Value::Bool(true)) => Some(0.0),
            Some(Value::Bool(false)) => None,
            Some(v) => Some(
                v.as_f64()
                    .filter(|g| g.is_finite() && *g > 0.0 && *g <= 10_000.0)
                    .ok_or("`grid` must be true, false or a step in mm")?,
            ),
        };
        let grid = match grid {
            // Default step: about ten lines over the crop's longer side.
            Some(0.0) => {
                let (w, h) = match region {
                    Some(r) => (r.width, r.height),
                    None => d.page_size_mm(page).map_err(|e| e.to_string())?,
                };
                Some(tectora_drawing::grid_auto_step(w.max(h)))
            }
            g => g,
        };
        let r = d
            .render(page, region, arg_f64(args, "dpi")?, max_px, turn, grid)
            .map_err(|e| e.to_string())?;
        let m = r.pixel_to_sheet.map(|v| round(v, 6));
        let info = json!({
            "page": page,
            "image_px": [r.width, r.height],
            "region_mm": [round(r.region.x, 2), round(r.region.y, 2), round(r.region.width, 2), round(r.region.height, 2)],
            "dpi": round(r.px_per_mm * 25.4, 1),
            "rotate": r.turn.degrees(),
            "pixel_to_sheet": m,
            "grid_mm": grid,
            "note": "sheet x = m0*u + m1*v + m2, y = m3*u + m4*v + m5 for image pixel (u, v)",
        });
        Ok(json!({
            CONTENT_KEY: [
                { "type": "image", "data": base64(&r.png), "mimeType": "image/png" },
                { "type": "text", "text": serde_json::to_string_pretty(&info).unwrap_or_default() },
            ]
        }))
    }

    fn drawing_lines(&self, args: &Value) -> ToolResult {
        let (_, d, page) = self.open_drawing(args)?;
        let region = region_arg(args)?.ok_or("missing argument `region`")?;
        let min_len = arg_f64(args, "min_length_mm")?.unwrap_or(2.0).max(0.0);
        let limit = args
            .get("limit")
            .and_then(Value::as_u64)
            .unwrap_or(200)
            .min(2000) as usize;
        let text_limit = args
            .get("text_limit")
            .and_then(Value::as_u64)
            .unwrap_or(1000)
            .min(5000) as usize;
        let contains: Vec<String> = match args.get("text_contains") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(a)) if a.len() <= 8 => a
                .iter()
                .map(|v| {
                    v.as_str()
                        .filter(|s| !s.is_empty() && s.len() <= 64)
                        .map(str::to_owned)
                })
                .collect::<Option<Vec<String>>>()
                .ok_or("`text_contains` must be non-empty strings of at most 64 bytes")?,
            Some(_) => return Err("`text_contains` must be an array of at most 8 strings".into()),
        };
        let directions: Vec<f64> = match args.get("directions_deg") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(a)) if a.len() <= 8 => a
                .iter()
                .map(|v| v.as_f64().filter(|x| x.is_finite()))
                .collect::<Option<Vec<f64>>>()
                .ok_or("`directions_deg` must be numbers")?,
            Some(_) => return Err("`directions_deg` must be an array of at most 8 numbers".into()),
        };
        let content = d.content(page).map_err(|e| e.to_string())?;
        let lines: Vec<Value> = if limit == 0 {
            Vec::new()
        } else {
            content.lines_along(Some(region), min_len, limit, &directions)
        }
        .iter()
        .map(|s| {
            json!({
                "a": [round(s.a[0], 2), round(s.a[1], 2)],
                "b": [round(s.b[0], 2), round(s.b[1], 2)],
                "length_mm": round(s.length(), 2),
                "angle_deg": round(s.angle_deg(), 2),
                "width_mm": round(s.width, 2),
                "dashed": s.dashed,
            })
        })
        .collect();
        let found = select_texts(content.texts_in(Some(region)), &contains);
        let total = found.len();
        let texts: Vec<Value> = found
            .into_iter()
            .take(text_limit)
            .map(|t| json!(t))
            .collect();
        let mut out = json!({ "page": page, "lines": lines, "texts": texts, "texts_total": total });
        if total > text_limit {
            out["texts_truncated"] = json!(true);
        }
        Ok(out)
    }

    fn measure_drawing(&self, args: &Value) -> ToolResult {
        let (_, d, page) = self.open_drawing(args)?;
        let from = point_arg(args, "from")?;
        let to = point_arg(args, "to")?;
        let snap = arg_f64(args, "snap_mm")?
            .unwrap_or(DEFAULT_SNAP_MM)
            .clamp(0.0, 20.0);
        let scale = scale_arg(args)?;
        let expected = arg_f64(args, "expected_mm")?;
        let content = d.content(page).map_err(|e| e.to_string())?;
        let m = content.measure(from, to, snap);
        let mut out = json!(m);
        out["page"] = json!(page);
        if let Some(s) = scale {
            out["real_m"] = json!(round(m.sheet_mm * s / 1000.0, 3));
        }
        if let Some(e) = expected {
            if m.sheet_mm > 0.0 {
                out["implied_scale"] = json!(round(e / m.sheet_mm, 2));
            }
            if let Some(s) = scale {
                let drawn = m.sheet_mm * s;
                let dev = if e != 0.0 {
                    (drawn - e) / e
                } else {
                    f64::INFINITY
                };
                out["deviation_pct"] = json!(round(dev * 100.0, 2));
                out["matches"] = json!(dev.abs() <= DIMENSION_TOLERANCE);
            }
        }
        if !m.snapped[0] || !m.snapped[1] {
            out["warning"] = json!(
                "an end was not snapped to a drawn line; the length is only as exact as the points given"
            );
        }
        Ok(out)
    }
}

impl Server {
    fn drawing_dimensions(&self, args: &Value) -> ToolResult {
        let (_, d, page) = self.open_drawing(args)?;
        let region = region_arg(args)?;
        let scale = scale_arg(args)?;
        let min_len = arg_f64(args, "min_length_mm")?.unwrap_or(2.0).max(0.0);
        let content = d.content(page).map_err(|e| e.to_string())?;
        let mut chains = content.dimensions(region);
        for c in &mut chains {
            c.dimensions.retain(|x| x.sheet_mm >= min_len);
        }
        chains.retain(|c| !c.dimensions.is_empty());
        let out: Vec<Value> = chains
            .iter()
            .map(|c| {
                let dims: Vec<Value> = c
                    .dimensions
                    .iter()
                    .map(|x| {
                        let mut v = json!({ "from": x.from, "to": x.to, "mid": x.mid, "sheet_mm": x.sheet_mm });
                        if let Some(s) = scale {
                            v["real_mm"] = json!(round(x.sheet_mm * s, 0));
                        }
                        v
                    })
                    .collect();
                let mut v = json!({
                    "angle_deg": c.angle_deg, "from": c.from, "to": c.to,
                    "sheet_mm": c.sheet_mm, "dimensions": dims,
                });
                if let Some(s) = scale {
                    v["real_mm"] = json!(round(c.sheet_mm * s, 0));
                }
                v
            })
            .collect();
        Ok(json!({
            "page": page,
            "chains": out,
            "truncated": chains.len() >= tectora_drawing::MAX_CHAINS,
            "note": "real_mm = sheet_mm x scale; it equals the written value only on a sheet plotted to scale. Compare with the text you read at `mid`; written / sheet_mm is the plot scale.",
        }))
    }

    fn trace_outline(&self, args: &Value) -> ToolResult {
        let (_, d, page) = self.open_drawing(args)?;
        let pts: Vec<[f64; 2]> = args
            .get("points")
            .and_then(Value::as_array)
            .filter(|a| (3..=500).contains(&a.len()))
            .ok_or("`points` must be 3 to 500 corners [x, y] in sheet mm")?
            .iter()
            .map(|p| {
                p.as_array()
                    .filter(|a| a.len() == 2)
                    .and_then(|a| Some([a[0].as_f64()?, a[1].as_f64()?]))
                    .filter(|p| p.iter().all(|x| x.is_finite() && x.abs() <= 100_000.0))
                    .ok_or_else(|| "each point must be [x, y] in sheet mm".to_string())
            })
            .collect::<Result<_, _>>()?;
        let snap = arg_f64(args, "snap_mm")?
            .unwrap_or(DEFAULT_SNAP_MM)
            .clamp(0.0, 20.0);
        let scale = scale_arg(args)?;
        let offset = arg_f64(args, "offset_m")?;
        if offset.is_some() && scale.is_none() {
            return Err("`offset_m` needs `scale`".into());
        }
        if offset.is_some_and(|o| o.abs() > 5.0) {
            return Err("`offset_m` must be within ±5 m".into());
        }
        let content = d.content(page).map_err(|e| e.to_string())?;
        let t = content.trace(&pts, snap);
        let mut out = json!(t);
        out["page"] = json!(page);
        if let Some(s) = scale {
            // Sheet y is down; the model's y is up. Mirroring flips the turning
            // direction, so the order is reversed when needed to keep the
            // polygon counter-clockwise.
            let mut m: Vec<[f64; 2]> = t
                .points
                .iter()
                .map(|p| [p[0] * s / 1000.0, -p[1] * s / 1000.0])
                .collect();
            if t.edges_sheet_mm.iter().any(|e| *e < 1e-6) {
                return Err(
                    "two neighbouring corners snapped to the same point; give the corners more exactly or use a smaller snap_mm".into(),
                );
            }
            if signed_area(&m) < 0.0 {
                m.reverse();
                m.rotate_right(1);
            }
            if let Some(o) = offset {
                m = offset_polygon(&m, o).ok_or("the outline is too small for this offset")?;
            }
            let (x0, y0) = (m[0][0], m[0][1]);
            let m: Vec<[f64; 2]> = m
                .iter()
                .map(|p| [round(p[0] - x0, 3), round(p[1] - y0, 3)])
                .collect();
            let n = m.len();
            let edges: Vec<f64> = (0..n)
                .map(|i| {
                    let (a, b) = (m[i], m[(i + 1) % n]);
                    round((b[0] - a[0]).hypot(b[1] - a[1]), 3)
                })
                .collect();
            // Model directions of the edges, degrees counter-clockwise from +x:
            // the `angle_deg` a dormer on that eave needs.
            let dirs: Vec<f64> = (0..n)
                .map(|i| {
                    let (a, b) = (m[i], m[(i + 1) % n]);
                    round(
                        (b[1] - a[1])
                            .atan2(b[0] - a[0])
                            .to_degrees()
                            .rem_euclid(360.0),
                        2,
                    )
                })
                .collect();
            out["outline_m"] = json!(m);
            out["edges_m"] = json!(edges);
            out["edge_directions_deg"] = json!(dirs);
            out["area_m2"] = json!(round(signed_area(&m), 3));
        }
        let mut warnings = Vec::new();
        if t.snapped.iter().any(|s| !s) {
            let missed: Vec<usize> = t
                .snapped
                .iter()
                .enumerate()
                .filter(|(_, s)| !**s)
                .map(|(i, _)| i)
                .collect();
            warnings.push(format!(
                "corners {missed:?} did not snap to a drawn corner; give them more exactly or check the view"
            ));
        }
        let loose: Vec<usize> = t
            .edges_drawn
            .iter()
            .enumerate()
            .filter(|(_, d)| **d < EDGE_DRAWN_MIN)
            .map(|(i, _)| i)
            .collect();
        if !loose.is_empty() {
            warnings.push(format!(
                "edges {loose:?} lie on a drawn line for less than half their length (`edges_drawn`): a corner of each snapped to a pattern or a parallel line (gutter, wall); read those corners from a sharp crop and trace again"
            ));
        }
        let shallow = shallow_corners(&t.points, SHALLOW_CORNER_DEG);
        if !shallow.is_empty() {
            warnings.push(format!(
                "corners {shallow:?} turn by less than {SHALLOW_CORNER_DEG}°: leave them out (straight edge) unless the walls really kink there; nearly straight neighbouring eaves make the roof calculation fail"
            ));
        }
        if !warnings.is_empty() {
            out["warning"] = json!(warnings.join("; "));
        }
        Ok(out)
    }
}

/// Traced edges that lie on drawn lines for less than this share are reported.
const EDGE_DRAWN_MIN: f64 = 0.5;

/// The texts that contain one of `contains` (all of them when it is empty),
/// in reading order: top to bottom, then left to right. A large sheet has
/// more labels than one answer can hold, and a filter such as `+` (heights)
/// or `°` (pitches) gets every label of one kind at once.
fn select_texts<'a>(
    mut texts: Vec<&'a tectora_drawing::TextItem>,
    contains: &[String],
) -> Vec<&'a tectora_drawing::TextItem> {
    if !contains.is_empty() {
        texts.retain(|t| contains.iter().any(|c| t.text.contains(c.as_str())));
    }
    texts.sort_by(|a, b| {
        (a.at[1] / 2.0)
            .floor()
            .total_cmp(&(b.at[1] / 2.0).floor())
            .then(a.at[0].total_cmp(&b.at[0]))
    });
    texts
}

/// Corners whose walls turn by less than this are reported: the core fails
/// on nearly collinear neighbouring eaves (job 4 of the drawings set, a 1.2°
/// kink, gave E00002).
const SHALLOW_CORNER_DEG: f64 = 3.0;

/// Indices of the corners of a closed polygon where the direction changes by
/// less than `limit_deg`.
fn shallow_corners(p: &[[f64; 2]], limit_deg: f64) -> Vec<usize> {
    let n = p.len();
    if n < 3 {
        return Vec::new();
    }
    (0..n)
        .filter(|&i| {
            let (a, b, c) = (p[(i + n - 1) % n], p[i], p[(i + 1) % n]);
            let (d1, d2) = ([b[0] - a[0], b[1] - a[1]], [c[0] - b[0], c[1] - b[1]]);
            let turn = (d1[0] * d2[1] - d1[1] * d2[0]).atan2(d1[0] * d2[0] + d1[1] * d2[1]);
            turn.abs().to_degrees() < limit_deg
        })
        .collect()
}

fn scale_arg(args: &Value) -> Result<Option<f64>, String> {
    let scale = arg_f64(args, "scale")?;
    if scale.is_some_and(|s| s.is_nan() || s <= 0.0 || s > 10_000.0) {
        return Err("`scale` must be the denominator, e.g. 50 for 1:50".into());
    }
    Ok(scale)
}

fn signed_area(p: &[[f64; 2]]) -> f64 {
    let n = p.len();
    (0..n)
        .map(|i| {
            let (a, b) = (p[i], p[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        / 2.0
}

/// Moves every edge of a counter-clockwise polygon outward by `d` (inward
/// when negative) and intersects neighbouring edges. `None` when a corner
/// would run away (an edge shorter than the offset needs).
fn offset_polygon(p: &[[f64; 2]], d: f64) -> Option<Vec<[f64; 2]>> {
    let n = p.len();
    // Outward normal of a CCW edge (a -> b) is (dy, -dx).
    let line = |i: usize| {
        let (a, b) = (p[i], p[(i + 1) % n]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let l = dx.hypot(dy);
        let (nx, ny) = (dy / l, -dx / l);
        ([a[0] + nx * d, a[1] + ny * d], [dx / l, dy / l])
    };
    let mut out = Vec::with_capacity(n);
    for (i, c) in p.iter().enumerate() {
        let (p1, d1) = line((i + n - 1) % n);
        let (p2, d2) = line(i);
        let den = d1[0] * d2[1] - d1[1] * d2[0];
        if den.abs() < 1e-9 {
            // Collinear neighbours: the corner just moves along the normal.
            out.push(p2);
            continue;
        }
        let t = ((p2[0] - p1[0]) * d2[1] - (p2[1] - p1[1]) * d2[0]) / den;
        let q = [p1[0] + t * d1[0], p1[1] + t * d1[1]];
        if !(q[0].is_finite() && q[1].is_finite())
            || (q[0] - c[0]).hypot(q[1] - c[1]) > d.abs() * 20.0 + 1e-9
        {
            return None;
        }
        out.push(q);
    }
    (signed_area(&out) > 0.0).then_some(out)
}

fn list_drawings() -> ToolResult {
    let dir = Settings::load().drawings_dir();
    let mut files = Vec::new();
    collect_pdfs(&dir, 0, &mut files);
    files.sort_by_key(|f| std::cmp::Reverse(f.1));
    files.truncate(MAX_INBOX_FILES);
    Ok(json!({
        "drawings_dir": dir.display().to_string(),
        "files": files.iter().map(|(p, modified, size)| json!({
            "path": p.display().to_string(),
            "folder": p.parent().and_then(|f| f.strip_prefix(&dir).ok()).map(|f| f.display().to_string()).unwrap_or_default(),
            "name": p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            "modified_ms": modified, "size": size,
        })).collect::<Vec<_>>(),
    }))
}

fn collect_pdfs(dir: &Path, depth: usize, out: &mut Vec<(PathBuf, u128, u64)>) {
    if depth > 3 || out.len() >= MAX_INBOX_FILES * 4 {
        return;
    }
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let Ok(t) = e.file_type() else { continue };
        let hidden = e.file_name().to_string_lossy().starts_with('.');
        if hidden {
            continue;
        }
        if t.is_dir() {
            collect_pdfs(&p, depth + 1, out);
        } else if t.is_file() && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("pdf")) {
            let meta = e.metadata().ok();
            let modified = meta
                .as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_millis());
            out.push((p, modified, meta.map_or(0, |m| m.len())));
        }
    }
}

fn region_arg(args: &Value) -> Result<Option<RectMm>, String> {
    let Some(v) = args.get("region").filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let a: Vec<f64> = v
        .as_array()
        .filter(|a| a.len() == 4)
        .map(|a| a.iter().filter_map(Value::as_f64).collect())
        .filter(|a: &Vec<f64>| a.len() == 4 && a.iter().all(|x| x.is_finite()))
        .ok_or("`region` must be [x, y, width, height] in mm")?;
    if a[2] <= 0.0 || a[3] <= 0.0 || a.iter().any(|x| x.abs() > 100_000.0) {
        return Err("`region` needs a positive width and height within the sheet".into());
    }
    Ok(Some(RectMm::new(a[0], a[1], a[2], a[3])))
}

fn point_arg(args: &Value, key: &str) -> Result<[f64; 2], String> {
    args.get(key)
        .and_then(Value::as_array)
        .filter(|a| a.len() == 2)
        .and_then(|a| Some([a[0].as_f64()?, a[1].as_f64()?]))
        .filter(|p| p.iter().all(|x| x.is_finite() && x.abs() <= 100_000.0))
        .ok_or_else(|| format!("`{key}` must be [x, y] in sheet mm"))
}

/// Standard base64 (RFC 4648) with padding.
fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn shallow_corners_finds_nearly_straight_corners() {
        // A rectangle with an extra corner on the bottom edge that turns by
        // about 1.1°: only that corner is reported.
        let p = [[0.0, 0.0], [5.0, 0.1], [10.0, 0.0], [10.0, 5.0], [0.0, 5.0]];
        assert_eq!(super::shallow_corners(&p, 3.0), vec![1]);
        assert!(super::shallow_corners(&p[..2], 3.0).is_empty());
    }

    #[test]
    fn select_texts_filters_and_sorts_labels() {
        let t = |text: &str, x: f64, y: f64| tectora_drawing::TextItem {
            text: text.into(),
            at: [x, y],
            size: 2.2,
            angle_deg: 0.0,
            end: [x, y],
        };
        let items = [
            t("30,0°", 50.0, 80.0),
            t("OP.21b", 10.0, 10.0),
            t("+14,565", 90.0, 10.5),
            t("+17,238", 20.0, 10.0),
        ];
        let all = super::select_texts(items.iter().collect(), &[]);
        let names: Vec<&str> = all.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(names, ["OP.21b", "+17,238", "+14,565", "30,0°"]);
        let some = super::select_texts(items.iter().collect(), &["+".into(), "°".into()]);
        let names: Vec<&str> = some.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(names, ["+17,238", "+14,565", "30,0°"]);
    }

    use super::base64;
    use crate::Server;
    use serde_json::{Value, json};
    use tectora_core::store::{Store, now_ms};

    /// A one-page A4 PDF with a 100 × 50 mm rectangle (1:1 lines).
    fn rect_pdf() -> Vec<u8> {
        let mm = 72.0 / 25.4;
        let content = format!(
            "1 w {} {} {} {} re S",
            20.0 * mm,
            217.0 * mm,
            100.0 * mm,
            50.0 * mm
        );
        let objs = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Contents 4 0 R /Resources << >> >>",
                210.0 * mm,
                297.0 * mm
            ),
            format!(
                "<< /Length {} >>\nstream\n{content}\nendstream",
                content.len()
            ),
        ];
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (i, o) in objs.iter().enumerate() {
            offsets.push(out.len());
            out.extend(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
        }
        let xref = out.len();
        out.extend(b"xref\n0 5\n0000000000 65535 f \n");
        for o in offsets {
            out.extend(format!("{o:010} 00000 n \n").as_bytes());
        }
        out.extend(
            format!("trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").as_bytes(),
        );
        out
    }

    fn call(s: &mut Server, name: &str, args: Value) -> Value {
        let r = s
            .handle(&json!({ "jsonrpc": "2.0", "id": 9, "method": "tools/call", "params": { "name": name, "arguments": args } }))
            .unwrap();
        r["result"].clone()
    }

    fn text(r: &Value) -> Value {
        serde_json::from_str(r["content"][0]["text"].as_str().unwrap()).unwrap()
    }

    #[test]
    fn reads_measures_and_renders_a_drawing() {
        let dir = std::env::temp_dir().join(format!(
            "tectora-mcp-drawing-{}-{}",
            now_ms(),
            std::process::id()
        ));
        let mut s = Server::new(Store::new(dir.clone()).unwrap());
        let pdf = dir.join("plan.pdf");
        std::fs::write(&pdf, rect_pdf()).unwrap();
        let path = pdf.display().to_string();

        let info = text(&call(&mut s, "drawing_info", json!({ "path": path })));
        assert_eq!(info["pages"][0]["paper"], "A4");

        // The 100 mm edge read as "5000" at 1:50 matches; "5300" does not.
        let m = text(&call(
            &mut s,
            "measure_drawing",
            json!({
                "path": path, "from": [20.5, 30.4], "to": [119.6, 29.8], "scale": 50, "expected_mm": 5000
            }),
        ));
        assert_eq!(m["matches"], true, "{m}");
        assert_eq!(m["implied_scale"], 50.0);
        assert_eq!(m["real_m"], 5.0);
        let bad = text(&call(
            &mut s,
            "measure_drawing",
            json!({
                "path": path, "from": [20.5, 30.4], "to": [119.6, 29.8], "scale": 50, "expected_mm": 5300
            }),
        ));
        assert_eq!(bad["matches"], false);

        let lines = text(&call(
            &mut s,
            "drawing_lines",
            json!({ "path": path, "region": [0, 0, 210, 297], "min_length_mm": 60 }),
        ));
        assert_eq!(lines["lines"].as_array().unwrap().len(), 2);
        let labels = text(&call(
            &mut s,
            "drawing_lines",
            json!({ "path": path, "region": [0, 0, 210, 297], "limit": 0, "text_contains": ["+"] }),
        ));
        assert_eq!(labels["lines"], json!([]));
        assert_eq!(labels["texts_total"], 0);
        let e = call(
            &mut s,
            "drawing_lines",
            json!({ "path": path, "region": [0, 0, 210, 297], "text_contains": "+" }),
        );
        assert_eq!(e["isError"], true);

        let view = call(
            &mut s,
            "view_drawing",
            json!({ "path": path, "region": [10, 20, 120, 70], "max_px": 600 }),
        );
        assert_eq!(view["content"][0]["type"], "image");
        assert_eq!(view["content"][0]["mimeType"], "image/png");
        assert!(
            view["content"][0]["data"]
                .as_str()
                .unwrap()
                .starts_with("iVBOR"),
            "PNG signature in base64"
        );
        let meta: Value =
            serde_json::from_str(view["content"][1]["text"].as_str().unwrap()).unwrap();
        assert_eq!(meta["image_px"], json!([600, 350]));

        // The rectangle traced from rough corners, in metres at 1:50, and
        // moved inward by 0.5 m (e.g. eave line -> wall line).
        let t = text(&call(
            &mut s,
            "trace_outline",
            json!({ "path": path, "points": [[20.6, 30.5], [119.5, 29.6], [120.4, 79.5], [19.8, 80.3]], "scale": 50 }),
        ));
        assert_eq!(t["snapped"], json!([true, true, true, true]), "{t}");
        assert_eq!(t["area_m2"], 12.5);
        assert_eq!(t["outline_m"][0], json!([0.0, 0.0]));
        let mut edges: Vec<f64> = t["edges_m"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect();
        edges.sort_by(f64::total_cmp);
        assert_eq!(edges, vec![2.5, 2.5, 5.0, 5.0]);
        let mut dirs: Vec<f64> = t["edge_directions_deg"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect();
        dirs.sort_by(f64::total_cmp);
        assert_eq!(dirs, vec![0.0, 90.0, 180.0, 270.0], "{t}");
        assert!(t.get("warning").is_none(), "{t}");
        let inner = text(&call(
            &mut s,
            "trace_outline",
            json!({ "path": path, "points": [[20.6, 30.5], [119.5, 29.6], [120.4, 79.5], [19.8, 80.3]], "scale": 50, "offset_m": -0.5 }),
        ));
        assert_eq!(inner["area_m2"], 6.0, "{inner}");
        let e = call(
            &mut s,
            "trace_outline",
            json!({ "path": path, "points": [[0, 0], [1, 1]] }),
        );
        assert_eq!(e["isError"], true);
        let e = call(
            &mut s,
            "trace_outline",
            json!({ "path": path, "points": [[20, 30], [120, 30], [120, 80]], "offset_m": 1 }),
        );
        assert_eq!(e["isError"], true, "offset needs a scale");

        // The rectangle has no dimension lines.
        let dims = text(&call(
            &mut s,
            "drawing_dimensions",
            json!({ "path": path, "scale": 50 }),
        ));
        assert_eq!(dims["chains"], json!([]));

        // Bad input is an error, not a crash.
        let e = call(&mut s, "view_drawing", json!({ "path": path, "page": 3 }));
        assert_eq!(e["isError"], true);
        let e = call(&mut s, "drawing_info", json!({ "path": "relative.pdf" }));
        assert_eq!(e["isError"], true);
        std::fs::write(dir.join("not.pdf"), b"hello").unwrap();
        let e = call(
            &mut s,
            "drawing_info",
            json!({ "path": dir.join("not.pdf").display().to_string() }),
        );
        assert_eq!(e["isError"], true);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn base64_matches_rfc4648_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}
