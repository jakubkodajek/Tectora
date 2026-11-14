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
use tectora_core::model::{GABLE_PITCH_DEG, MIN_PITCH_DEG};
use tectora_core::quantities::{area_by_pitch, dacad_line_totals, roof_totals};
use tectora_core::roof::LineKind;
use tectora_core::settings::Settings;
use tectora_core::store::{CheckedSave, LiveStatus, Store, StoreError, now_ms};
use tectora_core::{
    Chimney, EdgeDef, HalfHip, JobStatus, PitchBreak, Project, RoofPart, RoofWindow,
};

mod drawings;
mod more;
mod worker;

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
    json!({ "name": part.name, "building_height": part.building_height(), "edges": edges })
}

/// A plausibility warning of a calculated roof (`tectora_core::check`),
/// with what it means for whoever reads the result.
pub fn warning(w: &tectora_core::check::RoofWarning) -> Value {
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
        WarningKind::Covered => "no face of this part's roof is left after joining it with the other parts: another part covers it (a building drawn inside or on top of another); move or delete one of them".to_string(),
        WarningKind::CutExceedsFace => format!(
            "more is deducted from a roof face than the face has ({:.2} m2 below zero), so the roof area is too small: usually a roof window larger than its face (cm typed as m, width and height swapped) or joined buildings; check the windows and the joins",
            w.value
        ),
        WarningKind::DormerOffRoof => format!(
            "dormer {} (0-based) sits on no roof face, so it is left out of the result (no area, lines or material); move it onto the roof or delete it",
            w.dormer.unwrap_or(0)
        ),
        WarningKind::WindowOffRoof => format!(
            "roof window {} (0-based) sits on no roof face, so nothing is deducted for it and it is not in the result; move it onto the roof or delete it",
            w.item.unwrap_or(0)
        ),
        WarningKind::ChimneyOffRoof => format!(
            "chimney {} (0-based) lies on no roof face, so it cuts nothing and is not in the result; move it onto the roof or delete it",
            w.item.unwrap_or(0)
        ),
        WarningKind::OpeningOffRoof => format!(
            "roof opening {} (0-based) lies on no roof face, so it cuts nothing and is not in the result; move it onto the roof or delete it",
            w.item.unwrap_or(0)
        ),
        WarningKind::NotCalculatedByCore => "this roof was calculated by Tectora's fallback engine, not by the standard calculation, so its figures may differ from the standard result; tell the user to check the roof".to_string(),
        WarningKind::MergedWalls => {
            let (kept, ignored) = (w.edges.first(), w.edges.get(1));
            format!(
                "edges {} and {} (0-based) are in line with the same eave height and overhang, so they are calculated as one wall with the pitch and pitch breaks of edge {}; edge {}'s own pitch or pitch breaks are ignored. To keep them, give the walls different eave heights or overhangs, or use one pitch for both",
                kept.map_or(0, |e| *e),
                ignored.map_or(0, |e| *e),
                kept.map_or(0, |e| *e),
                ignored.map_or(0, |e| *e),
            )
        }
    };
    let mut v = json!({ "kind": w.kind, "value": w.value, "message": message });
    if !w.edges.is_empty() {
        v["edges"] = json!(w.edges);
    }
    if let Some(d) = w.dormer {
        v["dormer"] = json!(d);
    }
    if let Some(i) = w.item {
        v["item"] = json!(i);
    }
    v
}

/// Why the calculation gave no roof for a part, in English and without
/// the calculation's own (German) text: the AI passes it on to the user.
pub fn calc_error(e: &tectora_core::roof::CalcError) -> Value {
    use tectora_core::roof::CalcErrorKind;
    let message = match e.kind {
        CalcErrorKind::OpenFaces => {
            "the roof cannot be calculated: its faces do not close; adjust the pitch or length of the wall changed last (a very low pitch or a pitch break above the ridge often does it)"
        }
        CalcErrorKind::NoSolution => {
            "no roof fits these walls and pitches; check pitches and eave heights (very different pitches on neighbouring walls often have no solution)"
        }
        CalcErrorKind::NonConvexOpening => {
            "a chimney or roof opening is not convex, so no roof is calculated; give it no inward corners or split it"
        }
        CalcErrorKind::Cut => {
            "two roof planes never meet, so no roof is calculated; check pitches and short walls"
        }
        CalcErrorKind::Geometry => {
            "the roof geometry is invalid (a wall too short or an extreme pitch), so no roof is calculated"
        }
        CalcErrorKind::NoEnd => {
            "the roof calculation does not finish for this roof; undo the last change or adjust the pitches"
        }
    };
    json!({ "kind": e.kind, "message": message })
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
        if !(MIN_PITCH_DEG..90.0).contains(&p) {
            return Err(format!(
                "pitch_deg {p} must be at least {MIN_PITCH_DEG} and below 90 (use gable for 90)"
            ));
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
        if h < 0.0 {
            return Err(format!("eave_height {h} must be zero or positive"));
        }
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
    if let Some(c) = v.get("clad") {
        edge.clad = c.as_bool().ok_or("clad must be true or false")?;
    }
    if let Some(d) = arg_f64(v, "direction_deg")? {
        if edge.direction_rad.is_none() {
            return Err("direction_deg applies only to a direction change (a zero-length marker wall from edit_walls insert_direction_change)".into());
        }
        edge.direction_rad = Some(d.rem_euclid(360.0).to_radians());
    }
    match v.get("breaks") {
        Some(Value::Null) => edge.breaks.clear(),
        Some(b) => {
            let breaks: Vec<PitchBreak> =
                serde_json::from_value(b.clone()).map_err(|e| format!("breaks: {e}"))?;
            if let Some(bad) = breaks
                .iter()
                .find(|b| !(MIN_PITCH_DEG..=90.0).contains(&b.pitch_deg))
            {
                return Err(format!(
                    "breaks: pitch_deg {} must be between {MIN_PITCH_DEG} and 90",
                    bad.pitch_deg
                ));
            }
            edge.breaks = breaks;
        }
        None => {}
    }
    // The same input limits the editor and the engine use (DACAD's).
    if let Some((code, msg)) = edge.problems().next() {
        return Err(format!("{code}: {msg}"));
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

/// A new wall: pitch, eave height and overhang from `defaults`, else from
/// the user's settings (Settings › New roofs).
fn default_edge(defaults: &Value) -> Result<EdgeDef, String> {
    let preset = Settings::load().roof_defaults;
    let edge = EdgeDef {
        start: Point2::new(0.0, 0.0),
        pitch_deg: arg_f64(defaults, "pitch_deg")?.unwrap_or(preset.pitch_deg),
        eave_height: arg_f64(defaults, "eave_height")?.unwrap_or(preset.eave_height),
        overhang: arg_f64(defaults, "overhang")?.unwrap_or(preset.overhang),
        half_hip: None,
        direction_rad: None,
        breaks: Vec::new(),
        priority: 0,
        clad: false,
        material: tectora_core::model::DEFAULT_MATERIAL,
        source: None,
    };
    if let Some((code, msg)) = edge.problems().next() {
        return Err(format!("defaults: {code}: {msg}"));
    }
    Ok(edge)
}

/// Builds a part from `{name?, points, defaults?, edges?}`.
fn parse_part(v: &Value, index: usize) -> Result<RoofPart, String> {
    let points = parse_points(arg(v, "points")?)?;
    let defaults = v.get("defaults").cloned().unwrap_or(json!({}));
    let base = default_edge(&defaults)?;
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
        height: None,
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

/// The `part` argument: a part index from 0, 0 when left out. A string,
/// a fraction or a negative number is refused instead of editing part 0
/// (ERR-007).
pub(crate) fn part_arg(args: &Value) -> Result<usize, String> {
    match args.get("part") {
        None | Some(Value::Null) => Ok(0),
        Some(v) => v
            .as_u64()
            .map(|i| i as usize)
            .ok_or_else(|| format!("`part` must be a part index from 0, not {v}")),
    }
}

/// Collects the keys of `value` that `schema` does not name, as paths
/// (`changes[0].pitch`). Objects without `properties`, or that allow
/// additional ones, take any key.
fn unknown_keys(schema: &Value, value: &Value, path: &str, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            let Some(props) = schema.get("properties").and_then(Value::as_object) else {
                return;
            };
            if schema
                .get("additionalProperties")
                .is_some_and(|a| a != false)
            {
                return;
            }
            for (k, v) in map {
                let here = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                match props.get(k) {
                    Some(sub) => unknown_keys(sub, v, &here, out),
                    None => out.push(format!("`{here}`")),
                }
            }
        }
        Value::Array(items) => {
            if let Some(sub) = schema.get("items") {
                for (i, v) in items.iter().enumerate() {
                    unknown_keys(sub, v, &format!("{path}[{i}]"), out);
                }
            }
        }
        _ => {}
    }
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
        "clad": { "type": "boolean", "description": "Gable (or a wall leaning outward) covered with the roof material: its area counts as roof area (the editor's \"Clad gable\")." },
        "direction_deg": { "type": "number", "description": "Direction change walls only (zero-length markers from edit_walls insert_direction_change): the direction the roof turns to, degrees counter-clockwise from +x. Add 180 to flip it." },
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
    p["all"] = json!({ "type": "boolean", "description": "Apply to every wall of the part. A pitch_deg given with all leaves gables and direction changes as they are." });
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
        for t in a.iter_mut() {
            let name = t["name"].as_str().unwrap_or_default().to_string();
            let ann = annotations(&name, t);
            t["annotations"] = ann;
        }
    }
    list
}

/// Tools that delete or overwrite something the activity log cannot bring
/// back with Revert (a job, a file, the folders in the settings).
const DESTRUCTIVE: &[&str] = &[
    "delete_project",
    "remove_file",
    "export_job",
    "export_plan",
    "update_settings",
    "clear_roof",
    "delete_items",
];

/// MCP tool annotations (spec 2025-06-18, `ToolAnnotations`). Clients use
/// them to group tools for their permission prompts: Claude Desktop offers
/// "Always allow" per group of read-only and write tools, so a user can stop
/// the prompts for everything that only reads. A tool reads only when it
/// takes no `summary` (every write needs one, `AGENTS.md`). Everything is
/// local: no tool reaches the internet.
fn annotations(name: &str, tool: &Value) -> Value {
    let writes = tool["inputSchema"]["properties"].get("summary").is_some();
    let mut title: String = name.replace('_', " ");
    if let Some(c) = title.get_mut(0..1) {
        c.make_ascii_uppercase();
    }
    let mut a = json!({
        "title": title,
        "readOnlyHint": !writes,
        "openWorldHint": false,
    });
    if writes {
        a["destructiveHint"] = json!(DESTRUCTIVE.contains(&name));
    }
    a
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
            "description": "Change Tectora settings, same as the app's Settings screen. Pass only the keys to change; null resets a folder or the language to its default. A new projects_dir or drawings_dir is only a request: the app asks the user to allow it, and the folders stay as they are until then.",
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
                "follow_ai": { "type": "boolean" },
                "summary": summary_prop()
            }), &["summary"]),
        },
        {
            "name": "list_projects",
            "description": "Search the user's library of roofs made so far (newest first by default): `projects` with id, name, job, roof area, pitch range, dormers, attached files, stored offer result, last change, and `broken`: job files the library cannot read (code `unreadable`: damaged, `newer_format`: saved by a newer Tectora, or a file system error), which you cannot open; tell the user about them. Use it to find a past roof to start a new job from (duplicate_project with `job`).",
            "inputSchema": schema(json!({
                "query": { "type": "string", "description": "Words that must all appear in the name, job number, customer, address or notes (case and accents ignored)." },
                "status": { "type": "string", "enum": ["draft", "offer", "ordered", "done"] },
                "by_ai": { "type": "boolean", "description": "Only jobs last changed by an AI client." },
                "modified_from": { "type": "string", "description": "Last changed on or after this day, YYYY-MM-DD." },
                "modified_to": { "type": "string", "description": "Last changed on or before this day, YYYY-MM-DD." },
                "min_area": { "type": "number", "description": "Roof area at least, m²." },
                "max_area": { "type": "number", "description": "Roof area at most, m²." },
                "sort": { "type": "string", "enum": ["modified", "name", "number", "customer", "area"] },
                "desc": { "type": "boolean", "description": "Descending; default true for modified and area, false otherwise." },
                "limit": { "type": "integer", "minimum": 1 }
            }), &[]),
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
            "description": "Replace the roof windows, chimneys and/or roof openings of a design (omit a list to keep it). Windows deduct width × height from their roof face; a window larger than what is left of its face is refused (check cm vs m and width vs height). Chimneys deduct their plan area / cos(pitch) and add wall-abutment lines WV/WH/WS, as in DACAD; roof openings (holes) deduct like chimneys but their sides become an eave (uphill), a Pult (downhill) and verges.",
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
            "description": "Compute the roof: area, ridge height and line lengths by DACAD code (T eave, F ridge, G hip, K valley, OL/OR/OS verge left/right/oblique, P shed top edge, c pitch break), per part and in total; a part's line lengths are net of deductions like the totals. Each part may carry `warnings` (`kind`, `value`, `message`, and `edges`, `dormer` or `item` where they apply): `outside_outline`, `overlapping_faces`, `area_mismatch` (the roof looks geometrically wrong), `merged_walls` (two walls in line calculated as one, so one wall's pitch is not used), `covered` (no roof of its own: another part covers it; move or delete one), `cut_exceeds_face` (a face lost more than it has, e.g. a window larger than its face), `dormer_off_roof` (a dormer on no roof face, left out of the result), `window_off_roof`, `chimney_off_roof`, `opening_off_roof` (that object, `item` index, on no roof face, left out) and `not_calculated_by_core` (figures from the fallback engine). Tell the user about every warning. A part the calculation gave no roof for carries `error` (`kind`, `message`). `joined_with` lists the parts (`part` index, `name`) the calculation actually joined this part's roof with.",
            "inputSchema": schema(json!({ "id": { "type": "string" } }), &["id"]),
        },
        {
            "name": "calc_breakdown",
            "description": "How the roof's quantities were calculated, step by step, as the app's Calculation tab shows it: per part the walls entered (length, pitch, eave height, overhang, material), every roof face as plan_area / cos(pitch) = gross, minus windows/chimneys = area (`engine_gross` is set where the core's own area differs from that formula by over 1 dm²), clad gables and openings; area per material with the faces it sums; every line by code as sqrt(dx²+dy²+dz²), minus deductions = total. Use it to explain or check a result for the user. `part` (index from 0) limits the parts listed; totals stay for the whole roof.",
            "inputSchema": schema(json!({
                "id": { "type": "string" },
                "part": { "type": "integer", "minimum": 0, "description": "Only this part's walls, faces and lines." }
            }), &["id"]),
        },
        {
            "name": "show_project",
            "description": "Bring a design to the front in the user's Tectora app.",
            "inputSchema": schema(json!({ "id": { "type": "string" } }), &["id"]),
        },
        {
            "name": "revert_last_change",
            "description": "Undo the latest recorded change of a design.",
            "inputSchema": schema(json!({ "id": { "type": "string" }, "summary": summary_prop() }), &["id", "summary"]),
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

    /// Tells the app what the agent does now: `kind` (one of
    /// `store::LIVE_KINDS`, translated in the app) about `name` (the job or
    /// file), with `message` in English for logs and older apps. The app
    /// shows it and never opens the job for it (reading, calculating, exporting).
    fn live(&self, kind: &str, name: Option<&str>, message: String, project_id: Option<&str>) {
        self.publish(kind, name, message, project_id, false);
    }

    /// Publishes a write to (or `show_project` of) a job: the app opens it
    /// in follow mode when the user is not in the middle of something.
    fn live_follow(&self, kind: &str, name: Option<&str>, message: String, project_id: &str) {
        self.publish(kind, name, message, Some(project_id), true);
    }

    fn publish(
        &self,
        kind: &str,
        name: Option<&str>,
        message: String,
        project_id: Option<&str>,
        follow: bool,
    ) {
        debug_assert!(tectora_core::store::LIVE_KINDS.contains(&kind), "{kind}");
        self.store.set_live(&LiveStatus {
            ts_ms: now_ms(),
            actor: self.client.clone(),
            message,
            project_id: project_id.map(str::to_string),
            follow,
            kind: Some(kind.to_string()),
            name: name.map(str::to_string),
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
        let (id, before, project) = match (id, before) {
            (Some(id), Some(before)) => self.save_onto_current(id, before, project, summary)?,
            _ => {
                let id =
                    self.store
                        .save_as(id.map(str::to_string), project, &self.client, summary)?;
                (id, before.cloned(), project.clone())
            }
        };
        let project = &project;
        self.live_follow("changed", None, summary.to_string(), &id);
        let changes = before
            .as_ref()
            .map(|b| diff(b, project))
            .unwrap_or_default();
        Ok(json!({
            "id": id,
            "saved": true,
            "note": "Saved. The change is now visible in the Tectora app for the user to review.",
            "changes": changes,
            "quantities": quantities(project),
            "parts": project.parts.iter().map(describe_part).collect::<Vec<_>>(),
        }))
    }

    /// Saves a change made on `before` (the job as the tool loaded it). When
    /// the app saved the user's edit to the job meanwhile, the change is
    /// merged onto that version instead of writing over it; where both
    /// changed the same value nothing is saved and the AI is told to load
    /// the job again. Returns the id, the version the change was saved onto
    /// and the saved job.
    fn save_onto_current(
        &self,
        id: &str,
        before: &Project,
        project: &Project,
        summary: &str,
    ) -> Result<(String, Option<Project>, Project), String> {
        let mut base = before.clone();
        let mut next = project.clone();
        for _ in 0..5 {
            match self
                .store
                .save_as_checked(id, &next, &self.client, summary, Some(&base))?
            {
                CheckedSave::Saved { id } => return Ok((id, Some(base), next)),
                CheckedSave::Conflict { current } => {
                    let value = |p: &Project| serde_json::to_value(p).map_err(|e| e.to_string());
                    let merged = tectora_core::merge::merge3(
                        &value(&base)?,
                        &value(&next)?,
                        &value(&current)?,
                    )
                    .map_err(|paths| {
                        format!(
                            "design `{id}` was changed in the app while this change was made, \
                             at the same values ({}); nothing was saved. Load it again \
                             (get_project) and repeat the change.",
                            paths.join(", ")
                        )
                    })?;
                    next = serde_json::from_value(merged).map_err(|e| e.to_string())?;
                    next.check_limits()?;
                    base = *current;
                }
            }
        }
        Err(format!(
            "design `{id}` keeps changing in the app; nothing was saved. Try again in a moment."
        ))
    }

    fn load(&self, args: &Value) -> Result<(String, Project), String> {
        let id = arg_str(args, "id")?.to_string();
        // The library's own reason (damaged, newer, locked) with its code,
        // so the AI can tell the user why instead of calling it missing.
        let project = self.store.load(&id).map_err(|e| match e {
            StoreError::NotFound(_) | StoreError::InvalidId(_) => {
                format!("no design with id `{id}`; call list_projects to see the ids")
            }
            e => format!(
                "design `{id}` cannot be opened ({}): {e}; ask the user to check it in the app",
                e.code()
            ),
        })?;
        project.check_limits().map_err(|e| {
            format!("design `{id}` cannot be calculated: {e}; ask the user to fix it in the app")
        })?;
        Ok((id, project))
    }

    /// Writes the settings file; when this server uses the library folder
    /// from the settings and that folder changed, it switches to the new one.
    fn update_settings(&mut self, args: &Value) -> ToolResult {
        let summary = arg_str(args, "summary")?.to_string();
        // Everything but the summary is a setting.
        let mut patch = args.clone();
        if let Some(o) = patch.as_object_mut() {
            o.remove("summary");
        }
        let old = Settings::load();
        let (next, asked) = old
            .patched_by_ai(&patch, &self.client, &summary)
            .map_err(|e| format!("settings not saved: {e}"))?;
        next.save()
            .map_err(|e| format!("settings not saved: {e}"))?;
        self.live(
            "settings",
            None,
            format!("Changed Tectora settings: {summary}"),
            None,
        );
        let mut out = next.describe();
        if asked {
            // The folders change only when the user allows it in the app
            // (ARCH-052); this server keeps its library until then.
            out["note"] = json!(
                "The folder change waits for the user: the app asks them to allow it. Until then the jobs folder and the drawings inbox stay as they are; call get_settings later to see the answer."
            );
        }
        Ok(out)
    }

    /// Runs a tool; a panic in the geometry kernel becomes a tool error
    /// instead of ending the server for the rest of the session.
    pub fn call_tool(&mut self, name: &str, args: &Value) -> ToolResult {
        tectora_core::model::guarded(|| self.call_tool_unguarded(name, args))?
    }

    fn call_tool_unguarded(&mut self, name: &str, args: &Value) -> ToolResult {
        // Every write is shown to the user with its summary (AGENTS.md):
        // a blank one is refused before anything is done.
        if args
            .get("summary")
            .and_then(Value::as_str)
            .is_some_and(|s| s.trim().is_empty())
        {
            return Err("`summary` is empty: give one sentence for the user on what you change and why (it is shown in the app)".into());
        }
        // A misspelled key used to be ignored and the tool still answered
        // "Saved" with nothing changed (ERR-008): refuse keys the tool's
        // schema does not name.
        if let Some(schema) = tools()
            .as_array()
            .and_then(|a| a.iter().find(|t| t["name"] == name))
            .map(|t| t["inputSchema"].clone())
        {
            let mut unknown = Vec::new();
            unknown_keys(&schema, args, "", &mut unknown);
            if !unknown.is_empty() {
                return Err(format!(
                    "unknown argument {} for `{name}`: check the spelling against the tool's input schema",
                    unknown.join(", ")
                ));
            }
        }
        if let Some(r) = self.call_calc_isolated(name, args) {
            return r;
        }
        match name {
            "tectora_guide" => Ok(json!(GUIDE)),
            "get_settings" => Ok(Settings::load().describe()),
            "update_settings" => self.update_settings(args),
            "list_projects" => {
                self.live("browsing", None, "Browsing the design library".into(), None);
                let mut q: tectora_core::library::Query = serde_json::from_value(args.clone())
                    .map_err(|e| format!("invalid search: {e}"))?;
                if let Some(text) = args.get("query").and_then(Value::as_str) {
                    q.text = text.to_string();
                }
                let (all, broken) = self.store.scan()?;
                let list: Vec<Value> = tectora_core::library::search(all, &q)?
                    .into_iter()
                    .map(|p| {
                        json!({
                            "id": p.id, "name": p.name, "job": p.job,
                            "roof_area_m2": round(p.roof_area, 2), "parts": p.parts,
                            "pitch_range_deg": p.pitch_range.map(|[a, b]| [round(a, 2), round(b, 2)]),
                            "dormers": p.dormers, "files": p.files, "has_result": p.has_result,
                            "modified_ms": p.modified_ms, "last_change": p.last_activity,
                        })
                    })
                    .collect();
                // Files the library cannot read, as the app's library lists
                // them: the user should hear about them, not miss them.
                let broken: Vec<Value> = broken
                    .iter()
                    .map(|b| json!({ "id": b.id, "code": b.code, "detail": b.detail }))
                    .collect();
                Ok(json!({ "projects": list, "broken": broken }))
            }
            "get_project" => {
                let (id, p) = self.load(args)?;
                self.live(
                    "reading",
                    Some(&p.name),
                    format!("Reading “{}”", p.name),
                    Some(&id),
                );
                let host = if p.dormers.is_empty() {
                    Vec::new()
                } else {
                    tectora_core::edit::dormers::host_roof(&p)
                };
                let dormers: Vec<Value> = p
                    .dormers
                    .iter()
                    .enumerate()
                    .map(|(i, d)| more::describe_dormer(i, d, &host))
                    .collect();
                Ok(json!({
                    "id": id,
                    "name": p.name,
                    "notes": p.notes,
                    "job": p.job,
                    "parts": p.parts.iter().map(describe_part).collect::<Vec<_>>(),
                    "chimneys": p.chimneys,
                    "windows": p.windows,
                    "roof_openings": p.roof_openings,
                    "dormers": dormers,
                    // Each object's distances from the eave and the sides of
                    // its face, as place_item takes them.
                    "placements": tectora_core::edit::position::placements(&p),
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
                self.live(
                    "calculating",
                    Some(&p.name),
                    format!("Calculating “{}”", p.name),
                    Some(&id),
                );
                let parts: Vec<Value> = p
                    .parts
                    .iter()
                    .zip(tectora_core::openings::generate_project(&p))
                    .map(|(part, geo)| {
                        // Net of the core's negative lines, as the totals.
                        let q = tectora_core::quantities::part_quantities(&geo);
                        let mut v = json!({ "name": part.name, "quantities": q });
                        if !geo.warnings.is_empty() {
                            v["warnings"] = geo.warnings.iter().map(warning).collect();
                        }
                        if let Some(e) = &geo.error {
                            v["error"] = calc_error(e);
                        }
                        if !geo.joined_with.is_empty() {
                            v["joined_with"] = geo
                                .joined_with
                                .iter()
                                .map(|&j| json!({ "part": j, "name": p.parts.get(j).map(|x| x.name.as_str()) }))
                                .collect();
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
            "calc_breakdown" => {
                let (id, p) = self.load(args)?;
                let only = match args.get("part") {
                    None | Some(Value::Null) => None,
                    Some(v) => {
                        let i = v.as_u64().ok_or("`part` must be a part index from 0")? as usize;
                        if i >= p.parts.len() {
                            return Err(format!(
                                "`part` {i} out of range: the design has {} parts",
                                p.parts.len()
                            ));
                        }
                        Some(i)
                    }
                };
                self.live(
                    "explaining",
                    Some(&p.name),
                    format!("Explaining the calculation of “{}”", p.name),
                    Some(&id),
                );
                let b = tectora_core::breakdown::breakdown(&p);
                let mut v = serde_json::to_value(&b).map_err(|e| e.to_string())?;
                if let Some(i) = only {
                    v["parts"] = json!([{ "index": i, "part": v["parts"][i].take() }]);
                    for g in v["lines"].as_array_mut().into_iter().flatten() {
                        if let Some(items) = g["items"].as_array_mut() {
                            items.retain(|l| l["part"].as_u64() == Some(i as u64));
                        }
                    }
                }
                v["id"] = json!(id);
                Ok(v)
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
                let pi = part_arg(args)?;
                let part = p
                    .parts
                    .get_mut(pi)
                    .ok_or_else(|| format!("part {pi} does not exist"))?;
                let changes = arg(args, "changes")?
                    .as_array()
                    .ok_or("`changes` must be a list")?;
                // A wall's eave is not the building height: the building
                // keeps the height it shows (RoofPart::freeze_height).
                if changes.iter().any(|c| c.get("eave_height").is_some()) {
                    part.freeze_height();
                }
                for c in changes {
                    // As the editor's "apply to all walls": a pitch for every
                    // wall leaves gables and direction changes as they are,
                    // unless the same change turns the gables into eaves
                    // (then they take that pitch, ERR-020).
                    let all = c.get("all").and_then(Value::as_bool) == Some(true);
                    let to_eave = c.get("gable").and_then(Value::as_bool) == Some(false);
                    let mut no_pitch = c.clone();
                    if let Some(o) = no_pitch.as_object_mut() {
                        o.remove("pitch_deg");
                    }
                    for i in edge_indices(c, part.edges.len())? {
                        let e = &part.edges[i];
                        let keep = all && ((e.is_gable() && !to_eave) || e.direction_rad.is_some());
                        apply_edge_fields(&mut part.edges[i], if keep { &no_pitch } else { c })
                            .map_err(|m| format!("edge {i}: {m}"))?;
                    }
                }
                self.commit(Some(&id), Some(&before), &p, arg_str(args, "summary")?)
            }
            "set_outline" => {
                let (id, before) = self.load(args)?;
                let mut p = before.clone();
                let pi = part_arg(args)?;
                let old = p
                    .parts
                    .get(pi)
                    .ok_or_else(|| format!("part {pi} does not exist"))?
                    .clone();
                let points = parse_points(arg(args, "points")?)?;
                // A part stored without walls (ERR-010) takes the new-roof
                // defaults, as `add_part` does.
                let mut template = match old.edges.last() {
                    Some(e) => e.clone(),
                    None => default_edge(&json!({}))?,
                };
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
                    height: None,
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
                    let status = parse_status(s)?;
                    // Leaving the draft stores the result as the offer's
                    // figures: refused for a roof that cannot be calculated.
                    if p.job.status == JobStatus::Draft
                        && status != JobStatus::Draft
                        && p.result_snapshot.is_none()
                    {
                        p.result_snapshot = Some(
                            tectora_core::snapshot::take_checked(&p).map_err(|e| e.to_string())?,
                        );
                    }
                    p.set_status(status);
                }
                self.commit(Some(&id), Some(&before), &p, arg_str(args, "summary")?)
            }
            "set_openings" => {
                let (id, before) = self.load(args)?;
                let mut p = before.clone();
                if let Some(w) = args.get("windows") {
                    p.windows = serde_json::from_value::<Vec<RoofWindow>>(w.clone())
                        .map_err(|e| format!("windows: {e}"))?;
                    // A window larger than its face makes the face area
                    // negative: refused, as in the editor.
                    tectora_core::edit::check_windows(&p).map_err(|e| e.to_string())?;
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
                self.live_follow(
                    "showing",
                    Some(&p.name),
                    format!("Showing “{}”", p.name),
                    &id,
                );
                Ok(json!({ "id": id, "shown": true }))
            }
            "revert_last_change" => {
                let (id, _) = self.load(args)?;
                let summary = arg_str(args, "summary")?;
                let p = self.store.revert_last(&id, &self.client, Some(summary))?;
                self.live_follow(
                    "reverted",
                    Some(&p.name),
                    format!("Reverted the latest change: {summary}"),
                    &id,
                );
                Ok(json!({ "id": id, "reverted": true, "quantities": quantities(&p) }))
            }
            _ => self
                .call_more(name, args)
                .or_else(|| self.call_drawing_isolated(name, args))
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
    if worker::run_if_worker() {
        return Ok(());
    }
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
        // Annotations let clients group the permission prompt: reads can be
        // allowed once, every write keeps its summary.
        let tool = |n: &str| {
            r["result"]["tools"]
                .as_array()
                .unwrap()
                .iter()
                .find(|t| t["name"] == n)
                .unwrap()["annotations"]
                .clone()
        };
        assert_eq!(tool("calc")["readOnlyHint"], true);
        assert_eq!(tool("list_projects")["readOnlyHint"], true);
        assert_eq!(tool("update_edges")["readOnlyHint"], false);
        assert_eq!(tool("update_edges")["destructiveHint"], false);
        assert_eq!(tool("delete_project")["destructiveHint"], true);
        assert!(
            r["result"]["tools"]
                .as_array()
                .unwrap()
                .iter()
                .all(|t| t["annotations"]["openWorldHint"] == false)
        );
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
        let live = s.store.live().unwrap();
        assert_eq!(live.project_id.as_deref(), Some(id.as_str()));
        assert!(live.follow, "a write may be followed in the app");
        // Reading or calculating a job never makes the app switch to it (UX-001).
        for tool in ["get_project", "calc", "calc_breakdown"] {
            s.call_tool(tool, &json!({ "id": id })).unwrap();
            let live = s.store.live().unwrap();
            assert_eq!(live.project_id.as_deref(), Some(id.as_str()), "{tool}");
            assert!(!live.follow, "{tool} must not be followed");
        }
        s.call_tool("show_project", &json!({ "id": id })).unwrap();
        assert!(s.store.live().unwrap().follow, "show_project is followed");

        s.call_tool(
            "revert_last_change",
            &json!({ "id": id, "summary": "The west wall is not a gable" }),
        )
        .unwrap();
        let last = s.store.activity(&id).pop().unwrap();
        // The fixed sentence the app translates, then the AI's reason (UX-002).
        assert_eq!(
            last.summary,
            "Reverted: Gable at the west (reason: The west wall is not a gable)"
        );
        assert!(!s.store.load(&id).unwrap().parts[0].edges[5].is_gable());
        let _ = std::fs::remove_dir_all(&s.store.dir);
    }

    #[test]
    fn settings_read_and_update() {
        let _settings = SETTINGS_FILE.lock().unwrap_or_else(|e| e.into_inner());
        let base = std::env::temp_dir().join(format!("tectora-mcp-cfg-{}", std::process::id()));
        // Tests touching the settings file hold SETTINGS_FILE, so the env var is safe here.
        unsafe { std::env::set_var("TECTORA_CONFIG_DIR", base.join("cfg")) };
        let mut s = server();
        let r = s.call_tool("get_settings", &json!({})).unwrap();
        assert_eq!(r["settings"]["theme"], "system");
        let inbox = base.join("inbox");
        let r = s
            .call_tool(
                "update_settings",
                &json!({ "drawings_dir": inbox.to_string_lossy(), "roof_defaults": { "pitch_deg": 35 }, "summary": "Drawings inbox as the user asked" }),
            )
            .unwrap();
        // The folder waits for the user (ARCH-052, Jakub 2026-11-10: the app
        // asks); the other keys apply at once.
        assert_ne!(r["effective"]["drawings_dir"], json!(inbox));
        assert_eq!(
            r["settings"]["folder_request"]["changes"]["drawings_dir"],
            json!(inbox.to_string_lossy())
        );
        assert!(r["note"].as_str().unwrap().contains("waits for the user"));
        assert_eq!(r["settings"]["roof_defaults"]["pitch_deg"], 35.0);
        Settings::load()
            .answer_folder_request(true)
            .unwrap()
            .save()
            .unwrap();
        let r = s.call_tool("get_settings", &json!({})).unwrap();
        assert_eq!(r["effective"]["drawings_dir"], json!(inbox));
        assert!(r["settings"].get("folder_request").is_none());
        assert!(
            s.call_tool(
                "update_settings",
                &json!({ "locale": "fr", "summary": "s" })
            )
            .is_err()
        );
        // Like every write, a settings change carries a summary (ARCH-005).
        assert!(
            s.call_tool("update_settings", &json!({ "theme": "dark" }))
                .is_err()
        );
        let r = s.call_tool("get_settings", &json!({})).unwrap();
        assert_eq!(r["settings"]["roof_defaults"]["pitch_deg"], 35.0);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_tool_change_does_not_overwrite_a_user_edit_the_app_saved_meanwhile() {
        // The app autosaves the user's edit after the tool loaded the job
        // and before it saves: both stay.
        let s = server();
        let p = tectora_core::examples::find("hip").unwrap().project();
        let id = s.store.save(None, &p).unwrap();
        let before = s.store.load(&id).unwrap();
        let mut user = before.clone();
        user.parts[0].edges[0].overhang = 0.77;
        s.store.save(Some(id.clone()), &user).unwrap();
        let mut ai = before.clone();
        ai.parts[0].edges[2].pitch_deg = 41.0;
        let r = s.commit(Some(&id), Some(&before), &ai, "AI B").unwrap();
        let saved = s.store.load(&id).unwrap();
        assert_eq!(saved.parts[0].edges[0].overhang, 0.77, "user edit kept");
        assert_eq!(saved.parts[0].edges[2].pitch_deg, 41.0, "AI change saved");
        // The reply and the log name only the AI's own change.
        assert_eq!(r["changes"].as_array().unwrap().len(), 1, "{r}");
        assert_eq!(s.store.activity(&id)[0].before.as_ref(), Some(&user));

        // Both changed the same value: nothing saved, the AI is told why.
        let mut user2 = saved.clone();
        user2.parts[0].edges[2].pitch_deg = 30.0;
        s.store.save(Some(id.clone()), &user2).unwrap();
        let mut ai2 = saved.clone();
        ai2.parts[0].edges[2].pitch_deg = 45.0;
        let e = s.commit(Some(&id), Some(&saved), &ai2, "AI C").unwrap_err();
        assert!(e.contains("changed in the app"), "{e}");
        assert_eq!(s.store.load(&id).unwrap(), user2);
        assert_eq!(s.store.activity(&id).len(), 1);
        let _ = std::fs::remove_dir_all(&s.store.dir);
    }

    #[test]
    fn calc_breakdown_explains_every_face_and_line() {
        let mut s = server();
        let p = tectora_core::examples::find("l-shape").unwrap().project();
        let id = s.store.save(None, &p).unwrap();
        let total = s.call_tool("calc", &json!({ "id": id })).unwrap();
        let r = s.call_tool("calc_breakdown", &json!({ "id": id })).unwrap();
        let area = r["roof_area"].as_f64().unwrap();
        assert!(
            (area - total["total"]["roof_area_m2"].as_f64().unwrap()).abs() < 0.01,
            "{r}"
        );
        let f = &r["parts"][0]["faces"][0];
        let k = 1.0 / f["pitch_deg"].as_f64().unwrap().to_radians().cos();
        assert!((f["factor"].as_f64().unwrap() - k).abs() < 1e-9);
        assert!(
            r["lines"]
                .as_array()
                .unwrap()
                .iter()
                .any(|g| g["code"] == "T")
        );

        let one = s
            .call_tool("calc_breakdown", &json!({ "id": id, "part": 0 }))
            .unwrap();
        assert_eq!(one["parts"].as_array().unwrap().len(), 1);
        assert_eq!(one["parts"][0]["index"], 0);
        assert!(
            s.call_tool("calc_breakdown", &json!({ "id": id, "part": 9 }))
                .is_err()
        );
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

    fn gable_job(s: &mut Server, pitch: f64) -> String {
        let r = s
            .call_tool(
                "create_project",
                &json!({
                    "name": "gable", "summary": "Footprint",
                    "parts": [{
                        "points": [[0, 0], [12, 0], [12, 8], [0, 8]],
                        "defaults": { "pitch_deg": pitch, "eave_height": 3, "overhang": 0 },
                        "edges": [{}, { "gable": true }, {}, { "gable": true }]
                    }]
                }),
            )
            .unwrap();
        r["id"].as_str().unwrap().to_string()
    }

    #[test]
    fn live_status_is_a_code_and_a_name_the_app_translates() {
        // UX-002: the app shows the sentence in the user's language.
        let mut s = server();
        let id = gable_job(&mut s, 40.0);
        let live = s.store.live().unwrap();
        assert_eq!(
            (live.kind.as_deref(), live.message.as_str()),
            (Some("changed"), "Footprint")
        );
        s.call_tool("calc", &json!({ "id": id })).unwrap();
        let live = s.store.live().unwrap();
        assert_eq!(live.kind.as_deref(), Some("calculating"));
        assert_eq!(live.name.as_deref(), Some("gable"));
        assert_eq!(live.project_id.as_deref(), Some(id.as_str()));
        s.call_tool("list_projects", &json!({})).unwrap();
        let live = s.store.live().unwrap();
        assert_eq!(
            (live.kind.as_deref(), live.project_id),
            (Some("browsing"), None)
        );
        let _ = std::fs::remove_dir_all(&s.store.dir);
    }

    #[test]
    fn edge_input_outside_dacad_limits_is_refused() {
        // DACAD: pitch 2-178°, eave height >= 0, no pitch break below the
        // eave or twice at one height.
        let mut s = server();
        let part = |defaults: Value, edges: Value| {
            json!({ "name": "limits", "summary": "Footprint",
                "parts": [{ "points": [[0, 0], [12, 0], [12, 8], [0, 8]], "defaults": defaults, "edges": edges }] })
        };
        let none = json!([]);
        let cases = [
            (part(json!({ "pitch_deg": 1 }), none.clone()), "pitch_range"),
            (
                part(json!({ "pitch_deg": 40, "eave_height": -1 }), none.clone()),
                "eave_negative",
            ),
            (
                part(
                    json!({ "pitch_deg": 40, "eave_height": 3 }),
                    json!([{ "breaks": [{ "height": 2, "pitch_deg": 30 }] }, {}, {}, {}]),
                ),
                "break_below_eave",
            ),
            (
                part(
                    json!({ "pitch_deg": 40, "eave_height": 3 }),
                    json!([{ "breaks": [{ "height": 5, "pitch_deg": 30 }, { "height": 5, "pitch_deg": 20 }] }, {}, {}, {}]),
                ),
                "break_duplicate",
            ),
        ];
        for (args, code) in cases {
            let e = s.call_tool("create_project", &args).unwrap_err();
            assert!(e.contains(code), "{code}: {e}");
        }
        let ok = s
            .call_tool(
                "create_project",
                &part(json!({ "pitch_deg": 2, "eave_height": 0 }), none),
            )
            .unwrap();
        assert!(ok["id"].is_string(), "{ok}");
        let _ = std::fs::remove_dir_all(&s.store.dir);
    }

    #[test]
    fn calc_reports_a_roof_the_calculation_refused() {
        // MATH-003: a roof the core cannot close ends in its open-faces
        // error. (A 1° hip used to be the example; DACAD refuses pitches
        // under 2°, so now it never reaches the core.) Here one wall's eave
        // lies far above the ridge of the others.
        let mut s = server();
        let r = s
            .call_tool(
                "create_project",
                &json!({
                    "name": "flat hip", "summary": "Footprint",
                    "parts": [{ "points": [[0, 0], [12, 0], [12, 8], [0, 8]],
                        "defaults": { "pitch_deg": 40, "eave_height": 3, "overhang": 0.5 },
                        "edges": [{}, {}, {}, { "eave_height": 20 }] }]
                }),
            )
            .unwrap();
        let id = r["id"].as_str().unwrap().to_string();
        let r = s.call_tool("calc", &json!({ "id": id })).unwrap();
        let e = &r["parts"][0]["error"];
        assert_eq!(e["kind"], "open_faces", "{r}");
        let msg = e["message"].as_str().unwrap();
        assert!(msg.contains("cannot be calculated"), "{msg}");
        // No German text of the calculation reaches the AI.
        let text = r.to_string();
        for german in ["Fläche", "Nicht", "DACAD", "E0000"] {
            assert!(!text.contains(german), "{german} in {text}");
        }
        let _ = std::fs::remove_dir_all(&s.store.dir);
    }

    #[test]
    fn set_openings_refuses_a_window_larger_than_its_face() {
        // MATH-001: 10 × 9 m on a face of 12 × 5.22 m.
        let mut s = server();
        let id = gable_job(&mut s, 40.0);
        let err = s
            .call_tool(
                "set_openings",
                &json!({ "id": id, "summary": "Window", "windows": [{ "x": 1, "y": 0, "width": 10, "height": 9 }] }),
            )
            .unwrap_err();
        assert!(
            err.contains("larger than what is left of the roof face"),
            "{err}"
        );
        assert!(s.store.load(&id).unwrap().windows.is_empty());
        s.call_tool(
            "set_openings",
            &json!({ "id": id, "summary": "Window", "windows": [{ "x": 1, "y": 0, "width": 0.78, "height": 1.18 }] }),
        )
        .unwrap();
        assert_eq!(s.store.load(&id).unwrap().windows.len(), 1);
        let _ = std::fs::remove_dir_all(&s.store.dir);
    }

    #[test]
    fn calc_warns_about_a_face_that_lost_more_than_it_has() {
        // A file with an oversize window (written before the check, or by
        // hand) still calculates as DACAD books it, with a warning.
        let mut s = server();
        let id = gable_job(&mut s, 40.0);
        let mut p = s.store.load(&id).unwrap();
        p.windows.push(RoofWindow {
            x: 1.0,
            y: 0.0,
            width: 10.0,
            height: 9.0,
        });
        s.store.save(Some(id.clone()), &p).unwrap();
        let r = s.call_tool("calc", &json!({ "id": id })).unwrap();
        let w = r["parts"][0]["warnings"].as_array().unwrap();
        assert!(
            w.iter()
                .any(|w| w["kind"] == "cut_exceeds_face" && w["edges"] == json!([0])),
            "{r}"
        );
        let _ = std::fs::remove_dir_all(&s.store.dir);
    }

    #[test]
    fn calc_warns_about_a_window_on_no_roof_face() {
        // Re-check of ERR-011: the window was saved and left out silently.
        let mut s = server();
        let id = gable_job(&mut s, 40.0);
        let area = s.call_tool("calc", &json!({ "id": id })).unwrap()["quantities"]["roof_area_m2"]
            .clone();
        s.call_tool(
            "set_openings",
            &json!({ "id": id, "summary": "Window", "windows": [{ "x": -50, "y": -50, "width": 0.78, "height": 1.18 }] }),
        )
        .unwrap();
        let r = s.call_tool("calc", &json!({ "id": id })).unwrap();
        assert_eq!(r["quantities"]["roof_area_m2"], area, "result unchanged");
        let w = &r["parts"][0]["warnings"][0];
        assert_eq!(w["kind"], "window_off_roof", "{r}");
        assert_eq!(w["item"], 0, "{r}");
        assert!(
            w["message"].as_str().unwrap().contains("roof window 0"),
            "{w}"
        );
        let _ = std::fs::remove_dir_all(&s.store.dir);
    }

    #[test]
    fn calc_names_the_parts_a_roof_is_joined_with() {
        // ARCH-008: from the calculation's own join, not a plan guess.
        let mut s = server();
        let r = s
            .call_tool(
                "create_project",
                &json!({
                    "name": "two", "summary": "Footprint",
                    "parts": [
                        { "name": "Main", "points": [[0, 0], [12, 0], [12, 8], [0, 8]] },
                        { "name": "Wing", "points": [[10, 2], [18, 2], [18, 6], [10, 6]] },
                        { "name": "Shed", "points": [[40, 0], [46, 0], [46, 4], [40, 4]] }
                    ]
                }),
            )
            .unwrap();
        let id = r["id"].as_str().unwrap().to_string();
        let r = s.call_tool("calc", &json!({ "id": id })).unwrap();
        assert_eq!(
            r["parts"][0]["joined_with"],
            json!([{ "part": 1, "name": "Wing" }]),
            "{r}"
        );
        assert_eq!(
            r["parts"][1]["joined_with"],
            json!([{ "part": 0, "name": "Main" }])
        );
        assert!(r["parts"][2].get("joined_with").is_none());
        let _ = std::fs::remove_dir_all(&s.store.dir);
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

    /// Tests that read or write the settings file (through
    /// `TECTORA_CONFIG_DIR`) run one at a time, so one never sees the
    /// other's half-written defaults.
    static SETTINGS_FILE: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn set_outline_repairs_a_part_without_edges() {
        // ERR-010: a stored part with no walls (the app flags it
        // `too_few_edges`); the tool meant to repair it must not panic.
        let _settings = SETTINGS_FILE.lock().unwrap_or_else(|e| e.into_inner());
        let mut s = server();
        let mut p = Project::new("Empty");
        p.parts.push(RoofPart {
            height: None,
            name: "A".into(),
            edges: Vec::new(),
        });
        let id = s.store.save(None, &p).unwrap();
        let r = s
            .call_tool(
                "set_outline",
                &json!({ "id": id, "part": 0, "points": [[0, 0], [5, 0], [5, 4], [0, 4]], "summary": "Outline from the drawing" }),
            )
            .unwrap();
        assert_eq!(r["parts"][0]["edges"].as_array().unwrap().len(), 4, "{r}");
        let preset = Settings::load().roof_defaults;
        assert_eq!(r["parts"][0]["edges"][0]["pitch_deg"], preset.pitch_deg);
        let _ = std::fs::remove_dir_all(&s.store.dir);
    }

    impl Server {
        fn list_projects_empty(&self) -> bool {
            self.store.list().is_empty()
        }
    }
}
