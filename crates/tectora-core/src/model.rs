//! Roof project model — the single source of truth saved as `.tectora` JSON.

use serde::{Deserialize, Serialize};

use crate::geom::{Point2, signed_area};

/// Current version of the `.tectora` JSON schema.
pub const SCHEMA_VERSION: u32 = 1;

/// Pitch at or above this value (degrees) means a vertical gable wall.
pub const GABLE_PITCH_DEG: f64 = 89.999;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
}

impl Project {
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
        }
    }

    /// Checks the input the roof engine cannot work with: non-finite
    /// numbers, parts with fewer than three edges, pitches outside
    /// (0°, 180°) and negative overhangs. Returns one message per problem.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        for (pi, part) in self.parts.iter().enumerate() {
            let at = format!("part {} ({})", pi + 1, part.name);
            if part.edges.len() < 3 {
                errors.push(format!("{at}: needs at least 3 edges"));
            }
            for (ei, e) in part.edges.iter().enumerate() {
                errors.extend(e.problems().map(|p| format!("{at}, edge {}: {p}", ei + 1)));
            }
        }
        for (di, d) in self.dormers.iter().enumerate() {
            if !finite_point(d.end) {
                errors.push(format!("dormer {}: end point is not a number", di + 1));
            }
            for (ei, e) in d.edges.iter().enumerate() {
                errors.extend(
                    e.problems()
                        .map(|p| format!("dormer {}, edge {}: {p}", di + 1, ei + 1)),
                );
            }
        }
        for (ci, c) in self.chimneys.iter().chain(&self.roof_openings).enumerate() {
            if c.corners.len() < 3 || !c.corners.iter().all(|&v| finite_point(v)) {
                errors.push(format!(
                    "chimney/opening {}: needs 3 or more finite corners",
                    ci + 1
                ));
            }
        }
        for (wi, w) in self.windows.iter().enumerate() {
            let ok = [w.x, w.y, w.width, w.height].iter().all(|v| v.is_finite())
                && w.width > 0.0
                && w.height > 0.0;
            if !ok {
                errors.push(format!(
                    "window {}: needs a finite position and a positive size",
                    wi + 1
                ));
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(s: &str) -> serde_json::Result<Self> {
        serde_json::from_str(s)
    }
}

/// Customer and job details of a design, like the job header in DACAD lists.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct JobInfo {
    /// Job or offer number.
    pub number: String,
    pub customer: String,
    pub address: String,
    pub status: JobStatus,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// The DACAD wall record this edge was imported from. DACAD's core
    /// takes each wall as recorded (its own length and direction, in file
    /// order), which matters where the walls of a file do not quite meet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<WallSource>,
}

/// A wall record of an imported DACAD file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

fn finite_point(p: Point2) -> bool {
    p.x.is_finite() && p.y.is_finite()
}

fn is_false(v: &bool) -> bool {
    !*v
}

fn is_zero(v: &u8) -> bool {
    *v == 0
}

impl EdgeDef {
    /// What makes this edge unusable for the engine (see [`Project::validate`]).
    pub(crate) fn problems(&self) -> impl Iterator<Item = &'static str> {
        let stacked = self
            .half_hip
            .iter()
            .map(|h| (h.height, h.pitch_deg))
            .chain(self.breaks.iter().map(|b| (b.height, b.pitch_deg)));
        let checks = [
            (!finite_point(self.start), "start point is not a number"),
            (
                !(self.pitch_deg > 0.0 && self.pitch_deg < 180.0),
                "pitch must be between 0° and 180°",
            ),
            (!self.eave_height.is_finite(), "eave height is not a number"),
            (
                !(self.overhang >= 0.0 && self.overhang.is_finite()),
                "overhang must be zero or positive",
            ),
            (
                self.direction_rad.is_some_and(|a| !a.is_finite()),
                "direction is not a number",
            ),
            (
                stacked
                    .into_iter()
                    .any(|(h, p)| !h.is_finite() || !p.is_finite()),
                "half-hip or pitch break is not a number",
            ),
        ];
        checks.into_iter().filter(|c| c.0).map(|c| c.1)
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PitchBreak {
    /// Height of the break line, m above ground.
    pub height: f64,
    pub pitch_deg: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HalfHip {
    /// Height at which the hipped part starts, m above ground.
    pub height: f64,
    pub pitch_deg: f64,
}

/// A dormer stored as its open wall polyline (front edge plus cheeks).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoofWindow {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
