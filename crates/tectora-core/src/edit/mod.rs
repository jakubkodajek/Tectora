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
use crate::model::{EdgeDef, HalfHip, PitchBreak, Project};

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

/// Removes everything drawn (parts, dormers, chimneys, roof windows, roof
/// openings, hand-drawn lines and faces) and leaves an empty canvas, as the
/// editor's Clear. The job data, notes, roof options and the stored result
/// stay. Returns whether anything was removed.
pub fn clear_roof(project: &mut Project) -> bool {
    let had = !(project.parts.is_empty()
        && project.dormers.is_empty()
        && project.chimneys.is_empty()
        && project.windows.is_empty()
        && project.roof_openings.is_empty()
        && project.manual_lines.is_empty()
        && project.manual_faces.is_empty());
    project.parts.clear();
    project.dormers.clear();
    project.chimneys.clear();
    project.windows.clear();
    project.roof_openings.clear();
    project.manual_lines.clear();
    project.manual_faces.clear();
    had
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Chimney, ManualLine};

    #[test]
    fn clear_roof_leaves_an_empty_canvas_and_keeps_the_job() {
        let mut p = crate::examples::find("hip").expect("example").project();
        p.job.customer = "Novák".into();
        p.notes = "note".into();
        p.material_names = vec!["Tiles".into()];
        p.chimneys.push(Chimney {
            corners: Vec::new(),
            height: 0.0,
        });
        p.manual_lines.push(ManualLine {
            a: [0.0; 3],
            b: [1.0, 0.0, 0.0],
            code: "M".into(),
            length: None,
            label: String::new(),
        });
        assert!(clear_roof(&mut p));
        assert!(p.parts.is_empty() && p.chimneys.is_empty() && p.manual_lines.is_empty());
        assert!(p.dormers.is_empty() && p.windows.is_empty() && p.roof_openings.is_empty());
        assert!(p.manual_faces.is_empty());
        assert_eq!(p.job.customer, "Novák");
        assert_eq!(p.notes, "note");
        assert_eq!(p.material_names, vec!["Tiles".to_string()]);
        assert!(p.validate().is_ok());
        // Nothing left to remove.
        assert!(!clear_roof(&mut p));
    }
}
