//! Editing operations of the roof editor, shared by the desktop app, the
//! MCP server and the CLI: dormer kinds, moving / rotating / mirroring /
//! copying parts, the round-roof arc and the calculation helpers. Each one
//! is a port of the editor's TypeScript module named in its file, so an AI
//! agent gets exactly what the user gets from the same dialog.
//!
//! The helpers return [`Patches`]: new values for some fields of some walls
//! of one part, applied with [`apply_patches`].

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::geom::Point2;
use crate::model::{EdgeDef, HalfHip, PitchBreak};

pub mod calc;
pub mod dormers;
pub mod probe;
pub mod round;
pub mod transform;

/// New values for some fields of one wall; `None` keeps the field.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EdgePatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<Point2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pitch_deg: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eave_height: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overhang: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub breaks: Option<Vec<PitchBreak>>,
    /// `Some(None)` removes the half-hip.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub half_hip: Option<Option<HalfHip>>,
}

impl EdgePatch {
    pub fn apply(&self, e: &mut EdgeDef) {
        if let Some(v) = self.start {
            e.start = v;
        }
        if let Some(v) = self.pitch_deg {
            e.pitch_deg = v;
        }
        if let Some(v) = self.eave_height {
            e.eave_height = v;
        }
        if let Some(v) = self.overhang {
            e.overhang = v;
        }
        if let Some(v) = &self.breaks {
            e.breaks = v.clone();
        }
        if let Some(v) = &self.half_hip {
            e.half_hip = v.clone();
        }
    }

    /// `other`'s fields win.
    pub fn merge(&mut self, other: &EdgePatch) {
        macro_rules! take {
            ($($f:ident),*) => {$(
                if other.$f.is_some() {
                    self.$f = other.$f.clone();
                }
            )*};
        }
        take!(start, pitch_deg, eave_height, overhang, breaks, half_hip);
    }

    /// The patch without the fields that would not change `e`.
    pub fn changes(&self, e: &EdgeDef) -> EdgePatch {
        EdgePatch {
            start: self.start.filter(|v| *v != e.start),
            pitch_deg: self.pitch_deg.filter(|v| *v != e.pitch_deg),
            eave_height: self.eave_height.filter(|v| *v != e.eave_height),
            overhang: self.overhang.filter(|v| *v != e.overhang),
            breaks: self.breaks.clone().filter(|v| *v != e.breaks),
            half_hip: self.half_hip.clone().filter(|v| *v != e.half_hip),
        }
    }

    pub fn is_empty(&self) -> bool {
        *self == EdgePatch::default()
    }
}

/// Patches by wall index within one part.
pub type Patches = BTreeMap<usize, EdgePatch>;

/// Merges patch maps; later fields win.
pub fn combine<'a>(maps: impl IntoIterator<Item = &'a Patches>) -> Patches {
    let mut out = Patches::new();
    for m in maps {
        for (k, v) in m {
            out.entry(*k).or_default().merge(v);
        }
    }
    out
}

/// Applies `patches` to the walls; returns the patches that changed something.
pub fn apply_patches(edges: &mut [EdgeDef], patches: &Patches) -> Patches {
    let mut applied = Patches::new();
    for (&k, p) in patches {
        let Some(e) = edges.get_mut(k) else { continue };
        let p = p.changes(e);
        if !p.is_empty() {
            p.apply(e);
            applied.insert(k, p);
        }
    }
    applied
}

/// Rounds to `digits` decimals, as the editor's `+v.toFixed(digits)`.
pub(crate) fn round_to(v: f64, digits: i32) -> f64 {
    let f = 10f64.powi(digits);
    (v * f).round() / f
}

/// A wall that only gives a direction (DACAD: Null-Längen-Linie).
pub(crate) fn is_marker(e: &EdgeDef) -> bool {
    e.direction_rad.is_some()
}

/// A wall with the given start and roof values and nothing else set.
pub fn plain_edge(start: Point2, pitch_deg: f64, eave_height: f64, overhang: f64) -> EdgeDef {
    EdgeDef {
        start,
        pitch_deg,
        eave_height,
        overhang,
        half_hip: None,
        breaks: Vec::new(),
        priority: 0,
        direction_rad: None,
        clad: false,
        material: crate::model::DEFAULT_MATERIAL,
        source: None,
    }
}
