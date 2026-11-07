//! Model Context Protocol server for Tectora (stdio transport).
//!
//! AI clients (Claude, ChatGPT, Gemini, …) call these tools to create and
//! edit roof designs in the user's project library. Every write goes through
//! [`Store::save_as`], so the open desktop app hot-reloads it and shows the
//! user what changed, with the agent's summary and an option to revert.
//!
//! Spec: <https://modelcontextprotocol.io/specification/2025-06-18>.

use std::io::{self, BufRead, Write};

use serde_json::{Value, json};
use tectora_core::diff::diff;
use tectora_core::examples::{StandardRoof, standard_roof};
use tectora_core::geom::Point2;
use tectora_core::model::GABLE_PITCH_DEG;
use tectora_core::quantities::{area_by_pitch, dacad_line_totals, roof_totals};
use tectora_core::roof::LineKind;
use tectora_core::settings::Settings;
use tectora_core::store::{LiveStatus, Store, now_ms};
use tectora_core::{
    Chimney, EdgeDef, HalfHip, JobStatus, PitchBreak, Project, RoofPart, RoofWindow,
};

mod drawings;
mod more;

pub const GUIDE: &str = include_str!("guide.md");
const SUPPORTED_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

pub struct Server {
    store: Store,
    /// Name of the connected client, used as the actor in the activity log.
    client: String,
}

type ToolResult = Result<Value, String>;

fn arg<'a>(args: &'a Value, key: &str) -> Result<&'a Value, String> {
    args.get(key)
        .filter(|v| !v.is_null())
        .ok_or_else(|| format!("missing argument `{key}`"))
}

fn arg_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    arg(args, key)?
        .as_str()
        .ok_or_else(|| format!("`{key}` must be a string"))
}

fn arg_f64(args: &Value, key: &str) -> Result<Option<f64>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_f64()
            .filter(|f| f.is_finite())
            .map(Some)
            .ok_or_else(|| format!("`{key}` must be a number")),
    }
}

fn round(v: f64, digits: i32) -> f64 {
    let f = 10f64.powi(digits);
    (v * f).round() / f
}

/// Edge table the agent can reason about (lengths are derived).
fn describe_part(part: &RoofPart) -> Value {
    let n = part.edges.len();
    let edges: Vec<Value> = part
        .edges
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let b = part.edges[(i + 1) % n].start;
            let mut v = json!({
                "index": i,
                "from": [e.start.x, e.start.y],
                "to": [b.x, b.y],
                "length": round(e.start.distance(b), 3),
                "type": if e.is_gable() { "gable" } else { "eave" },
                "eave_height": e.eave_height,
                "overhang": e.overhang,
            });
            if !e.is_gable() {
                v["pitch_deg"] = json!(e.pitch_deg);
            }
            if let Some(h) = &e.half_hip {
                v["half_hip"] = json!(h);
            }
            if !e.breaks.is_empty() {
                v["breaks"] = json!(e.breaks);
            }
            if e.priority > 0 {
                v["priority"] = json!(e.priority);
            }
            if e.material != tectora_core::model::DEFAULT_MATERIAL {
                v["material"] = json!(e.material);
            }
            v
        })
        .collect();
    json!({ "name": part.name, "edges": edges })
}

/// A plausibility warning of a calculated roof (`tectora_core::check`),
/// with what it means for whoever reads the result.
fn warning(w: &tectora_core::check::RoofWarning) -> Value {
    use tectora_core::check::WarningKind;
    let message = match w.kind {
        WarningKind::OutsideOutline => format!(
            "part of the roof lies up to {:.2} m outside the eave outline; check pitches and overhangs",
            w.value
        ),
        WarningKind::OverlappingFaces => format!(
            "roof faces pass through or overlap each other over {:.2} m2; quantities may be wrong",
            w.value
        ),
        WarningKind::AreaMismatch => format!(
            "reported area differs from the roof faces by {:.1} %; check the area",
            w.value * 100.0
        ),
    };
    json!({ "kind": w.kind, "value": w.value, "message": message })
}

fn quantities(project: &Project) -> Value {
    let t = roof_totals(project);
    let mut q = json!({
        "roof_area_m2": round(t.roof_area, 2),
        "ridge_height_m": round(t.ridge_height, 2),
        "hip_count": t.hip_count,
        "valley_count": t.valley_count,
        "lines_by_dacad_code_m": dacad_line_totals(project),
        "area_by_pitch_m2": area_by_pitch(project)
            .iter()
            .map(|&(p, a)| json!({ "pitch_deg": p, "area": a }))
            .collect::<Vec<_>>(),
        "area_by_material_m2": t
            .area_by_material
            .iter()
            .map(|&(m, a)| json!({ "material": m, "name": project.material_name(m), "area": round(a, 2) }))
            .collect::<Vec<_>>(),
    });
    // With more than one material: the lines each one's offer needs.
    let by_line = tectora_core::quantities::lines_by_material(project);
    if by_line.len() > 1 {
        q["lines_by_material_m"] = by_line
            .into_iter()
            .map(|(m, lines)| json!({ "material": m, "name": project.material_name(m), "lines_by_dacad_code_m": lines }))
            .collect();
    }
    q["heights_m"] = heights(project);
    q
}

/// The heights a section drawing labels, per part: the distinct heights of
/// its level eaves and pitch breaks and its highest point, so they can be
/// checked against the drawing's labels without reading every line.
fn heights(project: &Project) -> Value {
    let distinct = |mut v: Vec<f64>| {
        v.sort_by(f64::total_cmp);
        v.dedup_by(|a, b| (*a - *b).abs() < 0.015);
        v
    };
    tectora_core::openings::generate_project(project)
        .iter()
        .zip(&project.parts)
        .enumerate()
        .map(|(i, (g, part))| {
            let level = |kind: LineKind| {
                distinct(
                    g.lines
                        .iter()
                        .filter(|l| l.kind == kind && (l.a[2] - l.b[2]).abs() < 0.005)
                        .map(|l| round(l.a[2], 2))
                        .collect(),
                )
            };
            let top = g
                .lines
                .iter()
                .flat_map(|l| [l.a[2], l.b[2]])
                .fold(f64::NEG_INFINITY, f64::max);
            json!({
                "part": i,
                "name": part.name,
                "eaves": level(LineKind::Eave),
                "pitch_breaks": level(LineKind::Knick),
                "top": if top.is_finite() { json!(round(top, 2)) } else { Value::Null },
            })
        })
        .collect()
}

fn parse_status(s: &str) -> Result<JobStatus, String> {
    serde_json::from_value(json!(s)).map_err(|_| format!("unknown status `{s}`"))
}

/// Applies the optional per-edge fields of `v` to `edge`.
fn apply_edge_fields(edge: &mut EdgeDef, v: &Value) -> Result<(), String> {
    if let Some(p) = arg_f64(v, "pitch_deg")? {
        if !(0.0 < p && p < 90.0) {
            return Err(format!("pitch_deg {p} must be between 0 and 90"));
        }
        edge.pitch_deg = p;
    }
    match v.get("gable").and_then(Value::as_bool) {
        Some(true) => edge.pitch_deg = GABLE_PITCH_DEG.ceil(),
        Some(false) if edge.is_gable() => {
            edge.pitch_deg = arg_f64(v, "pitch_deg")?.unwrap_or(40.0);
            edge.half_hip = None;
        }
        _ => {}
    }
    if let Some(h) = arg_f64(v, "eave_height")? {
        edge.eave_height = h;
    }
    if let Some(o) = arg_f64(v, "overhang")? {
        if o < 0.0 {
            return Err("overhang must not be negative".into());
        }
        edge.overhang = o;
    }
    match v.get("half_hip") {
        Some(Value::Null) => edge.half_hip = None,
        Some(h) => {
            let hh: HalfHip =
                serde_json::from_value(h.clone()).map_err(|e| format!("half_hip: {e}"))?;
            edge.half_hip = Some(hh);
        }
        None => {}
    }
    if let Some(m) = v.get("material") {
        edge.material = match m.as_u64() {
            Some(m @ 1..=255) => m as u8,
            _ => return Err("material must be a material number from 1 (1 tiles, 2 sheet metal, 3 stone, 4 slate, 5 solar panels)".into()),
        };
    }
    if let Some(p) = v.get("priority") {
        edge.priority = match p.as_u64().or_else(|| p.as_bool().map(u64::from)) {
            Some(0) => 0,
            Some(1) => 1,
            _ => return Err("priority must be 0 or 1".into()),
        };
    }
    match v.get("breaks") {
        Some(Value::Null) => edge.breaks.clear(),
        Some(b) => {
            let breaks: Vec<PitchBreak> =
                serde_json::from_value(b.clone()).map_err(|e| format!("breaks: {e}"))?;
            if let Some(bad) = breaks
                .iter()
                .find(|b| !(0.0 < b.pitch_deg && b.pitch_deg < 90.0))
            {
                return Err(format!(
                    "breaks: pitch_deg {} must be between 0 and 90",
                    bad.pitch_deg
                ));
            }
            edge.breaks = breaks;
        }
        None => {}
    }
    Ok(())
}

fn parse_points(v: &Value) -> Result<Vec<Point2>, String> {
    let pts: Vec<[f64; 2]> = serde_json::from_value(v.clone())
        .map_err(|_| "`points` must be [[x, y], …]".to_string())?;
    if pts.len() < 3 {
        return Err("a footprint needs at least 3 points".into());
    }
    Ok(pts.into_iter().map(|[x, y]| Point2::new(x, y)).collect())
}

/// Builds a part from `{name?, points, defaults?, edges?}`.
fn parse_part(v: &Value, index: usize) -> Result<RoofPart, String> {
    let points = parse_points(arg(v, "points")?)?;
    let defaults = v.get("defaults").cloned().unwrap_or(json!({}));
    // Values not given come from the user's settings (Settings › New roofs).
    let preset = Settings::load().roof_defaults;
    let base = EdgeDef {
        start: Point2::new(0.0, 0.0),
        pitch_deg: arg_f64(&defaults, "pitch_deg")?.unwrap_or(preset.pitch_deg),
        eave_height: arg_f64(&defaults, "eave_height")?.unwrap_or(preset.eave_height),
        overhang: arg_f64(&defaults, "overhang")?.unwrap_or(preset.overhang),
        half_hip: None,
        direction_rad: None,
        breaks: Vec::new(),
        priority: 0,
        clad: false,
        material: tectora_core::model::DEFAULT_MATERIAL,
        source: None,
    };
    let specs = v
        .get("edges")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if !specs.is_empty() && specs.len() != points.len() {
        return Err(format!(
            "`edges` has {} entries but there are {} points (one edge per point)",
            specs.len(),
            points.len()
        ));
    }
    let mut edges = Vec::with_capacity(points.len());
    for (i, p) in points.iter().enumerate() {
        let mut e = base.clone();
        e.start = *p;
        if let Some(spec) = specs.get(i) {
            apply_edge_fields(&mut e, spec).map_err(|m| format!("edge {i}: {m}"))?;
        }
        edges.push(e);
    }
    let mut part = RoofPart {
        name: v
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| {
                if index == 0 {
                    "Main".into()
                } else {
                    format!("Part {}", index + 1)
                }
            }),
        edges,
    };
    part.normalize_ccw();
    Ok(part)
}

/// Selected edge indices: `indices: [..]`, `all: true`, or `index`.
fn edge_indices(v: &Value, n: usize) -> Result<Vec<usize>, String> {
    if v.get("all").and_then(Value::as_bool) == Some(true) {
        return Ok((0..n).collect());
    }
    let list: Vec<usize> = if let Some(i) = v.get("index").and_then(Value::as_u64) {
        vec![i as usize]
    } else {
        serde_json::from_value(arg(v, "indices")?.clone())
            .map_err(|_| "`indices` must be a list of edge numbers".to_string())?
    };
    if let Some(bad) = list.iter().find(|&&i| i >= n) {
        return Err(format!(
            "edge {bad} does not exist (the part has {n} edges, 0..{})",
            n - 1
        ));
    }
    Ok(list)
}

fn schema(props: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": props, "required": required })
}

fn edge_props() -> Value {
    json!({
        "pitch_deg": { "type": "number", "description": "Roof pitch rising from the edge, degrees (0–90)." },
        "gable": { "type": "boolean", "description": "true = gable wall (no roof plane rises from it)." },
        "eave_height": { "type": "number", "description": "Eave height above ±0.00 at the overhang line, m." },
        "overhang": { "type": "number", "description": "Horizontal eave overhang, m." },
        "half_hip": {
            "type": ["object", "null"],
            "description": "Gable edges only: Krüppelwalm. null removes it.",
            "properties": { "height": { "type": "number" }, "pitch_deg": { "type": "number" } }
        },
        "priority": {
            "type": "integer",
            "enum": [0, 1],
            "description": "DACAD priority. Where this plane meets a parallel plane of another pitch in a horizontal line, the plane with priority 1 ends there; with no priority the steeper plane ends."
        },
        "material": {
            "type": "integer",
            "minimum": 1,
            "description": "Roof material of the wall's faces, DACAD numbering: 1 tiles (default), 2 sheet metal, 3 stone, 4 slate, 5 solar panels. Splits the roof area by material."
        },
        "breaks": {
            "type": ["array", "null"],
            "description": "Pitch breaks (Knick, e.g. mansard): from each `height` (m above ±0.00, above the eave) the plane continues at `pitch_deg`. [] or null removes them.",
            "items": {
                "type": "object",
                "properties": { "height": { "type": "number" }, "pitch_deg": { "type": "number" } },
                "required": ["height", "pitch_deg"]
            }
        }
    })
}

/// Edge fields plus the selection of edges, for `update_edges`.
fn edge_change_props() -> Value {
    let mut p = edge_props();
    p["indices"] = json!({ "type": "array", "items": { "type": "integer" } });
    p["index"] = json!({ "type": "integer" });
    p["all"] = json!({ "type": "boolean" });
    p
}

fn summary_prop() -> Value {
    json!({ "type": "string", "description": "One sentence for the user on what you changed and why (shown in the app)." })
}

fn tools() -> Value {
    let mut list = base_tools();
    if let Value::Array(a) = &mut list {
        a.extend(more::tools());
        a.extend(drawings::tools());
    }
    list
}

fn base_tools() -> Value {
    let part_schema = json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "points": {
                "type": "array",
                "description": "Wall-line footprint corners [[x, y], …] in metres, in order.",
                "items": { "type": "array", "items": { "type": "number" }, "minItems": 2, "maxItems": 2 }
            },
            "defaults": { "type": "object", "description": "Values for every edge.", "properties": edge_props() },
            "edges": {
                "type": "array",
                "description": "Optional per-edge overrides; edge i runs from points[i] to points[i+1].",
                "items": { "type": "object", "properties": edge_props() }
            }
        },
        "required": ["points"]
    });
    json!([
        {
            "name": "tectora_guide",
            "description": "Read first. Explains Tectora's roof model (DACAD conventions), units, result codes and how to turn a technical drawing into a roof.",
            "inputSchema": schema(json!({}), &[]),
        },
        {
            "name": "get_settings",
            "description": "The user's Tectora settings: projects folder, drawings inbox folder (technical drawings to build roofs from), language, theme, default pitch / eave height / overhang for new roofs, AI follow mode.",
            "inputSchema": schema(json!({}), &[]),
        },
        {
            "name": "update_settings",
            "description": "Change Tectora settings, same as the app's Settings screen. Pass only the keys to change; null resets a folder or the language to its default. Ask the user before moving the projects folder.",
            "inputSchema": schema(json!({
                "projects_dir": { "type": ["string", "null"], "description": "Absolute path of the design library folder." },
                "drawings_dir": { "type": ["string", "null"], "description": "Absolute path of the drawings inbox; default <projects_dir>/drawings." },
                "locale": { "type": ["string", "null"], "enum": ["cs", "sk", "en", "de", null] },
                "theme": { "type": "string", "enum": ["system", "light", "dark"] },
                "roof_defaults": {
                    "type": "object",
                    "properties": {
                        "pitch_deg": { "type": "number", "minimum": 0, "maximum": 89 },
                        "eave_height": { "type": "number" },
                        "overhang": { "type": "number", "minimum": 0 }
                    }
                },
                "follow_ai": { "type": "boolean" }
            }), &[]),
        },
        {
            "name": "list_projects",
            "description": "List roof designs in the user's library (id, name, job, roof area, last change).",
            "inputSchema": schema(json!({ "query": { "type": "string", "description": "Filter by name, customer, address or job number." } }), &[]),
        },
        {
            "name": "get_project",
            "description": "Full design: parts with an edge table (index, from, to, length, eave/gable, pitch, eave height, overhang), job data and quantities.",
            "inputSchema": schema(json!({ "id": { "type": "string" } }), &["id"]),
        },
        {
            "name": "create_standard_roof",
            "description": "Create a new design from a rectangle, like DACAD's Standard-Dach: gable, shed, hip, pyramid or half_hip. The long side `length` runs along x.",
            "inputSchema": schema(json!({
                "name": { "type": "string" },
                "type": { "type": "string", "enum": ["gable", "shed", "hip", "pyramid", "half_hip"] },
                "length": { "type": "number", "description": "Wall length along x, m." },
                "width": { "type": "number", "description": "Wall width along y, m (ignored for pyramid)." },
                "pitch_deg": { "type": "number" },
                "eave_height": { "type": "number" },
                "overhang": { "type": "number" },
                "summary": summary_prop(),
            }), &["name", "type", "length", "width", "pitch_deg", "summary"]),
        },
        {
            "name": "create_project",
            "description": "Create a new design from footprint points with per-edge roof definitions (any shape, several parts).",
            "inputSchema": schema(json!({
                "name": { "type": "string" },
                "parts": { "type": "array", "items": part_schema },
                "job": { "type": "object", "properties": {
                    "number": { "type": "string" }, "customer": { "type": "string" }, "address": { "type": "string" }
                } },
                "notes": { "type": "string" },
                "summary": summary_prop(),
            }), &["name", "parts", "summary"]),
        },
        {
            "name": "update_edges",
            "description": "Change roof definitions of edges (pitch, gable, eave height, overhang, half-hip, breaks, priority, material). Select edges with `indices`, `index` or `all`.",
            "inputSchema": schema(json!({
                "id": { "type": "string" },
                "part": { "type": "integer", "description": "Part index (default 0)." },
                "changes": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": edge_change_props()
                    }
                },
                "summary": summary_prop(),
            }), &["id", "changes", "summary"]),
        },
        {
            "name": "set_outline",
            "description": "Replace the footprint points of a part. Edge definitions are kept by index; new edges copy the previous last edge or `defaults`.",
            "inputSchema": schema(json!({
                "id": { "type": "string" },
                "part": { "type": "integer" },
                "points": part_schema["properties"]["points"].clone(),
                "defaults": { "type": "object", "properties": edge_props() },
                "summary": summary_prop(),
            }), &["id", "points", "summary"]),
        },
        {
            "name": "add_part",
            "description": "Add another roof part (e.g. a wing or garage) to an existing design.",
            "inputSchema": schema(json!({
                "id": { "type": "string" },
                "part": part_schema,
                "summary": summary_prop(),
            }), &["id", "part", "summary"]),
        },
        {
            "name": "update_job",
            "description": "Change the design name, notes or job data (number, customer, address, status: draft | offer | ordered | done). When the status leaves draft, the current result is stored as the job's result (see store_result).",
            "inputSchema": schema(json!({
                "id": { "type": "string" },
                "name": { "type": "string" }, "notes": { "type": "string" },
                "number": { "type": "string" }, "customer": { "type": "string" }, "address": { "type": "string" },
                "status": { "type": "string", "enum": ["draft", "offer", "ordered", "done"] },
                "summary": summary_prop(),
            }), &["id", "summary"]),
        },
        {
            "name": "set_openings",
            "description": "Replace the roof windows, chimneys and/or roof openings of a design (omit a list to keep it). Windows deduct width × height from their roof face; chimneys deduct their plan area / cos(pitch) and add wall-abutment lines WV/WH/WS, as in DACAD; roof openings (holes) deduct like chimneys but their sides become an eave (uphill), a Pult (downhill) and verges.",
            "inputSchema": schema(json!({
                "id": { "type": "string" },
                "windows": {
                    "type": "array",
                    "description": "Roof windows. x, y = plan point of the lower corner at the start of the eave (left end seen from outside); width along the eave and height up the slope, in the roof plane, m.",
                    "items": {
                        "type": "object",
                        "properties": { "x": { "type": "number" }, "y": { "type": "number" }, "width": { "type": "number" }, "height": { "type": "number" } },
                        "required": ["x", "y", "width", "height"]
                    }
                },
                "chimneys": {
                    "type": "array",
                    "description": "Chimneys as convex plan polygons (`corners`), or as a rectangle: corner `x`, `y`, `width`, `depth` and `rotation_deg`.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "corners": { "type": "array", "items": { "type": "array", "items": { "type": "number" }, "minItems": 2, "maxItems": 2 } },
                            "x": { "type": "number" }, "y": { "type": "number" },
                            "width": { "type": "number" }, "depth": { "type": "number" }, "rotation_deg": { "type": "number" },
                            "height": { "type": "number", "description": "Top height, m (display only)." }
                        }
                    }
                },
                "roof_openings": {
                    "type": "array",
                    "description": "Holes in the roof (DACAD: Öffnung) as convex plan polygons.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "corners": { "type": "array", "items": { "type": "array", "items": { "type": "number" }, "minItems": 2, "maxItems": 2 } }
                        },
                        "required": ["corners"]
                    }
                },
                "summary": summary_prop(),
            }), &["id", "summary"]),
        },
        {
            "name": "calc",
            "description": "Compute the roof: area, ridge height and line lengths by DACAD code (T eave, F ridge, G hip, K valley, OL/OR/OS verge left/right/oblique, P shed top edge, c pitch break), per part and in total. A part whose roof looks geometrically wrong (outside its eave outline, faces passing through each other, area not matching its faces) carries `warnings`: tell the user and suggest checking pitches and overhangs.",
            "inputSchema": schema(json!({ "id": { "type": "string" } }), &["id"]),
        },
        {
            "name": "show_project",
            "description": "Bring a design to the front in the user's Tectora app.",
            "inputSchema": schema(json!({ "id": { "type": "string" } }), &["id"]),
        },
        {
            "name": "revert_last_change",
            "description": "Undo the latest recorded change of a design.",
            "inputSchema": schema(json!({ "id": { "type": "string" } }), &["id"]),
        },
    ])
}

impl Server {
    pub fn new(store: Store) -> Self {
        Self {
            store,
            client: "AI".into(),
        }
    }

    fn live(&self, message: String, project_id: Option<&str>) {
        self.store.set_live(&LiveStatus {
            ts_ms: now_ms(),
            actor: self.client.clone(),
            message,
            project_id: project_id.map(str::to_string),
        });
    }

    /// Saves `project` as a change by the connected client and reports it.
    fn commit(
        &self,
        id: Option<&str>,
        before: Option<&Project>,
        project: &Project,
        summary: &str,
    ) -> ToolResult {
        project.check_limits()?;
        let id = self
            .store
            .save_as(id.map(str::to_string), project, &self.client, summary)?;
        self.live(summary.to_string(), Some(&id));
        let changes = before.map(|b| diff(b, project)).unwrap_or_default();
        Ok(json!({
            "id": id,
            "saved": true,
            "note": "Saved. The change is now visible in the Tectora app for the user to review.",
            "changes": changes,
            "quantities": quantities(project),
            "parts": project.parts.iter().map(describe_part).collect::<Vec<_>>(),
        }))
    }

    fn load(&self, args: &Value) -> Result<(String, Project), String> {
        let id = arg_str(args, "id")?.to_string();
        let project = self
            .store
            .load(&id)
            .map_err(|_| format!("no design with id `{id}`; call list_projects to see the ids"))?;
        project.check_limits().map_err(|e| {
            format!("design `{id}` cannot be calculated: {e}; ask the user to fix it in the app")
        })?;
        Ok((id, project))
    }

    /// Writes the settings file; when this server uses the library folder
    /// from the settings and that folder changed, it switches to the new one.
    fn update_settings(&mut self, args: &Value) -> ToolResult {
        let old = Settings::load();
        let next = old
            .patched(args)
            .map_err(|e| format!("settings not saved: {e}"))?;
        next.save()
            .map_err(|e| format!("settings not saved: {e}"))?;
        if self.store.dir == old.projects_dir() && next.projects_dir() != old.projects_dir() {
            self.store = Store::new(next.projects_dir()).map_err(|e| e.to_string())?;
        }
        self.live("Changed Tectora settings".into(), None);
        Ok(next.describe())
    }

    /// Runs a tool; a panic in the geometry kernel becomes a tool error
    /// instead of ending the server for the rest of the session.
    pub fn call_tool(&mut self, name: &str, args: &Value) -> ToolResult {
        tectora_core::model::guarded(|| self.call_tool_unguarded(name, args))?
    }

    fn call_tool_unguarded(&mut self, name: &str, args: &Value) -> ToolResult {
        match name {
            "tectora_guide" => Ok(json!(GUIDE)),
            "get_settings" => Ok(Settings::load().describe()),
            "update_settings" => self.update_settings(args),
            "list_projects" => {
                self.live("Browsing the design library".into(), None);
                let q = args
                    .get("query")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_lowercase();
                let list: Vec<Value> = self
                    .store
                    .list()
                    .into_iter()
                    .filter(|p| {
                        q.is_empty()
                            || [&p.name, &p.job.customer, &p.job.address, &p.job.number]
                                .iter()
                                .any(|s| s.to_lowercase().contains(&q))
                    })
                    .map(|p| {
                        json!({
                            "id": p.id, "name": p.name, "job": p.job,
                            "roof_area_m2": round(p.roof_area, 2), "parts": p.parts,
                            "modified_ms": p.modified_ms, "last_change": p.last_activity,
                        })
                    })
                    .collect();
                Ok(json!(list))
            }
            "get_project" => {
                let (id, p) = self.load(args)?;
                self.live(format!("Reading “{}”", p.name), Some(&id));
                Ok(json!({
                    "id": id,
                    "name": p.name,
                    "notes": p.notes,
                    "job": p.job,
                    "parts": p.parts.iter().map(describe_part).collect::<Vec<_>>(),
                    "chimneys": p.chimneys,
                    "windows": p.windows,
                    "roof_openings": p.roof_openings,
                    "dormers": p.dormers.iter().enumerate().map(|(i, d)| more::describe_dormer(i, d)).collect::<Vec<_>>(),
                    "manual_lines": p.manual_lines,
                    "manual_faces": p.manual_faces,
                    "flat_roof": p.flat_roof,
                    "materials": (1..=tectora_core::model::DACAD_MATERIALS).map(|m| json!({ "material": m, "name": p.material_name(m) })).collect::<Vec<_>>(),
                    "files": self.store.attachments(&id).iter().map(|a| a.name.clone()).collect::<Vec<_>>(),
                    "stored_result": tectora_core::snapshot::status(&p),
                    "quantities": quantities(&p),
                }))
            }
            "calc" => {
                let (id, p) = self.load(args)?;
                self.live(format!("Calculating “{}”", p.name), Some(&id));
                let parts: Vec<Value> = p
                    .parts
                    .iter()
                    .zip(tectora_core::openings::generate_project(&p))
                    .map(|(part, geo)| {
                        let q = geo.quantities();
                        let mut v = json!({ "name": part.name, "quantities": q });
                        if !geo.warnings.is_empty() {
                            v["warnings"] = geo.warnings.iter().map(warning).collect();
                        }
                        v
                    })
                    .collect();
                Ok(json!({
                    "id": id,
                    "total": quantities(&p),
                    "parts": parts,
                    "engine_version": tectora_core::snapshot::ENGINE_VERSION,
                    "stored_result": tectora_core::snapshot::status(&p),
                }))
            }
            "create_standard_roof" => {
                let kind: StandardRoof = serde_json::from_value(arg(args, "type")?.clone())
                    .map_err(|_| {
                        "type must be gable, shed, hip, pyramid or half_hip".to_string()
                    })?;
                let length = arg_f64(args, "length")?.ok_or("missing `length`")?;
                let width = arg_f64(args, "width")?.unwrap_or(length);
                let pitch = arg_f64(args, "pitch_deg")?.ok_or("missing `pitch_deg`")?;
                let preset = Settings::load().roof_defaults;
                if length <= 0.0 || width <= 0.0 || !(0.0 < pitch && pitch < 90.0) {
                    return Err(
                        "length and width must be positive and pitch between 0 and 90".into(),
                    );
                }
                let mut p = Project::new(arg_str(args, "name")?);
                p.parts.push(standard_roof(
                    kind,
                    length,
                    width,
                    pitch,
                    arg_f64(args, "eave_height")?.unwrap_or(preset.eave_height),
                    arg_f64(args, "overhang")?.unwrap_or(preset.overhang),
                ));
                self.commit(None, None, &p, arg_str(args, "summary")?)
            }
            "create_project" => {
                let mut p = Project::new(arg_str(args, "name")?);
                let parts = arg(args, "parts")?
                    .as_array()
                    .ok_or("`parts` must be a list")?;
                if parts.is_empty() {
                    return Err("at least one part is required".into());
                }
                for (i, v) in parts.iter().enumerate() {
                    p.parts
                        .push(parse_part(v, i).map_err(|m| format!("part {i}: {m}"))?);
                }
                if let Some(job) = args.get("job") {
                    for (key, slot) in [
                        ("number", &mut p.job.number),
                        ("customer", &mut p.job.customer),
                        ("address", &mut p.job.address),
                    ] {
                        if let Some(s) = job.get(key).and_then(Value::as_str) {
                            *slot = s.to_string();
                        }
                    }
                }
                if let Some(n) = args.get("notes").and_then(Value::as_str) {
                    p.notes = n.to_string();
                }
                self.commit(None, None, &p, arg_str(args, "summary")?)
            }
            "update_edges" => {
                let (id, before) = self.load(args)?;
                let mut p = before.clone();
                let pi = args.get("part").and_then(Value::as_u64).unwrap_or(0) as usize;
                let part = p
                    .parts
                    .get_mut(pi)
                    .ok_or_else(|| format!("part {pi} does not exist"))?;
                let changes = arg(args, "changes")?
                    .as_array()
                    .ok_or("`changes` must be a list")?;
                for c in changes {
                    for i in edge_indices(c, part.edges.len())? {
                        apply_edge_fields(&mut part.edges[i], c)
                            .map_err(|m| format!("edge {i}: {m}"))?;
                    }
                }
                self.commit(Some(&id), Some(&before), &p, arg_str(args, "summary")?)
            }
            "set_outline" => {
                let (id, before) = self.load(args)?;
                let mut p = before.clone();
                let pi = args.get("part").and_then(Value::as_u64).unwrap_or(0) as usize;
                let old = p
                    .parts
                    .get(pi)
                    .ok_or_else(|| format!("part {pi} does not exist"))?
                    .clone();
                let points = parse_points(arg(args, "points")?)?;
                let mut template = old.edges.last().cloned().expect("part has edges");
                template.half_hip = None;
                template.direction_rad = None;
                if let Some(d) = args.get("defaults") {
                    apply_edge_fields(&mut template, d)?;
                }
                let edges = points
                    .iter()
                    .enumerate()
                    .map(|(i, pt)| {
                        let mut e = old
                            .edges
                            .get(i)
                            .cloned()
                            .unwrap_or_else(|| template.clone());
                        e.start = *pt;
                        e.direction_rad = None;
                        e
                    })
                    .collect();
                let mut part = RoofPart {
                    name: old.name,
                    edges,
                };
                part.normalize_ccw();
                p.parts[pi] = part;
                self.commit(Some(&id), Some(&before), &p, arg_str(args, "summary")?)
            }
            "add_part" => {
                let (id, before) = self.load(args)?;
                let mut p = before.clone();
                let part = parse_part(arg(args, "part")?, p.parts.len())?;
                p.parts.push(part);
                self.commit(Some(&id), Some(&before), &p, arg_str(args, "summary")?)
            }
            "update_job" => {
                let (id, before) = self.load(args)?;
                let mut p = before.clone();
                if let Some(s) = args.get("name").and_then(Value::as_str) {
                    p.name = s.to_string();
                }
                if let Some(s) = args.get("notes").and_then(Value::as_str) {
                    p.notes = s.to_string();
                }
                for (key, slot) in [
                    ("number", &mut p.job.number),
                    ("customer", &mut p.job.customer),
                    ("address", &mut p.job.address),
                ] {
                    if let Some(s) = args.get(key).and_then(Value::as_str) {
                        *slot = s.to_string();
                    }
                }
                if let Some(s) = args.get("status").and_then(Value::as_str) {
                    p.set_status(parse_status(s)?);
                }
                self.commit(Some(&id), Some(&before), &p, arg_str(args, "summary")?)
            }
            "set_openings" => {
                let (id, before) = self.load(args)?;
                let mut p = before.clone();
                if let Some(w) = args.get("windows") {
                    p.windows = serde_json::from_value::<Vec<RoofWindow>>(w.clone())
                        .map_err(|e| format!("windows: {e}"))?;
                }
                if let Some(c) = args.get("chimneys") {
                    let raw: Vec<Value> = serde_json::from_value(c.clone())
                        .map_err(|_| "chimneys must be a list".to_string())?;
                    p.chimneys = raw
                        .iter()
                        .map(|c| {
                            let height = arg_f64(c, "height")?.unwrap_or(0.0);
                            if c.get("corners").is_none() {
                                let (w, d) = (arg_f64(c, "width")?, arg_f64(c, "depth")?);
                                let (Some(w), Some(d)) = (w, d) else {
                                    return Err("a chimney needs `corners` or `x`, `y`, `width` and `depth`".to_string());
                                };
                                return Ok(tectora_core::edit::probe::chimney_from_rect(
                                    arg_f64(c, "x")?.unwrap_or(0.0),
                                    arg_f64(c, "y")?.unwrap_or(0.0),
                                    w,
                                    d,
                                    arg_f64(c, "rotation_deg")?.unwrap_or(0.0),
                                    height,
                                ));
                            }
                            let corners = parse_points(arg(c, "corners")?)?;
                            Ok(Chimney { corners, height })
                        })
                        .collect::<Result<_, String>>()?;
                }
                if let Some(o) = args.get("roof_openings") {
                    let raw: Vec<Value> = serde_json::from_value(o.clone())
                        .map_err(|_| "roof_openings must be a list".to_string())?;
                    p.roof_openings = raw
                        .iter()
                        .map(|o| {
                            let corners = parse_points(arg(o, "corners")?)?;
                            Ok(Chimney {
                                corners,
                                height: 0.0,
                            })
                        })
                        .collect::<Result<_, String>>()?;
                }
                self.commit(Some(&id), Some(&before), &p, arg_str(args, "summary")?)
            }
            "show_project" => {
                let (id, p) = self.load(args)?;
                self.live(format!("Showing “{}”", p.name), Some(&id));
                Ok(json!({ "id": id, "shown": true }))
            }
            "revert_last_change" => {
                let (id, _) = self.load(args)?;
                let p = self.store.revert_last(&id, &self.client)?;
                self.live("Reverted the latest change".into(), Some(&id));
                Ok(json!({ "id": id, "reverted": true, "quantities": quantities(&p) }))
            }
            _ => self
                .call_more(name, args)
                .or_else(|| self.call_drawing(name, args))
                .unwrap_or_else(|| Err(format!("unknown tool `{name}`"))),
        }
    }

    /// Handles one JSON-RPC message; returns the response for requests.
    pub fn handle(&mut self, msg: &Value) -> Option<Value> {
        let id = msg.get("id").cloned();
        let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
        let params = msg.get("params").cloned().unwrap_or(json!({}));
        let result: Result<Value, (i64, String)> = match method {
            "initialize" => {
                if let Some(name) = params.pointer("/clientInfo/name").and_then(Value::as_str) {
                    self.client = name.to_string();
                }
                let requested = params
                    .get("protocolVersion")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let version = SUPPORTED_VERSIONS
                    .iter()
                    .find(|v| **v == requested)
                    .unwrap_or(&SUPPORTED_VERSIONS[0]);
                Ok(json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "tectora", "version": env!("CARGO_PKG_VERSION") },
                    "instructions": "Tectora roof design. Call tectora_guide first. Every save is shown live to the user in the Tectora app, so pass a clear `summary`.",
                }))
            }
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tools() })),
            "tools/call" => {
                let name = params.get("name").and_then(Value::as_str).unwrap_or("");
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                Ok(match self.call_tool(name, &args) {
                    Ok(Value::Object(mut o)) if o.contains_key(drawings::CONTENT_KEY) => {
                        json!({ "content": o.remove(drawings::CONTENT_KEY).unwrap_or_default() })
                    }
                    Ok(Value::String(text)) => {
                        json!({ "content": [{ "type": "text", "text": text }] })
                    }
                    Ok(v) => json!({
                        "content": [{ "type": "text", "text": serde_json::to_string_pretty(&v).unwrap_or_default() }],
                    }),
                    Err(e) => {
                        json!({ "content": [{ "type": "text", "text": e }], "isError": true })
                    }
                })
            }
            m if m.starts_with("notifications/") => return None,
            _ => Err((-32601, format!("method not found: {method}"))),
        };
        let id = id?;
        Some(match result {
            Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
            Err((code, message)) => {
                json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
            }
        })
    }
}

/// Serves MCP over stdin/stdout (one JSON-RPC message per line) until EOF.
pub fn run_stdio(store: Store) -> io::Result<()> {
    let mut server = Server::new(store);
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Value>(&line) {
            Ok(msg) => server.handle(&msg),
            Err(e) => Some(json!({
                "jsonrpc": "2.0", "id": null,
                "error": { "code": -32700, "message": format!("parse error: {e}") }
            })),
        };
        if let Some(r) = response {
            writeln!(stdout, "{r}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> Server {
        // Tests run in parallel and remove their folder, so each needs its own.
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "tectora-mcp-{}-{}-{n}",
            now_ms(),
            std::process::id()
        ));
        let mut s = Server::new(Store::new(dir).unwrap());
        s.handle(&json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": { "protocolVersion": "2025-06-18", "clientInfo": { "name": "test-agent" } }
        }));
        s
    }

    #[test]
    fn lists_tools() {
        let mut s = server();
        let r = s
            .handle(&json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }))
            .unwrap();
        let names: Vec<&str> = r["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"create_project") && names.contains(&"update_edges"));
    }

    #[test]
    fn create_update_and_revert() {
        let mut s = server();
        let r = s
            .call_tool(
                "create_project",
                &json!({
                    "name": "L dům",
                    "summary": "Footprint from drawing",
                    "parts": [{
                        "points": [[0, 0], [14, 0], [14, 7], [7, 7], [7, 14], [0, 14]],
                        "defaults": { "pitch_deg": 38, "eave_height": 3.2, "overhang": 0.6 }
                    }]
                }),
            )
            .unwrap();
        let id = r["id"].as_str().unwrap().to_string();
        assert!(r["quantities"]["roof_area_m2"].as_f64().unwrap() > 100.0);

        let r = s
            .call_tool(
                "update_edges",
                &json!({ "id": id, "summary": "Gable at the west", "changes": [{ "index": 5, "gable": true }] }),
            )
            .unwrap();
        assert_eq!(r["changes"][0]["field"], "pitch_deg");
        assert_eq!(r["parts"][0]["edges"][5]["type"], "gable");

        let log = s.store.activity(&id);
        assert_eq!(log.len(), 2);
        assert_eq!(log[1].actor, "test-agent");
        assert_eq!(
            s.store.live().unwrap().project_id.as_deref(),
            Some(id.as_str())
        );

        s.call_tool("revert_last_change", &json!({ "id": id }))
            .unwrap();
        assert!(!s.store.load(&id).unwrap().parts[0].edges[5].is_gable());
        let _ = std::fs::remove_dir_all(&s.store.dir);
    }

    #[test]
    fn settings_read_and_update() {
        let base = std::env::temp_dir().join(format!("tectora-mcp-cfg-{}", std::process::id()));
        // Only this test reads the settings file, so the env var is safe here.
        unsafe { std::env::set_var("TECTORA_CONFIG_DIR", base.join("cfg")) };
        let mut s = server();
        let r = s.call_tool("get_settings", &json!({})).unwrap();
        assert_eq!(r["settings"]["theme"], "system");
        let inbox = base.join("inbox");
        let r = s
            .call_tool(
                "update_settings",
                &json!({ "drawings_dir": inbox.to_string_lossy(), "roof_defaults": { "pitch_deg": 35 } }),
            )
            .unwrap();
        assert_eq!(r["effective"]["drawings_dir"], json!(inbox));
        assert_eq!(r["settings"]["roof_defaults"]["pitch_deg"], 35.0);
        assert!(inbox.is_dir());
        assert!(
            s.call_tool("update_settings", &json!({ "locale": "fr" }))
                .is_err()
        );
        let r = s.call_tool("get_settings", &json!({})).unwrap();
        assert_eq!(r["settings"]["roof_defaults"]["pitch_deg"], 35.0);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn calc_reports_roofs_that_look_wrong() {
        let mut s = server();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/check/C1_hips_outside.json");
        let p: Project = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let id = s.store.save(None, &p).unwrap();
        let r = s.call_tool("calc", &json!({ "id": id })).unwrap();
        let w = r["parts"][0]["warnings"].as_array().unwrap();
        assert_eq!(w[0]["kind"], "outside_outline");
        assert!(w[0]["message"].as_str().unwrap().contains("3.69 m"));

        let mut ok = p.clone();
        for e in &mut ok.parts[0].edges {
            e.overhang = 0.0;
        }
        let id = s.store.save(None, &ok).unwrap();
        let r = s.call_tool("calc", &json!({ "id": id })).unwrap();
        assert!(r["parts"][0].get("warnings").is_none(), "{r}");
    }

    #[test]
    fn bad_input_is_a_tool_error_not_a_crash() {
        let mut s = server();
        let r = s
            .handle(&json!({
                "jsonrpc": "2.0", "id": 3, "method": "tools/call",
                "params": { "name": "update_edges", "arguments": { "id": "missing", "changes": [], "summary": "x" } }
            }))
            .unwrap();
        assert_eq!(r["result"]["isError"], true);
    }

    #[test]
    fn absurd_values_are_rejected() {
        let mut s = server();
        let err = s
            .call_tool(
                "create_project",
                &json!({
                    "name": "x", "summary": "x",
                    "parts": [{ "points": [[0, 0], [1e300, 0], [10, 8]] }]
                }),
            )
            .unwrap_err();
        assert!(err.contains("out of range"), "{err}");
        assert!(s.list_projects_empty());
        let _ = std::fs::remove_dir_all(&s.store.dir);
    }

    impl Server {
        fn list_projects_empty(&self) -> bool {
            self.store.list().is_empty()
        }
    }
}
