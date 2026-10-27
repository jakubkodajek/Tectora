//! The stages S2–S6 still to be ported, one function per routine of the
//! original's driver (map.md §1.2), each returning
//! [`CoreError::Unimplemented`] until its slice lands.
//!
//! A porter replaces the body of the stubs of their slice (or moves them
//! into their own module and re-exports them here) and keeps the
//! signatures: [`super::pipeline`] calls them in the original's order and
//! fires the stage hooks after them with the dump names in the comments.

use super::error::CoreError;
use super::state::Core;

/// S2 + S3: the height sweep, segment insertion and face closing of the
/// current model (phases 3–12 of the per-model plane cut), started from the
/// state [`super::s1::model_start`] leaves (`s1_end`).
///
/// Ends with `c.g.ok = 1` and `Ok(0)` on success. A cut error (E00002 with
/// 0xa02, 0xa04, 0xa09..0xa0f) is `Err(CoreError::Cut { code: 2, msg })`;
/// the pipeline then runs the model's remaining stages and ends the run
/// with it. Dump point after it: `after_cut`.
pub fn sweep(c: &mut Core) -> Result<i32, CoreError> {
    let _ = c;
    Err(CoreError::Unimplemented(
        "S2/S3 height sweep and face closing",
    ))
}

/// S6: chimney holes (`da3dc0` → `da53f0`), only with flag 0x8: clips each
/// hole outline against the closed sloped faces of the current model and
/// hangs the pieces on the faces. Dump point: `after_holes` (reached also
/// when the flag is off).
pub fn chimney_holes(c: &mut Core) -> Result<(), CoreError> {
    let _ = c;
    Err(CoreError::Unimplemented("S6 chimney holes"))
}

/// S4: collinear merge of consecutive segments of every closed face
/// (`e562a0`), only after a successful model. Returns the number of merged
/// segments, which the pipeline subtracts from `counters[ctr::LINES]`.
/// Dump point: `after_merge`.
pub fn collinear_merge(c: &mut Core) -> Result<i32, CoreError> {
    let _ = c;
    Err(CoreError::Unimplemented("S4 collinear merge"))
}

/// S4: segment origins and the global edge table (`da4210`), run for every
/// processed model, also a failed one. Dump point: `after_edgetab`.
pub fn edge_table(c: &mut Core) -> Result<(), CoreError> {
    let _ = c;
    Err(CoreError::Unimplemented("S4 edge table"))
}

/// S4: face areas (`e56460`), only after a successful model. Dump point:
/// `after_area`.
pub fn areas(c: &mut Core) -> Result<(), CoreError> {
    let _ = c;
    Err(CoreError::Unimplemented("S4 face areas"))
}

/// S4: the foot outline of every `g` model (`d9be40`). Dump point:
/// `after_dormer`.
pub fn dormer_feet(c: &mut Core) -> Result<(), CoreError> {
    let _ = c;
    Err(CoreError::Unimplemented("S4 dormer feet"))
}

/// S5: line codes of D/N/W-origin edges (`da2710`). Dump point:
/// `after_classify`.
pub fn classify(c: &mut Core) -> Result<(), CoreError> {
    super::classify::classify(c);
    Ok(())
}

/// S5: codes of G/L-origin edges and refinements (`da3a10`). Runs twice;
/// dump points `after_codes1` and `after_codes2`.
pub fn codes(c: &mut Core) -> Result<(), CoreError> {
    super::classify::codes(c);
    Ok(())
}

/// S5: a D edge lying on another D edge gets code 17 (`da4ec0`). Dump
/// point: `after_merge17`.
pub fn merge_d17(c: &mut Core) -> Result<(), CoreError> {
    super::classify::merge_d17(c);
    Ok(())
}

/// S5: an L/G edge lying on another L/G edge gets code 17 (`da5060`). Dump
/// point: `after_mergeLG`.
pub fn merge_lg17(c: &mut Core) -> Result<(), CoreError> {
    super::classify::merge_lg17(c);
    Ok(())
}

/// S6: hole edges on roof edges (`da5160`). Dump point: `after_holeedge`.
pub fn hole_edges(c: &mut Core) -> Result<(), CoreError> {
    let _ = c;
    Err(CoreError::Unimplemented("S6 hole edges"))
}
