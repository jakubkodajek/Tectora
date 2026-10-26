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
    pub parts: Vec<RoofPart>,
    #[serde(default)]
    pub dormers: Vec<Dormer>,
    #[serde(default)]
    pub chimneys: Vec<Chimney>,
}

impl Project {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            name: name.into(),
            notes: String::new(),
            parts: Vec::new(),
            dormers: Vec::new(),
            chimneys: Vec::new(),
        }
    }

    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(s: &str) -> serde_json::Result<Self> {
        serde_json::from_str(s)
    }
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
}

impl EdgeDef {
    pub fn is_gable(&self) -> bool {
        self.pitch_deg >= GABLE_PITCH_DEG
    }
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chimney {
    /// Reference point in plan, m, as stored by DACAD
    /// (whether this is the centre or a corner is not verified yet).
    pub position: Point2,
    pub width: f64,
    pub depth: f64,
}
