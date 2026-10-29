//! Tectora core: roof project model, geometry and file formats.
//!
//! Everything here is pure and deterministic so the desktop app, the CLI
//! and the MCP server all share exactly the same behaviour.

pub mod coresol;
pub mod dad;
pub mod dcore;
pub mod diff;
pub mod dormer;
pub mod engine;
pub mod examples;
pub mod flat;
mod gable_wall;
pub mod geom;
pub mod model;
pub mod modell;
pub mod multiroof;
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
