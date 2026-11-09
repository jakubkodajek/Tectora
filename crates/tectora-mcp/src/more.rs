//! The tools that give agents every operation of the editor: walls and
//! parts, transforms, dormers, hand-drawn lines and faces, roof options, the
//! calculation helpers, history, the library, attached files, job bundles,
//! the stored result and the file schema. The geometry comes from
//! [`tectora_core::edit`], the same code the app's dialogs use.

use std::path::PathBuf;

use serde_json::{Value, json};
use tectora_core::edit::calc::{self as hc, BuildUpEave, ParallelField, Target};
use tectora_core::edit::dormers::{self, DormerKind, DormerParams};
use tectora_core::edit::probe::{host_height_at, roof_corner_points, roof_height_at, snap_to_roof};
use tectora_core::edit::round::{
    ArcInput, ArcMode, MAX_ROUND_SEGMENTS, arc_for, opposite_wall, span_between,
};
use tectora_core::edit::transform::{
    ItemKind, ItemRef, MAX_COPIES, MAX_POLYGON_SIDES, PlanMap, follow_roof, item_centre,
    offset_wall, part_centre, regular_polygon, transform_items, transform_part,
};
use tectora_core::edit::{EdgePatch, Patches, apply_patches, plain_edge};
use tectora_core::examples::{EXAMPLES, StandardRoof, standard_roof};
use tectora_core::geom::Point2;
use tectora_core::model::{ManualFace, ManualLine};
use tectora_core::{Project, RoofPart, bundle, snapshot};

use super::{Server, ToolResult, arg, arg_f64, arg_str, round, schema, summary_prop};

/// Snap radius to the calculated roof's points, m (as the editor).
const ROOF_SNAP: f64 = 0.25;

fn opt<T: serde::de::DeserializeOwned>(args: &Value, key: &str) -> Result<Option<T>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => serde_json::from_value(v.clone())
            .map(Some)
            .map_err(|e| format!("`{key}`: {e}")),
    }
}

fn req<T: serde::de::DeserializeOwned>(args: &Value, key: &str) -> Result<T, String> {
    opt(args, key)?.ok_or_else(|| format!("missing argument `{key}`"))
}

fn point(args: &Value, key: &str) -> Result<Option<Point2>, String> {
    Ok(opt::<Vec<f64>>(args, key)?.map(|v| {
        Point2::new(
            v.first().copied().unwrap_or(0.0),
            v.get(1).copied().unwrap_or(0.0),
        )
    }))
}

fn part_index(args: &Value, p: &Project) -> Result<usize, String> {
    let pi = args.get("part").and_then(Value::as_u64).unwrap_or(0) as usize;
    if pi >= p.parts.len() {
        return Err(format!(
            "part {pi} does not exist (the design has {} parts)",
            p.parts.len()
        ));
    }
    Ok(pi)
}

fn edge_index(args: &Value, key: &str, n: usize) -> Result<usize, String> {
    let i = args
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("missing argument `{key}`"))? as usize;
    if i >= n {
        return Err(format!(
            "edge {i} does not exist (the part has {n} edges, 0..{})",
            n - 1
        ));
    }
    Ok(i)
}

/// A dormer as the parameters of the dormer dialog.
pub(crate) fn describe_dormer(index: usize, d: &tectora_core::Dormer) -> Value {
    let p = dormers::parse_dormer(d);
    let mut v = json!({
        "index": index,
        "kind": p.kind,
        "start": [p.start.x, p.start.y],
        "end": [d.end.x, d.end.y],
        "width": round(p.width, 3),
        "angle_deg": round(p.angle.to_degrees(), 2),
        "eave_height": p.eave_height,
        "overhang": p.overhang,
    });
    match p.kind {
        DormerKind::Gable | DormerKind::Pointed | DormerKind::Hip | DormerKind::Round => {
            v["pitch_deg"] = json!(p.pitch_deg)
        }
        DormerKind::HalfHip => {
            v["pitch_deg"] = json!(p.pitch_deg);
            v["half_hip_height"] = json!(p.half_hip_height);
            v["half_hip_pitch_deg"] = json!(p.half_hip_pitch_deg);
        }
        DormerKind::Shed => v["front_pitch_deg"] = json!(p.front_pitch_deg),
        DormerKind::Trapezoid => {
            v["pitch_deg"] = json!(p.pitch_deg);
            v["front_pitch_deg"] = json!(p.front_pitch_deg);
        }
        DormerKind::General => v["edges"] = json!(d.edges),
    }
    if p.kind == DormerKind::Round {
        v["segments"] = json!(p.segments);
    }
    if p.material != tectora_core::model::DEFAULT_MATERIAL {
        v["material"] = json!(p.material);
    }
    v
}

fn dormer_props() -> Value {
    json!({
        "kind": { "type": "string", "enum": ["gable", "pointed", "hip", "half_hip", "shed", "trapezoid", "round"], "description": "Dormer type: gable (Satteldach), pointed (Spitzgaube), hip (Walm), half_hip (Krüppelwalm), shed (Schleppgaube), trapezoid, round (Rundgaube)." },
        "start": { "type": "array", "items": { "type": "number" }, "description": "Plan point [x, y] of the front's left corner seen from outside, on the roof." },
        "width": { "type": "number", "description": "Front width, m (default 2)." },
        "angle_deg": { "type": "number", "description": "Direction of the front wall, degrees; the dormer roof rises to its left. Default: along the nearest eave wall." },
        "eave_height": { "type": "number", "description": "Dormer eave height, m. Default: roof height at `start` + 1 m." },
        "pitch_deg": { "type": "number", "description": "Pitch of the cheek planes (gable, pointed, hip, half_hip, trapezoid), default 45." },
        "front_pitch_deg": { "type": "number", "description": "Pitch of the front plane (shed, trapezoid), default 15." },
        "overhang": { "type": "number", "description": "Default 0." },
        "half_hip_height": { "type": "number", "description": "half_hip: height of the clipped part above the dormer eave, default 0.6." },
        "half_hip_pitch_deg": { "type": "number", "description": "half_hip: its pitch, default 60." },
        "segments": { "type": "integer", "description": "round: polygon segments per side; 0 = automatic." },
        "material": { "type": "integer", "minimum": 1, "description": "Roof material number, default 1." }
    })
}

fn apply_dormer_fields(p: &mut DormerParams, args: &Value) -> Result<(), String> {
    if let Some(k) = opt::<DormerKind>(args, "kind")? {
        if k == DormerKind::General {
            return Err(
                "kind must be one of gable, pointed, hip, half_hip, shed, trapezoid, round".into(),
            );
        }
        p.kind = k;
    }
    if let Some(s) = point(args, "start")? {
        p.start = s;
    }
    if let Some(a) = arg_f64(args, "angle_deg")? {
        p.angle = a.to_radians();
    }
    for (key, slot) in [
        ("width", &mut p.width),
        ("eave_height", &mut p.eave_height),
        ("pitch_deg", &mut p.pitch_deg),
        ("front_pitch_deg", &mut p.front_pitch_deg),
        ("overhang", &mut p.overhang),
        ("half_hip_height", &mut p.half_hip_height),
        ("half_hip_pitch_deg", &mut p.half_hip_pitch_deg),
    ] {
        if let Some(v) = arg_f64(args, key)? {
            *slot = v;
        }
    }
    if let Some(s) = opt::<u32>(args, "segments")? {
        if s > MAX_ROUND_SEGMENTS {
            return Err(format!("`segments` must be 0 to {MAX_ROUND_SEGMENTS}"));
        }
        p.segments = s;
    }
    if let Some(m) = opt::<u8>(args, "material")? {
        p.material = m.max(1);
    }
    if p.width.is_nan() || p.width <= 0.0 {
        return Err("width must be positive".into());
    }
    for (name, v) in [
        ("pitch_deg", p.pitch_deg),
        ("front_pitch_deg", p.front_pitch_deg),
    ] {
        if !(0.0 < v && v < 90.0) {
            return Err(format!("{name} must be between 0 and 90"));
        }
    }
    Ok(())
}

fn patches_json(patches: &Patches) -> Value {
    json!(
        patches
            .iter()
            .map(|(k, p)| json!({ "edge": k, "set": p }))
            .collect::<Vec<_>>()
    )
}

fn manual_line(v: &Value, geos: &[tectora_core::roof::RoofGeometry]) -> Result<ManualLine, String> {
    let end = |key: &str| -> Result<[f64; 3], String> {
        let c: Vec<f64> = req(v, key)?;
        match c.as_slice() {
            [x, y, z] => Ok([*x, *y, *z]),
            [x, y] => Ok(snap_to_roof(geos, Point2::new(*x, *y), ROOF_SNAP)),
            _ => Err(format!("`{key}` must be [x, y] or [x, y, z]")),
        }
    };
    let code = v
        .get("code")
        .and_then(Value::as_str)
        .unwrap_or("M")
        .to_string();
    const CODES: [&str; 15] = [
        "M", "T", "F", "G", "K", "P", "OL", "OR", "OS", "c", "S", "WV", "WH", "WS", "X",
    ];
    if !CODES.contains(&code.as_str()) {
        return Err(format!("code `{code}` is not one of {}", CODES.join(", ")));
    }
    Ok(ManualLine {
        a: end("a")?,
        b: end("b")?,
        code,
        length: arg_f64(v, "length")?,
        label: v
            .get("label")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
    })
}

pub(crate) fn tools() -> Vec<Value> {
    let id = json!({ "type": "string" });
    let part = json!({ "type": "integer", "description": "Part index (default 0)." });
    let target = |list: &[&str]| json!({ "type": "string", "enum": list });
    vec![
        json!({
            "name": "edit_walls",
            "description": "Edit the walls of a part step by step, as in the plan editor. Operations, applied in order: move_point {index, x, y}; insert_point {after, x, y} (new corner on wall `after`, which keeps its roof definition); delete_point {index} (a part keeps at least 3 walls); set_length {index, length} (moves the wall's end point along it); insert_direction_change {index} (a zero-length marker wall at the wall's start that turns the roof, DACAD Richtungswechsel); shift_wall {index, distance} (moves the wall parallel, + outward, its neighbours stretched); rename {name}.",
            "inputSchema": schema(json!({
                "id": id, "part": part,
                "operations": { "type": "array", "items": { "type": "object", "properties": {
                    "op": { "type": "string", "enum": ["move_point", "insert_point", "delete_point", "set_length", "insert_direction_change", "shift_wall", "rename"] },
                    "index": { "type": "integer" }, "after": { "type": "integer" },
                    "x": { "type": "number" }, "y": { "type": "number" },
                    "length": { "type": "number" }, "distance": { "type": "number" }, "name": { "type": "string" }
                }, "required": ["op"] } },
                "summary": summary_prop(),
            }), &["id", "operations", "summary"]),
        }),
        json!({
            "name": "transform_part",
            "description": "Move, rotate or mirror a part together with the dormers, chimneys, roof windows, openings and hand-drawn lines and faces on it; with `copies` > 0 the part stays and that many copies are added, each one the transform applied once more (copy, copy-rotate, copy-mirror).",
            "inputSchema": schema(json!({
                "id": id, "part": part,
                "op": { "type": "string", "enum": ["move", "rotate", "mirror"] },
                "dx": { "type": "number" }, "dy": { "type": "number" },
                "centre": { "type": "array", "items": { "type": "number" }, "description": "rotate/mirror: pivot or a point on the mirror line [x, y]; default the part's centre." },
                "deg": { "type": "number", "description": "rotate: angle, counter-clockwise; mirror: direction of the mirror line." },
                "copies": { "type": "integer", "minimum": 0, "maximum": 50 },
                "summary": summary_prop(),
            }), &["id", "op", "summary"]),
        }),
        json!({
            "name": "transform_items",
            "description": "Move, rotate or copy single objects on the roof (not their part): dormers, chimneys, roof windows, roof openings, hand-drawn lines and faces, as the user drags them in the plan or picks Duplicate / Rotate in their context menu. `rotate` turns each item about its own centre unless `centre` is given (a window keeps its anchor and only moves). With `copies` > 0 the items stay and that many copies of each are added, each one the transform applied once more; the reply lists the new indices. On a move, `follow_roof` (default true) keeps chimney tops, dormer eaves and line ends at the same height above the roof.",
            "inputSchema": schema(json!({
                "id": id,
                "items": { "type": "array", "minItems": 1, "items": { "type": "object", "properties": {
                    "kind": { "type": "string", "enum": ["dormer", "chimney", "window", "roof_opening", "manual_line", "manual_face"] },
                    "index": { "type": "integer", "minimum": 0 }
                }, "required": ["kind", "index"] } },
                "op": { "type": "string", "enum": ["move", "rotate", "align"], "description": "align: turn each chimney or roof opening by the smallest angle that lines its first side up with the nearest eave wall (the context menu's Align to wall)." },
                "dx": { "type": "number" }, "dy": { "type": "number" },
                "deg": { "type": "number", "description": "rotate: angle, counter-clockwise." },
                "centre": { "type": "array", "items": { "type": "number" }, "description": "rotate: one pivot [x, y] for all items." },
                "copies": { "type": "integer", "minimum": 0, "maximum": 50 },
                "follow_roof": { "type": "boolean" },
                "summary": summary_prop(),
            }), &["id", "items", "op", "summary"]),
        }),
        json!({
            "name": "add_shape_part",
            "description": "Add a part from a shape: a standard roof on a rectangle (gable, shed, hip, pyramid, half_hip) or a regular polygon with a hip roof on every wall. Without `at`, a standard roof is placed 2 m right of the existing parts (as in the app). With `replace_part` it replaces that part instead.",
            "inputSchema": schema(json!({
                "id": id,
                "shape": { "type": "string", "enum": ["gable", "shed", "hip", "pyramid", "half_hip", "regular_polygon"] },
                "length": { "type": "number" }, "width": { "type": "number" },
                "sides": { "type": "integer", "minimum": 3, "maximum": 64, "description": "regular_polygon: number of walls." },
                "side": { "type": "number", "description": "regular_polygon: wall length, m." },
                "rotation_deg": { "type": "number" },
                "at": { "type": "array", "items": { "type": "number" }, "description": "Standard roof: its first corner; polygon: its centre [x, y]." },
                "pitch_deg": { "type": "number" }, "eave_height": { "type": "number" }, "overhang": { "type": "number" },
                "name": { "type": "string" },
                "replace_part": { "type": "integer" },
                "summary": summary_prop(),
            }), &["id", "shape", "summary"]),
        }),
        json!({
            "name": "delete_items",
            "description": "Delete parts, dormers, chimneys, roof windows, roof openings, hand-drawn lines or faces by index (indices as get_project lists them).",
            "inputSchema": schema(json!({
                "id": id,
                "items": { "type": "array", "items": { "type": "object", "properties": {
                    "kind": { "type": "string", "enum": ["part", "dormer", "chimney", "window", "roof_opening", "manual_line", "manual_face"] },
                    "index": { "type": "integer" }
                }, "required": ["kind", "index"] } },
                "summary": summary_prop(),
            }), &["id", "items", "summary"]),
        }),
        json!({
            "name": "face_to_part",
            "description": "Turn a hand-drawn face (`set_manual` faces, index from 0 as get_project lists them) into a building with the same outline, in one change, as the editor's \"Make it a building\": a hand-drawn face only adds its area and calculates no roof; the new part gets every wall as an eave with `pitch_deg`, `eave_height` and `overhang` (default: the user's roof defaults). Use it when a footprint was traced as a face.",
            "inputSchema": schema(json!({
                "id": id,
                "index": { "type": "integer", "minimum": 0, "description": "The hand-drawn face." },
                "name": { "type": "string", "description": "Name of the new part (default Main for the first part, else Part N)." },
                "pitch_deg": { "type": "number" },
                "eave_height": { "type": "number" },
                "overhang": { "type": "number" },
                "summary": summary_prop(),
            }), &["id", "index", "summary"]),
        }),
        json!({
            "name": "set_part_height",
            "description": "Set the height of a building (part) in one change, as the editor's \"Building height\" field: every wall of the part gets `eave_height` (m above ground); its pitch breaks and half-hips move with it, and so do the dormers, chimneys and hand-drawn lines on it. Where buildings of different heights meet, the roofs are joined by these eave heights, so use it to make a wing lower or higher than the main roof. To change only some walls, use update_edges.",
            "inputSchema": schema(json!({
                "id": id,
                "part": { "type": "integer", "minimum": 0, "description": "Part index (as get_project lists them)." },
                "eave_height": { "type": "number", "minimum": tectora_core::edit::height::MIN_PART_HEIGHT, "maximum": tectora_core::edit::height::MAX_PART_HEIGHT, "description": "Eave height of every wall, m above ground." },
                "summary": summary_prop(),
            }), &["id", "part", "eave_height", "summary"]),
        }),
        json!({
            "name": "clear_roof",
            "description": "Remove everything drawn (parts, dormers, chimneys, roof windows, roof openings, hand-drawn lines and faces) and leave an empty canvas, as the editor's Clear. Job data, notes, roof options and the stored result stay; the change can be reverted like any other.",
            "inputSchema": schema(json!({
                "id": id,
                "summary": summary_prop(),
            }), &["id", "summary"]),
        }),
        json!({
            "name": "add_dormer",
            "description": "Place a dormer on the roof (DACAD Gauben). The front's left corner is `start`; get_project lists dormers with these parameters.",
            "inputSchema": schema({
                let mut p = dormer_props();
                p["id"] = id.clone();
                p["summary"] = summary_prop();
                p
            }, &["id", "kind", "start", "summary"]),
        }),
        json!({
            "name": "update_dormer",
            "description": "Change a dormer: any of the add_dormer parameters; the rest is kept. Changing `kind` converts it.",
            "inputSchema": schema({
                let mut p = dormer_props();
                p["id"] = id.clone();
                p["index"] = json!({ "type": "integer" });
                p["summary"] = summary_prop();
                p
            }, &["id", "index", "summary"]),
        }),
        json!({
            "name": "set_manual",
            "description": "Replace the lines and/or faces drawn by hand on the calculated roof (omit a list to keep it). A line end given as [x, y] snaps to the nearest roof point within 0.25 m, else lies at height 0 (as DACAD); [x, y, z] is taken as it is. A line of a counted code adds to that code's totals; M (dimension) and X do not. A face is a plan polygon with its pitch (default 30°) and adds its sloped area.",
            "inputSchema": schema(json!({
                "id": id,
                "lines": { "type": "array", "items": { "type": "object", "properties": {
                    "a": { "type": "array", "items": { "type": "number" } },
                    "b": { "type": "array", "items": { "type": "number" } },
                    "code": { "type": "string", "enum": ["M", "T", "F", "G", "K", "P", "OL", "OR", "OS", "c", "S", "WV", "WH", "WS", "X"] },
                    "length": { "type": "number", "description": "Length to count instead of the measured one." },
                    "label": { "type": "string" }
                }, "required": ["a", "b"] } },
                "faces": { "type": "array", "items": { "type": "object", "properties": {
                    "points": { "type": "array", "items": { "type": "array", "items": { "type": "number" } } },
                    "pitch_deg": { "type": "number" }, "label": { "type": "string" }
                }, "required": ["points"] } },
                "summary": summary_prop(),
            }), &["id", "summary"]),
        }),
        json!({
            "name": "set_roof_options",
            "description": "Whole-roof options: `flat_roof` (calculate as a flat roof at the highest eave; dormers are left out) and `material_names` (own names of materials 1, 2, …; an empty name keeps the default).",
            "inputSchema": schema(json!({
                "id": id,
                "flat_roof": { "type": "boolean" },
                "material_names": { "type": "array", "items": { "type": "string" } },
                "summary": summary_prop(),
            }), &["id", "summary"]),
        }),
        json!({
            "name": "roof_helper",
            "description": "The calculation helpers of the roof definition, for wall `edge` of a part. Returns the values they would set; with `apply: true` it also saves them. Tools: triangle (pitch from two of width, rafter, ridge height; with breaks, the top break's pitch); parallel (two facing walls meeting in a ridge: type values in `steps` as in the dialog, solve for pitch, eave, overhang or position); point (the plane through a 3D point: `point` [x, y, z] or `point_index` into the roof corner list it returns); merge (walls parallel to this one take its plane; eave, overhang or position); hip (pitches of this wall and its neighbour from a hip or valley length and both rafters or eave pieces); build_up (correct for a build-up thickness); round_roof (this wall and the facing one get the polygon of a circular arc: default, radius, ridge or length).",
            "inputSchema": schema(json!({
                "id": id, "part": part,
                "edge": { "type": "integer" },
                "tool": { "type": "string", "enum": ["triangle", "parallel", "point", "merge", "hip", "build_up", "round_roof"] },
                "target": target(&["pitch", "eave", "overhang", "position"]),
                "width": { "type": "number" }, "rafter": { "type": "number" }, "ridge": { "type": "number" },
                "width_includes_overhang": { "type": "boolean", "description": "triangle (default false) and parallel (default true)." },
                "steps": { "type": "array", "items": { "type": "object", "properties": {
                    "field": { "type": "string", "enum": ["width_a", "width_b", "rafter_a", "rafter_b", "ridge", "top"], "description": "a = this wall, b = the facing wall; top = width of a flat strip between two ridges." },
                    "value": { "type": "number" }
                }, "required": ["field", "value"] } },
                "point": { "type": "array", "items": { "type": "number" } },
                "point_index": { "type": "integer" },
                "walls": { "type": "array", "items": { "type": "integer" }, "description": "merge: walls to merge (default all parallel walls facing the same way)." },
                "neighbour": { "type": "string", "enum": ["next", "prev"] },
                "hip": { "type": "number" },
                "by": { "type": "string", "enum": ["rafters", "eaves"] },
                "first": { "type": "number", "description": "hip: rafter or eave piece of this wall." },
                "second": { "type": "number", "description": "hip: rafter or eave piece of the neighbour." },
                "thickness": { "type": "number" },
                "eave": { "type": "string", "enum": ["normal", "horizontal", "vertical"] },
                "mode": { "type": "string", "enum": ["default", "radius", "ridge", "length"] },
                "value": { "type": "number" },
                "segments": { "type": "integer" },
                "apply": { "type": "boolean" },
                "summary": summary_prop(),
            }), &["id", "edge", "tool"]),
        }),
        json!({
            "name": "get_history",
            "description": "The change log of a design: who changed what and when (ts_ms), newest first.",
            "inputSchema": schema(json!({ "id": id }), &["id"]),
        }),
        json!({
            "name": "get_review",
            "description": "Whether the user has reviewed your changes to a design in the app. `pending` is null when every change is accepted or reverted; otherwise it gives the oldest unreviewed change (`since_ts_ms`), how many there are, who made them and how they change the figures (`differences`: roof_area, ridge_height, line:<code>, material:<n>, before → now). Only the user accepts changes, in the app.",
            "inputSchema": schema(json!({ "id": id }), &["id"]),
        }),
        json!({
            "name": "revert_to",
            "description": "Undo every change of a design made at or after `ts_ms` (from get_history).",
            "inputSchema": schema(json!({ "id": id, "ts_ms": { "type": "integer" }, "summary": summary_prop() }), &["id", "ts_ms", "summary"]),
        }),
        json!({
            "name": "list_examples",
            "description": "The example roofs a new design can start from.",
            "inputSchema": schema(json!({}), &[]),
        }),
        json!({
            "name": "create_from_example",
            "description": "Create a new design from an example roof.",
            "inputSchema": schema(json!({ "example_id": { "type": "string" }, "name": { "type": "string" }, "summary": summary_prop() }), &["example_id", "summary"]),
        }),
        json!({
            "name": "duplicate_project",
            "description": "Copy a design under a new name as a new draft (no stored offer result). Pass `job` to start a new job for another customer from a past roof; attached files are copied unless `with_files` is false.",
            "inputSchema": schema(json!({
                "id": id,
                "name": { "type": "string" },
                "job": {
                    "type": "object",
                    "description": "Job data of the new job; omitted fields are empty. Without `job` the old job data is kept.",
                    "properties": {
                        "number": { "type": "string" }, "customer": { "type": "string" }, "address": { "type": "string" }
                    }
                },
                "with_files": { "type": "boolean", "description": "Copy the attached drawings and photos (default true)." },
                "summary": summary_prop()
            }), &["id", "name", "summary"]),
        }),
        json!({
            "name": "delete_project",
            "description": "Delete a design with its history and attached files. Only when the user asked for it. The job goes to the library's trash (kept 30 days): restore_project with the returned `trash.entry` brings it back.",
            "inputSchema": schema(json!({ "id": id, "summary": summary_prop() }), &["id", "summary"]),
        }),
        json!({
            "name": "list_deleted",
            "description": "Jobs and attached files deleted in the last 30 days (by the user in the app or by an AI client), newest first: `entry` (pass it to restore_project), job id and name, `file` for a removed attachment, who deleted it, when (ts_ms) and why (summary).",
            "inputSchema": schema(json!({}), &[]),
        }),
        json!({
            "name": "restore_project",
            "description": "Bring a deleted job (with its history and files) or a removed attached file back from the trash. A job whose id is taken by now comes back under a new id; returns the id.",
            "inputSchema": schema(json!({ "entry": { "type": "string", "description": "`entry` from list_deleted or from the delete_project / remove_file reply." }, "summary": summary_prop() }), &["entry", "summary"]),
        }),
        json!({
            "name": "list_files",
            "description": "Files attached to a job (drawings, photos), with their paths on this computer.",
            "inputSchema": schema(json!({ "id": id }), &["id"]),
        }),
        json!({
            "name": "attach_file",
            "description": "Attach a file from this computer (e.g. the technical drawing the roof was made from) to a job. Only a regular file up to 256 MB of a drawing, photo or document type (pdf, png, jpg, jpeg, webp, gif, bmp, tif, tiff, heic, svg, txt, csv, dxf, dwg, xlsx, xls, ods, docx, doc, odt). Attach only files the user gave you for this job; the reply names the folder the file came from.",
            "inputSchema": schema(json!({ "id": id, "path": { "type": "string", "description": "Absolute path of the file." }, "summary": summary_prop() }), &["id", "path", "summary"]),
        }),
        json!({
            "name": "remove_file",
            "description": "Remove an attached file from a job. It goes to the library's trash (kept 30 days): restore_project with the returned `trash.entry` brings it back.",
            "inputSchema": schema(json!({ "id": id, "name": { "type": "string" }, "summary": summary_prop() }), &["id", "name", "summary"]),
        }),
        json!({
            "name": "export_job",
            "description": "Write a job to a .tectorajob file (project, attached files, history and a plan preview in one ZIP) in the `exports` folder of the job library. The file is named after the job (or `name`); an existing file is never replaced. Returns the path to give the user.",
            "inputSchema": schema(json!({
                "id": id,
                "name": { "type": "string", "description": "File name without folder, e.g. \"Novak-strecha\"; default: the job's id." },
                "summary": summary_prop(),
            }), &["id", "summary"]),
        }),
        json!({
            "name": "export_list",
            "description": "The roof as the text list TERRAN's Excel price sheet reads (the text the user copies with the copy button in the app's Export dialog): faces with pitch, area and rafter length, wall area and area by material, and line lengths by line type. Read-only. Give it to the user verbatim; do not translate or reformat it, the price sheet parses it.",
            "inputSchema": schema(json!({
                "id": id,
                "stamp": { "type": "string", "description": "Date in the header, e.g. \"08.11.2026 14:30\"; default: now (UTC)." },
            }), &["id"]),
        }),
        json!({
            "name": "export_plan",
            "description": "Write the roof sheet (A4 landscape plan with line lengths, face areas, pitches and totals; the picture the user adds to the Excel price sheet) to the `exports` folder of the job library as PDF or SVG. The file is named after the job (or `name`); an existing file is never replaced. Returns the path to give the user.",
            "inputSchema": schema(json!({
                "id": id,
                "format": { "type": "string", "enum": ["pdf", "svg"], "description": "Default: pdf." },
                "name": { "type": "string", "description": "File name without folder or extension; default: the job's id." },
                "stamp": { "type": "string", "description": "Date on the sheet; default: now (UTC)." },
                "summary": summary_prop(),
            }), &["id", "summary"]),
        }),
        json!({
            "name": "import_job",
            "description": "Add a job from a .tectorajob file, or a .tectora project file from outside the library, as a new design.",
            "inputSchema": schema(json!({ "path": { "type": "string" }, "summary": summary_prop() }), &["path", "summary"]),
        }),
        json!({
            "name": "store_result",
            "description": "Store the current result as the job's result (the figures for the offer), replacing the stored one. The result is stored by itself when the job leaves the draft status.",
            "inputSchema": schema(json!({ "id": id, "summary": summary_prop() }), &["id", "summary"]),
        }),
        json!({
            "name": "get_schema",
            "description": "JSON Schema of the .tectora project file.",
            "inputSchema": schema(json!({}), &[]),
        }),
    ]
}

impl Server {
    /// The tools of this module; `None` for a name it does not know.
    pub(crate) fn call_more(&self, name: &str, args: &Value) -> Option<ToolResult> {
        Some(match name {
            "edit_walls" => self.edit_walls(args),
            "transform_part" => self.transform(args),
            "transform_items" => self.transform_items(args),
            "add_shape_part" => self.add_shape_part(args),
            "delete_items" => self.delete_items(args),
            "face_to_part" => (|| {
                let (id, before) = self.load(args)?;
                let mut p = before.clone();
                let index: usize = req(args, "index")?;
                let preset = tectora_core::settings::Settings::load().roof_defaults;
                let n = p.parts.len();
                let name = args
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| if n == 0 { "Main".into() } else { format!("Part {}", n + 1) });
                tectora_core::edit::face_to_part(
                    &mut p,
                    index,
                    &name,
                    arg_f64(args, "pitch_deg")?.unwrap_or(preset.pitch_deg),
                    arg_f64(args, "eave_height")?.unwrap_or(preset.eave_height),
                    arg_f64(args, "overhang")?.unwrap_or(preset.overhang),
                )?;
                if let Err(e) = p.validate() {
                    return Err(e.join("; "));
                }
                self.commit(Some(&id), Some(&before), &p, arg_str(args, "summary")?)
            })(),
            "set_part_height" => (|| {
                let (id, before) = self.load(args)?;
                let mut p = before.clone();
                let part: usize = req(args, "part")?;
                let h = arg_f64(args, "eave_height")?.ok_or("`eave_height` is required")?;
                if !tectora_core::edit::height::set_part_height(&mut p, part, h)? {
                    return Err(format!("every wall of part {part} already has eave height {h} m"));
                }
                if let Err(e) = p.validate() {
                    return Err(e.join("; "));
                }
                self.commit(Some(&id), Some(&before), &p, arg_str(args, "summary")?)
            })(),
            "clear_roof" => (|| {
                let (id, before) = self.load(args)?;
                let mut p = before.clone();
                if !tectora_core::edit::clear_roof(&mut p) {
                    return Err("the job has nothing drawn to clear".into());
                }
                self.commit(Some(&id), Some(&before), &p, arg_str(args, "summary")?)
            })(),
            "add_dormer" | "update_dormer" => self.dormer(args, name == "add_dormer"),
            "set_manual" => self.set_manual(args),
            "set_roof_options" => self.set_roof_options(args),
            "roof_helper" => self.roof_helper(args),
            "get_history" => self.load(args).map(|(id, _)| {
                let since = self.store.pending_review(&id).map_or(u64::MAX, |r| r.since_ts_ms);
                json!(self
                    .store
                    .activity(&id)
                    .iter()
                    .rev()
                    .map(|e| {
                        let mut v = json!(e.info());
                        v["pending_review"] = json!(e.ts_ms >= since);
                        v
                    })
                    .collect::<Vec<_>>())
            }),
            "get_review" => (|| {
                let (id, _) = self.load(args)?;
                let status = self.store.review_status(&id)?;
                Ok(json!({
                    "id": id,
                    "accepted_ms": self.store.reviewed_at(&id),
                    "pending": status.map(|s| json!({
                        "since_ts_ms": s.pending.since_ts_ms,
                        "count": s.pending.count,
                        "actors": s.pending.actors,
                        "differences": s.differences,
                    })),
                }))
            })(),
            "revert_to" => (|| {
                let (id, _) = self.load(args)?;
                let ts: u64 = req(args, "ts_ms")?;
                let summary = arg_str(args, "summary")?;
                let p = self.store.revert_to(&id, ts, &self.client, Some(summary))?;
                self.live_follow("reverted_changes", Some(&p.name), format!("Reverted changes: {summary}"), &id);
                Ok(json!({ "id": id, "reverted": true, "quantities": super::quantities(&p) }))
            })(),
            "list_examples" => Ok(json!(EXAMPLES
                .iter()
                .map(|e| json!({ "id": e.id, "name": e.name, "description": e.description }))
                .collect::<Vec<_>>())),
            "create_from_example" => (|| {
                let ex_id = arg_str(args, "example_id")?;
                let ex = tectora_core::examples::find(ex_id).ok_or_else(|| format!("no example `{ex_id}`; see list_examples"))?;
                let mut p = ex.project();
                if let Some(n) = args.get("name").and_then(Value::as_str) {
                    p.name = n.to_string();
                }
                self.commit(None, None, &p, arg_str(args, "summary")?)
            })(),
            "duplicate_project" => (|| {
                let (id, mut p) = self.load(args)?;
                let job = match args.get("job") {
                    None | Some(Value::Null) => None,
                    Some(j) => {
                        let field = |k: &str| j.get(k).and_then(Value::as_str).unwrap_or("").to_string();
                        Some(tectora_core::JobInfo {
                            number: field("number"),
                            customer: field("customer"),
                            address: field("address"),
                            status: tectora_core::JobStatus::Draft,
                        })
                    }
                };
                tectora_core::library::prepare_copy(&mut p, arg_str(args, "name")?, job);
                let summary = arg_str(args, "summary")?;
                let new_id = self.store.save_as(None, &p, &self.client, summary)?;
                if args.get("with_files").and_then(Value::as_bool).unwrap_or(true) {
                    self.store.copy_attachments(&id, &new_id)?;
                }
                self.live_follow("copied", Some(&p.name), format!("Copied “{}”", p.name), &new_id);
                Ok(json!({ "id": new_id }))
            })(),
            "delete_project" => (|| {
                let id = arg_str(args, "id")?.to_string();
                let summary = arg_str(args, "summary")?;
                // A damaged job can be deleted too, so it is not loaded first.
                let entry = self.store.delete(&id, &self.client, summary)?;
                self.live("deleted", Some(&entry.name), format!("Deleted “{}”: {summary}", entry.name), None);
                Ok(json!({
                    "id": id, "deleted": true, "trash": entry,
                    "note": "Moved to the library's trash for 30 days; restore_project with trash.entry brings it back.",
                }))
            })(),
            "list_deleted" => Ok(json!(self.store.trash())),
            "restore_project" => (|| {
                let entry = arg_str(args, "entry")?;
                let summary = arg_str(args, "summary")?;
                let id = self.store.restore(entry)?;
                let name = self.store.load(&id).map(|p| p.name).unwrap_or_else(|_| id.clone());
                self.live_follow("restored", Some(&name), format!("Restored “{name}”: {summary}"), &id);
                Ok(json!({ "id": id, "restored": true }))
            })(),
            "list_files" => self.load(args).map(|(id, _)| {
                json!(self
                    .store
                    .attachments(&id)
                    .iter()
                    .map(|a| json!({
                        "name": a.name, "size": a.size, "modified_ms": a.modified_ms,
                        "path": self.store.attachment_path(&id, &a.name).map(|p| p.display().to_string()).unwrap_or_default(),
                    }))
                    .collect::<Vec<_>>())
            }),
            "attach_file" => (|| {
                let (id, _) = self.load(args)?;
                let summary = arg_str(args, "summary")?;
                let src = PathBuf::from(arg_str(args, "path")?);
                if !src.is_absolute() {
                    return Err("`path` must be an absolute path".into());
                }
                // Drawings, photos and documents only: a path from text in a
                // drawing must not copy a key or a program into a job that
                // is later handed over (ARCH-001).
                let name = src.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !tectora_core::store::Store::opens_directly(name) {
                    return Err(format!(
                        "{}: only drawings, photos and documents can be attached (pdf, png, jpg, …, dwg, xlsx, docx)",
                        src.display()
                    ));
                }
                let name = self.store.attach_file(&id, &src)?;
                self.live_follow("attached", Some(&name), format!("Attached {name}: {summary}"), &id);
                let from = src.parent().map(|d| d.display().to_string()).unwrap_or_default();
                Ok(json!({ "id": id, "name": name, "from": from }))
            })(),
            "remove_file" => (|| {
                let (id, _) = self.load(args)?;
                let name = arg_str(args, "name")?;
                let summary = arg_str(args, "summary")?;
                let entry = self.store.remove_attachment(&id, name, &self.client, summary)?;
                self.live_follow("removed", Some(name), format!("Removed {name}: {summary}"), &id);
                Ok(json!({ "id": id, "removed": name, "trash": entry }))
            })(),
            "export_job" => (|| {
                let (id, _) = self.load(args)?;
                let summary = arg_str(args, "summary")?;
                if args.get("path").is_some() {
                    return Err("`path` is not accepted: bundles are written to the library's exports folder; pass `name` for the file name".into());
                }
                let name = match args.get("name").and_then(Value::as_str) {
                    Some(n) => n.trim().trim_end_matches(".tectorajob").to_string(),
                    None => id.clone(),
                };
                let path = bundle::export_path(&self.store, &name)?;
                let m = bundle::export(&self.store, &id, &path)?;
                self.live("exported", Some(&path.display().to_string()), format!("Exported to {}: {summary}", path.display()), Some(&id));
                Ok(json!({ "id": id, "path": path.display().to_string(), "manifest": m }))
            })(),
            "export_list" => (|| {
                let (id, p) = self.load(args)?;
                let stamp = match args.get("stamp").and_then(Value::as_str) {
                    Some(s) if s.chars().count() <= 40 => s.to_string(),
                    Some(_) => return Err("`stamp` is too long".into()),
                    None => tectora_core::export::utc_stamp(tectora_core::store::now_ms()),
                };
                let title = if p.name.is_empty() { id.clone() } else { p.name.clone() };
                let opts = tectora_core::export::ListOptions::new(title, stamp);
                let (list, _) = tectora_core::export::checked(&p, &opts)?;
                Ok(json!({ "id": id, "list": list }))
            })(),
            "export_plan" => (|| {
                let (id, p) = self.load(args)?;
                let summary = arg_str(args, "summary")?;
                if args.get("path").is_some() {
                    return Err("`path` is not accepted: sheets are written to the library's exports folder; pass `name` for the file name".into());
                }
                let format = args.get("format").and_then(Value::as_str).unwrap_or("pdf");
                if format != "pdf" && format != "svg" {
                    return Err(format!("`format` must be pdf or svg, not `{format}`"));
                }
                let stamp = match args.get("stamp").and_then(Value::as_str) {
                    Some(s) if s.chars().count() <= 40 => s.to_string(),
                    Some(_) => return Err("`stamp` is too long".into()),
                    None => tectora_core::export::utc_stamp(tectora_core::store::now_ms()),
                };
                let name = match args.get("name").and_then(Value::as_str) {
                    Some(n) => n.trim().trim_end_matches(&format!(".{format}")).to_string(),
                    None => id.clone(),
                };
                let path = bundle::export_path_with(&self.store, &name, format)?;
                let bytes = tectora_core::export::checked_sheet_file(&p, &stamp, format)?;
                std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
                self.live("exported_sheet", Some(&path.display().to_string()), format!("Exported sheet to {}: {summary}", path.display()), Some(&id));
                Ok(json!({ "id": id, "path": path.display().to_string() }))
            })(),
            "import_job" => (|| {
                let path = PathBuf::from(arg_str(args, "path")?);
                let summary = arg_str(args, "summary")?;
                let id = bundle::open_path(&self.store, &path)?;
                self.live_follow("imported", Some(&path.display().to_string()), format!("Imported {}: {summary}", path.display()), &id);
                Ok(json!({ "id": id }))
            })(),
            "store_result" => (|| {
                let (id, before) = self.load(args)?;
                let mut p = before.clone();
                p.result_snapshot = Some(snapshot::take_checked(&p).map_err(|e| e.to_string())?);
                self.commit(Some(&id), Some(&before), &p, arg_str(args, "summary")?)
            })(),
            "get_schema" => Ok(tectora_core::model::json_schema()),
            _ => return None,
        })
    }

    fn edit_walls(&self, args: &Value) -> ToolResult {
        let (id, before) = self.load(args)?;
        let mut p = before.clone();
        let pi = part_index(args, &p)?;
        let ops = arg(args, "operations")?
            .as_array()
            .ok_or("`operations` must be a list")?;
        for (k, op) in ops.iter().enumerate() {
            let at = |m: String| format!("operation {k}: {m}");
            let part = &mut p.parts[pi];
            let n = part.edges.len();
            let xy = || -> Result<Point2, String> {
                Ok(Point2::new(
                    arg_f64(op, "x")?.ok_or("missing `x`")?,
                    arg_f64(op, "y")?.ok_or("missing `y`")?,
                ))
            };
            match arg_str(op, "op").map_err(at)? {
                "move_point" => {
                    let i = edge_index(op, "index", n).map_err(at)?;
                    let to = xy().map_err(at)?;
                    // As a corner drag in the plan: direction changes on the
                    // corner move with it.
                    for j in corner_group(&part.edges, i) {
                        part.edges[j].start = to;
                    }
                }
                "insert_point" => {
                    let i = edge_index(op, "after", n).map_err(at)?;
                    let mut e = part.edges[i].clone();
                    e.start = xy().map_err(at)?;
                    e.half_hip = None;
                    e.direction_rad = None;
                    e.source = None;
                    part.edges.insert(i + 1, e);
                }
                "delete_point" => {
                    let i = edge_index(op, "index", n).map_err(at)?;
                    let real = part
                        .edges
                        .iter()
                        .filter(|e| e.direction_rad.is_none())
                        .count();
                    if part.edges[i].direction_rad.is_none() && real <= 3 {
                        return Err(at("a part keeps at least 3 walls".into()));
                    }
                    part.edges.remove(i);
                }
                "set_length" => {
                    let i = edge_index(op, "index", n).map_err(at)?;
                    let len = arg_f64(op, "length")?
                        .filter(|l| *l > 0.0)
                        .ok_or_else(|| at("`length` must be positive".into()))?;
                    let a = part.edges[i].start;
                    let b = part.edges[(i + 1) % n].start;
                    let cur = a.distance(b).max(1e-9);
                    part.edges[(i + 1) % n].start = Point2::new(
                        round(a.x + (b.x - a.x) * len / cur, 3),
                        round(a.y + (b.y - a.y) * len / cur, 3),
                    );
                }
                "insert_direction_change" => {
                    let i = edge_index(op, "index", n).map_err(at)?;
                    let e = part.edges[i].clone();
                    let next = part.edges[(i + 1) % n].start;
                    let ang = (next.y - e.start.y).atan2(next.x - e.start.x)
                        + std::f64::consts::FRAC_PI_2;
                    // The wall's own definition, as the editor's "Direction
                    // change" button makes it; a gable's marker gets the
                    // pitch for new roofs from the settings.
                    let mut marker = e.clone();
                    marker.half_hip = None;
                    marker.breaks.clear();
                    marker.clad = false;
                    if e.is_gable() {
                        marker.pitch_deg = tectora_core::settings::Settings::load()
                            .roof_defaults
                            .pitch_deg;
                    }
                    marker.direction_rad = Some(ang.rem_euclid(std::f64::consts::TAU));
                    part.edges.insert(i, marker);
                }
                "shift_wall" => {
                    let i = edge_index(op, "index", n).map_err(at)?;
                    let d =
                        arg_f64(op, "distance")?.ok_or_else(|| at("missing `distance`".into()))?;
                    if !offset_wall(part, i, d) {
                        return Err(at(
                            "a neighbouring wall runs parallel; no corner can be found".into(),
                        ));
                    }
                }
                "rename" => part.name = arg_str(op, "name").map_err(at)?.to_string(),
                other => return Err(at(format!("unknown op `{other}`"))),
            }
        }
        self.commit(Some(&id), Some(&before), &p, arg_str(args, "summary")?)
    }

    fn transform(&self, args: &Value) -> ToolResult {
        let (id, before) = self.load(args)?;
        let mut p = before.clone();
        let pi = part_index(args, &p)?;
        let centre = point(args, "centre")?.unwrap_or_else(|| part_centre(&p.parts[pi]));
        let m = match arg_str(args, "op")? {
            "move" => PlanMap::Move {
                dx: arg_f64(args, "dx")?.unwrap_or(0.0),
                dy: arg_f64(args, "dy")?.unwrap_or(0.0),
            },
            "rotate" => PlanMap::Rotate {
                centre,
                deg: arg_f64(args, "deg")?.ok_or("missing `deg`")?,
            },
            "mirror" => PlanMap::Mirror {
                centre,
                deg: arg_f64(args, "deg")?.ok_or("missing `deg`")?,
            },
            other => return Err(format!("unknown op `{other}`")),
        };
        let copies: u32 = opt(args, "copies")?.unwrap_or(0);
        if copies > MAX_COPIES {
            return Err(format!("at most {MAX_COPIES} copies"));
        }
        transform_part(&mut p, pi, m, copies);
        self.commit(Some(&id), Some(&before), &p, arg_str(args, "summary")?)
    }

    fn transform_items(&self, args: &Value) -> ToolResult {
        let (id, before) = self.load(args)?;
        let mut p = before.clone();
        let items: Vec<ItemRef> = serde_json::from_value(arg(args, "items")?.clone())
            .map_err(|e| format!("items: {e}"))?;
        if items.is_empty() {
            return Err("`items` is empty".into());
        }
        if items.len() > 500 {
            return Err("at most 500 items".into());
        }
        let mut seen = std::collections::HashSet::new();
        if let Some(d) = items.iter().find(|it| !seen.insert(**it)) {
            return Err(format!("{:?} {} is listed twice", d.kind, d.index));
        }
        let copies: u32 = opt(args, "copies")?.unwrap_or(0);
        if copies > MAX_COPIES {
            return Err(format!("at most {MAX_COPIES} copies"));
        }
        let moved = match arg_str(args, "op")? {
            "move" => {
                let (dx, dy) = (
                    arg_f64(args, "dx")?.unwrap_or(0.0),
                    arg_f64(args, "dy")?.unwrap_or(0.0),
                );
                if !(dx.is_finite() && dy.is_finite() && dx.abs() < 1e4 && dy.abs() < 1e4) {
                    return Err("dx and dy must be finite and under 10 km".into());
                }
                let from: Vec<Point2> =
                    items.iter().filter_map(|&it| item_centre(&p, it)).collect();
                let out = transform_items(&mut p, &items, PlanMap::Move { dx, dy }, copies)?;
                if opt::<bool>(args, "follow_roof")?.unwrap_or(true) {
                    let geos = tectora_core::openings::generate_project(&before);
                    let n = items.len();
                    let pairs: Vec<_> = out
                        .iter()
                        .enumerate()
                        .map(|(j, &r)| {
                            let a = from[j % n];
                            let k = (j / n + 1) as f64;
                            (r, a, Point2::new(a.x + dx * k, a.y + dy * k))
                        })
                        .collect();
                    follow_roof(&mut p, &pairs, |q| host_height_at(&geos, q));
                }
                out
            }
            "rotate" => {
                let deg = arg_f64(args, "deg")?.ok_or("missing `deg`")?;
                if !deg.is_finite() {
                    return Err("`deg` must be finite".into());
                }
                match point(args, "centre")? {
                    Some(c) => {
                        transform_items(&mut p, &items, PlanMap::Rotate { centre: c, deg }, copies)?
                    }
                    None => {
                        let mut out = Vec::new();
                        for &it in &items {
                            let c = item_centre(&p, it).ok_or_else(|| {
                                format!("{:?} {} does not exist", it.kind, it.index)
                            })?;
                            out.extend(transform_items(
                                &mut p,
                                &[it],
                                PlanMap::Rotate { centre: c, deg },
                                copies,
                            )?);
                        }
                        out
                    }
                }
            }
            "align" => {
                let mut out = Vec::new();
                for &it in &items {
                    let corners = match it.kind {
                        ItemKind::Chimney => p.chimneys.get(it.index),
                        ItemKind::RoofOpening => p.roof_openings.get(it.index),
                        _ => return Err("align turns only chimneys and roof openings".into()),
                    }
                    .ok_or_else(|| format!("{:?} {} does not exist", it.kind, it.index))?
                    .corners
                    .clone();
                    let c = item_centre(&p, it)
                        .ok_or_else(|| format!("{:?} {} does not exist", it.kind, it.index))?;
                    if corners.len() < 2 {
                        continue;
                    }
                    let now = (corners[1].y - corners[0].y)
                        .atan2(corners[1].x - corners[0].x)
                        .to_degrees();
                    let want = nearest_wall_angle(&p, c).to_degrees();
                    // Any quarter turn of a rectangle lines up the same.
                    let mut d = (want - now).rem_euclid(90.0);
                    if d > 45.0 {
                        d -= 90.0;
                    }
                    let d = (d * 1e4).round() / 1e4;
                    if d.abs() < 1e-6 {
                        continue;
                    }
                    out.extend(transform_items(
                        &mut p,
                        &[it],
                        PlanMap::Rotate { centre: c, deg: d },
                        0,
                    )?);
                }
                out
            }
            other => return Err(format!("unknown op `{other}`")),
        };
        let mut reply = self.commit(Some(&id), Some(&before), &p, arg_str(args, "summary")?)?;
        if let Some(o) = reply.as_object_mut() {
            o.insert("items".into(), json!(moved));
        }
        Ok(reply)
    }

    fn add_shape_part(&self, args: &Value) -> ToolResult {
        let (id, before) = self.load(args)?;
        let mut p = before.clone();
        let pitch = arg_f64(args, "pitch_deg")?.unwrap_or(40.0);
        let eave = arg_f64(args, "eave_height")?.unwrap_or(3.0);
        let overhang = arg_f64(args, "overhang")?.unwrap_or(0.5);
        if !(0.0 < pitch && pitch < 90.0) || overhang < 0.0 {
            return Err("pitch must be between 0 and 90 and the overhang not negative".into());
        }
        let at = point(args, "at")?;
        let shape = arg_str(args, "shape")?;
        let mut part = if shape == "regular_polygon" {
            let sides: usize = req(args, "sides")?;
            let side = arg_f64(args, "side")?.ok_or("missing `side`")?;
            if !(3..=MAX_POLYGON_SIDES).contains(&sides) || side.is_nan() || side <= 0.0 {
                return Err(format!(
                    "a polygon needs 3 to {MAX_POLYGON_SIDES} sides of positive length"
                ));
            }
            let c = at.unwrap_or(Point2::new(0.0, 0.0));
            let pts = regular_polygon(
                c,
                sides,
                side,
                arg_f64(args, "rotation_deg")?.unwrap_or(0.0),
            );
            RoofPart {
                name: String::new(),
                edges: pts
                    .into_iter()
                    .map(|q| plain_edge(q, pitch, eave, overhang))
                    .collect(),
            }
        } else {
            let kind: StandardRoof = serde_json::from_value(json!(shape))
                .map_err(|_| format!("unknown shape `{shape}`"))?;
            let length = arg_f64(args, "length")?.ok_or("missing `length`")?;
            let width = arg_f64(args, "width")?.unwrap_or(length);
            if !(length > 0.0 && width > 0.0) {
                return Err("length and width must be positive".into());
            }
            let mut part = standard_roof(kind, length, width, pitch, eave, overhang);
            let (dx, dy) = match at {
                Some(q) => (q.x, q.y),
                None if args.get("replace_part").is_none() => {
                    let max_x = p
                        .parts
                        .iter()
                        .flat_map(|q| q.edges.iter().map(|e| e.start.x))
                        .reduce(f64::max);
                    (max_x.map_or(0.0, |x| x + 2.0), 0.0)
                }
                None => (0.0, 0.0),
            };
            for e in &mut part.edges {
                e.start = Point2::new(round(e.start.x + dx, 3), round(e.start.y + dy, 3));
            }
            part
        };
        let replace: Option<usize> = opt(args, "replace_part")?;
        part.name = match (args.get("name").and_then(Value::as_str), replace) {
            (Some(n), _) => n.to_string(),
            (None, Some(r)) => p.parts.get(r).map(|q| q.name.clone()).unwrap_or_default(),
            (None, None) if p.parts.is_empty() => "Main".into(),
            (None, None) => format!("Part {}", p.parts.len() + 1),
        };
        part.normalize_ccw();
        match replace {
            Some(r) if r < p.parts.len() => p.parts[r] = part,
            Some(r) => return Err(format!("part {r} does not exist")),
            None => p.parts.push(part),
        }
        self.commit(Some(&id), Some(&before), &p, arg_str(args, "summary")?)
    }

    fn delete_items(&self, args: &Value) -> ToolResult {
        let (id, before) = self.load(args)?;
        let mut p = before.clone();
        let items: Vec<Value> = req(args, "items")?;
        let mut list: Vec<(String, usize)> = items
            .iter()
            .map(|v| Ok((arg_str(v, "kind")?.to_string(), req::<usize>(v, "index")?)))
            .collect::<Result<_, String>>()?;
        // Highest index first, so the others keep theirs.
        list.sort_by_key(|x| std::cmp::Reverse(x.1));
        list.dedup();
        for (kind, i) in list {
            fn take<T>(v: &mut Vec<T>, i: usize, kind: &str) -> Result<(), String> {
                if i >= v.len() {
                    return Err(format!("{kind} {i} does not exist"));
                }
                v.remove(i);
                Ok(())
            }
            match kind.as_str() {
                "part" => take(&mut p.parts, i, &kind)?,
                "dormer" => take(&mut p.dormers, i, &kind)?,
                "chimney" => take(&mut p.chimneys, i, &kind)?,
                "window" => take(&mut p.windows, i, &kind)?,
                "roof_opening" => take(&mut p.roof_openings, i, &kind)?,
                "manual_line" => take(&mut p.manual_lines, i, &kind)?,
                "manual_face" => take(&mut p.manual_faces, i, &kind)?,
                other => return Err(format!("unknown kind `{other}`")),
            }
        }
        self.commit(Some(&id), Some(&before), &p, arg_str(args, "summary")?)
    }

    fn dormer(&self, args: &Value, add: bool) -> ToolResult {
        let (id, before) = self.load(args)?;
        let mut p = before.clone();
        let (index, mut params) = if add {
            let start = point(args, "start")?.ok_or("missing `start`")?;
            let kind: DormerKind = req(args, "kind")?;
            let angle = dormers::nearest_wall_angle(&p.parts, start);
            // The roof height just inside the front, as the editor places it.
            let geos = tectora_core::openings::generate_project(&p);
            let inside = Point2::new(start.x - angle.sin() * 0.01, start.y + angle.cos() * 0.01);
            let host = roof_height_at(&geos, inside).unwrap_or(3.0);
            (
                p.dormers.len(),
                dormers::default_params(kind, start, angle, round(host + 1.0, 2)),
            )
        } else {
            let i: usize = req(args, "index")?;
            let d = p
                .dormers
                .get(i)
                .ok_or_else(|| format!("dormer {i} does not exist"))?;
            let mut params = dormers::parse_dormer(d);
            if params.kind == DormerKind::General && args.get("kind").is_none() {
                return Err(format!(
                    "dormer {i} is not one of the standard kinds; give `kind` to rebuild it"
                ));
            }
            if params.kind == DormerKind::General {
                params.kind = DormerKind::Gable;
            }
            (i, params)
        };
        apply_dormer_fields(&mut params, args)?;
        let mut d = dormers::build_dormer(&params);
        if let Some(old) = p.dormers.get(index) {
            d.host_wall = old.host_wall;
            p.dormers[index] = d;
        } else {
            p.dormers.push(d);
        }
        let mut out = self.commit(Some(&id), Some(&before), &p, arg_str(args, "summary")?)?;
        out["dormer"] = describe_dormer(index, &p.dormers[index]);
        Ok(out)
    }

    fn set_manual(&self, args: &Value) -> ToolResult {
        let (id, before) = self.load(args)?;
        let mut p = before.clone();
        if let Some(lines) = opt::<Vec<Value>>(args, "lines")? {
            // Snap to the roof as it is without the hand-drawn lines.
            let geos = tectora_core::openings::generate_project(&p);
            p.manual_lines = lines
                .iter()
                .enumerate()
                .map(|(i, v)| manual_line(v, &geos).map_err(|m| format!("line {i}: {m}")))
                .collect::<Result<_, _>>()?;
        }
        if let Some(faces) = opt::<Vec<Value>>(args, "faces")? {
            p.manual_faces = faces
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    let points = super::parse_points(arg(v, "points")?)
                        .map_err(|m| format!("face {i}: {m}"))?;
                    let pitch_deg = arg_f64(v, "pitch_deg")?.unwrap_or(30.0);
                    if !(0.0..90.0).contains(&pitch_deg) {
                        return Err(format!("face {i}: pitch_deg must be between 0 and 90"));
                    }
                    Ok(ManualFace {
                        points,
                        pitch_deg,
                        label: v
                            .get("label")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                    })
                })
                .collect::<Result<_, String>>()?;
        }
        self.commit(Some(&id), Some(&before), &p, arg_str(args, "summary")?)
    }

    fn set_roof_options(&self, args: &Value) -> ToolResult {
        let (id, before) = self.load(args)?;
        let mut p = before.clone();
        if let Some(f) = opt::<bool>(args, "flat_roof")? {
            p.flat_roof = f;
        }
        if let Some(names) = opt::<Vec<String>>(args, "material_names")? {
            p.material_names = names;
            while p.material_names.last().is_some_and(|n| n.trim().is_empty()) {
                p.material_names.pop();
            }
        }
        self.commit(Some(&id), Some(&before), &p, arg_str(args, "summary")?)
    }

    fn roof_helper(&self, args: &Value) -> ToolResult {
        let (id, before) = self.load(args)?;
        let pi = part_index(args, &before)?;
        let es = &before.parts[pi].edges;
        let edge = edge_index(args, "edge", es.len())?;
        let e = &es[edge];
        let target: Option<Target> = opt(args, "target")?;
        let need_target = || {
            target.ok_or_else(|| "missing `target` (pitch, eave, overhang or position)".to_string())
        };
        let single = |patch: EdgePatch| Patches::from([(edge, patch)]);
        let mut extra = json!({});
        let patches: Patches = match arg_str(args, "tool")? {
            "triangle" => {
                let t = hc::triangle(
                    e,
                    arg_f64(args, "width")?,
                    arg_f64(args, "rafter")?,
                    arg_f64(args, "ridge")?,
                    opt(args, "width_includes_overhang")?.unwrap_or(false),
                )
                .ok_or("give two of width, rafter and ridge that make a triangle")?;
                extra = json!({ "triangle": t });
                single(hc::triangle_patch(e, t.pitch_deg))
            }
            "parallel" => {
                let target = need_target()?;
                let opp = opposite_wall(es, edge).ok_or("this wall has no facing wall")?;
                if !e.breaks.is_empty() || !es[opp].breaks.is_empty() {
                    return Err("not available on walls with pitch breaks".into());
                }
                let span = span_between(es, edge, opp);
                let (w, h) = hc::meeting(e, &es[opp], span).ok_or("the two planes do not meet")?;
                let mut st = hc::parallel_start(e, &es[opp], span, w, h);
                let with_ov = opt(args, "width_includes_overhang")?.unwrap_or(true);
                let steps: Vec<Value> = req(args, "steps")?;
                for (k, s) in steps.iter().enumerate() {
                    let field: ParallelField = req(s, "field")?;
                    let v = arg_f64(s, "value")?.ok_or("missing `value`")?;
                    st = hc::parallel_edit(&st, field, v, target, with_ov)
                        .ok_or_else(|| format!("step {k}: no solution for this value"))?;
                }
                extra = json!({ "opposite_edge": opp, "state": st });
                if target == Target::Position {
                    tectora_core::edit::combine([
                        &hc::move_wall(es, edge, st.shift_a),
                        &hc::move_wall(es, opp, st.shift_b),
                    ])
                } else {
                    let (pa, pb) = hc::parallel_patches(&st, target);
                    if target == Target::Eave
                        && (hc::eave_below_gable(es, edge, pa.eave_height.unwrap_or(0.0))
                            || hc::eave_below_gable(es, opp, pb.eave_height.unwrap_or(0.0)))
                    {
                        return Err(
                            "the eave would end up below the eave of a neighbouring gable".into(),
                        );
                    }
                    Patches::from([(edge, pa), (opp, pb)])
                }
            }
            "point" => {
                let target = need_target()?;
                let geos = tectora_core::openings::generate_project(&before);
                let corners = roof_corner_points(&geos);
                let q = match (
                    opt::<Vec<f64>>(args, "point")?,
                    opt::<usize>(args, "point_index")?,
                ) {
                    (Some(v), _) if v.len() == 3 => [v[0], v[1], v[2]],
                    (Some(_), _) => return Err("`point` must be [x, y, z]".into()),
                    (None, Some(i)) => *corners
                        .get(i)
                        .ok_or_else(|| format!("point_index {i} out of range"))?,
                    (None, None) => return Ok(json!({ "roof_points": corners })),
                };
                let d = hc::inward(es, edge, Point2::new(q[0], q[1]));
                if target == Target::Position {
                    let sh =
                        hc::shift_for(e, d, q[2]).ok_or("no wall position reaches that point")?;
                    hc::move_wall(es, edge, sh)
                } else {
                    single(
                        hc::solve(e, d, q[2], target)
                            .ok_or("no plane of this wall reaches that point")?,
                    )
                }
            }
            "merge" => {
                let target = need_target()?;
                let walls: Vec<usize> =
                    opt(args, "walls")?.unwrap_or_else(|| hc::same_way(es, edge));
                match target {
                    Target::Pitch => {
                        return Err("merge solves for eave, overhang or position".into());
                    }
                    Target::Position => {
                        hc::merge_by_moving(es, edge, &walls).ok_or("this wall has no slope")?
                    }
                    t => hc::merge_onto(es, edge, &walls, t),
                }
            }
            "hip" => {
                let n = es.len();
                let other = match args
                    .get("neighbour")
                    .and_then(Value::as_str)
                    .unwrap_or("next")
                {
                    "prev" => (edge + n - 1) % n,
                    _ => (edge + 1) % n,
                };
                let hip = arg_f64(args, "hip")?.ok_or("missing `hip`")?;
                let a = arg_f64(args, "first")?.ok_or("missing `first`")?;
                let b = arg_f64(args, "second")?.ok_or("missing `second`")?;
                let f = if args.get("by").and_then(Value::as_str) == Some("eaves") {
                    hc::hip_pitches_from_eaves
                } else {
                    hc::hip_pitches
                };
                f(es, edge, other, hip, a, b).ok_or("no pitches fit these lengths (both walls need a slope and the same eave height)")?
            }
            "build_up" => {
                let d = arg_f64(args, "thickness")?.ok_or("missing `thickness`")?;
                let eave: BuildUpEave = opt(args, "eave")?.unwrap_or_default();
                single(hc::build_up(e, d, eave).ok_or("not possible on this wall")?)
            }
            "round_roof" => {
                let opp = opposite_wall(es, edge).ok_or("this wall has no facing wall")?;
                let arc = arc_for(&ArcInput {
                    span: span_between(es, edge, opp),
                    overhang: e.overhang,
                    eave_height: e.eave_height,
                    segments: opt(args, "segments")?.unwrap_or(0),
                    mode: opt::<ArcMode>(args, "mode")?.unwrap_or_default(),
                    value: arg_f64(args, "value")?.unwrap_or(0.0),
                })
                .ok_or("the value is out of range for this span")?;
                let patch = EdgePatch {
                    pitch_deg: Some(arc.pitch_deg),
                    eave_height: Some(e.eave_height),
                    overhang: Some(e.overhang),
                    breaks: Some(arc.breaks.clone()),
                    half_hip: Some(None),
                    ..EdgePatch::default()
                };
                extra = json!({ "opposite_edge": opp, "arc": arc });
                Patches::from([(edge, patch.clone()), (opp, patch)])
            }
            other => return Err(format!("unknown tool `{other}`")),
        };
        let mut p = before.clone();
        let applied = apply_patches(&mut p.parts[pi].edges, &patches);
        let mut out = if args.get("apply").and_then(Value::as_bool) == Some(true)
            && !applied.is_empty()
        {
            self.commit(Some(&id), Some(&before), &p, arg_str(args, "summary")?)?
        } else {
            json!({ "id": id, "saved": false, "note": "Not saved; call again with apply: true to save." })
        };
        out["patches"] = patches_json(&applied);
        if let Value::Object(m) = extra {
            for (k, v) in m {
                out[k] = v;
            }
        }
        Ok(out)
    }
}

/// Direction (radians) of the eave wall nearest to `q` over all parts, as
/// the editor's Align to wall uses it (`app/src/lib/dormers.ts`).
fn nearest_wall_angle(p: &Project, q: Point2) -> f64 {
    let mut best = (f64::INFINITY, 0.0);
    for part in &p.parts {
        let es = &part.edges;
        for (i, e) in es.iter().enumerate() {
            let b = es[(i + 1) % es.len()].start;
            let (dx, dy) = (b.x - e.start.x, b.y - e.start.y);
            let l2 = dx * dx + dy * dy;
            if l2 < 1e-12 || e.is_gable() {
                continue;
            }
            let t = (((q.x - e.start.x) * dx + (q.y - e.start.y) * dy) / l2).clamp(0.0, 1.0);
            let d = (e.start.x + t * dx - q.x).hypot(e.start.y + t * dy - q.y);
            if d < best.0 {
                best = (d, dy.atan2(dx));
            }
        }
    }
    best.1
}

/// Corner `v` and the direction changes (zero-length marker walls) on the
/// same point next to it, as the plan editor moves them together
/// (`app/src/lib/corners.ts`). O(n).
fn corner_group(es: &[tectora_core::EdgeDef], v: usize) -> Vec<usize> {
    let n = es.len();
    let p = es[v].start;
    let at = |j: usize| (es[j].start.x - p.x).abs() < 1e-6 && (es[j].start.y - p.y).abs() < 1e-6;
    let marker = |j: usize| es[j].direction_rad.is_some();
    let mut out = vec![v];
    for k in 1..n {
        let j = (v + n - k) % n;
        if !marker(j) || !at(j) {
            break;
        }
        out.push(j);
    }
    for k in 1..n {
        let (prev, j) = ((v + k - 1) % n, (v + k) % n);
        if !marker(prev) || !at(j) {
            break;
        }
        out.push(j);
    }
    out.sort_unstable();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tectora_core::store::{Store, now_ms};

    fn setup() -> (Server, String) {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "tectora-mcp-more-{}-{}-{n}",
            now_ms(),
            std::process::id()
        ));
        let mut s = Server::new(Store::new(dir).unwrap());
        let r = s
            .call_tool(
                "create_standard_roof",
                &json!({ "name": "Test", "type": "gable", "length": 10, "width": 8, "pitch_deg": 40, "summary": "start" }),
            )
            .unwrap();
        let id = r["id"].as_str().unwrap().to_string();
        (s, id)
    }

    fn call(s: &mut Server, name: &str, args: Value) -> Value {
        s.call_tool(name, &args)
            .unwrap_or_else(|e| panic!("{name}: {e}"))
    }

    #[test]
    fn huge_counts_are_refused_before_anything_is_built() {
        // ARCH-003: `sides: 1e12` aborted the server (out of memory).
        let (mut s, id) = setup();
        for (tool, want, args) in [
            (
                "add_shape_part",
                "3 to 64 sides",
                json!({ "id": id, "shape": "regular_polygon", "sides": 1_000_000_000_000u64, "side": 1, "summary": "s" }),
            ),
            (
                "transform_part",
                "at most 50 copies",
                json!({ "id": id, "part": 0, "op": "move", "dx": 20, "dy": 0, "copies": 100_000, "summary": "s" }),
            ),
            (
                "add_dormer",
                "0 to 30",
                json!({ "id": id, "kind": "round", "start": [2, 0], "width": 2, "segments": 1_000_000, "summary": "s" }),
            ),
        ] {
            let e = s.call_tool(tool, &args).unwrap_err();
            assert!(e.contains(want), "{tool}: {e}");
        }
        let p = s.store.load(&id).unwrap();
        assert_eq!((p.parts.len(), p.dormers.len()), (1, 0));
    }

    #[test]
    fn a_direction_change_moves_with_its_corner_and_can_be_turned() {
        let (mut s, id) = setup();
        call(
            &mut s,
            "edit_walls",
            json!({ "id": id, "summary": "m", "operations": [{ "op": "insert_direction_change", "index": 0 }] }),
        );
        let p = s.store.load(&id).unwrap();
        let (marker, wall) = (&p.parts[0].edges[0], &p.parts[0].edges[1]);
        assert!(marker.direction_rad.is_some() && wall.direction_rad.is_none());
        assert_eq!(marker.pitch_deg, wall.pitch_deg);
        assert_eq!(marker.start, wall.start);
        call(
            &mut s,
            "edit_walls",
            json!({ "id": id, "summary": "m", "operations": [{ "op": "move_point", "index": 1, "x": -1.0, "y": -0.5 }] }),
        );
        let p = s.store.load(&id).unwrap();
        assert_eq!(
            p.parts[0].edges[0].start,
            Point2::new(-1.0, -0.5),
            "the marker follows its corner"
        );
        assert_eq!(p.parts[0].edges[1].start, Point2::new(-1.0, -0.5));
        call(
            &mut s,
            "update_edges",
            json!({ "id": id, "summary": "d", "changes": [{ "index": 0, "direction_deg": 450 }] }),
        );
        let d = s.store.load(&id).unwrap().parts[0].edges[0]
            .direction_rad
            .unwrap();
        assert!((d - std::f64::consts::FRAC_PI_2).abs() < 1e-12, "{d}");
        let err = s
            .call_tool("update_edges", &json!({ "id": id, "summary": "d", "changes": [{ "index": 1, "direction_deg": 90 }] }))
            .unwrap_err();
        assert!(err.contains("direction change"), "{err}");
    }

    #[test]
    fn a_pitch_for_all_walls_keeps_the_gables_and_a_chimney_aligns_to_its_wall() {
        let (mut s, id) = setup();
        call(
            &mut s,
            "update_edges",
            json!({ "id": id, "summary": "p", "changes": [{ "all": true, "pitch_deg": 30 }] }),
        );
        let p = s.store.load(&id).unwrap();
        assert_eq!(p.parts[0].edges.iter().filter(|e| e.is_gable()).count(), 2);
        assert!(
            p.parts[0]
                .edges
                .iter()
                .filter(|e| !e.is_gable())
                .all(|e| e.pitch_deg == 30.0)
        );
        call(
            &mut s,
            "set_openings",
            json!({ "id": id, "summary": "c",
            "chimneys": [{ "x": 4, "y": 1, "width": 0.6, "depth": 0.6, "height": 6, "rotation_deg": 20 }] }),
        );
        call(
            &mut s,
            "transform_items",
            json!({ "id": id, "summary": "a", "op": "align", "items": [{ "kind": "chimney", "index": 0 }] }),
        );
        let c = &s.store.load(&id).unwrap().chimneys[0].corners;
        let a = (c[1].y - c[0].y)
            .atan2(c[1].x - c[0].x)
            .to_degrees()
            .rem_euclid(90.0);
        assert!(a < 1e-6 || (90.0 - a) < 1e-6, "{a}");
        let err = s
            .call_tool("transform_items", &json!({ "id": id, "summary": "a", "op": "align", "items": [{ "kind": "window", "index": 0 }] }))
            .unwrap_err();
        assert!(err.contains("only chimneys"), "{err}");
    }

    #[test]
    fn all_walls_to_eaves_take_the_given_pitch() {
        // ERR-020: the former gables kept a hard-coded 40°.
        let (mut s, id) = setup();
        assert!(
            s.store.load(&id).unwrap().parts[0]
                .edges
                .iter()
                .any(|e| e.is_gable())
        );
        call(
            &mut s,
            "update_edges",
            json!({ "id": id, "summary": "hip", "changes": [{ "all": true, "gable": false, "pitch_deg": 30 }] }),
        );
        let p = s.store.load(&id).unwrap();
        let pitches: Vec<f64> = p.parts[0].edges.iter().map(|e| e.pitch_deg).collect();
        assert!(pitches.iter().all(|&a| a == 30.0), "{pitches:?}");
        // A pitch alone for all walls still leaves gables alone.
        call(
            &mut s,
            "update_edges",
            json!({ "id": id, "summary": "g", "changes": [{ "index": 1, "gable": true }] }),
        );
        call(
            &mut s,
            "update_edges",
            json!({ "id": id, "summary": "p", "changes": [{ "all": true, "pitch_deg": 25 }] }),
        );
        assert!(s.store.load(&id).unwrap().parts[0].edges[1].is_gable());
    }

    #[test]
    fn a_gable_can_be_clad() {
        let (mut s, id) = setup();
        let p = s.store.load(&id).unwrap();
        let g = p.parts[0]
            .edges
            .iter()
            .position(|e| e.is_gable())
            .expect("a gable");
        call(
            &mut s,
            "update_edges",
            json!({ "id": id, "summary": "c", "changes": [{ "index": g, "clad": true }] }),
        );
        assert!(s.store.load(&id).unwrap().parts[0].edges[g].clad);
        call(
            &mut s,
            "update_edges",
            json!({ "id": id, "summary": "c", "changes": [{ "index": g, "clad": false }] }),
        );
        assert!(!s.store.load(&id).unwrap().parts[0].edges[g].clad);
    }

    #[test]
    fn transform_items_moves_copies_and_follows_the_roof() {
        let (mut s, id) = setup();
        call(
            &mut s,
            "set_openings",
            json!({ "id": id, "summary": "c",
            "chimneys": [{ "x": 4, "y": 1, "width": 0.6, "depth": 0.6, "height": 6 }] }),
        );
        // 40° roof rising towards y = 4: 1 m north lifts the top by tan 40°.
        let r = call(
            &mut s,
            "transform_items",
            json!({ "id": id, "summary": "m",
            "items": [{ "kind": "chimney", "index": 0 }], "op": "move", "dy": 1 }),
        );
        assert_eq!(r["items"], json!([{ "kind": "chimney", "index": 0 }]));
        let p = s.store.load(&id).unwrap();
        assert_eq!(p.chimneys[0].corners[0], Point2::new(4.0, 2.0));
        assert!((p.chimneys[0].height - (6.0 + 40f64.to_radians().tan())).abs() < 2e-3);
        let r = call(
            &mut s,
            "transform_items",
            json!({ "id": id, "summary": "c",
            "items": [{ "kind": "chimney", "index": 0 }], "op": "rotate", "deg": 90, "copies": 1 }),
        );
        assert_eq!(r["items"][0]["index"], 1);
        assert_eq!(s.store.load(&id).unwrap().chimneys.len(), 2);
        assert!(
            s.call_tool(
                "transform_items",
                &json!({ "id": id, "summary": "x",
            "items": [{ "kind": "dormer", "index": 3 }], "op": "move", "dx": 1 })
            )
            .is_err()
        );
    }

    #[test]
    fn every_tool_has_a_schema_and_a_handler() {
        let (mut s, _) = setup();
        for t in tools() {
            let name = t["name"].as_str().unwrap();
            assert!(t["inputSchema"]["type"] == "object", "{name}");
            // A call with no arguments is an error message or a result, never "unknown tool".
            if let Err(e) = s.call_tool(name, &json!({})) {
                assert!(!e.starts_with("unknown tool"), "{name}: {e}");
            }
        }
    }

    #[test]
    fn walls_transform_and_shapes() {
        let (mut s, id) = setup();
        call(
            &mut s,
            "edit_walls",
            json!({ "id": id, "summary": "s", "operations": [
                { "op": "insert_point", "after": 0, "x": 5, "y": 0 },
                { "op": "move_point", "index": 1, "x": 5, "y": -1 },
                { "op": "delete_point", "index": 1 },
                { "op": "set_length", "index": 0, "length": 12 },
                { "op": "shift_wall", "index": 2, "distance": 1 },
                { "op": "rename", "name": "House" }
            ]}),
        );
        let p = s.store.load(&id).unwrap();
        assert_eq!(p.parts[0].name, "House");
        assert_eq!(p.parts[0].edges.len(), 4);
        assert_eq!(p.parts[0].edges[1].start, Point2::new(12.0, 0.0));
        call(
            &mut s,
            "transform_part",
            json!({ "id": id, "op": "mirror", "deg": 90, "centre": [-1, 0], "copies": 1, "summary": "s" }),
        );
        call(
            &mut s,
            "add_shape_part",
            json!({ "id": id, "shape": "regular_polygon", "sides": 6, "side": 3, "at": [0, 20], "summary": "s" }),
        );
        call(
            &mut s,
            "add_shape_part",
            json!({ "id": id, "shape": "hip", "length": 6, "width": 5, "summary": "s" }),
        );
        let p = s.store.load(&id).unwrap();
        assert_eq!(p.parts.len(), 4);
        assert!(p.parts[1].edges.iter().all(|e| e.start.x <= -2.0 + 1e-9));
        assert_eq!(p.parts[2].edges.len(), 6);
        call(
            &mut s,
            "delete_items",
            json!({ "id": id, "items": [{ "kind": "part", "index": 3 }, { "kind": "part", "index": 2 }], "summary": "s" }),
        );
        assert_eq!(s.store.load(&id).unwrap().parts.len(), 2);
        call(
            &mut s,
            "clear_roof",
            json!({ "id": id, "summary": "Start over" }),
        );
        let p = s.store.load(&id).unwrap();
        assert!(p.parts.is_empty() && p.dormers.is_empty());
        let r = s.call_tool("clear_roof", &json!({ "id": id, "summary": "again" }));
        assert!(r.is_err(), "nothing left to clear");
    }

    #[test]
    fn a_hand_drawn_face_becomes_a_building_in_one_save() {
        let (mut s, id) = setup();
        let mut p = s.store.load(&id).unwrap();
        let parts = p.parts.len();
        p.manual_faces.push(tectora_core::model::ManualFace {
            points: [(30.0, 0.0), (40.0, 0.0), (40.0, 6.0), (30.0, 6.0)]
                .iter()
                .map(|&(x, y)| tectora_core::geom::Point2::new(x, y))
                .collect(),
            pitch_deg: 30.0,
            label: String::new(),
        });
        s.store.save(Some(id.clone()), &p).unwrap();
        let log = s.store.activity(&id).len();
        call(
            &mut s,
            "face_to_part",
            json!({ "id": id, "index": 0, "pitch_deg": 35, "eave_height": 3, "overhang": 0.4, "summary": "Garage traced as a face" }),
        );
        let p = s.store.load(&id).unwrap();
        assert!(p.manual_faces.is_empty());
        assert_eq!(p.parts.len(), parts + 1);
        assert_eq!(p.parts[parts].name, format!("Part {}", parts + 1));
        assert_eq!(s.store.activity(&id).len(), log + 1, "one change");
        // Reverting it brings the face back and removes the part.
        s.call_tool(
            "revert_last_change",
            &json!({ "id": id, "summary": "The garage stays a face" }),
        )
        .unwrap();
        let p = s.store.load(&id).unwrap();
        assert_eq!((p.manual_faces.len(), p.parts.len()), (1, parts));
        assert!(
            s.call_tool(
                "face_to_part",
                &json!({ "id": id, "index": 5, "summary": "x" })
            )
            .is_err()
        );
    }

    #[test]
    fn a_building_height_sets_every_wall_in_one_save_and_changes_the_join() {
        let (mut s, id) = setup();
        call(
            &mut s,
            "add_part",
            json!({ "id": id, "summary": "Wing",
                "part": { "name": "Wing", "points": [[6, 4], [14, 4], [14, 10], [6, 10]], "pitch_deg": 35 } }),
        );
        let before = s.store.load(&id).unwrap();
        let roofs = tectora_core::engine::generate(&before).unwrap();
        let log = s.store.activity(&id).len();
        call(
            &mut s,
            "set_part_height",
            json!({ "id": id, "part": 1, "eave_height": 1.8, "summary": "The wing is lower" }),
        );
        let p = s.store.load(&id).unwrap();
        assert!(p.parts[1].edges.iter().all(|e| e.eave_height == 1.8));
        assert_eq!(p.parts[0], before.parts[0]);
        assert_eq!(s.store.activity(&id).len(), log + 1, "one change");
        let after = tectora_core::engine::generate(&p).unwrap();
        let area =
            |g: &tectora_core::roof::RoofGeometry| g.faces.iter().map(|f| f.area).sum::<f64>();
        assert!(
            (area(&roofs[0]) - area(&after[0])).abs() > 0.01,
            "the main roof is joined anew"
        );
        for bad in [
            json!({ "id": id, "part": 1, "eave_height": 1.8, "summary": "same" }),
            json!({ "id": id, "part": 9, "eave_height": 3, "summary": "no part" }),
            json!({ "id": id, "part": 1, "eave_height": -1, "summary": "below ground" }),
            json!({ "id": id, "part": 1, "eave_height": 5000, "summary": "cm for m" }),
            json!({ "id": id, "part": 1, "summary": "no height" }),
        ] {
            assert!(s.call_tool("set_part_height", &bad).is_err(), "{bad}");
        }
        assert_eq!(
            s.store.activity(&id).len(),
            log + 1,
            "refused calls save nothing"
        );
    }

    #[test]
    fn dormers_manual_lines_and_options() {
        let (mut s, id) = setup();
        let r = call(
            &mut s,
            "add_dormer",
            json!({ "id": id, "kind": "hip", "start": [4, 1], "summary": "s" }),
        );
        assert_eq!(r["dormer"]["kind"], "hip");
        assert!(r["dormer"]["eave_height"].as_f64().unwrap() > 3.0);
        let r = call(
            &mut s,
            "update_dormer",
            json!({ "id": id, "index": 0, "kind": "shed", "width": 3, "summary": "s" }),
        );
        assert_eq!(r["dormer"]["kind"], "shed");
        assert_eq!(r["dormer"]["width"], 3.0);
        call(
            &mut s,
            "set_manual",
            json!({ "id": id, "summary": "s",
            "lines": [{ "a": [0, 0], "b": [10, 0, 3], "code": "T" }],
            "faces": [{ "points": [[0, 0], [1, 0], [1, 1]] }] }),
        );
        let p = s.store.load(&id).unwrap();
        assert_eq!(p.manual_faces[0].pitch_deg, 30.0);
        call(
            &mut s,
            "set_roof_options",
            json!({ "id": id, "flat_roof": true, "material_names": ["Bramac", ""], "summary": "s" }),
        );
        let p = s.store.load(&id).unwrap();
        assert!(p.flat_roof);
        assert_eq!(p.material_names, vec!["Bramac"]);
    }

    #[test]
    fn helpers_preview_and_apply() {
        let (mut s, id) = setup();
        // Edge 0 is the south eave of the 10 x 8 gable roof.
        let r = call(
            &mut s,
            "roof_helper",
            json!({ "id": id, "edge": 0, "tool": "triangle", "width": 4, "ridge": 6 }),
        );
        assert_eq!(r["saved"], false);
        assert_eq!(s.store.load(&id).unwrap().parts[0].edges[0].pitch_deg, 40.0);
        let r = call(
            &mut s,
            "roof_helper",
            json!({ "id": id, "edge": 0, "tool": "parallel", "target": "pitch",
            "steps": [{ "field": "ridge", "value": 7.5 }], "apply": true, "summary": "Ridge 7.5 m" }),
        );
        assert_eq!(r["saved"], true);
        assert_eq!(r["patches"].as_array().unwrap().len(), 2);
        let r = call(
            &mut s,
            "roof_helper",
            json!({ "id": id, "edge": 0, "tool": "point", "target": "pitch" }),
        );
        assert!(!r["roof_points"].as_array().unwrap().is_empty());
        let r = call(
            &mut s,
            "roof_helper",
            json!({ "id": id, "edge": 0, "tool": "round_roof", "apply": true, "summary": "Round roof" }),
        );
        assert!(r["arc"]["segments"].as_u64().unwrap() >= 1);
        let p = s.store.load(&id).unwrap();
        assert!(!p.parts[0].edges[0].breaks.is_empty());
        call(
            &mut s,
            "roof_helper",
            json!({ "id": id, "edge": 0, "tool": "build_up", "thickness": 0.2 }),
        );
    }

    #[test]
    fn library_files_bundle_and_stored_result() {
        let (mut s, id) = setup();
        let dir = s.store.dir.clone();
        let drawing = dir.join("plan.pdf");
        std::fs::write(&drawing, b"%PDF").unwrap();
        call(
            &mut s,
            "attach_file",
            json!({ "id": id, "path": drawing, "summary": "Drawing" }),
        );
        assert_eq!(
            call(&mut s, "list_files", json!({ "id": id }))[0]["name"],
            "plan.pdf"
        );
        call(
            &mut s,
            "update_job",
            json!({ "id": id, "status": "offer", "summary": "Offer sent" }),
        );
        let r = call(&mut s, "calc", json!({ "id": id }));
        assert_eq!(
            r["stored_result"]["differences"].as_array().unwrap().len(),
            0
        );
        call(
            &mut s,
            "update_edges",
            json!({ "id": id, "changes": [{ "index": 0, "pitch_deg": 45 }], "summary": "s" }),
        );
        let r = call(&mut s, "calc", json!({ "id": id }));
        assert!(
            !r["stored_result"]["differences"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(
            s.call_tool(
                "export_job",
                &json!({ "id": id, "path": dir.join("x.txt"), "summary": "s" })
            )
            .is_err()
        );
        let out = call(&mut s, "export_job", json!({ "id": id, "summary": "s" }))["path"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(out.ends_with(".tectorajob"));
        let again = call(&mut s, "export_job", json!({ "id": id, "summary": "s" }))["path"]
            .as_str()
            .unwrap()
            .to_string();
        assert_ne!(out, again, "an existing export is never replaced");
        let list = call(
            &mut s,
            "export_list",
            json!({ "id": id, "stamp": "08.11.2026 12:00" }),
        )["list"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(
            list.contains("STREŠNÉ PLOCHY")
                && list.contains("LÍNIE")
                && list.contains("08.11.2026 12:00"),
            "{list}"
        );
        let sheet = call(&mut s, "export_plan", json!({ "id": id, "summary": "s" }))["path"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(sheet.ends_with(".pdf"));
        assert!(std::fs::read(&sheet).unwrap().starts_with(b"%PDF-"));
        let svg = call(
            &mut s,
            "export_plan",
            json!({ "id": id, "format": "svg", "name": "plan", "summary": "s" }),
        )["path"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(svg.ends_with("plan.svg"));
        for bad in [
            json!({ "id": id, "format": "exe", "summary": "s" }),
            json!({ "id": id, "name": "../x", "summary": "s" }),
        ] {
            assert!(s.call_tool("export_plan", &bad).is_err());
        }
        let new_id = call(
            &mut s,
            "import_job",
            json!({ "path": out, "summary": "Received job" }),
        )["id"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(
            call(&mut s, "list_files", json!({ "id": new_id }))
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let copy = call(
            &mut s,
            "duplicate_project",
            json!({ "id": id, "name": "Copy", "summary": "A variant" }),
        )["id"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(s.store.load(&copy).unwrap().result_snapshot.is_none());
        let fresh = call(
            &mut s,
            "duplicate_project",
            json!({ "id": id, "name": "New job", "job": { "customer": "Dvořák" }, "with_files": false, "summary": "Start from a past roof" }),
        )["id"]
            .as_str()
            .unwrap()
            .to_string();
        let p = s.store.load(&fresh).unwrap();
        assert_eq!(
            (p.name.as_str(), p.job.customer.as_str()),
            ("New job", "Dvořák")
        );
        assert!(
            call(&mut s, "list_files", json!({ "id": fresh }))
                .as_array()
                .unwrap()
                .is_empty()
        );
        let found = call(
            &mut s,
            "list_projects",
            json!({ "query": "DVORAK", "sort": "name" }),
        );
        let found = &found["projects"];
        assert_eq!(found.as_array().unwrap().len(), 1);
        assert_eq!(found[0]["id"], fresh.as_str());
        assert!(found[0]["pitch_range_deg"].is_array());
        call(
            &mut s,
            "delete_project",
            json!({ "id": fresh, "summary": "s" }),
        );
        assert!(
            call(&mut s, "get_history", json!({ "id": id }))
                .as_array()
                .unwrap()
                .len()
                >= 2
        );
        // Nothing accepted in the app yet: the agent's changes wait for review.
        let history = call(&mut s, "get_history", json!({ "id": id }));
        assert_eq!(history[0]["pending_review"], true);
        let review = call(&mut s, "get_review", json!({ "id": id }));
        assert!(review["pending"]["count"].as_u64().unwrap() >= 2);
        let last = s.store.activity(&id).last().unwrap().ts_ms;
        s.store.mark_reviewed(&id, last).unwrap();
        let review = call(&mut s, "get_review", json!({ "id": id }));
        assert!(review["pending"].is_null());
        assert_eq!(review["accepted_ms"], last);
        call(
            &mut s,
            "delete_project",
            json!({ "id": copy, "summary": "s" }),
        );
        assert!(!s.store.exists(&copy));
        assert!(
            call(&mut s, "get_schema", json!({}))["$defs"]
                .get("EdgeDef")
                .is_some()
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn attach_file_takes_only_regular_documents_and_images() {
        // ARCH-001: devices, folders and private files without a document
        // extension (`~/.ssh/id_rsa`) were copied into the job.
        let (mut s, id) = setup();
        let dir = s.store.dir.join("outside");
        std::fs::create_dir_all(&dir).unwrap();
        let key = dir.join("id_rsa");
        std::fs::write(&key, b"PRIVATE").unwrap();
        let script = dir.join("plan.pdf.command");
        std::fs::write(&script, b"#!/bin/sh").unwrap();
        let mut bad = vec![dir.clone(), key, script];
        if cfg!(unix) {
            bad.push(PathBuf::from("/dev/null"));
        }
        for path in bad {
            let r = s.call_tool(
                "attach_file",
                &json!({ "id": id, "path": path, "summary": "Drawing" }),
            );
            assert!(r.is_err(), "{}: {r:?}", path.display());
        }
        let plan = dir.join("plan.pdf");
        std::fs::write(&plan, b"%PDF").unwrap();
        // A write without a summary is refused (ARCH-005).
        assert!(
            s.call_tool("attach_file", &json!({ "id": id, "path": plan }))
                .is_err()
        );
        let r = call(
            &mut s,
            "attach_file",
            json!({ "id": id, "path": plan, "summary": "The drawing the roof was made from" }),
        );
        assert_eq!(r["name"], "plan.pdf");
        assert_eq!(r["from"], dir.display().to_string());
        assert_eq!(s.store.attachments(&id).len(), 1);
        let _ = std::fs::remove_dir_all(&s.store.dir);
    }

    #[test]
    fn every_write_tool_requires_a_summary() {
        // ARCH-005: AGENTS.md, every MCP write carries a one-sentence summary.
        const READ_ONLY: &[&str] = &[
            "tectora_guide",
            "get_settings",
            "list_projects",
            "get_project",
            "calc",
            "calc_breakdown",
            "show_project",
            "get_history",
            "get_review",
            "list_examples",
            "list_files",
            "export_list",
            "get_schema",
            "list_deleted",
            "roof_helper",
            "list_drawings",
            "drawing_info",
            "view_drawing",
            "drawing_lines",
            "measure_drawing",
            "drawing_dimensions",
            "trace_outline",
        ];
        let tools = crate::tools();
        let mut missing = Vec::new();
        for t in tools.as_array().unwrap() {
            let name = t["name"].as_str().unwrap();
            if READ_ONLY.contains(&name) {
                continue;
            }
            let required = t["inputSchema"]["required"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            if !required.iter().any(|r| r == "summary") {
                missing.push(name.to_string());
            }
        }
        assert!(
            missing.is_empty(),
            "writes without a required summary: {missing:?}"
        );
    }

    #[test]
    fn an_empty_or_blank_summary_is_refused_by_every_write_tool() {
        // Re-check of ARCH-005: `"summary": ""` and `"  "` were accepted.
        let (mut s, id) = setup();
        let before = s.store.load(&id).unwrap();
        let log = s.store.activity(&id).len();
        let tools = crate::tools();
        let writes: Vec<&str> = tools
            .as_array()
            .unwrap()
            .iter()
            .filter(|t| {
                t["inputSchema"]["required"]
                    .as_array()
                    .is_some_and(|r| r.iter().any(|r| r == "summary"))
            })
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(writes.len() > 20, "{writes:?}");
        for name in writes {
            for blank in ["", "   ", "\n\t "] {
                let e = s
                    .call_tool(name, &json!({ "id": id, "summary": blank }))
                    .expect_err(name);
                assert!(
                    e.contains("`summary`") && e.contains("empty"),
                    "{name}: {e}"
                );
            }
        }
        let e = s
            .call_tool(
                "update_edges",
                &json!({ "id": id, "summary": " ", "changes": [{ "index": 0, "pitch_deg": 30 }] }),
            )
            .unwrap_err();
        assert!(e.contains("summary"), "{e}");
        assert_eq!(s.store.load(&id).unwrap(), before, "nothing written");
        assert_eq!(s.store.activity(&id).len(), log, "nothing logged");
        // A real sentence still works.
        call(
            &mut s,
            "update_edges",
            json!({ "id": id, "summary": "Flatter wall 1", "changes": [{ "index": 0, "pitch_deg": 30 }] }),
        );
        let _ = std::fs::remove_dir_all(&s.store.dir);
    }

    #[test]
    fn deleted_jobs_and_files_go_to_the_trash_and_come_back() {
        // ARCH-004: delete_project and remove_file deleted for good.
        let (mut s, id) = setup();
        let plan = s.store.dir.join("plan.pdf");
        std::fs::write(&plan, b"%PDF").unwrap();
        call(
            &mut s,
            "attach_file",
            json!({ "id": id, "path": plan, "summary": "Drawing" }),
        );
        assert!(
            s.call_tool("delete_project", &json!({ "id": id })).is_err(),
            "no summary"
        );
        assert!(s.store.exists(&id));
        let r = call(
            &mut s,
            "remove_file",
            json!({ "id": id, "name": "plan.pdf", "summary": "Wrong drawing" }),
        );
        let file_entry = r["trash"]["entry"].as_str().unwrap().to_string();
        assert!(s.store.attachments(&id).is_empty());
        let r = call(
            &mut s,
            "delete_project",
            json!({ "id": id, "summary": "The user asked to remove the old draft" }),
        );
        let job_entry = r["trash"]["entry"].as_str().unwrap().to_string();
        assert!(!s.store.exists(&id));
        let deleted = call(&mut s, "list_deleted", json!({}));
        let entries: Vec<&str> = deleted
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["entry"].as_str().unwrap())
            .collect();
        assert!(entries.contains(&job_entry.as_str()) && entries.contains(&file_entry.as_str()));
        assert_eq!(
            deleted[0]["summary"],
            "The user asked to remove the old draft"
        );
        let r = call(
            &mut s,
            "restore_project",
            json!({ "entry": job_entry, "summary": "Deleted by mistake" }),
        );
        assert_eq!(r["id"], id.as_str());
        assert!(s.store.exists(&id));
        assert!(!s.store.activity(&id).is_empty(), "history comes back too");
        call(
            &mut s,
            "restore_project",
            json!({ "entry": file_entry, "summary": "Needed after all" }),
        );
        assert_eq!(s.store.attachments(&id).len(), 1);
        assert!(
            call(&mut s, "list_deleted", json!({}))
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(
            s.call_tool(
                "restore_project",
                &json!({ "entry": "../x", "summary": "s" })
            )
            .is_err()
        );
        let _ = std::fs::remove_dir_all(&s.store.dir);
    }

    #[test]
    fn a_damaged_or_newer_job_is_named_as_such_not_missing() {
        // ERR-008: the store's reason was replaced by "no design with id".
        let (mut s, _) = setup();
        std::fs::write(s.store.dir.join("damaged.tectora"), "{ \"name\": ").unwrap();
        std::fs::write(
            s.store.dir.join("newer.tectora"),
            r#"{"schema_version": 999, "name": "x", "parts": []}"#,
        )
        .unwrap();
        let e = s
            .call_tool("get_project", &json!({ "id": "damaged" }))
            .unwrap_err();
        assert!(e.contains("damaged") && e.contains("unreadable"), "{e}");
        let e = s
            .call_tool(
                "update_edges",
                &json!({ "id": "newer", "changes": [], "summary": "s" }),
            )
            .unwrap_err();
        assert!(
            e.contains("newer Tectora") && e.contains("newer_format"),
            "{e}"
        );
        let e = s
            .call_tool("get_project", &json!({ "id": "nope" }))
            .unwrap_err();
        assert!(e.contains("list_projects"), "{e}");
        let list = call(&mut s, "list_projects", json!({}));
        let broken = list["broken"].as_array().unwrap();
        let ids: Vec<&str> = broken.iter().map(|b| b["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["damaged", "newer"]);
        assert_eq!(list["projects"].as_array().unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&s.store.dir);
    }
}
