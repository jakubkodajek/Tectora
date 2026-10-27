//! The files between DACAD's roof core and its later stages: the RD2 the
//! core reads, the core SOL it writes, and what is derived from them, the
//! INT (input of the Modellverschnitt) and the W3D wall polygons with the
//! SOL with walls.
//!
//! - [`Rd2`]: the parsed RD2 with the state the core derives before
//!   cutting (wall ring after chain sort and collinear merge, face
//!   frames).
//! - [`CoreSol`]: what the core SOL holds (lines with codes, faces with
//!   planes, wall lines and signed edge references), with reader and
//!   writer.
//! - [`write_int`] / [`write_int_geometry`]: the INT, from a core SOL or
//!   from face polygons and edge flags a producer knows itself
//!   ([`IntGeometry`]).
//! - [`wall_stage`] / [`write_w3d`] / [`write_wall_sol`]: the wall stage.
//!
//! All of it is a faithful re-implementation: values are float32 where
//! DACAD stores floats and double in between, the printf rounding is that
//! of DACAD's CRT (ties round up), and all list orders and first-match
//! rules are DACAD's. Do not tidy it up.

mod int;
pub mod num;
mod rd2;
mod sol;
mod walls;

pub use int::{
    IntEdge, IntError, IntFace, IntGeometry, PlanEdge, sol_edge_flag, write_int, write_int_geometry,
};
pub use rd2::{Face, Hole, Model, Rd2, RingWall, Wall};
pub use sol::{CoreSol, SolFace, SolLine};
pub use walls::{WallError, WallStage, wall_stage, write_w3d, write_wall_sol};

/// Why a text could not be read.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ParseError {
    #[error("RD2: unexpected end or token at token {0}")]
    Rd2(usize),
    #[error("RD2: the walls of model {0} do not form a closed chain")]
    WallChain(i32),
    #[error("SOL: unexpected end or token at token {0}")]
    Sol(usize),
}
