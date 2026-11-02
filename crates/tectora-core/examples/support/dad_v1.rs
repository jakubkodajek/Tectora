//! Reader for DACAD V1 `.DAD` sample files, used only by the developer
//! regression tool `engine_check` to run DACAD's own samples through the
//! engine. It is not part of the product: Tectora does not import DACAD files.
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

use thiserror::Error;

use tectora_core::geom::Point2;
use tectora_core::model::{Chimney, HalfHip, PitchBreak, Project, RoofWindow};
use tectora_core::walls::{PlanWall, WallList};

const V1_MAGIC: &[u8] = b"DACAD\x1a";
const V2_MAGIC: &[u8] = b"\x08DACADV2\x1a";
const TAG_ROOF: u8 = 0x11;
const TAG_WALL: u8 = 0x12;
const TAG_CHIMNEY: u8 = 0x13;
const SUB_WALL: u8 = 0x00;
const SUB_STACKED: u8 = 0x10;
const SUB_WINDOW: u8 = 0x20;

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

/// A gable wall; a pitch over 90° is a plane leaning outward (C71).
fn is_vertical(pitch_deg: f64) -> bool {
    (pitch_deg - 90.0).abs() <= 90.0 - tectora_core::model::GABLE_PITCH_DEG
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

pub fn parse(bytes: &[u8]) -> Result<WallList, DadError> {
    if bytes.starts_with(V2_MAGIC) {
        return Err(DadError::UnsupportedV2);
    }
    if !bytes.starts_with(V1_MAGIC) {
        return Err(DadError::NotDacad);
    }
    let mut file = WallList::default();
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
            (TAG_ROOF, 0x00) if len >= 4 => {
                file.roofs.push((u16_at(payload, 0), u16_at(payload, 2)));
            }
            (TAG_WALL, SUB_WALL) => {
                if len != 42 {
                    return Err(DadError::BadWallRecord { offset: p, len });
                }
                file.walls.push(PlanWall {
                    id: u16_at(payload, 0),
                    group: u16_at(payload, 2),
                    start: Point2::new(num(4)?, num(8)?),
                    length: num(12)?,
                    stored_length: f64::from(f32::from_le_bytes([
                        payload[12],
                        payload[13],
                        payload[14],
                        payload[15],
                    ])),
                    angle_rad: angle(16)?,
                    eave_height: num(24)?,
                    overhang: num(28)?,
                    pitch_deg: num(32)?,
                    priority: u8::from(num(36)? > 0.5),
                    half_hip: None,
                    breaks: Vec::new(),
                    clad: num(32)? >= tectora_core::model::GABLE_PITCH_DEG
                        && u16_at(payload, 22) != 0,
                    material: match u16_at(payload, 22) {
                        0 => tectora_core::model::DEFAULT_MATERIAL,
                        m => u8::try_from(m).unwrap_or(tectora_core::model::DEFAULT_MATERIAL),
                    },
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

/// Parses a `.DAD` file and converts it to a project in one step.
pub fn import(bytes: &[u8], name: impl Into<String>) -> Result<Project, DadError> {
    Ok(parse(bytes)?.to_project(name))
}
