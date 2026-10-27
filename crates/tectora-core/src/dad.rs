//! Importer for DACAD `.DAD` roof files.
//!
//! Only the V1.x format (files starting with `DACAD\x1a`) is decoded so far.
//! It is a flat stream of records `[sub: u8][tag: u8][len: u16 LE][payload]`
//! terminated by `ff ff`. The records Tectora uses:
//!
//! * tag `0x12`, sub `0x00` (42 bytes) — one wall line with its roof definition:
//!   `id u16, group u16, f32 x4 [x, y, length, angle_rad], u16 ?,
//!   u16 material, f32 x4 [eave_height, overhang, pitch_deg, priority],
//!   flags u16`. Bit 0 of `flags` means a half-hip
//!   record follows; the high byte is the number of window records that follow.
//! * tag `0x12`, sub `0x10` (16 bytes) — a further (stacked) roof definition
//!   of the preceding wall: `u16 ?, u16 material, f32 height, f32 overhang,
//!   f32 pitch_deg`. On a gable it is the Krüppelwalm, otherwise a pitch
//!   break.
//! * tag `0x13`, sub `0x00` (8 bytes) — chimney: `u16 id, f32 height,
//!   u16 corner count`, followed by that many sub `0x10` records `f32 x, y`.
//! * tag `0x12`, sub `0x20` (16 bytes) — roof window: `f32 x, y, width, height`.
//!
//! Walls of one group that chain into a closed loop form a roof part; open
//! chains are dormers. The newer `DACADV2` format (MFC archive) is detected
//! and reported as unsupported.

use std::collections::BTreeMap;

use thiserror::Error;

use crate::geom::Point2;
use crate::model::{Chimney, Dormer, EdgeDef, HalfHip, PitchBreak, Project, RoofPart, RoofWindow};

const V1_MAGIC: &[u8] = b"DACAD\x1a";
const V2_MAGIC: &[u8] = b"\x08DACADV2\x1a";
const TAG_ROOF: u8 = 0x11;
const TAG_WALL: u8 = 0x12;
const TAG_CHIMNEY: u8 = 0x13;
const SUB_WALL: u8 = 0x00;
const SUB_STACKED: u8 = 0x10;
const SUB_WINDOW: u8 = 0x20;
/// Endpoint matching tolerance when chaining walls, m: DACAD closes
/// outlines with gaps up to a centimetre (samples 92091101, 3314_A).
const CHAIN_TOL: f64 = 0.011;

#[derive(Debug, Error, PartialEq)]
pub enum DadError {
    #[error("not a DACAD file")]
    NotDacad,
    #[error("DACAD V2 files are not supported yet")]
    UnsupportedV2,
    #[error("truncated record at byte {0}")]
    Truncated(usize),
    #[error("record at byte {0} holds a number that is not finite")]
    NotFinite(usize),
    #[error("wall record at byte {offset} has unexpected length {len}")]
    BadWallRecord { offset: usize, len: usize },
}

/// A wall line exactly as stored in the file.
#[derive(Debug, Clone, PartialEq)]
pub struct DadWall {
    pub id: u16,
    pub group: u16,
    pub start: Point2,
    pub length: f64,
    pub angle_rad: f64,
    pub eave_height: f64,
    pub overhang: f64,
    pub pitch_deg: f64,
    pub priority: u8,
    pub half_hip: Option<HalfHip>,
    pub breaks: Vec<PitchBreak>,
    /// The wall is a gable covered with roof material.
    pub clad: bool,
}

impl DadWall {
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
            source: Some(crate::model::WallSource {
                id: self.id,
                roof: self.group,
                start: self.start,
                length: self.length,
                angle_deg: self.angle_rad.to_degrees(),
            }),
        }
    }
}

/// Raw content of a V1 file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DadFile {
    pub version: String,
    pub walls: Vec<DadWall>,
    pub windows: Vec<RoofWindow>,
    pub chimneys: Vec<Chimney>,
    /// Roof records `0x11`: roof number and, for a dormer, the id of the
    /// wall it sits on (DACAD takes the dormer's host plane from it).
    pub roofs: Vec<(u16, u16)>,
}

/// A gable wall; a pitch over 90° is a plane leaning outward (C71).
fn is_vertical(pitch_deg: f64) -> bool {
    (pitch_deg - 90.0).abs() <= 90.0 - crate::model::GABLE_PITCH_DEG
}

fn f32_at(b: &[u8], off: usize) -> f64 {
    let v = f32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]]);
    // Values are stored as f32; round to 0.1 mm to drop float noise.
    (f64::from(v) * 1e4).round() / 1e4
}

fn angle_at(b: &[u8], off: usize) -> f64 {
    let v = f32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]]);
    // An f32 angle is off by up to 4e-8 rad, which a zero-length dormer
    // side carries far back (TRAING01: -90° lost the dormers); round to
    // 1e-4 degrees.
    ((f64::from(v).to_degrees() * 1e4).round() / 1e4).to_radians()
}

fn u16_at(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([b[off], b[off + 1]])
}

pub fn parse(bytes: &[u8]) -> Result<DadFile, DadError> {
    if bytes.starts_with(V2_MAGIC) {
        return Err(DadError::UnsupportedV2);
    }
    if !bytes.starts_with(V1_MAGIC) {
        return Err(DadError::NotDacad);
    }
    let mut file = DadFile::default();
    let mut p = V1_MAGIC.len();
    while p + 4 <= bytes.len() {
        let (sub, tag) = (bytes[p], bytes[p + 1]);
        if sub == 0xff && tag == 0xff {
            break;
        }
        let len = usize::from(u16_at(bytes, p + 2));
        let start = p + 4;
        let payload = bytes
            .get(start..start + len)
            .ok_or(DadError::Truncated(p))?;
        let num = |off: usize| {
            Some(f32_at(payload, off))
                .filter(|v| v.is_finite())
                .ok_or(DadError::NotFinite(p))
        };
        let angle = |off: usize| {
            Some(angle_at(payload, off))
                .filter(|v| v.is_finite())
                .ok_or(DadError::NotFinite(p))
        };
        match (tag, sub) {
            (0x01, _) => {
                file.version = String::from_utf8_lossy(payload)
                    .trim_end_matches('\0')
                    .to_string();
            }
            (TAG_ROOF, 0x00) if len >= 4 => {
                file.roofs.push((u16_at(payload, 0), u16_at(payload, 2)));
            }
            (TAG_WALL, SUB_WALL) => {
                if len != 42 {
                    return Err(DadError::BadWallRecord { offset: p, len });
                }
                file.walls.push(DadWall {
                    id: u16_at(payload, 0),
                    group: u16_at(payload, 2),
                    start: Point2::new(num(4)?, num(8)?),
                    length: num(12)?,
                    angle_rad: angle(16)?,
                    eave_height: num(24)?,
                    overhang: num(28)?,
                    pitch_deg: num(32)?,
                    priority: u8::from(num(36)? > 0.5),
                    half_hip: None,
                    breaks: Vec::new(),
                    clad: is_vertical(num(32)?) && u16_at(payload, 22) != 0,
                });
            }
            (TAG_WALL, SUB_STACKED) if len == 16 => {
                if let Some(wall) = file.walls.last_mut() {
                    let (height, pitch_deg) = (num(4)?, num(12)?);
                    if is_vertical(wall.pitch_deg) && wall.half_hip.is_none() {
                        wall.half_hip = Some(HalfHip { height, pitch_deg });
                    } else {
                        wall.breaks.push(PitchBreak { height, pitch_deg });
                    }
                }
            }
            (TAG_WALL, SUB_WINDOW) if len == 16 => {
                file.windows.push(RoofWindow {
                    x: num(0)?,
                    y: num(4)?,
                    width: num(8)?,
                    height: num(12)?,
                });
            }
            (TAG_CHIMNEY, 0x00) if len == 8 => {
                file.chimneys.push(Chimney {
                    corners: Vec::new(),
                    height: num(2)?,
                });
            }
            (TAG_CHIMNEY, 0x10) if len == 8 => {
                if let Some(c) = file.chimneys.last_mut() {
                    c.corners.push(Point2::new(num(0)?, num(4)?));
                }
            }
            _ => {}
        }
        p = start + len;
    }
    Ok(file)
}

/// Chains walls end-to-start. Returns (edges, end point, closed?).
/// When several walls start at the same point, the next one by id wins,
/// then zero-length marker walls, which always sit between two real walls.
fn chain(walls: &[&DadWall]) -> Vec<(Vec<EdgeDef>, Point2, bool)> {
    let is_marker = |w: &DadWall| w.length <= 1e-6;
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

impl DadFile {
    pub fn to_project(&self, name: impl Into<String>) -> Project {
        let mut project = Project::new(name);
        project.notes = format!("Imported from DACAD ({})", self.version);
        let mut groups: BTreeMap<u16, Vec<&DadWall>> = BTreeMap::new();
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

/// Parses a `.DAD` file and converts it to a project in one step.
pub fn import(bytes: &[u8], name: impl Into<String>) -> Result<Project, DadError> {
    Ok(parse(bytes)?.to_project(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a minimal synthetic V1 file (real DACAD examples are not
    /// redistributable, so tests never depend on them).
    struct Writer(Vec<u8>);

    impl Writer {
        fn new() -> Self {
            let mut w = Writer(V1_MAGIC.to_vec());
            w.record(0x00, 0x01, b"DACAD V1.13\0");
            w
        }
        fn record(&mut self, sub: u8, tag: u8, payload: &[u8]) {
            self.0.extend([sub, tag]);
            self.0.extend((payload.len() as u16).to_le_bytes());
            self.0.extend(payload);
        }
        #[allow(clippy::too_many_arguments)]
        fn wall(
            &mut self,
            id: u16,
            group: u16,
            x: f32,
            y: f32,
            len: f32,
            ang: f32,
            pitch: f32,
            flags: u16,
        ) {
            let mut b = Vec::new();
            b.extend(id.to_le_bytes());
            b.extend(group.to_le_bytes());
            for v in [x, y, len, ang, 0.0, 3.8, 0.9, pitch, 0.0] {
                b.extend(v.to_le_bytes());
            }
            b.extend(flags.to_le_bytes());
            self.record(0x00, TAG_WALL, &b);
        }
        fn floats(&mut self, sub: u8, v: [f32; 4]) {
            let b: Vec<u8> = v.iter().flat_map(|f| f.to_le_bytes()).collect();
            self.record(sub, TAG_WALL, &b);
        }
        fn finish(mut self) -> Vec<u8> {
            self.0.extend([0xff, 0xff]);
            self.0
        }
    }

    use std::f32::consts::{FRAC_PI_2, PI};

    fn sample() -> Vec<u8> {
        let mut w = Writer::new();
        // 10 x 6 house, stored out of order, with a half-hipped gable on the east.
        w.wall(1, 1, 0.0, 0.0, 10.0, 0.0, 40.0, 0);
        w.wall(3, 1, 10.0, 6.0, 10.0, PI, 40.0, 0x0100);
        w.floats(SUB_WINDOW, [4.0, 5.0, 1.1, 1.1]);
        w.wall(2, 1, 10.0, 0.0, 6.0, FRAC_PI_2, 90.0, 1);
        w.floats(SUB_STACKED, [0.0, 7.2, 0.0, 50.0]);
        w.wall(4, 1, 0.0, 6.0, 6.0, -FRAC_PI_2, 90.0, 0);
        // An open dormer polyline.
        w.wall(5, 2, 3.0, 0.0, 0.0, -FRAC_PI_2, 53.0, 0);
        w.wall(6, 2, 3.0, 0.0, 2.3, 0.0, 90.0, 0);
        w.finish()
    }

    #[test]
    fn rejects_foreign_and_v2_files() {
        assert_eq!(parse(b"hello"), Err(DadError::NotDacad));
        assert_eq!(
            parse(b"\x08DACADV2\x1a\xff\xff"),
            Err(DadError::UnsupportedV2)
        );
    }

    #[test]
    fn parses_walls_half_hip_and_window() {
        let f = parse(&sample()).unwrap();
        assert_eq!(f.version, "DACAD V1.13");
        assert_eq!(f.walls.len(), 6);
        assert_eq!(f.windows.len(), 1);
        let gable = f.walls.iter().find(|w| w.id == 2).unwrap();
        assert_eq!(
            gable.half_hip,
            Some(HalfHip {
                height: 7.2,
                pitch_deg: 50.0
            })
        );
        assert!(gable.end().approx_eq(Point2::new(10.0, 6.0), 1e-4));
    }

    #[test]
    fn builds_closed_ccw_part_and_dormer() {
        let p = import(&sample(), "Test").unwrap();
        assert_eq!(p.parts.len(), 1);
        assert_eq!(p.dormers.len(), 1);
        let part = &p.parts[0];
        assert_eq!(part.edges.len(), 4);
        assert!(crate::geom::signed_area(&part.outline()) > 0.0);
        assert_eq!(part.edges.iter().filter(|e| e.is_gable()).count(), 2);
    }

    #[test]
    fn f32_angles_are_snapped() {
        let b = (-std::f32::consts::FRAC_PI_2).to_le_bytes();
        assert_eq!(angle_at(&b, 0), -std::f64::consts::FRAC_PI_2);
    }

    #[test]
    fn truncated_record_is_an_error() {
        let mut bytes = sample();
        bytes.truncate(bytes.len() - 10);
        assert!(matches!(parse(&bytes), Err(DadError::Truncated(_))));
    }
}
