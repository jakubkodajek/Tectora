//! A 1:1 port of DACAD's roof calculation core (the stage between the RD2
//! and the core SOL/INT), validated against an emulator of the original.
//!
//! See `PORTING.md` next to this file for the state model, the arithmetic
//! conventions and how to test a slice against the emulator dumps.

pub mod dump;
pub mod error;
pub mod num;
pub mod out;
pub mod pipeline;
pub mod rd2;
pub mod s1;
pub mod stages;
pub mod state;
pub mod x87;

pub use error::CoreError;
pub use state::Core;
