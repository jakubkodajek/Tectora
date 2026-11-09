//! Roof project model — the single source of truth saved as `.tectora` JSON.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::geom::{Point2, signed_area};

/// Current version of the `.tectora` JSON schema.
pub const SCHEMA_VERSION: u32 = 2;

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
            }
        }
        for (ci, c) in self.roof_openings.iter().enumerate() {
            if corners(c) {
                out.push(Problem::of(ProblemItem::Opening, ci, "bad_corners"));
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
        let n = p.index + 1;
        let at = match p.item {
            ProblemItem::Part => format!(
                "part {n} ({})",
                self.parts
                    .get(p.index)
                    .map(|x| x.name.as_str())
                    .unwrap_or("")
            ),
            ProblemItem::Dormer => format!("dormer {n}"),
            ProblemItem::Chimney => format!("chimney {n}"),
            ProblemItem::Opening => format!("roof opening {n}"),
            ProblemItem::Window => format!("window {n}"),
        };
        let what = EdgeDef::PROBLEMS
            .iter()
            .find(|(c, _)| *c == p.code)
            .map(|(_, m)| *m)
            .unwrap_or(match p.code {
                "too_few_edges" => "needs at least 3 edges",
                "end_nan" => "end point is not a number or out of range",
                "bad_corners" => "needs 3 or more finite corners",
                "bad_size" => "needs a finite position and a positive size",
                other => other,
            });
        match p.edge {
            Some(e) => format!("{at}, edge {}: {what}", e + 1),
            None => format!("{at}: {what}"),
        }
    }

    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
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
        let mut checks = vec![
            over("parts", self.parts.len(), MAX_PARTS),
            over("walls in all parts", walls, MAX_WALLS),
            over("dormers", self.dormers.len(), MAX_OBJECTS),
            over("chimneys", self.chimneys.len(), MAX_OBJECTS),
            over("roof windows", self.windows.len(), MAX_OBJECTS),
            over("roof openings", self.roof_openings.len(), MAX_OBJECTS),
            over("hand-drawn lines", self.manual_lines.len(), MAX_OBJECTS),
            over("hand-drawn faces", self.manual_faces.len(), MAX_OBJECTS),
        ];
        for (i, part) in self.parts.iter().enumerate() {
            checks.push(over(
                &format!("walls in part {}", i + 1),
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
        match first_out_of_range(&value, &mut String::new()) {
            Some((path, v)) => Err(format!(
                "value {v} at `{path}` is out of range (allowed ±{MAX_ABS_VALUE})"
            )),
            None => Ok(()),
        }
    }
}

/// Largest absolute number accepted anywhere in a design: 100 km in
/// metres, far above any building and far below where f64 geometry breaks.
pub const MAX_ABS_VALUE: f64 = 1.0e5;

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
pub fn guarded<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).map_err(|e| {
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
    /// `stacked_nan`, `break_below_eave`, `break_duplicate`, `end_nan`,
    /// `bad_corners` or `bad_size`.
    pub code: &'static str,
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
    let version = obj
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(1);
    if version > u64::from(SCHEMA_VERSION) {
        return Err(serde_json::Error::custom(format!(
            "the file was saved by a newer Tectora (format {version}, this version reads up to {SCHEMA_VERSION})"
        )));
    }
    // 1 -> 2: adds the optional `result_snapshot`; nothing to convert.
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
}

impl RoofPart {
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
    pub const PROBLEMS: [(&'static str, &'static str); 9] = [
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
            !pitch_ok(self.pitch_deg) || self.breaks.iter().any(|b| !pitch_ok(b.pitch_deg)),
            !sane(self.eave_height),
            self.eave_height < 0.0,
            !(self.overhang >= 0.0 && sane(self.overhang)),
            self.direction_rad.is_some_and(|a| !a.is_finite()),
            stacked.into_iter().any(|(h, p)| !sane(h) || !p.is_finite()),
            breaks_low,
            breaks_dup,
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
    }

    #[test]
    fn guarded_turns_a_panic_into_an_error() {
        assert_eq!(guarded(|| 2), Ok(2));
        let err = guarded(|| -> u8 { panic!("boom") }).unwrap_err();
        assert!(err.contains("boom"), "{err}");
    }
}
