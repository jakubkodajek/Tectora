//! Tectora core: roof project model, geometry and file formats.
//!
//! Everything here is pure and deterministic so the desktop app, the CLI
//! and the MCP server all share exactly the same behaviour.

pub mod assistants;
pub mod breakdown;
pub mod bundle;
pub mod check;
pub mod commands;
pub mod coresol;
pub mod dcore;
pub mod diff;
pub mod dormer;
pub mod edit;
pub mod engine;
pub mod examples;
pub mod export;
pub mod flat;
mod gable_wall;
pub mod geom;
pub mod library;
pub mod model;
pub mod modell;
pub mod multiroof;
pub mod openings;
pub mod quantities;
pub mod raised;
pub mod report;
pub mod roof;
pub mod settings;
pub mod snapshot;
pub mod store;
pub mod union;
pub mod walls;

pub use model::{
    Chimney, Dormer, EdgeDef, HalfHip, JobInfo, JobStatus, PitchBreak, Project, RoofPart,
    RoofWindow,
};
