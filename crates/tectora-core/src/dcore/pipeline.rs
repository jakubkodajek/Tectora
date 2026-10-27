//! The core's top level: read the RD2, run every model, post-process,
//! write the core SOL and the INT. Stages not ported yet return
//! [`CoreError::Unimplemented`].

use super::error::*;
use super::out;
use super::rd2;
use super::s1;
use super::state::*;

/// What a finished run produced.
#[derive(Debug, Clone, PartialEq)]
pub struct Outputs {
    /// The core SOL (`.SO2`) bytes.
    pub sol: Vec<u8>,
    /// The INT bytes, when requested.
    pub int: Option<Vec<u8>>,
    /// The pipeline's return code: 0, or 3 for the fallback SOL of a broken wall chain.
    pub code: u32,
}

/// One core run.
pub struct Run<'a> {
    pub core: Core,
    /// Write the INT (DACAD does when the RD2 has W records of more than one model).
    pub want_int: bool,
    /// Called at the stage boundaries with the state (names as in the emulator dumps).
    pub hook: s1::Hook<'a>,
}

impl<'a> Run<'a> {
    /// A run with the flags word DACAD passes (0x4a8 without, 0x4a0 with INT).
    pub fn new(flags: u32) -> Run<'a> {
        Run {
            core: Core::new(flags),
            want_int: flags & 0x8 == 0,
            hook: None,
        }
    }

    fn at(&mut self, stage: &str) {
        if let Some(h) = &mut self.hook {
            h(stage, &self.core);
        }
    }

    /// Runs the core on an RD2.
    pub fn run(&mut self, rd2_bytes: &[u8]) -> Result<Outputs, CoreError> {
        match self.run_inner(rd2_bytes) {
            Err(CoreError::NoPlan { .. }) => Ok(Outputs {
                sol: out::fallback_sol(&self.core)?,
                int: None,
                code: 3,
            }),
            r => r,
        }
    }

    fn run_inner(&mut self, rd2_bytes: &[u8]) -> Result<Outputs, CoreError> {
        self.core.reset_model();
        rd2::read(&mut self.core, rd2_bytes)?;
        self.at("after_read");
        let mut m = self.core.g.models;
        while let Some(mi) = m {
            let c = &mut self.core;
            c.g.cur_model = Some(mi);
            c.reset_model();
            c.g.counters = c.model(mi).counters;
            c.g.ring = c.model(mi).ring;
            if c.model(mi).typ == b'w' && c.g.flags & 0x20 == 0 {
                c.g.ok = 1;
            } else {
                let mut hook = self.hook.take();
                let r = s1::model_start(&mut self.core, &mut hook);
                self.hook = hook;
                r?;
                return Err(CoreError::Unimplemented("S2 height sweep"));
            }
            m = self.core.model(mi).next;
        }
        Err(CoreError::Unimplemented("S6 post-processing"))
    }

    /// The writers on the current state (e.g. a state loaded from a dump):
    /// the core SOL and, if wanted, the INT.
    pub fn write_outputs(&mut self) -> Result<Outputs, CoreError> {
        let sol = out::write_sol(&mut self.core)?;
        self.at("after_sol");
        let int = if self.want_int {
            Some(out::write_int(&mut self.core)?)
        } else {
            None
        };
        Ok(Outputs { sol, int, code: 0 })
    }
}
