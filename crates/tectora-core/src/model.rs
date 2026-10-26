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
}

fn is_zero(v: &u8) -> bool {
    *v == 0
}

impl EdgeDef {
    pub fn is_gable(&self) -> bool {
        self.pitch_deg >= GABLE_PITCH_DEG
    }

    /// The sloped planes of this wall from bottom to top, as
    /// `(start height, pitch)`, like DACAD's stacked roof definitions.
    /// A gable has none unless it is clipped (Krüppelwalm): then its plane
    /// starts at the half-hip height. Breaks at or below the previous
    /// plane's start, and vertical breaks, are ignored.
    pub fn planes(&self) -> Vec<(f64, f64)> {
        let mut out: Vec<(f64, f64)> = Vec::new();
        if !self.is_gable() {
            out.push((self.eave_height, self.pitch_deg));
        } else if let Some(h) = &self.half_hip
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
            if b.height > last + 1e-6 && b.pitch_deg > 0.0 && b.pitch_deg < GABLE_PITCH_DEG {
                out.push((b.height, b.pitch_deg));
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
