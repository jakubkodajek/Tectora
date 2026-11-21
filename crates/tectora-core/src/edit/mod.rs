//! Editing operations of the roof editor, shared by the desktop app, the
//! MCP server and the CLI: dormer kinds, moving / rotating / mirroring /
//! copying parts, the round-roof arc and the calculation helpers. The editor
//! calls them (a few are still ports of a TypeScript module named in their
//! file, ADR-0005), so an AI agent gets exactly what the user gets from the
//! same dialog.
//!
//! The helpers return [`Patches`]: new values for some fields of some walls
//! of one part, applied with [`apply_patches`].

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::geom::Point2;
use crate::model::{EdgeDef, HalfHip, PitchBreak, Project};

pub mod calc;
pub mod corners;
pub mod dormers;
pub mod height;
pub mod helper;
pub mod openings;
pub mod position;
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

/// Checks the pitch typed for an eave wall (wall bar, New job, MCP
/// `update_edges` and `create_standard_roof`): at least `min` and below
/// 90°. 90° is a gable, set with the eave / gable switch; a pitch over 90°
/// (a plane leaning out, which DACAD's core takes up to 178°) is almost
/// always a typo (95 for 45) and is refused at the input (review UX-010).
/// Files that already carry one still open: the file limits
/// (`EdgeDef::problems`) keep DACAD's 178°.
pub fn check_eave_pitch(p: f64, min: f64) -> Result<(), String> {
    if !p.is_finite() || p < min {
        return Err(format!("pitch_deg {p} must be at least {min} and below 90"));
    }
    if p >= 90.0 {
        return Err(format!(
            "pitch_deg {p} must be below 90: use gable for a 90° wall; a pitch over 90° is not taken"
        ));
    }
    Ok(())
}

/// Rounds to `digits` decimals, halves away from zero; the editor rounds the
/// same way (`app/src/lib/rounding.ts`, checked by `tests/edit_parity.rs`).
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

/// The steepest pitch a hand-drawn face takes, degrees (the editor's field
/// stops here; MCP `set_manual` uses the same bound, MATH-006).
pub const MAX_MANUAL_FACE_PITCH_DEG: f64 = 89.9;

/// Refuses a hand-drawn face pitch outside `0..=89.9` and a typed line
/// length below 0 or not finite (MATH-005, MATH-006).
pub fn check_manual_face_pitch(pitch_deg: f64) -> Result<(), String> {
    if pitch_deg.is_finite() && (0.0..=MAX_MANUAL_FACE_PITCH_DEG).contains(&pitch_deg) {
        Ok(())
    } else {
        Err(format!(
            "pitch_deg must be between 0 and {MAX_MANUAL_FACE_PITCH_DEG} (got {pitch_deg})"
        ))
    }
}

/// See [`check_manual_face_pitch`]: a typed length must be a number of 0 or more.
pub fn check_manual_line_length(length: f64) -> Result<(), String> {
    if length.is_finite() && length >= 0.0 {
        Ok(())
    } else {
        Err(format!("length must be 0 or more (got {length})"))
    }
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

/// Smallest and largest side of a roof window, m (`width` and `height`).
pub const MIN_WINDOW_SIDE: f64 = 0.01;
pub const MAX_WINDOW_SIDE: f64 = 100.0;

/// Refuses a roof window that has no usable size: not finite, under 1 cm or
/// over 100 m on a side (HUNTC-003: 179 m passed `check_values`).
pub fn check_window_sizes(project: &Project) -> Result<(), String> {
    let side = MIN_WINDOW_SIDE..=MAX_WINDOW_SIDE;
    match project
        .windows
        .iter()
        .position(|w| !(side.contains(&w.width) && side.contains(&w.height)))
    {
        Some(i) => Err(format!(
            "roof window {i}: width and height must be between {MIN_WINDOW_SIDE} and {MAX_WINDOW_SIDE} m"
        )),
        None => Ok(()),
    }
}

/// The roof-window rules for every write (MCP `commit`, the app's save):
/// no window of unusable size, none larger than its face (also on a flat
/// roof). Only what the write introduces is refused: a job that already
/// carries such a window stays editable, and a write that leaves the
/// windows and the roof as they were is not calculated again.
pub fn check_windows_write(before: Option<&Project>, after: &Project) -> Result<(), String> {
    if after.windows.is_empty() {
        return Ok(());
    }
    if let Some(b) = before {
        let same_roof = b.windows == after.windows
            && b.parts == after.parts
            && b.dormers == after.dormers
            && b.chimneys == after.chimneys
            && b.roof_openings == after.roof_openings
            && b.manual_faces == after.manual_faces
            && b.flat_roof == after.flat_roof;
        if same_roof {
            return Ok(());
        }
    }
    let was_bad = |check: &dyn Fn(&Project) -> bool| before.is_some_and(check);
    if let Err(e) = check_window_sizes(after)
        && !was_bad(&|b| check_window_sizes(b).is_err())
    {
        return Err(e);
    }
    if let Err(e) = check_windows(after)
        && !was_bad(&|b| check_windows(b).is_err())
    {
        return Err(e.to_string());
    }
    Ok(())
}

/// The values of a new roof wall as every creating command takes them
/// (the app's New job and standard roof dialog, MCP `create_standard_roof`
/// and `add_shape_part`): a pitch from [`crate::model::MIN_PITCH_DEG`] to
/// below 90, an eave height and an overhang that are numbers of 0 or more
/// within the file limits (ARCH-007, UI-010).
pub fn check_new_wall(pitch_deg: f64, eave_height: f64, overhang: f64) -> Result<(), String> {
    check_eave_pitch(pitch_deg, crate::model::MIN_PITCH_DEG)?;
    for (name, v) in [("eave_height", eave_height), ("overhang", overhang)] {
        if !v.is_finite() || !(0.0..=crate::model::MAX_ABS_VALUE).contains(&v) {
            return Err(format!(
                "{name} {v} must be a number from 0 to {} m",
                crate::model::MAX_ABS_VALUE
            ));
        }
    }
    Ok(())
}

/// Shortest side of a standard roof or a regular polygon, m: the editor's
/// standard-roof form takes 0.1 m and up; a centimetre-rounded corner of a
/// smaller shape leaves walls without length (ARCH-001, HUNTC-007).
pub const MIN_STANDARD_SIDE: f64 = 0.1;
/// Longest side of a standard roof, m (the form's bound, far beyond any
/// building and inside the file limits).
pub const MAX_STANDARD_SIDE: f64 = 10_000.0;

/// The length and width of a standard roof (or the side of a regular
/// polygon, given as both) as the editor's standard-roof form bounds them,
/// for every creating command: the app's New job and standard-roof dialog
/// (Tauri `create_standard` / `standard_part`) and MCP
/// `create_standard_roof` / `add_shape_part` (ARCH-001).
pub fn check_standard_size(length: f64, width: f64) -> Result<(), String> {
    for (name, v) in [("length", length), ("width", width)] {
        if !v.is_finite() || !(MIN_STANDARD_SIDE..=MAX_STANDARD_SIDE).contains(&v) {
            // A tiny or huge value in its short form, not with hundreds
            // of digits (round 2 ARCH-106).
            let v = if v != 0.0 && !(1e-4..1e6).contains(&v.abs()) {
                format!("{v:e}")
            } else {
                v.to_string()
            };
            return Err(format!(
                "{name} {v} out of range ({MIN_STANDARD_SIDE} to {MAX_STANDARD_SIDE} m): check m against cm or mm"
            ));
        }
    }
    Ok(())
}

/// The pitch a gable wall gets when it is turned back into an eave (the
/// wall bar's Eave button, the plan's context menu, MCP `update_edges`
/// with `gable: false`): the pitch the wall had before, when the editor
/// remembers it (`remembered`), else the pitch of the nearest eave wall of
/// the same part (searching both ways, the previous wall first), else the
/// user's default pitch `preset` (ARCH-005, UI-007). Mirrored in
/// `app/src/lib/walltype.ts` and checked by `tests/edit_parity.rs`.
pub fn eave_pitch(edges: &[EdgeDef], edge: usize, remembered: Option<f64>, preset: f64) -> f64 {
    if let Some(p) = remembered.filter(|p| p.is_finite()) {
        return p;
    }
    let n = edges.len() as isize;
    for d in 1..=n / 2 {
        for k in [edge as isize - d, edge as isize + d] {
            let e = &edges[k.rem_euclid(n) as usize];
            if !e.is_gable() && !is_marker(e) {
                return e.pitch_deg;
            }
        }
    }
    preset
}

/// The ranges a wall bar field takes (eave height 0 to 1000 m, overhang 0
/// to 20 m, an eave wall's pitch from [`crate::model::MIN_PITCH_DEG`] to
/// below 90°), checked on the walls `changed` of `after`: a calculation
/// helper's OK writes no value the wall bar would refuse, whether it comes
/// from the dialog or MCP `roof_helper` (ARCH-003). A wall that was a
/// gable in `before` keeps its gable pitch.
pub fn check_wall_values(
    before: &[EdgeDef],
    after: &[EdgeDef],
    changed: impl IntoIterator<Item = usize>,
) -> Result<(), String> {
    let (eave_lo, eave_hi) = crate::settings::RoofDefaults::EAVE;
    let (over_lo, over_hi) = crate::settings::RoofDefaults::OVERHANG;
    for i in changed {
        let Some(e) = after.get(i) else { continue };
        let was_gable = before.get(i).is_some_and(|b| b.is_gable());
        if !was_gable && !is_marker(e) {
            check_eave_pitch(e.pitch_deg, crate::model::MIN_PITCH_DEG)
                .map_err(|m| format!("wall {i}: {m}"))?;
        }
        if !(e.eave_height.is_finite() && (eave_lo..=eave_hi).contains(&e.eave_height)) {
            return Err(format!(
                "wall {i}: eave_height {} out of range ({eave_lo} to {eave_hi} m)",
                e.eave_height
            ));
        }
        if !(e.overhang.is_finite() && (over_lo..=over_hi).contains(&e.overhang)) {
            return Err(format!(
                "wall {i}: overhang {} out of range ({over_lo} to {over_hi} m)",
                e.overhang
            ));
        }
        if let Some((code, why)) = e.problems().next() {
            return Err(format!("wall {i}: {code}: {why}"));
        }
    }
    Ok(())
}

/// A job name must not be empty or blank (the editor refuses it too), and
/// must not hold control characters (NUL, line breaks, tabs) or invisible
/// ones (bidirectional overrides, zero-width spaces) that change how the
/// name reads in the library, the file name and the sheets (HUNTIO-007).
pub fn check_job_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("the job name must not be empty".into());
    }
    if let Some(c) = name
        .chars()
        .find(|&c| c.is_control() || crate::store::is_invisible(c))
    {
        return Err(format!(
            "the job name must not contain control or invisible characters (U+{:04X}); use one line of plain text",
            c as u32
        ));
    }
    Ok(())
}

/// Longest summary of a write kept, in characters (ARCH-105): one
/// sentence for the user, not a document.
pub const MAX_SUMMARY_CHARS: usize = 500;

/// Longest job number, customer or address, in characters.
pub const MAX_JOB_TEXT_CHARS: usize = 500;

/// Longest job notes, in characters: well inside what a job bundle carries.
pub const MAX_NOTES_CHARS: usize = 1_000_000;

/// `text` cut to at most `max` characters, with `…` when it was cut.
pub fn capped(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// The summary of a write (every MCP write needs one, `AGENTS.md`) must
/// have a visible character, at most [`MAX_SUMMARY_CHARS`] characters and
/// no control or invisible characters (ARCH-104, ARCH-105): it is shown to
/// the user as the reason for the change.
pub fn check_summary(summary: &str) -> Result<(), String> {
    if !summary
        .chars()
        .any(|c| !c.is_whitespace() && !c.is_control() && !crate::store::is_invisible(c))
    {
        return Err("`summary` is empty: give one sentence for the user on what you change and why (it is shown in the app)".into());
    }
    if summary.chars().count() > MAX_SUMMARY_CHARS {
        return Err(format!(
            "`summary` is too long: one sentence of at most {MAX_SUMMARY_CHARS} characters"
        ));
    }
    if let Some(c) = summary
        .chars()
        .find(|&c| c.is_control() || crate::store::is_invisible(c))
    {
        return Err(format!(
            "`summary` must not contain control or invisible characters (U+{:04X}); use one line of plain text",
            c as u32
        ));
    }
    Ok(())
}

/// A job's number, customer or address (`field`): one line of plain text,
/// at most [`MAX_JOB_TEXT_CHARS`] characters, the character rule of
/// [`check_job_name`] (NUL or a bidirectional override reorders what the
/// library card shows, ARCH-104). Notes may have line breaks and tabs and
/// are capped at [`MAX_NOTES_CHARS`].
pub fn check_job_text(field: &str, text: &str) -> Result<(), String> {
    let notes = field == "notes";
    let max = if notes {
        MAX_NOTES_CHARS
    } else {
        MAX_JOB_TEXT_CHARS
    };
    if text.chars().count() > max {
        return Err(format!("{field} is too long (at most {max} characters)"));
    }
    if let Some(c) = text.chars().find(|&c| {
        (c.is_control() && !(notes && matches!(c, '\n' | '\r' | '\t')))
            || crate::store::is_invisible(c)
    }) {
        return Err(format!(
            "{field} must not contain control or invisible characters (U+{:04X})",
            c as u32
        ));
    }
    Ok(())
}

/// [`check_windows`] for a save that changes `before` into `after`, used by
/// every write path (MCP `commit`): refuses when a window no longer fits
/// its face because of the edit (a window moved onto a smaller face, the
/// flat roof turned on, a wall moved). A job that already had an oversize
/// window stays editable, so the user can fix it step by step (HUNTC-003).
pub fn check_windows_change(
    before: Option<&Project>,
    after: &Project,
) -> Result<(), OversizeWindow> {
    let Err(e) = check_windows(after) else {
        return Ok(());
    };
    match before {
        Some(b) if check_windows(b).is_err() => Ok(()),
        _ => Err(e),
    }
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
        height: None,
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
    fn a_typed_eave_pitch_over_90_is_refused_but_files_keep_dacads_range() {
        for ok in [2.0, 45.0, 89.99] {
            assert!(check_eave_pitch(ok, 2.0).is_ok(), "{ok}");
        }
        for bad in [1.9, 90.0, 95.0, 178.0, f64::NAN, f64::INFINITY] {
            assert!(check_eave_pitch(bad, 2.0).is_err(), "{bad}");
        }
        // A job saved with a leaning plane still opens (UX-010).
        let mut e = plain_edge(Point2::new(0.0, 0.0), 95.0, 3.0, 0.5);
        assert_eq!(e.problems().count(), 0);
        e.pitch_deg = 179.0;
        assert!(e.problems().count() > 0);
    }

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
    fn a_change_that_makes_a_window_too_large_is_refused_on_every_roof_kind() {
        use crate::model::RoofWindow;
        let mut before = gable_12x8();
        before.windows.push(RoofWindow {
            x: 40.0,
            y: 40.0,
            width: 10.0,
            height: 9.0,
        });
        // Beside the roof: fine. Moved onto it: refused.
        assert!(check_windows_change(Some(&before), &before).is_ok());
        let mut moved = before.clone();
        moved.windows[0].x = 1.0;
        moved.windows[0].y = 0.0;
        assert_eq!(
            check_windows_change(Some(&before), &moved)
                .unwrap_err()
                .index,
            0
        );
        // The flat roof gives a face the window does not fit either.
        let mut flat = before.clone();
        flat.flat_roof = true;
        flat.windows[0].x = 1.0;
        flat.windows[0].y = 1.0;
        flat.windows[0].width = 30.0;
        assert!(check_windows_change(Some(&before), &flat).is_err());
        // A job that was already wrong can still be edited.
        assert!(check_windows_change(Some(&moved), &moved).is_ok());
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

    /// TEST-010: clearing the roof empties every kind of item, including
    /// the hand-drawn lines and faces, and reports that it did something.
    #[test]
    fn clear_roof_removes_every_kind_of_item() {
        use crate::model::{Chimney, ManualFace, ManualLine, RoofWindow};
        let full = || {
            let mut p = gable_12x8();
            p.chimneys.push(Chimney {
                corners: vec![
                    Point2::new(1.0, 1.0),
                    Point2::new(2.0, 1.0),
                    Point2::new(2.0, 2.0),
                ],
                height: 6.0,
            });
            p.roof_openings.push(p.chimneys[0].clone());
            p.windows.push(RoofWindow {
                x: 4.0,
                y: 0.5,
                width: 1.0,
                height: 1.0,
            });
            p.dormers.push(crate::edit::dormers::build_dormer(
                &crate::edit::dormers::default_params(
                    crate::edit::dormers::DormerKind::Gable,
                    Point2::new(5.0, 1.0),
                    0.0,
                    4.0,
                ),
            ));
            p.manual_lines.push(ManualLine {
                a: [0.0; 3],
                b: [1.0, 0.0, 0.0],
                code: "T".into(),
                length: None,
                label: String::new(),
            });
            p.manual_faces.push(ManualFace {
                points: vec![
                    Point2::new(30.0, 0.0),
                    Point2::new(40.0, 0.0),
                    Point2::new(40.0, 6.0),
                ],
                pitch_deg: 30.0,
                label: String::new(),
            });
            p
        };
        let mut p = full();
        assert!(!p.parts.is_empty() && !p.dormers.is_empty() && !p.chimneys.is_empty());
        assert!(!p.windows.is_empty() && !p.roof_openings.is_empty());
        assert!(!p.manual_lines.is_empty() && !p.manual_faces.is_empty());
        assert!(clear_roof(&mut p));
        assert!(p.parts.is_empty() && p.dormers.is_empty() && p.chimneys.is_empty());
        assert!(p.windows.is_empty() && p.roof_openings.is_empty());
        assert!(p.manual_lines.is_empty(), "hand-drawn lines stay");
        assert!(p.manual_faces.is_empty(), "hand-drawn faces stay");
        assert!(!clear_roof(&mut p), "nothing left to clear");
        // A job with nothing but a hand-drawn face has something to clear.
        let mut q = full();
        q.parts.clear();
        q.dormers.clear();
        q.chimneys.clear();
        q.windows.clear();
        q.roof_openings.clear();
        q.manual_lines.clear();
        assert!(clear_roof(&mut q));
        assert!(q.manual_faces.is_empty());
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

    #[test]
    fn a_write_that_puts_a_window_on_a_flat_face_too_small_for_it_is_refused() {
        // HUNTC-003: moving the window onto the roof saved -160 m2.
        let mut p = gable_12x8();
        p.flat_roof = true;
        p.windows.push(crate::model::RoofWindow {
            x: 40.0,
            y: 40.0,
            width: 1.45,
            height: 179.0,
        });
        let off_roof = p.clone();
        assert!(check_windows_write(Some(&off_roof), &off_roof).is_ok());
        assert!(check_window_sizes(&p).is_err(), "179 m is not a window");
        p.windows[0] = crate::model::RoofWindow {
            x: 2.0,
            y: 2.0,
            width: 11.0,
            height: 10.0,
        };
        let e = check_windows_write(Some(&gable_12x8()), &p).unwrap_err();
        assert!(e.contains("roof window 0"), "{e}");
        // A job that already carries one is not locked by an unrelated edit.
        assert!(check_windows_write(Some(&p), &p).is_ok());
    }

    #[test]
    fn a_new_wall_takes_only_what_the_editor_takes() {
        assert!(check_new_wall(40.0, 3.0, 0.5).is_ok());
        for (p, e, o) in [
            (1.0, 3.0, 0.5),
            (40.0, -3.0, 0.5),
            (40.0, 3.0, -1.0),
            (40.0, f64::NAN, 0.0),
        ] {
            assert!(check_new_wall(p, e, o).is_err(), "{p} {e} {o}");
        }
        assert!(check_job_name("  ").is_err() && check_job_name("A").is_ok());
        // HUNTIO-007: one line of visible text.
        for bad in ["A\r\nB", "A\tB", "A\u{0}B", "file\u{202e}gnp", "A\u{200b}B"] {
            assert!(check_job_name(bad).is_err(), "{bad:?}");
        }
        assert!(check_job_name("Rodinný dům – garáž 6×4 m ≥ 2°").is_ok());
    }
}
