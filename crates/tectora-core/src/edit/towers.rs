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
            t.tiers = vec![
                tier(0.25 * w, 1.15 * w, 0.0),
                tier(0.45 * w, 0.25 * w, 0.2 * w),
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

/// Adds `tower` (`index` `None`) or replaces tower `index`; refuses a
/// tower with problems, with the first one's English message. Returns the
/// tower's index.
pub fn set_tower(
    project: &mut Project,
    index: Option<usize>,
    tower: Tower,
) -> Result<usize, String> {
    if let Some((code, msg)) = tower.problems().first() {
        return Err(format!("{msg} ({code})"));
    }
    let mut tower = tower;
    tower.label = super::clean_line(&tower.label, 200);
    match index {
        None => {
            if project.towers.len() >= crate::tower::MAX_TOWERS {
                return Err(format!(
                    "a job has at most {} towers",
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
            None => Err(format!("tower {i} does not exist")),
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
}
