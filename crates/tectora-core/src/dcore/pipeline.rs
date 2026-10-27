//! The core's top level: read the RD2, run every model, post-process,
//! write the core SOL and the INT. Stages not ported yet return
//! [`CoreError::Unimplemented`].

use super::error::*;
use super::out;
use super::rd2;
use super::s1;
use super::stages;
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
        let mut code = 0;
        let mut m = self.core.g.models;
        while let Some(mi) = m {
            let c = &mut self.core;
            c.g.cur_model = Some(mi);
            c.reset_model();
            c.g.counters = c.model(mi).counters;
            c.g.ring = c.model(mi).ring;
            if c.model(mi).typ == b'w' && c.g.flags & 0x20 == 0 {
                c.g.ok = 1;
                m = c.model(mi).next;
                continue;
            }
            let cut = match self.plane_cut() {
                Ok(r) => {
                    code = r;
                    None
                }
                Err(e @ CoreError::Cut { .. }) => Some(e),
                Err(e) => return Err(e),
            };
            // After a cut error the run's outcome is decided (the model
            // fails, the loop breaks, the run ends at 0xa10); the stages
            // still run for the state, but one not ported yet does not hide
            // the error.
            let r = self.model_post();
            if let Some(e) = cut {
                return match r {
                    Ok(()) | Err(CoreError::Unimplemented(_)) => Err(e),
                    Err(other) => Err(other),
                };
            }
            r?;
            if self.core.g.ok == 0 {
                break;
            }
            stages::areas(&mut self.core)?;
            self.at("after_area");
            if self.core.model(mi).typ == b'G' {
                self.core.g.n_g_models += 1;
            }
            m = self.core.model(mi).next;
        }
        // The loop variable is the current-model global: null after the
        // last model, the failed model after a break.
        self.core.g.cur_model = m;
        if self.core.g.ok == 0 {
            // progress 0xa10; DACAD (and the emulator) cancel there.
            return Err(CoreError::NoSolution(MSG_NO_SOLUTION));
        }
        self.global_stages()?;
        let mut o = self.write_outputs()?;
        o.code = code as u32;
        Ok(o)
    }

    /// The per-model plane cut: S1 up to the first slab, then the sweep.
    fn plane_cut(&mut self) -> Result<i32, CoreError> {
        let mut hook = self.hook.take();
        let r = s1::model_start(&mut self.core, &mut hook);
        self.hook = hook;
        r?;
        let r = stages::sweep(&mut self.core)?;
        self.at("after_cut");
        Ok(r)
    }

    /// The model's stages after the plane cut up to the edge table.
    fn model_post(&mut self) -> Result<(), CoreError> {
        if self.core.g.flags & 0x8 != 0 {
            stages::chimney_holes(&mut self.core)?;
        }
        self.at("after_holes");
        if self.core.g.ok != 0 {
            let n = stages::collinear_merge(&mut self.core)?;
            self.core.g.counters[ctr::LINES] -= n;
            self.at("after_merge");
        }
        stages::edge_table(&mut self.core)?;
        self.at("after_edgetab");
        Ok(())
    }

    /// The stages over all models, each only while the run is still valid.
    fn global_stages(&mut self) -> Result<(), CoreError> {
        type Stage = fn(&mut Core) -> Result<(), CoreError>;
        let list: [(Stage, &str); 6] = [
            (stages::dormer_feet, "after_dormer"),
            (stages::classify, "after_classify"),
            (stages::codes, "after_codes1"),
            (stages::merge_d17, "after_merge17"),
            (stages::merge_lg17, "after_mergeLG"),
            (stages::hole_edges, "after_holeedge"),
        ];
        for (f, name) in list {
            if self.core.g.ok == 0 {
                break;
            }
            f(&mut self.core)?;
            self.at(name);
        }
        if self.core.g.ok != 0 {
            stages::codes(&mut self.core)?;
        }
        self.at("after_codes2");
        Ok(())
    }

    /// The writers on the current state (e.g. a state loaded from a dump):
    /// the core SOL and, if wanted, the INT.
    pub fn write_outputs(&mut self) -> Result<Outputs, CoreError> {
        let sol = out::write_sol(&mut self.core)?;
        self.at("after_sol");
        let int = if self.want_int {
            let int = out::write_int(&mut self.core)?;
            self.at("after_int");
            Some(int)
        } else {
            None
        };
        Ok(Outputs { sol, int, code: 0 })
    }
}
