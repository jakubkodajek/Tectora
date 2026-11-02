//! Wall lists: plan walls with their roof definitions, in any order, chained
//! into closed roof parts and open dormer polylines the way DACAD does it
//! (reverse-engineered from DACAD's V1 project files).

use std::collections::BTreeMap;

use crate::geom::Point2;
use crate::model::{Chimney, Dormer, EdgeDef, HalfHip, PitchBreak, Project, RoofPart, RoofWindow};

/// Endpoint matching tolerance when chaining walls, m: DACAD closes
/// outlines with gaps up to a centimetre (samples 92091101, 3314_A).
const CHAIN_TOL: f64 = 0.011;

/// One plan wall with its roof definition.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanWall {
    pub id: u16,
    pub group: u16,
    pub start: Point2,
    pub length: f64,
    /// The length as the file stores it (a float, not rounded), which
    /// DACAD writes into the RD2 to five decimals.
    pub stored_length: f64,
    pub angle_rad: f64,
    pub eave_height: f64,
    pub overhang: f64,
    pub pitch_deg: f64,
    pub priority: u8,
    pub half_hip: Option<HalfHip>,
    pub breaks: Vec<PitchBreak>,
    /// The wall is a gable covered with roof material.
    pub clad: bool,
    /// DACAD's material number of the wall (0 none).
    pub material: u8,
}

impl PlanWall {
    pub fn end(&self) -> Point2 {
        Point2::new(
            self.start.x + self.length * self.angle_rad.cos(),
            self.start.y + self.length * self.angle_rad.sin(),
        )
    }

    fn edge_def(&self) -> EdgeDef {
        EdgeDef {
            start: self.start,
            pitch_deg: self.pitch_deg,
            eave_height: self.eave_height,
            overhang: self.overhang,
            half_hip: self.half_hip.clone(),
            direction_rad: (self.length <= 1e-6).then_some(self.angle_rad),
            breaks: self.breaks.clone(),
            priority: self.priority,
            clad: self.clad,
            material: self.material,
            source: Some(crate::model::WallSource {
                id: self.id,
                roof: self.group,
                start: self.start,
                length: self.stored_length,
                angle_deg: self.angle_rad.to_degrees(),
            }),
        }
    }
}

/// Walls, windows and chimneys of a project before chaining.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WallList {
    pub walls: Vec<PlanWall>,
    pub windows: Vec<RoofWindow>,
    pub chimneys: Vec<Chimney>,
    /// Roof records `0x11`: roof number and, for a dormer, the id of the
    /// wall it sits on (DACAD takes the dormer's host plane from it).
    pub roofs: Vec<(u16, u16)>,
}

/// Chains walls end-to-start. Returns (edges, end point, closed?).
/// When several walls start at the same point, the next one by id wins,
/// then zero-length marker walls, which always sit between two real walls.
fn chain(walls: &[&PlanWall]) -> Vec<(Vec<EdgeDef>, Point2, bool)> {
    let is_marker = |w: &PlanWall| w.length <= 1e-6;
    let mut used = vec![false; walls.len()];
    let mut chains = Vec::new();
    // An open chain (a dormer) must start at its first wall: one that no
    // other wall leads to.
    let head = |i: usize, used: &[bool]| {
        !(0..walls.len())
            .any(|j| j != i && !used[j] && walls[j].end().approx_eq(walls[i].start, CHAIN_TOL))
    };
    while let Some(first) = (0..walls.len())
        .find(|&i| !used[i] && !is_marker(walls[i]) && head(i, &used))
        .or_else(|| (0..walls.len()).find(|&i| !used[i] && head(i, &used)))
        .or_else(|| (0..walls.len()).find(|&i| !used[i] && !is_marker(walls[i])))
        .or_else(|| used.iter().position(|u| !u))
    {
        used[first] = true;
        let origin = walls[first].start;
        let mut edges = vec![walls[first].edge_def()];
        let mut last = walls[first];
        let mut end = last.end();
        loop {
            let candidates: Vec<usize> = (0..walls.len())
                .filter(|&i| !used[i] && walls[i].start.approx_eq(end, CHAIN_TOL))
                .collect();
            let closing = end.approx_eq(origin, CHAIN_TOL) && edges.len() >= 3;
            // A marker at this corner comes before the next wall, even when
            // the file lists it elsewhere (ZWEIPULT).
            let next = candidates
                .iter()
                .copied()
                .find(|&i| walls[i].id == last.id.wrapping_add(1) && is_marker(walls[i]))
                .or_else(|| candidates.iter().copied().find(|&i| is_marker(walls[i])))
                .or_else(|| {
                    candidates
                        .iter()
                        .copied()
                        .find(|&i| walls[i].id == last.id.wrapping_add(1) && !closing)
                })
                .or_else(|| {
                    if closing {
                        None
                    } else {
                        candidates.first().copied()
                    }
                });
            match next {
                Some(i) => {
                    used[i] = true;
                    edges.push(walls[i].edge_def());
                    last = walls[i];
                    end = last.end();
                }
                None => break,
            }
        }
        let real = edges.iter().filter(|e| e.direction_rad.is_none()).count();
        if real == 0 {
            continue; // stray markers
        }
        let closed = real >= 3 && end.approx_eq(origin, CHAIN_TOL);
        chains.push((edges, end, closed));
    }
    chains
}

impl WallList {
    pub fn to_project(&self, name: impl Into<String>) -> Project {
        let mut project = Project::new(name);
        let mut groups: BTreeMap<u16, Vec<&PlanWall>> = BTreeMap::new();
        for w in &self.walls {
            groups.entry(w.group).or_default().push(w);
        }
        for (group, walls) in groups {
            // Zero-length walls are kept: they carry the direction of the
            // roof at steps between collinear walls.
            let chains = chain(&walls);
            if chains.is_empty() {
                continue;
            }
            // Closed loops are buildings, open chains dormers. Files saved
            // by DACAD 2 keep no roof numbers, so both can share a group.
            let mut k = 0;
            for (edges, end, closed) in chains {
                if closed {
                    let suffix = if k == 0 {
                        String::new()
                    } else {
                        format!(".{}", k + 1)
                    };
                    k += 1;
                    let mut part = RoofPart {
                        name: format!("Part {group}{suffix}"),
                        edges,
                    };
                    part.normalize_ccw();
                    project.parts.push(part);
                } else {
                    let host_wall = self
                        .roofs
                        .iter()
                        .find(|r| r.0 == group && r.1 != 0)
                        .and_then(|r| self.walls.iter().find(|w| w.id == r.1 && w.group != group))
                        .map(|w| [w.start, w.end()]);
                    project.dormers.push(Dormer {
                        edges,
                        end,
                        host_wall,
                    });
                }
            }
        }
        project.windows = self.windows.clone();
        project.chimneys = self.chimneys.clone();
        project
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::{FRAC_PI_2, PI};

    fn wall(
        id: u16,
        group: u16,
        x: f64,
        y: f64,
        length: f64,
        angle_rad: f64,
        pitch_deg: f64,
    ) -> PlanWall {
        PlanWall {
            id,
            group,
            start: Point2::new(x, y),
            length,
            stored_length: length,
            angle_rad,
            eave_height: 3.8,
            overhang: 0.9,
            pitch_deg,
            priority: 0,
            half_hip: None,
            breaks: Vec::new(),
            clad: false,
            material: crate::model::DEFAULT_MATERIAL,
        }
    }

    #[test]
    fn chains_closed_ccw_part_and_open_dormer() {
        let list = WallList {
            // A 10 x 6 house listed out of order, and an open dormer polyline.
            walls: vec![
                wall(1, 1, 0.0, 0.0, 10.0, 0.0, 40.0),
                wall(3, 1, 10.0, 6.0, 10.0, PI, 40.0),
                wall(2, 1, 10.0, 0.0, 6.0, FRAC_PI_2, 90.0),
                wall(4, 1, 0.0, 6.0, 6.0, -FRAC_PI_2, 90.0),
                wall(5, 2, 3.0, 0.0, 0.0, -FRAC_PI_2, 53.0),
                wall(6, 2, 3.0, 0.0, 2.3, 0.0, 90.0),
            ],
            ..WallList::default()
        };
        let p = list.to_project("Test");
        assert_eq!(p.parts.len(), 1);
        assert_eq!(p.dormers.len(), 1);
        let part = &p.parts[0];
        assert_eq!(part.edges.len(), 4);
        assert!(crate::geom::signed_area(&part.outline()) > 0.0);
        assert_eq!(part.edges.iter().filter(|e| e.is_gable()).count(), 2);
    }
}
