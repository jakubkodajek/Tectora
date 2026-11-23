//! The tools that give agents every operation of the editor: walls and
//! parts, transforms, dormers, hand-drawn lines and faces, roof options, the
//! calculation helpers, history, the library, attached files, job bundles,
//! the stored result and the file schema. The geometry comes from
//! [`tectora_core::edit`], the same code the app's dialogs use.

use std::path::PathBuf;

use serde_json::{Value, json};
#[cfg(test)]
use tectora_core::edit::apply_patches;
#[cfg(test)]
use tectora_core::edit::calc as hc;
use tectora_core::edit::calc::{BuildUpEave, Target};
use tectora_core::edit::corners::corner_group;
use tectora_core::edit::dormers::{self, DormerKind, DormerParams, SectionTarget};
use tectora_core::edit::helper::{self, HelperRequest, HipBy, Neighbour, ParallelStep};
use tectora_core::edit::openings::{OpeningBox, opening_box, reshape_opening};
use tectora_core::edit::probe::{host_height_at, roof_corner_points, roof_height_at, snap_to_roof};
use tectora_core::edit::round::{ArcMode, MAX_ROUND_SEGMENTS};
use tectora_core::edit::transform::{
    ItemKind, ItemRef, MAX_COPIES, MAX_POLYGON_SIDES, PlanMap, follow_roof, item_centre,
    offset_wall, part_centre, regular_polygon, transform_items, transform_part,
};
use tectora_core::edit::{Patches, plain_edge};
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

/// `line_names` of `export_plan`: code → name, at most 64 codes of up to 8
/// characters and names of up to 60 characters.
fn line_names_arg(args: &Value) -> Result<std::collections::BTreeMap<String, String>, String> {
    let names: std::collections::BTreeMap<String, String> =
        opt(args, "line_names")?.unwrap_or_default();
    if names.len() > 64
        || names
            .iter()
            .any(|(k, v)| k.chars().count() > 8 || v.chars().count() > 60)
    {
        return Err(
            "`line_names`: at most 64 codes of up to 8 characters, names up to 60 characters"
                .into(),
        );
    }
    // One line of plain text each, as every label (HUNT-303).
    for (k, v) in &names {
        tectora_core::edit::check_label("`line_names`", k)?;
        tectora_core::edit::check_label("`line_names`", v)?;
    }
    Ok(names)
}

/// `stamp` of `export_list` and `export_plan`: one line of plain text of
/// at most 40 characters (a control character made the SVG sheet invalid
/// XML, HUNT-303); default now (UTC).
fn stamp_arg(args: &Value) -> Result<String, String> {
    match args.get("stamp").and_then(Value::as_str) {
        Some(s) if s.chars().count() > 40 => {
            Err("`stamp` is too long (at most 40 characters)".into())
        }
        Some(s) => {
            tectora_core::edit::check_label("`stamp`", s)?;
            Ok(s.to_string())
        }
        None => Ok(tectora_core::export::utc_stamp(
            tectora_core::store::now_ms(),
        )),
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
    let pi = crate::part_arg(args)?;
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

/// A dormer as the parameters of the dormer dialog, with its side section
/// on `host` (the roof without dormers, [`dormers::host_roof`]).
pub(crate) fn describe_dormer(
    index: usize,
    d: &tectora_core::Dormer,
    host: &[tectora_core::roof::RoofGeometry],
) -> Value {
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
        "side_overhang": p.side_overhang,
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
    if p.kind != DormerKind::General {
        v["section"] = describe_section(&p, host);
    }
    v
}

/// DACAD's dormer dialog section, rounded to mm; null off the roof.
fn describe_section(p: &DormerParams, host: &[tectora_core::roof::RoofGeometry]) -> Value {
    let Some(h) = dormers::host_slope_in(host, p) else {
        return Value::Null;
    };
    let s = dormers::section(p, h);
    let r = |v: Option<f64>| v.map(|v| round(v, 3));
    json!({
        "roof_height": round(h.height, 3),
        "roof_pitch_deg": round(s.host_pitch_deg, 2),
        "wall_height": round(s.wall_height, 3),
        "depth": r(s.depth),
        "ridge_height": r(s.ridge_height),
        "ridge_length": r(s.ridge_length),
        "roof_length": r(s.roof_length),
        "top_height": r(s.top_height),
        "roof_slope_length": r(s.host_length),
        "back_width": r(s.back_width),
    })
}

fn dormer_props() -> Value {
    json!({
        "kind": { "type": "string", "enum": ["gable", "pointed", "hip", "half_hip", "shed", "trapezoid", "round"], "description": "Dormer type: gable (Satteldach), pointed (Spitzgaube), hip (Walm), half_hip (Krüppelwalm), shed (Schleppgaube), trapezoid, round (Rundgaube)." },
        "start": { "type": "array", "items": { "type": "number" }, "description": "Plan point [x, y] of the front's left corner seen from outside, on the roof." },
        "width": { "type": "number", "description": "Front width, m (default 2)." },
        "angle_deg": { "type": "number", "description": "Direction of the front wall, degrees; the dormer roof rises to its left. Default: along the nearest eave wall." },
        "eave_height": { "type": "number", "description": "Dormer eave height above ground, m. Default: roof height at `start` + 1 m. Instead give one of wall_height, depth or ridge_length." },
        "wall_height": { "type": "number", "description": "Front wall height above the roof at the middle of the front, m (as the dormer dialog); sets eave_height." },
        "depth": { "type": "number", "description": "Horizontal distance from the front to where the dormer's ridge (shed: its roof) runs into the roof, m; sets eave_height." },
        "ridge_length": { "type": "number", "description": "Ridge length from its front end to the roof, m (not for shed); sets eave_height." },
        "pitch_deg": { "type": "number", "description": "Pitch of the cheek planes (gable, pointed, hip, half_hip, trapezoid), default 45." },
        "front_pitch_deg": { "type": "number", "description": "Pitch of the front plane (shed, trapezoid), default 15." },
        "overhang": { "type": "number", "description": "Overhang of the front and the cheeks together, m (default 0); front_overhang and side_overhang set them apart." },
        "front_overhang": { "type": "number", "description": "Overhang of the front only, m (the dialog's front overhang)." },
        "side_overhang": { "type": "number", "description": "Overhang of the cheeks only, m; the dormer roof is width + 2 × side_overhang wide." },
        "back_width": { "type": "number", "description": "trapezoid: width of the shed roof where it meets the roof, m, between 0 and width + 2 × side_overhang; sets pitch_deg (the cheek pitch), so do not give both." },
        "half_hip_height": { "type": "number", "description": "half_hip: height of the clipped part above the dormer eave, default 0.6." },
        "half_hip_pitch_deg": { "type": "number", "description": "half_hip: its pitch, default 60." },
        "segments": { "type": "integer", "description": "round: polygon segments per side; 0 = automatic. Too many for the width (the flattest segment under 2°) is refused with `dormer_segments`." },
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
        ("half_hip_height", &mut p.half_hip_height),
        ("half_hip_pitch_deg", &mut p.half_hip_pitch_deg),
    ] {
        if let Some(v) = arg_f64(args, key)? {
            *slot = v;
        }
    }
    if let Some(v) = arg_f64(args, "overhang")? {
        p.overhang = v;
        p.side_overhang = v;
    }
    if let Some(v) = arg_f64(args, "front_overhang")? {
        p.overhang = v;
    }
    if let Some(v) = arg_f64(args, "side_overhang")? {
        p.side_overhang = v;
    }
    for (name, v) in [("overhang", p.overhang), ("side_overhang", p.side_overhang)] {
        if !(v.is_finite() && v >= 0.0) {
            return Err(format!("{name} must be 0 or more"));
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
        ("half_hip_pitch_deg", p.half_hip_pitch_deg),
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

/// Refuses an edit that adds an input problem (`Project::validate`) the
/// job did not have before. A problem already there (a zero-length wall
/// from `edit_walls`, which saves, leaving only that building out) does not
/// block edits of other items (HUNTC-005); indices in the messages are from
/// 0, as in every tool argument.
pub(crate) fn refuse_new_problems(before: &Project, after: &Project) -> Result<(), String> {
    let Err(now) = after.validate() else {
        return Ok(());
    };
    let was = before.validate().err().unwrap_or_default();
    let new: Vec<String> = now.into_iter().filter(|m| !was.contains(m)).collect();
    if new.is_empty() {
        Ok(())
    } else {
        Err(new.join("; "))
    }
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
    const CODES: [&str; 15] = tectora_core::model::MANUAL_LINE_CODES;
    if !CODES.contains(&code.as_str()) {
        return Err(format!("code `{code}` is not one of {}", CODES.join(", ")));
    }
    let length = arg_f64(v, "length")?;
    if let Some(l) = length {
        tectora_core::edit::check_manual_line_length(l)?;
    }
    Ok(ManualLine {
        a: end("a")?,
        b: end("b")?,
        code,
        length,
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
            "description": "Edit the walls of a part step by step, as in the plan editor. Operations, applied in order: move_point {index, x, y}; insert_point {after, x, y} (new corner on wall `after`, which keeps its roof definition); delete_point {index} (a part keeps at least 3 walls); set_length {index, length} (the angles stay: the end corner and the corners after it up to the next parallel wall move along the wall, so a rectangle stays a rectangle; with no parallel wall only the end corner moves); insert_direction_change {index} (a zero-length marker wall at the wall's start that turns the roof, DACAD Richtungswechsel); shift_wall {index, distance} (moves the wall parallel, + outward, its neighbours stretched); rename {name}.",
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
                "copies": { "type": "integer", "minimum": 0, "maximum": 50, "description": "Copies to add (a mirror: 0 or 1)." },
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
            "name": "place_item",
            "description": "Put a dormer, chimney, roof window or roof opening at given distances on the roof face it stands on, as the editor's dimension fields (\"From eave\", \"From left\" / \"From right\"): `from_eave` is measured along the slope from the lowest edge of the face (the eave at the overhang, or a pitch break) to the object's lowest point; `from_left` or `from_right` level along the eave from the face's edge (verge, hip, valley) to the object, at the height of its lowest point, left and right as seen looking up the slope. An omitted `from_eave` keeps the distance from the eave; omitted side values keep the place along the eave (on a hip or valley face, whose sides slant, the side distances then change). The object must stay on the same face with each given value reached, else nothing is saved and the error says why (distances are 0 or more). The object keeps its height above the roof. get_project lists each object's current `placement`.",
            "inputSchema": schema(json!({
                "id": id,
                "kind": { "type": "string", "enum": ["dormer", "chimney", "window", "roof_opening"] },
                "index": { "type": "integer", "minimum": 0 },
                "from_eave": { "type": "number", "description": "m along the slope from the eave edge of the face." },
                "from_left": { "type": "number", "description": "m from the left edge of the face (give this or from_right)." },
                "from_right": { "type": "number", "description": "m from the right edge of the face." },
                "summary": summary_prop(),
            }), &["id", "kind", "index", "summary"]),
        }),
        json!({
            "name": "update_opening",
            "description": "Change the position or size of one chimney, roof opening or roof window, as the object bar of the editor does. A chimney or roof opening is edited through its box: the smallest box around its plan polygon that lines up with its first side, with corner `x`, `y`, `width` along the first side, `depth` across it and `rotation_deg` (direction of the first side); every corner keeps its place in the box, so a rectangle stays a rectangle and any other shape is stretched. `height` is a chimney's top (m) or a window's height up the slope. Omitted fields keep their value; the reply has the new box.",
            "inputSchema": schema(json!({
                "id": id,
                "kind": { "type": "string", "enum": ["chimney", "roof_opening", "window"] },
                "index": { "type": "integer", "minimum": 0 },
                "x": { "type": "number" }, "y": { "type": "number" },
                "width": { "type": "number" }, "depth": { "type": "number" },
                "rotation_deg": { "type": "number" },
                "height": { "type": "number" },
                "summary": summary_prop(),
            }), &["id", "kind", "index", "summary"]),
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
            "description": "Set the height of a building (part) in one change, as the editor's \"Building height\" and \"Eave height\" fields. Give `height` for the building height (top of the walls, m above ground): the roof moves with it, every wall's eave height, pitch breaks and half-hips shift by the change, and so do the dormers, chimneys and hand-drawn lines on it; each wall keeps its own eave below or above the wall top. The building height itself changes no roof figure (the roof is calculated from the eave heights; an eave is measured at the overhang line and is usually lower than the building). Give `eave_height` instead to put every wall of the part at that eave height (breaks, half-hips and objects move with them) and keep the building height. Where buildings of different heights meet, the roofs are joined by their eave heights, so use it to make a wing lower or higher than the main roof. To change only some walls, use update_edges.",
            "inputSchema": schema(json!({
                "id": id,
                "part": { "type": "integer", "minimum": 0, "description": "Part index (as get_project lists them)." },
                "height": { "type": "number", "minimum": tectora_core::edit::height::MIN_PART_HEIGHT, "maximum": tectora_core::edit::height::MAX_PART_HEIGHT, "description": "Building height (top of the walls), m above ground. Give this or eave_height." },
                "eave_height": { "type": "number", "minimum": tectora_core::edit::height::MIN_PART_HEIGHT, "maximum": tectora_core::edit::height::MAX_PART_HEIGHT, "description": "Eave height of every wall, m above ground. Give this or height." },
                "summary": summary_prop(),
            }), &["id", "part", "summary"]),
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
                    "length": { "type": "number", "minimum": 0, "description": "Length to count instead of the measured one (0 or more)." },
                    "label": { "type": "string" }
                }, "required": ["a", "b"] } },
                "faces": { "type": "array", "items": { "type": "object", "properties": {
                    "points": { "type": "array", "items": { "type": "array", "items": { "type": "number" } } },
                    "pitch_deg": { "type": "number", "minimum": 0, "maximum": 89.9 }, "label": { "type": "string" }
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
            "description": "The change log of a design: who changed what and when (ts_ms), newest first, at most the 50 changes a log keeps. `stored_result_recalculation: true` marks Tectora's own recalculation of an offer's stored result while the job's stored result still is that one (restore_offer_result puts the offer's figures back); `restores` on an entry is the ts_ms of the recalculation it put back.",
            "inputSchema": schema(json!({ "id": id }), &["id"]),
        }),
        json!({
            "name": "get_review",
            "description": "Whether the user has reviewed your changes to a design in the app. `pending` is null when every change is accepted or reverted; otherwise it gives the oldest unreviewed change (`since_ts_ms`), how many there are, who made them and how they change the figures (`differences`: roof_area, ridge_height, line:<code>, material:<n>, before → now). Only the user accepts changes, in the app.",
            "inputSchema": schema(json!({ "id": id }), &["id"]),
        }),
        json!({
            "name": "revert_to",
            "description": "Undo every change of a design made at or after `ts_ms` (from get_history). Refused with `busy` (nothing done, try again in a moment) while another program is saving the job.",
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
            "inputSchema": schema(json!({ "id": id, "path": { "type": "string", "description": "Absolute path of the file. A file outside the jobs folder and the drawings inbox is refused until the user allows it in the app." }, "summary": summary_prop() }), &["id", "path", "summary"]),
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
            "description": "Write the roof sheet (A4 landscape plan with line lengths, face areas, pitches and totals; the picture the user adds to the Excel price sheet) to the `exports` folder of the job library as PDF or SVG (a PNG can only be saved from the app's Export dialog; there is no PNG here). The file is named after the job (or `name`); an existing file is never replaced. Returns the path to give the user.",
            "inputSchema": schema(json!({
                "id": id,
                "format": { "type": "string", "enum": ["pdf", "svg"], "description": "Default: pdf." },
                "name": { "type": "string", "description": "File name without folder or extension; default: the job's id." },
                "stamp": { "type": "string", "description": "Date on the sheet; default: now (UTC)." },
                "line_names": { "type": "object", "additionalProperties": { "type": "string" }, "description": "Names of the line codes for the legend at the sheet's foot, in the user's language, e.g. {\"T\": \"Okapy\", \"WV\": \"Napojení čelní\"}; codes without a name are listed without one. Default: no legend." },
                "summary": summary_prop(),
            }), &["id", "summary"]),
        }),
        json!({
            "name": "import_job",
            "description": "Add a job from a .tectorajob file, or a .tectora project file from outside the library, as a new design.",
            "inputSchema": schema(json!({ "path": { "type": "string", "description": "Absolute path of the file. A file outside the jobs folder and the drawings inbox is refused until the user allows it in the app." }, "summary": summary_prop() }), &["path", "summary"]),
        }),
        json!({
            "name": "store_result",
            "description": "Store the current result as the job's result (the figures for the offer), replacing the stored one. The result is stored by itself when the job leaves the draft status.",
            "inputSchema": schema(json!({ "id": id, "summary": summary_prop() }), &["id", "summary"]),
        }),
        json!({
            "name": "restore_offer_result",
            "description": "Put the offer's own stored figures back in place of Tectora's recalculation of them (the entry by `tectora` that get_history marks `stored_result_recalculation`). Only the stored result changes: the design and every other change stay. The figures put back are never recalculated again. Refused with `revert_stale` when the stored result is not that recalculation (any more), and with `busy` (nothing done, try again in a moment) while another program is saving the job. Do it only when the user asks for the figures the customer was offered.",
            "inputSchema": schema(json!({ "id": id, "ts_ms": { "type": "integer", "description": "The recalculation's entry (get_history); left out: the one the stored result is." }, "summary": summary_prop() }), &["id", "summary"]),
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
            "update_opening" => self.update_opening(args),
            "place_item" => (|| {
                let (id, before) = self.load(args)?;
                let mut p = before.clone();
                let kind: tectora_core::edit::transform::ItemKind = serde_json::from_value(
                    args.get("kind").cloned().unwrap_or(Value::Null),
                )
                .map_err(|_| "`kind` must be dormer, chimney, window or roof_opening".to_string())?;
                let index: usize = req(args, "index")?;
                let it = tectora_core::edit::transform::ItemRef { kind, index };
                let moved = tectora_core::edit::position::place(
                    &mut p,
                    it,
                    arg_f64(args, "from_eave")?,
                    arg_f64(args, "from_left")?,
                    arg_f64(args, "from_right")?,
                )?;
                if !moved {
                    return Err("the object already is at these distances".into());
                }
                refuse_new_problems(&before, &p)?;
                let r = self.commit(Some(&id), Some(&before), &p, arg_str(args, "summary")?)?;
                let pl = tectora_core::edit::position::placement(&p, it);
                Ok(json!({ "saved": r, "placement": pl }))
            })(),
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
                refuse_new_problems(&before, &p)?;
                self.commit(Some(&id), Some(&before), &p, arg_str(args, "summary")?)
            })(),
            "set_part_height" => (|| {
                let (id, before) = self.load(args)?;
                let mut p = before.clone();
                let part: usize = req(args, "part")?;
                match (arg_f64(args, "height")?, arg_f64(args, "eave_height")?) {
                    (Some(h), None) => {
                        if !tectora_core::edit::height::set_part_height(&mut p, part, h)? {
                            return Err(format!("part {part} already is {h} m high"));
                        }
                    }
                    (None, Some(h)) => {
                        if !tectora_core::edit::height::set_part_eave_height(&mut p, part, h)? {
                            return Err(format!("every wall of part {part} already has eave height {h} m"));
                        }
                    }
                    _ => return Err("give one of `height` (building) or `eave_height` (every wall)".into()),
                }
                refuse_new_problems(&before, &p)?;
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
                let recalculation = self.store.recalculation(&id).map(|r| r.entry_ts_ms);
                json!(self
                    .store
                    .history(&id)
                    .iter()
                    .rev()
                    .map(|e| {
                        let mut v = json!(e.info());
                        v["pending_review"] = json!(e.ts_ms >= since);
                        if recalculation == Some(e.ts_ms) {
                            v["stored_result_recalculation"] = json!(true);
                        }
                        if let Some(ts) = e.restores {
                            v["restores"] = json!(ts);
                        }
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
                let p = self
                    .store
                    .revert_to(&id, ts, &self.client, Some(summary))
                    .map_err(super::coded)?;
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
                        // A number sent as a JSON number is refused, not dropped (ERR-002).
                        let field = |k: &str| {
                            crate::text_arg(j, k)
                                .map(Option::unwrap_or_default)
                                .map_err(|m| format!("job.{m}"))
                        };
                        Some(tectora_core::JobInfo {
                            number: field("number")?,
                            customer: field("customer")?,
                            address: field("address")?,
                            status: tectora_core::JobStatus::Draft,
                        })
                    }
                };
                tectora_core::edit::check_job_name(arg_str(args, "name")?)?;
                let original = p.clone();
                tectora_core::library::prepare_copy(&mut p, arg_str(args, "name")?, job);
                // The copy's new job texts keep the rules of `update_job`
                // (ARCH-202); what the original had is copied as it is.
                tectora_core::edit::check_texts_change(Some(&original), &p)?;
                let summary = arg_str(args, "summary")?;
                let with_files = args.get("with_files").and_then(Value::as_bool).unwrap_or(true);
                // A refused copy leaves no new job (ARCH-602).
                let new_id = self.store.copy_job(&id, &p, with_files, Some((&self.client, summary)))?;
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
            "list_files" => self.load(args).and_then(|(id, _)| {
                // Never the files where a link in the library points (ARCH-602).
                Ok(json!(self
                    .store
                    .try_attachments(&id)?
                    .iter()
                    .map(|a| json!({
                        "name": a.name, "size": a.size, "modified_ms": a.modified_ms,
                        "path": self.store.attachment_path(&id, &a.name).map(|p| p.display().to_string()).unwrap_or_default(),
                    }))
                    .collect::<Vec<_>>()))
            }),
            "attach_file" => (|| {
                let (id, _) = self.load(args)?;
                let summary = arg_str(args, "summary")?;
                let src = PathBuf::from(arg_str(args, "path")?);
                self.readable_path(&src, "attach")?;
                // Drawings, photos and documents only, the rule the app
                // shares (`Store::attach_file`): a path from text in a
                // drawing must not copy a key or a program into a job that
                // is later handed over (ARCH-001, HUNTIO-001).
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
                let stamp = stamp_arg(args)?;
                let title = if p.name.is_empty() { id.clone() } else { p.name.clone() };
                let opts = tectora_core::export::ListOptions::new(title, stamp);
                p.check_limits()?;
                crate::export_ready(&p)?;
                // The list alone: the sheet is not drawn (HUNTC-001).
                let list = tectora_core::export::checked_list(&p, &opts)?;
                Ok(json!({ "id": id, "list": list }))
            })(),
            "export_plan" => (|| {
                let (id, p) = self.load(args)?;
                let summary = arg_str(args, "summary")?;
                p.check_limits()?;
                crate::export_ready(&p)?;
                if args.get("path").is_some() {
                    return Err("`path` is not accepted: sheets are written to the library's exports folder; pass `name` for the file name".into());
                }
                let format = args.get("format").and_then(Value::as_str).unwrap_or("pdf");
                if format.eq_ignore_ascii_case("png") {
                    // The app rasterises the sheet in its web view; the core has no
                    // rasteriser (ARCH-009).
                    return Err("`png` is not available over MCP: the app draws the PNG in its own window. Use `svg` (the same sheet, opens in any browser or image program) or `pdf`, or tell the user to press PNG in the app's Export dialog".into());
                }
                if format != "pdf" && format != "svg" {
                    return Err(format!("`format` must be pdf or svg, not `{format}`"));
                }
                let stamp = stamp_arg(args)?;
                let name = match args.get("name").and_then(Value::as_str) {
                    Some(n) => n.trim().trim_end_matches(&format!(".{format}")).to_string(),
                    None => id.clone(),
                };
                // Checked before the sheet is drawn (a bad name, a linked folder).
                bundle::export_path_with(&self.store, &name, format)?;
                let line_names = line_names_arg(args)?;
                let bytes =
                    tectora_core::export::checked_sheet_file_named(&p, &stamp, format, &line_names)?;
                // Never through a link at the name (ARCH-603).
                let path = bundle::write_export(&self.store, &name, format, &bytes)?;
                self.live("exported_sheet", Some(&path.display().to_string()), format!("Exported sheet to {}: {summary}", path.display()), Some(&id));
                Ok(json!({ "id": id, "path": path.display().to_string() }))
            })(),
            "import_job" => (|| {
                let path = PathBuf::from(arg_str(args, "path")?);
                let summary = arg_str(args, "summary")?;
                self.readable_path(&path, "import")?;
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
            "restore_offer_result" => (|| {
                let (id, _) = self.load(args)?;
                let summary = arg_str(args, "summary")?;
                let ts: u64 = match opt(args, "ts_ms")? {
                    Some(ts) => ts,
                    None => self
                        .store
                        .recalculation(&id)
                        .map(|r| r.entry_ts_ms)
                        .ok_or("the job's stored result is not a recalculation by Tectora: there are no offer figures to put back (see get_history)")?,
                };
                let p = self
                    .store
                    .restore_offer_result(&id, ts, &self.client, Some(summary))
                    .map_err(|e| format!("{}: {e}", e.code()))?;
                self.live_follow("changed", Some(&p.name), summary.to_string(), &id);
                Ok(json!({
                    "id": id,
                    "restored": true,
                    "stored_result": p.result_snapshot,
                }))
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
                    // As the wall bar's length field: the angles stay (UX-005).
                    if !tectora_core::edit::corners::set_wall_length(&mut part.edges, i, len) {
                        return Err(at("the wall is a direction change or has no length".into()));
                    }
                }
                "insert_direction_change" => {
                    let i = edge_index(op, "index", n).map_err(at)?;
                    let e = part.edges[i].clone();
                    let ang = tectora_core::edit::corners::default_direction(&part.edges, i);
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
                    marker.direction_rad = Some(ang);
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
        // The second mirror image is the part itself, stacked on it (HUNTC-001).
        if matches!(m, PlanMap::Mirror { .. }) && copies > 1 {
            return Err("a mirror adds at most 1 copy (a second mirror image lies on the building itself); use copies 0 or 1, or `rotate` / `move` with copies for a row".into());
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

    fn update_opening(&self, args: &Value) -> ToolResult {
        let (id, before) = self.load(args)?;
        let mut p = before.clone();
        let index: usize = req(args, "index")?;
        let kind = arg_str(args, "kind")?;
        let num = |k: &str| -> Result<Option<f64>, String> {
            match arg_f64(args, k)? {
                Some(v) if !v.is_finite() => Err(format!("`{k}` must be a finite number")),
                v => Ok(v),
            }
        };
        let reply_box = match kind {
            "window" => {
                if num("depth")?.is_some() || num("rotation_deg")?.is_some() {
                    return Err("a roof window has x, y, width and height (up the slope)".into());
                }
                let w = p
                    .windows
                    .get_mut(index)
                    .ok_or_else(|| format!("window {index} does not exist"))?;
                w.x = num("x")?.unwrap_or(w.x);
                w.y = num("y")?.unwrap_or(w.y);
                w.width = num("width")?.unwrap_or(w.width);
                w.height = num("height")?.unwrap_or(w.height);
                if !(w.width >= 0.01 && w.height >= 0.01 && w.width <= 100.0 && w.height <= 100.0) {
                    return Err("width and height must be between 0.01 and 100 m".into());
                }
                let out = json!({ "x": w.x, "y": w.y, "width": w.width, "height": w.height });
                tectora_core::edit::check_windows(&p).map_err(|e| e.to_string())?;
                out
            }
            "chimney" | "roof_opening" => {
                let list = if kind == "chimney" {
                    &mut p.chimneys
                } else {
                    &mut p.roof_openings
                };
                let c = list
                    .get_mut(index)
                    .ok_or_else(|| format!("{kind} {index} does not exist"))?;
                let was = opening_box(&c.corners)
                    .ok_or_else(|| format!("{kind} {index} has no area to resize"))?;
                let to = OpeningBox {
                    x: num("x")?.unwrap_or(was.x),
                    y: num("y")?.unwrap_or(was.y),
                    width: num("width")?.unwrap_or(was.width),
                    depth: num("depth")?.unwrap_or(was.depth),
                    rotation_deg: num("rotation_deg")?.unwrap_or(was.rotation_deg),
                };
                c.corners = reshape_opening(&c.corners, &to)?;
                if let Some(h) = num("height")? {
                    if kind != "chimney" {
                        return Err("a roof opening has no height".into());
                    }
                    c.height = h;
                }
                json!(opening_box(&c.corners))
            }
            other => return Err(format!("unknown kind `{other}`")),
        };
        let mut reply = self.commit(Some(&id), Some(&before), &p, arg_str(args, "summary")?)?;
        if let Some(o) = reply.as_object_mut() {
            o.insert("box".into(), reply_box);
        }
        Ok(reply)
    }

    fn add_shape_part(&self, args: &Value) -> ToolResult {
        let (id, before) = self.load(args)?;
        let mut p = before.clone();
        let preset = tectora_core::settings::Settings::load().roof_defaults;
        let pitch = arg_f64(args, "pitch_deg")?.unwrap_or(preset.pitch_deg);
        let eave = arg_f64(args, "eave_height")?.unwrap_or(preset.eave_height);
        let overhang = arg_f64(args, "overhang")?.unwrap_or(preset.overhang);
        tectora_core::edit::check_new_wall(pitch, eave, overhang)?;
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
            // The standard-roof form's side bounds (HUNTC-007): a side of
            // 1 cm left walls without length after the corners' rounding.
            tectora_core::edit::check_standard_size(side, side)
                .map_err(|m| m.replacen("length", "side", 1))?;
            let c = at.unwrap_or(Point2::new(0.0, 0.0));
            let pts = regular_polygon(
                c,
                sides,
                side,
                arg_f64(args, "rotation_deg")?.unwrap_or(0.0),
            );
            RoofPart {
                height: None,
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
            tectora_core::edit::check_standard_size(length, width)?;
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
        // As every other creating tool: no new input problem (HUNTC-007).
        refuse_new_problems(&before, &p)?;
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
        // Highest index first within each kind, so the others keep theirs. A
        // repeated (kind, index) must be adjacent to be dropped, so sort by
        // both (ERR-001).
        list.sort();
        list.dedup();
        list.reverse();
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
        let targets: Vec<SectionTarget> = [
            (
                "wall_height",
                SectionTarget::WallHeight as fn(f64) -> SectionTarget,
            ),
            ("depth", SectionTarget::Depth),
            ("ridge_length", SectionTarget::RidgeLength),
        ]
        .into_iter()
        .filter_map(|(k, f)| arg_f64(args, k).transpose().map(|v| v.map(f)))
        .collect::<Result<_, _>>()?;
        let target = match targets.as_slice() {
            [] => None,
            [t] => {
                if args.get("eave_height").is_some() {
                    return Err(
                        "give eave_height or one of wall_height, depth, ridge_length, not both"
                            .into(),
                    );
                }
                Some(*t)
            }
            _ => return Err("give only one of wall_height, depth, ridge_length".into()),
        };
        let back_width = arg_f64(args, "back_width")?;
        if back_width.is_some() {
            if params.kind != DormerKind::Trapezoid {
                return Err("back_width is only for a trapezoid dormer".into());
            }
            if args.get("pitch_deg").is_some() {
                return Err(
                    "give back_width or pitch_deg, not both: the back width sets the cheek pitch"
                        .into(),
                );
            }
        }
        if target.is_some() || back_width.is_some() {
            let host = dormers::host_slope_in(&dormers::host_roof(&p), &params)
                .ok_or("the dormer's front is not on the roof, so wall_height, depth, ridge_length and back_width cannot be used; give eave_height and pitch_deg")?;
            params = dormers::solve(&params, host, target, back_width)?;
        }
        // The pitch and eave rules of the editor's dormer bar (MATH-001,
        // MATH-003): a dormer flatter than 2° or below the roof is left out
        // or takes the building's roof with it.
        let host = dormers::host_slope_in(&dormers::host_roof(&p), &params);
        dormers::check_params(&params, host).map_err(|(code, why)| format!("{code}: {why}"))?;
        let mut d = dormers::build_dormer(&params);
        if let Some(old) = p.dormers.get(index) {
            d.host_wall = old.host_wall;
            p.dormers[index] = d;
        } else {
            p.dormers.push(d);
        }
        let mut out = self.commit(Some(&id), Some(&before), &p, arg_str(args, "summary")?)?;
        out["dormer"] = describe_dormer(index, &p.dormers[index], &dormers::host_roof(&p));
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
                    tectora_core::edit::check_manual_face_pitch(pitch_deg)
                        .map_err(|m| format!("face {i}: {m}"))?;
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
        let target: Option<Target> = opt(args, "target")?;
        let need_target = || {
            target.ok_or_else(|| "missing `target` (pitch, eave, overhang or position)".to_string())
        };
        let req = match arg_str(args, "tool")? {
            "triangle" => HelperRequest::Triangle {
                width: arg_f64(args, "width")?,
                rafter: arg_f64(args, "rafter")?,
                ridge: arg_f64(args, "ridge")?,
                width_includes_overhang: opt(args, "width_includes_overhang")?.unwrap_or(false),
            },
            "parallel" => {
                let target = need_target()?;
                let steps: Vec<Value> = req(args, "steps")?;
                let mut parsed = Vec::with_capacity(steps.len());
                for s in &steps {
                    parsed.push(ParallelStep {
                        field: req(s, "field")?,
                        value: arg_f64(s, "value")?.ok_or("missing `value`")?,
                    });
                }
                HelperRequest::Parallel {
                    target,
                    steps: parsed,
                    width_includes_overhang: opt(args, "width_includes_overhang")?.unwrap_or(true),
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
                HelperRequest::Point { target, point: q }
            }
            "merge" => HelperRequest::Merge {
                target: need_target()?,
                walls: opt(args, "walls")?,
            },
            "hip" => HelperRequest::Hip {
                // Refused unless one of the schema's values (ERR-005): a
                // misspelt neighbour gave the other wall's pitch.
                neighbour: opt::<Neighbour>(args, "neighbour")?.unwrap_or_default(),
                by: opt::<HipBy>(args, "by")?.unwrap_or_default(),
                hip: arg_f64(args, "hip")?.ok_or("missing `hip`")?,
                first: arg_f64(args, "first")?.ok_or("missing `first`")?,
                second: arg_f64(args, "second")?.ok_or("missing `second`")?,
            },
            "build_up" => HelperRequest::BuildUp {
                thickness: arg_f64(args, "thickness")?.ok_or("missing `thickness`")?,
                eave: opt::<BuildUpEave>(args, "eave")?.unwrap_or_default(),
            },
            "round_roof" => HelperRequest::RoundRoof {
                eave_height: None,
                overhang: None,
                segments: opt(args, "segments")?.unwrap_or(0),
                mode: opt::<ArcMode>(args, "mode")?.unwrap_or_default(),
                value: arg_f64(args, "value")?.unwrap_or(0.0),
            },
            other => return Err(format!("unknown tool `{other}`")),
        };
        let helped = helper::run(&before, pi, edge, &req)?;
        let mut p = before.clone();
        // A helper that sets eave heights keeps the building's height, as
        // update_edges does (review ARCH-001).
        let applied = helper::apply_to_part(&mut p, pi, &helped.patches);
        // The wall bar's ranges, as the dialog's OK (ARCH-003).
        let refused = helper::check_applied(&before.parts[pi].edges, &p, pi, &applied).err();
        let apply = args.get("apply").and_then(Value::as_bool) == Some(true);
        if apply && let Some(why) = refused.clone() {
            return Err(format!("{why}; nothing was saved"));
        }
        let mut out = if apply && !applied.is_empty() {
            self.commit(Some(&id), Some(&before), &p, arg_str(args, "summary")?)?
        } else {
            json!({ "id": id, "saved": false, "note": "Not saved; call again with apply: true to save." })
        };
        if let Some(why) = refused {
            out["note"] = json!(format!("Not saved: apply would be refused, {why}."));
        }
        out["patches"] = patches_json(&applied);
        if let Some(t) = helped.triangle {
            out["triangle"] = json!(t);
        }
        if let Some(o) = helped.opposite_edge {
            out["opposite_edge"] = json!(o);
        }
        if let Some(st) = &helped.state {
            out["state"] = json!(st);
        }
        if let Some(arc) = &helped.arc {
            out["arc"] = json!(arc);
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
    fn moving_a_window_onto_a_face_too_small_for_it_is_refused_on_a_flat_roof() {
        // HUNTC-003: saved with a roof area of -160 m2 and no warning.
        let (mut s, id) = setup();
        call(
            &mut s,
            "set_roof_options",
            json!({ "id": id, "flat_roof": true, "summary": "flat" }),
        );
        call(
            &mut s,
            "set_openings",
            json!({ "id": id, "summary": "w", "windows": [{ "x": 40, "y": 40, "width": 1.45, "height": 90 }] }),
        );
        let e = s
            .call_tool(
                "transform_items",
                &json!({ "id": id, "summary": "m", "op": "move", "dx": -38, "dy": -38,
                         "items": [{ "kind": "window", "index": 0 }] }),
            )
            .unwrap_err();
        assert!(e.contains("larger than"), "{e}");
        assert_eq!(s.store.load(&id).unwrap().windows[0].x, 40.0);
        let e = s
            .call_tool(
                "set_openings",
                &json!({ "id": id, "summary": "w", "windows": [{ "x": 40, "y": 40, "width": 0, "height": 1 }] }),
            )
            .unwrap_err();
        // set_openings' own field check (from the editor-parity work)
        // answers first; the write check covers the other tools.
        assert!(e.contains("positive size") || e.contains("between"), "{e}");
    }

    #[test]
    fn creating_tools_refuse_what_the_editor_refuses() {
        // ARCH-007, HUNTIO-012.
        let (mut s, id) = setup();
        for (tool, args) in [
            (
                "create_standard_roof",
                json!({ "name": "A", "type": "gable", "length": 10, "width": 8, "pitch_deg": 1, "summary": "s" }),
            ),
            (
                "create_standard_roof",
                json!({ "name": "A", "type": "gable", "length": 10, "width": 8, "pitch_deg": 30, "eave_height": -3, "summary": "s" }),
            ),
            (
                "create_standard_roof",
                json!({ "name": " ", "type": "gable", "length": 10, "width": 8, "pitch_deg": 30, "summary": "s" }),
            ),
            (
                "update_job",
                json!({ "id": id, "name": "  ", "summary": "s" }),
            ),
            (
                "update_job",
                json!({ "id": id, "customer": ["x"], "summary": "s" }),
            ),
            (
                "update_edges",
                json!({ "id": id, "part": 0, "changes": [{ "index": -1, "pitch_deg": 30 }], "summary": "s" }),
            ),
            (
                "update_edges",
                json!({ "id": id, "part": 0, "changes": [{ "index": 0, "gable": "yes" }], "summary": "s" }),
            ),
            (
                "add_shape_part",
                json!({ "id": id, "shape": "hip", "length": 6, "width": 5, "overhang": -1, "summary": "s" }),
            ),
        ] {
            assert!(s.call_tool(tool, &args).is_err(), "{tool} {args}");
        }
        let r = call(
            &mut s,
            "update_job",
            json!({ "id": id, "name": "Test", "summary": "same" }),
        );
        assert_eq!(r["saved"], false, "{r}");
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
    fn a_pitch_over_90_is_refused_as_in_the_wall_bar() {
        // UX-010: 95 typed for 45 used to make a plane leaning out.
        let (mut s, id) = setup();
        let before = s.store.load(&id).unwrap();
        for pitch in [95.0, 90.0, 178.0] {
            let err = s
                .call_tool(
                    "update_edges",
                    &json!({ "id": id, "summary": "p", "changes": [{ "index": 0, "pitch_deg": pitch }] }),
                )
                .unwrap_err();
            assert!(err.contains("below 90"), "{pitch}: {err}");
        }
        assert_eq!(s.store.load(&id).unwrap().parts, before.parts);
        call(
            &mut s,
            "update_edges",
            json!({ "id": id, "summary": "p", "changes": [{ "index": 0, "pitch_deg": 89.5 }] }),
        );
        assert_eq!(s.store.load(&id).unwrap().parts[0].edges[0].pitch_deg, 89.5);
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
    fn update_opening_resizes_a_rotated_chimney_an_opening_and_a_window() {
        // A chimney placed along a 20° wall: its corners are rounded to mm,
        // which the old rectangle test refused, so it could not be resized.
        let (mut s, id) = setup();
        call(
            &mut s,
            "set_openings",
            json!({ "id": id, "summary": "c",
            "chimneys": [{ "x": 4, "y": 1, "width": 0.6, "depth": 0.6, "height": 6, "rotation_deg": 20 }],
            "roof_openings": [{ "corners": [[2, 1], [2, 2], [3, 2], [3, 1]] }],
            "windows": [{ "x": 6, "y": 1, "width": 0.78, "height": 1.18 }] }),
        );
        let r = call(
            &mut s,
            "update_opening",
            json!({ "id": id, "summary": "w", "kind": "chimney", "index": 0, "width": 1.2, "height": 7 }),
        );
        assert!(
            (r["box"]["width"].as_f64().unwrap() - 1.2).abs() < 2e-3,
            "{r}"
        );
        assert!(
            (r["box"]["depth"].as_f64().unwrap() - 0.6).abs() < 2e-3,
            "{r}"
        );
        assert!(
            (r["box"]["rotation_deg"].as_f64().unwrap() - 20.0).abs() < 0.1,
            "{r}"
        );
        let p = s.store.load(&id).unwrap();
        assert_eq!(p.chimneys[0].corners[0], Point2::new(4.0, 1.0));
        assert_eq!(p.chimneys[0].height, 7.0);
        // A clockwise opening keeps its corner order and grows across its
        // first side, away from the box corner (3, 1).
        call(
            &mut s,
            "update_opening",
            json!({ "id": id, "summary": "o", "kind": "roof_opening", "index": 0, "depth": 1.5 }),
        );
        let p = s.store.load(&id).unwrap();
        assert_eq!(
            p.roof_openings[0].corners,
            [(1.5, 1.0), (1.5, 2.0), (3.0, 2.0), (3.0, 1.0)].map(|(x, y)| Point2::new(x, y))
        );
        call(
            &mut s,
            "update_opening",
            json!({ "id": id, "summary": "f", "kind": "window", "index": 0, "width": 1.14 }),
        );
        assert_eq!(s.store.load(&id).unwrap().windows[0].width, 1.14);
        for (args, want) in [
            (
                json!({ "kind": "chimney", "index": 0, "width": 0 }),
                "at least 0.01",
            ),
            (
                json!({ "kind": "chimney", "index": 3, "width": 1 }),
                "does not exist",
            ),
            (
                json!({ "kind": "roof_opening", "index": 0, "height": 2 }),
                "no height",
            ),
            (
                json!({ "kind": "window", "index": 0, "depth": 2 }),
                "up the slope",
            ),
            (
                json!({ "kind": "window", "index": 0, "width": 40, "height": 20 }),
                "larger",
            ),
        ] {
            let mut a = args.clone();
            a["id"] = json!(id);
            a["summary"] = json!("x");
            let e = s.call_tool("update_opening", &a).unwrap_err();
            assert!(e.contains(want), "{args}: {e}");
        }
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
    fn delete_items_deletes_exactly_the_listed_items_in_any_order() {
        // ERR-001 / TEST-001: a repeated item between other kinds removed
        // the neighbouring building too.
        let (mut s, id) = setup();
        for (k, at) in [(1, 20), (2, 40), (3, 60)] {
            call(
                &mut s,
                "add_shape_part",
                json!({ "id": id, "shape": "hip", "length": 6, "width": 5, "at": [0, at], "summary": format!("part {k}") }),
            );
        }
        call(
            &mut s,
            "set_openings",
            json!({ "id": id, "summary": "windows",
                "windows": [
                    { "x": 2, "y": 1, "width": 0.8, "height": 1.0 },
                    { "x": 4, "y": 1, "width": 0.8, "height": 1.0 },
                    { "x": 6, "y": 1, "width": 0.8, "height": 1.0 }] }),
        );
        let names = |p: &tectora_core::model::Project| -> Vec<String> {
            p.parts.iter().map(|x| x.name.clone()).collect()
        };
        let before = s.store.load(&id).unwrap();
        assert_eq!((before.parts.len(), before.windows.len()), (4, 3));
        let item = |k: &str, i: usize| json!({ "kind": k, "index": i });
        call(
            &mut s,
            "delete_items",
            json!({ "id": id, "items": [item("part", 1), item("window", 1), item("part", 1)], "summary": "s" }),
        );
        let p = s.store.load(&id).unwrap();
        let mut want = names(&before);
        want.remove(1);
        assert_eq!(names(&p), want);
        assert_eq!(p.windows.len(), 2);
        // Ascending order deletes the same two.
        call(
            &mut s,
            "delete_items",
            json!({ "id": id, "items": [item("part", 0), item("part", 2)], "summary": "s" }),
        );
        assert_eq!(s.store.load(&id).unwrap().parts.len(), 1);
        // An item that does not exist saves nothing.
        let log = s.store.activity(&id).len();
        assert!(
            s.call_tool(
                "delete_items",
                &json!({ "id": id, "items": [item("dormer", 5)], "summary": "s" })
            )
            .is_err()
        );
        assert_eq!(s.store.activity(&id).len(), log);
    }

    #[test]
    fn moving_a_window_cannot_save_a_window_larger_than_its_face() {
        // HUNTC-003: -160 m2 roofs were saved by a move.
        let (mut s, id) = setup();
        // A window beside the roof is saved; it deducts nothing.
        call(
            &mut s,
            "set_openings",
            json!({ "id": id, "summary": "far window",
                "windows": [{ "x": 40, "y": 40, "width": 1.45, "height": 60 }] }),
        );
        let before = s.store.load(&id).unwrap();
        let err = s
            .call_tool(
                "transform_items",
                &json!({ "id": id, "op": "move", "dx": -38, "dy": -38, "summary": "move",
                    "items": [{ "kind": "window", "index": 0 }] }),
            )
            .unwrap_err();
        assert!(err.contains("larger than"), "{err}");
        assert_eq!(s.store.load(&id).unwrap(), before, "nothing saved");
    }

    #[test]
    fn a_mirror_with_more_than_one_copy_is_refused() {
        // HUNTC-001: copies 2 saved the building twice on the same spot.
        let (mut s, id) = setup();
        let before = s.store.load(&id).unwrap();
        let err = s
            .call_tool(
                "transform_part",
                &json!({ "id": id, "op": "mirror", "deg": 90, "centre": [12, 4], "copies": 2, "summary": "x" }),
            )
            .unwrap_err();
        assert!(err.contains("mirror adds at most 1"), "{err}");
        assert_eq!(s.store.load(&id).unwrap(), before);
    }

    #[test]
    fn a_bad_item_does_not_block_edits_of_other_items_and_messages_count_from_zero() {
        // HUNTC-005 / ERR-007.
        let (mut s, id) = setup();
        // A window of size 0 is refused where it is made, by its 0-based index.
        let err = s
            .call_tool(
                "set_openings",
                &json!({ "id": id, "summary": "w", "windows": [
                    { "x": 2, "y": 1, "width": 0.8, "height": 1.2 },
                    { "x": 3, "y": 1, "width": 0, "height": 1.2 }] }),
            )
            .unwrap_err();
        assert!(err.contains("window index 1"), "{err}");
        assert!(s.store.load(&id).unwrap().windows.is_empty());
        // A zero-length wall saves (only that building is left out) ...
        call(
            &mut s,
            "add_shape_part",
            json!({ "id": id, "shape": "hip", "length": 6, "width": 5, "at": [0, 20], "summary": "second" }),
        );
        call(
            &mut s,
            "edit_walls",
            json!({ "id": id, "part": 0, "summary": "collapse a wall",
                "operations": [{ "op": "move_point", "index": 1, "x": 0, "y": 0 }] }),
        );
        // ... and does not block the height of the other building.
        call(
            &mut s,
            "set_part_height",
            json!({ "id": id, "part": 1, "height": 4.5, "summary": "second building" }),
        );
        // The offer refusal names the faulty building as get_project and calc do.
        let err = s
            .call_tool("store_result", &json!({ "id": id, "summary": "x" }))
            .unwrap_err();
        assert!(err.contains("Part index 0, edge index"), "{err}");
    }

    #[test]
    fn export_plan_says_why_png_is_not_available() {
        // ARCH-009: the PNG is drawn by the app's web view only.
        let (mut s, id) = setup();
        let err = s
            .call_tool(
                "export_plan",
                &json!({ "id": id, "format": "png", "summary": "x" }),
            )
            .unwrap_err();
        assert!(err.contains("svg") && err.contains("app"), "{err}");
    }

    #[test]
    fn set_manual_refuses_a_negative_length_and_a_near_vertical_face() {
        // MATH-005 / MATH-006.
        let (mut s, id) = setup();
        let before = s.store.load(&id).unwrap();
        let line = |l: f64| json!({ "lines": [{ "a": [0, 0, 0], "b": [1, 0, 0], "code": "T", "length": l }] });
        let face = |d: f64| json!({ "faces": [{ "points": [[20, 0], [24, 0], [24, 3], [20, 3]], "pitch_deg": d }] });
        for (extra, want) in [
            (line(-50.0), "length"),
            (face(89.9999), "pitch_deg"),
            (face(90.0), "pitch_deg"),
            (face(-1.0), "pitch_deg"),
        ] {
            let mut args = json!({ "id": id, "summary": "x" });
            args.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            let err = s.call_tool("set_manual", &args).unwrap_err();
            assert!(err.contains(want), "{err}");
        }
        assert_eq!(s.store.load(&id).unwrap(), before);
        call(
            &mut s,
            "set_manual",
            json!({ "id": id, "summary": "x", "lines": [{ "a": [0, 0, 0], "b": [1, 0, 0], "code": "T", "length": 0 }],
                "faces": [{ "points": [[20, 0], [24, 0], [24, 3], [20, 3]], "pitch_deg": 89.9 }] }),
        );
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
    fn a_part_index_that_is_not_a_whole_number_is_refused_not_applied_to_part_0() {
        // ERR-007: `"part": "1"` edited the first building.
        let (mut s, id) = setup();
        let before = s.store.load(&id).unwrap();
        for bad in [json!("1"), json!(-1), json!(1.5)] {
            let err = s
                .call_tool(
                    "update_edges",
                    &json!({ "id": id, "part": bad, "changes": [{ "all": true, "pitch_deg": 25 }], "summary": "x" }),
                )
                .unwrap_err();
            assert!(err.contains("part"), "{err}");
        }
        assert_eq!(s.store.load(&id).unwrap(), before);
    }

    #[test]
    fn a_roof_that_cannot_be_calculated_is_not_stored_as_an_offer() {
        // DR-001: update_job status=offer and store_result saved 0 m².
        let (mut s, id) = setup();
        call(
            &mut s,
            "set_openings",
            json!({ "id": id, "summary": "chimney",
                "chimneys": [{ "corners": [[3, 3], [4, 3], [3.5, 3.4], [4, 4], [3, 4]] }] }),
        );
        let before = s.store.load(&id).unwrap();
        for (tool, extra) in [
            ("update_job", json!({ "status": "offer" })),
            ("store_result", json!({})),
        ] {
            let mut args = json!({ "id": id, "summary": "x" });
            args.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            let err = s.call_tool(tool, &args).unwrap_err();
            assert!(err.contains("cannot be calculated"), "{tool}: {err}");
        }
        assert_eq!(s.store.load(&id).unwrap(), before);
    }

    #[test]
    fn a_misspelled_argument_is_refused_instead_of_saving_nothing() {
        // ERR-008: `pitch` for `pitch_deg` answered "Saved" with no change.
        let (mut s, id) = setup();
        let log = s.store.activity(&id).len();
        let err = s
            .call_tool(
                "update_edges",
                &json!({ "id": id, "changes": [{ "all": true, "pitch": 25 }], "summary": "x" }),
            )
            .unwrap_err();
        assert!(err.contains("`changes[0].pitch`"), "{err}");
        let err = s
            .call_tool(
                "update_job",
                &json!({ "id": id, "state": "offer", "summary": "x" }),
            )
            .unwrap_err();
        assert!(err.contains("`state`"), "{err}");
        assert_eq!(s.store.activity(&id).len(), log, "nothing saved");
    }

    #[test]
    fn a_window_is_placed_by_its_distances_from_the_eave_and_the_side() {
        let (mut s, id) = setup();
        call(
            &mut s,
            "set_openings",
            json!({ "id": id, "windows": [{ "x": 3, "y": 1, "width": 1.14, "height": 1.4 }], "summary": "Window" }),
        );
        let r = call(
            &mut s,
            "place_item",
            json!({ "id": id, "kind": "window", "index": 0, "from_eave": 1.5, "from_left": 2.0, "summary": "Window 1.5 m from the eave" }),
        );
        let pl = &r["placement"];
        assert!(
            (pl["from_eave"].as_f64().unwrap() - 1.5).abs() < 1e-5,
            "{r}"
        );
        assert!(
            (pl["from_left"].as_f64().unwrap() - 2.0).abs() < 1e-5,
            "{r}"
        );
        let listed = call(&mut s, "get_project", json!({ "id": id }));
        let w = &listed["placements"]["windows"][0];
        assert!(
            (w["from_eave"].as_f64().unwrap() - 1.5).abs() < 1e-5,
            "{listed}"
        );
        for bad in [
            json!({ "id": id, "kind": "window", "index": 0, "from_eave": 1.5, "from_left": 2.0, "summary": "same" }),
            json!({ "id": id, "kind": "window", "index": 3, "from_eave": 1.0, "summary": "none" }),
            json!({ "id": id, "kind": "manual_line", "index": 0, "from_eave": 1.0, "summary": "line" }),
            json!({ "id": id, "kind": "window", "index": 0, "from_left": 1.0, "from_right": 1.0, "summary": "both" }),
        ] {
            assert!(s.call_tool("place_item", &bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_building_height_carries_the_roof_and_keeps_a_lower_eave() {
        // Jakub (2026-11-14): the building height is not the eave height.
        let (mut s, id) = setup();
        let p0 = s.store.load(&id).unwrap();
        let eave = p0.parts[0].edges[0].eave_height;
        call(
            &mut s,
            "update_edges",
            json!({ "id": id, "part": 0, "changes": [{ "index": 0, "eave_height": eave - 0.5 }], "summary": "Lower eave" }),
        );
        call(
            &mut s,
            "set_part_height",
            json!({ "id": id, "part": 0, "height": eave + 1.0, "summary": "Higher building" }),
        );
        let p = s.store.load(&id).unwrap();
        assert_eq!(p.parts[0].height, Some(eave + 1.0));
        let e = &p.parts[0].edges;
        let lowest = e
            .iter()
            .map(|e| e.eave_height)
            .fold(f64::INFINITY, f64::min);
        assert!((lowest - (eave + 0.5)).abs() < 1e-9, "{lowest}");
        assert!(
            e[1..]
                .iter()
                .all(|w| (w.eave_height - (eave + 1.0)).abs() < 1e-9),
            "{e:?}"
        );
        for bad in [
            json!({ "id": id, "part": 0, "height": eave + 1.0, "summary": "same" }),
            json!({ "id": id, "part": 0, "height": 3, "eave_height": 3, "summary": "both" }),
            json!({ "id": id, "part": 0, "height": -2, "summary": "below ground" }),
        ] {
            assert!(s.call_tool("set_part_height", &bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn moves_new_outlines_and_helpers_keep_the_building_height() {
        // Review ARCH-001 / TEST-002: these dropped or changed the height
        // the user typed, so the next height entry moved every eave wrongly.
        let (mut s, id) = setup();
        let eave = s.store.load(&id).unwrap().parts[0].edges[0].eave_height;
        call(
            &mut s,
            "set_part_height",
            json!({ "id": id, "part": 0, "height": eave + 0.4, "summary": "Walls higher than the eave" }),
        );
        let h = Some(eave + 0.4);
        call(
            &mut s,
            "transform_part",
            json!({ "id": id, "part": 0, "op": "move", "dx": 1, "dy": 0, "copies": 1, "summary": "Move and copy" }),
        );
        let p = s.store.load(&id).unwrap();
        assert_eq!(p.parts[0].height, h);
        assert_eq!(p.parts[1].height, h, "the copy keeps it too");
        call(
            &mut s,
            "transform_part",
            json!({ "id": id, "part": 1, "op": "rotate", "deg": 90, "summary": "Turn" }),
        );
        assert_eq!(s.store.load(&id).unwrap().parts[1].height, h);
        call(
            &mut s,
            "set_outline",
            json!({ "id": id, "part": 0, "points": [[0, 0], [9, 0], [9, 7], [0, 7]], "summary": "New outline" }),
        );
        assert_eq!(s.store.load(&id).unwrap().parts[0].height, h);
        // A helper that sets eaves on a building without a stored height
        // stores the height it showed first.
        let mut p = s.store.load(&id).unwrap();
        p.parts[0].height = None;
        let shown = p.parts[0].building_height();
        s.store.save(Some(id.clone()), &p).unwrap();
        call(
            &mut s,
            "roof_helper",
            json!({ "id": id, "part": 0, "edge": 0, "tool": "build_up", "thickness": 0.2, "eave": "vertical", "apply": true, "summary": "Build-up" }),
        );
        let p = s.store.load(&id).unwrap();
        assert_eq!(p.parts[0].height, shown);
    }

    #[test]
    fn one_tool_call_calculates_the_roof_once() {
        // Review ARCH-006: a write calculated it 6 times, get_project up to 9.
        let (mut s, id) = setup();
        let runs = |s: &mut Server, name: &str, args: Value| {
            let before = tectora_core::openings::core_runs();
            call(s, name, args);
            tectora_core::openings::core_runs() - before
        };
        assert_eq!(runs(&mut s, "get_project", json!({ "id": id })), 1);
        assert_eq!(runs(&mut s, "calc", json!({ "id": id })), 1);
        // The write needs one run for the design, which `commit` reuses for
        // the quantities and the roof trouble.
        let n = runs(
            &mut s,
            "add_part",
            json!({ "id": id, "summary": "Wing",
                "part": { "name": "Wing", "points": [[12, 0], [18, 0], [18, 5], [12, 5]], "defaults": { "pitch_deg": 35 } } }),
        );
        assert_eq!(n, 1, "a write");
    }

    #[test]
    fn a_write_that_leaves_a_building_without_roof_says_so_and_calc_names_it() {
        // Review ERR-003 / ARCH-002.
        let (mut s, id) = setup();
        let r = call(
            &mut s,
            "add_part",
            json!({ "id": id, "summary": "Wing with a 4 mm jog",
                "part": { "name": "Wing", "points": [[6, 4], [14, 4], [14, 4.004], [14, 10], [6, 10]], "defaults": { "pitch_deg": 35 } } }),
        );
        let t = &r["roof_trouble"];
        assert_eq!(t["problems"][0]["code"], "zero_length_wall", "{r}");
        let ok = call(&mut s, "calc", json!({ "id": id }));
        assert!(
            ok["parts"][0]["error"].is_null(),
            "the first building keeps its roof: {ok}"
        );
        // calc says the wing was left out, it is not just 0 m².
        assert_eq!(ok["problems"][0]["code"], "zero_length_wall", "{ok}");
        // Exports wait, as the editor's Export does (review: MCP parity).
        for (tool, args) in [
            ("export_list", json!({ "id": id })),
            ("export_plan", json!({ "id": id, "summary": "Export" })),
        ] {
            let err = s.call_tool(tool, &args).expect_err(tool);
            assert!(err.contains("not exported"), "{tool}: {err}");
        }
    }

    #[test]
    fn a_building_height_sets_every_wall_in_one_save_and_changes_the_join() {
        let (mut s, id) = setup();
        call(
            &mut s,
            "add_part",
            json!({ "id": id, "summary": "Wing",
                "part": { "name": "Wing", "points": [[6, 4], [14, 4], [14, 10], [6, 10]], "defaults": { "pitch_deg": 35 } } }),
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
    fn a_dormer_is_placed_by_its_wall_height_or_depth_as_in_the_dialog() {
        let (mut s, id) = setup();
        let r = call(
            &mut s,
            "add_dormer",
            json!({ "id": id, "kind": "shed", "start": [4, 1], "front_pitch_deg": 10, "wall_height": 1.2, "summary": "s" }),
        );
        let sec = &r["dormer"]["section"];
        assert_eq!(sec["wall_height"], 1.2, "{sec}");
        let depth = sec["depth"].as_f64().unwrap();
        assert!(depth > 0.0, "{sec}");
        let r = call(
            &mut s,
            "update_dormer",
            json!({ "id": id, "index": 0, "depth": depth * 2.0, "summary": "s" }),
        );
        let got = r["dormer"]["section"]["depth"].as_f64().unwrap();
        assert!((got - depth * 2.0).abs() < 2e-3, "{got} vs {}", depth * 2.0);
        let p = call(&mut s, "get_project", json!({ "id": id }));
        assert_eq!(
            p["dormers"][0]["section"]["depth"],
            r["dormer"]["section"]["depth"]
        );
        for bad in [
            json!({ "id": id, "index": 0, "depth": 2, "wall_height": 1, "summary": "s" }),
            json!({ "id": id, "index": 0, "eave_height": 5, "wall_height": 1, "summary": "s" }),
            json!({ "id": id, "index": 0, "ridge_length": 2, "summary": "s" }),
            json!({ "id": id, "index": 0, "wall_height": -1, "summary": "s" }),
            // MATH-001: an infinite eave was saved as null and broke the job.
            json!({ "id": id, "index": 0, "kind": "half_hip", "half_hip_pitch_deg": 0, "ridge_length": 1, "summary": "s" }),
        ] {
            assert!(s.call_tool("update_dormer", &bad).is_err(), "{bad}");
        }
        call(&mut s, "get_project", json!({ "id": id }));
    }

    #[test]
    fn a_dormer_takes_front_and_side_overhangs_and_a_back_width() {
        let (mut s, id) = setup();
        let r = call(
            &mut s,
            "add_dormer",
            json!({ "id": id, "kind": "trapezoid", "start": [4, 1], "overhang": 0.3, "summary": "s" }),
        );
        assert_eq!(r["dormer"]["overhang"], 0.3);
        assert_eq!(r["dormer"]["side_overhang"], 0.3);
        let r = call(
            &mut s,
            "update_dormer",
            json!({ "id": id, "index": 0, "front_overhang": 0.5, "side_overhang": 0.15, "wall_height": 1.2, "back_width": 1.0, "summary": "s" }),
        );
        let d = &r["dormer"];
        assert_eq!(d["overhang"], 0.5, "{d}");
        assert_eq!(d["side_overhang"], 0.15, "{d}");
        let sec = &d["section"];
        assert!(
            (sec["back_width"].as_f64().unwrap() - 1.0).abs() < 2e-3,
            "{sec}"
        );
        assert!(
            (sec["wall_height"].as_f64().unwrap() - 1.2).abs() < 2e-3,
            "{sec}"
        );
        let p = call(&mut s, "get_project", json!({ "id": id }));
        assert_eq!(p["dormers"][0]["side_overhang"], 0.15);
        for bad in [
            json!({ "id": id, "index": 0, "side_overhang": -0.1, "summary": "s" }),
            json!({ "id": id, "index": 0, "back_width": 1.0, "pitch_deg": 40, "summary": "s" }),
            json!({ "id": id, "index": 0, "back_width": 9.0, "summary": "s" }),
            json!({ "id": id, "index": 0, "kind": "gable", "back_width": 1.0, "summary": "s" }),
        ] {
            assert!(s.call_tool("update_dormer", &bad).is_err(), "{bad}");
        }
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
        // The legend of line names, as the app's Export dialog gives it (UX-013).
        let named = call(
            &mut s,
            "export_plan",
            json!({ "id": id, "format": "svg", "name": "named", "summary": "s",
                    "line_names": { "T": "Okapy", "F": "Hřebeny" } }),
        )["path"]
            .as_str()
            .unwrap()
            .to_string();
        let text = std::fs::read_to_string(&named).unwrap();
        assert!(text.contains("T Okapy"), "{text}");
        for bad in [
            json!({ "id": id, "format": "exe", "summary": "s" }),
            json!({ "id": id, "name": "../x", "summary": "s" }),
            json!({ "id": id, "summary": "s", "line_names": { "T": "x".repeat(61) } }),
            json!({ "id": id, "summary": "s", "line_names": ["T"] }),
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

/// MCP tools no other test called, or called without looking at the result
/// (TEST-004): the wrapper's arguments (`ts_ms`, `example_id`, `walls`,
/// `neighbour`, `thickness`) must reach the core operation the editor uses.
#[cfg(test)]
mod tool_coverage {
    use super::*;
    use tectora_core::store::{Store, now_ms};

    fn server() -> Server {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "tectora-mcp-coverage-{}-{}-{n}",
            now_ms(),
            std::process::id()
        ));
        Server::new(Store::new(dir).unwrap())
    }

    fn call(s: &mut Server, name: &str, args: Value) -> Value {
        s.call_tool(name, &args)
            .unwrap_or_else(|e| panic!("{name}: {e}"))
    }

    fn example(s: &mut Server, example_id: &str) -> String {
        let r = call(
            s,
            "create_from_example",
            json!({ "example_id": example_id, "summary": "start" }),
        );
        r["id"].as_str().unwrap().to_string()
    }

    /// The patches a `roof_helper` call returned, as the core's map.
    fn patches(r: &Value) -> Patches {
        r["patches"]
            .as_array()
            .unwrap_or_else(|| panic!("no patches in {r}"))
            .iter()
            .map(|p| {
                (
                    p["edge"].as_u64().unwrap() as usize,
                    serde_json::from_value(p["set"].clone()).unwrap(),
                )
            })
            .collect()
    }

    /// `patches` with only the fields that change the edge (what
    /// `apply_patches` reports).
    fn applied(edges: &[tectora_core::EdgeDef], patches: &Patches) -> Patches {
        let mut es = edges.to_vec();
        apply_patches(&mut es, patches)
    }

    /// MCP parity for "put the offer's figures back" (audit 2026-11-22
    /// Design A item 3): get_history marks the recalculation the stored
    /// result is, restore_offer_result puts the offer's figures back with
    /// the AI's summary and nothing else, and revert_last_change can name
    /// the change it expects (item 4, ERR-801).
    #[test]
    fn restore_offer_result_puts_only_the_offer_figures_back() {
        let mut s = server();
        let id = example(&mut s, "gable");
        call(
            &mut s,
            "update_job",
            json!({ "id": id, "status": "offer", "summary": "Offer sent" }),
        );
        // No recalculation yet: nothing to put back.
        let e = s
            .call_tool("restore_offer_result", &json!({ "id": id, "summary": "x" }))
            .unwrap_err();
        assert!(e.contains("not a recalculation"), "{e}");
        // As if engine 0.0.1 had stored figures 2 m2 off; the user accepted
        // the AI's changes and opens the offer in the app.
        let file = s.store.dir.join(format!("{id}.tectora"));
        let mut p = s.store.load(&id).unwrap();
        let offer = {
            let r = p
                .result_snapshot
                .as_mut()
                .expect("the offer stored a result");
            r.engine_version = "0.0.1".into();
            r.roof_area += 2.0;
            r.clone()
        };
        std::fs::write(&file, p.to_json().unwrap()).unwrap();
        let origins = s.store.activity_dir().join(format!("{id}.result.json"));
        let text = std::fs::read_to_string(&origins)
            .unwrap()
            .replace(tectora_core::snapshot::ENGINE_VERSION, "0.0.1");
        std::fs::write(&origins, text).unwrap();
        s.store.mark_reviewed(&id, now_ms() + 1000).unwrap();
        let done = s.store.refresh_entry(&id).unwrap().expect("recalculated");

        let history = call(&mut s, "get_history", json!({ "id": id }));
        let marked: Vec<&Value> = history
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["stored_result_recalculation"] == true)
            .collect();
        assert_eq!(marked.len(), 1, "{history}");
        assert_eq!(marked[0]["ts_ms"], done.entry_ts_ms);
        assert_eq!(marked[0]["actor"], "tectora");

        // An AI save after it stays; a stale revert_last_change is refused.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        call(
            &mut s,
            "update_job",
            json!({ "id": id, "notes": "Call the customer", "summary": "Note" }),
        );
        let e = s
            .call_tool(
                "revert_last_change",
                &json!({ "id": id, "ts_ms": done.entry_ts_ms, "summary": "x" }),
            )
            .unwrap_err();
        assert!(e.starts_with("revert_stale"), "{e}");
        let e = s
            .call_tool(
                "revert_last_change",
                &json!({ "id": id, "ts_ms": "now", "summary": "x" }),
            )
            .unwrap_err();
        assert!(e.contains("ts_ms"), "{e}");
        assert_eq!(s.store.load(&id).unwrap().notes, "Call the customer");

        // The summary is required, as for every write.
        assert!(
            s.call_tool("restore_offer_result", &json!({ "id": id }))
                .is_err()
        );
        let r = call(
            &mut s,
            "restore_offer_result",
            json!({ "id": id, "summary": "The customer signed these figures" }),
        );
        assert_eq!(r["restored"], true);
        assert_eq!(r["stored_result"]["roof_area"], offer.roof_area);
        let now = s.store.load(&id).unwrap();
        assert_eq!(now.result_snapshot, Some(offer));
        assert_eq!(now.notes, "Call the customer");
        let entry = s.store.activity(&id).last().unwrap().clone();
        assert_eq!(entry.restores, Some(done.entry_ts_ms));
        assert_ne!(entry.actor, tectora_core::store::USER_ACTOR);
        assert!(
            entry
                .summary
                .ends_with("(reason: The customer signed these figures)"),
            "{}",
            entry.summary
        );
        let history = call(&mut s, "get_history", json!({ "id": id }));
        assert_eq!(history[0]["restores"], done.entry_ts_ms, "{history}");
        assert!(
            history
                .as_array()
                .unwrap()
                .iter()
                .all(|e| e.get("stored_result_recalculation").is_none()),
            "{history}"
        );
        // Again, by the entry: no longer that recalculation.
        let e = s
            .call_tool(
                "restore_offer_result",
                &json!({ "id": id, "ts_ms": done.entry_ts_ms, "summary": "x" }),
            )
            .unwrap_err();
        assert!(e.starts_with("revert_stale"), "{e}");
        let _ = std::fs::remove_dir_all(&s.store.dir);
    }

    /// The job's stored result set back as if engine 0.0.1 had stored it
    /// 2 m2 off, the AI's changes accepted: the app's next open
    /// recalculates it. Returns the offer's result.
    fn make_old(s: &Server, id: &str) -> tectora_core::snapshot::ResultSnapshot {
        let file = s.store.dir.join(format!("{id}.tectora"));
        let mut p = s.store.load(id).unwrap();
        let offer = {
            let r = p.result_snapshot.as_mut().expect("a stored result");
            r.engine_version = "0.0.1".into();
            r.roof_area += 2.0;
            r.clone()
        };
        std::fs::write(&file, p.to_json().unwrap()).unwrap();
        let origins = s.store.activity_dir().join(format!("{id}.result.json"));
        let text = std::fs::read_to_string(&origins)
            .unwrap()
            .replace(tectora_core::snapshot::ENGINE_VERSION, "0.0.1");
        std::fs::write(&origins, text).unwrap();
        s.store.mark_reviewed(id, now_ms() + 1000).unwrap();
        offer
    }

    /// An offer whose stored result the app's open recalculated.
    fn recalculated_offer(
        s: &mut Server,
    ) -> (
        String,
        tectora_core::snapshot::ResultSnapshot,
        tectora_core::store::RefreshedEntry,
    ) {
        let id = example(s, "gable");
        call(
            s,
            "update_job",
            json!({ "id": id, "status": "offer", "summary": "Offer sent" }),
        );
        let offer = make_old(s, &id);
        let done = s.store.refresh_entry(&id).unwrap().expect("recalculated");
        (id, offer, done)
    }

    /// Audit 2026-11-23 HUNT-902: get_project names the recalculation the
    /// stored result is, so an AI can put the offer's figures back after
    /// the engine's entry left the change log.
    #[test]
    fn the_offer_figures_go_back_after_the_change_log_dropped_the_recalculation() {
        let mut s = server();
        let (id, offer, done) = recalculated_offer(&mut s);
        let p = call(&mut s, "get_project", json!({ "id": id }));
        assert_eq!(
            p["result_recalculation"]["entry_ts_ms"], done.entry_ts_ms,
            "{p}"
        );
        for i in 0..55 {
            call(
                &mut s,
                "update_job",
                json!({ "id": id, "notes": format!("note {i}"), "summary": "Note" }),
            );
        }
        let history = call(&mut s, "get_history", json!({ "id": id }));
        assert!(
            history
                .as_array()
                .unwrap()
                .iter()
                .all(|e| e["ts_ms"] != done.entry_ts_ms),
            "the engine's entry is trimmed"
        );
        let p = call(&mut s, "get_project", json!({ "id": id }));
        let ts = p["result_recalculation"]["entry_ts_ms"].clone();
        assert_eq!(ts, done.entry_ts_ms, "{p}");
        let r = call(
            &mut s,
            "restore_offer_result",
            json!({ "id": id, "ts_ms": ts, "summary": "The customer signed these figures" }),
        );
        assert_eq!(r["restored"], true);
        assert_eq!(s.store.load(&id).unwrap().result_snapshot, Some(offer));
        let p = call(&mut s, "get_project", json!({ "id": id }));
        assert_eq!(p["result_recalculation"], Value::Null, "{p}");
        let _ = std::fs::remove_dir_all(&s.store.dir);
    }

    /// Audit 2026-11-23 TEST-903: restore_offer_result refuses a ts_ms
    /// that is no entry time and writes nothing, tells the app live, and
    /// get_history marks only the recalculation the stored result is now.
    #[test]
    fn restore_offer_result_checks_its_input_tells_the_app_and_marks_one_entry() {
        let mut s = server();
        let (id, _, first) = recalculated_offer(&mut s);
        // The user stores an own result, an older engine's again, and the
        // app recalculates it once more: two engine entries.
        std::thread::sleep(std::time::Duration::from_millis(5));
        call(
            &mut s,
            "store_result",
            json!({ "id": id, "summary": "Store the figures" }),
        );
        make_old(&s, &id);
        let second = s.store.refresh_entry(&id).unwrap().expect("recalculated");
        assert_ne!(first.entry_ts_ms, second.entry_ts_ms);
        let history = call(&mut s, "get_history", json!({ "id": id }));
        let marked: Vec<&Value> = history
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["stored_result_recalculation"] == true)
            .collect();
        assert_eq!(marked.len(), 1, "{history}");
        assert_eq!(marked[0]["ts_ms"], second.entry_ts_ms);
        assert!(
            history
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["ts_ms"] == first.entry_ts_ms),
            "{history}"
        );

        let file = s.store.dir.join(format!("{id}.tectora"));
        let bytes = std::fs::read(&file).unwrap();
        let entries = s.store.activity(&id).len();
        for ts in [
            json!(-1),
            json!("x"),
            json!(1.5),
            json!([second.entry_ts_ms]),
            json!({}),
        ] {
            let e = s
                .call_tool(
                    "restore_offer_result",
                    &json!({ "id": id, "ts_ms": ts, "summary": "x" }),
                )
                .unwrap_err();
            assert!(e.contains("ts_ms"), "{ts}: {e}");
        }
        assert_eq!(std::fs::read(&file).unwrap(), bytes);
        assert_eq!(s.store.activity(&id).len(), entries);

        s.call_tool("list_projects", &json!({})).unwrap();
        call(
            &mut s,
            "restore_offer_result",
            json!({ "id": id, "ts_ms": second.entry_ts_ms, "summary": "Signed figures" }),
        );
        let live = s.store.live().unwrap();
        assert_eq!(live.kind.as_deref(), Some("changed"));
        assert_eq!(live.project_id.as_deref(), Some(id.as_str()));
        assert!(live.follow);
        let _ = std::fs::remove_dir_all(&s.store.dir);
    }

    /// Audit 2026-11-23 ARCH-902: a stored result with an impossible date
    /// is left out when the job is read; over MCP the read and the write
    /// that then saves the job without it say so.
    #[test]
    fn mcp_says_when_a_stored_result_was_left_out() {
        let mut s = server();
        let id = example(&mut s, "gable");
        call(
            &mut s,
            "update_job",
            json!({ "id": id, "status": "offer", "summary": "Offer sent" }),
        );
        let file = s.store.dir.join(format!("{id}.tectora"));
        let mut v: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        v["result_snapshot"]["taken_ms"] = json!(8_640_000_000_000_001u64);
        std::fs::write(&file, serde_json::to_string(&v).unwrap()).unwrap();
        let p = call(&mut s, "get_project", json!({ "id": id }));
        assert_eq!(p["load_warnings"][0]["code"], "result_dropped", "{p}");
        assert!(
            p["load_warnings"][0]["detail"]
                .as_str()
                .unwrap()
                .contains("8640000000000001"),
            "{p}"
        );
        let r = call(
            &mut s,
            "update_job",
            json!({ "id": id, "notes": "AI note", "summary": "Note" }),
        );
        assert_eq!(r["load_warnings"][0]["code"], "result_dropped", "{r}");
        assert!(r["note"].as_str().unwrap().contains("left out"), "{r}");
        // Saved without it: nothing more to say.
        let p = call(&mut s, "get_project", json!({ "id": id }));
        assert!(p.get("load_warnings").is_none(), "{p}");
        let r = call(
            &mut s,
            "update_job",
            json!({ "id": id, "notes": "AI note 2", "summary": "Note" }),
        );
        assert!(r.get("load_warnings").is_none(), "{r}");
        let _ = std::fs::remove_dir_all(&s.store.dir);
    }

    /// Audit 2026-11-23 ERR-1002: while another program holds the job's
    /// lock, the revert tools and the put-back answer `busy` after a short
    /// wait, with nothing done, so the AI knows to try again.
    #[test]
    fn mcp_reverts_answer_busy_while_the_job_is_being_saved() {
        let mut s = server();
        let id = example(&mut s, "gable");
        call(
            &mut s,
            "update_job",
            json!({ "id": id, "notes": "AI note", "summary": "Note" }),
        );
        let dir = s.store.dir.clone();
        s.store = Store::at(dir.clone()).with_action_wait(std::time::Duration::from_millis(100));
        let ts = s.store.activity(&id).last().unwrap().ts_ms;
        let log = s.store.activity(&id).len();
        let held = std::fs::OpenOptions::new()
            .write(true)
            .open(dir.join(".activity").join(format!("{id}.lock")))
            .unwrap();
        held.lock().unwrap();
        for (tool, args) in [
            ("revert_last_change", json!({ "id": id, "summary": "Undo" })),
            (
                "revert_to",
                json!({ "id": id, "ts_ms": ts, "summary": "Undo" }),
            ),
            (
                "restore_offer_result",
                json!({ "id": id, "ts_ms": ts, "summary": "Back" }),
            ),
        ] {
            let e = s.call_tool(tool, &args).unwrap_err();
            assert!(e.starts_with("busy: "), "{tool}: {e}");
            assert!(e.contains("try again"), "{tool}: {e}");
        }
        assert_eq!(s.store.activity(&id).len(), log);
        drop(held);
        call(
            &mut s,
            "revert_last_change",
            json!({ "id": id, "summary": "Undo" }),
        );
        assert_eq!(s.store.load(&id).unwrap().notes, "");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn revert_to_undoes_every_change_since_the_given_step() {
        let mut s = server();
        let id = example(&mut s, "gable");
        let original = s.store.load(&id).unwrap();
        for (k, pitch) in [35.0, 36.0, 37.0].into_iter().enumerate() {
            // One millisecond apart: the log tells changes apart by ts_ms.
            std::thread::sleep(std::time::Duration::from_millis(3));
            call(
                &mut s,
                "update_edges",
                json!({ "id": id, "changes": [{ "index": k, "pitch_deg": pitch }], "summary": format!("step {k}") }),
            );
        }
        let history = call(&mut s, "get_history", json!({ "id": id }));
        let steps = history.as_array().unwrap();
        // Newest first: step 2, step 1, step 0, start.
        assert_eq!(steps[2]["summary"], "step 0", "{history}");
        let first = steps[2]["ts_ms"].as_u64().unwrap();
        let r = call(
            &mut s,
            "revert_to",
            json!({ "id": id, "ts_ms": first, "summary": "Back to the example" }),
        );
        assert_eq!(r["reverted"], true);
        let p = s.store.load(&id).unwrap();
        assert_eq!(
            serde_json::to_value(&p.parts).unwrap(),
            serde_json::to_value(&original.parts).unwrap()
        );
        let log = s.store.activity(&id);
        assert_eq!(log.len(), 5, "start, 3 steps, one revert");
        let last = &log[4].summary;
        assert!(last.contains("Reverted 3 changes since: step 0"), "{last}");
        assert!(last.contains("Back to the example"), "{last}");
        // An unknown step changes nothing.
        let e = s
            .call_tool(
                "revert_to",
                &json!({ "id": id, "ts_ms": 1u64, "summary": "x" }),
            )
            .unwrap_err();
        assert!(e.contains("no recorded change at 1"), "{e}");
        assert_eq!(s.store.activity(&id).len(), 5);
        let _ = std::fs::remove_dir_all(&s.store.dir);
    }

    #[test]
    fn create_from_example_makes_the_example_job() {
        let mut s = server();
        let list = call(&mut s, "list_examples", json!({}));
        let ids: Vec<&str> = list
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["id"].as_str().unwrap())
            .collect();
        let want: Vec<&str> = EXAMPLES.iter().map(|e| e.id).collect();
        assert_eq!(ids, want);
        for ex in EXAMPLES {
            let id = example(&mut s, ex.id);
            let p = s.store.load(&id).unwrap();
            let core = tectora_core::examples::find(ex.id).unwrap().project();
            assert_eq!(p.name, core.name, "{}", ex.id);
            assert_eq!(
                serde_json::to_value(&p.parts).unwrap(),
                serde_json::to_value(&core.parts).unwrap(),
                "{}",
                ex.id
            );
        }
        let r = call(
            &mut s,
            "create_from_example",
            json!({ "example_id": "hip", "name": "Novák", "summary": "s" }),
        );
        let p = s.store.load(r["id"].as_str().unwrap()).unwrap();
        assert_eq!(p.name, "Novák");
        let e = s
            .call_tool(
                "create_from_example",
                &json!({ "example_id": "nope", "summary": "s" }),
            )
            .unwrap_err();
        assert!(e.contains("list_examples"), "{e}");
        assert_eq!(
            s.store.list().len(),
            EXAMPLES.len() + 1,
            "the unknown example made no job"
        );
        let _ = std::fs::remove_dir_all(&s.store.dir);
    }

    #[test]
    fn roof_helper_merge_sets_the_core_values() {
        let mut s = server();
        // L-shape: walls 2 and 4 both face north; wall 4 (the outer one)
        // is the reference, wall 2 lies 7 m further in and takes its plane.
        let id = example(&mut s, "l-shape");
        let mut p = s.store.load(&id).unwrap();
        p.parts[0].edges[2].eave_height -= 0.8;
        p.parts[0].edges[2].pitch_deg = 30.0;
        s.store.save(Some(id.clone()), &p).unwrap();
        let es = p.parts[0].edges.clone();
        assert_eq!(hc::same_way(&es, 4), vec![2, 4]);
        for target in ["eave", "overhang"] {
            let t: Target = serde_json::from_value(json!(target)).unwrap();
            let want = applied(&es, &hc::merge_onto(&es, 4, &[2], t));
            assert!(!want.is_empty(), "{target}");
            // Default walls (all parallel ones facing the same way) and the
            // explicit list give the same.
            for walls in [Value::Null, json!([2])] {
                let mut args = json!({ "id": id, "edge": 4, "tool": "merge", "target": target });
                if !walls.is_null() {
                    args["walls"] = walls;
                }
                let r = call(&mut s, "roof_helper", args);
                assert_eq!(r["saved"], false);
                assert_eq!(patches(&r), want, "{target}");
            }
        }
        let want = applied(&es, &hc::merge_by_moving(&es, 4, &[2]).unwrap());
        let r = call(
            &mut s,
            "roof_helper",
            json!({ "id": id, "edge": 4, "tool": "merge", "target": "position", "walls": [2] }),
        );
        assert_eq!(patches(&r), want);
        let r = call(
            &mut s,
            "roof_helper",
            json!({ "id": id, "edge": 4, "tool": "merge", "target": "eave", "apply": true, "summary": "One plane" }),
        );
        assert_eq!(r["saved"], true);
        let saved = s.store.load(&id).unwrap();
        let want = applied(&es, &hc::merge_onto(&es, 4, &[2], Target::Eave));
        let wall2 = want[&2].eave_height.unwrap();
        assert!((saved.parts[0].edges[2].eave_height - wall2).abs() < 1e-9);
        assert_eq!(saved.parts[0].edges[2].pitch_deg, es[4].pitch_deg);
        assert!(
            s.call_tool(
                "roof_helper",
                &json!({ "id": id, "edge": 4, "tool": "merge", "target": "pitch" })
            )
            .unwrap_err()
            .contains("eave, overhang or position")
        );
        let _ = std::fs::remove_dir_all(&s.store.dir);
    }

    #[test]
    fn roof_helper_hip_sets_the_core_values() {
        let mut s = server();
        let id = example(&mut s, "hip");
        let es = s.store.load(&id).unwrap().parts[0].edges.clone();
        let n = es.len();
        // Measured case of hip_pitches_from_eaves: hip 7.5, pieces 5 and 4.5.
        type Hip = fn(&[tectora_core::EdgeDef], usize, usize, f64, f64, f64) -> Option<Patches>;
        let cases: [(&str, f64, f64, Hip); 2] = [
            ("rafters", 6.0, 5.5, hc::hip_pitches),
            ("eaves", 5.0, 4.5, hc::hip_pitches_from_eaves),
        ];
        for (by, a, b, f) in cases {
            for (neighbour, other) in [("next", 1), ("prev", n - 1)] {
                let want = applied(&es, &f(&es, 0, other, 7.5, a, b).unwrap());
                let r = call(
                    &mut s,
                    "roof_helper",
                    json!({ "id": id, "edge": 0, "tool": "hip", "neighbour": neighbour,
                            "hip": 7.5, "by": by, "first": a, "second": b }),
                );
                let got = patches(&r);
                assert_eq!(got, want, "{by} {neighbour}");
                assert_eq!(
                    got.keys().copied().collect::<Vec<_>>(),
                    {
                        let mut k = vec![0, other];
                        k.sort_unstable();
                        k
                    },
                    "{by} {neighbour}: this wall and the named neighbour"
                );
            }
        }
        let e = s
            .call_tool(
                "roof_helper",
                &json!({ "id": id, "edge": 0, "tool": "hip", "hip": 7.5, "first": 5 }),
            )
            .unwrap_err();
        assert!(e.contains("second"), "{e}");
        let _ = std::fs::remove_dir_all(&s.store.dir);
    }

    #[test]
    fn roof_helper_build_up_returns_the_core_patch() {
        let mut s = server();
        let id = example(&mut s, "gable");
        let es = s.store.load(&id).unwrap().parts[0].edges.clone();
        for (eave, e) in [
            ("normal", BuildUpEave::Normal),
            ("horizontal", BuildUpEave::Horizontal),
            ("vertical", BuildUpEave::Vertical),
        ] {
            let core = hc::build_up(&es[0], 0.2, e).unwrap();
            let want = applied(&es, &Patches::from([(0, core)]));
            let r = call(
                &mut s,
                "roof_helper",
                json!({ "id": id, "edge": 0, "tool": "build_up", "thickness": 0.2, "eave": eave }),
            );
            assert_eq!(patches(&r), want, "{eave}");
        }
        let gable = es.iter().position(|e| e.is_gable()).unwrap();
        assert!(
            s.call_tool(
                "roof_helper",
                &json!({ "id": id, "edge": gable, "tool": "build_up", "thickness": 0.2 })
            )
            .is_err()
        );
        let _ = std::fs::remove_dir_all(&s.store.dir);
    }
}
