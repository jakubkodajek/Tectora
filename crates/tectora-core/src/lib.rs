//! Tectora core: roof project model, geometry and file formats.
//!
//! Everything here is pure and deterministic so the desktop app, the CLI
//! and the MCP server all share exactly the same behaviour.

pub mod dad;
pub mod diff;
pub mod dormer;
pub mod examples;
mod gable_wall;
pub mod geom;
pub mod model;
pub mod openings;
pub mod quantities;
pub mod raised;
pub mod roof;
pub mod store;
pub mod union;

pub use model::{
    Chimney, Dormer, EdgeDef, HalfHip, JobInfo, JobStatus, PitchBreak, Project, RoofPart,
    RoofWindow,
};
