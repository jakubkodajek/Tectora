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

/// A roof window whose `width × height` is more than what is left of the
/// roof face it sits on (after the windows before it): its deduction would
/// make the face area negative.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OversizeWindow {
    /// Index of the window in `project.windows`.
    pub index: usize,
    /// The window's area, `width × height`, m².
    pub area: f64,
    /// What is left of its face before it is deducted, m².
    pub face_area: f64,
}

impl std::fmt::Display for OversizeWindow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "roof window {} ({:.2} m2) is larger than what is left of the roof face it sits on ({:.2} m2); check its width and height (m, in the roof plane) and its position",
            self.index, self.area, self.face_area
        )
    }
}

/// Refuses roof windows larger than their face, before they are saved
/// (the editor's window fields and MCP `set_openings`). DACAD deducts a
/// window's `width × height` from its face whatever its size, and the
/// calculation keeps that; a window that does not fit is a typing error
/// (cm for m, width and height swapped). A window on no roof face deducts
/// nothing and is not refused.
pub fn check_windows(project: &Project) -> Result<(), OversizeWindow> {
    if project.windows.is_empty() {
        return Ok(());
    }
    let mut bare = project.clone();
    bare.windows.clear();
    let mut geos = crate::openings::generate_project(&bare);
    for (index, w) in project.windows.iter().enumerate() {
        let Some((g, k)) = crate::openings::window_face(&geos, w) else {
            continue;
        };
        let area = w.width * w.height;
        let face = &mut geos[g].faces[k];
        if area > face.area + crate::check::OVERCUT_TOL {
            return Err(OversizeWindow {
                index,
                area,
                face_area: face.area.max(0.0),
            });
        }
        face.area -= area;
    }
    Ok(())
}

/// Turns hand-drawn face `index` into a building with the same outline:
/// the face (which only adds its area and calculates no roof) is removed
/// and a part named `name` takes its place, every wall an eave with the
/// given pitch, eave height and overhang, so its roof is calculated. One
/// change, as the editor's "Make it a building". Returns the new part's
/// index.
pub fn face_to_part(
    project: &mut Project,
    index: usize,
    name: &str,
    pitch_deg: f64,
    eave_height: f64,
    overhang: f64,
) -> Result<usize, String> {
    let face = project
        .manual_faces
        .get(index)
        .ok_or_else(|| format!("no hand-drawn face {index}"))?;
    if face.points.len() < 3 {
        return Err(format!("hand-drawn face {index} has fewer than 3 corners"));
    }
    let mut part = crate::model::RoofPart {
        name: name.to_string(),
        edges: face
            .points
            .iter()
            .map(|&p| plain_edge(p, pitch_deg, eave_height, overhang))
            .collect(),
    };
    part.normalize_ccw();
    project.manual_faces.remove(index);
    project.parts.push(part);
    Ok(project.parts.len() - 1)
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

    fn gable_12x8() -> Project {
        let mut p = Project::new("gable");
        p.parts.push(crate::examples::standard_roof(
            crate::examples::StandardRoof::Gable,
            12.0,
            8.0,
            40.0,
            3.0,
            0.0,
        ));
        p
    }

    #[test]
    fn a_window_larger_than_its_face_is_refused() {
        use crate::model::RoofWindow;
        let mut p = gable_12x8();
        p.windows.push(RoofWindow {
            x: 1.0,
            y: 0.0,
            width: 0.78,
            height: 1.18,
        });
        assert_eq!(check_windows(&p), Ok(()));
        // 10 × 9 m on a face of 12 × 5.22 m (MATH-001).
        p.windows.push(RoofWindow {
            x: 1.0,
            y: 0.0,
            width: 10.0,
            height: 9.0,
        });
        let e = check_windows(&p).unwrap_err();
        assert_eq!(e.index, 1);
        assert!((e.area - 90.0).abs() < 1e-9);
        assert!(e.face_area > 60.0 && e.face_area < 63.0, "{}", e.face_area);
        assert!(e.to_string().contains("roof window 1"));
        // Windows that fit one by one but not together.
        p.windows[1] = RoofWindow {
            x: 3.0,
            y: 0.0,
            width: 8.0,
            height: 7.0,
        };
        assert!(check_windows(&p).is_ok());
        p.windows.push(RoofWindow {
            x: 4.0,
            y: 0.0,
            width: 2.0,
            height: 5.0,
        });
        assert_eq!(check_windows(&p).unwrap_err().index, 2);
    }

    #[test]
    fn a_window_beside_the_roof_is_not_refused() {
        let mut p = gable_12x8();
        p.windows.push(crate::model::RoofWindow {
            x: 40.0,
            y: 40.0,
            width: 10.0,
            height: 10.0,
        });
        assert!(check_windows(&p).is_ok());
    }

    #[test]
    fn a_face_becomes_a_building_in_one_change() {
        use crate::model::ManualFace;
        let mut p = Project::new("face");
        let pt = Point2::new;
        // Clockwise, as a face may be drawn.
        p.manual_faces.push(ManualFace {
            points: vec![pt(0.0, 0.0), pt(0.0, 8.0), pt(12.0, 8.0), pt(12.0, 0.0)],
            pitch_deg: 30.0,
            label: String::new(),
        });
        assert!(crate::openings::generate_project(&p).is_empty());
        let i = face_to_part(&mut p, 0, "Main", 40.0, 3.0, 0.5).unwrap();
        assert_eq!(i, 0);
        assert!(p.manual_faces.is_empty());
        assert_eq!(p.parts[0].name, "Main");
        assert!(crate::geom::signed_area(&p.parts[0].outline()) > 0.0);
        assert!(
            p.parts[0]
                .edges
                .iter()
                .all(|e| e.pitch_deg == 40.0 && e.overhang == 0.5)
        );
        let geo = &crate::openings::generate_project(&p)[0];
        assert!(geo.error.is_none() && !geo.faces.is_empty());
        assert!(face_to_part(&mut p, 0, "x", 40.0, 3.0, 0.5).is_err());
    }
}
