//! Tectora core: roof project model, geometry and file formats.
//!
//! Everything here is pure and deterministic so the desktop app, the CLI
//! and the MCP server all share exactly the same behaviour.

pub mod dad;
pub mod examples;
pub mod geom;
pub mod model;
pub mod quantities;
pub mod roof;

pub use model::{Chimney, Dormer, EdgeDef, HalfHip, Project, RoofPart};
