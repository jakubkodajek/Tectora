//! Importer for DACAD `.DAD` roof files.
//!
//! Only the V1.x format (files starting with `DACAD\x1a`) is decoded so far.
//! It is a flat stream of records `[sub: u8][tag: u8][len: u16 LE][payload]`
//! terminated by `ff ff`. The records Tectora uses:
//!
//! * tag `0x12`, sub `0x00` (42 bytes) — one wall line with its roof definition:
//!   `id u16, group u16, f32 x9 [x, y, length, angle_rad, ?, eave_height,
//!   overhang, pitch_deg, priority], flags u16`. Bit 0 of `flags` means a half-hip
//!   record follows; the high byte is the number of chimney records that follow.
//! * tag `0x12`, sub `0x10` (16 bytes) — half-hip of the preceding gable wall:
//!   `u32 ?, f32 height, f32 ?, f32 pitch_deg`.
//! * tag `0x12`, sub `0x20` (16 bytes) — chimney: `f32 x, y, width, depth`.
//!
//! Walls of one group that chain into a closed loop form a roof part; open
//! chains are dormers. The newer `DACADV2` format (MFC archive) is detected
//! and reported as unsupported.

use std::collections::BTreeMap;

use thiserror::Error;

use crate::geom::Point2;
use crate::model::{Chimney, Dormer, EdgeDef, HalfHip, Project, RoofPart};

const V1_MAGIC: &[u8] = b"DACAD\x1a";
const V2_MAGIC: &[u8] = b"\x08DACADV2\x1a";
const TAG_WALL: u8 = 0x12;
const SUB_WALL: u8 = 0x00;
const SUB_HALF_HIP: u8 = 0x10;
const SUB_CHIMNEY: u8 = 0x20;
/// Endpoint matching tolerance when chaining walls, m.
const CHAIN_TOL: f64 = 0.005;

#[derive(Debug, Error, PartialEq)]
pub enum DadError {
    #[error("not a DACAD file")]
    NotDacad,
    #[error("DACAD V2 files are not supported yet")]
    UnsupportedV2,
    #[error("truncated record at byte {0}")]
    Truncated(usize),
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
            breaks: Vec::new(),
            priority: self.priority,
        }
    }
}

/// Raw content of a V1 file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DadFile {
    pub version: String,
    pub walls: Vec<DadWall>,
    pub chimneys: Vec<Chimney>,
}

fn f32_at(b: &[u8], off: usize) -> f64 {
    let v = f32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]]);
    // Values are stored as f32; round to 0.1 mm to drop float noise.
    (f64::from(v) * 1e4).round() / 1e4
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
        match (tag, sub) {
            (0x01, _) => {
                file.version = String::from_utf8_lossy(payload)
                    .trim_end_matches('\0')
                    .to_string();
            }
            (TAG_WALL, SUB_WALL) => {
                if len != 42 {
                    return Err(DadError::BadWallRecord { offset: p, len });
                }
                file.walls.push(DadWall {
                    id: u16_at(payload, 0),
                    group: u16_at(payload, 2),
                    start: Point2::new(f32_at(payload, 4), f32_at(payload, 8)),
                    length: f32_at(payload, 12),
                    angle_rad: f64::from(f32::from_le_bytes(
                        payload[16..20].try_into().expect("4 bytes"),
                    )),
                    eave_height: f32_at(payload, 24),
                    overhang: f32_at(payload, 28),
                    pitch_deg: f32_at(payload, 32),
                    priority: u8::from(f32_at(payload, 36) > 0.5),
                    half_hip: None,
                });
            }
            (TAG_WALL, SUB_HALF_HIP) if len == 16 => {
                if let Some(wall) = file.walls.last_mut() {
                    wall.half_hip = Some(HalfHip {
                        height: f32_at(payload, 4),
                        pitch_deg: f32_at(payload, 12),
                    });
                }
            }
            (TAG_WALL, SUB_CHIMNEY) if len == 16 => {
                file.chimneys.push(Chimney {
                    position: Point2::new(f32_at(payload, 0), f32_at(payload, 4)),
                    width: f32_at(payload, 8),
                    depth: f32_at(payload, 12),
                });
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
    while let Some(first) = (0..walls.len())
        .find(|&i| !used[i] && !is_marker(walls[i]))
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
            let next = candidates
                .iter()
                .copied()
                .find(|&i| {
                    walls[i].id == last.id.wrapping_add(1) && !(closing && !is_marker(walls[i]))
                })
                .or_else(|| candidates.iter().copied().find(|&i| is_marker(walls[i])))
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
            let all_closed = !chains.is_empty() && chains.iter().all(|c| c.2);
            if all_closed {
                for (i, (edges, _, _)) in chains.into_iter().enumerate() {
                    let suffix = if i == 0 {
                        String::new()
                    } else {
                        format!(".{}", i + 1)
                    };
                    let mut part = RoofPart {
                        name: format!("Part {group}{suffix}"),
                        edges,
                    };
                    part.normalize_ccw();
                    project.parts.push(part);
                }
            } else {
                let edges: Vec<EdgeDef> = walls.iter().map(|w| w.edge_def()).collect();
                let end = walls
                    .last()
                    .map(|w| w.end())
                    .unwrap_or(Point2::new(0.0, 0.0));
                project.dormers.push(Dormer { edges, end });
            }
        }
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
        w.floats(SUB_CHIMNEY, [4.0, 5.0, 1.1, 1.1]);
        w.wall(2, 1, 10.0, 0.0, 6.0, FRAC_PI_2, 90.0, 1);
        w.floats(SUB_HALF_HIP, [0.0, 7.2, 0.0, 50.0]);
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
    fn parses_walls_half_hip_and_chimney() {
        let f = parse(&sample()).unwrap();
        assert_eq!(f.version, "DACAD V1.13");
        assert_eq!(f.walls.len(), 6);
        assert_eq!(f.chimneys.len(), 1);
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
    fn truncated_record_is_an_error() {
        let mut bytes = sample();
        bytes.truncate(bytes.len() - 10);
        assert!(matches!(parse(&bytes), Err(DadError::Truncated(_))));
    }
}
