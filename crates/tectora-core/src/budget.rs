//! A bound on the work spent on one building where the core stops.
//!
//! Each way Tectora calculates such a building (`openings::depart`) runs
//! DACAD's core several times: three times to join planes
//! ([`crate::coplanar`]), up to [`crate::wing_split::MAX_RUNS`] times to
//! peel wings, and all of it once more when the outline is straightened
//! first. Most runs take a millisecond. One that ends at the core's own
//! iteration limit ([`CalcErrorKind::NoEnd`]) takes half a second, and the
//! wing search then peels on and meets the limit again and again (a
//! 44-wall building turned on the site plan: 22 such runs, 10 to 60 s).
//! So all the ways tried for one building share one budget: a number of
//! runs, and a much smaller number of runs that end at that limit. It is
//! counted, not timed: the same building gives the same result on every
//! machine.

use crate::engine::Failure;
use crate::model::Project;
use crate::roof::{CalcErrorKind, RoofGeometry};

/// The most core runs for one building, over all the ways tried.
pub(crate) const MAX_RUNS: usize = 64;
/// The most of those runs that may end at the core's iteration limit.
pub(crate) const MAX_NO_END: usize = 2;
/// Why a way stopped when the budget is spent.
pub(crate) const SPENT: &str = "too much to calculate for one building";

/// The core runs made for one building.
#[derive(Debug, Default)]
pub(crate) struct Budget {
    runs: usize,
    no_end: usize,
    unlimited: bool,
}

impl Budget {
    /// The budget of one building.
    pub(crate) fn new() -> Budget {
        Budget::default()
    }

    /// No bound beyond each way's own: for a way run by itself (three runs
    /// to join the planes of a roof the core calculated; unit tests).
    pub(crate) fn unlimited() -> Budget {
        Budget {
            unlimited: true,
            ..Budget::default()
        }
    }

    /// Whether nothing more may be run.
    pub(crate) fn spent(&self) -> bool {
        !self.unlimited && (self.runs >= MAX_RUNS || self.no_end >= MAX_NO_END)
    }

    /// The core's result for `p`, or `None` when the budget is spent.
    pub(crate) fn run(&mut self, p: &Project) -> Option<Result<Vec<RoofGeometry>, Failure>> {
        if self.spent() {
            return None;
        }
        self.runs += 1;
        let result = crate::engine::generate(p);
        if matches!(&result, Err(Failure::Dacad(e)) if e.kind == CalcErrorKind::NoEnd) {
            self.no_end += 1;
        }
        Some(result)
    }

    /// The runs made, and how many of them ended at the core's limit.
    pub(crate) fn runs(&self) -> (usize, usize) {
        (self.runs, self.no_end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Point2;
    use crate::model::{DEFAULT_MATERIAL, EdgeDef, RoofPart};

    fn rect() -> Project {
        let edges = [(0.0, 0.0), (8.0, 0.0), (8.0, 6.0), (0.0, 6.0)]
            .iter()
            .map(|&(x, y)| EdgeDef {
                start: Point2::new(x, y),
                pitch_deg: 40.0,
                eave_height: 3.0,
                overhang: 0.5,
                half_hip: None,
                breaks: Vec::new(),
                priority: 0,
                direction_rad: None,
                clad: false,
                source: None,
                material: DEFAULT_MATERIAL,
            })
            .collect();
        let mut p = Project::new("rect");
        p.parts.push(RoofPart::new("A", edges));
        p
    }

    #[test]
    fn a_budget_stops_after_its_runs() {
        let p = rect();
        let mut b = Budget::new();
        for k in 0..MAX_RUNS {
            assert!(b.run(&p).is_some(), "run {k}");
        }
        assert!(b.spent());
        assert!(b.run(&p).is_none());
        assert_eq!(b.runs(), (MAX_RUNS, 0));
        let mut free = Budget::unlimited();
        for _ in 0..MAX_RUNS + 1 {
            assert!(free.run(&p).is_some());
        }
        assert!(!free.spent());
    }
}
