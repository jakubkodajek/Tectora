//! Adding and changing towers ([`crate::tower`]): the editor's tower dialog
//! and the MCP tool `set_tower` both go through [`set_tower`], and both
//! start from the same profiles ([`preset`]).

use serde::{Deserialize, Serialize};

use crate::geom::Point2;
use crate::model::Project;
use crate::tower::{DEFAULT_TOWER_MATERIAL, Tower, TowerShape, TowerTier};

/// The starting profiles the dialog and the MCP offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TowerPreset {
    /// Square shaft, pyramid roof.
    Pyramid,
    /// Octagonal shaft, octagonal spire.
    Octagon,
    /// Round shaft, cone.
    Cone,
    /// Square shaft, concave bell-shaped helm ending in a spire.
    Helm,
    /// Round shaft, onion dome ending in a spire.
    Onion,
    /// Square shaft with rounded corners: bell helm, onion, lantern,
    /// small onion and spire (a Baroque church or town-hall tower).
    Baroque,
}

pub const TOWER_PRESETS: [TowerPreset; 6] = [
    TowerPreset::Pyramid,
    TowerPreset::Octagon,
    TowerPreset::Cone,
    TowerPreset::Helm,
    TowerPreset::Onion,
    TowerPreset::Baroque,
];

fn tier(rise: f64, top_width: f64, bulge: f64) -> TowerTier {
    TowerTier {
        rise: r3(rise),
        top_width: r3(top_width),
        bulge: r3(bulge),
        material: DEFAULT_TOWER_MATERIAL,
    }
}

fn r3(v: f64) -> f64 {
    super::round_to(v, 3)
}

/// A tower of `kind` on `center`, `width` wide, its roof starting at
/// `base_height`; the profile in proportion to the width. [no source —
/// typical proportions of such roofs, a start to edit, not a standard]
pub fn preset(kind: TowerPreset, center: Point2, width: f64, base_height: f64) -> Tower {
    let w = width;
    let mut t = Tower::pyramid(center, r3(w), r3(base_height), r3(0.75 * w));
    match kind {
        TowerPreset::Pyramid => {}
        TowerPreset::Octagon => {
            t.shape = TowerShape::Polygon;
            t.sides = 8;
            t.tiers = vec![tier(1.5 * w, 0.0, 0.0)];
        }
        TowerPreset::Cone => {
            t.shape = TowerShape::Circle;
            t.tiers = vec![tier(1.25 * w, 0.0, 0.0)];
        }
        TowerPreset::Helm => {
            t.tiers = vec![tier(0.6 * w, 0.3 * w, -0.06 * w), tier(0.9 * w, 0.0, 0.0)];
        }
        TowerPreset::Onion => {
            t.shape = TowerShape::Circle;
            // The bulb in two curved tiers (a single arc would rise above
            // its top: audit MATH-007).
            t.tiers = vec![
                tier(0.25 * w, 1.15 * w, 0.0),
                tier(0.25 * w, 0.75 * w, 0.06 * w),
                tier(0.25 * w, 0.25 * w, 0.06 * w),
                tier(0.5 * w, 0.0, 0.0),
            ];
        }
        TowerPreset::Baroque => {
            t.corner_radius = r3(0.18 * w);
            t.tiers = vec![
                tier(0.3 * w, 0.65 * w, -0.03 * w),
                tier(0.15 * w, 0.72 * w, 0.02 * w),
                tier(0.3 * w, 0.35 * w, 0.06 * w),
                tier(0.3 * w, 0.35 * w, 0.0),
                tier(0.12 * w, 0.42 * w, 0.0),
                tier(0.25 * w, 0.2 * w, 0.05 * w),
                tier(0.4 * w, 0.0, 0.0),
            ];
        }
    }
    t
}

/// The code that ends the refusal of a tower for a job that has
/// [`crate::tower::MAX_TOWERS`] already: no value of the tower is wrong
/// (audit ERR-004).
pub const TOWER_LIMIT: &str = "tower_limit";
/// The code that ends the refusal of a tower index the job does not have.
pub const TOWER_MISSING: &str = "tower_missing";
/// The code that ends the refusal of a tower without problems that still
/// gives no roof (`commands::tower_preview`).
pub const TOWER_NOT_CALCULATED: &str = "tower_not_calculated";
/// The code that ends the refusal of a tower whose tiers would take the
/// job over [`crate::tower::MAX_TIERS_TOTAL`] tiers of all towers (audit r2
/// ARCH-002: it came as the general "too many objects").
pub const TOWER_TIERS_LIMIT: &str = "tower_tiers_limit";
/// The code that ends the refusal of a tower label too long or with a
/// line break or control character (audit r2 ARCH-004: it was cut
/// silently, where a part name is refused).
pub const TOWER_LABEL: &str = "tower_label";

/// The code that ends the refusal of a tier pitch that sets no rise: the
/// tier keeps its width (a lantern), the pitch is not over 0° and under
/// 180° or not 90° when the tier narrows, or the rise would be over the
/// limit (audit r2 UX-010).
pub const TOWER_TIER_PITCH: &str = "tower_tier_pitch";

/// The width tier `tier` of `tower` starts at: the shaft's for the first,
/// else the tier below's top width; `None` for a tier the tower lacks.
pub fn tier_bottom_width(tower: &Tower, tier: usize) -> Option<f64> {
    match tier {
        0 => (tier < tower.tiers.len()).then_some(tower.width),
        k => tower
            .tiers
            .get(k)
            .and(tower.tiers.get(k - 1))
            .map(|t| t.top_width),
    }
}

/// The rise that gives a tier from `bottom_width` to `top_width` the pitch
/// `pitch_deg` along its chord, as `Tower::calculate` measures it (the
/// sides at half the width; over 90° a tier leaning outward). Refused
/// with [`TOWER_TIER_PITCH`] when no rise of 0 to 100 m gives it.
pub fn rise_for_pitch(bottom_width: f64, top_width: f64, pitch_deg: f64) -> Result<f64, String> {
    let refuse = || {
        format!(
            "a pitch sets the rise of a tier that narrows (0° to 90°) or widens (90° to 180°) to a rise of at most {} m ({TOWER_TIER_PITCH})",
            crate::tower::MAX_RISE
        )
    };
    let run = (bottom_width - top_width) / 2.0;
    if !(bottom_width.is_finite() && top_width.is_finite() && pitch_deg.is_finite())
        || !(pitch_deg > 0.0 && pitch_deg < 180.0)
        || (pitch_deg - 90.0).abs() < 1e-9
        || run.abs() < 1e-6
    {
        return Err(refuse());
    }
    // Checked as it is stored, to 0.1 mm: a pitch so slight its rise
    // rounds to 0 would save a flat tier (audit r3, errors To check 4).
    let rise = super::round_to(run * pitch_deg.to_radians().tan(), 4);
    if !(rise > 0.0 && rise <= crate::tower::MAX_RISE) {
        return Err(refuse());
    }
    Ok(rise)
}

/// The job's towers against [`crate::tower::MAX_TOWERS`] and
/// [`crate::tower::MAX_TIERS_TOTAL`], refused with [`TOWER_LIMIT`] or
/// [`TOWER_TIERS_LIMIT`] as `set_tower` refuses one tower: what a copy of a
/// building or of towers (`transform_part`, `transform_items`, the editor's
/// duplicate) checks before the general count limit (audit r2: it came as
/// "too many objects").
pub fn check_tower_counts(project: &Project) -> Result<(), String> {
    if project.towers.len() > crate::tower::MAX_TOWERS {
        return Err(format!(
            "a job has at most {} towers; this change would make {} ({TOWER_LIMIT})",
            crate::tower::MAX_TOWERS,
            project.towers.len()
        ));
    }
    let tiers: usize = project.towers.iter().map(|t| t.tiers.len()).sum();
    if tiers > crate::tower::MAX_TIERS_TOTAL {
        return Err(format!(
            "a job has at most {} tiers in all its towers; this change would make {tiers} ({TOWER_TIERS_LIMIT})",
            crate::tower::MAX_TIERS_TOTAL
        ));
    }
    Ok(())
}

/// Adds `tower` (`index` `None`) or replaces tower `index`; refuses a
/// tower with problems, with the first one's English message. Returns the
/// tower's index. Every refusal ends with its code in parentheses (a
/// problem's, [`TOWER_LIMIT`], [`TOWER_TIERS_LIMIT`], [`TOWER_LABEL`] or
/// [`TOWER_MISSING`]), which the app translates by.
pub fn set_tower(
    project: &mut Project,
    index: Option<usize>,
    tower: Tower,
) -> Result<usize, String> {
    if let Some((code, msg)) = tower.problems().first() {
        return Err(format!("{msg} ({code})"));
    }
    // A label the tower already had stays (a job received with one is
    // cleaned on import); a new one is checked as a part name is.
    let old = index.and_then(|i| project.towers.get(i));
    if old.is_none_or(|o| o.label != tower.label) {
        super::check_label("the tower label", &tower.label)
            .map_err(|e| format!("{e} ({TOWER_LABEL})"))?;
    }
    let others: usize = project
        .towers
        .iter()
        .enumerate()
        .filter(|&(k, _)| Some(k) != index)
        .map(|(_, t)| t.tiers.len())
        .sum();
    if others + tower.tiers.len() > crate::tower::MAX_TIERS_TOTAL {
        return Err(format!(
            "a job has at most {} tiers in all its towers; this tower would make {} ({TOWER_TIERS_LIMIT})",
            crate::tower::MAX_TIERS_TOTAL,
            others + tower.tiers.len()
        ));
    }
    let mut tower = tower;
    tower.label = super::clean_line(&tower.label, super::MAX_LABEL_CHARS);
    match index {
        None => {
            if project.towers.len() >= crate::tower::MAX_TOWERS {
                return Err(format!(
                    "a job has at most {} towers ({TOWER_LIMIT})",
                    crate::tower::MAX_TOWERS
                ));
            }
            project.towers.push(tower);
            Ok(project.towers.len() - 1)
        }
        Some(i) => match project.towers.get_mut(i) {
            Some(t) => {
                *t = tower;
                Ok(i)
            }
            None => Err(format!("tower {i} does not exist ({TOWER_MISSING})")),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_is_a_tower_without_problems_at_any_usual_size() {
        for kind in TOWER_PRESETS {
            for w in [0.8, 2.0, 4.4, 12.0] {
                let t = preset(kind, Point2::new(3.0, 4.0), w, 15.0);
                assert!(t.problems().is_empty(), "{kind:?} {w}: {:?}", t.problems());
                let r = t.calculate().unwrap();
                assert!(r.roof_area() > 0.0);
                // Every preset ends in a point: no flat top.
                assert!(r.cap.is_none(), "{kind:?}");
            }
        }
    }

    #[test]
    fn set_tower_adds_replaces_and_refuses_bad_towers() {
        let mut p = Project::new("t");
        let t = preset(TowerPreset::Pyramid, Point2::new(0.0, 0.0), 3.0, 10.0);
        assert_eq!(set_tower(&mut p, None, t.clone()), Ok(0));
        let mut t2 = t.clone();
        t2.width = 4.0;
        assert_eq!(set_tower(&mut p, Some(0), t2), Ok(0));
        assert_eq!(p.towers[0].width, 4.0);
        assert!(set_tower(&mut p, Some(3), t.clone()).is_err());
        let bad = Tower { width: -1.0, ..t };
        let e = set_tower(&mut p, None, bad).unwrap_err();
        assert!(e.contains("tower_size"), "{e}");
        assert_eq!(p.towers.len(), 1);
    }

    /// Audit ERR-004: the refusals that are not about a value end with
    /// their own stable code, as a tower's problems do, so the app can
    /// say "the job is full" instead of "change this value"; the English
    /// sentence before it stays.
    #[test]
    fn a_full_job_and_a_missing_tower_are_refused_with_their_own_codes() {
        let mut p = Project::new("t");
        let t = preset(TowerPreset::Pyramid, Point2::new(0.0, 0.0), 3.0, 10.0);
        for _ in 0..crate::tower::MAX_TOWERS {
            set_tower(&mut p, None, t.clone()).unwrap();
        }
        let e = set_tower(&mut p, None, t.clone()).unwrap_err();
        assert!(e.starts_with("a job has at most 50 towers"), "{e}");
        assert!(e.ends_with(&format!("({TOWER_LIMIT})")), "{e}");
        assert_eq!(p.towers.len(), crate::tower::MAX_TOWERS);
        let e = set_tower(&mut p, Some(77), t).unwrap_err();
        assert!(e.starts_with("tower 77 does not exist"), "{e}");
        assert!(e.ends_with(&format!("({TOWER_MISSING})")), "{e}");
    }

    #[test]
    fn a_received_tower_label_is_cleaned_like_a_part_name() {
        // Audit ARCH-002.
        let mut p = Project::new("t");
        let mut t = preset(TowerPreset::Cone, Point2::new(0.0, 0.0), 2.0, 9.0);
        t.label = format!("a\nb\u{202e}c{}", "x".repeat(5000));
        p.towers.push(t);
        crate::edit::clean_received(&mut p, "t");
        let l = &p.towers[0].label;
        assert!(!l.contains('\n') && !l.contains('\u{202e}'), "{l:?}");
        assert!(
            l.chars().count() <= super::super::MAX_LABEL_CHARS,
            "{}",
            l.chars().count()
        );
        assert!(crate::edit::check_texts_change(None, &p).is_ok());
    }

    /// Audit r2 UX-010: the rise from a pitch gives the tier that pitch in
    /// the tower's own calculation; a pitch that sets no rise is refused.
    #[test]
    fn a_tier_pitch_sets_the_rise_that_gives_it() {
        let mut t = preset(TowerPreset::Cone, Point2::new(0.0, 0.0), 8.0, 10.0);
        t.tiers.truncate(1);
        t.tiers[0].bulge = 0.0;
        for pitch in [20.0, 43.2, 60.0, 75.0] {
            let rise = rise_for_pitch(
                tier_bottom_width(&t, 0).unwrap(),
                t.tiers[0].top_width,
                pitch,
            )
            .unwrap();
            t.tiers[0].rise = rise;
            let r = t.calculate().unwrap();
            assert!(
                (r.tiers[0].pitch_deg - pitch).abs() < 0.01,
                "{pitch}: {}",
                r.tiers[0].pitch_deg
            );
        }
        // A cornice leaning outward: from 2 m to 3 m wide at 120°.
        let rise = rise_for_pitch(2.0, 3.0, 120.0).unwrap();
        assert!(
            (rise - 0.5 * 60f64.to_radians().tan()).abs() < 1e-4,
            "{rise}"
        );
        for (b, top, p) in [
            (2.0, 2.0, 45.0),
            (4.0, 0.0, 90.0),
            (4.0, 0.0, 0.0),
            (4.0, 0.0, 120.0),
            (2.0, 3.0, 30.0),
            (4.0, 0.0, 89.999),
            (4.0, 0.0, f64::NAN),
        ] {
            let e = rise_for_pitch(b, top, p).unwrap_err();
            assert!(
                e.ends_with(&format!("({TOWER_TIER_PITCH})")),
                "{b} {top} {p}: {e}"
            );
        }
        assert_eq!(tier_bottom_width(&t, 0), Some(8.0));
        assert_eq!(tier_bottom_width(&t, 1), None);
    }

    /// Audit r2 ARCH-002: a tower that takes the job over the tiers all its
    /// towers may have is refused with its own code before anything else is
    /// checked, and replacing a tower counts the tiers it replaces.
    #[test]
    fn the_tier_limit_of_a_job_is_refused_with_its_own_code() {
        let mut p = Project::new("t");
        let t = preset(TowerPreset::Baroque, Point2::new(0.0, 0.0), 1.0, 10.0);
        let per = t.tiers.len();
        let fit = crate::tower::MAX_TIERS_TOTAL / per;
        for _ in 0..fit {
            set_tower(&mut p, None, t.clone()).unwrap();
        }
        let e = set_tower(&mut p, None, t.clone()).unwrap_err();
        assert!(e.ends_with(&format!("({TOWER_TIERS_LIMIT})")), "{e}");
        assert!(e.contains("120"), "{e}");
        assert_eq!(p.towers.len(), fit);
        // The same tower again in place of the first fits.
        assert_eq!(set_tower(&mut p, Some(0), t), Ok(0));
    }

    /// Audit r2 ARCH-004: a tower label with a line break or longer than a
    /// part name may be is refused as a part name is, nothing changed; a
    /// label the tower already had is kept.
    #[test]
    fn a_bad_tower_label_is_refused_like_a_part_name() {
        let mut p = Project::new("t");
        let mut t = preset(TowerPreset::Cone, Point2::new(0.0, 0.0), 2.0, 9.0);
        for bad in [
            "a\nb".to_string(),
            "x".repeat(201),
            "a\u{202e}b".to_string(),
        ] {
            t.label = bad;
            let e = set_tower(&mut p, None, t.clone()).unwrap_err();
            assert!(e.ends_with(&format!("({TOWER_LABEL})")), "{e}");
        }
        assert!(p.towers.is_empty());
        t.label = "Zvonice".into();
        assert_eq!(set_tower(&mut p, None, t.clone()), Ok(0));
        // An old label (as a received job may still have) is not refused
        // when only another value changes.
        p.towers[0].label = "x".repeat(201);
        let mut w = p.towers[0].clone();
        w.width = 3.0;
        assert_eq!(set_tower(&mut p, Some(0), w), Ok(0));
    }
}
