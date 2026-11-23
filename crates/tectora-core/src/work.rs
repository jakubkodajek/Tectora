//! A bound on the work of one calculation of a whole job (audit HUNTIO-004).
//!
//! The joins of several buildings ([`crate::union`], DACAD's
//! Modellverschnitt in [`crate::modell`]) compare every face and edge of
//! every building with every other one. For separate buildings that is
//! quick; for buildings on top of each other (a job saved fifty times onto
//! itself, one rotate with many copies, a hostile bundle) it grows with a
//! high power of their number: 10 stacked copies take a second, 25 take
//! 50 s, 200 never end. The per-building [`crate::budget`] does not see
//! this, so the calculating entry points (app commands, the CLI, the MCP
//! calculating and export tools) run in [`bounded`]: the hot loops count
//! their steps with [`tick`], and when a job has used [`LIMIT`] steps the
//! calculation stops with [`TooComplex`] instead of keeping a core busy for
//! minutes. Like the per-building budget it is counted, not timed, so the
//! same job gives the same answer on every machine. A step is one edge
//! compared with another; the reference cases need well under 1 % of the
//! limit.

use std::cell::Cell;

/// Steps one bounded calculation may take: about 5 s of a release build on
/// an ordinary laptop (25 stacked copies of a 10-wall building need more;
/// 10 copies need about 74 million, every reference case under 1 million).
pub const LIMIT: u64 = 250_000_000;
/// Overrides [`LIMIT`] (tests of the CLI and the MCP server, which run a
/// debug build in a child process).
pub const LIMIT_ENV: &str = "TECTORA_WORK_LIMIT";

/// [`LIMIT`], or the number in [`LIMIT_ENV`].
pub fn limit() -> u64 {
    static L: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *L.get_or_init(|| {
        std::env::var(LIMIT_ENV)
            .ok()
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(LIMIT)
    })
}

/// The English reason given when the limit is reached; the app shows its
/// own text for the code [`CODE`].
pub const TOO_COMPLEX: &str = "the design is too complex to calculate: its buildings overlap too much or it has too many walls. Remove copies of buildings that lie on top of each other or split the job";

/// The stable code of [`TOO_COMPLEX`] (`app/src/lib/errors.ts`).
pub const CODE: &str = "too_complex";

thread_local! {
    /// Steps left in the running bounded calculation; `None` outside one.
    static LEFT: Cell<Option<u64>> = const { Cell::new(None) };
}

/// The panic payload that stops a calculation over its limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TooComplex;

impl std::fmt::Display for TooComplex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(TOO_COMPLEX)
    }
}

/// Counts `n` steps of the running bounded calculation and stops it once
/// its limit is used up. Outside [`bounded`] it does nothing.
#[inline]
pub(crate) fn tick(n: usize) {
    LEFT.with(|left| {
        if let Some(l) = left.get() {
            let n = n as u64;
            if l < n {
                left.set(Some(0));
                // `resume_unwind`: no panic message on stderr.
                std::panic::resume_unwind(Box::new(TooComplex));
            }
            left.set(Some(l - n));
        }
    });
}

/// Whether a bounded calculation is running on this thread.
pub fn active() -> bool {
    LEFT.with(|l| l.get().is_some())
}

/// Whether a panic payload is the stop of [`tick`].
pub fn is_too_complex(payload: &(dyn std::any::Any + Send)) -> bool {
    payload.is::<TooComplex>()
}

/// Runs `f` with [`limit`] steps; [`TooComplex`] when it needs more. Inside
/// another bounded calculation it shares that one's steps and lets the
/// outer one answer.
pub fn bounded<T>(f: impl FnOnce() -> T) -> Result<T, TooComplex> {
    #[cfg(test)]
    if let Some(limit) = TEST_LIMIT.with(|l| l.get()) {
        return bounded_by(limit, f).0;
    }
    bounded_by(limit(), f).0
}

#[cfg(test)]
thread_local! {
    /// The limit [`bounded`] uses on this thread instead of [`limit`]
    /// ([`with_limit`]).
    static TEST_LIMIT: Cell<Option<u64>> = const { Cell::new(None) };
}

/// Runs `f` with every [`bounded`] calculation it starts on this thread
/// held to `limit` steps, so a test sees what an entry point does with a
/// roof over its bound (audit 2026-11-22 TEST-801).
#[cfg(test)]
pub(crate) fn with_limit<T>(limit: u64, f: impl FnOnce() -> T) -> T {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            TEST_LIMIT.with(|l| l.set(None));
        }
    }
    TEST_LIMIT.with(|l| l.set(Some(limit)));
    let _reset = Reset;
    f()
}

/// [`bounded`] with its own limit, and the steps used (tests, calibration).
pub fn bounded_by<T>(limit: u64, f: impl FnOnce() -> T) -> (Result<T, TooComplex>, u64) {
    if active() {
        return (Ok(f()), 0);
    }
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            LEFT.with(|l| l.set(None));
        }
    }
    LEFT.with(|l| l.set(Some(limit)));
    let reset = Reset;
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    let used = limit - LEFT.with(|l| l.get()).unwrap_or(0);
    drop(reset);
    match r {
        Ok(v) => (Ok(v), used),
        Err(p) if is_too_complex(&*p) => (Err(TooComplex), used),
        Err(p) => std::panic::resume_unwind(p),
    }
}

/// Why a guarded, bounded calculation gave no result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// Over [`limit`]: [`TOO_COMPLEX`].
    TooComplex,
    /// The calculation panicked ([`crate::model::guarded`]'s text).
    Failed(String),
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::TooComplex => f.write_str(TOO_COMPLEX),
            Refusal::Failed(d) => f.write_str(d),
        }
    }
}

impl From<Refusal> for String {
    fn from(r: Refusal) -> String {
        r.to_string()
    }
}

/// Runs `f` in [`bounded`] and [`crate::model::guarded`]: the way every
/// calculating entry point (app command, CLI, MCP tool, export) runs a
/// calculation of a whole job.
pub fn calculate<T>(f: impl FnOnce() -> T) -> Result<T, Refusal> {
    match bounded(|| crate::model::guarded(f)) {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(d)) if d == TOO_COMPLEX => Err(Refusal::TooComplex),
        Ok(Err(d)) => Err(Refusal::Failed(d)),
        Err(TooComplex) => Err(Refusal::TooComplex),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_calculation_over_its_limit_stops_and_the_next_one_starts_afresh() {
        let (r, used) = bounded_by(10, || {
            for _ in 0..5 {
                tick(3);
            }
        });
        assert_eq!(r, Err(TooComplex));
        assert_eq!(used, 10);
        assert!(!active());
        let (r, used) = bounded_by(10, || tick(4));
        assert_eq!((r, used), (Ok(()), 4));
        // Outside a bounded calculation nothing is counted.
        tick(usize::MAX);
    }

    #[test]
    fn a_nested_calculation_shares_the_outer_limit() {
        let (r, _) = bounded_by(10, || {
            let inner = bounded(|| tick(8));
            assert_eq!(inner, Ok(()));
            // An inner guard does not swallow the stop.
            let caught = crate::model::guarded(|| tick(8));
            unreachable!("{caught:?}");
        });
        assert_eq!(r, Err(TooComplex));
    }
}
