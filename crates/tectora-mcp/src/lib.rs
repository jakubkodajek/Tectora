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
use tectora_core::quantities::{
    RoofCoverage, area_by_pitch_in, dacad_line_totals_of, roof_totals_of,
};
use tectora_core::roof::LineKind;
use tectora_core::roof::RoofGeometry;
use tectora_core::settings::{RoofDefaults, Settings};
use tectora_core::store::{CheckedSave, LiveStatus, Store, StoreError, now_ms};
use tectora_core::{
    Chimney, EdgeDef, HalfHip, JobStatus, PitchBreak, Project, RoofPart, RoofWindow,
};

mod drawings;
#[cfg(test)]
mod input_tests;
mod more;
mod worker;

pub const GUIDE: &str = include_str!("guide.md");
const SUPPORTED_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

pub struct Server {
    store: Store,
    /// Name of the connected client, used as the actor in the activity log.
    client: String,
    /// Set while a calculation that overran [`COMMIT_CALC_TIMEOUT`] still
    /// runs in its thread: no second one is started beside it (ERR-011).
    calc_busy: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// How long a write waits for its calculations (tests shorten it).
    commit_timeout: std::time::Duration,
}

/// Longest a write waits for the calculations behind its answer (window
/// check, quantities). A job inside every limit can still take over a
/// minute; the write is saved first and the answer says the figures are
/// missing (ERR-011).
const COMMIT_CALC_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

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

/// An optional text argument: a string, or left out / null. Any other
/// value is refused, never dropped (HUNTIO-012, ERR-002).
pub(crate) fn text_arg(args: &Value, key: &str) -> Result<Option<String>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(v) => Err(format!("`{key}` must be a string, not {v}")),
    }
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
    // `+ 0.0`: a total a hair below zero (a roof an object took away
    // completely) rounds to -0.0, which JSON writes as "-0.0".
    (v * f).round() / f + 0.0
}

#[cfg(test)]
mod round_tests {
    /// Audit HUNTC-004: an area of -0.00002 m2 was reported as "-0.0".
    #[test]
    fn a_total_a_hair_below_zero_is_zero_not_minus_zero() {
        let v = super::round(-0.0000209, 2);
        assert_eq!(serde_json::json!(v).to_string(), "0.0");
        assert_eq!(super::round(-0.006, 2), -0.01);
        assert_eq!(super::round(152.734, 2), 152.73);
    }
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
        WarningKind::NotCalculatedByCore => "the standard calculation stops on this roof (or could not run), so Tectora calculated it with its own engine; the roof of a building that stands clear of the others passed Tectora's self-check (planar faces at their pitches, every wall's eave height, the plan covered, every line on a face), buildings that touch each other are joined without that check. The figures are not the standard result; tell the user to look at the roof".to_string(),
        WarningKind::PlaneOverWall => {
            let (outer, inner) = (w.edges.first().map_or(0, |e| *e), w.edges.get(1).map_or(0, |e| *e));
            format!(
                "the roof plane of edge {outer} (0-based) is parallel to that of edge {inner} (same direction and pitch) and lies {:.2} m higher, so it runs on past edge {inner}'s eave as one plane: the part in front has no ridge of its own. If it should have one (a wing with a cross gable), make its side walls eaves and its front wall a gable or a hip",
                w.value
            )
        }
        WarningKind::MergedPlanes => {
            let (outer, inner) = (w.edges.first().map_or(0, |e| *e), w.edges.get(1).map_or(0, |e| *e));
            format!(
                "the roof planes of edges {outer} and {inner} (0-based) lie in one plane ({:.1} cm apart), so Tectora calculated them as one face, with edge {outer}'s eave in that plane; the standard calculation stops with an error on such a roof or leaves a step of those few centimetres. Tell the user; no change needed unless the planes were meant to differ",
                w.value * 100.0
            )
        }
        WarningKind::WingJoined => format!(
            "the roof of this building did not close in one calculation (a wing with eaves too low to reach the house's roof), so the wing with edges {:?} (0-based) was calculated as a building of its own against the house and the two roofs joined; the standard calculation stops with an error on this building. Tell the user",
            w.edges
        ),
        WarningKind::HiddenUnderNeighbour => format!(
            "{:.2} m2 of this building's roof lies inside another building's walls, under that building's roof: hidden in its attic, yet counted in the area (usually the overhang of a wall standing on the other building's wall, where the join does not end the roof at that wall). The result is the standard calculation's and is kept. Tell the user; setting that wall's overhang to 0 removes the hidden piece",
            w.value
        ),
        WarningKind::UncoveredPlan => format!(
            "{:.2} m2 of plan within this building's walls and overhangs has no roof face of any building over it, so the roof area is too small (the join of the roofs lost faces, e.g. two pyramid roofs in a row). The result is the standard calculation's and is kept. Tell the user to check the roof at the join",
            w.value
        ),
        WarningKind::DoubledLine => format!(
            "roof lines of the joined roofs lie on top of each other, so {:.2} m is counted twice and one of the two lines usually has a code the roof does not have (a hip reported once more as an oblique verge where two hip roofs with different eave heights touch); the result is kept as calculated. Tell the user to check the line lengths at the join",
            w.value
        ),
        WarningKind::FarFromOrigin => format!(
            "the building stands {:.1} km from the plan's origin; the calculation works in single precision on plan coordinates, so that far out the roof area can be wrong (about 1 % at 2.5 km, far more beyond 60 km). Move the whole plan close to the origin (transform_part) and calculate again",
            w.value / 1000.0
        ),
        WarningKind::NoRoofArea => "the roof of this building has no area (0.00 m2): a chimney or roof opening larger than the roof took all of it (cm typed as m), or every eave is at about 100 m, where the calculation closes the faces empty; check the chimneys, openings and eave heights".to_string(),
        WarningKind::ObjectsOverlap => format!(
            "{:.0} pair(s) of objects on this roof lie on top of each other in plan (windows, chimneys, roof openings, dormers); each is still deducted and its lines counted in full. Usually one object was added twice: delete the duplicate",
            w.value
        ),
        WarningKind::DormerAboveRoof => format!(
            "a dormer rises {:.2} m above the highest point of the roof it sits on: its roof misses the roof face before the ridge, so the area and line lengths are too large; lower the dormer's pitch or eave height",
            w.value
        ),
        WarningKind::InsideAnother => "this building's walls lie wholly within another building's walls and its roof, outside the other roof's planes, still adds area and lines; check whether it is a copy left on its original or a building added twice. A building that marks part of a roof face (for example another material) is meant: keep it and give it the pitch and eave height of the roof around it".to_string(),
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
    // The sentences add up: each says something the others do not
    // (review ERR-005: the second used to overwrite the first).
    let mut text = message.to_string();
    let mut v = json!({ "kind": e.kind });
    if let [outer, inner] = e.edges[..] {
        v["edges"] = json!(e.edges);
        text.push_str(&format!(
            ". The roof planes of edges {outer} and {inner} (0-based) are parallel (same direction and pitch) and edge {outer}'s lies in or a little below edge {inner}'s, so they never meet: raise edge {outer}'s eave, make the side walls beside edge {outer} gables (one shared plane), or, for a wing with its own ridge, make its side walls eaves and its front wall a gable or a hip"
        ));
    }
    // Which buildings end with the error on their own too: fix those; the
    // others are calculated without them (review ARCH-002).
    if !e.parts.is_empty() {
        v["failing_parts"] = json!(e.parts);
        text.push_str(
            ". The building(s) in failing_parts (indices) fail on their own; the other buildings are calculated without them",
        );
    }
    v["message"] = text.into();
    v
}

/// Refuses an export while a building has no roof or input is left out:
/// its figures would be missing from the sheet, as the editor's Export is
/// blocked then too.
pub(crate) fn export_ready(project: &Project) -> Result<(), String> {
    // No building and no hand-drawn face: no figures, as `store_result`
    // and the CLI say (HUNT-305 item 4).
    if project.parts.is_empty() && project.manual_faces.is_empty() {
        return Err("not exported: the job has no building, so there are no figures to export; nothing was written".into());
    }
    let Some(t) = roof_trouble(project) else {
        return Ok(());
    };
    // Its own sentence, nothing of the write path's "Saved, but … revert"
    // (ERR-008): nothing was saved, and there is nothing to revert.
    let mut why: Vec<String> = Vec::new();
    for e in t["roof_errors"].as_array().into_iter().flatten() {
        why.push(format!(
            "building {} has no roof ({}: {})",
            e["part"],
            e["error"]["kind"].as_str().unwrap_or("error"),
            e["error"]["message"].as_str().unwrap_or("")
        ));
    }
    for p in t["problems"].as_array().into_iter().flatten() {
        let edge = p["edge"]
            .as_u64()
            .map(|e| format!(", wall {e}"))
            .unwrap_or_default();
        why.push(format!(
            "{} {}{edge} is left out ({})",
            p["item"].as_str().unwrap_or("item"),
            p["index"],
            p["code"].as_str().unwrap_or("problem")
        ));
    }
    for c in t["cut_exceeds_face"].as_array().into_iter().flatten() {
        why.push(format!(
            "building {}: {}",
            c["part"],
            c["message"]
                .as_str()
                .unwrap_or("a face lost more than it has")
        ));
    }
    Err(format!(
        "not exported: {}. Fix the job first (calc lists the details); nothing was written.",
        why.join("; ")
    ))
}

/// The buildings without a roof after a write, and the input problems that
/// left a building out, so an AI learns at once that its change broke a
/// roof instead of reading "Saved" (review ERR-003). Empty when all is well.
fn roof_trouble(project: &Project) -> Option<Value> {
    roof_trouble_of(&tectora_core::openings::generate_project(project), project)
}

/// [`roof_trouble`] of roofs already calculated.
fn roof_trouble_of(geos: &[RoofGeometry], project: &Project) -> Option<Value> {
    let errors: Vec<Value> = geos
        .iter()
        .enumerate()
        .filter_map(|(i, g)| {
            g.error.as_ref().map(|e| {
                json!({ "part": i, "name": project.parts.get(i).map(|p| p.name.as_str()), "error": calc_error(e) })
            })
        })
        .collect();
    let problems: Vec<Value> = project
        .problems()
        .iter()
        .map(|p| serde_json::to_value(p).unwrap_or(Value::Null))
        .collect();
    // A face that lost more than it has: a negative area reaches the totals
    // (HUNTC-003).
    let cut: Vec<Value> = geos
        .iter()
        .enumerate()
        .flat_map(|(i, g)| {
            g.warnings
                .iter()
                .filter(|w| w.kind == tectora_core::check::WarningKind::CutExceedsFace)
                .map(move |w| {
                    let mut v = warning(w);
                    v["part"] = json!(i);
                    v
                })
        })
        .collect();
    if errors.is_empty() && problems.is_empty() && cut.is_empty() {
        return None;
    }
    let mut out = json!({
        "message": "Saved, but some roofs are not calculated or have a negative face; fix them (or revert this change) before you continue",
        "roof_errors": errors,
        "problems": problems,
    });
    if !cut.is_empty() {
        out["cut_exceeds_face"] = Value::Array(cut);
    }
    Some(out)
}

/// A library error for an AI client, with its stable code in front where
/// the client is to act on it: `revert_stale` (look at the history again)
/// and `busy` (another program holds the job's lock; nothing was done, try
/// again in a moment, audit 2026-11-23 ERR-1002), `revert_out_of_limits`
/// (the earlier version is outside the limits; the job is unchanged) and
/// `revert_out_of_limits_damaged` (the same, while the job file itself
/// cannot be read, audit 2026-11-23 ARCH-1503).
pub(crate) fn coded(e: StoreError) -> String {
    match e {
        StoreError::RevertStale { .. }
        | StoreError::Busy(_)
        | StoreError::TooMany { .. }
        | StoreError::RevertOutOfLimits { .. }
        | StoreError::RevertOutOfLimitsDamaged { .. } => {
            format!("{}: {e}", e.code())
        }
        e => e.to_string(),
    }
}

/// A revert that wrote over a job file another program had made
/// unreadable says so, for the AI to tell the user (audit 2026-11-23
/// ERR-1502).
pub(crate) fn replaced_note(out: &mut Value, repaired: bool) {
    if repaired {
        out["replaced_unreadable_file"] = json!(true);
        out["note"] = json!(
            "the job file could not be read (another program had damaged it or nested it too deep); it was replaced with the restored version: tell the user"
        );
    }
}

fn quantities(project: &Project) -> Value {
    quantities_of(&tectora_core::openings::generate_project(project), project)
}

/// [`quantities`] of roofs already calculated: one request calculates the
/// roof once and hands it to every total (review ARCH-006).
fn quantities_of(geos: &[RoofGeometry], project: &Project) -> Value {
    let t = roof_totals_of(geos, project);
    let mut q = json!({
        "roof_area_m2": round(t.roof_area, 2),
        "ridge_height_m": round(t.ridge_height, 2),
        "hip_count": t.hip_count,
        "valley_count": t.valley_count,
        "lines_by_dacad_code_m": dacad_line_totals_of(geos, project),
        "area_by_pitch_m2": area_by_pitch_in(geos, project)
            .iter()
            .map(|&(p, a)| json!({ "pitch_deg": p, "area": a }))
            .collect::<Vec<_>>(),
        "area_by_material_m2": t
            .area_by_material
            .iter()
            .map(|&(m, a)| json!({ "material": m, "name": project.material_label(m), "area": round(a, 2) }))
            .collect::<Vec<_>>(),
    });
    // With more than one material: the lines each one's offer needs.
    let by_line = tectora_core::quantities::lines_by_material_of(geos);
    if by_line.len() > 1 {
        q["lines_by_material_m"] = by_line
            .into_iter()
            .map(|(m, lines)| json!({ "material": m, "name": project.material_label(m), "lines_by_dacad_code_m": lines }))
            .collect();
    }
    q["heights_m"] = heights(geos, project);
    q
}

/// The heights a section drawing labels, per part: the distinct heights of
/// its level eaves and pitch breaks and its highest point, so they can be
/// checked against the drawing's labels without reading every line.
fn heights(geos: &[RoofGeometry], project: &Project) -> Value {
    let distinct = |mut v: Vec<f64>| {
        v.sort_by(f64::total_cmp);
        v.dedup_by(|a, b| (*a - *b).abs() < 0.015);
        v
    };
    geos.iter()
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
    apply_edge_fields_with(edge, v, None)
}

/// [`apply_edge_fields`]; `eave_fallback` is the pitch a gable turned into
/// an eave without a `pitch_deg` gets (`edit::eave_pitch`: the nearest eave
/// wall's, as in the editor), else the user's default pitch (ARCH-005).
fn apply_edge_fields_with(
    edge: &mut EdgeDef,
    v: &Value,
    eave_fallback: Option<f64>,
) -> Result<(), String> {
    if let Some(p) = arg_f64(v, "pitch_deg")? {
        // The editor's wall bar refuses the same values (UX-010).
        tectora_core::edit::check_eave_pitch(p, MIN_PITCH_DEG)?;
        edge.pitch_deg = p;
    }
    let gable = match v.get("gable") {
        None | Some(Value::Null) => None,
        Some(g) => Some(g.as_bool().ok_or("gable must be true or false")?),
    };
    match gable {
        Some(true) => edge.pitch_deg = GABLE_PITCH_DEG.ceil(),
        Some(false) if edge.is_gable() => {
            edge.pitch_deg = match arg_f64(v, "pitch_deg")? {
                Some(p) => p,
                None => eave_fallback.unwrap_or_else(|| Settings::load().roof_defaults.pitch_deg),
            };
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
            // Checked like `breaks` (HUNTIO-012): a half-hip is a roof plane.
            if !edge.is_gable() {
                return Err("half_hip applies only to a gable wall; set `gable` first".into());
            }
            edge.half_hip = Some(hh);
            // The core's own limits (pitch 2..90, not below the eave).
            if let Some((code, why)) = edge
                .problems()
                .find(|(c, _)| ["pitch_range", "stacked_nan", "half_hip_below_eave"].contains(c))
            {
                return Err(format!("half_hip: {code}: {why}"));
            }
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
    let mut pts: Vec<Point2> = pts.into_iter().map(|[x, y]| Point2::new(x, y)).collect();
    // A closed polygon as most tools write it, the first corner repeated
    // at the end: that corner is dropped, not made a wall of no length
    // that leaves the building out (HUNT-108).
    if pts.len() > 3 && pts[0].distance(pts[pts.len() - 1]) < tectora_core::model::MIN_WALL_LENGTH {
        pts.pop();
    }
    Ok(pts)
}

/// A new wall: every edge field given in `defaults`; pitch, eave height and
/// overhang otherwise from the user's settings (Settings › New roofs).
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
    // Every other field of an edge (gable, breaks, half_hip, material,
    // priority, clad) applies to every edge too, as the schema says.
    let mut edge = edge;
    let basic = ["pitch_deg", "eave_height", "overhang"];
    let more = defaults
        .as_object()
        .is_some_and(|o| o.keys().any(|k| !basic.contains(&k.as_str())));
    if more {
        apply_edge_fields(&mut edge, defaults).map_err(|m| format!("defaults: {m}"))?;
    }
    Ok(edge)
}

/// Builds a part from `{name?, points, defaults?, edges?}`.
fn parse_part(v: &Value, index: usize) -> Result<RoofPart, String> {
    let raw = arg(v, "points")?;
    let points = parse_points(raw)?;
    let defaults = v.get("defaults").cloned().unwrap_or(json!({}));
    let base = default_edge(&defaults)?;
    let mut specs = v
        .get("edges")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    // The entry of a dropped closing corner went with it.
    if raw.as_array().is_some_and(|a| a.len() == points.len() + 1)
        && specs.len() == points.len() + 1
    {
        specs.pop();
    }
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
    // `n - 1` of an empty part read "0..18446744073709551615" (ERR-007).
    if n == 0 {
        return Err("the part has no walls; give it an outline with set_outline first".to_string());
    }
    if v.get("all").and_then(Value::as_bool) == Some(true) {
        return Ok((0..n).collect());
    }
    let list: Vec<usize> = if let Some(i) = v.get("index") {
        let i = i
            .as_u64()
            .ok_or_else(|| format!("`index` must be an edge number from 0, not {i}"))?;
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

/// Argument paths whose enum the tool checks with its own explanation
/// (`export_plan` says why `png` is not available over MCP).
const ENUMS_EXPLAINED_BY_TOOL: &[&str] = &["format"];

/// Checks `value` against the types, enums and item counts of `schema`
/// (the tool's published `inputSchema`): a value of the wrong type or
/// outside an enum used to be dropped or replaced by a default, and the
/// tool still answered "Saved" (ERR-002, ERR-005). `null` counts as left
/// out, as every tool reads it. Number ranges stay with each tool, which
/// says why a value is out of range. Returns the first mismatch, with its
/// path.
fn schema_mismatch(schema: &Value, value: &Value, path: &str) -> Option<String> {
    if value.is_null() {
        return None;
    }
    // Enums a tool answers itself with a better reason.
    if ENUMS_EXPLAINED_BY_TOOL.contains(&path) && value.is_string() {
        return None;
    }
    // The value as the client sent it, cut so an answer never echoes a
    // whole document.
    let shown: String = {
        let t = value.to_string();
        if t.chars().count() > 60 {
            format!("{}…", t.chars().take(60).collect::<String>())
        } else {
            t
        }
    };
    let at = if path.is_empty() {
        "the arguments".to_string()
    } else {
        format!("`{path}`")
    };
    let is = |t: &str| match t {
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "object" => value.is_object(),
        "array" => value.is_array(),
        "number" => value.is_number(),
        "integer" => {
            value.is_i64()
                || value.is_u64()
                || value
                    .as_f64()
                    .is_some_and(|f| f.is_finite() && f.fract() == 0.0)
        }
        "null" => value.is_null(),
        _ => true,
    };
    let types: Vec<&str> = match schema.get("type") {
        Some(Value::String(t)) => vec![t.as_str()],
        Some(Value::Array(ts)) => ts.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    if !types.is_empty() && !types.iter().any(|t| is(t)) {
        let want: Vec<&str> = types.iter().copied().filter(|t| *t != "null").collect();
        return Some(format!("{at} must be {}, not {shown}", want.join(" or ")));
    }
    if let Some(options) = schema.get("enum").and_then(Value::as_array)
        && !options.contains(value)
    {
        let list: Vec<String> = options
            .iter()
            .filter(|o| !o.is_null())
            .map(|o| o.as_str().map_or_else(|| o.to_string(), str::to_string))
            .collect();
        return Some(format!(
            "{at} must be one of {}, not {shown}",
            list.join(", ")
        ));
    }
    match value {
        Value::Object(map) => {
            let props = schema.get("properties").and_then(Value::as_object);
            let extra = schema.get("additionalProperties").filter(|a| a.is_object());
            for (k, v) in map {
                let here = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                if let Some(sub) = props.and_then(|p| p.get(k)).or(extra)
                    && let Some(m) = schema_mismatch(sub, v, &here)
                {
                    return Some(m);
                }
            }
            None
        }
        Value::Array(items) => {
            let n = items.len() as u64;
            if let Some(min) = schema.get("minItems").and_then(Value::as_u64)
                && n < min
            {
                return Some(format!("{at} must have at least {min} entries, not {n}"));
            }
            if let Some(max) = schema.get("maxItems").and_then(Value::as_u64)
                && n > max
            {
                return Some(format!("{at} must have at most {max} entries, not {n}"));
            }
            let sub = schema.get("items")?;
            items
                .iter()
                .enumerate()
                .find_map(|(i, v)| schema_mismatch(sub, v, &format!("{path}[{i}]")))
        }
        _ => None,
    }
}

fn schema(props: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": props, "required": required })
}

fn edge_props() -> Value {
    json!({
        "pitch_deg": { "type": "number", "description": "Roof pitch rising from the edge, degrees: 2 to below 90 for an eave (use `gable` for a gable wall)." },
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
            "description": "Change Tectora settings, same as the app's Settings screen. Pass only the keys to change; null resets a folder or the language to its default. A new projects_dir or drawings_dir is only a request: the app asks the user to allow it, and the folders stay as they are until then. Every other change applies at once and is shown to the user with your summary and the values before and after, to accept or revert (`settings.ai_changes`).",
            "inputSchema": schema(json!({
                "projects_dir": { "type": ["string", "null"], "description": "Absolute path of the design library folder." },
                "drawings_dir": { "type": ["string", "null"], "description": "Absolute path of the drawings inbox; default <projects_dir>/drawings." },
                "locale": { "type": ["string", "null"], "enum": ["cs", "sk", "en", "de", null] },
                "theme": { "type": "string", "enum": ["system", "light", "dark"] },
                "roof_defaults": {
                    "type": "object",
                    "properties": {
                        "pitch_deg": { "type": "number", "minimum": RoofDefaults::PITCH.0, "maximum": RoofDefaults::PITCH.1 },
                        "eave_height": { "type": "number", "minimum": RoofDefaults::EAVE.0, "maximum": RoofDefaults::EAVE.1 },
                        "overhang": { "type": "number", "minimum": RoofDefaults::OVERHANG.0, "maximum": RoofDefaults::OVERHANG.1 }
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
            "description": "Full design: parts with an edge table (index, from, to, length, eave/gable, pitch, eave height, overhang), job data and quantities. `result_recalculation`: Tectora's recalculation the stored result is (its `entry_ts_ms` for restore_offer_result), or null. `load_warnings`: what reading the job left out (`result_dropped`: a stored result with an impossible date); tell the user.",
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
                            "height": { "type": "number", "description": "Top height, m above ground (drawn in 3D only; a chimney is calculated as a chimney whatever its height, also without one)." }
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
            "description": "Compute the roof: area, ridge height and line lengths by DACAD code (T eave, F ridge, G hip, K valley, OL/OR/OS verge left/right/oblique, P shed top edge, c pitch break), per part and in total; a part's line lengths are net of deductions like the totals. Each part may carry `warnings` (`kind`, `value`, `message`, and `edges`, `dormer` or `item` where they apply): `outside_outline`, `overlapping_faces`, `area_mismatch` (the roof looks geometrically wrong), `merged_walls` (two walls in line calculated as one, so one wall's pitch is not used), `covered` (no roof of its own: another part covers it; move or delete one), `cut_exceeds_face` (a face lost more than it has, e.g. a window larger than its face), `dormer_off_roof` (a dormer on no roof face, left out of the result), `window_off_roof`, `chimney_off_roof`, `opening_off_roof` (that object, `item` index, on no roof face, left out), `doubled_line` (joined roofs: a line lies on another one, `value` m counted twice), `no_roof_area` (roofed walls, 0 m² of roof: a chimney or opening took all of it, or eaves at 100 m), `objects_overlap` (`value` pairs of windows, chimneys, openings or dormers on top of each other, each still deducted), `dormer_above_roof` (a dormer `value` m above the roof it sits on: area and lines too large), `uncovered_plan` (joined buildings: `value` m2 of plan within this building's eave outline has no roof over it, the area is too small), `hidden_under_neighbour` (joined buildings: `value` m2 of this roof lies inside another building's walls under its roof, hidden yet counted; usually the overhang on a shared wall, which the user can set to 0), `inside_another` (a building wholly within another one's walls whose roof leaves the other roof's planes still adds area; keep a building that marks part of a face and give it the roof's pitch and eave height), `not_calculated_by_core` (the standard calculation stops on this roof and Tectora's own engine calculated it, result self-checked: its figures may differ from the standard result, so check the roof); and these notices, which say the roof is sound as calculated: `far_from_origin` (the building is `value` m, 2.5 km or more, from the origin, where the area loses precision; move the plan to the origin), `plane_over_wall` (a wing's plane runs on over the house's eave: no ridge of its own), and two of a roof that is not the standard result because the standard calculation stops on it: `merged_planes` (two parallel planes within 10 cm calculated as one face) and `wing_joined` (a low wing calculated as a lean-to against the house). Tell the user about every warning and notice. A part the calculation gave no roof for carries `error` (`kind`, `message`). `joined_with` lists the parts (`part` index, `name`) the calculation actually joined this part's roof with.",
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
            "description": "Undo the latest recorded change of a design. With `ts_ms` (from get_history) it is undone only if it is still that change: when another one was saved meanwhile nothing is undone (`revert_stale`). Refused with `busy` (nothing done, try again in a moment) while another program is saving the job.",
            "inputSchema": schema(json!({ "id": { "type": "string" }, "ts_ms": { "type": "integer", "description": "The change you expect to undo." }, "summary": summary_prop() }), &["id", "summary"]),
        },
    ])
}

impl Server {
    pub fn new(store: Store) -> Self {
        Self {
            store,
            client: "AI".into(),
            calc_busy: Default::default(),
            commit_timeout: COMMIT_CALC_TIMEOUT,
        }
    }

    /// Parts from which a job may take long enough to need the deadline of
    /// [`Server::within`]: the join of the roofs grows with the square of
    /// them. Smaller jobs are calculated on the calling thread.
    const HEAVY_PARTS: usize = 30;

    /// `f` at once for a small job, within the commit timeout for a heavy one.
    fn bounded<T: Send + 'static>(
        &self,
        project: &Project,
        f: impl FnOnce() -> T + Send + 'static,
    ) -> Option<T> {
        if project.parts.len() < Self::HEAVY_PARTS {
            Some(f())
        } else {
            self.within(f)
        }
    }

    /// Runs `f` on its own thread and waits for it at most the commit
    /// timeout; `None` when it took longer (it then finishes unobserved and
    /// no other one starts until it has) or panicked.
    fn within<T: Send + 'static>(&self, f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
        use std::sync::atomic::Ordering::SeqCst;
        if self.calc_busy.swap(true, SeqCst) {
            return None;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let busy = std::sync::Arc::clone(&self.calc_busy);
        let started = std::thread::Builder::new()
            .name("commit calculation".into())
            .spawn(move || {
                // Cleared also when `f` panics.
                struct Clear(std::sync::Arc<std::sync::atomic::AtomicBool>);
                impl Drop for Clear {
                    fn drop(&mut self) {
                        self.0.store(false, SeqCst);
                    }
                }
                let _clear = Clear(busy);
                let _ = tx.send(f());
            });
        if started.is_err() {
            self.calc_busy.store(false, SeqCst);
            return None;
        }
        rx.recv_timeout(self.commit_timeout).ok()
    }

    /// Tells the app what the agent does now: `kind` (one of
    /// `store::LIVE_KINDS`, translated in the app) about `name` (the job or
    /// file), with `message` in English for logs and older apps. The app
    /// shows it and never opens the job for it (reading, calculating, exporting).
    /// Checks a `path` argument of an AI client: a file in the jobs folder
    /// or the drawings inbox, or one the user allowed, passes. Any other
    /// path becomes a request the user answers in the app
    /// (`Settings::file_request`, ARCH-002) and the call is refused until
    /// then, with the same words whether the file exists or not.
    pub(crate) fn readable_path(
        &self,
        path: &std::path::Path,
        purpose: &str,
    ) -> Result<(), String> {
        if !path.is_absolute() {
            return Err("`path` must be an absolute path".into());
        }
        let settings = Settings::load();
        if settings.ai_may_read(path, &self.store.dir) {
            return Ok(());
        }
        let asked =
            Settings::update(|s| Ok(s.asking_for_file(path, &self.client, purpose))).is_ok();
        // A settings change: the app reads the settings again and asks.
        self.live(
            "settings",
            None,
            format!("Asked to read {}", path.display()),
            None,
        );
        Err(if asked {
            format!(
                "{} is outside the jobs folder and the drawings inbox, so Tectora now asks the user to allow it. Tell the user to answer the question in the Tectora window, then call this tool again with the same path. Do not try other paths or tools to reach the file.",
                path.display()
            )
        } else {
            format!(
                "{} is outside the jobs folder and the drawings inbox and the request to the user could not be stored; ask the user to move the file into the drawings inbox (see get_settings).",
                path.display()
            )
        })
    }

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

    /// [`Store::load_warnings`] of `id` as the tools give them: what
    /// reading the job left out (a stored result with an impossible date).
    fn load_warnings(&self, id: &str) -> Vec<Value> {
        self.store
            .load_warnings(id)
            .unwrap_or_default()
            .into_iter()
            .map(|w| json!({ "code": w.code, "detail": w.detail }))
            .collect()
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
        // A job text or name this write sets is one line of plain text
        // (ARCH-104, ARCH-202); one the job already had stays editable
        // around it (TEST-206).
        tectora_core::edit::check_texts_change(before, project)?;
        // Every write: no window of no size or larger than its face
        // (HUNTC-003), not only `set_openings`.
        let skipped_check = if project.windows.is_empty() {
            false
        } else {
            let (b, p) = (before.cloned(), project.clone());
            let check = move || tectora_core::edit::check_windows_write(b.as_ref(), &p);
            match self.bounded(project, check) {
                Some(r) => {
                    r?;
                    false
                }
                None => true,
            }
        };
        if let (Some(id), Some(b)) = (id, before)
            && b == project
        {
            return Ok(json!({
                "id": id,
                "saved": false,
                "note": "Nothing changed: the job already has these values, so nothing was saved.",
                "changes": [],
            }));
        }
        // Read before the save, which writes the job without what reading
        // it left out (audit 2026-11-23 ARCH-902).
        let dropped = id.map(|id| self.load_warnings(id)).unwrap_or_default();
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
        let p = project.clone();
        let geos = self.bounded(project, move || {
            tectora_core::openings::generate_project(&p)
        });
        self.live_follow("changed", None, summary.to_string(), &id);
        let changes = before
            .as_ref()
            .map(|b| diff(b, project))
            .unwrap_or_default();
        let mut out = json!({
            "id": id,
            "saved": true,
            "note": "Saved. The change is now visible in the Tectora app for the user to review.",
            "changes": changes,
            "parts": project.parts.iter().map(describe_part).collect::<Vec<_>>(),
        });
        match geos {
            Some(geos) => {
                out["quantities"] = quantities_of(&geos, project);
                if let Some(t) = roof_trouble_of(&geos, project) {
                    out["roof_trouble"] = t;
                }
                // The roof's warnings, as calc gives them, so an AI hears
                // about a roof that looks wrong at the write that made it
                // (HUNT-106).
                let warnings: Vec<Value> = geos
                    .iter()
                    .enumerate()
                    .flat_map(|(i, g)| {
                        g.warnings.iter().map(move |w| {
                            let mut v = warning(w);
                            v["part"] = json!(i);
                            v
                        })
                    })
                    .collect();
                if !warnings.is_empty() {
                    out["warnings"] = Value::Array(warnings);
                }
                // A job past the draft keeps the figures its offer was made
                // with: say at once when this write makes them differ
                // (ERR-006), not only in a later calc.
                if let Some(st) = tectora_core::snapshot::status_of(&geos, project)
                    && !st.differences.is_empty()
                {
                    let status = serde_json::to_value(project.job.status)
                        .ok()
                        .and_then(|v| v.as_str().map(str::to_string))
                        .unwrap_or_default();
                    let area = st.differences.iter().find(|d| d.key == "roof_area");
                    let figures = match area {
                        Some(d) => format!(
                            " (roof area stored {} m², now {} m²)",
                            round(d.stored, 2),
                            round(d.current, 2)
                        ),
                        None => String::new(),
                    };
                    out["note"] = json!(format!(
                        "Saved. The change is now visible in the Tectora app for the user to review. The job is `{status}` and its stored offer figures differ from the roof now{figures}: tell the user, and call store_result only if they want the offer figures replaced."
                    ));
                    out["stored_result"] = json!({
                        "status": status,
                        "differences": st.differences,
                    });
                }
            }
            None => {
                out["quantities"] = Value::Null;
                out["calculation_note"] = json!(
                    "Saved, but the roof of this job takes too long to calculate here, so no figures are given. Call calc on it (it has its own limit) and tell the user."
                );
            }
        }
        if !dropped.is_empty() {
            let note = out["note"].as_str().unwrap_or("Saved.").to_string();
            // What the saved job holds, not what was read (audit
            // 2026-11-23 ERR-1001: store_result and a status change store
            // the current figures in its place).
            let what = if project.result_snapshot.is_some() {
                "this save stored the current figures in its place: tell the user that the offer's own figures are lost and the stored result is now the current one."
            } else {
                "this save wrote the job without it: tell the user, and call store_result if they want the current figures stored as the offer's."
            };
            out["note"] = json!(format!(
                "{note} The job's stored offer result had an impossible date, so it was left out when the job was read, and {what}"
            ));
            out["load_warnings"] = Value::Array(dropped);
        }
        if skipped_check {
            out["window_check_skipped"] = json!(
                "The roof is too slow to check that the roof windows fit their faces; check them with calc."
            );
        }
        Ok(out)
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
            let saved = self
                .store
                .save_as_checked(id, &next, &self.client, summary, Some(&base))
                .map_err(|e| match e {
                    // The user deleted the job while this change was made:
                    // nothing is written, the job stays in the trash (audit
                    // 2026-11-24 ARCH-1702).
                    StoreError::NotFound(_) => format!(
                        "no design with id `{id}`: it was deleted in the app while this \
                         change was made, and nothing was saved. Do not create it again \
                         unless the user asks; call list_projects to see the ids"
                    ),
                    e => e.to_string(),
                })?;
            match saved {
                CheckedSave::Saved { id, .. } => return Ok((id, Some(base), next)),
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
        // Read, changed and written under the settings lock: a change the
        // user makes in the app at the same moment is not lost, nor this one (TEST-307).
        let mut asked = false;
        let (old, next) = Settings::update(|s| {
            let (next, a) = s.patched_by_ai(&patch, &self.client, &summary)?;
            asked = a;
            Ok(next)
        })
        .map_err(|e| format!("settings not saved: {}", e.explain()))?;
        self.live(
            "settings",
            None,
            format!("Changed Tectora settings: {summary}"),
            None,
        );
        let mut out = next.describe();
        let mut notes = Vec::new();
        if next.ai_changes.last().map(|c| c.id) != old.ai_changes.last().map(|c| c.id) {
            // Applied at once, but the user sees it and can put the old
            // values back (UX-003).
            notes.push(
                "Applied. The app shows the user this change with your summary and the values before and after, to Accept or Revert; call get_settings later to see the answer (`ai_changes`).",
            );
        }
        if asked {
            // The folders change only when the user allows it in the app
            // (ARCH-052); this server keeps its library until then.
            notes.push(
                "The folder change waits for the user: the app asks them to allow it. Until then the jobs folder and the drawings inbox stay as they are; call get_settings later to see the answer.",
            );
        }
        if !notes.is_empty() {
            out["note"] = json!(notes.join(" "));
        }
        Ok(out)
    }

    /// Runs a tool; a panic in the geometry kernel becomes a tool error
    /// instead of ending the server for the rest of the session.
    pub fn call_tool(&mut self, name: &str, args: &Value) -> ToolResult {
        // A tool that calculates the whole job stops at the work bound
        // with "too complex" rather than keeping the server busy (HUNTIO-004).
        if worker::bounds_whole_call(name) {
            return tectora_core::work::calculate(|| self.call_tool_unguarded(name, args))?;
        }
        tectora_core::model::guarded(|| self.call_tool_unguarded(name, args))?
    }

    fn call_tool_unguarded(&mut self, name: &str, args: &Value) -> ToolResult {
        // Every write is shown to the user with its summary (AGENTS.md):
        // a blank one is refused before anything is done.
        // Only invisible characters count as blank too, and a summary is
        // one short line (ARCH-104, ARCH-105).
        if let Some(summary) = args.get("summary").and_then(Value::as_str) {
            tectora_core::edit::check_summary(summary)?;
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
            if let Some(m) = schema_mismatch(&schema, args, "") {
                return Err(format!(
                    "{m} (`{name}`'s input schema); nothing was changed"
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
                            // A roof the core refused has no area, not 0 m² (ERRU-005).
                            // Hand-drawn faces are a roof too (ERR-202); a
                            // job with nothing has no figure.
                            "roof_area_m2": matches!(
                                p.coverage(),
                                Some(RoofCoverage::Complete | RoofCoverage::Partial)
                            )
                            .then(|| round(p.roof_area, 2)),
                            "roof_state": p.coverage(),
                            "figures_pending": p.figures_pending, "parts": p.parts,
                            "parts_without_roof": p.parts_failed, "too_complex": p.too_complex,
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
                // A read: the stored result stays as it is. Only the app's
                // open recalculates an older engine's result (Jakub's
                // decision 2026-11-22, audit UX-702h).
                let (id, p) = self.load(args)?;
                self.live(
                    "reading",
                    Some(&p.name),
                    format!("Reading “{}”", p.name),
                    Some(&id),
                );
                // The roof figures in the work bound: a job too complex to
                // calculate is still read, walls and objects, so the AI can
                // repair it (ARCH-103); only its figures are left out.
                let calculated = tectora_core::work::calculate(|| {
                    let host = if p.dormers.is_empty() {
                        Vec::new()
                    } else {
                        tectora_core::edit::dormers::host_roof(&p)
                    };
                    let geos = tectora_core::openings::generate_project(&p);
                    (
                        host,
                        // Each object's distances from the eave and the sides
                        // of its face, as place_item takes them.
                        json!(tectora_core::edit::position::placements(&p)),
                        json!(tectora_core::snapshot::status_of(&geos, &p)),
                        quantities_of(&geos, &p),
                    )
                });
                let (host, placements, stored_result, quantities, refusal) = match calculated {
                    Ok((h, pl, st, q)) => (h, pl, st, q, None),
                    Err(r) => (Vec::new(), Value::Null, Value::Null, Value::Null, Some(r)),
                };
                let dormers: Vec<Value> = p
                    .dormers
                    .iter()
                    .enumerate()
                    .map(|(i, d)| more::describe_dormer(i, d, &host))
                    .collect();
                let mut v = json!({
                    "id": id,
                    "name": p.name,
                    "notes": p.notes,
                    "job": p.job,
                    "parts": p.parts.iter().map(describe_part).collect::<Vec<_>>(),
                    "chimneys": p.chimneys,
                    "windows": p.windows,
                    "roof_openings": p.roof_openings,
                    "dormers": dormers,
                    "placements": placements,
                    "manual_lines": p.manual_lines,
                    "manual_faces": p.manual_faces,
                    "flat_roof": p.flat_roof,
                    "materials": (1..=tectora_core::model::DACAD_MATERIALS).map(|m| json!({ "material": m, "name": p.material_label(m) })).collect::<Vec<_>>(),
                    "files": self.store.attachments(&id).iter().map(|a| a.name.clone()).collect::<Vec<_>>(),
                    "stored_result": stored_result,
                    "quantities": quantities,
                    // The recalculation the stored result is (its entry
                    // for restore_offer_result), also once the change log
                    // no longer holds the entry (audit 2026-11-23 HUNT-902).
                    "result_recalculation": self.store.recalculation(&id),
                });
                // What reading the job left out, as the app says it on
                // open (audit 2026-11-23 ARCH-902).
                let warnings = self.load_warnings(&id);
                if !warnings.is_empty() {
                    v["load_warnings"] = Value::Array(warnings);
                }
                match refusal {
                    None => {}
                    Some(tectora_core::work::Refusal::TooComplex) => {
                        v["too_complex"] = json!(true);
                        v["note"] = json!(format!(
                            "{}. The walls and objects are listed; the roof, its figures, placements and the stored result's status are not calculated",
                            tectora_core::work::TOO_COMPLEX
                        ));
                    }
                    Some(tectora_core::work::Refusal::Failed(d)) => {
                        v["calc_error"] = json!(d);
                        v["note"] = json!(
                            "the roof could not be calculated; the walls and objects are listed, its figures are not"
                        );
                    }
                }
                Ok(v)
            }
            "calc" => {
                let (id, p) = self.load(args)?;
                self.live(
                    "calculating",
                    Some(&p.name),
                    format!("Calculating “{}”", p.name),
                    Some(&id),
                );
                let geos = tectora_core::openings::generate_project(&p);
                let parts: Vec<Value> = p
                    .parts
                    .iter()
                    .zip(&geos)
                    .map(|(part, geo)| {
                        // Net of the core's negative lines, as the totals.
                        let q = tectora_core::quantities::part_quantities(geo);
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
                let mut out = json!({
                    "id": id,
                    "total": quantities_of(&geos, &p),
                    "parts": parts,
                    "engine_version": tectora_core::snapshot::ENGINE_VERSION,
                    "stored_result": tectora_core::snapshot::status_of(&geos, &p),
                });
                // Input the calculation left out (a building with a wall
                // under 1 cm, …): its figures are missing from the totals.
                let problems = p.problems();
                if !problems.is_empty() {
                    out["problems"] = serde_json::to_value(&problems).unwrap_or(Value::Null);
                }
                // The one rule of the CLI, `calc` and the library card:
                // hand-drawn faces are a roof (audit 2026-11-21 ERR-202).
                let failed = tectora_core::quantities::parts_without_roof(&geos, &p);
                out["roof_state"] = json!(tectora_core::quantities::RoofCoverage::of(&p, failed));
                Ok(out)
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
                // The pitch is checked with the other wall values below, with
                // its real lower limit (HUNT-108).
                if length <= 0.0 || width <= 0.0 {
                    return Err("length and width must be positive".into());
                }
                let eave = arg_f64(args, "eave_height")?.unwrap_or(preset.eave_height);
                let overhang = arg_f64(args, "overhang")?.unwrap_or(preset.overhang);
                // One check for every creating command (ARCH-007).
                tectora_core::edit::check_new_wall(pitch, eave, overhang)?;
                // The standard-roof form's bounds (ARCH-001): 4 mm typed for
                // 4 m gave a job without a roof.
                tectora_core::edit::check_standard_size(length, width)?;
                tectora_core::edit::check_job_name(arg_str(args, "name")?)?;
                let empty = Project::new(arg_str(args, "name")?);
                let mut p = empty.clone();
                p.parts
                    .push(standard_roof(kind, length, width, pitch, eave, overhang));
                crate::more::refuse_new_problems(&empty, &p)?;
                self.commit(None, None, &p, arg_str(args, "summary")?)
            }
            "create_project" => {
                tectora_core::edit::check_job_name(arg_str(args, "name")?)?;
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
                if let Some(job) = args.get("job").filter(|j| !j.is_null()) {
                    for (key, slot) in [
                        ("number", &mut p.job.number),
                        ("customer", &mut p.job.customer),
                        ("address", &mut p.job.address),
                    ] {
                        if let Some(s) = text_arg(job, key).map_err(|m| format!("job.{m}"))? {
                            *slot = s;
                        }
                    }
                }
                if let Some(n) = text_arg(args, "notes")? {
                    p.notes = n;
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
                    let preset = Settings::load().roof_defaults.pitch_deg;
                    for i in edge_indices(c, part.edges.len())? {
                        let e = &part.edges[i];
                        let keep = all && ((e.is_gable() && !to_eave) || e.direction_rad.is_some());
                        let fallback = tectora_core::edit::eave_pitch(&part.edges, i, None, preset);
                        apply_edge_fields_with(
                            &mut part.edges[i],
                            if keep { &no_pitch } else { c },
                            Some(fallback),
                        )
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
                    // A new outline keeps the building's height (review ARCH-001).
                    height: old.height,
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
                // A present key of the wrong type is refused, not ignored
                // (HUNTIO-012).
                let text = |key: &str| text_arg(args, key);
                if let Some(s) = text("name")? {
                    tectora_core::edit::check_job_name(&s)?;
                    p.name = s;
                }
                if let Some(s) = text("notes")? {
                    p.notes = s;
                }
                for (key, slot) in [
                    ("number", &mut p.job.number),
                    ("customer", &mut p.job.customer),
                    ("address", &mut p.job.address),
                ] {
                    if let Some(s) = text(key)? {
                        *slot = s;
                    }
                }
                if let Some(s) = text("status")? {
                    let s = s.as_str();
                    let status = parse_status(s)?;
                    // Leaving the draft stores the result as the offer's
                    // figures: refused for a roof that cannot be calculated,
                    // with the app's check (JobPanel `takeSnapshot`), also
                    // when the job keeps a result stored for an earlier
                    // offer, which is not of this roof (HUNT-504, ERR-303).
                    // The kept result stays the offer's.
                    if p.job.status == JobStatus::Draft && status != JobStatus::Draft {
                        let fresh =
                            tectora_core::snapshot::take_checked(&p).map_err(|e| e.to_string())?;
                        p.result_snapshot.get_or_insert(fresh);
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
                // A chimney or opening without plan area (width 0 for an
                // unknown size) would take the whole roof face away.
                let flat = |list: &[Chimney], what: &str| {
                    list.iter().position(|c| c.corners.len() >= 3 && !c.has_area()).map(|i| {
                        format!(
                            "{what} index {i} has no plan area: give it its real width and depth (at least 1 cm each), or leave it out and tell the user its size is unknown"
                        )
                    })
                };
                if let Some(e) =
                    flat(&p.chimneys, "chimney").or(flat(&p.roof_openings, "roof opening"))
                {
                    return Err(e);
                }
                // A window of size 0 would block later edits (HUNTC-005).
                crate::more::refuse_new_problems(&before, &p)?;
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
                let expect: Option<u64> = match args.get("ts_ms") {
                    None | Some(Value::Null) => None,
                    Some(v) => Some(
                        v.as_u64()
                            .ok_or("`ts_ms` must be a whole number (from get_history)")?,
                    ),
                };
                let r = self
                    .store
                    .revert_last_reported(&id, &self.client, Some(summary), expect)
                    .map_err(coded)?;
                let p = &r.project;
                self.live_follow(
                    "reverted",
                    Some(&p.name),
                    format!("Reverted the latest change: {summary}"),
                    &id,
                );
                let mut out = json!({ "id": id, "reverted": true, "quantities": quantities(p) });
                replaced_note(&mut out, r.repaired);
                Ok(out)
            }
            _ => self
                .call_more(name, args)
                .or_else(|| self.call_drawing_isolated(name, args))
                .unwrap_or_else(|| Err(format!("unknown tool `{name}`"))),
        }
    }

    /// Handles one JSON-RPC message; returns the response for requests.
    pub fn handle(&mut self, msg: &Value) -> Option<Value> {
        // A message that is not an object, or a request without a method,
        // is an invalid request (JSON-RPC 2.0, -32600): it is answered, with
        // id null when it has none, instead of left without an answer
        // (HUNTIO-007).
        let invalid = |id: Value, why: &str| {
            Some(json!({
                "jsonrpc": "2.0", "id": id,
                "error": { "code": -32600, "message": format!("invalid request: {why}") }
            }))
        };
        if !msg.is_object() {
            return invalid(Value::Null, "a message must be a JSON object");
        }
        if msg.get("method").is_none()
            && (msg.get("result").is_some() || msg.get("error").is_some())
        {
            // A response from the client: this server sends no requests.
            return None;
        }
        if !msg.get("method").is_some_and(Value::is_string) {
            return invalid(
                msg.get("id").cloned().unwrap_or(Value::Null),
                "`method` is missing or not a string",
            );
        }
        let id = msg.get("id").cloned();
        let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
        let params = msg.get("params").cloned().unwrap_or(json!({}));
        let result: Result<Value, (i64, String)> = match method {
            "initialize" => {
                if let Some(name) = params.pointer("/clientInfo/name").and_then(Value::as_str) {
                    self.client = client_actor(name);
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

impl Server {
    /// Answers one line read from the client. A line that is not UTF-8 or
    /// not JSON gets a parse error instead of ending the session (audit
    /// 2026-11-15 ERR-002), and a JSON-RPC batch gets one answer per
    /// request instead of none, which left the client waiting (ERR-003).
    pub fn respond_line(&mut self, line: &[u8]) -> Option<Value> {
        let parse_error = |e: String| {
            Some(json!({
                "jsonrpc": "2.0", "id": null,
                "error": { "code": -32700, "message": format!("parse error: {e}") }
            }))
        };
        let text = match std::str::from_utf8(line) {
            Ok(t) => t,
            Err(e) => return parse_error(format!("the message is not UTF-8 ({e})")),
        };
        if text.trim().is_empty() {
            return None;
        }
        match serde_json::from_str::<Value>(text) {
            Ok(Value::Array(batch)) if batch.is_empty() => Some(json!({
                "jsonrpc": "2.0", "id": null,
                "error": { "code": -32600, "message": "invalid request: empty batch" }
            })),
            Ok(Value::Array(batch)) => {
                let answers: Vec<Value> = batch.iter().filter_map(|m| self.handle(m)).collect();
                (!answers.is_empty()).then_some(Value::Array(answers))
            }
            Ok(msg) => self.handle(&msg),
            Err(e) => parse_error(e.to_string()),
        }
    }
}

/// Serves MCP over stdin/stdout (one JSON-RPC message per line) until EOF.
/// Runs as a calculation or drawing worker when the environment says so
/// (see `worker`): answers its one call on stdin and returns true. A worker
/// opens the library it is told, so the entry points call this before they
/// open any library of their own (ARCH-001): an unusable default folder
/// must not break `--dir`.
pub fn run_worker_if_requested() -> bool {
    worker::run_if_worker()
}

/// The actor name a client's writes are logged under. The name comes from
/// the client; one that reads as the user ("user", any case) would make its
/// saves count as the user's own and skip the review, so it is given a
/// different name; an empty name is "AI", and the length is capped
/// (HUNTIO-003).
fn client_actor(name: &str) -> String {
    // The text rules of a received change log's actor (ARCH-306 item 1):
    // no control, invisible or line-breaking characters, and a name of
    // blank-looking characters only is no name.
    let clean = tectora_core::edit::clean_line(name, tectora_core::edit::MAX_ACTOR_CHARS)
        .trim()
        .to_string();
    if !tectora_core::edit::has_visible(&clean) {
        "AI".into()
    } else if tectora_core::store::is_reserved_actor(&clean) {
        // One list of the names only Tectora writes (audit 2026-11-23
        // ARCH-901: `tectora (received)` was missing).
        format!("AI ({clean})")
    } else {
        clean
    }
}

pub fn run_stdio(store: Store) -> io::Result<()> {
    if worker::run_if_worker() {
        return Ok(());
    }
    let mut server = Server::new(store);
    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    let mut line = Vec::new();
    loop {
        line.clear();
        if stdin.read_until(b'\n', &mut line)? == 0 {
            return Ok(());
        }
        let text = line.strip_suffix(b"\n").unwrap_or(&line);
        let text = text.strip_suffix(b"\r").unwrap_or(text);
        if let Some(r) = server.respond_line(text) {
            writeln!(stdout, "{r}")?;
            stdout.flush()?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_the_guide_and_the_skill_name_exists() {
        // Round 2 HUNT-102: the guide sent the AI to `delete_part` and
        // `undo`, which do not exist. A backticked name with a tool's verb
        // must be a tool.
        let names: Vec<String> = tools()
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|t| t["name"].as_str().map(String::from))
            .collect();
        let verbs = [
            "get_",
            "list_",
            "delete_",
            "create_",
            "update_",
            "add_",
            "remove_",
            "revert_",
            "export_",
            "import_",
            "show_",
            "place_",
            "transform_",
            "duplicate_",
            "restore_",
            "view_",
            "measure_",
            "trace_",
            "set_",
        ];
        for (file, text) in [
            ("guide.md", GUIDE),
            (
                "tectora-roof/SKILL.md",
                include_str!("../../../integrations/skills/tectora-roof/SKILL.md"),
            ),
        ] {
            for (i, quoted) in text.split('`').enumerate() {
                let word = quoted.trim();
                let is_name = i % 2 == 1
                    && !word.is_empty()
                    && word.chars().all(|c| c.is_ascii_lowercase() || c == '_');
                if is_name && (word == "undo" || verbs.iter().any(|v| word.starts_with(v))) {
                    assert!(
                        names.iter().any(|n| n == word),
                        "{file}: `{word}` is not a tool"
                    );
                }
            }
        }
    }

    #[test]
    fn a_client_named_user_is_still_reviewed() {
        // HUNTIO-003: its saves were logged as the user's own.
        // Audit 2026-11-23 ARCH-901: and like a received engine entry.
        for name in [
            "user",
            "User",
            "  USER ",
            "",
            "tectora",
            "Tectora",
            "tectora (received)",
            " Tectora (Received) ",
        ] {
            let mut s = server();
            s.handle(&json!({
                "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": { "protocolVersion": "2025-06-18", "clientInfo": { "name": name } }
            }));
            let r = s
                .call_tool(
                    "create_standard_roof",
                    &json!({ "name": "A", "type": "gable", "length": 10, "width": 8, "pitch_deg": 40, "summary": "x" }),
                )
                .unwrap();
            let id = r["id"].as_str().unwrap();
            let log = s.store.activity(id);
            assert!(!log.is_empty());
            assert!(
                log.iter()
                    .all(|e| !tectora_core::store::is_reserved_actor(&e.actor)
                        && e.actor != tectora_core::store::USER_ACTOR
                        && e.actor != tectora_core::store::ENGINE_ACTOR
                        && !e.actor.is_empty()),
                "{name:?}: {:?}",
                log.iter().map(|e| &e.actor).collect::<Vec<_>>()
            );
            let _ = std::fs::remove_dir_all(&s.store.dir);
        }
        assert_eq!(client_actor("Claude Desktop"), "Claude Desktop");
        // Hunt R2 BUG-1: a client named like the engine's recalculation
        // would skip the AI review.
        for name in ["tectora", "Tectora", " TECTORA "] {
            assert_ne!(
                client_actor(name).to_lowercase(),
                tectora_core::store::ENGINE_ACTOR,
                "{name:?}"
            );
        }
        assert_eq!(client_actor(&"x".repeat(500)).chars().count(), 60);
    }

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
    fn a_bad_line_or_a_batch_gets_an_answer_and_the_session_goes_on() {
        let mut s = server();
        let r = s.respond_line(b"\xff\xfe").unwrap();
        assert_eq!(r["error"]["code"], -32700);
        let r = s.respond_line(b"{ not json").unwrap();
        assert_eq!(r["error"]["code"], -32700);
        assert!(s.respond_line(b"  ").is_none());
        let r = s
            .respond_line(br#"[{"jsonrpc":"2.0","id":7,"method":"ping"},{"jsonrpc":"2.0","method":"notifications/initialized"},{"jsonrpc":"2.0","id":8,"method":"ping"}]"#)
            .unwrap();
        let ids: Vec<_> = r
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["id"].clone())
            .collect();
        assert_eq!(ids, [json!(7), json!(8)]);
        assert_eq!(s.respond_line(b"[]").unwrap()["error"]["code"], -32600);
        let r = s
            .respond_line(br#"{"jsonrpc":"2.0","id":9,"method":"ping"}"#)
            .unwrap();
        assert_eq!(r["id"], 9);
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
        let shown = Settings::load().folder_request.unwrap().changes;
        Settings::update(|s| s.answer_folder_request(&shown, true)).unwrap();
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
    fn an_ai_settings_change_is_recorded_for_the_user_to_revert() {
        // UX-003: update_settings changed the roof defaults with no record,
        // no before -> after and no way back.
        let _settings = SETTINGS_FILE.lock().unwrap_or_else(|e| e.into_inner());
        let base =
            std::env::temp_dir().join(format!("tectora-mcp-aichange-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        // Tests touching the settings file hold SETTINGS_FILE, so the env var is safe here.
        unsafe { std::env::set_var("TECTORA_CONFIG_DIR", base.join("cfg")) };
        let mut s = server();
        let r = s
            .call_tool(
                "update_settings",
                &json!({ "roof_defaults": { "pitch_deg": 25, "overhang": 0.9 }, "summary": "Company defaults from the drawings" }),
            )
            .unwrap();
        assert!(r["note"].as_str().unwrap().contains("Revert"), "{r}");
        let saved = Settings::load();
        let change = saved.open_ai_change().cloned().expect("recorded");
        assert_eq!(change.summary, "Company defaults from the drawings");
        assert_eq!(change.before["roof_defaults.pitch_deg"], 40.0);
        assert_eq!(change.after["roof_defaults.pitch_deg"], 25.0);
        assert_eq!(change.before["roof_defaults.overhang"], 0.5);
        assert_eq!(change.after["roof_defaults.overhang"], 0.9);
        assert_eq!(
            r["settings"]["ai_changes"][0]["id"],
            json!(change.id),
            "the AI sees the record too"
        );
        // The user reverts it in the app.
        Settings::update(|s| s.answer_ai_change(change.id, true)).unwrap();
        let r = s.call_tool("get_settings", &json!({})).unwrap();
        assert_eq!(r["settings"]["roof_defaults"]["pitch_deg"], 40.0);
        assert_eq!(r["settings"]["roof_defaults"]["overhang"], 0.5);
        assert_eq!(r["settings"]["ai_changes"][0]["state"], "reverted");
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

    /// Audit 2026-11-21 HUNT-106: a write answers with the roof's warnings
    /// as calc gives them, and an outline drawn round twice is a problem.
    #[test]
    fn a_write_answers_with_the_roof_warnings_and_a_doubled_outline_is_refused() {
        let mut s = server();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/check/C1_hips_outside.json");
        let p: Project = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let id = s.store.save(None, &p).unwrap();
        let r = s
            .call_tool(
                "update_job",
                &json!({ "id": id, "number": "2026-7", "summary": "Job number from the order" }),
            )
            .unwrap();
        let w = r["warnings"].as_array().expect("warnings in the answer");
        assert_eq!(w[0]["kind"], "outside_outline", "{r}");
        assert_eq!(w[0]["part"], 0);

        let r = s
            .call_tool(
                "create_project",
                &json!({ "name": "twice", "summary": "Footprint from the drawing",
                         "parts": [{ "points": [[0, 0], [12, 0], [12, 8], [0, 8], [0, 0], [12, 0], [12, 8], [0, 8]],
                                     "defaults": { "pitch_deg": 35, "eave_height": 3, "overhang": 0.5 } }] }),
            )
            .unwrap();
        let problems = r["roof_trouble"]["problems"]
            .as_array()
            .expect("roof_trouble");
        assert!(
            problems
                .iter()
                .any(|q| q["code"] == "walls_overlap" && q["edge"] == 4),
            "{r}"
        );
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
    fn every_field_in_defaults_reaches_every_edge() {
        // Drawing check B1: `defaults` kept only pitch, eave height and
        // overhang, so a mansard came out as a plain hip roof and
        // `gable: true` as eaves.
        let mut s = server();
        let r = s
            .call_tool(
                "create_project",
                &json!({
                    "name": "mansard", "summary": "Footprint",
                    "parts": [{
                        "points": [[0, 0], [12, 0], [12, 8], [0, 8]],
                        "defaults": {
                            "pitch_deg": 60, "eave_height": 3, "overhang": 0.3,
                            "breaks": [{ "height": 5, "pitch_deg": 25 }],
                            "material": 2, "priority": 1
                        }
                    }]
                }),
            )
            .unwrap();
        let id = r["id"].as_str().unwrap().to_string();
        let p = s.store.load(&id).unwrap();
        for e in &p.parts[0].edges {
            assert_eq!(e.breaks.len(), 1, "{e:?}");
            assert_eq!((e.material, e.priority), (2, 1), "{e:?}");
        }
        let r = s
            .call_tool(
                "add_part",
                &json!({
                    "id": id, "summary": "Garage",
                    "part": {
                        "points": [[20, 0], [26, 0], [26, 6], [20, 6]],
                        "defaults": { "gable": true }
                    }
                }),
            )
            .unwrap();
        assert!(r.get("error").is_none(), "{r}");
        let p = s.store.load(&id).unwrap();
        assert!(p.parts[1].edges.iter().all(|e| e.is_gable()));
        let _ = std::fs::remove_dir_all(&s.store.dir);
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
    fn list_projects_gives_no_area_for_a_roof_the_core_refused() {
        // ERRU-005: a refused roof was listed as 0 m², like an empty roof.
        let mut s = server();
        let walls: Vec<Value> = [(0.0, 0.0), (12.0, 0.0), (12.0, 8.0), (0.0, 8.0)]
            .iter()
            .map(|&(x, y)| json!({ "start": { "x": x, "y": y }, "pitch_deg": 35.0, "eave_height": 3.0, "overhang": 0.5 }))
            .collect();
        let corners: Vec<Value> = [
            (4.0, 3.0),
            (6.0, 3.0),
            (6.0, 4.0),
            (5.0, 4.0),
            (5.0, 5.0),
            (4.0, 5.0),
        ]
        .iter()
        .map(|&(x, y)| json!({ "x": x, "y": y }))
        .collect();
        let p = json!({
            "schema_version": 2, "name": "refused",
            "parts": [{ "name": "A", "edges": walls }],
            "chimneys": [{ "corners": corners, "height": 9.0 }],
        });
        let id = s
            .store
            .save(None, &Project::from_json(&p.to_string()).unwrap())
            .unwrap();
        let r = s.call_tool("list_projects", &json!({})).unwrap();
        let card = r["projects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == id.as_str())
            .unwrap()
            .clone();
        assert!(card["roof_area_m2"].is_null(), "{card}");
        assert_eq!(card["parts_without_roof"], 1, "{card}");
        let _ = std::fs::remove_dir_all(&s.store.dir);
    }

    #[test]
    fn calc_and_the_card_count_hand_drawn_faces_as_a_roof() {
        // Audit 2026-11-21 ERR-202: a broken building plus a hand-drawn
        // face was listed without an area while `calc` gave one; HUNT-203:
        // the area per pitch left the face out.
        let mut s = server();
        let p = json!({
            "schema_version": 3, "name": "faces",
            "parts": [{ "name": "A", "edges": [
                { "start": { "x": 0, "y": 0 }, "pitch_deg": 35, "eave_height": 3, "overhang": 0.5 },
                { "start": { "x": 5, "y": 0 }, "pitch_deg": 35, "eave_height": 3, "overhang": 0.5 }
            ] }],
            "manual_faces": [{ "points": [{"x": 0, "y": 0}, {"x": 5, "y": 0}, {"x": 5, "y": 4}, {"x": 0, "y": 4}], "pitch_deg": 30 }],
        });
        let id = s
            .store
            .save(None, &Project::from_json(&p.to_string()).unwrap())
            .unwrap();
        let empty = s
            .store
            .save(
                None,
                &Project::from_json(r#"{"name": "nothing", "parts": []}"#).unwrap(),
            )
            .unwrap();
        let r = s.call_tool("list_projects", &json!({})).unwrap();
        let card = |id: &str| {
            r["projects"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["id"] == id)
                .unwrap()
                .clone()
        };
        let c = card(&id);
        assert_eq!(c["roof_state"], "partial", "{c}");
        assert!(c["roof_area_m2"].as_f64().unwrap() > 23.0, "{c}");
        let c = card(&empty);
        assert_eq!(c["roof_state"], "no_building", "{c}");
        assert!(c["roof_area_m2"].is_null(), "{c}");
        let calc = s.call_tool("calc", &json!({ "id": id })).unwrap();
        assert_eq!(calc["roof_state"], "partial", "{calc}");
        let total = &calc["total"];
        let sum: f64 = total["area_by_pitch_m2"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x["area"].as_f64().unwrap())
            .sum();
        assert!(
            (sum - total["roof_area_m2"].as_f64().unwrap()).abs() < 0.02,
            "{calc}"
        );
        let calc = s.call_tool("calc", &json!({ "id": empty })).unwrap();
        assert_eq!(calc["roof_state"], "no_building", "{calc}");
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
            (
                part(
                    json!({ "pitch_deg": 40, "eave_height": 3 }),
                    json!([{ "gable": true, "half_hip": { "height": 5, "pitch_deg": 0 }, "breaks": [{ "height": 4, "pitch_deg": 30 }] }, {}, {}, {}]),
                ),
                "pitch_range",
            ),
            (
                part(
                    json!({ "pitch_deg": 40, "eave_height": 3 }),
                    json!([{ "gable": true, "half_hip": { "height": 2, "pitch_deg": 50 } }, {}, {}, {}]),
                ),
                "half_hip_below_eave",
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

    /// Jakub's T: house 15 × 5, wing 5 × 2.5 between two gables, the
    /// wing's front eave at `front`; `garage` adds a building 40 m away.
    fn t_house(s: &mut Server, front: f64, garage: bool) -> String {
        let g = json!({ "gable": true });
        let mut parts = vec![json!({
            "points": [[5, 0], [10, 0], [10, 2.5], [15, 2.5], [15, 7.5], [0, 7.5], [0, 2.5], [5, 2.5]],
            "defaults": { "pitch_deg": 40, "eave_height": 5, "overhang": 0.5 },
            "edges": [{ "eave_height": front }, g, {}, g, {}, g, {}, g]
        })];
        if garage {
            parts.push(json!({
                "points": [[40, 0], [46, 0], [46, 5], [40, 5]],
                "defaults": { "pitch_deg": 30, "eave_height": 3, "overhang": 0.5 }
            }));
        }
        let r = s
            .call_tool(
                "create_project",
                &json!({ "name": "T", "summary": "Footprint", "parts": parts }),
            )
            .unwrap();
        r["id"].as_str().unwrap().to_string()
    }

    #[test]
    fn calc_names_merged_planes_and_wing_joined_with_edges() {
        // TEST-007: the notices of a roof that is not the standard result.
        let mut s = server();
        for (front, kind, edges, says) in [
            (2.9, "merged_planes", json!([0, 2]), "0.2 cm apart"),
            (2.0, "wing_joined", json!([0, 1, 7]), "edges [0, 1, 7]"),
        ] {
            for garage in [false, true] {
                let id = t_house(&mut s, front, garage);
                let r = s.call_tool("calc", &json!({ "id": id })).unwrap();
                let part = &r["parts"][0];
                assert!(part.get("error").is_none(), "{front} {garage}: {r}");
                let w = &part["warnings"];
                assert_eq!(w.as_array().map(Vec::len), Some(1), "{front}: {w}");
                assert_eq!(w[0]["kind"], kind, "{front}: {w}");
                assert_eq!(w[0]["edges"], edges, "{front}: {w}");
                let msg = w[0]["message"].as_str().unwrap();
                assert!(msg.contains(says), "{msg}");
                assert!(msg.contains("0-based"), "{msg}");
                if garage {
                    assert!(r["parts"][1].get("error").is_none(), "{r}");
                    assert!(r["parts"][1].get("warnings").is_none(), "{r}");
                }
                assert!(!r.to_string().contains("DACAD"), "{r}");
            }
        }
        // A hip roof with one corner moved: Tectora's own engine, said.
        let r = s
            .call_tool(
                "create_project",
                &json!({
                    "name": "quad", "summary": "Footprint",
                    "parts": [{ "points": [[0, 0.2], [12, 0], [12, 8], [0, 8]],
                        "defaults": { "pitch_deg": 35, "eave_height": 3, "overhang": 0.5 } }]
                }),
            )
            .unwrap();
        let id = r["id"].as_str().unwrap().to_string();
        let r = s.call_tool("calc", &json!({ "id": id })).unwrap();
        let w = &r["parts"][0]["warnings"];
        assert_eq!(w[0]["kind"], "not_calculated_by_core", "{r}");
        assert!(
            w[0]["message"].as_str().unwrap().contains("self-check"),
            "{w}"
        );
        assert!(r["total"]["roof_area_m2"].as_f64().unwrap() > 100.0, "{r}");
        let _ = std::fs::remove_dir_all(&s.store.dir);
    }

    #[test]
    fn calc_error_keeps_every_sentence() {
        // ERR-005: the parallel-walls sentence and the failing-buildings
        // sentence add up; ARCH-006: no walls for an object's error.
        use tectora_core::roof::{CalcError, CalcErrorKind};
        let e = |kind, edges: &[usize], parts: &[usize]| CalcError {
            kind,
            code: None,
            message_id: None,
            message: String::new(),
            parts: parts.to_vec(),
            edges: edges.to_vec(),
        };
        let v = calc_error(&e(CalcErrorKind::Cut, &[0, 2], &[1]));
        let msg = v["message"].as_str().unwrap();
        assert!(msg.starts_with("two roof planes never meet"), "{msg}");
        assert!(
            msg.contains("edges 0 and 2 (0-based) are parallel"),
            "{msg}"
        );
        assert!(msg.contains("failing_parts"), "{msg}");
        assert_eq!(v["edges"], json!([0, 2]));
        assert_eq!(v["failing_parts"], json!([1]));
        let v = calc_error(&e(CalcErrorKind::Cut, &[], &[]));
        assert!(v.get("edges").is_none() && v.get("failing_parts").is_none());
        assert_eq!(
            v["message"],
            "two roof planes never meet, so no roof is calculated; check pitches and short walls"
        );

        // A shallow wing 3 cm under the house's plane beside a garage:
        // the error of building 0 names its walls and says it fails alone.
        let mut s = server();
        let g = json!({ "gable": true, "overhang": 0 });
        let front = 12.0 - 25f64.to_radians().tan() * 0.3 - 0.03;
        let r = s
            .call_tool(
                "create_project",
                &json!({
                    "name": "shallow", "summary": "Footprint",
                    "parts": [{
                        "points": [[5, 0], [10, 0], [10, 0.6], [15, 0.6], [15, 5.6], [0, 5.6], [0, 0.6], [5, 0.6]],
                        "defaults": { "pitch_deg": 25, "eave_height": 12, "overhang": 0.5 },
                        "edges": [{ "eave_height": front, "overhang": 0.2 }, g, {}, g, {}, g, {}, g]
                    }, {
                        "points": [[40, 0], [46, 0], [46, 5], [40, 5]],
                        "defaults": { "pitch_deg": 30, "eave_height": 3, "overhang": 0.5 }
                    }]
                }),
            )
            .unwrap();
        let id = r["id"].as_str().unwrap().to_string();
        let r = s.call_tool("calc", &json!({ "id": id })).unwrap();
        let err = &r["parts"][0]["error"];
        assert_eq!(err["edges"], json!([0, 2]), "{r}");
        assert_eq!(err["failing_parts"], json!([0]), "{r}");
        let msg = err["message"].as_str().unwrap();
        assert!(msg.contains("edges 0 and 2"), "{msg}");
        assert!(msg.contains("fail on their own"), "{msg}");
        // The garage keeps its roof.
        assert!(r["parts"][1].get("error").is_none(), "{r}");
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
    fn set_openings_refuses_a_chimney_or_opening_without_area() {
        // ERR-002: width 0 was saved and the whole roof came out as 0 m².
        let mut s = server();
        let id = gable_job(&mut s, 40.0);
        let err = s
            .call_tool(
                "set_openings",
                &json!({ "id": id, "summary": "Chimney", "chimneys": [{ "x": 2, "y": 2, "width": 0, "depth": 0, "height": 6 }] }),
            )
            .unwrap_err();
        assert!(err.contains("chimney index 0 has no plan area"), "{err}");
        let line = json!([[2, 2], [3, 2], [4, 2]]);
        let err = s
            .call_tool(
                "set_openings",
                &json!({ "id": id, "summary": "Opening", "roof_openings": [{ "corners": line }] }),
            )
            .unwrap_err();
        assert!(
            err.contains("roof opening index 0 has no plan area"),
            "{err}"
        );
        let p = s.store.load(&id).unwrap();
        assert!(p.chimneys.is_empty() && p.roof_openings.is_empty());
        s.call_tool(
            "set_openings",
            &json!({ "id": id, "summary": "Chimney", "chimneys": [{ "x": 2, "y": 2, "width": 0.4, "depth": 0.4, "height": 6 }] }),
        )
        .unwrap();
        assert_eq!(s.store.load(&id).unwrap().chimneys.len(), 1);
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

    /// Audit 2026-11-23 ARCH-1504 / TEST-1505: the `calc` description
    /// lists every warning kind of the core, the notices after "these
    /// notices" and the rest before, as `WarningKind::is_notice` splits
    /// them.
    #[test]
    fn calc_description_splits_warnings_and_notices_as_the_core_does() {
        let check = include_str!("../../tectora-core/src/check.rs");
        let body = |from: &str, to: &str| {
            let at = check.find(from).unwrap() + from.len();
            &check[at..at + check[at..].find(to).unwrap()]
        };
        let snake = |s: &str| {
            let mut out = String::new();
            for (i, c) in s.chars().enumerate() {
                if c.is_ascii_uppercase() {
                    if i > 0 {
                        out.push('_');
                    }
                    out.push(c.to_ascii_lowercase());
                } else {
                    out.push(c);
                }
            }
            out
        };
        let kinds: Vec<String> = body("pub enum WarningKind {", "\n}")
            .lines()
            .map(str::trim)
            .filter(|l| l.chars().next().is_some_and(|c| c.is_ascii_uppercase()))
            .map(|l| snake(l.trim_end_matches(',')))
            .collect();
        let notices: Vec<String> = body("pub fn is_notice(self) -> bool {", "\n    }")
            .split("WarningKind::")
            .skip(1)
            .map(|w| {
                snake(
                    w.split(|c: char| !c.is_ascii_alphanumeric())
                        .next()
                        .unwrap(),
                )
            })
            .collect();
        assert!(
            kinds.len() > 10 && notices.len() >= 4,
            "{kinds:?} {notices:?}"
        );
        let tools = tools();
        let calc = tools
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == "calc")
            .unwrap()["description"]
            .as_str()
            .unwrap();
        let (warnings, notes) = calc.split_once("these notices").unwrap();
        for k in &kinds {
            let named = format!("`{k}`");
            let (in_warnings, in_notes) = (warnings.contains(&named), notes.contains(&named));
            assert!(in_warnings != in_notes, "{k}: named once, in one list");
            assert_eq!(in_notes, notices.contains(k), "{k}");
        }
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

    /// Audit 2026-11-23 HUNT-1502: a window on no roof face beside a
    /// building that calculated is named while another building of the
    /// job has no roof; one over the failing building is not.
    #[test]
    fn set_openings_names_a_window_off_the_roof_while_another_building_fails() {
        let mut s = server();
        let id = gable_job(&mut s, 40.0);
        let mut p = s.store.load(&id).unwrap();
        let mut b = p.parts[0].clone();
        b.name = "B".into();
        for (e, (x, y)) in
            b.edges
                .iter_mut()
                .zip([(40.0, 0.0), (46.0, 0.0), (46.0, 6.0), (40.0, 6.0)])
        {
            e.start = Point2::new(x, y);
            e.pitch_deg = GABLE_PITCH_DEG;
        }
        p.parts.push(b);
        s.store.save(Some(id.clone()), &p).unwrap();
        let r = s
            .call_tool(
                "set_openings",
                &json!({ "id": id, "summary": "Windows", "windows": [
                    { "x": 6, "y": -5, "width": 0.78, "height": 1.18 },
                    { "x": 43, "y": 3, "width": 0.78, "height": 1.18 }
                ] }),
            )
            .unwrap();
        assert!(r.to_string().contains("window_off_roof"), "{r}");
        let r = s.call_tool("calc", &json!({ "id": id })).unwrap();
        assert!(r["parts"][1]["error"].is_object(), "B has no roof: {r}");
        let warned: Vec<&Value> = r["parts"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|p| p["warnings"].as_array().into_iter().flatten())
            .filter(|w| w["kind"] == "window_off_roof")
            .collect();
        assert_eq!(warned.len(), 1, "{r}");
        assert_eq!(warned[0]["item"], 0, "{r}");
        assert!(
            r["parts"][0]["warnings"]
                .as_array()
                .is_some_and(|w| w.iter().any(|w| w["kind"] == "window_off_roof")),
            "{r}"
        );
        let _ = std::fs::remove_dir_all(&s.store.dir);
    }

    /// ARCH-1503 / ERR-1502: the revert codes an AI acts on keep their
    /// code in front, and a revert over an unreadable file says so.
    #[test]
    fn a_damaged_out_of_limits_revert_keeps_its_code_and_a_replaced_file_is_named() {
        let e = coded(StoreError::RevertOutOfLimitsDamaged {
            id: "j".into(),
            detail: "x".into(),
        });
        assert!(e.starts_with("revert_out_of_limits_damaged: "), "{e}");
        assert!(e.contains("cannot be read"), "{e}");
        let mut out = json!({ "reverted": true });
        replaced_note(&mut out, false);
        assert_eq!(out, json!({ "reverted": true }));
        replaced_note(&mut out, true);
        assert_eq!(out["replaced_unreadable_file"], true);
        assert!(out["note"].as_str().unwrap().contains("replaced"), "{out}");
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
    fn a_file_outside_the_job_folders_waits_for_the_user() {
        // ARCH-002: drawing tools, attach_file and import_job took any path.
        let _settings = SETTINGS_FILE.lock().unwrap_or_else(|e| e.into_inner());
        let base = std::env::temp_dir().join(format!("tectora-mcp-file-{}", std::process::id()));
        unsafe { std::env::set_var("TECTORA_CONFIG_DIR", base.join("cfg")) };
        std::fs::create_dir_all(base.join("other")).unwrap();
        let outside = base.join("other").join("contract.pdf");
        std::fs::write(&outside, b"%PDF-1.4 not a drawing").unwrap();
        let missing = base.join("other").join("missing.pdf");
        let mut s = server();
        let id = gable_job(&mut s, 40.0);
        let words = |e: &str, p: &std::path::Path| e.replace(&p.display().to_string(), "<path>");
        let there = s
            .call_tool("drawing_info", &json!({ "path": outside }))
            .unwrap_err();
        assert!(there.contains("asks the user to allow it"), "{there}");
        let gone = s
            .call_tool("drawing_info", &json!({ "path": missing }))
            .unwrap_err();
        // No way to tell an existing file from a missing one.
        assert_eq!(words(&there, &outside), words(&gone, &missing));
        for (tool, args) in [
            ("drawing_lines", json!({ "path": outside })),
            (
                "attach_file",
                json!({ "id": id, "path": outside, "summary": "Drawing" }),
            ),
            ("import_job", json!({ "path": outside, "summary": "Job" })),
        ] {
            let e = s.call_tool(tool, &args).unwrap_err();
            assert!(e.contains("asks the user to allow it"), "{tool}: {e}");
        }
        assert!(s.store.attachments(&id).is_empty());
        // The request names the file and who asked; the user says yes.
        let asked = Settings::load();
        let req = asked.file_request.clone().unwrap();
        assert_eq!(req.purpose, "import");
        assert!(req.path.ends_with("other/contract.pdf"), "{:?}", req.path);
        Settings::update(|s| Ok(s.answer_file_request(&req.path, true))).unwrap();
        s.call_tool(
            "attach_file",
            &json!({ "id": id, "path": outside, "summary": "Drawing" }),
        )
        .unwrap();
        assert_eq!(s.store.attachments(&id).len(), 1);
        // Only that file: its neighbour still waits.
        assert!(
            s.call_tool("drawing_info", &json!({ "path": missing }))
                .is_err()
        );
        // An AI client cannot allow a file for itself.
        assert!(
            s.call_tool(
                "update_settings",
                &json!({ "allowed_files": [missing], "summary": "s" })
            )
            .is_err()
        );
        let _ = std::fs::remove_dir_all(&s.store.dir);
        let _ = std::fs::remove_dir_all(&base);
    }

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

    #[test]
    fn a_calculation_that_overruns_does_not_block_the_write() {
        // ERR-011: a write on a job that needs over a minute waited for it.
        let mut s = server();
        s.commit_timeout = std::time::Duration::from_millis(50);
        let slow = || std::thread::sleep(std::time::Duration::from_millis(400));
        assert_eq!(s.within(slow), None, "answers after the timeout");
        assert_eq!(s.within(|| 1), None, "no second one beside the running one");
        // Free again once the running one has finished, also after a panic
        // (the panic hook may print a backtrace first, so wait for it).
        let free_within = |s: &Server, v: u8| {
            let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
            loop {
                if let Some(x) = s.within(move || v) {
                    return Some(x);
                }
                if std::time::Instant::now() > until {
                    return None;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        };
        std::thread::sleep(std::time::Duration::from_millis(500));
        assert_eq!(
            free_within(&s, 1),
            Some(1),
            "free again when it has finished"
        );
        assert_eq!(s.within(|| -> u8 { panic!("kernel") }), None);
        assert_eq!(free_within(&s, 2), Some(2), "a panic frees it too");
        let _ = std::fs::remove_dir_all(&s.store.dir);
    }

    impl Server {
        fn list_projects_empty(&self) -> bool {
            self.store.list().is_empty()
        }
    }
}

#[cfg(test)]
mod material_names {
    use super::*;

    fn server() -> Server {
        let dir = std::env::temp_dir().join(format!(
            "tectora-mcp-materials-{}-{}",
            now_ms(),
            std::process::id()
        ));
        Server::new(Store::new(dir).unwrap())
    }

    #[test]
    fn default_material_names_are_english_never_the_reference_programs_german() {
        // ERR-004: `calc`, `get_project` and every write named the default
        // materials "Ziegel", "Blech", "Stein", "Schiefer".
        let mut s = server();
        let r = s
            .call_tool(
                "create_standard_roof",
                &json!({ "name": "A", "type": "gable", "length": 10, "width": 8, "pitch_deg": 40, "summary": "x" }),
            )
            .unwrap();
        let id = r["id"].as_str().unwrap().to_string();
        let calc = s.call_tool("calc", &json!({ "id": id })).unwrap();
        let project = s.call_tool("get_project", &json!({ "id": id })).unwrap();
        for text in [r.to_string(), calc.to_string(), project.to_string()] {
            for german in ["Ziegel", "Blech", "Stein", "Schiefer", "Solarpanel"] {
                assert!(!text.contains(german), "{german} in {text}");
            }
        }
        assert!(calc.to_string().contains("\"tiles\""), "{calc}");
        assert!(project.to_string().contains("\"sheet metal\""), "{project}");
        let _ = std::fs::remove_dir_all(&s.store.dir);
    }
}
