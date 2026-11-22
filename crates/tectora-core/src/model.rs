//! Roof project model — the single source of truth saved as `.tectora` JSON.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::geom::{Point2, signed_area};

/// Current version of the `.tectora` JSON schema.
///
/// Raise it whenever a field is added to the model, not only when one
/// changes meaning: a release that reads an older version does not know the
/// new field and would drop it silently on its next save (ARCH-003). With
/// the bump it refuses the file instead (`newer_format`). The test
/// `every_model_change_has_a_format_version` in `tests/schema.rs` enforces it.
///
/// - 1: first format.
/// - 2: `result_snapshot`.
/// - 3: fields added to version 2 without a bump (e.g. `parts[].height`,
///   written by 0.8.0 as version 2) and unknown top-level keys kept
///   (`Project::extra`). Releases up to 0.8.0 refuse a version 3 file.
pub const SCHEMA_VERSION: u32 = 3;

/// Pitch at or above this value (degrees) means a vertical gable wall.
pub const GABLE_PITCH_DEG: f64 = 89.999;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Project {
    pub schema_version: u32,
    pub name: String,
    #[serde(default)]
    pub notes: String,
    /// Job (Auftrag) data shown in the design manager.
    #[serde(default)]
    pub job: JobInfo,
    pub parts: Vec<RoofPart>,
    #[serde(default)]
    pub dormers: Vec<Dormer>,
    #[serde(default)]
    pub chimneys: Vec<Chimney>,
    #[serde(default)]
    pub windows: Vec<RoofWindow>,
    /// Holes cut into the roof (DACAD: Öffnung), as convex plan polygons.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roof_openings: Vec<Chimney>,
    /// Lines drawn by hand on the calculated roof (DACAD: Dachlinieneingabe).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub manual_lines: Vec<ManualLine>,
    /// Roof faces added by hand (DACAD: Neue Dachfläche).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub manual_faces: Vec<ManualFace>,
    /// Calculate as a flat roof at the highest eave (DACAD: Berechnung ›
    /// Flachdach); see [`crate::flat`].
    #[serde(default, skip_serializing_if = "is_false")]
    pub flat_roof: bool,
    /// Own names of the roof materials (DACAD: Materialnamen), index 0 for
    /// material 1. A missing or empty name keeps DACAD's default name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub material_names: Vec<String>,
    /// The result as it was when the job left the draft status (the figures
    /// an offer was made with), with the engine version that calculated it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_snapshot: Option<crate::snapshot::ResultSnapshot>,
    /// Top-level keys this build does not know (written by a later release
    /// of the same format version), kept and written back as they are.
    /// Never read by the core; bounded by [`MAX_EXTRA_BYTES`].
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// A line drawn by hand on the calculated roof (DACAD: Dachlinieneingabe).
/// DACAD takes its ends from the roof points it snaps to (z included, else
/// 0), lets the user pick its type, length and label, and counts it in the
/// line totals of its type. Unlike DACAD, which drops such lines when the
/// roof is recalculated, Tectora keeps them in the project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ManualLine {
    pub a: [f64; 3],
    pub b: [f64; 3],
    /// DACAD line code: T, F, G, K, OL, OR, OS, P, c, S, WV, WH, WS count in
    /// the line totals; M (dimension line), X (division) and the rest do not.
    pub code: String,
    /// Length typed in by hand instead of the measured one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<f64>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
}

/// DACAD line codes that count in the line totals.
pub const COUNTED_LINE_CODES: [&str; 13] = [
    "T", "F", "G", "K", "OL", "OR", "OS", "P", "c", "S", "WV", "WH", "WS",
];

impl ManualLine {
    /// The line's length: the one typed in, else the 3D distance of its ends.
    pub fn length(&self) -> f64 {
        self.length.unwrap_or_else(|| {
            let d = [
                self.b[0] - self.a[0],
                self.b[1] - self.a[1],
                self.b[2] - self.a[2],
            ];
            (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
        })
    }

    /// Whether the line counts in the line totals of its code.
    pub fn counted(&self) -> bool {
        COUNTED_LINE_CODES.contains(&self.code.as_str())
    }
}

/// A roof face added by hand (DACAD: Neue Dachfläche): a plan polygon and
/// the pitch DACAD asks for. Its area is the plan area over the cosine of
/// the pitch (Tectora's reading; not yet checked against DACAD's numbers).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ManualFace {
    pub points: Vec<Point2>,
    pub pitch_deg: f64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
}

impl ManualFace {
    /// Sloped area, m². 0 for fewer than 3 corners or a pitch outside
    /// 0°..90°, and for a (near) vertical pitch, whose cosine (below 1e-6,
    /// about 89.99994°) would blow a small plan area up to millions of m².
    pub fn area(&self) -> f64 {
        let n = self.points.len();
        if n < 3 || !self.pitch_counts() {
            return 0.0;
        }
        let plan: f64 = (0..n)
            .map(|i| {
                let (p, q) = (self.points[i], self.points[(i + 1) % n]);
                p.x * q.y - q.x * p.y
            })
            .sum::<f64>()
            .abs()
            / 2.0;
        plan / self.pitch_deg.to_radians().cos()
    }

    /// Whether the pitch gives the face an area (see [`ManualFace::area`]).
    pub fn pitch_counts(&self) -> bool {
        self.pitch_deg >= 0.0 && self.pitch_deg < 90.0 && self.pitch_deg.to_radians().cos() > 1e-6
    }
}

impl Project {
    /// The name of roof material `m`: the project's own name, else DACAD's.
    pub fn material_name(&self, m: u8) -> String {
        let i = usize::from(m.max(1) - 1);
        match self.material_names.get(i).map(|s| s.trim()) {
            Some(n) if !n.is_empty() => n.to_string(),
            _ => DACAD_MATERIAL_NAMES
                .get(i)
                .map_or_else(|| format!("Material {m}"), |s| (*s).to_string()),
        }
    }

    /// The name of roof material `m` for MCP and the CLI: the project's own
    /// name, else a neutral English one ([`MATERIAL_LABELS`]). The
    /// reference program's German defaults stay in its file writers and the
    /// price-sheet export (audit ERR-004).
    pub fn material_label(&self, m: u8) -> String {
        let i = usize::from(m.max(1) - 1);
        match self.material_names.get(i).map(|s| s.trim()) {
            Some(n) if !n.is_empty() => n.to_string(),
            _ => MATERIAL_LABELS
                .get(i)
                .map_or_else(|| format!("material {m}"), |s| (*s).to_string()),
        }
    }

    pub fn new(name: impl Into<String>) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            name: name.into(),
            notes: String::new(),
            job: JobInfo::default(),
            parts: Vec::new(),
            dormers: Vec::new(),
            chimneys: Vec::new(),
            windows: Vec::new(),
            roof_openings: Vec::new(),
            manual_lines: Vec::new(),
            manual_faces: Vec::new(),
            flat_roof: false,
            material_names: Vec::new(),
            result_snapshot: None,
            extra: serde_json::Map::new(),
        }
    }

    /// Checks the input the roof engine cannot work with: non-finite
    /// numbers, parts with fewer than three edges, pitches outside
    /// (0°, 180°) and negative overhangs. Returns one message per problem.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let errors: Vec<String> = self.problems().iter().map(|p| self.describe(p)).collect();
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// The problems [`Project::validate`] reports, as data (for a
    /// translated message in the app).
    pub fn problems(&self) -> Vec<Problem> {
        let mut out = Vec::new();
        let mut edges = |item, index, list: &[EdgeDef]| {
            for (ei, e) in list.iter().enumerate() {
                out.extend(e.problems().map(|(code, _)| Problem {
                    item,
                    index,
                    edge: Some(ei),
                    code,
                }));
            }
        };
        for (pi, part) in self.parts.iter().enumerate() {
            edges(ProblemItem::Part, pi, &part.edges);
        }
        for (di, d) in self.dormers.iter().enumerate() {
            edges(ProblemItem::Dormer, di, &d.edges);
        }
        for (pi, part) in self.parts.iter().enumerate() {
            if part.edges.len() < 3 {
                out.push(Problem::of(ProblemItem::Part, pi, "too_few_edges"));
            }
            for ei in zero_length_walls(part) {
                out.push(Problem {
                    item: ProblemItem::Part,
                    index: pi,
                    edge: Some(ei),
                    code: "zero_length_wall",
                });
            }
            for ei in crossing_walls(part) {
                out.push(Problem {
                    item: ProblemItem::Part,
                    index: pi,
                    edge: Some(ei),
                    code: "walls_cross",
                });
            }
            for ei in overlapping_walls(part) {
                out.push(Problem {
                    item: ProblemItem::Part,
                    index: pi,
                    edge: Some(ei),
                    code: "walls_overlap",
                });
            }
        }
        for (di, d) in self.dormers.iter().enumerate() {
            if !finite_point(d.end) {
                out.push(Problem::of(ProblemItem::Dormer, di, "end_nan"));
            }
        }
        let corners =
            |c: &Chimney| c.corners.len() < 3 || !c.corners.iter().all(|&v| finite_point(v));
        for (ci, c) in self.chimneys.iter().enumerate() {
            if corners(c) {
                out.push(Problem::of(ProblemItem::Chimney, ci, "bad_corners"));
            } else if !c.has_area() {
                out.push(Problem::of(ProblemItem::Chimney, ci, "bad_size"));
            }
        }
        for (ci, c) in self.roof_openings.iter().enumerate() {
            if corners(c) {
                out.push(Problem::of(ProblemItem::Opening, ci, "bad_corners"));
            } else if !c.has_area() {
                out.push(Problem::of(ProblemItem::Opening, ci, "bad_size"));
            }
        }
        for (wi, w) in self.windows.iter().enumerate() {
            let ok = [w.x, w.y, w.width, w.height].iter().all(|&v| sane(v))
                && w.width > 0.0
                && w.height > 0.0;
            if !ok {
                out.push(Problem::of(ProblemItem::Window, wi, "bad_size"));
            }
        }
        out.sort_by_key(|p| (p.item, p.index, p.edge));
        out
    }

    /// The English message of a problem (CLI, MCP).
    fn describe(&self, p: &Problem) -> String {
        // 0-based, as every MCP tool argument and `calc.problems` (ERR-007).
        let n = p.index;
        let at = match p.item {
            ProblemItem::Part => format!(
                "part index {n} ({})",
                self.parts
                    .get(p.index)
                    .map(|x| x.name.as_str())
                    .unwrap_or("")
            ),
            ProblemItem::Dormer => format!("dormer index {n}"),
            ProblemItem::Chimney => format!("chimney index {n}"),
            ProblemItem::Opening => format!("roof opening index {n}"),
            ProblemItem::Window => format!("window index {n}"),
        };
        let what = EdgeDef::PROBLEMS
            .iter()
            .find(|(c, _)| *c == p.code)
            .map(|(_, m)| *m)
            .unwrap_or(match p.code {
                "too_few_edges" => "needs at least 3 edges",
                "zero_length_wall" => {
                    "has no length (two corners on one point); delete one of the corners"
                }
                "walls_cross" => "crosses another wall of the same building; move a corner",
                "walls_overlap" => {
                    "runs over another wall of the same building, or the outline goes round more than once; delete the walls of the extra lap or the wall drawn back"
                }
                "end_nan" => "end point is not a number or out of range",
                "bad_corners" => "needs 3 or more finite corners",
                "bad_size" => "needs a finite position and a positive size",
                other => other,
            });
        match p.edge {
            Some(e) => format!("{at}, edge index {e}: {what}"),
            None => format!("{at}: {what}"),
        }
    }

    /// The project file's text. Always stamped with the current
    /// [`SCHEMA_VERSION`]: whatever version the design was read at (or the
    /// editor made it with), the file holds the current fields, and a
    /// release that does not know them must refuse it.
    pub fn to_json(&self) -> serde_json::Result<String> {
        if self.schema_version == SCHEMA_VERSION {
            return serde_json::to_string_pretty(self);
        }
        let mut stamped = self.clone();
        stamped.schema_version = SCHEMA_VERSION;
        serde_json::to_string_pretty(&stamped)
    }

    /// Reads a project file of this or an older format version, migrating
    /// it to [`SCHEMA_VERSION`]; a newer format is an error.
    pub fn from_json(s: &str) -> serde_json::Result<Self> {
        let mut v: serde_json::Value = serde_json::from_str(s)?;
        migrate(&mut v)?;
        serde_json::from_value(v)
    }

    /// Sets the job status. When the job leaves the draft status its result
    /// is stored (see [`Project::result_snapshot`]) unless one is stored
    /// already; returns whether it was.
    pub fn set_status(&mut self, status: JobStatus) -> bool {
        let leaves_draft = self.job.status == JobStatus::Draft && status != JobStatus::Draft;
        self.job.status = status;
        if leaves_draft && self.result_snapshot.is_none() {
            self.result_snapshot = Some(crate::snapshot::take(self));
            return true;
        }
        false
    }

    /// Rejects designs with a number no real roof has (see
    /// [`MAX_ABS_VALUE`]). Such values come from a typo or a broken file,
    /// and the geometry kernel can spend minutes on them or panic, so
    /// callers check before calculating or saving input from outside.
    pub fn check_limits(&self) -> Result<(), String> {
        self.check_input().map_err(|e| e.message)
    }

    /// [`Project::check_limits`] with a code for the app: `too_many` when
    /// the design has more parts, walls or objects than [`MAX_PARTS`] and
    /// the limits beside it allow, `out_of_range` for a number out of range.
    pub fn check_input(&self) -> Result<(), InputError> {
        self.check_counts().map_err(|message| InputError {
            code: "too_many",
            message,
        })?;
        self.check_values().map_err(|message| InputError {
            code: "out_of_range",
            message,
        })
    }

    /// The counts limits (ARCH-002): the core's time grows about with the
    /// square of the parts and walls, so a file with thousands of them
    /// would block the library, the editor or the MCP server for hours.
    fn check_counts(&self) -> Result<(), String> {
        let over = |what: &str, n: usize, max: usize| {
            (n > max).then(|| format!("too many {what}: {n} (at most {max})"))
        };
        let walls: usize = self.parts.iter().map(|p| p.edges.len()).sum();
        let dormer_walls: usize = self.dormers.iter().map(|d| d.edges.len()).sum();
        let all_edges = || {
            self.parts
                .iter()
                .flat_map(|p| &p.edges)
                .chain(self.dormers.iter().flat_map(|d| &d.edges))
        };
        let breaks: usize = all_edges().map(|e| e.breaks.len()).sum();
        let wall_breaks = all_edges().map(|e| e.breaks.len()).max().unwrap_or(0);
        let mut checks = vec![
            over("parts", self.parts.len(), MAX_PARTS),
            over("walls in all parts", walls, MAX_WALLS),
            over("walls of all dormers", dormer_walls, MAX_WALLS),
            over(
                "walls of one dormer",
                self.dormers
                    .iter()
                    .map(|d| d.edges.len())
                    .max()
                    .unwrap_or(0),
                MAX_PART_WALLS,
            ),
            over("pitch breaks of one wall", wall_breaks, MAX_WALL_BREAKS),
            over("pitch breaks in all walls", breaks, MAX_BREAKS),
            over("dormers", self.dormers.len(), MAX_OBJECTS),
            over("chimneys", self.chimneys.len(), MAX_OBJECTS),
            over("roof windows", self.windows.len(), MAX_OBJECTS),
            over("roof openings", self.roof_openings.len(), MAX_OBJECTS),
            over("hand-drawn lines", self.manual_lines.len(), MAX_OBJECTS),
            over("hand-drawn faces", self.manual_faces.len(), MAX_OBJECTS),
        ];
        for (i, part) in self.parts.iter().enumerate() {
            checks.push(over(
                // From 0, as every MCP tool and `calc.problems` (HUNTIO-007).
                &format!("walls in part index {i}"),
                part.edges.len(),
                MAX_PART_WALLS,
            ));
        }
        let corners = self
            .chimneys
            .iter()
            .chain(&self.roof_openings)
            .map(|c| c.corners.len())
            .chain(self.manual_faces.iter().map(|f| f.points.len()))
            .max()
            .unwrap_or(0);
        checks.push(over("corners of one object", corners, MAX_PART_WALLS));
        // The stored result comes with the file as well (ARCH-303).
        checks.push(
            self.result_snapshot
                .as_ref()
                .and_then(crate::snapshot::ResultSnapshot::problem),
        );
        // Unknown top-level keys are kept, not used: bound what they cost.
        let extra_bytes = self
            .extra
            .iter()
            .map(|(k, v)| k.len() + v.to_string().len())
            .sum::<usize>();
        checks.push((extra_bytes > MAX_EXTRA_BYTES).then(|| {
            format!(
                "too much data in unknown keys: {extra_bytes} bytes (at most {MAX_EXTRA_BYTES})"
            )
        }));
        match checks.into_iter().flatten().next() {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    fn check_values(&self) -> Result<(), String> {
        let mut value = serde_json::to_value(self).map_err(|e| e.to_string())?;
        // The stored result is computed output with a timestamp, not input.
        if let Some(obj) = value.as_object_mut() {
            obj.remove("result_snapshot");
        }
        if let Some((path, v)) = first_out_of_range(&value, &mut String::new()) {
            return Err(format!(
                "value {v} at `{path}` is out of range (allowed ±{MAX_ABS_VALUE})"
            ));
        }
        // JSON has no infinity or NaN: such a number is written as null and
        // the file can no longer be read (MATH-001), so refuse it here.
        if let Some(why) = non_finite(self) {
            return Err(format!("a value is not a finite number ({why})"));
        }
        if let Some((i, h)) = self
            .parts
            .iter()
            .enumerate()
            .find_map(|(i, p)| p.height.filter(|h| *h < 0.0).map(|h| (i, h)))
        {
            return Err(format!(
                "value {h} at `parts[{i}].height` is out of range (a building height is 0 m or more)"
            ));
        }
        Ok(())
    }
}

/// Largest absolute number accepted anywhere in a design: 100 km in
/// metres, far above any building and far below where f64 geometry breaks.
pub const MAX_ABS_VALUE: f64 = 1.0e5;

/// Most bytes the unknown top-level keys of a file may hold together
/// ([`Project::extra`]).
pub const MAX_EXTRA_BYTES: usize = 64 * 1024;

/// Most parts (buildings, wings) a design may have. Generous: a large
/// TERRAN job has a few dozen; 200 still calculate in a few seconds.
pub const MAX_PARTS: usize = 200;
/// Most walls of one part (a round roof uses up to 30 per arc).
pub const MAX_PART_WALLS: usize = 500;
/// Most walls of all parts together.
pub const MAX_WALLS: usize = 2000;
/// Most dormers, chimneys, windows, openings, hand-drawn lines or faces,
/// each.
pub const MAX_OBJECTS: usize = 500;
/// Most pitch breaks on one wall. A mansard has one or two; a round
/// dormer stores up to `MAX_ROUND_SEGMENTS` (30) on its front wall.
pub const MAX_WALL_BREAKS: usize = 64;
/// Most pitch breaks of all walls together (ARCH-053: the core's time grows
/// faster than linearly with the breaks of a wall, 36 000 took 15 s).
pub const MAX_BREAKS: usize = 2000;

/// Why a design is refused before it is calculated (see
/// [`Project::check_input`]).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct InputError {
    /// `too_many` or `out_of_range`.
    pub code: &'static str,
    /// English, for the CLI, the MCP server and the details view.
    pub message: String,
}

/// Whether the project reads back from its own JSON; `serde_json` writes an
/// infinite or NaN number as null, which no `f64` field accepts.
fn non_finite(p: &Project) -> Option<String> {
    let v = serde_json::to_value(p).ok()?;
    serde_json::from_value::<Project>(v)
        .err()
        .map(|e| e.to_string())
}

fn first_out_of_range(v: &serde_json::Value, path: &mut String) -> Option<(String, f64)> {
    use serde_json::Value;
    match v {
        Value::Number(n) => n
            .as_f64()
            .filter(|f| !f.is_finite() || f.abs() > MAX_ABS_VALUE)
            .map(|f| (path.clone(), f)),
        Value::Array(items) => items.iter().enumerate().find_map(|(i, item)| {
            let len = path.len();
            path.push_str(&format!("[{i}]"));
            let found = first_out_of_range(item, path);
            path.truncate(len);
            found
        }),
        Value::Object(map) => map.iter().find_map(|(key, item)| {
            let len = path.len();
            if !path.is_empty() {
                path.push('.');
            }
            path.push_str(key);
            let found = first_out_of_range(item, path);
            path.truncate(len);
            found
        }),
        _ => None,
    }
}

/// Runs `f` and turns a panic into an error, so one bad design cannot take
/// down the app or the MCP server. The geometry kernel is a port of DACAD's
/// core and still has `expect`s on cases DACAD itself aborts on.
///
/// The stop of a bounded calculation ([`crate::work`]) is passed on to the
/// [`crate::work::bounded`] that set the limit, so an inner guard never
/// turns it into an empty roof; outside one it is [`crate::work::TOO_COMPLEX`].
pub fn guarded<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).map_err(|e| {
        if crate::work::is_too_complex(&*e) {
            if crate::work::active() {
                std::panic::resume_unwind(e);
            }
            return crate::work::TOO_COMPLEX.to_string();
        }
        let msg = e
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| e.downcast_ref::<&str>().copied())
            .unwrap_or("unknown error");
        format!("roof calculation failed: {msg}")
    })
}

/// What a problem found by [`Project::problems`] is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProblemItem {
    Part,
    Dormer,
    Chimney,
    Opening,
    Window,
}

/// One input the roof engine cannot work with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Problem {
    pub item: ProblemItem,
    /// Index of the part, dormer, chimney, opening or window (from 0).
    pub index: usize,
    /// Index of the edge within the part or dormer (from 0), if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge: Option<usize>,
    /// A stable code: `too_few_edges`, `start_nan`, `pitch_range`,
    /// `eave_nan`, `eave_negative`, `overhang_negative`, `direction_nan`,
    /// `stacked_nan`, `break_below_eave`, `break_duplicate`,
    /// `half_hip_below_eave`, `end_nan`,
    /// `bad_corners`, `bad_size`, `zero_length_wall`, `walls_cross` or
    /// `walls_overlap`.
    pub code: &'static str,
}

/// Shortest wall that is not a direction marker, m. Two corners on one
/// point (a corner clicked twice while drawing, the closing corner drawn
/// onto the first one) give a wall of no length and no direction. DACAD
/// has no such wall: every DACAD wall record carries its own direction,
/// and a zero-length one is a direction change (`direction_rad`). The core
/// takes the missing direction as east and ends with a cut error, which
/// stopped the roof of every building in the job; so such a part is
/// reported as invalid and left out on its own, like any other invalid
/// input. The limit is the core's own: a wall record shorter than 0.01 m
/// is a point (`coresol::rd2::Wall::zero`), and walls of 1 to 10 mm ended
/// with the same cut error (review ERR-001, 2026-11-14).
pub const MIN_WALL_LENGTH: f64 = 0.01;

/// The edges of `part` that are walls of no length (see
/// [`MIN_WALL_LENGTH`]). Edges imported from a DACAD wall record keep their
/// recorded direction and are not listed.
pub fn zero_length_walls(part: &RoofPart) -> Vec<usize> {
    let n = part.edges.len();
    if n < 3 {
        return Vec::new();
    }
    (0..n)
        .filter(|&i| {
            let e = &part.edges[i];
            let next = part.edges[(i + 1) % n].start;
            e.direction_rad.is_none()
                && e.source.is_none()
                && finite_point(e.start)
                && finite_point(next)
                && e.start.distance(next) < MIN_WALL_LENGTH
        })
        .collect()
}

/// The first wall of each pair of walls of `part` that cross each other.
/// A wall loop that crosses itself (a bow tie, a corner dragged over the
/// opposite wall) is not a building: the core still returned a roof for it,
/// with faces of one corner and areas of no meaning, and nothing warned
/// (audit 2026-11-15 HUNTC-002, HUNTIO-009). Only a proper crossing counts:
/// walls that touch at a corner or run along each other are left alone.
/// Parts with a direction marker or imported wall records are not checked,
/// because their walls are not the straight lines between the corners.
pub fn crossing_walls(part: &RoofPart) -> Vec<usize> {
    let n = part.edges.len();
    if n < 4
        || part
            .edges
            .iter()
            .any(|e| e.direction_rad.is_some() || e.source.is_some() || !finite_point(e.start))
    {
        return Vec::new();
    }
    let seg = |i: usize| (part.edges[i].start, part.edges[(i + 1) % n].start);
    // Side of `c` relative to the line a-b, with a tolerance of 1 mm in
    // distance so that corners lying on a wall do not count as crossing.
    let side = |a: Point2, b: Point2, c: Point2| {
        let len = a.distance(b);
        let cross = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
        if len <= 0.0 || cross.abs() <= 1e-3 * len {
            0
        } else if cross > 0.0 {
            1
        } else {
            -1
        }
    };
    let mut out = Vec::new();
    for i in 0..n {
        let (a, b) = seg(i);
        let crosses = (i + 2..n).any(|j| {
            if (j + 1) % n == i {
                return false; // neighbours share a corner
            }
            let (c, d) = seg(j);
            let (s1, s2) = (side(a, b, c), side(a, b, d));
            let (s3, s4) = (side(c, d, a), side(c, d, b));
            s1 * s2 < 0 && s3 * s4 < 0
        });
        if crosses {
            out.push(i);
        }
    }
    out
}

/// A direction marker shorter than this is a point to the core
/// (`roof::snap_markers`), m.
pub(crate) const MARKER_SNAP: f64 = 0.005;

/// The later wall of each pair of walls of `part` that lie on each other
/// along more than 1 mm: an outline that runs round the building twice,
/// or a wall drawn back over another one. The core still returned a roof
/// for it, each face twice and no eave (audit 2026-11-21 HUNT-106).
/// Walls that only touch, at a wall or at a corner (two wings meeting
/// corner to corner, which the core calculates, HUNT-202), that continue
/// one another in a line, or that run side by side apart (a second lap a
/// centimetre inside the first is two walls, not one) are left alone.
/// Direction markers (zero-length walls with a direction) are skipped on
/// their own, not with the whole part (HUNT-201). Parts with imported
/// wall records are checked too (HUNT-302): of 3102 parts in 2670 DACAD
/// files, none has walls on each other.
pub fn overlapping_walls(part: &RoofPart) -> Vec<usize> {
    let n = part.edges.len();
    if n < 4 || part.edges.iter().any(|e| !finite_point(e.start)) {
        return Vec::new();
    }
    // A marker a few millimetres long is a point to the core, which moves
    // the next corner onto it (`roof::snap_markers`): so here too, or the
    // wall after it runs those millimetres back over the wall before it
    // (sample HPS01 without its wall records, HUNT-304).
    let mut pts: Vec<Point2> = part.edges.iter().map(|e| e.start).collect();
    for i in 0..n {
        let j = (i + 1) % n;
        let d = pts[i].distance(pts[j]);
        if part.edges[i].direction_rad.is_some() && d > 0.0 && d < MARKER_SNAP {
            pts[j] = pts[i];
        }
    }
    // The walls with a length, by their index in `part.edges`; markers
    // and walls of no length (reported as `zero_length_wall`) are left out.
    let walls: Vec<(usize, Point2, Point2)> = (0..n)
        .map(|i| (i, pts[i], pts[(i + 1) % n]))
        .filter(|&(_, a, b)| a.distance(b) >= MIN_WALL_LENGTH)
        .collect();
    if walls.len() < 3 {
        return Vec::new();
    }
    const TOL: f64 = 1e-3;
    let mut out = Vec::new();
    for (k, &(j, c, d)) in walls.iter().enumerate().skip(1) {
        let on_top = walls[..k].iter().any(|&(_, a, b)| {
            let len = a.distance(b);
            let (ux, uy) = ((b.x - a.x) / len, (b.y - a.y) / len);
            // Distance of a point from the line a-b and its position along it.
            let off = |p: Point2| ((p.x - a.x) * uy - (p.y - a.y) * ux).abs();
            let along = |p: Point2| (p.x - a.x) * ux + (p.y - a.y) * uy;
            if off(c) > TOL || off(d) > TOL {
                return false;
            }
            let (lo, hi) = {
                let (s, t) = (along(c), along(d));
                (s.min(t), s.max(t))
            };
            lo.max(0.0) + TOL < hi.min(len)
        });
        if on_top {
            out.push(j);
        }
    }
    out
}

impl Problem {
    fn of(item: ProblemItem, index: usize, code: &'static str) -> Problem {
        Problem {
            item,
            index,
            edge: None,
            code,
        }
    }
}

/// Upgrades the JSON of an older project file in place, one format version
/// at a time.
fn migrate(v: &mut serde_json::Value) -> serde_json::Result<()> {
    use serde::de::Error;
    let Some(obj) = v.as_object_mut() else {
        return Err(serde_json::Error::custom(
            "a project file holds a JSON object",
        ));
    };
    // A file without the field is format 1; a field that is no whole
    // number is a damaged file, not format 1 (it could be a newer one).
    // `2.0` (as some JSON writers put a whole number) is version 2; a whole
    // number beyond `u64` (`1e30`) is a version this release cannot know
    // (ARCH-205 item 1).
    let version = match obj.get("schema_version") {
        None => 1,
        Some(v) => match (v.as_u64(), v.as_f64()) {
            (Some(n), _) => n,
            (None, Some(f)) if f.is_finite() && f.fract() == 0.0 && f >= 0.0 => {
                if f < u64::MAX as f64 {
                    f as u64
                } else {
                    return Err(serde_json::Error::custom(format!(
                        "the file's format version {f:e} is newer than this Tectora reads (up to {SCHEMA_VERSION}) or the file is damaged"
                    )));
                }
            }
            _ => {
                return Err(serde_json::Error::custom(
                    "the file's format version `schema_version` is not a whole number",
                ));
            }
        },
    };
    if version > u64::from(SCHEMA_VERSION) {
        return Err(serde_json::Error::custom(format!(
            "the file was saved by a newer Tectora (format {version}, this version reads up to {SCHEMA_VERSION})"
        )));
    }
    // 1 -> 2: adds the optional `result_snapshot`; 2 -> 3: adds the other
    // optional fields listed at [`SCHEMA_VERSION`]. Every new field is
    // optional, so there is nothing to convert; the bump makes older
    // releases refuse what this one writes.
    obj.insert("schema_version".into(), SCHEMA_VERSION.into());
    Ok(())
}

/// Customer and job details of a design, like the job header in DACAD lists.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct JobInfo {
    /// Job or offer number.
    pub number: String,
    pub customer: String,
    pub address: String,
    pub status: JobStatus,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    #[default]
    Draft,
    Offer,
    Ordered,
    Done,
}

/// One closed footprint whose edges each carry their own roof definition
/// (like DACAD: every wall line has pitch, eave height and overhang).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RoofPart {
    pub name: String,
    /// Edges in order; edge `i` runs from `edges[i].start` to `edges[i+1].start`.
    pub edges: Vec<EdgeDef>,
    /// Height of the building: the top of its walls above ground, m.
    /// Tectora's own value: DACAD has no building height (its core reads
    /// every height from the walls' roof definitions), so it changes no
    /// roof figure. The eave height of a wall is its own input and may be
    /// lower than the building (DACAD measures the eave at the overhang
    /// line, below the wall top) or higher. `None` in files from before
    /// the field: the building is as high as its lowest eave, as the
    /// editor showed it then ([`RoofPart::building_height`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
}

impl RoofPart {
    /// A part with these walls and no building height of its own.
    pub fn new(name: impl Into<String>, edges: Vec<EdgeDef>) -> Self {
        RoofPart {
            name: name.into(),
            edges,
            height: None,
        }
    }

    /// The building's height ([`RoofPart::height`]), else its lowest eave
    /// height; `None` for a part without walls.
    pub fn building_height(&self) -> Option<f64> {
        self.height
            .or_else(|| self.edges.iter().map(|e| e.eave_height).reduce(f64::min))
    }

    /// Gives a part without a height of its own the height it shows now
    /// (its lowest eave), so that changing a wall's eave height afterwards
    /// does not change the building height. Called before an eave edit.
    pub fn freeze_height(&mut self) {
        if self.height.is_none() {
            self.height = self.building_height();
        }
    }

    pub fn outline(&self) -> Vec<Point2> {
        self.edges.iter().map(|e| e.start).collect()
    }

    /// Reorders edges so the outline is counter-clockwise.
    pub fn normalize_ccw(&mut self) {
        if signed_area(&self.outline()) < 0.0 {
            // Reversing the vertex order also flips which edge each definition
            // belongs to: edge i (v_i -> v_i+1) becomes (v_i+1 -> v_i).
            let n = self.edges.len();
            let starts: Vec<Point2> = self.edges.iter().map(|e| e.start).collect();
            let mut edges: Vec<EdgeDef> = Vec::with_capacity(n);
            for i in (0..n).rev() {
                let mut e = self.edges[i].clone();
                e.start = starts[(i + 1) % n];
                e.direction_rad = e.direction_rad.map(|a| a + std::f64::consts::PI);
                edges.push(e);
            }
            self.edges = edges;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EdgeDef {
    pub start: Point2,
    /// Roof pitch in degrees; 90 means a gable (vertical wall, no roof plane).
    pub pitch_deg: f64,
    /// Height of the eave line above ground, m.
    pub eave_height: f64,
    /// Horizontal eave overhang beyond the wall, m.
    pub overhang: f64,
    /// Clipped gable (Krüppelwalm) on a gable edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub half_hip: Option<HalfHip>,
    /// Pitch breaks (DACAD: Knick) above the eave, e.g. a mansard roof:
    /// from each break's height on, the plane continues at its pitch.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub breaks: Vec<PitchBreak>,
    /// DACAD priority (0 or 1). Where this plane and a parallel one meet in a
    /// horizontal line, the plane with priority ends there; without one, the
    /// steeper plane ends.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub priority: u8,
    /// Direction of a zero-length edge (radians). DACAD uses such marker
    /// edges at steps between collinear walls to say how the roof turns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction_rad: Option<f64>,
    /// A gable, or a wall leaning outward (over 90°), covered with the roof
    /// material (DACAD: such a wall with a material): its area counts as
    /// roof area. Walls sloped below 90° always carry the material.
    #[serde(default, skip_serializing_if = "is_false")]
    pub clad: bool,
    /// Roof material of the wall's faces (DACAD: Material in the roof
    /// definition), numbered as in DACAD's material list from 1: 1 tiles,
    /// 2 sheet metal, 3 stone, 4 slate, 5 solar panels (DACAD.exe string
    /// table 1601-1605; 1600 is "no material"). Whether a gable carries it
    /// is [`EdgeDef::clad`]; DACAD's core only sees that flag, so the
    /// number only splits the roof area by material.
    #[serde(
        default = "default_material",
        skip_serializing_if = "is_default_material"
    )]
    pub material: u8,
    /// The DACAD wall record this edge was imported from. DACAD's core
    /// takes each wall as recorded (its own length and direction, in file
    /// order), which matters where the walls of a file do not quite meet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<WallSource>,
}

/// A wall record of an imported DACAD file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WallSource {
    /// Wall number in the file, from 1; walls are listed in this order.
    pub id: u16,
    /// Roof number.
    pub roof: u16,
    pub start: Point2,
    pub length: f64,
    /// Direction, degrees.
    pub angle_deg: f64,
}

/// Largest coordinate, height or overhang the engine accepts, m. Finite
/// values beyond it (e.g. `1e308` in a hand-edited file) overflow in the
/// engine's arithmetic and give NaN geometry; no roof is 1000 km away.
pub const MAX_COORD: f64 = 1e6;

/// Smallest and largest wall pitch DACAD accepts, degrees (help,
/// "Voreinstellungen": "Dachneigungen unter 2 Grad oder über 178 Grad sind
/// nicht zugelassen").
pub const MIN_PITCH_DEG: f64 = 2.0;
pub const MAX_PITCH_DEG: f64 = 178.0;

/// Heights closer than this count as the same height for the pitch-break
/// checks: half of DACAD's 1 cm input step.
const HEIGHT_EPS: f64 = 0.005;

/// A finite number within [`MAX_COORD`].
fn sane(v: f64) -> bool {
    v.is_finite() && v.abs() <= MAX_COORD
}

fn finite_point(p: Point2) -> bool {
    sane(p.x) && sane(p.y)
}

/// DACAD's default material names (DACAD.exe strings 1601-1605, German
/// build), material 1 first.
pub const DACAD_MATERIAL_NAMES: [&str; DACAD_MATERIALS as usize] =
    ["Ziegel", "Blech", "Stein", "Schiefer", "Solarpanel"];

/// Neutral English names of the default materials, material 1 first, as
/// MCP and the CLI give them.
pub const MATERIAL_LABELS: [&str; DACAD_MATERIALS as usize] =
    ["tiles", "sheet metal", "stone", "slate", "solar panels"];

/// DACAD's first material (tiles), the material of a new wall.
pub const DEFAULT_MATERIAL: u8 = 1;
/// Materials in DACAD's default material list (DACAD.exe strings 1601-1605).
pub const DACAD_MATERIALS: u8 = 5;

fn default_material() -> u8 {
    DEFAULT_MATERIAL
}

fn is_default_material(v: &u8) -> bool {
    *v == DEFAULT_MATERIAL
}

fn is_false(v: &bool) -> bool {
    !*v
}

fn is_zero(v: &u8) -> bool {
    *v == 0
}

impl EdgeDef {
    /// What makes this edge unusable for the engine (see [`Project::validate`]).
    /// Codes and English messages of [`EdgeDef::problems`].
    pub const PROBLEMS: [(&'static str, &'static str); 10] = [
        ("start_nan", "start point is not a number or out of range"),
        ("pitch_range", "pitch must be between 2° and 178°"),
        ("eave_nan", "eave height is not a number or out of range"),
        ("eave_negative", "eave height must be zero or positive"),
        ("overhang_negative", "overhang must be zero or positive"),
        ("direction_nan", "direction is not a number"),
        (
            "stacked_nan",
            "half-hip or pitch break is not a number or out of range",
        ),
        ("break_below_eave", "pitch break lies below the eave"),
        ("break_duplicate", "two pitch breaks at the same height"),
        ("half_hip_below_eave", "half-hip lies below the eave"),
    ];

    /// What keeps the roof engine from using this edge, as
    /// `(code, English message)`.
    pub fn problems(&self) -> impl Iterator<Item = (&'static str, &'static str)> {
        let stacked = self
            .half_hip
            .iter()
            .map(|h| (h.height, h.pitch_deg))
            .chain(self.breaks.iter().map(|b| (b.height, b.pitch_deg)));
        // DACAD's input limits (help, "Voreinstellungen" and
        // "Dachdefinitionen"): "Dachneigungen unter 2 Grad oder über 178 Grad
        // sind nicht zugelassen", "Minimale Traufenhöhe ist 0"; pitch-break
        // errors "Knickhöhe liegt tiefer als Traufe!" and "Knickhöhe bereits
        // vorhanden!" (DACAD.exe strings 211, 212).
        let pitch_ok = |p: f64| (MIN_PITCH_DEG..=MAX_PITCH_DEG).contains(&p);
        // A half-hip is a roof plane like any other: DACAD's pitch limit
        // holds for it too, and a pitch of 0 divides by zero in the core
        // (x87 division by zero in the plane setup). It starts above the
        // eave like a pitch break.
        let half_hip_low = self
            .half_hip
            .as_ref()
            .is_some_and(|h| h.height < self.eave_height - HEIGHT_EPS);
        let breaks_low = self
            .breaks
            .iter()
            .any(|b| b.height < self.eave_height - HEIGHT_EPS);
        let breaks_dup = self.breaks.iter().enumerate().any(|(i, a)| {
            self.breaks[..i]
                .iter()
                .any(|b| (a.height - b.height).abs() <= HEIGHT_EPS)
        });
        let checks = [
            !finite_point(self.start),
            !pitch_ok(self.pitch_deg)
                || self.breaks.iter().any(|b| !pitch_ok(b.pitch_deg))
                || self
                    .half_hip
                    .as_ref()
                    .is_some_and(|h| h.pitch_deg.is_finite() && !pitch_ok(h.pitch_deg)),
            !sane(self.eave_height),
            self.eave_height < 0.0,
            !(self.overhang >= 0.0 && sane(self.overhang)),
            self.direction_rad.is_some_and(|a| !a.is_finite()),
            stacked.into_iter().any(|(h, p)| !sane(h) || !p.is_finite()),
            breaks_low,
            breaks_dup,
            half_hip_low,
        ];
        checks
            .into_iter()
            .zip(Self::PROBLEMS)
            .filter(|c| c.0)
            .map(|c| c.1)
    }

    /// A vertical wall. A pitch over 90° is a plane leaning outward, not a
    /// gable (DACAD case C71).
    pub fn is_gable(&self) -> bool {
        (self.pitch_deg - 90.0).abs() <= 90.0 - GABLE_PITCH_DEG
    }

    /// The sloped planes of this wall from bottom to top, as
    /// `(start height, pitch)`, like DACAD's stacked roof definitions.
    /// A gable has none unless it is clipped (Krüppelwalm): then its plane
    /// starts at the half-hip height. Breaks at or below the previous
    /// plane's start are ignored; a vertical break ends the planes (the
    /// wall is a gable above it).
    pub fn planes(&self) -> Vec<(f64, f64)> {
        let mut out: Vec<(f64, f64)> = Vec::new();
        if !self.is_gable() {
            out.push((self.eave_height, self.pitch_deg));
        } else if let Some(h) = &self.half_hip
            && h.pitch_deg > 0.0
            && h.pitch_deg < GABLE_PITCH_DEG
        {
            out.push((h.height, h.pitch_deg));
        }
        if out.is_empty() {
            return out;
        }
        let mut breaks = self.breaks.clone();
        breaks.sort_by(|a, b| a.height.total_cmp(&b.height));
        for b in breaks {
            let last = out.last().expect("not empty").0;
            if b.height > last + 1e-6 && b.pitch_deg > 0.0 {
                // A vertical break turns the plane into a gable from there.
                out.push((b.height, b.pitch_deg.min(90.0)));
                if b.pitch_deg >= GABLE_PITCH_DEG {
                    break;
                }
            }
        }
        out
    }
}

/// A change of pitch at a height above the eave (DACAD: Knick).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PitchBreak {
    /// Height of the break line, m above ground.
    pub height: f64,
    pub pitch_deg: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HalfHip {
    /// Height at which the hipped part starts, m above ground.
    pub height: f64,
    pub pitch_deg: f64,
}

/// A dormer stored as its open wall polyline (front edge plus cheeks).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Dormer {
    pub edges: Vec<EdgeDef>,
    pub end: Point2,
    /// The main-roof wall (start and end point) whose roof plane the dormer
    /// sits on. DACAD cuts the dormer with that plane wherever the dormer
    /// stands (V1 roof record, DACAD cases D101_0013, D101_0020); without
    /// it the faces under the dormer's walls are the hosts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_wall: Option<[Point2; 2]>,
}

/// A chimney (or other penetration) as a convex plan polygon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Chimney {
    #[serde(default)]
    pub corners: Vec<Point2>,
    /// Top height, m above ground. Does not affect quantities.
    #[serde(default)]
    pub height: f64,
}

/// The smallest plan area of a chimney or roof opening, m² (1 cm²).
pub const MIN_OPENING_AREA: f64 = 1e-4;

impl Chimney {
    /// False for a chimney or roof opening without plan area (all corners
    /// on one point or one line, e.g. width 0 from an AI client or a
    /// damaged file). The cut treats such a polygon as covering its whole
    /// face, so the roof area drops to zero without a warning (ERR-002):
    /// it is a problem of the object ([`Project::problems`]), never input
    /// for the calculation.
    pub fn has_area(&self) -> bool {
        let a = crate::geom::signed_area(&self.corners).abs();
        a.is_finite() && a >= MIN_OPENING_AREA
    }
}

/// A roof window (DACAD: Dachfenster). The position is the plan point of
/// its lower corner at the start of the eave; width runs along the eave and
/// height up the slope, both in the roof plane.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RoofWindow {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// JSON Schema of the `.tectora` project file (committed as
/// `docs/tectora.schema.json`).
pub fn json_schema() -> serde_json::Value {
    let mut v = serde_json::to_value(schemars::schema_for!(Project)).expect("schema is JSON");
    v["$id"] = "https://github.com/jakubkodajek/tectora/blob/main/docs/tectora.schema.json".into();
    v["title"] = "Tectora project".into();
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_infinite_number_is_refused_before_it_is_saved_as_null() {
        // MATH-001: JSON writes inf as null, and the job no longer opens.
        let mut p = Project::new("t");
        let mut e = crate::edit::plain_edge(Point2::new(0.0, 0.0), 40.0, 3.0, 0.5);
        p.parts.push(RoofPart {
            height: None,
            name: "A".into(),
            edges: vec![e.clone(); 3],
        });
        assert!(p.check_input().is_ok());
        e.eave_height = f64::INFINITY;
        p.parts[0].edges[1] = e;
        assert_eq!(p.check_input().unwrap_err().code, "out_of_range");
    }

    #[test]
    fn edges_outside_dacad_input_limits_are_problems() {
        // DACAD help: pitch 2-178°, eave height >= 0; strings 211/212:
        // a pitch break below the eave or twice at one height.
        let codes = |f: &dyn Fn(&mut EdgeDef)| {
            let mut e = crate::edit::plain_edge(Point2::new(0.0, 0.0), 40.0, 3.0, 0.5);
            f(&mut e);
            e.problems().map(|(c, _)| c).collect::<Vec<_>>()
        };
        assert!(codes(&|_| {}).is_empty());
        assert!(codes(&|e| e.pitch_deg = 2.0).is_empty());
        assert!(codes(&|e| e.pitch_deg = 178.0).is_empty());
        assert!(codes(&|e| e.eave_height = 0.0).is_empty());
        assert_eq!(codes(&|e| e.pitch_deg = 1.9), ["pitch_range"]);
        assert_eq!(codes(&|e| e.pitch_deg = 178.1), ["pitch_range"]);
        assert_eq!(codes(&|e| e.eave_height = -0.01), ["eave_negative"]);
        let brk = |h: f64, p: f64| PitchBreak {
            height: h,
            pitch_deg: p,
        };
        assert!(codes(&|e| e.breaks = vec![brk(3.0, 30.0), brk(5.0, 20.0)]).is_empty());
        assert_eq!(
            codes(&|e| e.breaks = vec![brk(2.9, 30.0)]),
            ["break_below_eave"]
        );
        assert_eq!(
            codes(&|e| e.breaks = vec![brk(5.0, 30.0), brk(5.004, 20.0)]),
            ["break_duplicate"]
        );
        assert_eq!(codes(&|e| e.breaks = vec![brk(5.0, 1.0)]), ["pitch_range"]);
        // A half-hip is held to the same limits; pitch 0 with a pitch break
        // on the same gable divided by zero in the core (audit ERR-010).
        let hh = |h: f64, p: f64| {
            Some(HalfHip {
                height: h,
                pitch_deg: p,
            })
        };
        assert!(codes(&|e| e.half_hip = hh(5.0, 50.0)).is_empty());
        assert_eq!(codes(&|e| e.half_hip = hh(5.0, 0.0)), ["pitch_range"]);
        assert_eq!(codes(&|e| e.half_hip = hh(5.0, -10.0)), ["pitch_range"]);
        assert_eq!(codes(&|e| e.half_hip = hh(5.0, 1.9)), ["pitch_range"]);
        assert_eq!(
            codes(&|e| e.half_hip = hh(2.0, 50.0)),
            ["half_hip_below_eave"]
        );
        assert_eq!(
            codes(&|e| e.half_hip = hh(-1.0, 50.0)),
            ["half_hip_below_eave"]
        );
        assert_eq!(codes(&|e| e.half_hip = hh(5.0, f64::NAN)), ["stacked_nan"]);
    }

    #[test]
    fn walls_that_cross_are_a_problem() {
        let part = |pts: &[(f64, f64)]| {
            RoofPart::new(
                "A",
                pts.iter()
                    .map(|&(x, y)| crate::edit::plain_edge(Point2::new(x, y), 40.0, 3.0, 0.5))
                    .collect(),
            )
        };
        let rect = part(&[(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)]);
        assert!(crossing_walls(&rect).is_empty());
        // Bow tie: walls 0 and 2 cross.
        let bow = part(&[(0.0, 0.0), (10.0, 0.0), (0.0, 8.0), (10.0, 8.0)]);
        assert_eq!(crossing_walls(&bow), [1]);
        // An L and a notch touching a wall at a corner do not cross.
        let l = part(&[
            (0.0, 0.0),
            (10.0, 0.0),
            (10.0, 4.0),
            (5.0, 4.0),
            (5.0, 8.0),
            (0.0, 8.0),
        ]);
        assert!(crossing_walls(&l).is_empty());
        let touch = part(&[(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (5.0, 0.0), (0.0, 8.0)]);
        assert!(crossing_walls(&touch).is_empty());
        // A corner dragged over the opposite wall.
        let dragged = part(&[
            (0.0, 0.0),
            (10.0, 0.0),
            (10.0, 8.0),
            (5.0, -4.0),
            (0.0, 8.0),
        ]);
        assert!(!crossing_walls(&dragged).is_empty());
        let mut p = crate::examples::find("gable").unwrap().project();
        p.parts = vec![bow];
        assert!(
            p.problems()
                .iter()
                .any(|q| q.code == "walls_cross" && q.edge == Some(1))
        );
    }

    /// Audit 2026-11-21 HUNT-106: an outline that runs round twice, or
    /// whose walls lie on each other, is not one building: the roof had
    /// twice the area and no eave, with no problem reported.
    #[test]
    fn walls_that_retrace_the_outline_are_a_problem() {
        let part = |pts: &[(f64, f64)]| {
            RoofPart::new(
                "A",
                pts.iter()
                    .map(|&(x, y)| crate::edit::plain_edge(Point2::new(x, y), 40.0, 3.0, 0.5))
                    .collect(),
            )
        };
        let r = [(0.0, 0.0), (12.0, 0.0), (12.0, 8.0), (0.0, 8.0)];
        let twice: Vec<(f64, f64)> = r.iter().chain(r.iter()).copied().collect();
        assert_eq!(overlapping_walls(&part(&twice)), [4, 5, 6, 7]);
        // A wall run back over part of another one.
        let back = part(&[
            (0.0, 0.0),
            (12.0, 0.0),
            (12.0, 8.0),
            (6.0, 8.0),
            (6.0, 0.0),
            (3.0, 0.0),
            (3.0, -0.0001),
            (0.0, 8.0),
        ]);
        assert!(!overlapping_walls(&back).is_empty());
        // Ordinary outlines: rectangle, L, a notch touching a wall, two
        // walls in line one after the other.
        assert!(overlapping_walls(&part(&r)).is_empty());
        let l = part(&[
            (0.0, 0.0),
            (10.0, 0.0),
            (10.0, 4.0),
            (5.0, 4.0),
            (5.0, 8.0),
            (0.0, 8.0),
        ]);
        assert!(overlapping_walls(&l).is_empty());
        let touch = part(&[(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (5.0, 0.0), (0.0, 8.0)]);
        assert!(overlapping_walls(&touch).is_empty());
        let inline = part(&[(0.0, 0.0), (5.0, 0.0), (12.0, 0.0), (12.0, 8.0), (0.0, 8.0)]);
        assert!(overlapping_walls(&inline).is_empty());
        let u = part(&[
            (0.0, 0.0),
            (12.0, 0.0),
            (12.0, 8.0),
            (8.0, 8.0),
            (8.0, 3.0),
            (4.0, 3.0),
            (4.0, 8.0),
            (0.0, 8.0),
        ]);
        assert!(overlapping_walls(&u).is_empty());
        let mut p = crate::examples::find("gable").unwrap().project();
        p.parts = vec![part(&twice)];
        let codes: Vec<_> = p.problems().iter().map(|q| (q.code, q.edge)).collect();
        assert!(codes.contains(&("walls_overlap", Some(4))), "{codes:?}");
        assert!(p.validate().unwrap_err()[0].contains("runs over another wall"));
    }

    /// Audit 2026-11-21 HUNT-202, TEST-208: two wings meeting corner to
    /// corner touch at one point only; the core calculates the roof, so it
    /// is no `walls_overlap`.
    #[test]
    fn an_outline_touching_itself_at_one_corner_is_no_overlap() {
        let part = |pts: &[(f64, f64)]| {
            RoofPart::new(
                "A",
                pts.iter()
                    .map(|&(x, y)| crate::edit::plain_edge(Point2::new(x, y), 35.0, 3.0, 0.5))
                    .collect(),
            )
        };
        let bow = part(&[
            (0.0, 0.0),
            (5.0, 0.0),
            (5.0, 5.0),
            (10.0, 5.0),
            (10.0, 10.0),
            (5.0, 10.0),
            (5.0, 5.0),
            (0.0, 5.0),
        ]);
        assert!(overlapping_walls(&bow).is_empty());
        let corner_l = part(&[
            (0.0, 0.0),
            (10.0, 0.0),
            (10.0, 5.0),
            (5.0, 5.0),
            (5.0, 10.0),
            (0.0, 10.0),
            (0.0, 5.0),
            (5.0, 5.0),
        ]);
        assert!(overlapping_walls(&corner_l).is_empty());
        let mut p = crate::examples::find("gable").unwrap().project();
        p.parts = vec![bow];
        assert!(p.validate().is_ok(), "{:?}", p.validate());
        let area = crate::quantities::roof_totals(&p).roof_area;
        assert!(area > 50.0, "{area}");
    }

    /// Audit 2026-11-21 HUNT-201: a direction marker does not switch the
    /// check off for the whole part; imported wall records still do.
    #[test]
    fn a_direction_marker_does_not_switch_the_overlap_check_off() {
        let edge = |x: f64, y: f64| crate::edit::plain_edge(Point2::new(x, y), 40.0, 3.0, 0.5);
        let r = [(0.0, 0.0), (12.0, 0.0), (12.0, 8.0), (0.0, 8.0)];
        let twice: Vec<EdgeDef> = r.iter().chain(r.iter()).map(|&(x, y)| edge(x, y)).collect();
        let marker = || {
            let mut m = edge(12.0, 0.0);
            m.direction_rad = Some(0.0);
            m
        };
        let mut marked = twice.clone();
        marked.insert(1, marker());
        assert_eq!(overlapping_walls(&RoofPart::new("A", marked)), [5, 6, 7, 8]);
        let mut p = crate::examples::find("gable").unwrap().project();
        let mut marked = twice.clone();
        marked.insert(1, marker());
        p.parts = vec![RoofPart::new("A", marked)];
        assert!(p.problems().iter().any(|q| q.code == "walls_overlap"));
        // A rectangle with a marker stays fine.
        let mut one: Vec<EdgeDef> = r.iter().map(|&(x, y)| edge(x, y)).collect();
        one.insert(1, marker());
        assert!(overlapping_walls(&RoofPart::new("A", one)).is_empty());
        // Imported wall records do not switch the check off (HUNT-302).
        let mut sourced = twice.clone();
        sourced[2].source = Some(WallSource {
            id: 1,
            roof: 1,
            start: Point2::new(12.0, 8.0),
            length: 12.0,
            angle_deg: 180.0,
        });
        assert!(!overlapping_walls(&RoofPart::new("A", sourced)).is_empty());
        // A second lap a centimetre inside the first: walls side by side,
        // not on each other.
        let g = 0.01;
        let pts = [
            (0.0, 0.0),
            (12.0, 0.0),
            (12.0, 8.0),
            (0.0, 8.0),
            (g * 4.0 / (8.0 - g), 4.0),
            (g, g),
            (12.0 - g, g),
            (12.0 - g, 8.0 - g),
            (g, 8.0 - g),
        ];
        let spiral = RoofPart::new("A", pts.iter().map(|&(x, y)| edge(x, y)).collect());
        assert!(overlapping_walls(&spiral).is_empty());
    }

    /// Audit 2026-11-22 HUNT-304: a direction marker a few millimetres
    /// long, which the core snaps back to a point, is no wall drawn back:
    /// the wall after it continues the one before it (sample HPS01 without
    /// its wall records).
    #[test]
    fn a_millimetre_marker_is_no_wall_drawn_back() {
        let edge = |x: f64, y: f64, dir: Option<f64>| {
            let mut e = crate::edit::plain_edge(Point2::new(x, y), 35.0, 3.0, 0.5);
            e.direction_rad = dir;
            e
        };
        let pi = std::f64::consts::PI;
        // A wing up along x = 11 with a 2.4 mm marker pointing back down.
        let hps = RoofPart::new(
            "A",
            vec![
                edge(0.0, 0.0, None),
                edge(15.8, 0.0, None),
                edge(15.8, 12.0, None),
                edge(11.0, 12.0, None),
                edge(11.0, 14.7624, Some(pi)),
                edge(11.0, 14.76, None),
                edge(11.0, 20.0, None),
                edge(5.6, 20.0, None),
                edge(5.6, 12.0, None),
                edge(0.0, 12.0, None),
            ],
        );
        assert!(overlapping_walls(&hps).is_empty());
        // A rectangle with a 3 mm marker pointing back along its eave.
        let mm = RoofPart::new(
            "A",
            vec![
                edge(0.0, 0.0, None),
                edge(6.0, 0.0, Some(pi)),
                edge(5.997, 0.0, None),
                edge(12.0, 0.0, None),
                edge(12.0, 8.0, None),
                edge(0.0, 8.0, None),
            ],
        );
        assert!(overlapping_walls(&mm).is_empty());
        // A wall drawn back 2 cm after a marker is still one.
        let back = RoofPart::new(
            "A",
            vec![
                edge(0.0, 0.0, None),
                edge(6.0, 0.0, Some(pi)),
                edge(5.98, 0.0, None),
                edge(12.0, 0.0, None),
                edge(12.0, 8.0, None),
                edge(0.0, 8.0, None),
            ],
        );
        assert!(!overlapping_walls(&back).is_empty());
    }

    /// Audit 2026-11-21 ARCH-205 item 1: `2.0` is version 2, `1e30` a
    /// version too new (or a damaged file), `3.5` no whole number.
    #[test]
    fn a_format_version_written_as_a_float_is_read_as_its_number() {
        let mut v: serde_json::Value = serde_json::from_str(
            &crate::examples::find("gable")
                .unwrap()
                .project()
                .to_json()
                .unwrap(),
        )
        .unwrap();
        let with = |v: &mut serde_json::Value, version: &str| {
            v["schema_version"] = serde_json::from_str(version).unwrap();
            Project::from_json(&v.to_string())
        };
        assert!(with(&mut v, "2.0").is_ok());
        let e = with(&mut v, "1e30").unwrap_err().to_string();
        assert!(e.contains("newer") && !e.contains("whole number"), "{e}");
        let e = with(&mut v, "3.5").unwrap_err().to_string();
        assert!(e.contains("whole number"), "{e}");
        let e = with(&mut v, "99").unwrap_err().to_string();
        assert!(e.contains("newer Tectora"), "{e}");
    }

    #[test]
    fn check_limits_reports_the_bad_value() {
        let mut p = crate::examples::find("gable").unwrap().project();
        assert!(p.check_limits().is_ok());
        p.parts[0].edges[1].overhang = 1e300;
        let err = p.check_limits().unwrap_err();
        assert!(err.contains("parts[0].edges[1].overhang"), "{err}");
    }

    #[test]
    fn the_limits_name_a_part_by_its_index_from_0_as_mcp_does() {
        // HUNTIO-007 item 5: "part 1" for the only part, index 0.
        let mut p = crate::examples::find("gable").unwrap().project();
        let wall = p.parts[0].edges[0].clone();
        p.parts[0].edges = vec![wall; MAX_PART_WALLS + 1];
        let err = p.check_limits().unwrap_err();
        assert!(err.contains("walls in part index 0:"), "{err}");
    }

    #[test]
    fn check_limits_ignores_the_stored_result() {
        let mut p = crate::examples::find("gable").unwrap().project();
        assert!(p.set_status(JobStatus::Offer));
        assert!(p.check_limits().is_ok());
    }

    #[test]
    fn check_limits_refuses_more_parts_walls_or_objects_than_a_real_job_has() {
        // ARCH-002: 1000 parts took 77 s per calculation and a bundle-sized
        // job hung the library and the MCP server.
        let gable = crate::examples::find("gable").unwrap().project();
        let mut p = gable.clone();
        let part = p.parts[0].clone();
        p.parts = vec![part.clone(); 200];
        assert!(p.check_limits().is_ok(), "200 buildings still pass");
        p.parts.push(part.clone());
        let err = p.check_limits().unwrap_err();
        assert!(err.contains("parts") && err.contains("200"), "{err}");

        let mut p = gable.clone();
        let e = p.parts[0].edges[0].clone();
        p.parts[0].edges = vec![e.clone(); 501];
        assert!(p.check_limits().unwrap_err().contains("walls"));

        let mut p = gable.clone();
        let c = Chimney {
            corners: Vec::new(),
            height: 0.0,
        };
        p.chimneys = vec![c; 501];
        assert!(p.check_limits().unwrap_err().contains("chimneys"));

        // ARCH-053: 36 000 breaks on one wall passed and took 15 s; 300 000
        // did not finish in 25 minutes.
        let mut p = gable.clone();
        let brk = |i: usize| PitchBreak {
            height: 10.0 + i as f64 * 0.01,
            pitch_deg: 30.0,
        };
        p.parts[0].edges[0].breaks = (0..MAX_WALL_BREAKS).map(brk).collect();
        assert!(
            p.check_limits().is_ok(),
            "a round dormer's breaks still pass"
        );
        p.parts[0].edges[0].breaks.push(brk(MAX_WALL_BREAKS));
        let err = p.check_limits().unwrap_err();
        assert!(err.contains("pitch breaks of one wall"), "{err}");

        let mut p = gable.clone();
        p.parts[0].edges[0].breaks = (0..MAX_WALL_BREAKS).map(brk).collect();
        p.parts = vec![p.parts[0].clone(); MAX_BREAKS / MAX_WALL_BREAKS + 1];
        let err = p.check_limits().unwrap_err();
        assert!(err.contains("pitch breaks in all walls"), "{err}");
    }

    #[test]
    fn guarded_turns_a_panic_into_an_error() {
        assert_eq!(guarded(|| 2), Ok(2));
        let err = guarded(|| -> u8 { panic!("boom") }).unwrap_err();
        assert!(err.contains("boom"), "{err}");
    }
}
